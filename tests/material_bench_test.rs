//! Task 1 material sampling cap benchmark. Run through
//! `scripts/bench-materials.sh`; a normal `cargo test` returns immediately.

use std::time::Instant;

use glam::{UVec3, Vec3};
use vector_field_explorer::domain::Domain;
use vector_field_explorer::material::{
    MaterialCase, MaterialScene, MaterialView, sample_materials,
};
use vector_field_explorer::plane::{PlaneAxis, PlaneFrame, PlaneView};

#[test]
fn material_sampling_cap_benchmark() {
    if std::env::var_os("VFE_MATERIAL_BENCH").is_none() {
        return;
    }

    let host = std::env::var("HOSTNAME")
        .unwrap_or_else(|_| format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH));
    let domain = Domain::new(Vec3::splat(-2.0), Vec3::splat(2.0));
    for (case, eps, mu, applied) in [
        (MaterialCase::DielectricSphere, 4.0, 1.0, [1.0, 0.0, 0.0]),
        (MaterialCase::DielectricBoundary, 4.0, 1.0, [1.0, 0.0, 0.0]),
        (MaterialCase::PermeableSphere, 1.0, 4.0, [1.0, 0.0, 0.0]),
        (MaterialCase::PermeableBoundary, 1.0, 4.0, [1.0, 0.0, 0.0]),
    ] {
        let scene = MaterialScene {
            case,
            background_epsilon_r: 1.0,
            background_mu_r: 1.0,
            material_epsilon_r: eps,
            material_mu_r: mu,
            radius: 1.0,
            interface_axis: PlaneAxis::X,
            interface_offset: 0.0,
            applied,
            view: MaterialView::Total,
        };

        for (label, grid, plane) in [
            ("3d", UVec3::splat(40), None),
            ("2d", UVec3::new(200, 200, 1), Some(())),
        ] {
            let frame = plane.map(|()| {
                PlaneFrame::new(&domain, PlaneView::middle(UVec3::new(200, 200, 1)), grid)
            });
            let started = Instant::now();
            let sampling = sample_materials(&scene, &domain, grid, frame.as_ref());
            let elapsed = started.elapsed();
            let samples = sampling.arrows.len() + sampling.omitted_interface;
            println!(
                "MATERIAL BENCH host={host} case={case:?} dim={label} samples={} ms={:.3} omitted={}",
                samples,
                elapsed.as_secs_f64() * 1000.0,
                sampling.omitted_interface
            );
            assert!(
                elapsed.as_secs_f64() < 2.0,
                "material sampling exceeded the 2 s cap: {elapsed:?}"
            );
        }
    }
}
