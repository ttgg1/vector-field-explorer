//! Top formula bar: editable formula text and the 2D/3D toggle. All edits hand
//! their values to the viewmodel setters, which own parsing and the dirty flag.
//!
//! Field presets live in the grouped Presets panel (ticket 31), which owns the
//! single "which preset is selected" state. The bar deliberately keeps no
//! preset index, so the two surfaces cannot disagree.

use eframe::egui;

use crate::source_scene::FieldMode;
use crate::viewmodel::State;

/// Bar-local state kept out of [`State`]: the text the user has typed, and a
/// one-shot request for the app to open/expand the Sources panel.
#[derive(Debug, Clone, Default)]
pub struct FormulaBarState {
    pub text: String,
    pub open_sources: bool,
}

impl FormulaBarState {
    pub fn new() -> FormulaBarState {
        FormulaBarState::default()
    }
}

/// Draw the bar and apply every edit to `state`. Typed garbage keeps the last
/// good field (the setter's rule) and shows up in the status bar.
///
/// `compact` keeps the same widgets but gives them finger-sized hit targets and
/// applies the formula-field width floor; the wide path is unchanged.
pub fn show(ui: &mut egui::Ui, state: &mut State, bar: &mut FormulaBarState, compact: bool) {
    ui.horizontal_wrapped(|ui| {
        if compact {
            ui.spacing_mut().interact_size.y = crate::ui::layout::TOUCH_TARGET;
        }
        let full_width = ui.available_width();
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

        let kind = state.plot.kind;
        if let Some(label) = crate::ui::plot_panel::source_label(kind) {
            ui.label(label);
        }
        let vector_mode = state.is_vector_mode();
        if vector_mode {
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
        }
        if vector_mode && state.field_mode == FieldMode::Sources {
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
        // The one mirror follows the active editor. Vector edits and scalar
        // drafts never overwrite each other because only the active source is
        // read here; the app reconciles the mirror on kind changes too.
        let active = state.active_source().to_string();
        if bar.text != active {
            bar.text = active;
        }
        // Compact pins the field's 40% floor after the 2D/3D toggles; wide keeps
        // the historical "fill the rest of the bar" behaviour exactly.
        let field_width = if compact {
            crate::ui::layout::formula_field_width(full_width, full_width - ui.available_width())
        } else {
            f32::INFINITY
        };
        ui.add(
            egui::TextEdit::singleline(&mut bar.text)
                .hint_text(crate::ui::plot_panel::source_hint(kind))
                .desired_width(field_width),
        )
        .on_hover_text(crate::ui::plot_panel::source_help(kind));
        if bar.text != state.active_source() {
            state.set_active_source(bar.text.clone());
        }
    });
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
