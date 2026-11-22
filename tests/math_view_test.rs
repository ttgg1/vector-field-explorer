//! Egui adapter for katex-native display lists (issue #171).

use eframe::egui;
use vector_field_explorer::ui::math_view;

fn with_ui<R>(f: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let ctx = crate::support::context();
    // A warm-up pass lets queued KaTeX faces materialise before layout.
    let mut warmup = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(640.0, 480.0),
            )),
            ..Default::default()
        },
        |_| {},
    );
    warmup.textures_delta.clear();
    let mut result = None;
    let mut f = Some(f);
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(640.0, 480.0),
            )),
            ..Default::default()
        },
        |ui| {
            if let Some(f) = f.take() {
                result = Some(f(ui));
            }
        },
    );
    output.textures_delta.clear();
    result.expect("one UI pass")
}

fn context() -> &'static katex_native::katex::KatexContext {
    math_view::shared_context()
}

#[test]
fn layout_math_paints_a_supported_formula() {
    let layout = with_ui(|ui| math_view::layout_math(ui, r"\frac{x^{2}+1}{x-3}", context()))
        .expect("supported TeX lays out");
    assert!(layout.width > 0.0);
    assert!(layout.height > 0.0);
    assert!(!layout.is_empty());
}

#[test]
fn layout_math_rejects_unknown_tex_without_panicking() {
    for tex in [
        r"\unknowncommand{x}",
        "",
        r"\\",
        "{",
        r"\frac{",
        r"\sqrt[",
        "\u{0}",
        r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}",
    ] {
        let _ = with_ui(|ui| math_view::layout_math(ui, tex, context()));
    }
    assert!(with_ui(|ui| math_view::layout_math(ui, r"\unknowncommand{x}", context())).is_none());
}

#[test]
fn radicals_fall_back_to_linear() {
    assert!(with_ui(|ui| math_view::layout_math(ui, r"\sqrt{x^2+y^2}", context())).is_none());
}

#[test]
fn katex_context_is_built_once() {
    assert!(std::ptr::eq(context(), context()));
}

#[test]
fn paint_helper_falls_back_to_linear_on_layout_failure() {
    let ctx = crate::support::context();
    ctx.enable_accesskit();
    let mut painted = None;
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(640.0, 480.0),
            )),
            ..Default::default()
        },
        |ui| {
            painted = Some(math_view::paint_tex_or_linear(
                ui,
                Some(r"\unknowncommand{x}"),
                "x",
            ));
        },
    );
    output.textures_delta.clear();
    assert_eq!(painted, Some(false), "a failed layout must not paint TeX");
    let update = output
        .platform_output
        .accesskit_update
        .as_ref()
        .expect("accesskit update");
    assert!(
        update.nodes.iter().any(|(_, node)| {
            [node.label(), node.value()]
                .into_iter()
                .flatten()
                .any(|text| text == "Showing linear result")
        }),
        "the fallback must explain itself"
    );
}

/// The shared app-startup seam must register the KaTeX typeset families, not
/// only the text faces. Both the native and WebAssembly creators call it, so
/// this guards the wasm startup path that previously omitted the math faces.
#[test]
fn startup_font_helper_registers_katex_families() {
    let ctx = egui::Context::default();
    vector_field_explorer::fonts::install_startup_fonts(&ctx);
    crate::support::run_empty_pass(&ctx);
    let families = ctx.fonts(|fonts| fonts.families());
    assert!(
        families.iter().any(|family| matches!(
            family,
            egui::FontFamily::Name(name) if name.starts_with("KaTeX_")
        )),
        "startup must register the KaTeX families before the first frame: {families:?}"
    );
    assert!(
        ctx.fonts_mut(|fonts| fonts.has_glyph(&egui::FontId::proportional(14.0), 'A')),
        "startup must also install the shipped text faces",
    );
}

#[test]
fn install_math_fonts_registers_families_without_an_intervening_pass() {
    let ctx = crate::support::context();
    crate::support::run_empty_pass(&ctx);
    math_view::install_math_fonts(&ctx);
    let families = ctx.fonts(|fonts| fonts.families());
    assert!(
        families.iter().any(|family| matches!(
            family,
            egui::FontFamily::Name(name) if name.starts_with("KaTeX_")
        )),
        "KaTeX families must be available immediately after registration: {families:?}"
    );
    let before = families.len();
    math_view::install_math_fonts(&ctx);
    assert_eq!(
        ctx.fonts(|fonts| fonts.families()).len(),
        before,
        "registration is idempotent"
    );
}

#[test]
fn non_finite_display_items_fall_back() {
    use katex_native::{Font, Item};

    let text = Item::Text {
        x: f64::NAN,
        y: 0.0,
        text: "x".into(),
        font: Font::default(),
        size: 1.0,
        color: None,
    };
    let rect = Item::Rect {
        x: 0.0,
        y: 0.0,
        width: f64::INFINITY,
        height: 1.0,
        color: None,
    };
    let line = Item::Line {
        from: (0.0, f64::NEG_INFINITY),
        to: (1.0, 0.0),
        thickness: 1.0,
        color: None,
    };
    for item in [text, rect, line] {
        let result = with_ui(|ui| {
            math_view::math_layout_from_items(ui, 10.0, 1.0, 0.0, std::slice::from_ref(&item))
        });
        assert!(result.is_none(), "non-finite item must fall back: {item:?}");
    }
}

#[test]
fn oversized_layout_falls_back_to_linear() {
    let ctx = crate::support::context();
    ctx.enable_accesskit();
    let tex = r"\frac{x^{10}+3x^{9}-2x^{8}+7x^{7}-5x^{6}+11x^{5}-13x^{4}+17x^{3}-19x^{2}+23x-29}{x^{5}-3x^{4}+5x^{3}-7x^{2}+11x-13}";
    let mut painted = None;
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(200.0, 300.0),
            )),
            ..Default::default()
        },
        |ui| {
            painted = Some(math_view::paint_tex_or_linear(ui, Some(tex), "linear"));
        },
    );
    output.textures_delta.clear();
    assert_eq!(
        painted,
        Some(false),
        "a layout wider than the available space must not be clipped"
    );
    let update = output
        .platform_output
        .accesskit_update
        .as_ref()
        .expect("accesskit update");
    assert!(
        update.nodes.iter().any(|(_, node)| {
            [node.label(), node.value()]
                .into_iter()
                .flatten()
                .any(|text| text == "Showing linear result")
        }),
        "the oversized fallback must explain itself"
    );
}
