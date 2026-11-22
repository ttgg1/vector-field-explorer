//! Issue #97 slice B: persisted plane-wave controls, separate E/B overlays on
//! the shared clock, panel honesty and capture paths.
//!
//! Capture is proven at the `SceneGPU` level: `assemble_viewport_scene` is the
//! exact geometry the GUI, PNG and animation paths consume, so the wave arrows
//! being present there is the device-independent capture assertion. No GPU probe
//! is needed and none is gated here.

use eframe::egui;

use crate::support;

use glam::Vec3;

use vector_field_explorer::VfApp;
use vector_field_explorer::animation_job::AnimationEligibility;
use vector_field_explorer::field_probe::{
    CirculationLoop, GaussianSurface, LoopKind, ProbeField, SurfaceKind, circulation, flux,
};
use vector_field_explorer::maxwell_wave::{MAX_AMPLITUDE, WaveSpec, sample_wave};
use vector_field_explorer::persistence::ViewRecord;
use vector_field_explorer::source_scene::{ELECTRIC_COLOR, FieldMode, MAGNETIC_COLOR};
use vector_field_explorer::sources::FieldFamily;
use vector_field_explorer::ui::maxwell_panel::{
    self, MaxwellPanelState, WAVE_ANIMATION_NOTE, WAVE_NORMALIZED_NOTE, WAVE_SCOPE_NOTE,
    WAVE_STATUS_CLOCK_NOTE, WAVE_SVG_NOTE,
};
use vector_field_explorer::ui::sidebar::Controls;
use vector_field_explorer::viewmodel::State;

fn valid_wave() -> WaveSpec {
    WaveSpec {
        amplitude: 1.5,
        wavelength: 2.0,
        direction: Vec3::X,
        polarization: Vec3::Y,
        phase: 0.25,
    }
}

fn wave_state(mode_3d: bool) -> State {
    let mut state = State::new(mode_3d);
    state.set_field_mode(FieldMode::Maxwell);
    state.set_mode(mode_3d);
    assert!(state.set_maxwell_wave(Some(valid_wave())));
    state.refresh_maxwell_sampling();
    state
}

/// The same analytic wave seam the display samples, exposed as a probe field so
/// the test can compute the expected integral independently of the app.
struct WaveProbeField {
    spec: WaveSpec,
    family: FieldFamily,
}

impl ProbeField for WaveProbeField {
    fn eval(&self, point: Vec3, time: f64) -> Option<Vec3> {
        let sample = sample_wave(&self.spec, point, time)?;
        Some(match self.family {
            FieldFamily::Electric => sample.e,
            FieldFamily::Magnetic => sample.b,
        })
    }
}

#[test]
fn wave_state_round_trips_through_the_settings_envelope() {
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Maxwell);
    let mut spec = valid_wave();
    spec.validate().expect("the sample wave validates");
    assert!(state.set_maxwell_wave(Some(spec)));

    let record = ViewRecord::capture(&state);
    assert_eq!(
        record.maxwell_wave,
        Some(spec),
        "capture carries the validated wave spec"
    );

    let json = serde_json::to_string(&record).expect("the envelope serializes");
    assert!(
        json.contains("\"maxwell_wave\""),
        "the wave is a persisted field: {json}"
    );
    let back: ViewRecord = serde_json::from_str(&json).expect("the envelope deserializes");
    assert_eq!(back.maxwell_wave, Some(spec));

    let mut restored = State::new(false);
    back.restore(&mut restored, &mut Controls::new());
    assert_eq!(restored.field_mode, FieldMode::Maxwell);
    assert_eq!(restored.maxwell_wave(), Some(spec));
    assert!(
        restored.maxwell_wave_error.is_none(),
        "a valid stored wave restores without an error"
    );
}

#[test]
fn invalid_stored_wave_is_dropped_on_restore_without_dirtying() {
    let record: ViewRecord = serde_json::from_str(
        r#"{
            "maxwell_wave": {
                "amplitude": 2000000.0,
                "wavelength": 2.0,
                "direction": [1.0, 0.0, 0.0],
                "polarization": [0.0, 1.0, 0.0],
                "phase": 0.0
            }
        }"#,
    )
    .expect("the record decodes");

    let mut state = State::new(false);
    state.dirty = false;
    record.restore_maxwell_wave(&mut state);

    assert_eq!(
        state.maxwell_wave(),
        None,
        "an invalid stored wave is dropped, not clamped"
    );
    assert!(!state.dirty, "restore must not dirty the first scene");
    assert!(state.maxwell_wave_error.is_none());
}

#[test]
fn set_wave_is_a_noop_for_an_equal_spec_and_dirties_once_on_change() {
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Maxwell);
    state.dirty = false;

    assert!(state.set_maxwell_wave(Some(valid_wave())));
    let stored = state.maxwell_wave().expect("wave enabled");
    assert!(state.dirty, "a changed wave dirties the scene once");
    assert!(state.maxwell_wave_error.is_none());

    state.dirty = false;
    assert!(!state.set_maxwell_wave(Some(stored)));
    assert!(!state.dirty, "an equal spec is a no-op");
    assert!(state.maxwell_wave_error.is_none());

    // Validation repairs non-unit axes; the setter still accepts the spec.
    let mut unnormalized = valid_wave();
    unnormalized.direction = Vec3::new(0.0, 3.0, 0.0);
    unnormalized.polarization = Vec3::new(2.0, 2.0, 0.0);
    assert!(state.set_maxwell_wave(Some(unnormalized)));
    let repaired = state.maxwell_wave().expect("repaired wave");
    assert!((repaired.direction.length() - 1.0).abs() <= 1e-5);
    assert!((repaired.polarization.length() - 1.0).abs() <= 1e-5);
    assert!(repaired.polarization.dot(repaired.direction).abs() <= 1e-5);

    let mut invalid = valid_wave();
    invalid.amplitude = MAX_AMPLITUDE * 2.0;
    assert!(!state.set_maxwell_wave(Some(invalid)));
    assert_eq!(state.maxwell_wave(), Some(repaired));
    let error = state
        .maxwell_wave_error
        .as_deref()
        .expect("the rejected value reports its slice A message");
    assert!(error.contains("amplitude"), "{error}");
}

#[test]
fn wave_sampling_emits_separate_electric_and_magnetic_arrow_sets() {
    let state = wave_state(true);
    let wave = state
        .maxwell_wave_sampling
        .as_ref()
        .expect("wave arrows sampled");
    assert!(
        !wave.electric.is_empty(),
        "the electric set is emitted separately"
    );
    assert!(
        !wave.magnetic.is_empty(),
        "the magnetic set is emitted separately"
    );
    assert!(
        wave.electric
            .iter()
            .all(|arrow| arrow.color == ELECTRIC_COLOR),
        "E keeps the electric-family colour"
    );
    assert!(
        wave.magnetic
            .iter()
            .all(|arrow| arrow.color == MAGNETIC_COLOR),
        "B keeps the magnetic-family colour"
    );
    assert!(
        state.maxwell_sampling.is_none(),
        "the case sampling is not mixed into the wave"
    );

    let first = &wave.electric[0];
    let sample = sample_wave(&valid_wave(), first.position, 0.0);
    assert!(
        sample.is_some(),
        "the emitted arrows come from slice A samples"
    );
}

#[test]
fn wave_arrows_follow_the_clock_and_freeze_when_paused() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Maxwell);
    assert!(app.state.set_maxwell_wave(Some(valid_wave())));
    app.test_harness().advance_simulation();
    let first = app
        .state
        .maxwell_wave_sampling
        .clone()
        .expect("sampled while playing");

    app.test_harness()
        .advance_clock(vector_field_explorer::sim_clock::MAX_STEP);
    app.test_harness().advance_simulation();
    let second = app
        .state
        .maxwell_wave_sampling
        .clone()
        .expect("sampled after the clock moved");
    assert_ne!(
        first, second,
        "the same sampling grid changes with state.time"
    );

    app.clock.set_playing(false);
    let paused = app.state.maxwell_wave_sampling.clone();
    app.test_harness()
        .advance_clock(vector_field_explorer::sim_clock::MAX_STEP);
    app.test_harness().advance_simulation();
    assert_eq!(
        app.state.maxwell_wave_sampling, paused,
        "a paused clock freezes the wave arrows"
    );
    assert!(app.state.has_time_dependent_consumer());
}

#[test]
fn wave_arrows_skip_non_finite_samples() {
    let mut state = wave_state(false);
    state.time = f32::NAN;
    state.refresh_maxwell_sampling();

    let wave = state
        .maxwell_wave_sampling
        .as_ref()
        .expect("a wave sampling pass exists even when every sample is rejected");
    assert!(
        wave.electric.is_empty() && wave.magnetic.is_empty(),
        "non-finite time draws no arrows instead of NaN"
    );
    for arrow in wave.electric.iter().chain(&wave.magnetic) {
        assert!(arrow.position.is_finite());
        assert!(arrow.dir.is_finite());
        assert!(arrow.mag.is_finite());
    }
}

#[test]
fn wave_panel_shows_validation_messages_and_equation_statuses() {
    let ctx = context();
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Maxwell);
    assert!(state.set_maxwell_wave(Some(valid_wave())));
    let mut panel = MaxwellPanelState::default();
    panel.sync_from(&state);
    panel.wave_open = true;

    let out = panel_frame(&ctx, 1200.0, 900.0, &mut state, &mut panel);
    let texts = painted_texts(&out).join("\n");
    for needle in [
        WAVE_SCOPE_NOTE,
        WAVE_NORMALIZED_NOTE,
        WAVE_STATUS_CLOCK_NOTE,
        WAVE_SVG_NOTE,
        WAVE_ANIMATION_NOTE,
        "Gauss's law for electricity: Analytic",
        "Gauss's law for magnetism: Analytic",
        "Faraday's law of induction: Analytic",
        "Ampere-Maxwell law: Unsupported (j)",
    ] {
        assert!(
            texts.contains(needle),
            "the panel is missing {needle:?}: {texts}"
        );
    }
    assert!(
        texts.to_lowercase().contains("source-free"),
        "Ampere-Maxwell's source-free limit is stated"
    );

    panel.wave = Some(WaveSpec {
        amplitude: MAX_AMPLITUDE * 2.0,
        ..valid_wave()
    });
    assert!(!panel.commit_wave(&mut state));
    let out = panel_frame(&ctx, 1200.0, 900.0, &mut state, &mut panel);
    let texts = painted_texts(&out).join("\n");
    assert!(
        texts.contains("amplitude must be finite and within +/-1000000"),
        "the slice A validation message is shown verbatim: {texts}"
    );
}

#[test]
fn wave_capture_includes_the_arrows() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Maxwell);
    app.state.set_mode(true);
    assert!(app.state.set_maxwell_wave(Some(valid_wave())));
    app.test_harness().advance_simulation();

    let scene = app.test_harness().test_assemble_viewport_scene();
    let wave = app
        .state
        .maxwell_wave_sampling
        .as_ref()
        .expect("wave sampling");
    assert_eq!(wave.magnetic[0].color, MAGNETIC_COLOR);
    assert_eq!(wave.electric[0].color, ELECTRIC_COLOR);
    assert_eq!(
        scene.arrows.len(),
        wave.electric.len() + wave.magnetic.len(),
        "the GPU scene carries both fixed-family arrow sets for captures"
    );
    assert!(
        scene
            .arrows
            .iter()
            .any(|arrow| arrow.color[..3] == rgb(ELECTRIC_COLOR)),
        "the PNG/GUI scene includes the electric arrows"
    );
    assert!(
        scene
            .arrows
            .iter()
            .any(|arrow| arrow.color[..3] == rgb(MAGNETIC_COLOR)),
        "the PNG/GUI scene includes the magnetic arrows"
    );
}

#[test]
fn enabling_a_wave_clears_a_stale_case_error() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Maxwell);
    app.state.maxwell_error = Some("stale Conductor2D BEM failure".to_owned());

    assert!(app.state.set_maxwell_wave(Some(valid_wave())));
    app.state.refresh_maxwell_sampling();

    assert!(
        app.state.maxwell_wave_sampling.is_some(),
        "the wave arrows are drawn"
    );
    assert!(
        app.state.maxwell_error.is_none(),
        "a case error from before the wave is not kept as the wave's error"
    );
    assert_eq!(
        app.test_harness().maxwell_sampling_error(),
        None,
        "the stale case error is not readable while the wave is active"
    );
}

#[test]
fn re_enabling_the_wave_restores_the_last_spec() {
    let mut panel = MaxwellPanelState::default();
    let mut spec = valid_wave();
    spec.amplitude = 2.5;
    spec.wavelength = 3.5;
    spec.phase = 0.75;
    spec.direction = Vec3::Z;
    spec.polarization = Vec3::X;
    panel.wave = Some(spec);

    panel.set_wave_enabled(false);
    assert!(panel.wave.is_none(), "unchecking disables the wave");

    panel.set_wave_enabled(true);
    assert_eq!(
        panel.wave,
        Some(spec),
        "re-enabling restores the last amplitude, wavelength, phase and axes"
    );
}

#[test]
fn disabling_the_wave_restores_the_case_sampling() {
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Maxwell);
    state.refresh_maxwell_sampling();
    let baseline = state
        .maxwell_sampling
        .as_ref()
        .expect("the default case samples")
        .arrows
        .clone();

    assert!(state.set_maxwell_wave(Some(valid_wave())));
    state.refresh_maxwell_sampling();
    assert!(state.maxwell_sampling.is_none());
    assert!(state.maxwell_wave_sampling.is_some());

    assert!(state.set_maxwell_wave(None));
    state.refresh_maxwell_sampling();
    assert!(state.maxwell_wave_sampling.is_none());
    assert_eq!(
        state
            .maxwell_sampling
            .as_ref()
            .expect("case sampling")
            .arrows,
        baseline,
        "disabling the wave restores the exact case sampling"
    );
}

#[test]
fn wave_display_routes_surface_and_loop_probes_to_the_wave_field() {
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Maxwell);
    // The default electric canonical case is the hidden field. A probe routed
    // to the case would still resolve, so only a direct comparison with the
    // wave sample distinguishes the two displays.
    assert!(state.set_maxwell_wave(Some(valid_wave())));
    state.time = 0.37;
    state.set_probe_family(FieldFamily::Electric);

    let surface = GaussianSurface {
        kind: SurfaceKind::Sphere,
        center: Vec3::new(0.2, -0.1, 0.05),
        axis: Vec3::Z,
        size: Vec3::splat(1.4),
        segments: 24,
    };
    let loop_ = CirculationLoop {
        kind: LoopKind::Circle,
        center: Vec3::new(-0.15, 0.25, 0.0),
        axis: Vec3::Z,
        size: Vec3::splat(1.6),
        segments: 24,
    };
    state.add_probe_surface(surface).expect("valid surface");
    state.add_probe_loop(loop_).expect("valid loop");
    state.refresh_probe_reports(true);

    let time = f64::from(state.time);
    let wave = WaveProbeField {
        spec: valid_wave(),
        family: FieldFamily::Electric,
    };
    let expected_surface = flux(&wave, &surface, FieldFamily::Electric, time);
    let expected_loop = circulation(&wave, &loop_, FieldFamily::Electric, time);
    assert!(matches!(
        expected_surface.status,
        vector_field_explorer::field_probe::IntegralStatus::Estimate { .. }
    ));
    assert!(matches!(
        expected_loop.status,
        vector_field_explorer::field_probe::IntegralStatus::Estimate { .. }
    ));

    let actual_surface = state
        .probe_surface_report(0)
        .expect("the visible refresh built a surface report")
        .clone();
    let actual_loop = state
        .probe_loop_report(0)
        .expect("the visible refresh built a loop report")
        .clone();
    assert_eq!(actual_surface.family, FieldFamily::Electric);
    assert_eq!(actual_loop.family, FieldFamily::Electric);
    assert_eq!(
        actual_surface.status, expected_surface.status,
        "the flux must come from the displayed wave, not the hidden case"
    );
    assert_eq!(actual_surface.samples, expected_surface.samples);
    assert_eq!(
        actual_loop.status, expected_loop.status,
        "the circulation must come from the displayed wave, not the hidden case"
    );
    assert_eq!(actual_loop.samples, expected_loop.samples);

    // The magnetic family is displayed too and uses the same wave evaluator.
    state.set_probe_family(FieldFamily::Magnetic);
    state.refresh_probe_reports(true);
    let magnetic_wave = WaveProbeField {
        spec: valid_wave(),
        family: FieldFamily::Magnetic,
    };
    let expected_magnetic = flux(&magnetic_wave, &surface, FieldFamily::Magnetic, time);
    assert_eq!(
        state
            .probe_surface_report(0)
            .expect("a magnetic report exists")
            .status,
        expected_magnetic.status
    );
}

#[test]
fn wave_panel_never_promises_an_unsupported_animation_export() {
    assert!(
        !WAVE_SVG_NOTE.to_lowercase().contains("animations include"),
        "the capture note must not promise animation export"
    );
    assert!(
        WAVE_ANIMATION_NOTE.contains("unavailable"),
        "the wave panel names animation export as unavailable: {WAVE_ANIMATION_NOTE}"
    );

    let state = wave_state(true);
    let eligibility = AnimationEligibility::from_state(&state);
    assert!(
        !eligibility.allowed,
        "Maxwell animation export is disabled rather than promised"
    );
    assert!(eligibility.reason.is_some());
}

fn rgb(color: [u8; 3]) -> [f32; 3] {
    [
        color[0] as f32 / 255.0,
        color[1] as f32 / 255.0,
        color[2] as f32 / 255.0,
    ]
}

fn painted_texts(output: &egui::FullOutput) -> Vec<String> {
    fn collect(shape: &egui::Shape, out: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(text) => out.push(text.galley.text().to_owned()),
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect(shape, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut out);
    }
    out
}

fn context() -> egui::Context {
    let ctx = support::context();
    ctx.enable_accesskit();
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    ctx
}

fn panel_frame(
    ctx: &egui::Context,
    width: f32,
    height: f32,
    state: &mut State,
    panel: &mut MaxwellPanelState,
) -> egui::FullOutput {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, height),
            )),
            ..Default::default()
        },
        |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let _ = maxwell_panel::show(ui, panel, state, false);
            });
        },
    );
    output.textures_delta.clear();
    output
}
