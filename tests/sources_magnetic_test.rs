//! Task 4 current-loop and finite continuous-solenoid reductions, independent
//! magnetic fixtures, far-field dipole and budget/status coverage.

use std::f64::consts::PI;

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

fn current_loop(radius: f64, current: f64) -> Source {
    Source {
        center: DVec3::ZERO,
        axis: DVec3::Z,
        kind: SourceKind::CurrentLoop { current, radius },
    }
}

fn solenoid(radius: f64, length: f64, turns: u32, current: f64) -> Source {
    Source {
        center: DVec3::ZERO,
        axis: DVec3::Z,
        kind: SourceKind::Solenoid {
            current,
            radius,
            length,
            turns,
        },
    }
}

fn source_for(row: &reference::Fixture) -> Source {
    let kind = match row.kind.as_str() {
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
        "wire" => SourceKind::StraightWire {
            current: row.charge,
        },
        other => panic!("unknown magnetic fixture kind {other}"),
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

fn magnetic_fixture_rows() -> Vec<reference::Fixture> {
    reference::fixtures()
        .into_iter()
        .filter(|row| matches!(row.kind.as_str(), "loop" | "solenoid" | "wire"))
        .collect()
}

#[test]
fn analytic_loop_and_solenoid_axial_fields() {
    // Current loop on axis: Bz = 2 pi I R^2 / (R^2 + z^2 + a^2)^(3/2).
    let mut runner = evaluator(&[current_loop(1.0, 1.0)]);
    let loop_axis = estimate(
        runner
            .evaluate(DVec3::new(0.0, 0.0, 0.7), &|| false)
            .magnetic,
    );
    let expected_loop = 2.0 * PI / 1.5f64.powf(1.5);
    assert!(
        (loop_axis.value.z - expected_loop).abs() <= 2e-14,
        "loop axis {loop_axis:?}, expected {expected_loop:e}"
    );
    assert_eq!(loop_axis.value.x, 0.0);
    assert_eq!(loop_axis.value.y, 0.0);

    // Finite continuous solenoid on axis:
    // Bz = 2 pi N I R^2 / ((R^2+a^2) sqrt(R^2+a^2+L^2/4)) at z = 0.
    let mut runner = evaluator(&[solenoid(1.0, 2.0, 1, 1.0)]);
    let solenoid_axis = estimate(runner.evaluate(DVec3::ZERO, &|| false).magnetic);
    let expected_solenoid = 2.0 * PI / (1.01 * 2.01f64.sqrt());
    assert!(
        (solenoid_axis.value.z - expected_solenoid).abs() <= 2e-14,
        "solenoid axis {solenoid_axis:?}, expected {expected_solenoid:e}"
    );
    assert_eq!(solenoid_axis.value.x, 0.0);
    assert_eq!(solenoid_axis.value.y, 0.0);

    // Three turns triple the field once; they are not counted twice.
    let mut runner = evaluator(&[solenoid(1.0, 2.0, 3, 1.0)]);
    let three_turns = estimate(runner.evaluate(DVec3::ZERO, &|| false).magnetic);
    assert!(
        (three_turns.value.z - 3.0 * expected_solenoid).abs() <= 3e-13,
        "N=3 {three_turns:?}, expected {:?}",
        3.0 * expected_solenoid
    );

    // The full analytic axial expression across short, ordinary and long
    // lengths at z = 0.
    let softening = SOFTENING;
    let c = 1.0 + softening * softening;
    for length in [0.02_f64, 2.0, 20.0] {
        let expected = 2.0 * PI / (c * (c + 0.25 * length * length).sqrt());
        let mut runner = evaluator_with(
            &[solenoid(1.0, length, 1, 1.0)],
            EvalOptions {
                softening,
                ..EvalOptions::default()
            },
        );
        let actual = estimate(runner.evaluate(DVec3::ZERO, &|| false).magnetic);
        assert!(
            (actual.value.z - expected).abs() <= 2e-13 * expected.max(1.0),
            "L={length}: got {:e}, expected {expected:e}",
            actual.value.z
        );
    }

    // Negative current and reversed oriented axis both reverse B, for an
    // off-axis query where both local components are nonzero.
    let query = DVec3::new(0.4, 0.0, 0.3);
    let base = estimate(
        evaluator(&[current_loop(1.0, 1.0)])
            .evaluate(query, &|| false)
            .magnetic,
    );
    let negated = estimate(
        evaluator(&[current_loop(1.0, -1.0)])
            .evaluate(query, &|| false)
            .magnetic,
    );
    assert_close(negated.value, -base.value, 2e-13, "loop negated current");
    let reversed_axis = estimate(
        evaluator(&[Source {
            center: DVec3::ZERO,
            axis: -DVec3::Z,
            kind: SourceKind::CurrentLoop {
                current: 1.0,
                radius: 1.0,
            },
        }])
        .evaluate(query, &|| false)
        .magnetic,
    );
    assert_close(
        reversed_axis.value,
        -base.value,
        2e-13,
        "loop reversed axis",
    );

    let solenoid_query = DVec3::new(0.5, 0.0, 0.3);
    let solenoid_base = estimate(
        evaluator(&[solenoid(1.0, 2.0, 1, 1.0)])
            .evaluate(solenoid_query, &|| false)
            .magnetic,
    );
    let solenoid_negated = estimate(
        evaluator(&[solenoid(1.0, 2.0, 1, -1.0)])
            .evaluate(solenoid_query, &|| false)
            .magnetic,
    );
    assert_close(
        solenoid_negated.value,
        -solenoid_base.value,
        2e-13,
        "solenoid negated current",
    );
    let solenoid_reversed = estimate(
        evaluator(&[Source {
            center: DVec3::ZERO,
            axis: -DVec3::Z,
            kind: SourceKind::Solenoid {
                current: 1.0,
                radius: 1.0,
                length: 2.0,
                turns: 1,
            },
        }])
        .evaluate(solenoid_query, &|| false)
        .magnetic,
    );
    assert_close(
        solenoid_reversed.value,
        -solenoid_base.value,
        2e-13,
        "solenoid reversed axis",
    );

    // Translated tilted axes reproduce the rotated local vector.
    let centre = DVec3::new(0.5, -0.25, 2.0);
    let unit_axis = DVec3::new(1.0, 2.0, 3.0).normalize();
    let seed = DVec3::X - unit_axis * unit_axis.dot(DVec3::X);
    let radial = seed.normalize();
    let tangent = unit_axis.cross(radial);
    let rotation = DMat3::from_cols(radial, tangent, unit_axis);

    let local_loop_query = DVec3::new(0.4, 0.0, 0.3);
    let world_loop_query = centre + rotation * local_loop_query;
    let rotated_loop = estimate(
        evaluator(&[Source {
            center: centre,
            axis: unit_axis,
            kind: SourceKind::CurrentLoop {
                current: 1.0,
                radius: 1.0,
            },
        }])
        .evaluate(world_loop_query, &|| false)
        .magnetic,
    );
    assert_close(
        rotated_loop.value,
        rotation * base.value,
        2e-12 * base.value.length(),
        "rotated loop",
    );

    let local_solenoid_query = DVec3::new(0.5, 0.0, 0.3);
    let world_solenoid_query = centre + rotation * local_solenoid_query;
    let rotated_solenoid = estimate(
        evaluator(&[Source {
            center: centre,
            axis: unit_axis,
            kind: SourceKind::Solenoid {
                current: 1.0,
                radius: 1.0,
                length: 2.0,
                turns: 1,
            },
        }])
        .evaluate(world_solenoid_query, &|| false)
        .magnetic,
    );
    assert_close(
        rotated_solenoid.value,
        rotation * solenoid_base.value,
        2e-12 * solenoid_base.value.length(),
        "rotated solenoid",
    );

    // The infinite wire keeps its exact factor 2: B = 2 I / (r^2 + a^2) e_phi.
    let mut wire_runner = evaluator(&[Source {
        center: DVec3::ZERO,
        axis: DVec3::Z,
        kind: SourceKind::StraightWire { current: 1.0 },
    }]);
    let wire = estimate(wire_runner.evaluate(DVec3::X, &|| false).magnetic);
    assert_close(
        wire.value,
        DVec3::new(0.0, 2.0 / 1.01, 0.0),
        2e-15,
        "wire factor 2",
    );
}

#[test]
fn off_axis_fixtures_meet_requested_target() {
    let rows = magnetic_fixture_rows();
    assert_eq!(rows.len(), 24, "magnetic fixture matrix changed");

    let mut kinds = std::collections::BTreeSet::new();
    for row in &rows {
        kinds.insert(row.kind.as_str());
        let mut runner = evaluator_with(&[source_for(row)], options_for(row));
        let result = runner.evaluate(row.query, &|| false).magnetic;
        let fixture_estimate = estimate(result);
        let tolerance = 1e-9 + 1e-7 * row.reference.length();
        let error = (fixture_estimate.value - row.reference).length();
        assert!(
            error <= tolerance,
            "{} ({}): got {:?}, reference {:?}, error {error:e} > {tolerance:e}; \
             U={:e} work={} cells={}",
            row.id,
            row.method,
            fixture_estimate.value,
            row.reference,
            fixture_estimate.uncertainty,
            fixture_estimate.diagnostics.work_units,
            fixture_estimate.diagnostics.cells_high_water
        );
        assert!(
            fixture_estimate.uncertainty.is_finite()
                && fixture_estimate.uncertainty <= fixture_estimate.target,
            "{}: uncertainty {:e} exceeds target {:e}",
            row.id,
            fixture_estimate.uncertainty,
            fixture_estimate.target
        );
        assert!(fixture_estimate.diagnostics.work_units >= 1);
        assert!(row.uncertainty <= 1e-12 + 1e-10 * row.reference.length());
    }
    assert_eq!(
        kinds,
        std::collections::BTreeSet::from(["loop", "solenoid", "wire"])
    );

    // Pinned near-axis loop radial cancellation: the committed reference is
    // 2.3940930163021465e-10, which the direct angular integral loses.
    let near_axis = rows
        .iter()
        .find(|row| row.id == "loop_near_axis_0p7")
        .expect("near-axis loop fixture");
    let mut runner = evaluator_with(&[source_for(near_axis)], options_for(near_axis));
    let near_axis_estimate = estimate(runner.evaluate(near_axis.query, &|| false).magnetic);
    let expected_br = 2.3940930163021465e-10;
    let expected_bz = 3.420132880431638;
    assert!(
        (near_axis_estimate.value.x - expected_br).abs() <= 1e-20,
        "loop near-axis Br: got {:e}, expected {expected_br:e}",
        near_axis_estimate.value.x
    );
    assert!(
        (near_axis_estimate.value.z - expected_bz).abs() <= 2e-14,
        "loop near-axis Bz: got {:e}, expected {expected_bz:e}",
        near_axis_estimate.value.z
    );
    assert!(near_axis_estimate.uncertainty <= near_axis_estimate.target);

    // The same near-axis query on the negative world-x half-plane exercises
    // the azimuthal sign reversal. The reported uncertainty must remain an
    // upper bound on the error against the independent reference, not just
    // below the target.
    let negative_axis = rows
        .iter()
        .find(|row| row.id == "loop_near_axis_neg_x")
        .expect("negative near-axis loop fixture");
    let mut runner = evaluator_with(&[source_for(negative_axis)], options_for(negative_axis));
    let negative_estimate = estimate(runner.evaluate(negative_axis.query, &|| false).magnetic);
    let expected_bx = -2.3940930163021465e-10;
    assert!(
        (negative_estimate.value.x - expected_bx).abs() <= 1e-20,
        "loop negative-radial Bx: got {:e}, expected {expected_bx:e}",
        negative_estimate.value.x
    );
    assert!(
        (negative_estimate.value.z - expected_bz).abs() <= 2e-14,
        "loop negative-radial Bz: got {:e}, expected {expected_bz:e}",
        negative_estimate.value.z
    );
    assert!(negative_estimate.uncertainty <= negative_estimate.target);
    let negative_error = (negative_estimate.value - negative_axis.reference).length();
    assert!(
        negative_error <= negative_estimate.uncertainty,
        "loop negative-radial uncertainty {:e} under-bounds error {negative_error:e}",
        negative_estimate.uncertainty
    );

    // An ordinary off-axis loop row at z = 0.2 keeps the radial term away from
    // the nearly vanishing near-axis leading contribution.
    let ordinary = rows
        .iter()
        .find(|row| row.id == "loop_offaxis_z0p2")
        .expect("ordinary off-axis loop fixture");
    let mut runner = evaluator_with(&[source_for(ordinary)], options_for(ordinary));
    let ordinary_estimate = estimate(runner.evaluate(ordinary.query, &|| false).magnetic);
    assert!(
        ordinary_estimate.value.x.abs() > 0.1,
        "ordinary loop radial term vanished: {:?}",
        ordinary_estimate.value
    );

    // Complete finite results at ordinary softening for on-winding, interior
    // and end queries; none may be a blanket exclusion.
    for id in [
        "loop_near_support",
        "loop_on_winding",
        "solenoid_interior",
        "solenoid_on_winding",
        "solenoid_axis_end",
        "solenoid_end_effect",
    ] {
        let row = rows.iter().find(|row| row.id == id).expect("fixture row");
        let mut runner = evaluator_with(&[source_for(row)], options_for(row));
        let query_estimate = estimate(runner.evaluate(row.query, &|| false).magnetic);
        assert!(
            query_estimate.value.is_finite(),
            "{id} published a non-finite vector"
        );
    }
}

#[test]
fn loop_near_axis_and_branch_sweep() {
    let rows: Vec<reference::Fixture> = reference::fixtures_of("loop")
        .filter(|row| row.id.starts_with("loop_delta_"))
        .collect();
    assert_eq!(rows.len(), 3, "expected the three loop branch-sweep rows");

    let mut evaluated = Vec::new();
    for row in &rows {
        let mut runner = evaluator_with(&[source_for(row)], options_for(row));
        let fixture_estimate = estimate(runner.evaluate(row.query, &|| false).magnetic);
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

    // The two sides use different production branches: the series side costs
    // at most 64 terms, while the direct side evaluates the angular rule.
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
fn loop_magnetic_dipole_far_field() {
    // Magnetic dipole moment m = pi I R^2 axis. In the same source units
    // (omitting mu0/4pi) B = (3(m.rhat)rhat - m)/r^3. At the axial point that
    // is 2m/r^3; at the equatorial point it is -m/r^3. A charged-ring monopole
    // would instead be radial with 1/r^2 decay.
    let moment = PI;
    let mut runner = evaluator(&[current_loop(1.0, 1.0)]);
    let mut axial_discrepancy = Vec::new();
    let mut equatorial_discrepancy = Vec::new();
    for distance in [100.0_f64, 200.0] {
        let axial = estimate(
            runner
                .evaluate(DVec3::new(0.0, 0.0, distance), &|| false)
                .magnetic,
        );
        assert_eq!(axial.value.x, 0.0);
        assert_eq!(axial.value.y, 0.0);
        assert!(axial.value.z > 0.0, "axial far field must point outward");
        let ideal_axial = 2.0 * moment / distance.powi(3);
        axial_discrepancy.push((axial.value.z - ideal_axial).abs());

        let equatorial = estimate(
            runner
                .evaluate(DVec3::new(distance, 0.0, 0.0), &|| false)
                .magnetic,
        );
        assert_eq!(equatorial.value.x, 0.0);
        assert_eq!(equatorial.value.y, 0.0);
        assert!(
            equatorial.value.z < 0.0,
            "equatorial dipole field must point opposite the moment"
        );
        let ideal_equatorial = -moment / distance.powi(3);
        equatorial_discrepancy.push((equatorial.value.z - ideal_equatorial).abs());

        // Direction and normalization: B*r^3 approaches the dipole moment
        // constant, not a 1/r^2 monopole scale.
        let axial_scaled = axial.value.z * distance.powi(3);
        let equatorial_scaled = equatorial.value.z * distance.powi(3);
        assert!(
            (axial_scaled - 2.0 * moment).abs() <= 5e-3,
            "axial B*r^3 {axial_scaled:e} not near {:e}",
            2.0 * moment
        );
        assert!(
            (equatorial_scaled + moment).abs() <= 5e-3,
            "equatorial B*r^3 {equatorial_scaled:e} not near {:e}",
            -moment
        );
    }
    assert!(
        axial_discrepancy[1] < axial_discrepancy[0],
        "axial dipole discrepancy did not decrease: {axial_discrepancy:?}"
    );
    assert!(
        equatorial_discrepancy[1] < equatorial_discrepancy[0],
        "equatorial dipole discrepancy did not decrease: {equatorial_discrepancy:?}"
    );
}

#[test]
fn finite_current_scales_inverse_length() {
    // Scaling every length and the softening by s (fixed I and N) scales B by
    // 1/s, not 1/s^2 as a fixed electric charge would.
    let scale = 2.0;
    let scaled_options = EvalOptions {
        softening: scale * SOFTENING,
        ..EvalOptions::default()
    };
    let query = DVec3::new(0.4, 0.0, 0.7);
    let loop_base = estimate(
        evaluator(&[current_loop(1.0, 1.0)])
            .evaluate(query, &|| false)
            .magnetic,
    );
    let loop_scaled = estimate(
        evaluator_with(&[current_loop(scale, 1.0)], scaled_options)
            .evaluate(query * scale, &|| false)
            .magnetic,
    );
    assert_close(
        loop_scaled.value,
        loop_base.value / scale,
        2e-13 * loop_base.value.length(),
        "scaled loop",
    );

    let solenoid_query = DVec3::new(0.5, 0.0, 0.3);
    let solenoid_base = estimate(
        evaluator(&[solenoid(1.0, 2.0, 1, 1.0)])
            .evaluate(solenoid_query, &|| false)
            .magnetic,
    );
    let solenoid_scaled = estimate(
        evaluator_with(&[solenoid(scale, 2.0 * scale, 1, 1.0)], scaled_options)
            .evaluate(solenoid_query * scale, &|| false)
            .magnetic,
    );
    assert_close(
        solenoid_scaled.value,
        solenoid_base.value / scale,
        2e-13 * solenoid_base.value.length(),
        "scaled solenoid",
    );

    // Contrast: a fixed electric charge scales as Q/s^2.
    let ring = Source {
        center: DVec3::ZERO,
        axis: DVec3::Z,
        kind: SourceKind::ChargedRing {
            charge: 1.0,
            radius: 1.0,
        },
    };
    let ring_scaled = Source {
        center: DVec3::ZERO,
        axis: DVec3::Z,
        kind: SourceKind::ChargedRing {
            charge: 1.0,
            radius: scale,
        },
    };
    let electric_base = estimate(evaluator(&[ring]).evaluate(query, &|| false).electric);
    let electric_scaled = estimate(
        evaluator_with(&[ring_scaled], scaled_options)
            .evaluate(query * scale, &|| false)
            .electric,
    );
    assert_close(
        electric_scaled.value,
        electric_base.value / (scale * scale),
        2e-13 * electric_base.value.length(),
        "scaled charged ring",
    );
}

#[test]
fn tiny_budget_distinguishes_complete_from_unavailable() {
    let row = reference::fixtures()
        .into_iter()
        .find(|row| row.id == "solenoid_interior")
        .expect("solenoid interior fixture");
    let source = source_for(&row);

    // A very tight target still has a complete resolved partition: the answer
    // is finite AccuracyNotMet, never an unavailable partial integral.
    let tight = EvalOptions {
        softening: row.softening,
        accuracy: AccuracyRequest {
            epsilon: 1e-30,
            ..AccuracyRequest::default()
        },
        ..EvalOptions::default()
    };
    let mut runner = evaluator_with(&[source], tight);
    match runner.evaluate(row.query, &|| false).magnetic {
        FamilyResult::AccuracyNotMet(estimate, reason) => {
            assert!(estimate.value.is_finite());
            assert!(estimate.uncertainty > estimate.target);
            assert!(estimate.uncertainty.is_finite());
            // The complete resolved partition reaches the roundoff floor
            // rather than an incomplete integral or a work/storage refusal.
            assert_eq!(
                reason,
                StopReason::RoundoffLimited,
                "unexpected tight-target stop reason {reason:?}"
            );
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
    let result = runner.evaluate(row.query, &|| false).magnetic;
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
    let result = runner.evaluate(row.query, &|| false).magnetic;
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
    let mut runner = evaluator_with(&[solenoid(1.0, 2.0, 1, 1.0)], narrow_options);
    let result = runner
        .evaluate(DVec3::new(1.0, 0.0, 0.0), &|| false)
        .magnetic;
    assert!(
        matches!(
            result,
            FamilyResult::Unavailable(StopReason::UnresolvedFeature | StopReason::WorkLimit)
        ),
        "expected depth refusal, got {result:?}"
    );
}
