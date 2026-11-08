//! UI-chrome style coverage: the persisted `AppStyle` record applied to egui.
//!
//! Moved out of `src/ui/style_panel.rs`'s inline module (ticket 13) without
//! changing a test or an assertion: `apply`, `AppStyle` and `SPACING_Y_RATIO`
//! are all public.

use eframe::egui;
use vector_field_explorer::ui::style_panel::{AppStyle, SPACING_Y_RATIO, apply};

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
