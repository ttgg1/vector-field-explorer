// End-to-end tests for the ticket-02 seam behavior (Task 8B): the formula bar
// edits and the mode toggle route through the viewmodel State, which owns
// parsing, the dirty flag and the scene snapshots the viewport paints.

use glam::{Mat4, Vec3};
use vector_field_explorer::camera::OrbitCamera;
use vector_field_explorer::colormap::Colormap;
use vector_field_explorer::plane::{DIM_ALPHA, PlaneAxis, PlaneView, plane_coordinate};
use vector_field_explorer::renderer::{
    ArrowInstanceGPU, LineInstanceGPU, combined_instances, sort_back_to_front,
    sort_scene_back_to_front, sorted_arrows_back_to_front,
};
use vector_field_explorer::sampling::ArrowData;
use vector_field_explorer::particles::{DOT_SPACING_FRACTION, MAX_PARTICLES};
use vector_field_explorer::viewmodel::{
    ArrowLengthMode, DEFAULT_DENSITY_2D, DEFAULT_DENSITY_3D, DEFAULT_PARTICLE_SIZE,
    DEFAULT_PROPORTIONAL_EXPONENT, FORMULA_2D, FORMULA_3D, MAX_DENSITY_2D, MAX_DENSITY_3D,
    MAX_DOMAIN_EXTENT, MAX_MAGNITUDE_SCALE, MAX_PARTICLE_SIZE, MAX_PROPORTIONAL_EXPONENT,
    MIN_DENSITY, MIN_DOMAIN_EXTENT, MIN_MAGNITUDE_SCALE, MIN_PARTICLE_SIZE,
    MIN_PROPORTIONAL_EXPONENT, STREAMLINE_MAX_LEN, State, StreamlineSeedMode,
    clamp_domain_extent, domain_between,
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
fn leaving_a_one_slice_grid_starts_at_the_middle_plane() {
    // The app boots 2D with index 0, where the single z slice makes 0 the only
    // possible index. The first transition to a multi-slice grid (3D mode plus
    // a three-component formula) is therefore not a user selection and should
    // land on the middle plane, matching `PlaneView::middle`.
    let mut st = State::new(false);
    assert_eq!(st.plane.index, 0, "the one-slice startup grid sits at index 0");
    assert!(st.set_mode(true), "switching to 3D mode marks dirty");
    assert_eq!(st.plane.index, 0, "a 2D field still samples one z slice");
    assert!(st.set_formula(FORMULA_3D.to_string()), "the preset formula applies");
    assert_eq!(
        st.plane.index,
        (st.density - 1) / 2,
        "leaving the one-slice grid recentres to the middle plane",
    );
    assert!(!st.plane.enabled, "recentring does not switch the mode on");
}

#[test]
fn a_user_selected_plane_zero_survives_disable_and_enable() {
    // Regression: index 0 is a legitimate plane once the cube exists (enable,
    // drag to plane 0, disable, re-enable). Toggling the mode must keep the
    // user's slice instead of jumping back to the middle.
    let mut st = State::new(true);
    assert!(st.set_plane_index(0), "drag the plane to the first z slice");
    assert!(st.set_plane_enabled(true), "show plane 0");
    assert_eq!(st.plane.index, 0);
    assert!(st.set_plane_enabled(false), "hide the plane");
    assert_eq!(st.plane.index, 0, "hiding keeps the selected slice");
    assert!(st.set_plane_enabled(true), "show it again");
    assert_eq!(st.plane.index, 0, "re-enabling must not jump to the middle plane");

    let middle = (st.density - 1) / 2;
    assert!(st.set_plane_index(middle), "a middle selection");
    assert!(st.set_plane_enabled(false));
    assert!(st.set_plane_enabled(true));
    assert_eq!(st.plane.index, middle, "a non-zero selection survives the toggle too");
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

// ---------------------------------------------------------------------------
// Moved out of `src/viewmodel.rs`'s inline module (ticket 13). Names,
// assertions and helper semantics are unchanged; only the imports moved from
// `super::*` / `crate::` to `vector_field_explorer::`.
// ---------------------------------------------------------------------------

#[test]
fn new_state_starts_at_default_density() {
    let s2 = State::new(false);
    assert_eq!(s2.density, DEFAULT_DENSITY_2D);
    assert_eq!(s2.field.dimension, 2);
    assert_eq!(s2.formula, FORMULA_2D);
    assert_eq!(s2.parse_error, "");
    assert!(s2.dirty, "a fresh state must ask for a scene");
    let s3 = State::new(true);
    assert_eq!(s3.density, DEFAULT_DENSITY_3D);
    assert_eq!(s3.field.dimension, 3);
}

#[test]
fn domain_extrema_swap_makes_valid_box() {
    let low = Vec3::new(1.0, -5.0, 3.0);
    let high = Vec3::new(-2.0, 7.0, 0.0);
    let d = domain_between(low, high);
    assert_eq!(d.min, Vec3::new(-2.0, -5.0, 0.0));
    assert_eq!(d.max, Vec3::new(1.0, 7.0, 3.0));
    let size = d.domain_size();
    assert!(size.x > 0.0 && size.y > 0.0 && size.z > 0.0, "swapped box must not be empty");
    let mut s = State::new(false);
    let changed = s.set_domain(low, high);
    assert!(changed, "domain actually moved");
    assert_eq!(s.domain.min, d.min);
}

#[test]
fn density_clamped_per_mode() {
    let mut s2 = State::new(false);
    assert!(s2.set_density(2), "below floor clamps up");
    assert_eq!(s2.density, MIN_DENSITY);
    assert!(s2.set_density(999), "above ceiling clamps down");
    assert_eq!(s2.density, MAX_DENSITY_2D);
    assert!(!s2.set_density(1000), "already at the cap");
    let mut s3 = State::new(true);
    assert!(s3.set_density(1), "below floor clamps up");
    assert_eq!(s3.density, MIN_DENSITY);
    assert!(s3.set_density(999), "above ceiling clamps down");
    assert_eq!(s3.density, MAX_DENSITY_3D);
    assert!(!s3.set_density(40), "already at the cap");
}

#[test]
fn garbage_formula_keeps_field_and_reports_error() {
    let mut s = State::new(false);
    let before = s.field.clone();
    let probe = Vec3::new(0.5, -0.25, 0.0);
    assert!(s.set_formula(")(".to_string()), "an invalid edit still marks the state dirty");
    assert!(!s.parse_error.is_empty(), "the parse failure is reported");
    assert!(
        s.field.eval3(probe) == before.eval3(probe),
        "the last good field must stay in place"
    );
    assert!(s.dirty);
}

#[test]
fn valid_formula_replaces_field_and_clears_error() {
    let mut s = State::new(false);
    assert!(s.set_formula(")(".to_string()), "invalid edit reports…");
    assert!(!s.parse_error.is_empty(), "…and sets an error");
    assert!(s.set_formula("(-y, x)".to_string()), "a valid replacement applies");
    assert_eq!(s.parse_error, "", "the error clears");
    assert_eq!(s.field.dimension, 2);
    assert_eq!(s.field.eval3(Vec3::new(2.0, 1.0, 0.0)), Some(Vec3::new(-1.0, 2.0, 0.0)));
    assert!(s.dirty, "a valid replacement marks the scene dirty");
}

#[test]
fn unchanged_setters_report_false_and_stay_clean() {
    let mut s = State::new(false);
    assert!(s.snapshot_if_dirty().is_some(), "fresh state snapshots once");
    assert!(!s.dirty, "rebuild consumes the dirty flag");
    assert!(!s.set_colormap(s.colormap));
    assert!(!s.set_density(s.density));
    assert!(!s.set_formula(s.formula.clone()), "same text is not a change");
    assert!(!s.set_domain(s.domain.min, s.domain.max));
    assert!(!s.set_arrow_scale(s.arrow_scale));
    assert!(!s.set_arrow_length_mode(s.arrow_length_mode));
    assert!(!s.set_magnitude_scale(s.magnitude_scale));
    assert!(!s.set_proportional_exponent(s.proportional_exponent));
    assert!(!s.set_mode(s.mode_3d));
    assert!(!s.dirty, "a failed setter must not re-dirty the state");
    assert!(s.snapshot_if_dirty().is_none(), "no rebuild when nothing changed");
}

#[test]
fn snapshot_rebuilds_only_when_dirty() {
    let mut s = State::new(false);
    assert!(s.snapshot_if_dirty().is_some());
    assert!(s.set_colormap(Colormap::Turbo), "a real change marks dirty");
    assert!(s.snapshot_if_dirty().is_some(), "a real change rebuilds once");
    assert!(s.snapshot_if_dirty().is_none(), "and only once");
}

#[test]
fn pump_snapshot_has_the_whole_grid() {
    let s = State::new(false);
    let arrows = s.sample();
    assert_eq!(arrows.len(), 576, "24x24 pump grid: 576 sampled arrows");
    let scene = s.snapshot();
    assert_eq!(scene.arrows.len(), 576, "the scene carries the whole grid");
    assert!(scene.lines.is_empty(), "streamlines join in Task 9");
    assert!(scene.dots.is_empty(), "particles join in Task 10");
}

#[test]
fn uniform_arrows_share_one_length() {
    // Ticket 27's bug report: Uniform must still paint every arrow exactly
    // `arrow_scale` grid cells long through the real state path.
    let mut s = State::new(false);
    assert!(
        s.set_arrow_length_mode(ArrowLengthMode::Uniform),
        "the mode change marks dirty",
    );
    let inst = s.arrow_instances(&s.sample());
    let expected = s.arrow_scale * s.spacing();
    assert!(expected > 0.0);
    for a in &inst {
        assert!(
            (a.dir_len[3] - expected).abs() < 1e-5,
            "every uniform arrow is arrow_scale cells long",
        );
    }
}

#[test]
fn speaking_arrows_are_magnitude_proportional() {
    let s = State::new(false);
    let arrows = s.sample();
    let inst = s.arrow_instances(&arrows);
    let lengths: Vec<f32> = inst.iter().map(|a| a.dir_len[3]).collect();
    let expected_max = s.arrow_scale * s.spacing();
    let max_len = lengths.iter().fold(0.0_f32, |m, l| m.max(*l));
    assert!((max_len - expected_max).abs() < 1e-5, "longest arrow is exactly arrow_scale cells");
    for l in &lengths {
        assert!(*l <= expected_max + 1e-5, "no arrow exceeds arrow_scale cells");
    }
    let by_mag: Vec<(f32, f32)> = arrows
        .iter()
        .zip(lengths)
        .map(|(a, l)| (a.mag, l))
        .collect();
    let mut sorted = by_mag.clone();
    sorted.sort_by(|(a, _), (b, _)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    assert!(sorted.len() > 1, "need at least two arrows to compare");
    for i in 1..sorted.len() {
        assert!(
            sorted[i - 1].1 <= sorted[i].1 + 1e-5,
            "length must follow magnitude (speaking arrows)",
        );
    }
}

// --- Arrow length modes and magnitude scale (ticket 33) ---------------------

/// A sampled arrow with a synthetic magnitude, so a test can pin exact
/// lengths without depending on the sampling grid's own values.
fn raw_arrow(position: Vec3, dir: Vec3, mag: f32) -> ArrowData {
    ArrowData { position, dir, mag, color: [128, 128, 128] }
}

#[test]
fn magnitude_mode_uses_the_raw_magnitude_as_world_length() {
    let mut s = State::new(false);
    assert_eq!(s.arrow_length_mode, ArrowLengthMode::Proportional, "proportional is the default");
    assert!(s.set_arrow_length_mode(ArrowLengthMode::Magnitude));
    assert!(s.set_magnitude_scale(0.25));
    assert!(s.set_arrow_scale(3.0), "a non-default arrow scale must not leak in");
    // A 100:1 magnitude ratio: proportional normalises both against the max,
    // magnitude mode keeps each sampled magnitude as the world length.
    let arrows = [
        raw_arrow(Vec3::new(-0.5, 0.0, 0.0), Vec3::X, 100.0),
        raw_arrow(Vec3::new(0.5, 0.0, 0.0), Vec3::Y, 1.0),
    ];
    let inst = s.arrow_instances(&arrows);
    assert!((inst[0].dir_len[3] - 25.0).abs() < 1e-5, "100 x 0.25 world units");
    assert!((inst[1].dir_len[3] - 0.25).abs() < 1e-5, "1 x 0.25 world units");
    assert_eq!(inst[0].dir_len[..3], [1.0, 0.0, 0.0], "direction stays unit");
    assert_eq!(inst[1].dir_len[..3], [0.0, 1.0, 0.0]);
}

#[test]
fn magnitude_mode_differs_from_proportional_on_a_varying_field() {
    let mut s = State::new(false);
    assert!(s.set_formula("(x, y * 3.0)".to_string()), "a field whose magnitude varies");
    let arrows = s.sample();
    let proportional = s.arrow_instances(&arrows);
    assert!(s.set_arrow_length_mode(ArrowLengthMode::Magnitude));
    let magnitude = s.arrow_instances(&arrows);
    let differing = proportional
        .iter()
        .zip(&magnitude)
        .filter(|(p, m)| (p.dir_len[3] - m.dir_len[3]).abs() > 1e-5)
        .count();
    assert!(
        differing > arrows.len() / 2,
        "magnitude mode is a visibly different length mapping, not a relabel",
    );
}

#[test]
fn every_mode_keeps_unit_direction_and_magnitude_colour() {
    let mut s = State::new(false);
    assert!(s.set_formula("(x, y * 3.0)".to_string()));
    let arrows = s.sample();
    let min = arrows.iter().min_by(|a, b| a.mag.total_cmp(&b.mag)).unwrap();
    let max = arrows.iter().max_by(|a, b| a.mag.total_cmp(&b.mag)).unwrap();
    assert_ne!(min.color, max.color, "the colormap really varies with magnitude");
    let min_index = arrows.iter().position(|a| a.mag == min.mag).unwrap();
    let max_index = arrows.iter().position(|a| a.mag == max.mag).unwrap();

    for mode in ArrowLengthMode::ALL {
        s.set_arrow_length_mode(mode);
        let inst = s.arrow_instances(&arrows);
        for (arrow, instance) in arrows.iter().zip(&inst) {
            let len = (instance.dir_len[0].powi(2)
                + instance.dir_len[1].powi(2)
                + instance.dir_len[2].powi(2))
            .sqrt();
            assert!((len - 1.0).abs() < 1e-5, "{mode:?} keeps the direction unit");
            for channel in 0..3 {
                assert!(
                    (instance.color[channel] - arrow.color[channel] as f32 / 255.0).abs() < 1e-6,
                    "{mode:?} keeps the magnitude colour",
                );
            }
        }
        assert_ne!(
            inst[min_index].color, inst[max_index].color,
            "{mode:?} still colours by magnitude",
        );
    }
}

#[test]
fn arrow_scale_and_magnitude_scale_are_independent() {
    let mut s = State::new(false);
    let arrows = s.sample();
    for mode in ArrowLengthMode::ALL {
        s.set_arrow_length_mode(mode);
        s.set_arrow_scale(1.0);
        s.set_magnitude_scale(1.0);
        let base = s.arrow_instances(&arrows);
        assert!(s.set_arrow_scale(2.0), "the shared length scale moves");
        let wider = s.arrow_instances(&arrows);
        s.set_arrow_scale(1.0);
        assert!(s.set_magnitude_scale(3.0), "the magnitude scale moves");
        let stronger = s.arrow_instances(&arrows);
        let arrow_factor = if mode == ArrowLengthMode::Magnitude { 1.0 } else { 2.0 };
        let magnitude_factor = if mode == ArrowLengthMode::Magnitude { 3.0 } else { 1.0 };
        for i in 0..base.len() {
            assert!(
                (wider[i].dir_len[3] - arrow_factor * base[i].dir_len[3]).abs() < 1e-5,
                "{mode:?}: arrow_scale applies only to Proportional/Uniform",
            );
            assert!(
                (stronger[i].dir_len[3] - magnitude_factor * base[i].dir_len[3]).abs() < 1e-5,
                "{mode:?}: magnitude_scale applies only to Magnitude",
            );
        }
        s.set_magnitude_scale(1.0);
    }
}

#[test]
fn uniform_snapshot_has_one_length_on_a_high_contrast_field() {
    // Ticket 27 carry-over through the real snapshot path: Uniform paints one
    // length while the same field's Proportional arrows vary with magnitude.
    let formula = "(x, y * 50.0)".to_string();
    let mut uniform = State::new(false);
    assert!(uniform.set_formula(formula.clone()));
    assert!(uniform.set_arrow_length_mode(ArrowLengthMode::Uniform));
    let scene = uniform.snapshot();
    assert!(!scene.arrows.is_empty(), "the high-contrast field samples arrows");
    let expected = uniform.arrow_scale * uniform.spacing();
    let first = scene.arrows[0].dir_len[3];
    assert!((first - expected).abs() < 1e-5, "the shared length is arrow_scale * spacing");
    for arrow in scene.arrows.iter() {
        assert!(
            (arrow.dir_len[3] - first).abs() < 1e-5,
            "every uniform snapshot arrow has the same length",
        );
    }

    let mut proportional = State::new(false);
    assert!(proportional.set_formula(formula));
    let lengths: Vec<f32> =
        proportional.snapshot().arrows.iter().map(|a| a.dir_len[3]).collect();
    let min = lengths.iter().copied().fold(f32::INFINITY, f32::min);
    let max = lengths.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    assert!(
        max - min > 1e-3,
        "the same field's proportional snapshot has distinct lengths",
    );
}

#[test]
fn magnitude_scale_clamps_and_rejects_nan() {
    let mut s = State::new(false);
    assert_eq!(s.arrow_length_mode, ArrowLengthMode::Proportional);
    assert!(!s.set_arrow_length_mode(ArrowLengthMode::Proportional), "same mode is a no-op");
    assert!(s.set_arrow_length_mode(ArrowLengthMode::Magnitude), "a new mode marks dirty");
    assert_eq!(s.arrow_length_mode, ArrowLengthMode::Magnitude);

    assert!(s.set_magnitude_scale(0.001), "below the floor clamps up");
    assert_eq!(s.magnitude_scale, MIN_MAGNITUDE_SCALE);
    assert!(s.set_magnitude_scale(1_000.0), "above the ceiling clamps down");
    assert_eq!(s.magnitude_scale, MAX_MAGNITUDE_SCALE);
    let kept = s.magnitude_scale;
    assert!(!s.set_magnitude_scale(f32::NAN), "NaN is rejected");
    assert_eq!(s.magnitude_scale, kept, "a rejected NaN cannot poison the scene");
    s.set_magnitude_scale(1.0);
    assert!(s.set_magnitude_scale(f32::INFINITY), "+inf clamps to the ceiling");
    assert_eq!(s.magnitude_scale, MAX_MAGNITUDE_SCALE);
}

#[test]
fn extreme_magnitudes_are_not_clamped_because_the_user_owns_the_scale() {
    let mut s = State::new(false);
    assert!(s.set_arrow_length_mode(ArrowLengthMode::Magnitude));
    assert!(s.set_magnitude_scale(MAX_MAGNITUDE_SCALE));
    let arrows = [raw_arrow(Vec3::ZERO, Vec3::X, 1.0e6)];
    let inst = s.arrow_instances(&arrows);
    assert!(
        (inst[0].dir_len[3] - 1.0e7).abs() < 1.0,
        "length = magnitude x scale in world units, deliberately not clamped",
    );
}

// --- Proportional contrast (ticket 57) -------------------------------------

#[test]
fn proportional_exponent_raises_the_normalized_magnitude_to_its_power() {
    let mut s = State::new(false);
    assert_eq!(s.proportional_exponent, DEFAULT_PROPORTIONAL_EXPONENT, "linear by default");
    let unit = s.arrow_scale * s.spacing();
    // Two arrows at 4x and 1x the max, so the normalized magnitudes are 1 and
    // 0.25 and every expected length is exact.
    let arrows = [
        raw_arrow(Vec3::ZERO, Vec3::X, 4.0),
        raw_arrow(Vec3::ZERO, Vec3::Y, 1.0),
    ];

    assert!(s.set_proportional_exponent(2.0), "a new exponent dirties the scene");
    let squared = s.arrow_instances(&arrows);
    assert!((squared[0].dir_len[3] - unit).abs() < 1e-5, "the longest arrow stays unit long");
    assert!((squared[1].dir_len[3] - unit * 0.25 * 0.25).abs() < 1e-6, "0.25 squared");

    assert!(s.set_proportional_exponent(0.5));
    let rooted = s.arrow_instances(&arrows);
    assert!((rooted[0].dir_len[3] - unit).abs() < 1e-5, "the longest arrow is still unit long");
    assert!((rooted[1].dir_len[3] - unit * 0.5).abs() < 1e-6, "sqrt(0.25) = 0.5");
}

#[test]
fn exponent_one_is_exactly_the_old_linear_formula() {
    let mut s = State::new(false);
    assert_eq!(s.proportional_exponent, 1.0, "the default is the pre-ticket-57 formula");
    assert!(s.set_formula("(x, y * 3.0)".to_string()), "a field whose magnitude varies");
    let arrows = s.sample();
    let unit = s.arrow_scale * s.spacing();
    let max_mag = arrows.iter().fold(0.0_f32, |m, a| m.max(a.mag));
    let inst = s.arrow_instances(&arrows);
    for (arrow, instance) in arrows.iter().zip(&inst) {
        let expected = unit * (arrow.mag / max_mag);
        assert_eq!(
            instance.dir_len[3].to_bits(),
            expected.to_bits(),
            "exponent 1.0 must reproduce the old formula bit for bit",
        );
    }
    assert!(!s.set_proportional_exponent(1.0), "1.0 is still a no-op");
}

#[test]
fn proportional_exponent_leaves_uniform_and_magnitude_lengths_untouched() {
    let mut s = State::new(false);
    let arrows = [
        raw_arrow(Vec3::ZERO, Vec3::X, 4.0),
        raw_arrow(Vec3::ZERO, Vec3::Y, 1.0),
    ];
    for mode in [ArrowLengthMode::Uniform, ArrowLengthMode::Magnitude] {
        s.set_arrow_length_mode(mode);
        s.set_proportional_exponent(1.0);
        let base = s.arrow_instances(&arrows);
        assert!(s.set_proportional_exponent(3.0), "a strong contrast exponent");
        let contrasted = s.arrow_instances(&arrows);
        for (before, after) in base.iter().zip(&contrasted) {
            assert_eq!(
                before.dir_len[3].to_bits(),
                after.dir_len[3].to_bits(),
                "{mode:?} ignores the Proportional exponent",
            );
        }
    }
}

#[test]
fn arrow_scale_still_scales_proportional_and_uniform_with_a_custom_exponent() {
    let mut s = State::new(false);
    let arrows = [
        raw_arrow(Vec3::ZERO, Vec3::X, 4.0),
        raw_arrow(Vec3::ZERO, Vec3::Y, 1.0),
    ];
    for mode in [ArrowLengthMode::Proportional, ArrowLengthMode::Uniform] {
        s.set_arrow_length_mode(mode);
        s.set_proportional_exponent(2.5);
        s.set_arrow_scale(1.0);
        let base = s.arrow_instances(&arrows);
        assert!(s.set_arrow_scale(2.0), "the shared length scale moves");
        let wider = s.arrow_instances(&arrows);
        for (before, after) in base.iter().zip(&wider) {
            assert!(
                (after.dir_len[3] - 2.0 * before.dir_len[3]).abs() < 1e-6,
                "{mode:?}: arrow_scale keeps scaling linearly alongside the exponent",
            );
        }
    }
}

#[test]
fn proportional_exponent_clamps_and_maps_non_finite_to_linear() {
    let mut s = State::new(false);
    assert_eq!(s.proportional_exponent, DEFAULT_PROPORTIONAL_EXPONENT);

    assert!(s.set_proportional_exponent(0.001), "below the floor clamps up");
    assert_eq!(s.proportional_exponent, MIN_PROPORTIONAL_EXPONENT);
    assert!(s.set_proportional_exponent(1_000.0), "above the ceiling clamps down");
    assert_eq!(s.proportional_exponent, MAX_PROPORTIONAL_EXPONENT);

    assert!(s.set_proportional_exponent(f32::NAN), "a NaN resets the contrast");
    assert_eq!(s.proportional_exponent, DEFAULT_PROPORTIONAL_EXPONENT, "NaN maps to linear");

    assert!(s.set_proportional_exponent(2.0));
    assert!(s.set_proportional_exponent(f32::INFINITY), "+inf resets the contrast");
    assert_eq!(s.proportional_exponent, DEFAULT_PROPORTIONAL_EXPONENT, "+inf maps to linear");

    assert!(s.set_proportional_exponent(2.0));
    assert!(s.set_proportional_exponent(f32::NEG_INFINITY), "-inf resets the contrast");
    assert_eq!(s.proportional_exponent, DEFAULT_PROPORTIONAL_EXPONENT, "-inf maps to linear");
}

#[test]
fn color_range_follows_auto_fit() {
    let mut s = State::new(false);
    let v = s.view_options();
    assert!(v.color_range.is_none(), "auto-fit lets the sampler fit colors");
    assert_eq!(v.colormap, Colormap::Viridis);
    assert!(s.set_auto_fit(false), "auto-fit off marks dirty");
    assert!(s.set_color_range(2.0, -3.0), "bounds swap and apply");
    assert_eq!(s.color_min, -3.0);
    assert_eq!(s.color_max, 2.0);
    assert_eq!(s.view_options().color_range, Some((-3.0, 2.0)));
}

#[test]
fn density_clamps_to_dimension_being_sampled() {
    let mut s = State::new(false);
    assert!(s.set_mode(true), "switching to 3D mode marks dirty");
    assert!(s.set_density(MAX_DENSITY_2D), "a flat field still affords the 2D cap");
    assert_eq!(s.density, MAX_DENSITY_2D, "cap keys to the sampled dimension, not the mode");
    assert_eq!(s.grid().z, 1, "a 2D field still samples flat in 3D mode");
}

#[test]
fn plane_size_scales_with_largest_extent() {
    let mut s = State::new(false);
    let size = s.plane_size();
    assert!((size - 2.7).abs() < 1e-4, "default 2-unit domain times the margin");
    assert!(
        s.set_domain(Vec3::new(-2.0, 0.0, 0.0), Vec3::new(2.0, 1.0, 0.0)),
        "a wider-than-tall domain applies",
    );
    let grown = s.plane_size();
    assert!((grown - 5.4).abs() < 1e-4, "follows the x extent once it is largest");
}

#[test]
fn snapshot_reports_active_mode() {
    let mut s = State::new(false);
    assert!(s.set_mode(true), "3D mode on a 2D field marks dirty");
    let scene = s.snapshot();
    assert_eq!(
        scene.arrows.len(),
        (DEFAULT_DENSITY_2D * DEFAULT_DENSITY_2D) as usize,
        "the flat sample count, not the cubic one",
    );
    let flat = scene.arrows.iter().all(|a| a.origin[2] == 0.0);
    assert!(flat, "every arrow origin sits on the z = 0 plane when sampling runs flat");
}

#[test]
fn snapshot_leaves_the_combined_stream_empty_for_the_app_to_sort() {
    // Ticket 62 ruling: the viewmodel keeps emitting frame-invariant arrows in
    // sampling order; the app builds and sorts the combined arrow+particle
    // stream each frame. An empty combined here is the contract.
    let s = State::new(true);
    let scene = s.snapshot();
    assert!(
        scene.combined.is_empty(),
        "the viewmodel does not own the per-frame combined stream",
    );
}

#[test]
fn particle_free_combined_matches_the_arrow_count_and_order() {
    // Pixel identity when particles are off: the combined stream has exactly
    // one instance per arrow and the same comparator produces the same order
    // as the legacy arrow sort.
    let s = State::new(true);
    let scene = s.snapshot();
    let mut combined = combined_instances(&scene.arrows, &[]);
    assert_eq!(
        combined.len(),
        scene.arrows.len(),
        "particles off: one combined instance per arrow",
    );

    let view = Mat4::from_rotation_x(0.4);
    sort_scene_back_to_front(&mut combined, &view);
    let mut reference = scene.arrows.as_ref().clone();
    sort_back_to_front(&mut reference, &view);

    let key = |i: &ArrowInstanceGPU| (i.origin, i.dir_len, i.color);
    let combined_keys: Vec<_> = combined.iter().map(key).collect();
    let reference_keys: Vec<_> = reference.iter().map(key).collect();
    assert_eq!(
        combined_keys, reference_keys,
        "the combined stream orders arrows exactly like the legacy sort",
    );
}

#[test]
fn two_d_arrow_draw_order_matches_the_old_sorted_order() {
    // Ticket 62 fix: 2D keeps its own two-draw path but still paints a stable
    // far-to-near copy of the arrows (the pre-ticket-62 order), while the
    // snapshot itself stays unsorted. The orbit view tilts across the z = 0
    // grid, so input order and sorted order genuinely differ here.
    let s = State::new(false);
    let scene = s.snapshot();
    let view = OrbitCamera::default().view();
    let sorted = sorted_arrows_back_to_front(&scene.arrows, &view);

    let mut reference = scene.arrows.as_ref().clone();
    sort_back_to_front(&mut reference, &view);

    let key = |i: &ArrowInstanceGPU| i.origin;
    let sorted_keys: Vec<_> = sorted.iter().map(key).collect();
    let reference_keys: Vec<_> = reference.iter().map(key).collect();
    assert_eq!(sorted_keys, reference_keys, "the 2D draw order is the legacy sorted order");

    let input_keys: Vec<_> = scene.arrows.iter().map(key).collect();
    assert_ne!(
        input_keys, sorted_keys,
        "this case really reorders: the snapshot order is not the sorted draw order",
    );
}

#[test]
fn two_d_streamlines_snapshot_emits_line_segments() {
    let mut s = State::new(false);
    assert!(s.set_streamlines(true), "enabling streamlines marks the scene dirty");
    let scene = s.snapshot();
    assert!(!scene.lines.is_empty(), "enabled streamlines paint at least one line segment");
    assert!(scene.dots.is_empty(), "particles still wait for their task");
}

#[test]
fn streamlines_run_in_both_modes_and_off_by_default() {
    let mut s3 = State::new(true);
    assert!(s3.set_streamlines(true), "the toggle still works in 3D");
    assert!(!s3.snapshot().lines.is_empty(), "the orbit view gets 3D streamlines (ticket 6)");
    let s2 = State::new(false);
    assert!(s2.snapshot().lines.is_empty(), "streamlines are off at the defaults");
}

#[test]
fn streamlines_use_the_fixed_point_budget() {
    assert_eq!(STREAMLINE_MAX_LEN, 120, "streamlines keep a fixed point budget");
    let mut state = State::new(false);
    assert!(state.set_formula("(-y, x)".to_string()), "switch the plane to a vortex");
    assert!(state.set_streamlines(true));
    assert!(!state.snapshot().lines.is_empty(), "the static trace still draws");
}

#[test]
fn streamline_seeds_survive_identical_rebuilds() {
    let mut s = State::new(false);
    assert!(s.set_formula("(-y, x)".to_string()));
    assert!(s.set_streamlines(true));
    let first = s.snapshot().lines.len();
    let second = s.snapshot().lines.len();
    let third = s.snapshot().lines.len();
    assert_eq!(first, second, "seeds are deterministic: rebuilds agree");
    assert_eq!(second, third, "and again, with no state change in between");
}

/// The payload of every line instance, for equality checks
/// (`LineInstanceGPU` has no `PartialEq` of its own).
fn segments(lines: &[LineInstanceGPU]) -> Vec<([f32; 4], [f32; 4], [f32; 4])> {
    lines.iter().map(|l| (l.a, l.b, l.color)).collect()
}

#[test]
fn click_streamlines_seed_a_burst_while_the_grid_is_hidden() {
    // Click mode hides the automatic grid but a click is still an explicit
    // request: it plants a burst, and the burst stays on the 2D plane.
    let mut s = State::new(false);
    assert!(s.set_formula("(1, 0)".to_string()), "a field that flows at the origin");
    assert!(s.set_streamlines(true), "the Feature is on; Click mode only hides the grid");
    assert!(s.set_seed_mode(StreamlineSeedMode::Click));
    assert!(s.snapshot().lines.is_empty(), "Click hides the grid seeds");
    let lines = s.click_streamlines_snapshot((0.0, 1.0), &[Vec3::ZERO]);
    assert!(!lines.is_empty(), "the click seeds a burst anyway");
    assert!(
        lines.iter().all(|l| l.a[2] == 0.0 && l.b[2] == 0.0),
        "the 2D burst stays on the click's plane",
    );
    let starts: Vec<[f32; 3]> = lines.iter().map(|l| [l.a[0], l.a[1], l.a[2]]).collect();
    let unique = starts.iter().fold(Vec::new(), |mut acc: Vec<[f32; 3]>, p| {
        if !acc.contains(p) {
            acc.push(*p);
        }
        acc
    });
    assert!(unique.len() > 1, "the burst traces several seeds, not one: {} starts", unique.len());
}

#[test]
fn click_streamlines_are_deterministic_and_bounded_by_the_clicks() {
    // Same clicks redraw the same bursts; each extra click adds its own
    // geometry; and a click off the field contributes nothing.
    let mut s = State::new(true);
    assert!(s.set_formula("(-y, x, 0.5)".to_string()), "a depth-carrying field");
    assert!(s.set_streamlines(true), "click bursts are part of the Streamlines feature");
    let clicks = [Vec3::new(0.1, 0.1, 0.0), Vec3::new(-0.2, 0.3, 0.1)];
    let first = s.click_streamlines_snapshot((0.0, 1.5), &clicks);
    let second = s.click_streamlines_snapshot((0.0, 1.5), &clicks);
    assert_eq!(segments(&first), segments(&second), "rebuilds redraw the same bursts");
    let one = s.click_streamlines_snapshot((0.0, 1.5), &clicks[..1]);
    assert!(first.len() > one.len(), "each click adds its own burst");
    assert!(s.click_streamlines_snapshot((0.0, 1.5), &[]).is_empty(), "no clicks, no click lines");
    assert!(
        s.click_streamlines_snapshot((0.0, 1.5), &[Vec3::new(9.0, 9.0, 9.0)]).is_empty(),
        "a click off the field draws nothing",
    );
}

// --- Streamline seed modes (ticket 50) -------------------------------------

#[test]
fn streamline_seed_mode_defaults_to_auto_and_accepts_clicks() {
    let mut s = State::new(false);
    assert_eq!(s.seed_mode, StreamlineSeedMode::Auto, "a fresh state starts on Auto");
    assert_eq!(StreamlineSeedMode::default(), StreamlineSeedMode::Auto);
    assert_eq!(StreamlineSeedMode::ALL, [StreamlineSeedMode::Auto, StreamlineSeedMode::Click]);
    assert_eq!(StreamlineSeedMode::ALL.map(StreamlineSeedMode::label), ["Auto", "Click"]);
    assert!(!s.accepts_click_seed(), "the Feature still starts off");
    assert!(s.set_streamlines(true));
    assert!(s.accepts_click_seed(), "Auto keeps the default click live");
}

#[test]
fn auto_seed_mode_shows_grid_seeds_and_accepts_clicks() {
    let mut s = State::new(false);
    assert!(s.set_streamlines(true));
    assert_eq!(s.seed_mode, StreamlineSeedMode::Auto);
    assert!(!s.snapshot().lines.is_empty(), "Auto draws the automatic grid seeds");
    assert!(
        !s.click_streamlines_snapshot((0.0, 1.0), &[Vec3::ZERO]).is_empty(),
        "Auto still accepts a click seed",
    );
}

#[test]
fn click_seed_mode_hides_the_grid_but_shows_click_seeds() {
    let mut s = State::new(false);
    assert!(s.set_streamlines(true));
    assert!(!s.snapshot().lines.is_empty(), "Auto starts with grid seeds");
    assert!(s.set_seed_mode(StreamlineSeedMode::Click));
    assert_eq!(s.seed_mode, StreamlineSeedMode::Click);
    assert!(s.snapshot().lines.is_empty(), "Click hides the grid seeds");
    assert!(s.accepts_click_seed(), "Click still accepts clicks");
    assert!(
        !s.click_streamlines_snapshot((0.0, 1.0), &[Vec3::ZERO]).is_empty(),
        "Click shows the click seeds",
    );
}

#[test]
fn streamlines_off_hides_both_sources_and_keeps_the_previous_clicks() {
    let mut s = State::new(false);
    assert!(s.set_formula("(1, 0)".to_string()), "a field that flows at the origin");
    assert!(s.set_streamlines(true));
    let clicks = [Vec3::ZERO];
    let before = s.click_streamlines_snapshot((0.0, 1.0), &clicks);
    assert!(!before.is_empty(), "the click lines exist while Streamlines is on");

    assert!(s.set_streamlines(false));
    assert!(s.snapshot().lines.is_empty(), "off hides the grid seeds");
    assert!(
        s.click_streamlines_snapshot((0.0, 1.0), &clicks).is_empty(),
        "off hides the click seeds too",
    );
    assert!(!s.accepts_click_seed(), "off refuses to store a new click seed");

    assert!(s.set_streamlines(true));
    assert_eq!(
        segments(&s.click_streamlines_snapshot((0.0, 1.0), &clicks)),
        segments(&before),
        "turning Streamlines back on reproduces the retained click lines",
    );
}

#[test]
fn clamp_domain_extent_is_finite_and_bounded() {
    // The pure seam bound the sidebar hands extents through: finite values
    // pass through inside the box, extremes clamp to the documented edge,
    // and non-finite input still yields a usable number.
    assert_eq!(clamp_domain_extent(5.0), 5.0, "an in-range extent is unchanged");
    assert_eq!(clamp_domain_extent(-5.0), -5.0, "negative extents too");
    assert_eq!(
        clamp_domain_extent(MAX_DOMAIN_EXTENT + 1.0),
        MAX_DOMAIN_EXTENT,
        "above the ceiling clamps down",
    );
    assert_eq!(
        clamp_domain_extent(MIN_DOMAIN_EXTENT - 1.0),
        MIN_DOMAIN_EXTENT,
        "below the floor clamps up",
    );
    assert_eq!(clamp_domain_extent(f32::INFINITY), MAX_DOMAIN_EXTENT, "+inf clamps down");
    assert_eq!(clamp_domain_extent(f32::NEG_INFINITY), MIN_DOMAIN_EXTENT, "−inf clamps up");
    assert_eq!(clamp_domain_extent(f32::NAN), 0.0, "NaN becomes a finite default");
    assert!(clamp_domain_extent(1.0e30).is_finite(), "huge values stay finite");
}

// --- Ticket 30: particle size and count --------------------------------

fn close(actual: f32, expected: f32) -> bool {
    (actual - expected).abs() < 1e-6
}

#[test]
fn particle_size_defaults_to_one_and_clamps() {
    let mut s = State::new(false);
    assert_eq!(s.particle_size, DEFAULT_PARTICLE_SIZE, "dots start at the base fraction");
    assert!(s.set_particle_size(0.1), "below the floor moves the value");
    assert_eq!(s.particle_size, MIN_PARTICLE_SIZE, "size clamps up to 0.25");
    assert!(s.set_particle_size(9.0), "above the ceiling moves the value");
    assert_eq!(s.particle_size, MAX_PARTICLE_SIZE, "size clamps down to 3.0");
    assert!(s.set_particle_size(1.75), "in-range sizes pass through");
    assert_eq!(s.particle_size, 1.75);
    assert!(!s.set_particle_size(1.75), "an unchanged size is a no-op");
    assert!(s.set_particle_size(f32::NAN), "NaN moves the non-default size");
    assert_eq!(s.particle_size, DEFAULT_PARTICLE_SIZE, "NaN resets to the default");
    assert!(!s.set_particle_size(f32::NAN), "NaN at the default is a no-op");
    assert!(s.set_particle_size(2.0));
    assert!(s.set_particle_size(f32::INFINITY), "a non-finite size also moves the value");
    assert_eq!(s.particle_size, DEFAULT_PARTICLE_SIZE, "infinity resets to the default");
}

#[test]
fn particle_radius_is_linear_in_size_and_spacing() {
    let mut s = State::new(false);
    assert!(
        close(s.particle_radius(0.5), DOT_SPACING_FRACTION * 0.5),
        "at the default size the radius is the base fraction of the spacing",
    );
    let base = s.particle_radius(0.5);
    assert!(s.set_particle_size(2.0));
    assert!(close(s.particle_radius(0.5), base * 2.0), "twice the size is twice the radius");
    assert!(close(s.particle_radius(1.0), s.particle_radius(0.5) * 2.0),
        "twice the spacing is twice the radius");
    assert_eq!(DOT_SPACING_FRACTION, 0.3, "the base fraction itself never moves");
}

#[test]
fn particle_count_defaults_to_auto_and_clamps() {
    let mut s = State::new(false);
    assert_eq!(s.particle_count, 0, "0 is the Auto sentinel");
    assert!(s.set_particle_count(100), "an explicit count moves the value");
    assert_eq!(s.particle_count, 100);
    assert!(!s.set_particle_count(100), "an unchanged count is a no-op");
    assert!(s.set_particle_count(5000), "above the cap moves the value");
    assert_eq!(s.particle_count, MAX_PARTICLES as u32, "the count caps at 4000");
    assert!(s.set_particle_count(0), "zero returns to Auto");
    assert_eq!(s.particle_count, 0);
}

#[test]
fn particle_size_and_count_do_not_redirty_the_scene() {
    // Dots are rebuilt from the live cloud every frame; size/count are not
    // sampling-grid inputs, so they must not trigger a scene rebuild.
    let mut s = State::new(true);
    assert!(s.snapshot_if_dirty().is_some(), "consume the fresh scene");
    assert!(s.set_particle_size(2.5));
    assert!(s.set_particle_count(500));
    assert!(!s.dirty, "the setters leave the dirty flag alone");
    assert!(s.snapshot_if_dirty().is_none(), "and no rebuild is requested");
}

#[test]
fn particle_target_is_none_while_off_or_in_2d() {
    let mut s = State::new(true);
    s.set_density(8);
    assert_eq!(s.particle_target(), None, "particles off spawns nothing");
    assert!(s.set_particles(true));
    assert_eq!(s.particle_target(), Some(128), "Auto follows the density (8 x 16)");
    assert!(s.set_particle_count(100));
    assert_eq!(s.particle_target(), Some(100), "an explicit count overrides Auto");
    assert!(s.set_mode(false));
    assert_eq!(s.particle_target(), None, "the 2D view has no depth for the cloud");
    assert!(s.set_mode(true));
    assert_eq!(s.particle_target(), Some(100), "returning to 3D restores the target");
    assert!(s.set_particles(false));
    assert_eq!(s.particle_target(), None);
}
