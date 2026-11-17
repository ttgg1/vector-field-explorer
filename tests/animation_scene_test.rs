//! Task 2: the detached animation snapshot advances its own state and cloud.

use vector_field_explorer::animation::ANIMATION_PARTICLE_SUBSTEP;
use vector_field_explorer::animation_scene::{AnimationFrame, AnimationSnapshot};
use vector_field_explorer::camera::OrbitCamera;
use vector_field_explorer::field::FieldExpr;
use vector_field_explorer::particles::{self, AdvectSettings, Particles, advect};
use vector_field_explorer::viewmodel::State;

fn snapshot(state: State, particles: Particles) -> AnimationSnapshot {
    AnimationSnapshot::from_parts(state, particles, OrbitCamera::default(), None, [0.0; 4])
}

/// The exact subdivision the exporter must use for a frame step.
fn expected_substeps(dt: f32) -> usize {
    (dt / ANIMATION_PARTICLE_SUBSTEP).ceil() as usize
}

#[test]
fn snapshot_captures_the_time_dependent_formula_scene() {
    let mut state = State::new(true);
    state.set_formula("sin(t), 0, 0".into());
    // Fix the colour range so magnitude changes are visible instead of being
    // normalised away by auto-fit. The direction is always +X, so the colour
    // is the frame-varying evidence.
    state.auto_fit = false;
    state.color_min = 0.0;
    state.color_max = 1.0;
    state.set_time(0.25);
    let snapshot = snapshot(state, Particles::default());

    let first: AnimationFrame = snapshot
        .clone()
        .frame_at(0.25, 0.25, true)
        .expect("first frame");
    let later = snapshot
        .clone()
        .frame_at(1.25, 1.0, false)
        .expect("later frame");
    assert!(!first.scene.arrows.is_empty());
    assert!(!later.scene.arrows.is_empty());
    assert_ne!(
        first.scene.arrows[0].color, later.scene.arrows[0].color,
        "the sampled field magnitude follows frame time"
    );
}

#[test]
fn frame_step_does_not_use_the_gui_step_limit() {
    let mut state = State::new(true);
    state.set_formula("1, 0, 0".into());
    state.set_particles(true);
    let mut particles = Particles::default();
    particles::spawn(&mut particles, &state.domain, 1, 3);
    let start = particles.pos[0];
    let mut snapshot = snapshot(state, particles);

    let _frame = snapshot
        .frame_at(0.5, 0.5, false)
        .expect("a frame without a preceding capture frame");

    assert_eq!(snapshot.particles.pos.len(), 1);
    assert_eq!(expected_substeps(0.5), 60);
    let moved = snapshot.particles.pos[0] - start;
    assert!(
        (moved.x - 0.5).abs() < 1e-4,
        "a 0.5 s frame must be subdivided, not clamped to MAX_STEP: moved {moved:?}"
    );
}

#[test]
fn exact_substeps_use_each_substep_time() {
    let mut state = State::new(true);
    state.set_formula("sin(t), 0, 0".into());
    state.set_particles(true);
    let mut particles = Particles::default();
    particles::spawn(&mut particles, &state.domain, 1, 3);
    let start = particles.pos[0];
    let mut snapshot = snapshot(state.clone(), particles);

    let _frame = snapshot.frame_at(0.5, 0.5, false).expect("first frame");

    // Replay the same 60 nominal substeps directly through `advect`, passing
    // each substep's exact start time. A single 0.5 s step from t=0 would take
    // a different path through sin(t).
    let flow = FieldExpr::parse("sin(t), 0, 0").expect("flow");
    let mut expected = Particles::default();
    particles::spawn(&mut expected, &state.domain, 1, 3);
    for substep in 0..60 {
        advect(
            &mut expected,
            &flow,
            &state.domain,
            AdvectSettings {
                dt: ANIMATION_PARTICLE_SUBSTEP,
                time: substep as f32 * ANIMATION_PARTICLE_SUBSTEP,
                ..AdvectSettings::default()
            },
        );
    }
    let actual = snapshot.particles.pos[0];
    let expected = expected.pos[0];
    assert!(
        actual.distance(expected) < 1e-5,
        "substeps must evaluate the field at their exact time: {actual:?} vs {expected:?}"
    );
    assert!(
        actual.distance(start) > 0.1,
        "the time-dependent field must advance the point: {actual:?}"
    );
}
