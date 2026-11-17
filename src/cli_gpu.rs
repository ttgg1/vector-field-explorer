//! Surface-free native wgpu backend. All sizing is checked before GPU allocation.
use std::sync::{Arc, Mutex};

use eframe::wgpu;
use glam::{Mat4, Vec3};

use super::{CliError, PreparedScene};
use crate::animation_scene::AnimationFrame;
use crate::renderer::{
    ArrowInstanceGPU, DrawState, Globals, LineInstanceGPU, OffscreenError, SceneGPU,
};

#[doc(hidden)]
pub fn offscreen_error_to_cli(error: OffscreenError) -> CliError {
    match error {
        OffscreenError::Setup(_) => CliError::Render(error.to_string()),
        OffscreenError::Readback(_) => CliError::Readback(error.to_string()),
    }
}

struct ResourceSizes {
    readback: u64,
    storage: u64,
}

/// Check the renderer's actual resource requirements without allocating a GPU.
#[doc(hidden)]
pub fn validate_gpu_limits(
    prepared: &PreparedScene,
    limits: &wgpu::Limits,
) -> Result<(), CliError> {
    validate_sizes(prepared, limits).map(|_| ())
}

/// Invoke device acquisition once and retain its original diagnostic context.
#[doc(hidden)]
pub fn acquire_device_with<T, E: std::fmt::Display>(
    request: impl FnOnce() -> Result<T, E>,
) -> Result<T, CliError> {
    request()
        .map_err(|error| CliError::Device(format!("headless PNG device request failed: {error}")))
}

fn validate_sizes(
    prepared: &PreparedScene,
    limits: &wgpu::Limits,
) -> Result<ResourceSizes, CliError> {
    validate_frame_sizes(
        prepared.frame.width,
        prepared.frame.height,
        &prepared.scene,
        limits,
    )
}

fn validate_frame_sizes(
    width: u32,
    height: u32,
    scene: &SceneGPU,
    limits: &wgpu::Limits,
) -> Result<ResourceSizes, CliError> {
    let pixels = u64::from(width) * u64::from(height);
    if !(1..=4096).contains(&width) || !(1..=4096).contains(&height) || pixels > 16_777_216 {
        return Err(CliError::Validation(
            "PNG dimensions must be 1..=4096 with at most 16,777,216 pixels".into(),
        ));
    }
    if width.max(height) > limits.max_texture_dimension_2d {
        return Err(CliError::Device(format!(
            "PNG dimensions {width}x{height} exceed adapter/device texture limit {}",
            limits.max_texture_dimension_2d
        )));
    }
    let packed = pixels
        .checked_mul(4)
        .ok_or_else(|| CliError::Validation("PNG pixel bytes overflow".into()))?;
    let padded = u64::from(crate::screenshot::padded_row_bytes(width)) * u64::from(height);
    if usize::try_from(packed).is_err() || padded > limits.max_buffer_size {
        return Err(CliError::Device(format!(
            "PNG readback needs {padded} bytes; adapter/device buffer limit is {}",
            limits.max_buffer_size
        )));
    }
    let arrows = if scene.mode_3d {
        scene.combined.len()
    } else {
        scene.arrows.len()
    };
    let mut storage = 0;
    for (name, count, stride, vertices) in [
        (
            "arrows/combined",
            arrows,
            size_of::<ArrowInstanceGPU>(),
            scene.arrow_vertex_count(),
        ),
        ("lines", scene.lines.len(), size_of::<LineInstanceGPU>(), 6),
        (
            "trails",
            scene.trails.len(),
            size_of::<LineInstanceGPU>(),
            6,
        ),
        (
            "dots",
            if scene.mode_3d { 0 } else { scene.dots.len() },
            size_of::<ArrowInstanceGPU>(),
            6,
        ),
    ] {
        let bytes = count
            .max(1)
            .checked_mul(stride)
            .and_then(|n| u64::try_from(n).ok());
        if count > (u32::MAX / vertices) as usize
            || bytes.is_none_or(|n| {
                n > limits.max_buffer_size || n > limits.max_storage_buffer_binding_size
            })
        {
            return Err(CliError::Device(format!(
                "PNG {name} storage buffer exceeds adapter/device limits"
            )));
        }
        storage = storage.max(bytes.unwrap());
    }
    if limits.max_storage_buffers_per_shader_stage < 1
        || limits.max_uniform_buffers_per_shader_stage < 1
        || limits.max_uniform_buffer_binding_size < size_of::<Globals>() as u64
    {
        return Err(CliError::Device(
            "adapter/device lacks renderer storage/uniform binding limits".into(),
        ));
    }
    Ok(ResourceSizes {
        readback: padded,
        storage,
    })
}

/// One initialized headless GPU session that can render many frames.
pub struct HeadlessRenderSession {
    device: wgpu::Device,
    queue: wgpu::Queue,
    draw: DrawState,
    failures: Arc<Mutex<Vec<String>>>,
    limits: wgpu::Limits,
}

impl HeadlessRenderSession {
    /// Acquire one adapter/device sized for many animation frames.
    pub fn new() -> Result<Self, CliError> {
        Self::initialize(None)
    }

    /// Acquire a session sized to one prepared still scene, preserving the
    /// original one-shot PNG device request.
    #[doc(hidden)]
    pub fn for_scene(prepared: &PreparedScene) -> Result<Self, CliError> {
        Self::initialize(Some(prepared))
    }

    fn initialize(initial: Option<&PreparedScene>) -> Result<Self, CliError> {
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
        // Even if explicitly enabled in wgpu's environment, noop cannot produce
        // image content and is not a rendering fallback.
        descriptor.backends.remove(wgpu::Backends::NOOP);
        let instance = wgpu::Instance::new(descriptor);
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: None,
            ..Default::default()
        })).map_err(|e| CliError::Adapter(format!("no usable headless wgpu adapter: {e}; PNG requires a working GPU driver (no CPU fallback)")))?;
        let info = adapter.get_info();
        log::info!(
            "CLI offscreen adapter: {} ({:?}, {:?})",
            info.name,
            info.backend,
            info.device_type
        );
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let capabilities = adapter.get_texture_format_features(format);
        if !capabilities
            .allowed_usages
            .contains(wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC)
            || !capabilities
                .flags
                .contains(wgpu::TextureFormatFeatureFlags::BLENDABLE)
        {
            return Err(CliError::Adapter("adapter cannot render/blend/copy Rgba8UnormSrgb; refusing to change PNG color semantics".into()));
        }
        let limits = adapter.limits();
        let still_sizes = initial
            .map(|prepared| validate_sizes(prepared, &limits))
            .transpose()?;
        // A streaming session outlives one scene, so request the adapter's
        // storage/buffer maxima instead of sizing to a first frame that later
        // frames may exceed. Each frame is still checked against the device
        // limits. The still wrapper keeps its original minimal request.
        let mut required_limits =
            wgpu::Limits::downlevel_webgl2_defaults().using_alignment(limits.clone());
        required_limits.max_texture_dimension_1d = 1;
        required_limits.max_texture_dimension_2d = limits.max_texture_dimension_2d.max(1);
        required_limits.max_texture_dimension_3d = 1;
        required_limits.max_texture_array_layers = 1;
        required_limits.max_bind_groups = 1;
        required_limits.max_bind_groups_plus_vertex_buffers = 1;
        required_limits.max_bindings_per_bind_group = 2;
        required_limits.max_dynamic_uniform_buffers_per_pipeline_layout = 0;
        required_limits.max_dynamic_storage_buffers_per_pipeline_layout = 0;
        required_limits.max_sampled_textures_per_shader_stage = 0;
        required_limits.max_samplers_per_shader_stage = 0;
        required_limits.max_storage_buffers_per_shader_stage = 1;
        required_limits.max_storage_textures_per_shader_stage = 0;
        required_limits.max_uniform_buffers_per_shader_stage = 1;
        required_limits.max_uniform_buffer_binding_size = size_of::<Globals>() as u64;
        required_limits.max_storage_buffer_binding_size = limits.max_storage_buffer_binding_size;
        required_limits.max_buffer_size = limits.max_buffer_size;
        required_limits.max_vertex_buffers = 0;
        required_limits.max_vertex_attributes = 0;
        required_limits.max_vertex_buffer_array_stride = 0;
        required_limits.max_inter_stage_shader_variables = 4;
        required_limits.max_color_attachments = 1;
        required_limits.max_color_attachment_bytes_per_sample = 8;
        required_limits.max_buffers_and_acceleration_structures_per_shader_stage = 2;
        if let Some(prepared) = initial {
            let sizes = still_sizes.expect("sizes were validated above");
            required_limits.max_texture_dimension_2d =
                prepared.frame.width.max(prepared.frame.height);
            required_limits.max_storage_buffer_binding_size = sizes.storage;
            required_limits.max_buffer_size = sizes
                .readback
                .max(sizes.storage)
                .max(size_of::<Globals>() as u64);
        }
        let mut unsupported = Vec::new();
        required_limits.check_limits_with_fail_fn(&limits, false, |name, requested, supported| {
            unsupported.push(format!(
                "{name}: required {requested}, adapter supports {supported}"
            ));
        });
        if !unsupported.is_empty() {
            return Err(CliError::Device(format!(
                "adapter cannot satisfy PNG renderer limits: {}",
                unsupported.join("; ")
            )));
        }
        let (device, queue) = acquire_device_with(|| {
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("vfe CLI offscreen device"),
                required_features: wgpu::Features::empty(),
                required_limits,
                ..Default::default()
            }))
            .map_err(|error| format!("adapter '{}': {error}", info.name))
        })?;
        let failures = Arc::new(Mutex::new(Vec::new()));
        let errors = failures.clone();
        device.on_uncaptured_error(Arc::new(move |error: wgpu::Error| {
            errors
                .lock()
                .unwrap()
                .push(format!("uncaptured GPU error: {error}"));
        }));
        let errors = failures.clone();
        device.set_device_lost_callback(move |reason, message| {
            errors
                .lock()
                .unwrap()
                .push(format!("GPU device lost ({reason:?}): {message}"));
        });
        let draw = DrawState::new(&device, format);
        let limits = device.limits();
        Ok(Self {
            device,
            queue,
            draw,
            failures,
            limits,
        })
    }

    /// Render packed, display-encoded RGBA8 without a surface or GUI.
    pub fn render_frame(
        &mut self,
        scene: &SceneGPU,
        view_proj: Mat4,
        camera_pos: Vec3,
        width: u32,
        height: u32,
        clear: [f64; 4],
    ) -> Result<Vec<u8>, CliError> {
        validate_frame_sizes(width, height, scene, &self.limits)?;
        let device = &self.device;
        let queue = &self.queue;
        let failures = Arc::clone(&self.failures);
        let oom = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let internal = device.push_error_scope(wgpu::ErrorFilter::Internal);
        let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let result = self
            .draw
            .render_offscreen(
                device, queue, scene, view_proj, camera_pos, width, height, clear,
            )
            .map_err(offscreen_error_to_cli);
        for (stage, scope) in [
            ("validation", validation),
            ("internal", internal),
            ("out of memory", oom),
        ] {
            if let Some(error) = pollster::block_on(scope.pop()) {
                failures
                    .lock()
                    .unwrap()
                    .push(format!("PNG GPU {stage} error: {error}"));
            }
        }
        let mut errors = failures.lock().unwrap();
        if !errors.is_empty() {
            let message = errors.join("; ");
            errors.clear();
            return Err(CliError::Render(message));
        }
        result
    }
}

/// One frame source for the CLI animation loop; tests substitute a fake.
pub trait AnimationRenderSession {
    /// Render one detached frame for the requested dimensions and clear colour.
    fn render_animation_frame(
        &mut self,
        frame: &AnimationFrame,
        width: u32,
        height: u32,
        clear: [f64; 4],
    ) -> Result<Vec<u8>, CliError>;
}

impl AnimationRenderSession for HeadlessRenderSession {
    fn render_animation_frame(
        &mut self,
        frame: &AnimationFrame,
        width: u32,
        height: u32,
        clear: [f64; 4],
    ) -> Result<Vec<u8>, CliError> {
        self.render_frame(
            &frame.scene,
            frame.view_proj,
            frame.eye,
            width,
            height,
            clear,
        )
    }
}

/// Render one prepared scene through a fresh headless session.
///
/// Still PNG keeps its one-shot wrapper; animation uses one
/// [`HeadlessRenderSession`] for every frame.
pub fn render_offscreen(prepared: &PreparedScene) -> Result<Vec<u8>, CliError> {
    // Reject invalid requests even when no GPU exists. Adapter limits are checked
    // again before requesting a device and before creating renderer resources.
    validate_gpu_limits(prepared, &wgpu::Limits::unlimited())?;
    let mut session = HeadlessRenderSession::for_scene(prepared)?;
    session.render_frame(
        &prepared.scene,
        prepared.frame.view_proj,
        prepared.frame.eye,
        prepared.frame.width,
        prepared.frame.height,
        prepared.frame.background_linear,
    )
}
