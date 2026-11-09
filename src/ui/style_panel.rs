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

/// Edge length of one accent-preset swatch, in points.
const ACCENT_SWATCH_SIZE: f32 = 20.0;

/// One named accent colour offered above the custom picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccentPreset {
    /// Human-readable name, also used as the accessible label and tooltip.
    pub name: &'static str,
    /// Opaque sRGB colour written into [`AppStyle::accent`].
    pub color: egui::Color32,
}

/// The fixed accent palette. Material 700-series shades: distinguishable from
/// each other, and dark enough to keep white check marks and rings legible.
pub const ACCENT_PRESETS: [AccentPreset; 8] = [
    AccentPreset {
        name: "Teal",
        color: egui::Color32::from_rgb(0, 150, 136),
    },
    AccentPreset {
        name: "Blue",
        color: egui::Color32::from_rgb(25, 118, 210),
    },
    AccentPreset {
        name: "Indigo",
        color: egui::Color32::from_rgb(63, 81, 181),
    },
    AccentPreset {
        name: "Purple",
        color: egui::Color32::from_rgb(123, 31, 162),
    },
    AccentPreset {
        name: "Pink",
        color: egui::Color32::from_rgb(194, 24, 91),
    },
    AccentPreset {
        name: "Red",
        color: egui::Color32::from_rgb(211, 47, 47),
    },
    AccentPreset {
        name: "Orange",
        color: egui::Color32::from_rgb(239, 108, 0),
    },
    AccentPreset {
        name: "Green",
        color: egui::Color32::from_rgb(56, 142, 60),
    },
];

/// Index of the preset whose colour is exactly `accent`, or `None` for a
/// custom colour picked through the free-form picker.
pub fn selected_preset(accent: egui::Color32) -> Option<usize> {
    ACCENT_PRESETS
        .iter()
        .position(|preset| preset.color == accent)
}

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

/// sRGB channel to linear light, the WCAG relative-luminance transfer curve.
fn linearize(channel: u8) -> f32 {
    let channel = channel as f32 / 255.0;
    if channel <= 0.04045 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

/// WCAG relative luminance of an opaque sRGB colour.
fn relative_luminance(color: egui::Color32) -> f32 {
    0.2126 * linearize(color.r())
        + 0.7152 * linearize(color.g())
        + 0.0722 * linearize(color.b())
}

/// A readable text colour for selected text on `accent`.
///
/// Compares the WCAG contrast of white (`1.05 / (L + 0.05)`) against pure
/// black (`(L + 0.05) / 0.05`) and returns whichever wins. The chosen colour is
/// exactly the one the formula measured, so every accent clears 4.5:1 (the
/// crossover sits at ~4.58:1) and selected labels stay legible in both themes.
pub fn selection_text_color(accent: egui::Color32) -> egui::Color32 {
    let luminance = relative_luminance(accent);
    let on_white = 1.05 / (luminance + 0.05);
    let on_black = (luminance + 0.05) / 0.05;
    if on_white >= on_black {
        egui::Color32::WHITE
    } else {
        egui::Color32::BLACK
    }
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
        // egui derives selected text from `selection.stroke`; the theme
        // default (pale blue / dark blue) is unreadable on several accents.
        visuals.selection.stroke = egui::Stroke::new(1.0, selection_text_color(style.accent));
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

/// What one [`show_controls`] pass reports back to the owning popover.
pub struct StyleControls {
    /// A control moved, so the caller re-applies the style this frame.
    pub changed: bool,
    /// The nested color-picker popup's egui id. The picker is a floating area
    /// of its own, so the popover that owns these controls needs its id to
    /// tell "the user is using the picker" from "the user clicked outside".
    pub accent_popup_id: egui::Id,
}

/// Draw the fixed accent swatches above the custom picker. The row wraps, so
/// the eight swatches stay inside a narrow popover. A custom accent simply
/// leaves every swatch unselected.
fn show_accent_presets(ui: &mut egui::Ui, style: &mut AppStyle) -> bool {
    let selected = selected_preset(style.accent);
    let enabled = ui.is_enabled();
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        for (index, preset) in ACCENT_PRESETS.iter().enumerate() {
            let (rect, response) = ui.allocate_exact_size(
                egui::vec2(ACCENT_SWATCH_SIZE, ACCENT_SWATCH_SIZE),
                egui::Sense::click(),
            );
            let is_selected = selected == Some(index);

            let painter = ui.painter();
            let corner = egui::CornerRadius::same(3);
            painter.rect_filled(rect, corner, preset.color);
            // A mid-grey border keeps the swatch readable on both the dark and
            // the light panel fill instead of blending into either one.
            painter.rect_stroke(
                rect,
                corner,
                egui::Stroke::new(1.0, egui::Color32::from_gray(120)),
                egui::StrokeKind::Inside,
            );
            if is_selected {
                // White ring plus a dark inner edge: contrast against both the
                // panel and the swatch fill, in either theme.
                painter.rect_stroke(
                    rect,
                    corner,
                    egui::Stroke::new(2.0, egui::Color32::WHITE),
                    egui::StrokeKind::Inside,
                );
                painter.rect_stroke(
                    rect.shrink(2.0),
                    corner,
                    egui::Stroke::new(1.0, egui::Color32::from_gray(20)),
                    egui::StrokeKind::Inside,
                );
                let check = egui::Stroke::new(2.0, egui::Color32::WHITE);
                let start = rect.lerp_inside(egui::vec2(0.24, 0.52));
                let elbow = rect.lerp_inside(egui::vec2(0.42, 0.70));
                let end = rect.lerp_inside(egui::vec2(0.76, 0.30));
                painter.line_segment([start, elbow], check);
                painter.line_segment([elbow, end], check);
            }

            if response.clicked() {
                style.accent = preset.color;
                changed = true;
            }
            let response = response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(preset.name);
            response.widget_info(|| {
                egui::WidgetInfo::selected(egui::Role::Button, enabled, is_selected, preset.name)
            });
        }
    });
    changed
}

/// Draw the bare appearance controls, without a collapsing header, for the
/// Settings popover (ticket 14 moved them out of the sidebar).
pub fn show_controls(ui: &mut egui::Ui, style: &mut AppStyle) -> StyleControls {
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
    changed |= show_accent_presets(ui, style);
    // `color_edit_button_srgba` anchors a popup to this button, deriving the
    // popup id from the row ui's next auto id *before* it allocates the
    // swatch. Reading that id here is what lets the owning popover recognise
    // clicks on the picker as inside (ticket 41).
    let mut accent_popup_id = egui::Id::NULL;
    ui.horizontal(|ui| {
        ui.label("Accent");
        accent_popup_id = ui.auto_id_with("popup");
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
    StyleControls {
        changed,
        accent_popup_id,
    }
}

