use vector_field_explorer::scalar::{ScalarContext, ScalarExpr};

#[test]
fn scalar_rhs_context_and_errors() {
    for context in [ScalarContext::Curve, ScalarContext::Surface] {
        for (source, expected) in [("2", 2.0), ("sin(pi/2)", 1.0), ("x^2", 4.0)] {
            let expr = ScalarExpr::parse(source, context).unwrap();
            assert_eq!(expr.clone().eval(2.0, 3.0), Some(expected));
            assert!(!format!("{expr:?}").is_empty());
        }
        for source in ["z", "mystery", "foo(x)", "y=x", "x+"] {
            let error = ScalarExpr::parse(source, context).unwrap_err();
            let _: &dyn std::error::Error = &error;
            let message = error.to_string();
            assert!(message.contains("allowed variables"), "{message}");
            assert!(message.contains('x'), "{message}");
            if matches!(context, ScalarContext::Surface) {
                assert!(message.contains('y'), "{message}");
            }
            for symbol in ["z", "mystery", "foo"] {
                if source.starts_with(symbol) {
                    assert!(message.contains(symbol), "{message}");
                }
            }
        }
    }
    assert_eq!(
        ScalarExpr::parse("x+y", ScalarContext::Surface)
            .unwrap()
            .eval(2.0, 3.0),
        Some(5.0)
    );
    for source in ["y", "x+y"] {
        let message = ScalarExpr::parse(source, ScalarContext::Curve)
            .unwrap_err()
            .to_string();
        assert!(message.contains('y') && message.contains("allowed variables"));
    }
}

#[test]
fn partly_undefined_is_valid_syntax() {
    let expr = ScalarExpr::parse("sqrt(x)", ScalarContext::Curve).unwrap();
    assert_eq!(expr.eval(-1.0, 0.0), None);
    assert_eq!(expr.eval(4.0, 0.0), Some(2.0));
    // Parsing must not reject an expression just because all sample points fail.
    assert_eq!(
        ScalarExpr::parse("sqrt(-1)", ScalarContext::Surface)
            .unwrap()
            .eval(0.0, 0.0),
        None
    );
    assert_eq!(
        ScalarExpr::parse("1/x", ScalarContext::Curve)
            .unwrap()
            .eval(0.0, 0.0),
        None
    );
}
