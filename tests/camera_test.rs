//! Orbit-camera coverage: projections, ray unprojection, orbit and zoom.
//!
//! The ticket-02 `render::Camera` test below and the `OrbitCamera` tests moved
//! out of `src/camera.rs`'s inline module (ticket 13) live together here. The
//! moved tests are unchanged: every case already used the public camera API.

use glam::{Mat4, Vec2, Vec3, Vec4};
use vector_field_explorer::camera::OrbitCamera;
use vector_field_explorer::render::Camera;

#[test]
fn world_to_ndc_basic() {
    let cam = Camera { position: [0.0, 0.0], scale: 1.0 };
    let ndc = cam.world_to_ndc([1.0, 2.0]);
    assert!((ndc[0] - 1.0).abs() < 1e-6);
    assert!((ndc[1] - 2.0).abs() < 1e-6);
}

fn assert_close(a: Vec3, b: Vec3, what: &str) {
    let d = (a - b).length();
    assert!(d < 1e-4, "{what}: |{a:?} - {b:?}| = {d} > 1e-4");
}

fn ndc(m: Mat4, p: Vec4) -> Vec3 {
    let v = m * p;
    Vec3::new(v.x, v.y, v.z) / v.w
}

#[test]
fn target_maps_to_vp_center() {
    let c = OrbitCamera::default();
    let vp = c.proj_perspective(1.0) * c.view();
    let ndc = ndc(vp, Vec4::new(0.0, 0.0, 0.0, 1.0));
    // x/y = screen center; z within the frustum (plain perspective_rh puts
    // the target near the far plane when near is tiny — depth-zero exactness
    // would require near/far symmetric about the orbit distance).
    assert_close(Vec3::new(ndc.x, ndc.y, 0.0), Vec3::ZERO, "vp center x/y");
    assert!(ndc.z.abs() < 1.0, "target must lie inside the depth range");
}

#[test]
fn ndc_ray_at_center_looks_at_target() {
    let c = OrbitCamera::default();
    let (origin, dir) = c.ndc_ray(1.0, Vec2::ZERO);
    let to_target = (c.target - origin).normalize();
    assert_close(dir.normalize(), to_target, "ray direction");
}

#[test]
fn zoom_clamps_distance() {
    let mut c = OrbitCamera::default();
    c.zoom(100.0);
    assert!((c.distance - 20.0).abs() < 1e-6);
    c.zoom(0.001);
    assert!((c.distance - 0.5).abs() < 1e-6);
}

#[test]
fn ortho_maps_target_to_center() {
    let c = OrbitCamera::default();
    let m = c.proj_ortho(1.0, 2.0) * c.view();
    let ndc = ndc(m, Vec4::new(0.0, 0.0, 0.0, 1.0));
    // x/y = center (see the perspective-center test's ruling on z).
    assert_close(Vec3::new(ndc.x, ndc.y, 0.0), Vec3::ZERO, "ortho center x/y");
    assert!(ndc.z.abs() < 1.0, "target must lie inside the depth range");
}

#[test]
fn orbit_changes_view() {
    let mut c = OrbitCamera::default();
    let before = c.view();
    c.orbit(0.5, 0.2);
    assert_ne!(before, c.view());
}
