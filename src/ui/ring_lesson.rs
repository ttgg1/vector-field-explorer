//! Issue #224: the charged-ring axial-field activity.
//!
//! The activity reuses the shipped `Uniform ring` source preset and its
//! softened evaluator. It never mutates the source model or invents a second
//! quadrature: the softened curve comes from the same public [`Evaluator`] the
//! Sources adapter uses, while the analytic reference is the ideal axis field
//! `E_z = kQz/(R^2 + z^2)^(3/2)`.
//!
//! Everything here is session-only UI state. The app snapshots the starting
//! scene and camera before opening the lesson; this module only reports its
//! `Back/Exit` and `Keep exploring` actions.

use std::f64::consts::FRAC_1_SQRT_2;
use std::sync::OnceLock;

use eframe::egui;
use glam::DVec3;

use crate::source_presets::{SourcePreset, source_preset};
use crate::source_scene::SourceScene;
use crate::sources::{EvalOptions, Evaluator, FamilyResult, SourceSet, WorkLimits};

/// Window title for the activity.
pub const RING_LESSON_TITLE: &str = "Griffiths 2.5 · Charged ring";
/// The explicit physics question.
pub const RING_QUESTION: &str = "A positive ring of radius R lies in the xy plane. Where along its axis is a positive test charge pushed hardest, and what happens as it crosses the centre?";
/// The prediction prompt shown before any measured result.
pub const RING_PROMPT: &str =
    "Predict first: where is the push strongest, and what happens as the probe crosses the centre?";
/// The first hint distinguishes the ring radius from the probe coordinate.
pub const RING_HINT: &str = "R is the ring's fixed radius in the xy plane; z is the probe's signed position along the axis. Sliding the probe changes z, never R.";
/// The analytic axis field with the shipped preset's `Q = 1`, `R = 1` (normalized teaching units).
pub const RING_IDEAL_FORMULA: &str =
    "Continuous CAS form: Ez = k*Q*z / (R^2 + z^2)^(3/2), with Q and R retained.";
/// The same axis field as a CAS input line with `Q` and `R` kept symbolic.
pub const RING_CAS_EXPRESSION: &str = "k*Q*z/(R^2+z^2)^(3/2)";
/// One-line reminder that the plotted curve uses the preset's normalized units.
pub const RING_UNITS_NOTE: &str = "k = 1 in normalized teaching units";
/// Pairing explanation for the transverse components.
pub const RING_PAIRING_NOTE: &str = "Pair opposite ring elements: their transverse components are equal and opposite, so only their axial components add.";
/// Why the displayed softened evaluator is a different curve.
pub const RING_SOFTENED_NOTE: &str = "The shipped softened evaluator (softening 0.1) is drawn as a separate curve. It is not graded against the ideal |z| = R/√2 peak.";
/// The ideal reasoning after the reveal.
pub const RING_REASONING: &str = "The ideal axis field is zero at z = 0, is odd under z → -z, and reaches its maximum magnitude at |z| = R/√2.";
/// A short axis label shared by the chart and the side view.
pub const RING_AXIS_LABEL: &str = "z (signed axial probe position)";
/// The ring radius used by the shipped preset.
pub const RING_RADIUS: f64 = 1.0;
/// The ring charge used by the shipped preset.
pub const RING_CHARGE: f64 = 1.0;
/// The ideal peak location for `R = 1`.
pub const RING_STRONGEST_Z: f64 = FRAC_1_SQRT_2;
/// Probe range drawn and edited by the activity.
pub const RING_Z_MIN: f64 = -2.0;
/// Upper end of the probe range.
pub const RING_Z_MAX: f64 = 2.0;

/// The learner's prediction, revisable until the result is revealed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RingPrediction {
    /// The push is strongest at the centre.
    Centre,
    /// The push is strongest partway out along the axis.
    PartwayOut,
    /// The push keeps growing far from the ring.
    FarAway,
}

impl RingPrediction {
    /// Every offered prediction, in the order the prompt lists them.
    pub const ALL: [Self; 3] = [Self::Centre, Self::PartwayOut, Self::FarAway];

    /// The label shown on the prediction control.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Centre => "Strongest at the centre (z = 0)",
            Self::PartwayOut => "Strongest partway out along the axis",
            Self::FarAway => "Strongest far from the ring",
        }
    }
}

/// One ideal/softened reading at the probe position.
#[derive(Debug, Clone, PartialEq)]
pub struct RingReading {
    /// Signed probe position on the axis.
    pub z: f64,
    /// The ideal axis field `kQz/(R^2+z^2)^(3/2)`.
    pub ideal_ez: f64,
    /// The shipped softened evaluator's axial component, when available.
    pub softened_ez: Option<f64>,
}

impl RingReading {
    /// The signed difference between the evaluator and the ideal reference.
    pub fn softened_difference(&self) -> Option<f64> {
        self.softened_ez.map(|value| value - self.ideal_ez)
    }
}

/// Ideal axis field for the shipped preset, in normalized teaching units.
pub fn ideal_axial_field(z: f64) -> f64 {
    let radius_squared = RING_RADIUS * RING_RADIUS;
    RING_CHARGE * z / (radius_squared + z * z).powf(1.5)
}

/// The shipped `Uniform ring` preset's scene.
pub fn uniform_ring_scene() -> SourceScene {
    let preset: &SourcePreset =
        source_preset("Uniform ring").expect("the shipped Uniform ring preset exists");
    (preset.build)()
}

/// Evaluate the shipped ring model at `(0, 0, z)` through the ordinary public
/// evaluator and its scene-wide softening. This is the same evaluator the
/// Sources adapter uses; no separate quadrature is introduced.
pub fn softened_axial_field(z: f64) -> Option<f64> {
    let scene = uniform_ring_scene();
    if scene.validate().is_err() {
        return None;
    }
    let sources: Vec<_> = scene.enabled_sources().copied().collect();
    let set = SourceSet::new(&sources, &WorkLimits::default(), &|| false).ok()?;
    let options = EvalOptions {
        softening: scene.softening,
        ..EvalOptions::default()
    };
    let mut evaluator = Evaluator::new(&set, options, &|| false).ok()?;
    let sample = evaluator.evaluate(DVec3::new(0.0, 0.0, z), &|| false);
    match sample.electric {
        FamilyResult::TargetMet(estimate) | FamilyResult::AccuracyNotMet(estimate, _) => {
            Some(estimate.value.z)
        }
        FamilyResult::Unavailable(_) => None,
    }
}

/// Session-only activity state. `Default` is inactive.
#[derive(Debug, Clone, Default)]
pub struct RingLessonState {
    active: bool,
    probe_z: f64,
    prediction: Option<RingPrediction>,
    skipped: bool,
    revealed: bool,
    cache: Option<RingReading>,
}

impl RingLessonState {
    /// Activate a fresh session at `z = 1`.
    pub fn activate(&mut self) {
        self.active = true;
        self.probe_z = 1.0;
        self.prediction = None;
        self.skipped = false;
        self.revealed = false;
        self.cache = None;
    }

    /// Leave the activity; the caller decides whether to restore or adopt the scene.
    pub fn deactivate(&mut self) {
        *self = Self::default();
    }

    /// Whether the lesson window should be shown.
    pub fn active(&self) -> bool {
        self.active
    }

    /// The current signed probe position.
    pub fn probe_z(&self) -> f64 {
        self.probe_z
    }

    /// Move the probe. Non-finite values are refused; finite values clamp to the
    /// drawn range so the marker and the control cannot disagree.
    pub fn set_probe_z(&mut self, z: f64) -> bool {
        if !z.is_finite() {
            return false;
        }
        self.probe_z = z.clamp(RING_Z_MIN, RING_Z_MAX);
        self.cache = None;
        true
    }

    /// The current prediction, if the learner has made one.
    pub fn prediction(&self) -> Option<RingPrediction> {
        self.prediction
    }

    /// Select or revise the prediction before the reveal.
    pub fn select_prediction(&mut self, prediction: RingPrediction) {
        if !self.revealed {
            self.prediction = Some(prediction);
            self.skipped = false;
        }
    }

    /// Skip the prediction without pretending one was made.
    pub fn skip_prediction(&mut self) {
        if !self.revealed {
            self.skipped = true;
            self.prediction = None;
        }
    }

    /// Whether a prediction was skipped rather than supplied.
    pub fn skipped(&self) -> bool {
        self.skipped
    }

    /// Whether the result has been revealed.
    pub fn revealed(&self) -> bool {
        self.revealed
    }

    /// The reveal is gated on a prediction or an explicit skip.
    pub fn can_reveal(&self) -> bool {
        !self.revealed && (self.prediction.is_some() || self.skipped)
    }

    /// Reveal the comparison and reasoning. Returns false before the prompt is
    /// answered or explicitly skipped, so the prompt always precedes the result.
    pub fn reveal(&mut self) -> bool {
        if !self.can_reveal() {
            return false;
        }
        self.revealed = true;
        true
    }

    /// The ideal/softened reading at the current probe, cached per position.
    pub fn reading(&mut self) -> &RingReading {
        let stale = self
            .cache
            .as_ref()
            .is_none_or(|reading| reading.z.to_bits() != self.probe_z.to_bits());
        if stale {
            let z = self.probe_z;
            self.cache = Some(RingReading {
                z,
                ideal_ez: ideal_axial_field(z),
                softened_ez: softened_axial_field(z),
            });
        }
        self.cache.as_ref().expect("reading was just built")
    }
}

/// What the app should do after the lesson window's one-shot action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RingLessonAction {
    None,
    /// Restore the scene and camera captured before the lesson opened.
    Back,
    /// Keep the ring scene and close the lesson.
    KeepExploring,
    /// Put the analytic axis formula into the native CAS input.
    SendToCas,
}

/// Draw the activity window. The app restores or adopts the scene after it
/// applies the returned action.
pub fn show(ctx: &egui::Context, state: &mut RingLessonState) -> RingLessonAction {
    if !state.active() {
        return RingLessonAction::None;
    }
    let width = (ctx.content_rect().width() - 32.0).clamp(300.0, 460.0);
    let mut action = RingLessonAction::None;
    egui::Window::new(RING_LESSON_TITLE)
        .collapsible(false)
        .resizable(false)
        .default_width(width)
        .max_width(width)
        .show(ctx, |ui| {
            ui.label(RING_QUESTION);
            ui.add_space(4.0);
            ui.strong(RING_PROMPT);
            ui.label(egui::RichText::new(format!("Hint: {RING_HINT}")).small());
            ui.add_space(4.0);

            if !state.revealed() {
                ui.label("Your prediction (you can revise it before revealing):");
                for prediction in RingPrediction::ALL {
                    let selected = state.prediction() == Some(prediction);
                    if ui.selectable_label(selected, prediction.label()).clicked() {
                        state.select_prediction(prediction);
                    }
                }
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .button("Skip prediction")
                        .on_hover_text("Reveal the comparison without committing to a prediction")
                        .clicked()
                    {
                        state.skip_prediction();
                    }
                    let enabled = state.can_reveal();
                    if ui
                        .add_enabled(enabled, egui::Button::new("Reveal result"))
                        .on_hover_text("Predict or skip first, then reveal the comparison")
                        .clicked()
                    {
                        state.reveal();
                    }
                });
                if state.skipped() {
                    ui.label("Prediction skipped; the reasoning is still available.");
                }
            }

            let mut z = state.probe_z();
            let response = ui.add(
                egui::Slider::new(&mut z, RING_Z_MIN..=RING_Z_MAX)
                    .text("probe position z")
                    .step_by(0.01),
            );
            if response.changed() {
                state.set_probe_z(z);
            }
            ui.small(format!(
                "Ring radius R = {RING_RADIUS:.2} (fixed); probe z = {:.3}",
                state.probe_z()
            ));

            let reading = state.reading().clone();
            draw_side_view(ui, &reading, state.revealed());
            draw_graph(ui, &reading, state.revealed());
            ui.small(RING_UNITS_NOTE);

            if state.revealed() {
                ui.separator();
                ui.heading("Axial field result");
                ui.label(format!(
                    "At z = {:.3}: ideal signed E_z = {:.6}",
                    reading.z, reading.ideal_ez
                ));
                match reading.softened_ez {
                    Some(value) => ui.label(format!(
                        "Shipped softened evaluator E_z = {value:.6} (difference {:.6})",
                        value - reading.ideal_ez
                    )),
                    None => ui.colored_label(
                        ui.visuals().warn_fg_color,
                        "The shipped softened evaluator could not supply a finite sample here.",
                    ),
                };
                if let Some(prediction) = state.prediction() {
                    ui.label(format!("Your prediction: {}", prediction.label()));
                }
                ui.label(RING_REASONING);
                ui.label(RING_PAIRING_NOTE);
                ui.label(RING_IDEAL_FORMULA);
                #[cfg(not(target_arch = "wasm32"))]
                if ui
                    .button("Show axis formula in CAS")
                    .on_hover_text(
                        "Put Ez = k*Q*z/(R^2+z^2)^(3/2) in the CAS input with Q and R symbolic",
                    )
                    .clicked()
                {
                    action = RingLessonAction::SendToCas;
                }
                ui.label(RING_SOFTENED_NOTE);
            }

            ui.separator();
            ui.horizontal_wrapped(|ui| {
                if ui
                    .button("Back / Exit to my scene")
                    .on_hover_text("Restore the scene and camera from before this lesson")
                    .clicked()
                {
                    action = RingLessonAction::Back;
                }
                if ui
                    .button("Keep exploring this scene")
                    .on_hover_text("Adopt the ring scene and close the lesson")
                    .clicked()
                {
                    action = RingLessonAction::KeepExploring;
                }
            });
        });
    action
}

/// Which axial-graph layers are painted. The ideal/softened curves and their
/// legend wait for the result so the prediction is not pre-answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphVisibility {
    /// Whether the ideal and softened curves are drawn.
    pub curves: bool,
    /// Whether the two-curve legend is drawn.
    pub legend: bool,
}

/// The graph layers allowed at the current gate state.
#[doc(hidden)]
pub fn graph_visibility(revealed: bool) -> GraphVisibility {
    GraphVisibility {
        curves: revealed,
        legend: revealed,
    }
}

/// Map a signed probe reading to the side view's x coordinate.
///
/// The axis origin (`z = 0`) sits at the ring's centre; the full drawn range
/// `RING_Z_MIN..=RING_Z_MAX` spans the usable width with a 14 pt inset at each
/// end. The result is clamped so the probe marker can never leave the canvas.
#[doc(hidden)]
pub fn probe_x_for_z(rect: egui::Rect, z: f64) -> f32 {
    let z_span = RING_Z_MAX - RING_Z_MIN;
    let usable = (rect.width() - 28.0).max(0.0);
    let offset = if z_span > 0.0 {
        (z / z_span) as f32 * usable
    } else {
        0.0
    };
    let left_limit = rect.left() + 14.0;
    let right_limit = rect.right() - 14.0;
    if left_limit <= right_limit {
        (rect.center().x + offset).clamp(left_limit, right_limit)
    } else {
        rect.center().x
    }
}

fn draw_side_view(ui: &mut egui::Ui, reading: &RingReading, revealed: bool) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().max(1.0), 154.0),
        egui::Sense::hover(),
    );
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
    let axis_y = rect.center().y;
    let axis_stroke = egui::Stroke::new(1.5, ui.visuals().weak_text_color());
    painter.line_segment(
        [
            egui::pos2(rect.left() + 8.0, axis_y),
            egui::pos2(rect.right() - 8.0, axis_y),
        ],
        axis_stroke,
    );
    let centre_x = rect.center().x;
    // The ring is seen edge-on: a short vertical segment through the centre.
    let ring_half = (rect.height() * 0.32).min(44.0);
    painter.line_segment(
        [
            egui::pos2(centre_x, axis_y - ring_half),
            egui::pos2(centre_x, axis_y + ring_half),
        ],
        egui::Stroke::new(3.0, egui::Color32::from_rgb(120, 170, 255)),
    );
    painter.text(
        egui::pos2(centre_x + 6.0, axis_y - ring_half),
        egui::Align2::LEFT_BOTTOM,
        "ring R (edge-on)",
        egui::FontId::proportional(11.0),
        ui.visuals().text_color(),
    );
    // Pair two opposite ring elements and show their transverse components
    // cancelling while their axial components add.
    let pair_color = egui::Color32::from_rgb(255, 190, 80);
    let upper = egui::pos2(centre_x, axis_y - ring_half);
    let lower = egui::pos2(centre_x, axis_y + ring_half);
    painter.circle_filled(upper, 3.5, pair_color);
    painter.circle_filled(lower, 3.5, pair_color);
    painter.arrow(
        upper,
        egui::vec2(18.0, 0.0),
        egui::Stroke::new(1.5, pair_color),
    );
    painter.arrow(
        lower,
        egui::vec2(-18.0, 0.0),
        egui::Stroke::new(1.5, pair_color),
    );
    painter.text(
        egui::pos2(rect.left() + 8.0, rect.bottom() - 6.0),
        egui::Align2::LEFT_BOTTOM,
        "paired elements: transverse components cancel, axial components add",
        egui::FontId::proportional(10.5),
        pair_color,
    );

    let probe = egui::pos2(probe_x_for_z(rect, reading.z), axis_y);
    painter.circle_filled(probe, 4.5, egui::Color32::from_rgb(255, 90, 90));
    painter.text(
        egui::pos2(probe.x, axis_y + 10.0),
        egui::Align2::CENTER_TOP,
        "probe",
        egui::FontId::proportional(11.0),
        ui.visuals().text_color(),
    );
    if revealed {
        let field_scale = 120.0;
        let field = reading.ideal_ez * field_scale;
        let tip = egui::pos2(probe.x, probe.y - field as f32);
        painter.arrow(
            probe,
            tip - probe,
            egui::Stroke::new(2.0, egui::Color32::from_rgb(110, 220, 140)),
        );
        painter.text(
            egui::pos2(probe.x + 8.0, probe.y - field as f32),
            egui::Align2::LEFT_CENTER,
            format!("Ez = {:.3}", reading.ideal_ez),
            egui::FontId::proportional(11.0),
            ui.visuals().text_color(),
        );
    }
    painter.text(
        egui::pos2(rect.left() + 8.0, rect.top() + 6.0),
        egui::Align2::LEFT_TOP,
        "Side view: ring, axis and probe",
        egui::FontId::proportional(11.0),
        ui.visuals().weak_text_color(),
    );
}

fn draw_graph(ui: &mut egui::Ui, reading: &RingReading, revealed: bool) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().max(1.0), 170.0),
        egui::Sense::hover(),
    );
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
    let plot = rect.shrink2(egui::vec2(26.0, 12.0));
    let zero_y = plot.center().y;
    painter.line_segment(
        [
            egui::pos2(plot.left(), zero_y),
            egui::pos2(plot.right(), zero_y),
        ],
        egui::Stroke::new(1.0, ui.visuals().weak_text_color()),
    );
    painter.line_segment(
        [
            egui::pos2(plot.left(), plot.top()),
            egui::pos2(plot.left(), plot.bottom()),
        ],
        egui::Stroke::new(1.0, ui.visuals().weak_text_color()),
    );
    let e_max = 0.5;
    let to_point = |z: f64, e: f64| {
        let x = plot.left() + ((z - RING_Z_MIN) / (RING_Z_MAX - RING_Z_MIN)) as f32 * plot.width();
        let y = zero_y - (e / e_max) as f32 * (plot.height() * 0.5);
        egui::pos2(x, y)
    };
    let visibility = graph_visibility(revealed);
    if visibility.curves {
        let ideal: Vec<egui::Pos2> = (0..=160)
            .map(|step| {
                let z = RING_Z_MIN + (RING_Z_MAX - RING_Z_MIN) * step as f64 / 160.0;
                to_point(z, ideal_axial_field(z))
            })
            .collect();
        painter.add(egui::Shape::line(
            ideal,
            egui::Stroke::new(2.0, egui::Color32::from_rgb(120, 210, 255)),
        ));
        let softened: Vec<Option<egui::Pos2>> = softened_curve()
            .iter()
            .enumerate()
            .map(|(step, value)| {
                let z = RING_Z_MIN + (RING_Z_MAX - RING_Z_MIN) * step as f64 / 160.0;
                value.map(|e| to_point(z, e))
            })
            .collect();
        draw_dashed(&painter, &softened, egui::Color32::from_rgb(255, 190, 80));
    }
    painter.text(
        egui::pos2(plot.left(), rect.top() + 2.0),
        egui::Align2::LEFT_TOP,
        "Signed axial field Ez vs probe z",
        egui::FontId::proportional(11.0),
        ui.visuals().weak_text_color(),
    );
    painter.text(
        egui::pos2(plot.left(), rect.bottom() - 2.0),
        egui::Align2::LEFT_BOTTOM,
        "-z",
        egui::FontId::proportional(10.0),
        ui.visuals().weak_text_color(),
    );
    painter.text(
        egui::pos2(plot.right(), rect.bottom() - 2.0),
        egui::Align2::RIGHT_BOTTOM,
        "+z",
        egui::FontId::proportional(10.0),
        ui.visuals().weak_text_color(),
    );
    if visibility.legend {
        painter.text(
            egui::pos2(plot.right(), rect.top() + 2.0),
            egui::Align2::RIGHT_TOP,
            "ideal — · softened ----",
            egui::FontId::proportional(10.0),
            ui.visuals().text_color(),
        );
    }
    if revealed {
        let marker = to_point(reading.z, reading.ideal_ez);
        painter.line_segment(
            [
                egui::pos2(marker.x, plot.top()),
                egui::pos2(marker.x, plot.bottom()),
            ],
            egui::Stroke::new(1.0, egui::Color32::from_rgb(255, 90, 90)),
        );
        painter.circle_filled(marker, 4.0, egui::Color32::from_rgb(255, 90, 90));
        if let Some(value) = reading.softened_ez {
            painter.circle_stroke(
                to_point(reading.z, value),
                3.5,
                egui::Stroke::new(1.5, egui::Color32::from_rgb(255, 190, 80)),
            );
        }
    }
}

/// The softened evaluator curve, sampled once per process on the fixed graph
/// grid. This keeps the public evaluator out of the per-frame paint path.
fn softened_curve() -> &'static [Option<f64>] {
    static CURVE: OnceLock<Vec<Option<f64>>> = OnceLock::new();
    CURVE.get_or_init(|| {
        (0..=160)
            .map(|step| {
                let z = RING_Z_MIN + (RING_Z_MAX - RING_Z_MIN) * step as f64 / 160.0;
                softened_axial_field(z)
            })
            .collect()
    })
}

fn draw_dashed(painter: &egui::Painter, points: &[Option<egui::Pos2>], color: egui::Color32) {
    let stroke = egui::Stroke::new(1.5, color);
    let mut segment = Vec::new();
    for (index, point) in points.iter().enumerate() {
        if let Some(point) = point {
            segment.push(*point);
            if segment.len() == 2 {
                if index % 2 == 0 {
                    painter.line_segment([segment[0], segment[1]], stroke);
                }
                segment.clear();
            }
        } else {
            segment.clear();
        }
    }
}
