//! Task 5 Maxwell cap benchmark. Run through `scripts/bench-maxwell.sh`; a
//! normal `cargo test` returns immediately. It records pure sampling/solve work
//! only — no frame, GPU, browser or fps claim.

use std::time::Instant;

use glam::{UVec3, Vec3};
use vector_field_explorer::domain::Domain;
use vector_field_explorer::maxwell::{
    ConductorOutline, MaxwellCase, MaxwellScene, MaxwellStop, sample_maxwell,
    sample_maxwell_with_bem, solve_bem,
};
use vector_field_explorer::plane::{PlaneAxis, PlaneFrame, PlaneView};

fn host() -> String {
    std::env::var("HOSTNAME")
        .unwrap_or_else(|_| format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH))
}

fn stop_label(stop: MaxwellStop) -> &'static str {
    match stop {
        MaxwellStop::TargetMet => "target_met",
        MaxwellStop::AccuracyNotMet => "accuracy_not_met",
        MaxwellStop::Unavailable => "unavailable",
    }
}

#[test]
fn maxwell_cap_benchmark() {
    if std::env::var_os("VFE_MAXWELL_BENCH").is_none() {
        return;
    }

    let host = host();
    let domain = Domain::new(Vec3::splat(-2.0), Vec3::splat(2.0));
    let analytic = MaxwellScene {
        case: MaxwellCase::CoaxialElectric,
        ..MaxwellScene::default()
    };
    for (label, grid, plane) in [
        ("analytic-40cubed", UVec3::splat(40), None),
        (
            "analytic-200squared",
            UVec3::new(200, 200, 1),
            Some(PlaneView {
                enabled: true,
                axis: PlaneAxis::Z,
                index: 0,
            }),
        ),
    ] {
        let frame = plane.map(|view| PlaneFrame::new(&domain, view, grid));
        let started = Instant::now();
        let sampling = sample_maxwell(&analytic, &domain, grid, frame.as_ref());
        let elapsed = started.elapsed();
        std::hint::black_box(&sampling);
        println!(
            "MAXWELL BENCH host={host} row={label} case={:?} points={} ms={:.3} arrows={} omitted={} omitted_large={} accuracy={}",
            analytic.case,
            grid.x as u64 * grid.y as u64 * grid.z as u64,
            elapsed.as_secs_f64() * 1000.0,
            sampling.arrows.len(),
            sampling.omitted,
            sampling.omitted_large,
            stop_label(sampling.accuracy),
        );
        assert!(
            elapsed.as_secs_f64() < 2.0,
            "{label} sampling exceeded the 2 s cap: {elapsed:?}"
        );
    }

    let mut conductor = MaxwellScene {
        case: MaxwellCase::Conductor2D,
        outline: ConductorOutline::Circle { radius: 1.0 },
        elements: 256,
        ..MaxwellScene::default()
    };
    let started = Instant::now();
    let solution = solve_bem(&conductor).expect("BEM benchmark solve");
    let solve_elapsed = started.elapsed();
    let convergence = solution.convergence();
    println!(
        "MAXWELL BENCH host={host} row=bem-256-128 case={:?} solve_ms={:.3} work={} coarse={} fine={} relative_difference={:.6e} target={:.1e} net_charge={:.3e}",
        conductor.case,
        solve_elapsed.as_secs_f64() * 1000.0,
        solution.work_units,
        convergence.coarse_elements,
        convergence.fine_elements,
        convergence.relative_difference,
        convergence.target,
        solution.net_charge,
    );
    assert!(
        solve_elapsed.as_secs_f64() < 2.0,
        "BEM 256/128 solve exceeded the 2 s cap: {solve_elapsed:?}"
    );

    conductor.allow_inaccurate = true;
    let grid = UVec3::new(200, 200, 1);
    let frame = PlaneFrame::new(
        &domain,
        PlaneView {
            enabled: true,
            axis: PlaneAxis::Z,
            index: 0,
        },
        grid,
    );
    let started = Instant::now();
    let sampling =
        sample_maxwell_with_bem(&conductor, &domain, grid, Some(&frame), Some(&solution));
    let sample_elapsed = started.elapsed();
    std::hint::black_box(&sampling);
    println!(
        "MAXWELL BENCH host={host} row=bem-256-128-sampling points={} sample_ms={:.3} arrows={} omitted={} omitted_large={} accuracy={} convergence={:?}",
        grid.x as u64 * grid.y as u64,
        sample_elapsed.as_secs_f64() * 1000.0,
        sampling.arrows.len(),
        sampling.omitted,
        sampling.omitted_large,
        stop_label(sampling.accuracy),
        sampling.convergence,
    );
    assert!(
        sample_elapsed.as_secs_f64() < 2.0,
        "BEM 256/128 200^2 sampling exceeded the 2 s cap: {sample_elapsed:?}"
    );
}
