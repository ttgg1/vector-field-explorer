use std::ffi::OsString;

fn native_command(root: &std::path::Path) -> std::process::Command {
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_vector-field-explorer"));
    command
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("WAYLAND_SOCKET")
        .env("WGPU_BACKEND", "noop")
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_CACHE_HOME", root.join("gpu-cache"))
        .env("HOME", root.join("home"));
    command
}

#[test]
fn native_dispatch_help_version_exports_and_contextual_errors() {
    let root = std::env::temp_dir().join(format!("vfe-dispatch-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for flag in ["--help", "--version"] {
        let result = native_command(&root).arg(flag).output().unwrap();
        assert!(
            result.status.success(),
            "{flag}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(!result.stdout.is_empty());
    }
    let config = root.join("input.json");
    std::fs::write(&config, r#"{"output":"unused.png","width":91,"height":67,"formula":"(1,0)","references":{"grid_and_domain_box":false,"world_axes":false}}"#).unwrap();
    let output = root.join("field.SVG");
    let result = native_command(&root)
        .arg("--config")
        .arg(&config)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let svg = std::fs::read_to_string(&output).unwrap();
    assert!(svg.contains("<polygon"), "actual field geometry missing");
    assert!(svg.contains("91") && svg.contains("67"));
    for (args, expected) in [
        (vec!["--bogus"], "unexpected argument"),
        (vec!["--width", "0", "-o", "x.svg"], "width"),
        (vec!["--formula", "(unknown,0)", "-o", "x.svg"], "unknown"),
        (vec!["--mode", "3d", "-o", "x.svg"], "3D SVG"),
        (vec!["-o", "x.png"], "adapter"),
        (vec!["--config", "/nonexistent-vfe-config.json"], "config"),
        (
            vec![
                "--duration",
                "3600",
                "--fixed-step",
                "0.0001",
                "-o",
                "x.svg",
            ],
            "100,000",
        ),
    ] {
        let result = native_command(&root).args(&args).output().unwrap();
        assert!(!result.status.success(), "{args:?}");
        assert!(
            String::from_utf8_lossy(&result.stderr).contains(expected),
            "{args:?}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let result = native_command(&root)
        .arg("-o")
        .arg(root.join("missing/field.svg"))
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("write"));
    std::fs::write(&config, "{bad JSON").unwrap();
    let result = native_command(&root)
        .arg("--config")
        .arg(&config)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("config"));
    std::fs::write(&config, r#"{"density":201}"#).unwrap();
    let result = native_command(&root)
        .arg("--config")
        .arg(&config)
        .args(["--density", "8", "-o"])
        .arg(&output)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("density"));
    // Linux's GUI cannot start without a display; this proves the retained route.
    #[cfg(target_os = "linux")]
    {
        let result = native_command(&root).output().unwrap();
        assert!(!result.status.success());
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(!stderr.contains("output path is required"));
        assert!(stderr.contains("DISPLAY") || stderr.contains("display"));
    }
    for dir in ["config", "data", "home"] {
        assert!(
            !root.join(dir).exists(),
            "unexpected persistence directory: {dir}"
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn native_dispatch_preserves_non_utf8_config_and_output_paths() {
    use std::os::unix::ffi::OsStringExt;
    let root = std::env::temp_dir().join(format!("vfe-paths-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let config = root.join(OsString::from_vec(b"config-\xff.json".to_vec()));
    let output = root.join(OsString::from_vec(b"field-\xff.svg".to_vec()));
    std::fs::write(&config, r#"{"formula":"(1,0)"}"#).unwrap();
    let result = native_command(&root)
        .arg("--config")
        .arg(&config)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        std::fs::read_to_string(output)
            .unwrap()
            .contains("<polygon")
    );
    std::fs::remove_dir_all(root).unwrap();
}

/// Invoked explicitly by scripts/test-cli-dispatch.sh; ordinary suite runs
/// are not real-adapter evidence and do not require a GPU.
#[test]
fn displayless_native_dispatch_worker() {
    if std::env::var_os("VFE_CLI_RUNTIME").is_none() {
        return;
    }
    assert!(std::env::var_os("DISPLAY").is_none());
    assert!(std::env::var_os("WAYLAND_DISPLAY").is_none());
    let root = std::env::temp_dir().join(format!("vfe-cli-runtime-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for dir in ["config", "data", "home"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
        std::fs::write(
            root.join(dir).join("sentinel"),
            b"unchanged GUI persistence",
        )
        .unwrap();
    }
    for mode in ["2d", "3d"] {
        let path = root.join(format!("{mode}.PNG"));
        let mut command = native_command(&root);
        command.env_remove("WGPU_BACKEND");
        if let Some(backend) = std::env::var_os("WGPU_BACKEND") {
            command.env("WGPU_BACKEND", backend);
        }
        let result = command
            .args([
                "--mode",
                mode,
                "--width",
                "129",
                "--height",
                "97",
                "--density",
                "8",
                "--references-grid-and-box",
                "false",
                "--references-world-axes",
                "false",
                "--background-rgba",
                "73,131,207,109",
                "--output",
            ])
            .arg(&path)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let image = image::open(&path).unwrap().to_rgba8();
        assert_eq!(image.dimensions(), (129, 97));
        let field_pixels = image
            .pixels()
            .filter(|pixel| pixel.0 != [73, 131, 207, 109])
            .count();
        assert!(field_pixels > 0, "{mode}: blank field-only PNG");
        eprintln!(
            "native subprocess {mode}: decoded 129x97 PNG, {field_pixels} non-background pixels"
        );
    }
    let result = native_command(&root)
        .env_remove("WGPU_BACKEND")
        .args(["--width", "8", "--height", "8", "--output"])
        .arg(root.join("missing/field.png"))
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("write PNG"));
    for dir in ["config", "data", "home"] {
        let entries = std::fs::read_dir(root.join(dir))
            .unwrap()
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 1, "persistence write in {dir}");
        assert_eq!(
            std::fs::read(root.join(dir).join("sentinel")).unwrap(),
            b"unchanged GUI persistence"
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}

use vector_field_explorer::cli::{
    CameraKind, CliError, Mode, parse_config, parse_flags, resolve_config,
    validate_intrinsic_config,
};
use vector_field_explorer::{
    arrow_style::{ArrowStyle, ArrowStyle2D},
    camera::OrbitCamera,
    colormap::Colormap,
    fly_camera::FlyCamera,
    particles::ParticleColorMode,
    plane::PlaneAxis,
    viewmodel::{ArrowLengthMode, StreamlineSeedMode},
};

fn flags(args: &[&str]) -> vector_field_explorer::cli::CliFlags {
    parse_flags(std::iter::once(OsString::from("vfe")).chain(args.iter().map(OsString::from)))
        .unwrap()
}

#[test]
fn parser_help_version_and_empty_input() {
    assert!(flags(&[]).output.is_none());
    assert!(matches!(
        parse_flags(["vfe", "--help"].map(OsString::from)),
        Err(CliError::Parse(_))
    ));
    assert!(matches!(
        parse_flags(["vfe", "--version"].map(OsString::from)),
        Err(CliError::Parse(_))
    ));
}

#[test]
fn nested_merge_and_explicit_false_and_null() {
    let config = parse_config(r#"{"output":"plot.svg","width":900,"arrows":true,"arrow_params":{"head_width":0.2},"camera":{"target":[1,2,3]},"color_range":[1,4]}"#).unwrap();
    let options = resolve_config(
        Some(config),
        flags(&[
            "--arrows",
            "false",
            "--height",
            "320",
            "--head-length",
            "0.4",
            "--color-range",
            "auto",
        ]),
    )
    .unwrap();
    assert_eq!(options.width, 900);
    assert_eq!(options.height, 320);
    assert!(!options.arrows);
    assert_eq!(options.arrow_params.head_width, 0.2);
    assert_eq!(options.arrow_params.head_length, 0.4);
    assert!(options.color_range.is_none());
    assert_eq!(options.camera.target, [1.0, 2.0, 3.0]);
    assert!(options.references.grid_and_domain_box);
    let options = resolve_config(
        Some(parse_config(r#"{"output":"plot.png","color_range":null}"#).unwrap()),
        flags(&[]),
    )
    .unwrap();
    assert!(options.color_range.is_none());
}

#[test]
fn unknown_keys_invalid_overrides_and_output_errors() {
    assert!(parse_config(r#"{"camera":{"wat":1}}"#).is_err());
    assert!(parse_config(r#"{"seed_mode":"click"}"#).is_err());
    assert!(parse_config(r#"{"color_range":null,"plane":{"axis":"q"}}"#).is_err());
    assert!(
        resolve_config(
            Some(parse_config(r#"{"output":"x.svg","density":201}"#).unwrap()),
            flags(&["--density", "8"])
        )
        .is_err()
    );
    assert!(resolve_config(None, flags(&[])).is_err());
    assert!(resolve_config(None, flags(&["-o", "x.txt"])).is_err());
}

#[test]
fn density_depends_on_final_formula_not_mode_alone() {
    let config = parse_config(r#"{"output":"x.png","mode":"2d","density":100}"#).unwrap();
    assert!(
        resolve_config(
            Some(config.clone()),
            flags(&["--mode", "3d", "--formula", "(x,y)"])
        )
        .is_ok()
    );
    assert!(
        resolve_config(
            Some(config),
            flags(&["--mode", "3d", "--formula", "(x,y,z)"])
        )
        .is_err()
    );
}

#[test]
fn camera_rejects_inapplicable_explicit_members() {
    for json in [
        r#"{"output":"x.png","camera":{"kind":"fly"}}"#,
        r#"{"output":"x.png","camera":{"fov_y_degrees":60}}"#,
        r#"{"output":"x.png","mode":"3d","camera":{"position":[0,0,3]}}"#,
        r#"{"output":"x.png","mode":"3d","camera":{"kind":"fly","distance":3}}"#,
    ] {
        assert!(
            resolve_config(Some(parse_config(json).unwrap()), flags(&[])).is_err(),
            "{json}"
        );
    }
}

#[test]
fn formula_binding_allows_singular_and_zero_but_not_unknown_names() {
    for formula in ["(0,0)", "(1/x, 0)"] {
        let config =
            parse_config(&format!(r#"{{"output":"x.svg","formula":"{formula}"}}"#)).unwrap();
        assert!(resolve_config(Some(config), flags(&[])).is_ok());
    }
    for formula in ["(unknown,0)", "(bad(x),0)"] {
        let config =
            parse_config(&format!(r#"{{"output":"x.svg","formula":"{formula}"}}"#)).unwrap();
        assert!(resolve_config(Some(config), flags(&[])).is_err());
    }
}

#[test]
fn all_schema_members_have_equivalent_json_and_flags() {
    let json = r#"{
        "output":"plot.PNG","width":160,"height":120,"mode":"3d","formula":"(y,-x,z)",
        "domain":{"min":[-2,-3,-4],"max":[2,3,4]},"density":18,
        "arrows":false,"streamlines":true,"particles":true,"arrow_scale":2,
        "arrow_opacity":0.7,"plane_dim_opacity":0.4,"arrow_length_mode":"magnitude",
        "magnitude_scale":2,"proportional_exponent":1.5,"colormap":"turbo",
        "color_range":[0,5],"arrow_style_3d":"cylinder","arrow_style_2d":"streamline",
        "arrow_params":{"head_length":0.4,"head_width":0.2,"shaft_radius":0.07,"sides":8},
        "streamline_seed_mode":"click","plane":{"enabled":true,"axis":"x","index":3},
        "particle_count":42,"particle_size":1.5,
        "particle_appearance":{"opacity":0.6,"color_mode":"fixed","fixed_color":[1,2,3],
        "trail_len":10,"trail_fade":0.3,"trail_width":0.7,"loop_on_stagnation":false,
        "stagnation_seconds":1.2},
        "camera":{"kind":"fly","position":[1,2,3],"yaw":0.2,"pitch":0.3},
        "references":{"grid_and_domain_box":false,"world_axes":false},
        "background":[12,34,56,78],"seed":0,"start_time_seconds":1,
        "duration_seconds":2,"fixed_step_seconds":0.2
    }"#;
    let args = [
        "-o",
        "plot.PNG",
        "--width",
        "160",
        "--height",
        "120",
        "--mode",
        "3d",
        "--formula",
        "(y,-x,z)",
        "--domain-min",
        "-2,-3,-4",
        "--domain-max",
        "2,3,4",
        "--density",
        "18",
        "--arrows",
        "false",
        "--streamlines",
        "true",
        "--particles",
        "true",
        "--arrow-scale",
        "2",
        "--arrow-opacity",
        "0.7",
        "--plane-dim-opacity",
        "0.4",
        "--arrow-length-mode",
        "magnitude",
        "--magnitude-scale",
        "2",
        "--proportional-exponent",
        "1.5",
        "--colormap",
        "turbo",
        "--color-range",
        "0,5",
        "--arrow-style-3d",
        "cylinder",
        "--arrow-style-2d",
        "streamline",
        "--head-length",
        "0.4",
        "--head-width",
        "0.2",
        "--shaft-radius",
        "0.07",
        "--arrow-sides",
        "8",
        "--streamline-seed-mode",
        "click",
        "--plane-enabled",
        "true",
        "--plane-axis",
        "x",
        "--plane-index",
        "3",
        "--particle-count",
        "42",
        "--particle-size",
        "1.5",
        "--particle-opacity",
        "0.6",
        "--particle-color",
        "fixed",
        "--particle-color-rgb",
        "1,2,3",
        "--trail-len",
        "10",
        "--trail-fade",
        "0.3",
        "--trail-width",
        "0.7",
        "--loop-on-stagnation",
        "false",
        "--stagnation-seconds",
        "1.2",
        "--camera-kind",
        "fly",
        "--camera-position",
        "1,2,3",
        "--camera-yaw",
        "0.2",
        "--camera-pitch",
        "0.3",
        "--references-grid-and-box",
        "false",
        "--references-world-axes",
        "false",
        "--background-rgba",
        "12,34,56,78",
        "--seed",
        "0",
        "--start-time",
        "1",
        "--duration",
        "2",
        "--fixed-step",
        "0.2",
    ];
    let from_json = resolve_config(Some(parse_config(json).unwrap()), flags(&[])).unwrap();
    let from_flags = resolve_config(None, flags(&args)).unwrap();
    assert_eq!(format!("{from_json:?}"), format!("{from_flags:?}"));
}

#[test]
fn intrinsic_checks_reject_bad_inputs_even_when_overridden() {
    for (json, args) in [
        (
            r#"{"output":"x.svg","formula":"(bad(x),0)"}"#,
            vec!["--formula", "(x,y)"],
        ),
        (
            r#"{"output":"x.svg","color_range":[1,1]}"#,
            vec!["--color-range", "auto"],
        ),
        (
            r#"{"output":"x.svg","arrow_params":{"sides":3}}"#,
            vec!["--arrow-sides", "8"],
        ),
        (
            r#"{"output":"x.svg","camera":{"pitch":1.6}}"#,
            vec!["--camera-pitch", "0"],
        ),
        (
            r#"{"output":"x.svg","domain":{"min":[2,0,0],"max":[1,1,1]}}"#,
            vec!["--domain-min", "-1,-1,-1"],
        ),
        (
            r#"{"output":"x.svg","start_time_seconds":3599,"duration_seconds":2}"#,
            vec!["--duration", "0"],
        ),
        (r#"{"output":"x.dat"}"#, vec!["--output", "x.png"]),
    ] {
        assert!(
            resolve_config(Some(parse_config(json).unwrap()), flags(&args)).is_err(),
            "{json}"
        );
    }
    for json in [
        r#"{"output":"x.svg","domain":{"min":[0,0,0],"max":[0,1,1]}}"#,
        r#"{"output":"x.svg","plane":{"index":300}}"#,
        r#"{"output":"x.svg","start_time_seconds":3599,"duration_seconds":2}"#,
    ] {
        assert!(
            resolve_config(Some(parse_config(json).unwrap()), flags(&[])).is_err(),
            "{json}"
        );
    }
}

#[test]
fn signed_separate_token_camera_and_color_flags_match_json() {
    for (json, args) in [
        (r#"{"color_range":[-1,1]}"#, vec!["--color-range", "-1,1"]),
        (
            r#"{"camera":{"target":[-1,0,0]}}"#,
            vec!["--camera-target", "-1,0,0"],
        ),
        (r#"{"camera":{"yaw":-0.2}}"#, vec!["--camera-yaw", "-0.2"]),
        (
            r#"{"camera":{"pitch":-0.2}}"#,
            vec!["--camera-pitch", "-0.2"],
        ),
        (
            r#"{"camera":{"kind":"fly","position":[-1,0,0]}}"#,
            vec!["--camera-kind", "fly", "--camera-position", "-1,0,0"],
        ),
    ] {
        let base = format!(
            r#"{{"output":"x.png","mode":"3d",{}}}"#,
            &json[1..json.len() - 1]
        );
        let json = resolve_config(Some(parse_config(&base).unwrap()), flags(&[])).unwrap();
        let mut argv = vec!["-o", "x.png", "--mode", "3d"];
        argv.extend(args);
        let cli = resolve_config(None, flags(&argv)).unwrap();
        assert_eq!(format!("{json:?}"), format!("{cli:?}"), "{base}");
    }
}

#[test]
fn defaults_follow_mode_state_and_orbit_derived_fly_pose() {
    let orbit = OrbitCamera::default();
    assert_eq!(
        resolve_config(None, flags(&["-o", "x.png"])).unwrap().mode,
        Mode::TwoD
    );
    for (mode, density, formula) in [("2d", 24, "(x, y)"), ("3d", 16, "(x, y, z)")] {
        let options = resolve_config(None, flags(&["-o", "x.png", "--mode", mode])).unwrap();
        // Frozen spec defaults: literal expectations, not another State/default instance.
        assert_eq!(
            options.mode,
            if mode == "3d" {
                Mode::ThreeD
            } else {
                Mode::TwoD
            }
        );
        assert_eq!(
            (options.width, options.height, options.density),
            (1280, 800, density)
        );
        assert_eq!(options.formula, formula);
        assert_eq!(options.domain.min.to_array(), [-1.0; 3]);
        assert_eq!(options.domain.max.to_array(), [1.0; 3]);
        assert!(options.arrows);
        assert!(!options.streamlines && !options.particles);
        assert_eq!(
            (
                options.arrow_scale,
                options.arrow_opacity,
                options.plane_dim_opacity
            ),
            (1.0, 1.0, 0.2)
        );
        assert_eq!(options.arrow_length_mode, ArrowLengthMode::Proportional);
        assert_eq!(
            (options.magnitude_scale, options.proportional_exponent),
            (1.0, 1.0)
        );
        assert_eq!(options.colormap, Colormap::Viridis);
        assert_eq!(options.color_range, None);
        assert_eq!(options.arrow_style_3d, ArrowStyle::Cone);
        assert_eq!(options.arrow_style_2d, ArrowStyle2D::Flat);
        assert_eq!(
            (
                options.arrow_params.head_length,
                options.arrow_params.head_width,
                options.arrow_params.shaft_radius,
                options.arrow_params.sides
            ),
            (0.3, 0.12, 0.055, 12)
        );
        assert_eq!(options.streamline_seed_mode, StreamlineSeedMode::Auto);
        assert!(!options.plane.enabled);
        assert_eq!(options.plane.axis, PlaneAxis::Z);
        assert_eq!(options.plane.index, if mode == "3d" { 7 } else { 0 });
        assert_eq!((options.particle_count, options.particle_size), (0, 1.0));
        let appearance = options.particle_appearance;
        assert_eq!(appearance.opacity, 1.0);
        assert_eq!(appearance.color_mode, ParticleColorMode::Magnitude);
        assert_eq!(appearance.fixed_color, [0, 150, 136, 255]);
        assert_eq!(
            (
                appearance.trail_len,
                appearance.trail_fade,
                appearance.trail_width
            ),
            (0, 0.65, 1.0)
        );
        assert!(appearance.loop_on_stagnation);
        assert_eq!(appearance.stagnation_seconds, 0.5);
        assert_eq!(options.camera.kind, CameraKind::Orbit);
        assert_eq!(
            (
                options.camera.yaw,
                options.camera.pitch,
                options.camera.distance,
                options.camera.ortho_zoom
            ),
            (0.8, 0.9, 3.2, 1.0)
        );
        assert_eq!(options.camera.fov_y_degrees, 55.0);
        assert_eq!(options.camera.target, [0.0; 3]);
        assert_eq!(options.seed, 0x123456789ABCDEF0);
        assert_eq!(options.background, [27, 27, 27, 255]);
        assert_eq!(
            (
                options.start_time_seconds,
                options.duration_seconds,
                options.fixed_step_seconds
            ),
            (0.0, 0.0, 1.0 / 60.0)
        );
        assert!(options.references.grid_and_domain_box && options.references.world_axes);
    }
    let fly = FlyCamera::from_orbit(&orbit, FlyCamera::default().sensitivity);
    let options = resolve_config(
        None,
        flags(&["-o", "x.png", "--mode", "3d", "--camera-kind", "fly"]),
    )
    .unwrap();
    assert_eq!(options.camera.position, fly.position.to_array());
    assert_eq!(
        (options.camera.yaw, options.camera.pitch),
        (fly.yaw, fly.pitch)
    );
}

#[test]
fn orbit_and_front_camera_flags_match_json_without_cross_mode_leakage() {
    for (mode, camera, args) in [
        (
            "3d",
            r#"{"target":[-1,2,3],"yaw":-0.2,"pitch":-1.55,"distance":20,"fov_y_degrees":179}"#,
            vec![
                "--camera-target",
                "-1,2,3",
                "--camera-yaw",
                "-0.2",
                "--camera-pitch",
                "-1.55",
                "--camera-distance",
                "20",
                "--camera-fov-deg",
                "179",
            ],
        ),
        (
            "2d",
            r#"{"target":[-1,2,3],"ortho_zoom":0.05}"#,
            vec!["--camera-target", "-1,2,3", "--camera-ortho-zoom", "0.05"],
        ),
    ] {
        let json = format!(r#"{{"output":"x.png","mode":"{mode}","camera":{camera}}}"#);
        let expected = resolve_config(Some(parse_config(&json).unwrap()), flags(&[])).unwrap();
        let mut argv = vec!["-o", "x.png", "--mode", mode];
        argv.extend(args);
        let actual = resolve_config(None, flags(&argv)).unwrap();
        assert_eq!(format!("{expected:?}"), format!("{actual:?}"));
    }
}

#[test]
fn numeric_bounds_are_validated_for_each_supplied_schema_member() {
    for axis in 0..3 {
        for (min, max, accepted) in [
            (-100.0, 100.0, true),
            (-100.01, 100.0, false),
            (-100.0, 100.01, false),
        ] {
            let mut lo = [-1.0; 3];
            let mut hi = [1.0; 3];
            lo[axis] = min;
            hi[axis] = max;
            let json = format!(r#"{{"output":"x.png","domain":{{"min":{lo:?},"max":{hi:?}}}}}"#);
            assert_eq!(
                resolve_config(Some(parse_config(&json).unwrap()), flags(&[])).is_ok(),
                accepted,
                "axis {axis}: {json}"
            );
        }
    }
    // Each row checks both inclusive ends and both neighbours; camera applicability
    // is a separate effective-state check in camera_rejects_inapplicable_explicit_members.
    let rows = [
        ("width", "1", "4096", "0", "4097"),
        ("height", "1", "4096", "0", "4097"),
        ("density", "4", "200", "3", "201"),
        ("arrow_scale", "0.1", "4", "0.09", "4.01"),
        ("arrow_opacity", "0.05", "1", "0.04", "1.01"),
        ("plane_dim_opacity", "0", "1", "-0.01", "1.01"),
        ("magnitude_scale", "0.01", "10", "0", "10.01"),
        ("proportional_exponent", "0.25", "4", "0.24", "4.01"),
        ("particle_count", "0", "4000", "-1", "4001"),
        ("particle_size", "0.25", "3", "0.24", "3.01"),
        ("arrow_params.head_length", "0.1", "0.5", "0.09", "0.51"),
        ("arrow_params.head_width", "0.02", "0.3", "0.01", "0.31"),
        ("arrow_params.shaft_radius", "0.01", "0.12", "0", "0.13"),
        ("arrow_params.sides", "4", "16", "3", "17"),
        ("particle_appearance.opacity", "0.05", "1", "0.04", "1.01"),
        ("particle_appearance.trail_len", "0", "32", "-1", "33"),
        ("particle_appearance.trail_fade", "0", "1", "-0.01", "1.01"),
        (
            "particle_appearance.trail_width",
            "0.05",
            "2",
            "0.04",
            "2.01",
        ),
        (
            "particle_appearance.stagnation_seconds",
            "0.1",
            "5",
            "0.09",
            "5.01",
        ),
        ("camera.pitch", "-1.55", "1.55", "-1.56", "1.56"),
        ("camera.distance", "0.5", "20", "0.49", "20.01"),
        ("camera.ortho_zoom", "0.05", "50", "0.04", "50.01"),
        ("start_time_seconds", "0", "3600", "-0.01", "3600.01"),
        ("duration_seconds", "0", "3600", "-0.01", "3600.01"),
        ("fixed_step_seconds", "0.0001", "0.25", "0.00009", "0.26"),
    ];
    for (key, min, max, below, above) in rows {
        for (value, accepted) in [(min, true), (max, true), (below, false), (above, false)] {
            let parts: Vec<_> = key.split('.').collect();
            let json = if parts.len() == 1 {
                format!(r#"{{"{key}":{value}}}"#)
            } else {
                format!(r#"{{"{}":{{"{}":{value}}}}}"#, parts[0], parts[1])
            };
            let valid = parse_config(&json)
                .and_then(|c| validate_intrinsic_config(&c))
                .is_ok();
            assert_eq!(valid, accepted, "{key}={value}");
        }
    }
    for fov in [0.001, 179.999] {
        assert!(
            validate_intrinsic_config(
                &parse_config(&format!(r#"{{"camera":{{"fov_y_degrees":{fov}}}}}"#)).unwrap()
            )
            .is_ok()
        );
    }
    for fov in ["0", "180"] {
        assert!(
            validate_intrinsic_config(
                &parse_config(&format!(r#"{{"camera":{{"fov_y_degrees":{fov}}}}}"#)).unwrap()
            )
            .is_err()
        );
    }
}

#[test]
fn enums_tuples_range_patch_and_forbidden_null_keep_strict_schema() {
    for (key, values) in [
        ("mode", "2d,3d"),
        ("arrow_length_mode", "proportional,uniform,magnitude"),
        ("colormap", "viridis,turbo,mono"),
        ("arrow_style_3d", "cone,cylinder,flat,streamline"),
        ("arrow_style_2d", "flat,streamline"),
        ("streamline_seed_mode", "auto,click"),
        ("plane.axis", "x,y,z"),
        ("particle_appearance.color_mode", "magnitude,speed,fixed"),
        ("camera.kind", "orbit,fly"),
    ] {
        for value in values.split(',').chain(["INVALID"]) {
            let parts: Vec<_> = key.split('.').collect();
            let json = if parts.len() == 1 {
                format!(r#"{{"{key}":"{value}"}}"#)
            } else {
                format!(r#"{{"{}":{{"{}":"{value}"}}}}"#, parts[0], parts[1])
            };
            assert_eq!(parse_config(&json).is_ok(), value != "INVALID", "{json}");
        }
    }
    for json in [
        r#"{"output":null}"#,
        r#"{"domain":{"min":null}}"#,
        r#"{"camera":{"yaw":null}}"#,
        r#"{"plane":null}"#,
        r#"{"particle_appearance":{"fixed_color":null}}"#,
        r#"{"color_range":[0,null]}"#,
        r#"{"color_range":[0]}"#,
        r#"{"background":[0,0,0]}"#,
        r#"{"domain":{"min":[0,0]}}"#,
    ] {
        assert!(parse_config(json).is_err(), "{json}");
    }
    let config = parse_config(r#"{"output":"x.png","color_range":[-1,2]}"#).unwrap();
    assert_eq!(
        resolve_config(Some(config.clone()), flags(&[]))
            .unwrap()
            .color_range,
        Some([-1.0, 2.0])
    );
    assert_eq!(
        resolve_config(Some(config), flags(&["--width", "10"]))
            .unwrap()
            .color_range,
        Some([-1.0, 2.0])
    );
    assert_eq!(
        resolve_config(
            Some(parse_config(r#"{"output":"x.png","color_range":null}"#).unwrap()),
            flags(&["--color-range", "-1,2"])
        )
        .unwrap()
        .color_range,
        Some([-1.0, 2.0])
    );
}
#[test]
fn requested_times_retain_f64_precision_and_validate_before_overlay() {
    let json = r#"{"output":"x.png","start_time_seconds":2.0000000001,"duration_seconds":0.25,"fixed_step_seconds":0.01}"#;
    let config = parse_config(json).unwrap();
    assert_eq!(config.start_time_seconds.unwrap(), 2.0000000001);
    assert_eq!(config.fixed_step_seconds.unwrap(), 0.01);
    let o = resolve_config(Some(config), flags(&[])).unwrap();
    assert_eq!(o.start_time_seconds, 2.0000000001);
    assert_eq!(o.duration_seconds, 0.25);
    assert_eq!(o.fixed_step_seconds, 0.01);
    let o = resolve_config(
        None,
        flags(&[
            "-o",
            "x.png",
            "--start-time",
            "2.0000000001",
            "--duration",
            "0.25",
            "--fixed-step",
            "0.01",
        ]),
    )
    .unwrap();
    assert_eq!(o.start_time_seconds, 2.0000000001);
    assert_eq!(o.fixed_step_seconds, 0.01);
    for (key, value) in [
        ("start_time_seconds", "3600.0000000001"),
        ("duration_seconds", "3600.0000000001"),
        ("fixed_step_seconds", "0.25000000001"),
        ("fixed_step_seconds", "0.000099999999999"),
    ] {
        let c = parse_config(&format!(r#"{{"output":"x.png","{key}":{value}}}"#)).unwrap();
        assert!(
            resolve_config(
                Some(c),
                flags(&[
                    "--start-time",
                    "0",
                    "--duration",
                    "0",
                    "--fixed-step",
                    "0.01"
                ])
            )
            .is_err(),
            "{key}={value}"
        );
    }
    let c = parse_config(
        r#"{"output":"x.png","start_time_seconds":3600,"duration_seconds":0.0000000001}"#,
    )
    .unwrap();
    assert!(resolve_config(Some(c), flags(&[])).is_err());
}
