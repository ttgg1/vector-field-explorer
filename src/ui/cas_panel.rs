//! Native-only symbolic formula tools. All editor state is transient.
use eframe::egui;

use crate::cas_actions::{self, CasAction, CasActionError};
use crate::scalar_analysis::{
    ANALYTIC_DIPOLE_ROUTE_NOTE, NATIVE_SYMBOLIC_NOTE, potential_field_spec,
};
use crate::scalar_view::{sampled_layer_label, scalar_view_expr, slice_label, slice_valid_region};
use crate::ui::cas_preview::{self, PREVIEW_MAX_EDGE, PreviewCache, PreviewImage};
use crate::{
    cas, field::FieldExpr, scalar::ScalarContext, source_scene::FieldMode, viewmodel::State,
};

/// Where a scalar CAS result is plotted in the active scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneTarget {
    /// Plot the result as scalar contours on the active slice.
    Contours,
    /// Overlay the derived electric field `E = -∇V` on the active slice.
    PotentialField,
}

impl SceneTarget {
    /// The user-facing button/message name.
    pub fn label(self) -> &'static str {
        match self {
            Self::Contours => "Plot contours",
            Self::PotentialField => "Overlay E = -∇V",
        }
    }

    /// The one named history edit this application records.
    pub fn edit_label(self) -> &'static str {
        match self {
            Self::Contours => "plot CAS contours",
            Self::PotentialField => "overlay E = -∇V",
        }
    }
}

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
    /// The last scalar result applied to the active scene, with its region note.
    pub scene_message: Option<String>,
    /// The rejection from plotting a scalar result into the scene.
    pub scene_error: Option<String>,
    /// One named history edit reported to the app after a scene application.
    scene_edit: Option<&'static str>,
    /// Show a typeset rendering of the preview when possible (issue #171).
    /// The canonical linear string is always kept for Copy and Apply.
    pub typeset_output: bool,
    last_request: Option<Request>,
    /// The input text the apply result belongs to, so a new edit clears it.
    last_action_input: Option<String>,
    /// The last bounded scalar heatmap; resampled only when its key changes.
    preview_image: Option<PreviewImage>,
    preview_image_key: Option<u64>,
    /// The texture cached from [`Self::preview_image`].
    preview_cache: PreviewCache,
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
            scene_message: None,
            scene_error: None,
            scene_edit: None,
            typeset_output: true,
            last_request: None,
            last_action_input: None,
            preview_image: None,
            preview_image_key: None,
            preview_cache: PreviewCache::default(),
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
            self.scene_message = None;
            self.scene_error = None;
            self.scene_edit = None;
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
    /// The cached result-preview texture, if a scalar result has been painted.
    #[doc(hidden)]
    pub fn preview_texture(&self) -> Option<&egui::TextureHandle> {
        self.preview_cache.texture()
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
    /// The honest provenance shown beside a result: the expression is the
    /// analytic symbolic result, while every drawn value is sampled.
    pub fn preview_provenance(&self) -> &'static str {
        "Analytic symbolic result; plotted values are sampled on the active slice with the \
         committed parameters and current time."
    }

    /// The valid region for the current result: the live slice and its holes.
    pub fn preview_valid_region(&self, state: &State) -> String {
        let frame = state.scalar_view_frame();
        match self.preview.as_ref() {
            Some(preview) if preview.kind == ResultKind::Scalar => {
                match scalar_view_expr(&preview.formula, &state.parameters) {
                    Ok(_) => format!(
                        "{} {}",
                        sampled_layer_label(&frame, f64::from(state.time)),
                        slice_valid_region(&frame)
                    ),
                    Err(error) => error.to_string(),
                }
            }
            Some(_) => format!(
                "Components are sampled on the {} at t = {:.3}; each arrow uses the same \
                 committed parameters.",
                slice_label(&frame),
                state.time
            ),
            None => "No current result.".to_owned(),
        }
    }

    /// Plot the current scalar result into the active scene.
    ///
    /// Contours replace only the scalar-view layer that draws them and leave
    /// the CAS input and the physics model source untouched. The potential
    /// overlay reuses the shipped potential-gradient seam and leaves the
    /// scalar source untouched. Both bind the current committed parameters,
    /// simulated time and live slice; neither samples during this call beyond
    /// the ordinary visible refresh.
    pub fn apply_scene(
        &mut self,
        state: &mut State,
        dimension: usize,
        target: SceneTarget,
    ) -> Result<String, String> {
        self.scene_error = None;
        let current_dimension = cas_dimension(state);
        if dimension != current_dimension {
            return Err(format!(
                "the CAS result targets {dimension}D but the app is in {current_dimension}D"
            ));
        }
        let Some(preview) = self.preview.as_ref() else {
            return Err("there is no current CAS result to plot".to_owned());
        };
        if preview.kind != ResultKind::Scalar || preview.request != self.request(current_dimension)
        {
            return Err("only the current scalar result can be plotted".to_owned());
        }
        if state.field_mode != FieldMode::Formula || !state.is_vector_mode() {
            return Err(
                "switch to the Formula vector view to plot a CAS result on the scene".to_owned(),
            );
        }
        let formula = preview.formula.clone();
        scalar_view_expr(&formula, &state.parameters).map_err(|error| error.to_string())?;
        let frame = state.scalar_view_frame();
        let slice = slice_label(&frame);
        let time = f64::from(state.time);
        let region = slice_valid_region(&frame);
        match target {
            SceneTarget::Contours => {
                state.set_scalar_source(ScalarContext::Surface, formula.clone());
                if state.scalar_contour_levels() == 0 {
                    state
                        .set_scalar_contour_levels(8)
                        .map_err(|error| error.to_string())?;
                }
                state.refresh_scalar_view(true);
                self.scene_message = Some(format!(
                    "Plotting analytic contours of {formula} on the {slice}; values are \
                     sampled with the committed parameters at t = {time:.3}. {region} \
                     The CAS input and the physics model source are unchanged."
                ));
            }
            SceneTarget::PotentialField => {
                if !state.scalar_view_applicable() {
                    return Err(
                        "the scalar overlay needs a valid surface source; plot the result as \
                         contours first or fix the current scalar source"
                            .to_owned(),
                    );
                }
                let spec = potential_field_spec(formula.clone(), frame)?;
                state
                    .set_gradient(Some(spec))
                    .map_err(|error| error.to_string())?;
                state.refresh_scalar_analysis(true);
                self.scene_message = Some(format!(
                    "Overlaying E = -∇V from analytic V = {formula} on the {slice}. \
                     Arrows point downhill: the minus sign is applied, so E points from \
                     higher to lower V. Values are sampled with the committed parameters \
                     at t = {time:.3}; the scalar source is unchanged. {region}"
                ));
            }
        }
        self.scene_edit = Some(target.edit_label());
        Ok(self.scene_message.clone().unwrap_or_default())
    }

    /// Take the one named history edit the app should record for this frame.
    pub fn take_scene_edit(&mut self) -> Option<&'static str> {
        self.scene_edit.take()
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

/// Palette window for the teaching thumbnail. It is deliberately fixed: the
/// caption says the preview is normalized, not an SI-scaled plot.
const PREVIEW_RANGE: [f64; 2] = [-1.0, 1.0];

/// Paint the scalar heatmap under the result, resampling only on a key change.
///
/// A heatmap build failure is intentionally silent: the scalar result text still
/// conveys the answer, and storing no image keeps a stale texture off screen.
///
/// Exposed as a doc-hidden seam so the integration suite can prove a
/// parameter-only edit rebuilds the painted texture; ordinary callers use
/// [`show`].
#[doc(hidden)]
pub fn show_scalar_preview(
    ui: &mut egui::Ui,
    state: &State,
    panel: &mut CasPanelState,
    formula: &str,
) {
    let frame = state.scalar_view_frame();
    let time = f64::from(state.time);
    let Ok(expr) = scalar_view_expr(formula, &state.parameters) else {
        panel.preview_image = None;
        panel.preview_image_key = None;
        panel.preview_cache.clear();
        ui.colored_label(
            ui.visuals().error_fg_color,
            "This result cannot be sampled on the active slice.",
        );
        return;
    };
    let key = cas_preview::slice_preview_key(
        formula,
        &state.parameters,
        &frame,
        time,
        PREVIEW_RANGE,
        state.colormap,
    );
    if panel.preview_image_key != Some(key) {
        panel.preview_image_key = Some(key);
        // Silent on failure: the result text is the answer and `None` drops stale texels.
        panel.preview_image =
            cas_preview::scalar_slice_heatmap(&expr, &frame, time, PREVIEW_RANGE, state.colormap)
                .ok();
    }
    panel
        .preview_cache
        .sync(ui.ctx(), panel.preview_image.as_ref(), key);
    if let Some(texture) = panel.preview_cache.texture() {
        let side = ui.available_width().clamp(1.0, PREVIEW_MAX_EDGE as f32);
        ui.add(egui::Image::new((texture.id(), egui::vec2(side, side))));
        ui.label(format!(
            "Normalized teaching preview — not SI · {} at t = {time:.3} with the committed parameters",
            slice_label(&frame)
        ));
    }
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
    ui.small(NATIVE_SYMBOLIC_NOTE);
    ui.small(ANALYTIC_DIPOLE_ROUTE_NOTE);
    let mut apply_action = None;
    let mut scene_target = None;
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
        ui.small(panel.preview_provenance());
        ui.small(panel.preview_valid_region(state));
        if let Some(error) = &panel.scene_error {
            ui.colored_label(ui.visuals().error_fg_color, format!("Scene: {error}"));
        }
        if let Some(message) = &panel.scene_message {
            ui.label(format!("Scene: {message}"));
        }
        ui.checkbox(&mut panel.typeset_output, "Typeset result")
            .on_hover_text("Show this result as typeset math when the expression is supported");
        if panel.typeset_output {
            let tex = crate::cas_tex::linear_to_tex(&p.formula);
            crate::ui::math_view::paint_tex_or_linear(ui, tex.as_deref(), &p.formula);
        } else {
            crate::ui::math_view::show_linear(ui, &p.formula);
        }
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
        if p.kind == ResultKind::Scalar {
            show_scalar_preview(ui, state, panel, &p.formula);
            ui.horizontal_wrapped(|ui| {
                let scene_ready = !sources_mode && vector_mode;
                let contours = ui
                    .add_enabled(
                        scene_ready,
                        egui::Button::new(SceneTarget::Contours.label()),
                    )
                    .on_hover_text(
                        "Plot this analytic result as contours on the active slice; the CAS \
                         input and the physics model source stay unchanged",
                    );
                let contours = if scene_ready {
                    contours
                } else {
                    contours.on_disabled_hover_text(
                        "switch to the Formula vector view to plot on the scene",
                    )
                };
                if contours.clicked() {
                    scene_target = Some(SceneTarget::Contours);
                }
                let overlay = ui
                    .add_enabled(
                        scene_ready,
                        egui::Button::new(SceneTarget::PotentialField.label()),
                    )
                    .on_hover_text(
                        "Overlay E = -∇V from this potential without replacing the scalar \
                         source; arrows point downhill",
                    );
                let overlay = if scene_ready {
                    overlay
                } else {
                    overlay.on_disabled_hover_text(
                        "switch to the Formula vector view to overlay on the scene",
                    )
                };
                if overlay.clicked() {
                    scene_target = Some(SceneTarget::PotentialField);
                }
            });
        }
    }
    if let Some(target) = scene_target {
        match panel.apply_scene(state, dimension, target) {
            Ok(_) => panel.scene_error = None,
            Err(error) => panel.scene_error = Some(error),
        }
    }
    // Vector results and structured errors keep the text-only output; drop any
    // texture left by a previous scalar preview.
    if !panel
        .preview
        .as_ref()
        .is_some_and(|p| p.kind == ResultKind::Scalar)
    {
        panel.preview_image = None;
        panel.preview_image_key = None;
        panel.preview_cache.clear();
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
