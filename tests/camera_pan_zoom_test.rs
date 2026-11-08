// Moved out of the camera/`src` inline test modules (ticket-14 review I-4):
// the 2D view, pan basis and orbit geometry are exercised through the public
// camera API, as `tests/` requires.

use glam::{Mat4, Vec2, Vec3, Vec4};
use vector_field_explorer::camera::OrbitCamera;

fn ndc(m: Mat4, p: Vec4) -> Vec3 {
    let v = m * p;
    Vec3::new(v.x, v.y, v.z) / v.w
}

#[test]
fn ndc_ray_round_trips_off_centre_clicks_and_reaches_the_far_plane() {
    // Fix-round review: the analytic ray must agree with the projection for
    // off-centre clicks and a non-square viewport, and `t == 1` must land on
    // the far plane (view depth 100), not merely at Euclidean distance `FAR`.
    // The `pulled` camera (distance 12.8) is the regime where the old f32
    // view-projection inverse drifted.
    let mut pulled = OrbitCamera::default();
    pulled.zoom(4.0);
    let aspect = 1.6;
    for camera in [OrbitCamera::default(), pulled] {
        let vp = camera.vp_matrix(aspect, None);
        for ndc_xy in [
            Vec2::ZERO,
            Vec2::new(0.5, -0.25),
            Vec2::new(-0.4, 0.3),
            Vec2::new(0.9, 0.9),
        ] {
            let (origin, direction) = camera.ndc_ray(aspect, ndc_xy);
            let point = |t: f32| {
                Vec4::new(
                    origin.x + direction.x * t,
                    origin.y + direction.y * t,
                    origin.z + direction.z * t,
                    1.0,
                )
            };
            let half = ndc(vp, point(0.5));
            assert!((half.x - ndc_xy.x).abs() < 1e-5, "x round-trips at {ndc_xy:?}: {half:?}");
            assert!((half.y - ndc_xy.y).abs() < 1e-5, "y round-trips at {ndc_xy:?}: {half:?}");
            // `t == 1` must be at the far plane's *view depth*, which is what
            // the old normalise-then-scale direction got wrong for off-centre
            // rays (it measured Euclidean distance instead).
            let far_view = camera.view() * point(1.0);
            assert!(
                (far_view.z + 100.0).abs() < 1e-3,
                "t == 1 sits at view depth {} at {ndc_xy:?}, want -100",
                far_view.z
            );
            let far = ndc(vp, point(1.0));
            assert!((far.x - ndc_xy.x).abs() < 1e-4, "far x at {ndc_xy:?}: {far:?}");
            assert!((far.y - ndc_xy.y).abs() < 1e-4, "far y at {ndc_xy:?}: {far:?}");
            assert!((far.z - 1.0).abs() < 1e-4, "t == 1 lands on the far plane at {ndc_xy:?}: {far:?}");
        }
    }
}

#[test]
fn eye_distance_equals_orbit_distance_at_any_orientation() {
    // Review I-2: the spherical eye must sit exactly `distance` from the target
    // for every yaw/pitch, including the pitch clamp limits.
    for (yaw, pitch) in [
        (0.0, 0.0),
        (0.8, 0.9),
        (1.2, 0.4),
        (-2.0, 1.2),
        (std::f32::consts::FRAC_PI_2, 1.55),
        (-std::f32::consts::FRAC_PI_2, -1.55),
        (3.0, 0.0),
        (0.0, 1.55),
    ] {
        let camera = OrbitCamera {
            yaw,
            pitch,
            distance: 3.2,
            target: Vec3::new(1.0, -2.0, 0.5),
            ..OrbitCamera::default()
        };
        let eye_distance = (camera.position() - camera.target).length();
        assert!(
            (eye_distance - camera.distance).abs() < 1e-4,
            "yaw {yaw}, pitch {pitch}: |eye - target| = {eye_distance}, want {}",
            camera.distance,
        );
    }
}

#[test]
fn front_view_looks_down_negative_z_with_y_up() {
    let camera = OrbitCamera { target: Vec3::new(0.5, -1.0, 0.0), ..OrbitCamera::default() };
    let expected = Mat4::look_at_rh(camera.target + Vec3::Z * camera.distance, camera.target, Vec3::Y);
    assert_eq!(camera.front_view(), expected, "the 2D eye sits on +Z, looking at the target");

    let vp = camera.vp_matrix(1.0, Some(2.0));
    let up = ndc(vp, Vec4::new(camera.target.x, camera.target.y + 0.5, camera.target.z, 1.0));
    assert!(up.y > 0.0, "+Y is screen-up in the front view");
    let right = ndc(vp, Vec4::new(camera.target.x + 0.5, camera.target.y, camera.target.z, 1.0));
    assert!(right.x > 0.0, "+X is screen-right in the front view");
}

#[test]
fn two_d_projects_a_square_domain_with_equal_ndc_extents() {
    // Review I-1: the 2D view must not foreshorten the z=0 plane. A square
    // domain in an aspect-1 viewport covers the same NDC extent on both axes.
    let camera = OrbitCamera::default();
    let vp = camera.vp_matrix(1.0, Some(2.0));
    let corners = [
        Vec3::new(-1.0, -1.0, 0.0),
        Vec3::new(1.0, -1.0, 0.0),
        Vec3::new(1.0, 1.0, 0.0),
        Vec3::new(-1.0, 1.0, 0.0),
    ];
    let projected: Vec<Vec3> = corners
        .iter()
        .map(|corner| ndc(vp, Vec4::new(corner.x, corner.y, corner.z, 1.0)))
        .collect();
    let min_x = projected.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
    let max_x = projected.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max);
    let min_y = projected.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
    let max_y = projected.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max);
    let extent_x = max_x - min_x;
    let extent_y = max_y - min_y;
    assert!((extent_x - 2.0).abs() < 1e-4, "the domain spans the full width: {extent_x}");
    assert!((extent_y - 2.0).abs() < 1e-4, "the domain spans the full height: {extent_y}");
    assert!(
        (extent_x - extent_y).abs() < 1e-4,
        "a square domain must not be foreshortened in 2D: x {extent_x} vs y {extent_y}",
    );
    let center = ndc(vp, Vec4::new(camera.target.x, camera.target.y, camera.target.z, 1.0));
    assert!((center.x).abs() < 1e-5 && (center.y).abs() < 1e-5, "the target maps to the center");
}

#[test]
fn view_basis_is_orthonormal_and_perpendicular_to_the_view() {
    let camera = OrbitCamera::default();
    let (right, up) = camera.view_basis();
    let forward = (camera.target - camera.position()).normalize();
    assert!((right.length() - 1.0).abs() < 1e-5, "right is a unit vector");
    assert!((up.length() - 1.0).abs() < 1e-5, "up is a unit vector");
    assert!(right.dot(up).abs() < 1e-5, "the basis is orthogonal");
    assert!(right.dot(forward).abs() < 1e-5, "right is perpendicular to the view");
    assert!(up.dot(forward).abs() < 1e-5, "up is perpendicular to the view");
}

#[test]
fn pan_moves_the_target_in_the_view_plane_only() {
    let mut camera = OrbitCamera::default();
    let forward = (camera.target - camera.position()).normalize();
    let before = camera.target;
    camera.pan(1.0, 0.0);
    let moved = camera.target - before;
    assert!((moved.length() - 1.0).abs() < 1e-5, "one world unit right");
    assert!(moved.dot(forward).abs() < 1e-5, "a pan never changes the view direction");
    assert!(
        (camera.distance - OrbitCamera::default().distance).abs() < 1e-6,
        "pan keeps the zoom",
    );
    let mut second = OrbitCamera::default();
    second.pan(0.0, 2.0);
    let (_, up) = second.view_basis();
    assert!(up.dot(second.target) > 0.0, "a positive up pan moves the target up the screen");
}

#[test]
fn ortho_zoom_scales_the_visible_plane_and_clamps() {
    let mut camera = OrbitCamera::default();
    let plane = 2.0;
    // The 2D view is a front view, so the world +Y axis is screen-up.
    let edge = camera.target + Vec3::Y * (plane * 0.5);
    let edge_ndc = ndc(camera.vp_matrix(1.0, Some(plane)), Vec4::new(edge.x, edge.y, edge.z, 1.0));
    assert!((edge_ndc.y - 1.0).abs() < 1e-4, "default zoom fits the plane exactly");

    camera.zoom_2d(2.0);
    let half = camera.target + Vec3::Y * (plane * 0.25);
    let half_ndc = ndc(camera.vp_matrix(1.0, Some(plane)), Vec4::new(half.x, half.y, half.z, 1.0));
    assert!((half_ndc.y - 1.0).abs() < 1e-4, "2x zoom shows half the world height");
    assert!((camera.ortho_zoom - 2.0).abs() < 1e-6);

    camera.zoom_2d(1e9);
    assert!(
        (camera.ortho_zoom - OrbitCamera::MAX_ORTHO_ZOOM).abs() < 1e-6,
        "zoom-in clamps",
    );
    camera.zoom_2d(1e-9);
    assert!(
        (camera.ortho_zoom - OrbitCamera::MIN_ORTHO_ZOOM).abs() < 1e-6,
        "zoom-out clamps",
    );
    let before = camera.ortho_zoom;
    camera.zoom_2d(f32::NAN);
    assert!(
        (camera.ortho_zoom - before).abs() < 1e-6,
        "a nonsense zoom factor is ignored",
    );
}

#[test]
fn zoom_2d_recovers_from_a_non_finite_ortho_zoom() {
    // Review M-3: a NaN zoom (corrupt storage, guarded preset load) must not
    // stick forever; the next wheel notch resets it to 1 and applies the factor.
    let mut camera = OrbitCamera { ortho_zoom: f32::NAN, ..OrbitCamera::default() };
    camera.zoom_2d(2.0);
    assert_eq!(camera.ortho_zoom, 2.0);
    camera.ortho_zoom = f32::INFINITY;
    camera.zoom_2d(1.0);
    assert!(camera.ortho_zoom.is_finite(), "an infinite zoom resets to a usable value");
}

#[test]
fn two_d_vp_uses_the_front_view_not_the_orbit_view() {
    // The reviewer's arithmetic: with the old oblique view the z=0 plane was
    // foreshortened to ~0.47 of its height. Pin that the 2D projection no
    // longer equals `proj_ortho * view()` and does equal the front view.
    let camera = OrbitCamera::default();
    let ortho = camera.proj_ortho(1.0, 2.0);
    assert_eq!(camera.vp_matrix(1.0, Some(2.0)), ortho * camera.front_view());
    assert_ne!(camera.vp_matrix(1.0, Some(2.0)), ortho * camera.view());
}

#[test]
fn perspective_vp_still_uses_the_orbit_view() {
    let camera = OrbitCamera::default();
    assert_eq!(
        camera.vp_matrix(1.0, None),
        camera.proj_perspective(1.0) * camera.view(),
    );
}

/// The 2D pan basis is world X/Y in the fixed front view; this test pins the
/// projection side of that contract (the input seam has its own coverage).
#[test]
fn front_view_maps_world_xy_one_to_one() {
    let camera = OrbitCamera::default();
    let vp = camera.vp_matrix(1.0, Some(2.0));
    let origin = ndc(vp, Vec4::new(0.0, 0.0, 0.0, 1.0));
    let moved = ndc(vp, Vec4::new(0.5, 0.25, 0.0, 1.0));
    let delta = Vec2::new(moved.x - origin.x, moved.y - origin.y);
    assert!((delta.x - 0.5).abs() < 1e-5, "half a world unit is half an NDC unit");
    assert!((delta.y - 0.25).abs() < 1e-5, "x and y are equally scaled");
}
