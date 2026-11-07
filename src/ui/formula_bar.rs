//! Top formula bar: editable formula text, a field-preset ComboBox and the
//! 2D/3D toggle. All edits hand their values to the viewmodel setters, which
//! own parsing and the dirty flag.

use eframe::egui;

use crate::presets::presets;
use crate::viewmodel::State;

/// Bar-local state kept out of [`State`]: the text the user has typed and
/// the preset the ComboBox currently selects (an index so the ComboBox can
/// remember its own selection across frames).
#[derive(Debug, Clone, Default)]
pub struct FormulaBarState {
    pub text: String,
    pub preset_index: usize,
}

impl FormulaBarState {
    pub fn new() -> FormulaBarState {
        FormulaBarState::default()
    }
}

/// Draw the bar and apply every edit to `state`. The ComboBox may switch the
/// mode when the selected preset is three-dimensional; typed garbage keeps
/// the last good field (the setter's rule) and shows up in the status bar.
pub fn show(ui: &mut egui::Ui, state: &mut State, bar: &mut FormulaBarState) {
    let all = presets();
    if bar.preset_index >= all.len() {
        bar.preset_index = 0;
    }

    ui.horizontal(|ui| {
        ui.label("Field preset");
        let before = bar.preset_index;
        egui::ComboBox::from_id_salt("field_preset_combo")
            .selected_text(all[before].name)
            .show_index(ui, &mut bar.preset_index, all.len(), |i| all[i].name);
        if bar.preset_index != before {
            let preset = &all[bar.preset_index];
            state.set_mode(preset.dimension == 3);
            state.set_formula(preset.formula.to_string());
            bar.text = state.formula.clone();
        }

        if ui.selectable_label(state.mode_3d, "3D view").clicked() {
            state.set_mode(true);
        }
        if ui.selectable_label(!state.mode_3d, "2D view").clicked() {
            state.set_mode(false);
        }

        if bar.text != state.formula {
            bar.text = state.formula.clone();
        }
        ui.add(
            egui::TextEdit::singleline(&mut bar.text)
                .hint_text("(x, y)")
                .desired_width(f32::INFINITY),
        );
        if bar.text != state.formula {
            state.set_formula(bar.text.clone());
        }
    });
}
