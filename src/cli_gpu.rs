//! Surface-free native wgpu backend. All sizing is checked before GPU allocation.
use std::sync::{Arc, Mutex};

use eframe::wgpu;

use super::{CliError, PreparedScene};
use crate::renderer::{ArrowInstanceGPU, DrawState, Globals, LineInstanceGPU, OffscreenError};

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
    let (width, height) = (prepared.frame.width, prepared.frame.height);
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
    let scene = &prepared.scene;
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

/// Render packed, display-encoded RGBA8 without a surface, GUI or CPU fallback.
pub fn render_offscreen(prepared: &PreparedScene) -> Result<Vec<u8>, CliError> {
    // Reject invalid requests even when no GPU exists. Adapter limits are checked
    // again before requesting a device and before creating renderer resources.
    validate_gpu_limits(prepared, &wgpu::Limits::unlimited())?;
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
        "CLI PNG adapter: {} ({:?}, {:?})",
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
    let sizes = validate_sizes(prepared, &limits)?;
    // No optional features, compute, textures, samplers or vertex buffers. Size
    // requests to this scene, retain baseline limits for otherwise unused fields,
    // and check every requested limit against the adapter before requesting.
    let required_limits = wgpu::Limits {
        max_texture_dimension_1d: 1,
        max_texture_dimension_2d: prepared.frame.width.max(prepared.frame.height),
        max_texture_dimension_3d: 1,
        max_texture_array_layers: 1,
        max_bind_groups: 1,
        max_bind_groups_plus_vertex_buffers: 1,
        max_bindings_per_bind_group: 2,
        max_dynamic_uniform_buffers_per_pipeline_layout: 0,
        max_dynamic_storage_buffers_per_pipeline_layout: 0,
        max_sampled_textures_per_shader_stage: 0,
        max_samplers_per_shader_stage: 0,
        max_storage_buffers_per_shader_stage: 1,
        max_storage_textures_per_shader_stage: 0,
        max_uniform_buffers_per_shader_stage: 1,
        max_uniform_buffer_binding_size: size_of::<Globals>() as u64,
        max_storage_buffer_binding_size: sizes.storage,
        max_buffer_size: sizes
            .readback
            .max(sizes.storage)
            .max(size_of::<Globals>() as u64),
        max_vertex_buffers: 0,
        max_vertex_attributes: 0,
        max_vertex_buffer_array_stride: 0,
        max_inter_stage_shader_variables: 4,
        max_color_attachments: 1,
        max_color_attachment_bytes_per_sample: 8,
        max_buffers_and_acceleration_structures_per_shader_stage: 2,
        ..wgpu::Limits::downlevel_webgl2_defaults().using_alignment(limits.clone())
    };
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
            label: Some("vfe CLI PNG device"),
            required_features: wgpu::Features::empty(),
            required_limits,
            ..Default::default()
        }))
        .map_err(|error| format!("adapter '{}': {error}", info.name))
    })?;
    validate_gpu_limits(prepared, &device.limits())?;
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
    let oom = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    let internal = device.push_error_scope(wgpu::ErrorFilter::Internal);
    let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut draw = DrawState::new(&device, format);
    let result = draw
        .render_offscreen(
            &device,
            &queue,
            &prepared.scene,
            prepared.frame.view_proj,
            prepared.frame.eye,
            prepared.frame.width,
            prepared.frame.height,
            prepared.frame.background_linear,
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
    let errors = failures.lock().unwrap();
    if !errors.is_empty() {
        return Err(CliError::Render(errors.join("; ")));
    }
    result
}
