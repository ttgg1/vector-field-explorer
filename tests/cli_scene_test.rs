use glam::{Mat4, Vec3};
use vector_field_explorer::{
    camera::OrbitCamera,
    cli::{
        CliError, CliFlags, CliOptions, Mode, OutputFrame, PreparedScene, output_frame,
        parse_config, resolve_config,
    },
    fly_camera::FlyCamera,
    renderer::SceneGPU,
};

fn options(json: &str) -> CliOptions {
    resolve_config(
        Some(parse_config(json).unwrap()),
        CliFlags {
            output: Some("plot.png".into()),
            ..Default::default()
        },
    )
    .unwrap()
}

fn close_vec(actual: Vec3, expected: Vec3) {
    assert!(
        (actual - expected).length() < 1e-4,
        "{actual:?} != {expected:?}"
    );
}

fn close_mat(actual: Mat4, expected: Mat4) {
    assert!(
        actual.abs_diff_eq(expected, 1e-5),
        "{actual:?} != {expected:?}"
    );
}

fn valid(frame: &OutputFrame) {
    for m in [frame.view, frame.projection, frame.view_proj] {
        assert!(m.is_finite());
        assert!(m.determinant().is_finite());
        assert_ne!(m.determinant(), 0.0);
        assert!(m.inverse().is_finite());
    }
    close_mat(frame.view_proj, frame.projection * frame.view);
}

#[test]
fn front_frame_fits_off_origin_mid_z_corners_at_every_aspect() {
    for (width, height) in [(1280, 800), (800, 1280), (1, 4096), (4096, 1), (1, 1)] {
        let mut o = options(r#"{"domain":{"min":[4,-9,40],"max":[10,-5,42]}}"#);
        o.width = width;
        o.height = height;
        let f = output_frame(&o).unwrap();
        valid(&f);
        assert_eq!((f.width, f.height), (width, height));
        assert_eq!(f.aspect, width as f32 / height as f32);
        close_vec(f.target, Vec3::new(7.0, -7.0, 41.0));
        close_vec(f.eye, f.target + Vec3::Z * OrbitCamera::default().distance);
        let mut furthest = 0.0_f32;
        for x in [4.0, 10.0] {
            for y in [-9.0, -5.0] {
                let p = f.view_proj.project_point3(Vec3::new(x, y, 41.0));
                assert!(p.x.abs() <= 1.0 / 1.35 + 1e-5);
                assert!(p.y.abs() <= 1.0 / 1.35 + 1e-5);
                assert!((0.0..=1.0).contains(&p.z));
                furthest = furthest.max(p.x.abs()).max(p.y.abs());
            }
        }
        assert!((furthest - 1.0 / 1.35).abs() < 1e-5);
    }
}

#[test]
fn orbit_uses_recentered_app_pose_not_domain_fit() {
    let o = options(r#"{"mode":"3d","domain":{"min":[-90,-80,40],"max":[70,60,42]}}"#);
    let f = output_frame(&o).unwrap();
    let orbit = OrbitCamera {
        target: o.domain.center(),
        ..Default::default()
    };
    valid(&f);
    close_vec(f.target, orbit.target);
    close_vec(f.eye, orbit.position());
    close_mat(f.view, orbit.view());
    close_mat(f.projection, orbit.proj_perspective(f.aspect));
    assert!(((f.eye - f.target).length() - 3.2).abs() < 1e-4);
}

#[test]
fn orbit_components_merge_individually_and_override_defaults() {
    let config = parse_config(
        r#"{"mode":"3d","camera":{"target":[2,3,4],"yaw":0.3,"distance":7,"fov_y_degrees":73}}"#,
    )
    .unwrap();
    let o = resolve_config(
        Some(config),
        CliFlags {
            output: Some("plot.png".into()),
            camera_yaw: Some(-0.6),
            ..Default::default()
        },
    )
    .unwrap();
    let f = output_frame(&o).unwrap();
    let orbit = OrbitCamera {
        target: Vec3::new(2.0, 3.0, 4.0),
        yaw: -0.6,
        distance: 7.0,
        fov_y: 73.0_f32.to_radians(),
        ..Default::default()
    };
    close_vec(f.eye, orbit.position());
    close_mat(f.view, orbit.view());
    close_mat(f.projection, orbit.proj_perspective(f.aspect));
}

#[test]
fn fly_derives_pose_then_overrides_only_explicit_components() {
    for camera in [
        r#"{"kind":"fly"}"#,
        r#"{"kind":"fly","position":[3,4,5]}"#,
        r#"{"kind":"fly","yaw":0.2}"#,
        r#"{"kind":"fly","pitch":-0.4}"#,
    ] {
        let o = options(&format!(
            r#"{{"mode":"3d","domain":{{"min":[4,-9,40],"max":[10,-5,42]}},"camera":{camera}}}"#
        ));
        let orbit = OrbitCamera {
            target: o.domain.center(),
            ..Default::default()
        };
        let mut fly = FlyCamera::from_orbit(&orbit, FlyCamera::default().sensitivity);
        if o.camera.specified.position {
            fly.position = Vec3::from_array(o.camera.position);
        }
        if o.camera.specified.yaw {
            fly.yaw = o.camera.yaw;
        }
        if o.camera.specified.pitch {
            fly.pitch = o.camera.pitch;
        }
        let f = output_frame(&o).unwrap();
        valid(&f);
        close_vec(f.eye, fly.position);
        close_vec(f.target, fly.position + fly.look_dir());
        close_mat(f.view, fly.view());
        close_mat(f.projection, fly.proj_perspective(f.aspect));
    }
}

#[test]
fn front_view_uses_target_and_zoom_but_not_implicit_orbit_pose() {
    let mut o = options(r#"{"camera":{"target":[3,4,41],"ortho_zoom":2}}"#);
    let f = output_frame(&o).unwrap();
    let orbit = OrbitCamera {
        target: Vec3::new(3.0, 4.0, 41.0),
        ortho_zoom: 2.0,
        ..Default::default()
    };
    close_mat(f.view, orbit.front_view());
    close_mat(
        f.projection,
        orbit.proj_ortho(f.aspect, (2.0_f32 / f.aspect).max(2.0) * 1.35),
    );
    o.camera.yaw = -0.4;
    o.camera.pitch = 0.5;
    o.camera.distance = 19.0;
    o.camera.fov_y_degrees = 90.0;
    let same = output_frame(&o).unwrap();
    close_mat(same.view_proj, f.view_proj);
    close_vec(same.eye, f.eye);

    o.mode = Mode::ThreeD;
    o.camera.specified.ortho_zoom = false;
    let a = output_frame(&o).unwrap();
    o.camera.ortho_zoom = 30.0;
    close_mat(output_frame(&o).unwrap().view_proj, a.view_proj);
}

#[test]
fn frame_rejects_explicit_inapplicable_camera_members() {
    for (mode, kind, field) in [
        (Mode::TwoD, "orbit", "position"),
        (Mode::TwoD, "orbit", "yaw"),
        (Mode::TwoD, "orbit", "pitch"),
        (Mode::TwoD, "orbit", "distance"),
        (Mode::TwoD, "orbit", "fov_y_degrees"),
        (Mode::ThreeD, "orbit", "position"),
        (Mode::ThreeD, "orbit", "ortho_zoom"),
        (Mode::ThreeD, "fly", "target"),
        (Mode::ThreeD, "fly", "distance"),
        (Mode::ThreeD, "fly", "fov_y_degrees"),
        (Mode::ThreeD, "fly", "ortho_zoom"),
    ] {
        let mut o = options(&format!(r#"{{"mode":"3d","camera":{{"kind":"{kind}"}}}}"#));
        o.mode = mode;
        o.plane.index = 0;
        match field {
            "position" => o.camera.specified.position = true,
            "target" => o.camera.specified.target = true,
            "yaw" => o.camera.specified.yaw = true,
            "pitch" => o.camera.specified.pitch = true,
            "distance" => o.camera.specified.distance = true,
            "fov_y_degrees" => o.camera.specified.fov_y_degrees = true,
            "ortho_zoom" => o.camera.specified.ortho_zoom = true,
            _ => unreachable!(),
        }
        assert!(
            matches!(output_frame(&o), Err(CliError::Validation(message)) if message.contains("inapplicable")),
            "{mode:?}/{kind}/{field}"
        );
    }
    let mut o = options(r#"{"mode":"3d","camera":{"kind":"fly"}}"#);
    o.mode = Mode::TwoD;
    o.plane.index = 0;
    assert!(matches!(output_frame(&o), Err(CliError::Validation(_))));
}

#[test]
fn frame_rejects_degenerate_final_transforms_even_with_intrinsically_valid_inputs() {
    // Orbit pitch zero is within the schema but puts the eye on the up axis.
    for json in [
        r#"{"mode":"3d","camera":{"pitch":0}}"#,
        r#"{"mode":"3d","camera":{"target":[1e30,1e30,1e30]}}"#,
        r#"{"mode":"3d","camera":{"kind":"fly","position":[1e30,1e30,1e30]}}"#,
        r#"{"mode":"3d","camera":{"fov_y_degrees":1e-40}}"#,
    ] {
        let o = options(json);
        assert!(
            matches!(output_frame(&o), Err(CliError::Validation(message)) if message.contains("transform")),
            "{json}"
        );
    }
}

#[test]
fn background_decodes_srgb_without_premultiplying_alpha_and_carrier_keeps_frame() {
    let default = output_frame(&options("{}")).unwrap();
    assert_eq!(default.background_srgb, [27, 27, 27, 255]);
    assert!((default.background_linear[0] - 0.010960094006488246).abs() < 1e-12);
    assert_eq!(default.background_linear[3], 1.0);
    let f = output_frame(&options(r#"{"background":[10,128,255,64]}"#)).unwrap();
    assert_eq!(f.background_srgb, [10, 128, 255, 64]);
    assert!((f.background_linear[0] - 10.0 / 255.0 / 12.92).abs() < 1e-12);
    assert!((f.background_linear[1] - 0.21586050011389926).abs() < 1e-12);
    assert_eq!(f.background_linear[2], 1.0);
    assert_eq!(f.background_linear[3], 64.0 / 255.0);
    for (linear, byte) in f.background_linear[..3].iter().zip(f.background_srgb) {
        let encoded = if *linear <= 0.0031308 {
            linear * 12.92
        } else {
            1.055 * linear.powf(1.0 / 2.4) - 0.055
        };
        assert_eq!((encoded * 255.0).round() as u8, byte);
    }
    let prepared = PreparedScene {
        scene: SceneGPU::default(),
        frame: f,
    };
    assert_eq!(prepared.frame.background_srgb, [10, 128, 255, 64]);
    assert!(prepared.scene.is_empty());
}
