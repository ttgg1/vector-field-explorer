//! Issue #83 slice C1: portable palette document.
//!
//! Pins the strict, versioned JSON form for a custom colour ramp so a palette
//! can move between scenes and machines. Pure model only; no UI or renderer.

use vector_field_explorer::color_ramp::{ColorRamp, ColorStop, RampKind, Rgb};
use vector_field_explorer::colormap::Colormap;
use vector_field_explorer::palette_io::{
    PALETTE_FORMAT_VERSION, PaletteDocument, PaletteStop, export_ramp, import_ramp,
};

fn stop(position: f32, hex: &str) -> ColorStop {
    ColorStop {
        position,
        color: Rgb::from_hex(hex).expect("valid test fixture hex"),
    }
}

fn ramp(stops: Vec<ColorStop>, reversed: bool, kind: RampKind) -> ColorRamp {
    ColorRamp {
        stops,
        reversed,
        kind,
    }
}

fn sequential_ramp() -> ColorRamp {
    ramp(
        vec![
            stop(0.0, "#102030"),
            stop(0.5, "#405060"),
            stop(1.0, "#a0b0c0"),
        ],
        false,
        RampKind::Sequential,
    )
}

fn diverging_ramp() -> ColorRamp {
    ramp(
        vec![
            stop(0.0, "#102030"),
            stop(0.5, "#ffffff"),
            stop(1.0, "#304050"),
        ],
        true,
        RampKind::Diverging,
    )
}

fn cyclic_ramp() -> ColorRamp {
    ramp(
        vec![
            stop(0.0, "#112233"),
            stop(0.5, "#445566"),
            stop(1.0, "#112233"),
        ],
        false,
        RampKind::Cyclic,
    )
}

#[test]
fn valid_ramps_round_trip_exactly() {
    for original in [sequential_ramp(), diverging_ramp(), cyclic_ramp()] {
        let text = export_ramp(&original);
        let imported =
            import_ramp(&text).unwrap_or_else(|error| panic!("round trip failed: {error}"));
        assert_eq!(imported.stops, original.stops);
        assert_eq!(imported.reversed, original.reversed);
        assert_eq!(imported.kind, original.kind);
    }
}

#[test]
fn export_writes_lowercase_hex_and_the_current_version() {
    let document = PaletteDocument::from_ramp(&sequential_ramp());
    assert_eq!(document.version, PALETTE_FORMAT_VERSION);
    for palette_stop in &document.stops {
        assert_eq!(
            palette_stop.color,
            palette_stop.color.to_ascii_lowercase(),
            "colours must be canonical lowercase hex"
        );
    }

    let text = document.to_json();
    assert!(text.contains("\"version\":1"), "text: {text}");
    assert!(text.contains("\"#102030\""), "text: {text}");
    assert!(!text.contains("#10203O"), "text: {text}");
}

#[test]
fn import_accepts_uppercase_hex_and_an_optional_hash() {
    let text = r##"{"version":1,"kind":"Sequential","reversed":false,"stops":[
        {"position":0.0,"color":"#AABBCC"},
        {"position":1.0,"color":"00ff7F"}
    ]}"##;
    let imported = import_ramp(text).expect("uppercase and bare hex are accepted");
    assert_eq!(imported.stops[0].color, Rgb([0xaa, 0xbb, 0xcc]));
    assert_eq!(imported.stops[1].color, Rgb([0x00, 0xff, 0x7f]));
}

#[test]
fn unknown_fields_are_rejected() {
    let text = r##"{"version":1,"kind":"Sequential","reversed":false,
        "stops":[{"position":0.0,"color":"#000000"},
                 {"position":1.0,"color":"#ffffff"}],
        "extra":true}"##;
    let error =
        PaletteDocument::from_json(text).expect_err("unknown document fields must be rejected");
    assert!(error.contains("invalid palette JSON"), "error: {error}");

    let stop_text = r##"{"version":1,"kind":"Sequential","reversed":false,"stops":[
        {"position":0.0,"color":"#000000","width":2},
        {"position":1.0,"color":"#ffffff"}
    ]}"##;
    let stop_error =
        PaletteDocument::from_json(stop_text).expect_err("unknown stop fields must be rejected");
    assert!(
        stop_error.contains("invalid palette JSON"),
        "error: {stop_error}"
    );
}

#[test]
fn a_future_version_is_rejected_with_both_versions_named() {
    let text = r##"{"version":99,"kind":"Sequential","reversed":false,"stops":[
        {"position":0.0,"color":"#000000"},
        {"position":1.0,"color":"#ffffff"}
    ]}"##;
    let document = PaletteDocument::from_json(text).expect("syntax is valid");
    let error = document
        .to_ramp()
        .expect_err("a future version must be rejected");
    assert!(error.contains("99"), "error: {error}");
    assert!(error.contains("version 1"), "error: {error}");
}

#[test]
fn invalid_hex_is_rejected_with_the_stop_index() {
    let document = PaletteDocument {
        version: PALETTE_FORMAT_VERSION,
        kind: RampKind::Sequential,
        reversed: false,
        stops: vec![
            PaletteStop {
                position: 0.0,
                color: "#000000".to_owned(),
            },
            PaletteStop {
                position: 1.0,
                color: "#12345g".to_owned(),
            },
        ],
    };
    let error = document
        .to_ramp()
        .expect_err("invalid hex must be rejected");
    assert!(error.contains("stop 1"), "error: {error}");
    assert!(error.contains("#12345g"), "error: {error}");
}

#[test]
fn invalid_ramps_return_the_validation_reason() {
    let too_few = PaletteDocument {
        version: PALETTE_FORMAT_VERSION,
        kind: RampKind::Sequential,
        reversed: false,
        stops: vec![PaletteStop {
            position: 0.0,
            color: "#000000".to_owned(),
        }],
    };
    assert_eq!(
        too_few.to_ramp().expect_err("one stop is invalid"),
        "ramp needs at least 2 stops, got 1"
    );

    let duplicate_positions = PaletteDocument {
        version: PALETTE_FORMAT_VERSION,
        kind: RampKind::Sequential,
        reversed: false,
        stops: vec![
            PaletteStop {
                position: 0.0,
                color: "#000000".to_owned(),
            },
            PaletteStop {
                position: 0.5,
                color: "#808080".to_owned(),
            },
            PaletteStop {
                position: 0.5,
                color: "#808080".to_owned(),
            },
            PaletteStop {
                position: 1.0,
                color: "#ffffff".to_owned(),
            },
        ],
    };
    assert_eq!(
        duplicate_positions
            .to_ramp()
            .expect_err("duplicate positions are invalid"),
        "stop positions must be strictly increasing"
    );
}

#[test]
fn malformed_json_is_reported_without_panicking() {
    for text in ["", "{not json", "null", "[]", "{\"version\":1}"] {
        let error = match PaletteDocument::from_json(text) {
            Ok(_) => panic!("{text:?} should not parse as a palette document"),
            Err(error) => error,
        };
        assert!(
            error.starts_with("invalid palette JSON:"),
            "text {text:?} produced {error:?}"
        );
    }
}

#[test]
fn builtin_viridis_round_trips() {
    let original = ColorRamp::builtin(Colormap::Viridis);
    let imported = import_ramp(&export_ramp(&original)).expect("builtin viridis round trips");
    assert_eq!(imported, original);
    assert_eq!(imported.stops.len(), 9);
    assert_eq!(imported.stops[0].color, Rgb([68, 1, 84]));
    assert_eq!(imported.stops[8].color, Rgb([253, 231, 37]));
}

#[test]
fn document_json_order_is_stable() {
    let text = export_ramp(&sequential_ramp());
    assert!(
        text.starts_with("{\"version\":1,\"kind\":\"Sequential\",\"reversed\":false,\"stops\":["),
        "text: {text}"
    );
    let version = text.find("\"version\"").expect("version is present");
    let stops = text.find("\"stops\"").expect("stops is present");
    assert!(version < stops, "text: {text}");
}
