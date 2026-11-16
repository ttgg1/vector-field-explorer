//! Pure plot-camera geometry evidence; these tests never acquire a GPU device.

use eframe::egui::{Key, Modifiers};
use glam::{DVec3, Vec2, Vec3};
use vector_field_explorer::camera::OrbitCamera;
use vector_field_explorer::domain::Domain;
use vector_field_explorer::plot::{PlotBounds, PlotKind};
use vector_field_explorer::plot_camera::{PlotCamera, PlotFrame};
use vector_field_explorer::view_command::{
    ViewCommand, ViewContext, apply_view_command, command_for_key, fit_distance,
};
use vector_field_explorer::viewmodel::ORBIT_SENSITIVITY;
use vector_field_explorer::viewmodel::{PLANE_MARGIN, ViewGesture, apply_camera_input};

fn bounds(min: DVec3, max: DVec3) -> PlotBounds {
    PlotBounds { min, max }
}

fn corners(bounds: PlotBounds) -> [DVec3; 8] {
    std::array::from_fn(|i| {
        DVec3::new(
            if i & 1 == 0 {
                bounds.min.x
            } else {
                bounds.max.x
            },
            if i & 2 == 0 {
                bounds.min.y
            } else {
                bounds.max.y
            },
            if i & 4 == 0 {
                bounds.min.z
            } else {
                bounds.max.z
            },
        )
    })
}

fn frame_bits(frame: PlotFrame) -> Vec<u32> {
    frame
        .view_proj
        .to_cols_array()
        .into_iter()
        .chain(frame.eye.to_array())
        .chain([frame.aspect])
        .map(f32::to_bits)
        .collect()
}

fn state_bits(camera: &PlotCamera) -> Vec<u64> {
    camera
        .target
        .to_array()
        .into_iter()
        .chain([
            camera.distance,
            camera.ortho_extent,
            camera.yaw,
            camera.pitch,
            camera.fov_y,
        ])
        .chain(camera.fitted_bounds.min.to_array())
        .chain(camera.fitted_bounds.max.to_array())
        .map(f64::to_bits)
        .collect()
}

fn assert_framed(camera: &PlotCamera, kind: PlotKind, aspect: f32) {
    let frame = camera.frame(kind, aspect).unwrap();
    assert!(frame.eye.is_finite());
    assert!(frame.view_proj.is_finite());
    assert_eq!(frame.aspect, aspect);
    for corner in corners(camera.fitted_bounds) {
        let clip = frame.view_proj * corner.as_vec3().extend(1.0);
        assert!(clip.is_finite(), "{corner:?}: {clip:?}");
        assert!(clip.w > 0.0, "{corner:?}: non-positive w {clip:?}");
        let ndc = clip.truncate() / clip.w;
        assert!(
            ndc.x.abs() <= 1.0 / 1.1 + 2e-5,
            "{corner:?}: x outside 10% framing margin: {ndc:?}"
        );
        assert!(
            ndc.y.abs() <= 1.0 / 1.1 + 2e-5,
            "{corner:?}: y outside 10% framing margin: {ndc:?}"
        );
        assert!(
            (-2e-5..=1.0 + 2e-5).contains(&ndc.z),
            "{corner:?}: WebGPU depth outside [0,1]: {ndc:?}"
        );
    }
}

fn assert_rejected(
    camera: &mut PlotCamera,
    kind: PlotKind,
    operation: impl FnOnce(
        &mut PlotCamera,
    ) -> Result<(), vector_field_explorer::plot_camera::PlotFitError>,
) {
    let state = state_bits(camera);
    let debug = format!("{camera:?}");
    let frame = camera.frame(kind, 1.0).ok().map(frame_bits);
    assert!(operation(camera).is_err());
    assert_eq!(state_bits(camera), state);
    assert_eq!(format!("{camera:?}"), debug);
    assert_eq!(camera.frame(kind, 1.0).ok().map(frame_bits), frame);
}

#[test]
fn fit_tall_plot_in_portrait_and_landscape() {
    // y = 1e6*x, and a surface whose heights are far outside domain Z.
    let curve = bounds(DVec3::new(-1.0, -1e6, 0.0), DVec3::new(1.0, 1e6, 0.0));
    let surface = bounds(DVec3::new(-1.0, -1.0, 0.0), DVec3::new(1.0, 1.0, 1e6));
    for aspect in [0.4, 2.5] {
        for (kind, sampled) in [(PlotKind::Curve2D, curve), (PlotKind::Surface3D, surface)] {
            let mut camera = PlotCamera::default();
            camera.fit(sampled, kind, aspect).unwrap();
            assert_framed(&camera, kind, aspect);
            assert!(camera.fitted_bounds.min.cmple(sampled.min).all());
            assert!(camera.fitted_bounds.max.cmpge(sampled.max).all());
            // Plot fitting must not inherit the vector camera's distance clamp.
            assert!(camera.distance > f64::from(OrbitCamera::MAX_DISTANCE));
        }
    }

    // Wide geometry in a portrait viewport exercises the horizontal FOV limit.
    let wide = bounds(DVec3::new(-1e6, -1.0, -1.0), DVec3::new(1e6, 1.0, 1.0));
    for kind in [PlotKind::Curve2D, PlotKind::Surface3D] {
        for aspect in [0.4, 2.5] {
            let mut camera = PlotCamera::default();
            camera.fit(wide, kind, aspect).unwrap();
            assert_framed(&camera, kind, aspect);
        }
    }
}

#[test]
fn flat_and_single_axis_bounds_are_padded() {
    let cases = [
        bounds(DVec3::ZERO, DVec3::ZERO),
        bounds(DVec3::splat(1e-5), DVec3::splat(1e-5)),
        bounds(DVec3::ZERO, DVec3::new(0.0, 1e6, 0.0)),
        bounds(DVec3::ZERO, DVec3::new(4.0, 2.0, 0.0)),
    ];
    for kind in [PlotKind::Curve2D, PlotKind::Surface3D] {
        for input in cases {
            let span = input.max - input.min;
            let padding = (span.max_element() * 1e-6).max(1e-6);
            let mut expected = input;
            for axis in 0..3 {
                if span[axis] == 0.0 {
                    expected.min[axis] -= padding;
                    expected.max[axis] += padding;
                }
            }
            for aspect in [0.4, 2.5] {
                let mut camera = PlotCamera::default();
                camera.fit(input, kind, aspect).unwrap();
                for axis in 0..3 {
                    let tolerance = expected.max[axis].abs().max(1.0) * 1e-12;
                    assert!(
                        (camera.fitted_bounds.min[axis] - expected.min[axis]).abs() <= tolerance,
                        "axis {axis}: {:?} != {:?}",
                        camera.fitted_bounds,
                        expected
                    );
                    assert!(
                        (camera.fitted_bounds.max[axis] - expected.max[axis]).abs() <= tolerance,
                        "axis {axis}: {:?} != {:?}",
                        camera.fitted_bounds,
                        expected
                    );
                }
                assert_framed(&camera, kind, aspect);
            }
        }
    }
}

#[test]
fn invalid_fit_preserves_camera() {
    let mut camera = PlotCamera::default();
    let kind = PlotKind::Curve2D;
    camera
        .fit(
            bounds(DVec3::new(-1.0, -1e6, 0.0), DVec3::new(1.0, 1e6, 0.0)),
            kind,
            0.4,
        )
        .unwrap();
    let state = state_bits(&camera);
    let debug = format!("{camera:?}");
    let before = frame_bits(camera.frame(kind, 0.4).unwrap());
    let valid = bounds(DVec3::new(-1.0, -1e6, 0.0), DVec3::new(1.0, 1e6, 0.0));

    for aspect in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(camera.fit(valid, kind, aspect).is_err());
        assert_eq!(state_bits(&camera), state);
        assert_eq!(format!("{camera:?}"), debug);
        assert_eq!(frame_bits(camera.frame(kind, 0.4).unwrap()), before);
    }
    for invalid in [
        bounds(DVec3::new(f64::NAN, 0.0, 0.0), DVec3::ONE),
        bounds(DVec3::ZERO, DVec3::new(f64::INFINITY, 1.0, 1.0)),
        bounds(DVec3::new(0.0, f64::NEG_INFINITY, 0.0), DVec3::ONE),
        bounds(DVec3::new(2.0, 0.0, 0.0), DVec3::ONE),
        bounds(DVec3::new(0.0, 2.0, 0.0), DVec3::ONE),
        bounds(DVec3::new(0.0, 0.0, 2.0), DVec3::ONE),
        bounds(DVec3::splat(-f64::MAX), DVec3::splat(f64::MAX)),
        bounds(DVec3::splat(1e100), DVec3::splat(1e100)),
    ] {
        assert!(camera.fit(invalid, kind, 0.4).is_err());
        assert_eq!(state_bits(&camera), state);
        assert_eq!(format!("{camera:?}"), debug);
        assert_eq!(frame_bits(camera.frame(kind, 0.4).unwrap()), before);
    }
}

#[test]
fn plot_navigation_and_domain_fit_are_separate() {
    let domain = Domain::new(Vec3::new(2.0, 4.0, -3.0), Vec3::new(4.0, 8.0, 5.0));
    for kind in [PlotKind::Curve2D, PlotKind::Surface3D] {
        let sampled = bounds(DVec3::new(-1.0, -1e6, 0.0), DVec3::new(1.0, 1e6, 0.0));
        let mut camera = PlotCamera::default();
        camera.fit(sampled, kind, 0.4).unwrap();
        let angles = (camera.yaw, camera.pitch);
        assert_eq!(camera.target.x, 0.0);
        assert_eq!(camera.target.y, 0.0);

        camera
            .apply_command(kind, ViewCommand::Fit, &domain, 0.4)
            .unwrap();
        assert_eq!(camera.target, domain.center().as_dvec3());
        assert_eq!((camera.yaw, camera.pitch), angles);
        assert_eq!(camera.fitted_bounds.min, domain.min.as_dvec3());
        assert_eq!(camera.fitted_bounds.max, domain.max.as_dvec3());
        assert_framed(&camera, kind, 0.4);

        let retained = camera.fitted_bounds;
        let target = camera.target;
        camera
            .apply_command(kind, ViewCommand::Pan { dx: 0.25, dy: 0.5 }, &domain, 0.4)
            .unwrap();
        assert_ne!(camera.target, target);
        if kind == PlotKind::Curve2D {
            let delta = DVec3::new(0.25, 0.5, 0.0);
            assert!((camera.target - target - delta).length() < 1e-12);
        }
        assert_eq!(camera.fitted_bounds.min, retained.min);
        assert_eq!(camera.fitted_bounds.max, retained.max);

        camera
            .apply_gesture(
                kind,
                ViewGesture {
                    drag: Vec2::new(10.0, 5.0),
                    pan: Vec2::new(3.0, -2.0),
                    zoom: 1.1,
                    viewport_height: 600.0,
                },
                600.0,
            )
            .unwrap();
        assert!(camera.frame(kind, 0.4).unwrap().view_proj.is_finite());
        assert_eq!(camera.fitted_bounds.min, retained.min);
        assert_eq!(camera.fitted_bounds.max, retained.max);

        camera
            .apply_command(kind, ViewCommand::Reset, &domain, 0.4)
            .unwrap();
        assert_eq!(camera.target, domain.center().as_dvec3());
        assert_eq!(camera.yaw, PlotCamera::default().yaw);
        assert_eq!(camera.pitch, PlotCamera::default().pitch);
        assert!(camera.frame(kind, 0.4).unwrap().view_proj.is_finite());
    }

    // Existing vector Fit semantics remain untouched.
    let huge = Domain::new(Vec3::splat(-100.0), Vec3::splat(100.0));
    let tiny = Domain::new(Vec3::splat(-0.001), Vec3::splat(0.001));
    for (domain, distance) in [(huge, 20.0), (tiny, 0.5)] {
        let vector = apply_view_command(
            OrbitCamera::default(),
            ViewContext::new(true, domain, 1.0),
            ViewCommand::Fit,
        );
        assert_eq!(vector.distance, distance);
    }
}

#[test]
fn ordinary_gestures_match_vector_input_conventions() {
    let gesture = ViewGesture {
        drag: Vec2::new(20.0, 10.0),
        pan: Vec2::new(12.0, 8.0),
        zoom: 2.0,
        viewport_height: 600.0,
    };
    for (kind, mode_3d) in [(PlotKind::Curve2D, false), (PlotKind::Surface3D, true)] {
        let vector =
            apply_camera_input(OrbitCamera::default(), mode_3d, 2.0 * PLANE_MARGIN, gesture);
        let mut plot = PlotCamera::default();
        plot.apply_gesture(kind, gesture, 600.0).unwrap();
        assert!((plot.target - vector.target.as_dvec3()).length() < 1e-6);
        assert!((plot.yaw - f64::from(vector.yaw)).abs() < 1e-6);
        assert!((plot.pitch - f64::from(vector.pitch)).abs() < 1e-6);
        if mode_3d {
            assert!((plot.distance - f64::from(vector.distance)).abs() < 1e-6);
        } else {
            let extent = 2.0 * f64::from(PLANE_MARGIN / vector.ortho_zoom);
            assert!((plot.ortho_extent - extent).abs() < 1e-6);
        }
        assert!(plot.frame(kind, 1.0).unwrap().view_proj.is_finite());
    }

    // Front view stays +Y up, looks down -Z, and ignores orbit commands.
    let mut front = PlotCamera::default();
    let old = front.clone();
    front
        .apply_gesture(
            PlotKind::Curve2D,
            ViewGesture {
                drag: Vec2::new(10.0, 20.0),
                pan: Vec2::new(5.0, -10.0),
                zoom: 2.0,
                viewport_height: 600.0,
            },
            600.0,
        )
        .unwrap();
    let scale = old.ortho_extent / 600.0;
    assert!((front.target.x - (old.target.x - 15.0 * scale)).abs() < 1e-12);
    assert!((front.target.y - (old.target.y + 10.0 * scale)).abs() < 1e-12);
    assert_eq!((front.yaw, front.pitch), (old.yaw, old.pitch));
    assert_eq!(front.ortho_extent, old.ortho_extent / 2.0);
    let before = frame_bits(front.frame(PlotKind::Curve2D, 1.0).unwrap());
    front
        .apply_command(
            PlotKind::Curve2D,
            ViewCommand::Orbit { dx: 1.0, dy: 1.0 },
            &Domain::default(),
            1.0,
        )
        .unwrap();
    assert_eq!(
        before,
        frame_bits(front.frame(PlotKind::Curve2D, 1.0).unwrap())
    );
    let frame = front.frame(PlotKind::Curve2D, 1.0).unwrap();
    assert_eq!(frame.eye.x, front.target.x as f32);
    assert_eq!(frame.eye.y, front.target.y as f32);
    assert!(f64::from(frame.eye.z) > front.target.z);

    // 3D orbit follows the shared sensitivity and leaves +Y as world up.
    let mut orbit = PlotCamera::default();
    let initial = orbit.clone();
    orbit
        .apply_gesture(
            PlotKind::Surface3D,
            ViewGesture {
                drag: Vec2::new(100.0, 60.0),
                zoom: 2.0,
                viewport_height: 600.0,
                ..ViewGesture::default()
            },
            600.0,
        )
        .unwrap();
    let sensitivity = f64::from(ORBIT_SENSITIVITY);
    assert!((orbit.yaw - (initial.yaw - 100.0 * sensitivity)).abs() < 1e-12);
    assert!((orbit.pitch - (initial.pitch - 60.0 * sensitivity)).abs() < 1e-12);
    assert_eq!(orbit.distance, initial.distance / 2.0);
    let frame = orbit.frame(PlotKind::Surface3D, 1.0).unwrap();
    let back = (frame.eye.as_dvec3() - orbit.target).normalize();
    let right = DVec3::Y.cross(back).normalize();
    let up = back.cross(right).normalize();
    assert!(up.y > 0.0);
}

#[test]
fn invalid_navigation_is_transactional() {
    let domain = Domain::default();
    for kind in [PlotKind::Curve2D, PlotKind::Surface3D] {
        let mut camera = PlotCamera::default();
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            for gesture in [
                ViewGesture {
                    drag: Vec2::new(bad, 0.0),
                    ..ViewGesture::default()
                },
                ViewGesture {
                    drag: Vec2::new(0.0, bad),
                    ..ViewGesture::default()
                },
                ViewGesture {
                    pan: Vec2::new(bad, 0.0),
                    ..ViewGesture::default()
                },
                ViewGesture {
                    pan: Vec2::new(0.0, bad),
                    ..ViewGesture::default()
                },
            ] {
                assert_rejected(&mut camera, kind, |c| c.apply_gesture(kind, gesture, 600.0));
            }
            for command in [
                ViewCommand::Orbit { dx: bad, dy: 0.0 },
                ViewCommand::Orbit { dx: 0.0, dy: bad },
                ViewCommand::Pan { dx: bad, dy: 0.0 },
                ViewCommand::Pan { dx: 0.0, dy: bad },
            ] {
                assert_rejected(&mut camera, kind, |c| {
                    c.apply_command(kind, command, &domain, 1.0)
                });
            }
        }
        for bad in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_rejected(&mut camera, kind, |c| {
                c.apply_gesture(
                    kind,
                    ViewGesture {
                        zoom: bad,
                        ..ViewGesture::default()
                    },
                    600.0,
                )
            });
            assert_rejected(&mut camera, kind, |c| {
                c.apply_gesture(kind, ViewGesture::default(), bad)
            });
            assert_rejected(&mut camera, kind, |c| {
                c.apply_command(kind, ViewCommand::Zoom { factor: bad }, &domain, 1.0)
            });
            assert_rejected(&mut camera, kind, |c| {
                c.apply_command(kind, ViewCommand::Fit, &domain, bad)
            });
            assert_rejected(&mut camera, kind, |c| {
                c.apply_command(kind, ViewCommand::Reset, &domain, bad)
            });
            assert!(camera.frame(kind, bad).is_err());
        }
        for bad_domain in [
            Domain::new(Vec3::ONE, Vec3::ZERO),
            Domain::new(Vec3::splat(f32::NAN), Vec3::ONE),
            Domain::new(Vec3::ZERO, Vec3::splat(f32::INFINITY)),
        ] {
            for command in [ViewCommand::Fit, ViewCommand::Reset] {
                assert_rejected(&mut camera, kind, |c| {
                    c.apply_command(kind, command, &bad_domain, 1.0)
                });
            }
        }
        for field in 0..9 {
            let mut corrupt = camera.clone();
            match field {
                0 => corrupt.target.x = f64::NAN,
                1 => corrupt.distance = f64::INFINITY,
                2 => corrupt.ortho_extent = 0.0,
                3 => corrupt.yaw = f64::NAN,
                4 => corrupt.pitch = f64::INFINITY,
                5 => corrupt.fov_y = std::f64::consts::PI,
                6 => corrupt.fitted_bounds.min.x = f64::NEG_INFINITY,
                7 => corrupt.fitted_bounds.max = DVec3::splat(-2.0),
                _ => corrupt.distance = f64::MAX,
            }
            assert!(corrupt.frame(kind, 1.0).is_err());
            assert_rejected(&mut corrupt, kind, |c| {
                c.apply_gesture(kind, ViewGesture::default(), 600.0)
            });
            assert_rejected(&mut corrupt, kind, |c| {
                c.apply_command(kind, ViewCommand::Reset, &domain, 1.0)
            });
            assert_rejected(&mut corrupt, kind, |c| {
                c.fit(bounds(DVec3::NEG_ONE, DVec3::ONE), kind, 1.0)
            });
        }
    }
}

#[test]
fn extreme_bounds_are_framed_or_rejected_transactionally() {
    let cases = [
        bounds(
            DVec3::new(-1e12, -2e12, 3e12),
            DVec3::new(1.01e12, -1.99e12, 3.01e12),
        ),
        bounds(DVec3::splat(-1e30), DVec3::splat(1e30)),
        bounds(DVec3::splat(-3e38), DVec3::splat(3e38)),
        bounds(DVec3::splat(1e30), DVec3::splat(1e30 + 1e20)),
        bounds(DVec3::splat(1e20), DVec3::splat(1e20 + 1e8)),
        bounds(DVec3::new(-1.0, -1.0, 1e6), DVec3::new(1.0, 1.0, 1e6)),
    ];
    for input in cases {
        for kind in [PlotKind::Curve2D, PlotKind::Surface3D] {
            for aspect in [0.4, 2.5] {
                let mut camera = PlotCamera::default();
                let state = state_bits(&camera);
                let before = frame_bits(camera.frame(kind, aspect).unwrap());
                match camera.fit(input, kind, aspect) {
                    Ok(()) => assert_framed(&camera, kind, aspect),
                    Err(_) => {
                        assert_eq!(state_bits(&camera), state);
                        assert_eq!(frame_bits(camera.frame(kind, aspect).unwrap()), before);
                    }
                }
            }
        }
    }

    // f64 span exists, but it is far below the f32 resolution at this world
    // offset. A "successful" transform here would show a single point.
    for kind in [PlotKind::Curve2D, PlotKind::Surface3D] {
        let mut camera = PlotCamera::default();
        assert_rejected(&mut camera, kind, |c| {
            c.fit(bounds(DVec3::splat(1e20), DVec3::splat(1e20)), kind, 0.4)
        });
    }
}

#[test]
fn close_eye_and_extreme_near_far_reclip_without_moving_geometry() {
    for scale in [1e-12, 1e-9, 1e-6, 1.0, 1e6] {
        for kind in [PlotKind::Curve2D, PlotKind::Surface3D] {
            let mut camera = PlotCamera::default();
            camera
                .fit(bounds(DVec3::splat(-scale), DVec3::splat(scale)), kind, 0.4)
                .unwrap();
            assert_framed(&camera, kind, 0.4);
            let retained = camera.fitted_bounds;
            for factor in [0.01, 2.0, 100.0] {
                match camera.apply_command(
                    kind,
                    ViewCommand::Zoom { factor },
                    &Domain::default(),
                    0.4,
                ) {
                    Ok(()) => {
                        assert!(camera.frame(kind, 0.4).unwrap().view_proj.is_finite());
                        assert_eq!(camera.fitted_bounds.min, retained.min);
                        assert_eq!(camera.fitted_bounds.max, retained.max);
                    }
                    Err(_) => break,
                }
            }
        }
    }

    // Tiny plots use a tiny positive near plane instead of the vector near=0.01.
    let mut close = PlotCamera::default();
    close
        .fit(
            bounds(DVec3::splat(-1e-12), DVec3::splat(1e-12)),
            PlotKind::Surface3D,
            2.5,
        )
        .unwrap();
    assert!(close.distance < 1e-9);
    assert_framed(&close, PlotKind::Surface3D, 2.5);
}

#[test]
fn vector_limits_keyboard_f_and_domain_fit_remain_unchanged() {
    assert_eq!(OrbitCamera::MAX_DISTANCE, 20.0);
    let huge = Domain::new(Vec3::splat(-100.0), Vec3::splat(100.0));
    assert_eq!(fit_distance(huge, OrbitCamera::default().fov_y, 0.4), 20.0);
    let tiny = Domain::new(Vec3::splat(-0.001), Vec3::splat(0.001));
    assert_eq!(fit_distance(tiny, OrbitCamera::default().fov_y, 1.0), 0.5);

    let camera = OrbitCamera {
        ortho_zoom: 3.5,
        ..OrbitCamera::default()
    };
    let view = ViewContext::new(false, Domain::default(), 1.0);
    assert_eq!(
        command_for_key(&camera, view, Key::F, Modifiers::NONE),
        Some(ViewCommand::Fit)
    );
    let flat = apply_view_command(camera, view, ViewCommand::Fit);
    assert_eq!(flat.ortho_zoom, 1.0);
    assert_eq!(flat.target, Vec3::ZERO);
    assert_eq!(flat.distance, 3.2);
    let large = apply_view_command(
        OrbitCamera::default(),
        ViewContext::new(true, huge, 0.4),
        ViewCommand::Fit,
    );
    assert_eq!(large.distance, 20.0);
}
