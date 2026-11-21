//! Tolerant CAS input grammar and intent classification (#168).
//!
//! These tests pin the pure seams only: they never call an engine, touch the UI,
//! or mutate application state.

use vector_field_explorer::cas_intent::{
    CasIntent, CasIntentError, MAX_ASSIGNMENTS, MAX_INPUT_BYTES, MAX_NESTING, classify, normalize,
};

fn derivative(input: &str) -> (String, String) {
    match classify(input).unwrap().intent {
        CasIntent::Derivative { variable, expr } => (variable, expr),
        other => panic!("expected derivative, got {other:?}"),
    }
}

#[test]
fn derivative_accepts_slash_and_ddx_aliases() {
    assert_eq!(derivative("d/dx x^2 + y"), ("x".into(), "x^2 + y".into()));
    assert_eq!(derivative("d/dt sin(t)"), ("t".into(), "sin(t)".into()));
    assert_eq!(derivative("ddx x^3 - 1"), ("x".into(), "x^3 - 1".into()));
    assert_eq!(derivative("ddx(x^3)"), ("x".into(), "(x^3)".into()));
    // `ddxy` is a plain expression, not the `ddx` alias.
    assert!(matches!(
        classify("ddxy + 1").unwrap().intent,
        CasIntent::Plain { .. }
    ));
}

#[test]
fn derivative_accepts_diff_call() {
    assert_eq!(
        derivative("diff(x^2 + y, y)"),
        ("y".into(), "x^2 + y".into())
    );
    assert_eq!(derivative("diff(sin(t), t)"), ("t".into(), "sin(t)".into()));
}

#[test]
fn derivative_reports_missing_expression_and_bad_variables() {
    assert_eq!(
        classify("d/dx").unwrap_err(),
        CasIntentError::MissingExpression
    );
    assert_eq!(
        classify("d/d").unwrap_err(),
        CasIntentError::MissingExpression
    );
    assert_eq!(
        classify("ddx   ").unwrap_err(),
        CasIntentError::MissingExpression
    );
    assert_eq!(
        classify("d/d1 x").unwrap_err(),
        CasIntentError::BadVariable("1".into())
    );
    assert_eq!(
        classify("diff(x, 2x)").unwrap_err(),
        CasIntentError::BadVariable("2x".into())
    );
}

#[test]
fn gradient_accepts_word_and_nabla_aliases() {
    let cases = [
        ("grad x^2 + y^2", "x^2 + y^2", "grad x^2 + y^2"),
        ("∇ f", "f", "∇ f"),
        ("∇(x^2 + y^2)", "x^2 + y^2", "∇(x^2 + y^2)"),
        ("\\nabla f", "f", "∇ f"),
    ];
    for (input, expr, normalized) in cases {
        let parse = classify(input).unwrap();
        assert_eq!(parse.normalized, normalized, "{input}");
        assert_eq!(
            parse.intent,
            CasIntent::Gradient { expr: expr.into() },
            "{input}"
        );
    }
}

#[test]
fn divergence_accepts_word_dot_and_latex_dot_aliases() {
    for input in ["div F", "∇· F", "∇\\cdot F", "∇* F"] {
        let parse = classify(input).unwrap();
        assert_eq!(
            parse.intent,
            CasIntent::Divergence { expr: "F".into() },
            "{input}"
        );
    }
    assert_eq!(classify("∇· F").unwrap().normalized, "∇* F");
    assert_eq!(classify("∇\\cdot F").unwrap().normalized, "∇* F");
}

#[test]
fn curl_accepts_word_cross_and_latex_cross_aliases() {
    for input in ["curl F", "∇× F", "∇\\times F", "∇ × F"] {
        let parse = classify(input).unwrap();
        assert_eq!(
            parse.intent,
            CasIntent::Curl { expr: "F".into() },
            "{input}"
        );
    }
    assert_eq!(classify("∇\\times F").unwrap().normalized, "∇× F");
}

#[test]
fn nabla_without_an_operator_is_gradient_and_needs_an_expression() {
    assert_eq!(
        classify("∇").unwrap_err(),
        CasIntentError::MissingExpression
    );
    assert_eq!(
        classify("∇·").unwrap_err(),
        CasIntentError::MissingExpression
    );
    assert_eq!(
        classify("grad").unwrap_err(),
        CasIntentError::MissingExpression
    );
}

#[test]
fn integral_accepts_indefinite_definite_and_integrate_call() {
    assert_eq!(
        classify("∫ x^2 dx").unwrap().intent,
        CasIntent::Integral {
            variable: "x".into(),
            expr: "x^2".into(),
            bounds: None,
        }
    );
    assert_eq!(
        classify("∫ sin(t) dt").unwrap().intent,
        CasIntent::Integral {
            variable: "t".into(),
            expr: "sin(t)".into(),
            bounds: None,
        }
    );
    assert_eq!(
        classify("integrate(x^2, x)").unwrap().intent,
        CasIntent::Integral {
            variable: "x".into(),
            expr: "x^2".into(),
            bounds: None,
        }
    );
    assert_eq!(
        classify("integrate(x, x, 0, 1)").unwrap().intent,
        CasIntent::Integral {
            variable: "x".into(),
            expr: "x".into(),
            bounds: Some(("0".into(), "1".into())),
        }
    );
}

#[test]
fn integral_reports_wrong_arity_and_missing_differential() {
    assert_eq!(
        classify("integrate(x)").unwrap_err(),
        CasIntentError::BadArity
    );
    assert_eq!(
        classify("integrate(x, x, 0)").unwrap_err(),
        CasIntentError::BadArity
    );
    assert!(matches!(
        classify("∫ x^2").unwrap_err(),
        CasIntentError::Unknown(detail) if detail.contains("differential")
    ));
    assert_eq!(
        classify("integrate(x, 2x)").unwrap_err(),
        CasIntentError::BadVariable("2x".into())
    );
}

#[test]
fn solve_accepts_prefix_and_variable_forms() {
    assert_eq!(
        classify("solve x^2 - 4").unwrap().intent,
        CasIntent::Solve {
            expr: "x^2 - 4".into(),
            variable: None,
        }
    );
    assert_eq!(
        classify("solve(x^2 - 4, x)").unwrap().intent,
        CasIntent::Solve {
            expr: "x^2 - 4".into(),
            variable: Some("x".into()),
        }
    );
    assert_eq!(
        classify("solve(x^2 - 4)").unwrap().intent,
        CasIntent::Solve {
            expr: "(x^2 - 4)".into(),
            variable: None,
        }
    );
}

#[test]
fn simplify_accepts_a_prefix_expression() {
    assert_eq!(
        classify("simplify (x + 0) * 1").unwrap().intent,
        CasIntent::Simplify {
            expr: "(x + 0) * 1".into(),
        }
    );
    assert_eq!(
        classify("simplify").unwrap_err(),
        CasIntentError::MissingExpression
    );
}

#[test]
fn evaluate_splits_trailing_assignments() {
    let parse = classify("x^2 + y at x=1, y=2").unwrap();
    assert_eq!(parse.normalized, "x^2 + y at x=1, y=2");
    assert_eq!(
        parse.intent,
        CasIntent::Evaluate {
            expr: "x^2 + y".into(),
            assignments: vec![("x".into(), "1".into()), ("y".into(), "2".into())],
        }
    );
}

#[test]
fn evaluate_caps_assignments_at_eight_and_validates_names() {
    let eight = "x at a=1, b=2, c=3, d=4, e=5, f=6, g=7, h=8";
    let CasIntent::Evaluate { assignments, .. } = classify(eight).unwrap().intent else {
        panic!("expected evaluate");
    };
    assert_eq!(assignments.len(), MAX_ASSIGNMENTS);
    assert_eq!(assignments.len(), 8);

    let nine = "x at a=1, b=2, c=3, d=4, e=5, f=6, g=7, h=8, i=9";
    assert_eq!(classify(nine).unwrap_err(), CasIntentError::BadArity);
    assert!(matches!(
        classify("x at 2y=1").unwrap_err(),
        CasIntentError::BadVariable(name) if name == "2y"
    ));
    assert!(matches!(
        classify("x at y").unwrap_err(),
        CasIntentError::Unknown(detail) if detail.contains("assignment")
    ));
}

#[test]
fn leading_operators_bind_before_the_evaluate_suffix() {
    assert_eq!(
        classify("grad x at y=1").unwrap().intent,
        CasIntent::Gradient {
            expr: "x at y=1".into(),
        }
    );
}

#[test]
fn unknown_text_is_plain_after_normalisation() {
    let parse = classify("sin(x) + cos(y)").unwrap();
    assert_eq!(
        parse.intent,
        CasIntent::Plain {
            expr: "sin(x) + cos(y)".into(),
        }
    );
    assert_eq!(parse.normalized, "sin(x) + cos(y)");
}

#[test]
fn latex_lite_rewrites_fraction_sqrt_and_commands() {
    assert_eq!(normalize("\\frac{a}{b}"), "(a)/(b)");
    assert_eq!(normalize("\\frac{1}{2} + \\sqrt{x}"), "(1)/(2) + sqrt(x)");
    assert_eq!(normalize("\\sqrt{\\frac{a}{b}}"), "sqrt((a)/(b))");
    assert_eq!(normalize("x \\cdot y \\times z"), "x * y × z");
    assert_eq!(normalize("\\partial f / \\partial x"), "d f / d x");
    assert_eq!(normalize("\\nabla f"), "∇ f");
    assert_eq!(normalize("\\left(x\\right)"), "(x)");
}

#[test]
fn unicode_operators_and_superscripts_normalise() {
    assert_eq!(normalize("x−y∗z·w"), "x-y*z*w");
    assert_eq!(normalize("x^{2} + y² + zⁿ"), "x^2 + y^2 + z^n");
    assert_eq!(normalize("x · y"), "x * y");
}

#[test]
fn whitespace_collapses_to_single_spaces() {
    assert_eq!(normalize("  grad\t x \n +   y "), "grad x + y");
    assert_eq!(normalize("   "), "");
}

#[test]
fn normalisation_is_idempotent() {
    let samples = [
        "  grad\t x \n +   y ",
        "\\frac{1}{2} + \\sqrt{x}",
        "\\sqrt{\\frac{a}{b}}",
        "x \\cdot y \\times z",
        "x^{2} + y² + zⁿ",
        "x−y∗z·w",
        "\\partial f / \\partial x",
        "\\left(x\\right)",
        "∇\\cdot F",
        "∫ x^2 dx",
        "x^2 + y at x=1, y=2",
        "",
    ];
    for sample in samples {
        let once = normalize(sample);
        assert_eq!(normalize(&once), once, "{sample:?}");
    }
}

#[test]
fn deeply_nested_fractions_do_not_overflow_normalize() {
    // `normalize` is public and callers are not required to pre-check bounds.
    // Recurring once per `\frac`/`\sqrt` level must stay bounded, and whatever
    // the depth cutoff emits must remain idempotent.
    let depth = 100_000;
    let input = format!("{}x{}", "\\frac{".repeat(depth), "}{x}".repeat(depth));
    let once = normalize(&input);
    assert_eq!(normalize(&once), once);
}

#[test]
fn empty_input_is_rejected() {
    assert_eq!(classify("").unwrap_err(), CasIntentError::Empty);
    assert_eq!(classify(" \t\n ").unwrap_err(), CasIntentError::Empty);
}

#[test]
fn oversized_input_is_rejected_by_utf8_bytes() {
    let ascii = "x".repeat(MAX_INPUT_BYTES + 1);
    assert_eq!(classify(&ascii).unwrap_err(), CasIntentError::TooLong);

    // 257 two-byte characters exceed the byte limit without exceeding the
    // character count, which must not slice inside a multibyte sequence.
    let multibyte = "é".repeat(MAX_INPUT_BYTES / 2 + 1);
    assert_eq!(classify(&multibyte).unwrap_err(), CasIntentError::TooLong);
    assert!(multibyte.len() > MAX_INPUT_BYTES);
}

#[test]
fn nesting_over_the_limit_is_rejected() {
    let input = format!(
        "{}x{}",
        "(".repeat(MAX_NESTING + 1),
        ")".repeat(MAX_NESTING + 1)
    );
    assert_eq!(classify(&input).unwrap_err(), CasIntentError::TooDeep);

    let at_limit = format!("{}x{}", "(".repeat(MAX_NESTING), ")".repeat(MAX_NESTING));
    assert!(matches!(
        classify(&at_limit).unwrap().intent,
        CasIntent::Plain { .. }
    ));
}

#[test]
fn unbalanced_delimiters_are_rejected() {
    for input in ["(x", "x)", "((x)", "\\frac{x}{y", "x^{2"] {
        assert_eq!(
            classify(input).unwrap_err(),
            CasIntentError::UnbalancedDelimiters,
            "{input}"
        );
    }
}

#[test]
fn malformed_recognised_calls_do_not_fall_back_to_plain() {
    assert!(matches!(
        classify("diff(x)").unwrap_err(),
        CasIntentError::BadArity
    ));
    assert!(matches!(
        classify("diff(x, x) trailing").unwrap_err(),
        CasIntentError::Unknown(_)
    ));
    assert!(matches!(
        classify("integrate x, x").unwrap_err(),
        CasIntentError::Unknown(_)
    ));
}

#[test]
fn multibyte_and_adversarial_input_never_panics() {
    let inputs = [
        "\\frac{é}{ü}".to_owned(),
        "∂²∇·×∫−∗·ⁿ".to_owned(),
        "∇·é".to_owned(),
        "∫ é dü".to_owned(),
        "ddx ²".to_owned(),
        "é".repeat(MAX_INPUT_BYTES / 2),
        "\\sqrt{".to_owned() + &"é".repeat(MAX_INPUT_BYTES / 2 - 8),
    ];
    for input in inputs {
        let _ = classify(&input);
        let _ = normalize(&input);
    }
    // Concrete expectations alongside the never-panic sweep: multibyte text
    // stays in the classified expression and in the reported variable.
    assert_eq!(
        classify("∇·é").unwrap().intent,
        CasIntent::Divergence { expr: "é".into() }
    );
    assert_eq!(
        classify("∫ é dü").unwrap_err(),
        CasIntentError::BadVariable("ü".into())
    );
}

#[test]
fn operation_labels_cover_every_intent() {
    let intents = [
        CasIntent::Derivative {
            variable: "x".into(),
            expr: "x".into(),
        },
        CasIntent::Gradient { expr: "x".into() },
        CasIntent::Divergence { expr: "x".into() },
        CasIntent::Curl { expr: "x".into() },
        CasIntent::Integral {
            variable: "x".into(),
            expr: "x".into(),
            bounds: None,
        },
        CasIntent::Solve {
            expr: "x".into(),
            variable: None,
        },
        CasIntent::Simplify { expr: "x".into() },
        CasIntent::Evaluate {
            expr: "x".into(),
            assignments: Vec::new(),
        },
        CasIntent::Plain { expr: "x".into() },
    ];
    for intent in intents {
        assert!(!intent.operation_label().is_empty());
    }
}
