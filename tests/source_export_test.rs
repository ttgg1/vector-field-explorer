//! Source export provenance, PNG banner/metadata and SVG warning tests.

use std::sync::Arc;

use vector_field_explorer::export_provenance::{ExportProvenance, source_export_provenance};
use vector_field_explorer::renderer::{LineInstanceGPU, SceneGPU};
use vector_field_explorer::source_scene::{
    FieldMode, SourceDisplay, SourceFamilyStatus, SourceSampling, SourceScene,
};

fn sampling(allow_inaccurate: bool) -> SourceSampling {
    SourceSampling {
        arrows: Vec::new(),
        electric: SourceFamilyStatus {
            sources: 2,
            drawn: 1,
            target_met: 1,
            accuracy_not_met_total: 1,
            accuracy_not_met_drawn: usize::from(allow_inaccurate),
            unavailable: 1,
            empty: false,
        },
        magnetic: SourceFamilyStatus {
            sources: 0,
            drawn: 0,
            target_met: 0,
            accuracy_not_met_total: 0,
            accuracy_not_met_drawn: 0,
            unavailable: 0,
            empty: true,
        },
        softening: 0.1,
        allow_inaccurate,
        validation_error: None,
    }
}

#[test]
fn formula_exports_have_no_source_provenance() {
    let scene = SourceScene::default();
    assert!(source_export_provenance(FieldMode::Formula, &scene, None).is_none());
}

#[test]
fn source_warning_lines_and_metadata_are_exact_and_separate() {
    let provenance =
        source_export_provenance(FieldMode::Sources, &SourceScene::default(), None).unwrap();
    assert_eq!(
        provenance.warning_lines.last().unwrap(),
        "Source sampling unavailable"
    );
    assert_eq!(
        provenance.warning_lines[0],
        "Sources - softened teaching model - a = 0.1"
    );
    assert_eq!(
        provenance
            .metadata
            .iter()
            .find(|(key, _)| key == "vfe.source_model")
            .unwrap()
            .1,
        "softened teaching model"
    );

    let scene = SourceScene {
        display: SourceDisplay::Electric,
        ..SourceScene::default()
    };
    let provenance =
        source_export_provenance(FieldMode::Sources, &scene, Some(&sampling(true))).unwrap();
    assert!(
        provenance
            .warning_lines
            .iter()
            .any(|line| line == "Electric - accuracy not met: 0 hidden, 1 drawn")
    );
    assert!(
        provenance
            .warning_lines
            .iter()
            .any(|line| line == "Unavailable: 1")
    );
    assert!(
        provenance
            .metadata
            .iter()
            .any(|(key, value)| key == "vfe.accuracy_not_met_total" && value == "1")
    );
    assert!(
        provenance
            .metadata
            .iter()
            .any(|(key, value)| key == "vfe.accuracy_not_met_drawn" && value == "1")
    );
}

#[test]
fn opt_in_adds_visible_data_but_never_removes_the_warning() {
    let scene = SourceScene {
        display: SourceDisplay::Electric,
        ..SourceScene::default()
    };
    let without =
        source_export_provenance(FieldMode::Sources, &scene, Some(&sampling(false))).unwrap();
    let with = source_export_provenance(FieldMode::Sources, &scene, Some(&sampling(true))).unwrap();
    assert!(
        with.metadata
            .iter()
            .any(|(key, value)| key == "vfe.accuracy_not_met_drawn" && value == "1")
    );
    assert!(
        without
            .metadata
            .iter()
            .any(|(key, value)| key == "vfe.accuracy_not_met_drawn" && value == "0")
    );
    assert_eq!(
        without
            .warning_lines
            .iter()
            .filter(|line| line.contains("accuracy not met"))
            .count(),
        1
    );
    assert_eq!(
        with.warning_lines
            .iter()
            .filter(|line| line.contains("accuracy not met"))
            .count(),
        1
    );
}

#[test]
fn missing_or_invalid_sampling_keeps_the_warning() {
    let mut scene = SourceScene::default();
    let missing = source_export_provenance(FieldMode::Sources, &scene, None).unwrap();
    assert!(
        missing
            .warning_lines
            .iter()
            .any(|line| line == "Source sampling unavailable")
    );
    assert!(missing.warning_lines[0].contains("softened teaching model"));
    scene.softening = 0.1;
    let mut invalid = sampling(false);
    invalid.validation_error = Some("index 0: charge must be finite".to_string());
    let provenance = source_export_provenance(FieldMode::Sources, &scene, Some(&invalid)).unwrap();
    assert!(
        provenance
            .warning_lines
            .iter()
            .any(|line| line.contains("invalid"))
    );
    assert!(
        provenance
            .metadata
            .iter()
            .any(|(key, _)| key == "vfe.validation_error")
    );
}

fn sample_provenance() -> ExportProvenance {
    ExportProvenance {
        warning_lines: vec![
            "Sources - softened teaching model - a = 0.1".to_string(),
            "Electric - accuracy not met: 1 hidden, 0 drawn".to_string(),
        ],
        metadata: vec![
            (
                "vfe.source_model".to_string(),
                "softened teaching model".to_string(),
            ),
            ("vfe.softening".to_string(), "0.1".to_string()),
        ],
        source_mode: true,
    }
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
fn png_banner_and_text_chunks_are_in_the_decoded_file() {
    let mut rgba = vec![255u8; 160 * 100 * 4];
    let provenance = sample_provenance();
    vector_field_explorer::screenshot::apply_export_banner(&mut rgba, 160, 100, &provenance)
        .unwrap();
    let png = vector_field_explorer::export::encode_png_with_text(
        &rgba,
        160,
        100,
        vector_field_explorer::export_provenance::provenance_metadata(&provenance),
    )
    .unwrap();
    let (pixels, _, _, text) = decode_png(&png);
    assert!(text.iter().any(|chunk| {
        chunk.keyword == "vfe.source_model" && chunk.text == "softened teaching model"
    }));
    assert!(
        text.iter()
            .any(|chunk| { chunk.keyword == "vfe.softening" && chunk.text == "0.1" })
    );

    let bottom = &pixels[90 * 160 * 4..100 * 160 * 4];
    let bottom_pixels = bottom.as_chunks::<4>().0;
    // Existing assertion: the dark banner strip changed the white background.
    assert!(
        bottom_pixels
            .iter()
            .any(|pixel| pixel[..3] != [255, 255, 255])
    );
    // Controller addendum: prove the warning *text pixels* exist, not just the
    // dark backing strip. The built-in font paints near-white glyphs.
    let near_white = bottom_pixels
        .iter()
        .filter(|pixel| pixel[0] > 200 && pixel[1] > 200 && pixel[2] > 200)
        .count();
    assert!(
        near_white >= 20,
        "expected near-white warning text pixels in the banner, found {near_white}"
    );
}

#[test]
fn png_metadata_is_sanitized_to_printable_ascii() {
    let rgba = vec![255u8; 2 * 2 * 4];
    let metadata = vec![(
        "vfe.validation_error".to_string(),
        "bad\nvalue\u{00e9}".to_string(),
    )];
    let png = vector_field_explorer::export::encode_png_with_text(&rgba, 2, 2, &metadata).unwrap();
    let (_, _, _, text) = decode_png(&png);
    let chunk = text
        .iter()
        .find(|chunk| chunk.keyword == "vfe.validation_error")
        .unwrap();
    assert_eq!(chunk.text, "bad value?");
}

#[test]
fn svg_warning_is_visible_and_metadata_is_present() {
    let scene = vector_field_explorer::renderer::SceneGPU {
        mode_3d: false,
        ..Default::default()
    };
    let provenance = ExportProvenance {
        warning_lines: vec!["Sources - softened teaching model - a = 0.1".to_string()],
        metadata: vec![("vfe.softening".to_string(), "0.1".to_string())],
        source_mode: true,
    };
    let svg = vector_field_explorer::screenshot::scene_to_svg_with_provenance(
        &scene,
        glam::Mat4::IDENTITY,
        320,
        240,
        [0.0, 0.0, 0.0, 1.0],
        Some(&provenance),
    );
    assert!(svg.contains("<text"));
    assert!(svg.contains("softened teaching model"));
    assert!(svg.contains("<metadata"));
    assert!(svg.contains("vfe.softening"));
}

#[test]
fn svg_text_and_metadata_are_xml_escaped() {
    let scene = SceneGPU {
        mode_3d: false,
        ..Default::default()
    };
    let provenance = ExportProvenance {
        warning_lines: vec!["A & B <x>".to_string()],
        metadata: vec![(
            "vfe.validation_error".to_string(),
            "\"q\" & <tag>".to_string(),
        )],
        source_mode: true,
    };
    let svg = vector_field_explorer::screenshot::scene_to_svg_with_provenance(
        &scene,
        glam::Mat4::IDENTITY,
        320,
        240,
        [0.0, 0.0, 0.0, 1.0],
        Some(&provenance),
    );
    assert!(svg.contains("A &amp; B &lt;x&gt;"), "{svg}");
    assert!(svg.contains("&quot;q&quot; &amp; &lt;tag&gt;"), "{svg}");
}

fn line(a: [f32; 3], b: [f32; 3], width: f32) -> LineInstanceGPU {
    LineInstanceGPU {
        a: [a[0], a[1], a[2], 1.0],
        b: [b[0], b[1], b[2], width],
        color: [1.0, 1.0, 1.0, 1.0],
    }
}

fn svg_with_line(line: LineInstanceGPU) -> String {
    let scene = SceneGPU {
        mode_3d: false,
        lines: Arc::new(vec![line]),
        ..Default::default()
    };
    vector_field_explorer::screenshot::scene_to_svg_with_provenance(
        &scene,
        glam::Mat4::IDENTITY,
        320,
        240,
        [0.0, 0.0, 0.0, 1.0],
        Some(&ExportProvenance {
            warning_lines: vec!["Sources - softened teaching model - a = 0.1".to_string()],
            metadata: Vec::new(),
            source_mode: true,
        }),
    )
}

#[test]
fn svg_stroke_width_converts_world_units_to_pixels_and_clamps() {
    // Identity projection maps a 1-unit world span to 160 px here, so a 0.01
    // world width is 1.6 px.
    let svg = svg_with_line(line([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 0.01));
    assert!(svg.contains("stroke-width=\"1.60\""), "{svg}");
    // A very wide stroke is clamped to the 3 px ceiling.
    let svg = svg_with_line(line([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 10.0));
    assert!(svg.contains("stroke-width=\"3.00\""), "{svg}");
}

#[test]
fn svg_zero_world_distance_falls_back_to_existing_width_floor() {
    let svg = svg_with_line(line([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 0.8));
    assert!(svg.contains("stroke-width=\"0.80\""), "{svg}");
    let svg = svg_with_line(line([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 0.1));
    assert!(svg.contains("stroke-width=\"0.50\""), "{svg}");
}

#[test]
fn no_provenance_svg_keeps_existing_stroke_width_semantics() {
    let scene = SceneGPU {
        mode_3d: false,
        lines: Arc::new(vec![line([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 0.01)]),
        ..Default::default()
    };
    let svg = vector_field_explorer::screenshot::scene_to_svg(
        &scene,
        glam::Mat4::IDENTITY,
        320,
        240,
        [0.0, 0.0, 0.0, 1.0],
    );
    assert!(svg.contains("stroke-width=\"0.50\""), "{svg}");
    assert!(!svg.contains("<text"), "{svg}");
    assert!(!svg.contains("<metadata"), "{svg}");
}

#[test]
fn screenshot_request_keeps_the_provenance_captured_at_request_time() {
    use vector_field_explorer::screenshot::Screenshot;

    let provenance = sample_provenance();
    let mut shot = Screenshot::new();
    shot.request_with_provenance(Some(provenance.clone()));
    assert_eq!(
        shot.take_request_with_provenance(),
        Some(Some(provenance.clone()))
    );
    assert_eq!(shot.take_request_with_provenance(), None);

    shot.request();
    assert_eq!(shot.take_request_with_provenance(), Some(None));

    shot.request_svg_with_provenance(Some(provenance.clone()));
    assert_eq!(
        shot.take_svg_request_with_provenance(),
        Some(Some(provenance))
    );
    assert_eq!(shot.take_svg_request_with_provenance(), None);
    assert!(!shot.take_svg_request());
}
