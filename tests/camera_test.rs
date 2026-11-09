//! Orbit-camera coverage: projections, ray unprojection, orbit and zoom.
//!
//! The ticket-02 `render::Camera` test below and the `OrbitCamera` tests moved
//! out of `src/camera.rs`'s inline module (ticket 13) live together here. The
//! moved tests are unchanged: every case already used the public camera API.

use glam::{Mat4, Vec2, Vec3, Vec4};
use vector_field_explorer::camera::OrbitCamera;
use vector_field_explorer::domain::Domain;
use vector_field_explorer::render::Camera;
use vector_field_explorer::view_command::{
    ViewCommand, ViewContext, apply_view_command, domain_centre_delta,
};

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

#[test]
fn recentre_on_targets_the_domain_centre_without_changing_the_pose() {
    let mut camera = OrbitCamera {
        yaw: 1.3,
        pitch: -0.45,
        distance: 7.5,
        target: Vec3::new(9.0, -4.0, 2.0),
        fov_y: 0.8,
        ortho_zoom: 4.0,
    };
    let before = camera;
    // Asymmetric on every axis so a centre that only follows x/y cannot pass.
    let domain = Domain::new(Vec3::new(-3.0, 2.0, -1.0), Vec3::new(1.0, 6.0, 4.0));

    camera.recentre_on(&domain);

    assert_eq!(camera.target, Vec3::new(-1.0, 4.0, 1.5), "the target is the box centre");
    assert_eq!(camera.yaw, before.yaw, "recentring keeps yaw");
    assert_eq!(camera.pitch, before.pitch, "recentring keeps pitch");
    assert_eq!(camera.distance, before.distance, "recentring keeps the orbit distance");
    assert_eq!(camera.fov_y, before.fov_y, "recentring keeps the field of view");
    assert_eq!(camera.ortho_zoom, before.ortho_zoom, "recentring keeps the 2D zoom");
}

#[test]
fn recentre_on_2d_follows_x_and_y_but_leaves_z_and_panning_still_works() {
    let mut camera = OrbitCamera {
        target: Vec3::new(0.5, -0.5, 0.75),
        ortho_zoom: 2.5,
        ..OrbitCamera::default()
    };
    let domain = Domain::new(Vec3::new(-3.0, 2.0, -1.0), Vec3::new(1.0, 6.0, 4.0));

    camera.recentre_on_2d(&domain);

    assert_eq!(camera.target, Vec3::new(-1.0, 4.0, 0.75), "2D keeps the depth target untouched");
    assert_eq!(camera.ortho_zoom, 2.5, "2D recentring keeps the ortho zoom unchanged");

    // The 2D pan is a world X/Y shift through the command seam; it must still
    // move the recentred target and keep the zoom.
    let panned = apply_view_command(
        camera,
        ViewContext::new(false, domain, 1.5),
        ViewCommand::Pan { dx: 0.5, dy: -0.25 },
    );
    assert_eq!(panned.target, Vec3::new(-0.5, 3.75, 0.75), "a 2D pan still moves the target");
    assert_eq!(panned.ortho_zoom, camera.ortho_zoom, "a 2D pan keeps the zoom");
}

#[test]
fn domain_centre_delta_is_zero_for_equal_domains_and_follows_moved_ones() {
    let domain = Domain::new(Vec3::new(-3.0, 2.0, -1.0), Vec3::new(1.0, 6.0, 4.0));
    assert_eq!(
        domain_centre_delta(&domain, &domain),
        Vec3::ZERO,
        "an unchanged domain needs no camera shift",
    );

    let moved = Domain::new(Vec3::new(5.0, 5.0, 2.0), Vec3::new(9.0, 9.0, 7.0));
    assert_eq!(
        domain_centre_delta(&domain, &moved),
        Vec3::new(8.0, 3.0, 3.0),
        "the delta is the shift between the two centres",
    );
}
