use eframe::egui;

use crate::render::Camera;
use crate::ui::presets::{PresetPanel, default_preset_dir};

/// Directory where the app persists camera presets. Kept in one place so the
/// panel and any export helper always agree (preset-persistence plan Task 3).
pub fn presets_data_dir() -> std::path::PathBuf {
    default_preset_dir()
}

/// Application root state: formula bar, camera viewport, preset manager.
pub struct VfApp {
    pub formula: String,
    pub parse_message: String,
    pub camera: Camera,
    pub presets: PresetPanel,
}

impl VfApp {
    pub fn new() -> Self {
        Self {
            formula: "sin(x) * cos(y)".to_owned(),
            parse_message: "OK".to_owned(),
            camera: Camera::default(),
            presets: PresetPanel::new_with_dir(presets_data_dir())
                .unwrap_or_else(|_| PresetPanel::new()),
        }
    }
}

impl Default for VfApp {
    fn default() -> Self {
        Self::new()
    }
}

impl eframe::App for VfApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Top formula bar.
        egui::Panel::top("formula_bar").show(ui, |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.formula)
                    .desired_width(f32::INFINITY),
            );
        });

        // Bottom status bar.
        egui::Panel::bottom("status_bar").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.parse_message);
                ui.separator();
                ui.label(format!(
                    "camera at ({:.1}, {:.1}), scale {:.2}",
                    self.camera.position[0], self.camera.position[1], self.camera.scale
                ));
            });
        });

        // Presets panel on the right.
        egui::Panel::right("presets")
            .resizable(true)
            .min_size(200.0)
            .max_size(400.0)
            .show(ui, |ui| {
                self.presets.show(ui, &mut self.camera);
            });

        // Central viewport placeholder (wgpu field rendering arrives in a later task).
        egui::CentralPanel::default_margins().show(ui, |ui| {
            let rect = ui.max_rect().shrink(2.0);
            ui.painter()
                .rect_filled(rect, 8.0, egui::Color32::from_rgb(30, 33, 41));
            ui.centered_and_justified(|ui| {
                ui.weak("placeholder — wgpu field rendering arrives in a later task");
            });
        });
    }
}
