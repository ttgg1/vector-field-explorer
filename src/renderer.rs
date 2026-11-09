//! Instanced wgpu renderer for arrows and lines (main plan Task 7).
//!
//! Four pipelines (3D arrow + flat 2D arrow + line + dot) share one bind-group
//! layout holding the `Globals` uniform and the per-mode instance storage
//! array. The `DrawState` lives in the egui_wgpu callback_resources TypeMap
//! (installed once by the app); each frame a `DrawCallback` uploads the current
//! `SceneGPU` and draws.
//!
//! Depth note: egui_wgpu paint callbacks have no depth attachment, so arrows
//! are CPU-sorted far-to-near (painter's algorithm) by the caller. The arrow
//! pipeline blends straight alpha (`SrcAlpha` over `OneMinusSrcAlpha`) so
//! plane-by-plane mode can paint the off-plane arrows translucently; selected
//! plane arrows arrive with alpha 1.0 and therefore still paint opaquely.

use std::sync::Arc;

use eframe::egui_wgpu;
use eframe::wgpu;
use eframe::wgpu::util::DeviceExt;
use glam::{Mat4, Vec3};

use crate::arrow_style::{ArrowParams, ArrowStyle, ArrowStyle2D};

/// Per-arrow GPU instance: base position, unit direction + length, color.
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct ArrowInstanceGPU {
    pub origin: [f32; 4],  // pos.xyz, 1.0
    pub dir_len: [f32; 4], // unit dir.xyz, length
    pub color: [f32; 4],   // rgba 0..1
}

/// Per-line GPU instance: start, end + width, color.
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct LineInstanceGPU {
    pub a: [f32; 4],     // start.xyz, 1.0
    pub b: [f32; 4],     // end.xyz, line width
    pub color: [f32; 4], // rgba 0..1
}

/// Shared per-frame uniforms.
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Globals {
    pub view_proj: [[f32; 4]; 4],
    pub inv_view_proj: [[f32; 4]; 4],
    pub camera_pos: [f32; 4],
    pub light_dir: [f32; 4],
    /// World-space camera right axis, w = 0. Dot billboards expand along it.
    pub camera_right: [f32; 4],
    /// World-space camera up axis, w = 0.
    pub camera_up: [f32; 4],
    /// The active view's style id (3D family in 3D, 2D family in 2D), radial
    /// side count, then two unused slots.
    pub arrow_style: [u32; 4],
    /// Head length, head width, shaft radius, then one unused slot.
    pub arrow_params: [f32; 4],
}

/// Vertices the 3D `Flat` style emits per instance: a shaft quad (6) and a head
/// triangle (3). Kept in step with `ARROW_VERTS_FLAT` in the arrow shader.
pub const ARROW_VERTS_FLAT: u32 = 9;

/// Vertices the 3D `Line` style emits per instance: one headless ribbon quad.
/// Kept in step with `ARROW_VERTS_LINE` in the arrow shader.
pub const ARROW_VERTS_LINE: u32 = 6;

/// Vertices the flat 2D `Flat` arrow emits per instance. Kept in step with
/// `ARROW2D_VERTS` in `src/shaders.arrow2d.wgsl`.
pub const ARROW2D_VERTS: u32 = 9;

/// Vertices the 2D `Line` arrow emits per instance: one headless quad. Kept in
/// step with `ARROW2D_VERTS_LINE` in `src/shaders.arrow2d.wgsl`.
pub const ARROW2D_VERTS_LINE: u32 = 6;

/// Which arrow pipeline `record` uses and how many vertices per instance it
/// draws. Pure so the 2D/3D selection is testable without a GPU; `record`
/// matches on it directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrowDrawPlan {
    /// The flat 2D arrow through `arrow2d_pipeline`, at the selected 2D
    /// family's vertex count.
    Flat2D { vertices: u32 },
    /// The selected 3D family through `arrow_pipeline`.
    Mesh3D { vertices: u32 },
}

/// Vertices the dot shader emits per instance: the two triangles of a
/// camera-facing billboard quad. Kept in step with
/// [`crate::particles::DOT_QUAD_CORNERS`] (the CPU mirror of the same table).
pub const DOT_VERTS_PER_INSTANCE: u32 = 6;

/// The world-space camera right/up axes a billboard needs, recovered from the
/// view-projection's first two rows.
///
/// Rows 0/1 of a standard perspective or orthographic projection are scaled
/// copies of the view matrix's right/up rows (both projections are diagonal in
/// x and y), so normalising them gives the camera basis for either projection
/// mode. That matters: the orbit camera's own `view_basis()` only describes the
/// 3D orbit, while 2D mode paints through the front view.
pub fn camera_basis_from_view_proj(view_proj: &Mat4) -> (Vec3, Vec3) {
    let right = view_proj.row(0).truncate().normalize_or_zero();
    let up = view_proj.row(1).truncate().normalize_or_zero();
    (right, up)
}

/// The GPU scene uploaded each frame. `Arc` lets the app skip re-uploads when
/// a collection is unchanged (dots reuse the arrow instance layout with a
/// negligible head).
///
/// The arrow fields select the geometry family and the 2D/3D draw path. They
/// travel with the instance collections so screenshots and the viewport paint
/// exactly the same scene.
#[derive(Clone)]
pub struct SceneGPU {
    pub arrows: Arc<Vec<ArrowInstanceGPU>>,
    pub lines: Arc<Vec<LineInstanceGPU>>,
    pub dots: Arc<Vec<ArrowInstanceGPU>>,
    pub arrow_style: ArrowStyle,
    pub arrow_style_2d: ArrowStyle2D,
    pub arrow_params: ArrowParams,
    pub mode_3d: bool,
}

impl Default for SceneGPU {
    fn default() -> Self {
        Self {
            arrows: Arc::new(Vec::new()),
            lines: Arc::new(Vec::new()),
            dots: Arc::new(Vec::new()),
            arrow_style: ArrowStyle::default(),
            arrow_style_2d: ArrowStyle2D::default(),
            arrow_params: ArrowParams::default(),
            // The old renderer always drew the perspective mesh; keep that as
            // the default until the viewmodel's snapshot fills in the mode.
            mode_3d: true,
        }
    }
}

impl SceneGPU {
    pub fn is_empty(&self) -> bool {
        self.arrows.is_empty() && self.lines.is_empty() && self.dots.is_empty()
    }

    /// The pipeline and per-instance vertex count the next `record` call uses.
    /// 3D uses the selected [`ArrowStyle`] family's budget; 2D uses the
    /// independent [`ArrowStyle2D`] set (tickets 61 and 64), never the 3D
    /// selection, and carries that 2D family's own 9- or 6-vertex count.
    pub fn arrow_draw_plan(&self) -> ArrowDrawPlan {
        if self.mode_3d {
            ArrowDrawPlan::Mesh3D {
                vertices: self.arrow_style.vertex_count(self.arrow_params.sides),
            }
        } else {
            ArrowDrawPlan::Flat2D {
                vertices: self.arrow_style_2d.vertex_count(self.arrow_params.sides),
            }
        }
    }

    /// The per-instance vertex count the next `record` call draws.
    pub fn arrow_vertex_count(&self) -> u32 {
        match self.arrow_draw_plan() {
            ArrowDrawPlan::Flat2D { vertices } | ArrowDrawPlan::Mesh3D { vertices } => vertices,
        }
    }

    /// Whether arrows paint through the flat 2D pipeline rather than the 3D
    /// family pipeline.
    pub fn uses_flat_arrow(&self) -> bool {
        matches!(self.arrow_draw_plan(), ArrowDrawPlan::Flat2D { .. })
    }
}

const LIGHT_DIR: [f32; 4] = [0.4211, 0.8521, 0.3108, 0.0]; // normalize(0.42, 0.85, 0.31)

/// The arrow pipeline's blend state: straight alpha, so an instance's
/// `color.a` is honoured. Plane-by-plane mode relies on this; opaque arrows
/// (alpha 1.0) are unchanged by it.
///
/// There is no depth attachment to configure: egui_wgpu paint callbacks run
/// in a pass egui owns, so the existing far-to-near CPU sort is the depth
/// ordering. A second opaque pass for the selected plane is unnecessary:
/// alpha 1.0 under this blend resolves to a plain replace, so selected arrows
/// still paint opaquely inside the one sorted draw.
pub const ARROW_BLEND: wgpu::BlendState = wgpu::BlendState::ALPHA_BLENDING;

/// The flat 2D arrow pipeline shares the arrow blend: off-plane dimming and
/// screenshots must behave identically in both views.
pub const ARROW2D_BLEND: wgpu::BlendState = ARROW_BLEND;

fn mat4_to_mats(m: Mat4) -> [[f32; 4]; 4] {
    m.to_cols_array_2d()
}

/// One-time GPU resources: pipelines, layouts, and the (per-frame rewritten)
/// buffers. Installed into the egui_wgpu callback_resources TypeMap.
pub struct DrawState {
    bind_layout: wgpu::BindGroupLayout,
    arrow_pipeline: wgpu::RenderPipeline,
    arrow2d_pipeline: wgpu::RenderPipeline,
    line_pipeline: wgpu::RenderPipeline,
    dot_pipeline: wgpu::RenderPipeline,
    globals_buf: wgpu::Buffer,
    arrow_buf: wgpu::Buffer,
    line_buf: wgpu::Buffer,
    dot_buf: wgpu::Buffer,
    arrow_bind: wgpu::BindGroup,
    line_bind: wgpu::BindGroup,
    dot_bind: wgpu::BindGroup,
    light_dir: [f32; 4],
    /// The surface format the pipelines were built for. An offscreen capture
    /// renders in the same format, so the image matches the screen and the
    /// readback only needs the channel-order fix-up.
    format: wgpu::TextureFormat,
}

impl DrawState {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader_arrow = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vfe arrow shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders.arrow.wgsl").into()),
        });
        let shader_arrow2d = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vfe 2d arrow shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders.arrow2d.wgsl").into()),
        });
        let shader_line = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vfe line shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders.line.wgsl").into()),
        });
        let shader_dot = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vfe dot shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders.dot.wgsl").into()),
        });

        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vfe globals+instances bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("vfe pipeline layout"),
                bind_group_layouts: &[Some(&bind_layout)],
                immediate_size: 0,
            });

        let arrow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vfe arrow pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader_arrow,
                entry_point: Some("vs_arrow"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader_arrow,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(ARROW_BLEND),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let arrow2d_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vfe 2d arrow pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader_arrow2d,
                entry_point: Some("vs_arrow2d"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader_arrow2d,
                entry_point: Some("fs_arrow2d"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(ARROW2D_BLEND),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let line_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vfe line pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader_line,
                entry_point: Some("vs_line"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader_line,
                entry_point: Some("fs_line"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let dot_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vfe dot pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader_dot,
                entry_point: Some("vs_dot"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader_dot,
                entry_point: Some("fs_dot"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    // The rim is antialiased by fading the fragment's alpha
                    // over one pixel, so the dot pipeline blends; fully covered
                    // interior pixels (alpha 1) are indistinguishable from the
                    // arrows' REPLACE blend.
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        // Placeholder buffers (zero-length upload is illegal; start with one
        // instance each and let the first prepare() resize them).
        let globals_buf = wgpu::util::DeviceExt::create_buffer_init(
            device,
            &wgpu::util::BufferInitDescriptor {
                label: Some("vfe globals"),
                contents: bytemuck::bytes_of(&Globals {
                    view_proj: Mat4::IDENTITY.to_cols_array_2d(),
                    inv_view_proj: Mat4::IDENTITY.to_cols_array_2d(),
                    camera_pos: [0.0; 4],
                    light_dir: LIGHT_DIR,
                    camera_right: [1.0, 0.0, 0.0, 0.0],
                    camera_up: [0.0, 1.0, 0.0, 0.0],
                    arrow_style: [ArrowStyle::Cone.id(), ArrowParams::default().sides, 0, 0],
                    arrow_params: [0.3, 0.12, 0.055, 0.0],
                }),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            },
        );
        let arrow_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vfe arrows"),
            size: std::mem::size_of::<ArrowInstanceGPU>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let line_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vfe lines"),
            size: std::mem::size_of::<LineInstanceGPU>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let dot_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vfe dots"),
            size: std::mem::size_of::<ArrowInstanceGPU>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let arrow_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vfe arrow bind"),
            layout: &bind_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: globals_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: arrow_buf.as_entire_binding(),
                },
            ],
        });
        let line_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vfe line bind"),
            layout: &bind_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: globals_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: line_buf.as_entire_binding(),
                },
            ],
        });
        let dot_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vfe dot bind"),
            layout: &bind_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: globals_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: dot_buf.as_entire_binding(),
                },
            ],
        });

        Self {
            bind_layout,
            arrow_pipeline,
            arrow2d_pipeline,
            line_pipeline,
            dot_pipeline,
            globals_buf,
            arrow_buf,
            line_buf,
            dot_buf,
            arrow_bind,
            line_bind,
            dot_bind,
            light_dir: LIGHT_DIR,
            format,
        }
    }

    /// Rebuild instance buffers + write uniforms for this frame's scene.
    /// Takes the whole [`SceneGPU`]: its instance slices are the payload, and
    /// bundling them keeps the argument list clippy-clean.
    fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view_proj: Mat4,
        camera_pos: Vec3,
        scene: &SceneGPU,
    ) {
        let (camera_right, camera_up) = camera_basis_from_view_proj(&view_proj);
        let params = scene.arrow_params.sanitized();
        // `arrow_style.x` is mode-dependent: the 3D family id while the 3D
        // pipeline paints, the 2D family id while the 2D pipeline paints. The
        // 2D and 3D ids are separate namespaces, so the matching shader must
        // be the one that reads it (ticket 64).
        let arrow_style_id =
            if scene.mode_3d { scene.arrow_style.id() } else { scene.arrow_style_2d.id() };
        let globals = Globals {
            view_proj: mat4_to_mats(view_proj),
            inv_view_proj: mat4_to_mats(view_proj.inverse()),
            camera_pos: [camera_pos.x, camera_pos.y, camera_pos.z, 1.0],
            light_dir: self.light_dir,
            camera_right: camera_right.extend(0.0).to_array(),
            camera_up: camera_up.extend(0.0).to_array(),
            arrow_style: [arrow_style_id, params.sides, 0, 0],
            arrow_params: [params.head_length, params.head_width, params.shaft_radius, 0.0],
        };
        queue.write_buffer(&self.globals_buf, 0, bytemuck::bytes_of(&globals));

        // Empty collections are skipped: a zero-size storage buffer cannot be
        // bound (the bind group would be rejected with "binding size is zero").
        // The one-instance placeholders from `new()` remain bound and valid,
        // and `paint()` skips drawing any empty collection anyway.
        if !scene.arrows.is_empty() {
            self.arrow_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("vfe arrows (frame)"),
                contents: bytemuck::cast_slice(scene.arrows.as_slice()),
                usage: wgpu::BufferUsages::STORAGE,
            });
            self.arrow_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("vfe arrow bind (frame)"),
                layout: &self.bind_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.globals_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.arrow_buf.as_entire_binding(),
                    },
                ],
            });
        }

        if !scene.lines.is_empty() {
            self.line_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("vfe lines (frame)"),
                contents: bytemuck::cast_slice(scene.lines.as_slice()),
                usage: wgpu::BufferUsages::STORAGE,
            });
            self.line_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("vfe line bind (frame)"),
                layout: &self.bind_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.globals_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.line_buf.as_entire_binding(),
                    },
                ],
            });
        }

        if !scene.dots.is_empty() {
            self.dot_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("vfe dots (frame)"),
                contents: bytemuck::cast_slice(scene.dots.as_slice()),
                usage: wgpu::BufferUsages::STORAGE,
            });
            self.dot_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("vfe dot bind (frame)"),
                layout: &self.bind_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.globals_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.dot_buf.as_entire_binding(),
                    },
                ],
            });
        }
    }

    /// Record this frame's scene into a render pass: lines, then opaque
    /// arrows, then dots — the same order the on-screen pass uses, so an
    /// offscreen capture paints exactly what the viewport paints.
    fn record(&self, pass: &mut wgpu::RenderPass<'_>, scene: &SceneGPU) {
        if !scene.lines.is_empty() {
            pass.set_pipeline(&self.line_pipeline);
            pass.set_bind_group(0, &self.line_bind, &[]);
            pass.draw(0..(scene.lines.len() as u32) * 6, 0..1);
        }

        if !scene.arrows.is_empty() {
            let vertices = match scene.arrow_draw_plan() {
                ArrowDrawPlan::Flat2D { vertices } => {
                    pass.set_pipeline(&self.arrow2d_pipeline);
                    vertices
                }
                ArrowDrawPlan::Mesh3D { vertices } => {
                    pass.set_pipeline(&self.arrow_pipeline);
                    vertices
                }
            };
            pass.set_bind_group(0, &self.arrow_bind, &[]);
            pass.draw(0..(scene.arrows.len() as u32) * vertices, 0..1);
        }

        if !scene.dots.is_empty() {
            pass.set_pipeline(&self.dot_pipeline);
            pass.set_bind_group(0, &self.dot_bind, &[]);
            pass.draw(0..(scene.dots.len() as u32) * DOT_VERTS_PER_INSTANCE, 0..1);
        }
    }

    /// Submit `scene` into a fresh offscreen texture at `width`×`height` and
    /// return the readback buffer. The caller maps it: native waits, wasm
    /// resolves the map promise on the browser event loop.
    ///
    /// The caller passes the same `view_proj`/`camera_pos` the viewport is
    /// painting with, so the capture is the current view rather than a
    /// re-derived one. The texture borrows the surface format (the pipelines
    /// were created for it) and lives only for this submission: the live
    /// surface textures are never touched. `clear` is the viewport background
    /// in linear space, matching what egui paints behind the field.
    #[allow(clippy::too_many_arguments)]
    fn submit_offscreen(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &SceneGPU,
        view_proj: Mat4,
        camera_pos: Vec3,
        width: u32,
        height: u32,
        clear: [f64; 4],
    ) -> Result<wgpu::Buffer, String> {
        // Reject formats the PNG path cannot describe up front: a readback
        // stride/buffer sized for RGBA8 is wrong for, say, a 16-float band and
        // would surface as a wgpu validation panic instead of this error.
        crate::screenshot::png_readback_check(self.format)?;

        // Same uniforms + instance buffers the on-screen pass would use. The
        // paint callback re-uploads every frame, so this does not leave the
        // live frame stale.
        self.upload(device, queue, view_proj, camera_pos, scene);

        let output = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vfe screenshot target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let output_view = output.create_view(&wgpu::TextureViewDescriptor::default());

        let padded_row = crate::screenshot::padded_row_bytes(width);
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vfe screenshot readback"),
            size: u64::from(padded_row) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("vfe screenshot encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("vfe screenshot pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &output_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: clear[0],
                            g: clear[1],
                            b: clear[2],
                            a: clear[3],
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.record(&mut pass, scene);
        }
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &output,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(Some(encoder.finish()));
        Ok(readback)
    }

    /// Render `scene` offscreen and read it back synchronously (native).
    ///
    /// `map_async` is followed by `poll(Wait)` so the mapping cannot race the
    /// copy; one 4k frame is a short stall on native, where the save dialog
    /// runs on a worker thread afterwards.
    #[cfg(not(target_arch = "wasm32"))]
    #[allow(clippy::too_many_arguments)]
    pub fn render_offscreen(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &SceneGPU,
        view_proj: Mat4,
        camera_pos: Vec3,
        width: u32,
        height: u32,
        clear: [f64; 4],
    ) -> Result<Vec<u8>, String> {
        let readback = self.submit_offscreen(
            device, queue, scene, view_proj, camera_pos, width, height, clear,
        )?;

        let slice = readback.slice(..);
        let (mapped_tx, mapped_rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = mapped_tx.send(result);
        });
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|error| format!("readback poll failed: {error}"))?;
        mapped_rx
            .recv()
            .map_err(|error| format!("readback callback lost: {error}"))?
            .map_err(|error| format!("readback mapping failed: {error}"))?;

        let pixels = {
            let mapped = slice
                .get_mapped_range()
                .map_err(|error| format!("readback range unavailable: {error}"))?;
            crate::screenshot::strip_row_padding(&mapped, width, height)
        };
        readback.unmap();

        crate::screenshot::to_rgba(self.format, pixels)
    }

    /// Submit an offscreen capture and complete without blocking the browser
    /// main thread. `map_async` resolves through the WebGPU promise on the
    /// event loop (`Device::poll` is a no-op on wasm); `completion` receives
    /// the RGBA bytes — or the mapping error — with no `VfApp` borrow.
    #[cfg(target_arch = "wasm32")]
    #[allow(clippy::too_many_arguments)]
    pub fn render_offscreen_async(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &SceneGPU,
        view_proj: Mat4,
        camera_pos: Vec3,
        width: u32,
        height: u32,
        clear: [f64; 4],
        completion: impl FnOnce(Result<Vec<u8>, String>) + Send + 'static,
    ) -> Result<(), String> {
        let readback = self.submit_offscreen(
            device, queue, scene, view_proj, camera_pos, width, height, clear,
        )?;
        let buffer = readback.clone();
        let format = self.format;
        readback.slice(..).map_async(wgpu::MapMode::Read, move |result| {
            completion(readback_result(result, &buffer, format, width, height));
        });
        Ok(())
    }
}

/// Turn a wasm map callback into RGBA bytes, unmapping the buffer on every
/// path. Kept outside the callback so the failure paths stay readable.
#[cfg(target_arch = "wasm32")]
fn readback_result(
    result: Result<(), wgpu::BufferAsyncError>,
    buffer: &wgpu::Buffer,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, String> {
    result.map_err(|error| format!("readback mapping failed: {error:?}"))?;
    let mapped = match buffer.get_mapped_range(..) {
        Ok(mapped) => mapped,
        Err(error) => {
            buffer.unmap();
            return Err(format!("readback range unavailable: {error}"));
        }
    };
    let pixels = crate::screenshot::strip_row_padding(&mapped, width, height);
    drop(mapped);
    buffer.unmap();
    crate::screenshot::to_rgba(format, pixels)
}

/// Per-frame draw: carries the scene to upload and the camera matrices.
pub struct DrawCallback {
    pub scene: SceneGPU,
    pub view_proj: Mat4,
    pub camera_pos: Vec3,
}

impl egui_wgpu::CallbackTrait for DrawCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let ds = callback_resources
            .get_mut::<DrawState>()
            .expect("DrawState must be installed before the first paint callback");
        ds.upload(device, queue, self.view_proj, self.camera_pos, &self.scene);
        Vec::new()
    }

    fn paint(
        &self,
        _info: eframe::egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &egui_wgpu::CallbackResources,
    ) {
        let ds = callback_resources
            .get::<DrawState>()
            .expect("DrawState must be installed before the first paint callback");

        ds.record(render_pass, &self.scene);
    }
}

/// Painter's-algorithm helper: order arrow instances far-to-near along the
/// view direction, since the egui_wgpu pass has no depth attachment.
pub fn sort_back_to_front(instances: &mut [ArrowInstanceGPU], view: &Mat4) {
    instances.sort_by(|a, b| {
        let pa = Vec3::new(a.origin[0], a.origin[1], a.origin[2]);
        let pb = Vec3::new(b.origin[0], b.origin[1], b.origin[2]);
        let za = (*view * pa.extend(1.0)).z;
        let zb = (*view * pb.extend(1.0)).z;
        za.partial_cmp(&zb).unwrap_or(std::cmp::Ordering::Equal)
    });
}
