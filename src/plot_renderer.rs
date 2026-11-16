//! Depth-equipped scalar plot renderer, prepare-time targets and composite.
//!
//! Egui's paint pass has no depth attachment, so scalar geometry is rendered
//! into a plot-owned single-sample color [`wgpu::TextureFormat`] target with a
//! `Depth32Float` attachment during `CallbackTrait::prepare`, then composited
//! into the incoming egui pass during `paint` through a separate no-depth
//! pipeline. The vector renderer and its no-depth line pipeline are unchanged.
//!
//! Pure configuration seams ([`plot_target_extent`], [`plot_draw_order`],
//! [`plot_pipeline_policy`], [`prepared_matches`], format preflight) are public
//! and exercised by CPU-only tests. Pipeline creation, prepare, paint and the
//! capture owner consume those same production helpers. These tests never
//! create a wgpu device; the native device gate is a separate task.
//!
//! A [`PlotDrawState`] is installed per renderer/device and owns its targets and
//! buffers. [`PlotTargetKey`] deliberately does not identify resources across
//! devices: no cache is shared between [`PlotDrawState`] objects.

pub mod capture;

use std::sync::{Arc, Mutex};

use eframe::wgpu::util::DeviceExt;
use eframe::{egui, egui_wgpu, wgpu};

use crate::{
    plot::PlotKind,
    plot_camera::PlotFrame,
    plot_scene::{PlotGeometry, PlotScene, PlotVertex},
    renderer::{self, Globals, LineInstanceGPU},
};

/// Color target usages required by the live plot pass and its composite.
const PLOT_COLOR_USAGES: wgpu::TextureUsages = wgpu::TextureUsages::RENDER_ATTACHMENT
    .union(wgpu::TextureUsages::TEXTURE_BINDING)
    .union(wgpu::TextureUsages::COPY_SRC);
/// Depth target usage required by the live plot pass.
const PLOT_DEPTH_USAGES: wgpu::TextureUsages = wgpu::TextureUsages::RENDER_ATTACHMENT;
/// Depth format used by every plot geometry pipeline.
const PLOT_DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Physical identity of the plot-owned render target.
///
/// The key is only meaningful inside the [`PlotDrawState`] that created the
/// resources; object identity scopes them to one device lifetime.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlotTargetKey {
    pub extent: [u32; 2],
    pub format: wgpu::TextureFormat,
    pub composite_samples: u32,
}

/// One draw in the scalar plot pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlotDraw {
    Mesh,
    References,
    Curve,
}

/// The scalar draw plan for a kind. `VectorField` has no plot draw plan, so the
/// plot path can never fall back to vector geometry (carried finding M2).
pub fn plot_draw_order(kind: PlotKind) -> &'static [PlotDraw] {
    const SURFACE: &[PlotDraw] = &[PlotDraw::Mesh, PlotDraw::References];
    const CURVE: &[PlotDraw] = &[PlotDraw::References, PlotDraw::Curve];
    const NONE: &[PlotDraw] = &[];
    match kind {
        PlotKind::VectorField => NONE,
        PlotKind::Surface3D => SURFACE,
        PlotKind::Curve2D => CURVE,
    }
}

/// The immutable pipeline set owned by a plot renderer object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlotPipeline {
    Mesh,
    References,
    Curve,
    Composite,
}

/// Production pipeline configuration consumed by actual pipeline creation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlotPipelinePolicy {
    /// `(compare, depth_write_enabled)`; `None` means no depth attachment.
    pub depth: Option<(wgpu::CompareFunction, bool)>,
    pub cull: Option<wgpu::Face>,
    pub front_face: wgpu::FrontFace,
    pub blend: Option<wgpu::BlendState>,
}

/// The one policy table behind pipeline creation and its CPU tests.
pub fn plot_pipeline_policy(pipeline: PlotPipeline) -> PlotPipelinePolicy {
    match pipeline {
        // Opaque lit surface: CCW front faces, back-face culled, depth tested
        // and written so nearer geometry occludes farther geometry.
        PlotPipeline::Mesh => PlotPipelinePolicy {
            depth: Some((wgpu::CompareFunction::LessEqual, true)),
            cull: Some(wgpu::Face::Back),
            front_face: wgpu::FrontFace::Ccw,
            blend: None,
        },
        // References test depth but never write it, so they can be occluded by
        // the opaque surface while remaining visible in front of it.
        PlotPipeline::References => PlotPipelinePolicy {
            depth: Some((wgpu::CompareFunction::LessEqual, false)),
            cull: None,
            front_face: wgpu::FrontFace::Ccw,
            blend: Some(renderer::LINE_BLEND),
        },
        // The curve tests and writes depth; drawn after references it overlays
        // its same-plane grid at equal depth.
        PlotPipeline::Curve => PlotPipelinePolicy {
            depth: Some((wgpu::CompareFunction::LessEqual, true)),
            cull: None,
            front_face: wgpu::FrontFace::Ccw,
            blend: Some(renderer::LINE_BLEND),
        },
        // Composite runs in egui's no-depth pass and replaces the incoming
        // color; it inherits the incoming target's sample count at creation.
        PlotPipeline::Composite => PlotPipelinePolicy {
            depth: None,
            cull: None,
            front_face: wgpu::FrontFace::Ccw,
            blend: None,
        },
    }
}

impl PlotPipeline {
    /// Sample count used when creating this pipeline: plot attachments are
    /// always single-sample, while the composite matches the incoming target.
    pub fn sample_count(self, composite_samples: u32) -> u32 {
        match self {
            Self::Composite => composite_samples,
            Self::Mesh | Self::References | Self::Curve => 1,
        }
    }
}

/// Paint is allowed only for the exact generation/key that `prepare` recorded.
/// `None` means no preparation (released, resized or zero-area): nothing paints.
pub fn prepared_matches(
    prepared: Option<(u64, PlotTargetKey)>,
    generation: u64,
    key: PlotTargetKey,
) -> bool {
    prepared == Some((generation, key))
}

/// Adapter capabilities for the chosen incoming color format and the fixed
/// `Depth32Float` plot attachment. Queried by the app and validated here before
/// any pipeline or target creation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlotFormatCapabilities {
    pub color: wgpu::TextureFormatFeatures,
    pub depth: wgpu::TextureFormatFeatures,
}

impl PlotFormatCapabilities {
    /// Validate the live target + composite requirements. The plot color target
    /// must be a render+sample+copy attachment, filterable for the composite
    /// sampler, and support both the single-sample geometry pass and the
    /// incoming pass's sample count. Depth must be a depth render attachment.
    pub fn validate_plot(&self, composite_samples: u32) -> Result<(), String> {
        require_usages("plot color", self.color.allowed_usages, PLOT_COLOR_USAGES)?;
        require_usages("plot depth", self.depth.allowed_usages, PLOT_DEPTH_USAGES)?;
        if !self
            .color
            .flags
            .contains(wgpu::TextureFormatFeatureFlags::FILTERABLE)
        {
            return Err("plot color format is not filterable for the composite sampler".into());
        }
        if !self.color.flags.sample_count_supported(1) {
            return Err("plot color format does not support its single-sample target".into());
        }
        if !self.color.flags.sample_count_supported(composite_samples) {
            return Err(format!(
                "plot color format does not support composite sample count {composite_samples}"
            ));
        }
        Ok(())
    }

    /// Validate the capture requirements: render into a single-sample color
    /// target and copy it to a readback buffer. No sampler is used.
    pub fn validate_capture(&self) -> Result<(), String> {
        let capture_color = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC;
        require_usages("capture color", self.color.allowed_usages, capture_color)?;
        require_usages(
            "capture depth",
            self.depth.allowed_usages,
            PLOT_DEPTH_USAGES,
        )?;
        if !self.color.flags.sample_count_supported(1) {
            return Err("capture color format does not support a single-sample target".into());
        }
        Ok(())
    }
}

fn require_usages(
    label: &str,
    available: wgpu::TextureUsages,
    required: wgpu::TextureUsages,
) -> Result<(), String> {
    let missing = required - available;
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{label} format is missing required usage {missing:?}"
        ))
    }
}

/// Convert the callback's logical rect to the physical plot target extent.
///
/// The first step is exactly egui's `ViewportInPixels::from_points` rounding and
/// screen clamping, so the composite and the recorded viewport agree. A
/// zero-area result has no target. A target longer than `max_edge` is scaled on
/// both axes by the same factor (never stretched independently).
pub fn plot_target_extent(
    rect: egui::Rect,
    pixels_per_point: f32,
    screen_size: [u32; 2],
    max_edge: u32,
) -> Option<[u32; 2]> {
    let viewport =
        egui::epaint::ViewportInPixels::from_points(&rect, pixels_per_point, screen_size);
    let width = u32::try_from(viewport.width_px).ok()?;
    let height = u32::try_from(viewport.height_px).ok()?;
    if width == 0 || height == 0 {
        return None;
    }
    let longest = width.max(height);
    let cap = max_edge.max(1);
    if longest <= cap {
        return Some([width, height]);
    }
    let scale = f64::from(cap) / f64::from(longest);
    let scaled = |value: u32| -> u32 { ((f64::from(value) * scale).round() as u32).max(1) };
    Some([scaled(width), scaled(height)])
}

/// Immutable plot pipelines. Both the live renderer and the capture owner build
/// one per device; they never share mutable or device-bound state.
struct PlotPipelines {
    mesh: wgpu::RenderPipeline,
    references: wgpu::RenderPipeline,
    curve: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
    scene_layout: wgpu::BindGroupLayout,
    composite_layout: wgpu::BindGroupLayout,
    composite_sampler: wgpu::Sampler,
}

impl PlotPipelines {
    fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        composite_samples: u32,
    ) -> Arc<Self> {
        let mesh_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vfe plot mesh shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders.plot.wgsl").into()),
        });
        // Deliberately the unchanged vector line shader, used with new
        // depth-enabled pipeline variants only. The legacy no-depth line
        // pipeline is never bound in the plot pass.
        let line_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vfe plot line shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders.line.wgsl").into()),
        });
        let composite_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vfe plot composite shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders.plot_composite.wgsl").into()),
        });

        let scene_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vfe plot globals+instances bgl"),
            entries: &[
                uniform_entry(0),
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
        let scene_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("vfe plot pipeline layout"),
                bind_group_layouts: &[Some(&scene_layout)],
                immediate_size: 0,
            });

        let composite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vfe plot composite bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let composite_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("vfe plot composite pipeline layout"),
                bind_group_layouts: &[Some(&composite_layout)],
                immediate_size: 0,
            });

        let mesh = create_pipeline(
            device,
            "vfe plot mesh pipeline",
            &scene_pipeline_layout,
            &mesh_shader,
            ("vs_mesh", "fs_mesh"),
            PlotPipeline::Mesh,
            &[Some(plot_vertex_layout())],
            format,
            composite_samples,
        );
        let references = create_pipeline(
            device,
            "vfe plot references pipeline",
            &scene_pipeline_layout,
            &line_shader,
            ("vs_line", "fs_line"),
            PlotPipeline::References,
            &[],
            format,
            composite_samples,
        );
        let curve = create_pipeline(
            device,
            "vfe plot curve pipeline",
            &scene_pipeline_layout,
            &line_shader,
            ("vs_line", "fs_line"),
            PlotPipeline::Curve,
            &[],
            format,
            composite_samples,
        );
        let composite = create_pipeline(
            device,
            "vfe plot composite pipeline",
            &composite_pipeline_layout,
            &composite_shader,
            ("vs_composite", "fs_composite"),
            PlotPipeline::Composite,
            &[],
            format,
            composite_samples,
        );

        let composite_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("vfe plot composite sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Arc::new(Self {
            mesh,
            references,
            curve,
            composite,
            scene_layout,
            composite_layout,
            composite_sampler,
        })
    }
}

fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

#[allow(clippy::too_many_arguments)]
fn create_pipeline(
    device: &wgpu::Device,
    label: &str,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    entry_points: (&str, &str),
    pipeline: PlotPipeline,
    vertex_buffers: &[Option<wgpu::VertexBufferLayout<'_>>],
    format: wgpu::TextureFormat,
    composite_samples: u32,
) -> wgpu::RenderPipeline {
    let policy = plot_pipeline_policy(pipeline);
    let sample_count = pipeline.sample_count(composite_samples);
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(entry_points.0),
            compilation_options: Default::default(),
            buffers: vertex_buffers,
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(entry_points.1),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: policy.blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            front_face: policy.front_face,
            cull_mode: policy.cull,
            ..Default::default()
        },
        depth_stencil: policy
            .depth
            .map(|(compare, write)| wgpu::DepthStencilState {
                format: PLOT_DEPTH_FORMAT,
                depth_write_enabled: Some(write),
                depth_compare: Some(compare),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
        multisample: wgpu::MultisampleState {
            count: sample_count,
            ..Default::default()
        },
        multiview_mask: None,
        cache: None,
    })
}

fn plot_vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    debug_assert_eq!(std::mem::size_of::<PlotVertex>(), 48);
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<PlotVertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 16,
                shader_location: 1,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 32,
                shader_location: 2,
            },
        ],
    }
}

/// Mutable per-renderer plot buffers and their bind groups. Live and capture
/// each own one: mutable buffers are never shared between them.
pub struct PlotPassResources {
    pipelines: Arc<PlotPipelines>,
    globals_buf: wgpu::Buffer,
    placeholder_instances: wgpu::Buffer,
    vertex_buf: wgpu::Buffer,
    index_buf: wgpu::Buffer,
    index_count: u32,
    references_buf: wgpu::Buffer,
    curve_buf: wgpu::Buffer,
    references_count: u32,
    curve_count: u32,
    mesh_bind: wgpu::BindGroup,
    references_bind: wgpu::BindGroup,
    curve_bind: wgpu::BindGroup,
    cached: Option<CachedGeometry>,
}

#[derive(Clone)]
struct CachedGeometry {
    generation: u64,
    curve: Option<Arc<Vec<LineInstanceGPU>>>,
    vertices: Option<Arc<Vec<PlotVertex>>>,
    indices: Option<Arc<Vec<u32>>>,
    references: Arc<Vec<LineInstanceGPU>>,
}

fn arc_eq<T>(owned: &Option<Arc<T>>, incoming: &Option<&Arc<T>>) -> bool {
    match (owned, incoming) {
        (Some(owned), Some(incoming)) => Arc::ptr_eq(owned, incoming),
        (None, None) => true,
        _ => false,
    }
}

impl PlotPassResources {
    fn new(device: &wgpu::Device, pipelines: &Arc<PlotPipelines>) -> Self {
        let globals_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vfe plot globals"),
            contents: bytemuck::bytes_of(&zero_globals()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        // Storage bindings cannot be zero-sized, so empty geometry binds this
        // one-instance placeholder and simply records no draw.
        let placeholder_instances = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vfe plot placeholder instances"),
            contents: bytemuck::bytes_of(&LineInstanceGPU {
                a: [0.0; 4],
                b: [0.0; 4],
                color: [0.0; 4],
            }),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vfe plot placeholder vertices"),
            contents: bytemuck::bytes_of(&PlotVertex {
                position: [0.0; 4],
                normal: [0.0; 4],
                color: [0.0; 4],
            }),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vfe plot placeholder indices"),
            contents: bytemuck::bytes_of(&0u32),
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        });
        let mesh_bind = make_bind_group(
            device,
            "vfe plot mesh bind",
            &pipelines.scene_layout,
            &globals_buf,
            &placeholder_instances,
        );
        let references_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vfe plot placeholder references"),
            contents: bytemuck::bytes_of(&LineInstanceGPU {
                a: [0.0; 4],
                b: [0.0; 4],
                color: [0.0; 4],
            }),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        let curve_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vfe plot placeholder curve"),
            contents: bytemuck::bytes_of(&LineInstanceGPU {
                a: [0.0; 4],
                b: [0.0; 4],
                color: [0.0; 4],
            }),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        let references_bind = make_bind_group(
            device,
            "vfe plot references bind",
            &pipelines.scene_layout,
            &globals_buf,
            &references_buf,
        );
        let curve_bind = make_bind_group(
            device,
            "vfe plot curve bind",
            &pipelines.scene_layout,
            &globals_buf,
            &curve_buf,
        );
        Self {
            pipelines: Arc::clone(pipelines),
            globals_buf,
            placeholder_instances,
            vertex_buf,
            index_buf,
            index_count: 0,
            references_buf,
            curve_buf,
            references_count: 0,
            curve_count: 0,
            mesh_bind,
            references_bind,
            curve_bind,
            cached: None,
        }
    }

    /// Upload this frame's uniforms; mutable buffers are per-frame writes of the
    /// same immutable layout the vector renderer uses.
    fn write_globals(&self, queue: &wgpu::Queue, frame: PlotFrame) {
        let (camera_right, camera_up) = renderer::camera_basis_from_view_proj(&frame.view_proj);
        let globals = Globals {
            view_proj: frame.view_proj.to_cols_array_2d(),
            inv_view_proj: frame.view_proj.inverse().to_cols_array_2d(),
            camera_pos: frame.eye.extend(1.0).to_array(),
            light_dir: renderer::LIGHT_DIR,
            camera_right: camera_right.extend(0.0).to_array(),
            camera_up: camera_up.extend(0.0).to_array(),
            arrow_style: [0; 4],
            arrow_params: [0.0; 4],
        };
        queue.write_buffer(&self.globals_buf, 0, bytemuck::bytes_of(&globals));
    }

    /// Re-upload immutable geometry only when the scene generation or its Arc
    /// payload changes; uniforms are still updated every frame by the caller.
    fn upload_scene(&mut self, device: &wgpu::Device, scene: &PlotScene) {
        let (curve, vertices, indices) = match &scene.geometry {
            PlotGeometry::Curve(curve) => (Some(curve), None, None),
            PlotGeometry::Surface(vertices, indices) => (None, Some(vertices), Some(indices)),
        };
        let unchanged = self.cached.as_ref().is_some_and(|cached| {
            cached.generation == scene.generation
                && arc_eq(&cached.curve, &curve)
                && arc_eq(&cached.vertices, &vertices)
                && arc_eq(&cached.indices, &indices)
                && Arc::ptr_eq(&cached.references, &scene.references)
        });
        if unchanged {
            return;
        }

        if !scene.references.is_empty() {
            self.references_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("vfe plot references (scene)"),
                contents: bytemuck::cast_slice(scene.references.as_slice()),
                usage: wgpu::BufferUsages::STORAGE,
            });
            self.references_count = scene.references.len() as u32;
        } else {
            self.references_count = 0;
        }
        let references_buffer = if self.references_count > 0 {
            &self.references_buf
        } else {
            &self.placeholder_instances
        };
        self.references_bind = make_bind_group(
            device,
            "vfe plot references bind (scene)",
            &self.pipelines.scene_layout,
            &self.globals_buf,
            references_buffer,
        );

        if let Some(curve) = curve {
            if !curve.is_empty() {
                self.curve_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("vfe plot curve (scene)"),
                    contents: bytemuck::cast_slice(curve.as_slice()),
                    usage: wgpu::BufferUsages::STORAGE,
                });
                self.curve_count = curve.len() as u32;
            } else {
                self.curve_count = 0;
            }
        } else {
            self.curve_count = 0;
        }
        let curve_buffer = if self.curve_count > 0 {
            &self.curve_buf
        } else {
            &self.placeholder_instances
        };
        self.curve_bind = make_bind_group(
            device,
            "vfe plot curve bind (scene)",
            &self.pipelines.scene_layout,
            &self.globals_buf,
            curve_buffer,
        );

        match (vertices, indices) {
            (Some(vertices), Some(indices)) if !vertices.is_empty() && !indices.is_empty() => {
                self.vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("vfe plot mesh vertices (scene)"),
                    contents: bytemuck::cast_slice(vertices.as_slice()),
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                });
                self.index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("vfe plot mesh indices (scene)"),
                    contents: bytemuck::cast_slice(indices.as_slice()),
                    usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                });
                self.index_count = indices.len() as u32;
            }
            _ => self.index_count = 0,
        }

        self.cached = Some(CachedGeometry {
            generation: scene.generation,
            curve: curve.cloned(),
            vertices: vertices.cloned(),
            indices: indices.cloned(),
            references: Arc::clone(&scene.references),
        });
    }

    fn record(&self, pass: &mut wgpu::RenderPass<'_>, scene: &PlotScene) {
        for draw in plot_draw_order(scene.kind) {
            match draw {
                PlotDraw::Mesh => {
                    if self.index_count == 0 {
                        continue;
                    }
                    pass.set_pipeline(&self.pipelines.mesh);
                    pass.set_bind_group(0, &self.mesh_bind, &[]);
                    pass.set_vertex_buffer(0, self.vertex_buf.slice(..));
                    pass.set_index_buffer(self.index_buf.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..self.index_count, 0, 0..1);
                }
                PlotDraw::References => {
                    if self.references_count == 0 {
                        continue;
                    }
                    pass.set_pipeline(&self.pipelines.references);
                    pass.set_bind_group(0, &self.references_bind, &[]);
                    pass.draw(0..self.references_count * 6, 0..1);
                }
                PlotDraw::Curve => {
                    if self.curve_count == 0 {
                        continue;
                    }
                    pass.set_pipeline(&self.pipelines.curve);
                    pass.set_bind_group(0, &self.curve_bind, &[]);
                    pass.draw(0..self.curve_count * 6, 0..1);
                }
            }
        }
    }
}

fn zero_globals() -> Globals {
    Globals {
        view_proj: glam::Mat4::IDENTITY.to_cols_array_2d(),
        inv_view_proj: glam::Mat4::IDENTITY.to_cols_array_2d(),
        camera_pos: [0.0; 4],
        light_dir: renderer::LIGHT_DIR,
        camera_right: [1.0, 0.0, 0.0, 0.0],
        camera_up: [0.0, 1.0, 0.0, 0.0],
        arrow_style: [0; 4],
        arrow_params: [0.0; 4],
    }
}

fn make_bind_group(
    device: &wgpu::Device,
    label: &str,
    layout: &wgpu::BindGroupLayout,
    globals: &wgpu::Buffer,
    instances: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: instances.as_entire_binding(),
            },
        ],
    })
}

/// Record the one scalar geometry pass shared by the live viewport and capture.
/// Color is cleared to the supplied linear background (opaque) and depth to 1.0.
pub fn record_plot_pass(
    encoder: &mut wgpu::CommandEncoder,
    resources: &PlotPassResources,
    scene: &PlotScene,
    color: &wgpu::TextureView,
    depth: &wgpu::TextureView,
    clear: [f64; 4],
) {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("vfe plot geometry pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: color,
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
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view: depth,
            depth_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Clear(1.0),
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        }),
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    resources.record(&mut pass, scene);
}

struct PlotTarget {
    key: PlotTargetKey,
    _color: wgpu::Texture,
    color_view: wgpu::TextureView,
    _depth: wgpu::Texture,
    depth_view: wgpu::TextureView,
    composite_bind: wgpu::BindGroup,
}

fn create_target(
    device: &wgpu::Device,
    pipelines: &PlotPipelines,
    key: PlotTargetKey,
) -> Result<PlotTarget, String> {
    let [width, height] = key.extent;
    if width == 0 || height == 0 {
        return Err("plot target extent must be nonzero".into());
    }
    let limits = device.limits();
    if width > limits.max_texture_dimension_2d || height > limits.max_texture_dimension_2d {
        return Err(format!(
            "plot target {}x{} exceeds the device texture limit {}",
            width, height, limits.max_texture_dimension_2d
        ));
    }
    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let color = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("vfe plot color target"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: key.format,
        usage: PLOT_COLOR_USAGES,
        view_formats: &[],
    });
    let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("vfe plot depth target"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: PLOT_DEPTH_FORMAT,
        usage: PLOT_DEPTH_USAGES,
        view_formats: &[],
    });
    let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
    let composite_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("vfe plot composite bind"),
        layout: &pipelines.composite_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&color_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&pipelines.composite_sampler),
            },
        ],
    });
    Ok(PlotTarget {
        key,
        _color: color,
        color_view,
        _depth: depth,
        depth_view,
        composite_bind,
    })
}

/// Installed in egui_wgpu callback resources. Owns the live plot pipelines,
/// uniform/geometry buffers, and the current color/depth target.
pub struct PlotDrawState {
    format: wgpu::TextureFormat,
    composite_samples: u32,
    max_edge: u32,
    pipelines: Arc<PlotPipelines>,
    pass: PlotPassResources,
    target: Option<PlotTarget>,
    prepared: Option<(u64, PlotTargetKey)>,
}

impl PlotDrawState {
    /// Validate the chosen formats/capabilities and build the plot pipelines.
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        composite_samples: u32,
        capabilities: PlotFormatCapabilities,
    ) -> Result<Self, String> {
        capabilities.validate_plot(composite_samples)?;
        let pipelines = PlotPipelines::new(device, format, composite_samples);
        let pass = PlotPassResources::new(device, &pipelines);
        Ok(Self {
            format,
            composite_samples,
            max_edge: device.limits().max_texture_dimension_2d,
            pipelines,
            pass,
            target: None,
            prepared: None,
        })
    }

    /// Derive this renderer's target key from a callback viewport using the
    /// exact egui rounding plus this device's texture-edge cap.
    pub fn target_key(
        &self,
        rect: egui::Rect,
        pixels_per_point: f32,
        screen_size: [u32; 2],
    ) -> Option<PlotTargetKey> {
        let extent = plot_target_extent(rect, pixels_per_point, screen_size, self.max_edge)?;
        Some(PlotTargetKey {
            extent,
            format: self.format,
            composite_samples: self.composite_samples,
        })
    }

    /// Allocate/reuse the target, upload this frame's uniforms and changed
    /// geometry, then record the plot pass into the supplied egui encoder. The
    /// prepared generation/key becomes visible to [`Self::paint`] only on success.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        scene: &PlotScene,
        frame: PlotFrame,
        key: PlotTargetKey,
        clear: [f64; 4],
    ) -> Result<(), String> {
        self.prepared = None;
        if key.format != self.format || key.composite_samples != self.composite_samples {
            return Err("plot target key does not match this renderer's format/samples".into());
        }
        // Carried M2: the vector kind has no plot draw plan. Do not allocate a
        // target or draw anything, so the plot path cannot substitute vector
        // geometry for a scalar plot.
        if plot_draw_order(scene.kind).is_empty() {
            return Ok(());
        }
        if key.extent[0] == 0 || key.extent[1] == 0 {
            return Err("plot target extent must be nonzero".into());
        }
        if self.target.as_ref().map(|target| target.key) != Some(key) {
            // Build the replacement before dropping the old target so submitted
            // commands keep referencing their own resource set.
            let target = create_target(device, &self.pipelines, key)?;
            self.target = Some(target);
        }
        self.pass.write_globals(queue, frame);
        self.pass.upload_scene(device, scene);
        let target = self.target.as_ref().expect("plot target was just ensured");
        record_plot_pass(
            encoder,
            &self.pass,
            scene,
            &target.color_view,
            &target.depth_view,
            clear,
        );
        self.prepared = Some((scene.generation, key));
        Ok(())
    }

    /// Composite the prepared target into the incoming egui pass. Anything
    /// other than the exact prepared generation/key draws nothing.
    pub fn paint(&self, pass: &mut wgpu::RenderPass<'static>, generation: u64, key: PlotTargetKey) {
        if !prepared_matches(self.prepared, generation, key) {
            return;
        }
        let Some(target) = &self.target else {
            return;
        };
        pass.set_pipeline(&self.pipelines.composite);
        pass.set_bind_group(0, &target.composite_bind, &[]);
        pass.draw(0..3, 0..1);
    }

    /// Leaving-plot cleanup: invalidate the prepared identity and drop the
    /// large color/depth targets and composite bind group.
    pub fn release_targets(&mut self) {
        self.prepared = None;
        self.target = None;
    }
}

/// Per-frame plot callback. Prepare derives the target key from the egui screen
/// descriptor and records into the supplied encoder; paint composites only the
/// matching prepared resources. Callback traits cannot return errors, so status
/// is published through the shared slot for the app to read.
pub struct PlotCallback {
    pub scene: Arc<PlotScene>,
    pub frame: PlotFrame,
    pub rect: egui::Rect,
    pub clear: [f64; 4],
    pub errors: Arc<Mutex<Option<String>>>,
}

impl PlotCallback {
    fn publish_error(&self, message: impl Into<String>) {
        if let Ok(mut slot) = self.errors.lock() {
            *slot = Some(message.into());
        }
    }

    fn clear_error(&self) {
        if let Ok(mut slot) = self.errors.lock() {
            *slot = None;
        }
    }
}

impl egui_wgpu::CallbackTrait for PlotCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen_descriptor: &egui_wgpu::ScreenDescriptor,
        egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(state) = callback_resources.get_mut::<PlotDrawState>() else {
            self.publish_error("plot renderer is not installed before its paint callback");
            return Vec::new();
        };
        let Some(key) = state.target_key(
            self.rect,
            screen_descriptor.pixels_per_point,
            screen_descriptor.size_in_pixels,
        ) else {
            // Zero-area viewport: paint nothing rather than sizing a texture
            // from an empty rect.
            state.release_targets();
            self.clear_error();
            return Vec::new();
        };
        match state.prepare(
            device,
            queue,
            egui_encoder,
            &self.scene,
            self.frame,
            key,
            self.clear,
        ) {
            Ok(()) => self.clear_error(),
            Err(error) => self.publish_error(error),
        }
        Vec::new()
    }

    fn paint(
        &self,
        info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &egui_wgpu::CallbackResources,
    ) {
        let Some(state) = callback_resources.get::<PlotDrawState>() else {
            return;
        };
        let Some(key) = state.target_key(info.viewport, info.pixels_per_point, info.screen_size_px)
        else {
            return;
        };
        state.paint(render_pass, self.scene.generation, key);
    }
}
