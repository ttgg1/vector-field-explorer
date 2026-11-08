//! Style panel: the UI-chrome knobs (theme, accent, corner rounding, spacing
//! and font size) that restyle every widget, plus persistence of that record
//! through eframe's app-state storage.
//!
//! Scope ruling (ledger 2026-11-08): "style" is UI chrome per
//! `.scratch/field-viewport/spec.md` L32-33/L98-101 — it is *not* the field
//! knobs (colormap/density/arrow scale, ticket 03) and not part of the
//! viewmodel `State`. Only this record is persisted (`eframe::APP_KEY`);
//! formula/mode persistence stays out of scope. A corrupt or absent stored
//! record falls back to the defaults: cosmetic UI state must never brick
//! startup (camera presets stay fail-fast for data integrity).

use eframe::egui;
use serde::{Deserialize, Serialize};

/// Vertical item spacing as a fraction of the horizontal one, chosen so the
/// defaults (spacing 8) reproduce egui's own `item_spacing = (8, 3)`.
pub const SPACING_Y_RATIO: f32 = 3.0 / 8.0;

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

/// Map the style record onto egui's global styles (both themes, so switching
/// theme later never resurrects stale accents/spacing). Idempotent: it runs at
/// the start of every frame, so applying the same record twice must leave the
/// context identical.
pub fn apply(ctx: &egui::Context, style: &AppStyle) {
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

/// Draw the collapsible "Style" section. Returns `true` when a control moved
/// so the caller can re-apply the same frame.
pub fn show(ui: &mut egui::Ui, style: &mut AppStyle) -> bool {
    let mut changed = false;
    egui::CollapsingHeader::new("Style")
        .default_open(false)
        .show(ui, |ui| {
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
                .add(egui::Slider::new(&mut style.rounding, 0.0..=20.0).text("Rounding"))
                .changed();
            changed |= ui
                .add(egui::Slider::new(&mut style.spacing, 2.0..=24.0).text("Spacing"))
                .changed();
            changed |= ui
                .add(egui::Slider::new(&mut style.font_size, 8.0..=24.0).text("Font size"))
                .changed();
        });
    changed
}

/// Load the style record, falling back to the defaults when storage is absent,
/// the key is missing, or the stored RON is corrupt. Never panics.
pub fn load(storage: Option<&dyn eframe::Storage>) -> AppStyle {
    storage
        .and_then(|storage| eframe::get_value(storage, eframe::APP_KEY))
        .unwrap_or_default()
}

/// Persist the style record under eframe's app key.
pub fn save(storage: &mut dyn eframe::Storage, style: &AppStyle) {
    eframe::set_value(storage, eframe::APP_KEY, style);
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::Storage as _;

    /// In-memory stand-in for eframe's disk/localstorage-backed storage: the
    /// persistence helpers only need `get_string`/`set_string`.
    #[derive(Default)]
    struct MemoryStorage {
        values: std::collections::HashMap<String, String>,
    }

    impl eframe::Storage for MemoryStorage {
        fn get_string(&self, key: &str) -> Option<String> {
            self.values.get(key).cloned()
        }

        fn set_string(&mut self, key: &str, value: String) {
            self.values.insert(key.to_owned(), value);
        }

        fn remove_string(&mut self, key: &str) {
            self.values.remove(key);
        }

        fn flush(&mut self) {}
    }

    fn sample_style() -> AppStyle {
        AppStyle {
            theme_dark: false,
            accent: egui::Color32::from_rgb(12, 34, 56),
            rounding: 9.0,
            spacing: 11.0,
            font_size: 15.0,
        }
    }

    #[test]
    fn style_round_trips_through_eframe_storage() {
        let style = sample_style();
        let mut storage = MemoryStorage::default();
        save(&mut storage, &style);
        assert_eq!(load(Some(&storage)), style);
    }

    #[test]
    fn absent_storage_falls_back_to_defaults() {
        assert_eq!(load(None), AppStyle::default());
    }

    #[test]
    fn corrupt_stored_value_falls_back_to_defaults() {
        let mut storage = MemoryStorage::default();
        storage.set_string(eframe::APP_KEY, "this is not ron".to_owned());
        assert_eq!(load(Some(&storage)), AppStyle::default());
    }

    #[test]
    fn apply_maps_dark_style_onto_the_context() {
        let ctx = egui::Context::default();
        let style = sample_style();
        let mut dark = style.clone();
        dark.theme_dark = true;
        apply(&ctx, &dark);

        assert_eq!(ctx.theme(), egui::Theme::Dark);
        let active = ctx.global_style();
        assert!(active.visuals.dark_mode);
        assert_eq!(active.visuals.selection.bg_fill, dark.accent);
        assert_eq!(active.spacing.item_spacing, egui::vec2(dark.spacing, dark.spacing * SPACING_Y_RATIO));
        assert_eq!(active.text_styles[&egui::TextStyle::Body].size, dark.font_size);
        assert_eq!(active.visuals.window_corner_radius, egui::CornerRadius::same(9));
        // The light theme is configured too, and stays a light theme.
        let light = ctx.style_of(egui::Theme::Light);
        assert!(!light.visuals.dark_mode);
        assert_eq!(light.visuals.selection.bg_fill, dark.accent);
    }

    #[test]
    fn apply_maps_light_style_onto_the_context() {
        let ctx = egui::Context::default();
        let mut light = sample_style();
        light.theme_dark = false;
        apply(&ctx, &light);

        assert_eq!(ctx.theme(), egui::Theme::Light);
        let active = ctx.global_style();
        assert!(!active.visuals.dark_mode);
        assert_eq!(active.visuals.selection.bg_fill, light.accent);
        assert_eq!(active.spacing.item_spacing, egui::vec2(light.spacing, light.spacing * SPACING_Y_RATIO));
        assert_eq!(active.text_styles[&egui::TextStyle::Body].size, light.font_size);
        // The dark theme is configured too, and stays a dark theme.
        let dark = ctx.style_of(egui::Theme::Dark);
        assert!(dark.visuals.dark_mode);
    }

    #[test]
    fn apply_is_idempotent() {
        let ctx = egui::Context::default();
        let style = AppStyle::default();
        apply(&ctx, &style);
        let once = (*ctx.global_style()).clone();
        apply(&ctx, &style);
        assert_eq!(*ctx.global_style(), once);
    }
}
