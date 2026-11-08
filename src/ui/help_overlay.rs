//! The keyboard & mouse reference (ticket 18), behind the one help surface
//! (ticket 28).
//!
//! [`HELP_SECTIONS`] is the single source of strings: the compact overlay
//! anchored in the viewport's bottom-left corner renders the active view's
//! group plus Global. The overlay's visibility is a persisted
//! [`crate::ui::layout::LayoutState`] flag, visible on first run; the overlay's
//! ×, the Help > Keyboard & mouse entry, `F1`, `?` and `Esc` all toggle or
//! dismiss it.

use eframe::egui;

/// One documented input: the key or mouse gesture, and what it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HelpRow {
    pub input: &'static str,
    pub action: &'static str,
}

/// One titled group of rows. The overlay shows one view group plus Global;
/// [`HELP_SECTIONS`] keeps every group in table order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HelpSection {
    pub title: &'static str,
    pub rows: &'static [HelpRow],
}

const ROWS_3D: &[HelpRow] = &[
    HelpRow { input: "Left-drag", action: "Orbit" },
    HelpRow { input: "Right-drag", action: "Orbit" },
    HelpRow { input: "Middle-drag", action: "Pan" },
    HelpRow { input: "Shift + right-drag", action: "Pan" },
    HelpRow { input: "Wheel / pinch", action: "Zoom" },
    HelpRow { input: "Double-click", action: "Reset view" },
    HelpRow { input: "Arrow keys", action: "Orbit" },
    HelpRow { input: "Shift + arrows", action: "Pan" },
    HelpRow { input: "+ / -", action: "Zoom in / out" },
    HelpRow { input: "R", action: "Reset view" },
    HelpRow { input: "F", action: "Fit domain" },
    HelpRow { input: "Click", action: "Seed streamlines" },
];

const ROWS_2D: &[HelpRow] = &[
    HelpRow { input: "Left / right-drag", action: "Pan" },
    HelpRow { input: "Middle-drag", action: "Pan" },
    HelpRow { input: "Wheel / pinch", action: "Zoom" },
    HelpRow { input: "Double-click", action: "Reset view" },
    HelpRow { input: "Arrow keys", action: "Pan" },
    HelpRow { input: "Shift + arrows", action: "Pan faster" },
    HelpRow { input: "+ / -", action: "Zoom in / out" },
    HelpRow { input: "R", action: "Reset view" },
    HelpRow { input: "F", action: "Fit domain" },
    HelpRow { input: "Click", action: "Seed streamlines" },
];

const ROWS_GLOBAL: &[HelpRow] = &[
    HelpRow { input: "F1 / ?", action: "Toggle help" },
    HelpRow { input: "Ctrl+B / Ctrl+P", action: "Toggle panels (not while typing)" },
    HelpRow { input: "Ctrl+Shift+S", action: "Export screenshot" },
    HelpRow { input: "Ctrl+Shift+C", action: "Copy formula" },
    HelpRow { input: "Ctrl+Q", action: "Quit" },
    HelpRow { input: "Esc", action: "Close dialogs / dismiss help" },
];

/// The three groups, in table order. The overlay reads this table, so a
/// binding is spelled exactly once. A `static` (not a `const`) so the overlay
/// and the tests can be proven to hand back the very same allocation.
pub static HELP_SECTIONS: &[HelpSection] = &[
    HelpSection { title: "3D view", rows: ROWS_3D },
    HelpSection { title: "2D view", rows: ROWS_2D },
    HelpSection { title: "Global", rows: ROWS_GLOBAL },
];

/// Every documented row, across all groups.
pub fn all_rows() -> Vec<HelpRow> {
    HELP_SECTIONS
        .iter()
        .flat_map(|section| section.rows.iter().copied())
        .collect()
}

/// The sections the viewport overlay shows: the active view's bindings, then
/// Global.
pub fn sections_for(mode_3d: bool) -> [&'static HelpSection; 2] {
    let active = if mode_3d {
        &HELP_SECTIONS[0]
    } else {
        &HELP_SECTIONS[1]
    };
    [active, &HELP_SECTIONS[2]]
}

/// The overlay's flattened rows for the current mode.
pub fn rows_for(mode_3d: bool) -> Vec<HelpRow> {
    sections_for(mode_3d)
        .iter()
        .flat_map(|section| section.rows.iter().copied())
        .collect()
}

/// The formula-syntax reminder, kept apart from the binding table because it
/// is not an input. Both help surfaces show it under the Global group.
pub const FORMULA_HINT: &str = "Formula: (fx, fy) or (fx, fy, fz), e.g. (-y, x)";

fn table(ui: &mut egui::Ui, sections: &[&'static HelpSection]) {
    for (index, section) in sections.iter().enumerate() {
        if index > 0 {
            ui.add_space(4.0);
        }
        ui.label(
            egui::RichText::new(section.title)
                .strong()
                .size(11.0)
                .color(egui::Color32::from_gray(200)),
        );
        egui::Grid::new(("help_grid", section.title))
            .num_columns(2)
            .spacing([12.0, 1.0])
            .show(ui, |ui| {
                for row in section.rows {
                    ui.label(
                        egui::RichText::new(row.input)
                            .size(12.0)
                            .color(egui::Color32::from_gray(185)),
                    );
                    ui.label(
                        egui::RichText::new(row.action)
                            .size(12.0)
                            .color(egui::Color32::from_gray(155)),
                    );
                    ui.end_row();
                }
            });
    }
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(FORMULA_HINT)
            .size(11.0)
            .color(egui::Color32::from_gray(150)),
    );
}

/// Draw the compact overlay anchored at `anchor` (the viewport's bottom-left
/// corner in global coordinates). It shows the active view's bindings plus the
/// global ones. Returns true when the user dismissed it, so the caller can
/// persist the new visibility.
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
                            egui::RichText::new("Mouse & keyboard")
                                .strong()
                                .color(egui::Color32::from_gray(200)),
                        );
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                dismissed = ui
                                    .small_button("×")
                                    .on_hover_text("Hide the help overlay (F1)")
                                    .clicked();
                            },
                        );
                    });
                    let sections = sections_for(mode_3d);
                    table(ui, &sections);
                });
        });
    dismissed
}
