//! Issue #287: a floating, movable typeset view of the active vector formula.
//!
//! This module owns no physics and no second parser. The displayed text is the
//! authoritative committed formula from [`crate::viewmodel::State::formula`]
//! (the formula bar is the editor, but its draft is intentionally not read
//! here: in non-Formula vector modes the bar short-circuits before mirroring
//! and its text would be empty/stale). Committed parameter values and the
//! shared clock time are substituted into that text, each vector component is
//! translated by [`crate::cas_tex::linear_to_tex`], and display is delegated to
//! [`crate::ui::math_view::paint_tex_or_linear`]. Dragging, viewport clamping
//! and session-only placement use [`crate::ui::viewport_panel`].

use eframe::egui::{self, Context, InnerResponse, Pos2, Rect, Ui};

use crate::source_scene::FieldMode;
use crate::ui::viewport_panel::ViewportPanelState;
use crate::viewmodel::State;

/// Accessible label of the control that opens the panel.
pub const TOGGLE_SHOW: &str = "Show typeset formula";
/// Accessible label of the control that closes the panel.
pub const TOGGLE_HIDE: &str = "Hide typeset formula";
/// Panel heading; also the accessible name of the floating area.
pub const PANEL_TITLE: &str = "Typeset formula";
const CLOSE_LABEL: &str = "Close typeset formula";
const RESET_LABEL: &str = "Reset position";
const NOTE: &str = "Active formula - committed values substituted";
/// Finding F1: in a non-Formula vector mode the bar is not the live editor, so
/// the panel labels the committed formula neutrally instead of claiming it is
/// the active input the user is editing.
pub const NOTE_NON_FORMULA: &str = "Committed vector formula - not the live editor in this mode";
const FALLBACK_LABEL: &str = "linear fallback";

/// Session-only panel state: nothing here enters the scene or settings envelope.
#[derive(Debug, Clone, Default)]
pub struct FormulaTypesetPanelState {
    pub open: bool,
    pub placement: ViewportPanelState,
}

impl FormulaTypesetPanelState {
    /// Top-left of the panel before any drag, inset from the viewport's top
    /// right so the edit field and viewport controls stay clear.
    pub fn default_position(&self, viewport: Rect) -> Pos2 {
        let width = (viewport.width() * 0.42).clamp(180.0, 380.0);
        Pos2::new(viewport.right() - width - 12.0, viewport.top() + 12.0)
    }

    /// Restore the default placement on the next frame.
    pub fn reset_position(&mut self) {
        self.placement.reset();
    }

    /// Dismiss the panel without touching its placement.
    pub fn dismiss(&mut self) {
        self.open = false;
    }
}

/// The control in the formula-bar area. It never replaces the text editor.
pub fn toggle_button(ui: &mut Ui, panel: &mut FormulaTypesetPanelState) {
    let label = if panel.open { TOGGLE_HIDE } else { TOGGLE_SHOW };
    if ui
        .button(label)
        .on_hover_text("Show the active formula as typeset math in a floating panel")
        .clicked()
    {
        panel.open = !panel.open;
    }
}

/// The floating toggle strip over the viewport. It is an `Area`, so adding it
/// never changes the bar or sheet row counts in either layout mode.
pub fn toggle_area(ctx: &Context, viewport: Rect, panel: &mut FormulaTypesetPanelState) {
    egui::Area::new(egui::Id::unique("formula_typeset_toggle"))
        // Later foreground overlays (help/tour) can cover the top-left corner;
        // the top interactive layer keeps this control clickable in every mode.
        .order(egui::Order::Tooltip)
        .pivot(egui::Align2::LEFT_TOP)
        // O2: the Materials/Sources status lines paint at `left_top+(12,12)`
        // and `+(12,34)`. Anchor just below them so the strip never overpaints
        // that text, while keeping the same interactive layer and layout cost.
        .fixed_pos(viewport.left_top() + egui::vec2(12.0, 56.0))
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| toggle_button(ui, panel));
        });
}

/// Split a vector formula into components at top-level commas. Nested argument
/// lists are kept intact; this is grouping, not expression parsing.
fn components(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut depth = 0i32;
    for (index, ch) in text.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(text[start..index].trim().to_owned());
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(text[start..].trim().to_owned());
    parts
}

/// Commit the live substitutions into the displayed formula: parameter names
/// and `t` become their current numeric values. This is a bounded token walk,
/// not an expression parser; the arithmetic and TeX still come from the shipped
/// CAS seam.
pub fn composed_formula(text: &str, state: &State) -> Vec<String> {
    // Longest names first so a parameter `a` cannot consume the start of `alpha`.
    let mut names: Vec<(String, String)> = state
        .parameters
        .entries
        .iter()
        .filter(|entry| !entry.name.is_empty() && entry.value.is_finite())
        .map(|entry| (entry.name.clone(), format_number(entry.value)))
        .collect();
    names.push(("t".into(), format_number(f64::from(state.time))));
    names.sort_by_key(|(name, _)| std::cmp::Reverse(name.len()));

    components(text)
        .into_iter()
        .map(|part| substitute(&part, &names))
        .collect()
}

/// Replace whole identifier tokens with their committed numeric values.
fn substitute(text: &str, names: &[(String, String)]) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at].is_ascii_alphabetic() || bytes[at] == b'_' {
            let start = at;
            while at < bytes.len() && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_') {
                at += 1;
            }
            let token = &text[start..at];
            if let Some((_, value)) = names.iter().find(|(name, _)| name == token) {
                out.push_str(value);
            } else {
                out.push_str(token);
            }
        } else {
            let ch = text[at..].chars().next().expect("a byte remains");
            out.push(ch);
            at += ch.len_utf8();
        }
    }
    out
}

/// A committed value rendered exactly as the field evaluator substitutes it.
///
/// Finding F2: the previous `format!("{value:.4}")` collapsed small non-zero
/// values (|value| < 0.00005) to `0` and disagreed with `field.rs`'s
/// `format!("{value}")` at the substitution seam. The NOTE claims committed
/// values are substituted, so we must use the same full-precision rendering as
/// the evaluator rather than assert an exactness that is not there.
fn format_number(value: f64) -> String {
    format!("{value}")
}

/// Draw the panel. Callers do the `open` check; the viewport rect is the one
/// measured by the frame's central panel.
pub fn show(
    ctx: &Context,
    viewport: Rect,
    panel: &mut FormulaTypesetPanelState,
    formula: &str,
    state: &State,
) -> InnerResponse<()> {
    show_slide(ctx, viewport, panel, formula, state, egui::Vec2::ZERO)
}

/// As [`show`], with a purely visual slide offset for the open transition.
pub fn show_slide(
    ctx: &Context,
    viewport: Rect,
    panel: &mut FormulaTypesetPanelState,
    formula: &str,
    state: &State,
    slide: egui::Vec2,
) -> InnerResponse<()> {
    let default = panel.default_position(viewport);
    let mut close = false;
    let mut reset = false;
    let response = super::viewport_panel::show_slide(
        ctx,
        viewport,
        egui::Id::unique(PANEL_TITLE),
        default,
        &mut panel.placement,
        slide,
        |ui| {
            // The Area measures with a zero-size estimate; pin the width like
            // the shared legend overlay so text and KaTeX shapes get real space.
            ui.set_width((viewport.width() * 0.42).clamp(180.0, 380.0));
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading(PANEL_TITLE);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(CLOSE_LABEL).on_hover_text(CLOSE_LABEL).clicked() {
                            close = true;
                        }
                        if ui.button(RESET_LABEL).on_hover_text(RESET_LABEL).clicked() {
                            reset = true;
                        }
                    });
                });
                // Finding F1: only claim "active formula" where the bar is the
                // live editor. Other vector modes get a neutral label so the
                // committed formula stays visible without being mistaken for
                // the editable input.
                let note = if state.field_mode == FieldMode::Formula {
                    NOTE
                } else {
                    NOTE_NON_FORMULA
                };
                ui.label(note);
                let components = composed_formula(formula, state);
                let mut any_fallback = false;
                let body_height = (viewport.height() * 0.45).max(48.0);
                egui::ScrollArea::vertical()
                    .max_height(body_height)
                    .min_scrolled_height(body_height)
                    .auto_shrink([true, true])
                    .show(ui, |ui| {
                        for component in &components {
                            ui.horizontal(|ui| {
                                let tex = crate::cas_tex::linear_to_tex(component);
                                let typeset = crate::ui::math_view::paint_tex_or_linear(
                                    ui,
                                    tex.as_deref(),
                                    component,
                                );
                                any_fallback |= !typeset;
                            });
                        }
                    });
                if any_fallback {
                    ui.label(format!("{FALLBACK_LABEL}: unsupported or clipped notation"));
                }
                ui.weak(
                    "Parameters and time use their committed values; this is the formula, not a sample.",
                );
            });
        },
    );
    if close {
        panel.dismiss();
    }
    if reset {
        panel.reset_position();
    }
    response
}
