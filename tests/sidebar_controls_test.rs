// The sidebar owns a plain mirror of the scene knobs (`Controls`): pure data
// that the widgets edit between frames and that `show` hands to the viewmodel
// setters once per frame. The defaults must agree with the seam's or the first
// sidebar frame would change settings the user never touched (and the status
// bar would count a scene the user never asked for).

use vector_field_explorer::ui::sidebar::{
    ArrowStyleSet, Controls, DomainAxis, apply, arrow_scale_enabled, arrow_style_set, domain_axes,
    domain_summary, fixed_color_opaque, fixed_color_rgb, head_control_enabled,
    magnitude_scale_enabled, particle_count_label, particle_fixed_color_enabled,
    particle_size_enabled, proportional_exponent_enabled, seed_mode_enabled, sides_control_enabled,
    stagnation_seconds_enabled, trail_fade_enabled, trail_width_enabled,
};
use vector_field_explorer::arrow_style::{ArrowParams, ArrowStyle, ArrowStyle2D};
use vector_field_explorer::colormap::Colormap;
use vector_field_explorer::domain::Domain;
use vector_field_explorer::particles::{MAX_PARTICLES, ParticleAppearance, ParticleColorMode};
use vector_field_explorer::plane::{DIM_ALPHA, PlaneAxis, PlaneView};
use vector_field_explorer::viewmodel::{
    ArrowLengthMode, DEFAULT_PARTICLE_SIZE, LeftClickAction, State, StreamlineSeedMode,
    DEFAULT_DENSITY_2D, MAX_ARROW_OPACITY, MAX_DENSITY_3D, MAX_DOMAIN_EXTENT,
    MAX_PARTICLE_SIZE, MAX_PLANE_DIM_OPACITY, MAX_PROPORTIONAL_EXPONENT, MIN_ARROW_OPACITY,
    MIN_DOMAIN_EXTENT, MIN_PARTICLE_SIZE, domain_between,
};
use glam::Vec3;

#[test]
fn sidebar_defaults_mirror_the_seam_defaults() {
    let c = Controls::new();
    let s = State::new(false);
    assert_eq!(c.density, s.density, "density starts at the seam default");
    assert_eq!(c.density, DEFAULT_DENSITY_2D, "…and that default is pinned");
    assert_eq!(c.arrow_scale, s.arrow_scale, "arrow scale agrees");
    assert_eq!(c.arrow_opacity, s.arrow_opacity, "the global arrow opacity agrees");
    assert_eq!(c.arrow_opacity, 1.0, "arrows start fully opaque");
    assert_eq!(c.plane_dim_opacity, s.plane_dim_opacity, "the off-plane opacity agrees");
    assert_eq!(c.plane_dim_opacity, DIM_ALPHA, "off-plane starts at the dim alpha");
    assert_eq!(c.arrow_length_mode, s.arrow_length_mode, "arrow length mode agrees");
    assert_eq!(c.magnitude_scale, s.magnitude_scale, "magnitude scale agrees");
    assert_eq!(
        c.proportional_exponent, s.proportional_exponent,
        "the proportional contrast agrees",
    );
    assert_eq!(c.arrows, s.arrows, "arrows start visible");
    assert_eq!(c.arrow_style, s.arrow_style, "arrow style agrees");
    assert_eq!(c.arrow_style_2d, s.arrow_style_2d, "2D arrow style agrees");
    assert_eq!(c.arrow_params, s.arrow_params, "arrow appearance agrees");
    assert_eq!(c.colormap, s.colormap, "colormap agrees");
    assert_eq!(c.auto_fit, s.auto_fit, "auto-fit agrees");
    assert_eq!(c.color_min, s.color_min, "color min agrees");
    assert_eq!(c.color_max, s.color_max, "color max agrees");
    assert_eq!(c.streamlines, s.streamlines, "streamlines start off");
    assert_eq!(c.seed_mode, s.seed_mode, "the seed mode starts with the seam's");
    assert_eq!(
        c.left_click_action, s.left_click_action,
        "the left-click action starts with the seam's",
    );
    assert_eq!(c.particles, s.particles, "particles start off");
    assert_eq!(c.domain_min, s.domain.min, "domain min agrees");
    assert_eq!(c.domain_max, s.domain.max, "domain max agrees");
    assert_eq!(c.plane, s.plane, "the plane mirror starts where the seam does");
    assert!(!c.fly, "fly mode starts off; the app mirrors the active camera into it");
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
    assert!(!s.set_arrow_opacity(c.arrow_opacity));
    assert!(!s.set_plane_dim_opacity(c.plane_dim_opacity));
    assert!(!s.set_arrow_length_mode(c.arrow_length_mode));
    assert!(!s.set_magnitude_scale(c.magnitude_scale));
    assert!(!s.set_proportional_exponent(c.proportional_exponent));
    assert!(!s.set_arrows(c.arrows));
    assert!(!s.set_arrow_style(c.arrow_style));
    assert!(!s.set_arrow_style_2d(c.arrow_style_2d));
    assert!(!s.set_arrow_params(c.arrow_params));
    assert!(!s.set_colormap(c.colormap));
    assert!(!s.set_auto_fit(c.auto_fit));
    assert!(!s.set_color_range(c.color_min, c.color_max));
    assert!(!s.set_streamlines(c.streamlines));
    assert!(!s.set_seed_mode(c.seed_mode), "synced seed mode does not re-dirty");
    assert!(
        !s.set_left_click_action(c.left_click_action),
        "synced left-click action does not re-dirty",
    );
    assert!(!s.set_particles(c.particles));
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
    c.arrow_opacity = 0.5;
    apply(&mut s, &mut c);
    assert!(s.snapshot_if_dirty().is_some(), "arrow opacity dirties the scene");
    c.plane_dim_opacity = 0.4;
    apply(&mut s, &mut c);
    assert!(s.snapshot_if_dirty().is_some(), "the off-plane opacity dirties the scene");
    c.arrow_length_mode = ArrowLengthMode::Magnitude;
    apply(&mut s, &mut c);
    assert!(s.snapshot_if_dirty().is_some(), "arrow length mode dirties the scene");
    c.magnitude_scale = 2.0;
    apply(&mut s, &mut c);
    assert!(s.snapshot_if_dirty().is_some(), "magnitude scale dirties the scene");
    c.proportional_exponent = 2.0;
    apply(&mut s, &mut c);
    assert!(s.snapshot_if_dirty().is_some(), "the proportional contrast dirties the scene");
    c.arrows = false;
    apply(&mut s, &mut c);
    assert!(s.snapshot_if_dirty().is_some(), "the Show arrows toggle dirties the scene");
    c.arrow_style = ArrowStyle::Cylinder;
    apply(&mut s, &mut c);
    assert!(s.snapshot_if_dirty().is_some(), "the arrow style dirties the scene");
    c.arrow_params.head_length = 0.45;
    apply(&mut s, &mut c);
    assert!(s.snapshot_if_dirty().is_some(), "arrow appearance dirties the scene");
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
    c.seed_mode = StreamlineSeedMode::Click;
    apply(&mut s, &mut c);
    assert!(s.snapshot_if_dirty().is_some(), "the seed mode dirties the scene");
    c.particles = true;
    apply(&mut s, &mut c);
    assert!(s.snapshot_if_dirty().is_some(), "particles stub still flows state");
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
fn proportional_exponent_is_enabled_only_in_proportional_mode() {
    assert!(
        proportional_exponent_enabled(ArrowLengthMode::Proportional),
        "Proportional is the one mode the contrast applies to",
    );
    assert!(
        !proportional_exponent_enabled(ArrowLengthMode::Uniform),
        "Uniform arrows do not follow magnitude",
    );
    assert!(
        !proportional_exponent_enabled(ArrowLengthMode::Magnitude),
        "Magnitude length is linear in the raw magnitude",
    );
}

#[test]
fn proportional_exponent_flows_through_apply_and_the_mirror_stays_in_sync() {
    let mut c = Controls::new();
    let mut s = State::new(false);
    assert!(s.snapshot_if_dirty().is_some(), "consume the fresh scene");

    c.proportional_exponent = 2.0;
    apply(&mut s, &mut c);
    assert_eq!(s.proportional_exponent, 2.0, "the contrast lands on the seam");
    assert_eq!(c.proportional_exponent, 2.0, "the mirror follows");

    c.proportional_exponent = 1_000.0;
    apply(&mut s, &mut c);
    assert_eq!(s.proportional_exponent, MAX_PROPORTIONAL_EXPONENT, "the seam clamps");
    assert_eq!(c.proportional_exponent, MAX_PROPORTIONAL_EXPONENT, "the mirror shows the clamp");
}

#[test]
fn display_reset_restores_proportional_and_both_scales() {
    let mut c = Controls::new();
    c.arrow_length_mode = ArrowLengthMode::Magnitude;
    c.arrow_scale = 3.5;
    c.magnitude_scale = 4.0;
    c.proportional_exponent = 3.0;

    c.reset_display();

    assert_eq!(c.arrow_length_mode, ArrowLengthMode::Proportional, "the default mode returns");
    assert_eq!(c.arrow_scale, 1.0, "the default arrow scale returns");
    assert_eq!(c.magnitude_scale, 1.0, "the default magnitude scale returns");
    assert_eq!(c.proportional_exponent, 1.0, "the default proportional contrast returns");
}

#[test]
fn display_reset_restores_arrow_visibility_style_and_appearance() {
    let mut c = Controls::new();
    c.arrows = false;
    c.arrow_style = ArrowStyle::Streamline;
    c.arrow_style_2d = ArrowStyle2D::Streamline;
    c.arrow_params = ArrowParams { head_length: 0.5, head_width: 0.3, shaft_radius: 0.12, sides: 4 };

    c.reset_display();

    assert!(c.arrows, "arrows return");
    assert_eq!(c.arrow_style, ArrowStyle::Cone, "the default style returns");
    assert_eq!(c.arrow_style_2d, ArrowStyle2D::Flat, "the default 2D style returns");
    assert_eq!(c.arrow_params, ArrowParams::default(), "the default appearance returns");
}

// --- Arrow and off-plane opacity (ticket 36) -------------------------------

#[test]
fn arrow_and_plane_opacities_flow_through_apply_and_the_mirror_stays_in_sync() {
    let mut c = Controls::new();
    let mut s = State::new(true);
    assert!(s.snapshot_if_dirty().is_some(), "consume the fresh scene");

    c.arrow_opacity = 0.35;
    c.plane_dim_opacity = 0.7;
    apply(&mut s, &mut c);
    assert_eq!(s.arrow_opacity, 0.35, "the global opacity lands on the seam");
    assert_eq!(s.plane_dim_opacity, 0.7, "the off-plane opacity lands too");
    assert_eq!(c.arrow_opacity, s.arrow_opacity, "the mirror follows the global opacity");
    assert_eq!(c.plane_dim_opacity, s.plane_dim_opacity, "the mirror follows the off-plane one");
    assert!(s.snapshot_if_dirty().is_some(), "the edit rebuilds the scene once");

    c.arrow_opacity = 0.0;
    c.plane_dim_opacity = 5.0;
    apply(&mut s, &mut c);
    assert_eq!(s.arrow_opacity, MIN_ARROW_OPACITY, "the seam clamps the global opacity floor");
    assert_eq!(s.plane_dim_opacity, MAX_PLANE_DIM_OPACITY, "the seam clamps the off-plane ceiling");
    assert_eq!(c.arrow_opacity, MIN_ARROW_OPACITY, "the mirror shows the clamp");
    assert_eq!(c.plane_dim_opacity, MAX_PLANE_DIM_OPACITY);

    c.arrow_opacity = f32::NAN;
    c.plane_dim_opacity = f32::INFINITY;
    apply(&mut s, &mut c);
    assert_eq!(s.arrow_opacity, MAX_ARROW_OPACITY, "NaN falls back to full opacity");
    assert_eq!(s.plane_dim_opacity, DIM_ALPHA, "Infinity falls back to the dim alpha");
    assert_eq!(c.arrow_opacity, MAX_ARROW_OPACITY, "the mirror follows the fallback");
    assert_eq!(c.plane_dim_opacity, DIM_ALPHA);
}

#[test]
fn display_reset_restores_the_arrow_and_off_plane_opacities() {
    let mut c = Controls::new();
    c.arrow_opacity = 0.25;
    c.plane_dim_opacity = 0.75;

    c.reset_display();

    assert_eq!(c.arrow_opacity, 1.0, "the global opacity returns to fully opaque");
    assert_eq!(c.plane_dim_opacity, DIM_ALPHA, "the off-plane opacity returns to 0.2");
}



#[test]
fn the_sidebar_shows_the_matching_arrow_set() {
    assert_eq!(arrow_style_set(true), ArrowStyleSet::ThreeD, "3D offers the 3D families");
    assert_eq!(arrow_style_set(false), ArrowStyleSet::TwoD, "2D offers the 2D families");
    assert_eq!(ArrowStyleSet::ThreeD.label(), "Arrow style");
    assert_eq!(ArrowStyleSet::TwoD.label(), "2D arrow style");
    assert_eq!(ArrowStyle::ALL.len(), 4, "the 3D selector keeps its four families");
    assert_eq!(ArrowStyle::ALL.map(ArrowStyle::label), ["Cone", "Cylinder", "Flat", "Line"]);
    assert_eq!(
        ArrowStyle2D::ALL,
        [ArrowStyle2D::Flat, ArrowStyle2D::Streamline],
        "the 2D selector offers Flat and Line",
    );
    assert_eq!(ArrowStyle2D::ALL.map(ArrowStyle2D::label), ["Flat", "Line"]);
}

#[test]
fn sides_are_enabled_only_for_families_that_use_them() {
    assert!(
        !sides_control_enabled(false, ArrowStyle::Cone, ArrowStyle2D::Flat),
        "the 2D flat arrow has a fixed silhouette",
    );
    assert!(sides_control_enabled(true, ArrowStyle::Cone, ArrowStyle2D::Flat));
    assert!(sides_control_enabled(true, ArrowStyle::Cylinder, ArrowStyle2D::Flat));
    assert!(!sides_control_enabled(true, ArrowStyle::Flat, ArrowStyle2D::Flat));
    assert!(!sides_control_enabled(true, ArrowStyle::Streamline, ArrowStyle2D::Flat));
    assert!(
        !sides_control_enabled(false, ArrowStyle::Cone, ArrowStyle2D::Streamline),
        "the 2D line has a fixed silhouette too",
    );
}

#[test]
fn head_controls_are_enabled_only_for_families_with_a_head() {
    assert!(head_control_enabled(true, ArrowStyle::Cone, ArrowStyle2D::Flat));
    assert!(head_control_enabled(true, ArrowStyle::Flat, ArrowStyle2D::Flat));
    assert!(!head_control_enabled(true, ArrowStyle::Cylinder, ArrowStyle2D::Flat));
    assert!(!head_control_enabled(true, ArrowStyle::Streamline, ArrowStyle2D::Flat));

    assert!(head_control_enabled(false, ArrowStyle::Cone, ArrowStyle2D::Flat));
    assert!(!head_control_enabled(false, ArrowStyle::Cone, ArrowStyle2D::Streamline));
}

#[test]
fn switching_views_preserves_both_arrow_selections_through_apply() {
    let mut c = Controls::new();
    let mut s = State::new(false);
    assert!(s.snapshot_if_dirty().is_some(), "consume the fresh scene");

    c.arrow_style = ArrowStyle::Cylinder;
    c.arrow_style_2d = ArrowStyle2D::Streamline;
    apply(&mut s, &mut c);
    assert_eq!(s.arrow_style, ArrowStyle::Cylinder);
    assert_eq!(s.arrow_style_2d, ArrowStyle2D::Streamline);

    assert!(s.set_mode(true), "switch to 3D");
    apply(&mut s, &mut c);
    assert_eq!(s.arrow_style, ArrowStyle::Cylinder, "the 3D selection survives the view switch");
    assert_eq!(s.arrow_style_2d, ArrowStyle2D::Streamline, "the 2D selection survives too");
    assert_eq!(c.arrow_style, ArrowStyle::Cylinder, "the mirror keeps the 3D family");
    assert_eq!(c.arrow_style_2d, ArrowStyle2D::Streamline, "the mirror keeps the 2D family");
    assert_eq!(arrow_style_set(s.mode_3d), ArrowStyleSet::ThreeD);

    assert!(s.set_mode(false), "switch back to 2D");
    apply(&mut s, &mut c);
    assert_eq!(s.arrow_style, ArrowStyle::Cylinder, "the 3D selection still survives");
    assert_eq!(s.arrow_style_2d, ArrowStyle2D::Streamline, "the 2D selection still survives");
    assert_eq!(arrow_style_set(s.mode_3d), ArrowStyleSet::TwoD);
}

#[test]
fn changing_one_arrow_set_never_changes_the_other_through_apply() {
    let mut c = Controls::new();
    let mut s = State::new(false);
    assert!(s.snapshot_if_dirty().is_some(), "consume the fresh scene");

    c.arrow_style = ArrowStyle::Flat;
    apply(&mut s, &mut c);
    assert_eq!(s.arrow_style_2d, ArrowStyle2D::Flat, "the 2D set is untouched");

    c.arrow_style_2d = ArrowStyle2D::Streamline;
    apply(&mut s, &mut c);
    assert_eq!(s.arrow_style, ArrowStyle::Flat, "the 3D set is untouched");
    assert_eq!(c.arrow_style, ArrowStyle::Flat);
    assert_eq!(c.arrow_style_2d, ArrowStyle2D::Streamline);
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

// --- 2D domain rows and streamline seed modes (ticket 50) -------------------

#[test]
fn domain_axes_render_x_and_y_in_2d_and_add_z_in_3d() {
    assert_eq!(
        domain_axes(false),
        &[DomainAxis::X, DomainAxis::Y],
        "2D shows no z row at all, not a greyed one",
    );
    assert_eq!(
        domain_axes(true),
        &[DomainAxis::X, DomainAxis::Y, DomainAxis::Z],
        "3D restores the z row",
    );
    assert_eq!(
        [DomainAxis::X, DomainAxis::Y, DomainAxis::Z].map(DomainAxis::label),
        ["x", "y", "z"],
    );
}

#[test]
fn domain_summary_hides_a_changed_z_in_2d_and_shows_it_in_3d() {
    let domain = domain_between(Vec3::new(-2.0, -3.0, 4.0), Vec3::new(2.0, 3.0, 9.0));
    assert_eq!(
        domain_summary(&domain, false),
        "x -2…2 · y -3…3",
        "the 2D summary never advertises the stored z extent",
    );
    assert_eq!(
        domain_summary(&domain, true),
        "x -2…2 · y -3…3 · z 4…9",
        "3D brings the stored z extent back",
    );
}

#[test]
fn stored_z_survives_the_2d_to_3d_switch() {
    let mut s = State::new(true);
    assert!(
        s.set_domain(Vec3::new(-1.0, -1.0, -4.0), Vec3::new(1.0, 1.0, 4.0)),
        "a non-default z extent is stored",
    );
    assert!(s.set_mode(false), "switch to 2D");
    assert_eq!((s.domain.min.z, s.domain.max.z), (-4.0, 4.0), "2D keeps the stored z");
    assert_eq!(
        domain_axes(s.is_3d()),
        &[DomainAxis::X, DomainAxis::Y],
        "the 2D editor cannot touch z",
    );
    assert!(s.set_mode(true), "switch back to 3D");
    assert_eq!((s.domain.min.z, s.domain.max.z), (-4.0, 4.0), "the same z returns in 3D");
    assert_eq!(
        domain_axes(s.is_3d()),
        &[DomainAxis::X, DomainAxis::Y, DomainAxis::Z],
        "and so does its row",
    );
}

#[test]
fn seed_selector_is_enabled_only_while_streamlines_are_on() {
    assert!(!seed_mode_enabled(false), "Streamlines off disables the Auto/Click selector");
    assert!(seed_mode_enabled(true), "Streamlines on enables it");
}

#[test]
fn seed_mode_defaults_to_auto_and_flows_through_apply() {
    let mut c = Controls::new();
    let mut s = State::new(false);
    assert_eq!(c.seed_mode, StreamlineSeedMode::Auto, "the selector starts on Auto");
    assert!(s.snapshot_if_dirty().is_some(), "consume the fresh scene");

    c.seed_mode = StreamlineSeedMode::Click;
    apply(&mut s, &mut c);
    assert_eq!(s.seed_mode, StreamlineSeedMode::Click, "Click lands on the seam");
    assert_eq!(c.seed_mode, s.seed_mode, "the mirror follows the seam");
    assert!(s.snapshot_if_dirty().is_some(), "a seed-mode change re-instances the scene");
}

#[test]
fn features_reset_returns_the_seed_mode_to_auto() {
    let mut c = Controls::new();
    c.streamlines = true;
    c.seed_mode = StreamlineSeedMode::Click;

    c.reset_features();

    assert!(!c.streamlines, "the Features reset keeps hiding streamlines");
    assert_eq!(c.seed_mode, StreamlineSeedMode::Auto, "and returns the seed mode to Auto");
}

#[test]
fn left_click_action_defaults_and_flows_through_apply() {
    let mut c = Controls::new();
    let mut s = State::new(false);
    assert_eq!(
        c.left_click_action,
        LeftClickAction::SeedStreamline,
        "the row starts on Seed streamline",
    );
    assert!(s.snapshot_if_dirty().is_some(), "consume the fresh scene");

    c.left_click_action = LeftClickAction::AddParticles;
    apply(&mut s, &mut c);
    assert_eq!(s.left_click_action, LeftClickAction::AddParticles, "the choice lands on the seam");
    assert_eq!(c.left_click_action, s.left_click_action, "the mirror follows the seam");
}

#[test]
fn left_click_action_stays_available_while_its_feature_is_off() {
    // The row is never hidden or disabled by the feature toggles: it keeps both
    // choices visible and the click is a hinted no-op instead.
    assert_eq!(LeftClickAction::ALL.len(), 2, "both choices stay on the row");
    assert!(!Controls::new().streamlines, "the default Features state hides streamlines");
    assert!(!Controls::new().particles, "and particles");
}

#[test]
fn features_reset_returns_the_left_click_action_to_seed_streamline() {
    let mut c = Controls::new();
    c.left_click_action = LeftClickAction::AddParticles;

    c.reset_features();

    assert_eq!(
        c.left_click_action,
        LeftClickAction::SeedStreamline,
        "Reset Features restores Seed streamline",
    );
}

// --- Ticket 30: particle size and count controls -----------------------

#[test]
fn particle_controls_default_to_the_seam_defaults() {
    let c = Controls::new();
    let s = State::new(false);
    assert_eq!(c.particle_size, s.particle_size, "the size mirror starts at the seam's");
    assert_eq!(c.particle_size, DEFAULT_PARTICLE_SIZE, "…and that default is 1.0");
    assert_eq!(c.particle_count, s.particle_count, "the count mirror starts at the seam's");
    assert_eq!(c.particle_count, 0, "…and that default is Auto");
}

#[test]
fn particle_size_and_count_flow_through_apply_and_the_mirror_stays_in_sync() {
    let mut c = Controls::new();
    let mut s = State::new(true);
    assert!(s.set_particles(true));
    assert!(s.snapshot_if_dirty().is_some(), "consume the fresh scene");
    c.density = s.density;
    c.particles = s.particles;

    c.particle_size = 2.5;
    c.particle_count = 250;
    apply(&mut s, &mut c);

    assert_eq!(s.particle_size, 2.5, "the size lands on the seam");
    assert_eq!(s.particle_count, 250, "the count lands on the seam");
    assert_eq!(c.particle_size, s.particle_size, "the mirror follows the size");
    assert_eq!(c.particle_count, s.particle_count, "the mirror follows the count");
    assert!(s.snapshot_if_dirty().is_none(), "neither control rebuilds the scene");

    c.particle_size = 9.0;
    c.particle_count = 10_000;
    apply(&mut s, &mut c);
    assert_eq!(s.particle_size, MAX_PARTICLE_SIZE, "the seam clamps an oversized dot");
    assert_eq!(c.particle_size, MAX_PARTICLE_SIZE, "the mirror shows the clamped size");
    assert_eq!(s.particle_count, MAX_PARTICLES as u32, "the seam caps the count");
    assert_eq!(c.particle_count, MAX_PARTICLES as u32, "the mirror shows the capped count");
}

#[test]
fn particle_count_label_shows_auto_at_zero() {
    assert_eq!(particle_count_label(0), "Auto", "0 reads as Auto in the drag value");
    assert_eq!(particle_count_label(100), "100", "an explicit count shows its number");
    assert_eq!(particle_count_label(MAX_PARTICLES as u32), "4000");
}

#[test]
fn particle_size_is_enabled_only_while_particles_are_on() {
    assert!(!particle_size_enabled(false), "Particles off greys the size slider");
    assert!(particle_size_enabled(true), "Particles on enables it");
}

#[test]
fn features_reset_restores_particle_size_and_count() {
    let mut c = Controls::new();
    c.particles = true;
    c.particle_size = 2.75;
    c.particle_count = 320;

    c.reset_features();

    assert_eq!(c.particle_size, DEFAULT_PARTICLE_SIZE, "reset restores size 1.0");
    assert_eq!(c.particle_count, 0, "reset restores Auto");
    assert!(!c.particles, "reset still turns Particles off");
    assert!(
        (MIN_PARTICLE_SIZE..=MAX_PARTICLE_SIZE).contains(&c.particle_size),
        "the restored size is inside the documented range",
    );
}

// --- Ticket 48: particle appearance controls ----------------------------

#[test]
fn particle_appearance_defaults_mirror_the_seam_and_the_ticket() {
    let c = Controls::new();
    let s = State::new(false);
    assert_eq!(
        c.particle_appearance, s.particle_appearance,
        "the appearance mirror starts at the seam's defaults",
    );
    assert_eq!(c.particle_appearance, ParticleAppearance::default());
    assert_eq!(c.particle_appearance.opacity, 1.0);
    assert_eq!(c.particle_appearance.color_mode, ParticleColorMode::Magnitude);
    assert_eq!(c.particle_appearance.trail_len, 0, "trails start off");
    assert_eq!(c.particle_appearance.trail_fade, 0.65);
    assert_eq!(c.particle_appearance.trail_width, 1.0);
    assert!(c.particle_appearance.loop_on_stagnation);
    assert_eq!(c.particle_appearance.stagnation_seconds, 0.5);
}

#[test]
fn particle_appearance_flows_through_apply_and_the_mirror_stays_in_sync() {
    let mut c = Controls::new();
    let mut s = State::new(true);
    assert!(s.set_particles(true));
    assert!(s.snapshot_if_dirty().is_some(), "consume the fresh scene");
    c.density = s.density;
    c.particles = s.particles;
    c.particle_appearance = ParticleAppearance {
        opacity: 0.4,
        color_mode: ParticleColorMode::Fixed,
        fixed_color: [7, 8, 9, 255],
        trail_len: 12,
        trail_fade: 0.5,
        trail_width: 1.5,
        loop_on_stagnation: false,
        stagnation_seconds: 2.0,
    };

    apply(&mut s, &mut c);

    assert_eq!(s.particle_appearance, c.particle_appearance, "the values land on the seam");
    assert_eq!(c.particle_appearance, s.particle_appearance, "and the mirror follows");
    assert!(s.snapshot_if_dirty().is_none(), "appearance never rebuilds the scene");

    c.particle_appearance.opacity = -1.0;
    c.particle_appearance.trail_len = 999;
    c.particle_appearance.stagnation_seconds = f32::NAN;
    apply(&mut s, &mut c);
    assert_eq!(s.particle_appearance.opacity, 0.05, "opacity clamps to its floor");
    assert_eq!(s.particle_appearance.trail_len, 32, "trail length clamps to the ring size");
    assert_eq!(s.particle_appearance.stagnation_seconds, 0.5, "a NaN threshold falls back");
    assert_eq!(c.particle_appearance.opacity, s.particle_appearance.opacity);
}

#[test]
fn features_reset_restores_the_whole_particle_appearance() {
    let mut c = Controls::new();
    c.particles = true;
    c.particle_size = 2.75;
    c.particle_count = 320;
    c.particle_appearance = ParticleAppearance {
        opacity: 0.2,
        color_mode: ParticleColorMode::Speed,
        fixed_color: [1, 2, 3, 4],
        trail_len: 20,
        trail_fade: 0.1,
        trail_width: 1.8,
        loop_on_stagnation: false,
        stagnation_seconds: 4.0,
    };

    c.reset_features();

    assert_eq!(c.particle_appearance, ParticleAppearance::default());
    assert_eq!(c.particle_size, DEFAULT_PARTICLE_SIZE, "ticket 30's size still resets");
    assert_eq!(c.particle_count, 0, "ticket 30's count still resets");
    assert!(!c.particles, "the Feature toggle still resets off");
}

#[test]
fn appearance_controls_enable_only_when_they_can_do_something() {
    assert!(!particle_fixed_color_enabled(ParticleColorMode::Magnitude));
    assert!(!particle_fixed_color_enabled(ParticleColorMode::Speed));
    assert!(particle_fixed_color_enabled(ParticleColorMode::Fixed));

    assert!(!trail_fade_enabled(0));
    assert!(!trail_fade_enabled(1));
    assert!(trail_fade_enabled(2));
    assert!(!trail_width_enabled(0));
    assert!(!trail_width_enabled(1));
    assert!(trail_width_enabled(2));

    assert!(!stagnation_seconds_enabled(false));
    assert!(stagnation_seconds_enabled(true));
}

#[test]
fn fixed_colour_picker_round_trip_stores_opaque_rgb_unchanged() {
    // The stored vec4 may arrive translucent from an older edit; the picker
    // edits RGB only and stores the same RGB as fully opaque.
    let stored = [10u8, 20, 30, 7];
    let picker = fixed_color_rgb(stored);
    assert_eq!(picker, [10, 20, 30], "the picker sees the true RGB");
    let round_trip = fixed_color_opaque(picker);
    assert_eq!(round_trip, [10, 20, 30, 255], "alpha is forced opaque");
}
