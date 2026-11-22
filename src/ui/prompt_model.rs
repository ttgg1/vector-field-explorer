//! Issue #213: session-only prompt interpretation panel.
//!
//! The panel is pure UI over [`crate::prompt_model`]: it holds the typed text,
//! the editable interpretation and the preview, and returns one explicit
//! action per frame. Applying is the app handler's job and goes through the
//! ordinary scene setters. Previewing never mutates the document.

use eframe::egui;

use crate::prompt_model::{self, Axis, DipoleEnd, Preview, PromptModel, Question};

/// The panel's session-only state.
#[derive(Clone, Debug, Default)]
pub struct PromptModelState {
    /// The typed prompt.
    pub input: String,
    /// The parsed, editable interpretation, if any.
    pub model: Option<PromptModel>,
    /// The last parse error, if the prompt was rejected.
    pub error: Option<String>,
    /// The last successful apply/cancel message.
    pub message: Option<String>,
}

impl PromptModelState {
    /// Parse the current input. A rejection clears the model but keeps the text.
    pub fn parse(&mut self) {
        self.error = None;
        self.message = None;
        match prompt_model::parse_prompt(&self.input) {
            Ok(model) => self.model = Some(model),
            Err(error) => {
                self.model = None;
                self.error = Some(error.to_string());
            }
        }
    }

    /// Answer a dipole-orientation question.
    pub fn resolve(&mut self, question: &Question, answer: DipoleEnd) -> bool {
        self.model
            .as_mut()
            .is_some_and(|model| model.resolve_question(question, answer))
    }

    /// Answer the dipole-axis question.
    pub fn resolve_axis(&mut self, axis: Axis) -> bool {
        self.model
            .as_mut()
            .is_some_and(|model| model.resolve_axis(axis))
    }

    /// Edit the separation through the same model.
    pub fn set_separation(&mut self, value: f64) -> bool {
        self.model
            .as_mut()
            .is_some_and(|model| model.set_separation(value).is_ok())
    }

    /// Drop the interpretation and the typed text.
    pub fn cancel(&mut self) {
        self.input.clear();
        self.model = None;
        self.error = None;
        self.message = None;
    }

    /// The preview of the current model, if any.
    pub fn preview(&self) -> Option<Preview> {
        self.model.as_ref().map(prompt_model::preview)
    }
}

/// The one action the panel asks the app to perform this frame.
#[derive(Clone, Debug, PartialEq)]
pub enum PromptModelAction {
    /// Parse the current input.
    Parse,
    /// Answer a dipole-orientation question.
    Resolve {
        question: Question,
        answer: DipoleEnd,
    },
    /// Answer the dipole-axis question.
    ResolveAxis { axis: Axis },
    /// Edit the separation on the same model.
    SetSeparation(f64),
    /// Apply the preview through the ordinary scene setters.
    Apply,
    /// Drop the interpretation without touching the document.
    Cancel,
}

/// Draw the prompt-model section and return any requested action.
pub fn show(ui: &mut egui::Ui, state: &mut PromptModelState) -> Option<PromptModelAction> {
    ui.separator();
    ui.heading("Interpret a prompt");
    ui.small(
        "Offline grammar: a point charge at the origin, or two equal/opposite point charges \
         (a dipole). Nothing applies until you confirm the preview.",
    );

    let edit = ui.add(
        egui::TextEdit::singleline(&mut state.input)
            .hint_text("e.g. two equal and opposite charges separated by d along x")
            .desired_width(f32::INFINITY),
    );
    let mut action = None;
    let mut parse_clicked = false;
    ui.horizontal(|ui| {
        if ui.button("Interpret").clicked() {
            parse_clicked = true;
        }
        if ui.button("Cancel").clicked() {
            action = Some(PromptModelAction::Cancel);
        }
    });
    if edit.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
        parse_clicked = true;
    }
    if parse_clicked {
        action = Some(PromptModelAction::Parse);
    }

    if let Some(error) = &state.error {
        ui.colored_label(egui::Color32::RED, error);
    }
    if let Some(message) = &state.message {
        ui.colored_label(egui::Color32::GREEN, message);
    }

    let Some(model) = state.model.clone() else {
        return action;
    };
    let preview = prompt_model::preview(&model);
    ui.heading("Preview");
    ui.label(&preview.scene_summary);
    for equation in &preview.equations {
        ui.monospace(equation);
    }
    ui.label(&preview.frame);
    ui.label(&preview.units);
    ui.label(&preview.convention);
    if preview.requested.is_empty() {
        ui.small("Requested quantities: none stated; add a measurement after applying.");
    } else {
        ui.label(format!("Requested: {}", preview.requested.join(", ")));
    }

    if let Some(dipole) = &model.dipole {
        let mut separation = dipole.separation.value;
        ui.horizontal(|ui| {
            ui.label("Separation");
            let changed = ui
                .add(
                    egui::DragValue::new(&mut separation)
                        .speed(0.05)
                        .range(0.01..=100.0),
                )
                .changed();
            if changed {
                action = Some(PromptModelAction::SetSeparation(separation));
            }
            ui.small(dipole.separation.label());
        });
    }

    for question in &model.ambiguities {
        ui.label(question.text());
        match question {
            Question::DipoleOrientation { axis } => {
                ui.horizontal(|ui| {
                    if ui
                        .button(format!("+{} end is positive", axis.label()))
                        .clicked()
                    {
                        action = Some(PromptModelAction::Resolve {
                            question: question.clone(),
                            answer: DipoleEnd::PositiveAxis,
                        });
                    }
                    if ui
                        .button(format!("-{} end is positive", axis.label()))
                        .clicked()
                    {
                        action = Some(PromptModelAction::Resolve {
                            question: question.clone(),
                            answer: DipoleEnd::NegativeAxis,
                        });
                    }
                });
            }
            Question::DipoleAxis => {
                ui.horizontal(|ui| {
                    for axis in Axis::ALL {
                        if ui.button(axis.label()).clicked() {
                            action = Some(PromptModelAction::ResolveAxis { axis });
                        }
                    }
                });
            }
        }
    }

    if !preview.limitations.is_empty() {
        for limitation in &preview.limitations {
            ui.colored_label(egui::Color32::YELLOW, limitation);
        }
        ui.label(&preview.formula_path);
    }

    if ui
        .add_enabled(preview.can_apply, egui::Button::new("Apply"))
        .clicked()
    {
        action = Some(PromptModelAction::Apply);
    }

    action
}
