//! The dedicated Maxwell panel and its draft/commit seam.
//!
//! The panel edits one [`MaxwellScene`] draft. Only a valid edit is handed to
//! [`State::set_maxwell_scene`], so the committed scene never sees a rejected
//! value and the last valid arrows stay on screen. Numeric edits commit on
//! release, focus loss or Enter; a plain frame never commits merely because a
//! draft exists.
//!
//! Warnings in Advanced come from the sampling that belongs to the committed
//! scene. A valid scene edit clears the cached sampling, so a warning from a
//! prior scene can never be presented as current.

use eframe::egui;
use glam::Vec3;

use crate::maxwell::{
    ConductorOutline, MaxwellCase, MaxwellFamily, MaxwellScene, MaxwellStop, maxwell_assumptions,
    maxwell_family, maxwell_warning,
};
use crate::maxwell_learning::MaxwellEquation;
use crate::maxwell_residual::{TermStatus, residual};
use crate::maxwell_wave::{MAX_AMPLITUDE, MIN_WAVELENGTH, WaveSpec, equation_terms};
use crate::sources::FieldFamily;
use crate::ui::gauss_shell::{GaussShellAction, GaussShellState};
use crate::ui::help_overlay::HelpRow;
use crate::ui::maxwell_learning_panel::{MaxwellLearningPanel, MeasurementRequest};
use crate::viewmodel::State;

/// The exact arrows-only note shared by the panel and the Features section.
pub const MAXWELL_ONLY_NOTE: &str =
    "Arrows only: streamlines, particles and click seeding stay off in Maxwell.";

/// Exact 2D-conductor applicability note.
pub const MAXWELL_CONDUCTOR_NOTE: &str =
    "Conductor2D is an infinite-cylinder cross-section, not a 3D finite object.";

/// The Advanced accuracy opt-in label, shared by the panel and tests.
pub const MAXWELL_ACCURACY_OPT_IN: &str = "Show finite results labelled \"accuracy not met\"";

/// The plane wave's honest scope line, always visible when the section is open.
pub const WAVE_SCOPE_NOTE: &str = "Analytic full-wave illustration in normalized teaching units with c = 1; not SI, not a boundary-value solution, and not a solver for arbitrary sources. An enabled wave replaces the selected case's field arrows while the selected case's outline geometry remains drawn.";
/// The normalized-unit note attached to the analytic equation statuses.
pub const WAVE_NORMALIZED_NOTE: &str =
    "Normalized teaching units: c = 1, so |E| = |B|; these are not SI measurements.";
/// The equation statuses are sampled at the current clock time, not statically.
pub const WAVE_STATUS_CLOCK_NOTE: &str =
    "Equation statuses are evaluated at the current clock time.";
/// The Ampere-Maxwell limit: source-free means no modelled free current.
pub const WAVE_AMPERE_NOTE: &str = "Ampere-Maxwell is unsupported for a source-free wave: no free-current density j is modelled, and no zero is substituted.";
/// Capture honesty: the GPU path includes wave arrows, the SVG blueprint does not.
pub const WAVE_SVG_NOTE: &str = "PNG and GUI screenshots include the wave arrows through the GPU scene; the SVG blueprint carries no arrows today.";
/// The existing animation control is disabled in Maxwell, so the panel must not
/// imply that a wave animation can be exported.
pub const WAVE_ANIMATION_NOTE: &str = "Animation export is unavailable in Maxwell mode, so the wave arrows cannot be exported as video or GIF.";

/// A one-shot request from the panel to the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaxwellPanelAction {
    /// Frame the whole domain through the shared view-command seam.
    Fit,
    /// Restore the default Maxwell scene.
    Reset,
    /// Switch the projection to 2D without changing mode or case.
    SwitchTo2d,
    /// Open (or reuse) the probe measurement the selected law form checks.
    Measurement(MeasurementRequest),
    /// Load the hollow sphere and activate the guided Gauss-law investigation.
    InvestigateGaussShell,
    /// Load the hollow sphere without forcing the guided investigation.
    FreeExploreGaussShell,
}

/// Panel-local state: the draft scene, the last rejected edit and the Advanced
/// disclosure state.
#[derive(Debug, Clone, Default)]
pub struct MaxwellPanelState {
    pub draft: MaxwellScene,
    pub error: Option<String>,
    pub advanced: bool,
    /// The plane-wave draft. It commits through [`State::set_maxwell_wave`];
    /// `None` disables the wave and restores the normal case sampling.
    pub wave: Option<WaveSpec>,
    /// The last enabled spec, kept as the re-enable seed so unchecking and
    /// re-checking "Enable plane wave" does not discard the user's values.
    pub last_wave: Option<WaveSpec>,
    /// The last rejected wave edit, with the slice A message verbatim.
    pub wave_error: Option<String>,
    /// Plane-wave disclosure state, kept in the panel like Advanced.
    pub wave_open: bool,
    /// Session-only learning/residual section; it never enters persistence.
    pub learning: MaxwellLearningPanel,
    /// Session-only guided Gauss-law investigation; it never enters
    /// persistence and never adds a durable probe surface.
    pub gauss: GaussShellState,
}

impl MaxwellPanelState {
    /// Validate and hand the draft to the viewmodel. A rejected edit keeps the
    /// committed scene and the draft text, and records the actionable error.
    pub fn commit(&mut self, state: &mut State) -> bool {
        state.set_maxwell_scene(self.draft.clone());
        if state.maxwell_scene() == &self.draft {
            self.draft = state.maxwell.clone();
            self.error = None;
            true
        } else {
            self.error = state
                .maxwell_error
                .clone()
                .or_else(|| Some("Invalid Maxwell scene.".to_owned()));
            false
        }
    }

    /// Validate and hand the wave draft to the viewmodel. A rejected edit keeps
    /// the committed wave and sampling and records the slice A message.
    pub fn commit_wave(&mut self, state: &mut State) -> bool {
        state.set_maxwell_wave(self.wave);
        // Validation repairs non-unit direction/polarization vectors, so the
        // committed spec can differ from the draft even when accepted. The
        // setter records a rejection explicitly; no error means accepted.
        match state.maxwell_wave_error.clone() {
            Some(error) => {
                self.wave_error = Some(error);
                false
            }
            None => {
                self.wave = state.maxwell_wave();
                self.wave_error = None;
                true
            }
        }
    }

    /// Enable or disable the wave draft without losing the last enabled spec.
    /// Disabling remembers the current values; re-enabling restores them
    /// instead of falling back to [`default_wave`].
    pub fn set_wave_enabled(&mut self, enabled: bool) {
        if enabled {
            if self.wave.is_none() {
                self.wave = Some(self.last_wave.unwrap_or_else(default_wave));
            }
        } else if let Some(spec) = self.wave.take() {
            self.last_wave = Some(spec);
        }
    }

    /// Refresh the draft from the committed scene. Called on mode entry and
    /// after Reset, never every frame, so a dirty draft is not overwritten.
    pub fn sync_from(&mut self, state: &State) {
        self.draft = state.maxwell.clone();
        self.error = None;
        self.wave = state.maxwell_wave();
        self.last_wave = self.wave;
        self.wave_error = None;
        // Mode entry, restore and Reset start from a new committed scene, so a
        // report from the previous scene must not survive. The equation and
        // probe selections are session preferences and stay.
        self.learning.clear_report();
        self.gauss.invalidate_report();
    }
}

/// Stable UI text for a case; the formula-bar summary uses the same names.
pub fn case_label(case: MaxwellCase) -> &'static str {
    match case {
        MaxwellCase::ParallelPlates => "Parallel plates",
        MaxwellCase::CoaxialElectric => "Coaxial electric",
        MaxwellCase::CoaxialMagnetic => "Coaxial magnetic",
        MaxwellCase::HollowSphere => "Hollow sphere",
        MaxwellCase::InfiniteWire => "Infinite wire",
        MaxwellCase::Solenoid => "Ideal solenoid",
        MaxwellCase::ImageSphere => "Image sphere",
        MaxwellCase::ImagePlane => "Image plane",
        MaxwellCase::Conductor2D => "Conductor2D",
    }
}

fn family_label(family: MaxwellFamily) -> &'static str {
    match family {
        MaxwellFamily::Electric => "E",
        MaxwellFamily::Magnetic => "B",
    }
}

fn accuracy_text(stop: MaxwellStop) -> &'static str {
    match stop {
        MaxwellStop::TargetMet => "target met",
        MaxwellStop::AccuracyNotMet => "accuracy not met",
        MaxwellStop::Unavailable => "unavailable",
    }
}

/// Draw the Maxwell panel. `compact` applies the same finger-sized row floor as
/// the other compact sheets. The caller applies the returned action after the
/// panel has finished painting.
pub fn show(
    ui: &mut egui::Ui,
    panel: &mut MaxwellPanelState,
    state: &mut State,
    compact: bool,
) -> Option<MaxwellPanelAction> {
    if compact {
        ui.spacing_mut().interact_size.y = crate::ui::layout::COMPACT_ROW_HEIGHT;
        ui.spacing_mut().item_spacing.y = crate::ui::layout::COMPACT_ITEM_SPACING;
    }

    let mut commit_now = false;
    let mut wave_commit_now = false;
    let mut action = None;

    ui.heading("Maxwell");

    // A rejected draft error stays actionable until the next successful commit
    // or Reset; it is never replaced by a warning from another scene.
    if let Some(error) = panel.error.as_ref() {
        ui.colored_label(ui.visuals().error_fg_color, error);
    }

    ui.label("Case");
    ui.horizontal_wrapped(|ui| {
        for case in MaxwellCase::ALL {
            if ui
                .selectable_label(panel.draft.case == case, case_label(case))
                .clicked()
                && panel.draft.case != case
            {
                panel.draft.case = case;
                commit_now = true;
            }
        }
    });

    let family = maxwell_family(panel.draft.case);
    ui.label(format!("Family: {} (display)", family_label(family)));

    crate::ui::object_style_control::show(ui, state);
    if state.object_style() == crate::object_scene::ObjectStyle::Solid
        && let Some(note) =
            crate::object_scene::maxwell_style_note(&panel.draft, state.plane_frame().as_ref())
    {
        ui.label(note);
    }

    let gauss_family = match maxwell_family(state.maxwell.case) {
        MaxwellFamily::Electric => FieldFamily::Electric,
        MaxwellFamily::Magnetic => FieldFamily::Magnetic,
    };
    let gauss_action = crate::ui::gauss_shell::show(
        ui,
        &mut panel.gauss,
        &state.maxwell,
        gauss_family,
        f64::from(state.time),
    );

    ui.label("Geometry");
    match panel.draft.case {
        MaxwellCase::ParallelPlates => {
            let (_, release) = drag_edit(ui, &mut panel.draft.separation, "Separation", 0.0, 0.05);
            commit_now |= release;
        }
        MaxwellCase::CoaxialElectric | MaxwellCase::CoaxialMagnetic => {
            let (_, release) = drag_edit(ui, &mut panel.draft.radius, "Inner radius", 0.0, 0.05);
            commit_now |= release;
            let (_, release) =
                drag_edit(ui, &mut panel.draft.outer_radius, "Outer radius", 0.0, 0.05);
            commit_now |= release;
        }
        MaxwellCase::HollowSphere
        | MaxwellCase::InfiniteWire
        | MaxwellCase::Solenoid
        | MaxwellCase::ImageSphere => {
            let (_, release) = drag_edit(ui, &mut panel.draft.radius, "Radius", 0.0, 0.05);
            commit_now |= release;
        }
        MaxwellCase::ImagePlane => {
            let (_, release) = drag_edit(
                ui,
                &mut panel.draft.charge,
                "Charge q (display)",
                f64::NEG_INFINITY,
                0.05,
            );
            commit_now |= release;
            let (_, release) = drag_edit(
                ui,
                &mut panel.draft.charge_height,
                "Charge height",
                0.0,
                0.05,
            );
            commit_now |= release;
        }
        MaxwellCase::Conductor2D => {
            ui.horizontal_wrapped(|ui| {
                ui.label("Outline");
                let circle = matches!(panel.draft.outline, ConductorOutline::Circle { .. });
                if ui.selectable_label(circle, "Circle").clicked() && !circle {
                    panel.draft.outline = ConductorOutline::Circle { radius: 1.0 };
                    commit_now = true;
                }
                if ui.selectable_label(!circle, "Square").clicked() && circle {
                    panel.draft.outline = ConductorOutline::Square { half_extent: 1.0 };
                    commit_now = true;
                }
            });
            match &mut panel.draft.outline {
                ConductorOutline::Circle { radius } => {
                    let (_, release) = drag_edit(ui, radius, "Radius", 0.0, 0.05);
                    commit_now |= release;
                }
                ConductorOutline::Square { half_extent } => {
                    let (_, release) = drag_edit(ui, half_extent, "Half-extent", 0.0, 0.05);
                    commit_now |= release;
                }
            }
            commit_now |= elements_edit(ui, &mut panel.draft.elements);
            let (_, release) = drag_edit(
                ui,
                &mut panel.draft.applied[0],
                "Applied x",
                f64::NEG_INFINITY,
                0.05,
            );
            commit_now |= release;
            let (_, release) = drag_edit(
                ui,
                &mut panel.draft.applied[1],
                "Applied y",
                f64::NEG_INFINITY,
                0.05,
            );
            commit_now |= release;
        }
    }

    ui.label("Display amplitudes (normalized teaching values, not SI)");
    let (_, release) = drag_edit(
        ui,
        &mut panel.draft.e0,
        "E0 (display)",
        f64::NEG_INFINITY,
        0.05,
    );
    commit_now |= release;
    let (_, release) = drag_edit(
        ui,
        &mut panel.draft.b0,
        "B0 (display)",
        f64::NEG_INFINITY,
        0.05,
    );
    commit_now |= release;

    show_wave_section(ui, panel, state, &mut wave_commit_now);

    // Collapsed by default: the teaching/residual section is one bounded press
    // away and never samples the field on its own. It reads the committed
    // scene, not the draft, so a rejected edit cannot relabel the report.
    let learning = &mut panel.learning;
    let committed = &state.maxwell;
    let bem = state.maxwell_bem_solution();
    let mut measurement = None;
    egui::CollapsingHeader::new("Maxwell equations")
        .default_open(false)
        .show(ui, |ui| {
            measurement =
                learning.show_with_wave(ui, committed, state.maxwell_wave().as_ref(), bem);
        });

    ui.label("View: Field");

    ui.horizontal_wrapped(|ui| {
        if ui
            .button("Fit")
            .on_hover_text("Frame the durable domain or the selected plane (F)")
            .clicked()
        {
            action = Some(MaxwellPanelAction::Fit);
        }
        if ui
            .button("Reset Maxwell section")
            .on_hover_text("Restore the default Maxwell scene")
            .clicked()
        {
            panel.draft = MaxwellScene::default();
            panel.error = None;
            panel.wave = None;
            panel.last_wave = None;
            panel.wave_error = None;
            panel.gauss.reset();
            action = Some(MaxwellPanelAction::Reset);
        }
    });

    if state.mode_3d && panel.draft.case == MaxwellCase::Conductor2D {
        ui.label(MAXWELL_CONDUCTOR_NOTE);
        if ui
            .button("Switch to 2D")
            .on_hover_text("Move to the 2D plane where the conductor cross-section is defined")
            .clicked()
        {
            state.set_mode(false);
            state.refresh_maxwell_sampling();
            action = Some(MaxwellPanelAction::SwitchTo2d);
        }
    } else if panel.draft.case == MaxwellCase::Conductor2D {
        ui.label(MAXWELL_CONDUCTOR_NOTE);
    }

    ui.label(MAXWELL_ONLY_NOTE);

    let mut advanced = panel.advanced;
    let response = egui::CollapsingHeader::new("Advanced")
        .open(Some(advanced))
        .show(ui, |ui| {
            show_advanced(ui, panel, state, &mut commit_now);
        });
    if response.header_response.clicked() {
        advanced = !advanced;
    }
    panel.advanced = advanced;

    if let Some(request) = measurement {
        action = Some(MaxwellPanelAction::Measurement(request));
    }
    if let Some(gauss) = gauss_action {
        action = Some(match gauss {
            GaussShellAction::Investigate => MaxwellPanelAction::InvestigateGaussShell,
            GaussShellAction::FreeExplore => MaxwellPanelAction::FreeExploreGaussShell,
        });
    }
    if commit_now && action.is_none() {
        panel.commit(state);
    }
    if wave_commit_now && action.is_none() {
        panel.commit_wave(state);
    }
    action
}

/// The collapsed plane-wave controls and the analytic equation statuses. The
/// statuses describe the committed wave; a draft that has not committed yet
/// never claims to be the drawn field.
fn show_wave_section(
    ui: &mut egui::Ui,
    panel: &mut MaxwellPanelState,
    state: &State,
    commit_now: &mut bool,
) {
    let mut wave = panel.wave;
    let mut changed = false;
    let committed = state.maxwell_wave();
    let time = f64::from(state.time);
    let error = panel.wave_error.clone();
    let mut open = panel.wave_open;

    let response = egui::CollapsingHeader::new("Plane wave")
        .open(Some(open))
        .show(ui, |ui| {
            ui.label(WAVE_SCOPE_NOTE);
            if let Some(error) = error.as_ref() {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }

            let mut enabled = wave.is_some();
            if ui.checkbox(&mut enabled, "Enable plane wave").changed() {
                panel.set_wave_enabled(enabled);
                wave = panel.wave;
                changed = true;
            }

            if let Some(spec) = wave.as_mut() {
                let (_, release) =
                    drag_edit(ui, &mut spec.amplitude, "Amplitude", -MAX_AMPLITUDE, 0.05);
                changed |= release;
                let (_, release) =
                    drag_edit(ui, &mut spec.wavelength, "Wavelength", MIN_WAVELENGTH, 0.05);
                changed |= release;
                let (_, release) =
                    drag_edit(ui, &mut spec.phase, "Phase (rad)", f64::NEG_INFINITY, 0.05);
                changed |= release;

                changed |= axis_selector(ui, "Propagation", &mut spec.direction, true);
                changed |= axis_selector(ui, "Polarization", &mut spec.polarization, false);
                ui.label("Polarization is projected perpendicular to propagation by validation.");
            }

            if let Some(spec) = wave.as_ref().or(committed.as_ref()) {
                ui.separator();
                ui.label(WAVE_NORMALIZED_NOTE);
                ui.label(WAVE_STATUS_CLOCK_NOTE);
                match equation_terms(spec, Vec3::ZERO, time) {
                    Some(terms) => {
                        for equation in MaxwellEquation::ALL {
                            let check = residual(equation, &terms);
                            ui.label(format!(
                                "{}: {}",
                                equation.title(),
                                term_status_label(&check.status)
                            ));
                        }
                    }
                    None => {
                        ui.label("Equation statuses unavailable for the current wave values.");
                    }
                }
                ui.label(WAVE_AMPERE_NOTE);
            }
            ui.label(WAVE_SVG_NOTE);
            ui.label(WAVE_ANIMATION_NOTE);
        });
    if response.header_response.clicked() {
        open = !open;
    }
    panel.wave_open = open;
    panel.wave = wave;
    if changed {
        *commit_now = true;
    }
}

fn default_wave() -> WaveSpec {
    WaveSpec {
        amplitude: 1.0,
        wavelength: 2.0,
        direction: Vec3::X,
        polarization: Vec3::Y,
        phase: 0.0,
    }
}

fn term_status_label(status: &TermStatus) -> String {
    match status {
        TermStatus::Analytic => "Analytic".to_owned(),
        TermStatus::Numerical { error } => format!("Numerical +/- {error:.2e}"),
        TermStatus::Unsupported(symbol) => format!("Unsupported ({symbol})"),
        TermStatus::Singular(symbol) => format!("Singular ({symbol})"),
    }
}

/// Edit a direction/polarization axis with X/Y/Z selections and, for
/// propagation, an explicit sign. Returns whether a button changed the value.
fn axis_selector(ui: &mut egui::Ui, label: &str, value: &mut Vec3, signed: bool) -> bool {
    let mut changed = false;
    let selected = axis_index(*value);
    let negative = selected.is_some_and(|axis| value[axis] < 0.0);
    ui.horizontal_wrapped(|ui| {
        ui.label(label);
        for (index, name) in [(0, "X"), (1, "Y"), (2, "Z")] {
            if ui.selectable_label(selected == Some(index), name).clicked()
                && selected != Some(index)
            {
                *value = unit_axis(index, negative);
                changed = true;
            }
        }
        if signed {
            ui.label("Sign");
            if ui.selectable_label(!negative, "+").clicked() && negative {
                let axis = selected.unwrap_or(0);
                *value = unit_axis(axis, false);
                changed = true;
            }
            if ui.selectable_label(negative, "-").clicked() && !negative {
                let axis = selected.unwrap_or(0);
                *value = unit_axis(axis, true);
                changed = true;
            }
        }
    });
    changed
}

fn axis_index(value: Vec3) -> Option<usize> {
    if !value.is_finite() || value.length() < 1e-6 {
        return None;
    }
    let absolute = value.abs();
    let mut best = 0;
    if absolute.y > absolute[best] {
        best = 1;
    }
    if absolute.z > absolute[best] {
        best = 2;
    }
    Some(best)
}

fn unit_axis(index: usize, negative: bool) -> Vec3 {
    let sign = if negative { -1.0 } else { 1.0 };
    match index {
        0 => Vec3::X * sign,
        1 => Vec3::Y * sign,
        _ => Vec3::Z * sign,
    }
}

fn drag_edit(
    ui: &mut egui::Ui,
    value: &mut f64,
    label: &str,
    minimum: f64,
    speed: f64,
) -> (bool, bool) {
    let response = ui
        .horizontal(|ui| {
            ui.label(label);
            ui.add(
                egui::DragValue::new(value)
                    .speed(speed)
                    .range(minimum..=f64::INFINITY),
            )
        })
        .inner;
    let changed = response.changed();
    // Ordinary drag release and a text edit that loses focus commit once; a
    // frame never commits merely because a value exists.
    let release = response.drag_stopped()
        || response.lost_focus()
        || (changed && ui.input(|input| input.key_pressed(egui::Key::Enter)));
    (changed, release)
}

fn elements_edit(ui: &mut egui::Ui, elements: &mut u32) -> bool {
    let response = ui
        .horizontal(|ui| {
            ui.label("Elements");
            ui.add(egui::DragValue::new(elements).speed(2.0).range(
                crate::maxwell::MAXWELL_BEM_MIN_ELEMENTS..=crate::maxwell::MAXWELL_MAX_BEM_ELEMENTS,
            ))
            .on_hover_text("Fine BEM panel count; the coarse run uses half")
        })
        .inner;
    response.drag_stopped()
        || response.lost_focus()
        || (response.changed() && ui.input(|input| input.key_pressed(egui::Key::Enter)))
}

fn show_advanced(
    ui: &mut egui::Ui,
    panel: &mut MaxwellPanelState,
    state: &mut State,
    commit_now: &mut bool,
) {
    ui.label("Assumptions");
    ui.label(maxwell_assumptions(&panel.draft));
    if let Some(warning) = maxwell_warning(&panel.draft) {
        ui.label(warning);
    }

    let sampling = state.maxwell_sampling.as_ref();
    let mut accuracy = None;
    match sampling {
        Some(sampling) => {
            accuracy = Some(sampling.accuracy);
            ui.label(format!("Status: {}", accuracy_text(sampling.accuracy)));
            if let Some(warning) = &sampling.warning {
                ui.colored_label(ui.visuals().warn_fg_color, warning);
            }
            ui.label(format!(
                "Omitted samples: {} · omitted as large: {}",
                sampling.omitted, sampling.omitted_large
            ));
            if let Some(convergence) = sampling.convergence {
                ui.label(format!(
                    "Coarse elements: {} · Fine elements: {}",
                    convergence.coarse_elements, convergence.fine_elements
                ));
                ui.label(format!(
                    "Relative difference: {:.3e} · Target: {:.0e}",
                    convergence.relative_difference, convergence.target
                ));
            }
        }
        None => {
            ui.label("Status: no Maxwell results sampled yet");
            if let Some(error) = &state.maxwell_error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
        }
    }

    // The opt-in is enabled only for a finite AccuracyNotMet result: a target
    // met has nothing to reveal and an unavailable result must never be drawn.
    let opt_in_enabled = accuracy == Some(MaxwellStop::AccuracyNotMet);
    let mut allow = panel.draft.allow_inaccurate;
    let response = ui
        .add_enabled(
            opt_in_enabled,
            egui::Checkbox::new(&mut allow, MAXWELL_ACCURACY_OPT_IN),
        )
        .on_disabled_hover_text("Only a finite result labelled accuracy not met can be shown");
    if response.changed() {
        panel.draft.allow_inaccurate = allow;
        *commit_now = true;
    }

    ui.label("Elements are the fine BEM panel count; the coarse run uses half.");
}

/// Maxwell-mode help rows, shared by the Help window and tests.
pub static MAXWELL_HELP_ROWS: &[HelpRow] = &[
    HelpRow {
        input: "canonical cases",
        action: "ideal infinite plates, cylinders, a charged shell, a surface-current wire and a solenoid, plus image sphere/plane; closed-form normalized display amplitudes",
    },
    HelpRow {
        input: "Units",
        action: "E0/B0/q are display amplitudes for idealized normalized teaching fields; there is no SI constant engine",
    },
    HelpRow {
        input: "Infinite wire",
        action: "InfiniteWire is an ideal thin surface-current shell, not a general current-carrying wire",
    },
    HelpRow {
        input: "Conductor2D",
        action: "an infinite-cylinder cross-section solved by a bounded BEM approximation; not a 3D finite object",
    },
    HelpRow {
        input: "Scope",
        action: "bounded ideal teaching cases; no general solver and no arbitrary geometry",
    },
    HelpRow {
        input: "Accuracy",
        action: "finite results that miss the target are hidden by default and labelled accuracy not met when shown",
    },
    HelpRow {
        input: "Arrows only",
        action: "streamlines, particles and click seeding stay off in Maxwell",
    },
];

/// The Maxwell help rows, exposed for the Help window and tests.
pub fn maxwell_help_rows() -> &'static [HelpRow] {
    MAXWELL_HELP_ROWS
}

/// One-line summary used by the formula bar. Kept here so the selector and the
/// panel cannot disagree about the selected case or display amplitudes.
pub fn summary(state: &State) -> String {
    let scene = &state.maxwell;
    format!(
        "{} · {} display · E0 {:.2} · B0 {:.2}",
        case_label(scene.case),
        family_label(maxwell_family(scene.case)),
        scene.e0,
        scene.b0
    )
}
