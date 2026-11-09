//! Arrow geometry mirrors (tickets 44, 45 and 64).
//!
//! The shaders are authoritative and generate geometry from `vertex_index`.
//! These tests pin the CPU mirror in `arrow_geometry` so the retained families
//! stay non-degenerate and the budgets match the draw plan without a GPU.
//! Ticket 64 removed the Pyramid family and made Cylinder headless.

use glam::Vec3;
use vector_field_explorer::arrow_geometry::{
    cone_triangles, cylinder_triangles, flat_arrow_triangles,
};
use vector_field_explorer::arrow_style::{ArrowParams, ArrowStyle};
use vector_field_explorer::renderer::{
    ARROW2D_VERTS, ARROW2D_VERTS_LINE, ARROW_VERTS_FLAT, ARROW_VERTS_LINE, MAX_ARROW_VERTS,
};

/// Anything at or below this is a zero-area triangle for these sizes.
const MIN_AREA: f32 = 1e-6;
/// Tolerance for ring/axis measurements on a unit arrow.
const EPS: f32 = 1e-5;

const ARROW_SHADER: &str = include_str!("../src/shaders.arrow.wgsl");
const ARROW2D_SHADER: &str = include_str!("../src/shaders.arrow2d.wgsl");

/// Area of a triangle in triangle-list order.
fn area(tri: &[Vec3; 3]) -> f32 {
    0.5 * (tri[1] - tri[0]).cross(tri[2] - tri[0]).length()
}

fn radial(point: Vec3) -> Vec3 {
    Vec3::new(point.x, 0.0, point.z)
}

#[test]
fn the_cone_layout_matches_its_budget_and_has_area() {
    for sides in [4, 5, 8, 12, 16] {
        let params = ArrowParams { sides, ..ArrowParams::default() };
        let triangles = cone_triangles(1.0, params);
        assert_eq!(
            triangles.len() * 3,
            ArrowStyle::Cone.vertex_count(sides) as usize,
            "{sides} sides spell the cone budget",
        );
        for (index, tri) in triangles.iter().enumerate() {
            let area = area(tri);
            assert!(
                area > MIN_AREA,
                "cone with {sides} sides, triangle {index} has area {area}",
            );
        }
    }
}

#[test]
fn every_cone_head_triangle_is_outward_and_reaches_the_apex() {
    for sides in [4, 5, 8, 12, 16] {
        let params = ArrowParams { sides, ..ArrowParams::default() };
        let triangles = cone_triangles(1.0, params);
        let shaft_len = 1.0 - params.head_length;
        let head_len = params.head_length;
        let base_radius = params.head_width;

        let head_start = 2 * sides as usize;
        let head_end = head_start + sides as usize;
        let head = &triangles[head_start..head_end];
        assert_eq!(head.len(), sides as usize, "{sides} sides make {sides} head triangles");

        for (side, tri) in head.iter().enumerate() {
            let area = area(tri);
            assert!(area > MIN_AREA, "cone side {side} of {sides} has area {area}");
            assert_eq!(tri[0].y, shaft_len, "side {side} base vertex a sits on the shaft top");
            assert_eq!(tri[1].y, shaft_len, "side {side} base vertex b sits on the shaft top");
            assert_eq!(
                tri[2].y,
                shaft_len + head_len,
                "side {side} apex sits at the cone tip",
            );
            assert!(
                (radial(tri[0]).length() - base_radius).abs() < EPS,
                "side {side} base vertex a is on the head ring",
            );
            assert!(
                (radial(tri[1]).length() - base_radius).abs() < EPS,
                "side {side} base vertex b is on the head ring",
            );
            assert!(radial(tri[2]).length() < EPS, "side {side} apex is on the axis");

            // Mirror order is base-a, base-b, apex (the shader's triangle-list
            // order). This cross product orients away from the axis.
            let normal = (tri[2] - tri[0]).cross(tri[1] - tri[0]).normalize();
            let outward = radial(tri[0]).normalize();
            assert!(
                normal.dot(outward) > 0.0,
                "cone side {side} normal {normal:?} has a positive radial component",
            );
            assert!(normal.y > 0.0, "cone side {side} normal {normal:?} leans up +Y");
        }
    }
}

#[test]
fn the_cone_plate_fan_faces_down_and_covers_the_ring() {
    for sides in [4, 5, 8, 12, 16] {
        let params = ArrowParams { sides, ..ArrowParams::default() };
        let triangles = cone_triangles(1.0, params);
        let shaft_len = 1.0 - params.head_length;
        let head_radius = params.head_width;

        let plate_start = 2 * sides as usize + sides as usize;
        let plate = &triangles[plate_start..];
        assert_eq!(plate.len(), sides as usize - 2, "the full fan has sides - 2 triangles");

        let step = std::f32::consts::TAU / sides as f32;
        let mut covered = vec![false; sides as usize];
        for (index, tri) in plate.iter().enumerate() {
            let area = area(tri);
            assert!(area > MIN_AREA, "plate triangle {index} has area {area}");

            let normal = (tri[1] - tri[0]).cross(tri[2] - tri[0]).normalize();
            assert!(
                (normal - Vec3::NEG_Y).length() < EPS,
                "plate triangle {index} normal {normal:?} faces -Y",
            );
            for point in tri {
                assert_eq!(point.y, shaft_len, "plate triangle {index} caps the shaft top");
                assert!(
                    (radial(*point).length() - head_radius).abs() < EPS,
                    "plate triangle {index} corner sits on the head ring",
                );
                let angle = point.z.atan2(point.x).rem_euclid(std::f32::consts::TAU);
                let corner = (angle / step).round() as usize % sides as usize;
                covered[corner] = true;
            }
        }
        assert!(covered.iter().all(|&seen| seen), "the plate fan touches every ring corner");
    }
}

#[test]
fn shaft_quads_keep_the_shader_vertex_order() {
    // WGSL `select(shaft_len, 0.0, top)` returns its first argument when
    // `top` is false and its second when `top` is true. r0/r1 are therefore
    // the top vertices and r2 is the bottom vertex, so the shader emits
    // [top_a, top_b, bottom_a] then [top_b, bottom_b, bottom_a] — the order
    // this mirror repeats. Pin that order so the diagonal cannot drift.
    let params = ArrowParams { sides: 12, ..ArrowParams::default() };
    let shaft_len = 1.0 - params.head_length;
    let base_radius = params.shaft_radius;
    let tip_radius = base_radius * 0.6;

    let cone = cone_triangles(1.0, params);
    for quad in 0..params.sides as usize {
        let even = cone[quad * 2];
        let odd = cone[quad * 2 + 1];
        assert_eq!(
            [even[0].y, even[1].y, even[2].y],
            [shaft_len, shaft_len, 0.0],
            "cone shaft quad {quad} even triangle is top-top-bottom",
        );
        assert_eq!(
            [odd[0].y, odd[1].y, odd[2].y],
            [shaft_len, 0.0, 0.0],
            "cone shaft quad {quad} odd triangle is top-bottom-bottom",
        );
        let even_radii = [radial(even[0]).length(), radial(even[1]).length(), radial(even[2]).length()];
        let odd_radii = [radial(odd[0]).length(), radial(odd[1]).length(), radial(odd[2]).length()];
        assert!(
            (even_radii[0] - tip_radius).abs() < EPS
                && (even_radii[1] - tip_radius).abs() < EPS
                && (even_radii[2] - base_radius).abs() < EPS,
            "cone shaft quad {quad} even triangle radii {even_radii:?}",
        );
        assert!(
            (odd_radii[0] - tip_radius).abs() < EPS
                && (odd_radii[1] - base_radius).abs() < EPS
                && (odd_radii[2] - base_radius).abs() < EPS,
            "cone shaft quad {quad} odd triangle radii {odd_radii:?}",
        );
    }

    let cylinder = cylinder_triangles(1.0, params);
    for quad in 0..params.sides as usize {
        let even = cylinder[quad * 2];
        let odd = cylinder[quad * 2 + 1];
        assert_eq!(
            [even[0].y, even[1].y, even[2].y],
            [1.0, 1.0, 0.0],
            "cylinder shaft quad {quad} even triangle is top-top-bottom",
        );
        assert_eq!(
            [odd[0].y, odd[1].y, odd[2].y],
            [1.0, 0.0, 0.0],
            "cylinder shaft quad {quad} odd triangle is top-bottom-bottom",
        );
    }
}

#[test]
fn the_cylinder_layout_is_headless_and_matches_its_budget() {
    for sides in [4, 5, 8, 12, 16] {
        let params = ArrowParams { sides, ..ArrowParams::default() };
        let triangles = cylinder_triangles(1.0, params);
        assert_eq!(
            triangles.len() * 3,
            ArrowStyle::Cylinder.vertex_count(sides) as usize,
            "{sides} sides spell the headless cylinder budget",
        );
        assert_eq!(triangles.len(), (3 * sides - 2) as usize, "2*sides shaft + (sides-2) cap");

        let shaft_end = 2 * sides as usize;
        for (index, tri) in triangles.iter().enumerate() {
            let area = area(tri);
            assert!(
                area > MIN_AREA,
                "cylinder with {sides} sides, triangle {index} has area {area}",
            );
            for point in tri {
                let radius = radial(*point).length();
                assert!(
                    (radius - params.shaft_radius).abs() < 1e-5,
                    "cylinder triangle {index} stays at the constant shaft radius",
                );
                if index >= shaft_end {
                    assert_eq!(point.y, 1.0, "the cap triangle sits at the far end");
                } else {
                    assert!(
                        (0.0..=1.0).contains(&point.y),
                        "the shaft spans the full arrow length",
                    );
                }
            }
        }
    }
}

#[test]
fn the_cylinder_cap_fan_covers_the_whole_ring() {
    for sides in [4, 5, 8, 12, 16] {
        let params = ArrowParams { sides, ..ArrowParams::default() };
        let triangles = cylinder_triangles(1.0, params);
        let cap_start = 2 * sides as usize;
        let cap = &triangles[cap_start..];
        assert_eq!(cap.len(), sides as usize - 2, "the full fan has sides - 2 triangles");

        let step = std::f32::consts::TAU / sides as f32;
        let mut covered = vec![false; sides as usize];
        for (index, tri) in cap.iter().enumerate() {
            assert!(area(tri) > MIN_AREA, "cap triangle {index} has area");
            let normal = (tri[1] - tri[0]).cross(tri[2] - tri[0]).normalize();
            assert!(
                (normal - Vec3::Y).length() < EPS,
                "cap triangle {index} normal {normal:?} faces +Y",
            );
            for point in tri {
                assert_eq!(point.y, 1.0, "cap triangle {index} closes the far end");
                let angle = point.z.atan2(point.x).rem_euclid(std::f32::consts::TAU);
                let corner = (angle / step).round() as usize % sides as usize;
                covered[corner] = true;
            }
        }
        assert!(covered.iter().all(|&seen| seen), "the fan touches every ring corner");
    }
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
fn renderer_budgets_match_the_new_families() {
    assert_eq!(ARROW_VERTS_FLAT, 9, "the flat ribbon");
    assert_eq!(ARROW_VERTS_LINE, 6, "the headless line");
    assert_eq!(ARROW2D_VERTS, 9, "the 2D flat arrow");
    assert_eq!(ARROW2D_VERTS_LINE, 6, "the 2D headless line");
    assert_eq!(MAX_ARROW_VERTS, 186, "the combined 3D draw's per-instance budget is a 16-side cone");
    assert!(
        ARROW_SHADER.contains("const MAX_ARROW_VERTS: u32 = 186u;"),
        "the merged shader declares the same budget",
    );
}

#[test]
fn the_shader_declares_the_new_family_budgets() {
    assert!(ARROW_SHADER.contains("const ARROW_VERTS_FLAT: u32 = 9u;"));
    assert!(ARROW_SHADER.contains("const ARROW_VERTS_LINE: u32 = 6u;"));
    assert!(ARROW2D_SHADER.contains("const ARROW2D_VERTS: u32 = 9u;"));
    assert!(ARROW2D_SHADER.contains("const ARROW2D_VERTS_LINE: u32 = 6u;"));
    assert!(
        ARROW_SHADER.contains("return 9u * sides - 6u;"),
        "Cylinder is a full-length shaft plus a far-end cap",
    );
    assert!(
        ARROW_SHADER.contains("select(select(tri + 1u, tri + 2u, r == 1u), 0u, r == 0u)"),
        "the cylinder cap corner order winds toward +Y",
    );
    assert!(
        !ARROW_SHADER.contains("PYRAMID"),
        "the Pyramid branch and constants are gone",
    );
    assert!(
        !ARROW_SHADER.contains("const ARROW_VERTS: u32 = 42u;"),
        "the 42-vertex budget is gone",
    );
}
