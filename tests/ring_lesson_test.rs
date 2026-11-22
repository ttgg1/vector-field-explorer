//! Issue #224: the charged-ring axial-field activity.
//!
//! The lesson reuses the shipped `Uniform ring` source preset and its softened
//! evaluator. The ideal axis formula is a separate reference curve; the tests
//! pin its zero, odd symmetry and `|z| = R/sqrt(2)` extrema without grading the
//! softened evaluator's peak against it.

use eframe::egui;

use crate::support;
use vector_field_explorer::VfApp;
use vector_field_explorer::source_scene::FieldMode;
use vector_field_explorer::sources::SourceKind;
use vector_field_explorer::ui::griffiths::GriffithsRoute;
use vector_field_explorer::ui::ring_lesson::{
    RING_CAS_EXPRESSION, RING_HINT, RING_IDEAL_FORMULA, RING_PAIRING_NOTE, RING_PROMPT,
    RING_QUESTION, RING_SOFTENED_NOTE, RING_STRONGEST_Z, RING_UNITS_NOTE, RingLessonState,
    RingPrediction, graph_visibility, ideal_axial_field, probe_x_for_z, softened_axial_field,
};

/// The graph legend only names the ideal and softened curves; it must not be
/// visible before the reveal.
const CURVE_LEGEND: &str = "ideal — · softened ----";
struct Harness {
    ctx: egui::Context,
    app: VfApp,
    width: f32,
    height: f32,
}

impl Harness {
    fn new(width: f32, height: f32) -> Self {
        let ctx = support::context();
        ctx.enable_accesskit();
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let mut app = VfApp::new();
        app.layout.help_visible = false;
        Self {
            ctx,
            app,
            width,
            height,
        }
    }

    fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        let mut frame = eframe::Frame::_new_kittest();
        let mut out = self.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(self.width, self.height),
                )),
                events,
                ..Default::default()
            },
            |ui| eframe::App::ui(&mut self.app, ui, &mut frame),
        );
        out.textures_delta.clear();
        out
    }

    fn idle(&mut self) -> egui::FullOutput {
        for _ in 0..3 {
            self.frame(Vec::new());
        }
        self.frame(Vec::new())
    }
}

fn texts(out: &egui::FullOutput) -> Vec<String> {
    out.shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::Text(shape) => Some(shape.galley.text().to_owned()),
            _ => None,
        })
        .collect()
}

#[test]
fn the_ideal_curve_is_zero_odd_and_peaks_at_r_over_root_two() {
    assert!(ideal_axial_field(0.0).abs() < 1e-15);
    for z in [0.1, 0.4, 0.7, 1.0, 2.0] {
        assert!(
            (ideal_axial_field(-z) + ideal_axial_field(z)).abs() < 1e-15,
            "the ideal axis field is odd at z = {z}"
        );
    }
    let peak = ideal_axial_field(RING_STRONGEST_Z).abs();
    for offset in [-0.2, -0.05, 0.05, 0.2] {
        let z = RING_STRONGEST_Z + offset;
        assert!(
            ideal_axial_field(z).abs() < peak,
            "the ideal magnitude peaks at |z| = R/sqrt(2), not at {z}"
        );
    }
    assert!((RING_STRONGEST_Z - 1.0 / 2.0_f64.sqrt()).abs() < 1e-15);
}

#[test]
fn the_softened_evaluator_is_a_distinct_curve_and_zero_at_the_centre() {
    let centre = softened_axial_field(0.0).expect("the shipped ring evaluates at the centre");
    assert!(
        centre.abs() < 1e-15,
        "the softened axis sample is zero at the centre: {centre}"
    );
    let z = RING_STRONGEST_Z;
    let softened = softened_axial_field(z).expect("the shipped ring evaluates off-centre");
    let ideal = ideal_axial_field(z);
    assert!(
        (softened - ideal).abs() > 1e-6,
        "softening makes the evaluator a visibly different curve: {softened} vs {ideal}"
    );
    assert!(
        RING_SOFTENED_NOTE.contains("not graded"),
        "the UI states that the softened curve is not graded against the ideal peak"
    );
}

#[test]
fn a_fresh_lesson_shows_the_prompt_before_any_result() {
    let mut lesson = RingLessonState::default();
    lesson.activate();
    assert!(!lesson.revealed());
    assert!(!lesson.can_reveal());
    assert!(!lesson.reveal(), "no result reveal before a prediction");
    assert_eq!(lesson.probe_z(), 1.0);
    assert!(RING_PROMPT.contains("Predict first"));
    assert!(RING_QUESTION.contains("Where along its axis"));
    assert!(RING_HINT.contains('R') && RING_HINT.contains('z'));
}

#[test]
fn the_side_view_maps_the_probe_about_the_ring_axis() {
    let rect = egui::Rect::from_min_size(egui::Pos2::new(10.0, 20.0), egui::vec2(400.0, 154.0));
    let centre = rect.center().x;
    assert!(
        (probe_x_for_z(rect, 0.0) - centre).abs() < 1e-4,
        "z = 0 sits on the ring axis at the centre"
    );
    assert!(
        (probe_x_for_z(rect, -2.0) - (rect.left() + 14.0)).abs() < 1e-4,
        "z = -2 is the left end of the probe range"
    );
    let right = probe_x_for_z(rect, 2.0);
    assert!(
        (right - (rect.right() - 14.0)).abs() < 1e-4,
        "z = +2 is the right end of the probe range"
    );
    assert!(
        right < rect.right(),
        "the +z probe marker stays on-canvas at {right} inside {rect:?}"
    );
    assert!(
        probe_x_for_z(rect, -0.5) < centre && probe_x_for_z(rect, 0.5) > centre,
        "the signed probe is left of centre for negative z and right for positive z"
    );
}

#[test]
fn the_curves_and_legend_are_revealed_only_with_the_result() {
    let before = graph_visibility(false);
    assert!(
        !before.curves && !before.legend,
        "the ideal/softened curves and their legend stay hidden before the reveal"
    );
    let after = graph_visibility(true);
    assert!(
        after.curves && after.legend,
        "the reveal draws the curves with their legend"
    );
}

#[test]
fn the_axial_graph_states_k_is_one_in_normalized_units() {
    assert!(
        RING_UNITS_NOTE.contains("k = 1") && RING_UNITS_NOTE.contains("normalized teaching units"),
        "the one-line note explains the missing k: {RING_UNITS_NOTE}"
    );
}

#[test]
fn the_probe_updates_the_signed_component_and_marker_value() {
    let mut lesson = RingLessonState::default();
    lesson.activate();
    lesson.set_probe_z(0.8);
    let positive = lesson.reading().clone();
    lesson.set_probe_z(-0.8);
    let negative = lesson.reading().clone();
    assert!(positive.ideal_ez > 0.0 && negative.ideal_ez < 0.0);
    assert!(
        (positive.ideal_ez + negative.ideal_ez).abs() < 1e-15,
        "the signed ideal component is odd"
    );
    assert_eq!(positive.z, 0.8);
    assert_eq!(negative.z, -0.8);
}

#[test]
fn predicting_or_skipping_unlocks_the_reveal_and_revision_is_allowed() {
    let mut lesson = RingLessonState::default();
    lesson.activate();
    lesson.select_prediction(RingPrediction::PartwayOut);
    assert!(lesson.can_reveal());
    lesson.select_prediction(RingPrediction::Centre);
    assert_eq!(lesson.prediction(), Some(RingPrediction::Centre));
    assert!(lesson.reveal());

    let mut skipped = RingLessonState::default();
    skipped.activate();
    skipped.skip_prediction();
    assert!(skipped.can_reveal());
    assert!(skipped.reveal());
}

#[test]
fn the_ring_route_loads_the_shipped_preset_and_restores_the_starting_scene() {
    let mut h = Harness::new(1200.0, 800.0);
    h.idle();
    let start_document = h.app.capture_document_for_test();
    let start_orbit = h.app.orbit;
    let start_camera = (h.app.camera.position, h.app.camera.scale);

    h.app.open_griffiths_branch();
    h.app.start_griffiths_route(GriffithsRoute::Ring);
    h.idle();
    assert!(h.app.ring_lesson_active_for_test());
    assert_eq!(h.app.state.field_mode, FieldMode::Sources);
    assert_eq!(h.app.state.sources.entries.len(), 1);
    assert!(
        matches!(
            h.app.state.sources.entries[0].source.kind,
            SourceKind::ChargedRing { radius, .. } if (radius - 1.0).abs() < 1e-12
        ),
        "the shipped Uniform ring preset supplies the model"
    );

    let out = h.frame(Vec::new());
    let text = texts(&out).join("\n");
    assert!(
        text.contains(RING_QUESTION),
        "the question is in view: {text}"
    );
    assert!(
        text.contains(RING_PROMPT),
        "the prompt precedes the reveal: {text}"
    );
    assert!(
        !text.contains("Axial field result"),
        "no result block before the prediction: {text}"
    );
    assert!(
        !text.contains(CURVE_LEGEND),
        "the curve legend stays hidden before the reveal: {text}"
    );

    h.app.set_ring_probe_for_test(1.0);
    h.app.predict_ring_for_test(RingPrediction::PartwayOut);
    h.app.reveal_ring_for_test();
    h.idle();
    let out = h.frame(Vec::new());
    let text = texts(&out).join("\n");
    assert!(
        text.contains("Axial field result"),
        "the reveal arrives after the prediction: {text}"
    );
    assert!(text.contains(RING_IDEAL_FORMULA));
    assert!(text.contains(RING_PAIRING_NOTE));
    assert!(text.contains(RING_SOFTENED_NOTE));
    assert!(
        text.contains(CURVE_LEGEND),
        "the reveal draws the curve legend: {text}"
    );
    assert!(
        text.contains(RING_UNITS_NOTE),
        "the reveal states the normalized-unit convention for k: {text}"
    );

    h.app.exit_griffiths_lesson();
    assert!(!h.app.ring_lesson_active_for_test());
    assert_eq!(
        h.app.capture_document_for_test().view,
        start_document.view,
        "Back/Exit restores the starting scene"
    );
    assert_eq!(h.app.orbit, start_orbit, "Back/Exit restores the camera");
    assert_eq!((h.app.camera.position, h.app.camera.scale), start_camera);
}

#[test]
fn keep_exploring_adopts_the_ring_scene() {
    let mut h = Harness::new(1200.0, 800.0);
    h.idle();
    h.app.open_griffiths_branch();
    h.app.start_griffiths_route(GriffithsRoute::Ring);
    h.app.keep_griffiths_scene();
    assert!(!h.app.ring_lesson_active_for_test());
    assert!(h.app.griffiths_start_snapshot_is_none_for_test());
    assert_eq!(h.app.state.field_mode, FieldMode::Sources);
    assert!(
        matches!(
            h.app.state.sources.entries[0].source.kind,
            SourceKind::ChargedRing { .. }
        ),
        "Keep exploring keeps the ring scene instead of restoring"
    );
}

#[test]
fn the_reveal_reports_signed_values_and_the_softened_separately() {
    let mut lesson = RingLessonState::default();
    lesson.activate();
    lesson.set_probe_z(-0.7);
    lesson.select_prediction(RingPrediction::PartwayOut);
    lesson.reveal();
    let reading = lesson.reading();
    assert!(reading.ideal_ez < 0.0);
    assert!(reading.softened_ez.is_some());
    assert!(reading.ideal_ez.abs() > 0.0);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn the_axis_formula_can_reach_the_cas_input_with_q_and_r_symbolic() {
    let mut h = Harness::new(1200.0, 800.0);
    h.idle();
    h.app.open_griffiths_branch();
    h.app.start_griffiths_route(GriffithsRoute::Ring);
    h.app.send_ring_formula_to_cas_for_test();
    assert_eq!(h.app.cas.input, RING_CAS_EXPRESSION);
    assert!(RING_CAS_EXPRESSION.contains('Q') && RING_CAS_EXPRESSION.contains('R'));
}
