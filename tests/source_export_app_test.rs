//! App-level source export wiring: provenance is captured with the request and
//! cloned by the harness without consuming it.

use vector_field_explorer::VfApp;
use vector_field_explorer::source_scene::{FieldMode, SourceKindTag, SourceScene};

#[test]
fn png_request_captures_source_provenance_at_execution_time() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Sources);
    let mut scene = SourceScene::default();
    scene
        .add(SourceKindTag::PointCharge.default_source())
        .unwrap();
    app.state.set_source_scene(scene);
    app.test_harness().advance_simulation();
    app.test_harness().request_png_export();
    let provenance = app.test_harness().export_png_provenance().unwrap();
    assert!(
        provenance
            .warning_lines
            .iter()
            .any(|line| line.contains("softened teaching model"))
    );
    assert!(app.test_harness().export_svg_provenance().is_none());
}

#[test]
fn no_sampling_warns_instead_of_exporting_stale_vectors() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Sources);
    app.test_harness().request_png_export();
    let provenance = app.test_harness().export_png_provenance().unwrap();
    assert!(
        provenance
            .warning_lines
            .iter()
            .any(|line| line == "Source sampling unavailable")
    );
    assert!(app.test_harness().source_glyph_line_count() == 0);
}

#[test]
fn source_svg_request_captures_the_same_provenance() {
    let mut app = VfApp::new();
    app.state.set_mode(false);
    app.state.set_field_mode(FieldMode::Sources);
    app.test_harness().request_svg_export();
    let provenance = app.test_harness().export_svg_provenance().unwrap();
    assert!(provenance.source_mode);
    assert!(app.test_harness().export_png_provenance().is_none());
}
