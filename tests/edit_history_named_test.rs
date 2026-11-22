//! Issue #231: named document history and duplicate experiments.
//!
//! The pure history tests pin the label carried beside each snapshot. The app
//! tests drive the real frame driver, menu and Presets panel so the label a
//! learner reads and the duplicate/copy seam are exercised where they meet the
//! UI. No GPU device is created.

use eframe::egui::{self, vec2};
use glam::Vec3;

use crate::support;

use vector_field_explorer::VfApp;
use vector_field_explorer::edit_history::EditHistory;
use vector_field_explorer::field_probe::{GaussianSurface, SurfaceKind};
use vector_field_explorer::scene_document::SceneCamera;
use vector_field_explorer::scene_store::MAX_SCENES;
use vector_field_explorer::ui::presets::PresetPanel;

fn frame(ctx: &egui::Context, app: &mut VfApp, events: Vec<egui::Event>) -> egui::FullOutput {
    let mut native = eframe::Frame::_new_kittest();
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(900.0, 700.0),
            )),
            events,
            ..Default::default()
        },
        |ui| eframe::App::ui(app, ui, &mut native),
    );
    output.textures_delta.clear();
    output
}

fn idle(ctx: &egui::Context, app: &mut VfApp) {
    let _ = frame(ctx, app, Vec::new());
}

fn settle(ctx: &egui::Context, app: &mut VfApp) -> egui::FullOutput {
    for _ in 0..3 {
        idle(ctx, app);
    }
    frame(ctx, app, Vec::new())
}

fn moved(pos: egui::Pos2) -> egui::Event {
    egui::Event::PointerMoved(pos)
}

fn button(pos: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

/// The accesskit node carrying `label`, preferring button nodes.
fn labelled_rect(output: &egui::FullOutput, label: &str) -> Option<egui::Rect> {
    let update = output.platform_output.accesskit_update.as_ref()?;
    update
        .nodes
        .iter()
        .filter_map(|(_, node)| {
            let matches = |text: &str| {
                text == label
                    || text
                        .strip_prefix(label)
                        .is_some_and(|rest| rest.starts_with(' '))
            };
            if !node.label().is_some_and(matches) && !node.value().is_some_and(matches) {
                return None;
            }
            let bounds = node.bounds()?;
            Some((
                node.role() == egui::Role::Button,
                egui::Rect::from_min_max(
                    egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                    egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                ),
            ))
        })
        .min_by(|a, b| b.0.cmp(&a.0).then(a.1.left().total_cmp(&b.1.left())))
        .map(|(_, rect)| rect)
}

fn all_labels(output: &egui::FullOutput) -> Vec<String> {
    let Some(update) = output.platform_output.accesskit_update.as_ref() else {
        return Vec::new();
    };
    update
        .nodes
        .iter()
        .filter_map(|(_, node)| node.label().or_else(|| node.value()).map(str::to_owned))
        .collect()
}

fn click(ctx: &egui::Context, app: &mut VfApp, pos: egui::Pos2) {
    let _ = frame(ctx, app, vec![moved(pos), button(pos, true)]);
    let _ = frame(ctx, app, vec![button(pos, false)]);
}

fn spin_button_center(output: &egui::FullOutput, label_prefix: &str) -> Option<egui::Pos2> {
    let update = output.platform_output.accesskit_update.as_ref()?;
    update.nodes.iter().find_map(|(_, node)| {
        if node.role() != egui::Role::SpinButton {
            return None;
        }
        let label = node.label()?;
        if !label.starts_with(label_prefix) {
            return None;
        }
        let bounds = node.bounds()?;
        Some(egui::pos2(
            ((bounds.x0 + bounds.x1) / 2.0) as f32,
            ((bounds.y0 + bounds.y1) / 2.0) as f32,
        ))
    })
}

fn sphere(center: Vec3, radius: f32) -> GaussianSurface {
    GaussianSurface {
        kind: SurfaceKind::Sphere,
        center,
        axis: Vec3::Z,
        size: Vec3::splat(radius),
        segments: 8,
    }
}

fn panel_in(dir: &std::path::Path) -> PresetPanel {
    PresetPanel::new_with_dir(dir.to_path_buf()).expect("panel with tempdir")
}

fn app_in(dir: &std::path::Path) -> VfApp {
    let mut app = VfApp::new();
    app.presets = panel_in(dir);
    app
}

fn render_context() -> egui::Context {
    let ctx = support::context();
    ctx.enable_accesskit();
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    ctx
}

fn run_app_panel(
    ctx: &egui::Context,
    app: &mut VfApp,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 2200.0),
            )),
            events,
            ..Default::default()
        },
        |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let _ = app.presets.show(ui, &mut app.camera, &mut app.state);
                });
            });
        },
    );
    app.finish_presets_for_test();
    output.textures_delta.clear();
    output
}

/// Click the `index`-th visible node carrying `label` (insertion order).
fn click_app_label_nth(ctx: &egui::Context, app: &mut VfApp, label: &str, index: usize) {
    let out = run_app_panel(ctx, app, Vec::new());
    let rect = nodes_with_label(&out, label)
        .get(index)
        .copied()
        .unwrap_or_else(|| panic!("missing clickable {label} at index {index}"));
    run_app_panel(ctx, app, pointer(rect.center(), true));
    run_app_panel(ctx, app, pointer(rect.center(), false));
}

fn click_app_label(ctx: &egui::Context, app: &mut VfApp, label: &str) {
    click_app_label_nth(ctx, app, label, 0);
}

fn nodes_with_label(output: &egui::FullOutput, label: &str) -> Vec<egui::Rect> {
    let Some(update) = output.platform_output.accesskit_update.as_ref() else {
        return Vec::new();
    };
    update
        .nodes
        .iter()
        .filter_map(|(_, node)| {
            let matched = node.label() == Some(label) || node.value() == Some(label);
            if !matched {
                return None;
            }
            let bounds = node.bounds()?;
            Some(egui::Rect::from_min_max(
                egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
            ))
        })
        .collect()
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

// ---------------------------------------------------------------------------
// Pure history labels
// ---------------------------------------------------------------------------

#[test]
fn a_named_entry_describes_the_next_undo_and_survives_undo_and_redo() {
    let mut history = EditHistory::new();
    assert!(history.record_named(1u32, &2, "change formula"));
    assert_eq!(history.undo_label(), Some("change formula"));
    assert_eq!(history.redo_label(), None);

    assert_eq!(history.undo(2), Some(1));
    assert_eq!(history.undo_label(), None, "nothing left to undo");
    assert_eq!(
        history.redo_label(),
        Some("change formula"),
        "the redo describes the edit that was just undone"
    );

    assert_eq!(history.redo(1), Some(2));
    assert_eq!(history.undo_label(), Some("change formula"));
    assert_eq!(history.redo_label(), None);
}

#[test]
fn an_unnamed_entry_falls_back_to_the_honest_generic_label() {
    let mut history = EditHistory::new();
    history.begin(3u32);
    assert!(history.commit(&4));
    assert_eq!(history.undo_label(), Some("edit"));
}

// ---------------------------------------------------------------------------
// App-level labels
// ---------------------------------------------------------------------------

#[test]
fn one_drag_produces_one_named_entry() {
    let ctx = support::context();
    ctx.enable_accesskit();
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    let out = frame(&ctx, &mut app, Vec::new());
    let before = app.state.density;
    assert_eq!(app.history_len(), 0, "an idle frame records nothing");

    let start = spin_button_center(&out, "Samples per axis")
        .expect("the Controls panel paints the Density DragValue");
    let end = start + vec2(50.0, 0.0);
    let _ = frame(&ctx, &mut app, vec![moved(start), button(start, true)]);
    assert_eq!(app.history_len(), 0, "the press opens a transaction only");
    let _ = frame(&ctx, &mut app, vec![moved(start + vec2(25.0, 0.0))]);
    let _ = frame(&ctx, &mut app, vec![moved(end)]);
    assert_eq!(app.history_len(), 0, "mid-gesture frames stay suspended");
    let _ = frame(&ctx, &mut app, vec![button(end, false)]);

    assert_ne!(app.state.density, before, "the drag changed the density");
    assert_eq!(app.history_len(), 1, "one gesture is exactly one entry");
    assert_eq!(app.next_undo_label(), Some("change density"));
    assert_eq!(app.next_redo_label(), None);

    assert!(app.undo_document(&ctx));
    assert_eq!(app.next_undo_label(), None);
    assert_eq!(
        app.next_redo_label(),
        Some("change density"),
        "redo names the edit that was undone"
    );
    assert!(app.redo_document(&ctx));
    assert_eq!(app.next_undo_label(), Some("change density"));
}

#[test]
fn camera_navigation_and_playback_stay_outside_history() {
    let ctx = support::context();
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    idle(&ctx, &mut app);

    // Orbit/zoom/pan alone never create an entry: these are camera navigation,
    // not document edits.
    app.orbit.yaw += 0.4;
    app.orbit.pitch += 0.1;
    app.orbit.ortho_zoom *= 1.2;
    app.camera.scale *= 1.1;
    for _ in 0..3 {
        idle(&ctx, &mut app);
    }
    assert_eq!(app.history_len(), 0, "camera navigation is not an edit");
    assert_eq!(app.next_undo_label(), None);

    // Simulation playback moves runtime state only.
    for _ in 0..10 {
        app.test_harness().advance_clock(0.016);
        app.test_harness().advance_simulation();
    }
    idle(&ctx, &mut app);
    assert_eq!(app.history_len(), 0, "playback is not an edit");

    // A document edit is recorded and named; a later orbit does not rename or
    // extend it.
    app.state.set_formula("(y, -x)".into());
    idle(&ctx, &mut app);
    assert_eq!(app.history_len(), 1);
    assert_eq!(app.next_undo_label(), Some("change formula"));
    app.orbit.yaw += 0.2;
    idle(&ctx, &mut app);
    assert_eq!(app.history_len(), 1, "the orbit adds nothing");
    assert_eq!(app.next_undo_label(), Some("change formula"));
}

#[test]
fn the_edit_menu_shows_the_next_named_undo_and_its_shortcut() {
    let ctx = support::context();
    ctx.enable_accesskit();
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    settle(&ctx, &mut app);

    app.state.set_formula("(y, -x)".into());
    settle(&ctx, &mut app);
    assert_eq!(app.history_len(), 1);

    let out = settle(&ctx, &mut app);
    let edit = labelled_rect(&out, "Edit").expect("the menu bar paints Edit");
    click(&ctx, &mut app, edit.center());
    let out = settle(&ctx, &mut app);
    let labels = all_labels(&out);
    assert!(
        labels
            .iter()
            .any(|label| label.starts_with("Undo change formula")),
        "the Edit menu names the next undo: {labels:?}"
    );
    assert!(
        labels
            .iter()
            .any(|label| label.starts_with("Undo change formula") && label.contains("Ctrl+Z")),
        "the named entry also shows its shortcut: {labels:?}"
    );
}

#[test]
fn keyboard_undo_and_redo_keep_the_named_entry() {
    let ctx = support::context();
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    settle(&ctx, &mut app);
    let original = app.state.formula.clone();

    app.state.set_formula("(y, -x)".into());
    settle(&ctx, &mut app);
    assert_eq!(app.next_undo_label(), Some("change formula"));

    let undo = egui::Event::Key {
        key: egui::Key::Z,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::COMMAND,
    };
    let _ = frame(
        &ctx,
        &mut app,
        vec![
            egui::Event::ModifiersChanged(egui::Modifiers::COMMAND),
            undo,
        ],
    );
    assert_eq!(app.state.formula, original, "Ctrl+Z undoes the edit");
    assert_eq!(app.next_redo_label(), Some("change formula"));

    let redo = egui::Event::Key {
        key: egui::Key::Z,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
    };
    let _ = frame(
        &ctx,
        &mut app,
        vec![
            egui::Event::ModifiersChanged(egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT)),
            redo,
        ],
    );
    assert_eq!(app.state.formula, "(y, -x)", "Ctrl+Shift+Z redoes it");
    assert_eq!(app.next_undo_label(), Some("change formula"));
}

// ---------------------------------------------------------------------------
// Duplicate experiment
// ---------------------------------------------------------------------------

#[test]
fn duplicate_experiment_creates_a_unique_copy_and_keeps_the_original() {
    let ctx = render_context();
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.state.set_formula("x + y".to_string());
    app.presets.set_scene_name("Experiment");
    click_app_label(&ctx, &mut app, "Save current scene");
    assert!(app.presets.scenes().get("Experiment").is_some());

    app.state.set_formula("x - y".to_string());
    idle(&ctx, &mut app);
    assert_eq!(app.history_len(), 1, "the live formula edit is recorded");
    click_app_label(&ctx, &mut app, "Duplicate experiment");
    click_app_label(&ctx, &mut app, "Duplicate experiment");

    assert!(
        app.presets.scenes().get("Experiment copy").is_some(),
        "the first duplicate has a distinct name"
    );
    assert!(
        app.presets.scenes().get("Experiment copy 2").is_some(),
        "a second duplicate still gets a unique name"
    );
    assert!(
        app.presets.scenes().get("Experiment").is_some(),
        "the original stays accessible"
    );
    assert_eq!(
        app.presets
            .scenes()
            .get("Experiment")
            .expect("original")
            .view
            .formula,
        "x + y",
        "the original is unchanged"
    );
    assert_eq!(
        app.presets
            .scenes()
            .get("Experiment copy")
            .expect("copy")
            .view
            .formula,
        "x - y",
        "the copy captures the live experiment"
    );

    // The duplicate is a scene-store action, outside the document history: no
    // hidden history entry is invented.
    assert_eq!(
        app.history_len(),
        1,
        "only the live formula edit is in document history"
    );
    let message = app.presets.scene_message().unwrap_or_default();
    assert!(
        message.contains("Duplicated experiment") && message.contains("Experiment copy 2"),
        "the copy is named in the result line: {message:?}"
    );
}

#[test]
fn switching_between_original_and_copy_preserves_independent_state() {
    let ctx = render_context();
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());

    // The original experiment: one formula and one Gaussian surface.
    app.state.set_formula("x + y".to_string());
    app.state
        .add_probe_surface(sphere(Vec3::ZERO, 1.0))
        .expect("the original surface is valid");
    app.presets.set_scene_name("Experiment");
    click_app_label(&ctx, &mut app, "Save current scene");

    // Duplicate the saved original while the live experiment differs.
    app.state.set_formula("y - x".to_string());
    click_app_label(&ctx, &mut app, "Duplicate");

    // Rows are insertion order: Experiment, then Experiment copy.
    click_app_label_nth(&ctx, &mut app, "Load", 1);
    assert_eq!(app.state.formula, "x + y", "the copy of the original loads");
    assert_eq!(app.state.probe_surfaces().len(), 1);

    // Edit the copy: formula and its independent probe configuration.
    app.state.set_formula("2 * x".to_string());
    app.state
        .add_probe_surface(sphere(Vec3::new(1.0, 0.0, 0.0), 0.5))
        .expect("the copy surface is valid");
    idle(&ctx, &mut app);
    assert_eq!(app.next_undo_label(), Some("change formula"));

    // Switch back: the original's formula and probes are untouched.
    click_app_label_nth(&ctx, &mut app, "Load", 0);
    assert_eq!(app.state.formula, "x + y");
    assert_eq!(
        app.state.probe_surfaces().len(),
        1,
        "the original keeps its single surface"
    );
    assert_eq!(
        app.presets
            .scenes()
            .get("Experiment copy")
            .expect("copy")
            .view
            .formula,
        "2 * x",
        "the copy's stored formula is independent"
    );

    // Switch forward again: the copy's edits survived.
    click_app_label_nth(&ctx, &mut app, "Load", 1);
    assert_eq!(app.state.formula, "2 * x");
    assert_eq!(
        app.state.probe_surfaces().len(),
        2,
        "the copy keeps both of its surfaces"
    );
    idle(&ctx, &mut app);
    assert_eq!(
        app.next_undo_label(),
        Some("change formula"),
        "undo names the edit in the active copy"
    );
}

#[test]
fn undoing_after_switching_scenes_never_rewrites_the_bound_scene() {
    let ctx = render_context();
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());

    // Scene A: one formula and its own probe surface.
    app.state.set_formula("x + y".to_string());
    app.state
        .add_probe_surface(sphere(Vec3::ZERO, 1.0))
        .expect("the A surface is valid");
    app.presets.set_scene_name("A");
    click_app_label(&ctx, &mut app, "Save current scene");

    // Scene B: a different formula and a different probe surface.
    app.state.set_formula("y - x".to_string());
    assert!(app.state.remove_probe_surface(0));
    app.state
        .add_probe_surface(sphere(Vec3::new(1.0, 0.0, 0.0), 0.5))
        .expect("the B surface is valid");
    app.presets.set_scene_name("B");
    click_app_label(&ctx, &mut app, "Save current scene");

    // Visit A, then B; each load is a document change the driver records.
    click_app_label_nth(&ctx, &mut app, "Load", 0);
    idle(&ctx, &mut app);
    assert_eq!(app.state.formula, "x + y");
    click_app_label_nth(&ctx, &mut app, "Load", 1);
    idle(&ctx, &mut app);
    assert_eq!(app.state.formula, "y - x");
    assert_eq!(app.presets.active_scene(), Some("B"));

    // Undo restores A's document. It must also drop B's binding, otherwise the
    // next sync writes A's formula and probe under B's name.
    assert!(app.undo_document(&ctx));
    assert_eq!(app.state.formula, "x + y", "undo restores A's document");
    assert_eq!(
        app.presets.active_scene(),
        None,
        "a restored document is not attributed to the previously bound scene"
    );

    // Any scene action flushes pending in-memory scene edits.
    click_app_label(&ctx, &mut app, "Duplicate");

    let (persisted, _) = vector_field_explorer::scene_store::load_dir(dir.path());
    let a = persisted.get("A").expect("A survives");
    assert_eq!(a.view.formula, "x + y", "A keeps its own formula");
    assert_eq!(a.view.probe_surfaces.len(), 1);
    assert_eq!(a.view.probe_surfaces[0].center, [0.0, 0.0, 0.0]);

    let b = persisted.get("B").expect("B survives");
    assert_eq!(b.view.formula, "y - x", "B keeps its own formula");
    assert_eq!(b.view.probe_surfaces.len(), 1);
    assert_eq!(
        b.view.probe_surfaces[0].center,
        [1.0, 0.0, 0.0],
        "B keeps its own probe surface"
    );
}

#[test]
fn a_failed_scene_flush_keeps_the_edits_bound_to_the_same_scene() {
    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());
    let original = vector_field_explorer::scene_document::SceneDocument::capture(
        "A",
        Default::default(),
        Default::default(),
        SceneCamera::default(),
    );
    panel
        .insert_scene(original.clone())
        .expect("the scene saves");
    panel.bind_active_scene(original.clone());

    let mut edited = original;
    edited.camera.scale = 2.0;
    panel.sync_active_scene(edited);

    // Make the store write fail by replacing the file with a directory.
    let path = dir.path().join("scenes.json");
    std::fs::remove_file(&path).expect("the saved file exists");
    std::fs::create_dir(&path).expect("the file is replaced by a directory");
    panel
        .flush_scenes()
        .expect_err("the write is refused while the path is a directory");

    // The dirty flag must survive the failure, so a retry still has work.
    panel
        .flush_scenes()
        .expect_err("a failed flush does not clear the dirty state");
    assert_eq!(
        panel.scenes().get("A").map(|scene| scene.camera.scale),
        Some(2.0),
        "the in-memory edit stays on the scene it was made against"
    );

    std::fs::remove_dir(&path).expect("the blocking directory is removed");
    panel
        .flush_scenes()
        .expect("the retry writes the pending edit");
    let (persisted, _) = vector_field_explorer::scene_store::load_dir(dir.path());
    assert_eq!(
        persisted.get("A").map(|scene| scene.camera.scale),
        Some(2.0),
        "the retry persists the edit to the same scene"
    );
}

#[test]
fn saving_the_app_flushes_pending_active_scene_edits() {
    let ctx = render_context();
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.state.set_formula("x + y".to_string());
    app.presets.set_scene_name("Experiment");
    click_app_label(&ctx, &mut app, "Save current scene");
    click_app_label(&ctx, &mut app, "Load");

    app.state.set_formula("y - x".to_string());
    idle(&ctx, &mut app);

    // No scene action runs before exit: eframe's save hook is the reliable
    // flush point for the pending active-scene edit.
    let mut storage = crate::support::storage::MemoryStorage::default();
    eframe::App::save(&mut app, &mut storage);

    let (persisted, _) = vector_field_explorer::scene_store::load_dir(dir.path());
    assert_eq!(
        persisted
            .get("Experiment")
            .expect("the bound scene persists")
            .view
            .formula,
        "y - x",
        "the pending edit is written on save, not lost on exit"
    );
}

#[test]
fn visiting_a_scene_does_not_rewrite_its_stored_document() {
    let ctx = render_context();
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    let camera = SceneCamera {
        pitch: 2.0,
        ..SceneCamera::default()
    };
    app.presets
        .insert_scene(
            vector_field_explorer::scene_document::SceneDocument::capture(
                "hand edited",
                Default::default(),
                Default::default(),
                camera,
            ),
        )
        .expect("the stored scene keeps its hand-edited camera");

    click_app_label(&ctx, &mut app, "Load");
    for _ in 0..3 {
        idle(&ctx, &mut app);
    }
    // A scene action flushes pending active-scene edits; merely visiting the
    // scene must not write the live-clamped camera back over the stored value.
    click_app_label(&ctx, &mut app, "Duplicate");

    let (persisted, _) = vector_field_explorer::scene_store::load_dir(dir.path());
    assert_eq!(
        persisted
            .get("hand edited")
            .expect("the original survives")
            .camera
            .pitch,
        2.0,
        "visiting a scene does not silently repair its stored document"
    );
    assert!(
        persisted.get("hand edited copy").is_some(),
        "duplication still copies the stored document"
    );
}

#[test]
fn duplicate_names_fit_the_scene_limit_and_full_store_reports_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());
    let long = "a".repeat(64);
    panel
        .insert_scene(long_scene(&long))
        .expect("a 64-byte name is valid");
    let copy = panel.duplicate_scene(&long).expect("the copy stores");
    assert!(
        copy.len() <= 64,
        "copy names respect the byte limit: {copy}"
    );
    assert_ne!(copy, long);
    let second = panel.duplicate_scene(&long).expect("the next copy stores");
    assert_ne!(second, copy, "every copy name is unique");

    // Fill a fresh store to MAX_SCENES, then a duplicate must fail visibly.
    let full_dir = tempfile::tempdir().unwrap();
    let mut full = panel_in(full_dir.path());
    for index in 0..MAX_SCENES {
        full.insert_scene(long_scene(&format!("scene {index}")))
            .expect("fill the store");
    }
    let error = full
        .duplicate_scene("scene 0")
        .expect_err("a full store rejects the copy");
    assert!(
        error.contains(&MAX_SCENES.to_string()),
        "the error names the cap: {error}"
    );
    assert!(
        full.scene_message()
            .is_some_and(|message| message.contains("at most")),
        "the panel surfaces the reason"
    );
    assert!(full.scenes().get("scene 0 copy").is_none());
}

fn long_scene(name: &str) -> vector_field_explorer::scene_document::SceneDocument {
    vector_field_explorer::scene_document::SceneDocument::capture(
        name,
        Default::default(),
        Default::default(),
        SceneCamera::default(),
    )
}
