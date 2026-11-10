//! Native-only symbolic formula tools. All editor state is transient.
use eframe::egui;

use crate::{cas, field::FieldExpr, viewmodel::State};

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
    last_request: Option<Request>,
}
impl Default for CasPanelState {
    fn default() -> Self {
        Self {
            input: "x".into(),
            operation: Operation::Differentiate,
            variable: Variable::X,
            preview: None,
            error: None,
            last_request: None,
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
        let current_dimension = if state.mode_3d { 3 } else { 2 };
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

fn components(input: &str) -> Result<Vec<&str>, String> {
    let body = input
        .trim()
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or(input.trim());
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

pub fn show(ui: &mut egui::Ui, state: &mut State, panel: &mut CasPanelState) {
    panel.refresh(if state.mode_3d { 3 } else { 2 });
    let dimension = if state.mode_3d { 3 } else { 2 };
    let mut reset = false;
    egui::CollapsingHeader::new("Symbolic tools (native)").default_open(false).show(ui, |ui| {
        ui.horizontal(|ui| { ui.label("Symbolic tools").on_hover_text("Derive formulas symbolically; preview and copy before explicitly applying a vector result."); ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { reset = ui.button("Reset").on_hover_text("Clear CAS scratch state without changing the plotted field").clicked(); }); });
        ui.label("Input (scalar or vector)");
        let changed = ui.add(egui::TextEdit::singleline(&mut panel.input).desired_width(f32::INFINITY)).on_hover_text("Enter a scalar formula or a comma-separated vector formula").changed();
        ui.horizontal(|ui| { ui.label("Operation"); for op in Operation::ALL { if ui.selectable_value(&mut panel.operation, op, op.label()).on_hover_text("Choose a supported symbolic operation").changed() { panel.last_request = None; } } });
        if panel.operation == Operation::Differentiate { ui.horizontal(|ui| { ui.label("Variable"); for var in Variable::ALL { if ui.selectable_value(&mut panel.variable, var, var.label()).on_hover_text("Differentiate with respect to this variable").changed() { panel.last_request = None; } } }); }
        if changed { panel.last_request = None; }
        panel.refresh(dimension);
        ui.label(format!("Dimension: {}D (current app mode)", dimension));
        if let Some(p) = panel.preview.clone() { ui.label(format!("Result — {}", if p.kind == ResultKind::Scalar { "Scalar" } else { "Vector" })).on_hover_text("Result kind is determined by the operation, not component count"); let mut result = p.formula.clone(); ui.add(egui::TextEdit::multiline(&mut result).desired_rows(2).interactive(false)); ui.horizontal(|ui| { if ui.button("Copy").on_hover_text("Copy the complete current preview formula").clicked() { ui.ctx().copy_text(p.formula.clone()); }
            if p.kind == ResultKind::Vector && ui.add_enabled(panel.can_apply(dimension), egui::Button::new("Apply vector")).on_hover_text("Replace the plotted field with this valid vector formula").clicked() { panel.apply(state, dimension); } }); }
        if let Some(err) = &panel.error { ui.colored_label(ui.visuals().error_fg_color, format!("Error: {err}")); }
    });
    if reset {
        panel.reset();
    }
}
