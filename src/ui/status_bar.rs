use eframe::egui;

use crate::app::StatusBarState;

/// Bottom status bar showing the parse message and instance count.
pub fn show(ui: &mut egui::Ui, status: &mut StatusBarState) {
    ui.horizontal(|ui| {
        let message = status.parse_message.clone();
        if message.is_empty() {
            ui.weak("ready");
        } else {
            ui.colored_label(egui::Color32::from_rgb(140, 200, 150), message);
        }

        ui.separator();

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.weak(format!("{} instances rendered", status.instance_count));
        });
    });
}
