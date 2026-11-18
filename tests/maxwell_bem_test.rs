//! Bounded 2D boundary-element conductor: solve, convergence and sampling.

use glam::{UVec3, Vec3};
use vector_field_explorer::domain::Domain;
use vector_field_explorer::maxwell::{
    ConductorOutline, MAXWELL_BEM_TARGET, MAXWELL_MAX_BEM_ELEMENTS, MAXWELL_SWITCH_2D_WARNING,
    MaxwellCase, MaxwellError, MaxwellScene, MaxwellStop, sample_maxwell_with_bem, solve_bem,
};
use vector_field_explorer::plane::{PlaneAxis, PlaneFrame, PlaneView};

fn bem(case: MaxwellCase) -> MaxwellScene {
    MaxwellScene {
        case,
        ..MaxwellScene::default()
    }
}

fn z_slice(domain: &Domain, index: u32) -> PlaneFrame {
    PlaneFrame::new(
        domain,
        PlaneView {
            enabled: true,
            axis: PlaneAxis::Z,
            index,
        },
        UVec3::new(21, 21, 21),
    )
}

#[test]
fn grounded_circle_solve_matches_the_dipole_far_field() {
    let scene = bem(MaxwellCase::Conductor2D);
    let solution = solve_bem(&scene).expect("circle BEM");
    let field = solution
        .field_at(Vec3::new(4.0, 0.0, 0.0))
        .expect("finite field");
    // Analytic conducting cylinder at r=4, theta=0: E_x = E0 (1 + a^2/r^2).
    assert!((field.x - 1.0625).abs() < 0.05);
    assert!(field.x > 1.0);
    assert!(field.y.abs() < 1e-3);
    assert!(field.z.abs() < 1e-6);
    assert!(solution.net_charge.abs() < 1e-6);
    assert!(solution.convergence().relative_difference.is_finite());
}

#[test]
fn square_refines_globally_and_never_reports_identical_coarse_fine() {
    let mut scene = bem(MaxwellCase::Conductor2D);
    scene.outline = ConductorOutline::Square { half_extent: 1.0 };
    scene.elements = 64;
    let solution = solve_bem(&scene).expect("square BEM");
    let convergence = solution.convergence();
    assert_eq!(convergence.coarse_elements, 32);
    assert_eq!(convergence.fine_elements, 64);
    assert!(convergence.relative_difference >= 0.0);
    assert!(convergence.target == MAXWELL_BEM_TARGET);
    let mut tighter = scene.clone();
    tighter.elements = 128;
    let tighter = solve_bem(&tighter).expect("tighter square");
    assert!(tighter.convergence().relative_difference <= convergence.relative_difference + 1e-12);
}

#[test]
fn max_elements_still_uses_a_distinct_coarse_run() {
    let mut scene = bem(MaxwellCase::Conductor2D);
    scene.elements = MAXWELL_MAX_BEM_ELEMENTS;
    let solution = solve_bem(&scene).expect("max BEM");
    assert_eq!(
        solution.convergence().coarse_elements,
        MAXWELL_MAX_BEM_ELEMENTS / 2
    );
    assert_eq!(
        solution.convergence().fine_elements,
        MAXWELL_MAX_BEM_ELEMENTS
    );
    assert!(solution.work_units <= u64::from(MAXWELL_MAX_BEM_ELEMENTS).pow(3));
}

#[test]
fn net_charge_residual_is_explicit() {
    let mut scene = bem(MaxwellCase::Conductor2D);
    scene.outline = ConductorOutline::Circle { radius: 1.0 };
    let mut solution = solve_bem(&scene).expect("circle");
    solution.force_net_charge_for_test(1.0);
    assert!(matches!(
        solution.check_net_charge(),
        Err(MaxwellError::Unavailable(_))
    ));
}

#[test]
fn slice_2d_sampling_uses_the_solved_field_and_projection_warning() {
    let scene = bem(MaxwellCase::Conductor2D);
    let solution = solve_bem(&scene).expect("circle BEM");
    let domain = Domain::new(Vec3::splat(-2.0), Vec3::splat(2.0));
    let sample = sample_maxwell_with_bem(
        &scene,
        &domain,
        UVec3::new(21, 21, 21),
        Some(&z_slice(&domain, 10)),
        Some(&solution),
    );
    assert!(!sample.arrows.is_empty());
    assert!(sample.arrows.iter().all(|a| a.position.z.abs() < 1e-6));
    assert!(sample.accuracy != MaxwellStop::Unavailable);
    assert!(
        sample.warning.is_none() || sample.warning.as_deref().is_some_and(|w| w.contains("2D"))
    );
}

#[test]
fn slice_2d_sampling_without_a_solution_is_explicitly_unavailable() {
    let scene = bem(MaxwellCase::Conductor2D);
    let domain = Domain::new(Vec3::splat(-2.0), Vec3::splat(2.0));
    let sample = sample_maxwell_with_bem(
        &scene,
        &domain,
        UVec3::new(21, 21, 21),
        Some(&z_slice(&domain, 10)),
        None,
    );
    assert_eq!(sample.accuracy, MaxwellStop::Unavailable);
    assert!(sample.arrows.is_empty());
    assert!(sample.warning.is_some());
}

#[test]
fn conductor_3d_sampling_is_unavailable_with_the_design_message() {
    let scene = bem(MaxwellCase::Conductor2D);
    let solution = solve_bem(&scene).expect("circle BEM");
    let domain = Domain::new(Vec3::splat(-2.0), Vec3::splat(2.0));
    let sample =
        sample_maxwell_with_bem(&scene, &domain, UVec3::new(9, 9, 9), None, Some(&solution));
    assert_eq!(sample.accuracy, MaxwellStop::Unavailable);
    assert!(sample.arrows.is_empty());
    assert_eq!(sample.warning.as_deref(), Some(MAXWELL_SWITCH_2D_WARNING));
}

#[test]
fn stale_solution_and_non_conductor_scenes_are_explicit() {
    let scene = bem(MaxwellCase::Conductor2D);
    let solution = solve_bem(&scene).expect("circle BEM");

    let mut square = scene.clone();
    square.outline = ConductorOutline::Square { half_extent: 1.0 };
    let domain = Domain::new(Vec3::splat(-2.0), Vec3::splat(2.0));
    let sample = sample_maxwell_with_bem(
        &square,
        &domain,
        UVec3::new(21, 21, 21),
        Some(&z_slice(&domain, 10)),
        Some(&solution),
    );
    assert_eq!(sample.accuracy, MaxwellStop::Unavailable);
    assert!(sample.arrows.is_empty());

    assert!(matches!(
        solve_bem(&bem(MaxwellCase::ParallelPlates)),
        Err(MaxwellError::Unsupported(_))
    ));
    assert!(solution.field_at(Vec3::new(f32::NAN, 0.0, 0.0)).is_err());
}
