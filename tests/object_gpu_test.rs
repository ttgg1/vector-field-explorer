//! Object-pass CPU contract and native device probe (#112).
//!
//! Ordinary all-target runs exercise the device-free contracts below and print
//! `GPU not requested`. Only `scripts/test-object-gpu.sh` sets the child-only
//! `VFE_OBJECT_GPU_PROBE=1`, which requires a real adapter/device and drives the
//! actual `ObjectPass::prepare`/`paint` flow. There is no `#[ignore]`, noop
//! backend, fake adapter, or silent skip.

use std::sync::{Arc, Mutex};

use eframe::{egui, wgpu};
use glam::{DVec3, Mat4, UVec3, Vec3};

use vector_field_explorer::{
    VfApp,
    app::{compose_vector_lines, compose_vector_lines_blueprint},
    domain::Domain,
    layer_style::{LayerId, LayerOverride, LayerStyles},
    object_renderer::{
        OBJECT_PROBE_ENV, ObjectGlobals, ObjectInstance, ObjectPass, ObjectView, object_draw_plan,
    },
    object_scene::{MeshObject, MeshPrimitive, MeshScene, ObjectStyle, SliceClip},
    plane::{PlaneAxis, PlaneFrame, PlaneView},
    renderer::{self, DrawState, LineInstanceGPU, SceneGPU},
    screenshot,
    source_glyph::{
        source_body_lines, source_glyph_lines, source_glyph_lines_with_layer, source_glyphs,
        source_sign_marker_lines, source_sign_marker_lines_with_layer,
    },
    source_scene::{FieldMode, SourceEntry, SourceKindTag, SourceScene},
    sources::{Source, SourceKind},
    viewmodel::State,
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
        wireframe: ObjectStyle::Wireframe.is_wireframe(),
        ..view
    };
    assert_eq!(ObjectGlobals::from_view(&wireframe, None).params[0], 1.0);
    assert!(!ObjectStyle::Solid.is_wireframe());

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

fn point_glyph_state() -> State {
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Sources);
    let mut scene = SourceScene::default();
    scene.entries.push(SourceEntry {
        enabled: true,
        source: Source {
            center: DVec3::ZERO,
            axis: DVec3::Z,
            kind: SourceKind::PointCharge { charge: 1.0 },
        },
    });
    state.set_source_scene(scene);
    state
}

/// Compare every exposed line field. `b[3]` is the line width, so the full
/// `a.b == b.b` comparison pins width as well as the endpoint; the dash pattern
/// is realised as emitted segment splits, so equal endpoints and count also pin
/// the dashed geometry.
fn same_line(a: &LineInstanceGPU, b: &LineInstanceGPU) -> bool {
    a.a == b.a && a.b == b.b && a.color == b.color
}

fn assert_tail_is_markers(composed: &[LineInstanceGPU], expected: &[LineInstanceGPU]) {
    let tail = &composed[composed.len() - expected.len()..];
    assert_eq!(tail.len(), expected.len());
    assert!(
        tail.iter().zip(expected).all(|(a, b)| same_line(a, b)),
        "object-backed glyphs contribute only their sign markers"
    );
}

#[test]
fn composed_lines_keep_only_sign_markers_for_object_backed_glyphs() {
    let mut state = point_glyph_state();
    let domain = state.scene_domain();
    let glyphs = source_glyphs(&state.sources, &domain, state.plane_frame().as_ref());
    assert_eq!(glyphs.len(), 1);
    let bodies = source_body_lines(&glyphs, &domain);
    let markers = source_sign_marker_lines(&glyphs, &domain);
    let legacy_count = source_glyph_lines(&glyphs, &domain).len();
    assert!(!bodies.is_empty() && !markers.is_empty());
    assert!(
        !vector_field_explorer::app::object_scene_for_state(&state)
            .objects()
            .is_empty(),
        "the live Sources scene maps a body for this glyph"
    );

    for style in [ObjectStyle::Solid, ObjectStyle::Wireframe] {
        state.set_object_style(style);
        let (composed, count) = compose_vector_lines(&state, &[], &[]);
        assert_eq!(
            count, legacy_count,
            "the status count stays the legacy full-glyph count"
        );
        assert_tail_is_markers(&composed, &markers);
        assert!(
            composed
                .iter()
                .all(|line| !bodies.iter().any(|body| same_line(line, body))),
            "{style:?} must suppress the body line glyphs"
        );

        let (blueprint, blueprint_count) = compose_vector_lines_blueprint(&state, &[], &[]);
        assert!(
            blueprint.len() > composed.len(),
            "{style:?} blueprint composition must keep the extra body lines"
        );
        assert_eq!(blueprint_count, legacy_count);
        let legacy = source_glyph_lines(&glyphs, &domain);
        let blueprint_tail = &blueprint[blueprint.len() - blueprint_count..];
        assert_eq!(blueprint_tail.len(), legacy.len());
        assert!(
            blueprint_tail
                .iter()
                .zip(&legacy)
                .all(|(a, b)| same_line(a, b))
        );
    }
}

#[test]
fn induction_object_backed_glyphs_keep_only_markers_with_the_induction_override() {
    let mut state = State::new(true);
    assert!(state.set_field_mode(FieldMode::Induction));
    state.set_object_style(ObjectStyle::Solid);
    let domain = state.scene_domain();
    let mut scene = SourceScene::default();
    scene.entries.push(SourceEntry {
        enabled: true,
        source: state.induction.unit_source(),
    });
    let glyphs = source_glyphs(&scene, &domain, state.plane_frame().as_ref());
    assert!(!glyphs.is_empty(), "the induction conductor emits a glyph");
    assert!(
        !vector_field_explorer::app::object_scene_for_state(&state)
            .objects()
            .is_empty(),
        "the induction conductor maps an object body"
    );

    let slot = LayerId::ALL
        .iter()
        .position(|layer| *layer == LayerId::Induction)
        .expect("Induction is a layer");
    let mut styles = LayerStyles::default();
    styles.layers[slot] = LayerOverride {
        opacity: Some(0.5),
        width: Some(2.0),
        color: None,
    };
    state
        .commit_layer_styles(styles)
        .expect("the test induction style is valid");

    let bodies = source_body_lines(&glyphs, &domain);
    let neutral_markers = source_sign_marker_lines(&glyphs, &domain);
    let styled_markers =
        source_sign_marker_lines_with_layer(&glyphs, &domain, &styles, LayerId::Induction);
    let legacy = source_glyph_lines_with_layer(&glyphs, &domain, &styles, LayerId::Induction);
    assert!(!bodies.is_empty() && !styled_markers.is_empty());
    assert_eq!(
        legacy.len(),
        bodies.len() + styled_markers.len(),
        "the legacy full glyph is the body plus marker length"
    );

    let (composed, count) = compose_vector_lines(&state, &[], &[]);
    assert_eq!(count, legacy.len(), "the full-glyph count is preserved");
    assert_tail_is_markers(&composed, &styled_markers);
    assert!(
        composed
            .iter()
            .all(|line| !bodies.iter().any(|body| same_line(line, body))),
        "the Induction object-backed path suppresses the body glyph"
    );
    assert!(
        styled_markers.iter().any(|marker| neutral_markers
            .iter()
            .all(|neutral| !same_line(marker, neutral))),
        "the Induction layer override reaches the emitted markers"
    );
}

#[test]
fn empty_object_scene_keeps_the_legacy_line_glyphs() {
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Sources);
    let mut scene = SourceScene::default();
    scene.entries.push(SourceEntry {
        enabled: true,
        source: Source {
            // The glyph model repairs a non-finite centre, but the object
            // mapper rejects it, so this exercises the line-glyph fallback.
            center: DVec3::new(f64::NAN, 0.0, 0.0),
            axis: DVec3::Z,
            kind: SourceKind::PointCharge { charge: 1.0 },
        },
    });
    state.set_source_scene(scene);
    let domain = state.scene_domain();
    assert!(
        vector_field_explorer::app::object_scene_for_state(&state)
            .objects()
            .is_empty(),
        "the malformed source maps no object"
    );
    let glyphs = source_glyphs(&state.sources, &domain, state.plane_frame().as_ref());
    let legacy = source_glyph_lines(&glyphs, &domain);
    let (composed, count) = compose_vector_lines(&state, &[], &[]);
    assert_eq!(count, legacy.len());
    let tail = &composed[composed.len() - count..];
    assert_eq!(tail.len(), legacy.len());
    assert!(tail.iter().zip(&legacy).all(|(a, b)| same_line(a, b)));
}

#[test]
fn app_svg_blueprint_keeps_full_body_glyphs_when_objects_draw() {
    let mut app = VfApp::new();
    app.state.set_mode(false);
    app.state.set_field_mode(FieldMode::Sources);
    let mut scene = SourceScene::default();
    scene.entries.push(SourceEntry {
        enabled: true,
        source: Source {
            center: DVec3::ZERO,
            axis: DVec3::Z,
            kind: SourceKind::PointCharge { charge: 1.0 },
        },
    });
    app.state.set_source_scene(scene);

    for style in [ObjectStyle::Solid, ObjectStyle::Wireframe] {
        app.state.set_object_style(style);
        app.test_harness().advance_simulation();
        let blueprint = app.test_assemble_viewport_scene_with_styles(&LayerStyles::default());
        let live = app.test_assemble_viewport_scene();
        assert!(
            blueprint.lines.len() > live.lines.len(),
            "{style:?} SVG blueprint must carry the full line glyph, not just the \
             live marker: blueprint {}, live {}",
            blueprint.lines.len(),
            live.lines.len()
        );

        let domain = app.state.scene_domain();
        let glyphs = source_glyphs(
            &app.state.sources,
            &domain,
            app.state.plane_frame().as_ref(),
        );
        let bodies = source_body_lines(&glyphs, &domain);
        assert!(!bodies.is_empty());
        assert!(
            bodies
                .iter()
                .all(|body| blueprint.lines.iter().any(|line| same_line(line, body))),
            "{style:?}: every body segment is present in the SVG blueprint"
        );
        assert!(
            bodies
                .iter()
                .all(|body| !live.lines.iter().any(|line| same_line(line, body))),
            "{style:?}: the live object-backed scene still suppresses the body glyphs"
        );
    }
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

/// Proves the fully assembled offscreen capture path honours the committed
/// object style (#115). The capture call site sets [`DrawState::set_object_style`];
/// a Solid capture must stay byte-identical to the default (pre-style) capture,
/// while a Wireframe capture of the same scene must differ in pixels.
fn probe_capture_object_style(gpu: &Gpu, format: wgpu::TextureFormat) -> Result<(), String> {
    let viewport = [256u32, 256u32];
    let view = probe_view(viewport);
    let view_proj = Mat4::from_cols_array_2d(&view.view_proj);
    let camera_pos = Vec3::from_array(view.eye);
    let vector_scene = SceneGPU::default();
    let objects = Arc::new(sphere_scene());

    let mut state = DrawState::new(&gpu.device, format);
    if state.object_style() != ObjectStyle::Solid {
        return Err("a fresh DrawState does not default to the Solid capture style".into());
    }
    let capture = |state: &mut DrawState, style: Option<ObjectStyle>| {
        if let Some(style) = style {
            state.set_object_style(style);
        }
        state
            .render_offscreen_with_objects(
                &gpu.device,
                &gpu.queue,
                &vector_scene,
                Some(&objects),
                view_proj,
                camera_pos,
                viewport[0],
                viewport[1],
                PROBE_CLEAR,
            )
            .map_err(|error| format!("style capture failed: {error}"))
    };

    // No setter call: the default is the pre-change Solid behaviour.
    let default_capture = capture(&mut state, None)?;
    let solid_capture = capture(&mut state, Some(ObjectStyle::Solid))?;
    if default_capture != solid_capture {
        return Err(
            "an explicit Solid capture differs from the default capture; the pre-style \
             byte identity was lost"
                .into(),
        );
    }
    let wireframe_capture = capture(&mut state, Some(ObjectStyle::Wireframe))?;
    let differing = default_capture
        .as_chunks::<4>()
        .0
        .iter()
        .zip(wireframe_capture.as_chunks::<4>().0)
        .filter(|(solid, wire)| solid != wire)
        .count();
    if differing == 0 {
        return Err(
            "the Wireframe capture is byte-identical to the Solid capture; the committed style \
             did not reach the capture path"
                .into(),
        );
    }
    eprintln!(
        "PROBE capture-style: default == Solid, Wireframe differs in {differing} of {} pixels",
        viewport[0] * viewport[1]
    );
    Ok(())
}

/// An asymmetric sphere: its centre sits above the `y = 0` slice, so the
/// visible cap and the discarded cap have different silhouettes. A probe that
/// merely swapped the two clip comparisons could not pass by symmetry.
fn sphere_scene_at_y(center_y: f64, plane: Option<&PlaneFrame>) -> MeshScene {
    let domain = Domain::new(Vec3::splat(-2.0), Vec3::splat(2.0));
    let mut source = SourceKindTag::SolidSphere.default_source();
    source.center = DVec3::new(0.0, center_y, 0.0);
    let scene = SourceScene {
        entries: vec![SourceEntry {
            enabled: true,
            source,
        }],
        ..SourceScene::default()
    };
    MeshScene::from_sources(&scene, &domain, plane)
}

fn clipped_sphere_scene() -> MeshScene {
    let domain = Domain::new(Vec3::splat(-2.0), Vec3::splat(2.0));
    let frame = PlaneFrame::new(
        &domain,
        PlaneView {
            enabled: true,
            axis: PlaneAxis::Y,
            index: 2,
        },
        UVec3::new(5, 5, 5),
    );
    sphere_scene_at_y(0.5, Some(&frame))
}

/// Issue #114 slice 5 pixel evidence: the on-slice half is drawn by the normal
/// body pipeline, and the discarded half appears once as the attenuated
/// wireframe. The slice is world `y = 0` with the `+Y` half visible, so the
/// screen's lower half can only contain faint discarded-side pixels. An
/// unclipped reference frame of the same asymmetric sphere lets the probe
/// assert that those pixels are attenuated blends, not opaque body pixels that
/// a broken clip comparison would leave behind.
fn probe_slice_faint_side(gpu: &Gpu, format: wgpu::TextureFormat) -> Result<(), String> {
    const EDGE: u32 = 128;
    let scene = clipped_sphere_scene();
    if scene.clip().is_none() {
        return Err("the clipped sphere scene carries no slice plane".into());
    }
    let view = probe_view([EDGE, EDGE]);
    let rect = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(EDGE as f32, EDGE as f32));

    let mut pass = ObjectPass::new(&gpu.device, format);
    submit_prepare(gpu, &mut pass, &scene, &view)?;
    let frame = composite_frame(gpu, &pass, format, [EDGE, EDGE], rect)?;

    // The same sphere without the slice clip draws opaque bodies on both
    // halves; its lower-half pixels are the "body would have drawn here"
    // reference for the faint-pixel comparison.
    let mut reference = ObjectPass::new(&gpu.device, format);
    submit_prepare(gpu, &mut reference, &sphere_scene_at_y(0.5, None), &view)?;
    let full = composite_frame(gpu, &reference, format, [EDGE, EDGE], rect)?;

    let clear = pixel(&frame, EDGE, 0, 0);
    let mut on_slice = 0usize;
    let mut discarded = 0usize;
    let mut attenuated = 0usize;
    for y in 0..EDGE {
        for x in 0..EDGE {
            let current = pixel(&frame, EDGE, x, y);
            if current == clear {
                continue;
            }
            // World +Y maps to the top of the image.
            if y <= EDGE / 2 {
                on_slice += 1;
            } else {
                discarded += 1;
                let opaque = pixel(&full, EDGE, x, y);
                if opaque == clear {
                    return Err(format!(
                        "the unclipped reference has no body under the faint pixel ({x}, {y})"
                    ));
                }
                if current == opaque {
                    return Err(format!(
                        "the discarded-side pixel ({x}, {y}) is opaque body colour, not an attenuated faint pixel"
                    ));
                }
                attenuated += 1;
            }
        }
    }
    if on_slice == 0 {
        return Err("the on-slice half of the clipped sphere is missing".into());
    }
    if discarded == 0 {
        return Err(
            "the discarded side has no faint wireframe pixels; the extra attenuation draw did not record"
                .into(),
        );
    }
    if attenuated != discarded {
        return Err(format!(
            "only {attenuated} of {discarded} discarded-side pixels are attenuated; the rest are opaque body pixels"
        ));
    }
    eprintln!(
        "PROBE slice-faint: {on_slice} on-slice pixels, {discarded} attenuated discarded-side wireframe pixels"
    );
    Ok(())
}

fn run_probe() -> Result<(), String> {
    let gpu = request_device()?;
    let scopes = gpu.begin_scopes("object pass");
    let result = probe_object_pass(&gpu, wgpu::TextureFormat::Rgba8UnormSrgb)
        .and_then(|()| probe_offscreen_capture(&gpu, wgpu::TextureFormat::Rgba8UnormSrgb))
        .and_then(|()| probe_capture_object_style(&gpu, wgpu::TextureFormat::Rgba8UnormSrgb))
        .and_then(|()| probe_slice_faint_side(&gpu, wgpu::TextureFormat::Rgba8UnormSrgb));
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
