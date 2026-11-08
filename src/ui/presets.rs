use eframe::egui;
use std::fs;
use std::io;
use std::path::PathBuf;

use crate::export::{delete_preset, load_presets, save_preset, Preset};
use crate::render::Camera;

/// Directory where presets.json lives by default: XDG_CONFIG_HOME if set,
/// otherwise ~/.config/vector-field-explorer.
pub fn default_preset_dir() -> PathBuf {
    match std::env::var("XDG_CONFIG_HOME") {
        Ok(p) => PathBuf::from(p).join("vector-field-explorer"),
        Err(_) => {
            let home = dirs_next::home_dir().unwrap_or_else(|| PathBuf::from("."));
            home.join(".config/vector-field-explorer")
        }
    }
}

/// UI panel for managing camera presets. The panel's user-facing actions
/// (`save_current`, `load`, `delete`) are factored into plain methods so the
/// buttons in [`PresetPanel::show`] route through exactly the behavior the
/// integration tests in `tests/ui_preset_test.rs` exercise.
///
/// Ticket 14 additions: the row whose camera state was last saved/applied is
/// highlighted (`active`), and deleting takes a confirmation click
/// (`pending_delete`) instead of removing on the first click.
pub struct PresetPanel {
    /// Directory where presets.json lives.
    dir: PathBuf,
    /// Cached list of presets.
    presets: Vec<Preset>,
    /// Pending name for the "Save current view" name field.
    new_name: String,
    /// The preset last saved or applied; drives the row highlight.
    active: Option<String>,
    /// Preset awaiting the delete confirmation.
    pending_delete: Option<String>,
    /// A camera loaded this frame, for the app to hand to the live view. Taken
    /// once so one click applies one camera.
    applied: Option<Camera>,
}

impl Default for PresetPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl PresetPanel {
    /// Panel pointed at the default data directory.
    pub fn new() -> Self {
        let dir = default_preset_dir();
        let _ = fs::create_dir_all(&dir);
        let presets = load_presets(&dir).unwrap_or_default();
        Self {
            dir,
            presets,
            new_name: String::new(),
            active: None,
            pending_delete: None,
            applied: None,
        }
    }

    /// Panel pointed at an explicit directory (used by tests and by the app
    /// once it owns the data-directory configuration).
    pub fn new_with_dir(dir: PathBuf) -> io::Result<Self> {
        let _ = fs::create_dir_all(&dir);
        let presets = load_presets(&dir).unwrap_or_default();
        Ok(Self {
            dir,
            presets,
            new_name: String::new(),
            active: None,
            pending_delete: None,
            applied: None,
        })
    }

    /// Directory this panel persists to.
    pub fn dir(&self) -> &PathBuf {
        &self.dir
    }

    /// Presets currently listed in the panel.
    pub fn presets(&self) -> &[Preset] {
        &self.presets
    }

    /// The highlighted preset row, if any.
    pub fn active_name(&self) -> Option<&str> {
        self.active.as_deref()
    }

    /// The preset waiting for a delete confirmation, if any.
    pub fn pending_delete(&self) -> Option<&str> {
        self.pending_delete.as_deref()
    }

    /// The camera state applied by a `load` this frame, for the app to move
    /// the live view. Taken once per click.
    pub fn take_applied(&mut self) -> Option<Camera> {
        self.applied.take()
    }

    /// Drop the row highlight (e.g. after the camera was reset elsewhere).
    pub fn clear_active(&mut self) {
        self.active = None;
    }

    /// Persist the current camera state under `name` and list it immediately.
    /// Saving with an existing name replaces that entry (existing ruling);
    /// the saved preset becomes the highlighted row.
    pub fn save_current(&mut self, name: &str, camera: &Camera) -> io::Result<()> {
        save_preset(&self.dir, name, camera)?;
        self.presets.retain(|p| p.name != name);
        self.presets.push(Preset {
            name: name.to_owned(),
            position: camera.position,
            scale: camera.scale,
        });
        self.active = Some(name.to_owned());
        Ok(())
    }

    /// Apply the named preset to the camera and highlight its row. Returns
    /// false — leaving the camera and highlight untouched — if no such preset
    /// is listed.
    pub fn load(&mut self, name: &str, camera: &mut Camera) -> bool {
        match self.presets.iter().find(|p| p.name == name) {
            Some(p) => {
                camera.position = p.position;
                camera.scale = p.scale;
                self.active = Some(name.to_owned());
                self.applied = Some(Camera { position: p.position, scale: p.scale });
                true
            }
            None => false,
        }
    }

    /// Ask for confirmation before removing the named preset.
    pub fn request_delete(&mut self, name: &str) {
        self.pending_delete = Some(name.to_owned());
    }

    /// Drop the pending delete request (the user cancelled).
    pub fn cancel_delete(&mut self) {
        self.pending_delete = None;
    }

    /// Remove the named preset from the panel list and from disk. Clears both
    /// the pending request and the highlight when they name this preset.
    pub fn delete(&mut self, name: &str) -> io::Result<()> {
        delete_preset(&self.dir, name)?;
        self.presets.retain(|p| p.name != name);
        if self.active.as_deref() == Some(name) {
            self.active = None;
        }
        if self.pending_delete.as_deref() == Some(name) {
            self.pending_delete = None;
        }
        Ok(())
    }

    pub fn show(&mut self, ui: &mut egui::Ui, camera: &mut Camera) {
        ui.heading("Presets");

        if self.presets.is_empty() {
            ui.weak("No saved views yet.");
        }

        // Rows for the current list. Names are cloned so the list can be
        // mutated inside the loop.
        let names: Vec<String> = self.presets.iter().map(|p| p.name.clone()).collect();
        for name in &names {
            let is_active = self.active.as_deref() == Some(name.as_str());
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(is_active, name)
                    .on_hover_text("Apply this saved view")
                    .clicked()
                {
                    self.load(name, camera);
                }
                if is_active {
                    ui.weak("active");
                }
                if ui
                    .button("Delete")
                    .on_hover_text("Delete this preset (asks for confirmation)")
                    .clicked()
                {
                    self.request_delete(name);
                }
            });
            if self.pending_delete.as_deref() == Some(name.as_str()) {
                ui.horizontal(|ui| {
                    ui.colored_label(
                        egui::Color32::from_rgb(230, 70, 70),
                        format!("Delete \"{name}\"?"),
                    );
                    if ui.button("Delete").clicked()
                        && let Err(e) = self.delete(name)
                    {
                        eprintln!("Preset delete failed: {e:?}");
                    }
                    if ui.button("Cancel").clicked() {
                        self.cancel_delete();
                    }
                });
            }
        }

        // Save the current camera as a preset. Duplicate names replace the
        // existing entry (existing ruling), so the name field is not claimed
        // to be unique.
        ui.separator();
        ui.label("Save current view");
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.new_name)
                    .hint_text("Preset name")
                    .desired_width(140.0),
            );
            let name = self.new_name.clone();
            if ui
                .add_enabled(!name.is_empty(), egui::Button::new("Save current view"))
                .on_hover_text("Save the current camera under this name (replaces an existing name)")
                .clicked()
            {
                if let Err(e) = self.save_current(&name, camera) {
                    eprintln!("Preset save failed: {e:?}");
                }
                self.new_name.clear();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panel_in(dir: &std::path::Path) -> PresetPanel {
        PresetPanel::new_with_dir(dir.to_path_buf()).expect("panel with tempdir")
    }

    fn home() -> Camera {
        Camera { position: [1.0, 2.0], scale: 3.0 }
    }

    #[test]
    fn saving_highlights_the_row_and_a_duplicate_replaces_it() {
        let dir = tempfile::tempdir().unwrap();
        let mut panel = panel_in(dir.path());
        panel.save_current("home", &home()).expect("save");
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
        panel.save_current("home", &home()).expect("save");
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
        panel.save_current("home", &home()).expect("save");
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
        panel.save_current("home", &home()).expect("save");
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
        panel.save_current("home", &home()).expect("save");
        panel.clear_active();
        assert_eq!(panel.active_name(), None);
        assert_eq!(panel.presets().len(), 1, "clearing the highlight keeps the preset");
    }
}
