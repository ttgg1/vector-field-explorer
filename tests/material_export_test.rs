//! Task 4 Materials export provenance: exact request-time lines/metadata,
//! decoded PNG banner/tEXt, SVG visible text/metadata, mode isolation and
//! finite geometry.

use glam::{Mat4, UVec3, Vec3};

use vector_field_explorer::VfApp;
use vector_field_explorer::domain::Domain;
use vector_field_explorer::export_provenance::{
    ExportProvenance, material_export_provenance, provenance_lines, provenance_metadata,
};
use vector_field_explorer::material::{
    MaterialCase, MaterialScene, MaterialView, material_assumptions, material_family,
    sample_materials,
};
use vector_field_explorer::plane::PlaneAxis;
use vector_field_explorer::renderer::{ArrowInstanceGPU, SceneGPU};
use vector_field_explorer::source_scene::FieldMode;

fn scene() -> MaterialScene {
    MaterialScene {
        case: MaterialCase::PermeableBoundary,
        background_epsilon_r: 1.5,
        background_mu_r: 2.0,
        material_epsilon_r: 3.0,
        material_mu_r: 12.0,
        radius: 1.25,
        interface_axis: PlaneAxis::Y,
        interface_offset: 0.5,
        applied: [0.0, 2.0, -0.25],
        view: MaterialView::Applied,
    }
}

fn metadata_value<'a>(provenance: &'a ExportProvenance, key: &str) -> Option<&'a str> {
    provenance
        .metadata
        .iter()
        .find(|(candidate, _)| candidate == key)
        .map(|(_, value)| value.as_str())
}

#[test]
fn material_warning_lines_are_exact_and_ordered() {
    let scene = MaterialScene::default();
    let provenance = material_export_provenance(FieldMode::Materials, &scene, None);

    assert_eq!(
        provenance_lines(&provenance),
        &[
            "Materials - bounded analytic cases - no general solver".to_string(),
            "Background eps_r = 1, mu_r = 1".to_string(),
            "Material eps_r = 4, mu_r = 1".to_string(),
            "Applied/response: Total".to_string(),
            "Applied field: E = (1, 0, 0) V/m".to_string(),
            material_assumptions(MaterialCase::DielectricSphere).to_string(),
            "Materials sampling unavailable".to_string(),
        ]
    );
    assert!(provenance.material_mode);
    assert!(!provenance.source_mode);
    assert!(!provenance.induction_mode);
    assert!(!provenance.ode_mode);
}

#[test]
fn magnetic_applied_line_uses_tesla_and_the_material_family() {
    let scene = scene();
    assert_eq!(
        material_family(scene.case),
        vector_field_explorer::material::MaterialFamily::Magnetic
    );
    let provenance = material_export_provenance(FieldMode::Materials, &scene, None);
    assert!(
        provenance
            .warning_lines
            .iter()
            .any(|line| line == "Applied field: B = (0, 2, -0.25) T"),
        "{:?}",
        provenance.warning_lines,
    );
    assert!(
        provenance
            .warning_lines
            .iter()
            .any(|line| line == "Applied/response: Applied"),
    );
    assert!(
        provenance
            .warning_lines
            .iter()
            .any(|line| line == material_assumptions(MaterialCase::PermeableBoundary)),
    );
}

#[test]
fn material_metadata_uses_the_exact_keys_and_warning_only_when_present() {
    let scene = scene();
    let sampling = sample_materials(&scene, &Domain::default(), UVec3::splat(5), None);
    let provenance = material_export_provenance(FieldMode::Materials, &scene, Some(&sampling));

    for (key, value) in [
        ("vfe.model", "materials-bounded-analytic"),
        ("vfe.material_case", "permeable_boundary"),
        ("vfe.material_family", "magnetic"),
        ("vfe.background_eps_r", "1.5"),
        ("vfe.background_mu_r", "2"),
        ("vfe.material_eps_r", "3"),
        ("vfe.material_mu_r", "12"),
        ("vfe.material_view", "Applied"),
        ("vfe.material_units", "T"),
        ("vfe.material_interface_axis", "y"),
        ("vfe.material_interface_offset", "0.5"),
        ("vfe.material_applied", "(0, 2, -0.25)"),
    ] {
        assert_eq!(metadata_value(&provenance, key), Some(value), "{key}");
    }
    assert_eq!(
        metadata_value(&provenance, "vfe.material_warning"),
        sampling.warning.as_deref(),
    );

    let clean_sampling = sample_materials(
        &scene,
        &Domain::new(Vec3::splat(-0.4), Vec3::splat(0.4)),
        UVec3::splat(5),
        None,
    );
    assert!(clean_sampling.warning.is_none());
    let clean = material_export_provenance(FieldMode::Materials, &scene, Some(&clean_sampling));
    assert!(
        !clean
            .metadata
            .iter()
            .any(|(key, _)| key == "vfe.material_warning"),
        "the warning key is omitted when there is no warning",
    );
}

#[test]
fn non_materials_provenance_carries_no_material_context() {
    for mode in [
        FieldMode::Formula,
        FieldMode::Sources,
        FieldMode::Induction,
        FieldMode::Ode,
    ] {
        let provenance = material_export_provenance(mode, &scene(), None);
        assert!(provenance.warning_lines.is_empty());
        assert!(provenance.metadata.is_empty());
        assert!(!provenance.material_mode);
    }
}

#[test]
fn current_sampling_warning_is_never_suppressed() {
    let mut response = MaterialScene {
        case: MaterialCase::DielectricSphere,
        view: MaterialView::Response,
        ..MaterialScene::default()
    };
    response.material_epsilon_r = 1.0;
    let sampling = sample_materials(
        &response,
        &Domain::new(Vec3::splat(-2.0), Vec3::splat(2.0)),
        UVec3::splat(3),
        None,
    );
    assert!(sampling.response_empty);
    assert_eq!(
        sampling.warning.as_deref(),
        Some("Response is identically zero for this view."),
    );
    let provenance = material_export_provenance(FieldMode::Materials, &response, Some(&sampling));
    assert!(
        provenance
            .warning_lines
            .last()
            .is_some_and(|line| line == "Response is identically zero for this view."),
        "{:?}",
        provenance.warning_lines,
    );
    assert_eq!(
        metadata_value(&provenance, "vfe.material_warning"),
        Some("Response is identically zero for this view."),
    );

    let boundary = MaterialScene {
        case: MaterialCase::DielectricBoundary,
        radius: 1.0,
        ..MaterialScene::default()
    };
    let domain = Domain::new(Vec3::splat(-1.0), Vec3::splat(1.0));
    let sampling = sample_materials(&boundary, &domain, UVec3::splat(3), None);
    assert!(sampling.omitted_interface > 0);
    let provenance = material_export_provenance(FieldMode::Materials, &boundary, Some(&sampling));
    assert!(
        provenance
            .warning_lines
            .iter()
            .any(|line| line.contains("exactly on the interface")),
        "{:?}",
        provenance.warning_lines,
    );
    assert!(
        metadata_value(&provenance, "vfe.material_warning")
            .is_some_and(|warning| warning.contains("exactly on the interface")),
    );
}

#[test]
fn invalid_scene_keeps_the_materials_scene_invalid_warning() {
    let scene = MaterialScene {
        material_epsilon_r: 0.5,
        ..MaterialScene::default()
    };
    let provenance = material_export_provenance(FieldMode::Materials, &scene, None);
    assert!(
        provenance
            .warning_lines
            .iter()
            .any(|line| line.starts_with("Materials scene invalid: ")),
        "{:?}",
        provenance.warning_lines,
    );
    assert!(
        metadata_value(&provenance, "vfe.material_warning")
            .is_some_and(|warning| warning.contains("Materials scene invalid: ")),
    );
    assert!(
        provenance
            .warning_lines
            .iter()
            .any(|line| line == "Materials sampling unavailable"),
    );
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
fn decoded_png_banner_and_text_chunks_carry_materials_context() {
    let scene = scene();
    let sampling = sample_materials(&scene, &Domain::default(), UVec3::splat(5), None);
    let provenance = material_export_provenance(FieldMode::Materials, &scene, Some(&sampling));

    let (width, height) = (360u32, 220u32);
    let mut rgba = vec![255u8; (width * height * 4) as usize];
    vector_field_explorer::screenshot::apply_export_banner(&mut rgba, width, height, &provenance)
        .unwrap();
    let png = vector_field_explorer::export::encode_png_with_text(
        &rgba,
        width,
        height,
        provenance_metadata(&provenance),
    )
    .unwrap();
    let (pixels, _, _, text) = decode_png(&png);

    for key in [
        "vfe.model",
        "vfe.material_case",
        "vfe.material_family",
        "vfe.background_eps_r",
        "vfe.background_mu_r",
        "vfe.material_eps_r",
        "vfe.material_mu_r",
        "vfe.material_view",
        "vfe.material_units",
        "vfe.material_interface_axis",
        "vfe.material_interface_offset",
        "vfe.material_applied",
        "vfe.material_warning",
    ] {
        assert!(
            text.iter().any(|chunk| chunk.keyword == key),
            "missing PNG chunk {key}: {text:?}",
        );
    }
    assert!(text.iter().any(|chunk| {
        chunk.keyword == "vfe.model" && chunk.text == "materials-bounded-analytic"
    }));
    let units = text
        .iter()
        .find(|chunk| chunk.keyword == "vfe.material_units")
        .unwrap();
    assert_eq!(units.text, "T");

    // The baked strip and its near-white glyphs occupy the bottom of the frame.
    let banner = &pixels[((height - 10) * width * 4) as usize..];
    assert!(
        banner
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel[..3] != [255, 255, 255]),
        "the dark banner is decoded into the PNG pixels",
    );
    let near_white = banner
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[0] > 200 && pixel[1] > 200 && pixel[2] > 200)
        .count();
    assert!(near_white >= 40, "expected visible banner text pixels");
}

#[test]
fn svg_visible_text_and_metadata_carry_materials_context() {
    let scene = scene();
    let sampling = sample_materials(&scene, &Domain::default(), UVec3::splat(5), None);
    let provenance = material_export_provenance(FieldMode::Materials, &scene, Some(&sampling));
    let svg = vector_field_explorer::screenshot::scene_to_svg_with_provenance(
        &SceneGPU {
            mode_3d: false,
            ..Default::default()
        },
        Mat4::IDENTITY,
        640,
        360,
        [0.0, 0.0, 0.0, 1.0],
        Some(&provenance),
    );

    assert!(svg.contains("<text"), "{svg}");
    assert!(svg.contains("Materials - bounded analytic cases - no general solver"));
    assert!(svg.contains("Background eps_r = 1.5, mu_r = 2"));
    assert!(svg.contains("Applied/response: Applied"));
    assert!(svg.contains("Applied field: B = (0, 2, -0.25) T"));
    assert!(svg.contains("<metadata>"));
    assert!(svg.contains("<vfe.model>materials-bounded-analytic</vfe.model>"));
    assert!(svg.contains("<vfe.material_units>T</vfe.material_units>"));

    // The SVG export remains 2D-only in 3D; provenance does not relax it.
    assert!(!vector_field_explorer::screenshot::svg_export_enabled(true));
    assert!(vector_field_explorer::screenshot::svg_export_enabled(false));
}

#[test]
fn no_nonfinite_arrow_is_serialized_and_export_does_not_sample() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Materials);
    app.test_harness().advance_simulation();
    let sampling = app.state.material_sampling.clone().unwrap();
    assert!(sampling.arrows.iter().all(|arrow| {
        arrow.position.is_finite() && arrow.dir.is_finite() && arrow.mag.is_finite()
    }));

    let scene = SceneGPU {
        mode_3d: false,
        arrows: std::sync::Arc::new(vec![
            ArrowInstanceGPU {
                origin: [0.0, 0.0, 0.0, 1.0],
                dir_len: [1.0, 0.0, 0.0, 1.0],
                color: [1.0, 1.0, 1.0, 1.0],
            },
            ArrowInstanceGPU {
                origin: [f32::NAN, 0.0, 0.0, 1.0],
                dir_len: [f32::NAN, 0.0, 0.0, f32::INFINITY],
                color: [1.0, 1.0, 1.0, 1.0],
            },
        ]),
        ..Default::default()
    };
    let provenance = material_export_provenance(
        FieldMode::Materials,
        &app.state.material,
        app.state.material_sampling.as_ref(),
    );
    let svg = vector_field_explorer::screenshot::scene_to_svg_with_provenance(
        &scene,
        Mat4::IDENTITY,
        320,
        240,
        [0.0, 0.0, 0.0, 1.0],
        Some(&provenance),
    );
    assert!(!svg.to_lowercase().contains("nan"), "{svg}");
    assert_eq!(svg.matches("marker-end=\"url(#ah)\"").count(), 1);
}

#[test]
fn app_routing_uses_materials_provenance_and_formula_stays_clean() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Materials);
    app.test_harness().advance_simulation();
    app.test_harness().request_png_export();
    let provenance = app.test_harness().export_png_provenance().unwrap();
    assert!(provenance.material_mode);
    assert!(!provenance.source_mode);
    assert_eq!(
        provenance.warning_lines[0],
        "Materials - bounded analytic cases - no general solver",
    );
    assert!(
        provenance
            .metadata
            .iter()
            .any(|(key, value)| key == "vfe.model" && value == "materials-bounded-analytic"),
    );

    let mut formula = VfApp::new();
    formula.test_harness().request_png_export();
    assert!(formula.test_harness().export_png_provenance().is_none());
}
