//! UI-chrome style coverage: the persisted `AppStyle` record applied to egui.
//!
//! Moved out of `src/ui/style_panel.rs`'s inline module (ticket 13) without
//! changing a test or an assertion: `apply`, `AppStyle` and `SPACING_Y_RATIO`
//! are all public.

use eframe::egui;
use vector_field_explorer::ui::style_panel::{
    ACCENT_PRESETS, AppStyle, SPACING_Y_RATIO, apply, selected_preset, selection_text_color,
    show_controls,
};

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

/// WCAG relative luminance of an opaque sRGB colour.
fn relative_luminance(color: egui::Color32) -> f32 {
    fn linear(channel: u8) -> f32 {
        let channel = channel as f32 / 255.0;
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * linear(color.r()) + 0.7152 * linear(color.g()) + 0.0722 * linear(color.b())
}

fn contrast_ratio(a: egui::Color32, b: egui::Color32) -> f32 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

#[test]
fn selection_text_color_is_readable_on_every_accent() {
    let accents = [
        ("teal #009688", egui::Color32::from_rgb(0, 150, 136)),
        ("yellow", egui::Color32::from_rgb(255, 255, 0)),
        ("white", egui::Color32::WHITE),
        ("black", egui::Color32::BLACK),
        ("red", egui::Color32::from_rgb(211, 47, 47)),
        ("blue", egui::Color32::from_rgb(25, 118, 210)),
        ("green", egui::Color32::from_rgb(56, 142, 60)),
    ];
    for (name, accent) in accents {
        let text = selection_text_color(accent);
        let ratio = contrast_ratio(text, accent);
        assert!(
            ratio >= 4.5,
            "selected text on `{name}` must reach 4.5:1, got {ratio:.2}:1"
        );
    }
}

#[test]
fn apply_sets_the_selection_stroke_in_both_themes() {
    for theme_dark in [true, false] {
        let ctx = egui::Context::default();
        let mut style = sample_style();
        style.theme_dark = theme_dark;
        apply(&ctx, &style);

        let expected = selection_text_color(style.accent);
        for theme in [egui::Theme::Dark, egui::Theme::Light] {
            let visuals = &ctx.style_of(theme).visuals;
            assert_eq!(
                visuals.selection.bg_fill, style.accent,
                "the accent still fills the selection in {theme:?}"
            );
            assert_eq!(
                visuals.selection.stroke.color, expected,
                "selected text is readable in {theme:?}"
            );
            assert_eq!(
                visuals.selection.stroke.width, 1.0,
                "the text stroke keeps egui's 1 pt width"
            );
        }
    }
}

#[test]
fn accent_presets_are_eight_unique_named_fully_opaque_colours() {
    assert_eq!(ACCENT_PRESETS.len(), 8);
    let mut names = std::collections::HashSet::new();
    for preset in ACCENT_PRESETS {
        assert!(!preset.name.is_empty(), "every preset is named");
        assert!(
            names.insert(preset.name),
            "preset names are unique; `{}` appears twice",
            preset.name
        );
        assert_eq!(
            preset.color.a(),
            255,
            "`{}` must be fully opaque",
            preset.name
        );
    }
}

#[test]
fn accent_presets_are_pairwise_distinct() {
    for (index, preset) in ACCENT_PRESETS.iter().enumerate() {
        for other in &ACCENT_PRESETS[index + 1..] {
            assert_ne!(
                preset.color, other.color,
                "`{}` and `{}` must be distinguishable",
                preset.name, other.name
            );
        }
    }
}

#[test]
fn selected_preset_matches_exactly_or_not_at_all() {
    for (index, preset) in ACCENT_PRESETS.iter().enumerate() {
        assert_eq!(
            selected_preset(preset.color),
            Some(index),
            "`{}` is recognised as preset {index}",
            preset.name
        );
    }
    // A custom accent matches no preset, even one that is nearly a preset.
    assert_eq!(selected_preset(egui::Color32::from_rgb(12, 34, 56)), None);
    let teal = ACCENT_PRESETS[0].color;
    assert_eq!(
        selected_preset(egui::Color32::from_rgba_unmultiplied(
            teal.r(),
            teal.g(),
            teal.b(),
            teal.a() - 1
        )),
        None,
        "an accent only matches when every channel matches"
    );
}

/// One headless style panel running the real `show_controls` surface.
struct Panel {
    ctx: egui::Context,
    style: AppStyle,
    changed: bool,
}

impl Panel {
    fn new(style: AppStyle) -> Self {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        Self {
            ctx,
            style,
            changed: false,
        }
    }

    fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(400.0, 400.0),
            )),
            events,
            ..Default::default()
        };
        let (mut output, changed) = {
            let style = &mut self.style;
            let mut changed = false;
            let output = self.ctx.run_ui(input, |ui| {
                changed = show_controls(ui, style).changed;
            });
            (output, changed)
        };
        output.textures_delta.clear();
        self.changed = changed;
        output
    }

    fn idle(&mut self) -> egui::FullOutput {
        self.frame(Vec::new())
    }

    fn click(&mut self, pos: egui::Pos2) -> bool {
        self.frame(pointer(pos, true));
        let pressed_changed = self.changed;
        self.frame(pointer(pos, false));
        pressed_changed || self.changed
    }
}

fn pointer(pos: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(pos),
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        },
    ]
}

/// The accesskit view of one button node: label, selected state and rect.
struct Node {
    label: Option<String>,
    toggled: bool,
    rect: egui::Rect,
}

fn nodes(output: &egui::FullOutput) -> Vec<Node> {
    let Some(update) = output.platform_output.accesskit_update.as_ref() else {
        return Vec::new();
    };
    update
        .nodes
        .iter()
        .filter_map(|(_, node)| {
            let bounds = node.bounds()?;
            Some(Node {
                label: node.label().map(str::to_owned),
                toggled: node.toggled() == Some(egui::accesskit::Toggled::True),
                rect: egui::Rect::from_min_max(
                    egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                    egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                ),
            })
        })
        .collect()
}

fn preset_node(output: &egui::FullOutput, name: &str) -> Node {
    nodes(output)
        .into_iter()
        .find(|node| node.label.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("the `{name}` preset swatch must be exposed to accessibility"))
}

#[test]
fn clicking_a_preset_swatch_changes_the_accent() {
    let mut panel = Panel::new(AppStyle::default());
    panel.idle();
    panel.idle();
    let out = panel.idle();

    // The default accent is the first preset (Teal).
    assert!(
        preset_node(&out, "Teal").toggled,
        "the default teal accent marks the Teal preset"
    );

    let blue = preset_node(&out, "Blue");
    assert!(
        (blue.rect.width() - 20.0).abs() < 0.01 && (blue.rect.height() - 20.0).abs() < 0.01,
        "preset swatches are 20x20 pt, got {:?}",
        blue.rect.size()
    );

    assert!(
        panel.click(blue.rect.center()),
        "clicking a preset reports a style change"
    );

    let expected = ACCENT_PRESETS
        .iter()
        .find(|preset| preset.name == "Blue")
        .expect("Blue is a preset")
        .color;
    assert_eq!(panel.style.accent, expected);

    let out = panel.idle();
    assert!(
        preset_node(&out, "Blue").toggled,
        "the clicked preset is marked selected"
    );
    assert!(
        !preset_node(&out, "Teal").toggled,
        "only one preset is marked selected"
    );
}

#[test]
fn custom_accent_marks_no_preset() {
    let mut panel = Panel::new(sample_style());
    panel.idle();
    panel.idle();
    let out = panel.idle();

    for preset in ACCENT_PRESETS {
        assert!(
            !preset_node(&out, preset.name).toggled,
            "a custom accent marks no preset, but `{}` is selected",
            preset.name
        );
    }
}
