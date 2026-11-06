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
pub struct PresetPanel {
    /// Directory where presets.json lives.
    dir: PathBuf,
    /// Cached list of presets.
    presets: Vec<Preset>,
    /// Pending name for the "Save as" text field.
    new_name: String,
}

impl PresetPanel {
    /// Panel pointed at the default data directory.
    pub fn new() -> Self {
        let dir = default_preset_dir();
        let _ = fs::create_dir_all(&dir);
        let presets = load_presets(&dir).unwrap_or_default();
        Self { dir, presets, new_name: String::new() }
    }

    /// Panel pointed at an explicit directory (used by tests and by the app
    /// once it owns the data-directory configuration).
    pub fn new_with_dir(dir: PathBuf) -> io::Result<Self> {
        let _ = fs::create_dir_all(&dir);
        let presets = load_presets(&dir).unwrap_or_default();
        Ok(Self { dir, presets, new_name: String::new() })
    }

    /// Directory this panel persists to.
    pub fn dir(&self) -> &PathBuf {
        &self.dir
    }

    /// Presets currently listed in the panel.
    pub fn presets(&self) -> &[Preset] {
        &self.presets
    }

    /// Persist the current camera state under `name` and list it immediately.
    /// Saving with an existing name replaces that entry.
    pub fn save_current(&mut self, name: &str, camera: &Camera) -> io::Result<()> {
        save_preset(&self.dir, name, camera)?;
        self.presets.retain(|p| p.name != name);
        self.presets.push(Preset {
            name: name.to_owned(),
            position: camera.position,
            scale: camera.scale,
        });
        Ok(())
    }

    /// Apply the named preset to the camera. Returns false — leaving the
    /// camera untouched — if no such preset is listed.
    pub fn load(&self, name: &str, camera: &mut Camera) -> bool {
        match self.presets.iter().find(|p| p.name == name) {
            Some(p) => {
                camera.position = p.position;
                camera.scale = p.scale;
                true
            }
            None => false,
        }
    }

    /// Remove the named preset from the panel list and from disk.
    pub fn delete(&mut self, name: &str) -> io::Result<()> {
        delete_preset(&self.dir, name)?;
        self.presets.retain(|p| p.name != name);
        Ok(())
    }

    pub fn show(&mut self, ui: &mut egui::Ui, camera: &mut Camera) {
        ui.heading("Presets");

        // Rows for the current list. Names are cloned so the list can be
        // mutated inside the loop.
        let names: Vec<String> = self.presets.iter().map(|p| p.name.clone()).collect();
        for name in &names {
            ui.horizontal(|ui| {
                ui.label(name);
                if ui.button("Load").clicked() {
                    self.load(name, camera);
                }
                if ui.button("Delete").clicked() {
                    if let Err(e) = self.delete(name) {
                        eprintln!("Preset delete failed: {e:?}");
                    }
                }
            });
        }

        // Save the current camera as a new preset.
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Save as: ");
            ui.add(egui::TextEdit::singleline(&mut self.new_name));
            let name = self.new_name.clone();
            if ui.add_enabled(!name.is_empty(), egui::Button::new("Save")).clicked() {
                if let Err(e) = self.save_current(&name, camera) {
                    eprintln!("Preset save failed: {e:?}");
                }
                self.new_name.clear();
            }
        });
    }
}
