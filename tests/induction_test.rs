//! Task 1 induction model: waveform, softened vector-potential fixtures,
//! Faraday contour, classification and native CAS parity.

use glam::{DVec3, UVec3, Vec3};
use vector_field_explorer::domain::Domain;
use vector_field_explorer::induction::{
    InductionCase, InductionScene, evaluate_induction_point, prepare_induction, sample_induction,
};
use vector_field_explorer::sampling::ViewOptions;
use vector_field_explorer::source_scene::SourceDisplay;
use vector_field_explorer::sources::AccuracyRequest;

use crate::support;

fn loop_scene() -> InductionScene {
    InductionScene {
        case: InductionCase::CurrentLoop,
        center: DVec3::ZERO,
        axis: DVec3::Z,
        radius: 1.0,
        length: 0.0,
        turns: 1,
        amplitude: 1.0,
        frequency: 0.25,
        display: SourceDisplay::Both,
        allow_inaccurate: false,
    }
}

#[test]
fn waveform_and_static_predicate_are_exact() {
    let mut scene = loop_scene();
    assert!((scene.current(0.0) - 1.0).abs() < 1e-15);
    assert!(scene.current_derivative(0.0).abs() < 1e-15);
    assert!((scene.current(0.5) - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-15);
    // d/dt[cos(2*pi*0.25*t)] at t = 0.5 is exactly
    // -2*pi*0.25*sin(pi/4) = -1.1107207345395915.
    assert!((scene.current_derivative(0.5) + 1.110_720_734_539_591_5).abs() < 1e-12);
    assert!(scene.is_animated());
    scene.amplitude = 0.0;
    assert!(!scene.is_animated());
    scene.amplitude = 1.0;
    scene.frequency = 0.0;
    assert!(!scene.is_animated());
}

#[test]
fn loop_vector_potential_matches_independent_fixtures() {
    let scene = loop_scene();
    for row in support::induction::rows("loop") {
        let got = scene
            .unit_vector_potential(DVec3::new(row.r, 0.0, row.z), row.softening)
            .unwrap();
        let tolerance = 1e-9 * row.reference.abs().max(1.0);
        assert!(
            (got - row.reference).abs() <= tolerance,
            "{row:?}: got {got:e}"
        );
    }
}

#[test]
fn solenoid_vector_potential_matches_independent_fixtures_and_curl_matches_b() {
    let mut scene = loop_scene();
    scene.case = InductionCase::Solenoid;
    scene.radius = 0.5;
    scene.length = 2.0;
    scene.turns = 8;
    for row in support::induction::rows("solenoid") {
        let got = scene
            .unit_vector_potential(DVec3::new(row.r, 0.0, row.z), row.softening)
            .unwrap();
        let tolerance = 1e-9 * row.reference.abs().max(1.0);
        assert!(
            (got - row.reference).abs() <= tolerance,
            "{row:?}: got {got:e}"
        );
    }

    let point = DVec3::new(0.7, 0.0, 0.3);
    let h = 1e-4;
    let a = |p| scene.unit_vector_potential(p, 0.1).unwrap();
    let b_r = -(a(DVec3::new(0.7, 0.0, 0.3 + h)) - a(DVec3::new(0.7, 0.0, 0.3 - h))) / (2.0 * h);
    let br = a(DVec3::new(0.7 + h, 0.0, 0.3)) * (0.7 + h);
    let bl = a(DVec3::new(0.7 - h, 0.0, 0.3)) * (0.7 - h);
    let b_z = (br - bl) / (2.0 * h * 0.7);
    let sample = evaluate_induction_point(&scene, point, 0.0, 0.1);
    let vector_field_explorer::sources::FamilyResult::TargetMet(b) = sample.magnetic else {
        panic!("unit-current B must be TargetMet")
    };
    assert!(
        (b.value.x - b_r).abs() < 2e-5,
        "B_r: got {:e}, curl {:e}",
        b.value.x,
        b_r
    );
    assert!(
        (b.value.z - b_z).abs() < 2e-5,
        "B_z: got {:e}, curl {:e}",
        b.value.z,
        b_z
    );
}

#[test]
fn faraday_contour_matches_flux_rate_for_both_cases() {
    for case in [InductionCase::CurrentLoop, InductionCase::Solenoid] {
        let mut scene = loop_scene();
        scene.case = case;
        if case == InductionCase::Solenoid {
            scene.radius = 0.5;
            scene.length = 2.0;
            scene.turns = 8;
        }
        let t = 0.5;
        let radius = 1.25;
        let n = 64;
        let mut contour = 0.0;
        for i in 0..n {
            let phi = std::f64::consts::TAU * (i as f64 + 0.5) / n as f64;
            let p = DVec3::new(radius * phi.cos(), radius * phi.sin(), 0.0);
            let sample = evaluate_induction_point(&scene, p, t as f32, 0.1);
            let vector_field_explorer::induction::InductionFamilyResult::TargetMet(e) =
                sample.electric
            else {
                panic!("contour E must be TargetMet")
            };
            contour += e.value.dot(DVec3::new(-phi.sin(), phi.cos(), 0.0))
                * radius
                * std::f64::consts::TAU
                / n as f64;
        }
        let (flux_rate, flux_uncertainty) = flux_rate(&scene, radius, t, 0.1);
        let tolerance = (1e-3 * flux_rate.abs()).max(20.0 * flux_uncertainty);
        assert!(
            (contour - flux_rate).abs() <= tolerance,
            "{case:?}: {contour:e} vs {flux_rate:e}"
        );
    }
}

fn flux_rate(scene: &InductionScene, radius: f64, t: f64, softening: f64) -> (f64, f64) {
    // Integrate the unit-current B_z over a polar disk; the field at `t` is
    // scaled by I(t), so divide it out before applying the analytic dI/dt.
    let n = 64;
    let current = scene.current(t as f32);
    assert!(current.abs() > 1e-6);
    let mut flux_unit = 0.0;
    let mut uncertainty = 0.0;
    for ir in 0..n {
        let r = radius * (ir as f64 + 0.5) / n as f64;
        for ip in 0..n {
            let phi = std::f64::consts::TAU * (ip as f64 + 0.5) / n as f64;
            let p = DVec3::new(r * phi.cos(), r * phi.sin(), 0.0);
            let sample = evaluate_induction_point(scene, p, t as f32, softening);
            let vector_field_explorer::sources::FamilyResult::TargetMet(b) = sample.magnetic else {
                panic!("unit-current B must be TargetMet")
            };
            let area = r * (radius / n as f64) * (std::f64::consts::TAU / n as f64);
            flux_unit += b.value.z / current * area;
            uncertainty += b.uncertainty / current.abs() * area;
        }
    }
    (
        -scene.current_derivative(t as f32) * flux_unit,
        scene.current_derivative(t as f32).abs() * uncertainty,
    )
}

#[test]
fn static_current_has_exactly_zero_induced_field() {
    let mut scene = loop_scene();
    scene.frequency = 0.0;
    // Isolate E: `Both` also contains the static, non-zero B family.
    scene.display = SourceDisplay::Electric;
    let prepared = prepare_induction(
        &scene,
        &Domain::new(Vec3::splat(-2.0), Vec3::splat(2.0)),
        UVec3::new(5, 5, 5),
        None,
        0.1,
        Default::default(),
    )
    .unwrap();
    let sampled = sample_induction(&prepared, &scene, 0.0, &ViewOptions::default());
    assert!(sampled.arrows.iter().all(|a| a.mag == 0.0));
    assert_eq!(sampled.electric.drawn, 0);
    assert_eq!(sampled.electric.accuracy_not_met_drawn, 0);
    assert_eq!(sampled.derivative, 0.0);
    // The magnetic family remains present and finite for the static current.
    let point = evaluate_induction_point(&scene, DVec3::new(0.3, 0.0, 0.2), 0.0, 0.1);
    assert!(matches!(
        point.magnetic,
        vector_field_explorer::sources::FamilyResult::TargetMet(_)
    ));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn cas_derivative_matches_the_analytic_waveform() {
    let scene = loop_scene();
    let source = format!(
        "{}*cos({}*t)",
        scene.amplitude,
        std::f64::consts::TAU * scene.frequency
    );
    let derivative =
        vector_field_explorer::cas::differentiate(&source, vector_field_explorer::cas::Variable::T)
            .unwrap();
    let expr: meval::Expr = derivative.parse().unwrap();
    let f = expr.bind("t").unwrap();
    for t in [0.0, 0.25, 0.5, 1.0] {
        assert!(
            (f(t) - scene.current_derivative(t as f32)).abs()
                <= 1e-12 * scene.current_derivative(t as f32).abs().max(1.0)
        );
    }
}

#[test]
fn accuracy_not_met_is_hidden_until_the_opt_in() {
    let mut scene = loop_scene();
    scene.display = SourceDisplay::Electric;
    let accuracy = AccuracyRequest {
        epsilon: 1e-18,
        ..AccuracyRequest::default()
    };
    let point = Vec3::new(0.5, 0.0, 0.0);
    let prepared = prepare_induction(
        &scene,
        &Domain::new(point, point),
        UVec3::ONE,
        None,
        0.1,
        accuracy,
    )
    .unwrap();

    let hidden = sample_induction(&prepared, &scene, 0.25, &ViewOptions::default());
    assert_eq!(hidden.electric.accuracy_not_met_total, 1);
    assert_eq!(hidden.electric.accuracy_not_met_drawn, 0);
    assert!(hidden.arrows.is_empty());

    scene.allow_inaccurate = true;
    let shown = sample_induction(&prepared, &scene, 0.25, &ViewOptions::default());
    assert_eq!(shown.electric.accuracy_not_met_drawn, 1);
    assert_eq!(shown.arrows.len(), 1);
}

#[test]
fn nonfinite_scene_is_rejected() {
    let mut scene = loop_scene();
    scene.amplitude = f64::NAN;
    assert_eq!(
        scene.validate().unwrap_err(),
        "induction amplitude must be finite"
    );
    scene = loop_scene();
    scene.center = DVec3::new(f64::INFINITY, 0.0, 0.0);
    assert_eq!(
        scene.validate().unwrap_err(),
        "induction center must be finite"
    );
    let prepared = prepare_induction(
        &loop_scene(),
        &Domain::default(),
        UVec3::new(2, 2, 2),
        None,
        0.0,
        Default::default(),
    );
    assert_eq!(
        prepared.unwrap_err(),
        "induction softening must be finite and positive"
    );

    let query = evaluate_induction_point(&loop_scene(), DVec3::new(f64::NAN, 0.0, 0.0), 0.0, 0.1);
    assert!(matches!(
        query.electric,
        vector_field_explorer::induction::InductionFamilyResult::Unavailable(
            vector_field_explorer::sources::StopReason::InvalidQuery
        )
    ));
    assert!(matches!(
        query.magnetic,
        vector_field_explorer::sources::FamilyResult::Unavailable(
            vector_field_explorer::sources::StopReason::InvalidQuery
        )
    ));
}

#[test]
fn validation_rejects_bad_geometry() {
    let mut scene = loop_scene();
    scene.radius = 0.0;
    assert_eq!(
        scene.validate().unwrap_err(),
        "induction radius must be finite and positive"
    );

    scene = loop_scene();
    scene.axis = DVec3::ZERO;
    assert_eq!(
        scene.validate().unwrap_err(),
        "induction axis must be normalized"
    );

    scene = loop_scene();
    scene.axis = DVec3::new(0.0, 0.0, 2.0);
    assert_eq!(
        scene.validate().unwrap_err(),
        "induction axis must be normalized"
    );

    scene = loop_scene();
    scene.case = InductionCase::Solenoid;
    scene.radius = 0.5;
    scene.length = 0.0;
    scene.turns = 8;
    assert_eq!(
        scene.validate().unwrap_err(),
        "induction solenoid length must be finite and positive"
    );

    scene = loop_scene();
    scene.case = InductionCase::Solenoid;
    scene.radius = 0.5;
    scene.length = 2.0;
    scene.turns = 0;
    assert_eq!(
        scene.validate().unwrap_err(),
        "induction solenoid turns must be at least 1"
    );

    scene = loop_scene();
    scene.frequency = -1.0;
    assert_eq!(
        scene.validate().unwrap_err(),
        "induction frequency must be finite and non-negative"
    );
}
