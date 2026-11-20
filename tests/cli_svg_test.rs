use std::sync::Arc;

use glam::{Mat4, Vec3};
use vector_field_explorer::{
    arrow_style::ArrowStyle2D,
    cli::{
        CliError, CliFlags, PreparedScene, output_frame, parse_config, prepare_scene,
        prepare_scene_with_styles, resolve_config, scene_to_cli_svg,
    },
    color_ramp::Rgb,
    layer_style::{LayerId, LayerOverride, LayerStyles, MAX_WIDTH_MULTIPLIER, MIN_WIDTH_MULTIPLIER},
    reference::scaled_viewport_style,
    renderer::{ArrowInstanceGPU, LineInstanceGPU},
    screenshot,
    viewport_style::ViewportStyle,
    VfApp,
};

fn prepared(json: &str) -> PreparedScene {
    let options = resolve_config(
        Some(parse_config(json).unwrap()),
        CliFlags {
            output: Some("plot.png".into()),
            ..Default::default()
        },
    )
    .unwrap();
    prepare_scene(&options, output_frame(&options).unwrap()).unwrap()
}

// Identity clip space makes expected pixel coordinates hand-checkable:
// x=-.5 -> 50, x=.2 -> 120, x=.5 -> 150; y=.12 -> 44.
fn fixture() -> PreparedScene {
    let mut p =
        prepared(r#"{"density":4,"references":{"grid_and_domain_box":false,"world_axes":false}}"#);
    p.frame.width = 200;
    p.frame.height = 100;
    p.frame.view_proj = Mat4::IDENTITY;
    p.frame.eye = Vec3::new(0.0, 0.0, 3.2);
    p.scene.arrows = Arc::new(vec![ArrowInstanceGPU {
        origin: [-0.5, 0.0, 0.5, 1.0],
        dir_len: [3.0, 0.0, 8.0, 1.0],
        color: [0.5, 0.0031308, 1.0, 0.4],
    }]);
    p
}

fn elements<'a>(svg: &'a str, class: &str) -> Vec<&'a str> {
    svg.split('<')
        .filter(|s| s.contains(&format!("class=\"{class}\"")))
        .collect()
}

fn attribute<'a>(element: &'a str, name: &str) -> &'a str {
    element
        .split_once(&format!("{name}=\""))
        .unwrap()
        .1
        .split('"')
        .next()
        .unwrap()
}

fn points(element: &str) -> Vec<[f32; 2]> {
    attribute(element, "points")
        .split_whitespace()
        .map(|pair| {
            let (x, y) = pair.split_once(',').unwrap();
            [x.parse().unwrap(), y.parse().unwrap()]
        })
        .collect()
}

fn bounds(element: &str) -> [f32; 4] {
    points(element).iter().fold(
        [
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ],
        |b, p| {
            [
                b[0].min(p[0]),
                b[1].min(p[1]),
                b[2].max(p[0]),
                b[3].max(p[1]),
            ]
        },
    )
}

#[test]
fn flat_has_shader_shaft_and_filled_head_not_just_backdrop() {
    let svg = scene_to_cli_svg(&fixture()).unwrap();
    let shafts = elements(&svg, "arrow-shaft");
    let heads = elements(&svg, "arrow-head");
    assert_eq!(shafts.len(), 1);
    assert_eq!(heads.len(), 1);
    assert_eq!(bounds(shafts[0]), [50.0, 47.25, 120.0, 52.75]);
    assert_eq!(bounds(heads[0]), [120.0, 44.0, 150.0, 56.0]);
    assert_eq!(points(heads[0]).len(), 3);
    assert_eq!(attribute(heads[0], "fill"), "#bc0aff");
    assert_eq!(attribute(heads[0], "fill-opacity"), "0.400000");
    assert!(!svg.contains("<image"));
}

#[test]
fn headless_style_uses_full_length_and_ignores_head_controls() {
    let mut p = fixture();
    p.scene.arrow_style_2d = ArrowStyle2D::Streamline;
    let first = scene_to_cli_svg(&p).unwrap();
    assert!(elements(&first, "arrow-head").is_empty());
    assert_eq!(
        bounds(elements(&first, "arrow-shaft")[0]),
        [50.0, 47.25, 150.0, 52.75]
    );
    p.scene.arrow_params.head_length = 0.5;
    p.scene.arrow_params.head_width = 0.3;
    assert_eq!(first, scene_to_cli_svg(&p).unwrap());
}

#[test]
fn head_controls_and_shaft_radius_change_world_geometry() {
    let mut p = fixture();
    p.scene.arrow_params.head_length = 0.5;
    p.scene.arrow_params.head_width = 0.2;
    p.scene.arrow_params.shaft_radius = 0.1;
    let svg = scene_to_cli_svg(&p).unwrap();
    assert_eq!(
        bounds(elements(&svg, "arrow-head")[0]),
        [100.0, 40.0, 150.0, 60.0]
    );
    assert_eq!(
        bounds(elements(&svg, "arrow-shaft")[0]),
        [50.0, 45.0, 100.0, 55.0]
    );
}

#[test]
fn near_zero_xy_direction_falls_back_to_positive_y() {
    let mut p = fixture();
    Arc::make_mut(&mut p.scene.arrows)[0].dir_len = [0.000001, 0.0, 1.0, 0.5];
    let svg = scene_to_cli_svg(&p).unwrap();
    assert_eq!(
        bounds(elements(&svg, "arrow-head")[0]),
        [44.0, 25.0, 56.0, 32.5]
    );
}

#[test]
fn projection_scales_arrow_width_and_length() {
    let mut p = fixture();
    p.frame.view_proj = Mat4::from_scale(Vec3::new(0.5, 0.5, 1.0));
    let svg = scene_to_cli_svg(&p).unwrap();
    assert_eq!(
        bounds(elements(&svg, "arrow-shaft")[0]),
        [75.0, 48.625, 110.0, 51.375]
    );
}

#[test]
fn background_is_display_encoded_straight_alpha_and_output_is_deterministic() {
    let mut p = fixture();
    p.frame.background_srgb = [18, 128, 250, 128];
    let svg = scene_to_cli_svg(&p).unwrap();
    assert_eq!(svg, scene_to_cli_svg(&p).unwrap());
    let bg = svg.split('<').find(|s| s.starts_with("rect ")).unwrap();
    assert_eq!(attribute(bg, "fill"), "#1280fa");
    let alpha: f32 = attribute(bg, "fill-opacity").parse().unwrap();
    assert!((alpha - 128.0 / 255.0).abs() < 1e-6);
}

#[test]
fn prepared_controls_map_opacity_and_hide_references_without_hiding_field() {
    let hidden = prepared(
        r#"{"density":4,"formula":"(1,0)","arrow_opacity":0.25,"references":{"grid_and_domain_box":false,"world_axes":false}}"#,
    );
    let svg = scene_to_cli_svg(&hidden).unwrap();
    assert_eq!(elements(&svg, "arrow-head").len(), 16);
    assert!(elements(&svg, "scene-line").is_empty());
    for head in elements(&svg, "arrow-head") {
        assert_eq!(attribute(head, "fill-opacity"), "0.250000");
    }
    let visible = scene_to_cli_svg(&prepared(r#"{"density":4}"#)).unwrap();
    assert!(!elements(&visible, "scene-line").is_empty());
}

fn line(a: [f32; 3], b: [f32; 3], width: f32, alpha: f32) -> LineInstanceGPU {
    LineInstanceGPU {
        a: [a[0], a[1], a[2], 1.0],
        b: [b[0], b[1], b[2], width],
        color: [0.5, 0.5, 0.5, alpha],
    }
}

#[test]
fn trail_width_fade_and_dot_radius_opacity_are_projected_without_pixel_floor() {
    let mut p = fixture();
    p.scene.arrows = Arc::new(vec![]);
    p.scene.trails = Arc::new(vec![
        line([-0.5, 0.0, 0.5], [0.5, 0.0, 0.5], 0.02, 0.15),
        line([-0.5, 0.5, 0.5], [0.5, 0.5, 0.5], 0.04, 0.6),
    ]);
    p.scene.dots = Arc::new(vec![ArrowInstanceGPU {
        origin: [0.0, 0.0, 0.5, 0.0],
        dir_len: [0.0, 0.0, 0.0, 0.1],
        color: [0.5, 0.5, 0.5, 0.3],
    }]);
    let svg = scene_to_cli_svg(&p).unwrap();
    let trails = elements(&svg, "trail");
    assert_eq!(bounds(trails[0]), [50.0, 49.5, 150.0, 50.5]);
    // The off-axis ribbon tilts toward the camera, just like the line shader:
    // projected half-width is 1 * 2.7/sqrt(2.7^2 + .5^2) = .983282.
    assert_eq!(bounds(trails[1]), [50.0, 24.0167, 150.0, 25.9833]);
    assert_eq!(attribute(trails[0], "fill-opacity"), "0.150000");
    assert_eq!(attribute(trails[1], "fill-opacity"), "0.600000");
    let dots = elements(&svg, "particle");
    assert_eq!(bounds(dots[0]), [90.0, 45.0, 110.0, 55.0]);
    assert_eq!(attribute(dots[0], "fill-opacity"), "0.300000");
    assert_eq!(attribute(dots[0], "fill"), "#bcbcbc");
}

#[test]
fn clipping_keeps_crossing_lines_and_partial_polygons_but_omits_outside_geometry() {
    let mut p = fixture();
    p.scene.lines = Arc::new(vec![
        line([-3.0, 0.0, 0.5], [3.0, 0.0, 0.5], 0.02, 1.0),
        line([2.0, 0.0, 0.5], [3.0, 0.0, 0.5], 0.02, 1.0),
    ]);
    let arrow = Arc::make_mut(&mut p.scene.arrows);
    arrow[0].origin[0] = 0.8;
    let mut outside = arrow[0];
    outside.origin[0] = 3.0;
    arrow.push(outside);
    let svg = scene_to_cli_svg(&p).unwrap();
    assert_eq!(elements(&svg, "scene-line").len(), 1);
    assert_eq!(
        bounds(elements(&svg, "scene-line")[0]),
        [0.0, 49.5, 200.0, 50.5]
    );
    assert_eq!(elements(&svg, "arrow-shaft").len(), 1);
    assert!(elements(&svg, "arrow-head").is_empty());
    assert_eq!(
        bounds(elements(&svg, "arrow-shaft")[0]),
        [180.0, 47.25, 200.0, 52.75]
    );
    for element in svg.split('<').filter(|s| s.starts_with("polygon ")) {
        for [x, y] in points(element) {
            assert!((0.0..=200.0).contains(&x) && (0.0..=100.0).contains(&y));
        }
    }
}

#[test]
fn clipping_honors_near_and_far_planes() {
    let mut p = fixture();
    Arc::make_mut(&mut p.scene.arrows)[0].origin[2] = -0.1;
    p.scene.lines = Arc::new(vec![
        line([-0.5, 0.0, -0.5], [0.5, 0.0, 0.5], 0.02, 1.0),
        line([-0.5, 0.0, 1.1], [0.5, 0.0, 1.1], 0.02, 1.0),
    ]);
    let svg = scene_to_cli_svg(&p).unwrap();
    assert!(elements(&svg, "arrow-shaft").is_empty());
    assert_eq!(elements(&svg, "scene-line").len(), 1);
    assert!(bounds(elements(&svg, "scene-line")[0])[0] >= 99.0);
}

#[test]
fn asymmetric_domain_corners_fit_portrait_and_landscape_svg() {
    for (w, h) in [(200, 800), (800, 200)] {
        let mut p = prepared(&format!(
            r#"{{"width":{w},"height":{h},"domain":{{"min":[2,3,4],"max":[8,5,6]}},"arrows":false,"references":{{"grid_and_domain_box":false,"world_axes":false}}}}"#
        ));
        p.scene.lines = Arc::new(vec![
            line([2.0, 3.0, 5.0], [8.0, 3.0, 5.0], 0.001, 1.0),
            line([8.0, 3.0, 5.0], [8.0, 5.0, 5.0], 0.001, 1.0),
            line([8.0, 5.0, 5.0], [2.0, 5.0, 5.0], 0.001, 1.0),
            line([2.0, 5.0, 5.0], [2.0, 3.0, 5.0], 0.001, 1.0),
        ]);
        let svg = scene_to_cli_svg(&p).unwrap();
        assert_eq!(elements(&svg, "scene-line").len(), 4);
        for e in elements(&svg, "scene-line") {
            for [x, y] in points(e) {
                assert!(x > 0.0 && x < (w as f32) && y > 0.0 && y < (h as f32));
            }
        }
    }
}

#[test]
fn rejects_3d_before_serializing_even_an_empty_scene() {
    let p = prepared(r#"{"mode":"3d","density":4,"arrows":false}"#);
    let error = scene_to_cli_svg(&p).unwrap_err();
    assert!(matches!(error, CliError::Validation(_)));
    assert!(error.to_string().contains("3D SVG"));
}

fn slot(layer: LayerId) -> usize {
    LayerId::ALL
        .iter()
        .position(|candidate| *candidate == layer)
        .expect("every layer is in LayerId::ALL")
}

fn lines_differ(a: &[LineInstanceGPU], b: &[LineInstanceGPU]) -> bool {
    a.len() != b.len()
        || a.iter()
            .zip(b)
            .any(|(a, b)| a.a != b.a || a.b != b.b || a.color != b.color)
}

fn non_neutral_styles() -> LayerStyles {
    let mut styles = LayerStyles::default();
    styles.layers[slot(LayerId::Arrows)] = LayerOverride {
        opacity: Some(0.25),
        width: Some(2.5),
        color: Some(Rgb([12, 34, 56])),
    };
    styles.layers[slot(LayerId::Flow)] = LayerOverride {
        opacity: Some(0.35),
        width: Some(3.0),
        color: None,
    };
    styles.layers[slot(LayerId::Reference)] = LayerOverride {
        opacity: Some(0.45),
        width: Some(2.0),
        color: Some(Rgb([90, 80, 70])),
    };
    styles
}

fn two_d_options(json: &str) -> vector_field_explorer::cli::CliOptions {
    resolve_config(
        Some(parse_config(json).unwrap()),
        CliFlags {
            output: Some("plot.svg".into()),
            ..Default::default()
        },
    )
    .unwrap()
}

#[test]
fn cli_svg_export_ignores_arrows_flow_and_reference_overrides() {
    let options = two_d_options(
        r#"{"width":160,"height":100,"density":4,"formula":"(-y, x)","streamlines":true,"references":{"grid_and_domain_box":true,"world_axes":true}}"#,
    );
    let frame = output_frame(&options).unwrap();

    let mut styled = vector_field_explorer::viewmodel::State::new(false);
    assert!(styled.set_formula("(-y, x)".into()));
    styled.set_density(4);
    styled.set_streamlines(true);
    styled.layer_styles = non_neutral_styles();
    let mut neutral = styled.clone();
    neutral.layer_styles = LayerStyles::default();

    let blueprint_svg = |state: &vector_field_explorer::viewmodel::State| {
        let neutral_style = scaled_viewport_style(
            &state.viewport_style,
            &vector_field_explorer::layer_style::ResolvedLayerStyle {
                opacity: 1.0,
                width: 1.0,
                color: None,
            },
        );
        let mut scene = state.snapshot_blueprint();
        scene.lines = Arc::new(vector_field_explorer::app::compose_lines_with_frame_style(
            &scene.lines,
            &[],
            &state.scene_domain(),
            state.is_3d(),
            state.plane_frame(),
            state.show_grid,
            state.show_coordinate_system,
            &neutral_style,
        ));
        scene_to_cli_svg(&PreparedScene { scene, frame }).unwrap()
    };

    assert_eq!(
        blueprint_svg(&neutral),
        blueprint_svg(&styled),
        "SVG export must keep the unstyled blueprint"
    );
    // The styled raster scene really differs, so the equality above is not
    // vacuous.
    let styled_probe = prepare_scene_with_styles(&options, frame, &non_neutral_styles()).unwrap();
    let neutral_probe = prepare_scene(&options, frame).unwrap();
    assert_ne!(
        neutral_probe.scene.arrow_params, styled_probe.scene.arrow_params,
        "the Arrows override changes the styled scene"
    );
    assert!(
        lines_differ(&neutral_probe.scene.lines, &styled_probe.scene.lines),
        "the Flow/Reference overrides change the styled lines"
    );
    // The route the CLI uses for SVG (default styles) matches the neutral scene.
    assert_eq!(
        scene_to_cli_svg(&prepare_scene_with_styles(&options, frame, &LayerStyles::default()).unwrap())
            .unwrap(),
        scene_to_cli_svg(&neutral_probe).unwrap()
    );
}

#[test]
fn app_svg_entry_uses_the_neutral_blueprint_when_styles_are_committed() {
    let mut styled = VfApp::new();
    styled.state.set_mode(false);
    styled
        .state
        .commit_layer_styles(non_neutral_styles())
        .expect("test styles validate");
    styled.test_harness().advance_simulation();
    let styled_blueprint =
        styled.test_assemble_viewport_scene_with_styles(&LayerStyles::default());

    let mut neutral = VfApp::new();
    neutral.state.set_mode(false);
    neutral.test_harness().advance_simulation();
    let neutral_scene = neutral.test_harness().test_assemble_viewport_scene();

    let matrix = glam::Mat4::IDENTITY;
    assert_eq!(
        styled_blueprint.arrow_params, neutral_scene.arrow_params,
        "blueprint arrow params"
    );
    assert_eq!(
        styled_blueprint.arrows.len(),
        neutral_scene.arrows.len(),
        "arrow count"
    );
    assert_eq!(
        styled_blueprint.lines.len(),
        neutral_scene.lines.len(),
        "line count"
    );
    assert_eq!(
        styled_blueprint.dots.len(),
        neutral_scene.dots.len(),
        "dot count"
    );
    assert_eq!(
        styled_blueprint.trails.len(),
        neutral_scene.trails.len(),
        "trail count"
    );
    let styled_svg = screenshot::scene_to_svg(&styled_blueprint, matrix, 120, 80, [0.0; 4]);
    let neutral_svg = screenshot::scene_to_svg(&neutral_scene, matrix, 120, 80, [0.0; 4]);
    if styled_svg != neutral_svg {
        let at = styled_svg
            .bytes()
            .zip(neutral_svg.bytes())
            .position(|(a, b)| a != b)
            .unwrap_or(styled_svg.len().min(neutral_svg.len()));
        // `at` is a byte offset and may land inside a multi-byte character, so
        // build the diagnostic window from char boundaries instead of slicing.
        let window = |text: &str| {
            text.char_indices()
                .skip_while(|(index, _)| *index < at.saturating_sub(40))
                .take_while(|(index, _)| *index < at.saturating_add(80))
                .map(|(_, ch)| ch)
                .collect::<String>()
        };
        panic!(
            "first difference at {at}: styled {:?} neutral {:?}",
            window(&styled_svg),
            window(&neutral_svg)
        );
    }
    // The styled live assembly still keeps the committed style, so the
    // blueprint equality above is not vacuous.
    let styled_live = styled.test_harness().test_assemble_viewport_scene();
    assert_ne!(
        styled_live.arrow_params, styled_blueprint.arrow_params,
        "live scene should keep the Arrows width override"
    );
}

#[test]
fn directly_written_out_of_range_width_is_defensively_clamped() {
    for (written, expected) in [
        (-2.0, MIN_WIDTH_MULTIPLIER),
        (f32::INFINITY, 1.0),
        (99.0, MAX_WIDTH_MULTIPLIER),
    ] {
        let mut state = vector_field_explorer::viewmodel::State::new(true);
        let stored = state.arrow_params;
        state.layer_styles.layers[slot(LayerId::Arrows)].width = Some(written);
        let params = state.effective_arrow_params();
        assert_eq!(
            params.head_width,
            stored.head_width * expected,
            "arrow width {written} must resolve to {expected}"
        );
        assert_eq!(params.shaft_radius, stored.shaft_radius * expected);

        state.layer_styles.layers[slot(LayerId::Reference)].width = Some(written);
        let style = state.reference_viewport_style();
        let base = ViewportStyle::default();
        assert_eq!(
            style.grid_width,
            base.grid_width * expected,
            "reference width {written} must resolve to {expected}"
        );

        let resolved = vector_field_explorer::layer_style::ResolvedLayerStyle {
            opacity: 1.0,
            width: written,
            color: None,
        };
        let scaled = scaled_viewport_style(&base, &resolved);
        assert_eq!(scaled.box_width, base.box_width * expected);
        assert_eq!(scaled.axis_width, base.axis_width * expected);
    }
}

#[test]
fn directly_written_non_finite_opacity_is_defensively_neutral() {
    for (written, expected) in [
        (f32::NAN, 1.0),
        (f32::INFINITY, 1.0),
        (f32::NEG_INFINITY, 1.0),
        (99.0, 1.0),
        (-2.0, 0.0),
    ] {
        let mut state = vector_field_explorer::viewmodel::State::new(true);
        state.layer_styles.layers[slot(LayerId::Arrows)].opacity = Some(written);
        let arrows = state.snapshot().arrows;
        assert!(!arrows.is_empty(), "fixture must sample arrows");
        assert_eq!(
            arrows[0].color[3],
            (state.arrow_opacity * expected).clamp(0.0, 1.0),
            "arrow opacity {written} must resolve to {expected}"
        );

        state.layer_styles.layers[slot(LayerId::Reference)].opacity = Some(written);
        let style = state.reference_viewport_style();
        let base = ViewportStyle::default();
        for (actual, neutral) in [
            (style.grid_color[3], base.grid_color[3]),
            (style.box_color[3], base.box_color[3]),
            (style.axis_x_color[3], base.axis_x_color[3]),
            (style.axis_y_color[3], base.axis_y_color[3]),
            (style.axis_z_color[3], base.axis_z_color[3]),
        ] {
            assert_eq!(
                actual,
                (neutral * expected).clamp(0.0, 1.0),
                "reference opacity {written} must resolve to {expected}"
            );
        }
    }
}
