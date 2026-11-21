//! Adapter-gated proof that the heatmap texture binding and pipeline render.
//!
//! Set `VFE_HEATMAP_GPU_PROBE=1` to request a real headless adapter. Without
//! the variable, or when no adapter exists, the probe prints a clear skip
//! message and returns instead of failing.

use std::sync::Arc;

use eframe::wgpu;
use glam::{Mat4, UVec3, Vec3};

use vector_field_explorer::{
    color_mapping::{ColorMapping, ColorPalette, ColorQuantity},
    color_ramp::ScaleMode,
    colormap::Colormap,
    domain::Domain,
    plane::{PlaneAxis, PlaneFrame, PlaneView},
    renderer::{DrawState, HeatmapGPU, SceneGPU},
    scalar_view::sample_grid,
};

fn probe_grid() -> vector_field_explorer::scalar_view::ScalarGrid {
    let frame = PlaneFrame::new(
        &Domain::new(Vec3::new(-1.0, -1.0, 0.0), Vec3::new(1.0, 1.0, 0.0)),
        PlaneView {
            enabled: false,
            axis: PlaneAxis::Z,
            index: 0,
        },
        UVec3::splat(2),
    );
    let expression = vector_field_explorer::scalar::ScalarExpr::parse(
        "x",
        vector_field_explorer::scalar::ScalarContext::Surface,
    )
    .expect("valid probe formula");
    sample_grid(&expression, frame, 8, 8, 0.0).expect("valid probe grid")
}

fn probe_mapping() -> ColorMapping {
    ColorMapping {
        palette: ColorPalette::Builtin(Colormap::Turbo),
        mode: ScaleMode::Linear,
        quantity: ColorQuantity::Magnitude,
        manual_range: Some((-1.0, 1.0)),
    }
}

#[test]
fn heatmap_device_probe() {
    if std::env::var_os("VFE_HEATMAP_GPU_PROBE").is_none() {
        eprintln!(
            "heatmap GPU probe skipped: set VFE_HEATMAP_GPU_PROBE=1 to request a real adapter"
        );
        return;
    }
    match run_probe() {
        Ok(()) => {}
        Err(ProbeError::Unavailable(error)) => {
            eprintln!("heatmap GPU probe skipped: {error}");
        }
        Err(ProbeError::Failed(error)) => {
            panic!("heatmap GPU probe failed: {error}");
        }
    }
}

enum ProbeError {
    Unavailable(String),
    Failed(String),
}

fn run_probe() -> Result<(), ProbeError> {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
    descriptor.backends.remove(wgpu::Backends::NOOP);
    let instance = wgpu::Instance::new(descriptor);
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        compatible_surface: None,
        ..Default::default()
    }))
    .map_err(|error| ProbeError::Unavailable(format!("no real headless wgpu adapter: {error}")))?;
    let info = adapter.get_info();
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("vfe heatmap probe device"),
        required_features: wgpu::Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .map_err(|error| {
        ProbeError::Unavailable(format!(
            "adapter '{}' device request failed: {error}",
            info.name
        ))
    })?;

    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let mut draw = DrawState::new(&device, format);
    let heatmap = HeatmapGPU::from_grid(&probe_grid(), &probe_mapping())
        .ok_or_else(|| ProbeError::Failed("probe grid produced no heatmap".into()))?;
    let identity = heatmap.identity;
    let scene = SceneGPU {
        mode_3d: false,
        heatmap: Some(Arc::new(heatmap)),
        ..Default::default()
    };
    let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let pixels = draw
        .render_offscreen(
            &device,
            &queue,
            &scene,
            Mat4::IDENTITY,
            Vec3::new(0.0, 0.0, 5.0),
            64,
            64,
            [0.0, 0.0, 0.0, 1.0],
        )
        .map_err(|error| ProbeError::Failed(format!("offscreen render failed: {error}")))?;
    if let Some(error) = pollster::block_on(validation.pop()) {
        return Err(ProbeError::Failed(format!(
            "heatmap GPU validation error: {error}"
        )));
    }
    if draw.resident_heatmap_identity() != Some(identity) {
        return Err(ProbeError::Failed(
            "heatmap identity was not uploaded before the draw".into(),
        ));
    }
    let non_background = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[..3] != [0, 0, 0])
        .count();
    if non_background == 0 {
        return Err(ProbeError::Failed(
            "heatmap texture sampled as pure background".into(),
        ));
    }
    eprintln!(
        "heatmap GPU probe PASS: adapter={} backend={:?}, {non_background} non-background pixels",
        info.name, info.backend
    );
    Ok(())
}
