//! ODE solve benchmark. Run through `scripts/bench-ode.sh`; a normal
//! `cargo test` returns immediately.

use std::time::Instant;

use vector_field_explorer::ode::{ODE_MAX_DURATION, OdeScene, OdeSeed, OdeStop, solve_ode};
use vector_field_explorer::viewmodel::State;

#[test]
fn ode_benchmark_when_requested() {
    if std::env::var_os("VFE_ODE_BENCH").is_none() {
        return;
    }
    let state = State::new(true);
    let scene = OdeScene {
        seeds: (0..8)
            .map(|i| OdeSeed {
                position: [1.0 + i as f64 * 0.1, 0.0, 0.0],
            })
            .collect(),
        ..OdeScene::default()
    };
    let start = Instant::now();
    let solution = solve_ode(&state, &scene).expect("benchmark scene");
    let elapsed = start.elapsed();
    eprintln!(
        "ode-bench seeds={} samples={} elapsed_ms={:.3}",
        solution.trajectories.len(),
        solution
            .trajectories
            .iter()
            .map(|t| t.points.len())
            .sum::<usize>(),
        elapsed.as_secs_f64() * 1e3
    );
    assert!(
        solution.stopped == 0
            || solution
                .trajectories
                .iter()
                .any(|t| t.stop == OdeStop::StepLimit)
    );
}

/// The duration cap exercise: eight seeds, duration 25, fixed step. This is
/// the near-step-limit row recorded in `docs/perf/ode-performance.md`.
#[test]
fn ode_cap_benchmark_when_requested() {
    if std::env::var_os("VFE_ODE_BENCH").is_none() {
        return;
    }
    let state = State::new(true);
    let scene = OdeScene {
        duration: ODE_MAX_DURATION,
        seeds: (0..8)
            .map(|i| OdeSeed {
                position: [1.0 + i as f64 * 0.1, 0.0, 0.0],
            })
            .collect(),
        ..OdeScene::default()
    };
    let start = Instant::now();
    let solution = solve_ode(&state, &scene).expect("benchmark scene");
    let elapsed = start.elapsed();
    let samples = solution
        .trajectories
        .iter()
        .map(|t| t.points.len())
        .sum::<usize>();
    let approx_bytes = samples * std::mem::size_of::<vector_field_explorer::ode::OdeSample>()
        + solution.trajectories.len()
            * std::mem::size_of::<vector_field_explorer::ode::OdeTrajectory>();
    eprintln!(
        "ode-cap-bench seeds={} samples={} elapsed_ms={:.3} approx_bytes={}",
        solution.trajectories.len(),
        samples,
        elapsed.as_secs_f64() * 1e3,
        approx_bytes
    );
    assert!(solution.stopped > 0);
    assert!(
        solution
            .trajectories
            .iter()
            .all(|t| t.stop == OdeStop::StepLimit)
    );
}
