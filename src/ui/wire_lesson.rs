//! Issue #227: the Griffiths 5.14(a) surface-current wire route.
//!
//! The route wraps the shipped `MaxwellCase::InfiniteWire` field and the
//! shipped [`crate::field_probe::circulation`] loop in a prediction-gated
//! transverse slice. It adds no physical case, quadrature rule, tolerance or
//! durable probe surface: the field is the same [`crate::maxwell::maxwell_field`]
//! the Maxwell viewport draws, and the circulation is the same midpoint
//! quadrature the Field probes panel reports. All values are normalized
//! teaching quantities, not SI measurements.
//!
//! Everything here is session-only UI state. The app snapshots the starting
//! scene, projection and camera before opening the route; this module only
//! reports its `Back/Exit` and `Keep exploring` actions.

use std::f64::consts::TAU;

use eframe::egui;
use glam::Vec3;

use crate::field_probe::{
    CirculationLoop, CirculationReport, IntegralStatus, LoopKind, ProbeField, circulation,
};
use crate::maxwell::{MaxwellCase, MaxwellError, MaxwellScene, maxwell_field};
use crate::sources::FieldFamily;
use crate::ui::gauss_shell::{GAUSS_SHELL_MAX_RADIUS, GAUSS_SHELL_MIN_RADIUS};

/// Window title for the activity.
pub const WIRE_LESSON_TITLE: &str = "Griffiths 5.14(a) · Current-carrying wire";
/// The explicit physics question.
pub const WIRE_QUESTION: &str = "An ideal infinite straight wire carries a steady current along +z; its thin shell is the ideal surface-current layer. Outside, B circles the wire; inside, there is no field. What does a compass show outside, and what would a smaller loop entirely inside measure?";
/// The prediction prompt shown before any measured result.
pub const WIRE_PROMPT: &str = "Predict first, separately: outside the shell, which way do the compass needles point around the wire seen from +z, and what would a smaller loop entirely inside measure for B and circulation?";
/// Hint anchored to the shipped current direction and the right-hand rule,
/// without naming the exterior field direction the learner must predict.
pub const WIRE_DIRECTION_HINT: &str = "The current flows along +z (out of the transverse slice toward you). Point your right thumb along +z: which way does the right-hand rule curl your fingers in the slice, and which way would the compass needles point outside the shell?";
/// Hint anchored to the loop normal, which is independent of the current.
pub const WIRE_ORIENTATION_HINT: &str = "The loop's positive normal is +z for a counter-clockwise contour and -z for a clockwise contour. Reversing the normal reverses the sign of the circulation; it never changes the field arrows.";
/// Scope line for every displayed measurement.
pub const WIRE_UNITS_NOTE: &str = "Normalized teaching amplitudes from the shipped Infinite wire case (B0 and shell radius a as shown); |B| and the circulation are sampled estimates with quadrature uncertainty, not SI measurements.";
/// The ideal source assumption stated on the problem card.
pub const WIRE_ASSUMPTION_NOTE: &str = "Assumption: an ideal infinite straight thin surface-current shell, not a finite wire. Its field is defined only off the shell, is zero inside, and is discontinuous across the ideal interface.";
/// Why the ideal interface is excluded rather than reported.
pub const WIRE_INTERFACE_NOTE: &str = "At the ideal interface (loop radius within the shipped case tolerance of the shell radius) the analytic field is discontinuous, so this sample is excluded and no finite field or circulation is reported.";
/// Why the interior is zero rather than missing.
pub const WIRE_INSIDE_NOTE: &str =
    "Inside the shell (r < a) the ideal field and the circulation around the loop are both zero.";
/// What the compass shows outside, once the result is revealed.
pub const WIRE_DIRECTION_RESULT_NOTE: &str = "Outside, the compass needles circle counter-clockwise seen from +z, consistent with the +z current and the right-hand rule.";
/// Why orientation reversal affects only the signed integral.
pub const WIRE_ORIENTATION_NOTE: &str = "The field arrows depend on the current, not on the loop. Reversing only the loop normal flips the sign of the circulation and leaves the arrows unchanged.";
/// Pairing/geometry explanation after the reveal.
pub const WIRE_REASONING: &str = "Outside, B = B0 (a/r) phi-hat: doubling r halves |B|, while the circumference doubles, so the circulation stays 2*pi*B0*a. Inside, the enclosed current is zero, so B and the circulation vanish.";
/// Reminder that a comparison is sampled, not symbolic.
pub const WIRE_UNCERTAINTY_NOTE: &str = "The equal-circulation comparison holds within the reported half-resolution quadrature uncertainty; it is a sampled estimate, not an exact symbolic cancellation.";
/// Why the |B(r)| / |B(2r)| halving statement is withheld for mixed readings.
pub const WIRE_RATIO_EXTERIOR_ONLY_NOTE: &str = "The |B| ratio and the halving statement apply only when both circles are outside the shell (r > a); a circle inside the ideal shell sees B = 0, so this pair has no halving comparison.";

/// Subdivision count for the lesson loop, matching the shipped default probe.
pub const WIRE_LOOP_SEGMENTS: u32 = 16;
/// Smallest loop radius the exact-entry control accepts, reused from #219.
pub const WIRE_LOOP_MIN_RADIUS: f64 = GAUSS_SHELL_MIN_RADIUS;
/// Largest loop radius the exact-entry control accepts, reused from #219.
pub const WIRE_LOOP_MAX_RADIUS: f64 = GAUSS_SHELL_MAX_RADIUS;

/// Which way the oriented loop is traversed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WireOrientation {
    /// Positive normal +z; counter-clockwise seen from +z.
    #[default]
    PositiveNormal,
    /// Positive normal -z; clockwise seen from +z.
    NegativeNormal,
}

impl WireOrientation {
    /// Both orientations, in prompt order.
    pub const ALL: [Self; 2] = [Self::PositiveNormal, Self::NegativeNormal];

    /// The signed +z component of the positive normal.
    pub const fn normal(self) -> f32 {
        match self {
            Self::PositiveNormal => 1.0,
            Self::NegativeNormal => -1.0,
        }
    }

    /// The loop axis handed to [`CirculationLoop`].
    pub fn axis(self) -> Vec3 {
        Vec3::Z * self.normal()
    }

    /// The radio label, including the contour direction.
    pub const fn label(self) -> &'static str {
        match self {
            Self::PositiveNormal => "Loop normal +z (counter-clockwise seen from +z)",
            Self::NegativeNormal => "Loop normal -z (clockwise seen from +z)",
        }
    }

    /// The normal alone, for the selected-orientation summary.
    pub const fn normal_label(self) -> &'static str {
        match self {
            Self::PositiveNormal => "+z (out of the page)",
            Self::NegativeNormal => "-z (into the page)",
        }
    }

    /// The contour direction alone, for the selected-orientation summary.
    pub const fn contour_label(self) -> &'static str {
        match self {
            Self::PositiveNormal => "counter-clockwise seen from +z",
            Self::NegativeNormal => "clockwise seen from +z",
        }
    }
}

/// The learner's compass-direction prediction outside the shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireDirectionPrediction {
    /// The field circles counter-clockwise seen from +z.
    CounterClockwise,
    /// The field circles clockwise seen from +z.
    Clockwise,
    /// The ideal exterior field is zero.
    NoField,
}

impl WireDirectionPrediction {
    /// Both choices, in prompt order.
    pub const ALL: [Self; 3] = [Self::CounterClockwise, Self::Clockwise, Self::NoField];

    /// The label shown on the prediction control.
    pub const fn label(self) -> &'static str {
        match self {
            Self::CounterClockwise => {
                "Outside: compass needles circle counter-clockwise seen from +z"
            }
            Self::Clockwise => "Outside: compass needles circle clockwise seen from +z",
            Self::NoField => "Outside: there is no field",
        }
    }
}

/// The learner's prediction for a loop entirely inside the shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireInsidePrediction {
    /// Both the field and the circulation vanish.
    BothZero,
    /// The field is nonzero but the circulation vanishes.
    FieldOnly,
    /// Both the field and the circulation are nonzero.
    BothNonzero,
}

impl WireInsidePrediction {
    /// Every choice, in prompt order.
    pub const ALL: [Self; 3] = [Self::BothZero, Self::FieldOnly, Self::BothNonzero];

    /// The label shown on the prediction control.
    pub const fn label(self) -> &'static str {
        match self {
            Self::BothZero => "Inside: B = 0 and circulation = 0",
            Self::FieldOnly => "Inside: B ≠ 0 but circulation = 0",
            Self::BothNonzero => "Inside: both B and circulation ≠ 0",
        }
    }
}

/// What the app should do after the lesson window's one-shot action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireLessonAction {
    None,
    /// Restore the scene, projection and camera captured before the lesson.
    Back,
    /// Keep the wire scene and close the lesson.
    KeepExploring,
}

/// One loop radius's honest classification.
#[derive(Debug, Clone, PartialEq)]
pub enum WireReadingKind {
    /// `r > a`: a finite |B| and an oriented circulation estimate.
    Exterior {
        field: f64,
        circulation: CirculationReport,
    },
    /// `r < a`: the field and circulation vanish.
    Interior { circulation: CirculationReport },
    /// The ideal interface or an unavailable scene: never a finite reading.
    Excluded { reason: String },
}

/// One reading at the loop radius `r`.
#[derive(Debug, Clone, PartialEq)]
pub struct WireLessonReading {
    pub radius: f64,
    pub kind: WireReadingKind,
}

impl WireLessonReading {
    /// Field magnitude, or `None` for an excluded sample. The interior reports
    /// the ideal zero rather than a missing value.
    pub fn field(&self) -> Option<f64> {
        match &self.kind {
            WireReadingKind::Exterior { field, .. } => Some(*field),
            WireReadingKind::Interior { .. } => Some(0.0),
            WireReadingKind::Excluded { .. } => None,
        }
    }

    /// The shipped circulation report, or `None` for an excluded sample.
    pub fn circulation(&self) -> Option<&CirculationReport> {
        match &self.kind {
            WireReadingKind::Exterior { circulation, .. }
            | WireReadingKind::Interior { circulation } => Some(circulation),
            WireReadingKind::Excluded { .. } => None,
        }
    }

    /// The completed full/half-resolution circulation estimate and its
    /// uncertainty.
    pub fn circulation_estimate(&self) -> Option<(f64, f64)> {
        match self.circulation().map(|report| &report.status) {
            Some(IntegralStatus::Estimate { value, error }) => Some((*value, *error)),
            _ => None,
        }
    }
}

/// The live readings at `r` and `2r`, cached against their exact inputs.
#[derive(Debug, Clone, PartialEq)]
pub struct WireLessonReport {
    pub radius: f64,
    pub orientation: WireOrientation,
    /// The field at `(r, 0, 0)`, independent of the loop orientation.
    pub field_vector: Option<Vec3>,
    pub at_radius: WireLessonReading,
    pub at_double: WireLessonReading,
}

/// Local, never-stored field adapter over the current Maxwell scene.
struct MaxwellProbe<'a> {
    scene: &'a MaxwellScene,
}

impl ProbeField for MaxwellProbe<'_> {
    fn eval(&self, point: Vec3, _time: f64) -> Option<Vec3> {
        maxwell_field(self.scene, point).ok()
    }
}

/// The lesson loop geometry for `radius` and `orientation`.
pub fn loop_geometry(radius: f64, orientation: WireOrientation) -> CirculationLoop {
    CirculationLoop {
        kind: LoopKind::Circle,
        center: Vec3::ZERO,
        axis: orientation.axis(),
        size: Vec3::splat((2.0 * radius) as f32),
        segments: WIRE_LOOP_SEGMENTS,
    }
}

/// Compute one honest reading at `radius`. This is the pure seam shared by the
/// panel and the tests.
pub fn compute_reading(
    scene: &MaxwellScene,
    radius: f64,
    orientation: WireOrientation,
    family: FieldFamily,
    time: f64,
) -> WireLessonReading {
    if scene.case != MaxwellCase::InfiniteWire {
        return excluded(
            radius,
            "The wire lesson is defined for the shipped Infinite wire case.",
        );
    }
    if !radius.is_finite() || radius <= 0.0 {
        return excluded(
            radius,
            "The loop radius must be finite and greater than zero.",
        );
    }

    // The shipped field owns the interface rule: it returns `Unsupported` on
    // the ideal shell. Reusing that decision avoids a second tolerance.
    let point = Vec3::new(radius as f32, 0.0, 0.0);
    let field = match maxwell_field(scene, point) {
        Ok(field) if field.is_finite() => field,
        Ok(_) => return excluded(radius, "The field sample was not finite."),
        Err(error) => return excluded(radius, &maxwell_error_text(&error)),
    };

    let probe = MaxwellProbe { scene };
    let report = circulation(&probe, &loop_geometry(radius, orientation), family, time);
    if radius < scene.radius {
        return WireLessonReading {
            radius,
            kind: WireReadingKind::Interior {
                circulation: report,
            },
        };
    }
    WireLessonReading {
        radius,
        kind: WireReadingKind::Exterior {
            field: f64::from(field.length()),
            circulation: report,
        },
    }
}

fn excluded(radius: f64, reason: &str) -> WireLessonReading {
    WireLessonReading {
        radius,
        kind: WireReadingKind::Excluded {
            reason: reason.to_owned(),
        },
    }
}

fn maxwell_error_text(error: &MaxwellError) -> String {
    match error {
        MaxwellError::InvalidScene(text)
        | MaxwellError::Unsupported(text)
        | MaxwellError::Unavailable(text) => text.clone(),
    }
}

fn field_vector(scene: &MaxwellScene, radius: f64) -> Option<Vec3> {
    if !radius.is_finite() || radius <= 0.0 {
        return None;
    }
    maxwell_field(scene, Vec3::new(radius as f32, 0.0, 0.0)).ok()
}

/// Session-only activity state. `Default` is inactive.
#[derive(Debug, Clone, Default)]
pub struct WireLessonState {
    active: bool,
    loop_radius: f64,
    orientation: WireOrientation,
    direction_prediction: Option<WireDirectionPrediction>,
    inside_prediction: Option<WireInsidePrediction>,
    skipped: bool,
    revealed: bool,
    report: Option<WireLessonReport>,
    report_scene: Option<MaxwellScene>,
    report_family: Option<FieldFamily>,
    report_radius: f64,
    report_orientation: WireOrientation,
    report_time: f64,
}

impl WireLessonState {
    /// Activate a fresh session with the loop outside the shell.
    pub fn activate(&mut self, scene: &MaxwellScene) {
        self.active = true;
        self.loop_radius = if scene.radius.is_finite() && scene.radius > 0.0 {
            (scene.radius * 1.5).clamp(WIRE_LOOP_MIN_RADIUS, WIRE_LOOP_MAX_RADIUS)
        } else {
            1.5
        };
        self.orientation = WireOrientation::default();
        self.direction_prediction = None;
        self.inside_prediction = None;
        self.skipped = false;
        self.revealed = false;
        self.invalidate_report();
    }

    /// Leave the activity. The next [`Self::activate`] starts fresh.
    pub fn deactivate(&mut self) {
        *self = Self::default();
    }

    /// Whether the lesson window should be shown.
    pub fn active(&self) -> bool {
        self.active
    }

    /// The movable loop radius `r`, distinct from the scene's shell radius `a`.
    pub fn loop_radius(&self) -> f64 {
        self.loop_radius
    }

    /// Move the loop. Non-finite or out-of-range values are refused without
    /// touching the current radius or the shell.
    pub fn set_loop_radius(&mut self, value: f64) -> bool {
        if !value.is_finite() || !(WIRE_LOOP_MIN_RADIUS..=WIRE_LOOP_MAX_RADIUS).contains(&value) {
            return false;
        }
        if value.to_bits() == self.loop_radius.to_bits() {
            return true;
        }
        self.loop_radius = value;
        self.invalidate_report();
        true
    }

    /// Set `r = 2r`, leaving the shell radius `a` untouched.
    pub fn double_loop_radius(&mut self) -> bool {
        self.set_loop_radius(self.loop_radius * 2.0)
    }

    /// The selected loop orientation.
    pub fn orientation(&self) -> WireOrientation {
        self.orientation
    }

    /// Select the loop orientation. This never touches the scene or field.
    pub fn set_orientation(&mut self, orientation: WireOrientation) {
        if orientation != self.orientation {
            self.orientation = orientation;
            self.invalidate_report();
        }
    }

    /// The compass-direction prediction, if any.
    pub fn direction_prediction(&self) -> Option<WireDirectionPrediction> {
        self.direction_prediction
    }

    /// The inside-loop prediction, if any.
    pub fn inside_prediction(&self) -> Option<WireInsidePrediction> {
        self.inside_prediction
    }

    /// Select or revise the direction prediction before the reveal.
    pub fn select_direction_prediction(&mut self, prediction: WireDirectionPrediction) {
        if !self.revealed {
            self.direction_prediction = Some(prediction);
            self.skipped = false;
        }
    }

    /// Select or revise the inside prediction before the reveal.
    pub fn select_inside_prediction(&mut self, prediction: WireInsidePrediction) {
        if !self.revealed {
            self.inside_prediction = Some(prediction);
            self.skipped = false;
        }
    }

    /// Skip both predictions without pretending they were made.
    pub fn skip_prediction(&mut self) {
        if !self.revealed {
            self.skipped = true;
            self.direction_prediction = None;
            self.inside_prediction = None;
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
                || (self.direction_prediction.is_some() && self.inside_prediction.is_some()))
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

    fn invalidate_report(&mut self) {
        self.report = None;
        self.report_scene = None;
        self.report_family = None;
        self.report_radius = 0.0;
        self.report_time = 0.0;
    }

    /// The live `r` and `2r` readings, recomputed only when an input changes.
    pub fn report_for(
        &mut self,
        scene: &MaxwellScene,
        family: FieldFamily,
        time: f64,
    ) -> &WireLessonReport {
        let fresh = self.report.is_some()
            && self.report_scene.as_ref() == Some(scene)
            && self.report_family == Some(family)
            && self.report_radius.to_bits() == self.loop_radius.to_bits()
            && self.report_orientation == self.orientation
            && self.report_time.to_bits() == time.to_bits();
        if !fresh {
            let radius = self.loop_radius;
            let orientation = self.orientation;
            self.report = Some(WireLessonReport {
                radius,
                orientation,
                field_vector: field_vector(scene, radius),
                at_radius: compute_reading(scene, radius, orientation, family, time),
                at_double: compute_reading(scene, radius * 2.0, orientation, family, time),
            });
            self.report_scene = Some(scene.clone());
            self.report_family = Some(family);
            self.report_radius = radius;
            self.report_orientation = orientation;
            self.report_time = time;
        }
        self.report.as_ref().expect("report was just built")
    }
}

/// Draw the activity window. The app restores or adopts the scene after it
/// applies the returned action.
pub fn show(
    ctx: &egui::Context,
    state: &mut WireLessonState,
    scene: &MaxwellScene,
    family: FieldFamily,
    time: f64,
) -> WireLessonAction {
    if !state.active() {
        return WireLessonAction::None;
    }
    let width = (ctx.content_rect().width() - 32.0).clamp(300.0, 460.0);
    let mut action = WireLessonAction::None;
    egui::Window::new(WIRE_LESSON_TITLE)
        .collapsible(false)
        .resizable(false)
        .default_width(width)
        .max_width(width)
        .show(ctx, |ui| {
            ui.label(WIRE_QUESTION);
            if !state.revealed() {
                ui.label(WIRE_ASSUMPTION_NOTE);
            }
            ui.add_space(4.0);
            ui.strong(WIRE_PROMPT);
            if !state.revealed() {
                ui.label(egui::RichText::new(format!("Hint: {WIRE_DIRECTION_HINT}")).small());
                ui.label(egui::RichText::new(format!("Hint: {WIRE_ORIENTATION_HINT}")).small());
            }
            ui.add_space(4.0);

            // Keep the selected orientation visible at every layout width, even
            // when the scrolling-free compact window clips the lower controls.
            ui.strong(format!(
                "Selected loop normal: {}; contour {}",
                state.orientation().normal_label(),
                state.orientation().contour_label()
            ));

            if !state.revealed() {
                draw_predictions(ui, state);
                ui.add_space(4.0);
            }

            draw_loop_controls(ui, state, scene);

            if !state.revealed() {
                draw_diagram(ui, state, scene);
                ui.small("The measured |B| and circulation stay hidden until you predict or skip.");
            } else {
                ui.separator();
                draw_result(ui, state, scene, family, time);
                draw_diagram(ui, state, scene);
            }

            ui.separator();
            ui.horizontal_wrapped(|ui| {
                if ui
                    .button("Back / Exit to my scene")
                    .on_hover_text(
                        "Restore the scene, projection and camera from before this lesson",
                    )
                    .clicked()
                {
                    action = WireLessonAction::Back;
                }
                if ui
                    .button("Keep exploring this scene")
                    .on_hover_text("Adopt the wire scene and close the lesson")
                    .clicked()
                {
                    action = WireLessonAction::KeepExploring;
                }
            });
        });
    action
}

fn draw_predictions(ui: &mut egui::Ui, state: &mut WireLessonState) {
    ui.label("Your compass-direction prediction (you can revise it before revealing):");
    for prediction in WireDirectionPrediction::ALL {
        let selected = state.direction_prediction() == Some(prediction);
        if ui.selectable_label(selected, prediction.label()).clicked() {
            state.select_direction_prediction(prediction);
        }
    }
    ui.label("Your inside-loop prediction (separate from the exterior field):");
    for prediction in WireInsidePrediction::ALL {
        let selected = state.inside_prediction() == Some(prediction);
        if ui.selectable_label(selected, prediction.label()).clicked() {
            state.select_inside_prediction(prediction);
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

fn draw_loop_controls(ui: &mut egui::Ui, state: &mut WireLessonState, scene: &MaxwellScene) {
    let mut radius = state.loop_radius();
    let response = ui.add(
        egui::Slider::new(&mut radius, WIRE_LOOP_MIN_RADIUS..=WIRE_LOOP_MAX_RADIUS)
            .text("loop radius r")
            .step_by(0.01),
    );
    if response.changed() {
        state.set_loop_radius(radius);
    }
    let mut exact = state.loop_radius();
    let response = ui.add(
        egui::DragValue::new(&mut exact)
            .speed(0.01)
            .range(WIRE_LOOP_MIN_RADIUS..=WIRE_LOOP_MAX_RADIUS)
            .prefix("exact r = "),
    );
    if response.changed()
        && (WIRE_LOOP_MIN_RADIUS..=WIRE_LOOP_MAX_RADIUS).contains(&exact)
        && exact.to_bits() != state.loop_radius().to_bits()
    {
        state.set_loop_radius(exact);
    }
    ui.horizontal_wrapped(|ui| {
        if ui
            .button("Inside (r = 0.5 a)")
            .on_hover_text("Move the loop inside the shell without changing the shell radius")
            .clicked()
        {
            state.set_loop_radius(scene.radius * 0.5);
        }
        if ui
            .button("Outside (r = 1.5 a)")
            .on_hover_text("Move the loop outside the shell without changing the shell radius")
            .clicked()
        {
            state.set_loop_radius(scene.radius * 1.5);
        }
        if ui
            .button("Double loop radius")
            .on_hover_text("Set r = 2r; the shell radius a stays fixed")
            .clicked()
        {
            state.double_loop_radius();
        }
    });
    ui.small(format!(
        "Normalized units: B0 = {:.3}, shell radius a = {:.3} (fixed); loop radius \
         r = {:.3}; 2r = {:.3}",
        scene.b0,
        scene.radius,
        state.loop_radius(),
        state.loop_radius() * 2.0
    ));

    ui.label("Loop orientation (independent of the current direction):");
    let mut orientation = state.orientation();
    for choice in WireOrientation::ALL {
        ui.radio_value(&mut orientation, choice, choice.label());
    }
    if orientation != state.orientation() {
        state.set_orientation(orientation);
    }
}

fn draw_diagram(ui: &mut egui::Ui, state: &WireLessonState, scene: &MaxwellScene) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().max(1.0), 150.0),
        egui::Sense::hover(),
    );
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
    let centre = rect.center();
    let shell = scene.radius.max(0.01);
    let max_world = (state.loop_radius() * 2.0).max(shell * 1.25);
    let scale = ((rect.height() * 0.5 - 22.0) as f64 / max_world).max(1.0);
    let to_screen =
        |x: f64, y: f64| egui::pos2(centre.x + (x * scale) as f32, centre.y - (y * scale) as f32);

    painter.circle_stroke(
        centre,
        (shell * scale) as f32,
        egui::Stroke::new(3.0, egui::Color32::from_rgb(120, 170, 255)),
    );
    painter.circle_stroke(
        centre,
        (state.loop_radius() * scale) as f32,
        egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 190, 80)),
    );
    painter.circle_stroke(
        centre,
        (state.loop_radius() * 2.0 * scale) as f32,
        egui::Stroke::new(1.5, egui::Color32::from_rgb(180, 140, 60)),
    );
    painter.text(
        egui::pos2(rect.left() + 8.0, rect.top() + 6.0),
        egui::Align2::LEFT_TOP,
        "Transverse slice, looking down +z",
        egui::FontId::proportional(11.0),
        ui.visuals().weak_text_color(),
    );
    painter.text(
        egui::pos2(rect.right() - 8.0, rect.top() + 6.0),
        egui::Align2::RIGHT_TOP,
        format!("normal {}", state.orientation().normal_label()),
        egui::FontId::proportional(10.5),
        ui.visuals().text_color(),
    );
    painter.text(
        to_screen(shell * 0.72, 0.72 * shell),
        egui::Align2::LEFT_BOTTOM,
        "shell a",
        egui::FontId::proportional(10.5),
        egui::Color32::from_rgb(120, 170, 255),
    );
    painter.text(
        to_screen(state.loop_radius(), 0.0),
        egui::Align2::LEFT_CENTER,
        "loop r",
        egui::FontId::proportional(10.5),
        egui::Color32::from_rgb(255, 190, 80),
    );
    painter.text(
        to_screen(state.loop_radius() * 2.0, 0.0),
        egui::Align2::LEFT_CENTER,
        "2r",
        egui::FontId::proportional(10.5),
        egui::Color32::from_rgb(180, 140, 60),
    );

    if !state.revealed() {
        return;
    }
    let reading_radius = state.loop_radius();
    let exterior = reading_radius > scene.radius;
    if exterior {
        for step in 0..8 {
            let angle = TAU * f64::from(step) / 8.0;
            let x = reading_radius * angle.cos();
            let y = reading_radius * angle.sin();
            let point = Vec3::new(x as f32, y as f32, 0.0);
            let Ok(field) = maxwell_field(scene, point) else {
                continue;
            };
            let magnitude = field.length();
            if !magnitude.is_finite() || magnitude <= 0.0 {
                continue;
            }
            let direction = field / magnitude;
            let start = to_screen(x, y);
            let tip = to_screen(
                x + f64::from(direction.x) * shell * 0.22,
                y + f64::from(direction.y) * shell * 0.22,
            );
            painter.arrow(
                start,
                tip - start,
                egui::Stroke::new(1.6, egui::Color32::from_rgb(110, 220, 140)),
            );
        }
        painter.text(
            egui::pos2(centre.x, rect.bottom() - 6.0),
            egui::Align2::CENTER_BOTTOM,
            "green arrows: B follows the +z current",
            egui::FontId::proportional(10.5),
            egui::Color32::from_rgb(110, 220, 140),
        );
    } else {
        painter.text(
            centre,
            egui::Align2::CENTER_CENTER,
            "inside: B = 0",
            egui::FontId::proportional(12.0),
            ui.visuals().text_color(),
        );
    }
}

fn draw_result(
    ui: &mut egui::Ui,
    state: &mut WireLessonState,
    scene: &MaxwellScene,
    family: FieldFamily,
    time: f64,
) {
    ui.heading("B and circulation result");
    let report = state.report_for(scene, family, time).clone();
    reading_card(ui, "At loop r", &report.at_radius);
    reading_card(ui, "At 2r", &report.at_double);
    show_ratios(ui, &report);

    ui.label(WIRE_UNITS_NOTE);
    ui.label(WIRE_ASSUMPTION_NOTE);
    ui.label(WIRE_INTERFACE_NOTE);
    ui.label(WIRE_DIRECTION_RESULT_NOTE);
    ui.label(WIRE_ORIENTATION_NOTE);
    ui.small(WIRE_UNCERTAINTY_NOTE);
    ui.label(WIRE_REASONING);

    if let Some(prediction) = state.direction_prediction() {
        ui.label(format!("Your compass prediction: {}", prediction.label()));
    }
    if let Some(prediction) = state.inside_prediction() {
        ui.label(format!("Your inside prediction: {}", prediction.label()));
    }
}

fn reading_card(ui: &mut egui::Ui, title: &str, reading: &WireLessonReading) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.label(format!(
            "{title} = {:.4} (a = fixed shell radius, r = loop radius)",
            reading.radius
        ));
        match &reading.kind {
            WireReadingKind::Exterior { field, circulation } => {
                ui.label(format!("|B| at loop = {field:.6}"));
                ui.label(format!(
                    "Signed circulation ∮B·dl = {}",
                    circulation_text(circulation)
                ));
                ui.label("Region: outside the shell");
            }
            WireReadingKind::Interior { circulation } => {
                ui.label(WIRE_INSIDE_NOTE);
                ui.label("|B| at loop = 0.000000");
                ui.label(format!(
                    "Signed circulation ∮B·dl = {}",
                    circulation_text(circulation)
                ));
                ui.label("Region: inside the shell");
            }
            WireReadingKind::Excluded { reason } => {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    format!("Excluded boundary sample: {reason}"),
                );
                ui.label(WIRE_INTERFACE_NOTE);
            }
        }
    });
}

fn show_ratios(ui: &mut egui::Ui, report: &WireLessonReport) {
    let both_exterior = matches!(report.at_radius.kind, WireReadingKind::Exterior { .. })
        && matches!(report.at_double.kind, WireReadingKind::Exterior { .. });
    if both_exterior
        && let (Some(first_field), Some(second_field)) =
            (report.at_radius.field(), report.at_double.field())
        && second_field > 0.0
    {
        ui.label(format!(
            "|B(r)| / |B(2r)| = {:.4} (doubling the radius halves |B|)",
            first_field / second_field
        ));
    } else if report.at_radius.field().is_some() && report.at_double.field().is_some() {
        ui.label(WIRE_RATIO_EXTERIOR_ONLY_NOTE);
    }
    if let (Some((first_circulation, first_error)), Some((second_circulation, second_error))) = (
        report.at_radius.circulation_estimate(),
        report.at_double.circulation_estimate(),
    ) {
        let difference = (first_circulation - second_circulation).abs();
        let uncertainty = first_error + second_error;
        ui.label(format!(
            "|circulation(r) - circulation(2r)| = {difference:.3e}; combined quadrature \
             uncertainty = {uncertainty:.3e}; equal within uncertainty: {}",
            if difference <= uncertainty + 1e-9 {
                "yes"
            } else {
                "no"
            }
        ));
    }
}

fn circulation_text(report: &CirculationReport) -> String {
    match &report.status {
        IntegralStatus::Estimate { value, error } => {
            format!("{value:.6} +/- {error:.2e} (quadrature)")
        }
        IntegralStatus::Unresolved { reason } => format!("unavailable ({reason})"),
        IntegralStatus::Singular { reason } => format!("singular ({reason})"),
    }
}
