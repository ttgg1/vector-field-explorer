use std::collections::HashSet;
use vector_field_explorer::{
    domain::Domain,
    plot::{PlotKind, curve_edges, sample_curve, sample_surface},
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
        let domain = Domain::default();
        if kind == Curve2D {
            let slots = sample_curve(&expr, &domain, 513);
            assert!(slots.iter().all(Option::is_some), "{name}");
            let edges = curve_edges(&expr, &domain, &slots);
            assert!(!edges.segments.is_empty(), "{name}");
            assert!(
                edges.segments.iter().flatten().all(|p| p.is_finite()),
                "{name}"
            );
        } else {
            let mesh = sample_surface(&expr, &domain, glam::UVec2::splat(64));
            assert_eq!(mesh.positions.len(), 4096, "{name}");
            assert_eq!(mesh.indices.len(), 23814, "{name}");
            assert!(mesh.positions.iter().all(|p| p.is_finite()), "{name}");
            assert!(mesh.normals.iter().all(|n| n.is_finite()), "{name}");
            assert!(
                mesh.indices
                    .iter()
                    .all(|&i| (i as usize) < mesh.positions.len()),
                "{name}"
            );
        }
        for x in [-1.0, -0.5, 0.0, 0.5, 1.0] {
            for y in [-1.0, 0.0, 1.0] {
                assert!(expr.eval(x, y).unwrap().is_finite(), "{name}");
            }
        }
        let encoded = serde_json::to_string(&kind).unwrap();
        assert_eq!(serde_json::from_str::<PlotKind>(&encoded).unwrap(), kind);
    }
}
