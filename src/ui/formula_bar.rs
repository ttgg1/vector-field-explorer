//! Top formula bar: editable formula text and the 2D/3D toggle. All edits hand
//! their values to the viewmodel setters, which own parsing and the dirty flag.
//!
//! Field presets live in the grouped Presets panel (ticket 31), which owns the
//! single "which preset is selected" state. The bar deliberately keeps no
//! preset index, so the two surfaces cannot disagree.

use eframe::egui;

use crate::source_scene::FieldMode;
use crate::viewmodel::State;

/// Bar-local state kept out of [`State`]: the text the user has typed, and
/// one-shot requests for the app to open/expand the Sources or Induction panel.
#[derive(Debug, Clone, Default)]
pub struct FormulaBarState {
    pub text: String,
    pub open_sources: bool,
    pub open_induction: bool,
}

impl FormulaBarState {
    pub fn new() -> FormulaBarState {
        FormulaBarState::default()
    }
}

/// Draw the bar and apply every edit to `state`. Typed garbage keeps the last
/// good field (the setter's rule) and shows up in the status bar.
///
/// Wide keeps the historical single wrapping row. Compact puts the projection
/// and adapter selectors on one horizontally scrollable row and the editor or
/// summary beneath it, so a large font cannot tower over the viewport and
/// squeeze the bottom sheet out of existence.
pub fn show(ui: &mut egui::Ui, state: &mut State, bar: &mut FormulaBarState, compact: bool) {
    if compact {
        ui.spacing_mut().interact_size.y = crate::ui::layout::TOUCH_TARGET;
        ui.horizontal(|ui| {
            egui::ScrollArea::horizontal()
                .id_salt("formula_bar_selectors")
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        view_toggles(ui, state);
                        adapter_selectors(ui, state);
                    });
                });
        });
        show_active_source(ui, state, bar, true);
        return;
    }
    ui.horizontal_wrapped(|ui| {
        let full_width = ui.available_width();
        view_toggles(ui, state);
        adapter_selectors(ui, state);
        show_active_source_with_width(ui, state, bar, false, full_width);
    });
}

fn view_toggles(ui: &mut egui::Ui, state: &mut State) {
    let view_tooltip = if state.is_vector_mode() {
        "Switch between the 2D and 3D vector views"
    } else {
        "In plot mode, 2D selects the saved Curve and 3D the saved Surface"
    };
    if ui
        .selectable_label(state.mode_3d, "3D view")
        .on_hover_text(view_tooltip)
        .clicked()
    {
        state.set_mode(true);
    }
    if ui
        .selectable_label(!state.mode_3d, "2D view")
        .on_hover_text(view_tooltip)
        .clicked()
    {
        state.set_mode(false);
    }
}

fn adapter_selectors(ui: &mut egui::Ui, state: &mut State) {
    let kind = state.plot.kind;
    if let Some(label) = crate::ui::plot_panel::source_label(kind) {
        ui.label(label);
    }
    if !state.is_vector_mode() {
        return;
    }
    if ui
        .selectable_label(state.field_mode == FieldMode::Formula, "Formula")
        .on_hover_text("Edit and sample the formula field")
        .clicked()
    {
        state.set_field_mode(FieldMode::Formula);
    }
    if ui
        .selectable_label(state.field_mode == FieldMode::Sources, "Sources")
        .on_hover_text("Build the scene in the Sources panel")
        .clicked()
    {
        state.set_field_mode(FieldMode::Sources);
    }
    if ui
        .selectable_label(state.field_mode == FieldMode::Induction, "Induction")
        .on_hover_text(
            "Analytic teaching demo: quasistatic induced fields from a time-varying loop or solenoid",
        )
        .clicked()
    {
        state.set_field_mode(FieldMode::Induction);
    }
}

fn show_active_source(
    ui: &mut egui::Ui,
    state: &mut State,
    bar: &mut FormulaBarState,
    compact: bool,
) {
    let full_width = ui.available_width();
    show_active_source_with_width(ui, state, bar, compact, full_width);
}

fn show_active_source_with_width(
    ui: &mut egui::Ui,
    state: &mut State,
    bar: &mut FormulaBarState,
    compact: bool,
    full_width: f32,
) {
    if state.is_vector_mode() && state.field_mode == FieldMode::Sources {
        ui.label(sources_summary(state));
        if ui
            .button("Open Sources")
            .on_hover_text("Expand or focus the Sources panel")
            .clicked()
        {
            bar.open_sources = true;
        }
        return;
    }
    if state.is_vector_mode() && state.field_mode == FieldMode::Induction {
        ui.label("Induction scene summary");
        ui.label(induction_summary(state));
        if ui
            .button("Open induction")
            .on_hover_text("Expand or focus the Induction panel")
            .clicked()
        {
            bar.open_induction = true;
        }
        return;
    }
    // The one mirror follows the active editor. Vector edits and scalar
    // drafts never overwrite each other because only the active source is
    // read here; the app reconciles the mirror on kind changes too.
    let active = state.active_source().to_string();
    if bar.text != active {
        bar.text = active;
    }
    // Compact pins the field's 40% floor; wide keeps the historical "fill the
    // rest of the bar" behaviour exactly.
    let field_width = if compact {
        crate::ui::layout::formula_field_width(full_width, full_width - ui.available_width())
    } else {
        f32::INFINITY
    };
    ui.add(
        egui::TextEdit::singleline(&mut bar.text)
            .hint_text(crate::ui::plot_panel::source_hint(state.plot.kind))
            .desired_width(field_width),
    )
    .on_hover_text(crate::ui::plot_panel::source_help(state.plot.kind));
    if bar.text != state.active_source() {
        state.set_active_source(bar.text.clone());
    }
}

fn induction_summary(state: &State) -> String {
    use crate::induction::InductionCase;
    let scene = &state.induction;
    let case = match scene.case {
        InductionCase::CurrentLoop => "Current loop",
        InductionCase::Solenoid => "Finite solenoid",
    };
    let display = match scene.display {
        crate::source_scene::SourceDisplay::Electric => "Electric",
        crate::source_scene::SourceDisplay::Magnetic => "Magnetic",
        crate::source_scene::SourceDisplay::Both => "Both",
    };
    format!(
        "{case} · {display} · amplitude {:.2} · {:.2} Hz · softening {:.3}",
        scene.amplitude, scene.frequency, state.sources.softening
    )
}

fn sources_summary(state: &State) -> String {
    let display = match state.sources.display {
        crate::source_scene::SourceDisplay::Electric => "Electric",
        crate::source_scene::SourceDisplay::Magnetic => "Magnetic",
        crate::source_scene::SourceDisplay::Both => "Both",
    };
    let accuracy = if state.sources.allow_inaccurate {
        "accuracy not met shown"
    } else {
        "accuracy not met hidden"
    };
    format!(
        "{} sources · {display} · softening {:.3} · {accuracy}",
        state.sources.entries.len(),
        state.sources.softening
    )
}
