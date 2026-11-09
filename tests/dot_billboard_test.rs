//! Ticket 29: round particle dots.
//!
//! The dot shader replaces its octahedron with a camera-facing billboard quad
//! and keeps only the unit circle inscribed in it. The GPU geometry itself is
//! not unit-testable here, so these tests pin the pure CPU mirror the shader is
//! written against: the quad corner table, its world-space expansion, and the
//! camera basis recovered from the view-projection that feeds the `Globals`
//! uniform.

use glam::{Mat4, Vec3};
use vector_field_explorer::camera::OrbitCamera;
use vector_field_explorer::colormap::Colormap;
use vector_field_explorer::domain::Domain;
use vector_field_explorer::field::FieldExpr;
use vector_field_explorer::particles::{
    DOT_QUAD_CORNERS, Particles, dot_billboard_corners, dot_instances, spawn,
};
use vector_field_explorer::renderer::{DOT_VERTS_PER_INSTANCE, Globals, camera_basis_from_view_proj};

const EPS: f32 = 1e-5;

fn assert_close(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() <= EPS,
        "{what}: {actual} != {expected} (tolerance {EPS})",
    );
}

fn assert_vec_close(actual: Vec3, expected: Vec3, what: &str) {
    let delta = (actual - expected).length();
    assert!(delta <= EPS, "{what}: {actual:?} != {expected:?} (|delta| = {delta})");
}

/// The four distinct corners of the quad, in order (the table repeats two of
/// them to spell two triangles).
fn distinct_corners() -> Vec<[f32; 2]> {
    let mut out: Vec<[f32; 2]> = Vec::new();
    for corner in DOT_QUAD_CORNERS {
        if !out.contains(&corner) {
            out.push(corner);
        }
    }
    out
}

#[test]
fn quad_is_six_vertices_of_two_triangles() {
    // The instanced draw emits DOT_VERTS_PER_INSTANCE vertices per dot; the
    // corner table the shader mirrors must be exactly that big.
    assert_eq!(DOT_QUAD_CORNERS.len(), 6, "two triangles of three vertices");
    assert_eq!(
        DOT_VERTS_PER_INSTANCE as usize,
        DOT_QUAD_CORNERS.len(),
        "the draw count and the CPU corner table stay in sync",
    );
    assert_eq!(distinct_corners().len(), 4, "six entries spell four corners");
}

#[test]
fn quad_corners_are_finite_and_on_the_unit_square() {
    for corner in DOT_QUAD_CORNERS {
        assert!(corner[0].is_finite() && corner[1].is_finite(), "finite corner: {corner:?}");
        assert_close(corner[0].abs(), 1.0, "corner x on the square edge");
        assert_close(corner[1].abs(), 1.0, "corner y on the square edge");
    }
}

#[test]
fn quad_triangles_are_non_degenerate_and_centred() {
    // Two triangles: (0,1,2) and (3,4,5). Each must have area, and the four
    // distinct corners must be symmetric about the dot centre.
    for tri in [0usize, 3] {
        let a = DOT_QUAD_CORNERS[tri];
        let b = DOT_QUAD_CORNERS[tri + 1];
        let c = DOT_QUAD_CORNERS[tri + 2];
        let ab = [b[0] - a[0], b[1] - a[1]];
        let ac = [c[0] - a[0], c[1] - a[1]];
        let cross = ab[0] * ac[1] - ab[1] * ac[0];
        assert!(cross.abs() > 1.0, "triangle {tri}..{} has area", tri + 2);
    }
    let mut sum = [0.0_f32, 0.0_f32];
    for corner in distinct_corners() {
        sum[0] += corner[0];
        sum[1] += corner[1];
    }
    assert_close(sum[0], 0.0, "corners are symmetric in x");
    assert_close(sum[1], 0.0, "corners are symmetric in y");
}

#[test]
fn unit_circle_is_inscribed_in_the_quad() {
    // The fragment discards outside |uv| <= 1, so the round silhouette is the
    // circle inscribed in this square: every corner lies outside it, and the
    // midpoint of each square edge lies exactly on it.
    for corner in DOT_QUAD_CORNERS {
        let r = (corner[0] * corner[0] + corner[1] * corner[1]).sqrt();
        assert!(r > 1.0, "corner {corner:?} is outside the disc");
        assert_close(r, 2.0_f32.sqrt(), "corner radius is the square's half-diagonal");
    }
    let edges = [(0usize, 1usize), (1, 2), (4, 5), (5, 3)];
    for (a, b) in edges {
        let mid: [f32; 2] = [
            0.5 * (DOT_QUAD_CORNERS[a][0] + DOT_QUAD_CORNERS[b][0]),
            0.5 * (DOT_QUAD_CORNERS[a][1] + DOT_QUAD_CORNERS[b][1]),
        ];
        let r = (mid[0] * mid[0] + mid[1] * mid[1]).sqrt();
        assert_close(r, 1.0, "the disc touches the quad edge at its midpoint");
    }
}

#[test]
fn billboard_corners_expand_along_the_camera_basis() {
    // origin + (right * x + up * y) * radius, exactly what the vertex shader
    // does: the dot keeps its world-space radius and its quad faces the camera.
    let origin = Vec3::new(0.25, -0.5, 0.75);
    let radius = 0.125;
    let right = Vec3::X;
    let up = Vec3::Y;
    let corners = dot_billboard_corners(origin, radius, right, up);
    assert_eq!(corners.len(), 6);
    for (corner, uv) in corners.iter().zip(DOT_QUAD_CORNERS.iter()) {
        assert!(corner.is_finite(), "finite world corner: {corner:?}");
        assert_close(corner.z, origin.z, "the quad lies in the camera plane");
        assert_vec_close(
            *corner,
            origin + (right * uv[0] + up * uv[1]) * radius,
            "corner matches the shader's expansion",
        );
        assert_close(
            (*corner - origin).length(),
            radius * 2.0_f32.sqrt(),
            "corner distance round-trips the instance radius",
        );
    }
    // The quad's plane normal is the camera basis' cross product (the axis the
    // dot faces), so every corner offset is perpendicular to it.
    let normal = right.cross(up).normalize();
    for corner in &corners {
        assert_close((*corner - origin).dot(normal), 0.0, "corner is in the billboard plane");
    }
    // The four distinct corners average back to the instance origin.
    let mut sum = Vec3::ZERO;
    for [x, y] in distinct_corners() {
        sum += origin + (right * x + up * y) * radius;
    }
    assert_vec_close(sum * 0.25, origin, "the billboard is centred on the dot");
}

#[test]
fn billboard_corners_scale_with_the_instance_radius() {
    let origin = Vec3::new(1.0, 2.0, -3.0);
    let small = dot_billboard_corners(origin, 0.25, Vec3::X, Vec3::Y);
    let large = dot_billboard_corners(origin, 0.5, Vec3::X, Vec3::Y);
    for (s, l) in small.iter().zip(large.iter()) {
        assert_vec_close(*l - origin, (*s - origin) * 2.0, "radius doubles the offsets");
    }
    let collapsed = dot_billboard_corners(origin, 0.0, Vec3::X, Vec3::Y);
    for corner in collapsed {
        assert_vec_close(corner, origin, "a zero radius collapses the quad onto the centre");
    }
}

#[test]
fn billboard_corners_face_the_camera_from_any_orbit_angle() {
    // The quad's normal must stay parallel to the view direction whatever the
    // orbit: a billboard is camera-facing by construction, never edge-on.
    // (Pitch stays off the pole: look_at is degenerate looking straight down.)
    let mut camera = OrbitCamera::default();
    let origin = Vec3::new(-0.3, 0.2, 0.1);
    for (yaw, pitch) in [(0.0_f32, 0.9_f32), (0.8, 0.9), (-2.1, -1.2), (3.0, 1.5)] {
        camera.yaw = yaw;
        camera.pitch = pitch;
        let vp = camera.vp_matrix(16.0 / 9.0, None);
        let (right, up) = camera_basis_from_view_proj(&vp);
        let normal = right.cross(up).normalize();
        let forward = (camera.target - camera.position()).normalize();
        assert_close(normal.dot(forward).abs(), 1.0, "quad normal faces the camera");
        let corners = dot_billboard_corners(origin, 0.05, right, up);
        for corner in corners {
            assert!(corner.is_finite(), "finite corner at yaw {yaw}, pitch {pitch}");
            assert_close(
                (corner - origin).dot(forward),
                0.0,
                "the quad lies in the plane perpendicular to the view",
            );
        }
    }
}

#[test]
fn camera_basis_matches_the_orbit_camera() {
    let mut camera = OrbitCamera::default();
    for (yaw, pitch) in [(0.0_f32, 0.9_f32), (0.8, 0.9), (-2.1, -1.2)] {
        camera.yaw = yaw;
        camera.pitch = pitch;
        let vp = camera.vp_matrix(1.5, None);
        let (right, up) = camera_basis_from_view_proj(&vp);
        let (expected_right, expected_up) = camera.view_basis();
        assert_vec_close(right, expected_right, "recovered right axis");
        assert_vec_close(up, expected_up, "recovered up axis");
        assert_close(right.length(), 1.0, "right is normalized");
        assert_close(up.length(), 1.0, "up is normalized");
        assert_close(right.dot(up), 0.0, "right and up are perpendicular");
    }
}

#[test]
fn camera_basis_of_the_2d_front_view_is_screen_aligned() {
    // 2D mode paints through `front_view` (looking down -Z), not the orbit, so
    // the basis must come from the view-projection rather than the orbit
    // parameters: screen right is +X and screen up is +Y.
    // An oblique orbit that the front view must ignore.
    let camera = OrbitCamera { yaw: 1.3, pitch: 0.7, ..Default::default() };
    let vp = camera.vp_matrix(4.0 / 3.0, Some(2.0));
    let (right, up) = camera_basis_from_view_proj(&vp);
    assert_vec_close(right, Vec3::X, "2D screen right");
    assert_vec_close(up, Vec3::Y, "2D screen up");
}

#[test]
fn camera_basis_ignores_zoom_and_distance() {
    // Only the orientation matters: zooming or dollying must not rotate the
    // billboards.
    let mut camera = OrbitCamera::default();
    let vp = camera.vp_matrix(1.0, None);
    let (right, up) = camera_basis_from_view_proj(&vp);
    camera.distance = 12.0;
    camera.zoom(2.0);
    let zoomed = camera.vp_matrix(1.0, None);
    let (zoomed_right, zoomed_up) = camera_basis_from_view_proj(&zoomed);
    assert_vec_close(zoomed_right, right, "dollying keeps the right axis");
    assert_vec_close(zoomed_up, up, "dollying keeps the up axis");

    let ortho = camera.vp_matrix(1.0, Some(2.0));
    let ortho_zoomed = {
        camera.zoom_2d(4.0);
        camera.vp_matrix(1.0, Some(2.0))
    };
    let (r1, u1) = camera_basis_from_view_proj(&ortho);
    let (r2, u2) = camera_basis_from_view_proj(&ortho_zoomed);
    assert_vec_close(r1, r2, "2D zoom keeps the right axis");
    assert_vec_close(u1, u2, "2D zoom keeps the up axis");
}

#[test]
fn globals_uniform_layout_carries_the_camera_basis_and_arrow_uniforms() {
    // The WGSL `Globals` struct must stay in step with this Rust struct: the
    // two vec4 camera axes follow the two mat4s and the two existing vec4s,
    // then the arrow style/params vec4s (ticket 45).
    assert_eq!(std::mem::size_of::<Globals>(), 224, "7 x 32-byte members");
    assert_eq!(std::mem::size_of::<Globals>() % 16, 0, "the uniform size stays 16-byte aligned");
    assert_eq!(std::mem::offset_of!(Globals, camera_right), 160, "camera_right offset");
    assert_eq!(std::mem::offset_of!(Globals, camera_up), 176, "camera_up offset");
    assert_eq!(std::mem::offset_of!(Globals, arrow_style), 192, "arrow_style offset");
    assert_eq!(std::mem::offset_of!(Globals, arrow_params), 208, "arrow_params offset");
    let globals = Globals {
        view_proj: Mat4::IDENTITY.to_cols_array_2d(),
        inv_view_proj: Mat4::IDENTITY.to_cols_array_2d(),
        camera_pos: [0.0, 0.0, 0.0, 1.0],
        light_dir: [0.0, 1.0, 0.0, 0.0],
        camera_right: [1.0, 0.0, 0.0, 0.0],
        camera_up: [0.0, 1.0, 0.0, 0.0],
        arrow_style: [0, 12, 0, 0],
        arrow_params: [0.3, 0.12, 0.055, 0.0],
    };
    let bytes = bytemuck::bytes_of(&globals);
    assert_eq!(bytes.len(), 224, "the uniform upload is the full struct");
}

#[test]
fn dot_radius_feeds_the_billboard_quad() {
    // Extends the dot-radius test to the new geometry: `dir_len.w` is still the
    // world-space radius, and it is what the billboard expands by.
    let field = FieldExpr::parse("(1, 0)").expect("pump field parses");
    let domain = Domain::default();
    let mut cloud = Particles::default();
    spawn(&mut cloud, &domain, 16, 3);
    let radius = 0.05;
    let dots = dot_instances(&cloud, &field, &domain, Colormap::Turbo, (0.0, 1.0), radius);
    assert_eq!(dots.len(), 16, "one dot instance per particle");
    let (right, up) = camera_basis_from_view_proj(&OrbitCamera::default().vp_matrix(1.0, None));
    for dot in &dots {
        assert_eq!(dot.dir_len[3], radius, "the shader reads the radius from dir_len.w");
        assert_eq!(dot.color[3], 1.0, "dots stay opaque at the instance level");
        let origin = Vec3::new(dot.origin[0], dot.origin[1], dot.origin[2]);
        for corner in dot_billboard_corners(origin, dot.dir_len[3], right, up) {
            assert_close(
                (corner - origin).length(),
                radius * 2.0_f32.sqrt(),
                "the billboard's corner distance follows the instance radius",
            );
        }
    }
}
