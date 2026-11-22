//! Task 5 Maxwell export provenance: exact request-time lines/metadata, decoded
//! PNG banner/tEXt, SVG visible text/metadata, mode isolation, unavailable and
//! invalid warnings, and no sampling at export time.

use glam::{Mat4, UVec3};

use vector_field_explorer::VfApp;
use vector_field_explorer::domain::Domain;
use vector_field_explorer::export_provenance::{
    ExportProvenance, MAXWELL_DISPLAY_UNITS_LINE, MAXWELL_MODEL_LINE, MAXWELL_NO_SAMPLING_LINE,
    MAXWELL_WAVE_MODEL_LINE, MAXWELL_WAVE_NO_SAMPLING_LINE, MAXWELL_WAVE_UNITS_LINE,
    maxwell_export_provenance, maxwell_wave_export_provenance, provenance_lines,
    provenance_metadata,
};
use vector_field_explorer::maxwell::{
    ConductorOutline, MAXWELL_ACCURACY_NOT_MET_WARNING, MAXWELL_BEM_NOTE,
    MAXWELL_BEM_PENDING_WARNING, MAXWELL_SWITCH_2D_WARNING, MaxwellCase, MaxwellScene, MaxwellStop,
    maxwell_assumptions, sample_maxwell, sample_maxwell_with_bem, solve_bem,
};
use vector_field_explorer::maxwell_wave::WaveSpec;
use vector_field_explorer::plane::{PlaneAxis, PlaneFrame, PlaneView};
use vector_field_explorer::renderer::SceneGPU;
use vector_field_explorer::source_scene::FieldMode;

fn conductor() -> MaxwellScene {
    MaxwellScene {
        case: MaxwellCase::Conductor2D,
        outline: ConductorOutline::Circle { radius: 1.0 },
        elements: 32,
        allow_inaccurate: true,
        ..MaxwellScene::default()
    }
}

fn slice(domain: &Domain) -> PlaneFrame {
    PlaneFrame::new(
        domain,
        PlaneView {
            enabled: true,
            axis: PlaneAxis::Z,
            index: 10,
        },
        UVec3::new(21, 21, 1),
    )
}

fn metadata_value<'a>(provenance: &'a ExportProvenance, key: &str) -> Option<&'a str> {
    provenance
        .metadata
        .iter()
        .find(|(candidate, _)| candidate == key)
        .map(|(_, value)| value.as_str())
}

#[test]
fn maxwell_warning_lines_are_exact_for_a_missing_sampling_pass() {
    let scene = MaxwellScene::default();
    let provenance = maxwell_export_provenance(FieldMode::Maxwell, &scene, None).unwrap();

    assert_eq!(
        provenance_lines(&provenance),
        &[
            MAXWELL_MODEL_LINE.to_string(),
            MAXWELL_NO_SAMPLING_LINE.to_string(),
            MAXWELL_DISPLAY_UNITS_LINE.to_string(),
        ]
    );
    assert!(provenance.maxwell_mode);
    assert!(!provenance.source_mode);
    assert!(!provenance.induction_mode);
    assert!(!provenance.ode_mode);
    assert!(!provenance.material_mode);
}

#[test]
fn conductor_sampling_carries_the_bem_line_units_and_convergence() {
    let scene = conductor();
    let domain = Domain::default();
    let frame = slice(&domain);
    let solution = solve_bem(&scene).expect("circle BEM");
    let sampling = sample_maxwell_with_bem(
        &scene,
        &domain,
        UVec3::new(21, 21, 1),
        Some(&frame),
        Some(&solution),
    );
    let convergence = sampling.convergence.expect("BEM convergence evidence");
    let provenance =
        maxwell_export_provenance(FieldMode::Maxwell, &scene, Some(&sampling)).unwrap();
    let lines = provenance_lines(&provenance);

    assert_eq!(lines[0], MAXWELL_MODEL_LINE);
    assert_eq!(lines[1], MAXWELL_BEM_NOTE);
    assert_eq!(
        lines.last().map(String::as_str),
        Some(MAXWELL_DISPLAY_UNITS_LINE)
    );
    for (key, value) in [
        ("vfe.model", "maxwell-bounded-teaching"),
        ("vfe.maxwell_case", "conductor_2d"),
        ("vfe.maxwell_family", "electric"),
        ("vfe.maxwell_units", "display-only"),
        ("vfe.maxwell_assumptions", maxwell_assumptions(&scene)),
        (
            "vfe.maxwell_bem_coarse",
            &convergence.coarse_elements.to_string(),
        ),
        (
            "vfe.maxwell_bem_fine",
            &convergence.fine_elements.to_string(),
        ),
        (
            "vfe.maxwell_bem_relative_difference",
            &convergence.relative_difference.to_string(),
        ),
    ] {
        assert_eq!(metadata_value(&provenance, key), Some(value), "{key}");
    }
    assert_eq!(
        metadata_value(&provenance, "vfe.maxwell_accuracy"),
        Some(match sampling.accuracy {
            MaxwellStop::TargetMet => "target_met",
            MaxwellStop::AccuracyNotMet => "accuracy_not_met",
            MaxwellStop::Unavailable => "unavailable",
        })
    );
    let omitted = sampling.omitted.to_string();
    let omitted_large = sampling.omitted_large.to_string();
    assert_eq!(
        metadata_value(&provenance, "vfe.maxwell_omitted"),
        Some(omitted.as_str())
    );
    assert_eq!(
        metadata_value(&provenance, "vfe.maxwell_omitted_large"),
        Some(omitted_large.as_str())
    );
    assert!(metadata_value(&provenance, "vfe.validation_error").is_none());
}

#[test]
fn accuracy_not_met_is_visible_with_or_without_the_display_opt_in() {
    let mut scene = conductor();
    scene.outline = ConductorOutline::Square { half_extent: 1.0 };
    scene.elements = 32;
    scene.allow_inaccurate = false;
    let domain = Domain::new(glam::Vec3::splat(-2.0), glam::Vec3::splat(2.0));
    let frame = slice(&domain);
    let solution = solve_bem(&scene).expect("square BEM");
    assert!(
        solution.convergence().relative_difference > 1e-3,
        "the square coarse/fine run is expected to miss the target",
    );

    let hidden = sample_maxwell_with_bem(
        &scene,
        &domain,
        UVec3::new(21, 21, 1),
        Some(&frame),
        Some(&solution),
    );
    assert_eq!(hidden.accuracy, MaxwellStop::AccuracyNotMet);
    assert!(
        hidden.arrows.is_empty(),
        "default-off omits the inaccurate arrows"
    );
    let hidden_provenance =
        maxwell_export_provenance(FieldMode::Maxwell, &scene, Some(&hidden)).unwrap();
    assert!(
        hidden_provenance
            .warning_lines
            .iter()
            .any(|line| line == MAXWELL_ACCURACY_NOT_MET_WARNING),
        "{:?}",
        hidden_provenance.warning_lines,
    );

    scene.allow_inaccurate = true;
    let shown = sample_maxwell_with_bem(
        &scene,
        &domain,
        UVec3::new(21, 21, 1),
        Some(&frame),
        Some(&solution),
    );
    assert!(!shown.arrows.is_empty());
    let shown_provenance =
        maxwell_export_provenance(FieldMode::Maxwell, &scene, Some(&shown)).unwrap();
    assert!(
        shown_provenance
            .warning_lines
            .iter()
            .any(|line| line == MAXWELL_ACCURACY_NOT_MET_WARNING),
        "{:?}",
        shown_provenance.warning_lines,
    );
    assert!(shown.arrows.iter().all(|arrow| arrow.mag.is_finite()));
}

#[test]
fn unavailable_and_invalid_conductor_exports_still_warn_without_arrows() {
    let scene = conductor();
    let domain = Domain::default();
    let sampling = sample_maxwell(&scene, &domain, UVec3::new(21, 21, 1), None);
    assert_eq!(sampling.accuracy, MaxwellStop::Unavailable);
    assert!(sampling.arrows.is_empty());
    let provenance =
        maxwell_export_provenance(FieldMode::Maxwell, &scene, Some(&sampling)).unwrap();
    assert!(
        provenance
            .warning_lines
            .iter()
            .any(|line| line == MAXWELL_SWITCH_2D_WARNING),
        "{:?}",
        provenance.warning_lines,
    );
    assert_eq!(
        metadata_value(&provenance, "vfe.maxwell_accuracy"),
        Some("unavailable"),
    );

    let mut invalid = conductor();
    invalid.elements = 31;
    let provenance = maxwell_export_provenance(FieldMode::Maxwell, &invalid, None).unwrap();
    assert!(
        provenance
            .warning_lines
            .iter()
            .any(|line| line.starts_with("Maxwell scene invalid: ")),
        "{:?}",
        provenance.warning_lines,
    );
    assert!(
        metadata_value(&provenance, "vfe.validation_error")
            .is_some_and(|error| error.contains("element")),
    );

    // A conductor sampling pass with no cached solve also names the missing
    // boundary solve rather than silently drawing nothing.
    let pending = sample_maxwell(
        &scene,
        &domain,
        UVec3::new(21, 21, 1),
        Some(&slice(&domain)),
    );
    assert_eq!(
        pending.warning.as_deref(),
        Some(MAXWELL_BEM_PENDING_WARNING)
    );
    let provenance = maxwell_export_provenance(FieldMode::Maxwell, &scene, Some(&pending)).unwrap();
    assert!(
        provenance
            .warning_lines
            .iter()
            .any(|line| line == "Conductor2D - infinite-cylinder BEM approximation")
    );
    assert!(
        provenance
            .warning_lines
            .iter()
            .any(|line| line == MAXWELL_BEM_PENDING_WARNING)
    );
}

#[test]
fn non_maxwell_modes_have_no_maxwell_provenance() {
    let scene = MaxwellScene::default();
    for mode in [
        FieldMode::Formula,
        FieldMode::Sources,
        FieldMode::Induction,
        FieldMode::Materials,
        FieldMode::Ode,
    ] {
        assert!(
            maxwell_export_provenance(mode, &scene, None).is_none(),
            "{mode:?}",
        );
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
fn decoded_png_banner_and_text_chunks_carry_maxwell_context() {
    let scene = conductor();
    let domain = Domain::default();
    let frame = slice(&domain);
    let solution = solve_bem(&scene).expect("circle BEM");
    let sampling = sample_maxwell_with_bem(
        &scene,
        &domain,
        UVec3::new(21, 21, 1),
        Some(&frame),
        Some(&solution),
    );
    let provenance =
        maxwell_export_provenance(FieldMode::Maxwell, &scene, Some(&sampling)).unwrap();

    let (width, height) = (420u32, 260u32);
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
        "vfe.maxwell_case",
        "vfe.maxwell_family",
        "vfe.maxwell_units",
        "vfe.maxwell_assumptions",
        "vfe.maxwell_bem_coarse",
        "vfe.maxwell_bem_fine",
        "vfe.maxwell_bem_relative_difference",
    ] {
        assert!(
            text.iter().any(|chunk| chunk.keyword == key),
            "missing PNG chunk {key}: {text:?}",
        );
    }
    assert!(
        text.iter().any(|chunk| {
            chunk.keyword == "vfe.model" && chunk.text == "maxwell-bounded-teaching"
        })
    );
    let units = text
        .iter()
        .find(|chunk| chunk.keyword == "vfe.maxwell_units")
        .unwrap();
    assert_eq!(units.text, "display-only");

    // The baked strip and its near-white glyphs occupy the bottom of the frame.
    let banner = &pixels[((height - 12) * width * 4) as usize..];
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
fn svg_visible_text_and_metadata_carry_maxwell_context_and_stay_2d_only() {
    let scene = conductor();
    let provenance = maxwell_export_provenance(FieldMode::Maxwell, &scene, None).unwrap();
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
    assert!(svg.contains(MAXWELL_MODEL_LINE));
    assert!(svg.contains(MAXWELL_BEM_NOTE));
    assert!(svg.contains(MAXWELL_DISPLAY_UNITS_LINE));
    assert!(svg.contains("<metadata>"));
    assert!(svg.contains("<vfe.model>maxwell-bounded-teaching</vfe.model>"));
    assert!(svg.contains("<vfe.maxwell_case>conductor_2d</vfe.maxwell_case>"));
    assert!(svg.contains("<vfe.maxwell_units>display-only</vfe.maxwell_units>"));

    assert!(!vector_field_explorer::screenshot::svg_export_enabled(true));
    assert!(vector_field_explorer::screenshot::svg_export_enabled(false));
}

#[test]
fn app_requests_maxwell_provenance_without_sampling_again() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Maxwell);
    app.state.set_mode(false);
    assert!(app.state.set_maxwell_scene(conductor()));
    app.state.refresh_maxwell_sampling();
    assert!(app.state.maxwell_sampling.is_some());
    let solves = app.test_harness().maxwell_bem_solves();

    app.test_harness().request_png_export();
    let provenance = app
        .test_harness()
        .export_png_provenance()
        .expect("Maxwell export provenance");
    assert!(provenance.maxwell_mode);
    assert_eq!(provenance.warning_lines[0], MAXWELL_MODEL_LINE);
    assert_eq!(
        app.test_harness().maxwell_bem_solves(),
        solves,
        "an export request is pure and never samples or solves again",
    );
    assert!(
        provenance
            .metadata
            .iter()
            .any(|(key, _)| key == "vfe.maxwell_bem_relative_difference"),
    );

    let mut formula = VfApp::new();
    formula.test_harness().request_png_export();
    assert!(formula.test_harness().export_png_provenance().is_none());
}

#[test]
fn maxwell_sampling_arrows_are_finite_for_every_export_path() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Maxwell);
    app.state.set_mode(false);
    assert!(app.state.set_maxwell_scene(conductor()));
    app.state.refresh_maxwell_sampling();
    let sampling = app.state.maxwell_sampling().expect("sampling");
    assert!(sampling.arrows.iter().all(|arrow| {
        arrow.position.is_finite() && arrow.dir.is_finite() && arrow.mag.is_finite()
    }));
    assert!(sampling.arrows.iter().all(|arrow| arrow.mag > 0.0));
}

fn wave_spec() -> WaveSpec {
    WaveSpec {
        amplitude: 1.5,
        wavelength: 2.0,
        direction: glam::Vec3::X,
        polarization: glam::Vec3::Y,
        phase: 0.25,
    }
}

#[test]
fn wave_provenance_names_the_wave_display_and_reports_sampling_status() {
    let provenance =
        maxwell_wave_export_provenance(FieldMode::Maxwell, &wave_spec(), true).unwrap();
    assert!(provenance.maxwell_mode);
    assert_eq!(provenance_lines(&provenance)[0], MAXWELL_WAVE_MODEL_LINE);
    assert_eq!(
        provenance_lines(&provenance).last().map(String::as_str),
        Some(MAXWELL_WAVE_UNITS_LINE)
    );
    assert!(
        !provenance_lines(&provenance)
            .iter()
            .any(|line| line == MAXWELL_WAVE_NO_SAMPLING_LINE),
        "a sampled wave must not claim sampling is unavailable: {:?}",
        provenance_lines(&provenance)
    );
    for (key, value) in [
        ("vfe.model", "maxwell-plane-wave-teaching"),
        ("vfe.maxwell_display", "plane_wave"),
        ("vfe.maxwell_family", "electric+magnetic"),
        ("vfe.maxwell_units", "normalized-teaching"),
        ("vfe.maxwell_sampling", "sampled"),
    ] {
        assert_eq!(metadata_value(&provenance, key), Some(value), "{key}");
    }
    let assumptions = metadata_value(&provenance, "vfe.maxwell_assumptions").unwrap();
    assert!(assumptions.contains("source-free"), "{assumptions}");
    assert!(assumptions.contains("plane wave"), "{assumptions}");
    assert!(
        metadata_value(&provenance, "vfe.maxwell_case").is_none(),
        "the wave display must not inherit the hidden case's label"
    );

    let unsampled =
        maxwell_wave_export_provenance(FieldMode::Maxwell, &wave_spec(), false).unwrap();
    assert!(
        provenance_lines(&unsampled)
            .iter()
            .any(|line| line == MAXWELL_WAVE_NO_SAMPLING_LINE),
        "an unsampled wave names its missing sampling: {:?}",
        provenance_lines(&unsampled)
    );
    assert_eq!(
        metadata_value(&unsampled, "vfe.maxwell_sampling"),
        Some("unavailable")
    );

    for mode in [
        FieldMode::Formula,
        FieldMode::Sources,
        FieldMode::Induction,
        FieldMode::Materials,
        FieldMode::Ode,
    ] {
        assert!(
            maxwell_wave_export_provenance(mode, &wave_spec(), true).is_none(),
            "{mode:?}"
        );
    }
}

#[test]
fn wave_export_distinguishes_rejection_from_a_sampled_zero_field() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Maxwell);
    let mut wave = wave_spec();
    wave.amplitude = 0.0;
    assert!(app.state.set_maxwell_wave(Some(wave)));
    for (time, expected) in [(0.0, "sampled"), (f32::NAN, "unavailable")] {
        app.state.time = time;
        app.state.refresh_maxwell_sampling();
        assert!(app.state.maxwell_wave_sampling.as_ref().unwrap().is_empty());
        app.test_harness().request_png_export();
        let provenance = app.test_harness().export_png_provenance().unwrap();
        assert_eq!(
            metadata_value(&provenance, "vfe.maxwell_sampling"),
            Some(expected)
        );
    }
}

#[test]
fn app_requests_wave_provenance_while_the_wave_is_the_displayed_field() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Maxwell);
    app.state.set_mode(false);
    // A magnetic canonical case whose family differs from the wave display.
    assert!(app.state.set_maxwell_scene(MaxwellScene {
        case: MaxwellCase::Solenoid,
        ..MaxwellScene::default()
    }));
    assert!(app.state.set_maxwell_wave(Some(wave_spec())));
    app.state.refresh_maxwell_sampling();

    app.test_harness().request_png_export();
    let provenance = app
        .test_harness()
        .export_png_provenance()
        .expect("Maxwell export provenance");
    assert_eq!(provenance_lines(&provenance)[0], MAXWELL_WAVE_MODEL_LINE);
    assert_eq!(
        metadata_value(&provenance, "vfe.maxwell_display"),
        Some("plane_wave")
    );
    assert_eq!(
        metadata_value(&provenance, "vfe.maxwell_family"),
        Some("electric+magnetic"),
        "the wave display supplies both families, not the hidden case's"
    );
    assert_eq!(
        metadata_value(&provenance, "vfe.maxwell_sampling"),
        Some("sampled")
    );
    assert!(
        !provenance
            .warning_lines
            .iter()
            .any(|line| line == MAXWELL_NO_SAMPLING_LINE || line == MAXWELL_WAVE_NO_SAMPLING_LINE),
        "the sampled wave does not inherit the case's unavailable line: {:?}",
        provenance.warning_lines
    );

    // Disabling the wave returns the ordinary case provenance exactly.
    assert!(app.state.set_maxwell_wave(None));
    app.state.refresh_maxwell_sampling();
    app.test_harness().request_png_export();
    let case_provenance = app
        .test_harness()
        .export_png_provenance()
        .expect("case provenance");
    assert_eq!(case_provenance.warning_lines[0], MAXWELL_MODEL_LINE);
    assert_eq!(
        metadata_value(&case_provenance, "vfe.maxwell_case"),
        Some("solenoid")
    );
    assert_eq!(
        metadata_value(&case_provenance, "vfe.maxwell_family"),
        Some("magnetic")
    );
}
