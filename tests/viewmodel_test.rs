// End-to-end tests for the ticket-02 seam behavior (Task 8B): the formula bar
// edits and the mode toggle route through the viewmodel State, which owns
// parsing, the dirty flag and the scene snapshots the viewport paints.

use vector_field_explorer::viewmodel::{State, DEFAULT_DENSITY_2D, FORMULA_3D, MAX_DENSITY_2D};

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
    // Mirrors the formula bar's ComboBox path for the "3D radial" preset:
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
