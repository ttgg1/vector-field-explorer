//! CPU mirror of the 3D arrow's cone-head geometry (ticket 44).
//!
//! `src/shaders.arrow.wgsl` is authoritative: the vertex shader generates
//! the geometry from `vertex_index`, with no vertex buffer. This module repeats
//! that layout in Rust so it can be checked without a GPU — the same role
//! [`crate::particles::dot_billboard_corners`] plays for particle dots.
//!
//! Local space matches the shader: the arrow points along +Y, the shaft spans
//! `0..shaft_len`, the cone head sits above it, and a two-triangle base plate
//! caps the head at the shaft junction. The shader rotates this local geometry
//! by its per-instance basis, which the mirror does not repeat.

use glam::Vec3;

/// A full turn split into the cone's four sides (also the shaft's four quads).
const STEP: f32 = std::f32::consts::TAU / 4.0;
/// Shaft radii as fractions of the arrow length, matching the shader.
const SHAFT_BASE_RADIUS: f32 = 0.055;
const SHAFT_TIP_RADIUS: f32 = 0.03;
/// Shaft and head lengths as fractions of the arrow length.
const SHAFT_LEN_FRACTION: f32 = 0.7;
const HEAD_LEN_FRACTION: f32 = 0.3;
/// Cone base radius as a fraction of the arrow length.
const HEAD_BASE_RADIUS_FRACTION: f32 = 0.12;

fn ring_point(theta: f32, y: f32, radius: f32) -> Vec3 {
    Vec3::new(theta.cos() * radius, y, theta.sin() * radius)
}

/// The cone head's four side triangles, in the shader's triangle-list order:
/// each side is a base vertex at `ta`, a base vertex at `tb`, then the apex.
///
/// The base ring sits at `y = shaft_len` with `base_radius`; the apex sits at
/// `y = shaft_len + head_len` on the axis. The shader keeps one real triangle
/// per side (ticket 44); the old six-vertex quad left both triangles at zero
/// area.
pub fn cone_head_triangles(shaft_len: f32, head_len: f32, base_radius: f32) -> [[Vec3; 3]; 4] {
    let apex = Vec3::new(0.0, shaft_len + head_len, 0.0);
    std::array::from_fn(|side| {
        let ta = side as f32 * STEP;
        let tb = ta + STEP;
        [
            ring_point(ta, shaft_len, base_radius),
            ring_point(tb, shaft_len, base_radius),
            apex,
        ]
    })
}

/// The cone's base plate: two triangles over the ring corners at
/// `y = shaft_len`, in the shader's fan order (0,1,2) and (2,3,0).
///
/// Both face -Y, capping the shaft/head junction from below.
pub fn cone_base_plate_triangles(shaft_len: f32, base_radius: f32) -> [[Vec3; 3]; 2] {
    let corner = |index: usize| ring_point(index as f32 * STEP, shaft_len, base_radius);
    [[corner(0), corner(1), corner(2)], [corner(2), corner(3), corner(0)]]
}

/// The shaft's eight side triangles for an arrow of length `len`: four quads,
/// each split into the shader's two triangles.
///
/// Implementation detail of [`arrow_triangles`]; the shaft is unchanged by the
/// ticket 44 head fix and has no separate public seam.
fn shaft_triangles(len: f32) -> [[Vec3; 3]; 8] {
    let shaft_len = SHAFT_LEN_FRACTION * len;
    let base = SHAFT_BASE_RADIUS * len;
    let tip = SHAFT_TIP_RADIUS * len;
    std::array::from_fn(|index| {
        let quad = index / 2;
        let ta = quad as f32 * STEP;
        let tb = ta + STEP;
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

/// Every triangle of the full 42-vertex arrow layout for an arrow of length
/// `len`: 8 shaft + 4 head + 2 base plate, in the shader's vertex order.
///
/// Test seam (ticket 44): `#[doc(hidden)] pub` so `tests/arrow_geometry_test.rs`
/// can prove the whole 42-vertex layout has no zero-area triangle without a
/// GPU. The WGSL remains authoritative; this mirror is the checkable seam.
#[doc(hidden)]
pub fn arrow_triangles(len: f32) -> [[Vec3; 3]; 14] {
    let shaft_len = SHAFT_LEN_FRACTION * len;
    let head_len = HEAD_LEN_FRACTION * len;
    let base_radius = HEAD_BASE_RADIUS_FRACTION * len;
    let mut triangles = [[Vec3::ZERO; 3]; 14];
    triangles[..8].copy_from_slice(&shaft_triangles(len));
    triangles[8..12].copy_from_slice(&cone_head_triangles(shaft_len, head_len, base_radius));
    triangles[12..].copy_from_slice(&cone_base_plate_triangles(shaft_len, base_radius));
    triangles
}
