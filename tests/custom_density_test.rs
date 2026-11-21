//! Issue #96 slice A: the bounded custom density model and its diagnostics.
//!
//! These tests exercise the pure model only: no UI, persistence, scene document
//! or field solver. Density outside a support is identically zero by contract;
//! every report below is about the interior that was actually sampled.

use std::f64::consts::PI;

use glam::Vec3;
use vector_field_explorer::custom_density::{
    DensityFamily, DensitySpec, DensitySupport, DiagnosticsStatus, MAX_DENSITY_SAMPLES,
    MAX_SAMPLES, MIN_SAMPLES, SupportKind, charge_report, continuity_report, count_sign_changes,
};
use vector_field_explorer::parameters::{Parameter, ParameterSet};

fn params() -> ParameterSet {
    ParameterSet::default()
}

fn parameter(name: &str, value: f64) -> ParameterSet {
    ParameterSet {
        version: 1,
        entries: vec![Parameter {
            name: name.to_string(),
            value,
            min: None,
            max: None,
            step: 1.0,
        }],
    }
}

fn box_support() -> DensitySupport {
    DensitySupport {
        kind: SupportKind::Box,
        center: Vec3::ZERO,
        axis: Vec3::Z,
        size: Vec3::splat(2.0),
    }
}

fn sphere_support() -> DensitySupport {
    DensitySupport {
        kind: SupportKind::Sphere,
        center: Vec3::ZERO,
        axis: Vec3::Z,
        size: Vec3::splat(2.0),
    }
}

fn spec(
    family: DensityFamily,
    support: DensitySupport,
    formula: &str,
    samples: u32,
) -> DensitySpec {
    DensitySpec {
        name: "test".to_string(),
        family,
        support,
        formula: formula.to_string(),
        samples,
        softening: 0.0,
    }
}

#[test]
fn validation_rejects_bad_names_and_geometry() {
    let good = spec(DensityFamily::VolumeCharge, box_support(), "1", 8);

    for (name, expected) in [
        ("", "name"),
        (" padded", "trim"),
        ("padded ", "trim"),
        ("bad\nname", "control"),
    ] {
        let mut bad = good.clone();
        bad.name = name.to_string();
        let error = bad.validate(&params()).unwrap_err();
        assert!(
            error.to_lowercase().contains(expected),
            "name `{name}` error should mention `{expected}`: {error}"
        );
    }

    let mut too_long = good.clone();
    too_long.name = "n".repeat(49);
    let error = too_long.validate(&params()).unwrap_err();
    assert!(
        error.contains("48"),
        "the over-long name error names the byte limit: {error}"
    );

    let mut bad_sphere = good.clone();
    bad_sphere.support = DensitySupport {
        kind: SupportKind::Sphere,
        center: Vec3::ZERO,
        axis: Vec3::Z,
        size: Vec3::new(0.0, 2.0, 2.0),
    };
    let error = bad_sphere.validate(&params()).unwrap_err();
    assert!(
        error.to_lowercase().contains("size"),
        "a zero extent names the size rule: {error}"
    );

    let mut bad_axis = good.clone();
    bad_axis.support = DensitySupport {
        kind: SupportKind::Cylinder,
        center: Vec3::ZERO,
        axis: Vec3::ZERO,
        size: Vec3::splat(2.0),
    };
    let error = bad_axis.validate(&params()).unwrap_err();
    assert!(
        error.to_lowercase().contains("axis"),
        "a degenerate cylinder axis names the axis rule: {error}"
    );

    let mut bad_center = good.clone();
    bad_center.support.center = Vec3::new(f32::INFINITY, 0.0, 0.0);
    let error = bad_center.validate(&params()).unwrap_err();
    assert!(
        error.to_lowercase().contains("finite"),
        "a non-finite centre names the finite rule: {error}"
    );

    let mut bad_softening = good.clone();
    bad_softening.softening = 1.5;
    let error = bad_softening.validate(&params()).unwrap_err();
    assert!(
        error.to_lowercase().contains("softening"),
        "softening outside 0..=1 is rejected: {error}"
    );

    bad_softening.softening = f64::NAN;
    assert!(bad_softening.validate(&params()).is_err());

    // The report's `worst_point` is an f32 `Vec3`, so a centre plus half-extent
    // that overflows f32 would round a finite sample position to infinity.
    let mut unrepresentable = good.clone();
    unrepresentable.support.center = Vec3::new(3.0e38, 0.0, 0.0);
    unrepresentable.support.size = Vec3::new(2.0e38, 2.0, 2.0);
    let error = unrepresentable.validate(&params()).unwrap_err();
    assert!(
        error.to_lowercase().contains("f32"),
        "a centre plus half-extent beyond f32 is rejected: {error}"
    );

    // The valid records used by the other tests must pass.
    good.validate(&params()).unwrap();
    assert!(
        (sphere_support().volume() - 4.0 * PI / 3.0).abs() < 1e-9,
        "a diameter-2 sphere has unit radius"
    );
}

#[test]
fn validation_rejects_a_formula_that_is_non_finite_inside_the_support() {
    for formula in ["sqrt(-1)", "ln(-1)", "1/(x-x)"] {
        let bad = spec(DensityFamily::VolumeCharge, box_support(), formula, 8);
        let error = bad.validate(&params()).unwrap_err();
        assert!(
            error.to_lowercase().contains("finite"),
            "`{formula}` must be rejected as non-finite: {error}"
        );
    }

    let bad_current = spec(DensityFamily::SteadyCurrent, box_support(), "ln(-1),0,0", 8);
    assert!(
        bad_current
            .validate(&params())
            .unwrap_err()
            .to_lowercase()
            .contains("finite")
    );
}

#[test]
fn validation_rejects_sampling_over_the_budget() {
    let mut over = spec(DensityFamily::VolumeCharge, box_support(), "1", 49);
    let error = over.validate(&params()).unwrap_err();
    assert!(
        error.contains(&MAX_DENSITY_SAMPLES.to_string()),
        "the over-budget error names MAX_DENSITY_SAMPLES: {error}"
    );

    over.samples = MIN_SAMPLES - 1;
    let error = over.validate(&params()).unwrap_err();
    assert!(
        error.contains(&MIN_SAMPLES.to_string()),
        "the under-sampled error names MIN_SAMPLES: {error}"
    );

    // The largest accepted lattice is exactly the budget.
    let boundary = spec(DensityFamily::VolumeCharge, box_support(), "1", MAX_SAMPLES);
    boundary.validate(&params()).unwrap();
    assert_eq!(
        u64::from(MAX_SAMPLES).pow(3),
        u64::from(MAX_DENSITY_SAMPLES)
    );
}

#[test]
fn uniform_sphere_charge_total_matches_the_analytic_value() {
    let uniform = spec(
        DensityFamily::VolumeCharge,
        sphere_support(),
        "3",
        MAX_SAMPLES,
    );
    uniform.validate(&params()).unwrap();

    let report = charge_report(&uniform, &params(), 0.0).unwrap();
    let expected = 3.0 * 4.0 * PI / 3.0;
    let relative = (report.total - expected).abs() / expected;
    assert!(
        relative < 0.02,
        "midpoint total {} should match the analytic {expected} (relative {relative})",
        report.total
    );
    assert!(matches!(
        report.status,
        DiagnosticsStatus::Estimate { error } if error.is_finite()
    ));
    // The count is the accepted interior set, not the full bounding lattice.
    let full = u64::from(MAX_SAMPLES).pow(3);
    assert!(u64::from(report.samples) > full / 3);
    assert!(u64::from(report.samples) < full);
}

#[test]
fn sign_changes_are_counted_over_interior_lattice_pairs() {
    let linear = spec(DensityFamily::VolumeCharge, box_support(), "x", 10);
    linear.validate(&params()).unwrap();
    let report = charge_report(&linear, &params(), 0.0).unwrap();

    assert_eq!(report.samples, 10 * 10 * 10, "the box keeps every sample");
    // Exactly one x-adjacent pair straddles zero for each (y, z) line.
    assert_eq!(
        report.sign_changes,
        10 * 10,
        "one sign change per interior x line, zeros excluded: {report:?}"
    );
    assert!(report.min < 0.0 && report.max > 0.0);

    let never = spec(DensityFamily::VolumeCharge, box_support(), "-1", 10);
    never.validate(&params()).unwrap();
    assert_eq!(
        charge_report(&never, &params(), 0.0).unwrap().sign_changes,
        0
    );
}

#[test]
fn sign_change_counting_excludes_zero_samples() {
    // A zero between a positive and a negative sample changes sign at neither
    // adjacency: removing the zero guard would count both.
    let straddling_zero = [(0, 0, 0, 1.0), (0, 0, 1, 0.0), (0, 0, 2, -1.0)];
    assert_eq!(
        count_sign_changes(3, &straddling_zero),
        0,
        "a zero sample is neither positive nor negative"
    );

    // A direct positive-to-negative adjacency still counts exactly once.
    let direct = [(0, 0, 0, 1.0), (0, 0, 1, -1.0)];
    assert_eq!(count_sign_changes(2, &direct), 1);
}

#[test]
fn uniform_steady_current_has_zero_divergence() {
    let current = spec(DensityFamily::SteadyCurrent, box_support(), "0,0,1", 12);
    current.validate(&params()).unwrap();

    let report = continuity_report(&current, &params(), 0.0).unwrap();
    assert!(matches!(
        report.status,
        DiagnosticsStatus::Estimate { error } if error.is_finite()
    ));
    assert!(report.div_j_min.abs() < 1e-12, "{:?}", report);
    assert!(report.div_j_max.abs() < 1e-12, "{:?}", report);
    assert!(report.worst_point.is_some());
}

#[test]
fn divergent_steady_current_is_reported_as_a_violation() {
    let current = spec(DensityFamily::SteadyCurrent, box_support(), "x,0,0", 12);
    current.validate(&params()).unwrap();

    let report = continuity_report(&current, &params(), 0.0).unwrap();
    assert!(matches!(report.status, DiagnosticsStatus::Violation { .. }));
    assert!(report.div_j_max >= 0.99, "{:?}", report);
    assert!(report.worst_point.is_some());
    assert!(report.div_j_min.is_finite() && report.div_j_max.is_finite());
}

#[test]
fn unsupported_or_unevaluable_interiors_are_unresolved_not_zero() {
    // A steady current has no volume-charge total; the report must not claim 0.
    let current = spec(DensityFamily::SteadyCurrent, box_support(), "0,0,1", 8);
    let report = charge_report(&current, &params(), 0.0).unwrap();
    assert!(matches!(
        report.status,
        DiagnosticsStatus::Unresolved { .. }
    ));
    assert!(report.total.is_nan(), "no fabricated zero: {:?}", report);

    // A formula that is finite at validation time but not at report time.
    let transient = spec(DensityFamily::VolumeCharge, box_support(), "1/(t-1)", 8);
    transient.validate(&params()).unwrap();
    let report = charge_report(&transient, &params(), 1.0).unwrap();
    assert!(matches!(
        report.status,
        DiagnosticsStatus::Unresolved { .. }
    ));
    assert!(report.total.is_nan(), "no fabricated zero: {:?}", report);

    // Continuity is defined only for steady currents.
    let volume = spec(DensityFamily::VolumeCharge, box_support(), "1", 8);
    let error = continuity_report(&volume, &params(), 0.0).unwrap_err();
    assert!(
        error.contains("steady currents only"),
        "the family error is explicit: {error}"
    );

    // A current that becomes unevaluable cannot be silently treated as zero.
    let transient_current = spec(
        DensityFamily::SteadyCurrent,
        box_support(),
        "1/(t-1),0,0",
        8,
    );
    transient_current.validate(&params()).unwrap();
    let report = continuity_report(&transient_current, &params(), 1.0).unwrap();
    assert!(matches!(
        report.status,
        DiagnosticsStatus::Unresolved { .. }
    ));
    assert!(report.div_j_min.is_nan() && report.div_j_max.is_nan());
}

#[test]
fn parameter_changes_are_observed_by_the_next_report() {
    let gain = spec(DensityFamily::VolumeCharge, box_support(), "k", 12);
    let one = charge_report(&gain, &parameter("k", 1.0), 0.0).unwrap();
    let two = charge_report(&gain, &parameter("k", 2.0), 0.0).unwrap();
    assert!(one.total > 0.0);
    assert!(
        (two.total - 2.0 * one.total).abs() <= 1e-12 * two.total.abs().max(1.0),
        "the same lattice re-evaluates with the new parameter: {} vs {}",
        one.total,
        two.total
    );
}

#[test]
fn sampling_is_deterministic_and_bounded() {
    let spec = spec(DensityFamily::VolumeCharge, sphere_support(), "x+y+z", 20);
    spec.validate(&params()).unwrap();

    let first = charge_report(&spec, &params(), 0.0).unwrap();
    let second = charge_report(&spec, &params(), 0.0).unwrap();
    assert_eq!(
        first, second,
        "the jittered lattice and ordering are stable"
    );
    assert!(first.samples > 0);
    assert!(
        u64::from(first.samples) <= u64::from(MAX_DENSITY_SAMPLES),
        "the interior sample count never exceeds the lattice budget"
    );
}
