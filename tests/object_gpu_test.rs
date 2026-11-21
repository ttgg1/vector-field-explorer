//! Object-pass CPU contract and native device probe (#112).
//!
//! Ordinary all-target runs exercise the device-free contracts below and print
//! `GPU not requested`. Only `scripts/test-object-gpu.sh` sets the child-only
//! `VFE_OBJECT_GPU_PROBE=1`, which requires a real adapter/device and drives the
//! actual `ObjectPass::prepare`/`paint` flow. There is no `#[ignore]`, noop
//! backend, fake adapter, or silent skip.

use std::sync::{Arc, Mutex};

use eframe::{egui, wgpu};
use glam::{Mat4, Vec3};

use vector_field_explorer::{
    domain::Domain,
    object_renderer::{
        OBJECT_PROBE_ENV, ObjectGlobals, ObjectInstance, ObjectPass, ObjectView, object_draw_plan,
    },
    object_scene::{MeshObject, MeshPrimitive, MeshScene, SliceClip},
    renderer::{self, DrawState, SceneGPU},
    screenshot,
    source_scene::{SourceEntry, SourceKindTag, SourceScene},
};

const PROBE_CLEAR: [f64; 4] = [0.05, 0.06, 0.08, 1.0];
const INCOMING_CLEAR: [f64; 4] = [0.02, 0.05, 0.08, 1.0];

fn sphere_scene() -> MeshScene {
    let scene = SourceScene {
        entries: vec![SourceEntry {
            enabled: true,
            source: SourceKindTag::SolidSphere.default_source(),
        }],
        ..SourceScene::default()
    };
    MeshScene::from_sources(
        &scene,
        &Domain::new(Vec3::splat(-2.0), Vec3::splat(2.0)),
        None,
    )
}

fn probe_view(viewport: [u32; 2]) -> ObjectView {
    let aspect = viewport[0] as f32 / viewport[1].max(1) as f32;
    let view_proj = Mat4::perspective_rh(45_f32.to_radians(), aspect, 0.1, 100.0)
        * Mat4::look_at_rh(Vec3::new(0.0, 0.0, 3.0), Vec3::ZERO, Vec3::Y);
    ObjectView {
        view_proj: view_proj.to_cols_array_2d(),
        eye: [0.0, 0.0, 3.0],
        viewport,
        wireframe: false,
        clear: PROBE_CLEAR,
    }
}

fn all_finite(values: &[f32]) -> bool {
    values.iter().all(|value| value.is_finite())
}

/// Device-free contracts: they pin the prepare decision seam, the uniform ABI
/// and the instance sanitizer that `prepare` feeds to the GPU.
fn cpu_contract_checks() {
    let view = probe_view([64, 48]);

    // An empty scene idles: no target, no instance upload, no draw.
    assert!(object_draw_plan(&MeshScene::empty(), &view).is_none());

    // A zero-area viewport idles even when the scene has geometry.
    let scene = sphere_scene();
    assert_eq!(scene.objects().len(), 1);
    let zero_area = probe_view([64, 0]);
    assert!(object_draw_plan(&scene, &zero_area).is_none());
    assert_eq!(zero_area.target_extent(), None);
    assert_eq!(view.target_extent(), Some([64, 48]));

    // One mapped sphere is one drawable instance at the requested extent.
    let plan = object_draw_plan(&scene, &view).expect("a mapped sphere plans a draw");
    assert_eq!(plan.extent, [64, 48]);
    assert_eq!(plan.draws.len(), 1);
    assert_eq!(plan.draws[0].0, MeshPrimitive::Sphere);
    // Deterministic: the same scene + view plans the same draws.
    assert_eq!(object_draw_plan(&scene, &view), Some(plan.clone()));

    // Uniform ABI: the view round-trips exactly and stays finite.
    assert_eq!(std::mem::size_of::<ObjectInstance>(), 128);
    assert_eq!(std::mem::size_of::<ObjectGlobals>(), 128);
    let globals = ObjectGlobals::from_view(&view, None);
    assert_eq!(globals.view_proj, view.view_proj);
    assert_eq!(globals.eye, [0.0, 0.0, 3.0, 1.0]);
    assert_eq!(globals.params[0], 0.0);
    assert_eq!(globals.params[1], 0.0);
    assert!(all_finite(&globals.view_proj.concat()));
    assert!(all_finite(&globals.eye));
    assert!(all_finite(&globals.clip_plane));
    assert!(all_finite(&globals.light_dir));
    assert!(all_finite(&globals.params));

    // The wireframe flag reaches the uniform in the first params slot.
    let wireframe = ObjectView {
        wireframe: true,
        ..view
    };
    assert_eq!(ObjectGlobals::from_view(&wireframe, None).params[0], 1.0);

    // A slice clip becomes the packed plane and enables the fragment discard.
    let clipped = ObjectGlobals::from_view(
        &view,
        Some(SliceClip {
            normal: [0.0, 1.0, 0.0],
            offset: 0.5,
        }),
    );
    assert_eq!(clipped.clip_plane, [0.0, 1.0, 0.0, 0.5]);
    assert_eq!(clipped.params[1], 1.0);

    // Non-finite input is replaced, never uploaded as NaN/Inf.
    let broken_view = ObjectView {
        view_proj: [[f32::NAN; 4]; 4],
        eye: [f32::INFINITY; 3],
        viewport: [64, 48],
        wireframe: true,
        clear: [f64::NAN; 4],
    };
    let broken = ObjectGlobals::from_view(
        &broken_view,
        Some(SliceClip {
            normal: [f32::NAN, 0.0, 0.0],
            offset: f32::INFINITY,
        }),
    );
    assert!(all_finite(&broken.view_proj.concat()));
    assert!(all_finite(&broken.eye));
    assert!(all_finite(&broken.clip_plane));
    assert!(all_finite(&broken.params));
    assert_eq!(broken.params[0], 1.0, "a finite flag still uploads");
    assert_eq!(
        broken.params[1], 0.0,
        "an unusable clip plane disables discard"
    );

    // Instance sanitizing: a finite mapped object becomes one GPU instance;
    // non-finite or singular transforms are skipped rather than emitted.
    let object = scene.objects()[0];
    let instance = ObjectInstance::from_object(&object).expect("finite sphere instance");
    assert_eq!(instance.model, object.transform);
    assert_eq!(instance.color[3], 1.0);
    assert!(all_finite(&instance.model.concat()));
    assert!(all_finite(&instance.normal_0));
    assert!(all_finite(&instance.normal_1));
    assert!(all_finite(&instance.normal_2));

    let mut non_finite = object;
    non_finite.transform[0][0] = f32::NAN;
    assert!(ObjectInstance::from_object(&non_finite).is_none());

    let singular = MeshObject {
        transform: [[0.0; 4]; 4],
        ..object
    };
    assert!(ObjectInstance::from_object(&singular).is_none());

    let mut bad_colour = object;
    bad_colour.color[1] = f32::NAN;
    assert!(ObjectInstance::from_object(&bad_colour).is_none());

    // A scene of only skipped instances plans an empty draw list.
    assert!(
        plan.draws
            .iter()
            .all(|(_, instance)| all_finite(&instance.model.concat()))
    );

    eprintln!("GPU not requested: object device probe did not run");
}

#[test]
fn native_object_device_worker() {
    if std::env::var_os(OBJECT_PROBE_ENV).is_none() {
        cpu_contract_checks();
        return;
    }
    if let Err(error) = run_probe() {
        panic!("native object pass probe failed: {error}");
    }
}

// ---------------------------------------------------------------------------
// Device setup and error capture
// ---------------------------------------------------------------------------

struct Gpu {
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
        let failures = self.failures.lock().unwrap();
        errors.extend(failures.iter().cloned());
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
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
        label: Some("vfe object pass probe device"),
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
        device,
        queue,
        info,
        failures,
    })
}

// ---------------------------------------------------------------------------
// Probe stages
// ---------------------------------------------------------------------------

fn probe_object_pass(gpu: &Gpu, format: wgpu::TextureFormat) -> Result<(), String> {
    eprintln!(
        "PROBE device: adapter={} backend={:?} type={:?} max_texture_2d={}",
        gpu.info.name,
        gpu.info.backend,
        gpu.info.device_type,
        gpu.device.limits().max_texture_dimension_2d
    );

    let mut pass = ObjectPass::new(&gpu.device, format);
    if pass.texture_view().is_some() {
        return Err("a fresh ObjectPass owns no target".into());
    }

    let view = probe_view([64, 64]);
    submit_prepare(gpu, &mut pass, &MeshScene::empty(), &view)?;
    if pass.texture_view().is_some() {
        return Err("an empty scene allocated a target".into());
    }
    eprintln!("PROBE empty-scene: prepare returned Ok and allocated no target");

    let scene = sphere_scene();
    let zero_area = probe_view([0, 64]);
    submit_prepare(gpu, &mut pass, &scene, &zero_area)?;
    if pass.texture_view().is_some() {
        return Err("a zero-area viewport allocated a target".into());
    }
    eprintln!("PROBE zero-area: prepare returned Ok and allocated no target");

    submit_prepare(gpu, &mut pass, &scene, &view)?;
    if pass.texture_view().is_none() {
        return Err("a 64x64 viewport with one sphere allocated no target".into());
    }
    eprintln!("PROBE allocate: prepare created the 64x64 colour+depth target");

    let rect = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(64.0, 64.0));
    let first = composite_frame(gpu, &pass, format, [64, 64], rect)?;
    let clear = pixel(&first, 64, 0, 0);
    let differing = first
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|candidate| **candidate != clear)
        .count();
    if differing == 0 {
        return Err("every composited pixel equals the clear colour".into());
    }
    let expected_clear = expected_rgb(
        format,
        [
            PROBE_CLEAR[0] as f32,
            PROBE_CLEAR[1] as f32,
            PROBE_CLEAR[2] as f32,
        ],
    );
    assert_rgb("composite clear", clear, expected_clear);
    let centre = pixel(&first, 64, 32, 32);
    if centre == clear {
        return Err("the sphere did not cover the target centre".into());
    }
    eprintln!(
        "PROBE render: {differing} of 4096 pixels differ from the clear colour (centre {:?})",
        centre
    );

    // Same scene + view twice: the target contents are identical.
    let second = composite_frame(gpu, &pass, format, [64, 64], rect)?;
    if second != first {
        return Err("two identical frames produced different target contents".into());
    }
    eprintln!("PROBE determinism: a repeated frame is byte-identical");
    Ok(())
}

fn submit_prepare(
    gpu: &Gpu,
    pass: &mut ObjectPass,
    scene: &MeshScene,
    view: &ObjectView,
) -> Result<(), String> {
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("vfe object probe prepare encoder"),
        });
    pass.prepare(&gpu.device, &gpu.queue, &mut encoder, scene, view)?;
    gpu.queue.submit([encoder.finish()]);
    Ok(())
}

fn composite_frame(
    gpu: &Gpu,
    pass: &ObjectPass,
    format: wgpu::TextureFormat,
    screen: [u32; 2],
    rect: egui::Rect,
) -> Result<Vec<u8>, String> {
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
    let padded_row = screenshot::padded_row_bytes(screen[0]);
    let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("vfe probe object readback"),
        size: u64::from(padded_row) * u64::from(screen[1]),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("vfe probe composite encoder"),
        });
    {
        let mut render_pass = encoder
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
        pass.paint(&mut render_pass, rect);
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
    screenshot::to_rgba(format, pixels)
}

/// Proves the fully assembled offscreen capture path composites objects, not
/// just `ObjectPass` in isolation (#113). `DrawState::render_offscreen_with_objects`
/// is the native capture entry point; with the object encoder dropped the
/// readback is all clear, so this fails unless the object pass is submitted
/// with the screenshot pass.
fn probe_offscreen_capture(gpu: &Gpu, format: wgpu::TextureFormat) -> Result<(), String> {
    let viewport = [64u32, 64u32];
    let view = probe_view(viewport);
    let view_proj = Mat4::from_cols_array_2d(&view.view_proj);
    let camera_pos = Vec3::from_array(view.eye);

    let mut state = DrawState::new(&gpu.device, format);
    let vector_scene = SceneGPU::default();
    let capture = |state: &mut DrawState, objects: Option<&Arc<MeshScene>>| {
        state
            .render_offscreen_with_objects(
                &gpu.device,
                &gpu.queue,
                &vector_scene,
                objects,
                view_proj,
                camera_pos,
                viewport[0],
                viewport[1],
                PROBE_CLEAR,
            )
            .map_err(|error| format!("offscreen capture failed: {error}"))
    };

    // An empty object scene must keep the capture deterministic and equal to
    // the clear colour alone.
    let empty_first = capture(&mut state, None)?;
    let empty_second = capture(&mut state, None)?;
    if empty_first != empty_second {
        return Err("two empty-scene captures were not byte-identical".into());
    }
    let clear_pixel = pixel(&empty_first, viewport[0], 0, 0);
    let expected_clear = expected_rgb(
        format,
        [
            PROBE_CLEAR[0] as f32,
            PROBE_CLEAR[1] as f32,
            PROBE_CLEAR[2] as f32,
        ],
    );
    assert_rgb("capture clear", clear_pixel, expected_clear);
    if empty_first
        .as_chunks::<4>()
        .0
        .iter()
        .any(|candidate| *candidate != clear_pixel)
    {
        return Err("an empty object scene painted into the capture".into());
    }

    // A Sources-like sphere must reach the captured pixels.
    let objects = Arc::new(sphere_scene());
    let captured = capture(&mut state, Some(&objects))?;
    let differing = captured
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|candidate| **candidate != clear_pixel)
        .count();
    if differing == 0 {
        return Err(
            "the offscreen capture contains only the clear colour; the object pass was not submitted"
                .into(),
        );
    }
    let centre = pixel(&captured, viewport[0], viewport[0] / 2, viewport[1] / 2);
    if centre == clear_pixel {
        return Err("the sphere did not cover the capture centre".into());
    }
    eprintln!(
        "PROBE offscreen-capture: {differing} of {} pixels differ from the clear colour (centre {:?})",
        viewport[0] * viewport[1],
        centre
    );
    Ok(())
}

fn run_probe() -> Result<(), String> {
    let gpu = request_device()?;
    let scopes = gpu.begin_scopes("object pass");
    let result = probe_object_pass(&gpu, wgpu::TextureFormat::Rgba8UnormSrgb)
        .and_then(|()| probe_offscreen_capture(&gpu, wgpu::TextureFormat::Rgba8UnormSrgb));
    gpu.end_scopes(scopes)?;
    result
}

// ---------------------------------------------------------------------------
// Pixel helpers
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
