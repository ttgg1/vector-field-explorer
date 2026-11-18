//! Task 1 pure analytic Maxwell teaching-case tests.
//!
//! These pin the bounded analytic catalog, validation, interface guards,
//! deterministic sampling diagnostics and slice-only outlines. The 2D BEM
//! conductor solve lands in Task 2; here Conductor2D is explicitly unavailable.

use glam::{UVec3, Vec3};
use vector_field_explorer::domain::Domain;
use vector_field_explorer::maxwell::{
    ConductorOutline, MAXWELL_GROUND_SLICE_WARNING, MAXWELL_MAX_RELATIVE, MaxwellCase,
    MaxwellError, MaxwellScene, MaxwellStop, maxwell_field, maxwell_outline, sample_maxwell,
    validate_maxwell,
};
use vector_field_explorer::plane::{PlaneAxis, PlaneFrame, PlaneView};

fn scene(case: MaxwellCase) -> MaxwellScene {
    MaxwellScene {
        case,
        ..MaxwellScene::default()
    }
}

#[test]
fn plates_match_the_ideal_uniform_field_and_interface_guard() {
    let scene = scene(MaxwellCase::ParallelPlates);
    assert!((maxwell_field(&scene, Vec3::new(0.0, 0.0, 0.25)).unwrap() - Vec3::Z).length() < 1e-6);
    assert_eq!(
        maxwell_field(&scene, Vec3::new(0.0, 0.0, 0.75)).unwrap(),
        Vec3::ZERO
    );
    assert!(maxwell_field(&scene, Vec3::new(0.0, 0.0, 0.5)).is_err());
}

#[test]
fn coaxial_electric_and_magnetic_are_azimuthal_and_zero_outside() {
    let e = scene(MaxwellCase::CoaxialElectric);
    let p = Vec3::new(1.5, 0.0, 0.0);
    assert!((maxwell_field(&e, p).unwrap() - Vec3::new(1.0 / 1.5, 0.0, 0.0)).length() < 1e-6);
    assert_eq!(
        maxwell_field(&e, Vec3::new(3.0, 0.0, 0.0)).unwrap(),
        Vec3::ZERO
    );
    let b = scene(MaxwellCase::CoaxialMagnetic);
    assert!((maxwell_field(&b, p).unwrap() - Vec3::new(0.0, 1.0 / 1.5, 0.0)).length() < 1e-6);
}

#[test]
fn hollow_sphere_and_solenoid_have_expected_limits() {
    let sphere = scene(MaxwellCase::HollowSphere);
    let outside = maxwell_field(&sphere, Vec3::new(2.0, 0.0, 0.0)).unwrap();
    assert!((outside - Vec3::new(0.25, 0.0, 0.0)).length() < 1e-6);
    assert_eq!(
        maxwell_field(&sphere, Vec3::new(0.5, 0.0, 0.0)).unwrap(),
        Vec3::ZERO
    );
    let solenoid = scene(MaxwellCase::Solenoid);
    assert!(
        (maxwell_field(&solenoid, Vec3::new(0.0, 0.0, 0.5)).unwrap() - Vec3::Z).length() < 1e-6
    );
    assert_eq!(
        maxwell_field(&solenoid, Vec3::new(2.0, 0.0, 0.0)).unwrap(),
        Vec3::ZERO
    );
}

#[test]
fn infinite_wire_is_a_surface_shell_with_zero_interior() {
    let scene = scene(MaxwellCase::InfiniteWire);
    let b = maxwell_field(&scene, Vec3::new(2.0, 0.0, 0.0)).unwrap();
    assert!((b - Vec3::new(0.0, 0.5, 0.0)).length() < 1e-6);
    assert_eq!(
        maxwell_field(&scene, Vec3::new(0.5, 0.0, 0.0)).unwrap(),
        Vec3::ZERO
    );
    assert_eq!(maxwell_field(&scene, Vec3::ZERO).unwrap(), Vec3::ZERO);
}

#[test]
fn image_sphere_satisfies_the_conducting_boundary_and_far_field() {
    let scene = scene(MaxwellCase::ImageSphere);
    // On the pole just outside the surface the normal field approaches 3 E0;
    // the tangential component vanishes. The exact surface itself is omitted.
    let boundary = maxwell_field(&scene, Vec3::new(0.0, 0.0, 1.0001)).unwrap();
    assert!((boundary - Vec3::new(0.0, 0.0, 3.0)).length() < 1e-2);
    assert_eq!(
        maxwell_field(&scene, Vec3::new(0.5, 0.0, 0.0)).unwrap(),
        Vec3::ZERO
    );
    // Far away the field approaches the applied uniform E0 z-hat.
    let far = maxwell_field(&scene, Vec3::new(0.0, 0.0, 1000.0)).unwrap();
    assert!((far - Vec3::Z).length() < 1e-5);
}

#[test]
fn image_plane_is_zero_on_the_ground_and_has_no_tangential_field_above_it() {
    let mut scene = scene(MaxwellCase::ImagePlane);
    scene.charge_height = 1.0;
    assert_eq!(
        maxwell_field(&scene, Vec3::new(0.0, 0.0, -0.1)).unwrap(),
        Vec3::ZERO
    );
    assert_eq!(
        maxwell_field(&scene, Vec3::new(0.0, 0.0, 0.0)).unwrap(),
        Vec3::ZERO
    );
    let p = Vec3::new(0.4, 0.0, 0.001);
    let field = maxwell_field(&scene, p).unwrap();
    // At z = 1e-3 h the tangential component is O(z/h) of the normal field,
    // not exactly zero; the analytic image pair still cancels it at z = 0.
    assert!(field.x.abs() < 2e-3 && field.y.abs() < 1e-12);
    assert!(field.z < -1.0);
    // The real charge point is a singularity, above the supported plane.
    assert!(maxwell_field(&scene, Vec3::new(0.0, 0.0, 1.0)).is_err());
}

#[test]
fn conductor_case_is_2d_only_and_unavailable_in_3d_sampling() {
    let scene = scene(MaxwellCase::Conductor2D);
    let domain = Domain::new(Vec3::splat(-2.0), Vec3::splat(2.0));
    let sample = sample_maxwell(&scene, &domain, UVec3::new(9, 9, 9), None);
    assert_eq!(sample.accuracy, MaxwellStop::Unavailable);
    assert!(sample.arrows.is_empty());
    assert!(sample.warning.unwrap().contains("switch to 2D"));
}

#[test]
fn extreme_arrows_are_omitted_and_counted() {
    let mut scene = scene(MaxwellCase::ImagePlane);
    scene.charge_height = 0.01;
    let domain = Domain::new(Vec3::splat(-0.2), Vec3::splat(0.2));
    let sample = sample_maxwell(&scene, &domain, UVec3::new(21, 21, 21), None);
    assert!(sample.omitted_large > 0);
    assert!(
        sample
            .warning
            .unwrap()
            .contains("Extreme near-interface magnitudes")
    );
    assert!(sample.arrows.iter().all(|a| a.mag.is_finite()));
    assert!(
        sample
            .arrows
            .iter()
            .all(|a| f64::from(a.mag) <= MAXWELL_MAX_RELATIVE)
    );
}

#[test]
fn azimuthal_2d_slice_warns_about_the_suppressed_component() {
    let scene = scene(MaxwellCase::CoaxialMagnetic);
    let domain = Domain::new(Vec3::splat(-2.0), Vec3::splat(2.0));
    let grid = UVec3::new(21, 21, 21);
    let frame = PlaneFrame::new(
        &domain,
        PlaneView {
            enabled: true,
            axis: PlaneAxis::X,
            index: 10,
        },
        grid,
    );
    let sample = sample_maxwell(&scene, &domain, grid, Some(&frame));
    assert!(sample.out_of_plane_suppressed);
    assert!(sample.arrows.is_empty());
    assert!(sample.warning.unwrap().contains("out-of-plane component"));
}

#[test]
fn coincident_ground_slice_warns_and_shows_no_field() {
    let scene = scene(MaxwellCase::ImagePlane);
    let domain = Domain::new(Vec3::splat(-2.0), Vec3::splat(2.0));
    let grid = UVec3::new(21, 21, 21);
    let frame = PlaneFrame::new(
        &domain,
        PlaneView {
            enabled: true,
            axis: PlaneAxis::Z,
            index: 10,
        },
        grid,
    );
    let sample = sample_maxwell(&scene, &domain, grid, Some(&frame));
    assert!(sample.arrows.is_empty());
    assert!(
        sample
            .warning
            .unwrap()
            .contains("Ground-plane slice is empty")
    );
}

#[test]
fn coincident_slice_tolerance_includes_the_case_extent() {
    // The in-plane domain extent is only 2, but the case extent (charge
    // height) is 1e6. The design tolerance is
    // 1e-6 * max(domain extent, case extent, 1) = 1, so a 0.5 gap is a
    // coincident ground slice; the old domain-only form (2e-6) misses it.
    let mut scene = scene(MaxwellCase::ImagePlane);
    scene.charge_height = 1.0e6;
    let domain = Domain::new(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 2.0));
    let grid = UVec3::new(9, 9, 21);
    let frame = PlaneFrame::new(
        &domain,
        PlaneView {
            enabled: true,
            axis: PlaneAxis::Z,
            index: 10,
        },
        grid,
    );
    assert_eq!(frame.origin.z, 0.5);
    let sample = sample_maxwell(&scene, &domain, grid, Some(&frame));
    assert!(sample.arrows.is_empty());
    assert_eq!(
        sample.warning.as_deref(),
        Some(MAXWELL_GROUND_SLICE_WARNING)
    );
}

#[test]
fn zero_amplitude_and_no_contrast_cases_stay_finite_and_draw_nothing() {
    let domain = Domain::new(Vec3::splat(-2.0), Vec3::splat(2.0));
    let grid = UVec3::new(9, 9, 9);

    // Zero electric amplitude: every analytic electric case returns exactly
    // zero at a representative interior point and draws no arrows.
    for (case, point) in [
        (MaxwellCase::ParallelPlates, Vec3::new(0.0, 0.0, 0.25)),
        (MaxwellCase::CoaxialElectric, Vec3::new(1.5, 0.0, 0.0)),
        (MaxwellCase::HollowSphere, Vec3::new(2.0, 0.0, 0.0)),
        (MaxwellCase::ImageSphere, Vec3::new(2.0, 0.0, 0.0)),
    ] {
        let mut scene = scene(case);
        scene.e0 = 0.0;
        assert_eq!(maxwell_field(&scene, point).unwrap(), Vec3::ZERO);
        let sample = sample_maxwell(&scene, &domain, grid, None);
        assert!(sample.arrows.is_empty(), "{case:?} drew a zero-field arrow");
        assert!(sample.omitted > 0, "{case:?} should omit zero vectors");
        // Zero is an exact analytic field, not a false convergence claim.
        assert_eq!(sample.accuracy, MaxwellStop::TargetMet);
    }

    // Zero magnetic amplitude.
    for (case, point) in [
        (MaxwellCase::CoaxialMagnetic, Vec3::new(1.5, 0.0, 0.0)),
        (MaxwellCase::InfiniteWire, Vec3::new(2.0, 0.0, 0.0)),
        (MaxwellCase::Solenoid, Vec3::new(0.0, 0.0, 0.5)),
    ] {
        let mut scene = scene(case);
        scene.b0 = 0.0;
        assert_eq!(maxwell_field(&scene, point).unwrap(), Vec3::ZERO);
        let sample = sample_maxwell(&scene, &domain, grid, None);
        assert!(sample.arrows.is_empty(), "{case:?} drew a zero-field arrow");
        assert!(sample.omitted > 0, "{case:?} should omit zero vectors");
        assert_eq!(sample.accuracy, MaxwellStop::TargetMet);
    }

    // Zero image-plane charge: finite zero above the ground, no arrows.
    let mut image = scene(MaxwellCase::ImagePlane);
    image.charge = 0.0;
    assert_eq!(
        maxwell_field(&image, Vec3::new(0.4, 0.0, 0.5)).unwrap(),
        Vec3::ZERO
    );
    let sample = sample_maxwell(&image, &domain, grid, None);
    assert!(sample.arrows.is_empty());
    assert!(sample.omitted > 0);
    assert_eq!(sample.accuracy, MaxwellStop::TargetMet);

    // Zero amplitude must not make the pending conductor solve look successful.
    let mut conductor = scene(MaxwellCase::Conductor2D);
    conductor.e0 = 0.0;
    let sample = sample_maxwell(&conductor, &domain, grid, None);
    assert_eq!(sample.accuracy, MaxwellStop::Unavailable);
    assert!(sample.arrows.is_empty());
}

#[test]
fn exact_interface_slice_warns_and_omits_every_sample() {
    let scene = scene(MaxwellCase::ParallelPlates);
    let domain = Domain::new(Vec3::splat(-2.0), Vec3::splat(2.0));
    let grid = UVec3::new(9, 9, 9);
    // z = 0.5 is the upper plate and a grid sample for this domain.
    let frame = PlaneFrame::new(
        &domain,
        PlaneView {
            enabled: true,
            axis: PlaneAxis::Z,
            index: 5,
        },
        grid,
    );
    let sample = sample_maxwell(&scene, &domain, grid, Some(&frame));
    assert!(sample.arrows.is_empty());
    assert!(sample.omitted > 0);
    assert!(sample.warning.unwrap().contains("exactly on the interface"));
}

#[test]
fn empty_domain_is_explicitly_invalid_sampling() {
    let scene = scene(MaxwellCase::ParallelPlates);
    let domain = Domain::new(Vec3::ZERO, Vec3::ZERO);
    let sample = sample_maxwell(&scene, &domain, UVec3::new(9, 9, 9), None);
    assert_eq!(sample.accuracy, MaxwellStop::Unavailable);
    assert!(sample.arrows.is_empty());
    assert!(sample.warning.unwrap().contains("no samples"));
}

#[test]
fn outline_and_validation_cover_supported_geometry() {
    let domain = Domain::new(Vec3::splat(-2.0), Vec3::splat(2.0));
    let sphere = scene(MaxwellCase::HollowSphere);
    assert!(!maxwell_outline(&sphere, &domain, None).is_empty());
    let mut bad = scene(MaxwellCase::CoaxialElectric);
    bad.outer_radius = bad.radius;
    assert!(matches!(
        validate_maxwell(&bad),
        Err(MaxwellError::InvalidScene(_))
    ));
    bad = scene(MaxwellCase::Conductor2D);
    bad.elements = 31;
    assert!(matches!(
        validate_maxwell(&bad),
        Err(MaxwellError::InvalidScene(_))
    ));
    assert_eq!(
        ConductorOutline::Circle { radius: 1.0 },
        ConductorOutline::Circle { radius: 1.0 }
    );
}
