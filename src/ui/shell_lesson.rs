//! Issue #225: the Griffiths 2.11 charged-shell route.
//!
//! The route wraps the shipped `MaxwellCase::HollowSphere` scene and the #219
//! [`GaussShellState`] measurement seam in a prediction-gated lesson. It adds
//! no physical case, quadrature rule, tolerance or durable probe surface: the
//! field, area and flux are the same samples the guided Gauss investigation
//! reports, in normalized teaching units.
//!
//! Everything here is session-only UI state. The app snapshots the starting
//! scene and camera before opening the route; this module only reports its
//! `Back/Exit` and `Keep exploring` actions.

use eframe::egui;

use crate::maxwell::MaxwellScene;
use crate::sources::FieldFamily;
use crate::sweep::{SweepKind, SweepSession};
use crate::ui::gauss_shell::{
    GAUSS_SHELL_INSIDE_NOTE, GAUSS_SHELL_INTERFACE_NOTE, GAUSS_SHELL_MAX_RADIUS,
    GAUSS_SHELL_MIN_RADIUS, GaussShellReading, GaussShellReadingKind, GaussShellReport,
    GaussShellState,
};
use crate::ui::griffiths::{
    GRIFFITHS_EXPLORE_FROM_HERE, GRIFFITHS_HIDE_HINT, GRIFFITHS_SHOW_HINT, GRIFFITHS_SKIP_STEP,
    GriffithsCheckpoint,
};
use crate::ui::sweep::{self, SweepUiAction};

/// Window title for the activity.
pub const SHELL_LESSON_TITLE: &str = "Griffiths 2.11 · Charged shell";
/// The explicit physics question.
pub const SHELL_QUESTION: &str = "A uniformly charged thin spherical shell has radius R. One probe sits just inside and one just outside. Which sees a field, and how do the field and net flux change as a concentric measuring surface moves from inside to outside and then doubles its exterior radius?";
/// The prediction prompt shown before any measured result.
pub const SHELL_PROMPT: &str = "Predict first, separately: inside the shell, what is the radial field E, and what is the net outward flux Phi?";
/// The first hint distinguishes the shell radius from the measuring radius.
pub const SHELL_HINT: &str = "R is the shell's fixed radius (the charged sphere in the scene); r is the measuring surface's radius that you move. R is the shell, r is the measuring surface, and changing r never changes R.";
/// Scope line for every displayed measurement.
pub const SHELL_UNITS_NOTE: &str = "Normalized teaching units sampled from the shipped Hollow sphere case; E, A and Phi are sampled estimates with quadrature uncertainty, not SI measurements.";
/// What the reveal adds about the source.
pub const SHELL_ENCLOSED_CHARGE_NOTE: &str = "Enclosed charge: a Gaussian surface with r < R encloses no net charge; with r > R it encloses the shell charge Q.";
/// What the reveal adds about field direction.
pub const SHELL_OUTWARD_NORMAL_NOTE: &str = "Outward normals point radially away from the centre. Outside the shell the radial field points outward; inside it is zero.";
/// Why the ideal interface is excluded rather than reported as a finite value.
pub const SHELL_BOUNDARY_NOTE: &str = GAUSS_SHELL_INTERFACE_NOTE;
/// Why the interior is zero rather than missing.
pub const SHELL_INSIDE_NOTE: &str = GAUSS_SHELL_INSIDE_NOTE;
/// Pairing/geometry explanation after the reveal.
pub const SHELL_REASONING: &str = "The exterior field falls as 1/r^2 while the spherical area grows as r^2, so their product (the net flux) stays the same. Different Gaussian radii are different surfaces, not different enclosed charges.";
/// Reminder that a comparison is sampled, not symbolic.
pub const SHELL_UNCERTAINTY_NOTE: &str = "The equal-flux comparison holds within the reported half-resolution quadrature uncertainty; it is a sampled estimate, not an exact symbolic cancellation.";

/// The learner's separate field prediction inside the shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellFieldPrediction {
    /// The ideal interior field is zero.
    ZeroInside,
    /// The ideal interior field is nonzero.
    NonzeroInside,
}

impl ShellFieldPrediction {
    /// Both choices, in prompt order.
    pub const ALL: [Self; 2] = [Self::ZeroInside, Self::NonzeroInside];

    /// The label shown on the prediction control.
    pub const fn label(self) -> &'static str {
        match self {
            Self::ZeroInside => "Inside: radial field E = 0",
            Self::NonzeroInside => "Inside: radial field E ≠ 0",
        }
    }
}

/// The learner's separate net-flux prediction inside the shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellFluxPrediction {
    /// The ideal interior net flux is zero.
    ZeroInside,
    /// The ideal interior net flux is nonzero.
    NonzeroInside,
}

impl ShellFluxPrediction {
    /// Both choices, in prompt order.
    pub const ALL: [Self; 2] = [Self::ZeroInside, Self::NonzeroInside];

    /// The label shown on the prediction control.
    pub const fn label(self) -> &'static str {
        match self {
            Self::ZeroInside => "Inside: net outward flux Phi = 0",
            Self::NonzeroInside => "Inside: net outward flux Phi ≠ 0",
        }
    }
}

/// What the app should do after the lesson window's one-shot action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellLessonAction {
    None,
    /// Restore the scene and camera captured before the lesson opened.
    Back,
    /// Keep the shell scene and close the lesson.
    KeepExploring,
    /// Temporarily explore from the current checkpoint.
    ExploreFromHere,
}

/// Session-only activity state. `Default` is inactive.
#[derive(Debug, Clone, Default)]
pub struct ShellLessonState {
    active: bool,
    field_prediction: Option<ShellFieldPrediction>,
    flux_prediction: Option<ShellFluxPrediction>,
    skipped: bool,
    revealed: bool,
    completed: bool,
    hint_visible: bool,
    gauss: GaussShellState,
    sweep: SweepSession,
}

impl ShellLessonState {
    /// Activate a fresh session for `scene`.
    pub fn activate(&mut self, scene: &MaxwellScene) {
        self.active = true;
        self.field_prediction = None;
        self.flux_prediction = None;
        self.skipped = false;
        self.revealed = false;
        self.completed = false;
        self.hint_visible = false;
        self.gauss.activate(scene);
        self.sweep = SweepSession::new(SweepKind::ShellFlux, scene);
    }

    /// Leave the activity. Completion is kept for the branch's session summary;
    /// the next [`Self::activate`] starts a fresh prediction and completion.
    pub fn deactivate(&mut self) {
        self.active = false;
        self.gauss.deactivate();
        self.sweep = SweepSession::default();
    }

    /// Whether the lesson window should be shown.
    pub fn active(&self) -> bool {
        self.active
    }

    /// Whether the learner has returned after comparing inside and outside.
    pub fn completed(&self) -> bool {
        self.completed
    }

    /// The current field prediction, if any.
    pub fn field_prediction(&self) -> Option<ShellFieldPrediction> {
        self.field_prediction
    }

    /// The current flux prediction, if any.
    pub fn flux_prediction(&self) -> Option<ShellFluxPrediction> {
        self.flux_prediction
    }

    /// Select or revise the field prediction before the reveal.
    pub fn select_field_prediction(&mut self, prediction: ShellFieldPrediction) {
        if !self.revealed {
            self.field_prediction = Some(prediction);
            self.skipped = false;
        }
    }

    /// Select or revise the flux prediction before the reveal.
    pub fn select_flux_prediction(&mut self, prediction: ShellFluxPrediction) {
        if !self.revealed {
            self.flux_prediction = Some(prediction);
            self.skipped = false;
        }
    }

    /// Skip both predictions without pretending they were made.
    pub fn skip_prediction(&mut self) {
        if !self.revealed {
            self.skipped = true;
            self.field_prediction = None;
            self.flux_prediction = None;
        }
    }

    /// Whether the predictions were skipped rather than supplied.
    pub fn skipped(&self) -> bool {
        self.skipped
    }

    /// Whether the result has been revealed.
    pub fn revealed(&self) -> bool {
        self.revealed
    }

    /// The current meaningful step, derived from the prediction/reveal gate.
    pub fn checkpoint(&self) -> GriffithsCheckpoint {
        if self.revealed {
            GriffithsCheckpoint::Result
        } else if self.can_reveal() {
            GriffithsCheckpoint::Reveal
        } else {
            GriffithsCheckpoint::Predict
        }
    }

    /// Whether the contextual hint is currently shown.
    pub fn hint_visible(&self) -> bool {
        self.hint_visible
    }

    /// Toggle the contextual hint without changing the prediction gate.
    pub fn toggle_hint(&mut self) {
        self.hint_visible = !self.hint_visible;
    }

    /// The reveal is gated on both predictions or an explicit skip.
    pub fn can_reveal(&self) -> bool {
        !self.revealed
            && (self.skipped || (self.field_prediction.is_some() && self.flux_prediction.is_some()))
    }

    /// Reveal the result. Returns false before both prompts are answered or
    /// explicitly skipped, so the prompt always precedes the result.
    pub fn reveal(&mut self) -> bool {
        if !self.can_reveal() {
            return false;
        }
        self.revealed = true;
        true
    }

    /// The measuring radius `r`, distinct from the scene's shell radius `R`.
    pub fn probe_radius(&self) -> f64 {
        self.gauss.probe_radius
    }

    /// Move the measuring surface. Non-finite values are refused by #219's
    /// bounded radius control.
    pub fn set_probe_radius(&mut self, value: f64) -> bool {
        self.gauss.set_probe_radius(value)
    }

    /// The session-only sweep controls owned by this lesson.
    pub fn sweep(&self) -> &SweepSession {
        &self.sweep
    }

    /// The session-only sweep controls owned by this lesson.
    pub fn sweep_mut(&mut self) -> &mut SweepSession {
        &mut self.sweep
    }

    /// Set `r = 2r`, leaving the shell radius `R` untouched.
    pub fn double_probe_radius(&mut self) -> bool {
        self.gauss.double_probe_radius()
    }

    /// The live `r` and `2r` readings from #219's shared seam.
    pub fn report_for(
        &mut self,
        scene: &MaxwellScene,
        family: FieldFamily,
        time: f64,
    ) -> &GaussShellReport {
        self.gauss.report_for(scene, family, time)
    }

    /// Pin one reading snapshot. A third replaces the oldest, as in #219.
    pub fn pin(&mut self, reading: GaussShellReading) {
        self.gauss.pin(reading);
    }

    /// The pinned readings in pin order.
    pub fn pinned(&self) -> &[GaussShellReading] {
        self.gauss.pinned()
    }

    /// Remove both pinned readings.
    pub fn clear_pinned(&mut self) {
        self.gauss.clear_pinned();
    }

    /// Whether one pinned reading is interior and another is exterior. Only
    /// that comparison completes the activity.
    pub fn comparison_ready(&self, scene: &MaxwellScene) -> bool {
        let mut inside = false;
        let mut outside = false;
        for reading in self.gauss.pinned() {
            match &reading.kind {
                GaussShellReadingKind::Interior { .. } if reading.radius < scene.radius => {
                    inside = true;
                }
                GaussShellReadingKind::Exterior { .. } if reading.radius > scene.radius => {
                    outside = true;
                }
                _ => {}
            }
        }
        inside && outside
    }

    /// Mark the activity complete after a return, but only once the learner has
    /// pinned both an inside and an outside reading. Returns whether it
    /// completed.
    pub fn finish(&mut self, scene: &MaxwellScene) -> bool {
        if self.comparison_ready(scene) {
            self.completed = true;
            true
        } else {
            false
        }
    }
}

/// Draw the activity window. The app restores or adopts the scene after it
/// applies the returned action.
pub fn show(
    ctx: &egui::Context,
    state: &mut ShellLessonState,
    scene: &MaxwellScene,
    family: FieldFamily,
    time: f64,
) -> ShellLessonAction {
    if !state.active() {
        return ShellLessonAction::None;
    }
    let width = (ctx.content_rect().width() - 32.0).clamp(300.0, 460.0);
    let mut action = ShellLessonAction::None;
    egui::Window::new(SHELL_LESSON_TITLE)
        .collapsible(false)
        .resizable(false)
        .default_width(width)
        .max_width(width)
        .show(ctx, |ui| {
            ui.label(SHELL_QUESTION);
            ui.add_space(4.0);
            ui.strong(SHELL_PROMPT);
            ui.add_space(2.0);
            draw_step_controls(ui, state, &mut action);
            if state.hint_visible() {
                ui.label(egui::RichText::new(format!("Hint: {SHELL_HINT}")).small());
            }
            ui.add_space(4.0);

            if !state.revealed() {
                draw_predictions(ui, state);
                ui.add_space(4.0);
            }

            draw_radius_controls(ui, state, scene);

            if !state.revealed() {
                ui.small(
                    "The measured field, area and flux stay hidden until you predict or skip.",
                );
            } else {
                ui.separator();
                draw_result(ui, state, scene, family, time);
                ui.separator();
                draw_sweep(ui.ctx(), state, scene);
            }

            ui.separator();
            ui.horizontal_wrapped(|ui| {
                if ui
                    .button("Back / Exit to my scene")
                    .on_hover_text("Restore the scene and camera from before this lesson")
                    .clicked()
                {
                    action = ShellLessonAction::Back;
                }
                if ui
                    .button("Keep exploring this scene")
                    .on_hover_text("Adopt the shell scene and close the lesson")
                    .clicked()
                {
                    action = ShellLessonAction::KeepExploring;
                }
            });
        });
    action
}

fn draw_sweep(ctx: &egui::Context, state: &mut ShellLessonState, scene: &MaxwellScene) {
    let current = state.probe_radius();
    let action = sweep::show(ctx, &mut state.sweep, scene, true, current);
    match action {
        SweepUiAction::None => {}
        SweepUiAction::Preview(radius) => {
            state.set_probe_radius(radius);
            let actual = state.probe_radius();
            state.sweep.update_preview_target(actual);
        }
        SweepUiAction::ExitPreview(start) => {
            state.set_probe_radius(start);
        }
        SweepUiAction::Adopt => {}
    }
}

fn draw_step_controls(
    ui: &mut egui::Ui,
    state: &mut ShellLessonState,
    action: &mut ShellLessonAction,
) {
    let checkpoint = state.checkpoint();
    ui.strong(checkpoint.label());
    ui.horizontal_wrapped(|ui| {
        if checkpoint == GriffithsCheckpoint::Predict
            && ui
                .button(GRIFFITHS_SKIP_STEP)
                .on_hover_text("Advance to the reveal step without revealing the result")
                .clicked()
        {
            state.skip_prediction();
        }
        let hint_label = if state.hint_visible() {
            GRIFFITHS_HIDE_HINT
        } else {
            GRIFFITHS_SHOW_HINT
        };
        if ui
            .button(hint_label)
            .on_hover_text("Reveal this activity's contextual hint")
            .clicked()
        {
            state.toggle_hint();
        }
        if ui
            .button(GRIFFITHS_EXPLORE_FROM_HERE)
            .on_hover_text("Pause the activity and explore the scene freely")
            .clicked()
        {
            *action = ShellLessonAction::ExploreFromHere;
        }
    });
}

fn draw_predictions(ui: &mut egui::Ui, state: &mut ShellLessonState) {
    ui.label("Your field prediction (you can revise it before revealing):");
    for prediction in ShellFieldPrediction::ALL {
        let selected = state.field_prediction() == Some(prediction);
        if ui.selectable_label(selected, prediction.label()).clicked() {
            state.select_field_prediction(prediction);
        }
    }
    ui.label("Your flux prediction (separate from the field):");
    for prediction in ShellFluxPrediction::ALL {
        let selected = state.flux_prediction() == Some(prediction);
        if ui.selectable_label(selected, prediction.label()).clicked() {
            state.select_flux_prediction(prediction);
        }
    }
    ui.horizontal_wrapped(|ui| {
        if ui
            .button("Skip predictions")
            .on_hover_text("Reveal the comparison without committing to predictions")
            .clicked()
        {
            state.skip_prediction();
        }
        if ui
            .add_enabled(state.can_reveal(), egui::Button::new("Reveal result"))
            .on_hover_text("Predict both quantities or skip first, then reveal")
            .clicked()
        {
            state.reveal();
        }
    });
    if state.skipped() {
        ui.label("Predictions skipped; the reasoning is still available.");
    }
}

fn draw_radius_controls(ui: &mut egui::Ui, state: &mut ShellLessonState, scene: &MaxwellScene) {
    let max = (scene.radius * 3.0).clamp(GAUSS_SHELL_MIN_RADIUS, GAUSS_SHELL_MAX_RADIUS);
    let mut radius = state.probe_radius();
    let response = ui.add(
        egui::Slider::new(&mut radius, GAUSS_SHELL_MIN_RADIUS..=max)
            .text("measuring radius r")
            .step_by(0.01),
    );
    if response.changed() {
        state.set_probe_radius(radius);
    }
    ui.horizontal_wrapped(|ui| {
        if ui
            .button("Inside (r = 0.5 R)")
            .on_hover_text("Move the measuring surface inside the shell without changing R")
            .clicked()
        {
            state.set_probe_radius(scene.radius * 0.5);
        }
        if ui
            .button("Outside (r = 1.5 R)")
            .on_hover_text("Move the measuring surface outside the shell without changing R")
            .clicked()
        {
            state.set_probe_radius(scene.radius * 1.5);
        }
        if ui
            .button("Double r")
            .on_hover_text("Set r = 2r; the shell radius R stays fixed")
            .clicked()
        {
            state.double_probe_radius();
        }
    });
    ui.small(format!(
        "Shell radius R = {:.3} (fixed); measuring radius r = {:.3}; 2r = {:.3}",
        scene.radius,
        state.probe_radius(),
        state.probe_radius() * 2.0
    ));
}

fn draw_result(
    ui: &mut egui::Ui,
    state: &mut ShellLessonState,
    scene: &MaxwellScene,
    family: FieldFamily,
    time: f64,
) {
    ui.heading("Field and flux result");
    ui.label(SHELL_UNITS_NOTE);
    ui.label(SHELL_ENCLOSED_CHARGE_NOTE);
    ui.label(SHELL_OUTWARD_NORMAL_NOTE);

    let report = state.report_for(scene, family, time).clone();
    let pin_radius = reading_card(ui, "At r", &report.at_radius, "Pin r reading");
    let pin_double = reading_card(ui, "At 2r", &report.at_double, "Pin 2r reading");
    if pin_radius {
        state.pin(report.at_radius.clone());
    }
    if pin_double {
        state.pin(report.at_double.clone());
    }

    show_ratios(ui, &report);
    ui.small(SHELL_UNCERTAINTY_NOTE);
    ui.label(SHELL_REASONING);

    if !state.pinned().is_empty() {
        ui.separator();
        ui.label("Pinned readings (snapshot at pin time)");
        for reading in state.pinned() {
            pinned_card(ui, reading);
        }
        if ui.button("Clear pinned readings").clicked() {
            state.clear_pinned();
        }
    }

    if state.comparison_ready(scene) {
        ui.label("Inside and outside readings compared. Back / Exit completes the activity.");
    } else {
        ui.small("Pin one reading inside R and one outside R to complete the comparison.");
    }
    if state.completed() {
        ui.label("Activity complete. Replay it from Help for a fresh prediction.");
    }
}

fn reading_card(
    ui: &mut egui::Ui,
    title: &str,
    reading: &GaussShellReading,
    pin_label: &str,
) -> bool {
    let mut pinned = false;
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.label(format!(
            "{title} = {:.4} (R = shell, r = measuring surface)",
            reading.radius
        ));
        match &reading.kind {
            GaussShellReadingKind::Exterior { field, area, flux } => {
                ui.label(format!("Radial field E = {field:.6}"));
                ui.label(format!("Surface area A = {area:.6}"));
                ui.label(format!("Flux Phi = {}", flux_text(flux)));
            }
            GaussShellReadingKind::Interior { area, flux } => {
                ui.label(SHELL_INSIDE_NOTE);
                ui.label("Radial field E = 0.000000");
                ui.label(format!("Surface area A = {area:.6}"));
                ui.label(format!("Flux Phi = {}", flux_text(flux)));
            }
            GaussShellReadingKind::Excluded { reason } => {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    format!("Excluded boundary sample: {reason}"),
                );
                ui.label(SHELL_BOUNDARY_NOTE);
            }
        }
        pinned = ui
            .add_enabled(reading.field().is_some(), egui::Button::new(pin_label))
            .on_hover_text("Snapshot this reading; excluded samples cannot be pinned")
            .clicked();
    });
    pinned
}

fn pinned_card(ui: &mut egui::Ui, reading: &GaussShellReading) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.label(format!("Pinned r = {:.4}", reading.radius));
        match &reading.kind {
            GaussShellReadingKind::Exterior { field, area, flux } => {
                ui.label(format!(
                    "E = {field:.6} · A = {area:.6} · E*A = {:.6}",
                    field * area
                ));
                ui.label(format!("Flux Phi = {}", flux_text(flux)));
            }
            GaussShellReadingKind::Interior { area, flux } => {
                ui.label(format!("E = 0.000000 · A = {area:.6} · E*A = 0.000000"));
                ui.label(format!("Flux Phi = {}", flux_text(flux)));
            }
            GaussShellReadingKind::Excluded { reason } => {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    format!("Excluded sample: {reason}"),
                );
                ui.label(SHELL_BOUNDARY_NOTE);
            }
        }
    });
}

fn show_ratios(ui: &mut egui::Ui, report: &GaussShellReport) {
    let Some(first_field) = report.at_radius.field() else {
        return;
    };
    let Some(second_field) = report.at_double.field() else {
        return;
    };
    let (Some(first_area), Some(second_area)) = (report.at_radius.area(), report.at_double.area())
    else {
        return;
    };
    if second_field > 0.0 && first_area > 0.0 {
        ui.label(format!(
            "Outside doubling: E(r) / E(2r) = {:.4} (one quarter the field)",
            first_field / second_field
        ));
        ui.label(format!(
            "A(2r) / A(r) = {:.4} (four times the area)",
            second_area / first_area
        ));
    }
    if let (Some((first_flux, first_error)), Some((second_flux, second_error))) = (
        report.at_radius.flux_estimate(),
        report.at_double.flux_estimate(),
    ) {
        let difference = (first_flux - second_flux).abs();
        let uncertainty = first_error + second_error;
        ui.label(format!(
            "|Phi(r) - Phi(2r)| = {difference:.3e}; combined quadrature uncertainty = \
             {uncertainty:.3e}; equal within uncertainty: {}",
            if difference <= uncertainty + 1e-9 {
                "yes"
            } else {
                "no"
            }
        ));
    }
}

fn flux_text(report: &crate::field_probe::FluxReport) -> String {
    match &report.status {
        crate::field_probe::IntegralStatus::Estimate { value, error } => {
            format!("{value:.6} +/- {error:.2e} (quadrature)")
        }
        crate::field_probe::IntegralStatus::Unresolved { reason } => {
            format!("unavailable ({reason})")
        }
        crate::field_probe::IntegralStatus::Singular { reason } => {
            format!("singular ({reason})")
        }
    }
}
