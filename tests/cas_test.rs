use meval::{Context, Expr};
use vector_field_explorer::cas::{self, Variable};

fn eval(formula: &str, x: f64, y: f64, z: f64, t: f64) -> f64 {
    let mut context = Context::new();
    context.var("x", x).var("y", y).var("z", z).var("t", t);
    formula
        .parse::<Expr>()
        .unwrap()
        .eval_with_context(context)
        .unwrap()
}

fn derivative(formula: &str, variable: Variable, point: [f64; 4]) -> f64 {
    let result = cas::differentiate(formula, variable).unwrap();
    eval(&result, point[0], point[1], point[2], point[3])
}

#[test]
fn supported_unary_functions_differentiate_with_meval_semantics() {
    let cases = [
        ("sin(x)", 0.4),
        ("cos(x)", 0.4),
        ("tan(x)", 0.4),
        ("asin(x)", 0.4),
        ("acos(x)", 0.4),
        ("atan(x)", 0.4),
        ("sinh(x)", 0.4),
        ("cosh(x)", 0.4),
        ("tanh(x)", 0.4),
        ("asinh(x)", 0.4),
        ("acosh(x)", 2.0),
        ("atanh(x)", 0.4),
        ("sqrt(x)", 2.0),
        ("exp(x)", 0.4),
        ("ln(x)", 2.0),
    ];
    for (formula, x) in cases {
        let h = 1e-5;
        let numeric =
            (eval(formula, x + h, 0.3, 0.2, 0.1) - eval(formula, x - h, 0.3, 0.2, 0.1)) / (2.0 * h);
        let result = cas::differentiate(formula, Variable::X).unwrap();
        let symbolic = eval(&result, x, 0.3, 0.2, 0.1);
        assert!(
            (symbolic - numeric).abs() < 2e-5,
            "{formula}: {symbolic} != {numeric}"
        );
    }
}

#[test]
fn variables_constants_precedence_and_nested_expression_meaning_are_preserved() {
    let cases = [
        ("-x^2", Variable::X, [0.8, 1.1, 1.3, 0.2]),
        ("(-x)^2", Variable::X, [0.8, 1.1, 1.3, 0.2]),
        ("x/y*z", Variable::X, [1.2, 2.1, 0.7, 0.2]),
        ("x*y/z", Variable::Y, [1.2, 2.1, 0.7, 0.2]),
        ("x-y+z", Variable::Y, [1.2, 2.1, 0.7, 0.2]),
        ("x^y^z", Variable::Y, [1.2, 1.1, 2.0, 0.2]),
        ("(x^y)^z", Variable::Y, [1.2, 1.1, 2.0, 0.2]),
        ("x^(-y)^z", Variable::Y, [1.2, 0.7, 2.0, 0.2]),
        ("sin(-x+cos(y))", Variable::X, [0.8, 0.3, 0.2, 0.1]),
        ("pi*x+e*y", Variable::X, [0.8, 0.3, 0.2, 0.1]),
    ];
    for (formula, variable, point) in cases {
        let value = derivative(formula, variable, point);
        let step = 1e-5;
        let idx = match variable {
            Variable::X => 0,
            Variable::Y => 1,
            Variable::Z => 2,
            Variable::T => 3,
        };
        let mut a = point;
        let mut b = point;
        a[idx] -= step;
        b[idx] += step;
        let numeric = (eval(formula, b[0], b[1], b[2], b[3])
            - eval(formula, a[0], a[1], a[2], a[3]))
            / (2.0 * step);
        assert!(
            (value - numeric).abs() < 5e-4,
            "{formula}: {value} != {numeric}"
        );
    }
    for variable in [Variable::X, Variable::Y, Variable::Z, Variable::T] {
        assert!(!cas::differentiate("x+y+z+t", variable).unwrap().is_empty());
    }
    assert!(
        cas::differentiate("x", Variable::T)
            .unwrap()
            .parse::<Expr>()
            .is_ok()
    );
}

#[test]
fn vector_operators_have_declared_order_sign_and_identities() {
    let point = [0.4, 0.7, 0.9, 0.0];
    let gradient = cas::gradient("x^2+y^2+z^2", 3).unwrap();
    assert_eq!(gradient.len(), 3);
    for (component, expected) in gradient.iter().zip([0.8, 1.4, 1.8]) {
        assert!((eval(component, point[0], point[1], point[2], point[3]) - expected).abs() < 1e-10);
    }
    let curl2 = cas::curl(&["-y", "x"], 2).unwrap();
    assert_eq!(curl2.len(), 1);
    assert!((eval(&curl2[0], 0.2, 0.3, 0.4, 0.5) - 2.0).abs() < 1e-10);
    let nontrivial = cas::curl(&["x*y", "y*z", "z*x"], 3).unwrap();
    for (component, expected) in nontrivial.iter().zip([-point[1], -point[2], -point[0]]) {
        assert!((eval(component, point[0], point[1], point[2], point[3]) - expected).abs() < 1e-10);
    }
    assert!(
        (eval(
            &cas::divergence(&["x*y", "y*z", "z*x"], 3).unwrap(),
            point[0],
            point[1],
            point[2],
            point[3]
        ) - (point[0] + point[1] + point[2]))
            .abs()
            < 1e-10
    );
    let f = "x^2*y + y^2*z + z^2*x";
    let grad = cas::gradient(f, 3).unwrap();
    let grad_refs = [grad[0].as_str(), grad[1].as_str(), grad[2].as_str()];
    for component in cas::curl(&grad_refs, 3).unwrap() {
        assert!(eval(&component, 0.4, 0.7, 0.9, 0.0).abs() < 1e-9);
    }
    let field = ["x*y*z", "x^2+y*z", "y^2+x*z"];
    let curl = cas::curl(&field, 3).unwrap();
    let refs: Vec<&str> = curl.iter().map(String::as_str).collect();
    assert!(eval(&cas::divergence(&refs, 3).unwrap(), 0.4, 0.7, 0.9, 0.0).abs() < 1e-9);
}

#[test]
fn unsupported_tokens_are_rejected_before_absent_variable_shortcuts() {
    for formula in [
        "x%2",
        "abs(x)",
        "abs(2)",
        "floor(x)",
        "floor(2)",
        "ceil(x)",
        "ceil(2)",
        "round(x)",
        "round(2)",
        "signum(x)",
        "signum(2)",
        "atan2(x,y)",
        "atan2(2,3)",
        "max(x,y)",
        "max(2,3)",
        "min(x,y)",
        "min(2,3)",
        "mystery(x)",
        "unknown+x",
        "sqrt(x,y)",
        "sin(x,y)",
        "log(x)",
        "pi+abs(2)",
        "pi%2",
        "{x}+1",
        "NaN",
        "Infinity",
        "1e999",
    ] {
        assert!(
            cas::differentiate(formula, Variable::T).is_err(),
            "accepted {formula}"
        );
    }
}

#[test]
fn non_finite_folded_results_are_rejected_without_origin_domain_probes() {
    let finite_large = cas::differentiate("1e308*x", Variable::X).unwrap();
    assert_eq!(eval(&finite_large, 0.0, 0.0, 0.0, 0.0), 1e308);
    let overflow = cas::differentiate("1e308^2*x", Variable::X).unwrap_err();
    assert!(overflow.to_string().contains("non-finite"), "{overflow}");
    assert!(cas::differentiate("0/0*x", Variable::X).is_err());
    // A valid formula may be undefined at the origin; binding is a context check,
    // not an evaluation-based global-domain test.
    let reciprocal = cas::differentiate("ln(x)", Variable::X).unwrap();
    assert!(eval(&reciprocal, 2.0, 0.0, 0.0, 0.0).is_finite());
}

#[test]
fn every_coordinate_and_time_derivative_matches_meval_finite_differences() {
    let formula = "x^2 + 3*y - z*t + sin(t)";
    let point = [0.6, -0.8, 1.2, 0.35];
    for (variable, index) in [
        (Variable::X, 0),
        (Variable::Y, 1),
        (Variable::Z, 2),
        (Variable::T, 3),
    ] {
        let symbolic = derivative(formula, variable, point);
        let h = 1e-5;
        let mut low = point;
        let mut high = point;
        low[index] -= h;
        high[index] += h;
        let numeric = (eval(formula, high[0], high[1], high[2], high[3])
            - eval(formula, low[0], low[1], low[2], low[3]))
            / (2.0 * h);
        assert!(
            (symbolic - numeric).abs() < 1e-5,
            "{variable:?}: {symbolic} != {numeric}"
        );
    }
    assert!(
        cas::differentiate("x^2", Variable::Y)
            .unwrap()
            .parse::<Expr>()
            .is_ok()
    );
    assert_eq!(cas::differentiate("pi+e", Variable::T).unwrap(), "0");
    assert_eq!(cas::differentiate("x", Variable::T).unwrap(), "0");
}

#[test]
fn negative_integer_powers_and_nested_output_operations_retain_values() {
    let x3 = derivative("x^3", Variable::X, [-2.0, 0.4, 0.3, 0.2]);
    assert!(x3.is_finite());
    assert!((x3 - 12.0).abs() < 1e-10);

    let nested_power = derivative("(x^y)^z", Variable::Y, [1.5, 2.0, 3.0, 0.2]);
    let nested_power_numeric = {
        let h = 1e-5;
        (eval("(x^y)^z", 1.5, 2.0 + h, 3.0, 0.2) - eval("(x^y)^z", 1.5, 2.0 - h, 3.0, 0.2))
            / (2.0 * h)
    };
    assert!(nested_power.is_finite());
    assert!((nested_power - nested_power_numeric).abs() < 1e-5);

    let nested_division = derivative("x/(y/z)", Variable::X, [1.0, 2.0, 3.0, 0.0]);
    assert!(nested_division.is_finite());
    assert!((nested_division - 1.5).abs() < 1e-10);

    let negative_base = derivative("(-2)^3*x", Variable::X, [0.7, 0.2, 0.4, 0.0]);
    assert!(negative_base.is_finite());
    assert!((negative_base + 8.0).abs() < 1e-10);

    let nested_subtraction = derivative("ln(x-y-z)", Variable::X, [4.0, 1.0, 1.0, 0.0]);
    assert!(nested_subtraction.is_finite());
    assert!((nested_subtraction - 0.5).abs() < 1e-10);
}

#[test]
fn finite_decimal_scientific_and_subnormal_literals_cross_the_engine_boundary() {
    let cases = [
        ("0.000001*x", 1e-6),
        ("1e-6*x", 1e-6),
        ("10000000000000000*x", 1e16),
        ("1e16*x", 1e16),
        ("1e308*x", 1e308),
        ("5e-324*x", 5e-324),
    ];
    for (source, expected) in cases {
        let derivative = cas::differentiate(source, Variable::X).unwrap();
        let actual = eval(&derivative, 0.0, 0.0, 0.0, 0.0);
        assert!(actual.is_finite(), "{source}: {actual}");
        assert_eq!(actual, expected, "{source}: {derivative}");
    }

    let gradient = cas::gradient("0.000001*x+y", 2).unwrap();
    assert_eq!(eval(&gradient[0], 0.0, 0.0, 0.0, 0.0), 1e-6);
    assert_eq!(eval(&gradient[1], 0.0, 0.0, 0.0, 0.0), 1.0);
    let divergence = cas::divergence(&["0.000001*x", "y"], 2).unwrap();
    assert_eq!(eval(&divergence, 0.0, 0.0, 0.0, 0.0), 1.000001);
    let curl = cas::curl(&["y", "0.000001*x"], 2).unwrap();
    assert_eq!(eval(&curl[0], 0.0, 0.0, 0.0, 0.0), 1e-6 - 1.0);
}

#[test]
fn engine_output_precedence_is_reconstructed_before_meval_emission() {
    // exmex gives -, /, *, + distinct priorities; the adapter must parse that
    // printed language before emitting meval's different operator grammar.
    for (formula, variable, point) in [
        ("ln(x-y+z)", Variable::X, [3.0, 1.0, 1.0, 0.0]),
        ("ln(x/y*z)", Variable::X, [2.0, 3.0, 4.0, 0.0]),
        ("ln(x*y/z)", Variable::X, [2.0, 3.0, 4.0, 0.0]),
    ] {
        let value = derivative(formula, variable, point);
        let h = 1e-5;
        let mut low = point;
        let mut high = point;
        low[0] -= h;
        high[0] += h;
        let numeric = (eval(formula, high[0], high[1], high[2], high[3])
            - eval(formula, low[0], low[1], low[2], low[3]))
            / (2.0 * h);
        assert!(
            (value - numeric).abs() < 1e-6,
            "{formula}: {value} != {numeric}"
        );
    }
}

#[test]
fn dimensions_and_component_arities_are_checked() {
    assert!(cas::gradient("x", 1).is_err());
    assert!(cas::gradient("x", 4).is_err());
    assert!(cas::divergence(&["x"], 2).is_err());
    assert!(cas::curl(&["x"], 2).is_err());
    assert!(cas::divergence(&["x", "y", "z"], 2).is_err());
    assert!(cas::divergence(&["x"], 1).is_err());
    assert!(cas::divergence(&["x", "y"], 4).is_err());
    assert!(cas::curl(&["x", "y"], 3).is_err());
    assert!(cas::curl(&["x", "y", "z"], 4).is_err());
    assert!(cas::gradient("x,y", 2).is_err());
}

#[test]
fn outputs_are_deterministic_and_meval_bindable() {
    let first = cas::differentiate("sin(x*y)/(z-t)", Variable::Y).unwrap();
    assert_eq!(
        first,
        cas::differentiate("sin(x*y)/(z-t)", Variable::Y).unwrap()
    );
    let mut context = Context::new();
    context
        .var("x", 1.0)
        .var("y", 1.0)
        .var("z", 1.0)
        .var("t", 1.0);
    first
        .parse::<Expr>()
        .unwrap()
        .eval_with_context(context)
        .unwrap();
}
