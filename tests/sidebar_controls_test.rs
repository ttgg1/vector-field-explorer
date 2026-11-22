// The sidebar owns a plain mirror of the scene knobs (`Controls`): pure data
// that the widgets edit between frames and that `show` hands to the viewmodel
// setters once per frame. The defaults must agree with the seam's or the first
// sidebar frame would change settings the user never touched (and the status
// bar would count a scene the user never asked for).

use eframe::egui;

use crate::support;

use glam::Vec3;
use vector_field_explorer::arrow_style::{ArrowParams, ArrowStyle, ArrowStyle2D};
use vector_field_explorer::colormap::Colormap;
use vector_field_explorer::domain::Domain;
use vector_field_explorer::parameters::{PARAMETER_VERSION, Parameter, ParameterSet};
use vector_field_explorer::particles::{MAX_PARTICLES, ParticleAppearance, ParticleColorMode};
use vector_field_explorer::plane::{DIM_ALPHA, PlaneAxis, PlaneView};
use vector_field_explorer::plot::PlotKind;
use vector_field_explorer::scalar::ScalarContext;
use vector_field_explorer::scalar_analysis::{
    ComparisonSeries, ComparisonSet, SeriesId, SeriesRole, SeriesStyle,
};
use vector_field_explorer::scalar_view::UNSUPPORTED_Z_REASON;
use vector_field_explorer::seed_layout::{SeedLayout, SeedPoint};
use vector_field_explorer::seed_layout_library::{
    DEFAULT_LAYOUT_NAME, SEED_LAYOUT_FORMAT_VERSION, SeedLayoutDocument, SeedLayoutLibrary,
    SeedLayoutProfile,
};
use vector_field_explorer::source_scene::FieldMode;
use vector_field_explorer::ui::sidebar::{
    self, ArrowStyleSet, Controls, DomainAxis, PlaneControlMode, SEED_LAYOUT_DENSITY_INACTIVE_NOTE,
    SeriesDraft, apply, arrow_scale_enabled, arrow_style_set, commit_variables, domain_axes,
    domain_summary, fixed_color_opaque, fixed_color_rgb, head_control_enabled,
    magnitude_scale_enabled, particle_count_label, particle_fixed_color_enabled,
    particle_size_enabled, plane_control_mode, proportional_exponent_enabled,
    seed_layout_count_label, seed_layout_visible, seed_mode_enabled, sides_control_enabled,
    stagnation_seconds_enabled, streamline_count_enabled, streamline_count_enabled_with_layout,
    streamline_count_label, trail_fade_enabled, trail_width_enabled, variables_visible,
};
use vector_field_explorer::viewmodel::{
    ArrowLengthMode, DEFAULT_DENSITY_2D, DEFAULT_PARTICLE_SIZE, LeftClickAction, MAX_ARROW_OPACITY,
    MAX_DENSITY_3D, MAX_DOMAIN_EXTENT, MAX_PARTICLE_SIZE, MAX_PLANE_DIM_OPACITY,
    MAX_PROPORTIONAL_EXPONENT, MIN_ARROW_OPACITY, MIN_DOMAIN_EXTENT, MIN_PARTICLE_SIZE, State,
    StreamlineSeedMode, domain_between,
};

#[test]
fn flow_section_controls_follow_the_explicit_layout_state() {
    assert!(
        seed_layout_visible(FieldMode::Formula, true, true),
        "the Seed layout area belongs to a formula streamline view"
    );
    assert!(
        !seed_layout_visible(FieldMode::Formula, false, true),
        "Streamlines off hides the seed editor"
    );
    assert!(
        !seed_layout_visible(FieldMode::Formula, true, false),
        "plot mode has no vector seed editor"
    );
    for mode in [
        FieldMode::Sources,
        FieldMode::Induction,
        FieldMode::Materials,
        FieldMode::Maxwell,
        FieldMode::Ode,
    ] {
        assert!(
            !seed_layout_visible(mode, true, true),
            "{mode:?} owns its own seeding and hides the layout editor"
        );
    }

    assert!(
        streamline_count_enabled_with_layout(true, StreamlineSeedMode::Auto, true),
        "the automatic count applies while the layout is empty"
    );
    assert!(
        !streamline_count_enabled_with_layout(true, StreamlineSeedMode::Auto, false),
        "explicit seeds disable the automatic count control"
    );
    assert!(
        !streamline_count_enabled_with_layout(true, StreamlineSeedMode::Click, true),
        "Click-only seeding still has no automatic count"
    );

    assert_eq!(seed_layout_count_label(0), "No explicit seeds");
    assert_eq!(seed_layout_count_label(3), "3 explicit seeds");
    assert!(
        SEED_LAYOUT_DENSITY_INACTIVE_NOTE.contains("inactive"),
        "the note tells the user the count control is inactive"
    );
}

#[test]
fn reference_visibility_defaults_and_setters_are_independent() {
    for mode_3d in [false, true] {
        let mut state = State::new(mode_3d);
        assert!(state.show_grid, "the grid starts visible in either mode");
        assert!(
            state.show_coordinate_system,
            "the axes start visible in either mode"
        );
        assert!(
            state.snapshot_if_dirty().is_some(),
            "consume the initial scene"
        );

        assert!(
            !state.set_show_grid(true),
            "repeating the grid default is a no-op"
        );
        assert!(
            !state.set_show_coordinate_system(true),
            "repeating the axes default is a no-op"
        );
        assert!(!state.dirty);
        assert!(state.snapshot_if_dirty().is_none());

        assert!(state.set_show_grid(false));
        assert!(!state.show_grid);
        assert!(
            state.show_coordinate_system,
            "grid visibility does not change axes"
        );
        assert!(state.dirty);
        assert!(
            state.snapshot_if_dirty().is_some(),
            "grid change rebuilds once"
        );
        assert!(state.snapshot_if_dirty().is_none());
        assert!(!state.set_show_grid(false));
        assert!(!state.dirty, "a repeated grid choice does not rebuild");

        assert!(state.set_show_coordinate_system(false));
        assert!(!state.show_grid, "axes visibility does not change grid");
        assert!(!state.show_coordinate_system);
        assert!(state.dirty);
        assert!(
            state.snapshot_if_dirty().is_some(),
            "axes change rebuilds once"
        );
        assert!(state.snapshot_if_dirty().is_none());
        assert!(!state.set_show_coordinate_system(false));
        assert!(!state.dirty, "a repeated axes choice does not rebuild");

        assert!(state.set_show_grid(true));
        assert!(
            !state.show_coordinate_system,
            "restoring grid does not restore axes"
        );
        assert!(state.snapshot_if_dirty().is_some());
        assert!(state.set_show_coordinate_system(true));
        assert!(state.show_grid, "restoring axes does not hide grid");
        assert!(state.snapshot_if_dirty().is_some());
    }
}

#[test]
fn reference_visibility_is_shared_across_mode_switches() {
    let mut state = State::new(false);
    assert!(state.set_density(4), "keep the sampling comparison small");
    let options_before = state.view_options();
    let arrows_before = state.sample();
    assert!(!arrows_before.is_empty(), "compare actual field samples");

    assert!(state.set_show_grid(false));
    assert!(state.set_show_coordinate_system(false));
    assert_eq!(
        state.view_options(),
        options_before,
        "visibility never changes sampling options"
    );
    assert_eq!(
        state.sample(),
        arrows_before,
        "visibility never changes sampled arrows"
    );

    assert!(state.set_formula("(x, y, z)".into()));
    assert!(state.set_mode(true));
    assert!(state.is_3d());
    assert!(!state.show_grid);
    assert!(!state.show_coordinate_system);

    let options_3d = state.view_options();
    let arrows_3d = state.sample();
    assert!(!arrows_3d.is_empty());
    assert!(state.set_show_grid(true));
    assert_eq!(state.view_options(), options_3d);
    assert_eq!(
        state.sample(),
        arrows_3d,
        "grid visibility cannot change 3D samples"
    );
    assert!(state.set_show_coordinate_system(true));
    assert_eq!(state.view_options(), options_3d);
    assert_eq!(
        state.sample(),
        arrows_3d,
        "axes visibility cannot change 3D samples"
    );

    assert!(state.set_mode(false));
    assert!(state.show_grid, "the 3D grid choice persists in 2D");
    assert!(
        state.show_coordinate_system,
        "the 3D axes choice persists in 2D"
    );
}

#[test]
fn sidebar_defaults_mirror_the_seam_defaults() {
    let c = Controls::new();
    let s = State::new(false);
    assert_eq!(c.density, s.density, "density starts at the seam default");
    assert_eq!(c.density, DEFAULT_DENSITY_2D, "…and that default is pinned");
    assert_eq!(c.arrow_scale, s.arrow_scale, "arrow scale agrees");
    assert_eq!(
        c.arrow_opacity, s.arrow_opacity,
        "the global arrow opacity agrees"
    );
    assert_eq!(c.arrow_opacity, 1.0, "arrows start fully opaque");
    assert_eq!(
        c.plane_dim_opacity, s.plane_dim_opacity,
        "the off-plane opacity agrees"
    );
    assert_eq!(
        c.plane_dim_opacity, DIM_ALPHA,
        "off-plane starts at the dim alpha"
    );
    assert_eq!(
        c.arrow_length_mode, s.arrow_length_mode,
        "arrow length mode agrees"
    );
    assert_eq!(
        c.magnitude_scale, s.magnitude_scale,
        "magnitude scale agrees"
    );
    assert_eq!(
        c.proportional_exponent, s.proportional_exponent,
        "the proportional contrast agrees",
    );
    assert_eq!(c.arrows, s.arrows, "arrows start visible");
    assert!(c.show_grid, "the grid starts visible");
    assert_eq!(c.show_grid, s.show_grid, "grid visibility agrees");
    assert!(c.show_coordinate_system, "the axes start visible");
    assert_eq!(
        c.show_coordinate_system, s.show_coordinate_system,
        "axes visibility agrees"
    );
    assert_eq!(c.arrow_style, s.arrow_style, "arrow style agrees");
    assert_eq!(c.arrow_style_2d, s.arrow_style_2d, "2D arrow style agrees");
    assert_eq!(c.arrow_params, s.arrow_params, "arrow appearance agrees");
    assert_eq!(c.colormap, s.colormap, "colormap agrees");
    assert_eq!(c.auto_fit, s.auto_fit, "auto-fit agrees");
    assert_eq!(c.color_min, s.color_min, "color min agrees");
    assert_eq!(c.color_max, s.color_max, "color max agrees");
    assert_eq!(c.streamlines, s.streamlines, "streamlines start off");
    assert_eq!(
        c.seed_mode, s.seed_mode,
        "the seed mode starts with the seam's"
    );
    assert_eq!(
        c.left_click_action, s.left_click_action,
        "the left-click action starts with the seam's",
    );
    assert_eq!(c.particles, s.particles, "particles start off");
    assert_eq!(c.domain_min, s.domain.min, "domain min agrees");
    assert_eq!(c.domain_max, s.domain.max, "domain max agrees");
    assert_eq!(
        c.plane, s.plane,
        "the plane mirror starts where the seam does"
    );
    assert!(
        !c.fly,
        "fly mode starts off; the app mirrors the active camera into it"
    );
}

#[test]
fn apply_never_commits_the_plane_mirror() {
    // The plane index commits on slider release (ticket 23), so the per-frame
    // `apply` handoff must not touch it: a drag would otherwise resample the
    // field on every frame.
    let mut c = Controls::new();
    let mut s = State::new(false);
    assert!(s.snapshot_if_dirty().is_some(), "consume the fresh scene");
    c.plane = PlaneView {
        enabled: true,
        axis: PlaneAxis::X,
        index: 0,
    };
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
    assert!(
        s.snapshot_if_dirty().is_some(),
        "the fresh scene builds once"
    );
    assert!(
        !s.set_density(c.density),
        "synced density does not re-dirty"
    );
    assert!(!s.set_arrow_scale(c.arrow_scale));
    assert!(!s.set_arrow_opacity(c.arrow_opacity));
    assert!(!s.set_plane_dim_opacity(c.plane_dim_opacity));
    assert!(!s.set_arrow_length_mode(c.arrow_length_mode));
    assert!(!s.set_magnitude_scale(c.magnitude_scale));
    assert!(!s.set_proportional_exponent(c.proportional_exponent));
    assert!(!s.set_arrows(c.arrows));
    assert!(
        !s.set_show_grid(c.show_grid),
        "synced grid visibility is a no-op"
    );
    assert!(
        !s.set_show_coordinate_system(c.show_coordinate_system),
        "synced axes visibility is a no-op"
    );
    assert!(!s.set_arrow_style(c.arrow_style));
    assert!(!s.set_arrow_style_2d(c.arrow_style_2d));
    assert!(!s.set_arrow_params(c.arrow_params));
    assert!(!s.set_colormap(c.colormap));
    assert!(!s.set_auto_fit(c.auto_fit));
    assert!(!s.set_color_range(c.color_min, c.color_max));
    assert!(!s.set_streamlines(c.streamlines));
    assert!(
        !s.set_seed_mode(c.seed_mode),
        "synced seed mode does not re-dirty"
    );
    assert!(
        !s.set_left_click_action(c.left_click_action),
        "synced left-click action does not re-dirty",
    );
    assert!(!s.set_particles(c.particles));
    assert!(
        !s.set_domain(c.domain_min, c.domain_max),
        "synced extrema are a no-op"
    );
    assert!(
        s.snapshot_if_dirty().is_none(),
        "an untouched frame rebuilds nothing"
    );
}

#[test]
fn reference_visibility_applies_independently_without_redirtying() {
    let mut c = Controls::new();
    let mut s = State::new(false);
    assert!(s.snapshot_if_dirty().is_some(), "consume the initial scene");

    c.show_grid = false;
    apply(&mut s, &mut c);
    assert_eq!((s.show_grid, c.show_grid), (false, false));
    assert_eq!(
        (s.show_coordinate_system, c.show_coordinate_system),
        (true, true)
    );
    assert!(s.snapshot_if_dirty().is_some(), "grid edit rebuilds once");
    apply(&mut s, &mut c);
    assert!(
        s.snapshot_if_dirty().is_none(),
        "unchanged grid does not redirty"
    );

    c.show_coordinate_system = false;
    apply(&mut s, &mut c);
    assert_eq!(
        (s.show_coordinate_system, c.show_coordinate_system),
        (false, false)
    );
    assert_eq!(
        (s.show_grid, c.show_grid),
        (false, false),
        "axes edit leaves grid hidden"
    );
    assert!(s.snapshot_if_dirty().is_some(), "axes edit rebuilds once");
    apply(&mut s, &mut c);
    assert!(
        s.snapshot_if_dirty().is_none(),
        "unchanged axes do not redirty"
    );
}

#[test]
fn reference_visibility_survives_modes_and_display_reset() {
    let mut c = Controls::new();
    let mut s = State::new(false);
    assert!(s.snapshot_if_dirty().is_some());
    c.streamlines = true;
    c.seed_mode = StreamlineSeedMode::Click;
    c.show_grid = false;
    c.show_coordinate_system = false;
    apply(&mut s, &mut c);
    assert_eq!((s.show_grid, s.show_coordinate_system), (false, false));

    assert!(s.set_mode(true));
    c.density = s.density;
    apply(&mut s, &mut c);
    assert_eq!((s.show_grid, c.show_grid), (false, false));
    assert_eq!(
        (s.show_coordinate_system, c.show_coordinate_system),
        (false, false)
    );
    assert!(s.set_mode(false));
    c.density = s.density;
    apply(&mut s, &mut c);
    assert_eq!((s.show_grid, c.show_grid), (false, false));
    assert_eq!(
        (s.show_coordinate_system, c.show_coordinate_system),
        (false, false)
    );

    c.reset_display();
    apply(&mut s, &mut c);
    assert_eq!((s.show_grid, c.show_grid), (true, true));
    assert_eq!(
        (s.show_coordinate_system, c.show_coordinate_system),
        (true, true)
    );
    assert!(
        s.streamlines && c.streamlines,
        "Display reset leaves streamlines on"
    );
    assert_eq!(s.seed_mode, StreamlineSeedMode::Click);
    assert_eq!(c.seed_mode, StreamlineSeedMode::Click);
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
    assert!(
        s.set_domain(c.domain_min, c.domain_max),
        "the reversed pair changes the domain"
    );
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
    assert!(
        s.snapshot_if_dirty().is_some(),
        "the fresh scene builds once"
    );
    apply(&mut s, &mut c);
    assert!(
        s.snapshot_if_dirty().is_none(),
        "an untouched apply rebuilds nothing"
    );
    assert_eq!(c.density, s.density, "and leaves the mirror in sync");
}

#[test]
fn density_clamp_keys_to_sampled_dimension_through_apply() {
    let mut c = Controls::new();
    let mut s = State::new(true);
    assert!(s.snapshot_if_dirty().is_some(), "fresh scene built");
    c.density = 60;
    apply(&mut s, &mut c);
    assert_eq!(
        s.density, MAX_DENSITY_3D,
        "n³ sampling caps at the 40 ceiling"
    );
    assert_eq!(
        c.density, MAX_DENSITY_3D,
        "the mirror is refreshed to the effective cap"
    );
    assert_eq!(
        s.grid().z,
        MAX_DENSITY_3D,
        "sampling really runs n³ at the cap"
    );
    assert!(
        s.snapshot_if_dirty().is_some(),
        "the clamped density still moved the scene"
    );

    let mut c2 = Controls::new();
    let mut s2 = State::new(false);
    assert!(s2.set_mode(true), "3D mode on a flat field");
    assert!(s2.snapshot_if_dirty().is_some(), "the mode switch rebuilt");
    c2.density = 150;
    apply(&mut s2, &mut c2);
    assert_eq!(
        s2.density, 150,
        "flat sampling affords the 200 ceiling even in 3D mode"
    );
    assert_eq!(s2.grid().z, 1, "…and samples flat");
    assert!(
        s2.snapshot_if_dirty().is_some(),
        "the density change re-instanced"
    );
}

#[test]
fn colormap_row_and_manual_range_feed_color_range_through_apply() {
    let mut c = Controls::new();
    let mut s = State::new(false);
    assert!(s.snapshot_if_dirty().is_some(), "fresh scene built");
    apply(&mut s, &mut c);
    assert!(
        s.view_options().color_range.is_none(),
        "auto-fit keeps the sampler's own fit"
    );
    c.colormap = Colormap::Turbo;
    c.auto_fit = false;
    c.color_min = 5.0;
    c.color_max = -1.0;
    apply(&mut s, &mut c);
    assert_eq!(
        s.colormap,
        Colormap::Turbo,
        "the selectable-label row lands on the seam"
    );
    assert_eq!(
        s.view_options().color_range,
        Some((-1.0_f32, 5.0_f32)),
        "the manual pair feeds color_range, swapped by the seam",
    );
    assert_eq!(
        c.color_min, -1.0,
        "the mirror shows the seam's swapped pair"
    );
    assert_eq!(c.color_max, 5.0);
    assert!(
        s.snapshot_if_dirty().is_some(),
        "coloring changes re-instance"
    );
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
    assert!(
        s.snapshot_if_dirty().is_some(),
        "arrow scale dirties the scene"
    );
    c.arrow_opacity = 0.5;
    apply(&mut s, &mut c);
    assert!(
        s.snapshot_if_dirty().is_some(),
        "arrow opacity dirties the scene"
    );
    c.plane_dim_opacity = 0.4;
    apply(&mut s, &mut c);
    assert!(
        s.snapshot_if_dirty().is_some(),
        "the off-plane opacity dirties the scene"
    );
    c.arrow_length_mode = ArrowLengthMode::Magnitude;
    apply(&mut s, &mut c);
    assert!(
        s.snapshot_if_dirty().is_some(),
        "arrow length mode dirties the scene"
    );
    c.magnitude_scale = 2.0;
    apply(&mut s, &mut c);
    assert!(
        s.snapshot_if_dirty().is_some(),
        "magnitude scale dirties the scene"
    );
    c.proportional_exponent = 2.0;
    apply(&mut s, &mut c);
    assert!(
        s.snapshot_if_dirty().is_some(),
        "the proportional contrast dirties the scene"
    );
    c.arrows = false;
    apply(&mut s, &mut c);
    assert!(
        s.snapshot_if_dirty().is_some(),
        "the Show arrows toggle dirties the scene"
    );
    c.arrow_style = ArrowStyle::Cylinder;
    apply(&mut s, &mut c);
    assert!(
        s.snapshot_if_dirty().is_some(),
        "the arrow style dirties the scene"
    );
    c.arrow_params.head_length = 0.45;
    apply(&mut s, &mut c);
    assert!(
        s.snapshot_if_dirty().is_some(),
        "arrow appearance dirties the scene"
    );
    c.colormap = Colormap::Mono;
    apply(&mut s, &mut c);
    assert!(
        s.snapshot_if_dirty().is_some(),
        "colormap dirties the scene"
    );
    c.auto_fit = false;
    apply(&mut s, &mut c);
    assert!(
        s.snapshot_if_dirty().is_some(),
        "auto-fit toggle dirties the scene"
    );
    c.color_min = 0.25;
    c.color_max = 1.75;
    apply(&mut s, &mut c);
    assert!(
        s.snapshot_if_dirty().is_some(),
        "manual color range dirties the scene"
    );
    c.streamlines = true;
    apply(&mut s, &mut c);
    assert!(
        s.snapshot_if_dirty().is_some(),
        "streamlines stub still flows state"
    );
    c.seed_mode = StreamlineSeedMode::Click;
    apply(&mut s, &mut c);
    assert!(
        s.snapshot_if_dirty().is_some(),
        "the seed mode dirties the scene"
    );
    c.particles = true;
    apply(&mut s, &mut c);
    assert!(
        s.snapshot_if_dirty().is_some(),
        "particles stub still flows state"
    );
    c.domain_max.x = 0.5;
    apply(&mut s, &mut c);
    assert!(
        s.snapshot_if_dirty().is_some(),
        "domain extrema dirty the scene"
    );
}

// --- Arrow length modes and magnitude scale (ticket 33) ---------------------

#[test]
fn magnitude_scale_is_enabled_only_in_magnitude_mode() {
    assert!(
        !magnitude_scale_enabled(ArrowLengthMode::Proportional),
        "Proportional has no separate magnitude scale",
    );
    assert!(
        !magnitude_scale_enabled(ArrowLengthMode::Uniform),
        "Uniform has no magnitude scale"
    );
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
    assert!(
        arrow_scale_enabled(ArrowLengthMode::Uniform),
        "Uniform lengths are the arrow scale"
    );
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
    assert_eq!(
        s.arrow_length_mode,
        ArrowLengthMode::Magnitude,
        "the mode lands on the seam"
    );
    assert_eq!(s.arrow_scale, 2.0, "the numeric arrow scale lands too");
    assert_eq!(s.magnitude_scale, 0.25, "the magnitude scale lands too");
    assert_eq!(
        c.arrow_length_mode, s.arrow_length_mode,
        "the mirror follows the mode"
    );
    assert_eq!(
        c.arrow_scale, s.arrow_scale,
        "the mirror follows the arrow scale"
    );
    assert_eq!(
        c.magnitude_scale, s.magnitude_scale,
        "the mirror follows the magnitude scale"
    );
    assert!(
        !arrow_scale_enabled(s.arrow_length_mode),
        "Magnitude mode disables the arrow scale widget",
    );
    assert!(
        magnitude_scale_enabled(s.arrow_length_mode),
        "Magnitude mode enables the magnitude scale widget",
    );
    assert!(
        s.snapshot_if_dirty().is_some(),
        "the edit rebuilds the scene once"
    );
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
    assert_eq!(
        s.proportional_exponent, 2.0,
        "the contrast lands on the seam"
    );
    assert_eq!(c.proportional_exponent, 2.0, "the mirror follows");

    c.proportional_exponent = 1_000.0;
    apply(&mut s, &mut c);
    assert_eq!(
        s.proportional_exponent, MAX_PROPORTIONAL_EXPONENT,
        "the seam clamps"
    );
    assert_eq!(
        c.proportional_exponent, MAX_PROPORTIONAL_EXPONENT,
        "the mirror shows the clamp"
    );
}

#[test]
fn display_reset_restores_proportional_and_both_scales() {
    let mut c = Controls::new();
    c.arrow_length_mode = ArrowLengthMode::Magnitude;
    c.arrow_scale = 3.5;
    c.magnitude_scale = 4.0;
    c.proportional_exponent = 3.0;

    c.reset_display();

    assert_eq!(
        c.arrow_length_mode,
        ArrowLengthMode::Proportional,
        "the default mode returns"
    );
    assert_eq!(c.arrow_scale, 1.0, "the default arrow scale returns");
    assert_eq!(
        c.magnitude_scale, 1.0,
        "the default magnitude scale returns"
    );
    assert_eq!(
        c.proportional_exponent, 1.0,
        "the default proportional contrast returns"
    );
}

#[test]
fn display_reset_restores_arrow_visibility_style_and_appearance() {
    let mut c = Controls::new();
    c.arrows = false;
    c.arrow_style = ArrowStyle::Streamline;
    c.arrow_style_2d = ArrowStyle2D::Streamline;
    c.arrow_params = ArrowParams {
        head_length: 0.5,
        head_width: 0.3,
        shaft_radius: 0.12,
        sides: 4,
    };

    c.reset_display();

    assert!(c.arrows, "arrows return");
    assert_eq!(c.arrow_style, ArrowStyle::Cone, "the default style returns");
    assert_eq!(
        c.arrow_style_2d,
        ArrowStyle2D::Flat,
        "the default 2D style returns"
    );
    assert_eq!(
        c.arrow_params,
        ArrowParams::default(),
        "the default appearance returns"
    );
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
    assert_eq!(
        s.arrow_opacity, 0.35,
        "the global opacity lands on the seam"
    );
    assert_eq!(s.plane_dim_opacity, 0.7, "the off-plane opacity lands too");
    assert_eq!(
        c.arrow_opacity, s.arrow_opacity,
        "the mirror follows the global opacity"
    );
    assert_eq!(
        c.plane_dim_opacity, s.plane_dim_opacity,
        "the mirror follows the off-plane one"
    );
    assert!(
        s.snapshot_if_dirty().is_some(),
        "the edit rebuilds the scene once"
    );

    c.arrow_opacity = 0.0;
    c.plane_dim_opacity = 5.0;
    apply(&mut s, &mut c);
    assert_eq!(
        s.arrow_opacity, MIN_ARROW_OPACITY,
        "the seam clamps the global opacity floor"
    );
    assert_eq!(
        s.plane_dim_opacity, MAX_PLANE_DIM_OPACITY,
        "the seam clamps the off-plane ceiling"
    );
    assert_eq!(
        c.arrow_opacity, MIN_ARROW_OPACITY,
        "the mirror shows the clamp"
    );
    assert_eq!(c.plane_dim_opacity, MAX_PLANE_DIM_OPACITY);

    c.arrow_opacity = f32::NAN;
    c.plane_dim_opacity = f32::INFINITY;
    apply(&mut s, &mut c);
    assert_eq!(
        s.arrow_opacity, MAX_ARROW_OPACITY,
        "NaN falls back to full opacity"
    );
    assert_eq!(
        s.plane_dim_opacity, DIM_ALPHA,
        "Infinity falls back to the dim alpha"
    );
    assert_eq!(
        c.arrow_opacity, MAX_ARROW_OPACITY,
        "the mirror follows the fallback"
    );
    assert_eq!(c.plane_dim_opacity, DIM_ALPHA);
}

#[test]
fn display_reset_restores_the_arrow_and_off_plane_opacities() {
    let mut c = Controls::new();
    c.arrow_opacity = 0.25;
    c.plane_dim_opacity = 0.75;

    c.reset_display();

    assert_eq!(
        c.arrow_opacity, 1.0,
        "the global opacity returns to fully opaque"
    );
    assert_eq!(
        c.plane_dim_opacity, DIM_ALPHA,
        "the off-plane opacity returns to 0.2"
    );
}

#[test]
fn the_sidebar_shows_the_matching_arrow_set() {
    assert_eq!(
        arrow_style_set(true),
        ArrowStyleSet::ThreeD,
        "3D offers the 3D families"
    );
    assert_eq!(
        arrow_style_set(false),
        ArrowStyleSet::TwoD,
        "2D offers the 2D families"
    );
    assert_eq!(ArrowStyleSet::ThreeD.label(), "Arrow style");
    assert_eq!(ArrowStyleSet::TwoD.label(), "2D arrow style");
    assert_eq!(
        ArrowStyle::ALL.len(),
        4,
        "the 3D selector keeps its four families"
    );
    assert_eq!(
        ArrowStyle::ALL.map(ArrowStyle::label),
        ["Cone", "Cylinder", "Flat", "Line"]
    );
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
    assert!(sides_control_enabled(
        true,
        ArrowStyle::Cone,
        ArrowStyle2D::Flat
    ));
    assert!(sides_control_enabled(
        true,
        ArrowStyle::Cylinder,
        ArrowStyle2D::Flat
    ));
    assert!(!sides_control_enabled(
        true,
        ArrowStyle::Flat,
        ArrowStyle2D::Flat
    ));
    assert!(!sides_control_enabled(
        true,
        ArrowStyle::Streamline,
        ArrowStyle2D::Flat
    ));
    assert!(
        !sides_control_enabled(false, ArrowStyle::Cone, ArrowStyle2D::Streamline),
        "the 2D line has a fixed silhouette too",
    );
}

#[test]
fn head_controls_are_enabled_only_for_families_with_a_head() {
    assert!(head_control_enabled(
        true,
        ArrowStyle::Cone,
        ArrowStyle2D::Flat
    ));
    assert!(head_control_enabled(
        true,
        ArrowStyle::Flat,
        ArrowStyle2D::Flat
    ));
    assert!(!head_control_enabled(
        true,
        ArrowStyle::Cylinder,
        ArrowStyle2D::Flat
    ));
    assert!(!head_control_enabled(
        true,
        ArrowStyle::Streamline,
        ArrowStyle2D::Flat
    ));

    assert!(head_control_enabled(
        false,
        ArrowStyle::Cone,
        ArrowStyle2D::Flat
    ));
    assert!(!head_control_enabled(
        false,
        ArrowStyle::Cone,
        ArrowStyle2D::Streamline
    ));
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
    assert_eq!(
        s.arrow_style,
        ArrowStyle::Cylinder,
        "the 3D selection survives the view switch"
    );
    assert_eq!(
        s.arrow_style_2d,
        ArrowStyle2D::Streamline,
        "the 2D selection survives too"
    );
    assert_eq!(
        c.arrow_style,
        ArrowStyle::Cylinder,
        "the mirror keeps the 3D family"
    );
    assert_eq!(
        c.arrow_style_2d,
        ArrowStyle2D::Streamline,
        "the mirror keeps the 2D family"
    );
    assert_eq!(arrow_style_set(s.mode_3d), ArrowStyleSet::ThreeD);

    assert!(s.set_mode(false), "switch back to 2D");
    apply(&mut s, &mut c);
    assert_eq!(
        s.arrow_style,
        ArrowStyle::Cylinder,
        "the 3D selection still survives"
    );
    assert_eq!(
        s.arrow_style_2d,
        ArrowStyle2D::Streamline,
        "the 2D selection still survives"
    );
    assert_eq!(arrow_style_set(s.mode_3d), ArrowStyleSet::TwoD);
}

#[test]
fn changing_one_arrow_set_never_changes_the_other_through_apply() {
    let mut c = Controls::new();
    let mut s = State::new(false);
    assert!(s.snapshot_if_dirty().is_some(), "consume the fresh scene");

    c.arrow_style = ArrowStyle::Flat;
    apply(&mut s, &mut c);
    assert_eq!(
        s.arrow_style_2d,
        ArrowStyle2D::Flat,
        "the 2D set is untouched"
    );

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
    assert_eq!(
        domain_summary(&reversed, true),
        "x -2…2 · y -4…4 · z 0.5…1.5"
    );

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
    assert_eq!(
        s.domain.min.x, MIN_DOMAIN_EXTENT,
        "the min drag cannot leave the bound"
    );
    assert_eq!(
        s.domain.max.x, MAX_DOMAIN_EXTENT,
        "the max drag cannot leave the bound"
    );
    assert_eq!(
        c.domain_min.x, MIN_DOMAIN_EXTENT,
        "the mirror shows the clamped value"
    );
    assert_eq!(c.domain_max.x, MAX_DOMAIN_EXTENT);
    assert_eq!(
        (s.domain.min.y, s.domain.max.y),
        (0.0, 2.0),
        "other axes are untouched"
    );
    assert!(
        s.snapshot_if_dirty().is_some(),
        "a clamped edit still changes the scene"
    );
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
    assert_eq!(
        (s.domain.min.y, s.domain.max.y),
        (-2.0, 2.0),
        "the y pair swaps too"
    );
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
    assert_eq!(
        (s.domain.min.x, s.domain.max.x),
        (-1.0, 1.0),
        "NaN cannot enter the state"
    );
    assert_eq!(
        c.domain_min.x, -1.0,
        "the mirror falls back to the previous extent"
    );
    assert_eq!(c.domain_max.x, 1.0);
    assert_eq!(
        (s.domain.min.y, s.domain.max.y),
        (2.0, 3.0),
        "a valid axis still applies"
    );
}

#[test]
fn domain_reset_restores_all_axes_including_z_in_2d() {
    let mut s = State::new(false);
    assert!(
        s.set_domain(Vec3::splat(-5.0), Vec3::splat(5.0)),
        "a non-default box is stored"
    );
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

    assert_eq!(
        controls.domain_min,
        Vec3::splat(-1.0),
        "all three minima reset before apply"
    );
    assert_eq!(
        controls.domain_max,
        Vec3::splat(1.0),
        "all three maxima reset before apply"
    );
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
    assert_eq!(
        (s.domain.min.z, s.domain.max.z),
        (-4.0, 4.0),
        "2D keeps the stored z"
    );
    assert_eq!(
        domain_axes(s.is_3d()),
        &[DomainAxis::X, DomainAxis::Y],
        "the 2D editor cannot touch z",
    );
    assert!(s.set_mode(true), "switch back to 3D");
    assert_eq!(
        (s.domain.min.z, s.domain.max.z),
        (-4.0, 4.0),
        "the same z returns in 3D"
    );
    assert_eq!(
        domain_axes(s.is_3d()),
        &[DomainAxis::X, DomainAxis::Y, DomainAxis::Z],
        "and so does its row",
    );
}

#[test]
fn seed_selector_is_enabled_only_while_streamlines_are_on() {
    assert!(
        !seed_mode_enabled(false),
        "Streamlines off disables the Auto/Click selector"
    );
    assert!(seed_mode_enabled(true), "Streamlines on enables it");
}

#[test]
fn seed_mode_defaults_to_auto_and_flows_through_apply() {
    let mut c = Controls::new();
    let mut s = State::new(false);
    assert_eq!(
        c.seed_mode,
        StreamlineSeedMode::Auto,
        "the selector starts on Auto"
    );
    assert!(s.snapshot_if_dirty().is_some(), "consume the fresh scene");

    c.seed_mode = StreamlineSeedMode::Click;
    apply(&mut s, &mut c);
    assert_eq!(
        s.seed_mode,
        StreamlineSeedMode::Click,
        "Click lands on the seam"
    );
    assert_eq!(c.seed_mode, s.seed_mode, "the mirror follows the seam");
    assert!(
        s.snapshot_if_dirty().is_some(),
        "a seed-mode change re-instances the scene"
    );
}

#[test]
fn plane_selector_is_available_in_2d_and_preserves_the_dim_toggle() {
    assert_eq!(plane_control_mode(3, false), PlaneControlMode::SliceOnly);
    assert_eq!(plane_control_mode(3, true), PlaneControlMode::Full);
    assert_eq!(plane_control_mode(2, false), PlaneControlMode::Hidden);

    let mut state = State::new(false);
    state.set_formula("x, y, z".into());
    state.set_mode(true);
    state.set_plane_enabled(true);
    state.set_mode(false);
    state.set_plane_axis(PlaneAxis::X);
    assert!(state.plane.enabled);
}

#[test]
fn domain_axes_include_z_for_a_3d_field_in_2d() {
    assert!(domain_axes(true).contains(&DomainAxis::Z));
    assert!(!domain_axes(false).contains(&DomainAxis::Z));
}

#[test]
fn features_reset_returns_the_streamline_count_to_auto() {
    let mut c = Controls::new();
    c.streamline_count = 37;
    let mut s = State::new(false);
    apply(&mut s, &mut c);
    assert_eq!(s.streamline_count, 37, "the count lands on the seam");

    c.reset_features();
    assert_eq!(
        c.streamline_count, 0,
        "the Features reset returns the count to Auto"
    );
    apply(&mut s, &mut c);
    assert_eq!(s.streamline_count, 0, "and the seam follows the reset");
}

#[test]
fn features_reset_returns_the_seed_mode_to_auto() {
    let mut c = Controls::new();
    c.streamlines = true;
    c.seed_mode = StreamlineSeedMode::Click;

    c.reset_features();

    assert!(
        !c.streamlines,
        "the Features reset keeps hiding streamlines"
    );
    assert_eq!(
        c.seed_mode,
        StreamlineSeedMode::Auto,
        "and returns the seed mode to Auto"
    );
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
    assert_eq!(
        s.left_click_action,
        LeftClickAction::AddParticles,
        "the choice lands on the seam"
    );
    assert_eq!(
        c.left_click_action, s.left_click_action,
        "the mirror follows the seam"
    );
}

#[test]
fn left_click_action_offers_both_choices_with_the_features_default_off() {
    // The row is never hidden or disabled by the feature toggles: it keeps both
    // choices visible and the click is a hinted no-op instead.
    assert_eq!(
        LeftClickAction::ALL.len(),
        2,
        "both choices stay on the row"
    );
    assert!(
        !Controls::new().streamlines,
        "the default Features state hides streamlines"
    );
    assert!(!Controls::new().particles, "and particles");
}

#[test]
fn features_reset_returns_the_left_click_action_to_seed_streamline() {
    let mut c = Controls::new();
    c.left_click_action = LeftClickAction::AddParticles;
    assert!(c.viewport_sticky, "Keep placing is the historical default");
    c.viewport_sticky = false;

    c.reset_features();

    assert_eq!(
        c.left_click_action,
        LeftClickAction::SeedStreamline,
        "Reset Features restores Seed streamline",
    );
    assert!(
        c.viewport_sticky,
        "Reset Features restores the strip's Keep placing default",
    );
}

// --- Ticket 30: particle size and count controls -----------------------

#[test]
fn particle_controls_default_to_the_seam_defaults() {
    let c = Controls::new();
    let s = State::new(false);
    assert_eq!(
        c.particle_size, s.particle_size,
        "the size mirror starts at the seam's"
    );
    assert_eq!(
        c.particle_size, DEFAULT_PARTICLE_SIZE,
        "…and that default is 1.0"
    );
    assert_eq!(
        c.particle_count, s.particle_count,
        "the count mirror starts at the seam's"
    );
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
    assert_eq!(
        c.particle_size, s.particle_size,
        "the mirror follows the size"
    );
    assert_eq!(
        c.particle_count, s.particle_count,
        "the mirror follows the count"
    );
    assert!(
        s.snapshot_if_dirty().is_none(),
        "neither control rebuilds the scene"
    );

    c.particle_size = 9.0;
    c.particle_count = 10_000;
    apply(&mut s, &mut c);
    assert_eq!(
        s.particle_size, MAX_PARTICLE_SIZE,
        "the seam clamps an oversized dot"
    );
    assert_eq!(
        c.particle_size, MAX_PARTICLE_SIZE,
        "the mirror shows the clamped size"
    );
    assert_eq!(
        s.particle_count, MAX_PARTICLES as u32,
        "the seam caps the count"
    );
    assert_eq!(
        c.particle_count, MAX_PARTICLES as u32,
        "the mirror shows the capped count"
    );
}

#[test]
fn particle_count_label_shows_auto_at_zero() {
    assert_eq!(
        particle_count_label(0),
        "Auto",
        "0 reads as Auto in the drag value"
    );
    assert_eq!(
        particle_count_label(100),
        "100",
        "an explicit count shows its number"
    );
    assert_eq!(particle_count_label(MAX_PARTICLES as u32), "4000");
}

#[test]
fn particle_size_is_enabled_only_while_particles_are_on() {
    assert!(
        !particle_size_enabled(false),
        "Particles off greys the size slider"
    );
    assert!(particle_size_enabled(true), "Particles on enables it");
}

#[test]
fn features_reset_restores_particle_size_and_count() {
    let mut c = Controls::new();
    c.particles = true;
    c.particle_size = 2.75;
    c.particle_count = 320;

    c.reset_features();

    assert_eq!(
        c.particle_size, DEFAULT_PARTICLE_SIZE,
        "reset restores size 1.0"
    );
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
    assert_eq!(
        c.particle_appearance.color_mode,
        ParticleColorMode::Magnitude
    );
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

    assert_eq!(
        s.particle_appearance, c.particle_appearance,
        "the values land on the seam"
    );
    assert_eq!(
        c.particle_appearance, s.particle_appearance,
        "and the mirror follows"
    );
    assert!(
        s.snapshot_if_dirty().is_none(),
        "appearance never rebuilds the scene"
    );

    c.particle_appearance.opacity = -1.0;
    c.particle_appearance.trail_len = 999;
    c.particle_appearance.stagnation_seconds = f32::NAN;
    apply(&mut s, &mut c);
    assert_eq!(
        s.particle_appearance.opacity, 0.05,
        "opacity clamps to its floor"
    );
    assert_eq!(
        s.particle_appearance.trail_len, 32,
        "trail length clamps to the ring size"
    );
    assert_eq!(
        s.particle_appearance.stagnation_seconds, 0.5,
        "a NaN threshold falls back"
    );
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
    assert_eq!(
        c.particle_size, DEFAULT_PARTICLE_SIZE,
        "ticket 30's size still resets"
    );
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

#[test]
fn streamline_count_control_is_auto_only_and_mirrors_the_state() {
    assert_eq!(streamline_count_label(0), "Auto");
    assert_eq!(streamline_count_label(37), "37");
    assert!(!streamline_count_enabled(false, StreamlineSeedMode::Auto));
    assert!(streamline_count_enabled(true, StreamlineSeedMode::Auto));
    assert!(!streamline_count_enabled(true, StreamlineSeedMode::Click));

    let mut state = State::new(false);
    let mut controls = Controls::new();
    controls.streamline_count = 37;
    apply(&mut state, &mut controls);
    assert_eq!(state.streamline_count, 37);
    assert_eq!(controls.streamline_count, 37);
}

// ---------------------------------------------------------------------------
// Issue #87 slice S3b: named seed layouts inside Flow -> Seed layout.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Node {
    role: egui::Role,
    label: Option<String>,
    value: Option<String>,
    rect: egui::Rect,
}

impl Node {
    fn center(&self) -> egui::Pos2 {
        self.rect.center()
    }
}

struct NamedLayouts {
    ctx: egui::Context,
    state: State,
    controls: Controls,
    layouts: SeedLayoutLibrary,
    time: f64,
}

impl NamedLayouts {
    fn new(layouts: SeedLayoutLibrary) -> Self {
        let ctx = support::context();
        ctx.enable_accesskit();
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let mut controls = Controls::new();
        controls.streamlines = true;
        Self {
            ctx,
            state: State::new(false),
            controls,
            layouts,
            time: 0.0,
        }
    }

    fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(600.0, 4000.0),
            )),
            time: Some(self.time),
            predicted_dt: 1.0 / 60.0,
            events,
            ..Default::default()
        };
        self.time += 1.0 / 60.0;
        let mut output = {
            let state = &mut self.state;
            let controls = &mut self.controls;
            let layouts = &mut self.layouts;
            self.ctx.run_ui(input, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    sidebar::show(ui, state, controls, layouts);
                });
            })
        };
        output.textures_delta.clear();
        output
    }

    fn idle(&mut self) -> egui::FullOutput {
        self.frame(Vec::new())
    }

    fn click(&mut self, pos: egui::Pos2) -> egui::FullOutput {
        self.frame(pointer(pos, true));
        self.frame(pointer(pos, false))
    }

    fn open_header(&mut self, text: &str) {
        let out = self.idle();
        let rows = nodes(&out);
        let node = rows
            .iter()
            .find(|node| node.label.as_deref() == Some(text) || node.value.as_deref() == Some(text))
            .unwrap_or_else(|| {
                panic!(
                    "the {text:?} header is exposed; values: {:?}",
                    rows.iter()
                        .filter_map(|node| node.value.as_deref())
                        .collect::<Vec<_>>()
                )
            });
        self.click(node.center());
        // Let the open animation finish so lower rows accept clicks.
        for _ in 0..20 {
            self.idle();
        }
    }

    fn open_named_layouts(&mut self) {
        self.open_header("Seed layout");
        self.open_header("Named layouts");
    }

    fn click_button(&mut self, label: &str) -> egui::FullOutput {
        self.click_nth_button(label, 0)
    }

    fn click_nth_button(&mut self, label: &str, index: usize) -> egui::FullOutput {
        let out = self.idle();
        let pos = nth_button_center(&out, label, index)
            .unwrap_or_else(|| panic!("the {label:?} button {index} is painted"));
        self.click(pos)
    }
}

fn pointer(pos: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(pos),
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        },
    ]
}

fn nodes(output: &egui::FullOutput) -> Vec<Node> {
    let Some(update) = output.platform_output.accesskit_update.as_ref() else {
        return Vec::new();
    };
    update
        .nodes
        .iter()
        .filter_map(|(_, node)| {
            let bounds = node.bounds()?;
            Some(Node {
                role: node.role(),
                label: node.label().map(str::to_owned),
                value: node.value().map(str::to_owned),
                rect: egui::Rect::from_min_max(
                    egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                    egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                ),
            })
        })
        .collect()
}

fn button_rects(output: &egui::FullOutput, label: &str) -> Vec<egui::Rect> {
    let mut rects: Vec<egui::Rect> = nodes(output)
        .into_iter()
        .filter(|node| node.role == egui::Role::Button && node.label.as_deref() == Some(label))
        .map(|node| node.rect)
        .collect();
    rects.sort_by(|a, b| {
        a.center()
            .y
            .total_cmp(&b.center().y)
            .then(a.center().x.total_cmp(&b.center().x))
    });
    rects
}

fn nth_button_center(output: &egui::FullOutput, label: &str, index: usize) -> Option<egui::Pos2> {
    button_rects(output, label)
        .get(index)
        .map(egui::Rect::center)
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

fn accesskit_labels(output: &egui::FullOutput) -> Vec<String> {
    let Some(update) = output.platform_output.accesskit_update.as_ref() else {
        return Vec::new();
    };
    update
        .nodes
        .iter()
        .filter_map(|(_, node)| {
            node.label()
                .map(str::to_owned)
                .or_else(|| node.value().map(str::to_owned))
        })
        .collect()
}

fn widget_disabled(output: &egui::FullOutput, label: &str) -> Option<bool> {
    output
        .platform_output
        .accesskit_update
        .as_ref()?
        .nodes
        .iter()
        .find(|(_, node)| node.label() == Some(label) || node.value() == Some(label))
        .map(|(_, node)| node.is_disabled())
}

fn painted(output: &egui::FullOutput) -> String {
    painted_texts(output)
        .into_iter()
        .chain(accesskit_labels(output))
        .collect::<Vec<_>>()
        .join("\n")
}

fn label_rect(output: &egui::FullOutput, text: &str) -> Option<egui::Rect> {
    nodes(output)
        .into_iter()
        .find(|node| node.value.as_deref() == Some(text) || node.label.as_deref() == Some(text))
        .map(|node| node.rect)
}

fn copied_text(output: &egui::FullOutput) -> Vec<String> {
    output
        .platform_output
        .commands
        .iter()
        .filter_map(|command| match command {
            egui::OutputCommand::CopyText(text) => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn profile(name: &str, seeds: &[Vec3]) -> SeedLayoutProfile {
    SeedLayoutProfile {
        name: name.to_owned(),
        seeds: SeedLayout {
            seeds: seeds
                .iter()
                .map(|position| SeedPoint {
                    position: *position,
                    anchor: None,
                })
                .collect(),
        },
    }
}

fn library(profiles: Vec<SeedLayoutProfile>) -> SeedLayoutLibrary {
    SeedLayoutLibrary::new(profiles, &Domain::default())
        .expect("the test layouts are valid in the default domain")
}

#[test]
fn named_layouts_list_builtin_default_first_and_user_layouts_after() {
    let mut h = NamedLayouts::new(library(vec![
        profile("Alpha", &[Vec3::ZERO, Vec3::new(0.4, 0.0, 0.0)]),
        profile("Beta", &[Vec3::new(0.0, 0.4, 0.0)]),
    ]));
    h.open_named_layouts();
    let out = h.idle();
    let text = painted(&out);

    assert!(
        text.contains("Default (generated grid)"),
        "the built-in Default row is labelled with its generated-grid meaning: {text}"
    );
    assert!(
        text.contains("Alpha (2 seeds)"),
        "user layouts show their seed count: {text}"
    );
    assert!(
        text.contains("Beta (1 seed)"),
        "singular seed counts read naturally: {text}"
    );

    let default =
        label_rect(&out, "Default (generated grid)").expect("the built-in label is exposed");
    let alpha = label_rect(&out, "Alpha (2 seeds)").expect("the Alpha label is exposed");
    let beta = label_rect(&out, "Beta (1 seed)").expect("the Beta label is exposed");
    assert!(
        default.center().y < alpha.center().y,
        "the built-in Default row comes first"
    );
    assert!(
        alpha.center().y < beta.center().y,
        "user layouts keep library order"
    );
}

#[test]
fn applying_a_layout_replaces_seeds_and_reports_the_name() {
    let mut h = NamedLayouts::new(library(vec![profile(
        "Mine",
        &[Vec3::ZERO, Vec3::new(0.4, 0.0, 0.0)],
    )]));
    h.open_named_layouts();
    h.controls.selected_seed = Some(0);
    h.controls.seed_drag = Some(0);
    h.controls.place_seed = true;

    // The built-in row is first, so "Mine" is the second Apply button.
    let _ = h.click_nth_button("Apply", 1);
    let out = h.idle();

    assert_eq!(
        h.state.seed_layout().len(),
        2,
        "the stored layout replaced the seeds"
    );
    assert_eq!(
        h.controls.seed_layout_message, "Applied layout \"Mine\"",
        "the status slot names the applied layout"
    );
    assert_eq!(h.controls.selected_seed, None, "selection resets");
    assert_eq!(h.controls.seed_drag, None, "drag state resets");
    assert!(!h.controls.place_seed, "placement disarms");
    assert!(
        painted(&out).contains("Applied layout \"Mine\""),
        "the message is painted inline"
    );
}

#[test]
fn applying_an_invalid_layout_reports_the_reason_and_keeps_seeds() {
    let mut h = NamedLayouts::new(library(vec![profile(
        "Outside",
        &[Vec3::new(0.9, 0.0, 0.0)],
    )]));
    // Narrow the domain after the library was built: the stored layout now
    // fails validation against the current scene.
    h.state.set_domain(Vec3::splat(-0.5), Vec3::splat(0.5));
    // Keep the sidebar mirror in step: `show` hands it back through `apply`
    // and would otherwise restore the default unit cube.
    h.controls.domain_min = Vec3::splat(-0.5);
    h.controls.domain_max = Vec3::splat(0.5);
    h.state
        .commit_seed_layout(SeedLayout {
            seeds: vec![SeedPoint {
                position: Vec3::ZERO,
                anchor: None,
            }],
        })
        .expect("the in-domain seed commits");
    let before = h.state.seed_layout().clone();
    h.open_named_layouts();

    let _ = h.click_nth_button("Apply", 1);
    let out = h.idle();

    assert_eq!(
        h.state.seed_layout(),
        &before,
        "a rejected layout leaves the current seeds untouched"
    );
    assert!(
        h.controls
            .seed_layout_message
            .contains("outside the scene domain"),
        "the validation reason is reported: {}",
        h.controls.seed_layout_message
    );
    assert!(
        painted(&out).contains("outside the scene domain"),
        "the reason is painted inline"
    );
}

#[test]
fn saving_current_seeds_rejects_empty_builtin_and_oversized_names() {
    let mut h = NamedLayouts::new(SeedLayoutLibrary::default());
    h.state
        .commit_seed_layout(SeedLayout {
            seeds: vec![SeedPoint {
                position: Vec3::ZERO,
                anchor: None,
            }],
        })
        .expect("the seed commits");
    h.open_named_layouts();

    for (name, expected) in [
        ("", "layout name must not be empty"),
        (DEFAULT_LAYOUT_NAME, "built-in"),
        (&"x".repeat(49), "the limit is 48"),
    ] {
        h.controls.seed_layout_name = name.to_owned();
        let _ = h.click_button("Save current");
        assert!(
            h.controls.seed_layout_message.contains(expected),
            "saving {name:?} reports the library error text: {}",
            h.controls.seed_layout_message
        );
        assert!(
            h.layouts.is_empty(),
            "a rejected save never stores a layout: {name:?}"
        );
    }
}

#[test]
fn saving_twice_with_one_name_replaces_in_place() {
    let mut h = NamedLayouts::new(library(vec![profile("Zed", &[Vec3::ZERO])]));
    h.open_named_layouts();
    h.state
        .commit_seed_layout(SeedLayout {
            seeds: vec![
                SeedPoint {
                    position: Vec3::ZERO,
                    anchor: None,
                },
                SeedPoint {
                    position: Vec3::new(0.2, 0.0, 0.0),
                    anchor: None,
                },
            ],
        })
        .expect("the seeds commit");

    h.controls.seed_layout_name = "  Mine  ".to_owned();
    let _ = h.click_button("Save current");
    assert_eq!(
        h.controls.seed_layout_message, "Saved layout \"Mine\"",
        "a new layout reports Saved"
    );
    assert_eq!(h.layouts.len(), 2);
    assert_eq!(h.layouts.user()[1].name, "Mine", "a new layout appends");
    assert_eq!(h.layouts.user()[1].seeds.len(), 2);
    assert!(
        h.controls.seed_layout_name.is_empty(),
        "a successful save clears the name draft"
    );

    h.state
        .commit_seed_layout(SeedLayout {
            seeds: vec![
                SeedPoint {
                    position: Vec3::new(-0.2, 0.0, 0.0),
                    anchor: None,
                },
                SeedPoint {
                    position: Vec3::ZERO,
                    anchor: None,
                },
                SeedPoint {
                    position: Vec3::new(0.2, 0.0, 0.0),
                    anchor: None,
                },
            ],
        })
        .expect("the replacement seeds commit");
    h.controls.seed_layout_name = "Mine".to_owned();
    let _ = h.click_button("Save current");

    assert_eq!(
        h.controls.seed_layout_message, "Replaced layout \"Mine\"",
        "an existing name reports Replaced"
    );
    assert_eq!(h.layouts.len(), 2, "the save replaces in place");
    let names: Vec<&str> = h.layouts.user().iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, vec!["Zed", "Mine"], "library order is preserved");
    assert_eq!(
        h.layouts.user()[1].seeds.len(),
        3,
        "the stored seeds are the current committed seeds"
    );
}

#[test]
fn deleting_a_user_layout_needs_two_clicks_and_clears_on_other_actions() {
    let mut h = NamedLayouts::new(library(vec![profile("Mine", &[Vec3::ZERO])]));
    h.open_named_layouts();

    let _ = h.click_button("Delete");
    assert_eq!(h.layouts.len(), 1, "the first click only arms the delete");
    assert_eq!(h.controls.pending_layout_delete.as_deref(), Some("Mine"));
    assert_eq!(
        h.controls.seed_layout_message,
        "Click Delete again to remove \"Mine\""
    );
    assert!(
        button_rects(&h.idle(), "Confirm delete").len() == 1,
        "the armed row labels the second click"
    );

    let _ = h.click_button("Confirm delete");
    assert!(h.layouts.is_empty(), "the second click removes the layout");
    assert_eq!(h.controls.pending_layout_delete, None, "arming clears");
    assert_eq!(h.controls.seed_layout_message, "Deleted layout \"Mine\"");

    // Re-populate and check a successful save clears an armed delete.
    h.state
        .commit_seed_layout(SeedLayout {
            seeds: vec![SeedPoint {
                position: Vec3::ZERO,
                anchor: None,
            }],
        })
        .expect("a seed commits");
    h.controls.seed_layout_name = "Mine".to_owned();
    let _ = h.click_button("Save current");
    assert_eq!(h.layouts.len(), 1);
    let _ = h.click_button("Delete");
    assert_eq!(h.controls.pending_layout_delete.as_deref(), Some("Mine"));
    h.controls.seed_layout_name = "Other".to_owned();
    let _ = h.click_button("Save current");
    assert_eq!(
        h.controls.pending_layout_delete, None,
        "a successful save clears the armed delete"
    );

    // And a successful apply clears it too.
    let _ = h.click_button("Delete");
    assert_eq!(h.controls.pending_layout_delete.as_deref(), Some("Mine"));
    let _ = h.click_nth_button("Apply", 0);
    assert_eq!(
        h.controls.pending_layout_delete, None,
        "a successful apply clears the armed delete"
    );
}

#[test]
fn export_copy_and_import_report_deltas_and_reject_bad_documents_atomically() {
    let mut h = NamedLayouts::new(library(vec![profile("Mine", &[Vec3::ZERO])]));
    h.open_named_layouts();

    let out = h.click_button("Export text");
    assert!(
        !h.controls.seed_layout_text.is_empty(),
        "Export refreshes the portable buffer"
    );
    assert!(
        h.controls.seed_layout_text.contains("\"version\":1"),
        "the export uses the versioned compact document"
    );
    assert!(
        painted(&out).contains("\"version\":1"),
        "the read-only editor paints the export"
    );

    let out = h.click_button("Copy");
    assert_eq!(
        copied_text(&out),
        vec![h.controls.seed_layout_text.clone()],
        "Copy hands the export buffer to the clipboard"
    );

    let before = h.layouts.len();
    let document = SeedLayoutDocument {
        version: SEED_LAYOUT_FORMAT_VERSION,
        layouts: vec![
            profile("Mine", &[Vec3::new(0.2, 0.0, 0.0)]),
            profile("New one", &[Vec3::ZERO]),
            profile("New two", &[Vec3::new(0.0, 0.2, 0.0)]),
        ],
    };
    h.controls.seed_layout_import = document.to_json().expect("the document serializes");
    let _ = h.click_button("Apply text");
    assert_eq!(
        h.controls.seed_layout_message, "Imported 2 new layouts",
        "the delta counts only new user layouts"
    );
    assert_eq!(h.layouts.len(), before + 2);

    let replaced_only = SeedLayoutDocument {
        version: SEED_LAYOUT_FORMAT_VERSION,
        layouts: vec![
            profile("Mine", &[Vec3::new(-0.2, 0.0, 0.0)]),
            profile("New one", &[Vec3::new(0.0, -0.2, 0.0)]),
        ],
    };
    h.controls.seed_layout_import = replaced_only.to_json().expect("the document serializes");
    let _ = h.click_button("Apply text");
    assert_eq!(
        h.controls.seed_layout_message, "Import replaced existing layouts; no new layouts imported",
        "replacement-only imports say so honestly"
    );
    assert_eq!(h.layouts.len(), before + 2, "nothing was added");

    let singular = SeedLayoutDocument {
        version: SEED_LAYOUT_FORMAT_VERSION,
        layouts: vec![profile("Only new", &[Vec3::ZERO])],
    };
    h.controls.seed_layout_import = singular.to_json().expect("the document serializes");
    let _ = h.click_button("Apply text");
    assert_eq!(h.controls.seed_layout_message, "Imported 1 new layout");

    let stable = h.layouts.clone();
    h.controls.seed_layout_import = "{not json".to_owned();
    let _ = h.click_button("Apply text");
    assert!(
        h.controls
            .seed_layout_message
            .contains("invalid seed layout JSON"),
        "a malformed document reports its parse error: {}",
        h.controls.seed_layout_message
    );
    assert_eq!(
        h.layouts, stable,
        "a rejected import leaves the library untouched"
    );

    h.controls.seed_layout_import = r#"{"version":2,"layouts":[]}"#.to_owned();
    let _ = h.click_button("Apply text");
    assert!(
        h.controls
            .seed_layout_message
            .contains("version 2 is not supported"),
        "a future version is reported before parsing: {}",
        h.controls.seed_layout_message
    );
    assert_eq!(h.layouts, stable, "a future-version import stays atomic");
}

#[test]
fn empty_import_document_reports_nothing_to_import_and_changes_nothing() {
    let mut h = NamedLayouts::new(library(vec![profile("Mine", &[Vec3::ZERO])]));
    h.open_named_layouts();
    // Prime the panel so the empty import has observable buffer and arming state.
    let _ = h.click_button("Export text");
    let exported = h.controls.seed_layout_text.clone();
    assert!(
        !exported.is_empty(),
        "the export buffer holds the user layout before the import"
    );
    h.controls.pending_layout_delete = Some("Mine".to_owned());
    let stable = h.layouts.clone();

    h.controls.seed_layout_import = SeedLayoutDocument {
        version: SEED_LAYOUT_FORMAT_VERSION,
        layouts: Vec::new(),
    }
    .to_json()
    .expect("the empty document serializes");
    let out = h.click_button("Apply text");
    let text = painted(&out);

    assert!(
        text.contains("Nothing to import"),
        "an empty document says there is nothing to import:\n{text}"
    );
    assert!(
        !text.contains("replaced") && !text.contains("no new layouts"),
        "an empty document is not painted as a replacement:\n{text}"
    );
    assert_eq!(h.layouts, stable, "the library is unchanged");
    assert_eq!(
        h.controls.seed_layout_text, exported,
        "the export buffer is unchanged"
    );
    assert_eq!(
        h.controls.pending_layout_delete.as_deref(),
        Some("Mine"),
        "the armed delete is unchanged"
    );
}

// ---------------------------------------------------------------------------
// Issue #89 slice C: the live Variables editor in the Controls panel.
// ---------------------------------------------------------------------------

fn variable(name: &str, value: f64) -> Parameter {
    Parameter {
        name: name.to_owned(),
        value,
        min: None,
        max: None,
        step: 1.0,
    }
}

fn bounded_variable(name: &str, value: f64, min: f64, max: f64) -> Parameter {
    Parameter {
        name: name.to_owned(),
        value,
        min: Some(min),
        max: Some(max),
        step: 0.5,
    }
}

fn parameter_set(entries: Vec<Parameter>) -> ParameterSet {
    ParameterSet {
        version: PARAMETER_VERSION,
        entries,
    }
}

struct VariablesHarness {
    ctx: egui::Context,
    state: State,
    controls: Controls,
    layouts: SeedLayoutLibrary,
    time: f64,
}

impl VariablesHarness {
    fn new(entries: Vec<Parameter>, formula: &str) -> Self {
        let mut state = State::new(false);
        if !entries.is_empty() {
            state
                .commit_parameters(parameter_set(entries))
                .expect("the fixture parameters are valid");
        }
        state.set_formula(formula.to_owned());
        let _ = state.snapshot_if_dirty();
        let ctx = support::context();
        ctx.enable_accesskit();
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        Self {
            ctx,
            state,
            controls: Controls::new(),
            layouts: SeedLayoutLibrary::default(),
            time: 0.0,
        }
    }

    fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(600.0, 4000.0),
            )),
            time: Some(self.time),
            predicted_dt: 1.0 / 60.0,
            events,
            ..Default::default()
        };
        self.time += 1.0 / 60.0;
        let mut output = {
            let state = &mut self.state;
            let controls = &mut self.controls;
            let layouts = &mut self.layouts;
            self.ctx.run_ui(input, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    sidebar::show(ui, state, controls, layouts);
                });
            })
        };
        output.textures_delta.clear();
        output
    }

    fn idle(&mut self) -> egui::FullOutput {
        self.frame(Vec::new())
    }

    fn click(&mut self, pos: egui::Pos2) -> egui::FullOutput {
        self.frame(pointer(pos, true));
        self.frame(pointer(pos, false))
    }

    fn open_variables(&mut self) {
        let out = self.idle();
        let node = nodes(&out)
            .into_iter()
            .find(|node| {
                node.label.as_deref() == Some("Variables")
                    || node.value.as_deref() == Some("Variables")
            })
            .expect("the Variables header is exposed in formula vector mode");
        self.click(node.center());
        for _ in 0..20 {
            self.idle();
        }
    }

    fn click_button(&mut self, label: &str) -> egui::FullOutput {
        self.click_nth_button(label, 0)
    }

    fn click_nth_button(&mut self, label: &str, index: usize) -> egui::FullOutput {
        let out = self.idle();
        let pos = nth_button_center(&out, label, index)
            .unwrap_or_else(|| panic!("the {label:?} button {index} is painted"));
        self.click(pos)
    }
}

#[test]
fn variables_section_lists_parameters_and_hides_outside_formula_vector_mode() {
    assert!(variables_visible(FieldMode::Formula, true));
    assert!(!variables_visible(FieldMode::Formula, false));
    for mode in [
        FieldMode::Sources,
        FieldMode::Induction,
        FieldMode::Materials,
        FieldMode::Maxwell,
        FieldMode::Ode,
    ] {
        assert!(
            !variables_visible(mode, true),
            "{mode:?} owns its own field and hides the formula Variables editor"
        );
    }

    let mut h = VariablesHarness::new(
        vec![variable("alpha", 1.0), variable("beta", 2.0)],
        "(alpha*x, beta*y)",
    );
    h.open_variables();
    let out = h.idle();
    let text = painted(&out);
    assert!(label_rect(&out, "Variables").is_some(), "header is painted");
    for expected in [
        "alpha",
        "beta",
        "Value",
        "Min",
        "Max",
        "Step",
        "unbounded",
        "Add parameter",
        "Reset variables",
    ] {
        assert!(
            text.contains(expected),
            "the committed rows expose {expected:?}:\n{text}"
        );
    }

    // A non-formula vector field hides the editor with the other vector
    // formula sections.
    assert!(h.state.set_field_mode(FieldMode::Sources));
    let out = h.idle();
    assert!(
        label_rect(&out, "Variables").is_none(),
        "the Variables section is absent in Sources mode"
    );

    // Scalar/plot mode hides vector sections too.
    assert!(h.state.set_field_mode(FieldMode::Formula));
    assert!(h.state.set_plot_kind(PlotKind::Curve2D));
    let out = h.idle();
    assert!(
        label_rect(&out, "Variables").is_none(),
        "the Variables section is absent while plotting"
    );
}

#[test]
fn editing_a_value_commits_and_reparses_the_formula() {
    let mut h = VariablesHarness::new(
        vec![
            bounded_variable("a", 1.0, 0.0, 10.0),
            bounded_variable("b", 2.0, 0.0, 10.0),
        ],
        "(a*x, b*y)",
    );
    h.open_variables();
    let before = h
        .state
        .field
        .eval2(glam::vec2(3.0, 4.0))
        .expect("the fixture formula evaluates");
    assert_eq!(before, glam::vec2(3.0, 8.0));

    h.controls.variables.drafts[0].value = 2.5;
    assert!(
        commit_variables(&mut h.state, &mut h.controls),
        "a valid edit commits"
    );
    assert_eq!(h.state.parameter_value("a"), Some(2.5));
    assert_eq!(h.controls.variables.drafts[0].value, 2.5);
    let after = h
        .state
        .field
        .eval2(glam::vec2(3.0, 4.0))
        .expect("the committed formula still evaluates");
    assert_eq!(
        after,
        glam::vec2(7.5, 8.0),
        "the formula re-evaluated with the new parameter value"
    );

    // A rejected edit keeps the previous committed set and does not wipe the
    // other row's draft.
    let committed = h.state.parameters.clone();
    h.controls.variables.drafts[0].value = -1.0;
    h.controls.variables.drafts[1].value = 3.0;
    assert!(
        !commit_variables(&mut h.state, &mut h.controls),
        "the out-of-range value is rejected"
    );
    assert!(
        h.controls.variables.error.contains("below min"),
        "the model's message is kept: {}",
        h.controls.variables.error
    );
    assert_eq!(
        h.state.parameters, committed,
        "the rejected set leaves the committed set untouched"
    );
    assert_eq!(
        h.controls.variables.drafts[1].value, 3.0,
        "the rejected edit does not clear another row's draft"
    );
}

#[test]
fn adding_an_invalid_name_reports_the_model_message_and_changes_nothing() {
    let mut h = VariablesHarness::new(Vec::new(), "(x, y)");
    h.open_variables();

    h.controls.variables.name = "1bad".to_owned();
    let out = h.click_button("Add parameter");
    assert!(
        h.controls.variables.error.contains("must start with"),
        "the validation message is kept: {}",
        h.controls.variables.error
    );
    assert!(
        h.state.parameters.entries.is_empty(),
        "an invalid name never reaches the committed set"
    );
    assert!(
        h.controls.variables.drafts.is_empty(),
        "the rejected row is not kept"
    );
    assert_eq!(
        h.controls.variables.name, "1bad",
        "the name draft stays so the user can fix it"
    );
    assert!(
        painted(&out).contains("must start with"),
        "the model's message is painted inline"
    );
}

#[test]
fn duplicate_and_reserved_names_are_rejected_without_mutation() {
    let mut h = VariablesHarness::new(vec![variable("a", 1.0)], "(a*x, y)");
    h.open_variables();

    h.controls.variables.name = "a".to_owned();
    let _ = h.click_button("Add parameter");
    assert!(
        h.controls.variables.error.contains("duplicate"),
        "a duplicate name reports the model's message: {}",
        h.controls.variables.error
    );
    assert_eq!(h.state.parameters.entries.len(), 1, "no row is added");
    assert_eq!(
        h.controls.variables.drafts.len(),
        1,
        "no draft row survives"
    );

    h.controls.variables.name = "pi".to_owned();
    let _ = h.click_button("Add parameter");
    assert!(
        h.controls.variables.error.contains("reserved"),
        "a reserved name reports the model's message: {}",
        h.controls.variables.error
    );
    assert_eq!(h.state.parameters.entries.len(), 1, "no row is added");
    assert_eq!(
        h.controls.variables.drafts.len(),
        1,
        "no draft row survives"
    );
}

#[test]
fn deleting_rows_commits_the_smaller_set_and_deleting_all_commits_empty() {
    let mut h = VariablesHarness::new(vec![variable("a", 1.0), variable("b", 2.0)], "(a*x, b*y)");
    h.open_variables();

    let _ = h.click_nth_button("Delete", 0);
    let names: Vec<&str> = h
        .state
        .parameters
        .entries
        .iter()
        .map(|entry| entry.name.as_str())
        .collect();
    assert_eq!(names, vec!["b"], "the first row's deletion commits");
    assert_eq!(
        h.controls.variables.drafts.len(),
        1,
        "the editor mirror follows the committed set"
    );

    let _ = h.click_nth_button("Delete", 0);
    assert!(
        h.state.parameters.entries.is_empty(),
        "removing the last row commits an empty set"
    );
    assert!(h.controls.variables.drafts.is_empty());
}

#[test]
fn reset_variables_commits_an_empty_set() {
    let mut h = VariablesHarness::new(vec![variable("a", 1.0), variable("b", 2.0)], "(a*x, b*y)");
    h.open_variables();
    assert_eq!(h.state.parameters.entries.len(), 2);

    let _ = h.click_button("Reset variables");
    assert!(
        h.state.parameters.entries.is_empty(),
        "Reset variables commits an empty ParameterSet"
    );
    assert!(h.controls.variables.drafts.is_empty());
    assert!(h.controls.variables.error.is_empty());
}

#[test]
fn committing_an_unchanged_set_is_a_rebuild_no_op() {
    let mut h = VariablesHarness::new(vec![variable("a", 1.0)], "(a*x, y)");
    h.open_variables();
    let _ = h.state.snapshot_if_dirty();
    assert!(!h.state.dirty, "the idle panel leaves the scene settled");

    h.controls.variables.drafts[0].value = 1.0;
    assert!(commit_variables(&mut h.state, &mut h.controls));
    assert!(!h.state.dirty, "an unchanged set does not dirty the scene");
    assert!(
        h.state.snapshot_if_dirty().is_none(),
        "an unchanged set requests no rebuild"
    );
}

#[test]
fn scalar_section_hides_until_the_formula_can_be_sampled() {
    let mut h = NamedLayouts::new(SeedLayoutLibrary::default());
    h.state.set_field_mode(FieldMode::Sources);
    let out = h.idle();
    assert!(
        !painted(&out).contains("Scalar view"),
        "the Scalar view section is hidden outside the formula field"
    );

    h.state.set_field_mode(FieldMode::Formula);
    let out = h.idle();
    assert!(
        painted(&out).contains("Scalar view"),
        "the Scalar view section appears for the formula field"
    );
}

#[test]
fn unsupported_z_formula_shows_the_reason_instead_of_controls() {
    let mut h = NamedLayouts::new(SeedLayoutLibrary::default());
    h.state
        .set_scalar_source(ScalarContext::Surface, "z + x".into());
    h.controls.scalar_place_probe = true;
    h.controls.viewport_armed_one_shot = true;
    h.open_header("Scalar view");

    let out = h.idle();
    let text = painted(&out);
    assert!(
        text.contains(UNSUPPORTED_Z_REASON),
        "the section reports the model's unsupported reason: {text}"
    );
    assert!(
        !text.contains("Grid resolution") && !text.contains("Contour levels"),
        "no scalar controls are offered for a z formula"
    );
    assert!(
        !h.controls.scalar_place_probe,
        "the unsupported branch disarms the probe"
    );
    assert!(
        !h.controls.viewport_armed_one_shot,
        "the unsupported branch must not leak the one-shot marker"
    );
}

#[test]
fn unsupported_z_formula_disables_the_heatmap_checkbox_with_the_reason() {
    let mut h = NamedLayouts::new(SeedLayoutLibrary::default());
    h.state
        .set_scalar_source(ScalarContext::Surface, "z + x".into());
    h.open_header("Scalar view");

    let out = h.idle();
    let text = painted(&out);
    assert!(
        text.contains("Show heatmap"),
        "the heatmap toggle stays visible so its reason can be read: {text}"
    );
    assert_eq!(
        widget_disabled(&out, "Show heatmap"),
        Some(true),
        "an inapplicable scalar view must disable the heatmap toggle"
    );
}

#[test]
fn place_probe_checkbox_arms_one_shot_under_the_sticky_default() {
    let mut h = NamedLayouts::new(SeedLayoutLibrary::default());
    assert!(
        h.controls.viewport_sticky,
        "the strip default is Keep placing"
    );
    h.open_header("Scalar view");

    let out = h.idle();
    let probe = label_rect(&out, "Place probe").expect("the Place probe checkbox is exposed");
    h.click(probe.center());

    assert!(h.controls.scalar_place_probe, "the checkbox arms the click");
    assert!(
        h.controls.viewport_armed_one_shot,
        "the standalone checkbox forces one-shot even while Keep placing is selected"
    );
    assert!(
        h.controls.viewport_sticky,
        "the checkbox does not silently change the strip's mode"
    );

    // Unchecking cancels the arm without leaking the one-shot marker.
    let out = h.idle();
    let probe = label_rect(&out, "Place probe").expect("the Place probe checkbox is exposed");
    h.click(probe.center());
    assert!(!h.controls.scalar_place_probe, "the checkbox is unchecked");
    assert!(
        !h.controls.viewport_armed_one_shot,
        "unchecking clears the one-shot marker"
    );
}

#[test]
fn grid_and_contour_controls_commit_bounded_values() {
    let mut h = NamedLayouts::new(SeedLayoutLibrary::default());
    assert_eq!(h.state.scalar_grid_side(), 64, "the default grid side");
    assert_eq!(
        h.state.scalar_contour_levels(),
        8,
        "the default level count"
    );

    assert!(h.state.set_scalar_grid_side(8).is_err(), "below the floor");
    assert!(
        h.state.set_scalar_grid_side(168).is_err(),
        "above the ceiling"
    );
    assert!(h.state.set_scalar_grid_side(40).is_ok());
    assert_eq!(h.state.scalar_view_request().grid_side, 40);

    assert!(
        h.state.set_scalar_contour_levels(17).is_err(),
        "more levels than the cap"
    );
    assert!(h.state.set_scalar_contour_levels(0).is_ok());
    assert!(
        h.state.scalar_view_request().levels.is_empty(),
        "zero levels hides contours"
    );
    assert!(h.state.set_scalar_contour_levels(16).is_ok());
    assert!(!h.state.scalar_view_request().levels.is_empty());

    // The sidebar mirror commits through the same validated setters.
    h.controls.scalar_grid_side = 32;
    h.controls.scalar_contour_levels = 4;
    let _ = h.idle();
    assert_eq!(h.state.scalar_grid_side(), 32);
    assert_eq!(h.state.scalar_contour_levels(), 4);
    assert_eq!(h.controls.scalar_grid_side, 32, "the mirror is re-synced");
}

#[test]
fn a_successful_scalar_control_commit_clears_a_stale_message() {
    let mut c = Controls::new();
    let mut s = State::new(false);
    c.scalar_message = "Click inside the scene domain to place a probe".to_owned();
    c.scalar_grid_side = 32;

    apply(&mut s, &mut c);

    assert_eq!(s.scalar_grid_side(), 32, "the new side commits");
    assert!(
        c.scalar_message.is_empty(),
        "a successful commit clears the stale rejection: {:?}",
        c.scalar_message
    );

    // A rejected mirror value still reports and leaves the commit untouched.
    c.scalar_grid_side = 8;
    apply(&mut s, &mut c);
    assert_eq!(s.scalar_grid_side(), 32, "the rejection keeps the commit");
    assert!(
        c.scalar_message.contains("grid side") || c.scalar_message.contains("8"),
        "the rejection is still reported: {:?}",
        c.scalar_message
    );
}

#[test]
fn hidden_section_keeps_the_cache_without_resampling() {
    let mut h = NamedLayouts::new(SeedLayoutLibrary::default());
    h.state
        .set_scalar_source(ScalarContext::Surface, "x + t".into());
    h.open_header("Scalar view");
    let _ = h.idle();
    let resamples = h.state.scalar_view_cache().resamples();
    assert_eq!(resamples, 1, "opening the section samples once");
    assert!(h.state.scalar_view_cache().record().is_some());

    // Collapse the section through its real header toggle.
    let out = h.idle();
    let header = label_rect(&out, "Scalar view").expect("the header is exposed");
    h.click(header.center());
    for _ in 0..5 {
        h.idle();
    }

    h.state.set_time(0.4);
    for _ in 0..3 {
        h.idle();
    }
    assert_eq!(
        h.state.scalar_view_cache().resamples(),
        resamples,
        "a collapsed section performs no sampling"
    );
    assert!(
        h.state.scalar_view_cache().record().is_some(),
        "the last record is retained"
    );
}

// ---------------------------------------------------------------------------
// Issue #93 slice B: the Scalar analysis subsection.
// ---------------------------------------------------------------------------

#[test]
fn scalar_analysis_section_lists_series_and_reports_validation_errors() {
    let mut h = NamedLayouts::new(SeedLayoutLibrary::default());
    h.open_header("Scalar view");
    let _ = h.idle();
    assert_eq!(
        h.state.scalar_analysis_resamples(),
        0,
        "the closed Scalar analysis subsection performs no sampling"
    );
    h.open_header("Scalar analysis");
    let _ = h.idle();
    assert!(
        h.state.scalar_analysis_resamples() > 0,
        "opening the subsection samples once"
    );
    let out = h.idle();
    assert!(
        painted_texts(&out)
            .iter()
            .any(|text| text.contains("No comparison series")),
        "the empty list shows the honest empty state: {:?}",
        painted_texts(&out)
    );

    let mut set = ComparisonSet::new();
    set.push(ComparisonSeries::new(
        SeriesId(0),
        "Potential".into(),
        "x*x".into(),
        SeriesStyle::default(),
    ))
    .expect("room for one series");
    h.state.set_comparison_series(set).expect("valid set");
    // One frame syncs the drafts from the committed set, the next paints them.
    let _ = h.idle();
    let out = h.idle();
    let labels = accesskit_labels(&out);
    assert!(
        labels.iter().any(|label| label.contains("Potential")),
        "the committed label is listed: {labels:?}"
    );
    assert!(
        labels.iter().any(|label| label.contains("x*x")),
        "the committed formula is editable in place: {labels:?}"
    );

    // A rejected edit keeps the honest model message and leaves the committed
    // record untouched.
    let commitment = h.state.comparison_series.clone();
    h.controls.scalar_analysis.series[0].label.clear();
    let _ = h.idle();
    let out = h.idle();
    assert!(
        painted_texts(&out)
            .iter()
            .chain(accesskit_labels(&out).iter())
            .any(|text| text.contains("label must not be empty")),
        "the model's rejection is shown inline: {:?}",
        painted_texts(&out)
    );
    assert_eq!(
        h.state.comparison_series, commitment,
        "a rejected edit never reaches the document"
    );
}

#[test]
fn adding_and_removing_series_uses_the_bounded_commit_seam() {
    let mut h = NamedLayouts::new(SeedLayoutLibrary::default());
    h.open_header("Scalar view");
    h.open_header("Scalar analysis");

    let _ = h.click_button("Add series");
    assert_eq!(h.state.comparison_series.len(), 1);
    let first = &h.state.comparison_series.as_slice()[0];
    assert_eq!(first.label, "Series 1");
    assert_eq!(first.expr, "x");
    assert_eq!(first.style, SeriesStyle::fallback(0));

    let _ = h.click_button("Add series");
    assert_eq!(h.state.comparison_series.len(), 2);
    let second = &h.state.comparison_series.as_slice()[1];
    assert_eq!(second.label, "Series 2");
    assert_eq!(
        second.style,
        SeriesStyle::fallback(1),
        "each Add takes the next palette colour"
    );

    let _ = h.click_nth_button("Remove series", 0);
    assert_eq!(h.state.comparison_series.len(), 1);
    assert_eq!(
        h.state.comparison_series.as_slice()[0].label,
        "Series 2",
        "Remove removes exactly the clicked row"
    );

    // Fill the cap through the same Add seam, then prove a fifth draft is
    // rejected by the bounded model instead of growing the document.
    while h.state.comparison_series.len() < 4 {
        let _ = h.click_button("Add series");
    }
    assert_eq!(h.state.comparison_series.len(), 4);
    h.controls.scalar_analysis.series.push(SeriesDraft {
        id: SeriesId(99),
        label: "Overflow".into(),
        expr: "x".into(),
        style: SeriesStyle::default(),
    });
    let _ = h.idle();
    let out = h.idle();
    assert_eq!(
        h.state.comparison_series.len(),
        4,
        "the document stays at the cap"
    );
    assert!(
        painted_texts(&out)
            .iter()
            .chain(accesskit_labels(&out).iter())
            .any(|text| text.contains("at most 4")),
        "the bounded commit seam reports the cap: {:?}",
        painted_texts(&out)
    );
}

#[test]
fn gradient_controls_apply_role_step_and_max_error() {
    let mut state = State::new(false);
    let mut controls = Controls::new();
    controls.scalar_analysis.gradient.enabled = true;
    controls.scalar_analysis.gradient.role = SeriesRole::ElectricPotential;
    controls.scalar_analysis.gradient.expr = "x + y".into();
    controls.scalar_analysis.gradient.step = 1e-3;
    controls.scalar_analysis.gradient.max_error = 0.25;
    apply(&mut state, &mut controls);

    let spec = state.gradient.clone().expect("the gradient is committed");
    assert_eq!(spec.role, SeriesRole::ElectricPotential);
    assert_eq!(spec.expr, "x + y");
    assert_eq!(spec.step, 1e-3);
    assert_eq!(spec.max_error, 0.25);
    assert_eq!(
        spec.frame,
        state.scalar_view_frame(),
        "the panel commits the surface-context frame"
    );

    // An out-of-range step is rejected before sampling, never clamped.
    controls.scalar_analysis.gradient.step = 5.0;
    apply(&mut state, &mut controls);
    let kept = state.gradient.clone().expect("the previous spec survives");
    assert_eq!(
        kept.step, 1e-3,
        "the rejected edit leaves the document alone"
    );
    assert!(
        controls.scalar_analysis.gradient_error.contains("outside"),
        "the model's step bound is shown: {:?}",
        controls.scalar_analysis.gradient_error
    );

    // Unchecking enable clears the request through the same seam.
    controls.scalar_analysis.gradient.enabled = false;
    apply(&mut state, &mut controls);
    assert!(state.gradient.is_none());
}

#[test]
fn scalar_analysis_commits_only_while_the_scalar_view_is_applicable() {
    let mut state = State::new(false);
    let mut controls = Controls::new();
    assert!(
        state.scalar_view_applicable(),
        "the default vector formula view hosts scalar analysis"
    );
    let _ = state.snapshot_if_dirty();

    controls.scalar_analysis.gradient.enabled = true;
    controls.scalar_analysis.gradient.expr = "x + y".into();
    controls.scalar_analysis.series.push(SeriesDraft {
        id: SeriesId(0),
        label: "draft".into(),
        expr: "x".into(),
        style: SeriesStyle::default(),
    });

    // Curve2D/Surface3D make the scalar view inert. `apply` must not rewrite
    // the persisted gradient frame or commit the retained drafts.
    assert!(state.set_plot_kind(PlotKind::Curve2D));
    let _ = state.snapshot_if_dirty();
    apply(&mut state, &mut controls);
    assert!(
        state.gradient.is_none(),
        "a scalar plot mode never commits a gradient draft"
    );
    assert!(
        state.comparison_series.is_empty(),
        "a scalar plot mode never commits a series draft"
    );
    assert!(!state.dirty, "the inert drafts leave the document alone");
    apply(&mut state, &mut controls);
    assert!(
        !state.dirty,
        "repeated inert frames still leave the document alone"
    );
    assert!(
        controls.scalar_analysis.gradient.enabled,
        "the draft survives"
    );

    // Returning to the vector formula view commits the retained drafts.
    assert!(state.set_plot_kind(PlotKind::VectorField));
    let _ = state.snapshot_if_dirty();
    apply(&mut state, &mut controls);
    let gradient = state.gradient.as_ref().expect("the gradient commits");
    assert_eq!(gradient.expr, "x + y");
    assert_eq!(
        gradient.frame,
        state.scalar_view_frame(),
        "the committed frame is the applicable scalar view"
    );
    assert_eq!(
        state.comparison_series.len(),
        1,
        "the retained series draft commits"
    );
}
