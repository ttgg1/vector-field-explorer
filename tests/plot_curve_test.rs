use glam::Vec3;
use vector_field_explorer::{
    Domain,
    plot::{curve_edges, sample_curve, sample_curve_with_diagnostics},
    scalar::{ScalarContext, ScalarExpr},
};

fn expr(source: &str) -> ScalarExpr {
    ScalarExpr::parse(source, ScalarContext::Curve).unwrap()
}

#[test]
fn curve_slots_endpoints_and_caps() {
    let domain = Domain::default();
    let f = expr("x^2+10");
    for (requested, expected) in [(0, 0), (1, 1), (2, 2), (513, 513), (usize::MAX, 4097)] {
        let slots = sample_curve(&f, &domain, requested);
        assert_eq!(slots.len(), expected);
        if expected == 1 {
            assert_eq!(slots, vec![Some(Vec3::new(0.0, 10.0, 0.0))]);
        } else if expected > 1 {
            assert_eq!(slots[0], Some(Vec3::new(-1.0, 11.0, 0.0)));
            assert_eq!(slots[expected - 1], Some(Vec3::new(1.0, 11.0, 0.0)));
        }
    }
    let huge = Domain::new(Vec3::splat(-f32::MAX), Vec3::splat(f32::MAX));
    assert_eq!(
        sample_curve(&expr("0"), &huge, 3),
        vec![
            Some(Vec3::new(-f32::MAX, 0.0, 0.0)),
            Some(Vec3::ZERO),
            Some(Vec3::new(f32::MAX, 0.0, 0.0))
        ]
    );
    let ignored_z = Domain::new(
        Vec3::new(-1.0, -1.0, f32::NAN),
        Vec3::new(1.0, 1.0, f32::NEG_INFINITY),
    );
    assert_eq!(
        sample_curve(&f, &ignored_z, 2),
        sample_curve(&f, &domain, 2)
    );
    // Cancellation in lo + (hi-lo) must not lose the inclusive upper endpoint.
    let asymmetric = Domain::new(Vec3::new(-f32::MAX, -1.0, 0.0), Vec3::ONE);
    assert_eq!(
        sample_curve(&expr("0"), &asymmetric, 2)[1],
        Some(Vec3::new(1.0, 0.0, 0.0))
    );
    for invalid in [
        Domain::new(Vec3::new(2.0, -1.0, 0.0), Vec3::ONE),
        Domain::new(Vec3::new(-1.0, 2.0, 0.0), Vec3::ONE),
        Domain::new(Vec3::new(f32::NAN, -1.0, 0.0), Vec3::ONE),
        Domain::new(Vec3::NEG_ONE, Vec3::new(1.0, f32::INFINITY, 1.0)),
    ] {
        let (slots, diag) = sample_curve_with_diagnostics(&f, &invalid, 3);
        assert_eq!(slots, vec![None; 3]);
        assert!(diag.invalid_domain);
        let edges = curve_edges(&f, &invalid, &[Some(Vec3::ZERO), Some(Vec3::ONE)]);
        assert!(edges.segments.is_empty());
        assert!(edges.diagnostics.invalid_domain);
    }
}

#[test]
fn curve_missing_and_unrepresentable_slots() {
    let domain = Domain::default();
    let (slots, diag) = sample_curve_with_diagnostics(&expr("sqrt(x)"), &domain, 3);
    assert_eq!(
        slots,
        vec![None, Some(Vec3::ZERO), Some(Vec3::new(1.0, 1.0, 0.0))]
    );
    assert_eq!(diag.invalid_samples, 1);
    assert_eq!(diag.unrepresentable_samples, 0);
    let slots = sample_curve(&expr("1/x"), &domain, 3);
    assert!(slots[1].is_none());
    assert!(
        curve_edges(&expr("1/x"), &domain, &slots)
            .segments
            .is_empty()
    );
    let f = expr("1e100");
    assert_eq!(f.eval(0.0, 0.0), Some(1e100));
    let (slots, diag) = sample_curve_with_diagnostics(&f, &domain, 3);
    assert_eq!(slots, vec![None; 3]);
    assert_eq!(diag.unrepresentable_samples, 3);
    assert_eq!(diag.invalid_samples, 0);
    for slots in [
        vec![Some(Vec3::ZERO), Some(Vec3::ZERO)],
        vec![Some(Vec3::ZERO), Some(Vec3::NAN)],
        vec![Some(Vec3::ZERO), None, Some(Vec3::ONE)],
    ] {
        assert!(curve_edges(&expr("x"), &domain, &slots).segments.is_empty());
    }
    let slots = sample_curve(&expr("sqrt(x)"), &domain, 5);
    assert_eq!(
        curve_edges(&expr("sqrt(x)"), &domain, &slots)
            .segments
            .len(),
        2
    );
}

#[test]
fn curve_edges_do_not_bridge_poles() {
    let domain = Domain::new(Vec3::new(0.0, -1.0, 0.0), Vec3::new(0.5, 1.0, 0.0));
    for source in ["1/(x-0.25)", "1/(x-0.3)"] {
        let f = expr(source);
        let slots = sample_curve(&f, &domain, 2);
        assert!(slots.iter().all(Option::is_some));
        let before = slots.clone();
        let edges = curve_edges(&f, &domain, &slots);
        assert!(edges.segments.is_empty(), "{source}");
        assert_eq!(edges.diagnostics.rejected_edges, 1);
        assert!((1..=127).contains(&edges.diagnostics.probes));
        assert_eq!(slots, before);
    }
    let slots = sample_curve(&expr("x"), &domain, 3);
    let edges = curve_edges(&expr("x"), &domain, &slots);
    assert_eq!(
        edges.segments,
        vec![
            [Vec3::ZERO, Vec3::new(0.25, 0.25, 0.0)],
            [Vec3::new(0.25, 0.25, 0.0), Vec3::new(0.5, 0.5, 0.0)]
        ]
    );
    assert_eq!(edges.diagnostics.probes, 2);
}

#[test]
fn curve_probe_work_is_bounded() {
    let domain = Domain::default();
    let f = expr("1e10*x");
    let slots = sample_curve(&f, &domain, usize::MAX);
    let edges = curve_edges(&f, &domain, &slots);
    assert!(edges.segments.is_empty());
    assert_eq!(edges.diagnostics.rejected_edges, 4096);
    assert!(edges.diagnostics.probes <= 4096 * 127);
    assert_eq!(slots.len(), 4097);
    let mut extra: Vec<_> = (0..5000)
        .map(|i| Some(Vec3::new(i as f32, 0.0, 0.0)))
        .collect();
    // The unchecked caller's tail must not contribute an edge or a probe.
    extra[4097] = Some(Vec3::NAN);
    let edges = curve_edges(&expr("0"), &domain, &extra);
    assert_eq!(edges.segments.len(), 4096);
    assert_eq!(edges.diagnostics.probes, 4096);
}

#[test]
fn curve_probe_levels_threshold_and_f64_spread() {
    let domain = Domain::new(Vec3::new(0.0, -1.0, 0.0), Vec3::ONE);
    // 1024 / 2^6 == 16: root level zero plus six subdivisions, with equality
    // accepted. Every probe is needed and only the original edge is emitted.
    let f = expr("1024*x");
    let slots = sample_curve(&f, &domain, 2);
    let edges = curve_edges(&f, &domain, &slots);
    assert_eq!(edges.diagnostics.probes, 127);
    assert_eq!(edges.segments, vec![[slots[0].unwrap(), slots[1].unwrap()]]);
    let f = expr("1025*x");
    assert!(
        curve_edges(&f, &domain, &sample_curve(&f, &domain, 2))
            .segments
            .is_empty()
    );
    let huge = Domain::new(Vec3::splat(-f32::MAX), Vec3::splat(f32::MAX));
    let f = expr("0");
    assert_eq!(
        curve_edges(&f, &huge, &sample_curve(&f, &huge, 2))
            .segments
            .len(),
        1
    );
    let huge_y = Domain::new(
        Vec3::new(-1.0, -f32::MAX, 0.0),
        Vec3::new(1.0, f32::MAX, 0.0),
    );
    let f = expr("3e38*x");
    let edges = curve_edges(&f, &huge_y, &sample_curve(&f, &huge_y, 2));
    assert_eq!(edges.segments.len(), 1);
    assert_eq!(edges.diagnostics.probes, 1);
}
