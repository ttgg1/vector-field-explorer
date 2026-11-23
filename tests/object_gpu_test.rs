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

fn probe_view_from(viewport: [u32; 2], eye: Vec3, up: Vec3) -> ObjectView {
    let aspect = viewport[0] as f32 / viewport[1].max(1) as f32;
    let view_proj = Mat4::perspective_rh(45_f32.to_radians(), aspect, 0.1, 100.0)
        * Mat4::look_at_rh(eye, Vec3::ZERO, up);
    ObjectView {
        view_proj: view_proj.to_cols_array_2d(),
        eye: eye.to_array(),
        viewport,
        wireframe: false,
        clear: PROBE_CLEAR,
    }
}

fn probe_view(viewport: [u32; 2]) -> ObjectView {
    probe_view_from(viewport, Vec3::new(0.0, 0.0, 3.0), Vec3::Y)
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
    incoming_frame(gpu, format, screen, |rp| pass.paint(rp, rect))
}

fn incoming_frame(
    gpu: &Gpu,
    format: wgpu::TextureFormat,
    screen: [u32; 2],
    paint: impl FnOnce(&mut wgpu::RenderPass<'static>),
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
        paint(&mut render_pass);
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

/// A charged disc is a 48-triangle, two-sided fan, so its barycentric
/// wireframe leaves large gaps. That makes it the discriminating probe for
/// "the discarded slice half is a shaded body", which a dense sphere
/// wireframe would fill and hide.
fn disc_scene(domain: &Domain, plane: Option<&PlaneFrame>) -> MeshScene {
    let scene = SourceScene {
        entries: vec![SourceEntry {
            enabled: true,
            source: Source {
                center: DVec3::ZERO,
                axis: DVec3::Z,
                kind: SourceKind::ChargedDisc {
                    charge: 1.0,
                    radius: 1.0,
                },
            },
        }],
        ..SourceScene::default()
    };
    MeshScene::from_sources(&scene, domain, plane)
}

/// Issue #207: on the 2D slice the discarded half must still read as a shaded
/// body from every direction, not as a faint wireframe or a dark opening. The
/// slice is world `y = 0` with the `+Y` half visible, and the camera sits at
/// `y = 0`, so world `+Y` maps to the upper image half from either `±Z` eye.
/// An unclipped reference frame of the same low-poly disc gives the "full
/// silhouette" pixels: every lower-half reference pixel must be present in the
/// clipped frame and attenuated rather than byte-identical to the opaque body,
/// while the upper kept half stays byte-identical. A sparse 48-triangle
/// wireframe would leave interior lower-half pixels empty, so filling them
/// pins a shaded body rather than an edge-only representation.
fn probe_slice_attenuated_far_side(gpu: &Gpu, format: wgpu::TextureFormat) -> Result<(), String> {
    const EDGE: u32 = 128;
    const CUT_BAND: u32 = 2;
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
    let scene = disc_scene(&domain, Some(&frame));
    if scene.clip().is_none() {
        return Err("the clipped disc scene carries no slice plane".into());
    }
    let full_scene = disc_scene(&domain, None);
    let mut pass = ObjectPass::new(&gpu.device, format);

    for (label, eye) in [
        ("+Z", Vec3::new(0.0, 0.0, 3.0)),
        ("-Z", Vec3::new(0.0, 0.0, -3.0)),
    ] {
        let view = probe_view_from([EDGE, EDGE], eye, Vec3::Y);
        let (frame, clear) = render_scene_view(gpu, &mut pass, &scene, &view, EDGE)?;
        let (full, full_clear) = render_scene_view(gpu, &mut pass, &full_scene, &view, EDGE)?;
        if clear != full_clear {
            return Err(format!(
                "{label}: the clipped and unclipped frames use different clear colours"
            ));
        }
        let mut kept = 0usize;
        let mut kept_mismatch = 0usize;
        let mut far_covered = 0usize;
        let mut far_missing = 0usize;
        let mut far_opaque = 0usize;
        for y in 0..EDGE {
            for x in 0..EDGE {
                let reference = pixel(&full, EDGE, x, y);
                if reference == clear {
                    continue;
                }
                let current = pixel(&frame, EDGE, x, y);
                if y + CUT_BAND < EDGE / 2 {
                    kept += 1;
                    if current != reference {
                        kept_mismatch += 1;
                    }
                } else if y > EDGE / 2 + CUT_BAND {
                    far_covered += 1;
                    if current == clear {
                        far_missing += 1;
                    } else if current == reference {
                        far_opaque += 1;
                    }
                }
            }
        }
        eprintln!(
            "PROBE slice-attenuated: {label}: {kept} kept pixels ({kept_mismatch} differ from the opaque body), {far_covered} far-side silhouette pixels ({far_missing} missing, {far_opaque} opaque)"
        );
        if kept == 0 {
            return Err(format!(
                "{label}: the kept half of the clipped sphere is missing"
            ));
        }
        if kept_mismatch != 0 {
            return Err(format!(
                "{label}: {kept_mismatch} kept-half pixels differ from the unclipped body; the attenuated pass must not cover the kept half"
            ));
        }
        if far_covered == 0 {
            return Err(format!("{label}: the reference has no far-side silhouette"));
        }
        if far_missing != 0 {
            return Err(format!(
                "{label}: {far_missing} of {far_covered} far-side silhouette pixels are empty; the discarded side is not covered by a shaded body"
            ));
        }
        if far_opaque != 0 {
            return Err(format!(
                "{label}: {far_opaque} far-side pixels are byte-identical to the opaque body; the discarded side must be attenuated"
            ));
        }

        // Wireframe style: the far side must keep the edges-only reading, not
        // become the Solid shaded body. The sparse disc guarantees interior
        // far-side pixels that the Solid pass fills but a wireframe leaves
        // empty, while some edge pixels must still survive.
        let mut wire_view = view;
        wire_view.wireframe = true;
        let (wire, wire_clear) = render_scene_view(gpu, &mut pass, &scene, &wire_view, EDGE)?;
        if wire_clear != clear {
            return Err(format!(
                "{label}: the Wireframe frame uses a different clear colour"
            ));
        }
        let mut wire_edges = 0usize;
        let mut wire_interior_empty = 0usize;
        for y in 0..EDGE {
            for x in 0..EDGE {
                if pixel(&full, EDGE, x, y) == clear || y <= EDGE / 2 + CUT_BAND {
                    continue;
                }
                if pixel(&wire, EDGE, x, y) == clear {
                    if pixel(&frame, EDGE, x, y) != clear {
                        wire_interior_empty += 1;
                    }
                } else {
                    wire_edges += 1;
                }
            }
        }
        eprintln!(
            "PROBE slice-attenuated: {label}: Wireframe far side has {wire_edges} edge pixels and {wire_interior_empty} interior pixels empty where Solid fills"
        );
        if wire_edges == 0 {
            return Err(format!(
                "{label}: the Wireframe far side has no pixels; the discarded half disappeared"
            ));
        }
        if wire_interior_empty == 0 {
            return Err(format!(
                "{label}: the Wireframe far side fills every Solid interior pixel; the attenuated pass ignored the object style"
            ));
        }
    }
    Ok(())
}

/// Render one scene into a fresh incoming target and return the composited
/// pixels with the target's clear pixel (always sampled at `(0, 0)`).
fn render_scene_view(
    gpu: &Gpu,
    pass: &mut ObjectPass,
    scene: &MeshScene,
    view: &ObjectView,
    edge: u32,
) -> Result<(Vec<u8>, [u8; 4]), String> {
    submit_prepare(gpu, pass, scene, view)?;
    let rect = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(edge as f32, edge as f32));
    let frame = composite_frame(
        gpu,
        pass,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        [edge, edge],
        rect,
    )?;
    let clear = pixel(&frame, edge, 0, 0);
    Ok((frame, clear))
}

fn body_pixels(frame: &[u8], clear: [u8; 4]) -> usize {
    frame
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|candidate| **candidate != clear)
        .count()
}

/// Render one view of the unclipped sphere and return how many pixels differ
/// from the target clear colour, the centre pixel and the clear pixel.
fn render_sphere_view(
    gpu: &Gpu,
    pass: &mut ObjectPass,
    scene: &MeshScene,
    view: &ObjectView,
    edge: u32,
) -> Result<(usize, [u8; 4], [u8; 4]), String> {
    let (frame, clear) = render_scene_view(gpu, pass, scene, view, edge)?;
    let differing = body_pixels(&frame, clear);
    let centre = pixel(&frame, edge, edge / 2, edge / 2);
    Ok((differing, centre, clear))
}

/// Issue #207 reproduction: a closed, unclipped sphere must cover roughly the
/// same number of pixels and reach its centre from either side of every axis.
/// A one-sided failure shows up as zero coverage from the opposite eye while
/// the original view still passes.
fn probe_opposite_views(gpu: &Gpu) -> Result<(), String> {
    const EDGE: u32 = 128;
    let scene = sphere_scene();
    let mut pass = ObjectPass::new(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb);
    let eyes = [
        ("+Z", Vec3::new(0.0, 0.0, 3.0), Vec3::Y),
        ("-Z", Vec3::new(0.0, 0.0, -3.0), Vec3::Y),
        ("+X", Vec3::new(3.0, 0.0, 0.0), Vec3::Y),
        ("-X", Vec3::new(-3.0, 0.0, 0.0), Vec3::Y),
        ("+Y", Vec3::new(0.0, 3.0, 0.0), Vec3::Z),
        ("-Y", Vec3::new(0.0, -3.0, 0.0), Vec3::Z),
    ];
    let mut reference = None;
    for (label, eye, up) in eyes {
        let view = probe_view_from([EDGE, EDGE], eye, up);
        let (differing, centre, clear) = render_sphere_view(gpu, &mut pass, &scene, &view, EDGE)?;
        eprintln!("PROBE view {label}: {differing} pixels differ from clear, centre {centre:?}");
        if differing == 0 {
            return Err(format!(
                "the sphere is invisible from eye {label} ({eye:?}); one-sided rendering reproduced"
            ));
        }
        if centre == clear {
            return Err(format!(
                "the sphere covers no centre pixel from eye {label}; it is clipped or depth-rejected"
            ));
        }
        match reference {
            None => reference = Some(differing),
            Some(first) => {
                let ratio = differing as f32 / first.max(1) as f32;
                if !(0.7..=1.3).contains(&ratio) {
                    return Err(format!(
                        "coverage from eye {label} is {differing} vs {first} from the first eye (ratio {ratio:.2}); a full sphere must cover a similar area"
                    ));
                }
            }
        }
    }
    Ok(())
}

/// One Sources-mode state with a single solid sphere at `center`.
fn sources_state_with_sphere(center: DVec3, radius: f64) -> State {
    let mut state = State::new(false);
    state.set_field_mode(FieldMode::Sources);
    let mut sources = SourceScene::default();
    sources.entries.push(SourceEntry {
        enabled: true,
        source: Source {
            center,
            axis: DVec3::Z,
            kind: SourceKind::SolidSphere {
                charge: 1.0,
                radius,
            },
        },
    });
    state.set_source_scene(sources);
    state
}

/// Apply the documented live viewport cache rule to a sequence of mapped
/// scenes. Re-deriving the rule from `generation()` and `clip()` here (rather
/// than replaying `MeshScene::cache_matches`) keeps the probe an independent
/// statement of the contract: the viewport and this probe must both follow the
/// same documented key, not merely agree with one helper.
fn cached_scene_after(scenes: impl IntoIterator<Item = MeshScene>) -> MeshScene {
    let mut cached = MeshScene::empty();
    for next in scenes {
        if next.generation() != cached.generation() || next.clip() != cached.clip() {
            cached = next;
        }
    }
    cached
}

/// Issue #207 regression: the live viewport caches the object scene, and its
/// key must include the slice `clip`, not only `generation()`. A 2D → 3D switch
/// must replace the clipped scene with the unclipped one (full shaded body from
/// both `±Z` eyes), and 3D → 2D must replace it with the clipped one. The
/// generations are deliberately equal here, so a generation-only cache would
/// keep the wrong scene.
fn probe_stale_slice_clip_cache(gpu: &Gpu, format: wgpu::TextureFormat) -> Result<(), String> {
    const EDGE: u32 = 128;
    let mut pass = ObjectPass::new(&gpu.device, format);

    // Off-centre bodies expose a retained clip: the slice cut removes roughly
    // half of the body from one side and none from the other.
    for (label, center_z) in [("+Z source", 1.0), ("-Z source", -1.0)] {
        let mut state = sources_state_with_sphere(DVec3::new(0.0, 0.0, center_z), 0.6);
        let two_d = vector_field_explorer::app::object_scene_for_state(&state);
        if two_d.clip().is_none() {
            return Err(format!("{label}: the 2D Sources scene maps no slice clip"));
        }
        if !state.set_mode(true) {
            return Err(format!("{label}: switching the state to 3D was a no-op"));
        }
        let three_d = vector_field_explorer::app::object_scene_for_state(&state);
        if three_d.clip().is_some() {
            return Err(format!("{label}: the 3D scene still maps a slice clip"));
        }
        if two_d.generation() != three_d.generation() {
            return Err(format!(
                "{label}: the 2D and 3D generations differ ({} vs {}), so the cache key would not need the clip; the regression premise no longer holds",
                two_d.generation(),
                three_d.generation()
            ));
        }
        let cached = cached_scene_after([two_d.clone(), three_d.clone()]);
        if cached.clip().is_some() {
            return Err(format!(
                "{label}: 2D -> 3D kept the 2D slice clip; bodies stay cut in 3D"
            ));
        }
        let mut coverages = Vec::new();
        for (eye_label, eye) in [
            ("+Z", Vec3::new(0.0, 0.0, 3.0)),
            ("-Z", Vec3::new(0.0, 0.0, -3.0)),
        ] {
            let view = probe_view_from([EDGE, EDGE], eye, Vec3::Y);
            let (frame, clear) = render_scene_view(gpu, &mut pass, &cached, &view, EDGE)?;
            let pixels = body_pixels(&frame, clear);
            let centre = pixel(&frame, EDGE, EDGE / 2, EDGE / 2);
            if pixels == 0 {
                return Err(format!(
                    "{label}: the unclipped 3D body is invisible from {eye_label}; the stale clip survived"
                ));
            }
            if centre == clear {
                return Err(format!(
                    "{label}: the unclipped 3D body covers no centre pixel from {eye_label}"
                ));
            }
            coverages.push(pixels);
        }
        eprintln!(
            "PROBE stale-clip: {label}, generations {}/{}, cached clip {}, unclipped coverage {coverages:?} from ±Z",
            two_d.generation(),
            three_d.generation(),
            cached.clip().is_some(),
        );
    }

    // A centred body is symmetric under a plane through the origin, so only
    // the cache clip state and the two-sided coverage can distinguish the
    // stale scene from the fresh one.
    let mut state = sources_state_with_sphere(DVec3::ZERO, 0.8);
    let two_d = vector_field_explorer::app::object_scene_for_state(&state);
    if !state.set_mode(true) {
        return Err("centred body: switching the state to 3D was a no-op".into());
    }
    let three_d = vector_field_explorer::app::object_scene_for_state(&state);
    if two_d.generation() != three_d.generation() || two_d.clip().is_none() {
        return Err("centred body: the stale-clip premise no longer holds".into());
    }
    let cached = cached_scene_after([two_d, three_d.clone()]);
    if cached.clip().is_some() {
        return Err("centred body: 2D -> 3D kept the 2D slice clip".into());
    }
    let mut coverages = Vec::new();
    for (label, eye) in [
        ("+Z", Vec3::new(0.0, 0.0, 3.0)),
        ("-Z", Vec3::new(0.0, 0.0, -3.0)),
    ] {
        let view = probe_view_from([EDGE, EDGE], eye, Vec3::Y);
        let (frame, clear) = render_scene_view(gpu, &mut pass, &cached, &view, EDGE)?;
        let pixels = body_pixels(&frame, clear);
        if pixels == 0 || pixel(&frame, EDGE, EDGE / 2, EDGE / 2) == clear {
            return Err(format!(
                "centred body: the unclipped 3D body is invisible from {label}"
            ));
        }
        coverages.push(pixels);
    }
    let ratio = coverages[1] as f32 / coverages[0].max(1) as f32;
    eprintln!(
        "PROBE stale-clip: centred body, cached clip {}, coverage {coverages:?} from ±Z (ratio {ratio:.2})",
        cached.clip().is_some()
    );
    if !(0.7..=1.3).contains(&ratio) {
        return Err(format!(
            "centred body: unclipped coverage {coverages:?} differs between ±Z (ratio {ratio:.2})"
        ));
    }

    // The symmetric direction: enter 3D first, then switch to 2D. The cache
    // must now keep the clipped scene; a 2D view without the cut would show
    // full bodies.
    let mut state = sources_state_with_sphere(DVec3::new(0.0, 0.0, -1.0), 0.6);
    if !state.set_mode(true) {
        return Err("3D -> 2D: starting the probe state in 3D was a no-op".into());
    }
    let three_d = vector_field_explorer::app::object_scene_for_state(&state);
    if three_d.clip().is_some() {
        return Err("3D -> 2D: the 3D scene maps a slice clip".into());
    }
    if !state.set_mode(false) {
        return Err("3D -> 2D: switching the state to 2D was a no-op".into());
    }
    let two_d = vector_field_explorer::app::object_scene_for_state(&state);
    if two_d.clip().is_none() {
        return Err("3D -> 2D: the 2D scene maps no slice clip".into());
    }
    if two_d.generation() != three_d.generation() {
        return Err(format!(
            "3D -> 2D: the generations differ ({} vs {}), so the cache key would not need the clip",
            two_d.generation(),
            three_d.generation()
        ));
    }
    let cached = cached_scene_after([three_d, two_d.clone()]);
    eprintln!(
        "PROBE stale-clip: 3D -> 2D keeps clip {} (2D clip {}), generation {}",
        cached.clip().is_some(),
        two_d.clip().is_some(),
        two_d.generation()
    );
    if cached.clip().is_none() {
        return Err("3D -> 2D: the cache dropped the 2D slice clip".into());
    }
    Ok(())
}

fn run_probe() -> Result<(), String> {
    let gpu = request_device()?;
    let scopes = gpu.begin_scopes("object pass");
    let result = probe_grid_depth(&gpu)
        .and_then(|()| probe_object_pass(&gpu, wgpu::TextureFormat::Rgba8UnormSrgb))
        .and_then(|()| probe_offscreen_capture(&gpu, wgpu::TextureFormat::Rgba8UnormSrgb))
        .and_then(|()| probe_capture_object_style(&gpu, wgpu::TextureFormat::Rgba8UnormSrgb))
        .and_then(|()| probe_slice_attenuated_far_side(&gpu, wgpu::TextureFormat::Rgba8UnormSrgb))
        .and_then(|()| probe_opposite_views(&gpu))
        .and_then(|()| probe_stale_slice_clip_cache(&gpu, wgpu::TextureFormat::Rgba8UnormSrgb));
    gpu.end_scopes(scopes)?;
    result
}

fn probe_grid_depth(gpu: &Gpu) -> Result<(), String> {
    let mut draw = DrawState::new(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb);
    let objects = Arc::new(sphere_scene());
    let view = probe_view([128, 128]);
    let mut capture = |scene: &SceneGPU, bodies: Option<&Arc<MeshScene>>, style| {
        draw.set_object_style(style);
        draw.render_offscreen_with_objects(
            &gpu.device,
            &gpu.queue,
            scene,
            bodies,
            Mat4::from_cols_array_2d(&view.view_proj),
            Vec3::from_array(view.eye),
            128,
            128,
            PROBE_CLEAR,
        )
        .map_err(|e| e.to_string())
    };
    let solid = ObjectStyle::Solid;
    let body = capture(&SceneGPU::default(), Some(&objects), solid)?;
    for (label, z) in [("behind", -1.25), ("front", 1.25)] {
        let domain = Domain::new(Vec3::new(-3., 0., z - 0.1), Vec3::new(3., 1., z + 0.1));
        let mut line = vector_field_explorer::reference::reference_lines(&domain, false)[0];
        line.color = [1., 0., 1., 1.];
        line.b[3] = 0.12;
        let scene = SceneGPU {
            lines: Arc::new(vec![line]),
            ..Default::default()
        };
        let bare = capture(&scene, None, solid)?;
        let over = capture(&scene, Some(&objects), solid)?;
        let ink = pixel(&bare, 128, 64, 64);
        assert_eq!(ink, [255, 0, 255, 255]);
        if label == "behind" {
            let wire = capture(&scene, Some(&objects), ObjectStyle::Wireframe)?;
            assert_eq!(
                pixel(&wire, 128, 64, 64),
                ink,
                "wireframe remains painter-only"
            );
            let mut axis = vector_field_explorer::reference::reference_lines(&domain, false)
                .into_iter()
                .rev()
                .nth(1)
                .unwrap();
            axis.a[..3].copy_from_slice(&line.a[..3]);
            axis.b = line.b;
            axis.color = line.color;
            let painter = SceneGPU {
                lines: Arc::new(vec![axis]),
                ..Default::default()
            };
            let axis_over = capture(&painter, Some(&objects), solid)?;
            assert_eq!(
                pixel(&axis_over, 128, 64, 64),
                ink,
                "identically styled axis"
            );

            let domain = Domain::new(Vec3::splat(-2.), Vec3::splat(2.));
            let plane = PlaneFrame::new(
                &domain,
                PlaneView {
                    enabled: true,
                    axis: PlaneAxis::Y,
                    index: 2,
                },
                UVec3::splat(5),
            );
            let clipped = Arc::new(disc_scene(&domain, Some(&plane)));
            line.a[1] = -0.3;
            line.b[1] = -0.3;
            let shifted = SceneGPU {
                lines: Arc::new(vec![line]),
                ..Default::default()
            };
            let translucent = capture(&shifted, Some(&clipped), solid)?;
            assert_eq!(
                pixel(&translucent, 128, 64, 75),
                ink,
                "attenuated slice half"
            );
        }
        assert_eq!(pixel(&over, 128, 1, 64), ink, "{label} unobstructed");
        assert_eq!(
            pixel(&over, 128, 64, 64),
            if label == "behind" {
                pixel(&body, 128, 64, 64)
            } else {
                ink
            },
            "{label} grid over sphere"
        );
        let live = clipped_callback(gpu, scene, objects.clone(), &view)?;
        for x in 16..128 {
            assert_eq!(
                pixel(&live, 160, 16 + x, 72),
                pixel(&over, 128, x, 64),
                "{label} clipped scanline at {x}"
            );
        }
        assert_eq!(
            pixel(&live, 160, 80, 72),
            pixel(&over, 128, 64, 64),
            "{label} live"
        );
        assert_eq!(
            pixel(&live, 160, 140, 72),
            ink,
            "{label} live background grid"
        );
        assert_eq!(
            pixel(&live, 160, 20, 72),
            pixel(&live, 160, 0, 0),
            "scissor"
        );
    }
    Ok(())
}

fn clipped_callback(
    gpu: &Gpu,
    scene: SceneGPU,
    objects: Arc<MeshScene>,
    view: &ObjectView,
) -> Result<Vec<u8>, String> {
    use eframe::egui_wgpu::{CallbackResources, CallbackTrait, ScreenDescriptor};
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let mut resources = CallbackResources::default();
    resources.insert(DrawState::new(&gpu.device, format));
    let screen = egui::Rect::from_min_size(egui::pos2(8., 4.), egui::vec2(64., 64.));
    let callback = renderer::DrawCallback {
        scene,
        objects,
        object_style: ObjectStyle::Solid,
        screen,
        object_viewport: [128, 128],
        object_clear: PROBE_CLEAR,
        view_proj: Mat4::from_cols_array_2d(&view.view_proj),
        camera_pos: Vec3::from_array(view.eye),
    };
    let descriptor = ScreenDescriptor {
        size_in_pixels: [160, 144],
        pixels_per_point: 2.,
    };
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    let buffers = callback.prepare(
        &gpu.device,
        &gpu.queue,
        &descriptor,
        &mut encoder,
        &mut resources,
    );
    gpu.queue
        .submit(buffers.into_iter().chain([encoder.finish()]));
    incoming_frame(gpu, format, descriptor.size_in_pixels, |pass| {
        pass.set_viewport(16., 8., 128., 128., 0., 1.);
        pass.set_scissor_rect(32, 8, 112, 128);
        callback.paint(
            egui::PaintCallbackInfo {
                viewport: screen,
                clip_rect: egui::Rect::from_min_max(egui::pos2(16., 4.), screen.max),
                pixels_per_point: 2.,
                screen_size_px: descriptor.size_in_pixels,
            },
            pass,
            &resources,
        );
    })
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
