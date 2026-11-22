//! Issue #220: compare the induced circulation with the changing magnetic flux.
//!
//! The pure tests pin the sign and zero contracts on the shipped induction
//! model; the panel tests pin the orientation/method labels and the
//! session-only behaviour.

use eframe::egui;
use glam::{DVec3, Vec3};

use vector_field_explorer::field_probe::{
    CirculationLoop, IntegralStatus, LoopKind, ProbeField, open_surface_flux,
};
use vector_field_explorer::induction::{
    InductionCase, InductionScene, induction_flux_comparison, loop_orientation_label,
};
use vector_field_explorer::sources::FieldFamily;
use vector_field_explorer::ui::induction_panel::InductionPanelState;

const SOFTENING: f64 = 0.1;

fn scene() -> InductionScene {
    InductionScene {
        case: InductionCase::CurrentLoop,
        center: DVec3::ZERO,
        axis: DVec3::Z,
        radius: 1.0,
        length: 0.0,
        turns: 1,
        amplitude: 1.0,
        frequency: 0.25,
        ..InductionScene::default()
    }
}

fn circle(radius: f32, segments: u32) -> CirculationLoop {
    CirculationLoop {
        kind: LoopKind::Circle,
        center: Vec3::ZERO,
        axis: Vec3::Z,
        size: Vec3::splat(radius * 2.0),
        segments,
    }
}

fn ratio(estimate: (f64, f64)) -> f64 {
    let (value, error) = estimate;
    if value == 0.0 {
        0.0
    } else {
        error / value.abs()
    }
}

#[test]
fn static_current_has_exactly_zero_circulation_and_flux_derivative() {
    let mut scene = scene();
    scene.frequency = 0.0;
    assert_eq!(scene.current_derivative(1.0), 0.0);

    let comparison = induction_flux_comparison(&scene, &circle(0.5, 16), 1.0, SOFTENING);
    assert!(comparison.unavailable.is_none(), "{comparison:?}");
    let circulation = comparison
        .circulation_estimate()
        .expect("a static current still resolves the circulation integral");
    let flux_rate = comparison
        .flux_derivative_estimate()
        .expect("a static current still resolves the flux derivative");
    assert_eq!(circulation.0, 0.0, "static circulation is exactly zero");
    assert_eq!(flux_rate.0, 0.0, "static flux derivative is exactly zero");
    assert_eq!(circulation.1, 0.0, "zero circulation has zero spread");
    assert_eq!(flux_rate.1, 0.0, "zero flux derivative has zero spread");
    assert!(comparison.is_confident(), "{comparison:?}");
}

#[test]
fn sinusoidal_derivative_phase_has_a_positive_signed_pair() {
    let scene = scene();
    // cos(pi/2) is zero and sin(pi/2) is one: the current is at a zero crossing
    // and its derivative is at its negative extremum, so the magnetic field is
    // zero while both Faraday readouts are maximal.
    let t = 1.0_f32;
    assert!(scene.current(t).abs() < 1e-6, "I(t) = {}", scene.current(t));
    assert!(scene.current_derivative(t) < -1.5);

    let comparison = induction_flux_comparison(&scene, &circle(0.5, 16), t, SOFTENING);
    assert!(comparison.unavailable.is_none(), "{comparison:?}");
    let circulation = comparison
        .circulation_estimate()
        .expect("the circulation must resolve");
    let flux_rate = comparison
        .flux_derivative_estimate()
        .expect("the flux derivative must resolve");
    assert!(circulation.0 > 0.0, "contour around +z: {circulation:?}");
    assert!(flux_rate.0 > 0.0, "normal +z: {flux_rate:?}");
    assert!(
        ratio(circulation) < 0.5,
        "the circulation must be resolved, not noise: {circulation:?}"
    );
    assert!(
        ratio(flux_rate) < 0.5,
        "the flux derivative must be resolved, not noise: {flux_rate:?}"
    );
    // Faraday's law in the model's teaching units: both independent sides
    // agree well inside the combined reported spread plus one percent.
    let combined = circulation.1 + flux_rate.1 + 0.01 * flux_rate.0.abs();
    assert!(
        (circulation.0 - flux_rate.0).abs() <= combined,
        "circulation {:?} vs flux derivative {:?}",
        circulation,
        flux_rate
    );
    assert!(comparison.is_confident(), "{comparison:?}");
}

#[test]
fn reversing_the_loop_and_normal_negates_both_readouts() {
    let scene = scene();
    let t = 0.25_f32;
    let forward_loop = circle(0.5, 16);
    let reverse_loop = CirculationLoop {
        axis: -forward_loop.axis,
        ..forward_loop
    };
    let forward = induction_flux_comparison(&scene, &forward_loop, t, SOFTENING);
    let reverse = induction_flux_comparison(&scene, &reverse_loop, t, SOFTENING);

    let forward_circulation = forward.circulation_estimate().expect("forward circulation");
    let reverse_circulation = reverse.circulation_estimate().expect("reverse circulation");
    let forward_flux_rate = forward.flux_derivative_estimate().expect("forward rate");
    let reverse_flux_rate = reverse.flux_derivative_estimate().expect("reverse rate");

    let tolerance = forward_circulation.1 + reverse_circulation.1 + 1e-12;
    assert!(
        (forward_circulation.0 + reverse_circulation.0).abs() <= tolerance,
        "circulation signs: {forward_circulation:?} vs {reverse_circulation:?}"
    );
    let tolerance = forward_flux_rate.1 + reverse_flux_rate.1 + 1e-12;
    assert!(
        (forward_flux_rate.0 + reverse_flux_rate.0).abs() <= tolerance,
        "flux-rate signs: {forward_flux_rate:?} vs {reverse_flux_rate:?}"
    );
    assert!(
        (forward_circulation.1 - reverse_circulation.1).abs()
            <= 1e-9 * forward_circulation.0.abs().max(1.0),
        "circulation magnitudes keep the same spread"
    );
    assert!(
        (forward_flux_rate.1 - reverse_flux_rate.1).abs()
            <= 1e-9 * forward_flux_rate.0.abs().max(1.0),
        "flux-rate magnitudes keep the same spread"
    );
    assert_eq!(forward.normal, DVec3::Z);
    assert_eq!(reverse.normal, DVec3::NEG_Z);
}

#[test]
fn orientation_label_names_the_normal_and_the_right_hand_rule() {
    let label = loop_orientation_label(&circle(0.5, 16));
    assert!(label.contains("Normal"), "{label}");
    assert!(label.contains("right-hand rule"), "{label}");
    assert!(label.contains("counter-clockwise"), "{label}");
    let reversed = loop_orientation_label(&CirculationLoop {
        axis: Vec3::NEG_Z,
        ..circle(0.5, 16)
    });
    assert!(reversed.contains("-1.000"), "{reversed}");
}

#[test]
fn invalid_geometry_reports_an_explicit_unavailable_state() {
    let scene = scene();
    let invalid = CirculationLoop {
        segments: 4,
        ..circle(0.5, 16)
    };
    let comparison = induction_flux_comparison(&scene, &invalid, 0.0, SOFTENING);
    assert!(
        comparison.unavailable.is_some(),
        "invalid geometry must be named, not valued: {comparison:?}"
    );
    assert!(comparison.circulation_estimate().is_none());
    assert!(comparison.flux_estimate().is_none());
    assert!(comparison.flux_derivative_estimate().is_none());
    let label = comparison.status_label();
    assert!(
        label.to_lowercase().contains("unavailable"),
        "the status is explicit: {label}"
    );
}

#[test]
fn invalid_softening_reports_an_explicit_unavailable_state() {
    let scene = scene();
    let comparison = induction_flux_comparison(&scene, &circle(0.5, 16), 0.0, 0.0);
    assert!(comparison.unavailable.is_some(), "{comparison:?}");
    assert!(comparison.circulation_estimate().is_none());
    assert!(comparison.flux_estimate().is_none());
    assert!(comparison.flux_derivative_estimate().is_none());
}

struct ConstantField(DVec3);

impl ProbeField for ConstantField {
    fn eval(&self, _point: Vec3, _time: f64) -> Option<Vec3> {
        Some(self.0.as_vec3())
    }
}

#[test]
fn open_surface_flux_uses_the_loop_normal_and_area() {
    let field = ConstantField(DVec3::new(0.0, 0.0, 2.0));
    let loop_ = circle(0.5, 16);
    let report = open_surface_flux(&field, &loop_, FieldFamily::Magnetic, 0.0);
    let IntegralStatus::Estimate { value, error } = report.status else {
        panic!("constant field must resolve: {report:?}");
    };
    let expected = 2.0 * std::f64::consts::PI * 0.25;
    assert!(
        (value - expected).abs() <= error + 1e-12,
        "{value} vs {expected}"
    );

    let reversed = CirculationLoop {
        axis: -loop_.axis,
        ..loop_
    };
    let report = open_surface_flux(&field, &reversed, FieldFamily::Magnetic, 0.0);
    let IntegralStatus::Estimate { value, .. } = report.status else {
        panic!("constant field must resolve: {report:?}");
    };
    assert!((value + expected).abs() <= 1e-12, "{value}");
}

fn widget_labels(output: &eframe::egui::FullOutput) -> Vec<String> {
    let Some(update) = output.platform_output.accesskit_update.as_ref() else {
        return Vec::new();
    };
    update
        .nodes
        .iter()
        .filter_map(|(_, node)| {
            node.label()
                .or_else(|| node.value())
                .map(|text| text.to_owned())
        })
        .collect()
}

fn has_label(labels: &[String], needle: &str) -> bool {
    labels.iter().any(|label| label.contains(needle))
}

fn panel_frame(
    width: f32,
    panel: &mut InductionPanelState,
    state: &mut vector_field_explorer::viewmodel::State,
) -> Vec<String> {
    let ctx = crate::support::context();
    ctx.enable_accesskit();
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, 900.0),
            )),
            ..Default::default()
        },
        |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let _ = vector_field_explorer::ui::induction_panel::show(
                    ui,
                    panel,
                    state,
                    width < 600.0,
                );
            });
        },
    );
    output.textures_delta.clear();
    widget_labels(&output)
}

#[test]
fn comparison_panel_names_orientation_method_resolution_and_uncertainty() {
    let mut state = vector_field_explorer::viewmodel::State::new(true);
    state.set_field_mode(vector_field_explorer::source_scene::FieldMode::Induction);
    state.add_probe_loop(circle(0.5, 16)).expect("a valid loop");
    let mut panel = InductionPanelState::default();
    panel.faraday_open = true;

    let labels = panel_frame(1200.0, &mut panel, &mut state);
    for needle in [
        "Loop 1",
        "Reverse contour and normal (session only)",
        "Normal (right-hand rule)",
        "∮E·dl",
        "−dΦB/dt",
        "Method: midpoint segments",
        "full/half-resolution difference",
        "Normalized teaching units",
        "One period on the shared time axis:",
        "At t = 0.000",
    ] {
        assert!(
            has_label(&labels, needle),
            "the comparison is missing {needle}: {labels:?}"
        );
    }

    // The same entry path remains usable at narrow widths with the existing
    // induction controls.
    let labels = panel_frame(390.0, &mut panel, &mut state);
    assert!(has_label(&labels, "Loop 1"), "{labels:?}");
    assert!(has_label(&labels, "Fit"), "{labels:?}");
    assert!(has_label(&labels, "Normal (right-hand rule)"), "{labels:?}");
    assert!(has_label(&labels, "(0.000, 0.000, 1.000)"), "{labels:?}");

    // The session-only reverse flips the displayed normal, not the loop.
    panel.faraday_reverse = true;
    let labels = panel_frame(390.0, &mut panel, &mut state);
    assert!(has_label(&labels, "(0.000, 0.000, -1.000)"), "{labels:?}");
    assert_eq!(state.probe_loops()[0].axis, Vec3::Z);
}

#[test]
fn comparison_is_session_only_and_never_mutates_durable_probes() {
    let mut state = vector_field_explorer::viewmodel::State::new(true);
    state.set_field_mode(vector_field_explorer::source_scene::FieldMode::Induction);
    state.add_probe_loop(circle(0.5, 16)).expect("a valid loop");
    let loops_before = state.probe_loops().to_vec();
    let scene_before = state.induction.clone();
    state.dirty = false;

    let mut panel = InductionPanelState::default();
    panel.faraday_open = true;
    let _ = panel_frame(1200.0, &mut panel, &mut state);
    panel.faraday_reverse = true;
    let _ = panel_frame(1200.0, &mut panel, &mut state);
    panel.faraday_reverse = false;
    panel.faraday_open = false;
    let labels = panel_frame(1200.0, &mut panel, &mut state);

    assert_eq!(state.probe_loops(), &loops_before[..]);
    assert_eq!(state.induction, scene_before);
    assert!(
        !state.dirty,
        "opening the comparison is not a document edit"
    );
    assert!(!panel.faraday_open);
    assert!(
        !has_label(&labels, "One period on the shared time axis:"),
        "the collapsed section does not sample or paint its table"
    );
}
