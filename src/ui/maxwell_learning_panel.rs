//! Slice B: the Maxwell learning and residual panel.
//!
//! The panel is a teaching and diagnostics aid only. It shows both statement
//! forms of the four Maxwell equations, a symbol glossary and the active limit,
//! then optionally checks one sample point against the selected equation. The
//! check reuses [`crate::maxwell_residual::residual`] and
//! [`crate::maxwell::maxwell_field_with_bem`]; it adds no solver and evaluates
//! the field only when the user presses `Check residual`.
//!
//! Every numeric term comes from a central difference of actual field
//! evaluations, with an `h` versus `h/2` error estimate. Terms the teaching
//! cases cannot supply (volume charge, current density and every time
//! derivative) stay `Unsupported`, and a non-finite field or probe value stays
//! `Singular`; neither is ever replaced by a fabricated zero.

use eframe::egui;
use glam::Vec3;

use crate::maxwell::{
    maxwell_family, maxwell_field_with_bem, BemSolution, MaxwellCase, MaxwellError, MaxwellFamily,
    MaxwellScene, MAXWELL_BEM_PENDING_WARNING, MAXWELL_BEM_STALE_WARNING,
};
use crate::maxwell_learning::{spec, EquationForm, EquationSpec, MaxwellEquation};
use crate::maxwell_residual::{residual, FieldTerms, Residual, TermStatus};

/// The honest scope line, always visible in the section.
pub const MAXWELL_LEARNING_LIMITATION: &str = "These teaching cases are static or quasistatic; time-derivative terms are not available, so this is a partial check, not a full Maxwell verification.";

/// The documented normalized-residual scale.
pub const MAXWELL_NORMALIZED_SCALE: &str =
    "Normalized residual (|residual| / max(1, |lhs|, |rhs|); vector terms are scaled first)";

/// Clarifies that the numeric readout is always the pointwise differential form,
/// independent of which statement form is selected for display.
pub const MAXWELL_POINTWISE_NOTE: &str = "Numeric check: pointwise differential form at the probe point (the Integral/Differential selection changes only the displayed statement, not the check).";

/// Provenance of one term used by the selected equation.
#[derive(Debug, Clone, PartialEq)]
pub struct TermNote {
    /// Exact symbol the equation uses.
    pub symbol: &'static str,
    /// Whether the value came from a central difference.
    pub central_difference: bool,
    pub status: TermStatus,
}

/// Per-term explanation attached to a [`ResidualReport`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TermExplanation {
    pub terms: Vec<TermNote>,
}

/// One press-only residual check: the model result plus its term provenance and
/// the absolute (un-normalized) residual magnitude.
#[derive(Debug, Clone, PartialEq)]
pub struct ResidualReport {
    pub residual: Residual,
    pub terms: TermExplanation,
    /// Absolute residual magnitude; `None` whenever any required term is
    /// missing or singular.
    pub absolute: Option<f64>,
}

/// Session-only state of the learning section.
#[derive(Debug, Clone)]
pub struct MaxwellLearningPanel {
    pub equation: MaxwellEquation,
    pub form: EquationForm,
    pub probe: [f64; 3],
    pub report: Option<ResidualReport>,
    pub message: Option<String>,
}

impl Default for MaxwellLearningPanel {
    fn default() -> Self {
        Self {
            equation: MaxwellEquation::GaussElectric,
            form: EquationForm::Differential,
            probe: [0.0; 3],
            report: None,
            message: None,
        }
    }
}

impl MaxwellLearningPanel {
    /// Draw the section. Field work happens only when `Check residual` is
    /// pressed; a plain frame never evaluates the field or changes the report.
    pub fn show(&mut self, ui: &mut egui::Ui, scene: &mut MaxwellScene, bem: Option<&BemSolution>) {
        ui.label("Maxwell equations (learning and diagnostics)");
        ui.label("Normalized teaching units; this is a teaching aid, not a solver.");

        ui.label("Equation");
        ui.horizontal_wrapped(|ui| {
            for equation in MaxwellEquation::ALL {
                if ui
                    .selectable_label(self.equation == equation, equation.title())
                    .clicked()
                    && self.equation != equation
                {
                    self.equation = equation;
                    self.report = None;
                    self.message = None;
                }
            }
        });

        ui.label("Form");
        ui.horizontal_wrapped(|ui| {
            for (form, label) in [
                (EquationForm::Integral, "Integral"),
                (EquationForm::Differential, "Differential"),
            ] {
                if ui.selectable_label(self.form == form, label).clicked() {
                    self.form = form;
                }
            }
        });

        let record = spec(self.equation);
        show_statements(ui, &record, self.form);

        ui.label("Symbols");
        for (symbol, meaning) in record.symbols {
            ui.label(format!("{symbol}: {meaning}"));
        }

        ui.label("Assumptions");
        ui.label(record.assumptions);

        ui.label(MAXWELL_LEARNING_LIMITATION);

        if let Some(warning) = conductor_blocker(scene, bem) {
            ui.colored_label(ui.visuals().warn_fg_color, warning);
        }

        ui.label("Probe point (normalized display units)");
        let mut probe_changed = false;
        for (axis, value) in ["x", "y", "z"].into_iter().zip(self.probe.iter_mut()) {
            // Never paint a non-finite probe value; the check still sees the
            // original value and reports Singular.
            let mut shown = if value.is_finite() { *value } else { 0.0 };
            let response = ui
                .horizontal(|ui| {
                    ui.label(format!("Probe {axis}"));
                    ui.add(egui::DragValue::new(&mut shown).speed(0.01))
                })
                .inner;
            if response.changed() {
                *value = shown;
                probe_changed = true;
            }
        }
        if probe_changed {
            self.report = None;
            self.message = None;
        }

        if ui
            .button("Check residual")
            .on_hover_text("Evaluate the selected equation at this one point")
            .clicked()
        {
            self.check(scene, bem);
        }

        if let Some(report) = self.report.as_ref() {
            ui.separator();
            show_report(ui, report);
        } else {
            ui.label("No residual check run yet.");
        }

        if let Some(message) = self.message.as_ref() {
            ui.colored_label(ui.visuals().warn_fg_color, message);
        }
    }

    /// Run the bounded check for the current selection. This is public so the
    /// press-only UI and tests share exactly one evaluation path.
    pub fn check(&mut self, scene: &MaxwellScene, bem: Option<&BemSolution>) {
        if let Some(warning) = conductor_blocker(scene, bem) {
            self.report = None;
            self.message = Some(warning.to_owned());
            return;
        }
        let (terms, explanation, message) = sample_terms(self.equation, scene, self.probe, bem);
        let residual = residual(self.equation, &terms);
        self.report = Some(ResidualReport {
            residual,
            terms: explanation,
            absolute: absolute_residual(self.equation, &terms),
        });
        self.message = message;
    }
}

fn show_statements(ui: &mut egui::Ui, record: &EquationSpec, form: EquationForm) {
    ui.label(format!("Integral: {}", record.integral));
    ui.label(format!("Differential: {}", record.differential));
    ui.label(format!(
        "Selected form: {}",
        match form {
            EquationForm::Integral => "Integral",
            EquationForm::Differential => "Differential",
        }
    ));
}

fn show_report(ui: &mut egui::Ui, report: &ResidualReport) {
    let residual = &report.residual;
    ui.label(format!("Equation: {}", residual.equation.title()));
    ui.label(MAXWELL_POINTWISE_NOTE);
    match residual.normalized {
        Some(normalized) => ui.label(format!("{MAXWELL_NORMALIZED_SCALE}: {normalized:.3e}")),
        None => ui.label(format!("{MAXWELL_NORMALIZED_SCALE}: unavailable")),
    };
    match report.absolute {
        Some(absolute) => ui.label(format!("Absolute residual: {absolute:.3e}")),
        None => ui.label("Absolute residual: unavailable (a required term is missing or singular)"),
    };
    if let (Some(lhs), Some(rhs)) = (residual.lhs, residual.rhs) {
        ui.label(format!("Scalar terms: lhs {lhs:.3e} · rhs {rhs:.3e}"));
    }
    if let Some(vector) = residual.vector_residual {
        ui.label(format!(
            "Vector residual (scaled): [{:.3e}, {:.3e}, {:.3e}]",
            vector[0], vector[1], vector[2]
        ));
    }
    ui.label("Terms");
    for note in &report.terms.terms {
        ui.label(term_chip(note));
    }
    ui.label(&residual.explanation);
}

fn term_chip(note: &TermNote) -> String {
    match note.status {
        TermStatus::Analytic => format!("Analytic: {}", note.symbol),
        TermStatus::Numerical { error } => {
            if note.central_difference {
                format!("Numerical ± {error:.2e}: {}", note.symbol)
            } else {
                format!("Numerical ± {error:.2e}")
            }
        }
        TermStatus::Unsupported(symbol) => format!("Unsupported: {symbol}"),
        TermStatus::Singular(symbol) => format!("Singular: {symbol}"),
    }
}

fn conductor_blocker(scene: &MaxwellScene, bem: Option<&BemSolution>) -> Option<&'static str> {
    if scene.case != MaxwellCase::Conductor2D {
        return None;
    }
    match bem {
        None => Some(MAXWELL_BEM_PENDING_WARNING),
        Some(solution) if !solution.matches_scene(scene) => Some(MAXWELL_BEM_STALE_WARNING),
        Some(_) => None,
    }
}

#[derive(Debug, Clone, Copy)]
struct Stencil {
    div: f64,
    curl: [f64; 3],
}

fn sample_terms(
    equation: MaxwellEquation,
    scene: &MaxwellScene,
    probe: [f64; 3],
    bem: Option<&BemSolution>,
) -> (FieldTerms, TermExplanation, Option<String>) {
    let h = 1e-3 * learning_extent(scene).max(1e-6);
    let (coarse, mut message) = stencil(scene, probe, h, bem);
    let (fine, fine_message) = stencil(scene, probe, h * 0.5, bem);
    if message.is_none() {
        message = fine_message;
    }
    // The teaching seam exposes one family per scene: E for the electric cases
    // and B for the magnetic ones. Never reinterpret E as B or vice versa.
    let electric = maxwell_family(scene.case) == MaxwellFamily::Electric;
    let terms = FieldTerms {
        div_e: electric.then_some(coarse.div),
        div_b: (!electric).then_some(coarse.div),
        curl_e: electric.then_some(coarse.curl),
        curl_b: (!electric).then_some(coarse.curl),
        rho: None,
        j: None,
        de_dt: None,
        db_dt: None,
    };
    let error = StencilError {
        div: (coarse.div - fine.div).abs(),
        curl: max_abs3(sub3(coarse.curl, fine.curl)),
    };
    (terms, explanation(equation, &terms, error), message)
}

#[derive(Debug, Clone, Copy)]
struct StencilError {
    div: f64,
    curl: f64,
}

fn explanation(
    equation: MaxwellEquation,
    terms: &FieldTerms,
    error: StencilError,
) -> TermExplanation {
    let mut notes = Vec::new();
    match equation {
        MaxwellEquation::GaussElectric => {
            push_numeric(&mut notes, "div_e", terms.div_e, error.div);
            push_unsupported(&mut notes, "rho");
        }
        MaxwellEquation::GaussMagnetic => {
            push_numeric(&mut notes, "div_b", terms.div_b, error.div);
        }
        MaxwellEquation::Faraday => {
            push_numeric(&mut notes, "curl_e", terms.curl_e.map(norm3), error.curl);
            push_unsupported(&mut notes, "db_dt");
        }
        MaxwellEquation::AmpereMaxwell => {
            push_numeric(&mut notes, "curl_b", terms.curl_b.map(norm3), error.curl);
            push_unsupported(&mut notes, "j");
            push_unsupported(&mut notes, "displacement current (de_dt)");
        }
    }
    TermExplanation { terms: notes }
}

fn push_numeric(notes: &mut Vec<TermNote>, symbol: &'static str, value: Option<f64>, error: f64) {
    let status = match value {
        Some(value) if value.is_finite() && error.is_finite() => TermStatus::Numerical { error },
        Some(_) => TermStatus::Singular(symbol),
        None => TermStatus::Unsupported(symbol),
    };
    notes.push(TermNote {
        symbol,
        central_difference: value.is_some(),
        status,
    });
}

fn push_unsupported(notes: &mut Vec<TermNote>, symbol: &'static str) {
    notes.push(TermNote {
        symbol,
        central_difference: false,
        status: TermStatus::Unsupported(symbol),
    });
}

/// Build one six-offset central-difference stencil. The caller runs this twice
/// (`h` and `h/2`), so one press performs exactly twelve field evaluations.
fn stencil(
    scene: &MaxwellScene,
    probe: [f64; 3],
    h: f64,
    bem: Option<&BemSolution>,
) -> (Stencil, Option<String>) {
    let mut message = None;
    let mut at = |offset: [f64; 3]| -> [f64; 3] {
        let point = Vec3::new(
            (probe[0] + offset[0]) as f32,
            (probe[1] + offset[1]) as f32,
            (probe[2] + offset[2]) as f32,
        );
        match maxwell_field_with_bem(scene, point, bem) {
            Ok(value) if value.is_finite() => {
                [f64::from(value.x), f64::from(value.y), f64::from(value.z)]
            }
            Ok(_) => {
                message.get_or_insert_with(|| {
                    "Field evaluation returned a non-finite value.".to_owned()
                });
                [f64::NAN; 3]
            }
            Err(error) => {
                message.get_or_insert_with(|| error_text(&error));
                [f64::NAN; 3]
            }
        }
    };

    let xp = at([h, 0.0, 0.0]);
    let xm = at([-h, 0.0, 0.0]);
    let yp = at([0.0, h, 0.0]);
    let ym = at([0.0, -h, 0.0]);
    let zp = at([0.0, 0.0, h]);
    let zm = at([0.0, 0.0, -h]);

    let twice = 2.0 * h;
    let stencil = Stencil {
        div: (xp[0] - xm[0]) / twice + (yp[1] - ym[1]) / twice + (zp[2] - zm[2]) / twice,
        curl: [
            (zp[1] - zm[1]) / twice - (yp[2] - ym[2]) / twice,
            (xp[2] - xm[2]) / twice - (zp[0] - zm[0]) / twice,
            (yp[0] - ym[0]) / twice - (xp[1] - xm[1]) / twice,
        ],
    };
    (stencil, message)
}

fn absolute_residual(equation: MaxwellEquation, terms: &FieldTerms) -> Option<f64> {
    match equation {
        MaxwellEquation::GaussElectric => {
            let lhs = terms.div_e.filter(|value| value.is_finite())?;
            let rhs = terms.rho.filter(|value| value.is_finite())?;
            Some((lhs - rhs).abs())
        }
        MaxwellEquation::GaussMagnetic => {
            let lhs = terms.div_b.filter(|value| value.is_finite())?;
            Some(lhs.abs())
        }
        MaxwellEquation::Faraday => {
            let lhs = terms.curl_e.filter(|value| finite3(*value))?;
            let rhs = terms.db_dt.filter(|value| finite3(*value))?;
            Some(norm3(add3(lhs, rhs)))
        }
        MaxwellEquation::AmpereMaxwell => {
            let lhs = terms.curl_b.filter(|value| finite3(*value))?;
            let j = terms.j.filter(|value| finite3(*value))?;
            let de_dt = terms.de_dt.filter(|value| finite3(*value))?;
            Some(norm3(sub3(lhs, add3(j, de_dt))))
        }
    }
}

fn learning_extent(scene: &MaxwellScene) -> f64 {
    // Mirrors the private `case_extent` in `maxwell.rs`; the panel cannot see
    // that helper without changing the physics module.
    let extent = match scene.case {
        MaxwellCase::ParallelPlates => scene.separation,
        MaxwellCase::CoaxialElectric | MaxwellCase::CoaxialMagnetic => scene.outer_radius,
        MaxwellCase::HollowSphere
        | MaxwellCase::InfiniteWire
        | MaxwellCase::Solenoid
        | MaxwellCase::ImageSphere => scene.radius,
        MaxwellCase::ImagePlane => scene.charge_height.abs(),
        MaxwellCase::Conductor2D => match scene.outline {
            crate::maxwell::ConductorOutline::Circle { radius } => radius,
            crate::maxwell::ConductorOutline::Square { half_extent } => half_extent,
        },
    };
    extent.max(1.0)
}

fn error_text(error: &MaxwellError) -> String {
    match error {
        MaxwellError::InvalidScene(text)
        | MaxwellError::Unsupported(text)
        | MaxwellError::Unavailable(text) => text.clone(),
    }
}

fn finite3(value: [f64; 3]) -> bool {
    value.iter().all(|component| component.is_finite())
}

fn max_abs3(value: [f64; 3]) -> f64 {
    value
        .iter()
        .fold(0.0_f64, |largest, component| largest.max(component.abs()))
}

fn norm3(value: [f64; 3]) -> f64 {
    value
        .iter()
        .map(|component| component * component)
        .sum::<f64>()
        .sqrt()
}

fn add3(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] + right[0], left[1] + right[1], left[2] + right[2]]
}

fn sub3(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}
