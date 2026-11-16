//! Task 4: particles live in the plane slice's local frame and advect with the
//! projected flow, while ordinary two-component 2D still has no cloud.

use eframe::egui;
use glam::Vec3;
use vector_field_explorer::VfApp;
use vector_field_explorer::plane::PlaneAxis;
use vector_field_explorer::sim_clock::has_clock_consumer;
use vector_field_explorer::view_command::{ClickActionPlan, click_action_plan};
use vector_field_explorer::viewmodel::{LeftClickAction, State};

fn step(app: &mut VfApp, dt: f32) {
    let mut harness = app.test_harness();
    harness.advance_clock(dt);
    harness.advance_simulation();
}

#[test]
fn plane_particles_spawn_and_stay_on_slice() {
    let mut app = VfApp::new();
    app.state.set_formula("x, y, z".into());
    app.state.set_mode(false);
    app.state.set_particles(true);
    step(&mut app, 0.05);
    let (positions, trails, _, _) = app.test_harness().particle_snapshot();
    assert!(!positions.is_empty());
    assert!(positions.iter().all(|p| p.z.abs() < 1e-5));
    assert!(trails.iter().flatten().all(|p| p.z.abs() < 1e-5));
}

#[test]
fn plane_particles_advect_with_projected_flow() {
    let mut app = VfApp::new();
    app.state.set_formula("1, 0, 100".into());
    app.state.set_mode(false);
    app.state.set_particles(true);
    step(&mut app, 0.05);
    let before = app.test_harness().particle_snapshot().0;
    step(&mut app, 0.05);
    let after = app.test_harness().particle_snapshot().0;
    assert!(!before.is_empty() && before.len() == after.len());
    assert!(after.iter().all(|p| p.z.abs() < 1e-5));
    assert!(
        before
            .iter()
            .zip(&after)
            .any(|(a, b)| (a.x - b.x).abs() > 1e-6),
        "the in-plane component must move the particles"
    );
}

#[test]
fn ordinary_two_component_particles_remain_disabled() {
    let mut app = VfApp::new();
    app.state.set_particles(true);
    step(&mut app, 0.05);
    assert!(app.test_harness().particle_snapshot().0.is_empty());
}

#[test]
fn plane_particles_consume_the_clock() {
    let mut state = State::new(false);
    state.set_formula("x, y, z".into());
    state.set_particles(true);
    assert!(state.particle_target().is_some());
    assert!(has_clock_consumer(&state));
    state.set_particles(false);
    assert!(!has_clock_consumer(&state));
}

#[test]
fn click_plan_allows_particles_for_a_3d_field_in_2d() {
    assert_eq!(
        click_action_plan(LeftClickAction::AddParticles, false, true, false),
        ClickActionPlan::AddParticles
    );
}

#[test]
fn plane_transition_clears_seeds_and_refills_particles() {
    let mut app = VfApp::new();
    app.state.set_formula("y, -x, 1".into());
    app.state.set_mode(true);
    app.state.set_streamlines(true);
    app.state.set_particles(true);
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(200.0, 200.0));
    app.test_harness().advance_simulation();
    app.test_harness().apply_click(
        LeftClickAction::SeedStreamline,
        rect.center(),
        rect,
        1.0,
        0.0,
    );
    assert_eq!(app.test_harness().click_seed_count(), 1);

    app.state.set_mode(false);
    app.test_harness().advance_simulation();
    assert_eq!(app.test_harness().click_seed_count(), 0);
    let positions = app.test_harness().particle_snapshot().0;
    assert!(!positions.is_empty());
    assert!(positions.iter().all(|p| p.z.abs() < 1e-5));
}

#[test]
fn plane_axis_change_clears_stale_click_seeds() {
    let mut app = VfApp::new();
    app.state.set_formula("y, -x, 1".into());
    app.state.set_mode(false);
    app.state.set_streamlines(true);
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(200.0, 200.0));
    app.test_harness().advance_simulation();
    app.test_harness().apply_click(
        LeftClickAction::SeedStreamline,
        rect.center(),
        rect,
        1.0,
        0.0,
    );
    assert_eq!(app.test_harness().click_seed_count(), 1);

    app.state.set_plane_axis(PlaneAxis::X);
    app.test_harness().advance_simulation();
    assert_eq!(app.test_harness().click_seed_count(), 0);
}

#[test]
fn world_mode_keeps_existing_click_seeds() {
    let mut app = VfApp::new();
    app.state.set_formula("y, -x, 1".into());
    app.state.set_mode(true);
    app.state.set_streamlines(true);
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(200.0, 200.0));
    app.test_harness().advance_simulation();
    app.test_harness().apply_click(
        LeftClickAction::SeedStreamline,
        rect.center(),
        rect,
        1.0,
        0.0,
    );
    assert_eq!(app.test_harness().click_seed_count(), 1);

    app.state.set_formula("x, -y, 2".into());
    app.test_harness().advance_simulation();
    assert_eq!(app.test_harness().click_seed_count(), 1);
}

#[test]
fn plane_enter_fits_local_camera() {
    let mut app = VfApp::new();
    app.state.set_formula("x, y, z".into());
    app.state
        .set_domain(Vec3::new(10.0, -2.0, -3.0), Vec3::new(20.0, 2.0, 3.0));
    app.state.set_mode(true);
    app.test_harness().advance_simulation();
    app.state.set_mode(false);
    app.test_harness().advance_simulation();
    assert!(app.orbit.target.length() < 1e-5);
    assert!(app.orbit.ortho_zoom.is_finite() && app.orbit.ortho_zoom > 0.0);
}

#[test]
fn purely_normal_plane_particles_stay_in_place() {
    let mut app = VfApp::new();
    app.state.set_formula("0, 0, 1".into());
    app.state.set_mode(false);
    app.state.set_particles(true);
    step(&mut app, 0.05);
    let before = app.test_harness().particle_snapshot().0;
    assert!(!before.is_empty());
    step(&mut app, 0.05);
    let after = app.test_harness().particle_snapshot().0;
    assert_eq!(before.len(), after.len(), "the cloud keeps its size");
    assert!(after.iter().all(|p| p.z.abs() < 1e-5));
    for (a, b) in before.iter().zip(&after) {
        assert!(
            (a.x - b.x).abs() < 1e-6 && (a.y - b.y).abs() < 1e-6,
            "a purely normal field cannot move a particle within the slice"
        );
    }
}

#[test]
fn losing_particle_availability_clears_the_cloud_and_burst_undo() {
    let mut app = VfApp::new();
    app.state.set_formula("y, -x, 1".into());
    app.state.set_mode(true);
    app.state.set_particles(true);
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(200.0, 200.0));
    app.test_harness().advance_simulation();
    app.test_harness()
        .apply_click(LeftClickAction::AddParticles, rect.center(), rect, 1.0, 0.0);
    assert!(app.test_harness().particle_snapshot().3 > 0);
    assert!(app.pending_burst());

    app.state.set_formula("y, -x".into());
    app.test_harness().advance_simulation();
    assert!(app.test_harness().particle_snapshot().0.is_empty());
    assert_eq!(app.test_harness().particle_snapshot().3, 0);
    assert!(
        !app.pending_burst(),
        "a lost particle availability must drop the stale burst undo"
    );
}
