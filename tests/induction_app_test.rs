//! Task 2 induction app integration: shared clock, caps, cache identity,
//! arrows-only snapshots and mode isolation.

use std::sync::Arc;

use vector_field_explorer::VfApp;
use vector_field_explorer::induction::InductionScene;
use vector_field_explorer::plane::PlaneAxis;
use vector_field_explorer::plot::PlotKind;
use vector_field_explorer::source_scene::{FieldMode, SourceDisplay, SourceEntry, SourceScene};

fn step(app: &mut VfApp, dt: f32) {
    let mut harness = app.test_harness();
    harness.advance_clock(dt);
    harness.advance_simulation();
}

fn animated_scene() -> InductionScene {
    InductionScene {
        amplitude: 1.0,
        frequency: 0.25,
        ..InductionScene::default()
    }
}

#[test]
fn induction_mode_uses_caps_and_preserves_formula_and_sources() {
    let mut app = VfApp::new();
    let formula = app.state.formula.clone();
    let sources = app.state.sources.clone();
    app.state.set_density(200);
    app.state.set_mode(true);
    app.state.set_field_mode(FieldMode::Induction);
    app.test_harness().advance_simulation();
    let grid = app.state.plane_grid();
    assert!(
        grid.x.max(grid.y).max(grid.z) <= 20,
        "3D induction cap: {grid:?}"
    );
    let sampling = app.test_harness().induction_sampling().expect("sampling");
    // The default loop at t = 0 has dI/dt = 0, so E is empty and only B draws.
    // The 20³ grid therefore bounds each family at 20³ and both at 2·20³.
    assert!(
        sampling.magnetic.drawn <= 20 * 20 * 20,
        "3D per-family cap: {}",
        sampling.magnetic.drawn
    );
    assert!(
        sampling.arrows.len() <= 2 * 20 * 20 * 20,
        "3D total cap: {}",
        sampling.arrows.len()
    );
    assert_eq!(app.state.formula, formula);
    assert_eq!(app.state.sources, sources);
}

#[test]
fn induction_mode_preserves_the_current_projection() {
    let mut app = VfApp::new();
    assert!(!app.state.mode_3d, "a fresh app starts in 2D");
    app.state.set_field_mode(FieldMode::Induction);
    assert!(!app.state.mode_3d, "2D stays 2D");
    assert!(!app.state.is_3d());
    assert!(app.state.is_plane_slice());

    app.state.set_mode(true);
    app.state.set_field_mode(FieldMode::Formula);
    app.state.set_field_mode(FieldMode::Induction);
    assert!(app.state.mode_3d, "3D stays 3D");
    assert!(app.state.is_3d());
}

#[test]
fn animated_induction_consumes_the_shared_clock_and_static_does_not() {
    use vector_field_explorer::sim_clock::{has_clock_consumer, should_animate};
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Induction);
    let scene = InductionScene {
        amplitude: 1.0,
        frequency: 0.25,
        ..InductionScene::default()
    };
    assert!(app.state.set_induction_scene(scene));
    assert!(has_clock_consumer(&app.state));
    assert!(should_animate(&app.state, &app.clock));
    let mut still = app.state.induction.clone();
    still.frequency = 0.0;
    assert!(app.state.set_induction_scene(still));
    assert!(!has_clock_consumer(&app.state));
    assert!(!should_animate(&app.state, &app.clock));
}

#[test]
fn pause_freezes_arrows_and_step_advances_one_coherent_step() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Induction);
    assert!(app.state.set_induction_scene(animated_scene()));
    step(&mut app, 0.05);
    app.clock.set_playing(false);
    let stopped = app.clock.time;
    let stopped_sample = app.state.time;
    step(&mut app, 0.05);
    assert_eq!(app.clock.time, stopped, "pause holds time");
    assert_eq!(
        app.state.time, stopped_sample,
        "pause holds the sampled field"
    );
    app.clock.set_playing(true);
    step(&mut app, 0.05);
    assert!(app.clock.time > stopped, "one resumed frame advances");
    assert_ne!(app.state.time, stopped_sample);
}

#[test]
fn geometry_change_reprepares_and_time_only_change_rescales() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Induction);
    assert!(app.state.set_induction_scene(animated_scene()));
    step(&mut app, 0.05);
    let first = app.test_harness().induction_prepared_arc().unwrap();
    let first_sampling = app.test_harness().induction_sampling().unwrap();
    let first_positions: Vec<_> = first_sampling
        .arrows
        .iter()
        .map(|arrow| arrow.position)
        .collect();

    step(&mut app, 0.05);
    let second = app.test_harness().induction_prepared_arc().unwrap();
    let second_sampling = app.test_harness().induction_sampling().unwrap();
    assert!(
        Arc::ptr_eq(&first, &second),
        "time-only keeps the prepared cache"
    );
    assert_ne!(
        second_sampling.current, first_sampling.current,
        "sampling rescales with state.time"
    );
    assert_ne!(
        second_sampling.arrows, first_sampling.arrows,
        "the arrows follow the rescaling"
    );
    let second_positions: Vec<_> = second_sampling
        .arrows
        .iter()
        .map(|arrow| arrow.position)
        .collect();
    assert_eq!(
        second_positions, first_positions,
        "time-only keeps every arrow position"
    );

    // A geometry edit keeps the domain grid but changes the key and field.
    let mut moved = app.state.induction.clone();
    moved.radius = 2.0;
    assert!(app.state.set_induction_scene(moved));
    app.test_harness().advance_simulation();
    let third = app.test_harness().induction_prepared_arc().unwrap();
    assert!(!Arc::ptr_eq(&second, &third), "geometry change re-prepares");
    let third_sampling = app.test_harness().induction_sampling().unwrap();
    assert_ne!(
        third_sampling.arrows, second_sampling.arrows,
        "the field values follow the new geometry"
    );

    // A plane axis change resamples through the plane frame.
    app.state.set_mode(false);
    app.state.set_plane_axis(PlaneAxis::X);
    app.test_harness().advance_simulation();
    let axis_sampling = app.test_harness().induction_sampling().unwrap();
    assert!(!axis_sampling.arrows.is_empty());
    assert!(
        axis_sampling
            .arrows
            .iter()
            .all(|arrow| arrow.position.z == 0.0)
    );
    assert_ne!(axis_sampling.arrows, third_sampling.arrows);
}

#[test]
fn e_and_b_display_never_sum_and_use_fixed_colours() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Induction);
    let scene = InductionScene {
        display: SourceDisplay::Both,
        ..InductionScene::default()
    };
    assert!(app.state.set_induction_scene(scene));
    // A nonzero clock time gives a nonzero dI/dt, so both families draw.
    step(&mut app, 0.1);
    let sampling = app.test_harness().induction_sampling().unwrap();
    let colors: Vec<[u8; 3]> = sampling.arrows.iter().map(|arrow| arrow.color).collect();
    assert!(colors.contains(&[255, 176, 64]), "E amber present");
    assert!(colors.contains(&[64, 196, 255]), "B cyan present");
    assert!(sampling.electric.drawn > 0 && sampling.magnetic.drawn > 0);
    let mut electric = app.state.induction.clone();
    electric.display = SourceDisplay::Electric;
    assert!(app.state.set_induction_scene(electric));
    step(&mut app, 0.05);
    let sampling = app.test_harness().induction_sampling().unwrap();
    assert!(sampling.electric.drawn > 0);
    assert_eq!(sampling.magnetic.drawn, 0);
    assert!(
        sampling
            .arrows
            .iter()
            .all(|arrow| arrow.color != [64, 196, 255]),
        "Electric display never draws B's cyan"
    );
}

#[test]
fn induction_slice_uses_local_plane_positions_and_no_particles() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Induction);
    app.state.set_mode(false);
    app.state.set_plane_axis(PlaneAxis::X);
    app.test_harness().advance_simulation();
    let sampling = app.test_harness().induction_sampling().unwrap();
    assert!(!sampling.arrows.is_empty());
    assert!(sampling.arrows.iter().all(|arrow| arrow.position.z == 0.0));
    assert!(app.state.particle_target().is_none());
    let (positions, _, _, _) = app.test_harness().particle_snapshot();
    assert_eq!(positions.len(), 0, "no induction particle cloud is created");
}

#[test]
fn scalar_plot_switch_pauses_induction_and_preserves_time() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Induction);
    step(&mut app, 0.05);
    let before = app.clock.time;
    app.state.set_plot_kind(PlotKind::Curve2D);
    step(&mut app, 0.05);
    assert_eq!(
        app.clock.time, before,
        "scalar display pauses vector consumers"
    );
    app.state.set_plot_kind(PlotKind::VectorField);
    step(&mut app, 0.05);
    assert!(app.clock.time > before);
}

#[test]
fn invalid_induction_edit_retains_last_valid_scene_cache_and_arrows() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Induction);
    assert!(app.state.set_induction_scene(animated_scene()));
    app.test_harness().advance_simulation();
    let prepared = app.test_harness().induction_prepared_arc().unwrap();
    let arrows = app.test_harness().induction_sampling().unwrap().arrows;

    let mut invalid = app.state.induction.clone();
    invalid.radius = 0.0;
    assert!(!app.state.set_induction_scene(invalid));
    assert_eq!(
        app.test_harness().induction_error(),
        Some("induction radius must be finite and positive")
    );
    app.test_harness().advance_simulation();
    assert!(
        Arc::ptr_eq(
            &prepared,
            &app.test_harness().induction_prepared_arc().unwrap()
        ),
        "the last valid prepared cache survives an invalid edit"
    );
    assert_eq!(
        app.test_harness().induction_sampling().unwrap().arrows,
        arrows,
        "the last valid arrows survive an invalid edit"
    );

    let mut valid = app.state.induction.clone();
    valid.radius = 1.5;
    assert!(app.state.set_induction_scene(valid));
    assert_eq!(app.test_harness().induction_error(), None);
    app.test_harness().advance_simulation();
    assert!(
        !Arc::ptr_eq(
            &prepared,
            &app.test_harness().induction_prepared_arc().unwrap()
        ),
        "the valid edit re-prepares"
    );
}

#[test]
fn induction_snapshot_is_arrows_only_and_rebuild_composes_one_glyph() {
    let mut app = VfApp::new();
    app.state.set_mode(true);
    app.state.set_particles(true);
    app.state.set_streamlines(true);
    app.state.set_field_mode(FieldMode::Induction);
    assert!(app.state.set_induction_scene(animated_scene()));
    step(&mut app, 0.05);

    let snapshot = app.state.snapshot();
    assert!(!snapshot.arrows.is_empty());
    assert!(snapshot.lines.is_empty(), "no induction streamlines");
    assert!(snapshot.trails.is_empty(), "no induction trails");
    assert!(snapshot.dots.is_empty(), "no induction dots");
    assert!(snapshot.combined.is_empty());
    assert!(app.state.streamlines_snapshot(&[]).is_empty());
    assert!(
        app.state
            .click_streamlines_snapshot((0.0, 1.0), &[])
            .is_empty(),
        "no induction click lines"
    );
    assert!(app.state.particle_target().is_none());
    let (positions, _, _, _) = app.test_harness().particle_snapshot();
    assert_eq!(positions.len(), 0);

    // Exactly one conductor glyph is composed: the expected lines equal one
    // glyph on the selected source, and no formula/click geometry remains.
    let domain = app.state.scene_domain();
    let mut glyph_scene = SourceScene::default();
    glyph_scene.entries.push(SourceEntry {
        enabled: true,
        source: app.state.induction.unit_source(),
    });
    let glyphs = vector_field_explorer::source_glyph::source_glyphs(
        &glyph_scene,
        &domain,
        app.state.plane_frame().as_ref(),
    );
    assert_eq!(glyphs.len(), 1, "exactly one selected-conductor glyph");
    let expected = vector_field_explorer::source_glyph::source_glyph_lines(&glyphs, &domain).len();
    assert!(expected > 0);
    assert_eq!(
        app.test_harness().source_glyph_line_count(),
        expected,
        "the rebuild composes one conductor glyph"
    );
    let (arrows, trails, dots) = app.test_harness().vector_scene_counts();
    assert_eq!(arrows, snapshot.arrows.len());
    assert_eq!(trails, 0);
    assert_eq!(dots, 0);
    assert!(app.test_harness().viewport_line_count() >= expected);
}

#[test]
fn induction_caps_apply_per_view_and_do_not_clamp_formula_density() {
    // The 3D view caps both the sampling grid and the plane grid at 20³.
    let mut app = VfApp::new();
    app.state.set_density(200);
    app.state.set_mode(true);
    app.state.set_field_mode(FieldMode::Induction);
    assert_eq!(app.state.plane_grid(), glam::UVec3::splat(20));

    // A 2D slice keeps the user's projection and allows the 80² cap.
    let mut slice = VfApp::new();
    slice.state.set_density(200);
    slice.state.set_field_mode(FieldMode::Induction);
    assert!(!slice.state.mode_3d, "the projection is preserved");
    assert!(slice.state.set_density(80));
    slice.test_harness().advance_simulation();
    let sampling = slice.test_harness().induction_sampling().unwrap();
    assert!(
        sampling.magnetic.drawn <= 80 * 80,
        "2D slice cap: {}",
        sampling.magnetic.drawn
    );
    assert!(sampling.arrows.len() <= 80 * 80);
    assert_eq!(slice.state.density, 80);

    // Leaving Induction restores the untouched stored density semantics.
    slice.state.set_field_mode(FieldMode::Formula);
    assert_eq!(slice.state.density, 80);
    assert_eq!(slice.state.grid(), glam::UVec3::new(80, 80, 1));

    let mut untouched = VfApp::new();
    untouched.state.set_density(200);
    untouched.state.set_field_mode(FieldMode::Induction);
    untouched.state.set_field_mode(FieldMode::Formula);
    assert_eq!(
        untouched.state.density, 200,
        "entering Induction never destroys the formula density"
    );
    assert_eq!(untouched.state.grid(), glam::UVec3::new(200, 200, 1));
}

#[test]
fn induction_follows_the_mirrored_clock_time() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Induction);
    assert!(app.state.set_induction_scene(animated_scene()));
    step(&mut app, 0.05);
    let first = app.test_harness().induction_sampling().unwrap();
    assert!((app.state.time - app.clock.time).abs() < 1e-9);
    assert!((first.current - app.state.induction.current(app.state.time)).abs() < 1e-12);
}
