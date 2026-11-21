//! Tests for the bounded source-free plane-wave teaching model (issue #97 slice A).

use glam::Vec3;
use vector_field_explorer::maxwell_learning::MaxwellEquation;
use vector_field_explorer::maxwell_residual::{FieldTerms, TermStatus, residual};
use vector_field_explorer::maxwell_wave::{
    MAX_AMPLITUDE, MIN_WAVELENGTH, TEACHING_SPEED, WaveSample, WaveSpec, equation_terms,
    sample_wave,
};

/// Relative tolerance for `f32` public samples (unit axes, orthogonality).
const SAMPLE_TOLERANCE: f32 = 1e-5;
/// Pinned tolerance for the closed-form Faraday residual in `f64`.
///
/// The analytic terms are derived from the same phase, so only binary64
/// rounding remains. Observed values are around `1e-16`; `1e-9` leaves margin
/// for different wavelength/amplitude pairs without hiding an error as large
/// as a single term.
const FARADAY_TOLERANCE: f64 = 1e-9;

fn base_spec() -> WaveSpec {
    WaveSpec {
        amplitude: 1.5,
        wavelength: 2.0,
        direction: Vec3::new(1.0, 2.0, 3.0),
        polarization: Vec3::new(1.0, -1.0, 0.5),
        phase: 0.25,
    }
}

fn validated_spec() -> WaveSpec {
    let mut spec = base_spec();
    spec.validate().expect("base spec validates");
    spec
}

fn assert_sample_finite(sample: &WaveSample) {
    assert!(sample.e.is_finite(), "E is not finite: {:?}", sample.e);
    assert!(sample.b.is_finite(), "B is not finite: {:?}", sample.b);
    assert!(sample.phase.is_finite(), "phase is not finite");
}

#[test]
fn wavelength_and_amplitude_validation_rejects_non_finite_and_out_of_range() {
    let mut at_bounds = WaveSpec {
        amplitude: MAX_AMPLITUDE,
        wavelength: MIN_WAVELENGTH * 2.0,
        ..base_spec()
    };
    assert!(at_bounds.validate().is_ok());

    for amplitude in [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        MAX_AMPLITUDE * 2.0,
        -(MAX_AMPLITUDE * 2.0),
    ] {
        let mut candidate = base_spec();
        candidate.amplitude = amplitude;
        assert!(
            candidate.validate().is_err(),
            "amplitude {amplitude} must be rejected"
        );
        assert_eq!(
            candidate.amplitude.to_bits(),
            amplitude.to_bits(),
            "out-of-range amplitude must be rejected, not clamped"
        );
    }

    for wavelength in [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        0.0,
        -1.0,
        MIN_WAVELENGTH,
        MIN_WAVELENGTH * 0.5,
    ] {
        let mut candidate = base_spec();
        candidate.wavelength = wavelength;
        assert!(
            candidate.validate().is_err(),
            "wavelength {wavelength} must be rejected"
        );
        assert_eq!(
            candidate.wavelength.to_bits(),
            wavelength.to_bits(),
            "out-of-range wavelength must be rejected, not clamped"
        );
    }

    let mut non_finite_phase = base_spec();
    non_finite_phase.phase = f64::NAN;
    assert!(non_finite_phase.validate().is_err());

    let mut negative_amplitude = base_spec();
    negative_amplitude.amplitude = -MAX_AMPLITUDE;
    assert!(negative_amplitude.validate().is_ok());
}

#[test]
fn polarization_is_projected_perpendicular_to_propagation() {
    let mut candidate = WaveSpec {
        direction: Vec3::Z * 4.0,
        polarization: Vec3::new(1.0, 0.0, 1.0),
        ..base_spec()
    };
    candidate.validate().expect("projectable polarization");

    assert!(
        (candidate.direction.length() - 1.0).abs() <= SAMPLE_TOLERANCE,
        "direction is renormalized"
    );
    assert!(
        candidate.polarization.dot(candidate.direction).abs() <= SAMPLE_TOLERANCE,
        "polarization is perpendicular to direction: {:?} vs {:?}",
        candidate.polarization,
        candidate.direction
    );
    assert!(
        (candidate.polarization.length() - 1.0).abs() <= SAMPLE_TOLERANCE,
        "polarization is renormalized"
    );
    assert!(candidate.polarization.z.abs() <= SAMPLE_TOLERANCE);

    let mut parallel = WaveSpec {
        direction: Vec3::Z,
        polarization: Vec3::Z,
        ..base_spec()
    };
    let error = parallel
        .validate()
        .expect_err("parallel polarization fails");
    assert!(
        error.contains("polarization"),
        "parallel rejection names the input: {error}"
    );

    let mut zero_direction = WaveSpec {
        direction: Vec3::ZERO,
        ..base_spec()
    };
    let error = zero_direction
        .validate()
        .expect_err("degenerate direction fails");
    assert!(
        error.contains("direction"),
        "degenerate rejection names the input: {error}"
    );

    let mut non_finite_polarization = WaveSpec {
        polarization: Vec3::new(f32::NAN, 0.0, 1.0),
        ..base_spec()
    };
    assert!(non_finite_polarization.validate().is_err());
}

#[test]
fn wave_is_transverse_and_b_is_orthogonal_to_e_and_k() {
    let spec = validated_spec();
    let sample = sample_wave(&spec, Vec3::new(0.3, -0.7, 1.1), 0.4).expect("sample");

    assert!(
        (TEACHING_SPEED - 1.0).abs() < f64::EPSILON,
        "teaching units set c = 1"
    );
    let k = spec.k();
    let k_hat = k.normalize();
    assert!(
        (k.length() as f64 - spec.omega() / TEACHING_SPEED).abs() <= 1e-3,
        "|k| = omega / c"
    );

    assert!(sample.e.length() > 0.0);
    assert!(
        sample.e.dot(k_hat).abs() <= SAMPLE_TOLERANCE,
        "E is transverse: E·k = {}",
        sample.e.dot(k_hat)
    );
    assert!(
        sample.b.dot(k_hat).abs() <= SAMPLE_TOLERANCE,
        "B is transverse: B·k = {}",
        sample.b.dot(k_hat)
    );
    assert!(
        sample.e.dot(sample.b).abs() <= SAMPLE_TOLERANCE,
        "E and B are orthogonal: E·B = {}",
        sample.e.dot(sample.b)
    );
    let propagated = k_hat.cross(sample.e) / TEACHING_SPEED as f32;
    assert!(
        (sample.b - propagated).length() <= SAMPLE_TOLERANCE,
        "B = (1/c) k̂ × E"
    );
    assert!(
        (sample.e.length() - sample.b.length()).abs() <= SAMPLE_TOLERANCE,
        "|E| = |B| when c = 1"
    );
}

#[test]
fn sampled_e_and_b_are_finite_and_equal_in_magnitude_when_c_is_one() {
    let spec = validated_spec();
    for point in [
        Vec3::ZERO,
        Vec3::new(0.25, -1.5, 2.0),
        Vec3::new(-4.0, 0.5, 0.125),
    ] {
        for time in [-2.0, 0.0, 0.35, 10.0] {
            let sample = sample_wave(&spec, point, time).expect("finite sample");
            assert_sample_finite(&sample);
            assert!(
                (sample.e.length() - sample.b.length()).abs()
                    <= SAMPLE_TOLERANCE * (1.0 + sample.e.length()),
                "|E| and |B| agree at {point:?} t={time}: {} vs {}",
                sample.e.length(),
                sample.b.length()
            );
        }
    }
}

#[test]
fn gauss_equations_close_analytically_for_the_plane_wave() {
    let spec = validated_spec();
    let terms = equation_terms(&spec, Vec3::new(0.4, -0.2, 1.0), 0.7).expect("terms");

    assert_eq!(terms.div_e, Some(0.0));
    assert_eq!(terms.div_b, Some(0.0));
    assert_eq!(terms.rho, Some(0.0));
    assert_eq!(terms.j, None);

    let gauss_e = residual(MaxwellEquation::GaussElectric, &terms);
    assert_eq!(gauss_e.status, TermStatus::Analytic);
    assert_eq!(gauss_e.lhs, Some(0.0));
    assert_eq!(gauss_e.rhs, Some(0.0));
    assert_eq!(gauss_e.normalized, Some(0.0));

    let gauss_b = residual(MaxwellEquation::GaussMagnetic, &terms);
    assert_eq!(gauss_b.status, TermStatus::Analytic);
    assert_eq!(gauss_b.lhs, Some(0.0));
    assert_eq!(gauss_b.rhs, Some(0.0));
    assert_eq!(gauss_b.normalized, Some(0.0));
}

#[test]
fn faraday_residual_stays_within_the_analytic_tolerance() {
    let spec = validated_spec();
    for (point, time) in [
        (Vec3::ZERO, 0.0),
        (Vec3::new(0.3, -0.7, 1.1), 0.4),
        (Vec3::new(-2.0, 1.25, 0.5), 3.7),
        (Vec3::new(5.0, 5.0, 5.0), -1.5),
    ] {
        let terms = equation_terms(&spec, point, time).expect("terms");
        let faraday = residual(MaxwellEquation::Faraday, &terms);
        assert_eq!(faraday.status, TermStatus::Analytic);
        let normalized = faraday.normalized.expect("normalized residual");
        assert!(
            normalized <= FARADAY_TOLERANCE,
            "Faraday residual {normalized} exceeds {FARADAY_TOLERANCE} at {point:?} t={time}"
        );
        let vector = faraday.vector_residual.expect("vector residual");
        assert!(
            vector
                .iter()
                .all(|component| component.is_finite() && component.abs() <= FARADAY_TOLERANCE),
            "Faraday vector residual {vector:?} exceeds {FARADAY_TOLERANCE} at {point:?} t={time}"
        );
    }
}

#[test]
fn ampere_maxwell_reports_unsupported_without_sources() {
    let spec = validated_spec();
    let terms = equation_terms(&spec, Vec3::new(0.1, 0.2, 0.3), 1.2).expect("terms");

    assert!(terms.curl_b.is_some());
    assert!(terms.de_dt.is_some());
    assert_eq!(terms.j, None, "no free-current distribution is modelled");

    let ampere = residual(MaxwellEquation::AmpereMaxwell, &terms);
    assert_eq!(ampere.status, TermStatus::Unsupported("j"));
    assert_eq!(ampere.normalized, None);
    assert_eq!(ampere.vector_residual, None);
}

#[test]
fn non_finite_point_or_time_yields_no_sample() {
    let spec = validated_spec();
    for point in [
        Vec3::new(f32::NAN, 0.0, 0.0),
        Vec3::new(0.0, f32::INFINITY, 0.0),
        Vec3::new(0.0, 0.0, f32::NEG_INFINITY),
    ] {
        assert!(sample_wave(&spec, point, 1.0).is_none(), "point {point:?}");
        assert!(
            equation_terms(&spec, point, 1.0).is_none(),
            "point {point:?}"
        );
    }
    for time in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(
            sample_wave(&spec, Vec3::ZERO, time).is_none(),
            "time {time}"
        );
        assert!(
            equation_terms(&spec, Vec3::ZERO, time).is_none(),
            "time {time}"
        );
    }

    let mut invalid = base_spec();
    invalid.amplitude = MAX_AMPLITUDE * 2.0;
    assert!(sample_wave(&invalid, Vec3::ZERO, 0.0).is_none());
    assert!(equation_terms(&invalid, Vec3::ZERO, 0.0).is_none());

    assert!(sample_wave(&spec, Vec3::ZERO, 0.0).is_some());
    assert!(equation_terms(&spec, Vec3::ZERO, 0.0).is_some());
}

#[test]
fn samples_are_deterministic_and_repeatable() {
    let spec = validated_spec();
    let point = Vec3::new(-0.6, 0.2, 0.9);
    let time = 0.75;

    let first = sample_wave(&spec, point, time).expect("sample");
    let second = sample_wave(&spec, point, time).expect("sample");
    assert_eq!(first, second);

    let first_terms: FieldTerms = equation_terms(&spec, point, time).expect("terms");
    let second_terms: FieldTerms = equation_terms(&spec, point, time).expect("terms");
    assert_eq!(first_terms, second_terms);

    let later = sample_wave(&spec, point, time + 0.1).expect("sample");
    assert_ne!(first.phase, later.phase, "the phase advances with time");
}
