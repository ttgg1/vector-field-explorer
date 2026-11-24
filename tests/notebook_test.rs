//! Issue #222: the bounded experiment notebook backed by immutable Gauss-shell
//! measurement snapshots and captured `SceneDocument`s. The numeric checks use
//! the shipped `compute_reading` seam; the panel checks drive the real Maxwell
//! panel at wide and compact sizes.

use eframe::egui;

use crate::support;
use crate::support::storage::MemoryStorage;

use vector_field_explorer::VfApp;
use vector_field_explorer::maxwell::{MaxwellCase, MaxwellScene};
use vector_field_explorer::notebook::{
    MAX_NOTEBOOK_ENTRIES, MeasurementSnapshot, NOTEBOOK_VERSION, Notebook, NotebookEntry,
    SHELL_METHOD, SHELL_UNITS, compare,
};
use vector_field_explorer::parameters::ParameterSet;
use vector_field_explorer::persistence::{self, AppSettings, ViewRecord};
use vector_field_explorer::scene_document::{SceneCamera, SceneDocument};
use vector_field_explorer::source_scene::FieldMode;
use vector_field_explorer::sources::FieldFamily;
use vector_field_explorer::ui::gauss_shell::{GaussShellReadingKind, compute_reading};
use vector_field_explorer::ui::maxwell_panel::{self, MaxwellPanelAction, MaxwellPanelState};
use vector_field_explorer::ui::notebook::{
    NOTEBOOK_CAPTURE_LABEL, NOTEBOOK_EMPTY, NOTEBOOK_TITLE, NotebookAction,
};
use vector_field_explorer::viewmodel::State;

struct WidgetNode {
    label: String,
    rect: egui::Rect,
}

fn nodes(output: &egui::FullOutput) -> Vec<WidgetNode> {
    let Some(update) = output.platform_output.accesskit_update.as_ref() else {
        return Vec::new();
    };
    update
        .nodes
        .iter()
        .filter_map(|(_, node)| {
            let bounds = node.bounds()?;
            let label = node
                .label()
                .or_else(|| node.value())
                .unwrap_or_default()
                .to_owned();
            Some(WidgetNode {
                label,
                rect: egui::Rect::from_min_max(
                    egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                    egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                ),
            })
        })
        .collect()
}

fn find<'a>(nodes: &'a [WidgetNode], label: &str) -> Option<&'a WidgetNode> {
    nodes.iter().find(|node| node.label == label)
}

fn painted_texts(output: &egui::FullOutput) -> Vec<String> {
    fn collect(shape: &egui::Shape, out: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(text) => out.push(text.galley.text().to_owned()),
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect(shape, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut out);
    }
    out
}

fn pointer(pos: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(pos),
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        },
    ]
}

fn context() -> egui::Context {
    let ctx = support::context();
    ctx.enable_accesskit();
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    ctx
}

fn hollow(radius: f64) -> MaxwellScene {
    MaxwellScene {
        case: MaxwellCase::HollowSphere,
        radius,
        ..MaxwellScene::default()
    }
}

fn scene_doc(name: &str) -> SceneDocument {
    SceneDocument::capture(
        name,
        ViewRecord::default(),
        ParameterSet::default(),
        SceneCamera::default(),
    )
}

fn shell_snapshot(scene: &MaxwellScene, probe: f64) -> MeasurementSnapshot {
    let reading = compute_reading(scene, probe, FieldFamily::Electric, 0.0);
    MeasurementSnapshot::shell(
        scene,
        probe,
        FieldFamily::Electric,
        0.0,
        ParameterSet::default(),
        &reading,
    )
}

fn entry(id: &str, scene: &MaxwellScene, probe: f64) -> NotebookEntry {
    NotebookEntry {
        id: id.to_owned(),
        prediction: format!("prediction {id}"),
        note: format!("note {id}"),
        snapshot: shell_snapshot(scene, probe),
        scene: scene_doc(&format!("Notebook {id}")),
    }
}

#[test]
fn two_exterior_radii_keep_equal_flux_within_combined_uncertainty() {
    let scene = hollow(1.0);
    let reading = compute_reading(&scene, 1.5, FieldFamily::Electric, 0.0);
    assert!(matches!(
        reading.kind,
        GaussShellReadingKind::Exterior { .. }
    ));
    let first = entry("a", &scene, 1.5);
    let second = entry("b", &scene, 3.0);
    let (Some((value, uncertainty)), Some((other, other_uncertainty))) = (
        first.snapshot.value.estimate(),
        second.snapshot.value.estimate(),
    ) else {
        panic!("both exterior readings capture estimates");
    };
    assert!(uncertainty >= 0.0 && other_uncertainty >= 0.0);

    let comparison = compare(&first, &second);
    assert!(
        comparison
            .changes
            .iter()
            .any(|change| change.label == "Probe radius r"),
        "the measuring-radius change is listed: {:?}",
        comparison.changes
    );
    let flux = comparison.flux.expect("two estimates compare");
    assert_eq!(flux.first, value);
    assert_eq!(flux.second, other);
    assert_eq!(flux.combined_uncertainty, uncertainty + other_uncertainty);
    assert!(
        flux.equal_within_uncertainty,
        "difference {} <= combined {}",
        flux.difference, flux.combined_uncertainty
    );
}

#[test]
fn captured_entries_are_immutable_under_live_edits() {
    let mut scene = hollow(1.0);
    let mut parameters = ParameterSet::default();
    let mut time = 0.0;
    let mut notebook = Notebook::new();
    let id = notebook
        .pin(
            "flux stays the same",
            "the area grows while the field falls",
            shell_snapshot(&scene, 1.5),
            scene_doc("Captured A"),
        )
        .expect("entry fits");
    let captured = serde_json::to_string(notebook.get(&id).expect("entry")).expect("json");

    scene.radius = 2.0;
    parameters
        .entries
        .push(vector_field_explorer::parameters::Parameter {
            name: "q".to_owned(),
            value: 3.0,
            ..Default::default()
        });
    time += 4.5;
    // A live read does not touch the stored snapshot either.
    let _live = compute_reading(&scene, 3.0, FieldFamily::Electric, time);

    let stored = notebook.get(&id).expect("entry");
    assert_eq!(
        serde_json::to_string(stored).expect("json"),
        captured,
        "a live edit and a live read leave the captured entry byte-identical"
    );
    assert_eq!(stored.snapshot.geometry.probe_radius(), 1.5);
    assert_eq!(stored.scene.name, "Captured A");
}

#[test]
fn excluded_sample_is_unavailable_never_zero() {
    let scene = hollow(1.0);
    let interface = compute_reading(&scene, 1.0, FieldFamily::Electric, 0.0);
    assert!(matches!(
        interface.kind,
        GaussShellReadingKind::Excluded { .. }
    ));
    let snapshot = MeasurementSnapshot::shell(
        &scene,
        1.0,
        FieldFamily::Electric,
        0.0,
        ParameterSet::default(),
        &interface,
    );
    match &snapshot.value {
        vector_field_explorer::notebook::NotebookValue::Unavailable { reason } => {
            assert!(!reason.trim().is_empty());
            assert!(reason.contains("interface"), "reason: {reason}");
        }
        other => panic!("excluded sample must be unavailable, got {other:?}"),
    }
    assert_eq!(snapshot.value.estimate(), None);

    let other_case = MaxwellScene {
        case: MaxwellCase::ParallelPlates,
        ..MaxwellScene::default()
    };
    let excluded = compute_reading(&other_case, 1.0, FieldFamily::Electric, 0.0);
    let snapshot = MeasurementSnapshot::shell(
        &other_case,
        1.0,
        FieldFamily::Electric,
        0.0,
        ParameterSet::default(),
        &excluded,
    );
    assert!(matches!(
        snapshot.value,
        vector_field_explorer::notebook::NotebookValue::Unavailable { .. }
    ));
}

#[test]
fn json_round_trip_and_older_payload_default_new_fields() {
    let scene = hollow(1.0);
    let mut notebook = Notebook::new();
    notebook
        .pin(
            "prediction",
            "note",
            shell_snapshot(&scene, 1.5),
            scene_doc("Round trip"),
        )
        .expect("entry fits");
    let first = notebook.entries[0].clone();

    let document = first.scene.to_json().expect("scene json");
    assert_eq!(SceneDocument::from_json(&document), Ok(first.scene.clone()));

    let json = notebook.to_json().expect("notebook json");
    let reopened = Notebook::from_json(&json).expect("reopen");
    assert_eq!(
        reopened.to_json().expect("canonical json"),
        notebook.to_json().expect("canonical json"),
        "the export reopens with the same canonical JSON"
    );
    assert_eq!(reopened.version, NOTEBOOK_VERSION);

    // An older payload may omit the later addition/entry/note fields; defaults
    // must load cleanly rather than failing the whole notebook.
    let mut value: serde_json::Value = serde_json::from_str(&json).expect("json value");
    let entry = value["entries"][0].as_object_mut().expect("entry object");
    entry.remove("note");
    entry.remove("prediction");
    let snapshot = entry["snapshot"].as_object_mut().expect("snapshot");
    snapshot.remove("method");
    snapshot.remove("units");
    snapshot.remove("time");
    let older = serde_json::to_string(&value).expect("older json");
    let loaded = Notebook::from_json(&older).expect("older payload loads");
    assert_eq!(loaded.entries.len(), 1);
    let loaded = &loaded.entries[0];
    assert_eq!(loaded.note, "");
    assert_eq!(loaded.prediction, "");
    assert_eq!(loaded.snapshot.method, "");
    assert_eq!(loaded.snapshot.units, "");
    assert_eq!(loaded.snapshot.time, 0.0);
    assert_eq!(loaded.scene.name, "Round trip");
}

#[test]
fn sanitize_bounds_drops_duplicates_and_corrupt_entries() {
    let scene = hollow(1.0);
    let mut entries = Vec::new();
    for index in 0..(MAX_NOTEBOOK_ENTRIES + 5) {
        entries.push(entry(&format!("entry-{index}"), &scene, 1.0 + index as f64));
    }
    // Duplicate id and an invalid scene name; both must not survive.
    entries.push(entry("entry-0", &scene, 2.0));
    let mut broken = entry("broken", &scene, 1.5);
    broken.scene.name.clear();
    entries.push(broken);

    let notebook = Notebook {
        version: 99,
        entries,
    }
    .sanitized();
    assert_eq!(notebook.version, NOTEBOOK_VERSION);
    assert_eq!(notebook.entries.len(), MAX_NOTEBOOK_ENTRIES);
    let mut ids = std::collections::HashSet::new();
    for entry in &notebook.entries {
        assert!(ids.insert(entry.id.clone()), "duplicate id survived");
        assert!(entry.scene.validate().is_ok());
    }
    assert!(!notebook.entries.iter().any(|entry| entry.id == "broken"));

    // Restoring a bounded notebook with an explicit ordered id list keeps them.
    let mut first = Notebook::new();
    let id = first
        .pin("p", "n", shell_snapshot(&scene, 1.5), scene_doc("First"))
        .expect("entry fits");
    assert_eq!(first.get(&id).expect("stored").id, id);
    assert!(first.remove(&id));
    assert!(first.get(&id).is_none());
}

#[test]
fn export_import_round_trip_and_duplicate_or_corrupt_import_is_sanitized() {
    let scene = hollow(1.0);
    let mut source = Notebook::new();
    source
        .pin("p", "n", shell_snapshot(&scene, 1.5), scene_doc("One"))
        .expect("entry fits");
    source
        .pin("p2", "n2", shell_snapshot(&scene, 3.0), scene_doc("Two"))
        .expect("entry fits");
    let json = source.to_json().expect("export");

    let imported = Notebook::from_json(&json).expect("import");
    let mut target = Notebook::new();
    assert_eq!(target.append(imported), 2);
    assert_eq!(
        serde_json::to_string(&target.entries).expect("entries json"),
        serde_json::to_string(&source.entries).expect("entries json")
    );

    // Re-importing the same ids appends nothing.
    assert_eq!(target.append(source.clone()), 0);
    assert_eq!(target.entries.len(), 2);
    assert!(Notebook::from_json("{not json").is_err());

    // Replacement still sanitizes duplicate ids rather than keeping both.
    let mut duplicated = source.clone();
    duplicated.entries.push(source.entries[0].clone());
    let replacement = Notebook::from_json(&duplicated.to_json().expect("json")).expect("load");
    let mut replaced = Notebook::new();
    replaced.replace(replacement);
    assert_eq!(replaced.entries.len(), 2);
}

fn maxwell_app() -> VfApp {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Maxwell);
    assert!(app.state.set_maxwell_scene(hollow(1.0)));
    app.maxwell.sync_from(&app.state);
    app.maxwell.gauss.activate(&app.state.maxwell);
    assert!(app.maxwell.gauss.set_probe_radius(1.5));
    app
}

#[test]
fn notebook_restores_magnetic_wave_probe_over_hollow_sphere() {
    use glam::Vec3;
    use vector_field_explorer::field_probe::{
        CirculationLoop, IntegralStatus, LoopKind, ProbeField, circulation,
    };
    use vector_field_explorer::maxwell_wave::{WaveSpec, sample_wave};

    struct MagneticWave(WaveSpec);
    impl ProbeField for MagneticWave {
        fn eval(&self, point: Vec3, time: f64) -> Option<Vec3> {
            Some(sample_wave(&self.0, point, time)?.b)
        }
    }
    let mut app = maxwell_app();
    let wave = WaveSpec {
        amplitude: 1.5,
        wavelength: 4.0,
        direction: Vec3::X,
        polarization: Vec3::Y,
        phase: 0.0,
    };
    // B varies along x and points along z, giving nonzero circulation here.
    // Wave E is perpendicular to the loop; the hidden electrostatic shell
    // cannot produce this magnetic reading either.
    let path = CirculationLoop {
        kind: LoopKind::Circle,
        center: Vec3::X,
        axis: Vec3::Y,
        size: Vec3::splat(0.5),
        segments: 32,
    };
    assert!(app.state.set_maxwell_wave(Some(wave)));
    assert!(app.state.set_probe_family(FieldFamily::Magnetic));
    app.state.add_probe_loop(path).unwrap();
    assert!(app.test_notebook_action(None));
    let id = app.maxwell.notebook.entries[0].id.clone();
    assert!(app.state.set_maxwell_wave(None));
    app.state.set_probe_family(FieldFamily::Electric);
    assert!(app.test_notebook_action(Some(&id)));
    assert_eq!(app.state.probe_family(), FieldFamily::Magnetic);
    app.state.refresh_probe_reports(true);
    let expected = circulation(
        &MagneticWave(wave),
        &path,
        FieldFamily::Magnetic,
        f64::from(app.state.time),
    );
    assert!(matches!(expected.status, IntegralStatus::Estimate { value, .. } if value.abs() > 0.1));
    assert_eq!(
        app.state.probe_loop_report(0).unwrap().status,
        expected.status
    );
}

fn assert_invalid_import_leaves_experiment_unchanged(radius: &str, value: f64, error: &str) {
    let mut app = maxwell_app();
    assert!(app.test_notebook_action(None));
    let mut json = serde_json::to_value(&app.maxwell.notebook).expect("capture serializes");
    json["entries"][0]["snapshot"]["geometry"][radius] = serde_json::json!(value);
    let imported = Notebook::from_json(&json.to_string()).expect("JSON imports");
    let id = imported
        .entries
        .first()
        .expect("entry survives import")
        .id
        .clone();
    app.maxwell.notebook.replace(imported);

    let live_scene = hollow(1.6);
    assert!(app.state.set_maxwell_scene(live_scene.clone()));
    assert!(app.maxwell.gauss.set_probe_radius(6.0));
    app.state.time = 9.0;
    app.orbit.yaw = 1.25;
    let orbit = app.orbit;
    app.presets.bind_active_scene(scene_doc("Previously saved"));

    assert!(!app.test_notebook_action(Some(&id)));
    assert!(
        app.maxwell
            .notebook_ui
            .error
            .as_deref()
            .is_some_and(|text| text.contains(error)),
        "expected {error:?} in {:?}",
        app.maxwell.notebook_ui.error
    );
    assert_eq!(
        (
            &app.state.maxwell,
            app.state.time,
            app.maxwell.gauss.probe_radius,
            &app.orbit,
            app.presets.active_scene(),
        ),
        (&live_scene, 9.0, 6.0, &orbit, Some("Previously saved"))
    );
}

#[test]
fn invalid_imported_shell_radius_leaves_experiment_unchanged() {
    assert_invalid_import_leaves_experiment_unchanged("shell_radius", 1000.0, "outer radius");
}

#[test]
fn invalid_imported_probe_radius_leaves_experiment_unchanged() {
    assert_invalid_import_leaves_experiment_unchanged("probe_radius", 101.0, "probe radius");
}

#[test]
fn loading_entries_restores_each_scene_and_radius_and_keeps_the_notebook() {
    let mut app = maxwell_app();
    app.orbit.yaw = 0.25;
    app.maxwell.notebook_ui.prediction = "first".to_owned();
    app.maxwell.notebook_ui.note = "note first".to_owned();
    assert!(app.test_notebook_action(None));
    let first_id = app.maxwell.notebook.entries[0].id.clone();
    assert_eq!(
        app.maxwell.notebook.entries[0].snapshot.method,
        SHELL_METHOD
    );
    assert_eq!(app.maxwell.notebook.entries[0].snapshot.units, SHELL_UNITS);

    assert!(app.state.set_maxwell_scene(hollow(1.5)));
    app.maxwell.sync_from(&app.state);
    app.maxwell.gauss.activate(&app.state.maxwell);
    assert!(app.maxwell.gauss.set_probe_radius(3.0));
    app.orbit.yaw = 1.25;
    app.maxwell.notebook_ui.prediction = "second".to_owned();
    assert!(app.test_notebook_action(None));
    let second_id = app.maxwell.notebook.entries[1].id.clone();
    assert_ne!(first_id, second_id);

    // Live edits after both captures must not change either stored entry.
    let first_before = serde_json::to_string(&app.maxwell.notebook.entries[0]).expect("json");
    let second_before = serde_json::to_string(&app.maxwell.notebook.entries[1]).expect("json");
    assert!(app.state.set_maxwell_scene(hollow(1.6)));
    app.maxwell.gauss.set_probe_radius(6.0);
    app.state.time = 9.0;
    assert_eq!(
        serde_json::to_string(&app.maxwell.notebook.entries[0]).expect("json"),
        first_before
    );
    assert_eq!(
        serde_json::to_string(&app.maxwell.notebook.entries[1]).expect("json"),
        second_before
    );

    app.presets.bind_active_scene(scene_doc("Previously saved"));
    assert!(app.test_notebook_action(Some(&first_id)));
    assert_eq!(app.presets.active_scene(), None);
    assert_eq!(app.state.maxwell.radius, 1.0);
    assert_eq!(app.maxwell.gauss.probe_radius, 1.5);
    assert!((app.orbit.yaw - 0.25).abs() < 1e-6);
    assert_eq!(app.maxwell.notebook.entries.len(), 2);

    assert!(app.test_notebook_action(Some(&second_id)));
    assert_eq!(app.state.maxwell.radius, 1.5);
    assert_eq!(app.maxwell.gauss.probe_radius, 3.0);
    assert!((app.orbit.yaw - 1.25).abs() < 1e-6);
    assert_eq!(app.maxwell.notebook.entries.len(), 2);
}

#[test]
fn notebook_reopens_from_the_settings_envelope_without_applying_a_scene() {
    let mut app = maxwell_app();
    app.maxwell.notebook_ui.prediction = "saved prediction".to_owned();
    assert!(app.test_notebook_action(None));
    let expected = app.maxwell.notebook.clone();
    let mut storage = MemoryStorage::default();
    persistence::save(
        &mut storage,
        &AppSettings {
            view: ViewRecord::capture(&app.state),
            notebook: expected.clone(),
            ..AppSettings::default()
        },
    );

    let reopened = VfApp::new_with_storage(Some(&storage));
    assert_eq!(reopened.maxwell.notebook, expected);
    assert_eq!(reopened.maxwell.notebook.entries.len(), 1);
    assert_eq!(
        reopened.maxwell.notebook.entries[0]
            .snapshot
            .geometry
            .probe_radius(),
        1.5
    );
    assert_eq!(
        reopened.maxwell.notebook.entries[0].scene.name,
        "Notebook shell 1"
    );
}

fn panel_frame(
    ctx: &egui::Context,
    width: f32,
    height: f32,
    state: &mut State,
    panel: &mut MaxwellPanelState,
    compact: bool,
    events: Vec<egui::Event>,
) -> (egui::FullOutput, Option<MaxwellPanelAction>) {
    let mut action = None;
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, height),
            )),
            events,
            ..Default::default()
        },
        |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                action = maxwell_panel::show(ui, panel, state, compact);
            });
        },
    );
    output.textures_delta.clear();
    (output, action)
}

#[test]
fn notebook_buttons_are_keyboard_accessible_at_wide_and_compact_sizes() {
    for (width, height, compact) in [(1200.0, 3000.0, false), (390.0, 3000.0, true)] {
        let ctx = context();
        let mut state = State::new(true);
        state.set_field_mode(FieldMode::Maxwell);
        assert!(state.set_maxwell_scene(hollow(1.0)));
        state.refresh_maxwell_sampling();
        let mut panel = MaxwellPanelState::default();
        panel.sync_from(&state);
        panel.gauss.activate(&state.maxwell);
        assert!(panel.gauss.set_probe_radius(1.5));
        panel.notebook_ui.open = true;

        let (out, _) = panel_frame(
            &ctx,
            width,
            height,
            &mut state,
            &mut panel,
            compact,
            Vec::new(),
        );
        let widget_nodes = nodes(&out);
        for label in [
            NOTEBOOK_TITLE,
            NOTEBOOK_CAPTURE_LABEL,
            NOTEBOOK_EMPTY,
            "Export / import",
        ] {
            assert!(
                find(&widget_nodes, label).is_some(),
                "{width} pt is missing {label}"
            );
        }
        let capture = find(&widget_nodes, NOTEBOOK_CAPTURE_LABEL).expect("capture button");
        assert!(capture.rect.min.x >= -1.0);
        let center = capture.rect.center();

        let _ = panel_frame(
            &ctx,
            width,
            height,
            &mut state,
            &mut panel,
            compact,
            pointer(center, true),
        );
        let (out, action) = panel_frame(
            &ctx,
            width,
            height,
            &mut state,
            &mut panel,
            compact,
            pointer(center, false),
        );
        assert!(nodes(&out).iter().any(|node| node.label == NOTEBOOK_TITLE));
        assert_eq!(
            action,
            Some(MaxwellPanelAction::Notebook(NotebookAction::Capture)),
            "{width} pt capture button"
        );
    }
}

#[test]
fn comparison_and_load_controls_render_through_the_real_panel() {
    let ctx = context();
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Maxwell);
    assert!(state.set_maxwell_scene(hollow(1.0)));
    state.refresh_maxwell_sampling();
    let mut panel = MaxwellPanelState::default();
    panel.sync_from(&state);
    panel.gauss.activate(&state.maxwell);
    let scene = hollow(1.0);
    let first = entry("a", &scene, 1.5);
    let second = entry("b", &scene, 3.0);
    panel.notebook_ui.open = true;
    panel.notebook_ui.compare_first = Some(first.id.clone());
    panel.notebook_ui.compare_second = Some(second.id.clone());
    let first_id = first.id.clone();
    panel.notebook.entries.push(first);
    panel.notebook.entries.push(second);

    let (out, _) = panel_frame(
        &ctx,
        1200.0,
        3000.0,
        &mut state,
        &mut panel,
        false,
        Vec::new(),
    );
    let widget_nodes = nodes(&out);
    assert!(find(&widget_nodes, "Load scene").is_some());
    assert!(find(&widget_nodes, "Compare A").is_some());
    let texts = painted_texts(&out).join("\n");
    assert!(texts.contains(NOTEBOOK_TITLE), "notebook title: {texts}");
    assert!(texts.contains("Changed settings"), "comparison: {texts}");
    assert!(texts.contains("Probe radius r"), "changed radius: {texts}");
    assert!(
        texts.contains("Equal within combined uncertainty: yes"),
        "equal-flux line: {texts}"
    );

    let mut loads: Vec<&WidgetNode> = widget_nodes
        .iter()
        .filter(|node| node.label == "Load scene")
        .collect();
    loads.sort_by(|a, b| a.rect.min.y.total_cmp(&b.rect.min.y));
    let load = loads.first().expect("load button");
    let center = load.rect.center();
    panel_frame(
        &ctx,
        1200.0,
        3000.0,
        &mut state,
        &mut panel,
        false,
        pointer(center, true),
    );
    let (_, action) = panel_frame(
        &ctx,
        1200.0,
        3000.0,
        &mut state,
        &mut panel,
        false,
        pointer(center, false),
    );
    assert_eq!(
        action,
        Some(MaxwellPanelAction::Notebook(NotebookAction::Load(first_id)))
    );
}
