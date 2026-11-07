//! Instanced wgpu renderer for arrows and lines (main plan Task 7).
//!
//! Three pipelines (arrow + line + dot) share one bind-group layout holding the
//! `Globals` uniform and the per-mode instance storage array. The `DrawState`
//! lives in the egui_wgpu callback_resources TypeMap (installed once by the
//! app); each frame a `DrawCallback` uploads the current `SceneGPU` and draws.
//!
//! Depth note: egui_wgpu paint callbacks have no depth attachment, so arrows
//! are opaque REPLACE-blend and are CPU-sorted far-to-near
//! (painter's algorithm) by the caller.

use std::sync::Arc;

use eframe::egui_wgpu;
use eframe::wgpu;
use eframe::wgpu::util::DeviceExt;
use glam::{Mat4, Vec3};

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
}

/// The GPU scene uploaded each frame. `Arc` lets the app skip re-uploads when
/// a collection is unchanged (dots reuse the arrow instance layout with a
/// negligible head).
#[derive(Clone, Default)]
pub struct SceneGPU {
    pub arrows: Arc<Vec<ArrowInstanceGPU>>,
    pub lines: Arc<Vec<LineInstanceGPU>>,
    pub dots: Arc<Vec<ArrowInstanceGPU>>,
}

impl SceneGPU {
    pub fn is_empty(&self) -> bool {
        self.arrows.is_empty() && self.lines.is_empty() && self.dots.is_empty()
    }
}

const LIGHT_DIR: [f32; 4] = [0.4211, 0.8521, 0.3108, 0.0]; // normalize(0.42, 0.85, 0.31)

fn mat4_to_mats(m: Mat4) -> [[f32; 4]; 4] {
    m.to_cols_array_2d()
}

/// One-time GPU resources: pipelines, layouts, and the (per-frame rewritten)
/// buffers. Installed into the egui_wgpu callback_resources TypeMap.
pub struct DrawState {
    bind_layout: wgpu::BindGroupLayout,
    arrow_pipeline: wgpu::RenderPipeline,
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
}

impl DrawState {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader_arrow = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vfe arrow shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders.arrow.wgsl").into()),
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
        let globals = Globals {
            view_proj: mat4_to_mats(view_proj),
            inv_view_proj: mat4_to_mats(view_proj.inverse()),
            camera_pos: [camera_pos.x, camera_pos.y, camera_pos.z, 1.0],
            light_dir: self.light_dir,
        };
        queue.write_buffer(&self.globals_buf, 0, bytemuck::bytes_of(&globals));

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

        if !self.scene.lines.is_empty() {
            render_pass.set_pipeline(&ds.line_pipeline);
            render_pass.set_bind_group(0, &ds.line_bind, &[]);
            render_pass.draw(0..(self.scene.lines.len() as u32) * 6, 0..1);
        }

        if !self.scene.arrows.is_empty() {
            render_pass.set_pipeline(&ds.arrow_pipeline);
            render_pass.set_bind_group(0, &ds.arrow_bind, &[]);
            render_pass.draw(0..(self.scene.arrows.len() as u32) * 54, 0..1);
        }

        if !self.scene.dots.is_empty() {
            render_pass.set_pipeline(&ds.dot_pipeline);
            render_pass.set_bind_group(0, &ds.dot_bind, &[]);
            render_pass.draw(0..(self.scene.dots.len() as u32) * 24, 0..1);
        }
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
