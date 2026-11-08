//! Dismissible help overlay in the viewport's bottom-left corner (ticket 14).
//!
//! The text is deliberately terse and grey so it never competes with the
//! field: one line per gesture plus the formula-syntax reminder. Its
//! visibility is a persisted [`crate::ui::layout::LayoutState`] flag, visible
//! on first run; the × is the discoverable way to hide it.

use eframe::egui;

/// The overlay's lines for the current mode. 2D lists the pan/zoom gestures;
/// 3D adds orbit and the right-drag pan. Both add the formula reminder and the
/// shortcut rule (which now excludes text fields except Ctrl+Q).
pub fn help_lines(mode_3d: bool) -> &'static [&'static str] {
    if mode_3d {
        &[
            "Drag: orbit",
            "Right/middle-drag: pan",
            "Wheel/pinch: zoom",
            "Click: seed streamlines",
            "Formula: (fx, fy, fz), e.g. (-y, x, 0.5)",
            "Ctrl+B / Ctrl+P: toggle panels (not while typing)",
            "Ctrl+Q: quit; F1: toggle this help",
        ]
    } else {
        &[
            "Drag: pan",
            "Wheel/pinch: zoom",
            "Click: seed streamlines",
            "Formula: (fx, fy), e.g. (-y, x)",
            "Ctrl+B / Ctrl+P: toggle panels (not while typing)",
            "Ctrl+Q: quit; F1: toggle this help",
        ]
    }
}

/// Draw the overlay anchored at `anchor` (the viewport's bottom-left corner in
/// global coordinates). Returns true when the user dismissed it, so the caller
/// can persist the new visibility.
pub fn show(ctx: &egui::Context, anchor: egui::Pos2, mode_3d: bool) -> bool {
    let mut dismissed = false;
    egui::Area::new(egui::Id::unique("help_overlay"))
        .fixed_pos(anchor)
        .pivot(egui::Align2::LEFT_BOTTOM)
        .order(egui::Order::Foreground)
        .interactable(true)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_black_alpha(140))
                .inner_margin(egui::Margin::same(8))
                .corner_radius(egui::CornerRadius::same(6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Mouse & formula")
                                .strong()
                                .color(egui::Color32::from_gray(200)),
                        );
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                dismissed = ui
                                    .small_button("×")
                                    .on_hover_text("Hide the help overlay")
                                    .clicked();
                            },
                        );
                    });
                    for line in help_lines(mode_3d) {
                        ui.label(
                            egui::RichText::new(*line)
                                .size(12.0)
                                .color(egui::Color32::from_gray(175)),
                        );
                    }
                });
        });
    dismissed
}
