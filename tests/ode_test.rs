//! Pure ODE scene, RK4 integration, bounds and marker coverage.

use glam::Vec3;
use vector_field_explorer::ode::{
    ODE_COLORS, ODE_MAX_DURATION, ODE_MAX_SAMPLES, OdeChart, OdeError, OdeScene, OdeSeed,
    OdeSolution, OdeSource, OdeStop, integrate_ode, integrate_ode_with_step, ode_axes,
    ode_trajectory_lines, prepare_ode, solve_ode,
};
use vector_field_explorer::reference::{AXIS_X_COLOR, AXIS_Y_COLOR, AXIS_Z_COLOR};
use vector_field_explorer::viewmodel::State;

fn state_with_formula(formula: &str, dimension: usize) -> State {
    let mut state = State::new(true);
    state.set_formula(formula.to_string());
    assert_eq!(state.field.dimension, dimension);
    state
}

fn typed(equations: [&str; 3], dimension: usize) -> OdeScene {
    OdeScene {
        source: OdeSource::Equations,
        dimension,
        equations: [
            equations[0].to_owned(),
            equations[1].to_owned(),
            equations[2].to_owned(),
        ],
        seeds: vec![OdeSeed {
            position: [1.0, 0.0, 0.0],
        }],
        duration: 2.0,
    }
}

#[test]
fn harmonic_oscillator_matches_the_analytic_circle() {
    let state = State::new(true);
    let scene = typed(["-y", "x", "0"], 2);
    let solution = solve_ode(&state, &scene).expect("valid harmonic scene");
    let end = solution.sample_at(0, 2.0).expect("sample at duration");
    assert!((end.time - 2.0).abs() < 1e-12);
    assert!((end.position[0] - 2.0f64.cos()).abs() <= 1e-5);
    assert!((end.position[1] - 2.0f64.sin()).abs() <= 1e-5);
    assert_eq!(solution.trajectories[0].stop, OdeStop::Complete);
    assert_eq!(solution.stopped, 0);
}

#[test]
fn damped_oscillator_decays_like_the_reference() {
    let state = State::new(true);
    let scene = typed(["-y", "x - 0.2 * y", "0"], 2);
    let solution = solve_ode(&state, &scene).expect("valid damped scene");
    let end = solution.sample_at(0, 2.0).expect("sample at duration");
    let decay = (-0.1 * 2.0f64).exp();
    let omega = (1.0f64 - 0.01).sqrt();
    let reference = decay * ((omega * 2.0).cos() + 0.1 / omega * (omega * 2.0).sin());
    assert!((end.position[0] - reference).abs() <= 1e-5);
    assert!((end.position[1] - decay * (omega * 2.0).sin() / omega).abs() <= 1e-5);
}

#[test]
fn rk4_halving_the_step_reduces_error_by_at_least_eight() {
    let state = State::new(true);
    let scene = typed(["-y", "x", "0"], 2);
    let prepared = prepare_ode(&state, &scene).expect("prepared");
    let coarse = integrate_ode_with_step(&prepared, 0.01, ODE_MAX_SAMPLES);
    let fine = integrate_ode_with_step(&prepared, 0.005, ODE_MAX_SAMPLES);
    let analytic = |_t: f64| [2.0f64.cos(), 2.0f64.sin()];
    let error = |solution: &OdeSolution| {
        let point = solution.sample_at(0, 2.0).expect("point").position;
        (point[0] - analytic(2.0)[0]).abs() + (point[1] - analytic(2.0)[1]).abs()
    };
    assert!(error(&fine) * 8.0 <= error(&coarse));
}

#[test]
fn step_limit_is_explicit_and_bounded() {
    let state = State::new(true);
    let mut scene = typed(["-y", "x", "0"], 2);
    scene.duration = ODE_MAX_DURATION;
    let solution = solve_ode(&state, &scene).expect("max duration is legal");
    assert_eq!(solution.trajectories[0].points.len(), ODE_MAX_SAMPLES);
    assert_eq!(solution.trajectories[0].stop, OdeStop::StepLimit);
    assert_eq!(solution.stopped, 1);
}

#[test]
fn nonfinite_evolution_stops_without_fake_points() {
    let state = State::new(true);
    let scene = typed(["1", "1 / (x - 1)", "0"], 2);
    let solution = solve_ode(&state, &scene).expect("valid equations");
    assert_eq!(solution.trajectories[0].stop, OdeStop::NonFinite);
    assert!(solution.trajectories[0].points.len() < ODE_MAX_SAMPLES);
    assert!(
        solution.trajectories[0]
            .points
            .iter()
            .all(|s| s.position[0].is_finite())
    );
}

#[test]
fn current_field_and_typed_rhs_agree() {
    // Both sources are the same harmonic RHS; the durable field is
    // three-dimensional and the typed scene is two-dimensional.
    let state = state_with_formula("-y, x, 0", 3);
    let typed_scene = typed(["-y", "x", "0"], 2);
    let mut current = typed_scene.clone();
    current.source = OdeSource::CurrentField;
    current.dimension = 3;
    let typed_solution = solve_ode(&state, &typed_scene).expect("typed");
    let current_solution = solve_ode(&state, &current).expect("current field");
    let typed_end = typed_solution.sample_at(0, 2.0).expect("typed end");
    let current_end = current_solution.sample_at(0, 2.0).expect("current end");
    assert!((typed_end.position[0] - current_end.position[0]).abs() <= 1e-12);
    assert!((typed_end.position[1] - current_end.position[1]).abs() <= 1e-12);
}

#[test]
fn invalid_scene_and_nonfinite_seed_are_rejected() {
    let state = State::new(true);
    let mut scene = typed(["x", "y", "0"], 2);
    scene.seeds.clear();
    assert!(matches!(
        solve_ode(&state, &scene),
        Err(OdeError::EmptySeeds)
    ));
    scene = typed(["x", "y", "0"], 2);
    scene.seeds[0].position[0] = f64::NAN;
    assert!(matches!(
        solve_ode(&state, &scene),
        Err(OdeError::NonFiniteSeed(0))
    ));
    let bad = typed(["x +", "y", "0"], 2);
    assert!(matches!(
        solve_ode(&state, &bad),
        Err(OdeError::Parse { .. })
    ));
}

#[test]
fn deterministic_lines_and_axes_are_finite() {
    let state = State::new(true);
    let mut scene = typed(["-y", "x", "0"], 2);
    scene.seeds = (0..8)
        .map(|index| OdeSeed {
            position: [1.0 + index as f64 * 0.1, 0.0, 0.0],
        })
        .collect();
    let first = solve_ode(&state, &scene).expect("first solve");
    let second = solve_ode(&state, &scene).expect("second solve");
    assert_eq!(first, second);

    let lines = ode_trajectory_lines(&first);
    assert!(!lines.is_empty());
    assert!(
        lines.iter().all(|(start, end, color, _)| start
            .iter()
            .chain(end)
            .chain(color)
            .all(|v| v.is_finite()))
    );

    let axes = ode_axes(first.view_domain(), 2);
    assert_eq!(axes.len(), 2);
    assert_eq!(axes[0].2, AXIS_X_COLOR);
    assert_eq!(axes[1].2, AXIS_Y_COLOR);
    let axes_3d = ode_axes(first.view_domain(), 3);
    assert_eq!(axes_3d.len(), 3);
    assert_eq!(axes_3d[2].2, AXIS_Z_COLOR);
}

#[test]
fn markers_clamp_and_interpolate_between_samples() {
    let state = State::new(true);
    let scene = typed(["-y", "x", "0"], 2);
    let solution = solve_ode(&state, &scene).expect("solution");
    let start = solution.markers_at(0.0);
    let end = solution.markers_at(scene.duration);
    assert!((start[0][0] - 1.0).abs() <= 1e-12);
    assert!((end[0][0] - 2.0f64.cos()).abs() <= 1e-4);
    let clamped = solution.markers_at(-10.0);
    assert_eq!(clamped, solution.markers_at(0.0));
    assert!(solution.view_domain().domain_size().z >= 1e-3);
}

#[test]
fn time_series_defaults_and_chart_variants_are_total() {
    assert_eq!(OdeChart::default(), OdeChart::Phase);
    assert_eq!(
        OdeScene::default().seeds,
        vec![OdeSeed {
            position: [1.0, 0.0, 0.0]
        }]
    );
    assert_eq!(
        OdeScene::default().equations,
        ["y".to_owned(), "-x".to_owned(), "0".to_owned()]
    );
    assert_eq!(ODE_COLORS.len(), 8);
}

#[test]
fn bounds_pad_every_axis_by_the_largest_extent() {
    let state = State::new(true);
    let mut scene = typed(["50", "0.5", "0"], 2);
    scene.seeds = vec![OdeSeed {
        position: [0.0, 0.0, 0.0],
    }];
    let solution = solve_ode(&state, &scene).expect("valid linear scene");
    let bounds = solution.bounds;
    // x spans 0..100 and y spans 0..1. Both receive 5% of the largest
    // extent (100), so every axis is padded by 5.0, not 5.0 vs 0.05.
    assert!((bounds.min[0] + 5.0).abs() <= 1e-6, "{bounds:?}");
    assert!((bounds.min[1] + 5.0).abs() <= 1e-6, "{bounds:?}");
    assert!((bounds.max[0] - bounds.min[0] - 110.0).abs() <= 1e-6);
    assert!((bounds.max[1] - bounds.min[1] - 11.0).abs() <= 1e-6);

    let mut flat = typed(["0", "0", "0"], 2);
    flat.seeds = vec![OdeSeed {
        position: [1.0, 0.0, 0.0],
    }];
    let flat = solve_ode(&state, &flat).expect("flat scene");
    for axis in 0..3 {
        assert!((flat.bounds.max[axis] - flat.bounds.min[axis] - 2e-3).abs() <= 1e-12);
    }
}

#[test]
fn current_field_uses_the_durable_domain_not_the_view_box() {
    let mut state = state_with_formula("y, -x, 0", 3);
    assert!(state.set_domain(Vec3::new(-10.0, -10.0, -10.0), Vec3::new(10.0, 10.0, 10.0)));
    let mut scene = typed(["y", "-x", "0"], 3);
    scene.source = OdeSource::CurrentField;
    scene.seeds = vec![OdeSeed {
        position: [1.0, 0.0, 0.0],
    }];
    let prepared = prepare_ode(&state, &scene).expect("prepared current field");
    let durable = state.domain;
    assert_eq!(prepared.domain(), durable);

    // Integration consumes only the prepared snapshot: later durable-domain
    // edits cannot change what was integrated.
    assert!(state.set_domain(Vec3::new(-50.0, -50.0, -50.0), Vec3::new(50.0, 50.0, 50.0)));
    let solution = integrate_ode(&prepared);
    assert_eq!(prepared.domain(), durable);

    // The camera box is the padded trajectory box, observably different from
    // the durable domain that governed integration.
    let view = solution.view_domain();
    assert_ne!(view, durable);
    assert!(view.domain_size().x <= 2.5, "{view:?}");
}
