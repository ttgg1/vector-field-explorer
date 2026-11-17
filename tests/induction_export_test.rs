//! Induction export provenance: exact warning lines/metadata, decoded PNG
//! pixels/text chunks and visible SVG warning/metadata.

use glam::Mat4;

use vector_field_explorer::export_provenance::{
    induction_export_provenance, provenance_metadata, source_export_provenance,
};
use vector_field_explorer::induction::{
    InductionCase, InductionFamilyStatus, InductionSampling, InductionScene,
};
use vector_field_explorer::renderer::SceneGPU;
use vector_field_explorer::source_scene::{FieldMode, SourceDisplay, SourceScene};

fn status(
    drawn: usize,
    accuracy_not_met_total: usize,
    accuracy_not_met_drawn: usize,
    unavailable: usize,
) -> InductionFamilyStatus {
    InductionFamilyStatus {
        drawn,
        target_met: 0,
        accuracy_not_met_total,
        accuracy_not_met_drawn,
        unavailable,
    }
}

fn sampling(electric: InductionFamilyStatus, magnetic: InductionFamilyStatus) -> InductionSampling {
    InductionSampling {
        arrows: Vec::new(),
        electric,
        magnetic,
        current: 1.0,
        derivative: 0.25,
        derivative_source: None,
        softening: 0.1,
        validation_error: None,
    }
}

fn metadata_value<'a>(
    provenance: &'a vector_field_explorer::export_provenance::ExportProvenance,
    key: &str,
) -> Option<&'a str> {
    provenance
        .metadata
        .iter()
        .find(|(candidate, _)| candidate == key)
        .map(|(_, value)| value.as_str())
}

#[test]
fn induction_provenance_is_none_unless_induction_mode() {
    let scene = InductionScene::default();
    assert!(
        induction_export_provenance(FieldMode::Formula, &scene, 0.1, None).is_none(),
        "formula exports keep their existing bytes",
    );
    assert!(
        induction_export_provenance(FieldMode::Sources, &scene, 0.1, None).is_none(),
        "source exports use the source provenance helper",
    );
    let provenance = induction_export_provenance(FieldMode::Induction, &scene, 0.1, None)
        .expect("induction exports always carry provenance");
    assert!(provenance.induction_mode);
    assert!(!provenance.source_mode);
}

#[test]
fn induction_warning_lines_and_metadata_are_exact() {
    let scene = InductionScene {
        case: InductionCase::Solenoid,
        length: 2.0,
        turns: 8,
        ..InductionScene::default()
    };
    let sampling = sampling(status(1, 1, 1, 0), status(0, 0, 0, 0));
    let provenance =
        induction_export_provenance(FieldMode::Induction, &scene, 0.1, Some(&sampling)).unwrap();

    assert_eq!(
        provenance.warning_lines[0],
        "Induction - quasistatic axisymmetric softened model - a = 0.1",
    );
    assert!(
        provenance
            .warning_lines
            .iter()
            .any(|line| line == "Both - accuracy not met: 0 hidden, 1 drawn"),
        "{:?}",
        provenance.warning_lines,
    );

    for (key, value) in [
        ("vfe.model", "quasistatic-axisymmetric-faraday"),
        ("vfe.induction_case", "solenoid"),
        ("vfe.waveform", "amplitude*cos(2*pi*frequency*t)"),
        (
            "vfe.waveform_derivative",
            "-amplitude*2*pi*frequency*sin(2*pi*frequency*t)",
        ),
        ("vfe.amplitude", "1"),
        ("vfe.frequency", "0.25"),
        ("vfe.softening", "0.1"),
        ("vfe.display", "Both"),
        ("vfe.accuracy_not_met_total", "1"),
        ("vfe.accuracy_not_met_drawn", "1"),
        ("vfe.unavailable", "0"),
    ] {
        assert_eq!(
            metadata_value(&provenance, key),
            Some(value),
            "metadata {key}",
        );
    }
    assert!(
        metadata_value(&provenance, "vfe.validation_error").is_none(),
        "only present when there is an error",
    );

    // The current-loop spelling is the stable persisted/export token.
    let loop_scene = InductionScene::default();
    let loop_provenance =
        induction_export_provenance(FieldMode::Induction, &loop_scene, 0.1, None).unwrap();
    assert_eq!(
        metadata_value(&loop_provenance, "vfe.induction_case"),
        Some("current_loop"),
    );
}

#[test]
fn missing_or_invalid_sampling_keeps_a_warning() {
    let scene = InductionScene::default();
    let missing = induction_export_provenance(FieldMode::Induction, &scene, 0.1, None).unwrap();
    assert_eq!(
        missing.warning_lines.last().unwrap(),
        "Induction sampling unavailable",
    );
    assert_eq!(
        metadata_value(&missing, "vfe.accuracy_not_met_total"),
        Some("0")
    );
    assert_eq!(
        metadata_value(&missing, "vfe.accuracy_not_met_drawn"),
        Some("0")
    );
    assert_eq!(metadata_value(&missing, "vfe.unavailable"), Some("0"));

    let mut invalid = sampling(
        InductionFamilyStatus::default(),
        InductionFamilyStatus::default(),
    );
    invalid.validation_error = Some("induction waveform is not finite at this time".to_string());
    let invalid_provenance =
        induction_export_provenance(FieldMode::Induction, &scene, 0.1, Some(&invalid)).unwrap();
    assert!(
        invalid_provenance
            .warning_lines
            .iter()
            .any(|line| line.contains("invalid")),
        "{:?}",
        invalid_provenance.warning_lines,
    );
    assert_eq!(
        metadata_value(&invalid_provenance, "vfe.validation_error"),
        Some("induction waveform is not finite at this time"),
    );

    let bad_scene = InductionScene {
        radius: -1.0,
        ..InductionScene::default()
    };
    let bad_scene_provenance =
        induction_export_provenance(FieldMode::Induction, &bad_scene, 0.1, None).unwrap();
    assert!(
        bad_scene_provenance
            .warning_lines
            .iter()
            .any(|line| line
                == "Induction scene invalid: induction radius must be finite and positive"),
        "{:?}",
        bad_scene_provenance.warning_lines,
    );
    assert_eq!(
        metadata_value(&bad_scene_provenance, "vfe.validation_error"),
        Some("induction radius must be finite and positive"),
    );
}

#[test]
fn induction_opt_in_cannot_remove_warning_or_bypass_caps() {
    let scene = InductionScene {
        allow_inaccurate: true,
        ..InductionScene::default()
    };
    let sampling = sampling(status(3, 3, 3, 0), status(0, 0, 0, 2));
    let provenance =
        induction_export_provenance(FieldMode::Induction, &scene, 0.1, Some(&sampling)).unwrap();
    assert!(
        provenance
            .warning_lines
            .iter()
            .any(|line| line.contains("accuracy not met: 0 hidden, 3 drawn")),
        "{:?}",
        provenance.warning_lines,
    );
    assert!(
        provenance
            .warning_lines
            .iter()
            .any(|line| line.contains("Unavailable: 2")),
        "{:?}",
        provenance.warning_lines,
    );
    assert_eq!(sampling.electric.accuracy_not_met_drawn, 3);
    assert_eq!(sampling.magnetic.unavailable, 2);
}

#[test]
fn nonfinite_sampling_values_never_reach_export_output() {
    let scene = InductionScene::default();
    let mut sampling = sampling(status(1, 0, 0, 0), status(1, 0, 0, 0));
    sampling.current = f64::NAN;
    sampling.derivative = f64::INFINITY;
    let provenance =
        induction_export_provenance(FieldMode::Induction, &scene, 0.1, Some(&sampling)).unwrap();
    let serialized = format!("{:?}{:?}", provenance.warning_lines, provenance.metadata);
    assert!(!serialized.contains("NaN"), "{serialized}");
    assert!(!serialized.contains("inf"), "{serialized}");
    assert!(
        metadata_value(&provenance, "vfe.current").is_none()
            && metadata_value(&provenance, "vfe.derivative").is_none(),
        "sampled values are never exported, only the model and counts",
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
fn induction_png_bakes_warning_and_writes_case_metadata() {
    let scene = InductionScene::default();
    let sampling = sampling(status(1, 1, 1, 0), InductionFamilyStatus::default());
    let provenance =
        induction_export_provenance(FieldMode::Induction, &scene, 0.1, Some(&sampling)).unwrap();

    let (width, height) = (320u32, 120u32);
    let mut rgba = vec![20u8; width as usize * height as usize * 4];
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel[3] = 255;
    }
    vector_field_explorer::screenshot::apply_export_banner(&mut rgba, width, height, &provenance)
        .unwrap();
    let png = vector_field_explorer::export::encode_png_with_text(
        &rgba,
        width,
        height,
        provenance_metadata(&provenance),
    )
    .unwrap();

    let (pixels, decoded_width, decoded_height, text) = decode_png(&png);
    assert_eq!((decoded_width, decoded_height), (width, height));
    assert!(
        text.iter()
            .any(|chunk| { chunk.keyword == "vfe.induction_case" && chunk.text == "current_loop" }),
        "{text:?}",
    );
    assert!(
        text.iter().any(|chunk| chunk.keyword == "vfe.model"
            && chunk.text == "quasistatic-axisymmetric-faraday"),
        "{text:?}",
    );

    // Two warning lines put the bottom banner at y=84; line 0 (the model
    // line) is painted at y≈102..116. Prove near-white glyph pixels exist in
    // the decoded bottom region, not merely a dark backing strip.
    let bottom = &pixels[(decoded_height as usize - 20) * decoded_width as usize * 4..];
    let near_white = bottom
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[0] > 240 && pixel[1] > 240 && pixel[2] > 240)
        .count();
    assert!(
        near_white >= 20,
        "expected baked warning text pixels in the decoded PNG, found {near_white}",
    );
}

#[test]
fn induction_svg_shows_warning_text_and_metadata() {
    let scene = InductionScene::default();
    let sampling = sampling(status(1, 1, 1, 0), InductionFamilyStatus::default());
    let provenance =
        induction_export_provenance(FieldMode::Induction, &scene, 0.1, Some(&sampling)).unwrap();
    let render_scene = SceneGPU {
        mode_3d: false,
        ..Default::default()
    };
    let svg = vector_field_explorer::screenshot::scene_to_svg_with_provenance(
        &render_scene,
        Mat4::IDENTITY,
        320,
        240,
        [0.0, 0.0, 0.0, 1.0],
        Some(&provenance),
    );
    assert!(svg.contains("<text"), "{svg}");
    assert!(
        svg.contains("Induction - quasistatic axisymmetric softened model - a = 0.1"),
        "{svg}",
    );
    assert!(svg.contains("accuracy not met"), "{svg}");
    assert!(svg.contains("<metadata>"), "{svg}");
    assert!(
        svg.contains("<vfe.induction_case>current_loop</vfe.induction_case>"),
        "{svg}",
    );
    assert!(
        svg.contains("<vfe.model>quasistatic-axisymmetric-faraday</vfe.model>"),
        "{svg}",
    );
}

#[test]
fn source_provenance_remains_independent_of_induction_metadata() {
    let provenance =
        source_export_provenance(FieldMode::Sources, &SourceScene::default(), None).unwrap();
    assert!(provenance.source_mode);
    assert!(!provenance.induction_mode);
    assert!(metadata_value(&provenance, "vfe.induction_case").is_none());
}

#[test]
fn induction_display_controls_which_families_warn() {
    let scene = InductionScene {
        display: SourceDisplay::Magnetic,
        ..InductionScene::default()
    };
    let sampling = sampling(status(5, 5, 0, 0), status(1, 1, 1, 0));
    let provenance =
        induction_export_provenance(FieldMode::Induction, &scene, 0.1, Some(&sampling)).unwrap();
    assert!(
        provenance
            .warning_lines
            .iter()
            .any(|line| line == "Magnetic - accuracy not met: 0 hidden, 1 drawn"),
        "{:?}",
        provenance.warning_lines,
    );
    assert_eq!(metadata_value(&provenance, "vfe.display"), Some("Magnetic"));
    assert_eq!(
        metadata_value(&provenance, "vfe.accuracy_not_met_total"),
        Some("1")
    );
    assert_eq!(
        metadata_value(&provenance, "vfe.accuracy_not_met_drawn"),
        Some("1")
    );
}
