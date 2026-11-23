//! Issue #222: the Experiment notebook section drawn inside the Maxwell panel.
//!
//! The section owns the editable prediction/note draft, the entry list, the
//! two-entry comparison, and the bounded export/import text area. Scene capture
//! and restore are one-shot actions the app applies through its ordinary scene
//! path; this module never writes the committed scene itself.

use eframe::egui;

use crate::notebook::{MAX_NOTEBOOK_ENTRIES, Notebook, NotebookEntry, compare};

/// The section heading and accessibility label.
pub const NOTEBOOK_TITLE: &str = "Experiment notebook";
/// The capture button label.
pub const NOTEBOOK_CAPTURE_LABEL: &str = "Pin current reading";
/// The empty-state text.
pub const NOTEBOOK_EMPTY: &str = "No pinned readings yet. Capture the current Gauss-shell reading to keep its scene and measurement.";
/// The required scope help.
pub const NOTEBOOK_HELP: &str = "A pinned entry keeps an immutable copy of the committed scene, shell and probe radius, time, parameters, method, status and units. Live edits and later scene loads never change it.";
/// The largest accepted import/export text, in bytes.
pub const MAX_NOTEBOOK_TEXT_BYTES: usize = 256 * 1024;

/// Session-only notebook UI state. The durable store is [`Notebook`].
#[derive(Debug, Clone, Default)]
pub struct NotebookUiState {
    /// The prediction/note draft captured with the next pin.
    pub prediction: String,
    /// The longer note draft captured with the next pin.
    pub note: String,
    /// First entry selected for comparison.
    pub compare_first: Option<String>,
    /// Second entry selected for comparison.
    pub compare_second: Option<String>,
    /// The generated export text.
    pub export_text: String,
    /// The import text draft.
    pub import_text: String,
    /// Replace the notebook instead of appending on import.
    pub import_replace: bool,
    /// The last success message.
    pub message: Option<String>,
    /// The last actionable error.
    pub error: Option<String>,
    /// Whether the export/import disclosure is open.
    pub tools_open: bool,
    /// Whether the notebook section itself is expanded.
    pub open: bool,
}

impl NotebookUiState {
    fn succeed(&mut self, message: String) {
        self.message = Some(message);
        self.error = None;
    }

    fn fail(&mut self, error: String) {
        self.error = Some(error);
        self.message = None;
    }
}

/// One-shot requests the app applies through its ordinary scene/measurement
/// seam.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotebookAction {
    /// Capture the current Gauss-shell reading with the committed scene.
    Capture,
    /// Restore the entry with this id.
    Load(String),
}

/// Draw the notebook section. Capture is offered only while the Gauss activity
/// is active; the app makes the final capture decision against the live report.
pub fn show(
    ui: &mut egui::Ui,
    notebook: &mut Notebook,
    state: &mut NotebookUiState,
    gauss_active: bool,
    compact: bool,
) -> Option<NotebookAction> {
    let mut action = None;
    ui.separator();
    let mut open = state.open;
    let response = egui::CollapsingHeader::new(NOTEBOOK_TITLE)
        .open(Some(open))
        .show(ui, |ui| {
            ui.label(NOTEBOOK_HELP);
            ui.small(format!(
                "{} of {MAX_NOTEBOOK_ENTRIES} entries stored. Reopened automatically from saved settings.",
                notebook.entries.len()
            ));
            if let Some(error) = state.error.as_ref() {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            if let Some(message) = state.message.as_ref() {
                ui.label(message);
            }

            ui.add(
                egui::TextEdit::singleline(&mut state.prediction)
                    .hint_text("Prediction for this measurement")
                    .desired_width(if compact { ui.available_width() } else { 260.0 }),
            )
            .on_hover_text("Your prediction is copied into the pinned entry, not the live model");
            ui.add(
                egui::TextEdit::multiline(&mut state.note)
                    .hint_text("Note (optional)")
                    .desired_rows(2)
                    .desired_width(if compact { ui.available_width() } else { 360.0 }),
            )
            .on_hover_text("Your note is copied into the pinned entry");

            let capture = ui
                .add_enabled(gauss_active, egui::Button::new(NOTEBOOK_CAPTURE_LABEL))
                .on_hover_text(if gauss_active {
                    "Snapshot the committed scene and the current Gauss-shell reading"
                } else {
                    "Activate the Gauss-shell investigation first"
                });
            if capture.clicked() {
                action = Some(NotebookAction::Capture);
            }

            if notebook.entries.is_empty() {
                ui.label(NOTEBOOK_EMPTY);
            } else {
                ui.separator();
                ui.label("Pinned readings (snapshot at pin time)");
                let entries = notebook.entries.clone();
                for (index, entry) in entries.iter().enumerate() {
                    if draw_entry(ui, state, entry, index, compact, &mut action) {
                        notebook.remove(&entry.id);
                        if state.compare_first.as_deref() == Some(entry.id.as_str()) {
                            state.compare_first = None;
                        }
                        if state.compare_second.as_deref() == Some(entry.id.as_str()) {
                            state.compare_second = None;
                        }
                        state.succeed("Removed the pinned entry.".to_owned());
                    }
                }
            }

            draw_comparison(ui, notebook, state);
            draw_tools(ui, notebook, state, compact);
        });
    if response.header_response.clicked() {
        open = !open;
    }
    state.open = open;
    action
}

fn draw_entry(
    ui: &mut egui::Ui,
    state: &mut NotebookUiState,
    entry: &NotebookEntry,
    index: usize,
    compact: bool,
    action: &mut Option<NotebookAction>,
) -> bool {
    let mut remove = false;
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.strong(format!(
            "#{} {}",
            index + 1,
            if entry.prediction.trim().is_empty() {
                "(no prediction)"
            } else {
                entry.prediction.trim()
            }
        ));
        if !entry.note.trim().is_empty() {
            ui.label(entry.note.trim());
        }
        ui.label(entry.snapshot.geometry.text());
        ui.label(format!(
            "{} · {} · {:.3} s",
            entry.snapshot.family.label(),
            entry.snapshot.value.text(),
            entry.snapshot.time
        ));
        ui.small(&entry.snapshot.method);
        ui.small(&entry.snapshot.units);
        if compact {
            ui.vertical(|ui| {
                entry_buttons(ui, state, &entry.id, action, &mut remove);
            });
        } else {
            ui.horizontal_wrapped(|ui| {
                entry_buttons(ui, state, &entry.id, action, &mut remove);
            });
        }
    });
    remove
}

fn entry_buttons(
    ui: &mut egui::Ui,
    state: &mut NotebookUiState,
    id: &str,
    action: &mut Option<NotebookAction>,
    remove: &mut bool,
) {
    let is_first = state.compare_first.as_deref() == Some(id);
    let is_second = state.compare_second.as_deref() == Some(id);
    if ui
        .selectable_label(is_first, "Compare A")
        .on_hover_text("Select this entry as the first side of the comparison")
        .clicked()
    {
        state.compare_first = Some(id.to_owned());
        state.message = None;
        state.error = None;
    }
    if ui
        .selectable_label(is_second, "Compare B")
        .on_hover_text("Select this entry as the second side of the comparison")
        .clicked()
    {
        state.compare_second = Some(id.to_owned());
        state.message = None;
        state.error = None;
    }
    if ui
        .button("Load scene")
        .on_hover_text("Restore this captured scene, shell and probe radius")
        .clicked()
    {
        *action = Some(NotebookAction::Load(id.to_owned()));
    }
    if ui.button("Remove").clicked() {
        *remove = true;
    }
}

fn draw_comparison(ui: &mut egui::Ui, notebook: &Notebook, state: &NotebookUiState) {
    let (Some(first_id), Some(second_id)) = (
        state.compare_first.as_deref(),
        state.compare_second.as_deref(),
    ) else {
        return;
    };
    let (Some(first), Some(second)) = (notebook.get(first_id), notebook.get(second_id)) else {
        return;
    };
    if first.id == second.id {
        ui.small("Choose two different entries to compare.");
        return;
    }

    ui.separator();
    ui.strong("Two-entry comparison");
    let result = compare(first, second);
    if result.changes.is_empty() {
        ui.label("No captured setting changed.");
    } else {
        ui.label("Changed settings");
        egui::Grid::new("notebook_comparison")
            .num_columns(3)
            .striped(true)
            .show(ui, |ui| {
                ui.strong("Setting");
                ui.strong("Entry A");
                ui.strong("Entry B");
                ui.end_row();
                for change in &result.changes {
                    ui.label(&change.label);
                    ui.label(&change.first);
                    ui.label(&change.second);
                    ui.end_row();
                }
            });
    }
    match result.flux {
        Some(flux) => {
            ui.label(format!(
                "Flux A = {:.6} +/- {:.2e}",
                flux.first, flux.first_uncertainty
            ));
            ui.label(format!(
                "Flux B = {:.6} +/- {:.2e}",
                flux.second, flux.second_uncertainty
            ));
            ui.label(format!(
                "|A - B| = {:.3e}; combined quadrature uncertainty = {:.3e}",
                flux.difference, flux.combined_uncertainty
            ));
            ui.label(format!(
                "Equal within combined uncertainty: {}",
                if flux.equal_within_uncertainty {
                    "yes"
                } else {
                    "no"
                }
            ));
        }
        None => {
            ui.label("Flux comparison unavailable: one entry has no finite estimate.");
        }
    }
}

fn draw_tools(
    ui: &mut egui::Ui,
    notebook: &mut Notebook,
    state: &mut NotebookUiState,
    compact: bool,
) {
    let mut open = state.tools_open;
    let response = egui::CollapsingHeader::new("Export / import")
        .open(Some(open))
        .show(ui, |ui| {
            ui.label("Versioned notebook JSON. Import validates and sanitizes, then appends or replaces.");
            egui::ScrollArea::vertical()
                .id_salt("notebook_export")
                .max_height(90.0)
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut state.export_text)
                            .desired_rows(3)
                            .desired_width(f32::INFINITY)
                            .char_limit(MAX_NOTEBOOK_TEXT_BYTES)
                            .hint_text("Exported notebook JSON"),
                    );
                });
            ui.horizontal_wrapped(|ui| {
                if ui.button("Export notebook").clicked() {
                    match notebook.to_json() {
                        Ok(json) => {
                            state.export_text = json;
                            state.succeed("Notebook exported as JSON.".to_owned());
                        }
                        Err(error) => state.fail(error),
                    }
                }
                if ui.button("Copy export").clicked() {
                    ui.ctx().copy_text(state.export_text.clone());
                    state.succeed("Notebook JSON copied.".to_owned());
                }
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt("notebook_import")
                .max_height(90.0)
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut state.import_text)
                            .desired_rows(3)
                            .desired_width(f32::INFINITY)
                            .char_limit(MAX_NOTEBOOK_TEXT_BYTES)
                            .hint_text("Paste notebook JSON here"),
                    );
                });
            ui.checkbox(&mut state.import_replace, "Replace the notebook on import");
            ui.label(if state.import_replace {
                "Import replaces every stored entry after sanitizing."
            } else {
                "Import appends new entries and skips duplicate ids."
            });
            if ui.button("Import notebook").clicked() {
                import(notebook, state);
            }
            if compact {
                ui.small("The import/export text area stays bounded.");
            }
        });
    if response.header_response.clicked() {
        open = !open;
    }
    state.tools_open = open;
}

fn import(notebook: &mut Notebook, state: &mut NotebookUiState) {
    if state.import_text.len() > MAX_NOTEBOOK_TEXT_BYTES {
        state.fail(format!(
            "the import text is at most {MAX_NOTEBOOK_TEXT_BYTES} bytes"
        ));
        return;
    }
    let imported = match Notebook::from_json(&state.import_text) {
        Ok(imported) => imported,
        Err(error) => {
            state.fail(error);
            return;
        }
    };
    if state.import_replace {
        let count = imported.entries.len();
        notebook.replace(imported);
        state.succeed(format!("Replaced the notebook with {count} entries."));
    } else {
        let added = notebook.append(imported);
        if added == 0 {
            state.fail("No new entries to import (duplicate ids or a full notebook).".to_owned());
        } else {
            state.succeed(format!("Imported {added} entries."));
        }
    }
}
