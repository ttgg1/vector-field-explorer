//! Task 2 field modes, source sampling and source-mode gating coverage.

use glam::DVec3;
use vector_field_explorer::source_scene::{
    FamilyDisposition, FieldMode, SourceDisplay, SourceKindTag, SourceScene, classify_family_result,
};
use vector_field_explorer::sources::{
    Diagnostics, Estimate, FamilyResult, FieldFamily, StopReason,
};
use vector_field_explorer::viewmodel::State;

fn one_point() -> SourceScene {
    let mut scene = SourceScene::default();
    scene
        .add(SourceKindTag::PointCharge.default_source())
        .unwrap();
    scene
}

fn estimate(value: DVec3, uncertainty: f64, target: f64) -> Estimate {
    Estimate {
        value,
        uncertainty,
        target,
        absolute_contributions: uncertainty,
        diagnostics: Diagnostics {
            kernel_id: "test",
            family: FieldFamily::Electric,
            softening: 0.1,
            epsilon: 1e-7,
            family_scale: 1.0,
            source_count: 1,
            work_units: 0,
            cells_high_water: 0,
            model_regularized: true,
            roundoff_limited: false,
            underflow: false,
        },
    }
}

#[test]
fn mode_switches_preserve_formula_and_sources() {
    let mut state = State::new(false);
    state.set_formula("(y, -x, z)".into());
    let formula = state.formula.clone();
    state.set_source_scene(one_point());
    assert_eq!(state.sources.entries.len(), 1);
    assert!(state.set_field_mode(FieldMode::Sources));
    assert_eq!(state.field_mode, FieldMode::Sources);
    assert_eq!(state.formula, formula);
    assert_eq!(state.sources.entries.len(), 1);
    let three_d = state.mode_3d;
    assert!(state.set_field_mode(FieldMode::Formula));
    assert_eq!(state.mode_3d, three_d);
    assert_eq!(state.formula, formula);
    assert_eq!(state.sources.entries.len(), 1);
}

#[test]
fn source_mode_is_three_dimensional_and_2d_uses_the_plane_frame() {
    let mut state = State::new(false);
    state.set_source_scene(one_point());
    state.set_field_mode(FieldMode::Sources);
    state.set_mode(false);
    assert_eq!(state.active_dimension(), 3);
    assert!(state.is_plane_slice());
    state.snapshot_if_dirty();
    let sampling = state.source_sampling().expect("source mode sampled");
    assert!(!sampling.arrows.is_empty());
    assert!(sampling.arrows.iter().all(|a| a.position.z.abs() < 1e-6));
}

#[test]
fn accuracy_not_met_is_hidden_by_default_and_opt_in_draws_finite_only() {
    let estimate = estimate(DVec3::X, 1.0, 0.1);
    let not_met = FamilyResult::AccuracyNotMet(estimate.clone(), StopReason::WorkLimit);
    assert_eq!(
        classify_family_result(&not_met, false),
        FamilyDisposition::HiddenAccuracy
    );
    assert_eq!(
        classify_family_result(&not_met, true),
        FamilyDisposition::Draw
    );
    assert_eq!(
        classify_family_result(&FamilyResult::Unavailable(StopReason::NumericRange), true),
        FamilyDisposition::Unavailable
    );

    let mut state = State::new(false);
    state.set_source_scene(one_point());
    state.set_field_mode(FieldMode::Sources);
    state.sources.allow_inaccurate = false;
    state.mark_dirty();
    state.snapshot_if_dirty();
    let default_sampling = state.source_sampling().unwrap();
    assert_eq!(default_sampling.electric.accuracy_not_met_drawn, 0);
}

#[test]
fn both_display_uses_two_family_colours_without_summing() {
    let mut scene = SourceScene::default();
    scene
        .add(SourceKindTag::PointCharge.default_source())
        .unwrap();
    scene
        .add(SourceKindTag::CurrentLoop.default_source())
        .unwrap();
    scene.set_display(SourceDisplay::Both);
    let mut state = State::new(true);
    state.set_source_scene(scene);
    state.set_field_mode(FieldMode::Sources);
    state.snapshot_if_dirty();
    let sampling = state.source_sampling().unwrap();
    let electric: Vec<_> = sampling
        .arrows
        .iter()
        .filter(|a| a.color == [255, 176, 64])
        .collect();
    let magnetic: Vec<_> = sampling
        .arrows
        .iter()
        .filter(|a| a.color == [64, 196, 255])
        .collect();
    assert!(!electric.is_empty(), "E family is drawn");
    assert!(!magnetic.is_empty(), "B family is drawn");
}

#[test]
fn source_mode_has_no_formula_transients_and_does_not_consume_the_clock() {
    let mut state = State::new(true);
    state.set_formula("(-y, x, 0)".into());
    state.set_streamlines(true);
    state.set_particles(true);
    state.set_source_scene(one_point());
    state.set_field_mode(FieldMode::Sources);
    state.snapshot_if_dirty();
    assert!(state.streamlines_snapshot(&[]).is_empty());
    assert!(state.click_streamlines_snapshot((0.0, 1.0), &[]).is_empty());
    assert!(state.particle_target().is_none());
    assert!(!state.has_time_dependent_consumer());
}

#[test]
fn an_empty_family_is_zero_not_an_error() {
    let mut scene = SourceScene::default();
    scene
        .add(SourceKindTag::PointCharge.default_source())
        .unwrap();
    let mut state = State::new(true);
    state.set_source_scene(scene);
    state.set_field_mode(FieldMode::Sources);
    state.sources.set_display(SourceDisplay::Magnetic);
    state.snapshot_if_dirty();
    let sampling = state.source_sampling().unwrap();
    assert!(sampling.magnetic.empty);
    assert_eq!(sampling.magnetic.drawn, 0);
    assert!(sampling.validation_error.is_none());
}

#[test]
fn formula_edits_do_not_replace_source_samples() {
    let mut state = State::new(false);
    state.set_source_scene(one_point());
    state.set_field_mode(FieldMode::Sources);
    state.snapshot_if_dirty();
    let before = state.source_sampling().unwrap().arrows.clone();
    state.set_formula("(1, 2, 3)".into());
    state.snapshot_if_dirty();
    let after = state.source_sampling().unwrap().arrows.clone();
    assert_eq!(before, after);
}
