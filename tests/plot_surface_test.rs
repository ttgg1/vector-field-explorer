use glam::{UVec2, Vec3};
use vector_field_explorer::{
    domain::Domain,
    plot::{SurfaceMesh, sample_surface},
    scalar::{ScalarContext, ScalarExpr},
};

fn surface(source: &str, domain: &Domain, resolution: UVec2) -> SurfaceMesh {
    let expr = ScalarExpr::parse(source, ScalarContext::Surface).unwrap();
    sample_surface(&expr, domain, resolution)
}

fn assert_sound(mesh: &SurfaceMesh) {
    assert_eq!(mesh.positions.len(), mesh.normals.len());
    assert_eq!(mesh.indices.len() % 3, 0);
    assert!(mesh.positions.len() <= 9216);
    assert!(mesh.indices.len() <= 54150);
    assert!(mesh.diagnostics.probes <= 95 * 95);
    let mut used = vec![false; mesh.positions.len()];
    for &index in &mesh.indices {
        assert!((index as usize) < mesh.positions.len());
        used[index as usize] = true;
    }
    assert!(
        used.iter().all(|used| *used),
        "no isolated or fake vertices"
    );
    for (position, normal) in mesh.positions.iter().zip(&mesh.normals) {
        assert!(position.is_finite());
        assert!(normal.is_finite());
        assert!((normal.length() - 1.0).abs() < 1e-5);
    }
    for triangle in mesh.indices.as_chunks::<3>().0 {
        let a = mesh.positions[triangle[0] as usize].as_dvec3();
        let b = mesh.positions[triangle[1] as usize].as_dvec3();
        let c = mesh.positions[triangle[2] as usize].as_dvec3();
        let face = (b - a).cross(c - a);
        assert!(face.is_finite());
        assert!(face.z > 0.0, "nondegenerate CCW graph triangle");
    }
}

#[test]
fn surface_counts_winding_and_normals() {
    let domain = Domain::default();
    let plane = surface("0", &domain, UVec2::new(3, 4));
    assert_eq!((plane.positions.len(), plane.indices.len()), (12, 36));
    assert_eq!(plane.diagnostics.probes, 6);
    assert_sound(&plane);
    assert!(plane.normals.iter().all(|n| *n == Vec3::Z));
    assert!(plane.positions.contains(&Vec3::new(-1.0, -1.0, 0.0)));
    assert!(plane.positions.contains(&Vec3::new(1.0, 1.0, 0.0)));

    let sloped = surface("2*x+3*y", &domain, UVec2::new(3, 4));
    let expected = Vec3::new(-2.0, -3.0, 1.0).normalize();
    assert_sound(&sloped);
    assert!(sloped.normals.iter().all(|n| n.distance(expected) < 1e-5));
    assert!(sloped.positions.iter().any(|p| p.z > domain.max.z));

    for resolution in [UVec2::splat(96), UVec2::splat(u32::MAX)] {
        let mesh = surface("0", &domain, resolution);
        assert_eq!((mesh.positions.len(), mesh.indices.len()), (9216, 54150));
        assert_eq!(mesh.diagnostics.probes, 9025);
        assert_sound(&mesh);
    }
}

#[test]
fn surface_missing_corner_and_center_omit_whole_cell() {
    let mesh = surface("sqrt(x)", &Domain::default(), UVec2::new(3, 3));
    assert_eq!((mesh.positions.len(), mesh.indices.len()), (6, 12));
    assert_eq!(mesh.diagnostics.invalid_samples, 3);
    assert_eq!(mesh.diagnostics.rejected_cells, 2);
    assert_eq!(mesh.diagnostics.probes, 2);
    assert!(mesh.positions.iter().all(|p| p.x >= 0.0));
    assert_sound(&mesh);

    let pole = surface("1/(x^2+y^2)", &Domain::default(), UVec2::splat(2));
    assert!(pole.positions.is_empty());
    assert!(pole.indices.is_empty());
    assert_eq!(pole.diagnostics.probes, 1);
    assert_eq!(pole.diagnostics.invalid_samples, 1);
    assert_eq!(pole.diagnostics.rejected_cells, 1);
    assert_sound(&pole);

    // A finite but GPU-unrepresentable center must also reject the whole cell.
    let overflow = surface(
        "1e39*exp(-100*(x^2+y^2))",
        &Domain::default(),
        UVec2::splat(2),
    );
    assert!(overflow.indices.is_empty());
    assert!(overflow.positions.is_empty());
    assert_eq!(overflow.diagnostics.unrepresentable_samples, 1);
    assert_eq!(overflow.diagnostics.rejected_cells, 1);
}

#[test]
fn surface_limits_degeneracy_and_domain_rejection() {
    let domain = Domain::default();
    for dims in [(0, 4), (4, 0), (1, 4), (4, 1), (1, 1)] {
        let mesh = surface("1", &domain, UVec2::new(dims.0, dims.1));
        assert!(mesh.positions.is_empty());
        assert!(mesh.indices.is_empty());
        assert!(!mesh.diagnostics.invalid_domain);
        assert_sound(&mesh);
    }
    for axis in [0, 1] {
        let mut flat = domain;
        flat.max[axis] = flat.min[axis];
        let mesh = surface("1", &flat, UVec2::splat(4));
        assert!(mesh.indices.is_empty());
        assert!(!mesh.diagnostics.invalid_domain);
        assert_sound(&mesh);
    }
    for axis in 0..3 {
        for bad in [-2.0, f32::NAN, f32::INFINITY] {
            let mut invalid = domain;
            invalid.max[axis] = bad;
            let mesh = surface("0", &invalid, UVec2::splat(4));
            assert!(mesh.diagnostics.invalid_domain);
            assert!(mesh.positions.is_empty());
            assert_sound(&mesh);
        }
        let mut invalid = domain;
        invalid.min[axis] = f32::NEG_INFINITY;
        assert!(
            surface("0", &invalid, UVec2::splat(4))
                .diagnostics
                .invalid_domain
        );
    }
    for (source, invalid, overflow) in [("sqrt(-1)", 16, 0), ("1e39", 0, 16)] {
        let mesh = surface(source, &domain, UVec2::splat(4));
        assert!(mesh.indices.is_empty());
        assert!(mesh.positions.is_empty());
        assert_eq!(mesh.diagnostics.invalid_samples, invalid);
        assert_eq!(mesh.diagnostics.unrepresentable_samples, overflow);
        assert_eq!(mesh.diagnostics.rejected_cells, 9);
        assert_eq!(mesh.diagnostics.probes, 0);
        assert_sound(&mesh);
    }
    // Distinct f64 grid slots can round to repeated f32 positions.
    let narrow = Domain::new(
        Vec3::ONE,
        Vec3::new(f32::from_bits(1.0f32.to_bits() + 1), 2.0, 1.0),
    );
    let mesh = surface("0", &narrow, UVec2::splat(4));
    assert_eq!(mesh.indices.len(), 18);
    assert_sound(&mesh);
}

#[test]
fn surface_large_finite_normal_math() {
    let mesh = surface("1e30*x", &Domain::default(), UVec2::splat(4));
    assert_eq!((mesh.positions.len(), mesh.indices.len()), (16, 54));
    assert_sound(&mesh);
    let expected = glam::DVec3::new(-1e30, 0.0, 1.0).normalize().as_vec3();
    assert!(mesh.normals.iter().all(|n| n.distance(expected) < 1e-5));
    let huge = Domain::new(Vec3::splat(-1e30), Vec3::splat(1e30));
    let mesh = surface("x", &huge, UVec2::splat(3));
    assert_eq!((mesh.positions.len(), mesh.indices.len()), (9, 24));
    assert_sound(&mesh);
}

#[test]
fn surface_rounded_xy_collapse_cannot_cancel_normals() {
    let domain = Domain::new(
        Vec3::new(1.0, -2.0, -1.0),
        Vec3::new(f32::from_bits(1.0f32.to_bits() + 1), 7.0, 1.0),
    );
    let expr = ScalarExpr::parse("1e8*(x-1)*y", ScalarContext::Surface).unwrap();
    let mesh = sample_surface(&expr, &domain, UVec2::splat(4));
    assert_sound(&mesh);
    assert_eq!((mesh.positions.len(), mesh.indices.len()), (8, 18));
    assert_eq!(mesh.diagnostics.rejected_cells, 6);
    assert_eq!(mesh.diagnostics.probes, 9);
}
