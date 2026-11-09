// The sidebar owns a plain mirror of the scene knobs (`Controls`): pure data
// that the widgets edit between frames and that `show` hands to the viewmodel
// setters once per frame. The defaults must agree with the seam's or the first
// sidebar frame would change settings the user never touched (and the status
// bar would count a scene the user never asked for).

use vector_field_explorer::ui::sidebar::{
    Controls, apply, arrow_scale_enabled, domain_summary, magnitude_scale_enabled,
};
use vector_field_explorer::colormap::Colormap;
use vector_field_explorer::domain::Domain;
use vector_field_explorer::plane::{PlaneAxis, PlaneView};
use vector_field_explorer::viewmodel::{
    ArrowLengthMode, State, DEFAULT_DENSITY_2D, MAX_DENSITY_3D, MAX_DOMAIN_EXTENT,
    MIN_DOMAIN_EXTENT, domain_between,
};
use glam::Vec3;

#[test]
fn sidebar_defaults_mirror_the_seam_defaults() {
    let c = Controls::new();
    let s = State::new(false);
    assert_eq!(c.density, s.density, "density starts at the seam default");
    assert_eq!(c.density, DEFAULT_DENSITY_2D, "…and that default is pinned");
    assert_eq!(c.arrow_scale, s.arrow_scale, "arrow scale agrees");
    assert_eq!(c.arrow_length_mode, s.arrow_length_mode, "arrow length mode agrees");
    assert_eq!(c.magnitude_scale, s.magnitude_scale, "magnitude scale agrees");
    assert_eq!(c.colormap, s.colormap, "colormap agrees");
    assert_eq!(c.auto_fit, s.auto_fit, "auto-fit agrees");
    assert_eq!(c.color_min, s.color_min, "color min agrees");
    assert_eq!(c.color_max, s.color_max, "color max agrees");
    assert_eq!(c.streamlines, s.streamlines, "streamlines start off");
    assert_eq!(c.particles, s.particles, "particles start off");
    assert_eq!(c.speed, s.speed, "speed agrees");
    assert_eq!(c.domain_min, s.domain.min, "domain min agrees");
    assert_eq!(c.domain_max, s.domain.max, "domain max agrees");
    assert_eq!(c.plane, s.plane, "the plane mirror starts where the seam does");
}

#[test]
fn apply_never_commits_the_plane_mirror() {
    // The plane index commits on slider release (ticket 23), so the per-frame
    // `apply` handoff must not touch it: a drag would otherwise resample the
    // field on every frame.
    let mut c = Controls::new();
    let mut s = State::new(false);
    assert!(s.snapshot_if_dirty().is_some(), "consume the fresh scene");
    c.plane = PlaneView { enabled: true, axis: PlaneAxis::X, index: 0 };
    apply(&mut s, &mut c);
    assert!(!s.dirty, "apply leaves the plane alone");
    assert!(!s.plane.enabled, "the seam still has plane mode off");
    assert_eq!(s.plane.axis, PlaneAxis::Z);
}

#[test]
fn synced_controls_never_redirty_the_scene() {
    // Mirrors one sidebar frame with untouched widgets: every setter must
    // report "nothing changed" so the snapshot and the status-bar count stay.
    let c = Controls::new();
    let mut s = State::new(false);
    assert!(s.snapshot_if_dirty().is_some(), "the fresh scene builds once");
    assert!(!s.set_density(c.density), "synced density does not re-dirty");
    assert!(!s.set_arrow_scale(c.arrow_scale));
    assert!(!s.set_arrow_length_mode(c.arrow_length_mode));
    assert!(!s.set_magnitude_scale(c.magnitude_scale));
    assert!(!s.set_colormap(c.colormap));
    assert!(!s.set_auto_fit(c.auto_fit));
    assert!(!s.set_color_range(c.color_min, c.color_max));
    assert!(!s.set_streamlines(c.streamlines));
    assert!(!s.set_particles(c.particles));
    assert!(!s.set_speed(c.speed));
    assert!(!s.set_domain(c.domain_min, c.domain_max), "synced extrema are a no-op");
    assert!(s.snapshot_if_dirty().is_none(), "an untouched frame rebuilds nothing");
}

#[test]
fn reversed_drag_extrema_swap_through_the_seam() {
    // The domain row's drag values may end up reversed (min dragged past max);
    // show() hands them to set_domain as-is and the seam swaps them.
    let mut c = Controls::new();
    let mut s = State::new(false);
    assert!(s.snapshot_if_dirty().is_some());
    c.domain_min.x = 2.0;
    c.domain_max.x = -2.0;
    assert!(s.set_domain(c.domain_min, c.domain_max), "the reversed pair changes the domain");
    assert_eq!(s.domain.min.x, -2.0, "the seam enforces the min<max rule");
    assert_eq!(s.domain.max.x, 2.0, "…by swapping, not by rejecting");
    let scene = s.snapshot_if_dirty();
    assert!(scene.is_some(), "the change asks for a rebuild");
}

// The tests below drive `apply` — the one pure function `show` calls once a
// frame to hand the whole mirror to the seam — so the wiring is verified on
// its real per-frame code path without a window (the spec's testing rule).

#[test]
fn untouched_frame_rebuilds_nothing_through_apply() {
    let mut c = Controls::new();
    let mut s = State::new(false);
    assert!(s.snapshot_if_dirty().is_some(), "the fresh scene builds once");
    apply(&mut s, &mut c);
    assert!(s.snapshot_if_dirty().is_none(), "an untouched apply rebuilds nothing");
    assert_eq!(c.density, s.density, "and leaves the mirror in sync");
}

#[test]
fn density_clamp_keys_to_sampled_dimension_through_apply() {
    let mut c = Controls::new();
    let mut s = State::new(true);
    assert!(s.snapshot_if_dirty().is_some(), "fresh scene built");
    c.density = 60;
    apply(&mut s, &mut c);
    assert_eq!(s.density, MAX_DENSITY_3D, "n³ sampling caps at the 40 ceiling");
    assert_eq!(c.density, MAX_DENSITY_3D, "the mirror is refreshed to the effective cap");
    assert_eq!(s.grid().z, MAX_DENSITY_3D, "sampling really runs n³ at the cap");
    assert!(s.snapshot_if_dirty().is_some(), "the clamped density still moved the scene");

    let mut c2 = Controls::new();
    let mut s2 = State::new(false);
    assert!(s2.set_mode(true), "3D mode on a flat field");
    assert!(s2.snapshot_if_dirty().is_some(), "the mode switch rebuilt");
    c2.density = 150;
    apply(&mut s2, &mut c2);
    assert_eq!(s2.density, 150, "flat sampling affords the 200 ceiling even in 3D mode");
    assert_eq!(s2.grid().z, 1, "…and samples flat");
    assert!(s2.snapshot_if_dirty().is_some(), "the density change re-instanced");
}

#[test]
fn colormap_row_and_manual_range_feed_color_range_through_apply() {
    let mut c = Controls::new();
    let mut s = State::new(false);
    assert!(s.snapshot_if_dirty().is_some(), "fresh scene built");
    apply(&mut s, &mut c);
    assert!(s.view_options().color_range.is_none(), "auto-fit keeps the sampler's own fit");
    c.colormap = Colormap::Turbo;
    c.auto_fit = false;
    c.color_min = 5.0;
    c.color_max = -1.0;
    apply(&mut s, &mut c);
    assert_eq!(s.colormap, Colormap::Turbo, "the selectable-label row lands on the seam");
    assert_eq!(
        s.view_options().color_range,
        Some((-1.0_f32, 5.0_f32)),
        "the manual pair feeds color_range, swapped by the seam",
    );
    assert_eq!(c.color_min, -1.0, "the mirror shows the seam's swapped pair");
    assert_eq!(c.color_max, 5.0);
    assert!(s.snapshot_if_dirty().is_some(), "coloring changes re-instance");
}

#[test]
fn every_control_dirties_the_scene_through_apply() {
    let mut c = Controls::new();
    let mut s = State::new(false);
    assert!(s.snapshot_if_dirty().is_some(), "consume the fresh scene");
    c.density += 1;
    apply(&mut s, &mut c);
    assert!(s.snapshot_if_dirty().is_some(), "density dirties the scene");
    c.arrow_scale = 2.0;
    apply(&mut s, &mut c);
    assert!(s.snapshot_if_dirty().is_some(), "arrow scale dirties the scene");
    c.arrow_length_mode = ArrowLengthMode::Magnitude;
    apply(&mut s, &mut c);
    assert!(s.snapshot_if_dirty().is_some(), "arrow length mode dirties the scene");
    c.magnitude_scale = 2.0;
    apply(&mut s, &mut c);
    assert!(s.snapshot_if_dirty().is_some(), "magnitude scale dirties the scene");
    c.colormap = Colormap::Mono;
    apply(&mut s, &mut c);
    assert!(s.snapshot_if_dirty().is_some(), "colormap dirties the scene");
    c.auto_fit = false;
    apply(&mut s, &mut c);
    assert!(s.snapshot_if_dirty().is_some(), "auto-fit toggle dirties the scene");
    c.color_min = 0.25;
    c.color_max = 1.75;
    apply(&mut s, &mut c);
    assert!(s.snapshot_if_dirty().is_some(), "manual color range dirties the scene");
    c.streamlines = true;
    apply(&mut s, &mut c);
    assert!(s.snapshot_if_dirty().is_some(), "streamlines stub still flows state");
    c.particles = true;
    apply(&mut s, &mut c);
    assert!(s.snapshot_if_dirty().is_some(), "particles stub still flows state");
    c.speed = 2.0;
    apply(&mut s, &mut c);
    assert!(s.snapshot_if_dirty().is_some(), "speed slider flows to the seam");
    c.domain_max.x = 0.5;
    apply(&mut s, &mut c);
    assert!(s.snapshot_if_dirty().is_some(), "domain extrema dirty the scene");
}

// --- Arrow length modes and magnitude scale (ticket 33) ---------------------

#[test]
fn magnitude_scale_is_enabled_only_in_magnitude_mode() {
    assert!(
        !magnitude_scale_enabled(ArrowLengthMode::Proportional),
        "Proportional has no separate magnitude scale",
    );
    assert!(!magnitude_scale_enabled(ArrowLengthMode::Uniform), "Uniform has no magnitude scale");
    assert!(
        magnitude_scale_enabled(ArrowLengthMode::Magnitude),
        "Magnitude is the one mode the scale applies to",
    );
}

#[test]
fn arrow_scale_is_disabled_only_in_magnitude_mode() {
    assert!(
        arrow_scale_enabled(ArrowLengthMode::Proportional),
        "Proportional lengths follow the arrow scale",
    );
    assert!(arrow_scale_enabled(ArrowLengthMode::Uniform), "Uniform lengths are the arrow scale");
    assert!(
        !arrow_scale_enabled(ArrowLengthMode::Magnitude),
        "Magnitude length is magnitude x magnitude_scale alone",
    );
}

#[test]
fn length_mode_and_both_scales_flow_through_apply_and_the_mirror_stays_in_sync() {
    let mut c = Controls::new();
    let mut s = State::new(false);
    assert!(s.snapshot_if_dirty().is_some(), "consume the fresh scene");
    c.arrow_length_mode = ArrowLengthMode::Magnitude;
    c.arrow_scale = 2.0;
    c.magnitude_scale = 0.25;
    apply(&mut s, &mut c);
    assert_eq!(s.arrow_length_mode, ArrowLengthMode::Magnitude, "the mode lands on the seam");
    assert_eq!(s.arrow_scale, 2.0, "the numeric arrow scale lands too");
    assert_eq!(s.magnitude_scale, 0.25, "the magnitude scale lands too");
    assert_eq!(c.arrow_length_mode, s.arrow_length_mode, "the mirror follows the mode");
    assert_eq!(c.arrow_scale, s.arrow_scale, "the mirror follows the arrow scale");
    assert_eq!(c.magnitude_scale, s.magnitude_scale, "the mirror follows the magnitude scale");
    assert!(
        !arrow_scale_enabled(s.arrow_length_mode),
        "Magnitude mode disables the arrow scale widget",
    );
    assert!(
        magnitude_scale_enabled(s.arrow_length_mode),
        "Magnitude mode enables the magnitude scale widget",
    );
    assert!(s.snapshot_if_dirty().is_some(), "the edit rebuilds the scene once");
}

#[test]
fn display_reset_restores_proportional_and_both_scales() {
    let mut c = Controls::new();
    c.arrow_length_mode = ArrowLengthMode::Magnitude;
    c.arrow_scale = 3.5;
    c.magnitude_scale = 4.0;

    c.reset_display();

    assert_eq!(c.arrow_length_mode, ArrowLengthMode::Proportional, "the default mode returns");
    assert_eq!(c.arrow_scale, 1.0, "the default arrow scale returns");
    assert_eq!(c.magnitude_scale, 1.0, "the default magnitude scale returns");
}

// --- Domain summary, bounds and reset (ticket 32) ---------------------------

#[test]
fn domain_summary_is_stable_for_default_reversed_and_non_unit_boxes() {
    assert_eq!(
        domain_summary(&Domain::default(), true),
        "x -1…1 · y -1…1 · z -1…1",
        "the default unit cube reads as whole numbers",
    );
    assert_eq!(
        domain_summary(&Domain::default(), false),
        "x -1…1 · y -1…1",
        "2D omits the z extent (ticket 50 passes the flag)",
    );

    // A reversed pair is normalised by the seam; the summary shows the
    // effective box, not the raw drag mirror.
    let reversed = domain_between(Vec3::new(2.0, -4.0, 0.5), Vec3::new(-2.0, 4.0, 1.5));
    assert_eq!(domain_summary(&reversed, true), "x -2…2 · y -4…4 · z 0.5…1.5");

    let fractional = Domain::new(Vec3::new(-2.5, 0.125, -0.004), Vec3::new(3.0, 4.0, 10.0));
    assert_eq!(
        domain_summary(&fractional, true),
        "x -2.5…3 · y 0.13…4 · z 0…10",
        "decimals stay short and negative zero is not printed",
    );
}

#[test]
fn domain_extents_are_clamped_before_the_seam_through_apply() {
    let mut c = Controls::new();
    let mut s = State::new(false);
    assert!(s.snapshot_if_dirty().is_some(), "consume the fresh scene");
    c.domain_min = Vec3::new(-500.0, 0.0, 0.0);
    c.domain_max = Vec3::new(500.0, 2.0, 1.0);
    apply(&mut s, &mut c);
    assert_eq!(s.domain.min.x, MIN_DOMAIN_EXTENT, "the min drag cannot leave the bound");
    assert_eq!(s.domain.max.x, MAX_DOMAIN_EXTENT, "the max drag cannot leave the bound");
    assert_eq!(c.domain_min.x, MIN_DOMAIN_EXTENT, "the mirror shows the clamped value");
    assert_eq!(c.domain_max.x, MAX_DOMAIN_EXTENT);
    assert_eq!((s.domain.min.y, s.domain.max.y), (0.0, 2.0), "other axes are untouched");
    assert!(s.snapshot_if_dirty().is_some(), "a clamped edit still changes the scene");
}

#[test]
fn reversed_domain_still_swaps_after_clamping_through_apply() {
    let mut c = Controls::new();
    let mut s = State::new(false);
    assert!(s.snapshot_if_dirty().is_some(), "consume the fresh scene");
    c.domain_min = Vec3::new(500.0, 2.0, 0.0);
    c.domain_max = Vec3::new(-500.0, -2.0, 1.0);
    apply(&mut s, &mut c);
    assert_eq!(
        (s.domain.min.x, s.domain.max.x),
        (MIN_DOMAIN_EXTENT, MAX_DOMAIN_EXTENT),
        "clamp first, then the existing per-axis swap",
    );
    assert_eq!((s.domain.min.y, s.domain.max.y), (-2.0, 2.0), "the y pair swaps too");
}

#[test]
fn nan_domain_edit_keeps_the_previous_extent_through_apply() {
    let mut c = Controls::new();
    let mut s = State::new(false);
    assert!(s.snapshot_if_dirty().is_some(), "consume the fresh scene");
    c.domain_min.x = f32::NAN;
    c.domain_max.x = f32::NAN;
    c.domain_min.y = 2.0;
    c.domain_max.y = 3.0;
    apply(&mut s, &mut c);
    assert_eq!((s.domain.min.x, s.domain.max.x), (-1.0, 1.0), "NaN cannot enter the state");
    assert_eq!(c.domain_min.x, -1.0, "the mirror falls back to the previous extent");
    assert_eq!(c.domain_max.x, 1.0);
    assert_eq!((s.domain.min.y, s.domain.max.y), (2.0, 3.0), "a valid axis still applies");
}

#[test]
fn domain_reset_restores_all_axes_including_z_in_2d() {
    let mut s = State::new(false);
    assert!(s.set_domain(Vec3::splat(-5.0), Vec3::splat(5.0)), "a non-default box is stored");
    let mut c = Controls::new();
    c.domain_min = s.domain.min;
    c.domain_max = s.domain.max;
    assert_eq!(c.domain_min.z, -5.0, "2D still stores z");

    // The header and body reset buttons both call this same seam.
    c.reset_domain();
    apply(&mut s, &mut c);
    assert_eq!(s.domain, Domain::default(), "reset restores the unit cube");
    assert_eq!(c.domain_min, Domain::default().min, "the mirror follows");
    assert_eq!(c.domain_max, Domain::default().max);
}

#[test]
fn domain_reset_clears_a_partial_unapplied_mirror_edit() {
    // The Reset domain button acts on the local mirror before the next frame's
    // `apply`; a stale partial edit must not survive it.
    let mut controls = Controls::new();
    controls.domain_min = Vec3::new(0.25, -2.0, f32::NAN);
    controls.domain_max = Vec3::new(5.0, 3.0, -0.5);

    controls.reset_domain();

    assert_eq!(controls.domain_min, Vec3::splat(-1.0), "all three minima reset before apply");
    assert_eq!(controls.domain_max, Vec3::splat(1.0), "all three maxima reset before apply");
}
