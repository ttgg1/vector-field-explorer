//! CPU mirror of the arrow families' geometry (tickets 44, 45 and 64).
//!
//! `src/shaders.arrow.wgsl` and `src/shaders.arrow2d.wgsl` are authoritative:
//! the vertex shaders generate the geometry from `vertex_index`, with no vertex
//! buffer. This module repeats those layouts in Rust so they can be checked
//! without a GPU — the same role [`crate::particles::dot_billboard_corners`]
//! plays for particle dots.
//!
//! Local space matches the 3D shader: the arrow points along +Y, the shaft
//! spans `0..shaft_len` (or `0..len` for a headless family), the head sits above
//! it, and a full fan caps the head at the shaft junction. The shader rotates
//! this local geometry by its per-instance basis, which the mirror does not
//! repeat. The flat arrow's mirror stays in the local XY plane; the 2D shader
//! rotates it by the instance's world-XY direction instead.
//!
//! The mirrors are `#[doc(hidden)] pub` test seams: `cone_triangles`,
//! `cylinder_triangles`, and `flat_arrow_triangles` let `tests/` prove the
//! shader layouts have no degenerate triangles and that the cap fan covers
//! every corner, without a GPU. They are not intended as a stable library API.

use glam::Vec3;

use crate::arrow_style::ArrowParams;

/// The Cone's shaft tip radius as a fraction of its base radius.
const CONE_TIP_RATIO: f32 = 0.6;

fn ring_point(theta: f32, y: f32, radius: f32) -> Vec3 {
    Vec3::new(theta.cos() * radius, y, theta.sin() * radius)
}

/// An n-sided cone head's side triangles, one real triangle per side.
fn cone_head_triangles_n(
    shaft_len: f32,
    head_len: f32,
    base_radius: f32,
    sides: u32,
) -> Vec<[Vec3; 3]> {
    let step = std::f32::consts::TAU / sides as f32;
    let apex = Vec3::new(0.0, shaft_len + head_len, 0.0);
    (0..sides)
        .map(|side| {
            let ta = side as f32 * step;
            let tb = ta + step;
            [
                ring_point(ta, shaft_len, base_radius),
                ring_point(tb, shaft_len, base_radius),
                apex,
            ]
        })
        .collect()
}

/// A full triangle fan over every ring corner at `y`, in the shader's
/// `sides - 2` triangles. The fan starts at corner 0 and walks
/// `[0, i + 1, i + 2]`, so it covers the whole ring.
fn plate_triangles_n(y: f32, radius: f32, sides: u32) -> Vec<[Vec3; 3]> {
    let step = std::f32::consts::TAU / sides as f32;
    let corner = |index: u32| ring_point(index as f32 * step, y, radius);
    (0..sides - 2)
        .map(|triangle| [corner(0), corner(triangle + 1), corner(triangle + 2)])
        .collect()
}

/// The far-end cap fan: the same ring as [`plate_triangles_n`] but wound the
/// other way, so its geometric normal faces +Y like the shader's declared
/// `normal`. The cone's underside plate keeps the opposite winding.
fn cap_triangles_n(y: f32, radius: f32, sides: u32) -> Vec<[Vec3; 3]> {
    let step = std::f32::consts::TAU / sides as f32;
    let corner = |index: u32| ring_point(index as f32 * step, y, radius);
    (0..sides - 2)
        .map(|triangle| [corner(0), corner(triangle + 2), corner(triangle + 1)])
        .collect()
}

/// Every triangle the shader emits for a Cone of length `len`: `2 * sides`
/// shaft triangles (tapered), `sides` head triangles, and the full
/// `sides - 2` triangle plate — `4 * sides - 2` triangles, or `12 * sides - 6`
/// vertices.
///
/// Test seam: `#[doc(hidden)] pub` for the non-degenerate/resource tests.
#[doc(hidden)]
pub fn cone_triangles(len: f32, params: ArrowParams) -> Vec<[Vec3; 3]> {
    let params = params.sanitized();
    let sides = params.sides;
    let step = std::f32::consts::TAU / sides as f32;
    let shaft_len = (1.0 - params.head_length) * len;
    let head_len = params.head_length * len;
    let shaft_radius = params.shaft_radius * len;
    let head_radius = params.head_width * len;
    let tip_radius = shaft_radius * CONE_TIP_RATIO;

    let mut triangles = Vec::with_capacity((4 * sides - 2) as usize);
    for quad in 0..sides {
        let ta = quad as f32 * step;
        let tb = ta + step;
        let top_a = ring_point(ta, shaft_len, tip_radius);
        let top_b = ring_point(tb, shaft_len, tip_radius);
        let bottom_a = ring_point(ta, 0.0, shaft_radius);
        let bottom_b = ring_point(tb, 0.0, shaft_radius);
        // Matches the shader's `select(shaft_len, 0.0, top)` order: top
        // vertices come first, then the bottom vertex.
        triangles.push([top_a, top_b, bottom_a]);
        triangles.push([top_b, bottom_b, bottom_a]);
    }
    triangles.extend(cone_head_triangles_n(shaft_len, head_len, head_radius, sides));
    triangles.extend(plate_triangles_n(shaft_len, head_radius, sides));
    triangles
}

/// Every triangle the shader emits for a headless Cylinder of length `len`:
/// `2 * sides` constant-radius shaft triangles over the full length and the
/// `sides - 2` triangle far-end cap — `3 * sides - 2` triangles, or
/// `9 * sides - 6` vertices. No head and no underside plate.
///
/// Test seam: `#[doc(hidden)] pub` for the non-degenerate/cap-coverage tests.
#[doc(hidden)]
pub fn cylinder_triangles(len: f32, params: ArrowParams) -> Vec<[Vec3; 3]> {
    let params = params.sanitized();
    let sides = params.sides;
    let step = std::f32::consts::TAU / sides as f32;
    let shaft_radius = params.shaft_radius * len;

    let mut triangles = Vec::with_capacity((3 * sides - 2) as usize);
    for quad in 0..sides {
        let ta = quad as f32 * step;
        let tb = ta + step;
        let top_a = ring_point(ta, len, shaft_radius);
        let top_b = ring_point(tb, len, shaft_radius);
        let bottom_a = ring_point(ta, 0.0, shaft_radius);
        let bottom_b = ring_point(tb, 0.0, shaft_radius);
        // Same shader order as the cone shaft: top vertices first.
        triangles.push([top_a, top_b, bottom_a]);
        triangles.push([top_b, bottom_b, bottom_a]);
    }
    triangles.extend(cap_triangles_n(len, shaft_radius, sides));
    triangles
}

/// The flat 9-vertex arrow's three triangles in the local XY plane: a shaft
/// quad (two triangles) and a head triangle. The 3D `Flat` ribbon and the 2D
/// `Flat` pipeline share this silhouette; both then rotate it into their view.
///
/// Test seam: `#[doc(hidden)] pub` for the 2D non-degenerate geometry test.
#[doc(hidden)]
pub fn flat_arrow_triangles(len: f32, params: ArrowParams) -> [[Vec3; 3]; 3] {
    let params = params.sanitized();
    let shaft_len = (1.0 - params.head_length) * len;
    let half_shaft = params.shaft_radius * len;
    let half_head = params.head_width * len;

    let bottom_left = Vec3::new(-half_shaft, 0.0, 0.0);
    let bottom_right = Vec3::new(half_shaft, 0.0, 0.0);
    let top_left = Vec3::new(-half_shaft, shaft_len, 0.0);
    let top_right = Vec3::new(half_shaft, shaft_len, 0.0);
    let head_left = Vec3::new(-half_head, shaft_len, 0.0);
    let head_right = Vec3::new(half_head, shaft_len, 0.0);
    let tip = Vec3::new(0.0, len, 0.0);

    [
        [bottom_left, bottom_right, top_left],
        [bottom_right, top_right, top_left],
        [head_left, head_right, tip],
    ]
}
