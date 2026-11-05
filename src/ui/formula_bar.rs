use eframe::egui;

use crate::app::StatusBarState;

/// Top horizontal formula bar: text edit + 2D/3D toggle buttons (stubs).
pub fn show(
    ui: &mut egui::Ui,
    formula: &mut String,
    mode3d: &mut bool,
    status: &mut StatusBarState, // formula parsing wires into this in a later task
) {
    _ = status;
    ui.horizontal(|ui| {
        ui.label("f(x, y) =");

        let response = ui.add(
            egui::TextEdit::singleline(formula)
                .hint_text("e.g. sin(x) + cos(y)")
                .desired_width(ui.available_width() - 120.0),
        );
        _ = &response;

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .selectable_label(*mode3d, "3D")
                .clicked()
            {
                *mode3d = true;
            }
            if ui
                .selectable_label(!*mode3d, "2D")
                .clicked()
            {
                *mode3d = false;
            }
        });
    });
}
