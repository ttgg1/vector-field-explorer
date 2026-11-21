//! Bounded linear-to-TeX display printer (issue #171).

use vector_field_explorer::cas_tex::linear_to_tex;

#[test]
fn linear_to_tex_handles_the_supported_subset() {
    for (linear, tex) in [
        ("2*x+1", r"2\cdot x+1"),
        ("x*y", r"x\cdot y"),
        ("x/y", "x/y"),
        ("sin(x)", r"\sin(x)"),
        ("cos(x)", r"\cos(x)"),
        ("tan(x)", r"\tan(x)"),
        ("asin(x)", r"\arcsin(x)"),
        ("acos(x)", r"\arccos(x)"),
        ("atan(x)", r"\arctan(x)"),
        ("sinh(x)", r"\sinh(x)"),
        ("cosh(x)", r"\cosh(x)"),
        ("tanh(x)", r"\tanh(x)"),
        ("sqrt(x)", r"\sqrt{x}"),
        ("exp(-t)", r"\exp(-t)"),
        ("ln(x)", r"\ln(x)"),
        ("log(x)", r"\log(x)"),
        ("abs(x)", r"\left|x\right|"),
        ("x^2", "x^{2}"),
        ("(x+1)^2", "(x+1)^{2}"),
        ("x^(2)", "x^{2}"),
        ("x^-1", "x^{-1}"),
        ("pi", r"\pi"),
        ("e", "e"),
        ("(x^2+1)/(x-3)", r"\frac{x^{2}+1}{x-3}"),
        ("sin(x)*exp(-t)", r"\sin(x)\cdot \exp(-t)"),
        ("(x)", "(x)"),
        ("x + y", "x+y"),
    ] {
        assert_eq!(
            linear_to_tex(linear).as_deref(),
            Some(tex),
            "printer output for {linear:?}"
        );
    }
}

#[test]
fn linear_to_tex_rejects_unsupported_input() {
    let oversized = "x".repeat(513);
    for unsupported in [
        "x=1",
        "x|y",
        "[x]",
        "{x}",
        "x,y",
        "foo(x)",
        "",
        "   ",
        "x%2",
        "x;y",
        "x@y",
        "sin",
        "pi2",
        "()",
        "sqrt()",
        "x^",
        "x+",
        oversized.as_str(),
    ] {
        assert_eq!(
            linear_to_tex(unsupported),
            None,
            "printer must reject {unsupported:?}"
        );
    }
}
