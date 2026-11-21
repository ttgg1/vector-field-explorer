//! Teaching-text tests for the four Maxwell equations (issue #94 slice A).

use vector_field_explorer::maxwell_learning::{EquationForm, MaxwellEquation, spec};

#[test]
fn all_four_equations_have_integral_and_differential_text() {
    assert_eq!(MaxwellEquation::ALL.len(), 4);
    assert_ne!(EquationForm::Integral, EquationForm::Differential);

    for equation in MaxwellEquation::ALL {
        let equation_spec = spec(equation);
        assert_eq!(equation_spec.equation, equation);
        assert_eq!(equation_spec.form, EquationForm::Integral);
        assert!(!equation_spec.title.is_empty(), "title for {equation:?}");
        assert!(
            equation_spec.integral.len() > 30,
            "integral text for {equation:?}"
        );
        assert!(
            equation_spec.differential.len() > 20,
            "differential text for {equation:?}"
        );
        assert_ne!(equation_spec.integral, equation_spec.differential);
        assert!(
            equation_spec.integral.contains("Normalized teaching units"),
            "integral units for {equation:?}"
        );
        assert!(
            equation_spec
                .differential
                .contains("Normalized teaching units"),
            "differential units for {equation:?}"
        );
    }
}

#[test]
fn specs_name_every_symbol_used_in_the_statement() {
    for equation in MaxwellEquation::ALL {
        let equation_spec = spec(equation);
        assert!(
            !equation_spec.symbols.is_empty(),
            "symbols for {equation:?}"
        );

        let statement = format!(
            "{} {} {}",
            equation_spec.integral, equation_spec.differential, equation_spec.assumptions
        );
        for (symbol, meaning) in equation_spec.symbols {
            assert!(
                statement.contains(symbol),
                "{equation:?} does not use listed symbol {symbol}"
            );
            assert!(!meaning.is_empty(), "meaning for {equation:?}/{symbol}");
        }
        assert!(
            equation_spec
                .assumptions
                .to_lowercase()
                .contains("not a solver"),
            "solver disclaimer for {equation:?}"
        );
    }

    let electric = spec(MaxwellEquation::GaussElectric)
        .assumptions
        .to_lowercase();
    let magnetic = spec(MaxwellEquation::GaussMagnetic)
        .assumptions
        .to_lowercase();
    let faraday = spec(MaxwellEquation::Faraday).assumptions.to_lowercase();
    let ampere = spec(MaxwellEquation::AmpereMaxwell)
        .assumptions
        .to_lowercase();
    assert!(electric.contains("electrostatic"));
    assert!(magnetic.contains("magnetostatic"));
    assert!(faraday.contains("time-dependent"));
    assert!(ampere.contains("quasistatic"));
    assert!(ampere.contains("full time-dependent"));
}

#[test]
fn gauss_surfaces_are_named_in_the_symbol_tables() {
    for equation in [
        MaxwellEquation::GaussElectric,
        MaxwellEquation::GaussMagnetic,
    ] {
        let symbols = spec(equation).symbols;
        assert!(
            symbols.iter().any(|(symbol, meaning)| {
                *symbol == "S" && meaning.to_lowercase().contains("surface")
            }),
            "{equation:?} symbol table omits the closed surface S: {symbols:?}"
        );
    }
}

#[test]
fn assumptions_state_the_unit_conventions_used_by_the_residuals() {
    let electric = spec(MaxwellEquation::GaussElectric).assumptions;
    let ampere = spec(MaxwellEquation::AmpereMaxwell).assumptions;
    for assumptions in [electric, ampere] {
        assert!(
            assumptions.contains("eps0 = 1"),
            "assumptions omit eps0 = 1: {assumptions}"
        );
        assert!(
            assumptions.contains("mu0 = 1"),
            "assumptions omit mu0 = 1: {assumptions}"
        );
    }
}
