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
    assert!(!provenance.induction_mode);
    assert!(app.test_harness().export_png_provenance().is_none());
}

#[test]
fn induction_png_request_captures_induction_provenance_at_request_time() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Induction);
    app.test_harness().request_png_export();
    let provenance = app.test_harness().export_png_provenance().unwrap();
    assert!(provenance.induction_mode);
    assert!(!provenance.source_mode);
    assert_eq!(
        provenance.warning_lines[0],
        "Induction - quasistatic axisymmetric softened model - a = 0.1",
    );
    assert!(
        provenance
            .metadata
            .iter()
            .any(|(key, value)| key == "vfe.induction_case" && value == "current_loop"),
    );

    // The warning is captured now: editing the scene afterwards cannot make
    // the queued export forget it was an induction frame.
    let mut scene = app.state.induction.clone();
    scene.case = vector_field_explorer::induction::InductionCase::Solenoid;
    scene.length = 2.0;
    scene.turns = 8;
    assert!(app.state.set_induction_scene(scene));
    let pending = app.test_harness().export_png_provenance().unwrap();
    assert_eq!(pending, provenance);
}

#[test]
fn induction_svg_request_captures_induction_provenance() {
    let mut app = VfApp::new();
    app.state.set_mode(false);
    app.state.set_field_mode(FieldMode::Induction);
    app.test_harness().request_svg_export();
    let provenance = app.test_harness().export_svg_provenance().unwrap();
    assert!(provenance.induction_mode);
    assert_eq!(
        provenance.warning_lines[0],
        "Induction - quasistatic axisymmetric softened model - a = 0.1",
    );
    assert!(app.test_harness().export_png_provenance().is_none());
}

#[test]
fn formula_png_request_captures_no_provenance() {
    let mut app = VfApp::new();
    app.test_harness().request_png_export();
    assert!(app.test_harness().export_png_provenance().is_none());
}

#[test]
fn induction_export_uses_the_current_shared_softening() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Induction);
    app.test_harness().advance_simulation();
    assert!(
        app.test_harness().induction_sampling().is_some(),
        "the first pass must be cached at a = 0.1",
    );

    let mut sources = app.state.sources.clone();
    sources.set_softening(0.2).unwrap();
    app.state.set_source_scene(sources);

    app.test_harness().request_png_export();
    let provenance = app.test_harness().export_png_provenance().unwrap();
    assert_eq!(
        provenance.warning_lines[0],
        "Induction - quasistatic axisymmetric softened model - a = 0.2",
    );
    assert!(
        provenance
            .warning_lines
            .iter()
            .any(|line| line == "Induction sampling unavailable"),
        "a stale pass built at a = 0.1 must not supply counts for a = 0.2",
    );
    assert!(
        provenance
            .metadata
            .iter()
            .any(|(key, value)| key == "vfe.softening" && value == "0.2"),
    );
}
