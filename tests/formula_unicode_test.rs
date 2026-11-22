//! Issue #288: normalize the Unicode punctuation formula input can receive.
//!
//! The bundled DejaVu/STIX faces do not cover full-width ASCII punctuation, so
//! the setter normalizes it before comparison, parsing, and storage instead of
//! adding a font face.

use eframe::egui;
use glam::Vec3;

use crate::support;

use vector_field_explorer::{
    field::normalize_formula_input,
    plot::PlotKind,
    ui::formula_bar::{self, FormulaBarState},
    viewmodel::State,
};

#[test]
fn mapping_table_normalizes_every_supported_class() {
    let full_width: String = ('\u{FF01}'..='\u{FF5E}').collect();
    let ascii: String = ('!'..='~').collect();
    assert_eq!(
        normalize_formula_input(&full_width),
        ascii,
        "every full-width ASCII punctuation form maps to its ASCII counterpart"
    );

    assert_eq!(
        normalize_formula_input("\u{3000}\u{00A0}"),
        "  ",
        "ideographic and non-breaking spaces become ASCII spaces"
    );
    assert_eq!(
        normalize_formula_input("\u{2212}\u{2013}\u{2014}"),
        "---",
        "minus and dash variants become the ASCII hyphen-minus"
    );
    assert_eq!(
        normalize_formula_input("\u{00D7}\u{00F7}\u{2215}\u{2044}"),
        "*///",
        "multiplication and division variants become * and /"
    );
    assert_eq!(
        normalize_formula_input("\u{2329}\u{232A}\u{27E8}\u{27E9}\u{3008}\u{3009}"),
        "()()()",
        "angle brackets used as grouping become parentheses"
    );
    assert_eq!(
        normalize_formula_input("（x，y）"),
        "(x,y)",
        "a realistic full-width formula settles to its ASCII spelling"
    );
    assert_eq!(
        normalize_formula_input("（x， y）"),
        "(x, y)",
        "spacing the user typed is preserved as ASCII"
    );
}

#[test]
fn mapping_table_preserves_ascii_and_math_alphanumerics() {
    let ascii = "sin(x) + y^2 - 1.5*(z/2)";
    assert_eq!(normalize_formula_input(ascii), ascii);

    let math_alphanumerics = "\u{1D400}\u{1D7FF}\u{1D538}";
    assert_eq!(
        normalize_formula_input(math_alphanumerics),
        math_alphanumerics,
        "math alphanumeric symbols are valid input and must not be rewritten"
    );
}

#[test]
fn set_formula_normalizes_before_parse_and_storage() {
    let mut state = State::new(false);
    state.set_formula("(0, 0)".into());

    assert!(
        state.set_formula("（x， y）".into()),
        "the full-width formula changes the stored text"
    );
    assert_eq!(state.formula, "(x, y)", "the normalized text is stored");
    assert_eq!(state.parse_error, "", "the normalized formula parses");
    assert_eq!(state.field.dimension, 2);
    assert_eq!(
        state.field.eval3(Vec3::new(2.0, 3.0, 0.0)),
        Some(Vec3::new(2.0, 3.0, 0.0)),
        "the parsed field is the ASCII formula's field"
    );

    assert!(
        !state.set_formula("（x， y）".into()),
        "normalization happens before the equality check, so an equivalent edit is not dirty"
    );
}

#[test]
fn scalar_source_normalizes_before_comparison_and_parse() {
    let mut state = State::new(false);
    state.set_plot_kind(PlotKind::Curve2D);

    assert!(state.set_active_source("（x＋1）".into()));
    assert_eq!(state.plot.curve.source, "(x+1)");
    assert!(
        state.plot.curve.error.is_none(),
        "the normalized source parses"
    );
    assert_eq!(
        state
            .plot
            .curve
            .accepted
            .as_ref()
            .and_then(|expr| expr.eval(2.0, 0.0)),
        Some(3.0),
        "the accepted expression is the normalized curve"
    );
    assert!(
        !state.set_active_source("（x＋1）".into()),
        "normalization happens before the draft comparison"
    );
}

#[test]
fn invalid_normalized_formula_errors_and_keeps_the_last_field() {
    let mut state = State::new(false);
    state.set_formula("(-y, x)".into());
    let probe = Vec3::new(0.5, -0.25, 0.0);
    let before = state.field.eval3(probe);

    assert!(state.set_formula("（x,".into()));
    assert_eq!(
        state.formula, "(x,",
        "invalid normalized text is still stored"
    );
    assert!(
        !state.parse_error.is_empty(),
        "the normalized invalid formula reports its parse error"
    );
    assert_eq!(
        state.field.eval3(probe),
        before,
        "the last valid field stays in place"
    );
}

#[test]
fn invalid_normalized_scalar_source_errors_and_keeps_the_accepted_curve() {
    let mut state = State::new(false);
    state.set_plot_kind(PlotKind::Curve2D);
    state.set_active_source("x+1".into());
    let before = state
        .plot
        .curve
        .accepted
        .as_ref()
        .and_then(|expr| expr.eval(2.0, 0.0));

    state.set_active_source("（x＋".into());
    assert_eq!(state.plot.curve.source, "(x+");
    assert!(
        state.plot.curve.error.is_some(),
        "the normalized invalid source reports its parse error"
    );
    assert_eq!(
        state
            .plot
            .curve
            .accepted
            .as_ref()
            .and_then(|expr| expr.eval(2.0, 0.0)),
        before,
        "the last accepted curve stays in place"
    );
}

struct Node {
    role: egui::Role,
    text: String,
}

fn nodes(output: &egui::FullOutput) -> Vec<Node> {
    let Some(update) = output.platform_output.accesskit_update.as_ref() else {
        return Vec::new();
    };
    update
        .nodes
        .iter()
        .map(|(_, node)| Node {
            role: node.role(),
            text: node
                .value()
                .or_else(|| node.label())
                .unwrap_or_default()
                .to_owned(),
        })
        .collect()
}

fn context() -> egui::Context {
    let ctx = support::context();
    ctx.enable_accesskit();
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    ctx
}

fn run_frame(
    ctx: &egui::Context,
    events: Vec<egui::Event>,
    draw: impl FnMut(&mut egui::Ui),
) -> egui::FullOutput {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 300.0),
            )),
            events,
            ..Default::default()
        },
        draw,
    );
    output.textures_delta.clear();
    output
}

fn text_input_text(output: &egui::FullOutput) -> String {
    nodes(output)
        .into_iter()
        .find(|node| node.role == egui::Role::TextInput)
        .expect("the formula bar exposes its real text field")
        .text
}

#[test]
fn formula_bar_frame_mirrors_the_normalized_vector_formula() {
    let ctx = context();
    let mut state = State::new(false);
    let mut bar = FormulaBarState::default();

    assert!(state.set_active_source("（−y， x）".into()));
    assert_eq!(state.formula, "(-y, x)");
    bar.text = "（−y， x）".into();

    let output = run_frame(&ctx, Vec::new(), |ui| {
        formula_bar::show(ui, &mut state, &mut bar, false);
    });

    assert_eq!(
        bar.text, "(-y, x)",
        "the bar mirrors the normalized formula"
    );
    assert_eq!(text_input_text(&output), "(-y, x)");
}

#[test]
fn formula_bar_frame_mirrors_the_normalized_scalar_source() {
    let ctx = context();
    let mut state = State::new(false);
    state.set_plot_kind(PlotKind::Curve2D);
    let mut bar = FormulaBarState::default();

    assert!(state.set_active_source("（x＋1）".into()));
    assert_eq!(state.plot.curve.source, "(x+1)");
    bar.text = "（x＋1）".into();

    let output = run_frame(&ctx, Vec::new(), |ui| {
        formula_bar::show(ui, &mut state, &mut bar, false);
    });

    assert_eq!(bar.text, "(x+1)", "the bar mirrors the normalized curve");
    assert_eq!(text_input_text(&output), "(x+1)");
}
