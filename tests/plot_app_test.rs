//! Task 6 integration evidence: the app's scalar mode pauses the vector
//! simulation, guards SVG execution and dispatches captures from the same
//! scene the viewport displays. No GPU device is created here.

use std::sync::Arc;
use std::sync::mpsc::channel;
use std::time::Instant;

use eframe::egui::{self, pos2, vec2};
use glam::Vec3;
use vector_field_explorer::VfApp;
use vector_field_explorer::app::plot::SvgExportRoute;
use vector_field_explorer::camera::OrbitCamera;
use vector_field_explorer::fly_camera::FlyCamera;
use vector_field_explorer::plot::PlotKind;
use vector_field_explorer::plot_camera::PlotCamera;
use vector_field_explorer::reference::reference_lines;
use vector_field_explorer::screenshot::{
    CaptureReport, SCALAR_SVG_MESSAGE, SaveOutcome, Screenshot,
};
use vector_field_explorer::sim_clock::{has_clock_consumer, should_animate};
use vector_field_explorer::view_command::ViewCommand;
use vector_field_explorer::viewmodel::LeftClickAction;

use crate::support;

fn step(app: &mut VfApp, dt: f32) {
    let mut harness = app.test_harness();
    harness.reconcile();
    harness.advance_clock(dt);
    harness.advance_simulation();
}

fn snapshot(app: &mut VfApp) -> (Vec<Vec3>, Vec<Vec<Vec3>>, f32, usize) {
    app.test_harness().particle_snapshot()
}

#[test]
fn plot_mode_freezes_vector_consumers_and_restores() {
    let mut app = VfApp::new();
    app.state.set_mode(true);
    app.state.set_formula("(x+t, y, z)".into());
    app.state.particles = true;
    app.state.particle_appearance.trail_len = 8;
    step(&mut app, 0.05);
    step(&mut app, 0.05);

    let (held_positions, held_trails, held_time, held_count) = snapshot(&mut app);
    assert!(!held_positions.is_empty(), "the vector cloud spawned");
    assert!(
        held_trails.iter().any(|trail| trail.len() > 1),
        "trails advanced"
    );
    assert_eq!(held_time, 0.1);
    let vector_orbit = app.orbit;

    app.state.set_plot_kind(PlotKind::Curve2D);
    {
        let mut harness = app.test_harness();
        harness.reconcile();
        harness.sync_fly_capture(&support::context());
    }

    assert!(!has_clock_consumer(&app.state));
    assert!(!should_animate(&app.state, &app.clock));
    assert!(app.test_harness().scene_is_plot(), "plot scene is cached");
    assert_eq!(
        app.test_harness().plot_scene().unwrap().kind,
        PlotKind::Curve2D
    );

    for _ in 0..5 {
        step(&mut app, 0.05);
    }
    let (frozen_positions, frozen_trails, frozen_time, frozen_count) = snapshot(&mut app);
    assert_eq!(
        frozen_positions, held_positions,
        "particle positions are held"
    );
    assert_eq!(frozen_trails, held_trails, "trail history is held");
    assert_eq!(frozen_time, held_time, "simulated time is held");
    assert_eq!(frozen_count, held_count, "the cloud is paused, not deleted");

    let rect = egui::Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 200.0));
    {
        let mut harness = app.test_harness();
        harness.apply_click(
            LeftClickAction::SeedStreamline,
            rect.center(),
            rect,
            2.0,
            1.0,
        );
        harness.apply_click(LeftClickAction::AddParticles, rect.center(), rect, 2.0, 1.0);
        assert_eq!(
            harness.click_seed_count(),
            0,
            "no click seeding while plotting"
        );
    }
    assert_eq!(
        snapshot(&mut app).3,
        held_count,
        "a click cannot add particles"
    );

    // References follow the explicit scalar kind, never the vector arity.
    app.state.set_plot_kind(PlotKind::VectorField);
    step(&mut app, 0.05);
    app.state.set_formula("(x, y)".into());
    assert_eq!(app.state.field.dimension, 2);
    app.state.set_plot_kind(PlotKind::Surface3D);
    step(&mut app, 0.05);
    let surface = app.test_harness().plot_scene().unwrap().clone();
    let expected_3d = reference_lines(&app.state.domain, true);
    assert_eq!(
        surface.references.len(),
        expected_3d.len(),
        "surface uses 3D references"
    );

    app.state.set_plot_kind(PlotKind::Curve2D);
    step(&mut app, 0.05);
    let curve = app.test_harness().plot_scene().unwrap().clone();
    let expected_2d = reference_lines(&app.state.domain, false);
    assert_eq!(
        curve.references.len(),
        expected_2d.len(),
        "curve uses 2D references"
    );

    // Returning to vector restores the saved pose/projection and resumes from
    // the held time without catch-up.
    let resumed_from = app.clock.time;
    app.state.set_plot_kind(PlotKind::VectorField);
    step(&mut app, 0.05);
    assert!(app.state.is_vector_mode());
    assert!(app.state.mode_3d);
    assert_eq!(app.orbit, vector_orbit, "vector pose restored");
    assert_eq!(app.clock.time, resumed_from + 0.05, "resumes, no catch-up");
    assert!(app.state.particles, "vector settings survive");
    assert_eq!(app.state.particle_appearance.trail_len, 8);
    let (resumed_positions, resumed_trails, _, resumed_count) = snapshot(&mut app);
    assert_eq!(
        resumed_count, held_count,
        "the cloud is restored, not rebuilt"
    );
    assert!(resumed_trails.iter().any(|trail| trail.len() > 1));
    assert_ne!(
        resumed_positions, held_positions,
        "vector consumption resumes"
    );
}

#[test]
fn kind_change_releases_pointer_and_preserves_camera() {
    let ctx = support::context();
    let mut app = VfApp::new();
    app.state.set_mode(true);
    app.state.set_formula("(x, y, z)".into());
    app.orbit = OrbitCamera {
        yaw: 1.1,
        pitch: 0.4,
        distance: 7.0,
        target: Vec3::new(0.25, -0.5, 0.75),
        fov_y: 0.9,
        ortho_zoom: 2.5,
    };
    step(&mut app, 0.01);
    let pose = app.orbit;

    app.fly = Some(FlyCamera::default());
    {
        let mut harness = app.test_harness();
        harness.set_capture_active(true);
    }

    app.state.set_plot_kind(PlotKind::Curve2D);
    {
        let mut harness = app.test_harness();
        harness.reconcile();
        harness.sync_fly_capture(&ctx);
        assert!(!harness.capture_active(), "captured pointer is released");
        assert_eq!(harness.saved_vector_orbit(), Some(pose));

        let plot_before = harness.plot_camera().clone();
        harness.apply_view_command(ViewCommand::Orbit { dx: 30.0, dy: 12.0 });
        harness.apply_view_command(ViewCommand::Zoom { factor: 2.0 });
        assert_ne!(harness.plot_camera().bits(), plot_before.bits());
        assert_eq!(harness.saved_vector_orbit(), Some(pose));
    }
    assert!(app.fly.is_none(), "a kind change exits fly");
    assert_eq!(app.orbit, pose, "scalar entry leaves the vector pose alone");

    app.state.set_plot_kind(PlotKind::Surface3D);
    {
        let mut harness = app.test_harness();
        harness.reconcile();
        assert_eq!(harness.saved_vector_orbit(), Some(pose));
    }
    assert_eq!(app.orbit, pose);
    assert!(app.fly.is_none());

    // A successful Fit plot leaves the saved vector pose untouched.
    {
        let mut harness = app.test_harness();
        let saved = harness.saved_vector_orbit().unwrap();
        let before = harness.plot_camera().clone();
        harness.fit_plot(1.5).expect("a surface with bounds fits");
        assert_eq!(harness.saved_vector_orbit(), Some(saved));
        assert_ne!(harness.plot_camera().bits(), before.bits());
    }
    assert_eq!(app.orbit, pose);

    // An empty destination clears geometry, so Fit fails and retains the view.
    app.state.set_plot_kind(PlotKind::Curve2D);
    app.state.set_plot_kind(PlotKind::Surface3D);
    app.state.set_active_source("x +".into());
    app.state.set_plot_kind(PlotKind::VectorField);
    app.state.set_plot_kind(PlotKind::Surface3D);
    app.fly = Some(FlyCamera::default());
    {
        let mut harness = app.test_harness();
        harness.reconcile();
        let before = harness.plot_camera().clone();
        let error = harness.fit_plot(1.5).unwrap_err();
        assert!(!error.to_string().is_empty());
        assert_eq!(
            harness.plot_camera().bits(),
            before.bits(),
            "failed Fit retains the view"
        );
        assert!(
            !harness.plot_status().is_empty(),
            "failed Fit reports an error"
        );
    }
    assert!(app.fly.is_none(), "a failed Fit exits fly");
    assert_eq!(app.orbit, pose);

    app.state.set_plot_kind(PlotKind::VectorField);
    step(&mut app, 0.01);
    assert_eq!(app.orbit, pose, "returning restores the exact vector pose");
    assert!(app.state.mode_3d, "the saved vector projection returns");
    assert!(app.fly.is_none());
}

#[test]
fn plot_capture_dispatch_is_scene_exact() {
    let ctx = support::context();
    let mut app = VfApp::new();
    app.state.set_plot_kind(PlotKind::Curve2D);
    app.state.set_active_source("x^2".into());
    {
        let mut harness = app.test_harness();
        harness.reconcile();
    }

    let live = app.test_harness().plot_scene().unwrap().clone();
    app.screenshot.request();
    let rect = egui::Rect::from_min_size(pos2(0.0, 0.0), vec2(10_000.0, 3_000.0));
    let (scene, frame, extent, _generation) = app
        .test_harness()
        .begin_plot_png_plan(rect, 1.0)
        .expect("the live plot is captured");
    assert!(
        Arc::ptr_eq(&scene, &live),
        "capture shares the live scene Arc"
    );
    assert_eq!(scene.generation, live.generation);
    assert_eq!(extent[0], 4096, "existing capture cap still applies");
    assert_eq!(
        frame.aspect,
        rect.width() / rect.height(),
        "camera aspect comes from the source viewport, not the capped extent"
    );

    // Fractional DPI rounds the capture extent; the frame aspect stays exact.
    app.screenshot.request();
    let rect = egui::Rect::from_min_size(pos2(0.0, 0.0), vec2(801.0, 401.0));
    let (_, frame, extent, _) = app
        .test_harness()
        .begin_plot_png_plan(rect, 1.5)
        .expect("the live plot is captured");
    assert_eq!(extent, [1202, 602]);
    assert_eq!(frame.aspect, 801.0 / 401.0);

    // An invalid edit keeps displaying and capturing the last valid plot, not
    // a vector snapshot.
    app.state.set_active_source("x +".into());
    app.test_harness().advance_simulation();
    let displayed = app.test_harness().plot_scene().unwrap().clone();
    assert!(!displayed.status.is_empty());
    assert!(displayed.bounds.is_some());
    app.screenshot.request();
    let (captured, frame, _, _) = app
        .test_harness()
        .begin_plot_png_plan(rect, 1.0)
        .expect("the last valid plot is captured");
    assert!(Arc::ptr_eq(&captured, &displayed));
    assert_eq!(captured.kind, PlotKind::Curve2D);
    assert_eq!(frame.aspect, rect.width() / rect.height());

    // Setup/map/save failures reopen the slot, and Reset invalidates an
    // in-flight completion so it cannot attach to a later request.
    let (tx, rx) = channel();
    let mut shot = Screenshot::with_capture_channel(tx, rx);
    shot.request();
    assert!(shot.take_request());
    let _first = shot.capture_sink().expect("capture token");
    shot.fail("setup failed".into());
    assert!(
        shot.toast_text().unwrap().contains("setup failed"),
        "setup failure reports through the existing toast"
    );
    shot.request();
    assert!(shot.take_request(), "a setup failure reopens the slot");
    let second = shot.capture_sink().expect("capture token");
    second.send(SaveOutcome::Failed("map failed".into()));
    shot.tick(Instant::now());
    assert!(shot.toast_text().unwrap().contains("map failed"));
    shot.request();
    assert!(shot.take_request(), "a map failure reopens the slot");

    let stale = shot.capture_sink().expect("capture token");
    shot.cancel_pending();
    shot.request();
    assert!(shot.take_request(), "Reset leaves a fresh request possible");
    let fresh = shot.capture_sink().expect("capture token");
    shot.notify("stale toast".into(), true);
    stale.send(SaveOutcome::Saved("/tmp/stale.png".into()));
    shot.tick(Instant::now());
    assert!(
        shot.toast_text() != Some("saved /tmp/stale.png"),
        "a cancelled capture completion cannot attach to the new request"
    );
    fresh.send(SaveOutcome::DownloadStarted);
    shot.tick(Instant::now());
    assert_eq!(shot.toast_text(), Some("screenshot download started"));

    // The fixture sender is the legacy plain channel; keep it exercised too.
    let (tx, rx) = channel();
    let mut plain = Screenshot::with_channel(tx, rx);
    plain.request();
    assert!(plain.take_request());
    plain.sender().send(SaveOutcome::Cancelled).unwrap();
    plain.tick(Instant::now());
    assert_eq!(plain.toast_text(), Some("screenshot cancelled"));

    // The app-level request is consumed exactly once.
    app.screenshot.request();
    assert!(app.test_harness().begin_plot_png_plan(rect, 1.0).is_some());
    assert!(app.test_harness().begin_plot_png_plan(rect, 1.0).is_none());
    let _ = ctx;
}

#[test]
fn scalar_svg_is_guarded_at_execution() {
    let ctx = support::context();
    let mut app = VfApp::new();
    for kind in [PlotKind::Curve2D, PlotKind::Surface3D] {
        app.state.set_plot_kind(kind);
        app.test_harness().menu_svg_export(&ctx);
        assert_eq!(app.screenshot.toast_text(), Some(SCALAR_SVG_MESSAGE));
        assert!(
            !app.test_harness().svg_request_pending(),
            "the menu route never queues an SVG export"
        );

        // Even a programmatic request is rejected at the final viewport route.
        app.screenshot.request_svg();
        let route = app.test_harness().svg_dispatch();
        assert_eq!(
            route,
            SvgExportRoute::Rejected(SCALAR_SVG_MESSAGE.to_string())
        );
        assert_eq!(app.screenshot.toast_text(), Some(SCALAR_SVG_MESSAGE));
        assert!(!app.test_harness().svg_request_pending());
    }

    // Existing vector 2D SVG behavior is unchanged.
    app.state.set_plot_kind(PlotKind::VectorField);
    app.state.set_mode(false);
    app.screenshot.request_svg();
    assert_eq!(app.test_harness().svg_dispatch(), SvgExportRoute::Allowed);
    assert!(!app.test_harness().svg_request_pending());
}

#[test]
fn global_reset_clears_plot_and_capture() {
    let mut app = VfApp::new();
    app.state.set_mode(true);
    app.state.set_plot_kind(PlotKind::Surface3D);
    app.state.set_active_source("sin(x)*cos(y)".into());
    step(&mut app, 0.05);
    app.test_harness()
        .apply_view_command(ViewCommand::Orbit { dx: 20.0, dy: 15.0 });
    app.screenshot.request();
    assert!(app.screenshot.take_request());
    app.screenshot.notify("pending capture".into(), true);
    assert!(app.test_harness().scene_is_plot());

    app.test_harness().reset_all();

    assert!(
        app.state.is_vector_mode(),
        "global reset returns to vector mode"
    );
    assert!(app.state.mode_3d, "the saved vector projection is restored");
    assert_eq!(app.state.plot.curve.source, "0.5*x^2");
    assert_eq!(app.state.plot.surface.source, "exp(-(x^2+y^2))");
    assert!(app.state.plot.curve.samples.is_none());
    assert!(app.state.plot.surface.samples.is_none());
    assert!(app.state.plot.curve.error.is_none());
    assert!(app.state.plot.surface.error.is_none());
    assert_eq!(
        app.state.plot.settings.curve_samples,
        vector_field_explorer::plot_state::PlotSettings::default().curve_samples
    );
    assert_eq!(
        app.clock,
        vector_field_explorer::sim_clock::SimClock::default()
    );
    assert_eq!(
        app.test_harness().plot_camera().bits(),
        PlotCamera::default().bits()
    );
    assert!(
        app.test_harness().plot_scene().is_none(),
        "plot resources cleared"
    );
    assert!(app.test_harness().plot_release_pending());
    assert!(
        app.screenshot.toast_text().is_none(),
        "capture toast cleared"
    );
    assert!(!app.screenshot.take_request(), "pending capture cancelled");
}

trait CameraBits {
    fn bits(&self) -> Vec<u64>;
}

impl CameraBits for PlotCamera {
    fn bits(&self) -> Vec<u64> {
        self.target
            .to_array()
            .into_iter()
            .chain([
                self.distance,
                self.ortho_extent,
                self.yaw,
                self.pitch,
                self.fov_y,
                self.fitted_bounds.min.x,
                self.fitted_bounds.min.y,
                self.fitted_bounds.min.z,
                self.fitted_bounds.max.x,
                self.fitted_bounds.max.y,
                self.fitted_bounds.max.z,
            ])
            .map(f64::to_bits)
            .collect()
    }
}

#[allow(dead_code)]
fn _assert_capture_report_is_public(report: CaptureReport) -> u64 {
    report.generation
}
