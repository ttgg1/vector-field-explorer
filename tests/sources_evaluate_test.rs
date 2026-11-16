//! Task 5 family-global accuracy, independent family availability and the
//! delivered all-eight evaluator contract.
//!
//! References are independent of the production evaluator: elementary point
//! fields are closed form, current-loop fields use the complete elliptic
//! `K`/`E` reduction in `tests/support/sources`, and the committed fixture
//! matrix remains the high-precision off-axis oracle.

use std::cell::Cell;

use glam::DVec3;
use vector_field_explorer::sources::{
    AccuracyRequest, Estimate, EvalOptions, Evaluator, FamilyResult, FieldFamily, KERNEL_ID,
    Source, SourceKind, SourceSet, StopReason, WorkLimits,
};

use crate::support::sources as reference;

const SOFTENING: f64 = 0.1;
const EPSILON: f64 = 1e-7;
const SCALE: f64 = 1e-2;

fn evaluator_with(sources: &[Source], options: EvalOptions) -> Evaluator {
    let set = SourceSet::new(sources, &options.limits, &|| false).expect("source set");
    Evaluator::new(&set, options, &|| false).expect("evaluator")
}

fn estimate(result: FamilyResult) -> Estimate {
    match result {
        FamilyResult::TargetMet(estimate) => estimate,
        other => panic!("expected TargetMet, got {other:?}"),
    }
}

fn point(center: DVec3, charge: f64) -> Source {
    Source {
        center,
        axis: DVec3::Z,
        kind: SourceKind::PointCharge { charge },
    }
}

fn current_loop(center: DVec3, axis: DVec3, radius: f64, current: f64) -> Source {
    Source {
        center,
        axis,
        kind: SourceKind::CurrentLoop { current, radius },
    }
}

fn wire(center: DVec3, axis: DVec3, current: f64) -> Source {
    Source {
        center,
        axis,
        kind: SourceKind::StraightWire { current },
    }
}

fn solenoid(center: DVec3, radius: f64, length: f64, turns: u32, current: f64) -> Source {
    Source {
        center,
        axis: DVec3::Z,
        kind: SourceKind::Solenoid {
            current,
            radius,
            length,
            turns,
        },
    }
}

fn fixture_source(row: &reference::Fixture) -> Source {
    let kind = match row.kind.as_str() {
        "point" => SourceKind::PointCharge { charge: row.charge },
        "line" => SourceKind::FiniteLineCharge {
            charge: row.charge,
            length: row.dimension,
        },
        "ring" => SourceKind::ChargedRing {
            charge: row.charge,
            radius: row.dimension,
        },
        "disc" => SourceKind::ChargedDisc {
            charge: row.charge,
            radius: row.dimension,
        },
        "sphere" => SourceKind::SolidSphere {
            charge: row.charge,
            radius: row.dimension,
        },
        "wire" => SourceKind::StraightWire {
            current: row.charge,
        },
        "loop" => SourceKind::CurrentLoop {
            current: row.charge,
            radius: row.dimension,
        },
        "solenoid" => SourceKind::Solenoid {
            current: row.charge,
            radius: row.dimension,
            length: row.length,
            turns: row.turns,
        },
        other => panic!("unknown fixture kind {other}"),
    };
    Source {
        center: row.center,
        axis: row.axis,
        kind,
    }
}

fn fixture_family(row: &reference::Fixture) -> FieldFamily {
    match row.kind.as_str() {
        "point" | "line" | "ring" | "disc" | "sphere" => FieldFamily::Electric,
        "wire" | "loop" | "solenoid" => FieldFamily::Magnetic,
        other => panic!("unknown fixture kind {other}"),
    }
}

fn evaluate_fixture_row(row: &reference::Fixture) -> FamilyResult {
    let mut runner = evaluator_with(&[fixture_source(row)], fixture_options(row));
    let sample = runner.evaluate(row.query, &|| false);
    match fixture_family(row) {
        FieldFamily::Electric => sample.electric,
        FieldFamily::Magnetic => sample.magnetic,
    }
}

fn fixture_options(row: &reference::Fixture) -> EvalOptions {
    EvalOptions {
        softening: row.softening,
        ..EvalOptions::default()
    }
}

fn target(epsilon: f64, scale: f64, value: DVec3) -> f64 {
    epsilon * scale + epsilon * value.length()
}

fn assert_finite_estimate(estimate: &Estimate, label: &str) {
    assert!(estimate.value.is_finite(), "{label}: non-finite value");
    assert!(
        estimate.uncertainty.is_finite(),
        "{label}: non-finite uncertainty"
    );
    assert!(estimate.target.is_finite(), "{label}: non-finite target");
    assert!(
        estimate.absolute_contributions.is_finite(),
        "{label}: non-finite absolute contributions"
    );
}

#[test]
fn family_target_uses_cancelled_total() {
    let center = DVec3::new(0.2, -0.1, 0.3);
    let query = DVec3::new(0.7, 0.4, -0.2);
    let expected = 2.0 * reference::point_field(query, center, 1.0, SOFTENING);
    let charges = [1e6, -1e6, 2.0];
    let permutations = [
        [0usize, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];

    for order in permutations {
        let sources: Vec<Source> = order
            .iter()
            .map(|&index| point(center, charges[index]))
            .collect();
        let mut runner = evaluator_with(&sources, EvalOptions::default());
        let electric = estimate(runner.evaluate(query, &|| false).electric);
        assert_finite_estimate(&electric, "cancellation permutation");
        let error = (electric.value - expected).length();
        assert!(
            error <= 2e-14 + 2e-13 * expected.length(),
            "{order:?}: value {:?} differs from independent total {expected:?} by {error:e}",
            electric.value
        );
        assert!(
            (electric.target - target(EPSILON, SCALE, electric.value)).abs()
                <= 1e-18 + 1e-12 * electric.target,
            "{order:?}: target {} is not epsilon*S + epsilon*norm(V) for {:?}",
            electric.target,
            electric.value
        );
        assert!(
            electric.target < EPSILON * electric.absolute_contributions * 0.5,
            "{order:?}: cancellation target {} used the sum of absolute contributions {}",
            electric.target,
            electric.absolute_contributions
        );
        assert_eq!(electric.diagnostics.source_count, 3);
    }

    // The source values are roughly a million times the residual, so their
    // modeled absolute rounding, not the residual, dominates the uncertainty.
    // A family target must still be the residual criterion, not the sum of
    // source magnitudes.
    let cancelling = [point(center, 1e6), point(center, -1e6)];
    let mut runner = evaluator_with(&cancelling, EvalOptions::default());
    let zero = match runner.evaluate(query, &|| false).electric {
        FamilyResult::TargetMet(estimate) | FamilyResult::AccuracyNotMet(estimate, _) => estimate,
        other => panic!("expected a finite zero family, got {other:?}"),
    };
    assert_finite_estimate(&zero, "zero residual");
    assert_eq!(zero.value, DVec3::ZERO);
    assert_eq!(zero.target, EPSILON * SCALE);
    assert!(zero.target > 0.0);
    let source_value = 1e6 * reference::point_field(query, center, 1.0, SOFTENING).length();
    let per_source_allowance = 64.0 * f64::EPSILON * source_value;
    assert!(
        zero.uncertainty <= 2.1 * per_source_allowance,
        "zero residual uncertainty {:e} was inflated above the two source allowances {:e}",
        zero.uncertainty,
        per_source_allowance
    );

    // Family scales are independent: changing one only changes that family.
    let e_source = point(center, 1.0);
    let b_source = current_loop(DVec3::ZERO, DVec3::Z, 1.0, 1.0);
    let base = {
        let mut runner = evaluator_with(&[e_source, b_source], EvalOptions::default());
        runner.evaluate(query, &|| false)
    };
    let base_e = estimate(base.electric);
    let base_b = estimate(base.magnetic);

    let mut e_options = EvalOptions::default();
    e_options.accuracy.e_scale = 0.5;
    let changed_e_sample = {
        let mut runner = evaluator_with(&[e_source, b_source], e_options);
        runner.evaluate(query, &|| false)
    };
    let changed_e = estimate(changed_e_sample.electric);
    let unchanged_b = estimate(changed_e_sample.magnetic);
    assert!(
        (changed_e.target - target(EPSILON, 0.5, changed_e.value)).abs()
            <= 1e-18 + 1e-12 * changed_e.target
    );
    assert_eq!(unchanged_b.target, base_b.target);
    assert_eq!(unchanged_b.value, base_b.value);
    assert_eq!(unchanged_b.uncertainty, base_b.uncertainty);
    assert_ne!(changed_e.target, base_e.target);

    let mut b_options = EvalOptions::default();
    b_options.accuracy.b_scale = 0.5;
    let changed_b_sample = {
        let mut runner = evaluator_with(&[e_source, b_source], b_options);
        runner.evaluate(query, &|| false)
    };
    let unchanged_e = estimate(changed_b_sample.electric);
    let changed_b = estimate(changed_b_sample.magnetic);
    assert!(
        (changed_b.target - target(EPSILON, 0.5, changed_b.value)).abs()
            <= 1e-18 + 1e-12 * changed_b.target
    );
    assert_eq!(unchanged_e.target, base_e.target);
    assert_eq!(unchanged_e.value, base_e.value);
    assert_eq!(unchanged_e.uncertainty, base_e.uncertainty);
}

#[test]
fn both_family_orders_have_reserved_work() {
    let query = DVec3::new(0.7, 0.2, 1.1);
    let cheap_electric = point(DVec3::ZERO, 1.0);
    let difficult_magnetic = solenoid(DVec3::ZERO, 1.0, 2.0, 1, 1.0);
    let limits = WorkLimits {
        work_per_family: 2_000,
        ..WorkLimits::default()
    };
    let options = EvalOptions {
        limits,
        ..EvalOptions::default()
    };

    let orders = [
        vec![cheap_electric, difficult_magnetic],
        vec![difficult_magnetic, cheap_electric],
    ];
    let mut electric_work = None;
    let mut magnetic_work: Option<Option<u64>> = None;
    for sources in orders {
        let mut runner = evaluator_with(&sources, options);
        let sample = runner.evaluate(query, &|| false);
        let electric = estimate(sample.electric);
        assert_eq!(electric.diagnostics.source_count, 1);
        assert_eq!(electric.diagnostics.work_units, 1);
        electric_work.get_or_insert(electric.diagnostics.work_units);
        assert_eq!(Some(electric.diagnostics.work_units), electric_work);
        let expected = reference::point_field(query, DVec3::ZERO, 1.0, SOFTENING);
        assert!((electric.value - expected).length() <= 2e-15);
        let observed = match sample.magnetic {
            FamilyResult::AccuracyNotMet(estimate, reason) => {
                assert_eq!(reason, StopReason::WorkLimit);
                assert!(estimate.value.is_finite() && estimate.uncertainty.is_finite());
                assert!(estimate.uncertainty > estimate.target);
                assert_eq!(estimate.diagnostics.source_count, 1);
                assert!(estimate.diagnostics.work_units <= limits.work_per_family);
                Some(estimate.diagnostics.work_units)
            }
            FamilyResult::Unavailable(reason) => {
                assert_eq!(reason, StopReason::WorkLimit);
                None
            }
            other => panic!("expected the difficult solenoid to miss its work cap, got {other:?}"),
        };
        magnetic_work.get_or_insert(observed);
        assert_eq!(magnetic_work, Some(observed));
        let total = electric.diagnostics.work_units + observed.unwrap_or(0);
        assert!(
            total <= 2 * limits.work_per_family,
            "family work {total} exceeded two reservations"
        );
    }

    // The converse: a cheap magnetic wire stays usable when a difficult
    // electric disc cannot even complete its initial coverage.
    let cheap_magnetic = wire(DVec3::ZERO, DVec3::Z, 1.0);
    let difficult_electric = Source {
        center: DVec3::ZERO,
        axis: DVec3::Z,
        kind: SourceKind::ChargedDisc {
            charge: 1.0,
            radius: 1.0,
        },
    };
    let tight = EvalOptions {
        softening: SOFTENING,
        limits: WorkLimits {
            work_per_family: 40,
            ..WorkLimits::default()
        },
        ..EvalOptions::default()
    };
    for sources in [
        vec![difficult_electric, cheap_magnetic],
        vec![cheap_magnetic, difficult_electric],
    ] {
        let mut runner = evaluator_with(&sources, tight);
        let sample = runner.evaluate(DVec3::new(1.0, 0.0, 0.0), &|| false);
        assert!(
            matches!(
                sample.electric,
                FamilyResult::Unavailable(StopReason::WorkLimit | StopReason::UnresolvedFeature)
            ),
            "difficult electric disc unexpectedly published {:?}",
            sample.electric
        );
        let magnetic = estimate(sample.magnetic);
        assert_eq!(magnetic.diagnostics.source_count, 1);
        assert!(
            (magnetic.value - DVec3::Y * (2.0 * 1.0 / (1.0 + SOFTENING * SOFTENING))).length()
                <= 2e-15
        );
        assert!(magnetic.diagnostics.work_units <= tight.limits.work_per_family);
        assert_eq!(magnetic.diagnostics.work_units, 1);
    }

    // Query lifetime is a hard job limit: N accepted calls including an invalid
    // query, then QueryLimit for both families.
    let lifetime_limits = WorkLimits {
        max_queries: 3,
        ..WorkLimits::default()
    };
    let lifetime_options = EvalOptions {
        limits: lifetime_limits,
        ..EvalOptions::default()
    };
    let mut runner = evaluator_with(&[cheap_electric, cheap_magnetic], lifetime_options);
    let first = runner.evaluate(DVec3::new(0.4, 0.1, 0.2), &|| false);
    assert!(matches!(first.electric, FamilyResult::TargetMet(_)));
    assert!(matches!(first.magnetic, FamilyResult::TargetMet(_)));
    let invalid = runner.evaluate(DVec3::new(f64::NAN, 0.0, 0.0), &|| false);
    assert!(matches!(
        invalid.electric,
        FamilyResult::Unavailable(StopReason::InvalidQuery)
    ));
    assert!(matches!(
        invalid.magnetic,
        FamilyResult::Unavailable(StopReason::InvalidQuery)
    ));
    let third = runner.evaluate(DVec3::new(0.6, 0.3, 0.2), &|| false);
    assert!(matches!(third.electric, FamilyResult::TargetMet(_)));
    let over = runner.evaluate(DVec3::new(0.6, 0.3, 0.2), &|| false);
    assert!(matches!(
        over.electric,
        FamilyResult::Unavailable(StopReason::QueryLimit)
    ));
    assert!(matches!(
        over.magnetic,
        FamilyResult::Unavailable(StopReason::QueryLimit)
    ));

    // All eight kinds in one mixed set: family totals are the compensated sum
    // of the same independent per-source evaluations, and source order cannot
    // move work between families or alter the published totals.
    let mixed = [
        point(DVec3::new(0.1, 0.2, -0.1), 1.0),
        Source {
            center: DVec3::new(-0.2, 0.0, 0.1),
            axis: DVec3::Z,
            kind: SourceKind::FiniteLineCharge {
                charge: -1.5,
                length: 1.3,
            },
        },
        Source {
            center: DVec3::new(0.3, 0.0, 0.0),
            axis: DVec3::Z,
            kind: SourceKind::ChargedRing {
                charge: 2.0,
                radius: 0.8,
            },
        },
        Source {
            center: DVec3::new(0.0, 0.2, 0.0),
            axis: DVec3::Z,
            kind: SourceKind::ChargedDisc {
                charge: -0.8,
                radius: 0.7,
            },
        },
        Source {
            center: DVec3::new(0.0, 0.0, 0.2),
            axis: DVec3::Z,
            kind: SourceKind::SolidSphere {
                charge: 1.2,
                radius: 0.6,
            },
        },
        wire(DVec3::ZERO, DVec3::Z, 1.4),
        current_loop(DVec3::new(0.0, 0.1, 0.0), DVec3::Z, 0.9, -1.1),
        solenoid(DVec3::new(0.1, 0.0, 0.0), 0.5, 1.6, 2, 0.9),
    ];
    let mixed_query = DVec3::new(0.55, -0.35, 0.45);
    let mut expected_e = DVec3::ZERO;
    let mut expected_b = DVec3::ZERO;
    for source in &mixed {
        let mut runner = evaluator_with(&[*source], EvalOptions::default());
        let sample = runner.evaluate(mixed_query, &|| false);
        match source.kind {
            SourceKind::PointCharge { .. }
            | SourceKind::FiniteLineCharge { .. }
            | SourceKind::ChargedRing { .. }
            | SourceKind::ChargedDisc { .. }
            | SourceKind::SolidSphere { .. } => expected_e += estimate(sample.electric).value,
            SourceKind::StraightWire { .. }
            | SourceKind::CurrentLoop { .. }
            | SourceKind::Solenoid { .. } => expected_b += estimate(sample.magnetic).value,
        }
    }
    for order in [mixed.to_vec(), mixed.iter().rev().copied().collect()] {
        let mut runner = evaluator_with(&order, EvalOptions::default());
        let sample = runner.evaluate(mixed_query, &|| false);
        let electric = estimate(sample.electric);
        let magnetic = estimate(sample.magnetic);
        assert_eq!(electric.diagnostics.source_count, 5);
        assert_eq!(magnetic.diagnostics.source_count, 3);
        for (label, actual, expected) in [
            ("mixed electric", electric.value, expected_e),
            ("mixed magnetic", magnetic.value, expected_b),
        ] {
            let tolerance = 2e-14 + 2e-13 * expected.length().max(1.0);
            assert!(
                (actual - expected).length() <= tolerance,
                "{label}: {actual:?} differs from per-source sum {expected:?}"
            );
        }
    }
}

#[test]
fn all_eight_kinds_have_committed_references() {
    let kinds = [
        "point", "line", "ring", "disc", "sphere", "wire", "loop", "solenoid",
    ];
    for kind in kinds {
        let rows: Vec<reference::Fixture> = reference::fixtures_of(kind).collect();
        assert!(!rows.is_empty(), "missing committed {kind} references");
        for row in rows {
            let evaluated = estimate(evaluate_fixture_row(&row));
            let tolerance = 1e-9 + 1e-7 * row.reference.length();
            let error = (evaluated.value - row.reference).length();
            assert!(
                error <= tolerance,
                "{}: {kind} fixture error {error:e} > {tolerance:e}",
                row.id
            );
        }
    }
}

#[test]
fn finite_inaccurate_is_complete_only() {
    // A point is analytic, so a 1e-30 request can only reach its roundoff
    // floor. It still publishes a complete finite estimate and says why.
    let mut runner = evaluator_with(
        &[point(DVec3::ZERO, 1.0)],
        EvalOptions {
            accuracy: AccuracyRequest {
                epsilon: 1e-30,
                ..AccuracyRequest::default()
            },
            ..EvalOptions::default()
        },
    );
    match runner
        .evaluate(DVec3::new(0.4, 0.2, 0.1), &|| false)
        .electric
    {
        FamilyResult::AccuracyNotMet(estimate, reason) => {
            assert_finite_estimate(&estimate, "inaccurate point");
            assert_eq!(reason, StopReason::RoundoffLimited);
            assert!(estimate.uncertainty > estimate.target);
            assert!(estimate.target > 0.0);
            assert!(estimate.diagnostics.roundoff_limited);
            assert!(estimate.diagnostics.work_units >= 1);
            assert_eq!(estimate.diagnostics.kernel_id, KERNEL_ID);
            assert_eq!(estimate.diagnostics.family, FieldFamily::Electric);
            assert_eq!(estimate.diagnostics.softening, SOFTENING);
            assert_eq!(estimate.diagnostics.epsilon, 1e-30);
            assert_eq!(estimate.diagnostics.family_scale, SCALE);
            assert!(estimate.diagnostics.model_regularized);
            assert_eq!(estimate.diagnostics.source_count, 1);
        }
        other => panic!("expected finite AccuracyNotMet, got {other:?}"),
    }

    // A refiniable geometry with a tight request retains its complete coarse
    // partition but cannot meet the target within the hard work cap.
    let row = reference::fixtures()
        .into_iter()
        .find(|row| row.id == "disc_offaxis_z0")
        .expect("disc fixture");
    let mut runner = evaluator_with(
        &[fixture_source(&row)],
        EvalOptions {
            softening: row.softening,
            accuracy: AccuracyRequest {
                epsilon: 1e-30,
                ..AccuracyRequest::default()
            },
            ..EvalOptions::default()
        },
    );
    match runner.evaluate(row.query, &|| false).electric {
        FamilyResult::AccuracyNotMet(estimate, reason) => {
            assert_finite_estimate(&estimate, "inaccurate disc");
            assert!(estimate.uncertainty > estimate.target);
            assert!(matches!(
                reason,
                StopReason::WorkLimit
                    | StopReason::StorageLimit
                    | StopReason::RoundoffLimited
                    | StopReason::UnresolvedFeature
            ));
            assert!(estimate.diagnostics.work_units > 0);
            assert!(estimate.diagnostics.work_units <= WorkLimits::default().work_per_family);
        }
        other => panic!("expected complete finite AccuracyNotMet, got {other:?}"),
    }

    // Incomplete preparation never publishes a vector.
    let cell_limited = EvalOptions {
        softening: row.softening,
        limits: WorkLimits {
            cells_per_family: 1,
            ..WorkLimits::default()
        },
        ..EvalOptions::default()
    };
    let mut runner = evaluator_with(&[fixture_source(&row)], cell_limited);
    assert!(matches!(
        runner.evaluate(row.query, &|| false).electric,
        FamilyResult::Unavailable(_)
    ));

    // An unresolved narrow feature is unavailable, not a finite partial.
    let narrow = Source {
        center: DVec3::ZERO,
        axis: DVec3::Z,
        kind: SourceKind::ChargedDisc {
            charge: 1.0,
            radius: 1.0,
        },
    };
    let mut runner = evaluator_with(
        &[narrow],
        EvalOptions {
            softening: 1e-6,
            limits: WorkLimits {
                max_depth: 2,
                ..WorkLimits::default()
            },
            ..EvalOptions::default()
        },
    );
    let narrow_result = runner
        .evaluate(DVec3::new(1.0, 0.0, 0.0), &|| false)
        .electric;
    assert!(matches!(
        narrow_result,
        FamilyResult::Unavailable(StopReason::UnresolvedFeature | StopReason::WorkLimit)
    ));

    // NaN/infinite queries are invalid, not inaccurate estimates.
    let mut runner = evaluator_with(&[point(DVec3::ZERO, 1.0)], EvalOptions::default());
    for query in [
        DVec3::new(f64::NAN, 0.0, 0.0),
        DVec3::new(f64::INFINITY, 0.0, 0.0),
    ] {
        let sample = runner.evaluate(query, &|| false);
        assert!(matches!(
            sample.electric,
            FamilyResult::Unavailable(StopReason::InvalidQuery)
        ));
        assert!(matches!(
            sample.magnetic,
            FamilyResult::Unavailable(StopReason::InvalidQuery)
        ));
    }

    // Cancellation after the empty electric family completed preserves its
    // exact zero and never partially publishes the magnetic family.
    let mut runner = evaluator_with(&[wire(DVec3::ZERO, DVec3::Z, 1.0)], EvalOptions::default());
    let polls = Cell::new(0u32);
    let cancelled = || {
        let poll = polls.get();
        polls.set(poll + 1);
        poll >= 1
    };
    let sample = runner.evaluate(DVec3::new(0.3, 0.2, 0.1), &cancelled);
    let electric = estimate(sample.electric);
    assert_eq!(electric.value, DVec3::ZERO);
    assert_eq!(electric.uncertainty, 0.0);
    assert!(matches!(
        sample.magnetic,
        FamilyResult::Unavailable(StopReason::Cancelled)
    ));

    // Diagnostics are immutable provenance for the supplied options.
    let options = EvalOptions {
        softening: 0.25,
        accuracy: AccuracyRequest {
            epsilon: 1e-5,
            e_scale: 0.5,
            b_scale: 0.75,
        },
        ..EvalOptions::default()
    };
    let mut runner = evaluator_with(
        &[
            point(DVec3::new(0.1, 0.2, 0.3), 1.0),
            current_loop(DVec3::ZERO, DVec3::Z, 1.0, 1.0),
        ],
        options,
    );
    let sample = runner.evaluate(DVec3::new(0.4, 0.5, 0.6), &|| false);
    let electric = estimate(sample.electric);
    let magnetic = estimate(sample.magnetic);
    assert_eq!(electric.diagnostics.kernel_id, KERNEL_ID);
    assert_eq!(electric.diagnostics.family, FieldFamily::Electric);
    assert_eq!(electric.diagnostics.softening, 0.25);
    assert_eq!(electric.diagnostics.epsilon, 1e-5);
    assert_eq!(electric.diagnostics.family_scale, 0.5);
    assert_eq!(magnetic.diagnostics.family, FieldFamily::Magnetic);
    assert_eq!(magnetic.diagnostics.family_scale, 0.75);
    assert_eq!(electric.diagnostics.source_count, 1);
    assert_eq!(magnetic.diagnostics.source_count, 1);
}

#[test]
fn convergence_tracks_requested_accuracy() {
    let selected = [
        "disc_offaxis_z0p7",
        "solenoid_end_effect",
        "loop_offaxis_z0p2",
        "ring_delta_099",
        "loop_delta_099",
    ];
    let epsilons = [1e-4, 1e-7, 1e-10];
    let mut improving = 0usize;
    for id in selected {
        let row = reference::fixtures()
            .into_iter()
            .find(|row| row.id == id)
            .expect("selected fixture");
        let mut evaluated = Vec::new();
        for epsilon in epsilons {
            let options = EvalOptions {
                softening: row.softening,
                accuracy: AccuracyRequest {
                    epsilon,
                    ..AccuracyRequest::default()
                },
                ..EvalOptions::default()
            };
            let mut runner = evaluator_with(&[fixture_source(&row)], options);
            let sample = runner.evaluate(row.query, &|| false);
            let result = match fixture_family(&row) {
                FieldFamily::Electric => sample.electric,
                FieldFamily::Magnetic => sample.magnetic,
            };
            let estimate = match result {
                FamilyResult::TargetMet(estimate) => estimate,
                FamilyResult::AccuracyNotMet(estimate, reason) => {
                    assert!(estimate.value.is_finite());
                    assert!(estimate.uncertainty.is_finite());
                    assert!(estimate.uncertainty > estimate.target);
                    assert!(matches!(
                        reason,
                        StopReason::WorkLimit
                            | StopReason::StorageLimit
                            | StopReason::RoundoffLimited
                            | StopReason::UnresolvedFeature
                    ));
                    estimate
                }
                other => panic!("{id} at epsilon {epsilon:e}: {other:?}"),
            };
            assert_finite_estimate(&estimate, id);
            assert!(
                (estimate.target - target(epsilon, SCALE, estimate.value)).abs()
                    <= 1e-18 + 1e-12 * estimate.target,
                "{id}: target {} does not match epsilon {epsilon:e}",
                estimate.target
            );
            evaluated.push((epsilon, estimate));
        }
        // The default request is required to be met on these ordinary cases.
        assert!(
            evaluated[1].1.uncertainty <= evaluated[1].1.target,
            "{id}: default epsilon did not meet its target"
        );
        let errors: Vec<f64> = evaluated
            .iter()
            .map(|(_, estimate)| (estimate.value - row.reference).length())
            .collect();
        let reference_floor = 1e-15 + 1e-14 * row.reference.length() + row.uncertainty;
        assert!(
            errors[2] <= errors[0] + reference_floor,
            "{id}: tight error {:e} exceeds loose error {:e} plus floor {:e}",
            errors[2],
            errors[0],
            reference_floor
        );
        if let Some((_, tight)) = evaluated.get(2)
            && tight.uncertainty <= tight.target
        {
            let tolerance = 1e-12 + 1e-10 * row.reference.length();
            assert!(
                errors[2] <= tolerance,
                "{id}: measured error {:e} exceeds {tolerance:e} for TargetMet tight case",
                errors[2]
            );
        }
        if errors[2] < errors[0] {
            improving += 1;
        }
    }
    assert!(
        improving >= 1,
        "no selected case improved between loose and tight requests"
    );

    // Cancellation keeps the family target and available status stable while
    // the requested epsilon tightens; the residual is known independently.
    let center = DVec3::new(0.1, -0.2, 0.3);
    let query = DVec3::new(0.6, 0.1, 0.4);
    let residual = 1e-9;
    let sources = [point(center, 1.0), point(center, -1.0 + residual)];
    let expected = residual * reference::point_field(query, center, 1.0, SOFTENING);
    for epsilon in epsilons {
        let options = EvalOptions {
            softening: SOFTENING,
            accuracy: AccuracyRequest {
                epsilon,
                ..AccuracyRequest::default()
            },
            ..EvalOptions::default()
        };
        let mut runner = evaluator_with(&sources, options);
        let electric = estimate(runner.evaluate(query, &|| false).electric);
        assert_finite_estimate(&electric, "cancellation convergence");
        assert!(electric.uncertainty <= electric.target);
        assert!(
            (electric.target - target(epsilon, SCALE, electric.value)).abs()
                <= 1e-18 + 1e-12 * electric.target
        );
        let error = (electric.value - expected).length();
        assert!(
            error <= 1e-12 + 1e-10 * expected.length(),
            "cancellation at epsilon {epsilon:e}: error {error:e}"
        );
        if epsilon == epsilons[2] {
            assert!(error <= 1e-14 + 1e-12 * expected.length());
        }
    }
}

#[test]
fn point_dipole_grid_matches_analytic_sums() {
    let points = [
        point(DVec3::ZERO, 1.0),
        point(DVec3::new(0.5, 0.0, 0.5), -2.0),
        point(DVec3::new(-0.5, 0.5, 0.0), 0.5),
    ];
    let loops = [
        current_loop(DVec3::ZERO, DVec3::Z, 0.4, 1.0),
        current_loop(DVec3::new(0.5, 0.0, 0.5), DVec3::Y, 0.65, -2.0),
        current_loop(DVec3::new(-0.5, 0.5, -0.5), DVec3::X, 0.8, 0.5),
    ];
    let sources: Vec<Source> = points.iter().chain(loops.iter()).copied().collect();
    let options = EvalOptions {
        softening: SOFTENING,
        accuracy: AccuracyRequest {
            epsilon: 1e-13,
            ..AccuracyRequest::default()
        },
        ..EvalOptions::default()
    };
    let mut runner = evaluator_with(&sources, options);
    let coordinates = [-1.0, -0.5, 0.0, 0.5, 1.0];
    let mut checked = 0usize;
    for &x in &coordinates {
        for &y in &coordinates {
            for &z in &coordinates {
                let query = DVec3::new(x, y, z);
                let sample = runner.evaluate(query, &|| false);
                let electric = match sample.electric {
                    FamilyResult::TargetMet(estimate)
                    | FamilyResult::AccuracyNotMet(estimate, _) => estimate,
                    other => panic!("grid electric at {query:?}: {other:?}"),
                };
                let magnetic = match sample.magnetic {
                    FamilyResult::TargetMet(estimate)
                    | FamilyResult::AccuracyNotMet(estimate, _) => estimate,
                    other => panic!("grid magnetic at {query:?}: {other:?}"),
                };
                let mut expected_electric = DVec3::ZERO;
                for source in points {
                    let SourceKind::PointCharge { charge } = source.kind else {
                        unreachable!()
                    };
                    expected_electric +=
                        reference::point_field(query, source.center, charge, SOFTENING);
                }
                let mut expected_magnetic = DVec3::ZERO;
                for source in loops {
                    let SourceKind::CurrentLoop { current, radius } = source.kind else {
                        unreachable!()
                    };
                    expected_magnetic += reference::loop_field(
                        query,
                        source.center,
                        source.axis,
                        radius,
                        current,
                        SOFTENING,
                    );
                }
                for (family, actual, expected) in [
                    ("electric", electric.value, expected_electric),
                    ("magnetic", magnetic.value, expected_magnetic),
                ] {
                    let tolerance = 2e-14 + 2e-13 * expected.length();
                    let error = (actual - expected).length();
                    assert!(
                        error <= tolerance,
                        "{family} grid {query:?}: error {error:e} > {tolerance:e} \
                         (got {actual:?}, expected {expected:?})"
                    );
                }
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 125);
}

#[test]
fn underflow_and_numeric_range_are_distinct() {
    // A representable subnormal answer publishes a finite value with an
    // underflow flag, never an unavailable family.
    let far = point(DVec3::ZERO, 1.0);
    let mut runner = evaluator_with(&[far], EvalOptions::default());
    let sample = runner.evaluate(DVec3::new(1e154, 1e154, 0.0), &|| false);
    let estimate = match sample.electric {
        FamilyResult::TargetMet(estimate) => estimate,
        other => panic!("expected a finite underflow TargetMet, got {other:?}"),
    };
    assert!(estimate.value.is_finite());
    assert!(estimate.value.x > 0.0);
    assert!(estimate.diagnostics.underflow);

    // A displacement whose squared norm cannot be represented is explicitly
    // refused, with no fabricated zero.
    let unrepresentable = point(DVec3::new(-1e308, -1e308, 0.0), 1.0);
    let mut runner = evaluator_with(&[unrepresentable], EvalOptions::default());
    assert!(matches!(
        runner
            .evaluate(DVec3::new(1e308, 1e308, 0.0), &|| false)
            .electric,
        FamilyResult::Unavailable(StopReason::NumericRange)
    ));
}
