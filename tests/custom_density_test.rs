//! Issue #96 slice A: the bounded custom density model and its diagnostics.
//!
//! These tests exercise the pure model only: no UI, persistence, scene document
//! or field solver. Density outside a support is identically zero by contract;
//! every report below is about the interior that was actually sampled.

use std::f64::consts::PI;

use glam::Vec3;
use vector_field_explorer::custom_density::{
    ContinuityStatus, DensityFamily, DensitySpec, DensitySupport, DiagnosticsStatus,
    MAX_ANGULAR_FREQUENCY, MAX_DENSITY_SAMPLES, MAX_SAMPLES, MIN_SAMPLES, SupportKind, TimeLaw,
    charge_report, continuity_report, count_sign_changes, validate_time_law_combination,
};
use vector_field_explorer::parameters::{Parameter, ParameterSet};
use vector_field_explorer::persistence::{DensitySpecRecord, TimeLawRecord, ViewRecord};
use vector_field_explorer::ui::sidebar::Controls;
use vector_field_explorer::viewmodel::State;

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
        time_law: TimeLaw::Steady,
        charge_formula: None,
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
        ContinuityStatus::Estimate { error, .. } if error.is_finite()
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
    assert!(matches!(report.status, ContinuityStatus::Violation { .. }));
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
    assert!(matches!(report.status, ContinuityStatus::Unresolved { .. }));
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

// --- Issue #98 slice A: supported time-dependent distributions -------------

fn harmonic(omega: f64, phase: f64) -> TimeLaw {
    TimeLaw::Harmonic { omega, phase }
}

fn harmonic_spec(
    family: DensityFamily,
    support: DensitySupport,
    formula: &str,
    samples: u32,
    law: TimeLaw,
) -> DensitySpec {
    DensitySpec {
        time_law: law,
        ..spec(family, support, formula, samples)
    }
}

#[test]
fn steady_time_law_keeps_existing_documents_byte_identical() {
    let steady = spec(DensityFamily::VolumeCharge, box_support(), "1", 8);
    assert_eq!(steady.time_law, TimeLaw::Steady);

    let record = DensitySpecRecord::from_spec(&steady);
    assert_eq!(
        record.time_law, None,
        "a steady law writes no law key at all, so older documents stay byte-identical"
    );

    let mut state = State::new(true);
    state
        .commit_densities(vec![steady])
        .expect("the steady fixture is valid");
    let document = serde_json::to_value(ViewRecord::capture(&state)).expect("a record serializes");
    let as_text = document.to_string();
    assert!(
        !as_text.contains("time_law"),
        "the existing document shape must not gain a field: {as_text}"
    );

    // An older document with no law key restores the default without changing
    // anything else.
    let mut legacy: ViewRecord = serde_json::from_value(document).expect("the legacy shape parses");
    legacy.densities[0].time_law = None;
    let mut restored = State::new(true);
    legacy.restore(&mut restored, &mut Controls::new());
    assert_eq!(restored.densities()[0].time_law, TimeLaw::Steady);
}

#[test]
fn harmonic_time_law_round_trips_through_the_envelope() {
    let law = harmonic(2.5, -0.25);
    let density = harmonic_spec(DensityFamily::VolumeCharge, box_support(), "1 + x", 10, law);
    let mut source = State::new(true);
    source
        .commit_densities(vec![density.clone()])
        .expect("the harmonic fixture is valid");

    let document = ViewRecord::capture(&source);
    assert_eq!(
        document.densities[0].time_law,
        Some(TimeLawRecord {
            kind: "harmonic".to_string(),
            omega: 2.5,
            phase: -0.25,
        }),
        "the mirror stores plain tokens and numbers"
    );

    let encoded = serde_json::to_string(&document).expect("the envelope serializes");
    let decoded: ViewRecord = serde_json::from_str(&encoded).expect("the envelope parses");
    let mut restored = State::new(true);
    decoded.restore(&mut restored, &mut Controls::new());
    assert_eq!(restored.densities(), [density].as_slice());
    assert_eq!(restored.densities()[0].time_law, law);
}

#[test]
fn charge_formula_round_trips_and_an_invalid_one_is_dropped_on_restore() {
    let mut density = harmonic_spec(
        DensityFamily::SteadyCurrent,
        box_support(),
        "x,0,0",
        10,
        harmonic(0.5, 0.0),
    );
    density.charge_formula = Some("-2*sin(0.5*t)".to_string());
    let mut source = State::new(true);
    source
        .commit_densities(vec![density.clone()])
        .expect("the paired fixture is valid");

    let document = ViewRecord::capture(&source);
    assert_eq!(
        document.densities[0].charge_formula.as_deref(),
        Some("-2*sin(0.5*t)"),
        "the mirror stores the paired charge formula"
    );
    let encoded = serde_json::to_string(&document).expect("the envelope serializes");
    let decoded: ViewRecord = serde_json::from_str(&encoded).expect("the envelope parses");
    let mut restored = State::new(true);
    decoded.restore(&mut restored, &mut Controls::new());
    assert_eq!(restored.densities(), [density].as_slice());

    // An unparseable stored formula drops only the pairing: the spatial current
    // and its harmonic law survive, and the restore adds no dirtying.
    let spatial = harmonic_spec(
        DensityFamily::SteadyCurrent,
        box_support(),
        "x,0,0",
        10,
        harmonic(0.5, 0.0),
    );
    let mut bad_pair = DensitySpecRecord::from_spec(&spatial);
    bad_pair.charge_formula = Some("(".to_string());
    let clean = ViewRecord::capture(&State::new(true));
    let payload = ViewRecord {
        densities: vec![bad_pair],
        ..clean.clone()
    };

    let mut baseline = State::new(true);
    baseline.dirty = false;
    clean.restore(&mut baseline, &mut Controls::new());

    let mut state = State::new(true);
    state.dirty = false;
    payload.restore(&mut state, &mut Controls::new());
    assert_eq!(state.densities().len(), 1, "the spatial density is kept");
    assert_eq!(state.densities()[0].formula, "x,0,0");
    assert_eq!(
        state.densities()[0].charge_formula,
        None,
        "only the invalid pairing is dropped"
    );
    assert!(
        state.densities()[0].time_law.is_harmonic(),
        "the valid law survives the pairing loss"
    );
    assert_eq!(
        state.dirty, baseline.dirty,
        "dropping a pairing adds no dirtying beyond the enclosing restore"
    );

    // A charge family may never carry a paired formula; a stored one is
    // dropped alone and the scalar rho density survives.
    let charge = spec(DensityFamily::VolumeCharge, box_support(), "3", 8);
    let mut bad_family = DensitySpecRecord::from_spec(&charge);
    bad_family.charge_formula = Some("1".to_string());
    let payload = ViewRecord {
        densities: vec![bad_family],
        ..clean
    };
    let mut state = State::new(true);
    state.dirty = false;
    payload.restore(&mut state, &mut Controls::new());
    assert_eq!(state.densities().len(), 1);
    assert_eq!(state.densities()[0].formula, "3");
    assert_eq!(state.densities()[0].charge_formula, None);
}

#[test]
fn invalid_harmonic_law_is_rejected_and_a_stored_one_is_dropped_on_restore() {
    for law in [
        harmonic(0.0, 0.0),
        harmonic(-1.0, 0.0),
        harmonic(MAX_ANGULAR_FREQUENCY + 1.0, 0.0),
        harmonic(f64::NAN, 0.0),
        harmonic(f64::INFINITY, 0.0),
        harmonic(1.5, f64::NAN),
        harmonic(1.5, f64::INFINITY),
    ] {
        let bad = harmonic_spec(DensityFamily::VolumeCharge, box_support(), "1", 8, law);
        assert!(
            bad.validate(&params()).is_err(),
            "the law {law:?} must be rejected, not coerced"
        );
    }

    let spatial = spec(DensityFamily::VolumeCharge, box_support(), "3", 8);
    let mut record = DensitySpecRecord::from_spec(&spatial);
    record.time_law = Some(TimeLawRecord {
        kind: "harmonic".to_string(),
        omega: 0.0,
        phase: 0.0,
    });
    let clean = ViewRecord::capture(&State::new(true));
    let document = ViewRecord {
        densities: vec![record],
        ..clean.clone()
    };

    let mut baseline = State::new(true);
    baseline.dirty = false;
    clean.restore(&mut baseline, &mut Controls::new());

    let mut state = State::new(true);
    state.dirty = false;
    document.restore(&mut state, &mut Controls::new());
    assert_eq!(
        state.densities().len(),
        1,
        "an invalid stored law drops only the law, keeping the spatial density"
    );
    assert_eq!(state.densities()[0].formula, "3");
    assert_eq!(state.densities()[0].time_law, TimeLaw::Steady);
    assert_eq!(
        state.dirty, baseline.dirty,
        "dropping an invalid law adds no dirtying beyond the enclosing restore"
    );
}

#[test]
fn harmonic_density_samples_follow_the_clock() {
    let density = harmonic_spec(
        DensityFamily::VolumeCharge,
        box_support(),
        "2",
        12,
        harmonic(1.0, 0.0),
    );
    density.validate(&params()).unwrap();

    let at_zero = charge_report(&density, &params(), 0.0)
        .expect("the report is defined")
        .total;
    let at_pi = charge_report(&density, &params(), PI)
        .expect("the report is defined")
        .total;
    let at_half_pi = charge_report(&density, &params(), PI / 2.0)
        .expect("the report is defined")
        .total;

    let expected = 2.0 * box_support().volume();
    assert!(
        (at_zero - expected).abs() <= 1e-9 * expected.abs(),
        "cos(0) keeps the spatial total: {at_zero} vs {expected}"
    );
    assert!(
        (at_pi + expected).abs() <= 1e-9 * expected.abs(),
        "cos(pi) flips the spatial total: {} vs {}",
        at_pi,
        -expected
    );
    assert!(
        at_half_pi.abs() <= 1e-9 * expected.abs(),
        "cos(pi/2) is the zero crossing: {at_half_pi}"
    );
}

#[test]
fn harmonic_charge_density_satisfies_continuity_within_tolerance() {
    // J(t) = J0 cos(omega t + phase) with J0 = (x,0,0), so div J = cos(...).
    // The independent paired charge is rho = -(div J0 / omega) sin(...), whose
    // finite-difference -d rho/dt is the same cos(...). The check must compare
    // the sampled divergence with that independent formula.
    let mut current = harmonic_spec(
        DensityFamily::SteadyCurrent,
        box_support(),
        "x,0,0",
        12,
        harmonic(0.25, PI / 4.0),
    );
    current.charge_formula = Some(format!("-4*sin(0.25*t + {})", PI / 4.0));
    current.validate(&params()).unwrap();

    let report = continuity_report(&current, &params(), 0.0).expect("the report is defined");
    match report.status {
        ContinuityStatus::Estimate { value, error } => {
            assert!(
                value < 1e-2,
                "a consistent finite-difference pair has a small residual: {report:?}"
            );
            assert!(error.is_finite(), "the estimate carries a finite error");
        }
        other => panic!("expected a bounded estimate, got {other:?}"),
    }
}

#[test]
fn inconsistent_harmonic_charge_pair_reports_a_violation() {
    // The same current against a charge formula that is twice the consistent
    // amplitude. The old self-derived check ignored the declared charge and
    // passed; a genuine independent comparison must see the mismatch.
    let mut current = harmonic_spec(
        DensityFamily::SteadyCurrent,
        box_support(),
        "x,0,0",
        12,
        harmonic(0.25, PI / 4.0),
    );
    current.charge_formula = Some(format!("-8*sin(0.25*t + {})", PI / 4.0));
    current.validate(&params()).unwrap();

    let report = continuity_report(&current, &params(), 0.0).expect("the report is defined");
    match report.status {
        ContinuityStatus::Violation {
            residual,
            tolerance,
        } => {
            assert!(
                residual > tolerance,
                "the mismatched charge pair must violate: {report:?}"
            );
            assert!(
                residual > 0.1,
                "the factor-of-two mismatch is large: {report:?}"
            );
        }
        other => panic!("expected a violation from the inconsistent pair, got {other:?}"),
    }
}

#[test]
fn harmonic_current_without_paired_charge_is_unresolved() {
    let current = harmonic_spec(
        DensityFamily::SteadyCurrent,
        box_support(),
        "x,0,0",
        12,
        harmonic(0.25, PI / 4.0),
    );
    current.validate(&params()).unwrap();

    let report = continuity_report(&current, &params(), 0.0).expect("the report is defined");
    match report.status {
        ContinuityStatus::Unresolved { ref reason } => {
            let lower = reason.to_lowercase();
            assert!(
                lower.contains("charge") && lower.contains("pair"),
                "the reason names the missing paired charge density: {reason}"
            );
        }
        other => panic!("expected an unresolved report, got {other:?}"),
    }
    assert!(
        report.div_j_min.is_nan() && report.div_j_max.is_nan(),
        "an unresolved report fabricates no numbers: {report:?}"
    );
}

#[test]
fn harmonic_charge_density_reports_continuity_unresolved() {
    let charge = harmonic_spec(
        DensityFamily::VolumeCharge,
        box_support(),
        "1",
        12,
        harmonic(1.0, 0.0),
    );
    charge.validate(&params()).unwrap();

    let report = continuity_report(&charge, &params(), 0.0).expect("the report is defined");
    match report.status {
        ContinuityStatus::Unresolved { ref reason } => {
            let lower = reason.to_lowercase();
            assert!(
                lower.contains("induced current") && lower.contains("not model"),
                "the reason says the induced current is not modelled: {reason}"
            );
        }
        other => panic!("a harmonic charge must not fabricate a continuity estimate: {other:?}"),
    }
    assert!(
        report.div_j_min.is_nan() && report.div_j_max.is_nan(),
        "no self-derived current is reported: {report:?}"
    );
}

#[test]
fn charge_formula_is_validated_and_forbidden_on_charge_families() {
    let mut current = harmonic_spec(
        DensityFamily::SteadyCurrent,
        box_support(),
        "x,0,0",
        8,
        harmonic(1.0, 0.0),
    );
    current.charge_formula = Some("(".to_string());
    let error = current
        .validate(&params())
        .expect_err("a malformed paired charge formula is rejected");
    assert!(
        error.contains("paired charge formula"),
        "the error names the paired formula: {error}"
    );

    let mut empty = current.clone();
    empty.charge_formula = Some("   ".to_string());
    assert!(
        empty.validate(&params()).is_err(),
        "an empty paired charge formula is rejected"
    );

    let mut charge = spec(DensityFamily::VolumeCharge, box_support(), "1", 8);
    charge.charge_formula = Some("1".to_string());
    let error = charge
        .validate(&params())
        .expect_err("a charge family's own formula is already rho");
    assert!(
        error.contains("charge_formula must be absent"),
        "the rejection is explicit: {error}"
    );

    let mut good = current;
    good.charge_formula = Some("-sin(t)".to_string());
    good.validate(&params())
        .expect("a well-formed paired charge formula is accepted");
}

#[test]
fn discontinuous_charge_flow_reports_a_violation_with_the_residual() {
    // A near-step current has a large central-difference divergence while the
    // steady law declares a zero time derivative; the report must carry the
    // measured residual rather than pass silently.
    let current = spec(
        DensityFamily::SteadyCurrent,
        box_support(),
        "x/(abs(x)+1e-12),0,0",
        12,
    );
    current.validate(&params()).unwrap();

    let report = continuity_report(&current, &params(), 0.0).expect("the report is defined");
    match report.status {
        ContinuityStatus::Violation {
            residual,
            tolerance,
        } => {
            assert!(residual.is_finite() && tolerance.is_finite());
            assert!(
                residual > tolerance,
                "the reported residual {residual} exceeds {tolerance}: {report:?}"
            );
            assert!(residual > 1.0, "the jump is large: {report:?}");
        }
        other => panic!("expected a violation with its residual, got {other:?}"),
    }
}

#[test]
fn steady_current_with_harmonic_charge_density_is_rejected() {
    let steady_current = spec(DensityFamily::SteadyCurrent, box_support(), "0,0,1", 8);
    let harmonic_charge = harmonic_spec(
        DensityFamily::VolumeCharge,
        box_support(),
        "1",
        8,
        harmonic(1.0, 0.0),
    );

    let error = validate_time_law_combination(&[steady_current.clone(), harmonic_charge.clone()])
        .expect_err("a steady current cannot balance a harmonic charge density");
    let lower = error.to_lowercase();
    assert!(
        lower.contains("steady current") && lower.contains("harmonic"),
        "the rejection names both terms explicitly: {error}"
    );

    // The same list is accepted once the current declares its own harmonic law.
    let harmonic_current = harmonic_spec(
        DensityFamily::SteadyCurrent,
        box_support(),
        "0,0,1",
        8,
        harmonic(1.0, 0.0),
    );
    validate_time_law_combination(&[harmonic_current, harmonic_charge.clone()])
        .expect("a harmonic current may carry a harmonic charge density");

    // The real atomic commit path enforces the same rule.
    let mut state = State::new(true);
    assert!(
        state
            .commit_densities(vec![steady_current, harmonic_charge])
            .is_err(),
        "the document commit rejects the inconsistent pair"
    );
    assert!(state.densities().is_empty());
}

#[test]
fn time_law_sampling_stays_within_the_existing_sample_budget() {
    let density = harmonic_spec(
        DensityFamily::VolumeCharge,
        sphere_support(),
        "1",
        MAX_SAMPLES,
        harmonic(MAX_ANGULAR_FREQUENCY, 0.5),
    );
    density.validate(&params()).unwrap();
    let report = charge_report(&density, &params(), 0.25).expect("the report is defined");
    assert!(report.samples > 0);
    assert!(
        u64::from(report.samples) <= u64::from(MAX_DENSITY_SAMPLES),
        "the time law adds no samples: {} > {}",
        report.samples,
        MAX_DENSITY_SAMPLES
    );
}
