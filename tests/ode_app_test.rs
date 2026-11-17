//! Task 2 ODE app integration: the fourth field mode, lazy solving, the shared
//! clock, trajectory lines/markers, mode isolation, clicks and reset.

use eframe::egui;

use vector_field_explorer::VfApp;
use vector_field_explorer::ode::{ODE_STEP, OdeScene, OdeSeed, OdeSource};
use vector_field_explorer::plot::PlotKind;
use vector_field_explorer::source_scene::FieldMode;
use vector_field_explorer::viewmodel::LeftClickAction;

fn close(actual: f32, expected: f32) -> bool {
    (actual - expected).abs() < 1e-5
}

fn click_rect() -> egui::Rect {
    egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(400.0, 300.0))
}

#[test]
fn entering_ode_keeps_projection_and_uses_a_lazy_solution() {
    let mut app = VfApp::new();
    assert!(!app.state.mode_3d);
    let before = app.state.mode_3d;
    let formula = app.state.formula.clone();
    let sources = app.state.sources.clone();
    let induction = app.state.induction.clone();
    app.state.set_field_mode(FieldMode::Ode);
    assert_eq!(app.state.mode_3d, before);
    assert_eq!(app.state.formula, formula);
    assert_eq!(app.state.sources, sources);
    assert_eq!(app.state.induction, induction);
    assert_eq!(app.state.ode_solution, None);
    assert!(app.state.is_vector_mode());
    assert_eq!(app.state.plot.kind, PlotKind::VectorField);
    // No solution is stored yet, but the pending scene is a clock consumer.
    assert!(app.state.has_time_dependent_consumer());
    assert!(app.state.plane_frame().is_none());
    assert!(!app.state.is_plane_slice());
    assert!(app.state.particle_target().is_none());
    app.test_harness().advance_simulation();
    assert!(app.state.ode_solution.is_some());
    assert!(app.state.ode_error.is_none());
    assert_eq!(app.state.active_dimension(), 2);
    assert!(app.state.has_time_dependent_consumer());
}

#[test]
fn typed_solution_uses_the_active_dimension_and_clears_loop_period() {
    let mut app = VfApp::new();
    app.clock.loop_period = Some(3.0);
    let scene = OdeScene {
        source: OdeSource::Equations,
        dimension: 3,
        equations: ["-y".into(), "x".into(), "0".into()],
        seeds: vec![OdeSeed {
            position: [1.0, 0.0, 0.0],
        }],
        duration: 2.0,
    };
    app.state.set_ode_scene(scene).expect("valid");
    app.state.set_field_mode(FieldMode::Ode);
    assert_eq!(app.state.active_dimension(), 3);
    app.test_harness().advance_simulation();
    assert_eq!(app.clock.loop_period, None);
    assert_eq!(app.state.ode_solution.as_ref().unwrap().dimension, 3);
}

#[test]
fn clock_playback_and_step_share_the_solution_and_stop_at_duration() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Ode);
    app.test_harness().advance_simulation();
    let duration = app.state.ode_solution.as_ref().unwrap().duration as f32;
    app.test_harness().ode_step();
    assert!((app.clock.time as f64 - ODE_STEP).abs() < 1e-9);
    assert!(!app.clock.playing);
    app.clock.seek(duration - 0.01);
    app.clock.set_playing(true);
    app.test_harness().advance_clock(0.05);
    assert!(close(app.clock.time, duration));
    assert!(!app.clock.playing);
    assert!(close(app.state.time, duration));
}

#[test]
fn broken_current_field_draft_freezes_the_last_solution() {
    let mut app = VfApp::new();
    app.state.set_formula("y, -x".into());
    app.state.set_field_mode(FieldMode::Ode);
    app.state.ode.source = OdeSource::CurrentField;
    app.state
        .set_ode_scene(app.state.ode.clone())
        .expect("first solution");
    app.test_harness().advance_simulation();
    let before = app.state.ode_solution.clone().expect("solution");
    app.state.set_formula("y, -x +".into());
    app.test_harness().advance_simulation();
    assert!(app.state.ode_error.is_some());
    let after = app.state.ode_solution.as_ref().expect("retained solution");
    assert_eq!(after.bounds, before.bounds);
}

#[test]
fn ode_branch_has_lines_and_markers_but_no_particles_or_glyphs() {
    let mut app = VfApp::new();
    assert!(!app.state.mode_3d, "fresh app is 2D");
    // Enabled features must stay inert in Ode.
    app.state.set_streamlines(true);
    app.state.set_particles(true);
    app.state.set_field_mode(FieldMode::Ode);
    app.test_harness().advance_simulation();
    let scene = app.test_assemble_viewport_scene();
    assert!(!scene.lines.is_empty());
    assert_eq!(app.test_harness().ode_line_count(), scene.lines.len());
    assert!(app.test_harness().ode_marker_count() > 0);
    assert_eq!(scene.dots.len(), app.test_harness().ode_marker_count());
    assert!(scene.arrows.is_empty());
    assert!(scene.trails.is_empty());
    assert!(scene.combined.is_empty());
    // The pure snapshot never samples the formula field in Ode.
    let snapshot = app.state.snapshot();
    assert!(snapshot.arrows.is_empty());
    assert!(snapshot.lines.is_empty());
    assert!(snapshot.dots.is_empty());
    assert!(snapshot.combined.is_empty());

    app.state.set_mode(true);
    app.test_harness().advance_simulation();
    let scene = app.test_assemble_viewport_scene();
    assert!(!scene.combined.is_empty());
    assert!(
        scene
            .combined
            .iter()
            .all(|instance| instance.origin[3] == 0.0),
        "ODE has markers only, no arrows"
    );
}

#[test]
fn leaving_and_resetting_ode_preserves_formula_and_solves_the_default() {
    let mut app = VfApp::new();
    app.state.set_formula("y, -x, 0".into());
    app.state
        .set_ode_scene(OdeScene::default())
        .expect("ode scene");
    app.state.set_field_mode(FieldMode::Ode);
    app.state.reset_ode();
    let expected = OdeScene::default();
    assert_eq!(app.state.ode, expected);
    let solution = app.state.ode_solution.as_ref().expect("solved default");
    assert_eq!(solution.dimension, 2);
    assert!(app.state.ode_error.is_none());
    let epoch = app.state.ode_epoch;
    assert!(epoch > 0);
    app.state.set_field_mode(FieldMode::Formula);
    assert_eq!(app.state.formula, "y, -x, 0");
    assert!(
        app.state.ode_solution.is_some(),
        "leaving Ode preserves the last solution"
    );
    assert_eq!(app.state.ode_epoch, epoch);
    app.clock.reset();
    assert_eq!(app.clock.time, 0.0);
}

#[test]
fn ode_clicks_do_not_seed() {
    let mut app = VfApp::new();
    app.state.set_streamlines(true);
    app.state.set_particles(true);
    app.state.set_field_mode(FieldMode::Ode);
    app.test_harness().advance_simulation();
    let rect = click_rect();
    let seeds_before = app.test_harness().click_seed_count();
    let particles_before = app.test_harness().particle_snapshot().3;
    app.test_harness().apply_click(
        LeftClickAction::SeedStreamline,
        rect.center(),
        rect,
        4.0 / 3.0,
        1.0,
    );
    app.test_harness().apply_click(
        LeftClickAction::AddParticles,
        rect.center(),
        rect,
        4.0 / 3.0,
        2.0,
    );
    assert_eq!(
        app.test_harness().click_seed_count(),
        seeds_before,
        "Ode seeds no streamlines"
    );
    assert_eq!(
        app.test_harness().particle_snapshot().3,
        particles_before,
        "Ode spawns no particles"
    );
    assert!(app.status.hint.is_none(), "Ode clicks set no feature hint");
}

#[test]
fn first_solution_after_entry_fits_once_and_later_epochs_preserve_the_camera() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Ode);
    app.test_harness().advance_simulation();
    let solution = app.state.ode_solution.as_ref().expect("solution").clone();
    let center = solution.view_domain().center();
    assert!(close(app.orbit.target.x, center.x));
    assert!(close(app.orbit.target.y, center.y));
    assert!(close(app.orbit.target.z, center.z));
    let target = app.orbit.target;

    app.state.set_mode(true);
    app.test_harness().advance_simulation();
    assert_eq!(app.orbit.target, target, "projection keeps the camera");

    let mut edited = app.state.ode.clone();
    edited.duration = 5.0;
    app.state.set_ode_scene(edited).expect("edited scene");
    app.test_harness().advance_simulation();
    assert_eq!(
        app.orbit.target, target,
        "a later epoch preserves the camera"
    );
}

#[test]
fn unrelated_style_edit_does_not_restart_current_field_playback() {
    let mut app = VfApp::new();
    app.state.set_formula("y, -x".into());
    app.state.ode.source = OdeSource::CurrentField;
    app.state.set_field_mode(FieldMode::Ode);
    app.test_harness().advance_simulation();
    assert!(app.state.ode_solution.is_some());

    app.clock.seek(0.5);
    app.state.set_time(app.clock.time);
    let epoch = app.state.ode_epoch;
    let time = app.clock.time;

    app.state.set_show_grid(!app.state.show_grid);
    app.test_harness().advance_simulation();

    assert_eq!(
        app.state.ode_epoch, epoch,
        "a style edit keeps the CurrentField solution"
    );
    assert_eq!(
        app.clock.time, time,
        "a style edit does not restart playback"
    );
    assert!(close(app.clock.time, 0.5));
}

#[test]
fn reentering_ode_keeps_the_recorded_error() {
    let mut app = VfApp::new();
    app.state.set_formula("y, -x +".into());
    app.state.ode.source = OdeSource::CurrentField;
    app.state.set_field_mode(FieldMode::Ode);
    app.test_harness().advance_simulation();
    let error = app
        .state
        .ode_error
        .clone()
        .expect("the invalid entry records an error");
    assert!(app.state.ode_solution.is_none());

    app.state.set_field_mode(FieldMode::Formula);
    app.state.set_field_mode(FieldMode::Ode);
    assert_eq!(
        app.state.ode_error.as_deref(),
        Some(error.as_str()),
        "mode re-entry keeps the actionable error until a solve clears it"
    );
}

#[test]
fn first_valid_solution_after_invalid_entry_autofits() {
    let mut app = VfApp::new();
    app.state.set_formula("y, -x +".into());
    app.state.ode.source = OdeSource::CurrentField;
    app.state.set_field_mode(FieldMode::Ode);
    app.test_harness().advance_simulation();
    assert!(
        app.state.ode_solution.is_none(),
        "an invalid CurrentField entry has no solution"
    );
    assert!(app.state.ode_error.is_some());

    // A camera the deferred auto-fit cannot leave untouched.
    app.orbit.target.x = 9.0;
    app.orbit.target.y = -7.0;
    app.orbit.target.z = 0.0;
    app.orbit.ortho_zoom = 4.0;
    let target = app.orbit.target;
    let zoom = app.orbit.ortho_zoom;

    app.state.set_formula("y, -x".into());
    app.test_harness().advance_simulation();
    let center = app
        .state
        .ode_solution
        .as_ref()
        .expect("late solution")
        .view_domain()
        .center();
    assert!(app.state.ode_error.is_none());
    assert_ne!(app.orbit.target, target, "the first solution refits");
    assert_ne!(app.orbit.ortho_zoom, zoom, "the first solution refits zoom");
    assert!(close(app.orbit.target.x, center.x));
    assert!(close(app.orbit.target.y, center.y));
    assert!(close(app.orbit.target.z, center.z));
}

#[test]
fn set_time_mirrors_only_and_never_bumps_the_solution_epoch() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Ode);
    app.test_harness().advance_simulation();
    let epoch = app.state.ode_epoch;
    app.state.set_time(1.25);
    assert_eq!(app.state.time, 1.25);
    assert_eq!(app.state.ode_epoch, epoch);
    assert!(!app.state.dirty, "set_time never dirties the ODE scene");
}
