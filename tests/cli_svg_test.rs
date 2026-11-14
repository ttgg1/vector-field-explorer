use std::sync::Arc;

use glam::{Mat4, Vec3};
use vector_field_explorer::{
    arrow_style::ArrowStyle2D,
    cli::{
        CliError, CliFlags, PreparedScene, output_frame, parse_config, prepare_scene,
        resolve_config, scene_to_cli_svg,
    },
    renderer::{ArrowInstanceGPU, LineInstanceGPU},
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
