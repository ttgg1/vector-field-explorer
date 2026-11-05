use eframe::egui;

use crate::app::Controls;

const COLORMAPS: [&str; 4] = ["viridis", "inferno", "turbo", "grayscale"];

/// Resizable left sidebar with stubbed collapsing sections.
/// All controls register into `Controls`; behavior comes in later tasks.
pub fn show(ui: &mut egui::Ui, c: &mut Controls) {
    ui.heading("Vector Field Explorer");
    ui.separator();

    egui::CollapsingHeader::new("Display")
        .default_open(true)
        .show(ui, |ui| {
            ui.add(egui::Slider::new(&mut c.density, 8..=128).text("Grid density"));
            ui.add(
                egui::Slider::new(&mut c.arrow_scale, 0.1..=4.0)
                    .text("Arrow scale"),
            );
            ui.checkbox(&mut c.normalize, "Normalize vectors");
        });

    egui::CollapsingHeader::new("Coloring")
        .default_open(true)
        .show(ui, |ui| {
            egui::ComboBox::from_label("Colormap")
                .selected_text(c.colormap.clone())
                .show_ui(ui, |ui| {
                    for option in COLORMAPS {
                        ui.selectable_value(
                            &mut c.colormap,
                            option.to_owned(),
                            option,
                        );
                    }
                });
            ui.horizontal(|ui| {
                ui.label("Color range");
                ui.add(
                    egui::DragValue::new(&mut c.color_range.0)
                        .speed(0.05)
                        .prefix("min "),
                );
                ui.add(
                    egui::DragValue::new(&mut c.color_range.1)
                        .speed(0.05)
                        .prefix("max "),
                );
            });
        });

    egui::CollapsingHeader::new("Features")
        .default_open(false)
        .show(ui, |ui| {
            ui.checkbox(&mut c.features.arrows, "Arrow glyphs");
            ui.checkbox(&mut c.features.streamlines, "Streamlines");
            ui.checkbox(&mut c.features.equipotentials, "Equipotential lines");
        });

    egui::CollapsingHeader::new("Domain")
        .default_open(false)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("x");
                ui.add(
                    egui::DragValue::new(&mut c.extent.x_min)
                        .speed(0.1)
                        .prefix("min "),
                );
                ui.add(
                    egui::DragValue::new(&mut c.extent.x_max)
                        .speed(0.1)
                        .prefix("max "),
                );
            });
            ui.horizontal(|ui| {
                ui.label("y");
                ui.add(
                    egui::DragValue::new(&mut c.extent.y_min)
                        .speed(0.1)
                        .prefix("min "),
                );
                ui.add(
                    egui::DragValue::new(&mut c.extent.y_max)
                        .speed(0.1)
                        .prefix("max "),
                );
            });
        });
}
