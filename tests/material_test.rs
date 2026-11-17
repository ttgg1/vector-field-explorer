//! Task 1 pure analytic material model tests.
//!
//! These pin the four bounded cases, total/applied/response decomposition,
//! interface handling, validation, sampling and slice-only outlines.

use glam::{UVec3, Vec3};
use vector_field_explorer::domain::Domain;
use vector_field_explorer::material::{
    MaterialCase, MaterialError, MaterialFamily, MaterialScene, MaterialView, material_applied,
    material_assumptions, material_family, material_field, material_outline, material_response,
    material_warning, sample_materials, validate_material,
};
use vector_field_explorer::plane::{PlaneAxis, PlaneFrame, PlaneView};

fn sphere(case: MaterialCase, eps: f64, mu: f64, applied: [f64; 3]) -> MaterialScene {
    MaterialScene {
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
    }
}

fn boundary(case: MaterialCase, eps: f64, mu: f64, applied: [f64; 3]) -> MaterialScene {
    MaterialScene {
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
    }
}

fn domain() -> Domain {
    Domain::new(Vec3::splat(-2.0), Vec3::splat(2.0))
}

fn z_frame(domain: &Domain, grid: UVec3, index: u32) -> PlaneFrame {
    PlaneFrame::new(
        domain,
        PlaneView {
            enabled: true,
            axis: PlaneAxis::Z,
            index,
        },
        grid,
    )
}

#[test]
fn dielectric_sphere_matches_inside_far_field_and_no_contrast() {
    let scene = sphere(MaterialCase::DielectricSphere, 4.0, 1.0, [1.0, 0.0, 0.0]);
    let inside = material_field(&scene, Vec3::new(0.5, 0.0, 0.0)).unwrap();
    assert!((inside.x - 0.5).abs() <= 1e-12);
    assert!(inside.y.abs() <= 1e-12 && inside.z.abs() <= 1e-12);
    let outside = material_field(&scene, Vec3::new(2.0, 0.0, 0.0)).unwrap();
    assert!((outside.x - 1.125).abs() <= 1e-12);
    let mut no = sphere(MaterialCase::DielectricSphere, 4.0, 1.0, [1.0, 0.0, 0.0]);
    no.material_epsilon_r = 1.0;
    for point in [Vec3::new(0.5, 0.0, 0.0), Vec3::new(2.0, 0.0, 0.0)] {
        assert!((material_field(&no, point).unwrap() - Vec3::X).length() <= 1e-12);
    }
}

#[test]
fn dielectric_boundary_conditions_and_response_are_piecewise() {
    let scene = boundary(MaterialCase::DielectricBoundary, 4.0, 1.0, [1.0, 2.0, 3.0]);
    let background = material_field(&scene, Vec3::new(-0.5, 0.0, 0.0)).unwrap();
    let material = material_field(&scene, Vec3::new(0.5, 0.0, 0.0)).unwrap();
    assert!((background.x - 1.0).abs() <= 1e-12);
    assert!((material.x - 0.25).abs() <= 1e-12);
    assert!((material.y - 2.0).abs() <= 1e-12 && (material.z - 3.0).abs() <= 1e-12);
    assert!(
        (material_response(&scene, Vec3::new(0.5, 0.0, 0.0)).unwrap() - Vec3::new(-0.75, 0.0, 0.0))
            .length()
            <= 1e-12
    );
    assert!(
        material_response(&scene, Vec3::new(-0.5, 0.0, 0.0))
            .unwrap()
            .length()
            <= 1e-12
    );
}

#[test]
fn permeable_sphere_and_boundary_match_their_interface_conditions() {
    let sphere_scene = sphere(MaterialCase::PermeableSphere, 1.0, 4.0, [0.0, 0.0, 1.0]);
    let inside = material_field(&sphere_scene, Vec3::new(0.5, 0.0, 0.0)).unwrap();
    assert!((inside.z - 2.0).abs() <= 1e-12);
    let outside = material_field(&sphere_scene, Vec3::new(0.0, 0.0, 2.0)).unwrap();
    assert!((outside.z - 1.125).abs() <= 1e-12);

    let plane = boundary(MaterialCase::PermeableBoundary, 1.0, 4.0, [1.0, 2.0, 3.0]);
    let material = material_field(&plane, Vec3::new(0.5, 0.0, 0.0)).unwrap();
    assert!((material.x - 1.0).abs() <= 1e-12, "normal B is continuous");
    assert!(
        (material.y - 8.0).abs() <= 1e-12,
        "in-plane tangential B scales by mu_in/mu_bg"
    );
    assert!((material.z - 12.0).abs() <= 1e-12);
}

#[test]
fn applied_plus_response_reconstructs_total_for_all_cases() {
    let cases = [
        (MaterialCase::DielectricSphere, 4.0, 1.0),
        (MaterialCase::DielectricBoundary, 4.0, 1.0),
        (MaterialCase::PermeableSphere, 1.0, 4.0),
        (MaterialCase::PermeableBoundary, 1.0, 4.0),
    ];
    for (case, eps, mu) in cases {
        let scene = sphere(case, eps, mu, [0.3, -0.5, 0.8]);
        for point in [Vec3::new(0.5, 0.2, 0.1), Vec3::new(1.5, -0.4, 0.2)] {
            let total = material_field(&scene, point).unwrap();
            let response = material_response(&scene, point).unwrap();
            let applied = material_applied(&scene);
            assert!((total - (applied + response)).length() <= 1e-12);
        }
    }
}

#[test]
fn exact_interface_hits_are_unsupported() {
    let sphere_scene = sphere(MaterialCase::DielectricSphere, 4.0, 1.0, [1.0, 0.0, 0.0]);
    assert!(matches!(
        material_field(&sphere_scene, Vec3::new(1.0, 0.0, 0.0)),
        Err(MaterialError::Unsupported(_))
    ));
    assert!(material_field(&sphere_scene, Vec3::new(1.0 + 1e-5, 0.0, 0.0)).is_ok());
    assert!(material_field(&sphere_scene, Vec3::new(1.0 - 1e-5, 0.0, 0.0)).is_ok());

    let plane = boundary(MaterialCase::DielectricBoundary, 4.0, 1.0, [1.0, 0.0, 0.0]);
    assert!(matches!(
        material_field(&plane, Vec3::new(0.0, 0.0, 0.0)),
        Err(MaterialError::Unsupported(_))
    ));
    assert!(material_field(&plane, Vec3::new(1e-5, 0.0, 0.0)).is_ok());
    assert!(material_field(&plane, Vec3::new(-1e-5, 0.0, 0.0)).is_ok());
}

#[test]
fn validation_rejects_bad_constants_geometry_and_applied() {
    let valid = sphere(MaterialCase::DielectricSphere, 4.0, 1.0, [1.0, 0.0, 0.0]);
    validate_material(&valid).expect("the baseline scene is valid");

    let cases = [
        MaterialScene {
            background_epsilon_r: 0.5,
            ..valid.clone()
        },
        MaterialScene {
            material_epsilon_r: 0.5,
            ..valid.clone()
        },
        MaterialScene {
            background_mu_r: 101.0,
            ..valid.clone()
        },
        MaterialScene {
            material_mu_r: f64::NAN,
            ..valid.clone()
        },
        MaterialScene {
            radius: 0.0,
            ..valid.clone()
        },
        MaterialScene {
            radius: f64::NAN,
            ..valid.clone()
        },
        MaterialScene {
            applied: [0.0, 0.0, 0.0],
            ..valid.clone()
        },
        MaterialScene {
            applied: [f64::NAN, 0.0, 0.0],
            ..valid.clone()
        },
    ];
    for scene in cases {
        assert!(matches!(
            validate_material(&scene),
            Err(MaterialError::InvalidScene(_))
        ));
    }

    let plane = boundary(MaterialCase::DielectricBoundary, 4.0, 1.0, [1.0, 0.0, 0.0]);
    let bad_offset = MaterialScene {
        interface_offset: f64::NAN,
        ..plane.clone()
    };
    assert!(matches!(
        validate_material(&bad_offset),
        Err(MaterialError::InvalidScene(_))
    ));
    assert!(validate_material(&plane).is_ok());
}

#[test]
fn sampling_omits_exact_interface_but_keeps_near_interface() {
    let scene = sphere(MaterialCase::DielectricSphere, 4.0, 1.0, [1.0, 0.0, 0.0]);
    let sampling = sample_materials(&scene, &domain(), UVec3::splat(41), None);
    assert!(sampling.omitted_interface > 0);
    assert!(
        sampling
            .warning
            .as_deref()
            .unwrap_or_default()
            .contains("Samples exactly on the interface are omitted"),
        "warning was {:?}",
        sampling.warning
    );
    assert!(sampling.arrows.iter().all(|a| {
        a.position.is_finite() && a.dir.is_finite() && a.mag.is_finite() && a.mag > 1e-6
    }));
    assert!(
        sampling.arrows.iter().any(|a| a.position.x < 0.9),
        "samples on the background side survive"
    );
    assert!(
        sampling.arrows.iter().any(|a| a.position.x > 1.1),
        "samples on the material side survive"
    );
    assert!(
        !sampling
            .arrows
            .iter()
            .any(|a| (a.position.length() - 1.0).abs() < 1e-12),
        "no arrow is drawn exactly on the interface"
    );
}

#[test]
fn no_contrast_response_is_empty_with_exact_note() {
    let mut scene = sphere(MaterialCase::DielectricSphere, 4.0, 1.0, [1.0, 0.0, 0.0]);
    scene.material_epsilon_r = 1.0;
    scene.view = MaterialView::Response;
    let sampling = sample_materials(&scene, &domain(), UVec3::splat(21), None);
    assert!(sampling.arrows.is_empty());
    assert!(sampling.response_empty);
    assert!(
        sampling
            .warning
            .as_deref()
            .unwrap_or_default()
            .contains("Response is identically zero for this view."),
        "warning was {:?}",
        sampling.warning
    );
}

#[test]
fn empty_domain_returns_warning_not_arrows() {
    let scene = sphere(MaterialCase::DielectricSphere, 4.0, 1.0, [1.0, 0.0, 0.0]);
    for (domain, grid) in [
        (Domain::new(Vec3::ZERO, Vec3::ZERO), UVec3::new(9, 9, 9)),
        (domain(), UVec3::new(0, 9, 9)),
    ] {
        let sampling = sample_materials(&scene, &domain, grid, None);
        assert!(sampling.arrows.is_empty());
        assert!(
            sampling.warning.as_deref().is_some_and(|w| !w.is_empty()),
            "an empty/impossible domain is a visible warning"
        );
        assert!(!sampling.response_empty);
    }
}

#[test]
fn warnings_join_in_the_deterministic_documented_order() {
    let mut scene = sphere(MaterialCase::DielectricSphere, 4.0, 1.0, [1.0, 0.0, 0.0]);
    scene.material_epsilon_r = 1.0;
    scene.view = MaterialView::Response;
    let sampling = sample_materials(&scene, &domain(), UVec3::splat(5), None);
    let warning = sampling.warning.as_deref().expect("two warnings");
    let interface_at = warning
        .find("Samples exactly on the interface")
        .expect("interface warning");
    let response_at = warning
        .find("Response is identically zero")
        .expect("response warning");
    assert!(interface_at < response_at, "warning was {warning:?}");

    let mut plane_scene = boundary(MaterialCase::DielectricBoundary, 4.0, 1.0, [1.0, 0.0, 0.0]);
    plane_scene.interface_axis = PlaneAxis::Z;
    let empty = Domain::new(Vec3::ZERO, Vec3::ZERO);
    let coincident = z_frame(&empty, UVec3::new(9, 9, 9), 4);
    let sampling = sample_materials(&plane_scene, &empty, UVec3::new(9, 9, 1), Some(&coincident));
    let warning = sampling.warning.as_deref().expect("two warnings");
    let empty_at = warning
        .find("produces no samples")
        .expect("empty-domain warning");
    let slice_at = warning
        .find("Slice lies on the interface")
        .expect("coincident-slice warning");
    assert!(empty_at < slice_at, "warning was {warning:?}");
}

#[test]
fn outline_uses_slice_intersection_only() {
    let sphere_scene = sphere(MaterialCase::DielectricSphere, 4.0, 1.0, [1.0, 0.0, 0.0]);
    let boxed = domain();
    let world = material_outline(&sphere_scene, &boxed, None);
    assert!(!world.is_empty());
    assert!(world.iter().all(|line| {
        line.a.iter().all(|v| v.is_finite()) && line.b.iter().all(|v| v.is_finite())
    }));

    let grid = UVec3::new(41, 41, 41);
    let miss = z_frame(&boxed, grid, 40);
    assert!(material_outline(&sphere_scene, &boxed, Some(&miss)).is_empty());

    let cut = z_frame(&boxed, grid, 20);
    let cut_lines = material_outline(&sphere_scene, &boxed, Some(&cut));
    assert!(!cut_lines.is_empty());

    let mut plane_scene = boundary(MaterialCase::DielectricBoundary, 4.0, 1.0, [1.0, 0.0, 0.0]);
    plane_scene.interface_axis = PlaneAxis::Z;
    plane_scene.interface_offset = 0.0;
    let noncoincident = z_frame(&boxed, grid, 40);
    assert!(material_outline(&plane_scene, &boxed, Some(&noncoincident)).is_empty());

    let coincident = z_frame(&boxed, grid, 20);
    let square = material_outline(&plane_scene, &boxed, Some(&coincident));
    assert!(!square.is_empty());

    let sampling = sample_materials(
        &plane_scene,
        &boxed,
        UVec3::new(41, 41, 1),
        Some(&coincident),
    );
    assert!(
        sampling
            .warning
            .as_deref()
            .unwrap_or_default()
            .contains("Slice lies on the interface; the field is discontinuous here."),
        "warning was {:?}",
        sampling.warning
    );
}

#[test]
fn every_design_case_has_a_family_assumptions_and_fixed_colour() {
    let cases = [
        (
            MaterialCase::DielectricSphere,
            4.0,
            1.0,
            MaterialFamily::Electric,
        ),
        (
            MaterialCase::DielectricBoundary,
            4.0,
            1.0,
            MaterialFamily::Electric,
        ),
        (
            MaterialCase::PermeableSphere,
            1.0,
            4.0,
            MaterialFamily::Magnetic,
        ),
        (
            MaterialCase::PermeableBoundary,
            1.0,
            4.0,
            MaterialFamily::Magnetic,
        ),
    ];
    for (case, eps, mu, family) in cases {
        assert_eq!(material_family(case), family);
        let assumptions = material_assumptions(case);
        for word in ["homogeneous", "linear", "isotropic", "quasistatic"] {
            assert!(
                assumptions.contains(word),
                "{case:?} assumptions missing {word}: {assumptions}"
            );
        }
        assert!(assumptions.contains("visualization window"));
        assert_eq!(
            material_warning(case),
            "Bounded analytic case; no general material solver."
        );
        let scene = sphere(case, eps, mu, [1.0, 0.0, 0.0]);
        let sampling = sample_materials(&scene, &domain(), UVec3::splat(9), None);
        assert!(!sampling.arrows.is_empty());
        let expected = match family {
            MaterialFamily::Electric => [255, 176, 64],
            MaterialFamily::Magnetic => [64, 196, 255],
        };
        assert!(sampling.arrows.iter().all(|arrow| arrow.color == expected));
    }
}

#[test]
fn two_dimensional_sampling_is_a_projected_three_dimensional_slice() {
    let mut scene = sphere(MaterialCase::DielectricSphere, 4.0, 1.0, [1.0, 0.0, 0.0]);
    scene.view = MaterialView::Total;
    let boxed = domain();
    let frame = z_frame(&boxed, UVec3::new(41, 41, 41), 20);
    let sampling = sample_materials(&scene, &boxed, UVec3::new(21, 21, 1), Some(&frame));
    assert!(!sampling.arrows.is_empty());
    assert!(
        sampling
            .arrows
            .iter()
            .all(|arrow| arrow.position.z.abs() <= 1e-6)
    );
    for arrow in &sampling.arrows {
        let world = frame.to_world(arrow.position);
        let projected = frame.project(material_field(&scene, world).unwrap());
        assert!((arrow.dir - projected.normalize()).length() <= 1e-4);
        assert!((arrow.mag - projected.length()).abs() <= 1e-4);
    }
}

#[test]
fn sphere_and_boundary_fields_are_finite_off_the_interface() {
    let scenes = [
        sphere(MaterialCase::DielectricSphere, 4.0, 1.0, [0.3, -0.5, 0.8]),
        boundary(MaterialCase::DielectricBoundary, 4.0, 1.0, [0.3, -0.5, 0.8]),
        sphere(MaterialCase::PermeableSphere, 1.0, 4.0, [0.3, -0.5, 0.8]),
        boundary(MaterialCase::PermeableBoundary, 1.0, 4.0, [0.3, -0.5, 0.8]),
    ];
    for scene in scenes {
        for point in [
            Vec3::new(0.25, -0.5, 0.75),
            Vec3::new(1.5, -0.5, 0.75),
            Vec3::new(-0.5, 0.5, -0.75),
        ] {
            let field = material_field(&scene, point).unwrap();
            assert!(field.is_finite(), "{:?} at {point:?}", scene.case);
        }
    }
}
