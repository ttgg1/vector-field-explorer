//! The pin tray: pin/unpin/reorder controls and readouts, save named
//! arrangements, and show honest availability for the active model.
//!
//! The tray owns only pin preferences. A pinned control sends one edit action
//! so the app can call the same ordinary setter as the original control; a
//! pinned readout formats the ordinary report. No value, probe or document
//! state is copied into the preference.

use eframe::egui;

use crate::field_probe::IntegralStatus;
use crate::pin_layout::{PinItem, PinUiRecord};
use crate::ui::gauss_shell::{GAUSS_SHELL_MAX_RADIUS, GAUSS_SHELL_MIN_RADIUS, GaussShellReading};

/// Session message plus the durable pin preference.
#[derive(Debug, Clone, Default)]
pub struct PinsState {
    /// Active order and named arrangements; persisted by the app.
    pub record: PinUiRecord,
    /// The arrangement-name text field.
    pub name_draft: String,
    /// The last save/restore/delete result, shown until the next action.
    pub message: Option<String>,
}

impl PinsState {
    /// Build tray state from the loaded preference.
    pub fn new(record: PinUiRecord) -> Self {
        Self {
            record: record.sanitized(),
            ..Self::default()
        }
    }
}

/// The values the tray may show, built by the app from the ordinary reports.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PinSnapshot {
    /// Live measuring radius when the Gauss investigation is active.
    pub gauss_radius: Option<f64>,
    /// Formatted flux at that radius.
    pub gauss_flux: Option<String>,
    /// Why the Gauss items cannot be shown in the current model.
    pub gauss_unavailable: Option<String>,
    /// The committed induction frequency.
    pub induction_frequency: Option<f64>,
    /// Why the induction frequency cannot be shown in the current model.
    pub induction_unavailable: Option<String>,
}

impl PinSnapshot {
    /// The visible reason a pinned item is unavailable, if any.
    pub fn unavailable(&self, item: PinItem) -> Option<&str> {
        match item {
            PinItem::GaussRadius | PinItem::GaussFlux => self.gauss_unavailable.as_deref(),
            PinItem::InductionFrequency => self.induction_unavailable.as_deref(),
        }
    }

    /// Whether the item has a live value in the active model.
    pub fn is_available(&self, item: PinItem) -> bool {
        self.unavailable(item).is_none()
    }

    /// The formatted readout for an available item. A control has no text
    /// value here because the tray draws it as a live editable widget.
    pub fn value_text(&self, item: PinItem) -> Option<String> {
        match item {
            PinItem::GaussRadius => self.gauss_radius.map(|radius| format!("r = {radius:.6}")),
            PinItem::GaussFlux => self.gauss_flux.clone(),
            PinItem::InductionFrequency => self
                .induction_frequency
                .map(|frequency| format!("f = {frequency:.6} Hz (normalized)")),
        }
    }
}

/// One edit the app must apply through the ordinary model setter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PinAction {
    /// Set the Gauss measuring radius exactly as the original control does.
    SetGaussRadius(f64),
}

/// Format one reading's flux exactly as the shared report classifies it.
pub fn flux_text(reading: &GaussShellReading) -> String {
    if let Some((value, error)) = reading.flux_estimate() {
        return format!("Phi = {value:.6} ± {error:.2e} (full/half difference)");
    }
    match reading.flux().map(|report| &report.status) {
        Some(IntegralStatus::Unresolved { reason } | IntegralStatus::Singular { reason }) => {
            format!("Phi unavailable: {reason}")
        }
        Some(IntegralStatus::Estimate { value, error }) => {
            format!("Phi = {value:.6} ± {error:.2e}")
        }
        None => "Phi unavailable: the sample is excluded at this radius.".to_owned(),
    }
}

/// Draw the pin tray. Returns the model edit to apply through the ordinary
/// setter, if the learner changed a pinned control.
///
/// Rows stay one line each so a pinned flux remains visible beside its radius
/// in the compact sheet; the named-arrangement editor follows the working set.
pub fn show(
    ui: &mut egui::Ui,
    state: &mut PinsState,
    snapshot: &PinSnapshot,
    compact: bool,
) -> Option<PinAction> {
    if compact {
        ui.spacing_mut().interact_size.y = crate::ui::layout::COMPACT_ROW_HEIGHT;
    }
    let mut action = None;
    let mut toggle = None;
    let mut unpin = None;
    let mut move_up = None;
    let mut move_down = None;
    let mut restore = None;
    let mut delete = None;

    ui.horizontal(|ui| {
        ui.strong("Pinned controls and readings").on_hover_text(
            "Pins stay visible while you edit in compact layout. A pinned control and its \
             original share one model state; a pinned reading shows the ordinary report.",
        );
    });
    ui.horizontal_wrapped(|ui| {
        for item in PinItem::ALL {
            let pinned = state.record.active.contains(item);
            let label = if pinned {
                format!("Unpin {}", item.short_label())
            } else {
                format!("Pin {}", item.short_label())
            };
            if ui.button(label).on_hover_text(item.label()).clicked() {
                toggle = Some(item);
            }
        }
    });

    if !state.record.active.is_empty() {
        ui.separator();
        let items = state.record.active.items.clone();
        let len = items.len();
        for (index, item) in items.into_iter().enumerate() {
            ui.horizontal_wrapped(|ui| {
                ui.label(item.label());
                if let Some(reason) = snapshot.unavailable(item) {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        format!("Unavailable in the active model: {reason}"),
                    );
                } else {
                    match item {
                        PinItem::GaussRadius => {
                            if let Some(radius) = snapshot.gauss_radius {
                                let mut value = radius;
                                let response = ui.add(
                                    egui::DragValue::new(&mut value)
                                        .speed(0.01)
                                        .range(GAUSS_SHELL_MIN_RADIUS..=GAUSS_SHELL_MAX_RADIUS),
                                );
                                if response.changed() && value.is_finite() {
                                    let clamped =
                                        value.clamp(GAUSS_SHELL_MIN_RADIUS, GAUSS_SHELL_MAX_RADIUS);
                                    if (clamped - radius).abs() > f64::EPSILON {
                                        action = Some(PinAction::SetGaussRadius(clamped));
                                    }
                                }
                            }
                        }
                        PinItem::GaussFlux | PinItem::InductionFrequency => {
                            if let Some(text) = snapshot.value_text(item) {
                                ui.label(text);
                            }
                        }
                    }
                }
                let up = ui.add_enabled(
                    index > 0,
                    egui::Button::new(format!("Up {}", item.short_label())),
                );
                if up
                    .on_hover_text(format!("Move {} up", item.label()))
                    .clicked()
                {
                    move_up = Some(index);
                }
                let down = ui.add_enabled(
                    index + 1 < len,
                    egui::Button::new(format!("Down {}", item.short_label())),
                );
                if down
                    .on_hover_text(format!("Move {} down", item.label()))
                    .clicked()
                {
                    move_down = Some(index);
                }
                if ui
                    .button(format!("Unpin {}", item.short_label()))
                    .on_hover_text(format!("Unpin {}", item.label()))
                    .clicked()
                {
                    unpin = Some(item);
                }
            });
        }

        ui.separator();
        ui.horizontal_wrapped(|ui| {
            ui.label("Arrangement name");
            ui.add(
                egui::TextEdit::singleline(&mut state.name_draft).hint_text("e.g. Lab flux check"),
            );
            if ui.button("Save pin arrangement").clicked() {
                match state.record.save_arrangement(&state.name_draft) {
                    Ok(name) => {
                        state.name_draft.clear();
                        state.message = Some(format!("Saved pin arrangement {name:?}."));
                    }
                    Err(error) => state.message = Some(error),
                }
            }
        });

        if state.record.saved.is_empty() {
            ui.small("No named pin arrangements yet.");
        } else {
            let saved = state.record.saved.clone();
            for arrangement in saved {
                ui.horizontal_wrapped(|ui| {
                    ui.label(&arrangement.name);
                    if ui.button(format!("Restore {}", arrangement.name)).clicked() {
                        restore = Some(arrangement.name.clone());
                    }
                    if ui.button(format!("Delete {}", arrangement.name)).clicked() {
                        delete = Some(arrangement.name.clone());
                    }
                });
            }
        }
    } else {
        ui.small("Nothing pinned yet. Pin a control or reading above.");
    }

    if let Some(message) = &state.message {
        ui.label(message);
    }

    if let Some(item) = toggle {
        if !state.record.unpin(item) {
            state.record.pin(item);
        }
        // A toggle is a pin edit, not a model edit: no report is invalidated.
        state.message = None;
    }
    if let Some(index) = move_up {
        state.record.move_up(index);
    }
    if let Some(index) = move_down {
        state.record.move_down(index);
    }
    if let Some(item) = unpin {
        state.record.unpin(item);
    }
    if let Some(name) = restore
        && state.record.restore_arrangement(&name)
    {
        state.message = Some(format!("Restored pin arrangement {name:?}."));
    }
    if let Some(name) = delete
        && state.record.delete_arrangement(&name)
    {
        state.message = Some(format!("Deleted pin arrangement {name:?}."));
    }

    action
}
