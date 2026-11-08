// Tests for the preset management panel's action layer (Task 2).
//
// egui's immediate-mode UI has no offscreen test backend wired up here, so the
// panel's user-facing actions are factored into plain methods and the buttons
// in `PresetPanel::show` route through them. These tests drive the actions the
// way a button press would.

use vector_field_explorer::render::Camera;
use vector_field_explorer::ui::presets::PresetPanel;
use vector_field_explorer::export::{delete_preset, load_presets};

fn panel_in(dir: &std::path::Path) -> PresetPanel {
    PresetPanel::new_with_dir(dir.to_path_buf()).expect("panel with tempdir")
}

fn cam_home() -> Camera {
    Camera { position: [1.0, 2.0], scale: 3.0 }
}

#[test]
fn test_create_records_current_camera_state() {
    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());

    panel.save_current("home", &cam_home()).expect("save");

    let saved = panel.presets().iter().find(|p| p.name == "home").expect("created preset listed immediately");
    assert_eq!(saved.position, [1.0, 2.0]);
    assert_eq!(saved.scale, 3.0);
}

#[test]
fn test_load_restores_exact_camera_state() {
    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());
    panel.save_current("home", &cam_home()).expect("save");

    // Drag the camera somewhere else, then load the preset back.
    let mut camera = Camera { position: [7.0, -7.0], scale: 0.5 };
    assert!(panel.load("home", &mut camera), "preset found");
    assert_eq!(camera.position, [1.0, 2.0]);
    assert_eq!(camera.scale, 3.0);

    assert!(!panel.load("missing", &mut camera), "unknown preset leaves camera unchanged");
    assert_eq!(camera.position, [1.0, 2.0]);
}

#[test]
fn test_delete_removes_from_list_and_disk() {
    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());
    panel.save_current("home", &cam_home()).expect("save");

    panel.delete("home").expect("delete");

    assert!(!panel.presets().iter().any(|p| p.name == "home"), "removed from the panel list");
    let on_disk = load_presets(dir.path()).expect("reload file");
    assert!(!on_disk.iter().any(|p| p.name == "home"), "removed from disk");
}

#[test]
fn test_presets_survive_restart() {
    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());
    panel.save_current("home", &cam_home()).expect("save");
    drop(panel);

    // Simulate an application restart: a fresh panel pointing at the same directory.
    let mut fresh = panel_in(dir.path());
    let mut camera = Camera::default();
    assert!(fresh.load("home", &mut camera), "preset still present after restart");
    assert_eq!(camera.position, [1.0, 2.0]);
    assert_eq!(camera.scale, 3.0);
}

#[test]
fn test_delete_preset_function_removes_file_entry() {
    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());
    panel.save_current("home", &cam_home()).expect("save");
    panel.save_current("work", &Camera { position: [0.0, 0.0], scale: 2.0 }).expect("save");

    delete_preset(dir.path(), "work").expect("delete");
    let on_disk = load_presets(dir.path()).expect("reload file");
    assert_eq!(on_disk.len(), 1);
    assert_eq!(on_disk[0].name, "home");
}

// Moved out of `src/ui/presets.rs`'s inline module (ticket-14 review I-4):
// active-row highlight, delete confirmation and the applied-camera handoff.

#[test]
fn saving_highlights_the_row_and_a_duplicate_replaces_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());
    panel.save_current("home", &cam_home()).expect("save");
    assert_eq!(panel.active_name(), Some("home"), "a saved view is the active row");
    assert_eq!(panel.presets().len(), 1);

    let moved = Camera { position: [-4.0, 0.5], scale: 0.25 };
    panel.save_current("home", &moved).expect("replace");
    assert_eq!(panel.presets().len(), 1, "the duplicate name still replaces");
    let saved = &panel.presets()[0];
    assert_eq!(saved.position, moved.position);
    assert_eq!(saved.scale, moved.scale);
    assert_eq!(panel.active_name(), Some("home"));
}

#[test]
fn loading_highlights_the_applied_row_and_a_missing_name_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());
    panel.save_current("home", &cam_home()).expect("save");
    panel
        .save_current("work", &Camera { position: [0.0, 0.0], scale: 2.0 })
        .expect("save");
    let mut camera = Camera::default();
    assert!(panel.load("home", &mut camera));
    assert_eq!(panel.active_name(), Some("home"));
    assert!(panel.load("work", &mut camera));
    assert_eq!(panel.active_name(), Some("work"));
    assert!(!panel.load("missing", &mut camera), "unknown preset stays untouched");
    assert_eq!(panel.active_name(), Some("work"), "a failed load keeps the highlight");
}

#[test]
fn deleting_waits_for_confirmation_then_clears_the_highlight() {
    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());
    panel.save_current("home", &cam_home()).expect("save");
    panel.request_delete("home");
    assert_eq!(panel.pending_delete(), Some("home"));
    assert_eq!(panel.presets().len(), 1, "requesting a delete removes nothing yet");

    panel.cancel_delete();
    assert_eq!(panel.pending_delete(), None);
    assert_eq!(panel.presets().len(), 1, "cancel keeps the preset");

    panel.request_delete("home");
    panel.delete("home").expect("confirmed delete");
    assert!(panel.presets().is_empty(), "the confirmed delete removes the preset");
    assert_eq!(panel.pending_delete(), None, "the confirmation clears");
    assert_eq!(panel.active_name(), None, "deleting the active row clears the highlight");
}

#[test]
fn a_load_hands_the_camera_to_the_live_view_exactly_once() {
    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());
    panel.save_current("home", &cam_home()).expect("save");
    let mut camera = Camera::default();
    assert!(panel.take_applied().is_none(), "a save does not move the live view");
    assert!(panel.load("home", &mut camera));
    assert_eq!(
        panel.take_applied().map(|c| (c.position, c.scale)),
        Some(([1.0, 2.0], 3.0)),
        "the loaded camera reaches the live view",
    );
    assert!(panel.take_applied().is_none(), "and only once per click");
}

#[test]
fn resetting_the_camera_can_clear_the_highlight() {
    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());
    panel.save_current("home", &cam_home()).expect("save");
    panel.clear_active();
    assert_eq!(panel.active_name(), None);
    assert_eq!(panel.presets().len(), 1, "clearing the highlight keeps the preset");
}
