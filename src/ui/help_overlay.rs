//! The keyboard & mouse reference (ticket 18).
//!
//! One table is the single source of strings for both surfaces: the compact
//! overlay anchored in the viewport's bottom-left corner (active mode + global
//! bindings) and the Help menu's window (all three groups; ticket 28 keeps it
//! as the only help menu entry). The overlay's visibility is a persisted
//! [`crate::ui::layout::LayoutState`] flag, visible on first run; its ×, `F1`,
//! `?` and `Esc` all toggle or dismiss it.

use eframe::egui;

/// Font size of the help window/overlay title, in points (ticket 28).
pub const HELP_WINDOW_TITLE_SIZE: f32 = 16.0;
/// Font size of a help section heading, in points (ticket 28).
pub const HELP_SECTION_TITLE_SIZE: f32 = 15.0;
/// Font size of one binding row, in points (ticket 28).
pub const HELP_ROW_SIZE: f32 = 12.0;

/// The size of one help heading, in points.
///
/// `egui::RichText` keeps its fields private, so the shared table and the
/// typography test in `tests/help_overlay_test.rs` meet on this tiny value
/// instead. Emphasis is a colour, not part of the size (see
/// [`window_heading_color`] and [`OVERLAY_HEADING_COLOR`]).
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HelpHeadingStyle {
    pub size: f32,
}

/// The size of a section heading (`3D view`, `2D view`, `Global`).
///
/// Public only so the typography test can prove the shared table reads
/// [`HELP_SECTION_TITLE_SIZE`] rather than a literal; the render path is the
/// sole caller in the app.
#[doc(hidden)]
pub fn section_heading_style() -> HelpHeadingStyle {
    HelpHeadingStyle {
        size: HELP_SECTION_TITLE_SIZE,
    }
}

/// Fixed heading colour for the compact overlay.
///
/// The overlay sits on its own fixed dark translucent fill in both themes, so
/// it cannot use the theme's strong text colour (black in the light theme).
/// This grey stays clearly brighter than the 185/155/150 row/footer greys.
pub const OVERLAY_HEADING_COLOR: egui::Color32 = egui::Color32::from_gray(235);

/// Heading colour for the Help window and its title.
///
/// The window sits on the theme background, so egui's strong text colour is the
/// right emphasis there: light in dark mode, dark in light mode. egui 0.37 has
/// no bold face, so the ticket's "bold" is this stronger text colour. It is
/// applied explicitly because an explicit [`egui::RichText::color`] wins over
/// [`egui::RichText::strong`]; passing this value keeps the emphasis real and
/// lets the typography test pin it.
#[doc(hidden)]
pub fn window_heading_color(visuals: &egui::Visuals) -> egui::Color32 {
    visuals.strong_text_color()
}

/// One documented input: the key or mouse gesture, and what it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HelpRow {
    pub input: &'static str,
    pub action: &'static str,
}

/// One titled group of rows. The overlay shows one view group plus Global; the
/// Help window shows every group in [`HELP_SECTIONS`] order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HelpSection {
    pub title: &'static str,
    pub rows: &'static [HelpRow],
}

/// The fly-mode key hint (ticket 46): the input token and its action, split the
/// same way as every other help row. Concatenated they spell the line shown in
/// the overlay.
pub const FLY_HELP_INPUT: &str = "C";
pub const FLY_HELP_ACTION: &str =
    "fly · WASD move · Q/E down/up · Shift boost · mouse look · Esc exit · Camera toggles it";

/// The streamline seed-mode line (ticket 50 ruling): a short row under the
/// click entry, with the off-state explanation living in the tooltip so the
/// compact overlay does not grow a paragraph.
pub const SEED_HELP_INPUT: &str = "Seed:";
pub const SEED_HELP_ACTION: &str = "auto | click";
pub const SEED_HELP_TOOLTIP: &str =
    "Off hides both seed sources and disables click seeding";

/// The left-click action selector line (ticket 60): the selector lives in the
/// Features panel, and each choice is a no-op while its feature is off.
pub const LEFT_CLICK_HELP_INPUT: &str = "Left click:";
pub const LEFT_CLICK_HELP_ACTION: &str = "seed streamline | add particles";
pub const LEFT_CLICK_HELP_TOOLTIP: &str =
    "Choose the left-click action in Features; each does nothing while its feature is off";

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
    HelpRow { input: "Click", action: "Per Left click setting" },
    HelpRow { input: LEFT_CLICK_HELP_INPUT, action: LEFT_CLICK_HELP_ACTION },
    HelpRow { input: "S", action: "Seed streamline at pointer" },
    HelpRow { input: "P", action: "Particle burst at pointer" },
    HelpRow { input: SEED_HELP_INPUT, action: SEED_HELP_ACTION },
    HelpRow { input: FLY_HELP_INPUT, action: FLY_HELP_ACTION },
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
    HelpRow { input: "Click", action: "Per Left click setting" },
    HelpRow { input: LEFT_CLICK_HELP_INPUT, action: LEFT_CLICK_HELP_ACTION },
    HelpRow { input: "S", action: "Seed streamline at pointer" },
    HelpRow { input: "P", action: "Particle burst at pointer" },
    HelpRow { input: SEED_HELP_INPUT, action: SEED_HELP_ACTION },
];

const ROWS_GLOBAL: &[HelpRow] = &[
    HelpRow {
        input: "Space",
        action: "Play/pause simulation; particles follow the clock",
    },
    HelpRow { input: "F1 / ?", action: "Toggle help" },
    HelpRow { input: "Ctrl+B / Ctrl+P", action: "Toggle panels (not while typing)" },
    HelpRow { input: "Ctrl+Shift+S", action: "Export screenshot" },
    HelpRow { input: "Ctrl+Shift+C", action: "Copy formula" },
    HelpRow { input: "Ctrl+Q", action: "Quit" },
    HelpRow { input: "Esc", action: "Close dialogs / dismiss help" },
];

/// The three groups, in window order. Both help surfaces read this table, so a
/// binding is spelled exactly once. A `static` (not a `const`) so the overlay
/// and the window can be proven to hand back the very same allocation.
pub static HELP_SECTIONS: &[HelpSection] = &[
    HelpSection { title: "3D view", rows: ROWS_3D },
    HelpSection { title: "2D view", rows: ROWS_2D },
    HelpSection { title: "Global", rows: ROWS_GLOBAL },
];

/// Every documented row, across all groups (the Help window's contents).
pub fn all_rows() -> Vec<HelpRow> {
    HELP_SECTIONS
        .iter()
        .flat_map(|section| section.rows.iter().copied())
        .collect()
}

/// The sections the viewport overlay shows: the active view's bindings, then
/// Global. The Help window keeps all three groups from [`HELP_SECTIONS`].
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

/// A help title, in the same 16 pt emphasized style on both help surfaces.
/// The caller passes the surface's heading colour: the fixed overlay grey or
/// the Help window's theme strong colour.
fn title_text(text: &str, heading_color: egui::Color32) -> egui::RichText {
    egui::RichText::new(text)
        .size(HELP_WINDOW_TITLE_SIZE)
        .color(heading_color)
}

fn table(
    ui: &mut egui::Ui,
    sections: &[&'static HelpSection],
    heading_color: egui::Color32,
) {
    let heading_style = section_heading_style();
    for (index, section) in sections.iter().enumerate() {
        if index > 0 {
            ui.add_space(4.0);
        }
        let heading = egui::RichText::new(section.title)
            .size(heading_style.size)
            .color(heading_color);
        // Wrap inside the width the table already has: a bigger heading must
        // never widen the overlay.
        ui.add(egui::Label::new(heading).wrap());
        egui::Grid::new(("help_grid", section.title))
            .num_columns(2)
            .spacing([12.0, 1.0])
            .show(ui, |ui| {
                for row in section.rows {
                    ui.label(
                        egui::RichText::new(row.input)
                            .size(HELP_ROW_SIZE)
                            .color(egui::Color32::from_gray(185)),
                    );
                    let action = ui.label(
                        egui::RichText::new(row.action)
                            .size(HELP_ROW_SIZE)
                            .color(egui::Color32::from_gray(155)),
                    );
                    if row.input == SEED_HELP_INPUT {
                        action.on_hover_text(SEED_HELP_TOOLTIP);
                    } else if row.input == LEFT_CLICK_HELP_INPUT {
                        action.on_hover_text(LEFT_CLICK_HELP_TOOLTIP);
                    }
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
/// global ones; the Help window carries the full table. Returns true when the
/// user dismissed it, so the caller can persist the new visibility.
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
                        ui.label(title_text("Mouse & keyboard", OVERLAY_HEADING_COLOR));
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
                    table(ui, &sections, OVERLAY_HEADING_COLOR);
                });
        });
    dismissed
}

/// The Help menu's window: the same table, all three groups. `open` is the
/// window's visibility flag (the menu action sets it, the × clears it).
pub fn help_window(ctx: &egui::Context, open: &mut bool) {
    let heading_color = window_heading_color(&ctx.style_of(ctx.theme()).visuals);
    egui::Window::new(title_text("Keyboard & mouse", heading_color))
        .open(open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            let all: Vec<&'static HelpSection> = HELP_SECTIONS.iter().collect();
            table(ui, &all, heading_color);
        });
}
