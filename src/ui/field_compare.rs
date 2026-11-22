//! Session-only Candidate comparison sidebar section (issue #215).
//!
//! This section never touches the model, the scene, the document or history.
//! It parses a candidate beside the active formula field, gates the numerical
//! comparison behind an explicit prediction or Skip, and shows both readings at
//! two shared points in the same normalized teaching units.

use eframe::egui;
use glam::Vec3;

use crate::domain::Domain;
use crate::field::FieldExpr;
use crate::field_compare::{
    self, DirectionRelation, PointComparison, SideSample, TEACHING_UNITS_NOTE,
};
use crate::parameters::ParameterSet;
use crate::source_scene::FieldMode;
use crate::viewmodel::State;

/// The collapsed section's header.
pub const SECTION_TITLE: &str = "Candidate comparison";

/// The student's prediction for the sampled direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prediction {
    With,
    Against,
    Different,
}

impl Prediction {
    /// The button and reading label.
    pub fn label(self) -> &'static str {
        match self {
            Prediction::With => "with the model",
            Prediction::Against => "against the model",
            Prediction::Different => "differently from the model",
        }
    }

    fn relation(self) -> DirectionRelation {
        match self {
            Prediction::With => DirectionRelation::With,
            Prediction::Against => DirectionRelation::Against,
            Prediction::Different => DirectionRelation::Different,
        }
    }
}

/// Session-only drafts and gate state for the comparison.
#[derive(Debug, Clone)]
pub struct FieldCompareControls {
    /// The expression the user is editing.
    pub candidate_text: String,
    /// The last successfully applied candidate; never replaced by a parse error.
    pub candidate: Option<FieldExpr>,
    /// The last parse error, shown inline.
    pub error: String,
    /// Shared first sample point.
    pub point_a: Vec3,
    /// Shared second sample point (2× the first by default).
    pub point_b: Vec3,
    /// The chosen prediction, if any.
    pub prediction: Option<Prediction>,
    /// Whether the student explicitly skipped the prediction.
    pub skipped: bool,
    /// Point A when the prediction was made, so the grade stays tied to it.
    prediction_point_a: Option<Vec3>,
    /// Parameters the applied candidate was parsed with, for a stale warning.
    applied_parameters: ParameterSet,
}

impl Default for FieldCompareControls {
    fn default() -> Self {
        let point_a = Vec3::new(0.5, 0.5, 0.0);
        Self {
            candidate_text: String::new(),
            candidate: None,
            error: String::new(),
            point_a,
            point_b: point_a * 2.0,
            prediction: None,
            skipped: false,
            prediction_point_a: None,
            applied_parameters: ParameterSet::default(),
        }
    }
}

impl FieldCompareControls {
    /// Parse and apply the draft through the same parameter-aware parser as
    /// the model. Invalid text leaves the previous candidate intact.
    pub fn apply(&mut self, parameters: &ParameterSet) -> bool {
        match FieldExpr::parse_with_parameters(&self.candidate_text, parameters) {
            Ok(candidate) => {
                self.candidate = Some(candidate);
                self.applied_parameters = parameters.clone();
                self.error.clear();
                self.reset_gate();
                true
            }
            Err(error) => {
                self.error = format!("Candidate parse error: {error}");
                false
            }
        }
    }

    /// Record a prediction, revealing the comparison.
    pub fn predict(&mut self, prediction: Prediction) {
        self.prediction = Some(prediction);
        self.prediction_point_a = Some(self.point_a);
        self.skipped = false;
    }

    /// Explicitly skip the prediction and reveal the comparison.
    pub fn skip(&mut self) {
        self.prediction = None;
        self.prediction_point_a = None;
        self.skipped = true;
    }

    /// Whether the numerical comparison may be shown.
    pub fn revealed(&self) -> bool {
        self.prediction.is_some() || self.skipped
    }

    /// Whether the applied candidate was parsed with a different parameter set.
    pub fn parameters_stale(&self, parameters: &ParameterSet) -> bool {
        self.candidate.is_some() && self.applied_parameters != *parameters
    }

    fn reset_gate(&mut self) {
        self.prediction = None;
        self.prediction_point_a = None;
        self.skipped = false;
    }
}

/// The section belongs to the formula field in a vector view, like the other
/// formula-only vector sections.
pub fn visible(field_mode: FieldMode, vector_mode: bool) -> bool {
    vector_mode && field_mode == FieldMode::Formula
}

/// Draw the collapsed section. The body only samples while it is open.
pub fn show(ui: &mut egui::Ui, state: &State, controls: &mut FieldCompareControls) {
    egui::CollapsingHeader::new(SECTION_TITLE)
        .id_salt("candidate_comparison_section")
        .default_open(false)
        .show(ui, |ui| show_body(ui, state, controls));
}

/// Draw the section body without its collapsing header. Kept separate so the
/// gate and compact layout are testable without a window.
pub fn show_body(ui: &mut egui::Ui, state: &State, controls: &mut FieldCompareControls) {
    if !state.parse_error.is_empty() {
        ui.label(
            "The model has a parse error; the comparison uses the last model field that parsed.",
        );
    }

    ui.horizontal_wrapped(|ui| {
        ui.label("Candidate");
        ui.add(
            egui::TextEdit::singleline(&mut controls.candidate_text)
                .desired_width(96.0)
                .hint_text("(u, v) or (u, v, w)"),
        )
        .on_hover_text(
            "A candidate expression in x/y/z (and t), parsed with the active parameters",
        );
        if ui
            .button("Apply candidate")
            .on_hover_text("Parse the candidate; the active model field is never replaced")
            .clicked()
        {
            controls.apply(&state.parameters);
        }
    });

    if !controls.error.is_empty() {
        let error_color = ui.visuals().error_fg_color;
        ui.colored_label(error_color, &controls.error);
    }
    if controls.parameters_stale(&state.parameters) {
        ui.label(
            "Parameters changed since the candidate was applied; press Apply candidate to refresh it.",
        );
    }

    let Some(candidate) = controls.candidate.clone() else {
        ui.label("Enter a candidate expression and press Apply candidate.");
        return;
    };

    ui.add_space(ui.spacing().item_spacing.y);
    let domain = state.scene_domain();
    ui.horizontal_wrapped(|ui| point_row(ui, "Point A", &mut controls.point_a, &domain));
    ui.horizontal_wrapped(|ui| point_row(ui, "Point B", &mut controls.point_b, &domain));
    if ui
        .button("Use 2× A")
        .on_hover_text(
            "Sample Point B at twice Point A's distance from the origin, clamped to the visible domain",
        )
        .clicked()
    {
        controls.point_b = clamp_point(controls.point_a * 2.0, &domain);
    }
    ui.label(
        "Point B defaults to the radial 2× partner of Point A; both are labelled and use the same units and scale.",
    );

    if !controls.revealed() {
        ui.add_space(ui.spacing().item_spacing.y);
        ui.label(
            "Predict before comparing: does the candidate point with, against or differently from the model at these points?",
        );
        ui.horizontal_wrapped(|ui| {
            if ui.button("With model").clicked() {
                controls.predict(Prediction::With);
            }
            if ui.button("Against model").clicked() {
                controls.predict(Prediction::Against);
            }
            if ui.button("Differently").clicked() {
                controls.predict(Prediction::Different);
            }
            if ui
                .button("Skip prediction")
                .on_hover_text("Reveal the comparison without judging a prediction")
                .clicked()
            {
                controls.skip();
            }
        });
        ui.label("The numerical comparison stays hidden until you predict or skip.");
        return;
    }

    let report = field_compare::compare_samples(
        &state.field,
        &candidate,
        controls.point_a,
        controls.point_b,
        state.time as f64,
        &domain,
    );

    ui.add_space(ui.spacing().item_spacing.y);
    if let Some(prediction) = controls.prediction {
        // Grade at the point the student predicted, not at any later edit of
        // Point A; the comparison itself still shows the current points.
        let predicted_point = controls.prediction_point_a.unwrap_or(controls.point_a);
        let predicted = field_compare::compare_point(
            &state.field,
            &candidate,
            predicted_point,
            state.time as f64,
            &domain,
        );
        ui.label(format!("Prediction: {}.", prediction.label()));
        ui.label(format!(
            "The prediction was made at Point A ({}); the grade applies to that point.",
            point_text(predicted_point)
        ));
        if predicted.relation == DirectionRelation::Undefined {
            ui.label(
                "The predicted point's sampled direction is undefined, so the prediction is not graded.",
            );
        } else if prediction.relation() == predicted.relation {
            ui.label("Your prediction matches the first-point sampled direction at that point.");
        } else {
            ui.label(format!(
                "Your prediction does not match the first-point sampled reading at that point ({}).",
                predicted.relation.label()
            ));
        }
    } else if controls.skipped {
        ui.label("Prediction skipped; the comparison is shown without a prediction judgement.");
    }
    if ui
        .button("Change prediction")
        .on_hover_text("Hide the comparison and choose a prediction again")
        .clicked()
    {
        controls.reset_gate();
    }

    let first_label = format!("Point A ({})", point_text(controls.point_a));
    let second_label = format!("Point B ({})", point_text(controls.point_b));
    render_point(ui, &first_label, &report.first);
    render_point(ui, &second_label, &report.second);

    ui.add_space(ui.spacing().item_spacing.y);
    ui.label(&report.note);
    ui.label(TEACHING_UNITS_NOTE);
}

fn point_row(ui: &mut egui::Ui, label: &str, point: &mut Vec3, domain: &Domain) {
    ui.label(label);
    for (axis, value) in [
        ("x", &mut point.x),
        ("y", &mut point.y),
        ("z", &mut point.z),
    ] {
        let range = match axis {
            "x" => domain.min.x..=domain.max.x,
            "y" => domain.min.y..=domain.max.y,
            _ => domain.min.z..=domain.max.z,
        };
        ui.add(
            egui::DragValue::new(value)
                .speed(0.01)
                .range(range)
                .max_decimals(3),
        )
        .on_hover_text(format!(
            "{label} {axis}; constrained to the visible sampled domain"
        ));
    }
}

fn clamp_point(point: Vec3, domain: &Domain) -> Vec3 {
    point.clamp(domain.min, domain.max)
}

fn render_point(ui: &mut egui::Ui, label: &str, reading: &PointComparison) {
    ui.add_space(ui.spacing().item_spacing.y);
    ui.label(egui::RichText::new(label).strong());
    ui.label(format!("Model: {}", sample_text(&reading.model)));
    ui.label(format!("Candidate: {}", sample_text(&reading.candidate)));
    let difference = match (reading.difference, reading.difference_magnitude) {
        (Some(vector), Some(length)) => {
            format!("{} (|ΔF| = {})", vector_text(vector), number(length))
        }
        _ => "unavailable (one side cannot be sampled)".to_owned(),
    };
    ui.label(format!("Candidate - model: {difference}"));
    ui.label(format!(
        "|Model| = {}, |Candidate| = {}, |Candidate - model| = {}",
        optional_number(reading.model_magnitude),
        optional_number(reading.candidate_magnitude),
        optional_number(reading.difference_magnitude)
    ));
    let relative = match reading.relative_difference {
        Some(value) => format!("{} ({:.2}%)", number(value), value * 100.0),
        None => match &reading.relative_difference_note {
            Some(note) => format!("undefined: {note}"),
            None => "unavailable".to_owned(),
        },
    };
    ui.label(format!("Relative difference |ΔF| / |Model|: {relative}"));
    ui.label(format!("Direction: {}.", reading.relation.label()));
}

fn sample_text(sample: &SideSample) -> String {
    match sample {
        SideSample::Available(vector) => vector_text(*vector),
        SideSample::Unavailable(reason) => format!("unavailable ({reason})"),
    }
}

fn vector_text(vector: Vec3) -> String {
    format!(
        "({}, {}, {})",
        number(vector.x as f64),
        number(vector.y as f64),
        number(vector.z as f64)
    )
}

fn point_text(point: Vec3) -> String {
    format!(
        "{}, {}, {}",
        number(point.x as f64),
        number(point.y as f64),
        number(point.z as f64)
    )
}

fn optional_number(value: Option<f64>) -> String {
    match value {
        Some(value) => number(value),
        None => "unavailable".to_owned(),
    }
}

/// Compact, deterministic number text: four significant decimals for ordinary
/// values and scientific notation outside that range.
fn number(value: f64) -> String {
    if !value.is_finite() {
        return "not finite".to_owned();
    }
    if value == 0.0 {
        return "0".to_owned();
    }
    let magnitude = value.abs();
    if !(1.0e-3..1.0e4).contains(&magnitude) {
        return format!("{value:.3e}");
    }
    let mut text = format!("{value:.4}");
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    text
}
