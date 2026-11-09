//! CPU mirror of the arrow families' geometry (tickets 44 and 45).
//!
//! `src/shaders.arrow.wgsl` and `src/shaders.arrow2d.wgsl` are authoritative:
//! the vertex shaders generate the geometry from `vertex_index`, with no vertex
//! buffer. This module repeats those layouts in Rust so they can be checked
//! without a GPU — the same role [`crate::particles::dot_billboard_corners`]
//! plays for particle dots.
//!
//! Local space matches the 3D shader: the arrow points along +Y, the shaft
//! spans `0..shaft_len`, the head sits above it, and a full fan caps the head
//! at the shaft junction. The shader rotates this local geometry by its
//! per-instance basis, which the mirror does not repeat. The flat arrow's
//! mirror stays in the local XY plane; the 2D shader rotates it by the
//! instance's world-XY direction instead.
//!
//! Several mirrors are `#[doc(hidden)] pub` test seams: `cylinder_triangles`,
//! `pyramid_triangles`, and `flat_arrow_triangles` let `tests/` prove the
//! shader layouts have no degenerate triangles and that the plate fan covers
//! every corner, without a GPU. They are not intended as a stable library API.

use glam::Vec3;

use crate::arrow_style::ArrowParams;

/// A full turn split into the Pyramid's four sides (also its four shaft quads).
const PYRAMID_STEP: f32 = std::f32::consts::TAU / 4.0;
/// The ticket-44 shaft radii, as fractions of the arrow length. With the
/// default `shaft_radius` these reproduce the 42-vertex Pyramid exactly.
const PYRAMID_BASE_RADIUS: f32 = 0.055;
const PYRAMID_TIP_RADIUS: f32 = 0.03;
/// The Pyramid keeps the ticket-44 taper ratio relative to its base radius.
const PYRAMID_TIP_RATIO: f32 = PYRAMID_TIP_RADIUS / PYRAMID_BASE_RADIUS;

fn ring_point(theta: f32, y: f32, radius: f32) -> Vec3 {
    Vec3::new(theta.cos() * radius, y, theta.sin() * radius)
}

/// The four-sided cone head's side triangles (ticket 44), in the shader's
/// triangle-list order: base at `ta`, base at `tb`, then the apex.
pub fn cone_head_triangles(shaft_len: f32, head_len: f32, base_radius: f32) -> [[Vec3; 3]; 4] {
    let apex = Vec3::new(0.0, shaft_len + head_len, 0.0);
    std::array::from_fn(|side| {
        let ta = side as f32 * PYRAMID_STEP;
        let tb = ta + PYRAMID_STEP;
        [
            ring_point(ta, shaft_len, base_radius),
            ring_point(tb, shaft_len, base_radius),
            apex,
        ]
    })
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

/// The four-sided base plate: two triangles over the ring corners at
/// `y = shaft_len`, in the shader's fan order (0,1,2) and (2,3,0).
pub fn cone_base_plate_triangles(shaft_len: f32, base_radius: f32) -> [[Vec3; 3]; 2] {
    let corner = |index: usize| ring_point(index as f32 * PYRAMID_STEP, shaft_len, base_radius);
    [[corner(0), corner(1), corner(2)], [corner(2), corner(3), corner(0)]]
}

/// The n-sided base plate mirror: a full triangle fan over every ring corner
/// at `y = shaft_len`, in the shader's `sides - 2` triangles. The fan starts
/// at corner 0 and walks `[0, i + 1, i + 2]`, so it covers the whole ring.
fn base_plate_triangles_n(shaft_len: f32, base_radius: f32, sides: u32) -> Vec<[Vec3; 3]> {
    let step = std::f32::consts::TAU / sides as f32;
    let corner = |index: u32| ring_point(index as f32 * step, shaft_len, base_radius);
    (0..sides - 2)
        .map(|triangle| [corner(0), corner(triangle + 1), corner(triangle + 2)])
        .collect()
}

/// The Pyramid's eight shaft side triangles for an arrow of length `len`:
/// four quads, each split into the shader's two triangles.
fn pyramid_shaft_triangles(len: f32, params: ArrowParams) -> [[Vec3; 3]; 8] {
    let shaft_len = (1.0 - params.head_length) * len;
    let base = params.shaft_radius * len;
    let tip = params.shaft_radius * PYRAMID_TIP_RATIO * len;
    std::array::from_fn(|index| {
        let quad = index / 2;
        let ta = quad as f32 * PYRAMID_STEP;
        let tb = ta + PYRAMID_STEP;
        let top_a = ring_point(ta, shaft_len, tip);
        let top_b = ring_point(tb, shaft_len, tip);
        let bottom_a = ring_point(ta, 0.0, base);
        let bottom_b = ring_point(tb, 0.0, base);
        if index % 2 == 0 {
            [top_a, top_b, bottom_a]
        } else {
            [top_b, bottom_b, bottom_a]
        }
    })
}

/// Every triangle of the Pyramid's 42-vertex layout for an arrow of length
/// `len`: 8 shaft + 4 head + 2 base plate, in the shader's vertex order.
///
/// Test seam: `#[doc(hidden)] pub` so `tests/arrow_geometry_test.rs` can check
/// the layout without a GPU.
#[doc(hidden)]
pub fn pyramid_triangles(len: f32, params: ArrowParams) -> [[Vec3; 3]; 14] {
    let params = params.sanitized();
    let shaft_len = (1.0 - params.head_length) * len;
    let head_len = params.head_length * len;
    let base_radius = params.head_width * len;
    let mut triangles = [[Vec3::ZERO; 3]; 14];
    triangles[..8].copy_from_slice(&pyramid_shaft_triangles(len, params));
    triangles[8..12].copy_from_slice(&cone_head_triangles(shaft_len, head_len, base_radius));
    triangles[12..].copy_from_slice(&cone_base_plate_triangles(shaft_len, base_radius));
    triangles
}

/// The ticket-44 42-vertex Pyramid layout at the default parameters.
///
/// Test seam (ticket 44): `#[doc(hidden)] pub` so `tests/arrow_geometry_test.rs`
/// can prove the whole 42-vertex layout has no zero-area triangle without a
/// GPU. The WGSL remains authoritative; this mirror is the checkable seam.
#[doc(hidden)]
pub fn arrow_triangles(len: f32) -> [[Vec3; 3]; 14] {
    pyramid_triangles(len, ArrowParams::default())
}

/// Every triangle the shader emits for a Cylinder of length `len`: `2 * sides`
/// shaft triangles (constant radius), `sides` head triangles, and the full
/// `sides - 2` triangle plate — `4 * sides - 2` triangles, or `12 * sides - 6`
/// vertices.
///
/// Test seam: `#[doc(hidden)] pub` for the non-degenerate/plate-coverage tests.
#[doc(hidden)]
pub fn cylinder_triangles(len: f32, params: ArrowParams) -> Vec<[Vec3; 3]> {
    let params = params.sanitized();
    let sides = params.sides;
    let step = std::f32::consts::TAU / sides as f32;
    let shaft_len = (1.0 - params.head_length) * len;
    let head_len = params.head_length * len;
    let shaft_radius = params.shaft_radius * len;
    let head_radius = params.head_width * len;

    let mut triangles = Vec::with_capacity((4 * sides - 2) as usize);
    for quad in 0..sides {
        let ta = quad as f32 * step;
        let tb = ta + step;
        let top_a = ring_point(ta, shaft_len, shaft_radius);
        let top_b = ring_point(tb, shaft_len, shaft_radius);
        let bottom_a = ring_point(ta, 0.0, shaft_radius);
        let bottom_b = ring_point(tb, 0.0, shaft_radius);
        triangles.push([top_a, top_b, bottom_a]);
        triangles.push([top_b, bottom_b, bottom_a]);
    }
    triangles.extend(cone_head_triangles_n(shaft_len, head_len, head_radius, sides));
    triangles.extend(base_plate_triangles_n(shaft_len, head_radius, sides));
    triangles
}

/// The flat 9-vertex arrow's three triangles in the local XY plane: a shaft
/// quad (two triangles) and a head triangle. The 3D `Line` ribbon and the 2D
/// pipeline share this silhouette; both then rotate it into their view.
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
