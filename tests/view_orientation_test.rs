//! Issue #208: display-only vertical world axis.
//!
//! Orientation is a presentation setting. The default Z mode is the physics
//! convention and puts world +Z on screen up. The old historical view is now the
//! Y option. World coordinates, reference geometry, scene records and history
//! never move.

use glam::{Mat4, Vec2, Vec3, Vec4};

use crate::support;

use vector_field_explorer::VfApp;
use vector_field_explorer::camera::OrbitCamera;
use vector_field_explorer::domain::Domain;
use vector_field_explorer::persistence::{AppSettings, ViewRecord};
use vector_field_explorer::reference::{AXIS_X_COLOR, AXIS_Y_COLOR, AXIS_Z_COLOR, reference_lines};
use vector_field_explorer::screenshot;
use vector_field_explorer::ui::sidebar::Controls;
use vector_field_explorer::view_orientation::UpAxis;
use vector_field_explorer::viewmodel::State;

use support::storage::{from_ron, ron_string};

fn ndc(matrix: Mat4, point: Vec3) -> Vec3 {
    let clip = matrix * Vec4::new(point.x, point.y, point.z, 1.0);
    Vec3::new(clip.x, clip.y, clip.z) / clip.w
}

fn close(a: Vec3, b: Vec3, what: &str) {
    let distance = (a - b).length();
    assert!(
        distance < 1e-5,
        "{what}: |{a:?} - {b:?}| = {distance} > 1e-5"
    );
}

fn close_scalar(a: f32, b: f32, what: &str) {
    assert!((a - b).abs() < 1e-5, "{what}: {a} != {b}");
}

/// The world axis named by the control's label, so a label/rendering mismatch
/// fails the test instead of drifting silently.
fn labelled_world_axis(label: &str) -> Vec3 {
    match label.chars().next() {
        Some('X') => Vec3::X,
        Some('Y') => Vec3::Y,
        Some('Z') => Vec3::Z,
        other => panic!("unexpected up-axis label {label:?} ({other:?})"),
    }
}

#[test]
fn z_is_the_default_up_axis() {
    assert_eq!(UpAxis::default(), UpAxis::Z);
    assert_eq!(UpAxis::ALL, [UpAxis::Z, UpAxis::X, UpAxis::Y]);
    assert_eq!(OrbitCamera::default().up_axis, UpAxis::Z);
    assert_eq!(State::new(false).up_axis, UpAxis::Z);
    assert_eq!(State::new(true).up_axis, UpAxis::Z);
    assert_eq!(UpAxis::Z.label(), "Z up (default)");
    assert_eq!(UpAxis::X.label(), "X up");
    assert_eq!(UpAxis::Y.label(), "Y up");
}

#[test]
fn z_up_is_the_default_and_projects_world_z_to_screen_up() {
    let camera = OrbitCamera::default();
    assert_eq!(camera.up_axis, UpAxis::Z);
    let projected = ndc(camera.vp_matrix(1.3, None), camera.target + Vec3::Z);
    assert!(
        projected.y > 0.1,
        "the default view must put world +Z up on screen, got {projected:?}"
    );

    // The Y option is the old historical screen orientation: it still
    // reproduces the pre-#208 formula exactly, but it is no longer the default.
    let legacy = OrbitCamera {
        up_axis: UpAxis::Y,
        ..OrbitCamera::default()
    };
    let legacy_position = legacy.target
        + legacy.distance
            * Vec3::new(
                legacy.yaw.cos() * legacy.pitch.sin(),
                legacy.pitch.cos(),
                legacy.yaw.sin() * legacy.pitch.sin(),
            );
    assert_eq!(
        legacy.position(),
        legacy_position,
        "the Y option keeps the pre-#208 eye formula bit for bit"
    );
    assert_eq!(
        legacy.view(),
        Mat4::look_at_rh(legacy_position, legacy.target, Vec3::Y),
        "the Y option keeps the pre-#208 world-to-eye transform"
    );
    assert_ne!(
        camera.position(),
        legacy.position(),
        "the new default Z-up view intentionally differs from the old Y-up view"
    );
    assert_ne!(
        camera.view(),
        legacy.view(),
        "the new default Z-up view intentionally differs from the old Y-up view"
    );
}

#[test]
fn y_option_keeps_the_legacy_orbit_and_pan_basis() {
    let target = Vec3::new(1.0, -2.0, 0.5);
    let mut wild = OrbitCamera {
        yaw: 1.3,
        pitch: -0.45,
        distance: 7.5,
        target,
        fov_y: 0.8,
        up_axis: UpAxis::Y,
        ..OrbitCamera::default()
    };
    let legacy_position = target
        + wild.distance
            * Vec3::new(
                wild.yaw.cos() * wild.pitch.sin(),
                wild.pitch.cos(),
                wild.yaw.sin() * wild.pitch.sin(),
            );
    assert_eq!(wild.position(), legacy_position);
    assert_eq!(
        wild.view(),
        Mat4::look_at_rh(legacy_position, target, Vec3::Y)
    );

    // Orbit and pan keep following the unchanged legacy basis.
    let before = wild;
    wild.orbit(0.3, -0.2);
    assert_ne!(wild.yaw, before.yaw);
    let panned = wild;
    wild.pan(0.4, -0.1);
    let delta = wild.target - panned.target;
    let (right, up) = panned.view_basis();
    close(delta, right * 0.4 + up * -0.1, "legacy pan basis");
}

#[test]
fn each_axis_has_an_orthonormal_right_handed_basis() {
    for axis in UpAxis::ALL {
        let (up, forward) = axis.basis();
        close_scalar(up.length(), 1.0, "up length");
        close_scalar(forward.length(), 1.0, "forward length");
        close_scalar(up.dot(forward), 0.0, "up/forward orthogonality");

        let rotation = axis.rotation();
        close_scalar(rotation.length(), 1.0, "rotation is a unit quaternion");
        let rotation_matrix = Mat4::from_quat(rotation);
        assert!(
            rotation_matrix.determinant() > 0.0,
            "{axis:?} must be a proper (positive-determinant) view rotation"
        );
        close(
            rotation * up,
            Vec3::Y,
            "the chosen world up maps to display +Y",
        );
        close(
            rotation * forward.cross(up),
            Vec3::Z,
            "the basis stays right-handed after the display rotation",
        );

        let camera = OrbitCamera {
            up_axis: axis,
            ..OrbitCamera::default()
        };
        let (right, screen_up) = camera.view_basis();
        close_scalar(right.length(), 1.0, "camera right length");
        close_scalar(screen_up.length(), 1.0, "camera screen-up length");
        close_scalar(right.dot(screen_up), 0.0, "camera basis orthogonality");
    }
}

#[test]
fn every_label_names_the_world_axis_that_projects_to_screen_up() {
    for axis in UpAxis::ALL {
        let world_up = labelled_world_axis(axis.label());
        close(
            axis.basis().0,
            world_up,
            "the label matches the camera's world up vector",
        );
        let camera = OrbitCamera {
            up_axis: axis,
            ..OrbitCamera::default()
        };
        let projected = ndc(camera.vp_matrix(1.3, None), camera.target + world_up);
        assert!(
            projected.y > 0.1,
            "{} must put world {world_up:?} up on screen, got {projected:?}",
            axis.label()
        );
    }
    assert_eq!(
        labelled_world_axis(UpAxis::Z.label()),
        Vec3::Z,
        "the default is honestly labelled Z-up"
    );
}

#[test]
fn chosen_axis_projects_to_screen_up_in_3d() {
    for axis in UpAxis::ALL {
        let camera = OrbitCamera {
            up_axis: axis,
            ..OrbitCamera::default()
        };
        let vp = camera.vp_matrix(1.3, None);
        let up = labelled_world_axis(axis.label());
        let projected = ndc(vp, camera.target + up);
        assert!(
            projected.y > 0.1,
            "{axis:?}: the chosen up vector must point up on screen, got {projected:?}"
        );

        // Fit and reset keep the chosen orientation rather than falling back
        // to the default; `Fit` only moves the target and distance.
        let mut fitted = camera;
        fitted.recentre_on(&Domain::new(
            Vec3::new(-2.0, -1.0, -3.0),
            Vec3::new(2.0, 3.0, 1.0),
        ));
        fitted.distance = 9.0;
        assert_eq!(fitted.up_axis, axis);
        let fitted_up = ndc(fitted.vp_matrix(1.0, None), fitted.target + up);
        assert!(
            fitted_up.y > 0.1,
            "{axis:?}: fit keeps the chosen up on screen"
        );
    }
}

#[test]
fn display_rotation_round_trips_world_positions() {
    let points = [
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.0, -2.0, 3.0),
        Vec3::new(-0.25, 4.5, -8.0),
        Vec3::new(1e-4, -1e-4, 2e-5),
    ];
    for axis in UpAxis::ALL {
        let rotation = axis.rotation();
        for point in points {
            close(
                rotation.inverse() * (rotation * point),
                point,
                "display rotation round trip",
            );
        }
    }
}

#[test]
fn reference_axes_keep_world_geometry_and_colours_for_every_axis() {
    let domain = Domain::new(Vec3::new(-1.0, -2.0, -3.0), Vec3::new(1.0, 2.0, 3.0));
    let lines = reference_lines(&domain, true);
    for color in [AXIS_X_COLOR, AXIS_Y_COLOR, AXIS_Z_COLOR] {
        assert!(
            lines.iter().any(|line| line.color == color),
            "the world axis colours must survive orientation changes"
        );
    }

    // Reference geometry is world-space input; it is identical for every up
    // axis because only the camera changes. This pins the display-only rule.
    let plane = reference_lines(&domain, false);
    assert!(!plane.is_empty());
    let same_geometry = reference_lines(&domain, true);
    assert_eq!(same_geometry.len(), lines.len());
    for (left, right) in lines.iter().zip(&same_geometry) {
        assert_eq!(left.a, right.a);
        assert_eq!(left.b, right.b);
        assert_eq!(left.color, right.color);
    }
    for axis in UpAxis::ALL {
        let camera = OrbitCamera {
            up_axis: axis,
            ..OrbitCamera::default()
        };
        if axis != UpAxis::Z {
            assert_ne!(
                camera.vp_matrix(1.0, None),
                OrbitCamera::default().vp_matrix(1.0, None),
                "{axis:?} must change the 3D view"
            );
        }
    }
}

#[test]
fn two_d_ignores_up_axis_in_camera_and_svg() {
    let base = OrbitCamera::default();
    let scene = vector_field_explorer::renderer::SceneGPU {
        lines: std::sync::Arc::new(reference_lines(&Domain::default(), false)),
        ..Default::default()
    };
    let width = 160;
    let height = 120;
    let base_svg = screenshot::scene_to_svg(
        &scene,
        base.vp_matrix(1.0, Some(2.0)),
        width,
        height,
        [0.0; 4],
    );
    // The SVG byte comparison below is the load-bearing assertion; the
    // front-view and projection checks document why it must match.
    for axis in UpAxis::ALL {
        let camera = OrbitCamera {
            up_axis: axis,
            ..OrbitCamera::default()
        };
        assert_eq!(
            camera.front_view(),
            base.front_view(),
            "{axis:?}: the 2D front view is unchanged"
        );
        assert_eq!(
            camera.vp_matrix(1.0, Some(2.0)),
            base.vp_matrix(1.0, Some(2.0)),
            "{axis:?}: the 2D projection ignores orientation"
        );
        assert_eq!(
            screenshot::scene_to_svg(
                &scene,
                camera.vp_matrix(1.0, Some(2.0)),
                width,
                height,
                [0.0; 4],
            ),
            base_svg,
            "{axis:?}: 2D SVG output is byte-identical"
        );
    }
}

#[test]
fn switching_up_axis_leaves_document_dirty_flag_history_and_view_record_unchanged() {
    let mut app = VfApp::new();
    app.state.dirty = false;
    let before_history = app.history_len();
    let before_view = ViewRecord::capture(&app.state);
    app.state.up_axis = UpAxis::X;
    assert_eq!(app.state.up_axis, UpAxis::X);
    assert!(!app.state.dirty, "orientation never dirties the document");
    assert_eq!(
        app.history_len(),
        before_history,
        "orientation never records an undo entry"
    );
    assert_eq!(
        ViewRecord::capture(&app.state),
        before_view,
        "ViewRecord never carries the display orientation"
    );

    let json = serde_json::to_string(&ViewRecord::capture(&app.state)).unwrap();
    assert!(!json.contains("up_axis"));
}

#[test]
fn settings_restore_applies_up_axis_without_extra_dirtying() {
    // The view record's own setters may dirty the first scene. Vary only the
    // display axis and prove it adds no dirt and no document difference.
    let z_settings = AppSettings {
        up_axis: UpAxis::Z,
        ..AppSettings::default()
    };
    let y_settings = AppSettings {
        up_axis: UpAxis::Y,
        ..AppSettings::default()
    };

    let mut z_state = State::new(true);
    let mut z_controls = Controls::new();
    z_settings.restore(&mut z_state, &mut z_controls);

    let mut y_state = State::new(true);
    let mut y_controls = Controls::new();
    y_settings.restore(&mut y_state, &mut y_controls);

    assert_eq!(z_state.up_axis, UpAxis::Z);
    assert_eq!(y_state.up_axis, UpAxis::Y);
    assert_eq!(
        z_state.dirty, y_state.dirty,
        "the restored display axis adds no document dirt"
    );
    assert_eq!(
        ViewRecord::capture(&z_state),
        ViewRecord::capture(&y_state),
        "the restored display axis never changes the document"
    );

    // A direct startup assignment never dirties on its own.
    let mut direct = State::new(true);
    direct.dirty = false;
    direct.up_axis = UpAxis::Z;
    assert!(!direct.dirty);
}

#[test]
fn up_axis_round_trips_and_repairs_unknown_values() {
    for axis in UpAxis::ALL {
        let settings = AppSettings {
            up_axis: axis,
            ..AppSettings::default()
        };
        let token = match axis {
            UpAxis::Y => "y",
            UpAxis::Z => "z",
            UpAxis::X => "x",
        };
        let raw = ron_string(&settings);
        assert!(
            raw.contains(&format!("up_axis:{token}")),
            "the stable lowercase token is written: {raw}"
        );
        let loaded: AppSettings = from_ron(&raw);
        assert_eq!(loaded.up_axis, axis);
    }

    let default_raw = ron_string(&AppSettings::default());
    let y_raw = default_raw.replace("up_axis:z", "up_axis:y");
    assert_eq!(
        from_ron::<AppSettings>(&y_raw).up_axis,
        UpAxis::Y,
        "the stored token \"y\" means Y-up, the old historical screen orientation"
    );
    let unknown = default_raw.replace("up_axis:z", "up_axis:q");
    assert_ne!(unknown, default_raw);
    let loaded: AppSettings = from_ron(&unknown);
    assert_eq!(
        loaded.up_axis,
        UpAxis::Z,
        "an unknown stored axis is repaired to Z"
    );

    let missing = default_raw.replace(",up_axis:z", "");
    assert!(!missing.contains("up_axis"));
    let loaded: AppSettings = from_ron(&missing);
    assert_eq!(loaded.up_axis, UpAxis::Z, "a missing axis defaults to Z");

    let wrong_shape = default_raw.replace("up_axis:z", "up_axis:7");
    let loaded: AppSettings = from_ron(&wrong_shape);
    assert_eq!(
        loaded.up_axis,
        UpAxis::Z,
        "a non-string axis is repaired to Z without failing the envelope"
    );
}

#[test]
fn app_restore_applies_up_axis_to_the_live_camera() {
    let settings = AppSettings {
        up_axis: UpAxis::X,
        ..AppSettings::default()
    };
    let mut storage = support::storage::MemoryStorage::default();
    vector_field_explorer::persistence::save(&mut storage, &settings);
    let app = VfApp::new_with_storage(Some(&storage));
    assert_eq!(app.state.up_axis, UpAxis::X);
    assert_eq!(app.orbit.up_axis, UpAxis::X);
    assert_eq!(
        app.orbit.view(),
        OrbitCamera {
            up_axis: UpAxis::X,
            ..OrbitCamera::default()
        }
        .view()
    );
}

#[test]
fn camera_ndc_rays_pick_in_world_space_for_every_axis() {
    for axis in UpAxis::ALL {
        let camera = OrbitCamera {
            up_axis: axis,
            ..OrbitCamera::default()
        };
        let aspect = 1.5;
        let vp = camera.vp_matrix(aspect, None);
        for ndc_xy in [Vec2::ZERO, Vec2::new(0.4, -0.3), Vec2::new(-0.8, 0.7)] {
            let (origin, direction) = camera.ndc_ray(aspect, ndc_xy);
            let at_half = origin + direction * 0.5;
            let projected = ndc(vp, at_half);
            close_scalar(projected.x, ndc_xy.x, "ray/pick x round trip");
            close_scalar(projected.y, ndc_xy.y, "ray/pick y round trip");
        }
    }
}
