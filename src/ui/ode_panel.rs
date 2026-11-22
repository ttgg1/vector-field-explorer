//! The responsive ODE workbench panel.
//!
//! The panel edits a local [`OdeScene`] draft and hands only a valid scene to
//! [`State::set_ode_scene`]. Text and seed edits commit on focus loss, Enter,
//! release or an explicit button, never once per dragged frame. The charts are
//! painted from the shared precomputed [`crate::ode::OdeSolution`]; they never
//! solve, interpolate a second trajectory, or invent samples.

use eframe::egui;

use crate::ode::{
    ODE_COLORS, ODE_MAX_DIMENSION, ODE_MAX_DURATION, ODE_MAX_SEEDS, ODE_MIN_DIMENSION, ODE_STEP,
    OdeChart, OdeScene, OdeSeed,
};
use crate::ode_compare::OdeCompareState;
use crate::ui::help_overlay::{HelpRow, HelpSection};
use crate::viewmodel::State;

/// A one-shot request from the panel to the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OdePanelAction {
    /// Advance the shared clock by one fixed RK4 step.
    Step,
    /// Frame the whole solution through the shared view-command seam.
    Fit,
    /// Restore the default ODE scene and view.
    Reset,
    /// One-click `v = (1, t)` frozen-line reference.
    ReferenceTimeDependent,
    /// One-click steady uniform `v = (1, 0)` reference.
    ReferenceSteady,
    /// Put back the formula that the last reference replaced.
    RestoreFormula,
}

/// Panel-local state: the editable draft, the last rejected-edit message and
/// the Advanced disclosure. A failed commit keeps this draft and its error
/// visible while the last valid solution stays painted.
#[derive(Debug, Clone, Default)]
pub struct OdePanelState {
    pub draft: OdeScene,
    pub error: Option<String>,
    pub advanced: bool,
    /// Session-only frozen-line comparison. Never serialized.
    pub compare: OdeCompareState,
    variables_all_off: bool,
}

impl OdePanelState {
    /// Validate and hand the draft to the viewmodel. A rejected edit keeps the
    /// committed scene and this draft, recording the actionable error.
    pub fn commit(&mut self, state: &mut State) -> bool {
        match state.set_ode_scene(self.draft.clone()) {
            Ok(()) => {
                self.sync_from(state);
                self.error = None;
                true
            }
            Err(_) => {
                self.error = state
                    .ode_error
                    .clone()
                    .or_else(|| Some("Invalid ODE scene.".to_owned()));
                false
            }
        }
    }

    /// Load the committed scene into the draft. Called by the app on mode
    /// entry, after a section reset and after persistence restore; ordinary
    /// frames deliberately leave a dirty draft alone.
    pub fn sync_from(&mut self, state: &State) {
        self.draft = state.ode.clone();
        self.error = state.ode_error.clone();
        self.variables_all_off = false;
        // Entering/resetting the workbench cancels any stale pick arm; the
        // session-only comparison seed itself survives.
        self.compare.arm_seed = false;
    }

    /// Apply the `v = (1, t)` reference in one shot. The previous formula is
    /// kept in the session-only comparison state so the user can restore it.
    pub fn use_time_dependent_reference(&mut self, state: &mut State) -> bool {
        self.use_reference(state, true)
    }

    /// Apply the steady uniform `v = (1, 0)` reference in one shot.
    pub fn use_steady_reference(&mut self, state: &mut State) -> bool {
        self.use_reference(state, false)
    }

    fn use_reference(&mut self, state: &mut State, time_dependent: bool) -> bool {
        let result = if time_dependent {
            crate::ode_compare::apply_time_dependent_reference(state)
        } else {
            crate::ode_compare::apply_steady_reference(state)
        };
        match result {
            Ok(previous) => {
                self.compare.enabled = true;
                self.compare.arm_seed = false;
                self.compare.seed = crate::ode_compare::REFERENCE_SEED;
                self.compare.reference_time = crate::ode_compare::FROZEN_REFERENCE_TIME;
                self.compare.frozen = None;
                self.compare.previous_formula = Some(previous);
                self.sync_from(state);
                true
            }
            Err(_) => {
                self.error = state
                    .ode_error
                    .clone()
                    .or_else(|| Some("The reference scene could not be solved.".to_owned()));
                false
            }
        }
    }

    /// Put back the formula the last reference replaced. The scene re-solves
    /// lazily through the ordinary current-field edit path.
    pub fn restore_previous_formula(&mut self, state: &mut State) -> bool {
        let Some(previous) = self.compare.previous_formula.take() else {
            return false;
        };
        self.compare.frozen = None;
        let changed = state.set_formula(previous);
        self.sync_from(state);
        changed
    }
}

/// Draw the ODE panel. `compact` applies the same finger-sized row floor as the
/// other compact sheets; wide keeps ordinary panel spacing. The caller applies
/// the returned action after the panel has finished painting.
pub fn show(
    ui: &mut egui::Ui,
    panel: &mut OdePanelState,
    state: &mut State,
    compact: bool,
) -> Option<OdePanelAction> {
    if compact {
        ui.spacing_mut().interact_size.y = crate::ui::layout::COMPACT_ROW_HEIGHT;
        ui.spacing_mut().item_spacing.y = crate::ui::layout::COMPACT_ITEM_SPACING;
    }

    let mut commit = false;
    let mut action = None;

    ui.heading("ODE");
    if let Some(error) = panel.error.as_ref().or(state.ode_error.as_ref()) {
        ui.colored_label(ui.visuals().error_fg_color, error);
    }

    ui.horizontal_wrapped(|ui| {
        for (source, label) in [
            (crate::ode::OdeSource::Equations, "Equations"),
            (crate::ode::OdeSource::CurrentField, "Current field"),
        ] {
            if ui
                .selectable_label(panel.draft.source == source, label)
                .on_hover_text(match source {
                    crate::ode::OdeSource::Equations => "Integrate the typed 2D/3D equations",
                    crate::ode::OdeSource::CurrentField => {
                        "Integrate the durable formula field over its domain"
                    }
                })
                .clicked()
                && panel.draft.source != source
            {
                panel.draft.source = source;
                if source == crate::ode::OdeSource::CurrentField {
                    // The solved scene's dimension must match the durable
                    // field's; a 2D draft cannot integrate a 3D field.
                    panel.draft.dimension = state
                        .field
                        .dimension
                        .clamp(ODE_MIN_DIMENSION, ODE_MAX_DIMENSION);
                }
                commit = true;
            }
        }
    });

    if panel.draft.source == crate::ode::OdeSource::Equations {
        ui.horizontal_wrapped(|ui| {
            for (dimension, label) in [(2usize, "2D"), (3, "3D")] {
                if ui
                    .selectable_label(panel.draft.dimension == dimension, label)
                    .on_hover_text("Typed equations are integrated in this dimension")
                    .clicked()
                    && panel.draft.dimension != dimension
                {
                    panel.draft.dimension = dimension;
                    commit = true;
                }
            }
        });

        ui.label(egui::RichText::new("Equations").strong());
        for component in 0..panel.draft.dimension.min(3) {
            let label = match component {
                0 => "dx/dt =",
                1 => "dy/dt =",
                _ => "dz/dt =",
            };
            ui.horizontal(|ui| {
                ui.label(label);
                let response = ui.add(
                    egui::TextEdit::singleline(&mut panel.draft.equations[component])
                        .desired_width(ui.available_width()),
                );
                if response.lost_focus() {
                    commit = true;
                }
            });
        }
    }

    ui.label(egui::RichText::new("Initial conditions").strong());
    let mut remove_seed = None;
    let mut seed_released = false;
    for index in 0..panel.draft.seeds.len() {
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("{}.", index + 1));
            for (axis, label) in ["x", "y", "z"].into_iter().enumerate() {
                ui.label(label);
                let response = ui.add(
                    egui::DragValue::new(&mut panel.draft.seeds[index].position[axis])
                        .speed(0.1)
                        .range(-1_000_000.0..=1_000_000.0),
                );
                if response.drag_stopped() || (response.changed() && !response.dragged()) {
                    seed_released = true;
                }
            }
            if ui
                .add_enabled(panel.draft.seeds.len() > 1, egui::Button::new("Remove"))
                .on_disabled_hover_text("At least one initial condition is required")
                .clicked()
            {
                remove_seed = Some(index);
            }
        });
    }
    if let Some(index) = remove_seed {
        panel.draft.seeds.remove(index);
        commit = true;
    } else if seed_released {
        commit = true;
    }
    ui.horizontal_wrapped(|ui| {
        let can_add = panel.draft.seeds.len() < ODE_MAX_SEEDS;
        if ui
            .add_enabled(can_add, egui::Button::new("Add initial condition"))
            .on_disabled_hover_text("At most eight initial conditions are supported")
            .clicked()
        {
            let index = panel.draft.seeds.len();
            panel.draft.seeds.push(OdeSeed {
                position: [0.5 + 0.25 * index as f64, -0.25, 0.0],
            });
            commit = true;
        }
        if ui
            .button("Reset initial conditions")
            .on_hover_text("Restore the default seed")
            .clicked()
        {
            panel.draft.seeds = OdeScene::default().seeds;
            commit = true;
        }
    });

    ui.horizontal(|ui| {
        ui.label("Total time");
        let response = ui.add(
            egui::DragValue::new(&mut panel.draft.duration)
                .speed(0.1)
                .range(0.1..=ODE_MAX_DURATION),
        );
        if response.drag_stopped() || (response.changed() && !response.dragged()) {
            commit = true;
        }
    });

    if let Some(next) = show_compare(ui, panel, state) {
        action = Some(next);
    }

    ui.horizontal_wrapped(|ui| {
        for (chart, label) in [
            (OdeChart::Phase, "Phase portrait"),
            (OdeChart::TimeSeries, "Time series"),
        ] {
            if ui
                .selectable_label(state.ode_chart == chart, label)
                .on_hover_text(match chart {
                    OdeChart::Phase => "Plot x against y from the shared solution samples",
                    OdeChart::TimeSeries => "Plot the selected variables against time",
                })
                .clicked()
            {
                state.set_ode_chart(chart);
            }
        }
    });

    if state.ode_chart == OdeChart::TimeSeries {
        ui.label(egui::RichText::new("Variables").strong());
        ui.horizontal_wrapped(|ui| {
            for (index, label) in ["x", "y", "z"].into_iter().enumerate() {
                let mut checked = state.ode_variables[index];
                if ui.checkbox(&mut checked, label).changed() {
                    let mut variables = state.ode_variables;
                    variables[index] = checked;
                    if !variables.iter().any(|enabled| *enabled) {
                        panel.variables_all_off = true;
                    } else if state.set_ode_variables(variables) {
                        panel.variables_all_off = false;
                    }
                }
            }
        });
        if panel.variables_all_off {
            ui.colored_label(ui.visuals().warn_fg_color, "Select at least one variable.");
        }
    }

    let has_error = panel.error.is_some() || state.ode_error.is_some();
    if has_error {
        ui.label("ODE solution unavailable; fix the scene to draw a chart.");
    } else if state.ode_solution.is_none() {
        ui.label("No ODE solution yet; edit the scene to solve it.");
    } else {
        match state.ode_chart {
            OdeChart::Phase => paint_phase_portrait(ui, state),
            OdeChart::TimeSeries => paint_time_series(ui, state),
        }
    }

    ui.horizontal_wrapped(|ui| {
        let step_enabled = state.ode_solution.is_some();
        if ui
            .add_enabled(step_enabled, egui::Button::new("Step"))
            .on_hover_text("Advance one fixed RK4 step on the shared clock")
            .on_disabled_hover_text("Step needs a valid ODE solution")
            .clicked()
        {
            action = Some(OdePanelAction::Step);
        }
        if ui
            .button("Fit")
            .on_hover_text("Frame the whole solution (F)")
            .clicked()
        {
            action = Some(OdePanelAction::Fit);
        }
        if ui
            .button("Reset ODE section")
            .on_hover_text("Restore the default ODE scene and camera")
            .clicked()
        {
            action = Some(OdePanelAction::Reset);
        }
    });

    let mut advanced = panel.advanced;
    let response = egui::CollapsingHeader::new("Advanced")
        .open(Some(advanced))
        .show(ui, |ui| show_advanced(ui, panel, state));
    if response.header_response.clicked() {
        advanced = !advanced;
    }
    panel.advanced = advanced;

    if commit {
        panel.commit(state);
    }
    action
}

/// The session-only frozen-line comparison section: enable, seed readout and
/// armed pick, reference actions, legend and the velocity-field caveat.
fn show_compare(
    ui: &mut egui::Ui,
    panel: &mut OdePanelState,
    state: &State,
) -> Option<OdePanelAction> {
    let mut action = None;
    let compare = &mut panel.compare;
    ui.separator();
    ui.label(egui::RichText::new("Frozen line vs trajectory").strong());
    let mut enabled = compare.enabled;
    if ui
        .checkbox(&mut enabled, "Show frozen line vs trajectory")
        .on_hover_text("Trace the frozen field line of v(x, 0) beside the ODE trajectory")
        .changed()
    {
        compare.enabled = enabled;
        if !enabled {
            compare.arm_seed = false;
            compare.frozen = None;
        }
    }
    if compare.enabled && state.ode.source != crate::ode::OdeSource::CurrentField {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            "The comparison draws the current formula field; choose Current field as the ODE source.",
        );
    }

    ui.label(format!(
        "Seed at t = {:.0}: ({:.2}, {:.2}, {:.2})",
        compare.reference_time, compare.seed[0], compare.seed[1], compare.seed[2]
    ));
    if ui
        .selectable_label(compare.arm_seed, "Pick seed in viewport")
        .on_hover_text("Arm the next ODE viewport click as the shared seed; Escape cancels")
        .clicked()
    {
        compare.arm_seed = !compare.arm_seed;
    }

    ui.horizontal_wrapped(|ui| {
        if ui
            .button("Use v = (1, t) reference")
            .on_hover_text("One click: formula (1, t), current-field ODE from the origin")
            .clicked()
        {
            action = Some(OdePanelAction::ReferenceTimeDependent);
        }
        if ui
            .button("Use v = (1, 0) reference")
            .on_hover_text("One click: steady uniform field; both curves follow the same path")
            .clicked()
        {
            action = Some(OdePanelAction::ReferenceSteady);
        }
        if compare.previous_formula.is_some() && ui.button("Restore previous formula").clicked() {
            action = Some(OdePanelAction::RestoreFormula);
        }
    });

    if compare.enabled {
        ui.colored_label(
            rgba(crate::ode_compare::FROZEN_COLOR),
            crate::ode_compare::FROZEN_LABEL,
        );
        ui.colored_label(seed_color(0), crate::ode_compare::TRAJECTORY_LABEL);
        ui.add(
            egui::Label::new(
                "Playback advances the trajectory marker; the frozen line stays at its t = 0 reference.",
            )
            .wrap(),
        );
    }
    ui.add(egui::Label::new(crate::ode_compare::CAVEAT).wrap());
    action
}

fn show_advanced(ui: &mut egui::Ui, panel: &OdePanelState, state: &State) {
    ui.label(format!("Fixed-step RK4, h = {ODE_STEP}"));
    if let Some(solution) = state.ode_solution.as_deref() {
        let samples: usize = solution
            .trajectories
            .iter()
            .map(|traj| traj.points.len())
            .sum();
        ui.label(format!(
            "Samples: {samples} · steps: {} · stopped: {}",
            samples.saturating_sub(solution.trajectories.len().max(1)),
            solution.stopped,
        ));
    } else {
        ui.label("Samples: none · no solution");
    }
    if panel.draft.source == crate::ode::OdeSource::CurrentField {
        ui.label(format!(
            "Current field dimension: {}D",
            state.field.dimension.max(2)
        ));
        if !state.parse_error.is_empty() {
            ui.colored_label(ui.visuals().error_fg_color, &state.parse_error);
        }
    }
    ui.add(
        egui::Label::new(
            "The solution is bounded by the scene duration and the sample cap; the shared clock \
             is the only timer.",
        )
        .wrap(),
    );
}

fn paint_phase_portrait(ui: &mut egui::Ui, state: &State) {
    let Some(solution) = state.ode_solution.as_deref() else {
        return;
    };
    let width = ui.available_width().max(1.0);
    let height = (width * 0.55).clamp(120.0, 220.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    if !rect.is_positive() {
        return;
    }
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, ui.visuals().extreme_bg_color);
    let plot = rect.shrink(10.0);
    if !plot.is_positive() {
        return;
    }

    let mut min = [f64::INFINITY; 2];
    let mut max = [f64::NEG_INFINITY; 2];
    for trajectory in &solution.trajectories {
        for sample in &trajectory.points {
            for axis in 0..2 {
                let value = sample.position[axis];
                if value.is_finite() {
                    min[axis] = min[axis].min(value);
                    max[axis] = max[axis].max(value);
                }
            }
        }
    }
    for axis in 0..2 {
        if !min[axis].is_finite() || !max[axis].is_finite() {
            return;
        }
        if (max[axis] - min[axis]).abs() <= f64::EPSILON {
            min[axis] -= 0.5;
            max[axis] += 0.5;
        }
    }
    let map = |x: f64, y: f64| {
        egui::pos2(
            plot.left() + ((x - min[0]) / (max[0] - min[0])) as f32 * plot.width(),
            plot.bottom() - ((y - min[1]) / (max[1] - min[1])) as f32 * plot.height(),
        )
    };

    for trajectory in &solution.trajectories {
        let color = seed_color(trajectory.seed_index);
        let mut previous = None;
        for sample in &trajectory.points {
            let [x, y, _] = sample.position;
            if !x.is_finite() || !y.is_finite() {
                previous = None;
                continue;
            }
            let point = map(x, y);
            if let Some(previous) = previous {
                painter.line_segment([previous, point], egui::Stroke::new(1.5, color));
            }
            previous = Some(point);
        }
    }

    let origin = map(0.0, 0.0);
    if plot.contains(origin) {
        let stroke = egui::Stroke::new(0.5, ui.visuals().weak_text_color());
        painter.line_segment(
            [
                egui::pos2(plot.left(), origin.y),
                egui::pos2(plot.right(), origin.y),
            ],
            stroke,
        );
        painter.line_segment(
            [
                egui::pos2(origin.x, plot.top()),
                egui::pos2(origin.x, plot.bottom()),
            ],
            stroke,
        );
    }

    for (index, marker) in solution.markers_at(state.time as f64).iter().enumerate() {
        let [x, y, _] = *marker;
        if x.is_finite() && y.is_finite() {
            painter.circle_filled(map(x, y), 3.0, seed_color(index));
        }
    }
}

fn paint_time_series(ui: &mut egui::Ui, state: &State) {
    let Some(solution) = state.ode_solution.as_deref() else {
        return;
    };
    let selected: Vec<usize> = state
        .ode_variables
        .iter()
        .enumerate()
        .filter_map(|(index, enabled)| enabled.then_some(index))
        .collect();
    if selected.is_empty() {
        ui.label("Select at least one variable.");
        return;
    }

    let width = ui.available_width().max(1.0);
    let height = (width * 0.55).clamp(120.0, 220.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    if !rect.is_positive() {
        return;
    }
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, ui.visuals().extreme_bg_color);
    let plot = rect.shrink(10.0);
    if !plot.is_positive() {
        return;
    }

    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for trajectory in &solution.trajectories {
        for sample in &trajectory.points {
            for &axis in &selected {
                let value = sample.position[axis];
                if value.is_finite() {
                    min = min.min(value);
                    max = max.max(value);
                }
            }
        }
    }
    if !min.is_finite() || !max.is_finite() {
        return;
    }
    if (max - min).abs() <= f64::EPSILON {
        min -= 0.5;
        max += 0.5;
    }
    let duration = solution.duration.max(f64::EPSILON);
    let map = |time: f64, value: f64| {
        egui::pos2(
            plot.left() + (time / duration).clamp(0.0, 1.0) as f32 * plot.width(),
            plot.bottom() - ((value - min) / (max - min)) as f32 * plot.height(),
        )
    };

    for trajectory in &solution.trajectories {
        let color = seed_color(trajectory.seed_index);
        for &axis in &selected {
            let mut previous = None;
            for sample in &trajectory.points {
                let value = sample.position[axis];
                if !sample.time.is_finite() || !value.is_finite() {
                    previous = None;
                    continue;
                }
                let point = map(sample.time, value);
                if let Some(previous) = previous {
                    painter.line_segment([previous, point], egui::Stroke::new(1.5, color));
                }
                previous = Some(point);
            }
        }
    }

    let time = (state.time as f64).clamp(0.0, duration);
    let x = plot.left() + (time / duration) as f32 * plot.width();
    painter.line_segment(
        [egui::pos2(x, plot.top()), egui::pos2(x, plot.bottom())],
        egui::Stroke::new(1.0, ui.visuals().weak_text_color()),
    );

    for (index, marker) in solution.markers_at(state.time as f64).iter().enumerate() {
        for &axis in &selected {
            let value = marker[axis];
            if value.is_finite() {
                painter.circle_filled(map(time, value), 3.0, seed_color(index));
            }
        }
    }
}

fn seed_color(index: usize) -> egui::Color32 {
    rgba(ODE_COLORS[index % ODE_COLORS.len()])
}

fn rgba(color: [f32; 4]) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(
        (color[0].clamp(0.0, 1.0) * 255.0) as u8,
        (color[1].clamp(0.0, 1.0) * 255.0) as u8,
        (color[2].clamp(0.0, 1.0) * 255.0) as u8,
        (color[3].clamp(0.0, 1.0) * 255.0) as u8,
    )
}

/// ODE-mode notes. A separate section keeps the binding, scalar, source and
/// induction tables exactly as reviewed; the Help window appends it last.
pub static ODE_HELP_SECTION: HelpSection = HelpSection {
    title: "ODE",
    rows: ODE_HELP_ROWS,
};

pub static ODE_HELP_ROWS: &[HelpRow] = &[
    HelpRow {
        input: "Model",
        action: "fixed-step RK4 teaching model; bounded samples, not a physical solver",
    },
    HelpRow {
        input: "Clock",
        action: "one shared clock; no private timer or ODE loop period",
    },
    HelpRow {
        input: "Source",
        action: "typed 2D/3D equations or the current formula field over its durable domain",
    },
    HelpRow {
        input: "Initial conditions",
        action: "bounded to eight finite seeds; playback stops at the scene duration",
    },
    HelpRow {
        input: "Charts",
        action: "phase portrait x/y projection and per-variable time series from the same samples",
    },
    HelpRow {
        input: "Variables",
        action: "at least one time-series variable stays selected",
    },
    HelpRow {
        input: "Step",
        action: "advance one fixed RK4 step on the shared clock; disabled without a solution",
    },
    HelpRow {
        input: "Features",
        action: "streamlines, particles and click seeding stay off in ODE",
    },
    HelpRow {
        input: "CAS",
        action: "CAS edits the formula field; switch to Formula.",
    },
];

/// The ODE help rows, exposed for the Help window and tests.
pub fn ode_help_rows() -> &'static [HelpRow] {
    ODE_HELP_ROWS
}
