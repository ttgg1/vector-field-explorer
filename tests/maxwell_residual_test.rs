//! Residual-diagnostic tests for the four Maxwell equations (issue #94 slice A).

use vector_field_explorer::maxwell_learning::MaxwellEquation;
use vector_field_explorer::maxwell_residual::{FieldTerms, Residual, TermStatus, residual};

/// Terms that satisfy every equation exactly in normalized teaching units.
fn consistent_terms() -> FieldTerms {
    FieldTerms {
        div_e: Some(0.5),
        div_b: Some(0.0),
        curl_e: Some([1.0, 2.0, 3.0]),
        curl_b: Some([1.0, 2.0, 0.0]),
        rho: Some(0.5),
        j: Some([1.0, 0.0, 0.0]),
        de_dt: Some([0.0, 2.0, 0.0]),
        db_dt: Some([-1.0, -2.0, -3.0]),
    }
}

fn assert_no_nan(residual: &Residual) {
    if let Some(normalized) = residual.normalized {
        assert!(!normalized.is_nan(), "normalized NaN: {residual:?}");
    }
    if let Some(value) = residual.vector_residual {
        assert!(
            value.iter().all(|component| !component.is_nan()),
            "vector NaN: {residual:?}"
        );
    }
    if let Some(value) = residual.lhs {
        assert!(!value.is_nan(), "lhs NaN: {residual:?}");
    }
    if let Some(value) = residual.rhs {
        assert!(!value.is_nan(), "rhs NaN: {residual:?}");
    }
}

#[test]
fn gauss_magnetic_residual_uses_a_zero_rhs() {
    let terms = FieldTerms {
        div_b: Some(0.25),
        ..FieldTerms::default()
    };
    let checked = residual(MaxwellEquation::GaussMagnetic, &terms);

    assert_eq!(checked.equation, MaxwellEquation::GaussMagnetic);
    assert_eq!(checked.lhs, Some(0.25));
    assert_eq!(checked.rhs, Some(0.0));
    assert!(checked.normalized.is_some_and(|value| value > 0.0));
    assert!(matches!(checked.status, TermStatus::Analytic));
    assert_no_nan(&checked);

    let zero = FieldTerms {
        div_b: Some(0.0),
        ..FieldTerms::default()
    };
    let checked = residual(MaxwellEquation::GaussMagnetic, &zero);
    assert_eq!(checked.rhs, Some(0.0));
    assert_eq!(checked.normalized, Some(0.0));
}

#[test]
fn consistent_terms_give_zero_residual() {
    let terms = consistent_terms();
    for equation in MaxwellEquation::ALL {
        let checked = residual(equation, &terms);
        assert!(
            matches!(checked.status, TermStatus::Analytic),
            "{equation:?}: {:?}",
            checked.status
        );
        let normalized = checked
            .normalized
            .unwrap_or_else(|| panic!("{equation:?} has no normalized residual"));
        assert!(normalized.is_finite(), "{equation:?}: {normalized}");
        assert!(normalized.abs() < 1e-12, "{equation:?}: {normalized}");
        if let Some(vector) = checked.vector_residual {
            assert!(
                vector.iter().all(|component| component.abs() < 1e-12),
                "{equation:?}: {vector:?}"
            );
        }
        assert_no_nan(&checked);
    }
}

#[test]
fn missing_terms_are_reported_as_unsupported_not_zero() {
    let cases = [
        (
            MaxwellEquation::GaussElectric,
            FieldTerms {
                div_e: Some(1.0),
                ..FieldTerms::default()
            },
            "rho",
        ),
        (
            MaxwellEquation::GaussElectric,
            FieldTerms {
                rho: Some(1.0),
                ..FieldTerms::default()
            },
            "div_e",
        ),
        (
            MaxwellEquation::GaussMagnetic,
            FieldTerms::default(),
            "div_b",
        ),
        (
            MaxwellEquation::Faraday,
            FieldTerms {
                curl_e: Some([1.0, 0.0, 0.0]),
                ..FieldTerms::default()
            },
            "db_dt",
        ),
        (
            MaxwellEquation::Faraday,
            FieldTerms {
                db_dt: Some([1.0, 0.0, 0.0]),
                ..FieldTerms::default()
            },
            "curl_e",
        ),
        (
            MaxwellEquation::AmpereMaxwell,
            FieldTerms {
                curl_b: Some([1.0, 0.0, 0.0]),
                j: Some([1.0, 0.0, 0.0]),
                ..FieldTerms::default()
            },
            "displacement current (de_dt)",
        ),
        (
            MaxwellEquation::AmpereMaxwell,
            FieldTerms {
                curl_b: Some([1.0, 0.0, 0.0]),
                de_dt: Some([0.0, 0.0, 0.0]),
                ..FieldTerms::default()
            },
            "j",
        ),
        (
            MaxwellEquation::AmpereMaxwell,
            FieldTerms {
                j: Some([0.0, 0.0, 0.0]),
                de_dt: Some([0.0, 0.0, 0.0]),
                ..FieldTerms::default()
            },
            "curl_b",
        ),
    ];

    for (equation, terms, symbol) in cases {
        let checked = residual(equation, &terms);
        assert_eq!(
            checked.status,
            TermStatus::Unsupported(symbol),
            "{equation:?}: {:?}",
            checked.status
        );
        assert_eq!(checked.lhs, None, "{equation:?}/{symbol}");
        assert_eq!(checked.rhs, None, "{equation:?}/{symbol}");
        assert_eq!(checked.vector_residual, None, "{equation:?}/{symbol}");
        assert_eq!(checked.normalized, None, "{equation:?}/{symbol}");
        assert!(
            checked.explanation.contains(symbol),
            "{equation:?}/{symbol}: {}",
            checked.explanation
        );
        assert!(
            !checked.explanation.to_lowercase().contains("satisfied"),
            "{equation:?}/{symbol} claims satisfaction: {}",
            checked.explanation
        );
    }
}

#[test]
fn non_finite_terms_are_singular_and_never_nan() {
    let cases = [
        (
            MaxwellEquation::GaussElectric,
            FieldTerms {
                div_e: Some(f64::NAN),
                rho: Some(1.0),
                ..FieldTerms::default()
            },
            "div_e",
        ),
        (
            MaxwellEquation::GaussElectric,
            FieldTerms {
                div_e: Some(1.0),
                rho: Some(f64::INFINITY),
                ..FieldTerms::default()
            },
            "rho",
        ),
        (
            MaxwellEquation::GaussMagnetic,
            FieldTerms {
                div_b: Some(f64::NEG_INFINITY),
                ..FieldTerms::default()
            },
            "div_b",
        ),
        (
            MaxwellEquation::Faraday,
            FieldTerms {
                curl_e: Some([1.0, f64::NAN, 0.0]),
                db_dt: Some([0.0, 0.0, 0.0]),
                ..FieldTerms::default()
            },
            "curl_e",
        ),
        (
            MaxwellEquation::Faraday,
            FieldTerms {
                curl_e: Some([0.0, 0.0, 0.0]),
                db_dt: Some([0.0, f64::INFINITY, 0.0]),
                ..FieldTerms::default()
            },
            "db_dt",
        ),
        (
            MaxwellEquation::AmpereMaxwell,
            FieldTerms {
                curl_b: Some([f64::NAN, 0.0, 0.0]),
                j: Some([0.0, 0.0, 0.0]),
                de_dt: Some([0.0, 0.0, 0.0]),
                ..FieldTerms::default()
            },
            "curl_b",
        ),
        (
            MaxwellEquation::AmpereMaxwell,
            FieldTerms {
                curl_b: Some([0.0, 0.0, 0.0]),
                j: Some([0.0, f64::INFINITY, 0.0]),
                de_dt: Some([0.0, 0.0, 0.0]),
                ..FieldTerms::default()
            },
            "j",
        ),
        (
            MaxwellEquation::AmpereMaxwell,
            FieldTerms {
                curl_b: Some([0.0, 0.0, 0.0]),
                j: Some([0.0, 0.0, 0.0]),
                de_dt: Some([0.0, 0.0, f64::NEG_INFINITY]),
                ..FieldTerms::default()
            },
            "de_dt",
        ),
    ];

    for (equation, terms, symbol) in cases {
        let checked = residual(equation, &terms);
        assert!(
            matches!(checked.status, TermStatus::Singular(name) if name == symbol),
            "{equation:?}: {:?}",
            checked.status
        );
        assert_eq!(checked.lhs, None, "{equation:?}/{symbol}");
        assert_eq!(checked.rhs, None, "{equation:?}/{symbol}");
        assert_eq!(checked.vector_residual, None, "{equation:?}/{symbol}");
        assert_eq!(checked.normalized, None, "{equation:?}/{symbol}");
        assert!(
            checked.explanation.contains(symbol),
            "{equation:?}/{symbol}: {}",
            checked.explanation
        );
        assert_no_nan(&checked);
    }
}

#[test]
fn ampere_maxwell_requires_the_displacement_term() {
    let without_displacement = FieldTerms {
        curl_b: Some([1.0, 0.0, 0.0]),
        j: Some([1.0, 0.0, 0.0]),
        ..FieldTerms::default()
    };
    let checked = residual(MaxwellEquation::AmpereMaxwell, &without_displacement);
    assert_eq!(
        checked.status,
        TermStatus::Unsupported("displacement current (de_dt)")
    );
    assert_eq!(checked.normalized, None);
    assert!(checked.vector_residual.is_none());
    assert!(
        checked
            .explanation
            .to_lowercase()
            .contains("displacement current")
    );
    assert!(!checked.explanation.to_lowercase().contains("satisfied"));

    let with_displacement = FieldTerms {
        de_dt: Some([0.0, 2.0, 0.0]),
        ..without_displacement
    };
    let checked = residual(MaxwellEquation::AmpereMaxwell, &with_displacement);
    assert!(matches!(checked.status, TermStatus::Analytic));
    assert!(checked.normalized.is_some());
    assert!(
        checked
            .explanation
            .to_lowercase()
            .contains("displacement current")
    );
}

#[test]
fn normalized_residual_is_finite_and_bounded() {
    let scalar_cases = [
        (0.0, 0.0),
        (1.0e300, -1.0e300),
        (f64::MIN_POSITIVE, 0.0),
        (-1.0e-300, 1.0e300),
        (3.0, 1.0),
    ];
    for (lhs, rhs) in scalar_cases {
        let checked = residual(
            MaxwellEquation::GaussElectric,
            &FieldTerms {
                div_e: Some(lhs),
                rho: Some(rhs),
                ..FieldTerms::default()
            },
        );
        let normalized = checked.normalized.expect("scalar normalized residual");
        assert!(normalized.is_finite(), "{lhs}/{rhs}: {normalized}");
        assert!(
            (0.0..=2.0).contains(&normalized),
            "{lhs}/{rhs}: {normalized}"
        );
        assert_no_nan(&checked);
    }

    let vector_cases = [
        ([1.0e300, -1.0e300, 5.0e299], [-1.0e300, 1.0e300, -5.0e299]),
        ([0.0, 0.0, 0.0], [0.0, 0.0, 0.0]),
        ([1.0e-300, 0.0, 0.0], [0.0, 0.0, 0.0]),
    ];
    for (curl, db_dt) in vector_cases {
        let checked = residual(
            MaxwellEquation::Faraday,
            &FieldTerms {
                curl_e: Some(curl),
                db_dt: Some(db_dt),
                ..FieldTerms::default()
            },
        );
        let normalized = checked.normalized.expect("vector normalized residual");
        assert!(normalized.is_finite(), "{curl:?}/{db_dt:?}: {normalized}");
        assert!(
            (0.0..=2.0).contains(&normalized),
            "{curl:?}/{db_dt:?}: {normalized}"
        );
        assert_no_nan(&checked);
    }
}

#[test]
fn huge_same_sign_finite_vectors_keep_both_fields_finite() {
    let big = [1.0e308, 0.0, 0.0];

    let faraday = residual(
        MaxwellEquation::Faraday,
        &FieldTerms {
            curl_e: Some(big),
            db_dt: Some(big),
            ..FieldTerms::default()
        },
    );
    assert!(
        matches!(faraday.status, TermStatus::Analytic),
        "Faraday: {:?}",
        faraday.status
    );
    let vector = faraday
        .vector_residual
        .expect("Faraday vector residual for finite inputs");
    assert!(
        vector.iter().all(|component| component.is_finite()),
        "Faraday vector residual overflowed: {vector:?}"
    );
    assert!(
        faraday.normalized.is_some_and(f64::is_finite),
        "Faraday normalized not finite: {:?}",
        faraday.normalized
    );

    let ampere = residual(
        MaxwellEquation::AmpereMaxwell,
        &FieldTerms {
            curl_b: Some(big),
            j: Some(big),
            de_dt: Some(big),
            ..FieldTerms::default()
        },
    );
    assert!(
        matches!(ampere.status, TermStatus::Analytic),
        "Ampere-Maxwell: {:?}",
        ampere.status
    );
    let vector = ampere
        .vector_residual
        .expect("Ampere-Maxwell vector residual for finite inputs");
    assert!(
        vector.iter().all(|component| component.is_finite()),
        "Ampere-Maxwell vector residual overflowed: {vector:?}"
    );
    assert!(
        ampere.normalized.is_some_and(f64::is_finite),
        "Ampere-Maxwell normalized not finite: {:?}",
        ampere.normalized
    );
}

#[test]
fn unsupported_takes_precedence_over_singular() {
    // Missing terms are checked before non-finite terms, so an absent input is
    // reported as Unsupported even when another supplied term is non-finite.
    let cases = [
        (
            MaxwellEquation::GaussElectric,
            FieldTerms {
                div_e: Some(f64::NAN),
                rho: None,
                ..FieldTerms::default()
            },
            "rho",
        ),
        (
            MaxwellEquation::GaussElectric,
            FieldTerms {
                div_e: None,
                rho: Some(f64::INFINITY),
                ..FieldTerms::default()
            },
            "div_e",
        ),
        (
            MaxwellEquation::Faraday,
            FieldTerms {
                curl_e: Some([f64::NAN, 0.0, 0.0]),
                db_dt: None,
                ..FieldTerms::default()
            },
            "db_dt",
        ),
        (
            MaxwellEquation::Faraday,
            FieldTerms {
                curl_e: None,
                db_dt: Some([f64::INFINITY, 0.0, 0.0]),
                ..FieldTerms::default()
            },
            "curl_e",
        ),
        (
            MaxwellEquation::AmpereMaxwell,
            FieldTerms {
                curl_b: Some([f64::NAN, 0.0, 0.0]),
                j: Some([0.0, 0.0, 0.0]),
                de_dt: None,
                ..FieldTerms::default()
            },
            "displacement current (de_dt)",
        ),
        (
            MaxwellEquation::AmpereMaxwell,
            FieldTerms {
                curl_b: None,
                j: Some([f64::INFINITY, 0.0, 0.0]),
                de_dt: Some([0.0, 0.0, 0.0]),
                ..FieldTerms::default()
            },
            "curl_b",
        ),
    ];

    for (equation, terms, symbol) in cases {
        let checked = residual(equation, &terms);
        assert_eq!(
            checked.status,
            TermStatus::Unsupported(symbol),
            "{equation:?}: {:?}",
            checked.status
        );
        assert_eq!(checked.vector_residual, None, "{equation:?}/{symbol}");
        assert_eq!(checked.normalized, None, "{equation:?}/{symbol}");
    }
}

#[test]
fn residual_does_not_claim_a_full_time_dependent_solve() {
    let terms = consistent_terms();
    for equation in MaxwellEquation::ALL {
        let checked = residual(equation, &terms);
        let explanation = checked.explanation.to_lowercase();
        assert!(
            explanation.contains("does not solve"),
            "{equation:?}: {}",
            checked.explanation
        );
        assert!(
            explanation.contains("time-dependent"),
            "{equation:?}: {}",
            checked.explanation
        );
        assert!(
            !explanation.contains("solved the full")
                && !explanation.contains("full solution")
                && !explanation.contains("satisfied"),
            "{equation:?}: {}",
            checked.explanation
        );
    }
}
