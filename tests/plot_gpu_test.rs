//! Native scalar-plot device probe (#41, Task 8).
//!
//! Ordinary all-target runs exercise the CPU contracts below and print
//! `GPU not requested`. Only `scripts/test-scalar-gpu.sh` sets the child-only
//! `VFE_SCALAR_GPU_PROBE=1`, which requires a real adapter/device and runs the
//! actual `PlotCallback` prepare/paint flow. There is no `#[ignore]`, noop
//! backend, fake adapter, or silent skip.

use std::sync::{Arc, Mutex};

use eframe::{egui, egui_wgpu, wgpu};
use glam::{Mat4, UVec2, Vec3};

use vector_field_explorer::{
    colormap::Colormap,
    domain::Domain,
    export,
    plot::{PlotKind, curve_edges, sample_curve, sample_surface},
    plot_camera::{PlotCamera, PlotFrame},
    plot_renderer::{
        PlotCallback, PlotDrawState, PlotFormatCapabilities, PlotPipeline, PlotPipelinePolicy,
        PlotTargetKey,
        capture::{CaptureLimits, PlotCapture, validate_capture_extent},
        plot_draw_order, plot_pipeline_policy, plot_target_extent, prepared_matches,
    },
    plot_scene::{PlotGeometry, PlotScene, PlotVertex, build_plot_scene},
    plot_state::{PlotSettings, SampledPlot},
    renderer::{self, DrawState, LineInstanceGPU, SceneGPU},
    scalar::{ScalarContext, ScalarExpr},
    screenshot,
};

const LIGHT_DIR: [f32; 3] = [0.4211, 0.8521, 0.3108];
const PROBE_CLEAR: [f64; 4] = [0.1, 0.2, 0.3, 1.0];
const INCOMING_CLEAR: [f64; 4] = [0.02, 0.05, 0.08, 1.0];

fn cpu_contract_checks() {
    // egui's rounded viewport extent is the plot target key input.
    let rect = egui::Rect::from_min_max(egui::pos2(10.5, 7.5), egui::pos2(139.5, 104.5));
    assert_eq!(
        plot_target_extent(rect, 1.4, [320, 220], 4096),
        Some([180, 135])
    );
    assert_eq!(
        plot_target_extent(rect, 1.4, [320, 220], 100),
        Some([100, 75])
    );
    assert_eq!(
        plot_target_extent(
            egui::Rect::from_min_max(egui::pos2(10.0, 10.0), egui::pos2(10.0, 20.0)),
            1.0,
            [320, 220],
            4096
        ),
        None
    );

    // The plot path never substitutes vector geometry.
    assert!(plot_draw_order(PlotKind::VectorField).is_empty());
    assert_eq!(
        plot_draw_order(PlotKind::Surface3D),
        &[
            vector_field_explorer::plot_renderer::PlotDraw::Mesh,
            vector_field_explorer::plot_renderer::PlotDraw::References
        ]
    );
    assert_eq!(
        plot_draw_order(PlotKind::Curve2D),
        &[
            vector_field_explorer::plot_renderer::PlotDraw::References,
            vector_field_explorer::plot_renderer::PlotDraw::Curve
        ]
    );

    // Actual pipeline pairs are policy-backed.
    for (pipeline, expected) in [
        (
            PlotPipeline::Mesh,
            PlotPipelinePolicy {
                depth: Some((wgpu::CompareFunction::LessEqual, true)),
                cull: Some(wgpu::Face::Back),
                front_face: wgpu::FrontFace::Ccw,
                blend: None,
            },
        ),
        (
            PlotPipeline::References,
            PlotPipelinePolicy {
                depth: Some((wgpu::CompareFunction::LessEqual, false)),
                cull: None,
                front_face: wgpu::FrontFace::Ccw,
                blend: Some(renderer::LINE_BLEND),
            },
        ),
        (
            PlotPipeline::Curve,
            PlotPipelinePolicy {
                depth: Some((wgpu::CompareFunction::LessEqual, true)),
                cull: None,
                front_face: wgpu::FrontFace::Ccw,
                blend: Some(renderer::LINE_BLEND),
            },
        ),
        (
            PlotPipeline::Composite,
            PlotPipelinePolicy {
                depth: None,
                cull: None,
                front_face: wgpu::FrontFace::Ccw,
                blend: None,
            },
        ),
    ] {
        assert_eq!(plot_pipeline_policy(pipeline), expected);
    }

    // Stale/missing preparation is never allowed to draw.
    let key = PlotTargetKey {
        extent: [129, 97],
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        composite_samples: 1,
    };
    assert!(prepared_matches(Some((7, key)), 7, key));
    assert!(!prepared_matches(Some((7, key)), 8, key));
    assert!(!prepared_matches(None, 7, key));

    // Capture sizing rejects non-PNG formats before allocation.
    let limits = CaptureLimits {
        max_texture_dimension_2d: 4096,
        max_buffer_size: u64::MAX,
    };
    assert!(validate_capture_extent(wgpu::TextureFormat::Depth32Float, [129, 97], limits).is_err());
    assert!(
        validate_capture_extent(wgpu::TextureFormat::Rgba8UnormSrgb, [129, 97], limits).is_ok()
    );

    eprintln!("GPU not requested: scalar plot device probe did not run");
}

#[test]
fn native_plot_device_worker() {
    if std::env::var_os("VFE_SCALAR_GPU_PROBE").is_none() {
        cpu_contract_checks();
        return;
    }
    if let Err(error) = run_native_plot_probe() {
        panic!("native scalar plot probe failed: {error}");
    }
}

// ---------------------------------------------------------------------------
// Device setup and error capture
// ---------------------------------------------------------------------------

struct Gpu {
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    info: wgpu::AdapterInfo,
    failures: Arc<Mutex<Vec<String>>>,
}

struct ErrorScopes {
    validation: wgpu::ErrorScopeGuard,
    internal: wgpu::ErrorScopeGuard,
    oom: wgpu::ErrorScopeGuard,
    stage: &'static str,
}

impl Gpu {
    fn stage(&self, name: &'static str) {
        eprintln!("PROBE stage: {name}");
    }

    fn begin_scopes(&self, stage: &'static str) -> ErrorScopes {
        ErrorScopes {
            validation: self.device.push_error_scope(wgpu::ErrorFilter::Validation),
            internal: self.device.push_error_scope(wgpu::ErrorFilter::Internal),
            oom: self.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory),
            stage,
        }
    }

    fn end_scopes(&self, scopes: ErrorScopes) -> Result<(), String> {
        let mut errors = Vec::new();
        for (kind, scope) in [
            ("out of memory", scopes.oom),
            ("internal", scopes.internal),
            ("validation", scopes.validation),
        ] {
            if let Some(error) = pollster::block_on(scope.pop()) {
                errors.push(format!("{} GPU {kind} error: {error}", scopes.stage));
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }

    fn check_failures(&self) -> Result<(), String> {
        let failures = self.failures.lock().unwrap();
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; "))
        }
    }
}

fn request_device() -> Result<Gpu, String> {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
    descriptor.backends.remove(wgpu::Backends::NOOP);
    let instance = wgpu::Instance::new(descriptor);
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        compatible_surface: None,
        ..Default::default()
    }))
    .map_err(|error| format!("no real headless wgpu adapter (no CPU/noop fallback): {error}"))?;
    let info = adapter.get_info();
    eprintln!(
        "PROBE adapter: name={} backend={:?} device_type={:?} driver={} {}",
        info.name, info.backend, info.device_type, info.driver, info.driver_info
    );

    let required_limits = adapter.limits();
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("vfe scalar plot probe device"),
        required_features: wgpu::Features::empty(),
        required_limits,
        ..Default::default()
    }))
    .map_err(|error| format!("adapter '{}' device request failed: {error}", info.name))?;

    let failures = Arc::new(Mutex::new(Vec::<String>::new()));
    let errors = Arc::clone(&failures);
    device.on_uncaptured_error(Arc::new(move |error: wgpu::Error| {
        errors
            .lock()
            .unwrap()
            .push(format!("uncaptured GPU error: {error}"));
    }));
    let errors = Arc::clone(&failures);
    device.set_device_lost_callback(move |reason, message| {
        errors
            .lock()
            .unwrap()
            .push(format!("GPU device lost ({reason:?}): {message}"));
    });

    Ok(Gpu {
        adapter,
        device,
        queue,
        info,
        failures,
    })
}

fn adapter_caps(
    gpu: &Gpu,
    format: wgpu::TextureFormat,
) -> Result<wgpu::TextureFormatFeatures, String> {
    Ok(gpu.adapter.get_texture_format_features(format))
}

// ---------------------------------------------------------------------------
// Frame submission through the real CallbackTrait
// ---------------------------------------------------------------------------

struct FrameResult {
    pixels: Vec<u8>,
}

fn make_target(
    gpu: &Gpu,
    format: wgpu::TextureFormat,
    screen: [u32; 2],
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("vfe probe incoming no-depth target"),
        size: wgpu::Extent3d {
            width: screen[0],
            height: screen[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

#[allow(clippy::too_many_arguments)]
fn submit_frame(
    gpu: &Gpu,
    resources: &mut egui_wgpu::CallbackResources,
    prepare: Option<&PlotCallback>,
    paint: &PlotCallback,
    info: &egui::PaintCallbackInfo,
    format: wgpu::TextureFormat,
    screen: [u32; 2],
) -> Result<FrameResult, String> {
    let scopes = gpu.begin_scopes("frame");
    let result = submit_frame_inner(gpu, resources, prepare, paint, info, format, screen);
    gpu.end_scopes(scopes)?;
    result
}

#[allow(clippy::too_many_arguments)]
fn submit_frame_inner(
    gpu: &Gpu,
    resources: &mut egui_wgpu::CallbackResources,
    prepare: Option<&PlotCallback>,
    paint: &PlotCallback,
    info: &egui::PaintCallbackInfo,
    format: wgpu::TextureFormat,
    screen: [u32; 2],
) -> Result<FrameResult, String> {
    let (texture, view) = make_target(gpu, format, screen);
    let padded_row = screenshot::padded_row_bytes(screen[0]);
    let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("vfe probe frame readback"),
        size: u64::from(padded_row) * u64::from(screen[1]),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("vfe probe frame encoder"),
        });

    if let Some(callback) = prepare {
        let descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: screen,
            pixels_per_point: info.pixels_per_point,
        };
        let extra = egui_wgpu::CallbackTrait::prepare(
            callback,
            &gpu.device,
            &gpu.queue,
            &descriptor,
            &mut encoder,
            resources,
        );
        if !extra.is_empty() {
            return Err(format!(
                "prepare returned {} extra command buffers, expected none",
                extra.len()
            ));
        }
    }

    {
        let viewport = info.viewport_in_pixels();
        let clip = info.clip_rect_in_pixels();
        let mut pass = encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("vfe probe incoming no-depth pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: INCOMING_CLEAR[0],
                            g: INCOMING_CLEAR[1],
                            b: INCOMING_CLEAR[2],
                            a: INCOMING_CLEAR[3],
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            })
            .forget_lifetime();
        if viewport.width_px > 0 && viewport.height_px > 0 {
            pass.set_viewport(
                viewport.left_px as f32,
                viewport.top_px as f32,
                viewport.width_px as f32,
                viewport.height_px as f32,
                0.0,
                1.0,
            );
            pass.set_scissor_rect(
                clip.left_px.max(0) as u32,
                clip.top_px.max(0) as u32,
                clip.width_px.max(0) as u32,
                clip.height_px.max(0) as u32,
            );
            let owned_info = egui::PaintCallbackInfo {
                viewport: info.viewport,
                clip_rect: info.clip_rect,
                pixels_per_point: info.pixels_per_point,
                screen_size_px: info.screen_size_px,
            };
            egui_wgpu::CallbackTrait::paint(paint, owned_info, &mut pass, resources);
        }
    }

    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_row),
                rows_per_image: Some(screen[1]),
            },
        },
        wgpu::Extent3d {
            width: screen[0],
            height: screen[1],
            depth_or_array_layers: 1,
        },
    );
    gpu.queue.submit(Some(encoder.finish()));

    let slice = readback.slice(..);
    let (mapped_tx, mapped_rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = mapped_tx.send(result);
    });
    gpu.device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|error| format!("probe poll failed: {error}"))?;
    renderer::readback_map_status(
        mapped_rx
            .recv()
            .map_err(|error| format!("probe readback callback was lost: {error}"))?,
    )
    .map_err(|error| error.to_string())?;
    let pixels = {
        let mapped = slice
            .get_mapped_range()
            .map_err(|error| format!("probe mapped range unavailable: {error}"))?;
        screenshot::strip_row_padding(&mapped, screen[0], screen[1])
    };
    readback.unmap();
    let pixels = screenshot::to_rgba(format, pixels)?;
    Ok(FrameResult { pixels })
}

// ---------------------------------------------------------------------------
// Pixel helpers and geometry
// ---------------------------------------------------------------------------

fn srgb(linear: f32) -> f32 {
    let linear = linear.clamp(0.0, 1.0) as f64;
    let encoded = if linear <= 0.003_130_8 {
        12.92 * linear
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    };
    encoded as f32
}

fn byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn expected_rgb(format: wgpu::TextureFormat, linear: [f32; 3]) -> [u8; 3] {
    let srgb_format = matches!(
        format,
        wgpu::TextureFormat::Rgba8UnormSrgb | wgpu::TextureFormat::Bgra8UnormSrgb
    );
    if srgb_format {
        [
            byte(srgb(linear[0])),
            byte(srgb(linear[1])),
            byte(srgb(linear[2])),
        ]
    } else {
        [byte(linear[0]), byte(linear[1]), byte(linear[2])]
    }
}

fn pixel(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * width + x) * 4) as usize;
    pixels[offset..offset + 4].try_into().unwrap()
}

fn assert_rgb(label: &str, actual: [u8; 4], expected: [u8; 3]) {
    for channel in 0..3 {
        let difference = actual[channel].abs_diff(expected[channel]);
        assert!(
            difference <= 3,
            "{label}: channel {channel} is {:?}, expected {:?} (±3); pixel {actual:?}",
            actual[channel],
            expected[channel]
        );
    }
}

fn assert_background(label: &str, actual: [u8; 4], format: wgpu::TextureFormat) {
    assert_rgb(
        label,
        actual,
        expected_rgb(
            format,
            [
                PROBE_CLEAR[0] as f32,
                PROBE_CLEAR[1] as f32,
                PROBE_CLEAR[2] as f32,
            ],
        ),
    );
    assert_eq!(actual[3], 255, "{label}: opaque background");
}

fn light_normal() -> [f32; 3] {
    Vec3::from_array(LIGHT_DIR).normalize().to_array()
}

fn vertex(position: [f32; 3], color: [f32; 3]) -> PlotVertex {
    let normal = light_normal();
    PlotVertex {
        position: [position[0], position[1], position[2], 1.0],
        normal: [normal[0], normal[1], normal[2], 0.0],
        color: [color[0], color[1], color[2], 1.0],
    }
}

fn push_quad(
    vertices: &mut Vec<PlotVertex>,
    indices: &mut Vec<u32>,
    (x0, y0): (f32, f32),
    (x1, y1): (f32, f32),
    z: f32,
    color: [f32; 3],
) {
    let base = vertices.len() as u32;
    for position in [[x0, y0, z], [x1, y0, z], [x0, y1, z], [x1, y1, z]] {
        vertices.push(vertex(position, color));
    }
    indices.extend_from_slice(&[base, base + 1, base + 2, base + 1, base + 3, base + 2]);
}

fn surface_scene(
    vertices: Vec<PlotVertex>,
    indices: Vec<u32>,
    references: Vec<LineInstanceGPU>,
    generation: u64,
) -> Arc<PlotScene> {
    Arc::new(PlotScene {
        kind: PlotKind::Surface3D,
        geometry: PlotGeometry::Surface(Arc::new(vertices), Arc::new(indices)),
        references: Arc::new(references),
        bounds: None,
        height_range: (0.0, 1.0),
        generation,
        status: String::new(),
    })
}

fn quadrilateral_scene(generation: u64) -> Arc<PlotScene> {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let top_left = [0.20, 0.40, 0.60];
    let top_right = [0.60, 0.20, 0.40];
    let bottom_left = [0.40, 0.60, 0.20];
    let bottom_right = [0.80, 0.80, 0.20];
    push_quad(
        &mut vertices,
        &mut indices,
        (-1.0, 0.0),
        (0.0, 1.0),
        0.5,
        top_left,
    );
    push_quad(
        &mut vertices,
        &mut indices,
        (0.0, 0.0),
        (1.0, 1.0),
        0.5,
        top_right,
    );
    push_quad(
        &mut vertices,
        &mut indices,
        (-1.0, -1.0),
        (0.0, 0.0),
        0.5,
        bottom_left,
    );
    push_quad(
        &mut vertices,
        &mut indices,
        (0.0, -1.0),
        (1.0, 0.0),
        0.5,
        bottom_right,
    );
    surface_scene(vertices, indices, Vec::new(), generation)
}

fn triangle(
    vertices: &mut Vec<PlotVertex>,
    indices: &mut Vec<u32>,
    points: [[f32; 2]; 3],
    z: f32,
    color: [f32; 3],
) {
    let base = vertices.len() as u32;
    for [x, y] in points {
        vertices.push(vertex([x, y, z], color));
    }
    indices.extend_from_slice(&[base, base + 1, base + 2]);
}

fn depth_scene(generation: u64) -> Arc<PlotScene> {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    // Near green first, then far red: the far draw must fail depth.
    triangle(
        &mut vertices,
        &mut indices,
        [[-0.5, -0.5], [0.5, -0.5], [0.0, 0.5]],
        0.5,
        [0.0, 0.8, 0.2],
    );
    triangle(
        &mut vertices,
        &mut indices,
        [[-0.9, -0.9], [0.9, -0.9], [0.0, 0.9]],
        0.8,
        [0.8, 0.1, 0.1],
    );
    let references = vec![
        LineInstanceGPU {
            a: [-0.2, -0.25, 0.9, 1.0],
            b: [0.2, -0.25, 0.9, 0.12],
            color: [0.1, 0.2, 0.9, 1.0],
        },
        LineInstanceGPU {
            a: [-0.2, 0.25, 0.3, 1.0],
            b: [0.2, 0.25, 0.3, 0.12],
            color: [0.9, 0.9, 0.1, 1.0],
        },
    ];
    surface_scene(vertices, indices, references, generation)
}

fn reverse_triangle_scene(generation: u64, reverse: bool) -> Arc<PlotScene> {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let points = if reverse {
        [[-0.5, -0.5], [0.0, 0.5], [0.5, -0.5]]
    } else {
        [[-0.5, -0.5], [0.5, -0.5], [0.0, 0.5]]
    };
    triangle(&mut vertices, &mut indices, points, 0.5, [0.0, 0.8, 0.2]);
    surface_scene(vertices, indices, Vec::new(), generation)
}

fn identity_frame() -> PlotFrame {
    PlotFrame {
        view_proj: Mat4::IDENTITY,
        eye: Vec3::new(0.0, 0.0, 5.0),
        aspect: 1.0,
    }
}

fn callback(scene: Arc<PlotScene>, frame: PlotFrame, rect: egui::Rect) -> PlotCallback {
    PlotCallback {
        scene,
        frame,
        rect,
        clear: PROBE_CLEAR,
        errors: Arc::new(Mutex::new(None)),
    }
}

fn info(rect: egui::Rect, clip: egui::Rect, ppp: f32, screen: [u32; 2]) -> egui::PaintCallbackInfo {
    egui::PaintCallbackInfo {
        viewport: rect,
        clip_rect: clip,
        pixels_per_point: ppp,
        screen_size_px: screen,
    }
}

fn full_rect(size: [u32; 2], ppp: f32) -> egui::Rect {
    egui::Rect::from_min_max(
        egui::pos2(0.0, 0.0),
        egui::pos2(size[0] as f32 / ppp, size[1] as f32 / ppp),
    )
}

// ---------------------------------------------------------------------------
// Probe stages
// ---------------------------------------------------------------------------

fn run_native_plot_probe() -> Result<(), String> {
    let gpu = request_device()?;
    gpu.stage("device-ready");
    eprintln!(
        "PROBE device: adapter={:?} backend={:?} type={:?} max_texture_2d={} max_samples={}",
        gpu.info.name,
        gpu.info.backend,
        gpu.info.device_type,
        gpu.device.limits().max_texture_dimension_2d,
        gpu.device.limits().max_sampled_textures_per_shader_stage
    );

    let srgb = wgpu::TextureFormat::Rgba8UnormSrgb;
    let linear = wgpu::TextureFormat::Rgba8Unorm;
    for format in [srgb, linear] {
        let caps = PlotFormatCapabilities {
            color: adapter_caps(&gpu, format)?,
            depth: adapter_caps(&gpu, wgpu::TextureFormat::Depth32Float)?,
        };
        if caps.color.allowed_usages.is_empty() {
            return Err(format!("{format:?}: adapter reports no color usages"));
        }
        let state = PlotDrawState::new(&gpu.device, format, 1, caps)
            .map_err(|error| format!("{format:?} plot configuration unsupported: {error}"))?;
        eprintln!(
            "PROBE format: {format:?} samples=1 color={:?} depth=Depth32Float",
            caps
        );
        gpu.stage(if format == srgb {
            "srgb-format"
        } else {
            "linear-format"
        });

        let mut resources = egui_wgpu::CallbackResources::default();
        resources.insert(state);

        let screen = [129, 97];
        let rect = full_rect(screen, 1.0);
        let frame = identity_frame();
        let scene = quadrilateral_scene(1);
        let cb = callback(scene, frame, rect);
        let paint_info = info(rect, rect, 1.0, screen);
        let result = submit_frame(
            &gpu,
            &mut resources,
            Some(&cb),
            &cb,
            &paint_info,
            format,
            screen,
        )?;
        assert_eq!(result.pixels.len(), (129 * 97 * 4) as usize);

        let colors = [
            ([0.20, 0.40, 0.60], "top-left"),
            ([0.60, 0.20, 0.40], "top-right"),
            ([0.40, 0.60, 0.20], "bottom-left"),
            ([0.80, 0.80, 0.20], "bottom-right"),
        ];
        for (index, (linear_color, label)) in colors.iter().enumerate() {
            let x = if index % 2 == 0 { 32 } else { 96 };
            let y = if index < 2 { 24 } else { 72 };
            let actual = pixel(&result.pixels, 129, x, y);
            assert_rgb(
                &format!("{format:?} {label} quadrant (no vertical flip)"),
                actual,
                expected_rgb(format, *linear_color),
            );
        }
        eprintln!(
            "PROBE texture/gamma: {format:?} quadrant colors match expected linear-to-target bytes within 3 (top/bottom not flipped, no double gamma)"
        );

        // Resize the same live state 129x97 -> 257x113 and re-check quadrants.
        let resized = [257, 113];
        let rect = full_rect(resized, 1.0);
        let cb = callback(quadrilateral_scene(2), frame, rect);
        let paint_info = info(rect, rect, 1.0, resized);
        let result = submit_frame(
            &gpu,
            &mut resources,
            Some(&cb),
            &cb,
            &paint_info,
            format,
            resized,
        )?;
        assert_eq!(result.pixels.len(), (257 * 113 * 4) as usize);
        for (index, (linear_color, label)) in colors.iter().enumerate() {
            let x = if index % 2 == 0 { 64 } else { 192 };
            let y = if index < 2 { 28 } else { 84 };
            let actual = pixel(&result.pixels, 257, x, y);
            assert_rgb(
                &format!("{format:?} resize {label}"),
                actual,
                expected_rgb(format, *linear_color),
            );
        }
        eprintln!("PROBE resize: 129x97 -> 257x113 reused/resized target");

        // Offset viewport, fractional DPI and a clipped scissor.
        let offset_screen = [320, 220];
        let ppp = 1.4;
        let viewport_rect =
            egui::Rect::from_min_max(egui::pos2(10.5, 7.5), egui::pos2(170.5, 132.5));
        let clip_rect = egui::Rect::from_min_max(egui::pos2(40.5, 30.5), egui::pos2(110.5, 90.5));
        let cb = callback(quadrilateral_scene(3), frame, viewport_rect);
        let paint_info = info(viewport_rect, clip_rect, ppp, offset_screen);
        let result = submit_frame(
            &gpu,
            &mut resources,
            Some(&cb),
            &cb,
            &paint_info,
            format,
            offset_screen,
        )?;
        let vp = paint_info.viewport_in_pixels();
        let clip = paint_info.clip_rect_in_pixels();
        eprintln!(
            "PROBE viewport: offset={}x{} at ({},{}) ppp={ppp} clip={}x{} at ({},{})",
            vp.width_px,
            vp.height_px,
            vp.left_px,
            vp.top_px,
            clip.width_px,
            clip.height_px,
            clip.left_px,
            clip.top_px
        );
        let inside = pixel(
            &result.pixels,
            offset_screen[0],
            (clip.left_px + clip.width_px / 2) as u32,
            (clip.top_px + clip.height_px / 2) as u32,
        );
        let outside_clip = pixel(
            &result.pixels,
            offset_screen[0],
            clip.left_px.max(0) as u32 - 4,
            (clip.top_px + clip.height_px / 2) as u32,
        );
        let outside_viewport = pixel(&result.pixels, offset_screen[0], 2, 2);
        assert!(
            inside != outside_clip,
            "fractional-DPI viewport should paint inside the scissor and not outside it"
        );
        assert_eq!(
            outside_clip, outside_viewport,
            "clipped scissor must leave the incoming target clear"
        );
        eprintln!("PROBE scissor: clipped region only; outside remains incoming clear");
    }

    // ------------------------------------------------------------------
    // Real callback flow at the sRGB eframe format.
    // ------------------------------------------------------------------
    let format = srgb;
    let caps = PlotFormatCapabilities {
        color: adapter_caps(&gpu, format)?,
        depth: adapter_caps(&gpu, wgpu::TextureFormat::Depth32Float)?,
    };
    let state = PlotDrawState::new(&gpu.device, format, 1, caps)
        .map_err(|error| format!("sRGB plot state unsupported: {error}"))?;
    let mut resources = egui_wgpu::CallbackResources::default();
    resources.insert(state);

    // Surface near/far and reference occlusion.
    gpu.stage("surface-depth-references");
    let screen = [64, 64];
    let rect = full_rect(screen, 1.0);
    let frame = identity_frame();
    let scene = depth_scene(10);
    let cb = callback(scene, frame, rect);
    let paint_info = info(rect, rect, 1.0, screen);
    let surface = submit_frame(
        &gpu,
        &mut resources,
        Some(&cb),
        &cb,
        &paint_info,
        format,
        screen,
    )?;
    let centre = pixel(&surface.pixels, 64, 32, 32);
    assert_rgb(
        "near green triangle wins over later far red draw",
        centre,
        expected_rgb(format, [0.0, 0.8, 0.2]),
    );
    for y in 30..35 {
        for x in 30..35 {
            assert_rgb(
                "robust near interior",
                pixel(&surface.pixels, 64, x, y),
                expected_rgb(format, [0.0, 0.8, 0.2]),
            );
        }
    }
    let far_only = pixel(&surface.pixels, 64, 11, 52);
    assert_rgb(
        "far red triangle still visible outside the near triangle",
        far_only,
        expected_rgb(format, [0.8, 0.1, 0.1]),
    );
    let behind = pixel(&surface.pixels, 64, 32, 40);
    assert_rgb(
        "reference behind the mesh is occluded",
        behind,
        expected_rgb(format, [0.0, 0.8, 0.2]),
    );
    let front = pixel(&surface.pixels, 64, 32, 24);
    assert_rgb(
        "foreground reference remains visible over the mesh",
        front,
        expected_rgb(format, [0.9, 0.9, 0.1]),
    );
    eprintln!(
        "PROBE surface: near/far aggressive order, reference depth, foreground reference and lit mesh PASS"
    );

    // Reverse-facing surface culling.
    gpu.stage("backface-culling");
    let cb_reverse = callback(reverse_triangle_scene(11, true), frame, rect);
    let reverse = submit_frame(
        &gpu,
        &mut resources,
        Some(&cb_reverse),
        &cb_reverse,
        &paint_info,
        format,
        screen,
    )?;
    assert_background(
        "reverse-facing surface is culled",
        pixel(&reverse.pixels, 64, 32, 32),
        format,
    );
    let cb_forward = callback(reverse_triangle_scene(12, false), frame, rect);
    let forward = submit_frame(
        &gpu,
        &mut resources,
        Some(&cb_forward),
        &cb_forward,
        &paint_info,
        format,
        screen,
    )?;
    assert_rgb(
        "forward-facing control triangle is visible",
        pixel(&forward.pixels, 64, 32, 32),
        expected_rgb(format, [0.0, 0.8, 0.2]),
    );
    eprintln!("PROBE culling: reverse-facing surface omitted; forward control visible");

    // Sampled invalid cells: clear color, never fake origin/fill.
    gpu.stage("invalid-cells");
    let domain = Domain::new(Vec3::splat(-1.0), Vec3::splat(1.0));
    let settings = PlotSettings {
        surface_resolution: 7,
        ..Default::default()
    };
    let hole_expr = ScalarExpr::parse("1/(x-0.5)", ScalarContext::Surface).unwrap();
    let mesh = sample_surface(&hole_expr, &domain, UVec2::splat(7));
    assert!(mesh.diagnostics.rejected_cells > 0);
    let hole_scene = build_plot_scene(
        PlotKind::Surface3D,
        Some(&SampledPlot::Surface(mesh)),
        &domain,
        &settings,
        Colormap::Viridis,
        false,
        false,
        20,
    );
    let hole_cb = callback(Arc::new(hole_scene), frame, rect);
    let hole = submit_frame(
        &gpu,
        &mut resources,
        Some(&hole_cb),
        &hole_cb,
        &paint_info,
        format,
        screen,
    )?;
    assert_background(
        "sampled invalid center cell reveals clear color",
        pixel(&hole.pixels, 64, 48, 32),
        format,
    );
    let solid = pixel(&hole.pixels, 64, 12, 32);
    assert_ne!(
        &solid[..3],
        &expected_rgb(
            format,
            [
                PROBE_CLEAR[0] as f32,
                PROBE_CLEAR[1] as f32,
                PROBE_CLEAR[2] as f32,
            ],
        )[..],
        "mesh must render around the hole"
    );

    let corner_expr = ScalarExpr::parse("1/(x-1)", ScalarContext::Surface).unwrap();
    let mesh = sample_surface(&corner_expr, &domain, UVec2::splat(7));
    assert!(mesh.diagnostics.rejected_cells > 0);
    let corner_scene = build_plot_scene(
        PlotKind::Surface3D,
        Some(&SampledPlot::Surface(mesh)),
        &domain,
        &settings,
        Colormap::Viridis,
        false,
        false,
        21,
    );
    let corner_cb = callback(Arc::new(corner_scene), frame, rect);
    let corner = submit_frame(
        &gpu,
        &mut resources,
        Some(&corner_cb),
        &corner_cb,
        &paint_info,
        format,
        screen,
    )?;
    assert_background(
        "sampled invalid corner reveals clear color",
        pixel(&corner.pixels, 64, 58, 32),
        format,
    );
    eprintln!("PROBE holes: invalid center and corner cells are clear, no origin/fill geometry");

    // Curve overlay and singularity gap.
    gpu.stage("curve-overlay-gap");
    let curve_domain = Domain::new(Vec3::new(-1.0, -0.2, -1.0), Vec3::new(1.0, 0.2, 1.0));
    let curve_expr = ScalarExpr::parse("0/x", ScalarContext::Curve).unwrap();
    let slots = sample_curve(&curve_expr, &curve_domain, 9);
    assert!(slots.iter().any(Option::is_none));
    let edges = curve_edges(&curve_expr, &curve_domain, &slots);
    let settings = PlotSettings {
        curve_samples: 9,
        line_width: 0.06,
        ..Default::default()
    };
    let mut curve_scene = build_plot_scene(
        PlotKind::Curve2D,
        Some(&SampledPlot::Curve { slots, edges }),
        &curve_domain,
        &settings,
        Colormap::Mono,
        false,
        false,
        30,
    );
    curve_scene.references = Arc::new(vec![
        LineInstanceGPU {
            a: [-1.0, 0.0, 0.0, 1.0],
            b: [-0.25, 0.0, 0.0, 0.04],
            color: [0.24, 0.26, 0.30, 1.0],
        },
        LineInstanceGPU {
            a: [0.25, 0.0, 0.0, 1.0],
            b: [1.0, 0.0, 0.0, 0.04],
            color: [0.24, 0.26, 0.30, 1.0],
        },
    ]);
    let curve_screen = [200, 100];
    let curve_rect = full_rect(curve_screen, 1.0);
    let curve_cb = callback(Arc::new(curve_scene), frame, curve_rect);
    let curve_info = info(curve_rect, curve_rect, 1.0, curve_screen);
    let curve = submit_frame(
        &gpu,
        &mut resources,
        Some(&curve_cb),
        &curve_cb,
        &curve_info,
        format,
        curve_screen,
    )?;
    let overlay_x = ((0.2_f32) * 200.0) as u32; // NDC -0.6
    let curve_gray = expected_rgb(format, [0.501_960_8, 0.501_960_8, 0.501_960_8]);
    let overlay = pixel(&curve.pixels, 200, overlay_x, 50);
    assert_rgb(
        "curve overlays the same-plane reference",
        overlay,
        curve_gray,
    );
    assert_background(
        "curve singularity gap remains background",
        pixel(&curve.pixels, 200, 100, 50),
        format,
    );
    eprintln!("PROBE curve: curve over same-plane reference; singularity gap is clear");

    // Camera framing with references (production PlotCamera).
    gpu.stage("camera-framed-sampled-surface");
    let camera_domain = Domain::default();
    let camera_expr = ScalarExpr::parse("exp(-(x^2+y^2))", ScalarContext::Surface).unwrap();
    let camera_mesh = sample_surface(&camera_expr, &camera_domain, UVec2::splat(24));
    let camera_scene = build_plot_scene(
        PlotKind::Surface3D,
        Some(&SampledPlot::Surface(camera_mesh)),
        &camera_domain,
        &settings,
        Colormap::Viridis,
        true,
        true,
        40,
    );
    let bounds = camera_scene
        .bounds
        .ok_or_else(|| "camera stage produced no bounds".to_string())?;
    let mut camera = PlotCamera::default();
    let camera_screen = [160, 120];
    let camera_rect = full_rect(camera_screen, 1.0);
    camera
        .fit(bounds, PlotKind::Surface3D, 160.0 / 120.0)
        .map_err(|error| format!("PlotCamera::fit failed: {error}"))?;
    let camera_frame = camera
        .frame(PlotKind::Surface3D, 160.0 / 120.0)
        .map_err(|error| format!("PlotCamera::frame failed: {error}"))?;
    let camera_cb = callback(Arc::new(camera_scene), camera_frame, camera_rect);
    let camera_info = info(camera_rect, camera_rect, 1.0, camera_screen);
    let camera_result = submit_frame(
        &gpu,
        &mut resources,
        Some(&camera_cb),
        &camera_cb,
        &camera_info,
        format,
        camera_screen,
    )?;
    let background = expected_rgb(
        format,
        [
            PROBE_CLEAR[0] as f32,
            PROBE_CLEAR[1] as f32,
            PROBE_CLEAR[2] as f32,
        ],
    );
    let distinct = camera_result
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[..3] != background)
        .count();
    assert!(
        distinct > 500,
        "camera-framed surface/reference geometry should paint many pixels, got {distinct}"
    );
    eprintln!("PROBE camera: PlotCamera fit/frame rendered {distinct} non-background pixels");

    // Stale/mismatched preparation and target release.
    gpu.stage("stale-generation-release");
    let screen = [64, 64];
    let rect = full_rect(screen, 1.0);
    let paint_info = info(rect, rect, 1.0, screen);
    let live = callback(depth_scene(50), frame, rect);
    let baseline = submit_frame(
        &gpu,
        &mut resources,
        Some(&live),
        &live,
        &paint_info,
        format,
        screen,
    )?;
    let stale = callback(depth_scene(51), frame, rect);
    let stale_result = submit_frame(
        &gpu,
        &mut resources,
        None,
        &stale,
        &paint_info,
        format,
        screen,
    )?;
    let no_paint = submit_frame(
        &gpu,
        &mut resources,
        None,
        &stale,
        &info(
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            rect,
            1.0,
            screen,
        ),
        format,
        screen,
    )?;
    assert_eq!(
        stale_result.pixels, no_paint.pixels,
        "mismatched scene generation must not composite the stale target"
    );
    let correct_again = submit_frame(
        &gpu,
        &mut resources,
        None,
        &live,
        &paint_info,
        format,
        screen,
    )?;
    assert_eq!(
        correct_again.pixels, baseline.pixels,
        "matching prepared generation still paints the live target"
    );

    resources
        .get_mut::<PlotDrawState>()
        .ok_or_else(|| "plot draw state disappeared".to_string())?
        .release_targets();
    let released = submit_frame(
        &gpu,
        &mut resources,
        None,
        &live,
        &paint_info,
        format,
        screen,
    )?;
    let no_paint = submit_frame(
        &gpu,
        &mut resources,
        None,
        &live,
        &info(
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            rect,
            1.0,
            screen,
        ),
        format,
        screen,
    )?;
    assert_eq!(
        released.pixels, no_paint.pixels,
        "released targets must draw nothing"
    );

    // Missing preparation on a freshly installed renderer draws nothing.
    let fresh_caps = PlotFormatCapabilities {
        color: adapter_caps(&gpu, format)?,
        depth: adapter_caps(&gpu, wgpu::TextureFormat::Depth32Float)?,
    };
    resources.insert(PlotDrawState::new(&gpu.device, format, 1, fresh_caps)?);
    let missing = submit_frame(
        &gpu,
        &mut resources,
        None,
        &live,
        &paint_info,
        format,
        screen,
    )?;
    assert_eq!(
        missing.pixels, no_paint.pixels,
        "a renderer with no preparation must draw nothing"
    );

    // Reinstall after release and prepare/paint successfully.
    let reinstalled = submit_frame(
        &gpu,
        &mut resources,
        Some(&live),
        &live,
        &paint_info,
        format,
        screen,
    )?;
    assert_eq!(
        reinstalled.pixels, baseline.pixels,
        "reinstalled renderer must prepare and paint the same live scene"
    );
    eprintln!("PROBE lifecycle: stale/missing preparation, release and reinstall PASS");

    // Interleaved captures must not mutate live uniforms/targets.
    gpu.stage("capture-interleave");
    let capture_caps = PlotFormatCapabilities {
        color: adapter_caps(&gpu, format)?,
        depth: adapter_caps(&gpu, wgpu::TextureFormat::Depth32Float)?,
    };
    let mut capture =
        PlotCapture::new(&gpu.device, format, capture_caps).map_err(|error| error.to_string())?;
    let live_pixels = baseline.pixels.clone();
    let capture_scene = quadrilateral_scene(60);
    let capture_frame = PlotFrame {
        view_proj: Mat4::from_scale(Vec3::splat(0.7)),
        eye: Vec3::new(0.0, 0.0, 4.0),
        aspect: 1.0,
    };
    let first = capture
        .render(
            &gpu.device,
            &gpu.queue,
            &capture_scene,
            capture_frame,
            [129, 97],
            [0.4, 0.1, 0.2, 1.0],
        )
        .map_err(|error| error.to_string())?;
    assert_eq!(first.len(), (129 * 97 * 4) as usize);
    let second_scene = depth_scene(61);
    let second = capture
        .render(
            &gpu.device,
            &gpu.queue,
            &second_scene,
            identity_frame(),
            [97, 129],
            [0.2, 0.4, 0.1, 1.0],
        )
        .map_err(|error| error.to_string())?;
    assert_eq!(second.len(), (97 * 129 * 4) as usize);
    assert_ne!(
        first, second,
        "different capture camera/geometry must differ"
    );

    let live_after = submit_frame(
        &gpu,
        &mut resources,
        None,
        &live,
        &paint_info,
        format,
        screen,
    )?;
    assert_eq!(
        live_after.pixels, live_pixels,
        "captures must leave live uniforms and target unchanged"
    );
    eprintln!("PROBE capture: two interleaved captures; live target readback unchanged");

    // PNG encode/decode through the existing export seam.
    gpu.stage("png-readback");
    let encoded = export::encode_png(&first, 129, 97).map_err(|error| error.to_string())?;
    let decoded = image::load_from_memory(&encoded)
        .map_err(|error| format!("probe PNG decode failed: {error}"))?
        .to_rgba8();
    assert_eq!((decoded.width(), decoded.height()), (129, 97));
    assert_eq!(decoded.as_raw(), &first);
    let background = expected_rgb(format, [0.4, 0.1, 0.2]);
    let non_background = decoded
        .as_raw()
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[..3] != background)
        .count();
    assert!(
        non_background > 100,
        "PNG must contain non-background geometry, got {non_background}"
    );
    eprintln!(
        "PROBE PNG: encoded/decoded 129x97, exact rows, {non_background} non-background pixels"
    );

    // Return to the unchanged vector scene on the same device.
    gpu.stage("return-to-vector");
    let mut vector = DrawState::new(&gpu.device, format);
    let vector_scene = SceneGPU {
        mode_3d: false,
        lines: Arc::new(vec![LineInstanceGPU {
            a: [-0.5, 0.0, 0.0, 1.0],
            b: [0.5, 0.0, 0.0, 0.08],
            color: [0.9, 0.2, 0.1, 1.0],
        }]),
        ..Default::default()
    };
    let vector_pixels = vector
        .render_offscreen(
            &gpu.device,
            &gpu.queue,
            &vector_scene,
            Mat4::IDENTITY,
            Vec3::new(0.0, 0.0, 5.0),
            64,
            64,
            PROBE_CLEAR,
        )
        .map_err(|error| format!("vector return render failed: {error}"))?;
    let vector_background = expected_rgb(
        format,
        [
            PROBE_CLEAR[0] as f32,
            PROBE_CLEAR[1] as f32,
            PROBE_CLEAR[2] as f32,
        ],
    );
    assert!(
        vector_pixels
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[..3] != vector_background),
        "existing vector scene must still paint ink on the same device"
    );
    eprintln!("PROBE vector: existing DrawState scene rendered after scalar pipelines");

    gpu.check_failures()?;
    eprintln!(
        "PROBE PASS: real {} ({:?}, {:?}) device; sRGB+linear targets; mesh, curve, reference-depth and no-depth composite pipelines exercised without validation errors",
        gpu.info.name, gpu.info.backend, gpu.info.device_type
    );
    Ok(())
}
