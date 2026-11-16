//! Task 3 stable electric ring/disc/sphere reductions and independent fixtures.

use glam::{DMat3, DVec3};
use vector_field_explorer::sources::{
    AccuracyRequest, Estimate, EvalOptions, Evaluator, FamilyResult, Source, SourceKind, SourceSet,
    StopReason, WorkLimits,
};

use crate::support::sources as reference;

const SOFTENING: f64 = 0.1;

fn evaluator(sources: &[Source]) -> Evaluator {
    evaluator_with(sources, EvalOptions::default())
}

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

fn ring(radius: f64, charge: f64) -> Source {
    Source {
        center: DVec3::ZERO,
        axis: DVec3::Z,
        kind: SourceKind::ChargedRing { charge, radius },
    }
}

fn disc(radius: f64, charge: f64) -> Source {
    Source {
        center: DVec3::ZERO,
        axis: DVec3::Z,
        kind: SourceKind::ChargedDisc { charge, radius },
    }
}

fn sphere(radius: f64, charge: f64) -> Source {
    Source {
        center: DVec3::ZERO,
        axis: DVec3::ZERO,
        kind: SourceKind::SolidSphere { charge, radius },
    }
}

fn source_for(row: &reference::Fixture) -> Source {
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
        other => panic!("unknown fixture kind {other}"),
    };
    Source {
        center: row.center,
        axis: row.axis,
        kind,
    }
}

fn options_for(row: &reference::Fixture) -> EvalOptions {
    EvalOptions {
        softening: row.softening,
        ..EvalOptions::default()
    }
}

fn assert_close(actual: DVec3, expected: DVec3, tolerance: f64, label: &str) {
    let error = (actual - expected).length();
    assert!(
        error <= tolerance,
        "{label}: got {actual:?}, expected {expected:?}, error {error:e} > {tolerance:e}"
    );
}

#[test]
fn analytic_axial_fields_and_scaling() {
    // Charged ring on axis: Q z / (R^2 + z^2 + a^2)^(3/2).
    let mut runner = evaluator(&[ring(1.0, 1.0)]);
    let axis = estimate(
        runner
            .evaluate(DVec3::new(0.0, 0.0, 0.7), &|| false)
            .electric,
    );
    let expected_axis = 0.7 / 1.5f64.powf(1.5);
    assert!(
        (axis.value.z - expected_axis).abs() <= 2e-14,
        "ring axis {axis:?}"
    );
    assert_eq!(axis.value.x, 0.0);
    assert_eq!(axis.value.y, 0.0);

    // Charged disc on axis via the rationalized difference:
    // Ez = 2 Q z / (b D (D + b)), b = sqrt(z^2+a^2), D = sqrt(R^2+z^2+a^2).
    let mut runner = evaluator(&[disc(1.0, 1.0)]);
    let axis = estimate(
        runner
            .evaluate(DVec3::new(0.0, 0.0, 0.7), &|| false)
            .electric,
    );
    let b = (0.7f64 * 0.7 + SOFTENING * SOFTENING).sqrt();
    let d = (1.0 + 0.7f64 * 0.7 + SOFTENING * SOFTENING).sqrt();
    let expected_disc = 2.0 * 0.7 / (b * d * (d + b));
    assert!(
        (axis.value.z - expected_disc).abs() <= 2e-14,
        "disc axis {axis:?}, expected {expected_disc:e}"
    );

    // Solid sphere centre is exactly zero; the softened near-centre slope is
    // Q / (R^2 + a^2)^(3/2).
    let mut runner = evaluator(&[sphere(1.0, 1.0)]);
    let centre = estimate(runner.evaluate(DVec3::ZERO, &|| false).electric);
    assert_eq!(centre.value, DVec3::ZERO);
    assert!(centre.uncertainty.is_finite());

    let far = 1e-8;
    let near = estimate(
        runner
            .evaluate(DVec3::new(far, 0.0, 0.0), &|| false)
            .electric,
    );
    let slope = near.value.x / far;
    let expected_slope = 1.0 / 1.01f64.powf(1.5);
    let relative = (slope - expected_slope).abs() / expected_slope;
    assert!(
        relative <= 1e-12,
        "sphere near-centre slope {slope:e}, expected {expected_slope:e}, relative {relative:e}"
    );

    // Total-strength scaling and sign.
    let doubled = estimate(
        evaluator(&[ring(1.0, 2.0)])
            .evaluate(DVec3::new(0.4, 0.0, 0.7), &|| false)
            .electric,
    );
    let base = estimate(
        evaluator(&[ring(1.0, 1.0)])
            .evaluate(DVec3::new(0.4, 0.0, 0.7), &|| false)
            .electric,
    );
    assert_close(
        doubled.value,
        2.0 * base.value,
        2e-14,
        "doubled ring charge",
    );
    let negated = estimate(
        evaluator(&[ring(1.0, -1.0)])
            .evaluate(DVec3::new(0.4, 0.0, 0.7), &|| false)
            .electric,
    );
    assert_close(negated.value, -base.value, 2e-14, "negated ring charge");

    // Scaling every length (R, a, query) by s scales E by 1/s^2.
    let scaled_options = EvalOptions {
        softening: 2.0 * SOFTENING,
        ..EvalOptions::default()
    };
    let scaled = estimate(
        evaluator_with(&[ring(2.0, 1.0)], scaled_options)
            .evaluate(DVec3::new(0.8, 0.0, 1.4), &|| false)
            .electric,
    );
    assert_close(scaled.value, base.value / 4.0, 2e-14, "scaled ring");

    let disc_base = estimate(
        evaluator(&[disc(1.0, 1.0)])
            .evaluate(DVec3::new(0.4, 0.0, 0.7), &|| false)
            .electric,
    );
    let disc_scaled = estimate(
        evaluator_with(&[disc(2.0, 1.0)], scaled_options)
            .evaluate(DVec3::new(0.8, 0.0, 1.4), &|| false)
            .electric,
    );
    assert_close(
        disc_scaled.value,
        disc_base.value / 4.0,
        2e-13,
        "scaled disc",
    );

    // Off-axis frame covariance: rotate and translate the canonical ring and
    // disc. An orthogonal rotation must reproduce the rotated vector.
    let centre = DVec3::new(0.5, -0.25, 2.0);
    let unit_axis = DVec3::new(1.0, 2.0, 3.0).normalize();
    let seed = DVec3::X - unit_axis * unit_axis.dot(DVec3::X);
    let radial = seed.normalize();
    let tangent = unit_axis.cross(radial);
    let rotation = DMat3::from_cols(radial, tangent, unit_axis);
    let local_query = DVec3::new(0.4, 0.0, 0.7);
    let world_query = centre + rotation * local_query;

    let rotated_ring = estimate(
        evaluator(&[Source {
            center: centre,
            axis: unit_axis,
            kind: SourceKind::ChargedRing {
                charge: 1.0,
                radius: 1.0,
            },
        }])
        .evaluate(world_query, &|| false)
        .electric,
    );
    assert_close(
        rotated_ring.value,
        rotation * base.value,
        2e-12 * base.value.length(),
        "rotated ring",
    );
    assert!(
        (rotated_ring.value.length() - base.value.length()).abs() <= 2e-12 * base.value.length(),
        "rotation changed the ring vector norm"
    );

    let rotated_disc = estimate(
        evaluator(&[Source {
            center: centre,
            axis: unit_axis,
            kind: SourceKind::ChargedDisc {
                charge: 1.0,
                radius: 1.0,
            },
        }])
        .evaluate(world_query, &|| false)
        .electric,
    );
    assert_close(
        rotated_disc.value,
        rotation * disc_base.value,
        2e-12 * disc_base.value.length(),
        "rotated disc",
    );
}

#[test]
fn sphere_near_surface_small_softening_is_stable() {
    // Regression for the cancellation-prone A - B u sphere kernel: the softened
    // distance is dominated by a = 1e-6 while (s - R) = 1e-9, so forming
    // A - B u near u = 1 destroys the small-distance term. The committed
    // reference comes from the independent two-precision shell-volume generator.
    let row = reference::fixtures()
        .into_iter()
        .find(|row| row.id == "sphere_near_surface_tiny")
        .expect("near-surface tiny-softening sphere fixture");
    assert_eq!(row.softening, 1e-6);
    assert_eq!(row.dimension, 1.0);
    assert_eq!(row.query.x, 1.0 + 1e-9);

    let mut runner = evaluator_with(&[source_for(&row)], options_for(&row));
    let estimate = estimate(runner.evaluate(row.query, &|| false).electric);
    // The A - B u form published a value ~6.6e-11 from the independent
    // reference here (larger than its own reported uncertainty); the stable
    // form agrees to roughly machine precision. Keep this pin tighter than the
    // generic fixture contract so the regression actually catches the old form.
    let tolerance = 1e-12 + 1e-12 * row.reference.length();
    let error = (estimate.value - row.reference).length();
    assert!(
        error <= tolerance,
        "{}: got {:?}, reference {:?}, error {error:e} > {tolerance:e}",
        row.id,
        estimate.value,
        row.reference
    );
    assert!(
        estimate.uncertainty <= estimate.target,
        "{}: uncertainty {:e} exceeds target {:e}",
        row.id,
        estimate.uncertainty,
        estimate.target
    );
}

#[test]
fn disc_exact_axis_symmetry_is_not_underflow() {
    // The disc axial branch at z = 0 returns exact zero by symmetry, not by
    // rounded cancellation; that must not raise the underflow flag.
    let mut runner = evaluator(&[disc(1.0, 1.0)]);
    let estimate = estimate(runner.evaluate(DVec3::ZERO, &|| false).electric);
    assert_eq!(estimate.value, DVec3::ZERO);
    assert!(
        !estimate.diagnostics.underflow,
        "exact central symmetry was flagged as underflow"
    );
}

#[test]
fn off_axis_fixtures_meet_requested_target() {
    let rows: Vec<reference::Fixture> = reference::fixtures()
        .into_iter()
        .filter(|row| {
            matches!(
                row.kind.as_str(),
                "point" | "line" | "ring" | "disc" | "sphere"
            )
        })
        .collect();
    assert_eq!(rows.len(), 30, "electric fixture matrix changed");

    let mut kinds = std::collections::BTreeSet::new();
    for row in &rows {
        kinds.insert(row.kind.as_str());
        let source = source_for(row);
        let mut runner = evaluator_with(&[source], options_for(row));
        let result = runner.evaluate(row.query, &|| false).electric;
        let estimate = estimate(result);
        let tolerance = 1e-9 + 1e-7 * row.reference.length();
        let error = (estimate.value - row.reference).length();
        assert!(
            error <= tolerance,
            "{} ({}): got {:?}, reference {:?}, error {error:e} > {tolerance:e}; \
             U={:e} work={} cells={}",
            row.id,
            row.method,
            estimate.value,
            row.reference,
            estimate.uncertainty,
            estimate.diagnostics.work_units,
            estimate.diagnostics.cells_high_water
        );
        assert!(
            estimate.uncertainty.is_finite() && estimate.uncertainty <= estimate.target,
            "{}: uncertainty {:e} exceeds target {:e}",
            row.id,
            estimate.uncertainty,
            estimate.target
        );
        assert!(estimate.diagnostics.work_units >= 1);
        assert!(row.uncertainty <= 1e-12 + 1e-10 * row.reference.length());
    }
    assert_eq!(
        kinds,
        std::collections::BTreeSet::from(["point", "line", "ring", "disc", "sphere"])
    );

    // Every geometry has an independently resolved support/interior case at
    // ordinary softening; none of them may be a blanket exclusion.
    for id in [
        "ring_near_support",
        "disc_near_support",
        "sphere_near_surface",
        "sphere_interior",
    ] {
        let row = rows.iter().find(|row| row.id == id).expect("fixture row");
        let mut runner = evaluator_with(&[source_for(row)], options_for(row));
        let estimate = estimate(runner.evaluate(row.query, &|| false).electric);
        assert!(
            estimate.value.is_finite(),
            "{id} published a non-finite vector"
        );
    }
}

#[test]
fn tiny_budget_distinguishes_complete_from_unavailable() {
    let row = reference::fixtures()
        .into_iter()
        .find(|row| row.id == "disc_offaxis_z0")
        .expect("disc fixture");
    let source = source_for(&row);

    // An extremely tight target still has a complete resolved partition: the
    // answer is finite AccuracyNotMet, not an unavailable partial integral.
    let tight = EvalOptions {
        softening: row.softening,
        accuracy: AccuracyRequest {
            epsilon: 1e-30,
            ..AccuracyRequest::default()
        },
        ..EvalOptions::default()
    };
    let mut runner = evaluator_with(&[source], tight);
    match runner.evaluate(row.query, &|| false).electric {
        FamilyResult::AccuracyNotMet(estimate, _) => {
            assert!(estimate.value.is_finite());
            assert!(estimate.uncertainty > estimate.target);
            assert!(estimate.uncertainty.is_finite());
        }
        other => panic!("expected finite AccuracyNotMet, got {other:?}"),
    }

    // Too little work even for complete initial coverage: no vector at all.
    let work_limited = EvalOptions {
        softening: row.softening,
        limits: WorkLimits {
            work_per_family: 40,
            ..WorkLimits::default()
        },
        ..EvalOptions::default()
    };
    let mut runner = evaluator_with(&[source], work_limited);
    let result = runner.evaluate(row.query, &|| false).electric;
    assert!(
        matches!(
            result,
            FamilyResult::Unavailable(StopReason::WorkLimit | StopReason::UnresolvedFeature)
        ),
        "expected no publishable estimate, got {result:?}"
    );

    // One resident cell cannot hold a resolved partition.
    let cell_limited = EvalOptions {
        softening: row.softening,
        limits: WorkLimits {
            cells_per_family: 1,
            ..WorkLimits::default()
        },
        ..EvalOptions::default()
    };
    let mut runner = evaluator_with(&[source], cell_limited);
    let result = runner.evaluate(row.query, &|| false).electric;
    assert!(
        matches!(result, FamilyResult::Unavailable(_)),
        "expected Unavailable with a one-cell scratch, got {result:?}"
    );

    // A narrow displaced feature at reduced depth cannot be resolved.
    let narrow_options = EvalOptions {
        softening: 1e-6,
        limits: WorkLimits {
            max_depth: 2,
            ..WorkLimits::default()
        },
        ..EvalOptions::default()
    };
    let mut runner = evaluator_with(&[disc(1.0, 2e-6)], narrow_options);
    let result = runner
        .evaluate(DVec3::new(1.0, 0.0, 0.0), &|| false)
        .electric;
    assert!(
        matches!(
            result,
            FamilyResult::Unavailable(StopReason::UnresolvedFeature | StopReason::WorkLimit)
        ),
        "expected depth refusal, got {result:?}"
    );
}

#[test]
fn ring_near_axis_and_branch_sweep() {
    // Pinned near-axis radial cancellation: the direct angular integral loses
    // this term, so only the stable series/tail path may claim it.
    let mut runner = evaluator(&[ring(1.0, 1.0)]);
    let near_axis = estimate(
        runner
            .evaluate(DVec3::new(1e-10, 0.0, 0.7), &|| false)
            .electric,
    );
    let expected_er = -2.2156608357924563e-27;
    let expected_ez = 0.38103173776627215;
    assert!(
        (near_axis.value.x - expected_er).abs() <= 1e-22,
        "ring near-axis Er: got {:e}, expected {expected_er:e}",
        near_axis.value.x
    );
    assert!(
        (near_axis.value.z - expected_ez).abs() <= 2e-14,
        "ring near-axis Ez: got {:e}, expected {expected_ez:e}",
        near_axis.value.z
    );
    assert!(near_axis.uncertainty <= near_axis.target);

    let rows: Vec<reference::Fixture> = reference::fixtures_of("ring")
        .filter(|row| row.id.starts_with("ring_delta_"))
        .collect();
    assert_eq!(rows.len(), 3, "expected the three branch-sweep rows");

    let mut evaluated = Vec::new();
    for row in &rows {
        let mut runner = evaluator_with(&[source_for(row)], options_for(row));
        let fixture_estimate = estimate(runner.evaluate(row.query, &|| false).electric);
        let tolerance = 1e-9 + 1e-7 * row.reference.length();
        assert!(
            (fixture_estimate.value - row.reference).length() <= tolerance,
            "{}: got {:?}, reference {:?}",
            row.id,
            fixture_estimate.value,
            row.reference
        );
        evaluated.push((row.id.clone(), row.reference, fixture_estimate));
    }

    // The two sides must use different production branches but agree with the
    // independent references; no jump beyond the reference change plus target.
    assert!(
        evaluated[0].2.diagnostics.work_units <= 64,
        "series side unexpectedly integrated: {} work units",
        evaluated[0].2.diagnostics.work_units
    );
    assert!(
        evaluated[2].2.diagnostics.work_units >= 93,
        "direct side did not integrate: {} work units",
        evaluated[2].2.diagnostics.work_units
    );
    for pair in evaluated.windows(2) {
        let (left_id, left_ref, left) = (&pair[0].0, pair[0].1, &pair[0].2);
        let (right_id, right_ref, right) = (&pair[1].0, pair[1].1, &pair[1].2);
        let reference_change = (right_ref - left_ref).length();
        let targets = (1e-9 + 1e-7 * left_ref.length()) + (1e-9 + 1e-7 * right_ref.length());
        let jump = (right.value - left.value).length();
        assert!(
            jump <= reference_change + targets + 1e-12,
            "branch jump between {left_id} and {right_id}: {jump:e} > \
             reference change {reference_change:e} + targets {targets:e}"
        );
    }
}

#[test]
fn ring_monopole_far_field() {
    let mut runner = evaluator(&[ring(1.0, 1.0)]);
    let mut discrepancies = Vec::new();
    for distance in [100.0_f64, 200.0] {
        let estimate = estimate(
            runner
                .evaluate(DVec3::new(0.0, 0.0, distance), &|| false)
                .electric,
        );
        let ideal = 1.0 / (distance * distance);
        assert!(estimate.value.x == 0.0 && estimate.value.y == 0.0);
        assert!(
            estimate.value.z > 0.0,
            "expected outward axial far field at {distance}"
        );
        discrepancies.push((estimate.value.z - ideal).abs());
    }
    assert!(
        discrepancies[1] < discrepancies[0],
        "ideal monopole discrepancy did not decrease: {discrepancies:?}"
    );
}
