// Moved out of `src/viewmodel.rs`'s inline module (ticket-14 review I-4): the
// pure camera-input seam, 2D front-view pan and clamping, through the public
// API.

use glam::Vec2;
use vector_field_explorer::camera::OrbitCamera;
use vector_field_explorer::viewmodel::{ViewGesture, apply_camera_input};

fn gesture(drag: Vec2, pan: Vec2, zoom: f32, viewport_height: f32) -> ViewGesture {
    ViewGesture { drag, pan, zoom, viewport_height }
}

#[test]
fn camera_input_orbits_zooms_and_pans_per_mode() {
    let home = OrbitCamera::default();
    // 3D: a primary drag orbits, exactly as ticket 02 pinned it.
    let cube = apply_camera_input(
        home,
        true,
        2.7,
        gesture(Vec2::new(100.0, 60.0), Vec2::ZERO, 1.0, 600.0),
    );
    assert!((cube.yaw - (0.8 - 0.5)).abs() < 1e-6, "3D drag orbits by the delta");
    assert!((cube.pitch - (0.9 - 0.3)).abs() < 1e-6);
    // 3D wheel in: egui's zoom 2.0 halves the orbit distance.
    let zoomed = apply_camera_input(home, true, 2.7, gesture(Vec2::ZERO, Vec2::ZERO, 2.0, 600.0));
    assert!((zoomed.distance - 1.6).abs() < 1e-6, "3D mode applies the wheel zoom");
    let pulled = apply_camera_input(home, true, 2.7, gesture(Vec2::ZERO, Vec2::ZERO, 0.01, 600.0));
    assert!((pulled.distance - 20.0).abs() < 1e-6, "zoom clamp stays at 20");

    // 2D: a primary drag pans instead of orbiting (ticket 14 supersedes
    // ticket 02's fixed 2D view) and the wheel zooms the ortho view.
    let flat = apply_camera_input(
        home,
        false,
        2.7,
        gesture(Vec2::new(100.0, 40.0), Vec2::ZERO, 1.0, 600.0),
    );
    assert!((flat.yaw - home.yaw).abs() < 1e-6, "2D drag never orbits");
    assert!((flat.pitch - home.pitch).abs() < 1e-6);
    assert!((flat.distance - home.distance).abs() < 1e-6, "2D drag never moves the 3D eye");
    assert!(flat.target != home.target, "2D drag pans the ortho target");
    let flat_zoom = apply_camera_input(home, false, 2.7, gesture(Vec2::ZERO, Vec2::ZERO, 2.0, 600.0));
    assert!((flat_zoom.ortho_zoom - 2.0).abs() < 1e-6, "2D wheel gets closer");
    assert!(flat_zoom.target == home.target, "2D zoom keeps the target put");
    assert!((flat_zoom.distance - home.distance).abs() < 1e-6, "2D zoom keeps the 3D eye put");
}

#[test]
fn two_d_pan_follows_the_pointer_one_to_one_along_world_xy() {
    // Review I-1: the 2D view is a fixed front view, so panning is along world
    // X/Y, not the orbit camera's tilted screen basis.
    let home = OrbitCamera::default();
    // Visible height 2.0 over a 200-point viewport → 0.01 world per point.
    let right_drag = apply_camera_input(
        home,
        false,
        2.0,
        gesture(Vec2::new(100.0, 0.0), Vec2::ZERO, 1.0, 200.0),
    );
    let delta = right_drag.target - home.target;
    assert!((delta.x + 1.0).abs() < 1e-4, "dragging right slides the scene right: {delta:?}");
    assert!(delta.y.abs() < 1e-4, "a horizontal drag stays horizontal");
    assert!(delta.z.abs() < 1e-4, "a 2D pan never leaves the front plane");
    let down_drag = apply_camera_input(
        home,
        false,
        2.0,
        gesture(Vec2::new(0.0, 100.0), Vec2::ZERO, 1.0, 200.0),
    );
    let delta = down_drag.target - home.target;
    assert!((delta.y - 1.0).abs() < 1e-4, "dragging down slides the scene down: {delta:?}");
    assert!(delta.x.abs() < 1e-4);

    // The pan stays on world axes even when the orbit is tilted: the 2D view
    // ignores the 3D orientation entirely.
    let tilted = OrbitCamera { yaw: 1.9, pitch: 1.2, ..OrbitCamera::default() };
    let tilted_pan = apply_camera_input(
        tilted,
        false,
        2.0,
        gesture(Vec2::new(50.0, 0.0), Vec2::ZERO, 1.0, 200.0),
    );
    let delta = tilted_pan.target - tilted.target;
    assert!((delta.x + 0.5).abs() < 1e-4, "pan is world-X in 2D regardless of yaw: {delta:?}");
    assert!(delta.y.abs() < 1e-4);
}

#[test]
fn two_d_pan_scale_follows_the_zoom() {
    let mut zoomed = OrbitCamera::default();
    zoomed.zoom_2d(2.0);
    // 2x closer shows half the world: the same drag moves half as far.
    let panned = apply_camera_input(
        zoomed,
        false,
        2.0,
        gesture(Vec2::new(100.0, 0.0), Vec2::ZERO, 1.0, 200.0),
    );
    let delta = (panned.target.x - zoomed.target.x).abs();
    assert!((delta - 0.5).abs() < 1e-4, "pan scale halves at 2x zoom: got {delta}");
}

#[test]
fn three_d_right_drag_pans_without_orbiting() {
    let home = OrbitCamera::default();
    let (right, up) = home.view_basis();
    let forward = (home.target - home.position()).normalize();
    let panned = apply_camera_input(
        home,
        true,
        2.7,
        gesture(Vec2::ZERO, Vec2::new(100.0, 0.0), 1.0, 600.0),
    );
    assert!((panned.yaw - home.yaw).abs() < 1e-6, "a pan never orbits");
    assert!((panned.pitch - home.pitch).abs() < 1e-6);
    assert!((panned.distance - home.distance).abs() < 1e-6, "a pan never zooms");
    let delta = panned.target - home.target;
    assert!(delta.dot(right) < 0.0, "dragging right slides the scene right");
    assert!(delta.dot(up).abs() < 1e-4, "a horizontal pan stays horizontal");
    assert!(delta.dot(forward).abs() < 1e-4, "the pan stays in the view plane");
    let up_panned = apply_camera_input(
        home,
        true,
        2.7,
        gesture(Vec2::ZERO, Vec2::new(0.0, 100.0), 1.0, 600.0),
    );
    assert!(
        (up_panned.target - home.target).dot(up) > 0.0,
        "dragging down slides the scene down",
    );
}

#[test]
fn gesture_zoom_and_pan_stay_bounded_with_nonsense_input() {
    let home = OrbitCamera::default();
    let flat = apply_camera_input(home, false, 2.7, gesture(Vec2::ZERO, Vec2::ZERO, 1e9, 600.0));
    assert!(
        (flat.ortho_zoom - OrbitCamera::MAX_ORTHO_ZOOM).abs() < 1e-6,
        "2D zoom-in clamps",
    );
    let flat_out =
        apply_camera_input(home, false, 2.7, gesture(Vec2::ZERO, Vec2::ZERO, 1e-9, 600.0));
    assert!(
        (flat_out.ortho_zoom - OrbitCamera::MIN_ORTHO_ZOOM).abs() < 1e-6,
        "2D zoom-out clamps",
    );
    for nonsense in [f32::NAN, 0.0, -2.0, f32::INFINITY] {
        let same =
            apply_camera_input(home, true, 2.7, gesture(Vec2::ZERO, Vec2::ZERO, nonsense, 600.0));
        assert_eq!(same, home, "a nonsense zoom factor ({nonsense}) leaves the camera alone");
    }
    // A degenerate viewport height must not blow the pan up to infinity.
    let degenerate = apply_camera_input(
        home,
        false,
        2.7,
        gesture(Vec2::new(10.0, 10.0), Vec2::ZERO, 1.0, 0.0),
    );
    assert!(degenerate.target.is_finite(), "pan stays finite when the rect is degenerate");
}

#[test]
fn a_non_finite_ortho_zoom_does_not_poison_the_pan() {
    // Review M-3: after the camera guard, a NaN zoom must not turn the pan
    // scale into a huge jump.
    let camera = OrbitCamera { ortho_zoom: f32::NAN, ..OrbitCamera::default() };
    let panned = apply_camera_input(
        camera,
        false,
        2.0,
        gesture(Vec2::new(100.0, 0.0), Vec2::ZERO, 1.0, 200.0),
    );
    assert!(panned.target.is_finite(), "the pan stays finite");
    assert!((panned.target.x + 1.0).abs() < 1e-4, "the pan keeps its 1:1 scale");
    assert!(panned.ortho_zoom.is_finite(), "the zoom is restored on the next gesture");
}

#[test]
fn a_nonsense_plane_size_or_height_is_ignored() {
    let home = OrbitCamera::default();
    let panned = apply_camera_input(
        home,
        false,
        f32::NAN,
        gesture(Vec2::new(10.0, 0.0), Vec2::ZERO, 1.0, f32::INFINITY),
    );
    assert!(panned.target.is_finite(), "a corrupt plane/height still pans finitely");
}
