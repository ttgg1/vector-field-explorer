use std::collections::HashSet;
use vector_field_explorer::{
    plot::PlotKind,
    plot_examples::EXAMPLES,
    scalar::{ScalarContext, ScalarExpr},
};

#[test]
fn catalogue_is_exact_and_unique() {
    use PlotKind::{Curve2D, Surface3D};
    let expected = [
        (Curve2D, "Harmonic oscillator potential", "0.5*x^2"),
        (Curve2D, "Gaussian wave packet", "exp(-x^2)*cos(8*x)"),
        (Curve2D, "Damped oscillator", "exp(-0.2*abs(x))*sin(3*x)"),
        (Surface3D, "Point-charge potential", "1/(sqrt(x^2+y^2)+0.1)"),
        (
            Surface3D,
            "Two like charges",
            "1/(sqrt((x-0.6)^2+y^2)+0.1) + 1/(sqrt((x+0.6)^2+y^2)+0.1)",
        ),
        (
            Surface3D,
            "Quantum oscillator ground state",
            "exp(-(x^2+y^2))",
        ),
        (Surface3D, "Standing wave", "sin(3*x)*cos(3*y)"),
        (
            Surface3D,
            "Radial outgoing wave",
            "sin(6*sqrt(x^2+y^2))/(1+sqrt(x^2+y^2))",
        ),
    ];
    assert_eq!(EXAMPLES.len(), 8);
    assert_eq!(PlotKind::default(), PlotKind::VectorField);
    let mut names = HashSet::new();
    for (example, (kind, name, source)) in EXAMPLES.iter().zip(expected) {
        assert_eq!(
            (example.kind, example.name, example.source),
            (kind, name, source)
        );
        assert!(names.insert(example.name));
        let context = if kind == Curve2D {
            ScalarContext::Curve
        } else {
            ScalarContext::Surface
        };
        let expr = ScalarExpr::parse(example.source, context).unwrap();
        for x in [-1.0, -0.5, 0.0, 0.5, 1.0] {
            for y in [-1.0, 0.0, 1.0] {
                assert!(expr.eval(x, y).unwrap().is_finite(), "{name}");
            }
        }
        let encoded = serde_json::to_string(&kind).unwrap();
        assert_eq!(serde_json::from_str::<PlotKind>(&encoded).unwrap(), kind);
    }
}
