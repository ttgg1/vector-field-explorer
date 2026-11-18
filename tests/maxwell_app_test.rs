//! Task 3 Maxwell app integration: static arrows + outline, 2D plane reuse,
//! one cached BEM solve, validation/recovery, density caps and mode isolation.

use glam::UVec3;

use vector_field_explorer::VfApp;
use vector_field_explorer::induction::InductionCase;
use vector_field_explorer::maxwell::{
    ConductorOutline, MAXWELL_SWITCH_2D_WARNING, MaxwellCase, MaxwellScene,
};
use vector_field_explorer::plane::PlaneAxis;
use vector_field_explorer::source_scene::FieldMode;

fn conductor(radius: f64) -> MaxwellScene {
    MaxwellScene {
        case: MaxwellCase::Conductor2D,
        outline: ConductorOutline::Circle { radius },
        ..MaxwellScene::default()
    }
}

#[test]
fn maxwell_mode_isolated_from_formula_and_durable_scenes() {
    let mut app = VfApp::new();
    app.state.set_formula("y, -x, 0".into());
    app.state.induction.case = InductionCase::Solenoid;
    app.state.induction.radius = 1.7;
    app.state.material.radius = 0.7;
    app.state.ode.duration = 5.0;
    app.state.sources.softening = 0.01;
    let formula = app.state.formula.clone();
    let sources = app.state.sources.clone();
    let induction = app.state.induction.clone();
    let material = app.state.material.clone();
    let ode = app.state.ode.clone();

    assert!(app.state.set_field_mode(FieldMode::Maxwell));
    app.state.set_mode(true);
    app.test_harness().advance_simulation();

    let scene = app.test_harness().test_assemble_viewport_scene();
    let sampled = app.state.maxwell_sampling().expect("sampling").arrows.len();
    assert!(sampled > 0);
    assert_eq!(scene.arrows.len(), sampled, "only Maxwell arrows are drawn");
    assert_eq!(app.state.formula, formula);
    assert_eq!(app.state.sources, sources);
    assert_eq!(app.state.induction, induction);
    assert_eq!(app.state.material, material);
    assert_eq!(app.state.ode, ode);

    assert!(app.state.set_field_mode(FieldMode::Formula));
    assert_eq!(app.state.formula, formula);
    assert_eq!(app.state.sources, sources);
    assert_eq!(app.state.induction, induction);
    assert_eq!(app.state.material, material);
    assert_eq!(app.state.ode, ode);
}

#[test]
fn maxwell_sampling_is_arrow_and_outline_only() {
    let mut app = VfApp::new();
    app.state.particles = true;
    app.state.streamlines = true;
    app.state.set_field_mode(FieldMode::Maxwell);
    app.state.set_mode(true);
    app.test_harness().advance_simulation();

    let scene = app.test_harness().test_assemble_viewport_scene();
    assert!(app.test_harness().maxwell_arrow_count() > 0);
    assert!(app.test_harness().maxwell_line_count() > 0);
    assert!(!scene.arrows.is_empty());
    assert!(!scene.lines.is_empty());
    assert!(scene.trails.is_empty());
    assert!(scene.dots.is_empty());
    assert!(!app.state.has_time_dependent_consumer());
    assert!(app.state.particle_target().is_none());
    assert_eq!(
        app.test_harness().vector_scene_counts(),
        (scene.arrows.len(), 0, 0)
    );
}

#[test]
fn maxwell_2d_uses_the_selected_plane() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Maxwell);
    let scene = MaxwellScene {
        case: MaxwellCase::HollowSphere,
        radius: 0.8,
        ..MaxwellScene::default()
    };
    assert!(app.state.set_maxwell_scene(scene));
    app.state.set_mode(false);
    app.state.set_plane_index(4);
    app.test_harness().advance_simulation();
    let first = app
        .state
        .maxwell_sampling()
        .expect("sampling")
        .arrows
        .clone();
    assert!(!first.is_empty());
    assert!(first.iter().all(|arrow| arrow.position.z.abs() < 1e-6));
    let frame = app.state.plane_frame().expect("selected plane frame");
    let first_origin = frame.origin;

    app.state.set_plane_index(20);
    app.test_harness().advance_simulation();
    let second = app
        .state
        .maxwell_sampling()
        .expect("sampling")
        .arrows
        .clone();
    assert_ne!(first, second);
    assert!(second.iter().all(|arrow| arrow.position.z.abs() < 1e-6));
    assert!(
        app.state
            .plane_frame()
            .is_some_and(|next| next.origin != first_origin)
    );
}

#[test]
fn conductor_2d_solves_once_and_reuses_the_cache() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Maxwell);
    assert!(app.state.set_maxwell_scene(conductor(1.0)));
    app.state.set_mode(false);
    let before = app.test_harness().maxwell_bem_solves();

    app.state.refresh_maxwell_sampling();
    app.state.refresh_maxwell_sampling();
    let first = app
        .state
        .maxwell_sampling()
        .expect("sampling")
        .arrows
        .clone();
    app.state.set_plane_axis(PlaneAxis::X);
    app.state.set_plane_index(4);
    app.state.refresh_maxwell_sampling();
    let second = app
        .state
        .maxwell_sampling()
        .expect("sampling")
        .arrows
        .clone();
    app.test_harness().advance_simulation();

    assert_eq!(app.test_harness().maxwell_bem_solves(), before + 1);
    assert_eq!(app.test_harness().maxwell_bem_solve_count(), before + 1);
    assert_ne!(first, second);
    assert!(app.state.maxwell_error.is_none());
}

#[test]
fn conductor_2d_3d_is_unavailable_without_solving() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Maxwell);
    assert!(app.state.set_maxwell_scene(conductor(1.0)));
    assert!(app.state.set_mode(true));
    let before = app.test_harness().maxwell_bem_solves();
    app.test_harness().advance_simulation();

    assert_eq!(app.test_harness().maxwell_bem_solves(), before);
    assert!(app.test_harness().maxwell_arrow_count() == 0);
    assert_eq!(
        app.test_harness().maxwell_sampling_error(),
        Some(MAXWELL_SWITCH_2D_WARNING)
    );
}

#[test]
fn invalid_maxwell_edit_retains_the_last_valid_scene_and_cache() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Maxwell);
    let mut valid = MaxwellScene {
        case: MaxwellCase::CoaxialElectric,
        ..MaxwellScene::default()
    };
    valid.radius = 0.8;
    valid.outer_radius = 2.0;
    assert!(app.state.set_maxwell_scene(valid.clone()));
    app.state.set_mode(true);
    app.test_harness().advance_simulation();
    let before_sampling = app.state.maxwell_sampling().cloned();

    let invalid = MaxwellScene {
        radius: f64::NAN,
        ..valid.clone()
    };
    assert!(!app.state.set_maxwell_scene(invalid));
    assert_eq!(app.state.maxwell_scene(), &valid);
    assert_eq!(app.state.maxwell_sampling().cloned(), before_sampling);
    let error = app
        .state
        .maxwell_error
        .as_deref()
        .expect("actionable error");
    assert!(error.contains("radius"), "{error:?}");

    let mut recovered = valid;
    recovered.radius = 1.5;
    assert!(app.state.set_maxwell_scene(recovered.clone()));
    app.test_harness().advance_simulation();
    assert_eq!(app.state.maxwell_scene(), &recovered);
    assert!(app.state.maxwell_error.is_none());
    assert_ne!(app.state.maxwell_sampling().cloned(), before_sampling);
}

#[test]
fn maxwell_caps_apply_per_view() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Maxwell);
    assert!(app.state.set_maxwell_scene(MaxwellScene {
        case: MaxwellCase::CoaxialElectric,
        ..MaxwellScene::default()
    }));
    app.state.set_mode(true);
    assert!(app.state.set_density(200));
    assert_eq!(app.state.density, 40);
    assert_eq!(app.state.grid(), UVec3::splat(40));
    app.test_harness().advance_simulation();
    assert!(app.test_harness().maxwell_arrow_count() > 0);
    assert!(
        app.test_harness()
            .maxwell_sampling_error()
            .is_some_and(|warning| warning.contains("cap") || warning.contains("density")),
        "the 3D cap must be visible, not silent"
    );

    app.state.set_mode(false);
    app.state.set_density(40);
    assert_eq!(app.state.density, 40);
    assert!(app.state.set_density(200));
    assert_eq!(app.state.density, 200);
    assert_eq!(app.state.grid(), UVec3::new(200, 200, 1));
    app.state.set_plane_index(100);
    app.test_harness().advance_simulation();
    assert!(app.test_harness().maxwell_arrow_count() > 0);

    // A direct field write is still clamped by the refresh path.
    app.state.density = 400;
    app.state.refresh_maxwell_sampling();
    assert_eq!(app.state.density, 200);
    assert_eq!(app.state.grid(), UVec3::new(200, 200, 1));
}

#[test]
fn maxwell_mode_does_not_consume_the_clock_or_spawn_particles() {
    let mut app = VfApp::new();
    app.state.set_mode(true);
    app.state.particles = true;
    app.state.streamlines = true;
    app.state.set_field_mode(FieldMode::Maxwell);
    app.test_harness().advance_simulation();

    assert!(!app.state.has_time_dependent_consumer());
    assert!(app.state.particle_target().is_none());
    let (_, trails, _, particle_count) = app.test_harness().particle_snapshot();
    assert_eq!(particle_count, 0);
    assert!(trails.is_empty());
    let scene = app.test_harness().test_assemble_viewport_scene();
    assert!(scene.dots.is_empty());
    assert!(scene.trails.is_empty());
}

#[test]
fn maxwell_bem_failure_is_reported_and_not_retried_per_sample() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Maxwell);
    // A finite but extreme outline passes scene validation and then makes the
    // BEM distance matrix overflow, exercising the explicit failure path.
    let mut broken = conductor(1.0e300);
    broken.outer_radius = 1.0e301;
    assert!(app.state.set_maxwell_scene(broken));
    app.state.set_mode(false);
    app.test_harness().advance_simulation();
    let solves = app.test_harness().maxwell_bem_solves();
    assert_eq!(solves, 1);
    assert!(app.test_harness().maxwell_arrow_count() == 0);
    assert!(app.test_harness().maxwell_sampling_error().is_some());

    app.state.refresh_maxwell_sampling();
    app.state.refresh_maxwell_sampling();
    assert_eq!(app.test_harness().maxwell_bem_solves(), solves);
    assert!(app.test_harness().maxwell_arrow_count() == 0);

    assert!(app.state.set_maxwell_scene(conductor(1.0)));
    app.test_harness().advance_simulation();
    assert_eq!(app.test_harness().maxwell_bem_solves(), solves + 1);
    assert!(app.test_harness().maxwell_arrow_count() > 0);
}

#[test]
fn maxwell_round_trip_preserves_projection_and_camera() {
    let mut app = VfApp::new();
    // Entering Maxwell itself must not refit the camera, even though 2D is a
    // plane slice internally.
    let initial_orbit = app.orbit;
    app.state.set_field_mode(FieldMode::Maxwell);
    app.test_harness().advance_simulation();
    assert_eq!(app.orbit, initial_orbit);
    app.state.set_field_mode(FieldMode::Formula);
    app.test_harness().advance_simulation();

    // A three-component formula keeps the 2D world on the same plane frame, so
    // the round trip tests mode preservation rather than a fresh plane fit.
    app.state.set_formula("x, y, z".into());

    app.state.set_mode(true);
    app.test_harness().advance_simulation();
    app.state.set_field_mode(FieldMode::Maxwell);
    app.test_harness().advance_simulation();
    let orbit_3d = app.orbit;
    assert!(app.state.mode_3d);
    app.state.set_field_mode(FieldMode::Formula);
    app.test_harness().advance_simulation();
    assert!(app.state.mode_3d);
    assert_eq!(app.orbit, orbit_3d);
    app.state.set_field_mode(FieldMode::Maxwell);
    app.test_harness().advance_simulation();
    assert!(app.state.mode_3d);
    assert_eq!(app.orbit, orbit_3d);

    app.state.set_mode(false);
    app.test_harness().advance_simulation();
    let orbit_2d = app.orbit;
    assert!(!app.state.mode_3d);
    app.state.set_field_mode(FieldMode::Formula);
    app.test_harness().advance_simulation();
    assert!(!app.state.mode_3d);
    assert_eq!(app.orbit, orbit_2d);
    app.state.set_field_mode(FieldMode::Maxwell);
    app.test_harness().advance_simulation();
    assert!(!app.state.mode_3d);
    assert_eq!(app.orbit, orbit_2d);
}
