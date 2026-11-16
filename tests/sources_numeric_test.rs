//! Task 1 scaled elementary point and infinite-wire fields.

use glam::DVec3;
use vector_field_explorer::sources::{
    Estimate, EvalOptions, Evaluator, FamilyResult, Source, SourceKind, SourceSet, StopReason,
    WorkLimits,
};

fn evaluator(sources: &[Source]) -> Evaluator {
    let set = SourceSet::new(sources, &WorkLimits::default(), &|| false).expect("source set");
    Evaluator::new(&set, EvalOptions::default(), &|| false).expect("evaluator")
}

fn point(center: DVec3, charge: f64) -> Source {
    Source {
        center,
        axis: DVec3::ZERO,
        kind: SourceKind::PointCharge { charge },
    }
}

fn wire(axis: DVec3, current: f64) -> Source {
    Source {
        center: DVec3::ZERO,
        axis,
        kind: SourceKind::StraightWire { current },
    }
}

fn estimate(result: FamilyResult) -> Estimate {
    match result {
        FamilyResult::TargetMet(estimate) => estimate,
        other => panic!("expected TargetMet, got {other:?}"),
    }
}

fn relative_error(actual: DVec3, expected: DVec3) -> f64 {
    let difference = (actual - expected).abs().max_element();
    let scale = expected.abs().max_element();
    difference / scale
}

#[test]
fn huge_near_axis_wire_is_refused_not_confidently_wrong() {
    // Nearly parallel to the axis at extreme scale: the projected perpendicular
    // (here truly (0, 0, 1)) lies far below the rounding scale of the
    // displacement, so no direction or magnitude can be trusted. Refuse
    // explicitly rather than publishing a fabricated TargetMet.
    let axis = DVec3::new(1.0, 1.0, 0.0);
    let mut evaluator = evaluator(&[wire(axis, 1.0)]);
    let refused = evaluator.evaluate(DVec3::new(1e300, 1e300, 1.0), &|| false);
    assert!(
        matches!(
            refused.magnetic,
            FamilyResult::Unavailable(StopReason::NumericRange)
        ),
        "expected NumericRange, got {:?}",
        refused.magnetic
    );

    // An ordinary off-axis query on the same wire still evaluates.
    let unit = axis.normalize();
    let displacement = DVec3::new(1.0, 0.0, 0.0);
    let perpendicular = displacement - unit * displacement.dot(unit);
    let expected = 2.0 * unit.cross(perpendicular) / (perpendicular.length_squared() + 0.01);
    let ordinary = estimate(evaluator.evaluate(displacement, &|| false).magnetic);
    assert!(
        (ordinary.value - expected).length() <= 2e-13,
        "ordinary wire gave {:?}, expected {expected:?}",
        ordinary.value
    );
}

#[test]
fn scaled_elementary_fields() {
    let mut point_evaluator = evaluator(&[point(DVec3::ZERO, 1.0)]);
    let point_sample = point_evaluator.evaluate(DVec3::new(1.0, 0.0, 0.0), &|| false);
    let point_estimate = estimate(point_sample.electric);
    let expected = 1.0 / 1.01f64.powf(1.5);
    assert!((point_estimate.value.x - expected).abs() <= 2e-15);
    assert_eq!(point_estimate.value.y, 0.0);
    assert_eq!(point_estimate.value.z, 0.0);
    assert!(point_estimate.uncertainty.is_finite());
    assert!(point_estimate.uncertainty <= point_estimate.target);

    let center = point_evaluator.evaluate(DVec3::ZERO, &|| false);
    assert_eq!(estimate(center.electric).value, DVec3::ZERO);

    let mut wire_evaluator = evaluator(&[wire(DVec3::Z, 1.0)]);
    let wire_sample = wire_evaluator.evaluate(DVec3::new(1.0, 0.0, 0.0), &|| false);
    let wire_estimate = estimate(wire_sample.magnetic);
    let expected_wire = DVec3::new(0.0, 2.0 / 1.01, 0.0);
    assert!(
        (wire_estimate.value - expected_wire).length() <= 2e-15,
        "wire gave {:?}",
        wire_estimate.value
    );

    let on_axis = wire_evaluator.evaluate(DVec3::new(0.0, 0.0, 5.0), &|| false);
    assert_eq!(estimate(on_axis.magnetic).value, DVec3::ZERO);

    let mut huge = evaluator(&[point(DVec3::ZERO, 1.0)]);
    let sample_1e103 = huge.evaluate(DVec3::new(1e103, 1e103, 0.0), &|| false);
    let estimate_1e103 = estimate(sample_1e103.electric);
    let expected_1e103 = DVec3::new(3.5355339059327376e-207, 3.5355339059327376e-207, 0.0);
    assert!(
        relative_error(estimate_1e103.value, expected_1e103) <= 2e-14,
        "1e103 gave {:?}",
        estimate_1e103.value
    );
    assert!(!estimate_1e103.diagnostics.underflow);

    let sample_1e154 = huge.evaluate(DVec3::new(1e154, 1e154, 0.0), &|| false);
    let estimate_1e154 = estimate(sample_1e154.electric);
    let expected_1e154 = 3.535_533_905_932_736e-309;
    let subnormal_step = f64::from_bits(1);
    assert!(
        (estimate_1e154.value.x - expected_1e154).abs() <= 16.0 * subnormal_step,
        "1e154 x gave {:?}",
        estimate_1e154.value.x
    );
    assert!(
        (estimate_1e154.value.y - expected_1e154).abs() <= 16.0 * subnormal_step,
        "1e154 y gave {:?}",
        estimate_1e154.value.y
    );
    assert_eq!(estimate_1e154.value.z, 0.0);
    assert!(estimate_1e154.diagnostics.underflow);

    // A 1e308 displacement overflows d^2 and d^3 naively; the field genuinely
    // underflows to zero and must not be reported as NumericRange.
    let sample_1e308 = huge.evaluate(DVec3::new(1e308, 1e308, 0.0), &|| false);
    let estimate_1e308 = estimate(sample_1e308.electric);
    assert_eq!(estimate_1e308.value, DVec3::ZERO);
    assert!(estimate_1e308.diagnostics.underflow);
    assert!(estimate_1e308.uncertainty.is_finite());

    let mut unrepresentable = evaluator(&[point(DVec3::new(-1e308, -1e308, 0.0), 1.0)]);
    let sample = unrepresentable.evaluate(DVec3::new(1e308, 1e308, 0.0), &|| false);
    assert!(matches!(
        sample.electric,
        FamilyResult::Unavailable(StopReason::NumericRange)
    ));

    let mut reversed = evaluator(&[point(DVec3::ZERO, -1.0)]);
    let reversed_sample = reversed.evaluate(DVec3::new(1.0, 0.0, 0.0), &|| false);
    assert!((estimate(reversed_sample.electric).value.x + expected).abs() <= 2e-15);

    let mut translated = evaluator(&[point(DVec3::new(2.0, 3.0, 4.0), 1.0)]);
    let translated_sample = translated.evaluate(DVec3::new(3.0, 3.0, 4.0), &|| false);
    assert!((estimate(translated_sample.electric).value.x - expected).abs() <= 2e-15);

    let axis = DVec3::new(1.0, 2.0, 3.0).normalize();
    let center = DVec3::new(0.5, -0.25, 2.0);
    let query = center + DVec3::new(0.3, -0.7, 0.4);
    let mut tilted = evaluator(&[Source {
        center,
        axis,
        kind: SourceKind::StraightWire { current: 1.0 },
    }]);
    let tilted_estimate = estimate(tilted.evaluate(query, &|| false).magnetic);
    let delta = query - center;
    let perpendicular = delta - axis * delta.dot(axis);
    let cross = axis.cross(perpendicular);
    let expected_tilted = 2.0 * cross / (perpendicular.length_squared() + 0.01);
    assert!(
        (tilted_estimate.value - expected_tilted).length() <= 2e-13,
        "tilted wire gave {:?}, expected {expected_tilted:?}",
        tilted_estimate.value
    );
}
