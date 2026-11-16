//! Task 1 scaled elementary point and infinite-wire fields; Task 2 finite-line
//! quadrature, resolution and bounded-work integration.

use std::cell::Cell;

use glam::DVec3;
use vector_field_explorer::sources::{
    AccuracyRequest, Estimate, EvalOptions, Evaluator, FamilyResult, Source, SourceKind, SourceSet,
    StopReason, WorkLimits,
};

use crate::support::sources as reference;

fn evaluator(sources: &[Source]) -> Evaluator {
    let set = SourceSet::new(sources, &WorkLimits::default(), &|| false).expect("source set");
    Evaluator::new(&set, EvalOptions::default(), &|| false).expect("evaluator")
}

fn evaluator_with(sources: &[Source], options: EvalOptions) -> Evaluator {
    let set = SourceSet::new(sources, &WorkLimits::default(), &|| false).expect("source set");
    Evaluator::new(&set, options, &|| false).expect("evaluator")
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

fn line(center: DVec3, axis: DVec3, charge: f64, length: f64) -> Source {
    Source {
        center,
        axis,
        kind: SourceKind::FiniteLineCharge { charge, length },
    }
}

fn assert_component_close(actual: f64, expected: f64, tolerance: f64, label: &str) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{label}: got {actual:e}, expected {expected:e}, tolerance {tolerance:e}"
    );
}

fn unresolved_or_work_limit(result: &FamilyResult) -> bool {
    matches!(
        result,
        FamilyResult::Unavailable(StopReason::UnresolvedFeature | StopReason::WorkLimit)
    )
}

#[test]
fn line_against_closed_form() {
    // L=2, Q=1, a=0.1 at radial 1, z 0: the axial component cancels and the
    // analytic answer is Er = 1/(1.01*sqrt(2.01)).
    let mut runner = evaluator(&[line(DVec3::ZERO, DVec3::Z, 1.0, 2.0)]);
    let line_estimate = estimate(
        runner
            .evaluate(DVec3::new(1.0, 0.0, 0.0), &|| false)
            .electric,
    );
    let expected_er = 1.0 / (1.01 * 2.01f64.sqrt());
    assert_component_close(line_estimate.value.x, expected_er, 2e-13, "radial");
    assert_component_close(line_estimate.value.y, 0.0, 2e-13, "azimuthal");
    assert_component_close(line_estimate.value.z, 0.0, 2e-13, "axial");
    assert!(
        line_estimate.diagnostics.work_units > 31,
        "integrated path should cost more than one elementary probe, got {}",
        line_estimate.diagnostics.work_units
    );

    // Axial off-center/end and off-axis queries against the independent spec
    // primitive.
    for (radial, axial) in [(0.0, 0.3), (0.0, 1.0), (0.0, -1.0), (0.4, 0.35), (0.4, 0.9)] {
        let (er, ez) = reference::line_components(radial, axial, 2.0, 1.0, 0.1);
        let actual = estimate(
            runner
                .evaluate(DVec3::new(radial, 0.0, axial), &|| false)
                .electric,
        );
        let expected = DVec3::new(er, 0.0, ez);
        let tolerance = 2e-13 * expected.length().max(1.0);
        assert!(
            (actual.value - expected).length() <= tolerance,
            "r={radial}, z={axial}: got {:?}, expected {expected:?}",
            actual.value
        );
    }

    // Sign, scale and frame normalization.
    let mut reversed = evaluator(&[line(DVec3::ZERO, DVec3::Z, -1.0, 2.0)]);
    let reversed_estimate = estimate(
        reversed
            .evaluate(DVec3::new(1.0, 0.0, 0.0), &|| false)
            .electric,
    );
    assert_component_close(reversed_estimate.value.x, -expected_er, 2e-13, "negated Q");

    // Scaling every length (L, a, query) by two scales the field by 1/4.
    let a = 0.1;
    let scale = 2.0;
    let mut scaled = evaluator_with(
        &[line(DVec3::ZERO, DVec3::Z, 1.0, 2.0 * scale)],
        EvalOptions {
            softening: a * scale,
            ..EvalOptions::default()
        },
    );
    let scaled_estimate = estimate(
        scaled
            .evaluate(DVec3::new(scale, 0.0, 0.0), &|| false)
            .electric,
    );
    let (expected_scaled_er, expected_scaled_ez) =
        reference::line_components(scale, 0.0, 2.0 * scale, 1.0, a * scale);
    let expected_scaled = DVec3::new(expected_scaled_er, 0.0, expected_scaled_ez);
    assert!(
        (scaled_estimate.value - expected_scaled).length() <= 2e-13,
        "scaled line gave {:?}, expected {expected_scaled:?}",
        scaled_estimate.value
    );

    let center = DVec3::new(0.5, -0.25, 2.0);
    let axis = DVec3::new(1.0, 2.0, 3.0).normalize();
    let seed = DVec3::X - axis * axis.dot(DVec3::X);
    let radial_dir = seed.normalize();
    let query = center + radial_dir * 0.7 + axis * 0.4;
    let (tilted_er, tilted_ez) = reference::line_components(0.7, 0.4, 2.0, 1.0, 0.1);
    let expected_tilted = radial_dir * tilted_er + axis * tilted_ez;
    let mut tilted = evaluator(&[line(center, axis, 1.0, 2.0)]);
    let tilted_estimate = estimate(tilted.evaluate(query, &|| false).electric);
    assert!(
        (tilted_estimate.value - expected_tilted).length() <= 2e-12,
        "tilted line gave {:?}, expected {expected_tilted:?}",
        tilted_estimate.value
    );

    // Far same-sign endpoint cases against committed high-precision values.
    for case in reference::FAR_SAME_SIGN_REFERENCES {
        let mut runner = evaluator_with(
            &[line(DVec3::ZERO, DVec3::Z, case.charge, case.length)],
            EvalOptions {
                softening: case.softening,
                ..EvalOptions::default()
            },
        );
        let actual = estimate(
            runner
                .evaluate(DVec3::new(case.radial, 0.0, case.axial), &|| false)
                .electric,
        );
        let expected = DVec3::new(case.er, 0.0, case.ez);
        let tolerance = 2e-13 * expected.length().max(1.0);
        assert!(
            (actual.value - expected).length() <= tolerance,
            "far case radial={} axial={}: got {:?}, expected {expected:?}",
            case.radial,
            case.axial,
            actual.value
        );
        assert!(
            actual.diagnostics.work_units > 31,
            "far case should still integrate, got {} work units",
            actual.diagnostics.work_units
        );
    }
}

#[test]
fn narrow_displaced_line() {
    let softening = 1e-6;
    let length = 2.0;
    let charge = 2.0 * softening;
    let source = line(DVec3::ZERO, DVec3::Z, charge, length);
    let narrow_options = EvalOptions {
        softening,
        ..EvalOptions::default()
    };
    let mut runner = evaluator_with(&[source], narrow_options);

    for axial in [0.0, 0.123, 0.431271] {
        let (er, ez) = reference::line_components(softening, axial, length, charge, softening);
        let actual = estimate(
            runner
                .evaluate(DVec3::new(softening, 0.0, axial), &|| false)
                .electric,
        );
        let expected = DVec3::new(er, 0.0, ez);
        let tolerance = 1e-9 + 1e-7 * expected.length();
        assert!(
            (actual.value - expected).length() <= tolerance,
            "narrow line z={axial}: got {:?}, expected {expected:?}",
            actual.value
        );
        assert!(
            actual.diagnostics.cells_high_water > 1,
            "resolution guard should have produced multiple leaves for z={axial}"
        );
        assert!(actual.uncertainty <= actual.target);
    }

    // A tighter request exercises the largest-uncertainty refinement loop.
    let mut default = evaluator_with(&[source], narrow_options);
    let default_estimate = estimate(
        default
            .evaluate(DVec3::new(softening, 0.0, 0.431271), &|| false)
            .electric,
    );
    let tight_options = EvalOptions {
        softening,
        accuracy: AccuracyRequest {
            epsilon: 1e-9,
            ..AccuracyRequest::default()
        },
        ..EvalOptions::default()
    };
    let mut tight = evaluator_with(&[source], tight_options);
    let tight_estimate = estimate(
        tight
            .evaluate(DVec3::new(softening, 0.0, 0.431271), &|| false)
            .electric,
    );
    let (tight_er, tight_ez) =
        reference::line_components(softening, 0.431271, length, charge, softening);
    let tight_expected = DVec3::new(tight_er, 0.0, tight_ez);
    assert!(
        (tight_estimate.value - tight_expected).length() <= 1e-9 + 1e-7 * tight_expected.length()
    );
    assert!(tight_estimate.uncertainty <= tight_estimate.target);
    assert!(
        tight_estimate.diagnostics.work_units > default_estimate.diagnostics.work_units,
        "tight request should refine beyond the default coarse partition: {} <= {}",
        tight_estimate.diagnostics.work_units,
        default_estimate.diagnostics.work_units
    );
    assert!(
        tight_estimate.diagnostics.cells_high_water > default_estimate.diagnostics.cells_high_water
    );

    // Lower the cell budget so the displaced feature cannot be resolved. The
    // result must publish no finite partial vector.
    let limited = EvalOptions {
        softening,
        limits: WorkLimits {
            cells_per_family: 4,
            ..WorkLimits::default()
        },
        ..EvalOptions::default()
    };
    let mut cell_limited = evaluator_with(&[source], limited);
    let result = cell_limited
        .evaluate(DVec3::new(softening, 0.0, 0.0), &|| false)
        .electric;
    assert!(
        unresolved_or_work_limit(&result),
        "expected no publishable estimate, got {result:?}"
    );

    let limited = EvalOptions {
        softening,
        limits: WorkLimits {
            work_per_family: 40,
            ..WorkLimits::default()
        },
        ..EvalOptions::default()
    };
    let mut work_limited = evaluator_with(&[source], limited);
    let result = work_limited
        .evaluate(DVec3::new(softening, 0.0, 0.0), &|| false)
        .electric;
    assert!(
        unresolved_or_work_limit(&result),
        "expected WorkLimit or UnresolvedFeature, got {result:?}"
    );
}

#[test]
fn budget_and_cancellation_are_bounded() {
    let source = line(DVec3::ZERO, DVec3::Z, 1.0, 2.0);
    let limits = WorkLimits::default();

    let mut runner = evaluator(&[source]);
    let line_estimate = estimate(
        runner
            .evaluate(DVec3::new(1.0, 0.0, 0.0), &|| false)
            .electric,
    );
    assert!(
        line_estimate.diagnostics.work_units <= limits.work_per_family,
        "work {} exceeded limit {}",
        line_estimate.diagnostics.work_units,
        limits.work_per_family
    );
    assert!(
        line_estimate.diagnostics.work_units >= 93,
        "parent plus half probes should cost at least 93 samples, got {}",
        line_estimate.diagnostics.work_units
    );
    assert!(
        line_estimate.diagnostics.cells_high_water <= limits.cells_per_family,
        "cells {} exceeded limit {}",
        line_estimate.diagnostics.cells_high_water,
        limits.cells_per_family
    );
    assert!(line_estimate.diagnostics.cells_high_water >= 1);

    // Successful run with a poll counter: work may not advance more than 32
    // counted units between cancellation polls.
    let polls = Cell::new(0u64);
    let counting = || {
        polls.set(polls.get() + 1);
        false
    };
    let mut runner = evaluator(&[source]);
    let line_estimate = estimate(
        runner
            .evaluate(DVec3::new(1.0, 0.0, 0.0), &counting)
            .electric,
    );
    let poll_count = polls.get();
    assert!(poll_count >= 1);
    assert!(
        line_estimate.diagnostics.work_units <= 32 * poll_count,
        "work {} over {} polls exceeds a 32-unit polling gap",
        line_estimate.diagnostics.work_units,
        poll_count
    );

    // Cancellation observed mid-integration publishes no estimate.
    let calls = Cell::new(0u32);
    let cancel_on_second = || {
        let next = calls.get() + 1;
        calls.set(next);
        next >= 2
    };
    let mut runner = evaluator(&[source]);
    let cancelled = runner.evaluate(DVec3::new(1.0, 0.0, 0.0), &cancel_on_second);
    assert!(
        matches!(
            cancelled.electric,
            FamilyResult::Unavailable(StopReason::Cancelled)
        ),
        "expected Cancelled, got {:?}",
        cancelled.electric
    );

    // A work budget too small even for the first resolved cell.
    let limited = EvalOptions {
        limits: WorkLimits {
            work_per_family: 40,
            ..WorkLimits::default()
        },
        ..EvalOptions::default()
    };
    let mut runner = evaluator_with(&[source], limited);
    let result = runner
        .evaluate(DVec3::new(1.0, 0.0, 0.0), &|| false)
        .electric;
    assert!(
        matches!(result, FamilyResult::Unavailable(StopReason::WorkLimit)),
        "expected WorkLimit with no vector, got {result:?}"
    );

    // Scratch preparation cannot hold the initial partition: no estimate.
    let limited = EvalOptions {
        limits: WorkLimits {
            cells_per_family: 1,
            ..WorkLimits::default()
        },
        ..EvalOptions::default()
    };
    let mut runner = evaluator_with(&[source], limited);
    let result = runner
        .evaluate(DVec3::new(1.0, 0.0, 0.0), &|| false)
        .electric;
    assert!(
        unresolved_or_work_limit(&result),
        "expected no publishable estimate with a one-cell scratch, got {result:?}"
    );

    // Depth refusal on a feature that cannot be resolved inside one split.
    let narrow_source = line(DVec3::ZERO, DVec3::Z, 2e-6, 2.0);
    let limited = EvalOptions {
        softening: 1e-6,
        limits: WorkLimits {
            max_depth: 1,
            ..WorkLimits::default()
        },
        ..EvalOptions::default()
    };
    let mut runner = evaluator_with(&[narrow_source], limited);
    let result = runner
        .evaluate(DVec3::new(1e-6, 0.0, 0.0), &|| false)
        .electric;
    assert!(
        matches!(
            result,
            FamilyResult::Unavailable(StopReason::UnresolvedFeature)
        ),
        "expected depth refusal, got {result:?}"
    );

    // A line shorter than half the smallest subnormal has a zero-width domain;
    // no midpoint makes representable progress, so refuse explicitly.
    let degenerate_length = f64::from_bits(1);
    let mut runner = evaluator(&[line(
        DVec3::ZERO,
        DVec3::Z,
        degenerate_length,
        degenerate_length,
    )]);
    let result = runner
        .evaluate(DVec3::new(0.1, 0.0, 0.0), &|| false)
        .electric;
    assert!(
        matches!(
            result,
            FamilyResult::Unavailable(StopReason::UnresolvedFeature)
        ),
        "expected repeated-midpoint refusal, got {result:?}"
    );
}
