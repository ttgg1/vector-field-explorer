//! Ticket 44: the 3D arrow's cone-head geometry.
//!
//! The cone sides used to be emitted as six-vertex quads, which made every
//! head triangle degenerate and left only the flat base plate visible. These
//! tests pin the fixed layout through the CPU mirror in `arrow_geometry` and
//! read the shader source so the six-vertex pattern cannot come back unseen.
//! There is no GPU in the test harness, so the mirror is the testable seam.

use glam::Vec3;
use vector_field_explorer::arrow_geometry::{
    arrow_triangles, cone_base_plate_triangles, cone_head_triangles, cylinder_triangles,
    flat_arrow_triangles,
};
use vector_field_explorer::arrow_style::{ArrowParams, ArrowStyle};
use vector_field_explorer::renderer::{ARROW2D_VERTS, ARROW_VERTS};

/// The ticket's unit-arrow measurements.
const SHAFT_LEN: f32 = 0.7;
const HEAD_LEN: f32 = 0.3;
const BASE_RADIUS: f32 = 0.12;
const EPS: f32 = 1e-5;
/// Anything at or below this is a zero-area triangle for these sizes.
const MIN_AREA: f32 = 1e-6;

const ARROW_SHADER: &str = include_str!("../src/shaders.arrow.wgsl");

/// Area of a triangle in triangle-list order.
fn area(tri: &[Vec3; 3]) -> f32 {
    0.5 * (tri[1] - tri[0]).cross(tri[2] - tri[0]).length()
}

fn radial(point: Vec3) -> Vec3 {
    Vec3::new(point.x, 0.0, point.z)
}

#[test]
fn every_cone_side_triangle_has_area_and_an_outward_normal() {
    let sides = cone_head_triangles(SHAFT_LEN, HEAD_LEN, BASE_RADIUS);
    assert_eq!(sides.len(), 4, "a four-sided cone");

    for (side, tri) in sides.iter().enumerate() {
        let area = area(tri);
        assert!(area > MIN_AREA, "cone side {side} has area {area}, expected > {MIN_AREA}");
        assert_eq!(tri[0].y, SHAFT_LEN, "side {side} base vertex a sits on the shaft top");
        assert_eq!(tri[1].y, SHAFT_LEN, "side {side} base vertex b sits on the shaft top");
        assert_eq!(
            tri[2].y,
            SHAFT_LEN + HEAD_LEN,
            "side {side} apex sits at the cone tip",
        );
        assert!(
            (radial(tri[0]).length() - BASE_RADIUS).abs() < EPS,
            "side {side} base vertex a is on the base ring",
        );
        assert!(
            (radial(tri[1]).length() - BASE_RADIUS).abs() < EPS,
            "side {side} base vertex b is on the base ring",
        );
        assert!(radial(tri[2]).length() < EPS, "side {side} apex is on the axis");

        // Mirror order is base-a, base-b, apex (the shader's triangle-list
        // order). Taking the normal as (apex - base-a) x (base-b - base-a)
        // orients it away from the axis.
        let normal = (tri[2] - tri[0]).cross(tri[1] - tri[0]).normalize();
        let outward = radial(tri[0]).normalize();
        assert!(
            normal.dot(outward) > 0.0,
            "cone side {side} normal {normal:?} has a positive radial component",
        );
        // The cone narrows toward the apex, so an outward face also leans up.
        assert!(normal.y > 0.0, "cone side {side} normal {normal:?} leans up +Y");
    }
}

#[test]
fn base_plate_has_two_upward_facing_triangles() {
    let plate = cone_base_plate_triangles(SHAFT_LEN, BASE_RADIUS);
    assert_eq!(plate.len(), 2, "the plate is a fan of two triangles");

    for (index, tri) in plate.iter().enumerate() {
        let area = area(tri);
        assert!(area > MIN_AREA, "plate triangle {index} has area {area}");

        // Plate order is counter-clockwise in local +X/+Z, which faces -Y.
        let normal = (tri[1] - tri[0]).cross(tri[2] - tri[0]).normalize();
        assert!(
            (normal - Vec3::NEG_Y).length() < EPS,
            "plate triangle {index} normal {normal:?} faces -Y",
        );
        for point in tri {
            assert_eq!(point.y, SHAFT_LEN, "plate triangle {index} caps the shaft top");
            assert!(
                (radial(*point).length() - BASE_RADIUS).abs() < EPS,
                "plate triangle {index} corner sits on the base ring",
            );
        }
    }
}

#[test]
fn arrow_vertex_count_is_42() {
    assert_eq!(ARROW_VERTS, 42, "24 shaft + 12 head + 6 plate vertices");
}

#[test]
fn the_flat_2d_arrow_is_nine_non_degenerate_vertices() {
    let triangles = flat_arrow_triangles(1.0, ArrowParams::default());
    assert_eq!(
        triangles.len() * 3,
        ARROW2D_VERTS as usize,
        "the flat arrow is a shaft quad plus a head triangle",
    );
    for (index, tri) in triangles.iter().enumerate() {
        let area = area(tri);
        assert!(area > MIN_AREA, "flat arrow triangle {index} has area {area}");
        assert_eq!(tri[0].z, 0.0, "triangle {index} lies in the flat plane");
        assert_eq!(tri[1].z, 0.0);
        assert_eq!(tri[2].z, 0.0);
    }
    assert_eq!(triangles[2][2].y, 1.0, "the head tip reaches the arrow length");
}

#[test]
fn pyramid_42_vertices_remain_non_degenerate() {
    let params = ArrowParams::default();
    let triangles = arrow_triangles(1.0);
    assert_eq!(triangles.len() * 3, ArrowStyle::Pyramid.vertex_count(params.sides) as usize);
    for (index, tri) in triangles.iter().enumerate() {
        assert!(area(tri) > MIN_AREA, "pyramid triangle {index} is non-degenerate");
    }
}

#[test]
fn every_cylinder_side_is_non_degenerate() {
    for sides in [4, 5, 8, 12, 16] {
        let params = ArrowParams { sides, ..ArrowParams::default() };
        let triangles = cylinder_triangles(1.0, params);
        assert_eq!(
            triangles.len() * 3,
            ArrowStyle::Cylinder.vertex_count(sides) as usize,
            "{sides} sides spell the cylinder budget",
        );
        for (index, tri) in triangles.iter().enumerate() {
            let area = area(tri);
            assert!(
                area > MIN_AREA,
                "cylinder with {sides} sides, triangle {index} has area {area}",
            );
        }
    }
}

#[test]
fn cylinder_plate_fan_covers_the_whole_ring_and_faces_down() {
    for sides in [4, 5, 8, 12, 16] {
        let params = ArrowParams { sides, ..ArrowParams::default() };
        let triangles = cylinder_triangles(1.0, params);
        let plate_start = (2 * sides + sides) as usize;
        let plate = &triangles[plate_start..];
        assert_eq!(plate.len(), sides as usize - 2, "the full fan has sides - 2 triangles");

        let step = std::f32::consts::TAU / sides as f32;
        let mut covered = vec![false; sides as usize];
        for (index, tri) in plate.iter().enumerate() {
            let area = area(tri);
            assert!(area > MIN_AREA, "plate triangle {index} has area {area}");
            let normal = (tri[1] - tri[0]).cross(tri[2] - tri[0]).normalize();
            assert!((normal - Vec3::NEG_Y).length() < EPS, "plate triangle {index} faces -Y");
            for point in tri {
                assert_eq!(point.y, SHAFT_LEN, "plate triangle {index} caps the junction");
                let angle = point.z.atan2(point.x).rem_euclid(std::f32::consts::TAU);
                let corner = (angle / step).round() as usize % sides as usize;
                covered[corner] = true;
            }
        }
        assert!(covered.iter().all(|&seen| seen), "the fan touches every ring corner");
    }
}

#[test]
fn every_cylinder_ring_vertex_is_at_the_configured_radius() {
    let params = ArrowParams { sides: 12, shaft_radius: 0.08, ..ArrowParams::default() };
    let triangles = cylinder_triangles(1.0, params);
    let shaft_end = params.sides as usize * 2;
    for tri in &triangles[..shaft_end] {
        for point in tri {
            // The two shaft rings share the constant cylinder radius.
            let radius = radial(*point).length();
            assert!(
                (radius - params.shaft_radius).abs() < EPS,
                "cylinder shaft vertex radius {radius} is constant",
            );
        }
    }
}

#[test]
fn shader_declares_the_fixed_42_vertex_layout() {
    // The WGSL is authoritative; the Rust constant and the draw call mirror it.
    assert!(
        ARROW_SHADER.contains("const ARROW_VERTS: u32 = 42u;"),
        "the shader declares the 42-vertex arrow layout",
    );
    assert!(
        ARROW_SHADER.contains("j < 36u"),
        "the cone-head block ends at the 12-vertex (four-triangle) boundary",
    );
    assert!(
        !ARROW_SHADER.contains("j < 48u"),
        "the old six-vertex-per-side head boundary is gone",
    );
    assert!(!ARROW_SHADER.contains("54u"), "the old 54-vertex count is gone");
}

#[test]
fn shaft_triangles_keep_the_shader_vertex_order() {
    // The shader's r0..r5 mapping is two triangles per quad:
    //   even: [top_a, top_b, bottom_a]
    //   odd:  [top_b, bottom_b, bottom_a]
    // Pin the order (and the top_b-bottom_a diagonal) so the mirror cannot
    // silently pick the other diagonal again.
    const SHAFT_BASE_RADIUS: f32 = 0.055;
    const SHAFT_TIP_RADIUS: f32 = 0.03;

    let triangles = arrow_triangles(1.0);
    for (index, tri) in triangles[..8].iter().enumerate() {
        let ys = [tri[0].y, tri[1].y, tri[2].y];
        let expected_ys = if index % 2 == 0 {
            [SHAFT_LEN, SHAFT_LEN, 0.0]
        } else {
            [SHAFT_LEN, 0.0, 0.0]
        };
        assert_eq!(ys, expected_ys, "shaft triangle {index} y order");

        let radii = [
            radial(tri[0]).length(),
            radial(tri[1]).length(),
            radial(tri[2]).length(),
        ];
        let expected_radii = if index % 2 == 0 {
            [SHAFT_TIP_RADIUS, SHAFT_TIP_RADIUS, SHAFT_BASE_RADIUS]
        } else {
            [SHAFT_TIP_RADIUS, SHAFT_BASE_RADIUS, SHAFT_BASE_RADIUS]
        };
        for (vertex, (radius, expected)) in
            radii.iter().zip(expected_radii.iter()).enumerate()
        {
            assert!(
                (radius - expected).abs() < EPS,
                "shaft triangle {index} vertex {vertex} radius {radius} != {expected}",
            );
        }
    }
}

#[test]
fn full_42_vertex_layout_has_no_degenerate_triangle() {
    let triangles = arrow_triangles(1.0);
    assert_eq!(
        triangles.len() * 3,
        ARROW_VERTS as usize,
        "8 shaft + 4 head + 2 plate triangles spell the 42-vertex layout",
    );

    for (index, tri) in triangles.iter().enumerate() {
        let area = area(tri);
        assert!(
            area > MIN_AREA,
            "triangle {index} ({tri:?}) has area {area}, expected > {MIN_AREA}",
        );
    }

    // The composed layout uses the same head and plate as the dedicated
    // mirrors, so the two views cannot drift apart.
    assert_eq!(
        &triangles[8..12],
        &cone_head_triangles(SHAFT_LEN, HEAD_LEN, BASE_RADIUS),
        "the composed layout's head matches cone_head_triangles",
    );
    assert_eq!(
        &triangles[12..14],
        &cone_base_plate_triangles(SHAFT_LEN, BASE_RADIUS),
        "the composed layout's plate matches cone_base_plate_triangles",
    );
}
