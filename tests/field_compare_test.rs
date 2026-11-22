//! Issue #215: candidate field beside the active model.
//!
//! The comparison is sampled at explicit points. It is not a symbolic identity
//! check, and an unavailable side is never rendered as a fabricated zero.

use eframe::egui;
use glam::Vec3;

use crate::support;

use vector_field_explorer::domain::Domain;
use vector_field_explorer::field::FieldExpr;
use vector_field_explorer::field_compare::{
    ComparisonReport, DirectionRelation, SideSample, compare_point, compare_samples,
    compare_samples_in_frame,
};
use vector_field_explorer::parameters::{Parameter, ParameterSet};
use vector_field_explorer::plane::{PlaneAxis, PlaneFrame, PlaneView};
use vector_field_explorer::source_scene::FieldMode;
use vector_field_explorer::ui::field_compare::{
    FieldCompareControls, Prediction, show_body, visible,
};
use vector_field_explorer::viewmodel::State;

fn sampled(model: &str, candidate: &str, first: Vec3, second: Vec3) -> ComparisonReport {
    let model = FieldExpr::parse(model).expect("model parses");
    let candidate = FieldExpr::parse(candidate).expect("candidate parses");
    compare_samples(&model, &candidate, first, second, 0.0, &Domain::default())
}

fn available(side: &SideSample) -> Vec3 {
    match side {
        SideSample::Available(vector) => *vector,
        SideSample::Unavailable(reason) => panic!("side is unavailable: {reason}"),
    }
}

fn assert_vector_close(actual: Vec3, expected: Vec3) {
    assert!(
        (actual - expected).length() < 1e-5,
        "expected {expected:?}, got {actual:?}"
    );
}

#[test]
fn a_matching_candidate_agrees_at_regular_sampled_points() {
    let report = sampled(
        "(x, y)",
        "(x, y)",
        Vec3::new(0.25, -0.5, 0.0),
        Vec3::new(0.5, -1.0, 0.0),
    );

    for point in [&report.first, &report.second] {
        assert_eq!(point.relation, DirectionRelation::With);
        assert_vector_close(available(&point.model), available(&point.candidate));
        assert_eq!(point.relative_difference, Some(0.0));
    }
    assert!(
        report.note.contains("sampled"),
        "the note says the comparison is sampled: {}",
        report.note
    );
    assert!(
        report.note.contains("not a symbolic identity"),
        "no identity is claimed: {}",
        report.note
    );
}

#[test]
fn a_plane_comparison_lifts_local_points_to_world_before_sampling() {
    let domain = Domain::default();
    // An X-normal slice through the domain centre: local u is world -z, local
    // v is world y, and local w is the world x offset from the chosen plane.
    let frame = PlaneFrame::new(
        &domain,
        PlaneView {
            enabled: true,
            axis: PlaneAxis::X,
            index: 8,
        },
        glam::UVec3::new(17, 17, 17),
    );
    let local_first = Vec3::new(0.5, -0.25, 0.0);
    let local_second = Vec3::new(-0.75, 0.5, 0.0);
    let model = FieldExpr::parse("(x, y, z)").expect("model parses");
    let candidate = model.clone();

    let report = compare_samples_in_frame(
        &model,
        &candidate,
        local_first,
        local_second,
        0.0,
        &domain,
        &domain,
        Some(&frame),
    );

    for (reading, local) in [(&report.first, local_first), (&report.second, local_second)] {
        let world = frame.to_world(local);
        assert_vector_close(reading.point, world);
        assert_vector_close(available(&reading.model), world);
        assert_vector_close(available(&reading.candidate), world);
    }
    assert!(
        report.note.contains("world"),
        "the note distinguishes world sampling from slice-local inputs: {}",
        report.note
    );
}

#[test]
fn reversing_the_model_sign_exposes_the_opposite_reading() {
    let model = FieldExpr::parse("(x, y)").expect("model");
    let candidate = FieldExpr::parse("(-x, -y)").expect("candidate");
    // A non-zero regular point, so both directions are defined.
    let point = Vec3::new(0.5, 0.25, 0.0);
    let reading = compare_point(&model, &candidate, point, 0.0, &Domain::default());

    assert_eq!(reading.relation, DirectionRelation::Against);
    let model_vector = available(&reading.model);
    let candidate_vector = available(&reading.candidate);
    assert_vector_close(candidate_vector, -model_vector);
    assert_vector_close(
        reading
            .difference
            .expect("both sides are available, so candidate - model exists"),
        -2.0 * model_vector,
    );
    assert!(
        reading.relative_difference.expect("the model is non-zero") > 1.9,
        "a sign reversal differs by twice the model magnitude"
    );
}

#[test]
fn a_reversed_model_sign_also_reads_as_against() {
    let model = FieldExpr::parse("(-x, -y)").expect("model");
    let candidate = FieldExpr::parse("(x, y)").expect("candidate");
    let point = Vec3::new(0.5, 0.25, 0.0);
    let reading = compare_point(&model, &candidate, point, 0.0, &Domain::default());

    assert_eq!(reading.relation, DirectionRelation::Against);
    let model_vector = available(&reading.model);
    let candidate_vector = available(&reading.candidate);
    assert_vector_close(candidate_vector, -model_vector);
    assert!(
        reading.relative_difference.expect("the model is non-zero") > 1.9,
        "a reversed model differs by twice its magnitude"
    );
}

#[test]
fn a_direction_disagreement_between_points_is_named() {
    let report = sampled(
        "(x, y)",
        "(x*y, y*y)",
        Vec3::new(0.5, 0.5, 0.0),
        Vec3::new(0.5, -0.5, 0.0),
    );

    assert_eq!(report.first.relation, DirectionRelation::With);
    assert_eq!(report.second.relation, DirectionRelation::Against);
    assert!(
        report.note.contains("disagree in direction"),
        "the note names the disagreement: {}",
        report.note
    );
}

#[test]
fn a_second_sample_separates_a_sign_error_from_a_power_law_error() {
    // r and 2r along the same direction.
    let first = Vec3::new(0.5, 0.0, 0.0);
    let second = Vec3::new(1.0, 0.0, 0.0);

    let sign = sampled("(x, y)", "(-x, -y)", first, second);
    assert_eq!(sign.first.relation, DirectionRelation::Against);
    assert_eq!(sign.second.relation, DirectionRelation::Against);
    assert!(
        sign.note.contains("sign"),
        "an opposite candidate is read as a sign error: {}",
        sign.note
    );

    let power = sampled("(x, y)", "(x/(x*x+y*y), y/(x*x+y*y))", first, second);
    assert_eq!(power.first.relation, DirectionRelation::With);
    assert_eq!(power.second.relation, DirectionRelation::With);
    let first_relative = power
        .first
        .relative_difference
        .expect("the model is non-zero at r");
    let second_relative = power
        .second
        .relative_difference
        .expect("the model is non-zero at 2r");
    assert!(
        (first_relative - second_relative).abs() > 0.1,
        "a power-law error changes relative difference between r and 2r: \
         {first_relative} vs {second_relative}"
    );
    assert!(
        power.note.contains("power-law"),
        "the second sample identifies the changing magnitude error: {}",
        power.note
    );
}

#[test]
fn an_unavailable_side_is_named_and_never_fabricated_as_zero() {
    let model = FieldExpr::parse("(1, 0)").expect("model");
    let candidate = FieldExpr::parse("(1/x, 0)").expect("candidate");
    let reading = compare_point(
        &model,
        &candidate,
        Vec3::new(0.0, 0.0, 0.0),
        0.0,
        &Domain::default(),
    );

    assert!(reading.model.value().is_some(), "the model evaluates here");
    let reason = reading
        .candidate
        .unavailable_reason()
        .expect("the singular candidate is unavailable");
    assert!(
        reason.contains("could not be evaluated") || reason.contains("non-finite"),
        "the candidate side names its failure: {reason}"
    );
    assert!(
        reading.difference.is_none(),
        "no difference is fabricated from a missing side"
    );
    assert!(
        reading.relative_difference.is_none(),
        "no relative difference is fabricated from a missing side"
    );
}

#[test]
fn outside_the_visible_domain_both_sides_are_unavailable() {
    let model = FieldExpr::parse("(x, y)").expect("model");
    let candidate = FieldExpr::parse("(x, y)").expect("candidate");
    let reading = compare_point(
        &model,
        &candidate,
        Vec3::new(2.0, 0.0, 0.0),
        0.0,
        &Domain::default(),
    );

    assert!(matches!(reading.model, SideSample::Unavailable(_)));
    assert!(matches!(reading.candidate, SideSample::Unavailable(_)));
    assert!(
        reading
            .model
            .unavailable_reason()
            .expect("reason")
            .contains("outside"),
        "the reason names the visible domain"
    );
    assert!(reading.relative_difference.is_none());
}

#[test]
fn relative_difference_is_undefined_at_zero_model_magnitude() {
    let model = FieldExpr::parse("(0, 0)").expect("model");
    let candidate = FieldExpr::parse("(1, 0)").expect("candidate");
    let reading = compare_point(
        &model,
        &candidate,
        Vec3::new(0.5, 0.5, 0.0),
        0.0,
        &Domain::default(),
    );

    assert_eq!(reading.relation, DirectionRelation::Undefined);
    assert_eq!(reading.model_magnitude, Some(0.0));
    assert!(
        reading.relative_difference.is_none(),
        "dividing by a zero model magnitude is undefined"
    );
    let note = reading
        .relative_difference_note
        .expect("the undefined relative difference is explained");
    assert!(note.contains("undefined"), "{note}");
    assert!(note.contains("zero"), "{note}");
}

#[test]
fn a_two_times_candidate_reports_a_defined_relative_difference() {
    let report = sampled(
        "(x, y)",
        "(2*x, 2*y)",
        Vec3::new(0.5, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
    );
    assert_eq!(report.first.relation, DirectionRelation::With);
    assert_eq!(report.first.relative_difference, Some(1.0));
    assert_eq!(report.second.relative_difference, Some(1.0));
    assert!(
        report.note.contains("uniform scale"),
        "an unchanged relative difference is a scale error, not a power law: {}",
        report.note
    );
}

#[test]
fn a_named_parameter_binds_in_the_candidate_and_a_change_marks_it_stale() {
    let mut parameters = ParameterSet {
        version: 1,
        entries: vec![Parameter {
            name: "d".to_owned(),
            value: 2.0,
            min: None,
            max: None,
            step: 1.0,
        }],
    };
    let mut controls = FieldCompareControls::default();
    controls.candidate_text = "(d*x, d*y)".to_owned();
    assert!(
        controls.apply(&parameters),
        "the named parameter binds: {}",
        controls.error
    );
    let point = Vec3::new(1.0, 0.0, 0.0);
    assert_eq!(
        controls
            .candidate
            .as_ref()
            .expect("candidate applied")
            .eval3_at(point, 0.0),
        Some(Vec3::new(2.0, 0.0, 0.0)),
        "the candidate evaluates with the committed parameter value"
    );
    assert!(!controls.parameters_stale(&parameters));

    parameters.entries[0].value = 3.0;
    assert!(
        controls.parameters_stale(&parameters),
        "a changed parameter marks the applied candidate stale"
    );
    assert!(controls.apply(&parameters));
    assert!(!controls.parameters_stale(&parameters));
    assert_eq!(
        controls
            .candidate
            .as_ref()
            .expect("candidate reapplied")
            .eval3_at(point, 0.0),
        Some(Vec3::new(3.0, 0.0, 0.0)),
        "reapplying refreshes the evaluation"
    );
}

#[test]
fn the_section_is_formula_and_vector_only() {
    assert!(visible(FieldMode::Formula, true));
    assert!(!visible(FieldMode::Formula, false));
    for mode in [
        FieldMode::Sources,
        FieldMode::Induction,
        FieldMode::Materials,
        FieldMode::Maxwell,
        FieldMode::Ode,
    ] {
        assert!(!visible(mode, true), "{mode:?} has no formula candidate");
    }
}

#[test]
fn applying_a_candidate_arms_the_prediction_gate() {
    let mut controls = FieldCompareControls::default();
    assert!(!controls.revealed(), "the comparison starts hidden");

    controls.candidate_text = "(x, y)".to_owned();
    assert!(controls.apply(&ParameterSet::default()));
    assert!(controls.candidate.is_some());
    assert!(!controls.revealed(), "applying alone does not reveal");

    controls.predict(Prediction::With);
    assert!(controls.revealed(), "a prediction reveals the comparison");
    assert_eq!(controls.prediction, Some(Prediction::With));
}

#[test]
fn skip_is_an_explicit_way_past_the_prediction_gate() {
    let mut controls = FieldCompareControls::default();
    controls.candidate_text = "(x, y)".to_owned();
    controls.apply(&ParameterSet::default());

    controls.skip();
    assert!(controls.revealed());
    assert!(controls.skipped);
    assert_eq!(controls.prediction, None);
}

#[test]
fn a_parse_error_keeps_the_previous_candidate_and_names_the_failure() {
    let mut controls = FieldCompareControls::default();
    controls.candidate_text = "(x, y)".to_owned();
    controls.apply(&ParameterSet::default());
    let applied = controls
        .candidate
        .as_ref()
        .expect("the valid candidate applied")
        .clone();

    controls.candidate_text = "(x, sin(".to_owned();
    assert!(!controls.apply(&ParameterSet::default()));
    assert!(
        controls.error.contains("parse error"),
        "the inline error names the parse failure: {}",
        controls.error
    );
    assert!(
        controls.candidate.is_some(),
        "invalid input never discards the last valid candidate"
    );
    let still = controls
        .candidate
        .as_ref()
        .expect("the candidate remains parsed");
    assert_eq!(still.dimension, applied.dimension);
    assert_eq!(
        still.eval3_at(Vec3::new(0.25, 0.5, 0.0), 0.0),
        applied.eval3_at(Vec3::new(0.25, 0.5, 0.0), 0.0)
    );
}

#[test]
fn applying_an_invalid_candidate_never_touches_the_model_or_scene() {
    let mut state = State::new(false);
    let formula = state.formula.clone();
    let parameters = state.parameters.clone();
    let domain = state.domain;
    let before = state
        .field
        .eval3_at(Vec3::new(0.25, 0.5, 0.0), 0.0)
        .expect("the model evaluates");
    state.dirty = false;

    let mut controls = FieldCompareControls::default();
    controls.candidate_text = "(x, sin(".to_owned();
    controls.apply(&state.parameters);

    assert_eq!(state.formula, formula, "the model formula is authoritative");
    assert_eq!(state.parameters, parameters);
    assert_eq!(state.domain, domain);
    assert_eq!(
        state.field.eval3_at(Vec3::new(0.25, 0.5, 0.0), 0.0),
        Some(before),
        "the model field is unchanged"
    );
    assert!(!state.dirty, "candidate input never dirties the scene");
}

#[test]
fn the_comparison_body_renders_the_gate_before_the_numbers() {
    let ctx = support::context();
    ctx.enable_accesskit();
    let state = State::new(false);
    let mut controls = FieldCompareControls::default();
    controls.candidate_text = "(x, y)".to_owned();
    controls.apply(&state.parameters);

    let labels = run_body(&ctx, &state, &mut controls, true);
    assert!(
        labels
            .iter()
            .any(|label| label.contains("Predict before comparing")),
        "the prediction prompt is shown: {labels:?}"
    );
    assert!(
        !labels
            .iter()
            .any(|label| label.contains("Candidate - model")),
        "the numerical comparison stays hidden before a prediction: {labels:?}"
    );

    controls.skip();
    let labels = run_body(&ctx, &state, &mut controls, true);
    assert!(
        labels.iter().any(|label| label.contains("Point A")),
        "the revealed body labels the first point: {labels:?}"
    );
    assert!(
        labels
            .iter()
            .any(|label| label.contains("Candidate - model")),
        "the revealed body shows candidate minus model: {labels:?}"
    );
}

#[test]
fn the_comparison_body_fits_a_compact_sidebar_and_names_the_second_point() {
    let ctx = support::context();
    ctx.enable_accesskit();
    let state = State::new(false);
    let mut controls = FieldCompareControls::default();
    controls.candidate_text = "(x, y)".to_owned();
    controls.apply(&state.parameters);
    controls.skip();

    let labels = run_body(&ctx, &state, &mut controls, true);
    assert!(
        labels.iter().any(|label| label.contains("Point B")),
        "the second sample is labelled distinctly: {labels:?}"
    );
    assert!(
        labels
            .iter()
            .any(|label| label.contains("2×") || label.contains("2x")),
        "the second point offers the radial 2x reading: {labels:?}"
    );
}

#[test]
fn a_plane_slice_keeps_local_u_v_points_and_labels_them() {
    let ctx = support::context();
    ctx.enable_accesskit();
    let mut state = State::new(false);
    // A three-component field in 2D is a plane slice, so the editable points
    // are slice-local (u, v, w) rather than world (x, y, z).
    state.set_formula("(x, y, z)".to_owned());
    state.set_plane_axis(PlaneAxis::X);
    state.set_plane_index(8);
    assert!(
        state.plane_frame().is_some(),
        "the three-component field renders through a plane frame"
    );

    let mut controls = FieldCompareControls::default();
    controls.candidate_text = "(x, y, z)".to_owned();
    controls.apply(&state.parameters);
    controls.skip();
    controls.point_a = Vec3::new(0.5, -0.25, 0.0);
    controls.point_b = Vec3::new(-0.75, 0.5, 0.0);

    let labels = run_body(&ctx, &state, &mut controls, true);

    for axis in ["u", "v", "w"] {
        assert!(
            labels.iter().any(|label| label == axis),
            "the slice-local {axis} axis is labelled: {labels:?}"
        );
    }
    assert_eq!(
        controls.point_a,
        Vec3::new(0.5, -0.25, 0.0),
        "a nonzero local u/v point survives the frame instead of being clamped to zero"
    );
    assert_eq!(
        controls.point_b,
        Vec3::new(-0.75, 0.5, 0.0),
        "the second nonzero local u/v point survives the frame"
    );
}

#[test]
fn a_prediction_grade_names_the_point_it_was_made_at() {
    let ctx = support::context();
    ctx.enable_accesskit();
    let state = State::new(false);
    let mut controls = FieldCompareControls::default();
    // At (0.5, 0.5) this candidate points with the model; at (0.5, -0.5) it
    // points against the model, so editing Point A after predicting would
    // otherwise flip the grade.
    controls.candidate_text = "(x*y, y*y)".to_owned();
    controls.apply(&state.parameters);
    controls.predict(Prediction::With);

    controls.point_a = Vec3::new(0.5, -0.5, 0.0);
    let labels = run_body(&ctx, &state, &mut controls, true);
    assert!(
        labels
            .iter()
            .any(|label| label.contains("matches the first-point sampled direction")),
        "the grade still applies to the prediction point, not the edited point: {labels:?}"
    );
    assert!(
        labels
            .iter()
            .any(|label| label.contains("prediction was made at Point A")),
        "the prediction point is labelled: {labels:?}"
    );
    assert!(
        labels.iter().any(|label| label.contains("0.5, 0.5, 0")),
        "the label names the prediction point: {labels:?}"
    );
}

fn run_body(
    ctx: &egui::Context,
    state: &State,
    controls: &mut FieldCompareControls,
    compact: bool,
) -> Vec<String> {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(220.0, 1600.0),
            )),
            ..Default::default()
        },
        |ui| {
            if compact {
                ui.spacing_mut().interact_size.y = 20.0;
                ui.spacing_mut().item_spacing.y = 3.0;
            }
            show_body(ui, state, controls);
        },
    );
    output.textures_delta.clear();
    let mut labels: Vec<String> = output
        .platform_output
        .accesskit_update
        .as_ref()
        .map(|update| {
            update
                .nodes
                .iter()
                .filter_map(|(_, node)| {
                    node.label()
                        .map(str::to_owned)
                        .or_else(|| node.value().map(str::to_owned))
                })
                .collect()
        })
        .unwrap_or_default();
    if labels.is_empty() {
        // Keep the failure readable when the accesskit tree is unavailable.
        labels.push("<no accesskit labels>".to_owned());
    }
    labels
}
