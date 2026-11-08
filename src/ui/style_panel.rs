//! Style panel: the UI-chrome knobs (theme, accent, corner rounding, spacing
//! and font size) that restyle every widget, plus persistence of that record
//! through eframe's app-state storage.
//!
//! Scope note (updated for ticket 14): "style" is still only UI chrome — the
//! field knobs live in the viewmodel `State`. Ticket 14 wraps this record in
//! the app's [`crate::persistence::AppSettings`] envelope under the same
//! eframe key, alongside layout and view state. A corrupt or absent record
//! falls back to the defaults, and [`AppStyle::sanitized`] clamps stored values
//! into the Settings slider ranges before they reach egui: cosmetic UI state
//! must never brick startup (camera presets stay fail-fast for data
//! integrity).

use eframe::egui;
use serde::{Deserialize, Serialize};

/// Vertical item spacing as a fraction of the horizontal one, chosen so the
/// defaults (spacing 8) reproduce egui's own `item_spacing = (8, 3)`.
pub const SPACING_Y_RATIO: f32 = 3.0 / 8.0;

/// Settings-slider bounds, shared by the popover and [`AppStyle::sanitized`]
/// so a stored record can never hold a value the UI itself cannot produce.
pub const MIN_ROUNDING: f32 = 0.0;
pub const MAX_ROUNDING: f32 = 20.0;
pub const MIN_SPACING: f32 = 2.0;
pub const MAX_SPACING: f32 = 24.0;
pub const MIN_FONT_SIZE: f32 = 8.0;
pub const MAX_FONT_SIZE: f32 = 24.0;

/// The persisted UI-chrome style record. Field names are distinct from the
/// repo's three existing `Preset` types; this one only carries look-and-feel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppStyle {
    /// Dark theme when true, light theme when false.
    pub theme_dark: bool,
    /// Selection/accent color.
    pub accent: egui::Color32,
    /// Corner rounding in points, applied to windows, menus and widgets.
    pub rounding: f32,
    /// Horizontal item spacing in points (vertical keeps [`SPACING_Y_RATIO`]).
    pub spacing: f32,
    /// Base text size in points (Body/Button/Monospace).
    pub font_size: f32,
}

impl Default for AppStyle {
    fn default() -> Self {
        Self {
            theme_dark: true,
            // Material teal #009688.
            accent: egui::Color32::from_rgb(0, 150, 136),
            rounding: 6.0,
            spacing: 8.0,
            font_size: 13.0,
        }
    }
}

impl AppStyle {
    /// Clamp a stored record into what the Settings sliders can produce. A
    /// non-finite number takes the default before clamping (review I-3): a
    /// corrupt record must never hand egui an infinite font size or spacing.
    pub fn sanitized(mut self) -> Self {
        let defaults = Self::default();
        self.rounding = finite_or(self.rounding, defaults.rounding).clamp(MIN_ROUNDING, MAX_ROUNDING);
        self.spacing = finite_or(self.spacing, defaults.spacing).clamp(MIN_SPACING, MAX_SPACING);
        self.font_size =
            finite_or(self.font_size, defaults.font_size).clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
        self
    }
}

/// `value` when finite, `fallback` otherwise.
fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

/// Map the style record onto egui's global styles (both themes, so switching
/// theme later never resurrects stale accents/spacing). Idempotent: it runs at
/// the start of every frame, so applying the same record twice must leave the
/// context identical.
pub fn apply(ctx: &egui::Context, style: &AppStyle) {
    // Defensive: loaded records are sanitized, but `apply` also runs on user
    // edits and must never feed egui a value outside the sliders (review I-3).
    let style = style.clone().sanitized();
    let rounding = egui::CornerRadius::same(
        style.rounding.round().clamp(0.0, u8::MAX as f32) as u8,
    );

    // Pin the active theme explicitly instead of following the OS preference:
    // the style record is the single source of truth across restarts.
    ctx.set_theme(if style.theme_dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    });

    // Configure both stored themes so a later theme switch (or a popup created
    // under the other theme) never resurrects stale accent/spacing/rounding.
    ctx.all_styles_mut(|style_ctx| {
        let mut visuals = if style_ctx.visuals.dark_mode {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        visuals.selection.bg_fill = style.accent;
        visuals.hyperlink_color = style.accent;
        visuals.window_corner_radius = rounding;
        visuals.menu_corner_radius = rounding;
        visuals.widgets.noninteractive.corner_radius = rounding;
        visuals.widgets.inactive.corner_radius = rounding;
        visuals.widgets.hovered.corner_radius = rounding;
        visuals.widgets.active.corner_radius = rounding;
        visuals.widgets.open.corner_radius = rounding;
        style_ctx.visuals = visuals;

        style_ctx.spacing.item_spacing =
            egui::vec2(style.spacing, style.spacing * SPACING_Y_RATIO);

        // Base size for body text; headings and small text scale with it.
        let font_size = style.font_size.max(1.0);
        style_ctx
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(font_size));
        style_ctx
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(font_size));
        style_ctx
            .text_styles
            .insert(egui::TextStyle::Monospace, egui::FontId::monospace(font_size));
        style_ctx.text_styles.insert(
            egui::TextStyle::Heading,
            egui::FontId::proportional(font_size * 1.3),
        );
        style_ctx.text_styles.insert(
            egui::TextStyle::Small,
            egui::FontId::proportional(font_size * 0.85),
        );
    });
}

/// Draw the bare appearance controls, without a collapsing header, for the
/// Settings popover (ticket 14 moved them out of the sidebar). Returns `true`
/// when a control moved so the caller can re-apply the same frame.
pub fn show_controls(ui: &mut egui::Ui, style: &mut AppStyle) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label("Theme");
        changed |= ui
            .selectable_value(&mut style.theme_dark, false, "Light")
            .changed();
        changed |= ui
            .selectable_value(&mut style.theme_dark, true, "Dark")
            .changed();
    });
    ui.horizontal(|ui| {
        ui.label("Accent");
        changed |= ui.color_edit_button_srgba(&mut style.accent).changed();
    });
    changed |= ui
        .add(egui::Slider::new(&mut style.rounding, MIN_ROUNDING..=MAX_ROUNDING).text("Rounding"))
        .changed();
    changed |= ui
        .add(egui::Slider::new(&mut style.spacing, MIN_SPACING..=MAX_SPACING).text("Spacing"))
        .changed();
    changed |= ui
        .add(egui::Slider::new(&mut style.font_size, MIN_FONT_SIZE..=MAX_FONT_SIZE).text("Font size"))
        .changed();
    changed
}

