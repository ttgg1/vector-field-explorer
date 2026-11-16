//! Task 4: particles live in the plane slice's local frame and advect with the
//! projected flow, while ordinary two-component 2D still has no cloud.

use vector_field_explorer::VfApp;
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
