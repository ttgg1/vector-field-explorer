//! The dedicated Induction panel and its draft/commit seam.
//!
//! The panel edits one [`InductionScene`] draft. Only a valid edit is handed to
//! [`State::set_induction_scene`], so the committed scene never sees a rejected
//! value and the last valid arrows stay on screen. The shared softening edit is
//! deliberately ordinary shared state: it updates `SourceScene.softening` and
//! clears both adapter caches, exactly as the Sources panel does.

use eframe::egui;

use crate::field_probe::CirculationLoop;
use crate::induction::{
    InductionCase, InductionFamilyStatus, InductionFluxComparison, InductionSampling,
    InductionScene, induction_flux_comparison, loop_orientation_label,
};
use crate::source_scene::{MAX_SOFTENING, MIN_SOFTENING, SourceDisplay};
use crate::viewmodel::State;

/// The comparison refresh quantum, matching the probe-report convention.
const FARADAY_REFRESH_SECONDS: f32 = 0.1;
/// Phase rows in the one-period table (nine values including both ends).
const FARADAY_SERIES_ROWS: usize = 9;

/// A one-shot request from the panel to the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InductionPanelAction {
    /// Frame the whole domain through the shared view-command seam.
    Fit,
    /// Replace the induction scene with [`InductionScene::default`].
    Reset,
}

/// Panel-local state: the last text/draft, the last rejected edit, and the
/// Advanced disclosure state. These are the entire public interface; a draft
/// with an error is never overwritten by the committed scene, which is what
/// keeps invalid text visible without a hidden dirty flag.
#[derive(Debug, Clone, Default)]
pub struct InductionPanelState {
    pub scene: InductionScene,
    pub error: Option<String>,
    pub advanced: bool,
    /// Session-only Faraday comparison disclosure. Not part of the document.
    pub faraday_open: bool,
    /// Index into the committed probe loops; the comparison never edits them.
    pub faraday_loop: usize,
    /// Session-only sign flip of the selected loop's axis and contour.
    pub faraday_reverse: bool,
    faraday: FaradayCache,
}

/// Session-only cache so an open comparison does not re-integrate every frame.
#[derive(Debug, Clone, Default)]
struct FaradayCache {
    series: Option<FaradaySeries>,
    current: Option<FaradayCurrent>,
}

#[derive(Debug, Clone, PartialEq)]
struct FaradayKey {
    loop_: CirculationLoop,
    scene: InductionScene,
    softening: f64,
}

#[derive(Debug, Clone)]
struct FaradaySeries {
    key: FaradayKey,
    comparisons: Vec<InductionFluxComparison>,
}

#[derive(Debug, Clone)]
struct FaradayCurrent {
    key: FaradayKey,
    quantized: i64,
    comparison: InductionFluxComparison,
}

impl InductionPanelState {
    /// Validate and hand the draft to the viewmodel. A rejected edit keeps both
    /// the committed scene and this draft, and records the actionable error.
    pub fn commit(&mut self, state: &mut State) -> bool {
        if state.set_induction_scene(self.scene.clone()) {
            self.error = None;
            true
        } else {
            self.error = state
                .induction_error
                .clone()
                .or_else(|| Some("Invalid induction scene.".to_owned()));
            false
        }
    }

    /// Refresh a clean draft from the committed scene. A draft that failed to
    /// commit keeps its error and text visible while the last valid arrows stay
    /// painted.
    fn sync(&mut self, state: &State) {
        if self.error.is_none() && self.scene != state.induction {
            self.scene = state.induction.clone();
        }
    }

    /// Commit one shared softening edit and invalidate both adapter caches.
    fn commit_softening(&mut self, state: &mut State, softening: f64) {
        let mut scene = state.sources.clone();
        if scene.set_softening(softening).is_err() {
            return;
        }
        state.set_source_scene(scene);
        state.induction_prepared = None;
        state.induction_sampling = None;
    }
}

/// Draw the Induction panel. `compact` applies the same finger-sized row floor
/// as the other compact sheets; wide keeps ordinary panel spacing. The caller
/// applies the returned action after the panel has finished painting.
pub fn show(
    ui: &mut egui::Ui,
    panel: &mut InductionPanelState,
    state: &mut State,
    compact: bool,
) -> Option<InductionPanelAction> {
    if compact {
        ui.spacing_mut().interact_size.y = crate::ui::layout::COMPACT_ROW_HEIGHT;
        ui.spacing_mut().item_spacing.y = crate::ui::layout::COMPACT_ITEM_SPACING;
    }
    panel.sync(state);

    let mut edited = false;
    let mut action = None;

    ui.heading("Induction");
    if let Some(error) = &panel.error {
        ui.colored_label(ui.visuals().error_fg_color, error);
    }

    ui.horizontal_wrapped(|ui| {
        for (case, label) in [
            (InductionCase::CurrentLoop, "Current loop"),
            (InductionCase::Solenoid, "Finite solenoid"),
        ] {
            if ui
                .selectable_label(panel.scene.case == case, label)
                .clicked()
                && panel.scene.case != case
            {
                panel.scene.case = case;
                if case == InductionCase::Solenoid {
                    if !panel.scene.length.is_finite() || panel.scene.length <= 0.0 {
                        panel.scene.length = 2.0;
                    }
                    if panel.scene.turns == 0 {
                        panel.scene.turns = 8;
                    }
                }
                edited = true;
            }
        }
    });

    edited |= drag_edit(ui, &mut panel.scene.radius, "Radius", 0.01, 0.05);
    if panel.scene.case == InductionCase::Solenoid {
        edited |= drag_edit(ui, &mut panel.scene.length, "Length", 0.01, 0.05);
        let mut turns = panel.scene.turns;
        ui.horizontal(|ui| {
            ui.label("Turns");
            if ui
                .add(
                    egui::DragValue::new(&mut turns)
                        .speed(1.0)
                        .range(1..=u32::MAX),
                )
                .changed()
            {
                panel.scene.turns = turns;
                edited = true;
            }
        });
    }
    edited |= drag_edit(
        ui,
        &mut panel.scene.amplitude,
        "Amplitude",
        f64::NEG_INFINITY,
        0.05,
    );
    edited |= drag_edit(ui, &mut panel.scene.frequency, "Frequency", 0.0, 0.01);

    ui.horizontal_wrapped(|ui| {
        for (display, label) in [
            (SourceDisplay::Electric, "Electric"),
            (SourceDisplay::Magnetic, "Magnetic"),
            (SourceDisplay::Both, "Both"),
        ] {
            if ui
                .selectable_label(panel.scene.display == display, label)
                .clicked()
                && panel.scene.display != display
            {
                panel.scene.display = display;
                edited = true;
            }
        }
    });

    ui.horizontal_wrapped(|ui| {
        if ui
            .button("Fit")
            .on_hover_text("Frame the whole domain (F)")
            .clicked()
        {
            action = Some(InductionPanelAction::Fit);
        }
        if ui
            .button("Reset induction section")
            .on_hover_text("Restore the default induction scene")
            .clicked()
        {
            panel.scene = InductionScene::default();
            panel.error = None;
            action = Some(InductionPanelAction::Reset);
        }
    });

    ui.label("Arrows only: streamlines, particles and click seeding stay off in Induction.");

    let mut advanced = panel.advanced;
    let response = egui::CollapsingHeader::new("Advanced")
        .open(Some(advanced))
        .show(ui, |ui| {
            show_advanced(ui, panel, state, &mut edited);
        });
    if response.header_response.clicked() {
        advanced = !advanced;
    }
    panel.advanced = advanced;

    let mut faraday_open = panel.faraday_open;
    let response = egui::CollapsingHeader::new("Faraday check: ∮E·dl vs −dΦB/dt")
        .id_salt("induction_faraday_check")
        .open(Some(faraday_open))
        .show(ui, |ui| {
            show_faraday(ui, panel, state);
        });
    if response.header_response.clicked() {
        faraday_open = !faraday_open;
    }
    panel.faraday_open = faraday_open;

    if edited && action.is_none() {
        panel.commit(state);
    }
    action
}

fn drag_edit(ui: &mut egui::Ui, value: &mut f64, label: &str, minimum: f64, speed: f64) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        if ui
            .add(
                egui::DragValue::new(value)
                    .speed(speed)
                    .range(minimum..=f64::INFINITY),
            )
            .changed()
        {
            changed = true;
        }
    });
    changed
}

fn show_advanced(
    ui: &mut egui::Ui,
    panel: &mut InductionPanelState,
    state: &mut State,
    edited: &mut bool,
) {
    let mut softening = state.sources.softening;
    ui.horizontal(|ui| {
        ui.label("Softening");
        if ui
            .add(
                egui::DragValue::new(&mut softening)
                    .speed(0.001)
                    .range(MIN_SOFTENING..=MAX_SOFTENING),
            )
            .changed()
        {
            panel.commit_softening(state, softening);
        }
        ui.label(format!("{MIN_SOFTENING} .. {MAX_SOFTENING}"));
    });

    let mut allow = panel.scene.allow_inaccurate;
    if ui
        .checkbox(
            &mut allow,
            "Show finite results labelled \"accuracy not met\"",
        )
        .changed()
    {
        panel.scene.allow_inaccurate = allow;
        *edited = true;
    }

    ui.label(status_text(state.induction_sampling.as_ref()));

    #[cfg(not(target_arch = "wasm32"))]
    {
        if let Some(source) = state
            .induction_sampling
            .as_ref()
            .and_then(|sampling| sampling.derivative_source.as_deref())
        {
            ui.label(format!("CAS derivative: {source}"));
        }
    }

    ui.label(
        "Model note: quasistatic axisymmetric approximation. E is reconstructed from the \
         softened vector potential, so arbitrary B alone cannot determine E. A static field \
         (zero frequency or zero amplitude) induces no E.",
    );
}

/// A readable one-line status for the Advanced section. The panel and the app
/// status bar derive their text from the same counters.
pub fn status_text(sampling: Option<&InductionSampling>) -> String {
    let Some(sampling) = sampling else {
        return "Status: no induction results sampled yet".to_owned();
    };
    if let Some(error) = &sampling.validation_error {
        return format!("Status: {error}");
    }
    format!(
        "Status: E {} · B {} · I {:.3} · dI/dt {:.3} · softening {:.3}",
        family_text(&sampling.electric),
        family_text(&sampling.magnetic),
        sampling.current,
        sampling.derivative,
        sampling.softening,
    )
}

fn family_text(family: &InductionFamilyStatus) -> String {
    let mut parts = vec![format!("{} drawn", family.drawn)];
    if family.target_met > 0 {
        parts.push(format!("{} target met", family.target_met));
    }
    if family.accuracy_not_met_drawn > 0 {
        parts.push(format!(
            "{} accuracy not met shown",
            family.accuracy_not_met_drawn
        ));
    } else if family.accuracy_not_met_total > 0 {
        parts.push(format!(
            "{} accuracy not met hidden",
            family.accuracy_not_met_total
        ));
    }
    if family.unavailable > 0 {
        parts.push(format!("{} unavailable", family.unavailable));
    }
    format!("[{}]", parts.join(", "))
}

/// The session-only Faraday comparison section. It reads the committed
/// induction scene and the selected committed probe loop and never edits
/// either; the selected loop is copied, and an optional sign flip lives only
/// in the panel state.
fn show_faraday(ui: &mut egui::Ui, panel: &mut InductionPanelState, state: &State) {
    ui.label(
        "∮E·dl is the direct circulation of the model's E around the loop; −dΦB/dt \
         samples the model's magnetic field through the enclosed planar surface. \
         Both are evaluated independently at the same time.",
    );
    ui.label("Normalized teaching units; this is not an SI calibration.");

    let loops = state.probe_loops().to_vec();
    if loops.is_empty() {
        ui.label(
            "No circulation loop yet. Add one in Field probes, then return here to \
             compare it with the flux through its enclosed surface.",
        );
        return;
    }
    if panel.faraday_loop >= loops.len() {
        panel.faraday_loop = 0;
    }

    ui.horizontal_wrapped(|ui| {
        ui.label("Loop");
        for (index, loop_) in loops.iter().enumerate() {
            let kind = match loop_.kind {
                crate::field_probe::LoopKind::Circle => "circle",
                crate::field_probe::LoopKind::Rectangle => "rectangle",
            };
            if ui
                .selectable_label(panel.faraday_loop == index, format!("Loop {}", index + 1))
                .on_hover_text(format!("{kind} loop from the Field probes section"))
                .clicked()
            {
                panel.faraday_loop = index;
            }
        }
    });
    ui.checkbox(
        &mut panel.faraday_reverse,
        "Reverse contour and normal (session only)",
    );

    let mut loop_ = loops[panel.faraday_loop];
    if panel.faraday_reverse {
        loop_.axis = -loop_.axis;
    }
    ui.label(loop_orientation_label(&loop_));

    refresh_faraday(panel, state, &loop_);

    if let Some(current) = &panel.faraday.current {
        let comparison = &current.comparison;
        ui.label(format!(
            "At t = {:.3}: I = {:+.4}, dI/dt = {:+.4}",
            comparison.time, comparison.current, comparison.current_derivative
        ));
        ui.monospace(format!(
            "∮E·dl   = {}",
            estimate_text(comparison.circulation_estimate())
        ));
        ui.monospace(format!(
            "−dΦB/dt = {}",
            estimate_text(comparison.flux_derivative_estimate())
        ));
        ui.label(format!(
            "Method: midpoint segments on the loop ({} samples) and midpoint patches on \
             the enclosed planar surface ({} patches); uncertainty is the \
             full/half-resolution difference, not a certified bound.",
            comparison.circulation_samples, comparison.unit_flux_samples
        ));
        let status = comparison.status_label();
        if comparison.is_confident() {
            ui.label(status);
        } else {
            ui.colored_label(ui.visuals().error_fg_color, status);
        }
        ui.label(format!("Softening: {:.4}", comparison.softening));
    }

    if state.induction.is_animated() {
        if let Some(series) = &panel.faraday.series {
            ui.label("One period on the shared time axis:");
            egui::Grid::new("induction_faraday_series")
                .num_columns(3)
                .striped(true)
                .show(ui, |ui| {
                    ui.strong("t");
                    ui.strong("∮E·dl");
                    ui.strong("−dΦB/dt");
                    ui.end_row();
                    for comparison in &series.comparisons {
                        ui.monospace(format!("{:.3}", comparison.time));
                        ui.monospace(estimate_text(comparison.circulation_estimate()));
                        ui.monospace(estimate_text(comparison.flux_derivative_estimate()));
                        ui.end_row();
                    }
                });
        }
    } else {
        ui.label(
            "Static current: I is constant, so both readouts are exactly zero within \
             their reported uncertainty.",
        );
    }
}

fn refresh_faraday(panel: &mut InductionPanelState, state: &State, loop_: &CirculationLoop) {
    let key = FaradayKey {
        loop_: *loop_,
        scene: state.induction.clone(),
        softening: state.sources.softening,
    };
    if panel.faraday.series.as_ref().map(|series| &series.key) != Some(&key) {
        let comparisons = series_times(&state.induction)
            .into_iter()
            .map(|t| induction_flux_comparison(&state.induction, loop_, t, key.softening))
            .collect();
        panel.faraday.series = Some(FaradaySeries {
            key: key.clone(),
            comparisons,
        });
    }

    let quantized = (state.time / FARADAY_REFRESH_SECONDS).round() as i64;
    let quantized_time = quantized as f32 * FARADAY_REFRESH_SECONDS;
    let stale = match &panel.faraday.current {
        Some(current) => current.key != key || current.quantized != quantized,
        None => true,
    };
    if stale {
        panel.faraday.current = Some(FaradayCurrent {
            key: key.clone(),
            quantized,
            comparison: induction_flux_comparison(&key.scene, loop_, quantized_time, key.softening),
        });
    }
}

/// One period for an animated scene; a static scene has no phase axis.
fn series_times(scene: &InductionScene) -> Vec<f32> {
    if scene.is_animated() {
        let period = 1.0 / scene.frequency;
        if period.is_finite() && period > 0.0 {
            let last = (FARADAY_SERIES_ROWS - 1) as f64;
            return (0..FARADAY_SERIES_ROWS)
                .map(|index| (period * index as f64 / last) as f32)
                .collect();
        }
    }
    vec![0.0]
}

fn estimate_text(estimate: Option<(f64, f64)>) -> String {
    match estimate {
        Some((value, error)) => format!("{value:+.4} ± {error:.4}"),
        None => "unavailable".to_string(),
    }
}
