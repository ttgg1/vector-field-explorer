//! Issue #226: the Griffiths §3.2 grounded-plane image-charge route.
//!
//! The route loads the shipped `MaxwellCase::ImagePlane` scene in a vertical
//! `x-z` slice and wraps the shipped [`crate::maxwell::maxwell_field`] plus one
//! pure normalized `k = 1` potential helper in a prediction-gated lesson. It
//! adds no physical case, quadrature rule, solver, arbitrary tolerance or
//! durable probe surface: the trial image and both readings are session-only.
//!
//! Everything here is session-only UI state. The app snapshots the starting
//! scene, slice, projection and camera before opening the route; this module
//! only reports its `Back/Exit` and `Keep exploring` actions.

use eframe::egui;
use glam::Vec3;

use crate::maxwell::{
    IMAGE_PLANE_BOUNDARY_TOLERANCE, MaxwellScene, image_plane_trial_potential, maxwell_field,
};

/// Window title for the activity.
pub const PLANE_LESSON_TITLE: &str = "Griffiths §3.2 · Grounded plane";
/// The explicit physics question.
pub const PLANE_QUESTION: &str = "A point charge q sits at height h above an infinite grounded conducting plane at z = 0, which is held at V = 0. What image charge completes the field above the plane, and what does the boundary potential measure at several lateral positions?";
/// The prediction prompt shown before the mirror construction or result.
pub const PLANE_PROMPT: &str = "Predict first, separately: which sign should the trial image carry, and where should it sit relative to the grounded plane?";
/// Hint that keeps the image a construction aid rather than a second charge.
pub const PLANE_HINT: &str = "The grounded plane is an equipotential at V = 0. The image is a construction aid placed outside the physical region; it is not another charge in the scene.";
/// Scope line for every displayed reading.
pub const PLANE_UNITS_NOTE: &str = "Normalized teaching units with k = 1 (not the SI 1/(4*pi*epsilon_0)); the plot shows the shipped ImagePlane field with the scene's charge q and height h.";
/// The supported region and the renderer's deliberately omitted ground slice.
pub const PLANE_HALF_SPACE_NOTE: &str = "Valid half-space: z > 0. The shipped field is zero for z <= 0, and the renderer intentionally omits the ground-plane slice; the lesson checks the boundary analytically at z = 0.";
/// The boundary-potential rule and its stated numerical tolerance.
pub const PLANE_BOUNDARY_NOTE: &str = "Boundary potential: V = q/|r - r1| - q/|r - r2| with k = 1. At z = 0 the source and image distances are equal, so V = 0 within 1e-12 at every lateral position.";
/// Both point singularities are excluded rather than replaced.
pub const PLANE_SINGULARITY_NOTE: &str = "The source at (0, 0, h) and the image at (0, 0, -h) are point singularities; samples there are excluded rather than replaced by a finite value.";
/// The correct construction revealed after the prediction.
pub const PLANE_IMAGE_NOTE: &str = "Correct construction: the mirror-position image is -q at (0, 0, -h). It is a construction aid, not a second charge in the physical scene.";
/// Why the limiting field is normal.
pub const PLANE_FIELD_NOTE: &str = "Just above the plane the limiting field is normal: the tangential component cancels, leaving E_z = -2 k q h/(rho^2 + h^2)^(3/2). At a finite probe height the tangential part is small but nonzero.";
/// The trial boundary test is a candidate, not the physical scene.
pub const PLANE_CANDIDATE_NOTE: &str =
    "Candidate construction: your trial boundary test, not the physical scene.";
/// The correct trial verdict.
pub const PLANE_CORRECT_TRIAL_NOTE: &str =
    "Your trial matches the correct mirror construction: -q at z = -h.";
/// The same-sign failure verdict.
pub const PLANE_SAME_SIGN_FAIL_NOTE: &str =
    "Your same-sign trial fails the boundary test: V is not zero at the lateral positions above.";
/// The wrong-location verdict.
pub const PLANE_WRONG_LOCATION_NOTE: &str =
    "Your trial places the image on the source side, not at the mirror position z = -h.";
/// Text shown when the predictions were explicitly skipped.
pub const PLANE_SKIPPED_NOTE: &str =
    "Predictions skipped; the correct construction is shown without a trial verdict.";
/// The analytic potential expression offered to the CAS/scalar route.
pub const PLANE_CAS_EXPRESSION: &str = "k*q*(1/sqrt(x^2+(y-h)^2) - 1/sqrt(x^2+(y+h)^2))";
/// Provenance for the CAS expression and the available #214 route.
pub const PLANE_CAS_PROVENANCE: &str = "Analytic potential in the vertical x-z slice at y = 0, using the CAS surface's x for lateral position and y for height. The #214 CAS/scalar route carries this symbolic expression into the CAS input as a starting point; it does not run an automatic plane solve.";

/// Lateral positions tested on the boundary, as multiples of the charge height.
pub const PLANE_BOUNDARY_LATERALS: [f64; 3] = [0.5, 1.0, 2.0];
/// Stated numerical tolerance for a zero boundary potential.
pub const PLANE_BOUNDARY_TOLERANCE: f64 = IMAGE_PLANE_BOUNDARY_TOLERANCE;

/// The learner's trial image sign.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaneImageSignPrediction {
    /// The image has the opposite sign, `-q`.
    NegativeOpposite,
    /// The image has the same sign, `+q`.
    SameSign,
}

impl PlaneImageSignPrediction {
    /// Both choices, in prompt order.
    pub const ALL: [Self; 2] = [Self::NegativeOpposite, Self::SameSign];

    /// The signed image charge in units of the real charge.
    pub const fn sign(self) -> f64 {
        match self {
            Self::NegativeOpposite => -1.0,
            Self::SameSign => 1.0,
        }
    }

    /// The label shown on the prediction control.
    pub const fn label(self) -> &'static str {
        match self {
            Self::NegativeOpposite => "Trial image: opposite sign (-q)",
            Self::SameSign => "Trial image: same sign (+q)",
        }
    }
}

/// The learner's trial image location.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaneImageLocationPrediction {
    /// The mirror position below the grounded plane, `z = -h`.
    MirrorBelow,
    /// The source side above the plane, `z = +h`.
    SourceSideAbove,
}

impl PlaneImageLocationPrediction {
    /// Both choices, in prompt order.
    pub const ALL: [Self; 2] = [Self::MirrorBelow, Self::SourceSideAbove];

    /// The label shown on the prediction control.
    pub const fn label(self) -> &'static str {
        match self {
            Self::MirrorBelow => "Trial location: mirror below the plane (z = -h)",
            Self::SourceSideAbove => "Trial location: same side as the source (z = +h)",
        }
    }
}

/// One candidate image construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaneImageTrial {
    pub sign: PlaneImageSignPrediction,
    pub location: PlaneImageLocationPrediction,
}

impl PlaneImageTrial {
    /// The correct grounded-plane construction.
    pub const CORRECT: Self = Self {
        sign: PlaneImageSignPrediction::NegativeOpposite,
        location: PlaneImageLocationPrediction::MirrorBelow,
    };

    /// The signed image charge in units of the real charge.
    pub const fn image_sign(self) -> f64 {
        self.sign.sign()
    }

    /// The trial image height for `scene`.
    pub fn image_height(self, scene: &MaxwellScene) -> f64 {
        let height = scene.charge_height.abs().max(1e-3);
        match self.location {
            PlaneImageLocationPrediction::MirrorBelow => -height,
            PlaneImageLocationPrediction::SourceSideAbove => height,
        }
    }

    /// Whether this is the shipped correct construction.
    pub fn is_correct(self) -> bool {
        self == Self::CORRECT
    }

    /// The verdict shown once the result is revealed.
    pub const fn verdict(self) -> &'static str {
        match (self.sign, self.location) {
            (
                PlaneImageSignPrediction::NegativeOpposite,
                PlaneImageLocationPrediction::MirrorBelow,
            ) => PLANE_CORRECT_TRIAL_NOTE,
            (PlaneImageSignPrediction::SameSign, _) => PLANE_SAME_SIGN_FAIL_NOTE,
            _ => PLANE_WRONG_LOCATION_NOTE,
        }
    }
}

/// One analytic boundary reading at `z = 0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlaneBoundaryReading {
    pub lateral: f64,
    pub potential: f64,
    pub zero_within_tolerance: bool,
}

/// One shipped-field sample just above the plane.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlaneFieldReading {
    pub lateral: f64,
    pub height: f64,
    pub field: Option<Vec3>,
}

impl PlaneFieldReading {
    /// The in-plane (tangential) field magnitude, when the sample is defined.
    pub fn tangential(&self) -> Option<f64> {
        self.field
            .map(|field| (field.x * field.x + field.y * field.y).sqrt() as f64)
    }

    /// The normal field magnitude, when the sample is defined.
    pub fn normal(&self) -> Option<f64> {
        self.field.map(|field| field.z.abs() as f64)
    }
}

/// Evaluate one candidate boundary reading at a lateral position.
pub fn boundary_reading(
    scene: &MaxwellScene,
    lateral: f64,
    trial: PlaneImageTrial,
) -> Option<PlaneBoundaryReading> {
    if !lateral.is_finite() || lateral < 0.0 {
        return None;
    }
    let point = Vec3::new(lateral as f32, 0.0, 0.0);
    let potential =
        image_plane_trial_potential(scene, point, trial.image_sign(), trial.image_height(scene))
            .ok()?;
    Some(PlaneBoundaryReading {
        lateral,
        potential,
        zero_within_tolerance: potential.abs() <= PLANE_BOUNDARY_TOLERANCE,
    })
}

/// Evaluate the boundary at the lesson's fixed lateral positions.
pub fn boundary_readings(
    scene: &MaxwellScene,
    trial: PlaneImageTrial,
) -> Vec<PlaneBoundaryReading> {
    let height = scene.charge_height.abs().max(1e-3);
    PLANE_BOUNDARY_LATERALS
        .iter()
        .filter_map(|multiple| boundary_reading(scene, height * multiple, trial))
        .collect()
}

/// Sample the shipped field at one lateral position and probe height.
pub fn plane_field_reading(scene: &MaxwellScene, lateral: f64, height: f64) -> PlaneFieldReading {
    let field = if lateral.is_finite() && height.is_finite() && height > 0.0 {
        maxwell_field(scene, Vec3::new(lateral as f32, 0.0, height as f32)).ok()
    } else {
        None
    };
    PlaneFieldReading {
        lateral,
        height,
        field,
    }
}

/// What the app should do after the lesson window's one-shot action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaneLessonAction {
    None,
    /// Restore the scene, slice, projection and camera captured before the lesson.
    Back,
    /// Keep the plane scene and close the lesson.
    KeepExploring,
    /// Put the analytic potential formula into the native CAS input.
    SendToCas,
}

/// Session-only activity state. `Default` is inactive.
#[derive(Debug, Clone, Default)]
pub struct PlaneLessonState {
    active: bool,
    sign_prediction: Option<PlaneImageSignPrediction>,
    location_prediction: Option<PlaneImageLocationPrediction>,
    skipped: bool,
    revealed: bool,
    lateral: f64,
    probe_height: f64,
}

impl PlaneLessonState {
    /// Activate a fresh session for `scene`.
    pub fn activate(&mut self, scene: &MaxwellScene) {
        *self = Self::default();
        self.active = true;
        let height = scene.charge_height.abs().max(1e-3);
        self.lateral = height;
        self.probe_height = height * 0.01;
    }

    /// Leave the activity. The next [`Self::activate`] starts a fresh session.
    pub fn deactivate(&mut self) {
        *self = Self::default();
    }

    /// Whether the lesson window should be shown.
    pub fn active(&self) -> bool {
        self.active
    }

    /// The trial image-sign prediction, if any.
    pub fn sign_prediction(&self) -> Option<PlaneImageSignPrediction> {
        self.sign_prediction
    }

    /// The trial image-location prediction, if any.
    pub fn location_prediction(&self) -> Option<PlaneImageLocationPrediction> {
        self.location_prediction
    }

    /// Select or revise the trial sign before the reveal.
    pub fn select_sign_prediction(&mut self, prediction: PlaneImageSignPrediction) {
        if !self.revealed {
            self.sign_prediction = Some(prediction);
            self.skipped = false;
        }
    }

    /// Select or revise the trial location before the reveal.
    pub fn select_location_prediction(&mut self, prediction: PlaneImageLocationPrediction) {
        if !self.revealed {
            self.location_prediction = Some(prediction);
            self.skipped = false;
        }
    }

    /// Skip both predictions without pretending they were made.
    pub fn skip_prediction(&mut self) {
        if !self.revealed {
            self.skipped = true;
            self.sign_prediction = None;
            self.location_prediction = None;
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

    /// The reveal is gated on both predictions or an explicit skip.
    pub fn can_reveal(&self) -> bool {
        !self.revealed
            && (self.skipped
                || (self.sign_prediction.is_some() && self.location_prediction.is_some()))
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

    /// The current candidate construction, if a trial exists or was skipped.
    pub fn trial(&self) -> Option<PlaneImageTrial> {
        match (self.sign_prediction, self.location_prediction) {
            (Some(sign), Some(location)) => Some(PlaneImageTrial { sign, location }),
            _ if self.skipped => Some(PlaneImageTrial::CORRECT),
            _ => None,
        }
    }

    /// The movable lateral measurement position.
    pub fn lateral(&self) -> f64 {
        self.lateral
    }

    /// Move the lateral measurement position. Non-finite or negative values are
    /// refused without touching the current position.
    pub fn set_lateral(&mut self, value: f64) -> bool {
        if !value.is_finite() || value < 0.0 {
            return false;
        }
        if value.to_bits() == self.lateral.to_bits() {
            return true;
        }
        self.lateral = value;
        true
    }

    /// The movable probe height just above the plane.
    pub fn probe_height(&self) -> f64 {
        self.probe_height
    }

    /// Move the field probe. Non-finite or non-positive values are refused.
    pub fn set_probe_height(&mut self, value: f64) -> bool {
        if !value.is_finite() || value <= 0.0 {
            return false;
        }
        if value.to_bits() == self.probe_height.to_bits() {
            return true;
        }
        self.probe_height = value;
        true
    }

    /// The candidate boundary readings at the fixed lateral positions.
    pub fn boundary_readings(&self, scene: &MaxwellScene) -> Vec<PlaneBoundaryReading> {
        self.trial()
            .map(|trial| boundary_readings(scene, trial))
            .unwrap_or_default()
    }

    /// The candidate boundary reading at the current lateral position.
    pub fn boundary_reading(&self, scene: &MaxwellScene) -> Option<PlaneBoundaryReading> {
        self.trial()
            .and_then(|trial| boundary_reading(scene, self.lateral, trial))
    }

    /// The shipped field just above the plane at the current lateral position.
    pub fn field_reading(&self, scene: &MaxwellScene) -> PlaneFieldReading {
        plane_field_reading(scene, self.lateral, self.probe_height)
    }
}

/// Draw the activity window. The app restores or adopts the scene after it
/// applies the returned action.
pub fn show(
    ctx: &egui::Context,
    state: &mut PlaneLessonState,
    scene: &MaxwellScene,
) -> PlaneLessonAction {
    if !state.active() {
        return PlaneLessonAction::None;
    }
    let width = (ctx.content_rect().width() - 32.0).clamp(300.0, 460.0);
    let mut action = PlaneLessonAction::None;
    egui::Window::new(PLANE_LESSON_TITLE)
        .collapsible(false)
        .resizable(false)
        .default_width(width)
        .max_width(width)
        .show(ctx, |ui| {
            ui.label(PLANE_QUESTION);
            ui.add_space(4.0);
            ui.strong(PLANE_PROMPT);
            ui.label(egui::RichText::new(format!("Hint: {PLANE_HINT}")).small());
            ui.add_space(4.0);

            if !state.revealed() {
                draw_predictions(ui, state);
                ui.add_space(4.0);
            }

            if let Some(trial) = state.trial() {
                draw_boundary_test(ui, state, scene, trial);
            } else {
                ui.small(
                    "Choose a trial image sign and location to test the boundary before revealing.",
                );
            }

            if !state.revealed() {
                ui.small(
                    "The mirror construction and the field reading stay hidden until you predict or skip.",
                );
            } else {
                ui.separator();
                if draw_result(ui, state, scene) {
                    action = PlaneLessonAction::SendToCas;
                }
            }

            ui.separator();
            ui.horizontal_wrapped(|ui| {
                if ui
                    .button("Back / Exit to my scene")
                    .on_hover_text(
                        "Restore the scene, slice, projection and camera from before this lesson",
                    )
                    .clicked()
                {
                    action = PlaneLessonAction::Back;
                }
                if ui
                    .button("Keep exploring this scene")
                    .on_hover_text("Adopt the grounded-plane scene and close the lesson")
                    .clicked()
                {
                    action = PlaneLessonAction::KeepExploring;
                }
            });
        });
    action
}

fn draw_predictions(ui: &mut egui::Ui, state: &mut PlaneLessonState) {
    ui.label("Your trial image sign (you can revise it before revealing):");
    for prediction in PlaneImageSignPrediction::ALL {
        let selected = state.sign_prediction() == Some(prediction);
        if ui.selectable_label(selected, prediction.label()).clicked() {
            state.select_sign_prediction(prediction);
        }
    }
    ui.label("Your trial image location (separate from the sign):");
    for prediction in PlaneImageLocationPrediction::ALL {
        let selected = state.location_prediction() == Some(prediction);
        if ui.selectable_label(selected, prediction.label()).clicked() {
            state.select_location_prediction(prediction);
        }
    }
    ui.horizontal_wrapped(|ui| {
        if ui
            .button("Skip predictions")
            .on_hover_text("Reveal the construction without committing to a prediction")
            .clicked()
        {
            state.skip_prediction();
        }
        if ui
            .add_enabled(state.can_reveal(), egui::Button::new("Reveal result"))
            .on_hover_text("Predict both parts or skip first, then reveal")
            .clicked()
        {
            state.reveal();
        }
    });
    if state.skipped() {
        ui.label("Predictions skipped; the reasoning is still available.");
    }
}

fn draw_boundary_test(
    ui: &mut egui::Ui,
    state: &mut PlaneLessonState,
    scene: &MaxwellScene,
    trial: PlaneImageTrial,
) {
    ui.separator();
    ui.label(PLANE_CANDIDATE_NOTE);
    let height = scene.charge_height.abs().max(1e-3);
    let mut lateral = state.lateral();
    let response = ui.add(
        egui::Slider::new(&mut lateral, 0.0..=(height * 4.0))
            .text("lateral position")
            .step_by(0.01),
    );
    if response.changed() {
        state.set_lateral(lateral);
    }
    ui.horizontal_wrapped(|ui| {
        for multiple in PLANE_BOUNDARY_LATERALS {
            if ui
                .button(format!("x = {multiple:.1} h"))
                .on_hover_text("Move the boundary test to this lateral position")
                .clicked()
            {
                state.set_lateral(height * multiple);
            }
        }
    });

    ui.label(format!(
        "Boundary potential at z = 0 (tolerance {:.1e}):",
        PLANE_BOUNDARY_TOLERANCE
    ));
    let readings = boundary_readings(scene, trial);
    if state.revealed() {
        let zeros = readings
            .iter()
            .filter(|reading| reading.zero_within_tolerance)
            .count();
        ui.small(format!(
            "Tested at the fixed lateral positions: {zeros} of {} read V = 0 within tolerance.",
            readings.len()
        ));
    } else {
        for reading in readings {
            ui.label(format!(
                "x = {:.3} (lateral = {:.2} h): V = {:.6e} — zero: {}",
                reading.lateral,
                reading.lateral / height,
                reading.potential,
                if reading.zero_within_tolerance {
                    "yes"
                } else {
                    "no"
                }
            ));
        }
    }
    if let Some(reading) = state
        .trial()
        .and_then(|trial| boundary_reading(scene, state.lateral(), trial))
    {
        ui.small(format!(
            "At your lateral x = {:.3}: V = {:.6e} — zero within tolerance: {}",
            reading.lateral, reading.potential, reading.zero_within_tolerance
        ));
    }
}

fn draw_result(ui: &mut egui::Ui, state: &mut PlaneLessonState, scene: &MaxwellScene) -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    let mut send_to_cas = false;
    #[cfg(target_arch = "wasm32")]
    let send_to_cas = false;
    ui.heading("Grounded-plane image result");

    ui.strong("Field just above the plane");
    let height = scene.charge_height.abs().max(1e-3);
    let mut probe = state.probe_height();
    let response = ui.add(
        egui::Slider::new(&mut probe, (height * 1e-4)..=(height * 0.9))
            .text("probe height z")
            .logarithmic(true),
    );
    if response.changed() {
        state.set_probe_height(probe);
    }
    let reading = state.field_reading(scene);
    match (reading.tangential(), reading.normal(), reading.field) {
        (Some(tangential), Some(normal), Some(field)) => {
            ui.label(format!(
                "At x = {:.3}, z = {:.6}: E = ({:.6}, {:.6}, {:.6})",
                reading.lateral, reading.height, field.x, field.y, field.z
            ));
            ui.label(format!(
                "Tangential |E_parallel| = {tangential:.3e}; normal |E_z| = {normal:.3e}"
            ));
        }
        _ => {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "The shipped field is unavailable at this probe point.",
            );
        }
    }
    ui.label(PLANE_FIELD_NOTE);

    ui.separator();
    ui.label(PLANE_UNITS_NOTE);
    ui.label(PLANE_IMAGE_NOTE);
    if state.skipped() {
        ui.label(PLANE_SKIPPED_NOTE);
    } else if let Some(trial) = state.trial() {
        ui.label(trial.verdict());
    }
    ui.label(PLANE_HALF_SPACE_NOTE);
    ui.label(PLANE_BOUNDARY_NOTE);
    ui.label(PLANE_SINGULARITY_NOTE);

    ui.separator();
    ui.label("Analytic potential expression (vertical x-z slice):");
    ui.code(PLANE_CAS_EXPRESSION);
    ui.label(PLANE_CAS_PROVENANCE);
    #[cfg(not(target_arch = "wasm32"))]
    if ui
        .button("Show potential formula in CAS")
        .on_hover_text(
            "Put the normalized vertical-slice potential in the CAS input, a starting point for the #214 CAS/scalar route",
        )
        .clicked()
    {
        send_to_cas = true;
    }
    send_to_cas
}
