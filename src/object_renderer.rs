//! Offscreen, depth-enabled object pass for mapped source bodies (#112/#113).
//!
//! This module owns a colour + `Depth32Float` target, uploads the mapped
//! `MeshScene` instances, records the lit object pass into a caller-supplied
//! encoder, and composites the result with a fullscreen textured quad. The
//! renderer's `DrawCallback` and `submit_offscreen` drive it before the existing
//! vector draws, so the live viewport, screenshots and animation frames share
//! one image. It does not touch `SceneGPU` or the vector draw plans.
//!
//! The composite writes premultiplied alpha. The target is accumulated with
//! straight-alpha blending while bodies are accumulated, so premultiplying once
//! at composite time is only exact while every object's `opacity == 1.0`; a
//! later transparent-object slice must premultiply per object instead.
//!
//! Like the scalar plot pass, `prepare` is device-forward: the caller supplies
//! the egui command encoder, so this module never submits work itself.

use eframe::wgpu::util::DeviceExt;
use eframe::{egui, wgpu};
use glam::{Mat3, Mat4};

use crate::object_scene::{
    MeshObject, MeshPrimitive, MeshScene, MeshVertex, SliceClip, primitive_mesh,
};

/// Environment variable that turns the native device probe on for its child
/// test process only.
pub const OBJECT_PROBE_ENV: &str = "VFE_OBJECT_GPU_PROBE";

/// Depth format of the object target.
pub const OBJECT_DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// World-space direction *toward* the single directional light.
pub const OBJECT_LIGHT_DIR: [f32; 3] = [0.4211, 0.8521, 0.3108];

const OBJECT_COLOR_USAGES: wgpu::TextureUsages =
    wgpu::TextureUsages::RENDER_ATTACHMENT.union(wgpu::TextureUsages::TEXTURE_BINDING);
const OBJECT_DEPTH_USAGES: wgpu::TextureUsages =
    wgpu::TextureUsages::RENDER_ATTACHMENT.union(wgpu::TextureUsages::TEXTURE_BINDING);

/// Physical identity of the object-owned render target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjectTargetKey {
    pub extent: [u32; 2],
    pub format: wgpu::TextureFormat,
}

/// One frame's camera, viewport and style for the object pass.
///
/// `view_proj` is a WebGPU `[0, 1]`-depth world-to-clip transform. `viewport`
/// is in physical pixels; a zero-sized axis means there is nothing to draw.
/// `clear` is the linear colour the owned target is cleared to, matching the
/// plot pass convention.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObjectView {
    pub view_proj: [[f32; 4]; 4],
    pub eye: [f32; 3],
    pub viewport: [u32; 2],
    pub wireframe: bool,
    pub clear: [f64; 4],
}

impl ObjectView {
    /// The owned target extent, or `None` when either axis is zero.
    #[must_use]
    pub fn target_extent(&self) -> Option<[u32; 2]> {
        let [width, height] = self.viewport;
        if width == 0 || height == 0 {
            None
        } else {
            Some([width, height])
        }
    }
}

/// Uniform ABI shared by the object vertex and fragment stages (128 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ObjectGlobals {
    pub view_proj: [[f32; 4]; 4],
    /// Camera position, `w = 1`.
    pub eye: [f32; 4],
    /// Visible half-space: `xyz` normal, `w` offset. Active only when
    /// `params.y == 1`.
    pub clip_plane: [f32; 4],
    /// Direction *toward* the light, `w = 0`.
    pub light_dir: [f32; 4],
    /// `x` = wireframe flag, `y` = slice-clip enabled, `zw` unused.
    pub params: [f32; 4],
}

impl ObjectGlobals {
    /// The identity/no-scene uniform.
    #[must_use]
    pub fn zero() -> Self {
        Self {
            view_proj: Mat4::IDENTITY.to_cols_array_2d(),
            eye: [0.0, 0.0, 0.0, 1.0],
            clip_plane: [0.0, 0.0, 0.0, 0.0],
            light_dir: [
                OBJECT_LIGHT_DIR[0],
                OBJECT_LIGHT_DIR[1],
                OBJECT_LIGHT_DIR[2],
                0.0,
            ],
            params: [0.0; 4],
        }
    }

    /// Pack one view (and optional slice clip) into finite uniform values.
    ///
    /// A non-finite view matrix is replaced by the identity, non-finite eye
    /// components by zero, and a non-finite or degenerate clip plane disables
    /// the discard. No NaN or infinity can reach the GPU from this function.
    #[must_use]
    pub fn from_view(view: &ObjectView, clip: Option<SliceClip>) -> Self {
        let view_proj = if view
            .view_proj
            .concat()
            .iter()
            .all(|value| value.is_finite())
        {
            view.view_proj
        } else {
            Mat4::IDENTITY.to_cols_array_2d()
        };
        let eye = [
            finite_or(view.eye[0], 0.0),
            finite_or(view.eye[1], 0.0),
            finite_or(view.eye[2], 0.0),
            1.0,
        ];
        let (clip_plane, clip_enabled) = match clip {
            Some(clip) => {
                let mut normal = [0.0; 3];
                let mut finite = true;
                for (target, value) in normal.iter_mut().zip(clip.normal) {
                    if value.is_finite() {
                        *target = value;
                    } else {
                        finite = false;
                    }
                }
                let offset = finite_or(clip.offset, 0.0);
                let length =
                    (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
                if finite && length > 1e-6 && clip.offset.is_finite() {
                    ([normal[0], normal[1], normal[2], offset], 1.0)
                } else {
                    ([0.0; 4], 0.0)
                }
            }
            None => ([0.0; 4], 0.0),
        };
        Self {
            view_proj,
            eye,
            clip_plane,
            light_dir: [
                OBJECT_LIGHT_DIR[0],
                OBJECT_LIGHT_DIR[1],
                OBJECT_LIGHT_DIR[2],
                0.0,
            ],
            params: [
                if view.wireframe { 1.0 } else { 0.0 },
                clip_enabled,
                0.0,
                0.0,
            ],
        }
    }
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

/// One uploaded instance: model matrix, inverse-transpose normal matrix and
/// linear colour (128 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ObjectInstance {
    pub model: [[f32; 4]; 4],
    pub normal_0: [f32; 4],
    pub normal_1: [f32; 4],
    pub normal_2: [f32; 4],
    pub color: [f32; 4],
}

impl ObjectInstance {
    /// Sanitize one mapped object for the GPU.
    ///
    /// Returns `None` for a non-finite or singular transform, a non-invertible
    /// linear part or a non-finite colour/opacity, so no invalid instance is
    /// ever uploaded. Finite values are used as-is except that colour channels
    /// and opacity are clamped to `0..=1`.
    #[must_use]
    pub fn from_object(object: &MeshObject) -> Option<Self> {
        let model = Mat4::from_cols_array_2d(&object.transform);
        if !model.is_finite() {
            return None;
        }
        let linear = Mat3::from_mat4(model);
        let determinant = linear.determinant();
        if !determinant.is_finite() || determinant.abs() <= f32::EPSILON {
            return None;
        }
        let normal = linear.inverse().transpose();
        if !normal.is_finite() {
            return None;
        }
        if !object.color.iter().all(|channel| channel.is_finite()) || !object.opacity.is_finite() {
            return None;
        }
        let channel = |value: f32| value.clamp(0.0, 1.0);
        let color = [
            channel(object.color[0]),
            channel(object.color[1]),
            channel(object.color[2]),
            channel(object.opacity),
        ];
        Some(Self {
            model: model.to_cols_array_2d(),
            normal_0: [normal.x_axis.x, normal.x_axis.y, normal.x_axis.z, 0.0],
            normal_1: [normal.y_axis.x, normal.y_axis.y, normal.y_axis.z, 0.0],
            normal_2: [normal.z_axis.x, normal.z_axis.y, normal.z_axis.z, 0.0],
            color,
        })
    }
}

/// The device-free decision `prepare` makes before allocating anything.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectDrawPlan {
    /// Target extent in physical pixels; both axes are nonzero.
    pub extent: [u32; 2],
    /// Drawable instances in deterministic scene order, with the primitive
    /// each one references.
    pub draws: Vec<(MeshPrimitive, ObjectInstance)>,
}

/// Plan one frame without a device: `None` for an empty scene or a zero-area
/// viewport, otherwise the target extent and the sanitized draw list.
#[must_use]
pub fn object_draw_plan(scene: &MeshScene, view: &ObjectView) -> Option<ObjectDrawPlan> {
    let extent = view.target_extent()?;
    if scene.objects().is_empty() {
        return None;
    }
    let draws = scene
        .objects()
        .iter()
        .filter_map(|object| {
            ObjectInstance::from_object(object).map(|instance| (object.primitive, instance))
        })
        .collect();
    Some(ObjectDrawPlan { extent, draws })
}

/// Installed in egui_wgpu callback resources by a later integration slice.
pub struct ObjectDrawState {
    pub pass: ObjectPass,
}

struct ObjectPipelines {
    object: wgpu::RenderPipeline,
    object_attenuated: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
    scene_layout: wgpu::BindGroupLayout,
    composite_layout: wgpu::BindGroupLayout,
    grid_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

impl ObjectPipelines {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vfe object shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders.object.wgsl").into()),
        });
        let scene_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vfe object globals bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let scene_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("vfe object pipeline layout"),
                bind_group_layouts: &[Some(&scene_layout)],
                immediate_size: 0,
            });
        let grid_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vfe grid depth layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let composite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vfe object composite bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let composite_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("vfe object composite pipeline layout"),
                bind_group_layouts: &[Some(&composite_layout)],
                immediate_size: 0,
            });

        let object = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vfe object pipeline"),
            layout: Some(&scene_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_object"),
                compilation_options: Default::default(),
                buffers: &[Some(mesh_vertex_layout()), Some(instance_vertex_layout())],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_object"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                // Two-sided lighting: bodies are never culled.
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: OBJECT_DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        // Same geometry, layout and depth target as the body pipeline; only the
        // fragment stage flips the slice discard and draws the discarded side
        // as an attenuated shaded body. It never writes depth, so it cannot
        // hide the authoritative on-slice body or the composite.
        let object_attenuated = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vfe object attenuated discarded-side pipeline"),
            layout: Some(&scene_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_object"),
                compilation_options: Default::default(),
                buffers: &[Some(mesh_vertex_layout()), Some(instance_vertex_layout())],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_object_attenuated"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: OBJECT_DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let composite = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vfe object composite pipeline"),
            layout: Some(&composite_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_composite"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_composite"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("vfe object composite sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Self {
            object,
            object_attenuated,
            composite,
            scene_layout,
            composite_layout,
            grid_layout,
            sampler,
        }
    }
}

/// Pack the static mesh into the exact 36-byte vertex stride (nine `f32`).
///
/// `MeshVertex` is not a bytemuck `Pod` type and that slice must not change its
/// public layout, so the upload copies the nine fields explicitly.
fn vertex_bytes(vertices: &[MeshVertex]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(vertices.len() * 36);
    for vertex in vertices {
        for value in vertex
            .position
            .iter()
            .chain(&vertex.normal)
            .chain(&vertex.barycentric)
        {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    bytes
}

fn mesh_vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    debug_assert_eq!(std::mem::size_of::<MeshVertex>(), 36);
    wgpu::VertexBufferLayout {
        array_stride: 36,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 12,
                shader_location: 1,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 24,
                shader_location: 2,
            },
        ],
    }
}

fn instance_vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    debug_assert_eq!(std::mem::size_of::<ObjectInstance>(), 128);
    wgpu::VertexBufferLayout {
        array_stride: 128,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &[
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 0,
                shader_location: 3,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 16,
                shader_location: 4,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 32,
                shader_location: 5,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 48,
                shader_location: 6,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 64,
                shader_location: 7,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 80,
                shader_location: 8,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 96,
                shader_location: 9,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 112,
                shader_location: 10,
            },
        ],
    }
}

struct ObjectTarget {
    key: ObjectTargetKey,
    _color: wgpu::Texture,
    color_view: wgpu::TextureView,
    _depth: wgpu::Texture,
    depth_view: wgpu::TextureView,
    composite_bind: wgpu::BindGroup,
    grid_bind: wgpu::BindGroup,
}

fn create_target(
    device: &wgpu::Device,
    pipelines: &ObjectPipelines,
    key: ObjectTargetKey,
) -> Result<ObjectTarget, String> {
    let [width, height] = key.extent;
    if width == 0 || height == 0 {
        return Err("object target extent must be nonzero".into());
    }
    let limits = device.limits();
    if width > limits.max_texture_dimension_2d || height > limits.max_texture_dimension_2d {
        return Err(format!(
            "object target {width}x{height} exceeds the device texture limit {}",
            limits.max_texture_dimension_2d
        ));
    }
    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let color = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("vfe object color target"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: key.format,
        usage: OBJECT_COLOR_USAGES,
        view_formats: &[],
    });
    let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("vfe object depth target"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: OBJECT_DEPTH_FORMAT,
        usage: OBJECT_DEPTH_USAGES,
        view_formats: &[],
    });
    let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
    let grid_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("vfe grid depth bind"),
        layout: &pipelines.grid_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(&depth_view),
        }],
    });
    let composite_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("vfe object composite bind"),
        layout: &pipelines.composite_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&color_view),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(&pipelines.sampler),
            },
        ],
    });
    Ok(ObjectTarget {
        key,
        _color: color,
        color_view,
        _depth: depth,
        depth_view,
        composite_bind,
        grid_bind,
    })
}

fn make_scene_bind(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    globals: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("vfe object scene bind"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: globals.as_entire_binding(),
        }],
    })
}

fn sanitized_clear(clear: [f64; 4]) -> wgpu::Color {
    let channel = |value: f64| {
        if value.is_finite() {
            value.clamp(0.0, 1.0)
        } else {
            0.0
        }
    };
    wgpu::Color {
        r: channel(clear[0]),
        g: channel(clear[1]),
        b: channel(clear[2]),
        a: channel(clear[3]),
    }
}

/// Lazy offscreen object target, pipelines and per-scene instance buffers.
pub struct ObjectPass {
    format: wgpu::TextureFormat,
    pipelines: ObjectPipelines,
    globals_buf: wgpu::Buffer,
    globals_cache: ObjectGlobals,
    scene_bind: wgpu::BindGroup,
    instance_buf: Option<wgpu::Buffer>,
    instance_generation: Option<u64>,
    instance_count: usize,
    primitive_bufs: [Option<wgpu::Buffer>; 5],
    draws: Vec<(MeshPrimitive, u32)>,
    target: Option<ObjectTarget>,
    prepared: Option<[u32; 2]>,
}

impl ObjectPass {
    /// Build the pipelines, the composite sampler and the uniform buffer. The
    /// owned colour/depth target and the instance buffers stay unallocated
    /// until a non-empty scene with a nonzero viewport is prepared.
    #[must_use]
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let pipelines = ObjectPipelines::new(device, target_format);
        let globals = ObjectGlobals::zero();
        let globals_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vfe object globals"),
            contents: bytemuck::bytes_of(&globals),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let scene_bind = make_scene_bind(device, &pipelines.scene_layout, &globals_buf);
        Self {
            format: target_format,
            pipelines,
            globals_buf,
            globals_cache: globals,
            scene_bind,
            instance_buf: None,
            instance_generation: None,
            instance_count: 0,
            primitive_bufs: std::array::from_fn(|_| None),
            draws: Vec::new(),
            target: None,
            prepared: None,
        }
    }

    /// The prepared colour target, when a non-empty frame was recorded.
    #[must_use]
    pub fn texture_view(&self) -> Option<&wgpu::TextureView> {
        self.target.as_ref().map(|target| &target.color_view)
    }

    pub(crate) fn grid_layout(&self) -> &wgpu::BindGroupLayout {
        &self.pipelines.grid_layout
    }

    pub(crate) fn grid_depth(&self) -> Option<&wgpu::BindGroup> {
        self.prepared?;
        if self.globals_cache.params[0] > 0.5 {
            return None;
        }
        // All mapped bodies have unit opacity. Attenuated slice fragments
        // never write this depth attachment; neither should hide a grid line.
        self.target.as_ref().map(|target| &target.grid_bind)
    }

    /// Allocate/reuse the target, upload changed instances and uniforms, then
    /// record the object pass into the supplied encoder.
    ///
    /// An empty scene or a zero-area viewport returns `Ok` without allocating a
    /// target or recording anything. The instance buffer is uploaded only when
    /// the scene generation (or drawable count) changes; the uniform buffer
    /// only when the packed values differ. The prepared frame becomes visible
    /// to [`Self::paint`] only on success.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        scene: &MeshScene,
        view: &ObjectView,
    ) -> Result<(), String> {
        self.prepared = None;
        let Some(plan) = object_draw_plan(scene, view) else {
            return Ok(());
        };
        if plan.draws.is_empty() {
            return Ok(());
        }
        let key = ObjectTargetKey {
            extent: plan.extent,
            format: self.format,
        };
        if self.target.as_ref().map(|target| target.key) != Some(key) {
            // Build the replacement before dropping the old target so submitted
            // commands keep referencing their own resource set.
            self.target = Some(create_target(device, &self.pipelines, key)?);
        }

        if self.instance_generation != Some(scene.generation())
            || self.instance_buf.is_none()
            || self.instance_count != plan.draws.len()
        {
            let instances: Vec<ObjectInstance> =
                plan.draws.iter().map(|(_, instance)| *instance).collect();
            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("vfe object instances"),
                contents: bytemuck::cast_slice(instances.as_slice()),
                usage: wgpu::BufferUsages::VERTEX,
            });
            self.instance_buf = Some(buffer);
            self.instance_generation = Some(scene.generation());
            self.instance_count = plan.draws.len();
        }

        if self.primitive_bufs.iter().any(Option::is_none) {
            for (slot, primitive) in self.primitive_bufs.iter_mut().zip(MeshPrimitive::ALL) {
                let bytes = vertex_bytes(primitive_mesh(primitive).vertices);
                *slot = Some(
                    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("vfe object primitive mesh"),
                        contents: &bytes,
                        usage: wgpu::BufferUsages::VERTEX,
                    }),
                );
            }
        }

        let globals = ObjectGlobals::from_view(view, scene.clip());
        if globals != self.globals_cache {
            queue.write_buffer(&self.globals_buf, 0, bytemuck::bytes_of(&globals));
            self.globals_cache = globals;
        }

        self.draws = plan
            .draws
            .iter()
            .enumerate()
            .map(|(index, (primitive, _))| (*primitive, index as u32))
            .collect();
        self.record(encoder, sanitized_clear(view.clear));
        self.prepared = Some(plan.extent);
        Ok(())
    }

    fn record(&self, encoder: &mut wgpu::CommandEncoder, clear: wgpu::Color) {
        let (Some(target), Some(instance_buf)) = (self.target.as_ref(), self.instance_buf.as_ref())
        else {
            return;
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("vfe object geometry pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target.color_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &target.depth_view,
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
        pass.set_pipeline(&self.pipelines.object);
        pass.set_bind_group(0, &self.scene_bind, &[]);
        let instance_stride = std::mem::size_of::<ObjectInstance>() as u64;
        for (primitive, instance_index) in &self.draws {
            let Some(mesh) = self.primitive_bufs[primitive.index() as usize].as_ref() else {
                continue;
            };
            let offset = u64::from(*instance_index) * instance_stride;
            pass.set_vertex_buffer(0, mesh.slice(..));
            pass.set_vertex_buffer(1, instance_buf.slice(offset..));
            pass.draw(0..primitive.vertices() as u32, 0..1);
        }

        // Issue #207: one extra attenuated shaded draw over the same instances,
        // keeping only the discarded side of the slice. No new vertices or
        // buffers; the attenuated pipeline never writes depth, so the kept half
        // stays authoritative.
        if self.globals_cache.params[1] > 0.5 {
            pass.set_pipeline(&self.pipelines.object_attenuated);
            pass.set_bind_group(0, &self.scene_bind, &[]);
            for (primitive, instance_index) in &self.draws {
                let Some(mesh) = self.primitive_bufs[primitive.index() as usize].as_ref() else {
                    continue;
                };
                let offset = u64::from(*instance_index) * instance_stride;
                pass.set_vertex_buffer(0, mesh.slice(..));
                pass.set_vertex_buffer(1, instance_buf.slice(offset..));
                pass.draw(0..primitive.vertices() as u32, 0..1);
            }
        }
    }

    /// Composite the prepared target into the incoming egui pass as a
    /// fullscreen textured quad over the pass's *current* viewport.
    ///
    /// `paint` deliberately never calls `set_viewport`: egui_wgpu sets the pass
    /// viewport to the callback rect before invoking a paint callback and
    /// resets it to the full surface before the next egui primitive, so the
    /// composite already lands on the object rect and the shared pass state is
    /// left exactly as the host set it. wgpu exposes no render-pass viewport
    /// getter, so that host contract cannot be observed from a unit test; it is
    /// recorded here and in `docs/CODEBASE.md`, and `tests/object_gpu_test.rs`
    /// pins the full-target composite. Nothing is drawn without a successful
    /// `prepare` or with a zero/negative/inverted rect.
    pub fn paint(&self, pass: &mut wgpu::RenderPass<'_>, screen: egui::Rect) {
        if self.prepared.is_none() {
            return;
        }
        let Some(target) = &self.target else {
            return;
        };
        let (width, height) = (screen.width(), screen.height());
        if !screen.min.x.is_finite()
            || !screen.min.y.is_finite()
            || !width.is_finite()
            || !height.is_finite()
            || width <= 0.0
            || height <= 0.0
        {
            return;
        }
        pass.set_pipeline(&self.pipelines.composite);
        pass.set_bind_group(0, &target.composite_bind, &[]);
        pass.draw(0..3, 0..1);
    }
}
