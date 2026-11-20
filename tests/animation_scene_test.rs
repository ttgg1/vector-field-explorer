//! Task 2: the detached animation snapshot advances its own state and cloud.

use vector_field_explorer::VfApp;
use vector_field_explorer::animation::{ANIMATION_PARTICLE_SUBSTEP, AnimationError};
use vector_field_explorer::animation_scene::{AnimationFrame, AnimationSnapshot};
use vector_field_explorer::app::scene_color_facts_for_tests;
use vector_field_explorer::camera::OrbitCamera;
use vector_field_explorer::color_mapping::{ColorSample, ColorStats};
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
        .frame_at(0.25, 0.25, 1.0, true)
        .expect("first frame");
    let later = snapshot
        .clone()
        .frame_at(1.25, 1.0, 1.0, false)
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
        .frame_at(0.5, 0.5, 1.0, false)
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

    let _frame = snapshot
        .frame_at(0.5, 0.5, 1.0, false)
        .expect("first frame");

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

#[test]
fn aspect_changes_the_projection_without_changing_the_scene() {
    let mut state = State::new(true);
    state.set_formula("1, 0, 0".into());
    let snapshot = snapshot(state, Particles::default());

    let square = snapshot
        .clone()
        .frame_at(0.0, 0.0, 1.0, true)
        .expect("square frame");
    let wide = snapshot
        .clone()
        .frame_at(0.0, 0.0, 2.0, true)
        .expect("wide frame");

    assert!(
        (wide.view_proj.x_axis.x - square.view_proj.x_axis.x / 2.0).abs() < 1e-6,
        "a 2:1 aspect must halve the horizontal projection scale"
    );
    assert_ne!(square.view_proj, wide.view_proj);
    assert_eq!(square.scene.arrows.len(), wide.scene.arrows.len());
    for (left, right) in square.scene.arrows.iter().zip(wide.scene.arrows.iter()) {
        assert_eq!(left.origin, right.origin);
        assert_eq!(left.dir_len, right.dir_len);
        assert_eq!(left.color, right.color);
    }
    assert_eq!(snapshot.state.formula, "1, 0, 0");
    assert!(snapshot.particles.pos.is_empty());
}

#[test]
fn frame_at_rejects_nonpositive_or_nonfinite_aspect() {
    let snapshot = snapshot(State::new(true), Particles::default());
    for aspect in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        let mut snapshot = snapshot.clone();
        assert!(
            matches!(
                snapshot.frame_at(0.0, 0.0, aspect, true),
                Err(AnimationError::InvalidSpec(_))
            ),
            "aspect {aspect} must be rejected"
        );
    }
}

#[test]
fn first_frame_renders_the_captured_particle_cloud() {
    let mut app = VfApp::new();
    app.state.set_mode(true);
    app.state.set_formula("1, 0, 0".into());
    app.state.set_particles(true);
    app.state.particle_appearance.trail_len = 8;
    app.test_harness().advance_clock(0.05);
    app.test_harness().advance_simulation();

    let mut snapshot = app.animation_snapshot().expect("snapshot");
    let captured_positions = snapshot.particles.pos.clone();
    assert!(
        !captured_positions.is_empty(),
        "the captured app must have a particle cloud"
    );
    let time = snapshot.state.time;

    let frame = snapshot
        .frame_at(time, 0.0, 1.0, true)
        .expect("first frame");

    assert!(
        !frame.scene.dots.is_empty(),
        "the first frame must render the captured cloud"
    );
    assert_eq!(frame.scene.dots.len(), captured_positions.len());
    for (dot, position) in frame.scene.dots.iter().zip(captured_positions.iter()) {
        assert!((dot.origin[0] - position.x).abs() < 1e-5);
        assert!((dot.origin[1] - position.y).abs() < 1e-5);
        assert!((dot.origin[2] - position.z).abs() < 1e-5);
    }
    assert!(
        !frame.scene.trails.is_empty(),
        "captured trail history must render on the first frame"
    );
    assert!(
        !frame.scene.combined.is_empty(),
        "the 3D combined stream must include the captured dots"
    );
}

#[test]
fn scene_color_facts_collects_stats_from_drawn_arrows() {
    let mut state = State::new(false);
    state.set_formula("(x + 1, y - 1)".to_owned());
    state.set_density(8);
    let (range, stats, mapping) = scene_color_facts_for_tests(&state);
    let sampled = state.sample();
    assert!(!sampled.is_empty(), "the formula view has drawn arrows");
    let expected = ColorStats::from_samples(
        &sampled
            .iter()
            .map(|arrow| ColorSample {
                magnitude: arrow.mag,
                dir: arrow.dir,
            })
            .collect::<Vec<_>>(),
    );
    assert_eq!(stats, expected);
    assert_eq!(range, state.color_range(&sampled));
    assert_eq!(range, vector_field_explorer::sampling::magnitude_range(&sampled));
    assert!(mapping.is_none(), "the default mapping is legacy");
}
