//! Native-only symbolic formula tools. All editor state is transient.
use eframe::egui;

use crate::cas_actions::{self, CasAction, CasActionError};
use crate::{cas, field::FieldExpr, source_scene::FieldMode, viewmodel::State};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Operation {
    #[default]
    Differentiate,
    Gradient,
    Divergence,
    Curl,
}
impl Operation {
    const ALL: [Self; 4] = [
        Self::Differentiate,
        Self::Gradient,
        Self::Divergence,
        Self::Curl,
    ];
    fn label(self) -> &'static str {
        match self {
            Self::Differentiate => "Differentiate",
            Self::Gradient => "Gradient",
            Self::Divergence => "Divergence",
            Self::Curl => "Curl",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Variable {
    #[default]
    X,
    Y,
    Z,
    T,
}
impl Variable {
    const ALL: [Self; 4] = [Self::X, Self::Y, Self::Z, Self::T];
    fn label(self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Y => "y",
            Self::Z => "z",
            Self::T => "t",
        }
    }
    fn adapter(self) -> cas::Variable {
        match self {
            Self::X => cas::Variable::X,
            Self::Y => cas::Variable::Y,
            Self::Z => cas::Variable::Z,
            Self::T => cas::Variable::T,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultKind {
    Scalar,
    Vector,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    pub formula: String,
    pub kind: ResultKind,
    request: Request,
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct Request {
    input: String,
    operation: Operation,
    variable: Variable,
    dimension: usize,
}

#[derive(Debug, Clone)]
pub struct CasPanelState {
    pub input: String,
    pub operation: Operation,
    pub variable: Variable,
    pub preview: Option<Preview>,
    pub error: Option<String>,
    /// The parsed app-control action for the current line, if any.
    pub action: Option<CasAction>,
    /// The precise parse rejection for an app-control action line.
    pub action_error: Option<String>,
    /// Result of the most recent applied action (shown until the input edits).
    pub action_message: Option<String>,
    /// Rejection from applying a parsed action (shown until the input edits).
    pub action_rejection: Option<String>,
    last_request: Option<Request>,
    /// The input text the apply result belongs to, so a new edit clears it.
    last_action_input: Option<String>,
}
impl Default for CasPanelState {
    fn default() -> Self {
        Self {
            input: "x".into(),
            operation: Operation::Differentiate,
            variable: Variable::X,
            preview: None,
            error: None,
            action: None,
            action_error: None,
            action_message: None,
            action_rejection: None,
            last_request: None,
            last_action_input: None,
        }
    }
}
impl CasPanelState {
    fn request(&self, dimension: usize) -> Request {
        Request {
            input: self.input.clone(),
            operation: self.operation,
            variable: self.variable,
            dimension,
        }
    }
    pub fn refresh(&mut self, dimension: usize) {
        if self.last_action_input.as_deref() != Some(self.input.as_str()) {
            self.last_action_input = Some(self.input.clone());
            self.action_message = None;
            self.action_rejection = None;
        }
        // A recognised action verb wins over the mathematical intent grammar.
        match cas_actions::parse_action(&self.input) {
            Ok(action) => {
                self.action = Some(action);
                self.action_error = None;
                self.preview = None;
                self.error = None;
                self.last_request = None;
                return;
            }
            Err(CasActionError::UnknownVerb) | Err(CasActionError::Empty) => {
                self.action = None;
                self.action_error = None;
            }
            Err(error) => {
                self.action = None;
                self.action_error = Some(error.to_string());
                self.preview = None;
                self.error = None;
                self.last_request = None;
                return;
            }
        }
        let request = self.request(dimension);
        if self.last_request.as_ref() == Some(&request) {
            return;
        }
        self.last_request = Some(request.clone());
        self.preview = None;
        self.error = None;
        let result = derive(&request);
        match result {
            Ok((formula, kind)) => {
                self.preview = Some(Preview {
                    formula,
                    kind,
                    request,
                })
            }
            Err(err) => self.error = Some(err),
        }
    }
    pub fn copy_text(&self) -> Option<&str> {
        self.preview.as_ref().map(|p| p.formula.as_str())
    }
    pub fn can_apply(&self, dimension: usize) -> bool {
        self.preview.as_ref().is_some_and(|p| {
            p.kind == ResultKind::Vector
                && p.request == self.request(dimension)
                && FieldExpr::parse(&p.formula).is_ok_and(|f| f.dimension == dimension)
        })
    }
    pub fn apply(&mut self, state: &mut State, dimension: usize) -> bool {
        // CAS edits the formula editor only. Sources and Induction have no
        // formula editor, so refuse before any dimension/vector validation.
        if state.field_mode != FieldMode::Formula {
            return false;
        }
        // CAS mutates the vector editor only. A direct call in scalar mode is
        // inert; there is no scalar CAS semantics in this task.
        if !state.is_vector_mode() {
            return false;
        }
        let current_dimension = cas_dimension(state);
        if dimension != current_dimension {
            return false;
        }
        let dimension = current_dimension;
        let Some(preview) = self.preview.as_ref() else {
            return false;
        };
        if preview.kind != ResultKind::Vector || preview.request != self.request(dimension) {
            return false;
        }
        let Ok(field) = FieldExpr::parse(&preview.formula) else {
            return false;
        };
        if field.dimension != dimension {
            return false;
        }
        let formula = preview.formula.clone();
        state.set_formula(formula)
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// The dimension the CAS editor previews. Formula mode keeps its historical
/// authoritative 2D/3D app-mode source (not the field's arity); Sources mode has
/// no formula editor but its field is always three-dimensional.
fn cas_dimension(state: &State) -> usize {
    match state.field_mode {
        FieldMode::Sources
        | FieldMode::Induction
        | FieldMode::Materials
        | FieldMode::Ode
        | FieldMode::Maxwell => state.active_dimension(),
        FieldMode::Formula => {
            if state.mode_3d {
                3
            } else {
                2
            }
        }
    }
}

fn components(input: &str) -> Result<Vec<&str>, String> {
    let trimmed = input.trim();
    let body = strip_list_wrapper(trimmed).unwrap_or(trimmed);
    let mut depth = 0usize;
    let mut start = 0;
    let mut out = Vec::new();
    for (i, c) in body.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                out.push(body[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(body[start..].trim());
    if out.len() == 1 {
        input
            .trim()
            .parse::<meval::Expr>()
            .map_err(|e| e.to_string())?;
    } else if out.len() != 2 && out.len() != 3 {
        return Err(format!(
            "Expected a scalar or a 2- or 3-component vector; found {} components.",
            out.len()
        ));
    }
    Ok(out)
}

fn strip_list_wrapper(input: &str) -> Option<&str> {
    if !input.starts_with('(') || !input.ends_with(')') {
        return None;
    }
    let mut depth = 0usize;
    for (index, ch) in input.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 && index + ch.len_utf8() != input.len() {
                    return None;
                }
            }
            _ => {}
        }
    }
    (depth == 0).then(|| &input[1..input.len() - 1])
}
fn derive(r: &Request) -> Result<(String, ResultKind), String> {
    if r.dimension != 2 && r.dimension != 3 {
        return Err("Dimension must be 2D or 3D.".into());
    }
    let cs = components(&r.input)?;
    let scalar = cs.len() == 1;
    let vector = || {
        if cs.len() == r.dimension {
            Ok(())
        } else {
            Err(format!(
                "{} requires exactly {} vector components for the current {}D mode.",
                r.operation.label(),
                r.dimension,
                r.dimension
            ))
        }
    };
    let output = match r.operation {
        Operation::Differentiate => {
            if !scalar {
                vector()?;
            }
            let values = cs
                .iter()
                .map(|c| cas::differentiate(c, r.variable.adapter()).map_err(|e| e.to_string()))
                .collect::<Result<Vec<_>, _>>()?;
            if scalar {
                (values[0].clone(), ResultKind::Scalar)
            } else {
                (format!("({})", values.join(", ")), ResultKind::Vector)
            }
        }
        Operation::Gradient => {
            if !scalar {
                return Err("Gradient requires a scalar formula.".into());
            }
            (
                format!(
                    "({})",
                    cas::gradient(cs[0], r.dimension)
                        .map_err(|e| e.to_string())?
                        .join(", ")
                ),
                ResultKind::Vector,
            )
        }
        Operation::Divergence => {
            vector()?;
            (
                cas::divergence(&cs.to_vec(), r.dimension).map_err(|e| e.to_string())?,
                ResultKind::Scalar,
            )
        }
        Operation::Curl => {
            vector()?;
            let values = cas::curl(&cs.to_vec(), r.dimension).map_err(|e| e.to_string())?;
            if r.dimension == 2 {
                (values[0].clone(), ResultKind::Scalar)
            } else {
                (format!("({})", values.join(", ")), ResultKind::Vector)
            }
        }
    };
    Ok(output)
}

pub fn show(ui: &mut egui::Ui, state: &mut State, panel: &mut CasPanelState) -> Option<CasAction> {
    let disabled_mode = matches!(
        state.field_mode,
        FieldMode::Induction | FieldMode::Ode | FieldMode::Materials | FieldMode::Maxwell
    );
    if disabled_mode {
        ui.label("CAS edits the formula field; switch to Formula.");
    }
    ui.add_enabled_ui(!disabled_mode, |ui| show_enabled(ui, state, panel))
        .inner
}

fn show_enabled(
    ui: &mut egui::Ui,
    state: &mut State,
    panel: &mut CasPanelState,
) -> Option<CasAction> {
    let sources_mode = state.field_mode == FieldMode::Sources;
    let dimension = cas_dimension(state);
    panel.refresh(dimension);
    let content_width = ui.available_width();
    let reset = ui
        .button("Reset")
        .on_hover_text("Clear CAS scratch state without changing the plotted field")
        .clicked();
    ui.label("Input (scalar or vector)");
    let input_width = ui.available_width();
    let changed = ui
        .add(egui::TextEdit::singleline(&mut panel.input).desired_width(input_width))
        .on_hover_text(
            "Enter a scalar or vector formula, or an action such as \
             `domain 2, 2, 2`, `density 20`, `colors turbo magnitude linear`, \
             `camera fit` or `pause`",
        )
        .changed();
    ui.label("Operation");
    egui::ComboBox::from_id_salt("cas_operation")
        .selected_text(panel.operation.label())
        // Do not impose egui's default combo-width floor on a narrow
        // Controls panel. Both the button and menu text may wrap.
        .width(0.0)
        .wrap()
        .show_ui(ui, |ui| {
            // Combo menus otherwise override wrapping with Extend.
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            for op in Operation::ALL {
                if ui
                    .selectable_value(&mut panel.operation, op, op.label())
                    .on_hover_text("Choose a supported symbolic operation")
                    .changed()
                {
                    panel.last_request = None;
                }
            }
        });
    if panel.operation == Operation::Differentiate {
        ui.horizontal_wrapped(|ui| {
            ui.label("Variable");
            for var in Variable::ALL {
                if ui
                    .selectable_value(&mut panel.variable, var, var.label())
                    .on_hover_text("Differentiate with respect to this variable")
                    .changed()
                {
                    panel.last_request = None;
                }
            }
        });
    }
    if changed {
        panel.last_request = None;
    }
    panel.refresh(dimension);
    let mut apply_action = None;
    if let Some(error) = &panel.action_error {
        ui.colored_label(ui.visuals().error_fg_color, format!("Error: {error}"));
    }
    if let Some(action) = panel.action.clone() {
        ui.label(format!("Preview — {}", cas_actions::preview(&action)));
        if ui
            .button("Apply action")
            .on_hover_text("Apply this one validated action as a single undoable edit")
            .clicked()
        {
            apply_action = Some(action);
        }
    }
    if let Some(rejection) = &panel.action_rejection {
        ui.colored_label(
            ui.visuals().error_fg_color,
            format!("Rejected: {rejection}"),
        );
    }
    if let Some(message) = &panel.action_message {
        ui.label(format!("Applied: {message}"));
    }
    ui.label(format!("Dimension: {}D (current app mode)", dimension));
    let vector_mode = state.is_vector_mode();
    if let Some(p) = panel.preview.clone() {
        ui.label(format!(
            "Result — {}",
            if p.kind == ResultKind::Scalar {
                "Scalar"
            } else {
                "Vector"
            }
        ))
        .on_hover_text("Result kind is determined by the operation, not component count");
        let mut result = p.formula.clone();
        ui.add(
            egui::TextEdit::multiline(&mut result)
                .desired_width(content_width)
                .desired_rows(2)
                .interactive(false),
        );
        ui.horizontal_wrapped(|ui| {
            if ui
                .button("Copy")
                .on_hover_text("Copy the complete current preview formula")
                .clicked()
            {
                ui.ctx().copy_text(p.formula.clone());
            }
            if p.kind == ResultKind::Vector {
                let enabled = !sources_mode && vector_mode && panel.can_apply(dimension);
                let response = ui
                    .add_enabled(enabled, egui::Button::new("Apply vector"))
                    .on_hover_text("Replace the plotted field with this valid vector formula");
                let response = if sources_mode {
                    response
                        .on_disabled_hover_text("CAS edits the formula field; switch to Formula.")
                } else if !vector_mode {
                    response.on_disabled_hover_text(
                        "CAS apply is vector-only; return to Vector field mode",
                    )
                } else {
                    response
                };
                if response.clicked() {
                    panel.apply(state, dimension);
                }
            }
        });
    }
    if sources_mode {
        ui.label("CAS edits the formula field; switch to Formula.");
    } else if !vector_mode
        && panel
            .preview
            .as_ref()
            .is_some_and(|p| p.kind == ResultKind::Vector)
    {
        ui.label("CAS apply is vector-only; return to Vector field mode to apply this result.");
    }
    if let Some(err) = &panel.error {
        ui.colored_label(ui.visuals().error_fg_color, format!("Error: {err}"));
    }
    if reset {
        panel.reset();
    }
    apply_action
}
