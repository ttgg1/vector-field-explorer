//! Issue #16 Task 3: end-to-end scalar playback through the app's one clock.
//!
//! These tests drive the same production phases the frame drives
//! (`reconcile`, `advance_clock`, `advance_simulation`) and prove that a
//! time-dependent scalar resamples while playing, holds while paused or
//! invalid, recovers when fixed, samples at the current time on a kind switch,
//! and stops resampling once an animated source becomes static.

use std::sync::Arc;

use vector_field_explorer::{
    VfApp,
    plot::PlotKind,
    plot_scene::{PlotGeometry, PlotScene},
    sim_clock::{has_clock_consumer, should_animate},
};

fn step(app: &mut VfApp, dt: f32) {
    let mut harness = app.test_harness();
    harness.reconcile();
    harness.advance_clock(dt);
    harness.advance_simulation();
}

fn curve_geometry(scene: &PlotScene) -> Vec<([f32; 4], [f32; 4], [f32; 4])> {
    match &scene.geometry {
        PlotGeometry::Curve(lines) => lines
            .iter()
            .map(|line| (line.a, line.b, line.color))
            .collect(),
        PlotGeometry::Surface(_, _) => panic!("expected curve geometry"),
    }
}

fn scene(app: &mut VfApp) -> Arc<PlotScene> {
    app.test_harness()
        .plot_scene()
        .expect("the app holds a plot scene")
}

#[test]
fn animated_curve_resamples_while_playing_and_holds_while_paused() {
    let mut app = VfApp::new();
    app.state.set_plot_kind(PlotKind::Curve2D);
    app.state.set_active_source("sin(t)*x".into());
    step(&mut app, 0.1);
    let first = scene(&mut app);
    let first_geometry = curve_geometry(&first);
    let first_time = app.state.time;

    step(&mut app, 0.1);
    let second = scene(&mut app);
    assert!(second.generation > first.generation);
    assert_ne!(curve_geometry(&second), first_geometry);
    assert!(app.state.time > first_time);

    app.clock.set_playing(false);
    step(&mut app, 0.1);
    let paused = scene(&mut app);
    assert_eq!(paused.generation, second.generation);
    assert!(Arc::ptr_eq(&paused, &scene(&mut app)));
}

#[test]
fn static_scalar_is_idle_even_while_the_shared_clock_could_run() {
    let mut app = VfApp::new();
    app.state.set_plot_kind(PlotKind::Curve2D);
    app.state.set_active_source("x^2".into());
    step(&mut app, 0.1);
    let first = scene(&mut app);
    assert!(!has_clock_consumer(&app.state));
    assert!(!should_animate(&app.state, &app.clock));
    let time = app.clock.time;

    step(&mut app, 0.1);
    let second = scene(&mut app);
    assert_eq!(app.clock.time, time);
    assert_eq!(second.generation, first.generation);
    assert!(Arc::ptr_eq(&first, &second));
}

#[test]
fn invalid_edit_freezes_the_mesh_while_time_keeps_running() {
    let mut app = VfApp::new();
    app.state.set_plot_kind(PlotKind::Curve2D);
    app.state.set_active_source("sin(t)*x".into());
    step(&mut app, 0.1);
    let valid = scene(&mut app);
    let geometry = curve_geometry(&valid);
    let sampled_at = app.state.plot.curve.sampled_at;
    let time = app.clock.time;

    app.state.set_active_source("sin(t)*".into());
    step(&mut app, 0.1);
    let invalid = scene(&mut app);
    assert!(app.clock.time > time);
    assert_eq!(app.state.plot.curve.sampled_at, sampled_at);
    assert_eq!(curve_geometry(&invalid), geometry);
    assert!(invalid.status.contains("scalar parse error"));

    let frozen = scene(&mut app);
    step(&mut app, 0.1);
    let still_frozen = scene(&mut app);
    assert!(Arc::ptr_eq(&frozen, &still_frozen));
    assert!(app.clock.time > time);
}

#[test]
fn fixing_the_source_resumes_at_the_current_time() {
    let mut app = VfApp::new();
    app.state.set_plot_kind(PlotKind::Curve2D);
    app.state.set_active_source("sin(t)*x".into());
    step(&mut app, 0.1);
    let before = scene(&mut app).generation;
    app.state.set_active_source("sin(t)*".into());
    step(&mut app, 0.1);
    let frozen = scene(&mut app).generation;

    app.state.set_active_source("cos(t)*x".into());
    step(&mut app, 0.1);
    let recovered = scene(&mut app);
    assert!(recovered.generation > frozen);
    assert!(recovered.generation > before);
    assert!(app.state.plot.curve.error.is_none());
    assert_eq!(app.state.plot.curve.sampled_at, Some(app.state.time));
}

#[test]
fn kind_switch_samples_at_the_current_clock_time() {
    let mut app = VfApp::new();
    app.state.set_plot_kind(PlotKind::Curve2D);
    app.state.set_active_source("sin(t)*x".into());
    step(&mut app, 0.1);
    step(&mut app, 0.1);
    let switched_at = app.state.time;

    app.state.set_plot_kind(PlotKind::Surface3D);
    app.state.set_active_source("x+y+t".into());
    step(&mut app, 0.0);
    let surface = scene(&mut app);
    assert_eq!(surface.kind, PlotKind::Surface3D);
    assert_eq!(app.state.plot.surface.sampled_at, Some(switched_at));

    let curve_time = app.state.time;
    app.state.set_plot_kind(PlotKind::Curve2D);
    step(&mut app, 0.0);
    let curve = scene(&mut app);
    assert_eq!(curve.kind, PlotKind::Curve2D);
    assert_eq!(app.state.plot.curve.sampled_at, Some(curve_time));
}

#[test]
fn animated_scalar_switched_to_static_stops_resampling_as_time_advances() {
    let mut app = VfApp::new();
    app.state.set_plot_kind(PlotKind::Curve2D);
    app.state.set_active_source("sin(t)*x".into());
    step(&mut app, 0.1);
    let animated = scene(&mut app);
    let animated_at = app.state.plot.curve.sampled_at;
    assert_eq!(animated_at, Some(app.state.time));

    app.state.set_active_source("x^2".into());
    step(&mut app, 0.1);
    let static_first = scene(&mut app);
    let static_at = app.state.plot.curve.sampled_at;
    assert_eq!(static_at, Some(app.state.time));
    assert!(!has_clock_consumer(&app.state));
    assert_ne!(
        static_first.generation, animated.generation,
        "the source edit rebuilt the plot"
    );

    // A static plot has no clock consumer, so the shared clock holds; move it
    // explicitly to prove a later time cannot resample the static mesh.
    app.clock.time += 2.0;
    step(&mut app, 0.1);
    let later = scene(&mut app);
    assert!(
        app.state.time > static_at.expect("the static plot sampled"),
        "the mirrored time advanced past the static sample"
    );
    assert_eq!(
        app.state.plot.curve.sampled_at, static_at,
        "the static plot keeps its sampled time"
    );
    assert_eq!(later.generation, static_first.generation);
    assert!(Arc::ptr_eq(&static_first, &later));
}
