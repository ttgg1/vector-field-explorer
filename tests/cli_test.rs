use std::ffi::OsString;

use vector_field_explorer::cli::{CliError, parse_config, parse_flags, resolve_config};

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
