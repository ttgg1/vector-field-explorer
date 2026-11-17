//! Task 4 ODE export provenance: request-time warning text, decoded PNG banner
//! and `tEXt`, visible SVG text and `<metadata>`, and no fabricated geometry.

use std::sync::Arc;

use vector_field_explorer::VfApp;
use vector_field_explorer::export_provenance::{
    ExportProvenance, ode_export_provenance, provenance_metadata,
};
use vector_field_explorer::ode::{ODE_STEP, OdeScene, OdeSolution, solve_ode};
use vector_field_explorer::renderer::SceneGPU;
use vector_field_explorer::sim_clock::SimClock;
use vector_field_explorer::source_scene::FieldMode;
use vector_field_explorer::viewmodel::State;

fn solution_and_scene() -> (OdeScene, OdeSolution) {
    let state = State::new(false);
    let scene = OdeScene::default();
    let solution = solve_ode(&state, &scene).expect("default scene solves");
    (scene, solution)
}

fn metadata_value<'a>(provenance: &'a ExportProvenance, key: &str) -> Option<&'a str> {
    provenance
        .metadata
        .iter()
        .find(|(candidate, _)| candidate == key)
        .map(|(_, value)| value.as_str())
}

fn decode_png(png: &[u8]) -> (Vec<u8>, u32, u32, Vec<png::text_metadata::TEXtChunk>) {
    let decoder = png::Decoder::new(std::io::Cursor::new(png));
    let mut reader = decoder.read_info().unwrap();
    let info = reader.info();
    let width = info.width;
    let height = info.height;
    let text = info.uncompressed_latin1_text.clone();
    let mut pixels = vec![0u8; width as usize * height as usize * 4];
    reader.next_frame(&mut pixels).unwrap();
    (pixels, width, height, text)
}

#[test]
fn ode_available_provenance_has_exact_lines_and_metadata() {
    let (scene, solution) = solution_and_scene();
    let provenance = ode_export_provenance(
        FieldMode::Ode,
        &scene,
        Some(&solution),
        None,
        &SimClock::default(),
    )
    .expect("Ode always carries provenance");

    assert!(provenance.ode_mode);
    assert!(!provenance.source_mode);
    assert!(!provenance.induction_mode);
    assert_eq!(
        provenance.warning_lines[0],
        format!("ODE - fixed-step RK4 teaching model - h = {ODE_STEP}")
    );
    assert!(
        provenance
            .warning_lines
            .iter()
            .any(|line| line.contains("timeline: duration = 20") && line.contains("step = 0.005")),
        "{:?}",
        provenance.warning_lines
    );
    assert!(
        provenance
            .warning_lines
            .iter()
            .any(|line| line.contains("bounded")),
        "{:?}",
        provenance.warning_lines
    );
    assert_eq!(
        metadata_value(&provenance, "vfe.model"),
        Some("ode-fixed-step-rk4")
    );
    assert_eq!(
        metadata_value(&provenance, "vfe.ode_source"),
        Some("equations")
    );
    assert_eq!(metadata_value(&provenance, "vfe.ode_dimension"), Some("2"));
    assert_eq!(metadata_value(&provenance, "vfe.ode_step"), Some("0.005"));
    assert_eq!(metadata_value(&provenance, "vfe.ode_duration"), Some("20"));
}

#[test]
fn decoded_png_has_banner_text_pixels_and_ode_text_chunks() {
    let (scene, solution) = solution_and_scene();
    let provenance = ode_export_provenance(
        FieldMode::Ode,
        &scene,
        Some(&solution),
        None,
        &SimClock::default(),
    )
    .unwrap();

    let mut rgba = vec![255u8; 200 * 140 * 4];
    vector_field_explorer::screenshot::apply_export_banner(&mut rgba, 200, 140, &provenance)
        .unwrap();
    let png = vector_field_explorer::export::encode_png_with_text(
        &rgba,
        200,
        140,
        provenance_metadata(&provenance),
    )
    .unwrap();

    let (pixels, width, height, text) = decode_png(&png);
    assert_eq!((width, height), (200, 140));
    for (key, expected) in [
        ("vfe.model", "ode-fixed-step-rk4"),
        ("vfe.ode_source", "equations"),
        ("vfe.ode_dimension", "2"),
        ("vfe.ode_step", "0.005"),
        ("vfe.ode_duration", "20"),
    ] {
        let chunk = text
            .iter()
            .find(|chunk| chunk.keyword == key)
            .unwrap_or_else(|| panic!("missing {key} text chunk"));
        assert_eq!(chunk.text, expected, "{key}");
    }

    // Inspect the actual bottom banner region, not just the struct fields:
    // the dark strip exists and the built-in font painted near-white glyphs.
    let banner_start = (height as usize - 60) * width as usize * 4;
    let banner = &pixels[banner_start..];
    let banner_pixels = banner.as_chunks::<4>().0;
    assert!(
        banner_pixels
            .iter()
            .any(|pixel| pixel[..3] != [255, 255, 255]),
        "expected the dark banner to change the white capture"
    );
    let text_pixels = banner_pixels
        .iter()
        .filter(|pixel| pixel[0] > 200 && pixel[1] > 200 && pixel[2] > 200)
        .count();
    assert!(
        text_pixels >= 40,
        "expected near-white warning text pixels in the banner, found {text_pixels}"
    );
}

#[test]
fn svg_has_visible_ode_text_and_metadata() {
    let (scene, solution) = solution_and_scene();
    let provenance = ode_export_provenance(
        FieldMode::Ode,
        &scene,
        Some(&solution),
        None,
        &SimClock::default(),
    )
    .unwrap();
    let gpu_scene = SceneGPU {
        mode_3d: false,
        ..Default::default()
    };
    let svg = vector_field_explorer::screenshot::scene_to_svg_with_provenance(
        &gpu_scene,
        glam::Mat4::IDENTITY,
        320,
        240,
        [0.0, 0.0, 0.0, 1.0],
        Some(&provenance),
    );

    assert!(svg.contains("<text"), "{svg}");
    assert!(svg.contains("ODE - fixed-step RK4 teaching model"), "{svg}");
    assert!(svg.contains("bounded"), "{svg}");
    assert!(svg.contains("<metadata"), "{svg}");
    assert!(svg.contains("vfe.model"), "{svg}");
    assert!(svg.contains("ode-fixed-step-rk4"), "{svg}");
    assert!(svg.contains("vfe.ode_source"), "{svg}");
    assert!(svg.contains("vfe.ode_duration"), "{svg}");
}

#[test]
fn non_ode_exports_carry_no_ode_metadata_or_warning() {
    let (scene, solution) = solution_and_scene();
    assert!(
        ode_export_provenance(
            FieldMode::Formula,
            &scene,
            Some(&solution),
            None,
            &SimClock::default(),
        )
        .is_none()
    );

    let mut app = VfApp::new();
    app.test_harness().request_png_export();
    assert!(app.test_harness().export_png_provenance().is_none());
    app.test_harness().request_svg_export();
    assert!(app.test_harness().export_svg_provenance().is_none());
}

#[test]
fn unavailable_solution_warns_without_fabricated_geometry() {
    let (scene, _) = solution_and_scene();
    let provenance = ode_export_provenance(
        FieldMode::Ode,
        &scene,
        None,
        Some("initial condition 0 contains a non-finite value"),
        &SimClock::default(),
    )
    .unwrap();
    assert!(
        provenance
            .warning_lines
            .iter()
            .any(|line| line == "ODE solution unavailable"),
        "{:?}",
        provenance.warning_lines
    );
    assert!(
        provenance.warning_lines.iter().any(|line| {
            line == "ODE scene invalid: initial condition 0 contains a non-finite value"
        }),
        "{:?}",
        provenance.warning_lines
    );
    assert_eq!(
        metadata_value(&provenance, "vfe.validation_error"),
        Some("initial condition 0 contains a non-finite value")
    );

    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Ode);
    assert!(app.state.ode_solution.is_none());
    assert_eq!(app.test_harness().ode_line_count(), 0);
    assert_eq!(app.test_harness().ode_marker_count(), 0);
    app.test_harness().request_png_export();
    let pending = app.test_harness().export_png_provenance().unwrap();
    assert!(
        pending
            .warning_lines
            .iter()
            .any(|line| line == "ODE solution unavailable"),
        "{:?}",
        pending.warning_lines
    );
}

#[test]
fn app_captures_ode_provenance_for_png_and_svg_without_solving() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Ode);
    app.test_harness().advance_simulation();
    let before = Arc::clone(app.state.ode_solution.as_ref().expect("solution"));
    let epoch = app.test_harness().ode_epoch();

    app.test_harness().request_png_export();
    let png = app.test_harness().export_png_provenance().unwrap();
    assert!(png.ode_mode);
    assert_eq!(
        metadata_value(&png, "vfe.model"),
        Some("ode-fixed-step-rk4")
    );
    assert!(Arc::ptr_eq(
        &before,
        app.state.ode_solution.as_ref().expect("solution kept")
    ));
    assert_eq!(app.test_harness().ode_epoch(), epoch);

    app.test_harness().request_svg_export();
    let svg = app.test_harness().export_svg_provenance().unwrap();
    assert!(svg.ode_mode);
    assert!(Arc::ptr_eq(
        &before,
        app.state.ode_solution.as_ref().expect("solution kept")
    ));
    assert_eq!(app.test_harness().ode_epoch(), epoch);
}

#[test]
fn ode_svg_still_requires_2d_and_never_carries_provenance_when_rejected() {
    let mut app = VfApp::new();
    app.state.set_mode(true);
    app.state.set_field_mode(FieldMode::Ode);
    app.test_harness().request_svg_export();
    assert!(!app.test_harness().svg_request_pending());
    assert!(app.test_harness().export_svg_provenance().is_none());
    assert!(app.test_harness().export_png_provenance().is_none());
}
