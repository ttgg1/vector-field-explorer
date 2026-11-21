use vector_field_explorer::object_scene::{
    MeshPrimitive, MeshVertex, STATIC_TRIANGLES, STATIC_VERTICES, VERTEX_BYTES, primitive_mesh,
};

/// Barycentric corner markers in `push_triangle` order.
const CORNERS: [[f32; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[test]
fn primitive_triangle_counts_match_the_budget() {
    assert_eq!(MeshPrimitive::Sphere.triangles(), 768);
    assert_eq!(MeshPrimitive::Cylinder.triangles(), 96);
    assert_eq!(MeshPrimitive::Torus.triangles(), 768);
    assert_eq!(MeshPrimitive::Tube.triangles(), 1152);
    assert_eq!(MeshPrimitive::Disc.triangles(), 48);
    assert_eq!(STATIC_TRIANGLES, 2_832);
    assert_eq!(STATIC_VERTICES, 8_496);
    // Secondary arithmetic check: the byte budget follows from the constants.
    assert_eq!(STATIC_VERTICES * VERTEX_BYTES, 305_856);
}

#[test]
fn vertex_layout_is_thirty_six_bytes_and_aligns_to_four() {
    // Pin the on-GPU vertex stride to the constant the renderer will use.
    assert_eq!(std::mem::size_of::<[f32; 3]>(), 12);
    assert_eq!(std::mem::size_of::<MeshVertex>(), VERTEX_BYTES);
    assert!(std::mem::align_of::<MeshVertex>() <= 4);
}

#[test]
fn every_primitive_mesh_has_finite_vertices_and_unit_normals() {
    for primitive in MeshPrimitive::ALL {
        let mesh = primitive_mesh(primitive);
        assert_eq!(mesh.vertices.len(), primitive.vertices());
        for v in mesh.vertices {
            assert!(v.position.iter().all(|x| x.is_finite()));
            let len =
                (v.normal[0] * v.normal[0] + v.normal[1] * v.normal[1] + v.normal[2] * v.normal[2])
                    .sqrt();
            assert!((len - 1.0).abs() < 1e-4, "{primitive:?} normal {len}");
        }
    }
}

#[test]
fn barycentric_corners_are_one_hot_per_triangle() {
    for primitive in MeshPrimitive::ALL {
        let mesh = primitive_mesh(primitive);
        for (triangle, corners) in mesh.vertices.as_chunks::<3>().0.iter().enumerate() {
            for (corner, v) in corners.iter().enumerate() {
                assert_eq!(
                    v.barycentric, CORNERS[corner],
                    "{primitive:?} triangle {triangle} corner {corner}"
                );
            }
        }
    }
}

#[test]
fn every_triangle_winds_ccw_from_outside_and_is_non_degenerate() {
    // Convention: each triangle `(p0, p1, p2)` is counter-clockwise seen from
    // outside the body, so `(p1 - p0) x (p2 - p0)` points along the stored
    // outward per-vertex normals. Flipping any triangle's vertex order makes the
    // dot product negative; collapsing two corners makes the cross vanish.
    for primitive in MeshPrimitive::ALL {
        let mesh = primitive_mesh(primitive);
        for (triangle, tri) in mesh.vertices.as_chunks::<3>().0.iter().enumerate() {
            let [v0, v1, v2] = *tri;
            let geometric = cross(sub(v1.position, v0.position), sub(v2.position, v0.position));
            let length = dot(geometric, geometric).sqrt();
            assert!(
                length > 1e-8,
                "{primitive:?} triangle {triangle} is degenerate (cross length {length})"
            );
            let outward = [
                v0.normal[0] + v1.normal[0] + v2.normal[0],
                v0.normal[1] + v1.normal[1] + v2.normal[1],
                v0.normal[2] + v1.normal[2] + v2.normal[2],
            ];
            let orientation = dot(geometric, outward);
            assert!(
                orientation > 0.0,
                "{primitive:?} triangle {triangle} winds inward (dot {orientation})"
            );
        }
    }
}

#[test]
fn generated_geometry_matches_static_budget() {
    // Tie the documented constants to the meshes actually generated.
    let per_primitive_triangles: usize = MeshPrimitive::ALL
        .iter()
        .map(|primitive| primitive.triangles())
        .sum();
    assert_eq!(per_primitive_triangles, STATIC_TRIANGLES);

    let vertices: usize = MeshPrimitive::ALL
        .iter()
        .map(|primitive| primitive_mesh(*primitive).vertices.len())
        .sum();
    let triangles: usize = MeshPrimitive::ALL
        .iter()
        .map(|primitive| primitive_mesh(*primitive).vertices.len() / 3)
        .sum();
    assert_eq!(vertices, STATIC_VERTICES);
    assert_eq!(triangles, STATIC_TRIANGLES);
}

#[test]
fn sphere_vertices_lie_on_the_unit_sphere() {
    for v in primitive_mesh(MeshPrimitive::Sphere).vertices {
        let r = (v.position[0].powi(2) + v.position[1].powi(2) + v.position[2].powi(2)).sqrt();
        assert!((r - 1.0).abs() < 1e-4);
    }
}

#[test]
fn cylinder_spans_unit_height_and_has_caps() {
    let mesh = primitive_mesh(MeshPrimitive::Cylinder);
    assert!(
        mesh.vertices
            .iter()
            .any(|v| (v.position[1] - 0.5).abs() < 1e-6)
    );
    assert!(
        mesh.vertices
            .iter()
            .any(|v| (v.position[1] + 0.5).abs() < 1e-6)
    );
    assert!(
        mesh.vertices
            .iter()
            .any(|v| (v.normal[1] - 1.0).abs() < 1e-6)
    );
}

#[test]
fn disc_is_two_sided_and_flat() {
    let mesh = primitive_mesh(MeshPrimitive::Disc);
    assert!(mesh.vertices.iter().all(|v| v.position[1].abs() < 1e-6));
    assert!(mesh.vertices.iter().any(|v| v.normal[1] > 0.9));
    assert!(mesh.vertices.iter().any(|v| v.normal[1] < -0.9));
}

#[test]
fn tube_has_twelve_visible_turns() {
    // Name kept to match the plan's contract; the body now proves the turn
    // count directly instead of only the height span.
    let mesh = primitive_mesh(MeshPrimitive::Tube);

    let min = mesh
        .vertices
        .iter()
        .map(|v| v.position[1])
        .fold(f32::INFINITY, f32::min);
    let max = mesh
        .vertices
        .iter()
        .map(|v| v.position[1])
        .fold(f32::NEG_INFINITY, f32::max);
    assert!((max - min - 1.0).abs() < 1e-4, "tube spans unit height");
    assert_eq!(MeshPrimitive::Tube.triangles(), 12 * 8 * 6 * 2);

    // The generator emits 8 sides x 2 triangles x 3 vertices per helical step.
    // Average each step's ring (offsets cancel) to recover the path angle, then
    // unwrap and total the azimuthal advance: 12 turns == 24*pi radians.
    let vertices_per_step = 8 * 2 * 3;
    assert_eq!(
        mesh.vertices.len() % vertices_per_step,
        0,
        "tube vertices split into whole steps"
    );
    let mut first: Option<f32> = None;
    let mut previous: Option<f32> = None;
    let mut total = 0.0_f32;
    for step in mesh.vertices.chunks_exact(vertices_per_step) {
        let count = step.len() as f32;
        let x = step.iter().map(|v| v.position[0]).sum::<f32>() / count;
        let z = step.iter().map(|v| v.position[2]).sum::<f32>() / count;
        let angle = z.atan2(x);
        if let Some(previous) = previous {
            let mut delta = angle - previous;
            delta -= std::f32::consts::TAU * (delta / std::f32::consts::TAU).round();
            total += delta;
        } else {
            first = Some(angle);
        }
        previous = Some(angle);
    }
    // Close the loop from the last step back to the first so all 12 turns count.
    if let (Some(first), Some(previous)) = (first, previous) {
        let mut delta = first - previous;
        delta -= std::f32::consts::TAU * (delta / std::f32::consts::TAU).round();
        total += delta;
    }
    let expected = std::f32::consts::TAU * 12.0;
    assert!(
        (total.abs() - expected).abs() < 0.1,
        "tube wraps 12 turns: total azimuth {total} rad, expected {expected}"
    );
}
