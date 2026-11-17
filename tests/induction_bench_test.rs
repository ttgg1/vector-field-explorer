//! Task 1 cap benchmark. Run through `scripts/bench-induction.sh`; a normal
//! `cargo test` returns immediately.

use std::time::Instant;

use glam::{DVec3, UVec3};
use vector_field_explorer::domain::Domain;
use vector_field_explorer::induction::{InductionCase, InductionScene, prepare_induction};
use vector_field_explorer::plane::{PlaneFrame, PlaneView};
use vector_field_explorer::source_scene::SourceDisplay;

#[test]
fn induction_cap_benchmark() {
    if std::env::var_os("VFE_INDUCTION_BENCH").is_none() {
        return;
    }

    let domain = Domain::default();
    for case in [InductionCase::CurrentLoop, InductionCase::Solenoid] {
        let scene = InductionScene {
            case,
            center: DVec3::ZERO,
            axis: DVec3::Z,
            radius: if case == InductionCase::Solenoid {
                0.5
            } else {
                1.0
            },
            length: if case == InductionCase::Solenoid {
                2.0
            } else {
                0.0
            },
            turns: if case == InductionCase::Solenoid {
                8
            } else {
                1
            },
            amplitude: 1.0,
            frequency: 0.25,
            display: SourceDisplay::Both,
            allow_inaccurate: false,
        };

        for (label, grid, plane) in [
            ("3d", UVec3::splat(20), None),
            (
                "2d-provisional",
                UVec3::new(120, 120, 1),
                Some(UVec3::new(120, 120, 1)),
            ),
            ("2d-cap", UVec3::new(80, 80, 1), Some(UVec3::new(80, 80, 1))),
        ] {
            let plane_view = plane.map(|_| PlaneFrame::new(&domain, PlaneView::middle(grid), grid));
            let started = Instant::now();
            let prepared = prepare_induction(
                &scene,
                &domain,
                grid,
                plane_view.as_ref(),
                0.1,
                Default::default(),
            )
            .expect("benchmark scene prepares");
            let elapsed = started.elapsed();
            std::hint::black_box(&prepared);
            let points = if plane.is_some() {
                grid.x as u64 * grid.y as u64
            } else {
                grid.x as u64 * grid.y as u64 * grid.z as u64
            };
            println!(
                "INDUCTION BENCH case={case:?} view={label} points={points} ms={:.3}",
                elapsed.as_secs_f64() * 1000.0
            );
        }
    }
}
