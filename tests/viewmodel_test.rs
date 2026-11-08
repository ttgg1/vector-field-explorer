// End-to-end tests for the ticket-02 seam behavior (Task 8B): the formula bar
// edits and the mode toggle route through the viewmodel State, which owns
// parsing, the dirty flag and the scene snapshots the viewport paints.

use glam::Vec3;
use vector_field_explorer::plane::{DIM_ALPHA, PlaneAxis, PlaneView, plane_coordinate};
use vector_field_explorer::viewmodel::{
    State, DEFAULT_DENSITY_2D, DEFAULT_DENSITY_3D, FORMULA_3D, MAX_DENSITY_2D,
};

#[test]
fn valid_edit_rebuilds_and_reports_count() {
    let mut st = State::new(false);
    assert!(st.snapshot_if_dirty().is_some(), "fresh state rebuilds once");
    assert!(st.snapshot_if_dirty().is_none(), "clean state reuses the last scene");
    assert!(st.set_formula("(-y, x)".to_string()), "a valid edit marks the state dirty");
    assert_eq!(st.parse_error, "", "a valid edit clears any earlier parse error");
    let scene = st.snapshot_if_dirty();
    assert!(scene.is_some(), "the dirty state rebuilds exactly once");
    assert_eq!(scene.unwrap().arrows.len(), 576, "the rebuilt scene keeps the 24x24 grid");
    assert!(st.snapshot_if_dirty().is_none(), "a second pull finds nothing dirty");
}

#[test]
fn invalid_edit_reports_error_without_losing_the_field() {
    let mut st = State::new(false);
    assert!(st.snapshot_if_dirty().is_some());
    assert!(st.set_formula("(-y, x".to_string()), "an unbalanced edit is refused");
    assert!(!st.parse_error.is_empty(), "the parse failure surfaces as the state's error string");
    assert_eq!(st.field.dimension, 2, "the last good field is still in place");
    let scene = st.snapshot_if_dirty();
    assert!(scene.is_some(), "the refused edit still asks for a rebuild");
    assert_eq!(scene.unwrap().arrows.len(), 576,
        "the rebuilt scene keeps the previous field's 24x24 grid");
}

#[test]
fn preset_selection_switches_mode_and_field() {
    // Mirrors the grouped Presets panel's field-preset load for "3D radial":
    // set_mode(true) followed by the preset's formula, both through the seam.
    let mut st = State::new(false);
    assert!(st.snapshot_if_dirty().is_some());
    assert!(st.set_mode(true), "the mode toggle marks dirty");
    assert!(st.set_formula(FORMULA_3D.to_string()), "the preset formula applies");
    assert_eq!(st.field.dimension, 3, "the field is now three componential");
    assert!(st.is_3d(), "mode and field agree, so sampling runs cubic");
    assert_eq!(st.density, DEFAULT_DENSITY_2D,
        "the mode switch keeps the user's density: 24 is inside the 3D cap");
    assert_eq!(st.grid().z, 24, "sampling runs cubic at the preserved density");
    let scene = st.snapshot_if_dirty();
    assert!(scene.is_some(), "the mode switch rebuilds once");
    assert_eq!(scene.unwrap().arrows.len(), 24 * 24 * 24,
        "the cubic grid follows the preserved density, 24x24x24");
}

#[test]
fn density_cap_follows_sampling_dimension() {
    // A 2D field under 3D mode still affords the full 2D cap: the clamp keys
    // to the dimension being sampled, not the mode.
    let mut st = State::new(false);
    assert!(st.set_mode(true), "switching to 3D mode marks dirty");
    assert!(st.set_density(MAX_DENSITY_2D), "the 2D field still affords the 2D cap");
    assert_eq!(st.density, MAX_DENSITY_2D, "the cap is the sampling dimension's");
    assert_eq!(st.grid().z, 1, "a 2D field still samples flat in 3D mode");
}

#[test]
fn plane_state_defaults_to_the_middle_z_plane() {
    let st = State::new(true);
    assert_eq!(st.plane.axis, PlaneAxis::Z, "the default axis is z");
    assert!(!st.plane.enabled, "plane-by-plane starts off");
    assert_eq!(
        st.plane.index,
        (DEFAULT_DENSITY_3D - 1) / 2,
        "the default index is the middle z sample",
    );
    let flat = State::new(false);
    assert_eq!(flat.plane.index, 0, "a one-slice grid sits at index 0");
}

#[test]
fn enabling_plane_mode_from_a_fresh_selection_starts_at_the_middle() {
    // The app boots 2D with index 0; loading a 3D field leaves it there while
    // the mode is off. Turning plane mode on should show the middle slice,
    // matching `PlaneView::middle`.
    let mut st = State::new(false);
    assert_eq!(st.plane.index, 0);
    assert!(st.set_mode(true));
    assert!(st.set_formula(FORMULA_3D.to_string()));
    assert_eq!(st.plane.index, 0, "still the disabled startup index");
    assert!(st.set_plane_enabled(true));
    assert_eq!(st.plane.index, (st.density - 1) / 2, "enabling recentres to the middle plane");
}

#[test]
fn plane_setters_dirty_the_scene_without_reparsing_the_formula() {
    let mut st = State::new(true);
    assert!(st.snapshot_if_dirty().is_some(), "consume the fresh scene");
    let formula = st.formula.clone();
    let parse_error = st.parse_error.clone();
    let probe = Vec3::new(0.3, -0.2, 0.4);
    let evaluated = st.field.eval3(probe);

    assert!(st.set_plane_enabled(true), "enabling plane mode marks the scene dirty");
    assert!(st.dirty, "the app rebuilds the instances");
    assert_eq!(st.formula, formula, "the formula text is untouched");
    assert_eq!(st.parse_error, parse_error, "no parse runs, so no error can appear");
    assert_eq!(st.field.eval3(probe), evaluated, "the same field is still in place");
    assert!(st.snapshot_if_dirty().is_some(), "the rebuild happens once");
    assert!(!st.dirty);

    assert!(st.set_plane_index(2), "stepping the plane marks the scene dirty");
    assert!(st.set_plane_axis(PlaneAxis::X), "switching the axis marks the scene dirty");
    assert_eq!(st.formula, formula, "still no formula re-parse");
    assert_eq!(st.parse_error, parse_error);
    assert_eq!(st.field.eval3(probe), evaluated);
}

#[test]
fn plane_index_clamps_to_the_sampled_axis() {
    let mut st = State::new(true);
    assert!(st.set_plane_index(999), "an out-of-range step applies clamped");
    assert_eq!(st.plane.index, st.grid().z - 1, "clamped to the last z sample");
    assert!(!st.set_plane_index(st.plane.index), "already at the clamped index");
    assert!(st.set_plane_index(0));
    assert_eq!(st.plane.index, 0);
}

#[test]
fn plane_index_recentres_when_the_grid_shrinks_past_it() {
    let mut st = State::new(true);
    assert!(st.set_plane_index(15), "the last plane of the 16-cube");
    assert!(st.set_density(4), "a coarser grid has only 4 z samples");
    assert_eq!(st.plane.index, 1, "the vanished plane recentres to the middle sample");
    assert!(st.set_mode(false), "2D mode samples one z slice");
    assert_eq!(st.plane.index, 0, "a one-slice axis collapses to index 0");
}

#[test]
fn plane_mode_highlights_exactly_one_slice_and_dims_the_rest() {
    // 9 is odd, so the origin is a sample and `(x, y, z)` is degenerate there:
    // 9^3 - 1 arrows, of which 9^2 - 1 are on the middle z plane.
    let mut st = State::new(true);
    assert!(st.set_density(9));
    assert!(st.set_plane_enabled(true));
    assert!(st.set_plane_index(4), "the middle z sample");
    let arrows = st.sample();
    assert_eq!(arrows.len(), 9 * 9 * 9 - 1, "the degenerate origin arrow is dropped");
    let scene = st.snapshot();
    assert_eq!(scene.arrows.len(), 9 * 9 * 9 - 1, "the full cube is still painted, just dimmed");
    let instances = st.arrow_instances(&arrows);
    let selected: Vec<_> = instances.iter().filter(|a| a.color[3] == 1.0).collect();
    let dimmed: Vec<_> = instances.iter().filter(|a| a.color[3] != 1.0).collect();
    assert_eq!(selected.len(), 9 * 9 - 1, "exactly n^2 - 1 arrows on the middle plane");
    assert_eq!(dimmed.len(), instances.len() - selected.len());
    let plane_z = plane_coordinate(&st.domain, st.grid(), PlaneAxis::Z, 4);
    for arrow in &selected {
        assert!((arrow.origin[2] - plane_z).abs() < 1e-5, "selected arrows sit on z = {plane_z}");
    }
    for arrow in &dimmed {
        assert!((arrow.color[3] - DIM_ALPHA).abs() < 1e-6, "the rest is drawn translucent");
    }
}

#[test]
fn plane_mode_off_keeps_every_arrow_at_full_colour_and_alpha() {
    let mut st = State::new(true);
    assert!(st.set_density(8));
    assert!(!st.plane.enabled);
    let arrows = st.sample();
    let instances = st.arrow_instances(&arrows);
    assert_eq!(instances.len(), 8 * 8 * 8);
    for (arrow, instance) in arrows.iter().zip(&instances) {
        assert_eq!(instance.color[3], 1.0, "no dimming when the mode is off");
        let expected = [
            arrow.color[0] as f32 / 255.0,
            arrow.color[1] as f32 / 255.0,
            arrow.color[2] as f32 / 255.0,
        ];
        for (c, expected_channel) in expected.iter().enumerate() {
            assert!((instance.color[c] - *expected_channel).abs() < 1e-6, "colormap colour is unchanged");
        }
    }
}

#[test]
fn plane_mode_is_inert_in_two_d() {
    let mut st = State::new(false);
    assert!(st.set_plane_enabled(true), "the toggle still flows state in 2D");
    assert!(st.set_plane_axis(PlaneAxis::X), "and so does the axis");
    assert!(!st.plane_active(), "2D has no plane-by-plane inspection");
    let arrows = st.sample();
    let instances = st.arrow_instances(&arrows);
    assert!(instances.iter().all(|a| a.color[3] == 1.0), "2D arrows keep full alpha");
}

#[test]
fn plane_mode_is_inert_when_a_flat_field_samples_one_slice() {
    let mut st = State::new(false);
    assert!(st.set_mode(true), "3D mode on a 2D field");
    assert!(st.set_plane_enabled(true));
    assert!(st.set_plane_axis(PlaneAxis::X), "x would have slices, but sampling stays flat");
    assert!(!st.plane_active(), "a one-slice cube has nothing to step through");
    let instances = st.arrow_instances(&st.sample());
    assert!(instances.iter().all(|a| a.color[3] == 1.0), "nothing is dimmed");
}

#[test]
fn switching_the_axis_highlights_that_plane() {
    let mut st = State::new(true);
    assert!(st.set_density(8));
    assert!(st.set_plane_enabled(true));
    assert!(st.set_plane_axis(PlaneAxis::X));
    assert!(st.set_plane_index(0));
    let arrows = st.sample();
    let instances = st.arrow_instances(&arrows);
    let selected: Vec<_> = instances.iter().filter(|a| a.color[3] == 1.0).collect();
    assert_eq!(selected.len(), 8 * 8, "one x slice of the 8-cube");
    let x = plane_coordinate(&st.domain, st.grid(), PlaneAxis::X, 0);
    assert!(selected.iter().all(|a| (a.origin[0] - x).abs() < 1e-5));
}

#[test]
fn turning_plane_mode_off_restores_the_cube_exactly() {
    let mut st = State::new(true);
    assert!(st.set_density(8));
    let baseline = st.arrow_instances(&st.sample());
    assert!(st.set_plane_enabled(true));
    let dimmed = st.arrow_instances(&st.sample());
    assert!(dimmed.iter().any(|a| a.color[3] < 1.0), "the mode really dims something");
    assert!(st.set_plane_enabled(false));
    let restored = st.arrow_instances(&st.sample());
    assert_eq!(baseline.len(), restored.len());
    for (before, after) in baseline.iter().zip(&restored) {
        assert_eq!(before.origin, after.origin);
        assert_eq!(before.dir_len, after.dir_len);
        assert_eq!(before.color, after.color, "turning the mode off restores the normal cube");
    }
}

#[test]
fn plane_view_clamped_matches_the_state_grid() {
    let st = State::new(true);
    let raw = PlaneView { enabled: true, axis: PlaneAxis::Y, index: 999 };
    let clamped = raw.clamped(st.grid());
    assert_eq!(clamped.index, st.grid().y - 1, "clamped to the last y sample");
    assert_eq!(clamped.axis, PlaneAxis::Y, "the axis is untouched");
}
