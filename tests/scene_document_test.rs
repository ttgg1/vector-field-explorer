//! Issue #90 slice B2a: versioned scene document.
//!
//! Pins the strict, versioned JSON form for a whole scene (view, parameters and
//! camera). Pure model only: no UI, storage, camera or app wiring.

use glam::Vec3;
use vector_field_explorer::camera::OrbitCamera;
use vector_field_explorer::color_ramp::{ColorRamp, ColorStop, RampKind, Rgb};
use vector_field_explorer::parameters::{Parameter, ParameterSet};
use vector_field_explorer::persistence::ViewRecord;
use vector_field_explorer::plane::PlaneAxis;
use vector_field_explorer::scene_document::{
    MAX_SCENE_NAME_BYTES, SCENE_FORMAT_VERSION, SceneCamera, SceneDocument, export_scene,
    import_scene,
};
use vector_field_explorer::seed_layout::{SeedAnchor, SeedLayout, SeedPoint};

fn stop(position: f32, hex: &str) -> ColorStop {
    ColorStop {
        position,
        color: Rgb::from_hex(hex).expect("valid test fixture hex"),
    }
}

fn sample_camera() -> SceneCamera {
    SceneCamera {
        position: [1.25, -2.5],
        scale: 2.0,
        yaw: 0.5,
        pitch: 0.25,
        distance: 3.2,
        target: [0.5, -1.0, 2.0],
        fov_y: 1.1,
        ortho_zoom: 4.0,
    }
}

fn sample_view() -> ViewRecord {
    ViewRecord {
        formula: "x + y".to_string(),
        color_ramp: Some(ColorRamp {
            stops: vec![stop(0.0, "#102030"), stop(1.0, "#a0b0c0")],
            reversed: true,
            kind: RampKind::Diverging,
        }),
        density: 11,
        domain_min: [-2.0, -3.0, -4.0],
        domain_max: [2.0, 3.0, 4.0],
        color_min: -1.5,
        color_max: 4.5,
        show_grid: false,
        ..ViewRecord::default()
    }
}

fn sample_parameters() -> ParameterSet {
    ParameterSet {
        version: 1,
        entries: vec![Parameter {
            name: "k".to_string(),
            value: 2.5,
            min: Some(0.0),
            max: Some(10.0),
            step: 0.5,
        }],
    }
}

fn sample_document() -> SceneDocument {
    SceneDocument::capture(
        "my scene",
        sample_view(),
        sample_parameters(),
        sample_camera(),
    )
}

fn document_named(name: impl Into<String>) -> SceneDocument {
    let mut document = sample_document();
    document.name = name.into();
    document
}

fn document_with_camera(camera: SceneCamera) -> SceneDocument {
    let mut document = sample_document();
    document.camera = camera;
    document
}

/// One camera rejection case: the field the error must name plus the mutation.
type CameraCase = (&'static str, fn(&mut SceneCamera));

#[test]
fn scene_round_trips_through_json() {
    let document = sample_document();
    assert_eq!(document.version, SCENE_FORMAT_VERSION);

    let json = document.to_json().expect("valid scene serializes");
    assert!(!json.is_empty());
    let restored = SceneDocument::from_json(&json).expect("valid scene parses");
    assert_eq!(restored, document);
    assert_eq!(restored.camera, sample_camera());

    let ramp = restored
        .view
        .color_ramp
        .as_ref()
        .expect("custom ramp survives the round trip");
    assert_eq!(ramp.kind, RampKind::Diverging);
    assert!(ramp.reversed);
    assert_eq!(ramp.stops.len(), 2);
    assert_eq!(restored.parameters, sample_parameters());
}

#[test]
fn future_scene_versions_are_rejected_and_named() {
    let mut document = sample_document();
    document.version = 2;

    let error = document.validate().expect_err("future version is rejected");
    assert!(
        error.contains('2'),
        "error must name the found version: {error}"
    );
    assert!(
        error.contains('1'),
        "error must name the expected version: {error}"
    );
}

#[test]
fn unknown_scene_fields_are_rejected() {
    let json = sample_document().to_json().expect("valid scene serializes");

    let extra_top_level = json.replacen('{', "{\"extra\":1,", 1);
    let error = SceneDocument::from_json(&extra_top_level).expect_err("extra field rejected");
    assert!(error.starts_with("invalid scene JSON: "), "got {error}");
    assert!(error.contains("extra"), "got {error}");

    let extra_camera = json.replacen("\"camera\":{", "\"camera\":{\"extra\":1,", 1);
    let error = SceneDocument::from_json(&extra_camera).expect_err("extra camera field rejected");
    assert!(error.starts_with("invalid scene JSON: "), "got {error}");
    assert!(error.contains("extra"), "got {error}");
}

#[test]
fn scene_name_rules_are_enforced() {
    for name in [
        String::new(),
        "   ".to_string(),
        "a".repeat(MAX_SCENE_NAME_BYTES + 1),
        "bad\tname".to_string(),
    ] {
        let document = document_named(name.clone());
        let error = document
            .validate()
            .expect_err(&format!("name {name:?} must be rejected"));
        assert!(error.contains("name"), "error must name the field: {error}");
    }

    let accepted = "a".repeat(MAX_SCENE_NAME_BYTES);
    let document = document_named(accepted);
    assert_eq!(document.validate(), Ok(()));
}

#[test]
fn non_finite_camera_fields_are_rejected() {
    let cases: [CameraCase; 4] = [
        ("position", |camera| camera.position[0] = f32::NAN),
        ("target", |camera| camera.target[2] = f32::INFINITY),
        ("distance", |camera| camera.distance = f32::NAN),
        ("fov_y", |camera| camera.fov_y = f32::INFINITY),
    ];
    for (field, mutate) in cases {
        let mut camera = sample_camera();
        mutate(&mut camera);
        let error = document_with_camera(camera)
            .validate()
            .expect_err(&format!("non-finite {field} must be rejected"));
        assert!(error.contains(field), "error must name {field}: {error}");
    }
}

#[test]
fn camera_bounds_are_enforced() {
    let cases: [CameraCase; 8] = [
        ("scale", |camera| camera.scale = 0.04),
        ("scale", |camera| camera.scale = 51.0),
        ("distance", |camera| camera.distance = 0.04),
        ("distance", |camera| camera.distance = 1.1e4),
        ("fov_y", |camera| camera.fov_y = 0.0),
        ("fov_y", |camera| camera.fov_y = 180.0_f32.to_radians()),
        ("ortho_zoom", |camera| {
            camera.ortho_zoom = OrbitCamera::MIN_ORTHO_ZOOM - 0.01
        }),
        ("ortho_zoom", |camera| {
            camera.ortho_zoom = OrbitCamera::MAX_ORTHO_ZOOM + 1.0
        }),
    ];
    for (field, mutate) in cases {
        let mut camera = sample_camera();
        mutate(&mut camera);
        let error = document_with_camera(camera)
            .validate()
            .expect_err(&format!("out-of-range {field} must be rejected"));
        assert!(error.contains(field), "error must name {field}: {error}");
    }
}

#[test]
fn invalid_parameters_are_rejected_with_the_parameter_error() {
    let mut document = sample_document();
    document.parameters.entries = vec![Parameter {
        name: "pi".to_string(),
        value: 1.0,
        min: None,
        max: None,
        step: 0.1,
    }];

    let error = document.validate().expect_err("reserved name rejected");
    assert!(
        error.contains("pi"),
        "error must name the parameter: {error}"
    );
    assert!(
        error.contains("reserved"),
        "error must keep the ParameterSet message: {error}"
    );

    let json = serde_json::to_string(&document).expect("struct serializes");
    let error = SceneDocument::from_json(&json).expect_err("import validates parameters");
    assert!(error.starts_with("invalid scene: "), "got {error}");
    assert!(
        error.contains("pi"),
        "error must name the parameter: {error}"
    );
}

#[test]
fn malformed_json_is_rejected_with_a_prefix() {
    for text in ["{", "null", "{\"hello\":\"world\"}"] {
        let error = SceneDocument::from_json(text).expect_err("malformed JSON rejected");
        assert!(
            error.starts_with("invalid scene JSON: "),
            "input {text:?} produced {error}"
        );
    }

    let mut future = sample_document();
    future.version = 2;
    let json = serde_json::to_string(&future).expect("struct serializes");
    let error = SceneDocument::from_json(&json).expect_err("invalid scene rejected");
    assert!(error.starts_with("invalid scene: "), "got {error}");
}

#[test]
fn export_and_import_compose_the_versioned_helpers() {
    let document = sample_document();
    let text = export_scene(&document).expect("valid scene exports");
    let imported = import_scene(&text).expect("exported scene imports");
    assert_eq!(imported, document);

    let invalid = document_named("   ");
    let error = export_scene(&invalid).expect_err("invalid scene does not export");
    assert!(error.contains("name"), "got {error}");

    let mut future = sample_document();
    future.version = 2;
    let text = serde_json::to_string(&future).expect("struct serializes");
    let error = import_scene(&text).expect_err("future version does not import");
    assert!(error.contains('2') && error.contains('1'), "got {error}");
}

#[test]
fn json_field_order_is_pinned() {
    let json = sample_document().to_json().expect("valid scene serializes");
    let keys = [
        "\"version\":",
        "\"name\":",
        "\"view\":",
        "\"parameters\":",
        "\"camera\":",
    ];
    let mut previous = 0;
    for key in keys {
        let position = json.find(key).unwrap_or_else(|| panic!("missing {key}"));
        assert!(position >= previous, "{key} is out of order in {json}");
        previous = position;
    }
}

#[test]
fn validation_never_mutates_the_document() {
    let document = sample_document();
    let before = document.clone();
    assert_eq!(document.validate(), Ok(()));
    assert_eq!(document, before);

    let invalid = document_named("bad\nname");
    let before = invalid.clone();
    assert!(invalid.validate().is_err());
    assert_eq!(invalid, before);
}

#[test]
fn saved_scene_keeps_a_non_empty_seed_layout() {
    let layout = SeedLayout {
        seeds: vec![
            SeedPoint {
                position: Vec3::new(0.5, -0.25, 1.0),
                anchor: Some(SeedAnchor {
                    axis: PlaneAxis::X,
                    index: 1,
                    u: 0.5,
                    v: -0.25,
                }),
            },
            SeedPoint {
                position: Vec3::new(-1.0, 0.0, 0.5),
                anchor: None,
            },
        ],
    };
    let mut document = sample_document();
    document.view.seed_layout = layout.clone();

    let json = export_scene(&document).expect("a valid scene exports");
    let restored = import_scene(&json).expect("a valid scene imports");
    assert_eq!(
        restored.view.seed_layout, layout,
        "saved scenes keep the explicit seeds"
    );
}
