use eframe::egui;
use std::path::{Path, PathBuf};
use std::fs;
use crate::export::{load_presets, save_preset};
use crate::render::Camera;

/// UI panel for managing camera presets.
pub struct PresetPanel {
    /// Directory where presets.json lives.
    dir: PathBuf,
    /// Cached list of presets.
    presets: Vec<crate::export::Preset>,
}

impl PresetPanel {
    pub fn new() -> Self {
        // Determine app data directory. Use XDG config if available, otherwise HOME.
        let dir = match std::env::var("XDG_CONFIG_HOME") {
            Ok(p) => PathBuf::from(p).join("vector-field-explorer"),
            Err(_) => {
                let home = dirs_next::home_dir().unwrap_or_else(|| PathBuf::from("."));
                home.join(".config/vector-field-explorer")
            }
        };
        // Ensure directory exists
        let _ = fs::create_dir_all(&dir);
        let presets = load_presets(&dir).unwrap_or_default();
        Self { dir, presets }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, camera: &mut Camera) {
        ui.heading("Presets");
        // List presets with Load/Delete buttons
        for preset in &self.presets {
            ui.horizontal(|ui| {
                ui.label(&preset.name);
                if ui.button("Load").clicked() {
                    camera.position = preset.position;
                    camera.scale = preset.scale;
                }
                if ui.button("Delete").clicked() {
                    self.presets.retain(|p| p.name != preset.name);
                    let _ = save_preset(&self.dir, &preset.name, camera);
                }
            });
        }
        // Save current as new preset
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Save as: ");
            let mut name = String::new();
            if ui.text_edit_singleline(&mut name).changed() {
                // nothing
            }
            if ui.button("Save").clicked() && !name.is_empty() {
                let _ = save_preset(&self.dir, &name, camera);
                self.presets.push(crate::export::Preset { name: name.clone(), position: camera.position, scale: camera.scale });
            }
        });
    }
}
