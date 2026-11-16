use glam::{UVec2, Vec3};
use vector_field_explorer::{
    domain::Domain,
    plot::{curve_edges_at, sample_curve, sample_curve_at, sample_surface, sample_surface_at},
    scalar::{ScalarContext, ScalarExpr},
};

fn domain() -> Domain {
    Domain::new(Vec3::new(-1.0, -1.0, 0.0), Vec3::new(1.0, 1.0, 0.0))
}

fn close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 1e-6,
        "expected {expected}, got {actual}"
    );
}

#[test]
fn scalar_expr_tracks_time_only_when_it_uses_t() {
    let curve = ScalarExpr::parse("sin(t)*x", ScalarContext::Curve).unwrap();
    assert!(curve.is_time_dependent());
    close(
        curve
            .eval_at(1.0, 0.0, std::f64::consts::FRAC_PI_2)
            .unwrap() as f32,
        1.0,
    );
    assert_eq!(curve.eval(1.0, 0.0), Some(0.0));

    let still = ScalarExpr::parse("x^2", ScalarContext::Curve).unwrap();
    assert!(!still.is_time_dependent());
    assert_eq!(still.eval_at(2.0, 0.0, 9.0), still.eval(2.0, 0.0));

    let surface = ScalarExpr::parse("x+y+t", ScalarContext::Surface).unwrap();
    assert!(surface.is_time_dependent());
    assert_eq!(surface.eval_at(1.0, 2.0, 3.0), Some(6.0));
}

#[test]
fn curve_samples_move_with_time_and_static_samples_do_not() {
    let domain = domain();
    let expr = ScalarExpr::parse("sin(t)*x", ScalarContext::Curve).unwrap();
    let at_zero = sample_curve_at(&expr, &domain, 2, 0.0);
    let at_peak = sample_curve_at(&expr, &domain, 2, std::f32::consts::FRAC_PI_2);
    close(at_zero[0].unwrap().y, 0.0);
    close(at_zero[1].unwrap().y, 0.0);
    close(at_peak[0].unwrap().y, -1.0);
    close(at_peak[1].unwrap().y, 1.0);

    let stationary = ScalarExpr::parse("x", ScalarContext::Curve).unwrap();
    let first = sample_curve_at(&stationary, &domain, 5, 0.0);
    let later = sample_curve_at(&stationary, &domain, 5, 7.0);
    assert_eq!(first, later);
    assert_eq!(sample_curve(&stationary, &domain, 5), first);
}

#[test]
fn surface_samples_move_with_time_and_static_samples_do_not() {
    let domain = domain();
    let expr = ScalarExpr::parse("t", ScalarContext::Surface).unwrap();
    let at_zero = sample_surface_at(&expr, &domain, UVec2::splat(2), 0.0);
    let at_one = sample_surface_at(&expr, &domain, UVec2::splat(2), 1.0);
    assert_eq!(at_zero.positions.len(), 4);
    for point in &at_zero.positions {
        close(point.z, 0.0);
    }
    for point in &at_one.positions {
        close(point.z, 1.0);
    }

    let stationary = ScalarExpr::parse("x+y", ScalarContext::Surface).unwrap();
    assert_eq!(
        sample_surface(&stationary, &domain, UVec2::splat(3)),
        sample_surface_at(&stationary, &domain, UVec2::splat(3), 4.0)
    );
}

#[test]
fn curve_edge_probes_use_the_same_time() {
    let domain = domain();
    let expr = ScalarExpr::parse("1/(x-t)", ScalarContext::Curve).unwrap();
    let slots = vec![
        Some(Vec3::new(-1.0, 0.0, 0.0)),
        Some(Vec3::new(0.0, 0.0, 0.0)),
        Some(Vec3::new(1.0, 0.0, 0.0)),
    ];
    let at_zero = curve_edges_at(&expr, &domain, &slots, 0.0);
    let at_half = curve_edges_at(&expr, &domain, &slots, 0.5);
    assert_eq!(at_zero.diagnostics.rejected_edges, 0);
    assert!(
        at_half.diagnostics.rejected_edges > 0,
        "the midpoint probe at x = 0.5 must see the pole when t = 0.5"
    );
}
