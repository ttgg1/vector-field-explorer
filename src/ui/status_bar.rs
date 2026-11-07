//! Bottom status bar: the formula parse state and the instance count of the
//! snapshot currently being painted.

use eframe::egui;

use crate::app::StatusBarState;

/// Draw the status bar: green-ish "ready" or the parse error in red, plus
/// how many arrow instances the current snapshot holds.
pub fn show(ui: &mut egui::Ui, status: &StatusBarState) {
    ui.horizontal(|ui| {
        if status.parse_message.is_empty() {
            ui.weak("ready");
        } else {
            ui.colored_label(egui::Color32::from_rgb(230, 70, 70), status.parse_message.clone());
        }

        ui.separator();

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.weak(format!("{} instances rendered", status.instance_count));
        });
    });
}
