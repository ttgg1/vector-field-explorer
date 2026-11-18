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

use crate::maxwell::{
    ConductorOutline, MaxwellCase, MaxwellFamily, MaxwellScene, MaxwellStop, maxwell_assumptions,
    maxwell_family, maxwell_warning,
};
use crate::ui::help_overlay::HelpRow;
use crate::viewmodel::State;

/// The exact arrows-only note shared by the panel and the Features section.
pub const MAXWELL_ONLY_NOTE: &str =
    "Arrows only: streamlines, particles and click seeding stay off in Maxwell.";

/// Exact 2D-conductor applicability note.
pub const MAXWELL_CONDUCTOR_NOTE: &str =
    "Conductor2D is an infinite-cylinder cross-section, not a 3D finite object.";

/// The Advanced accuracy opt-in label, shared by the panel and tests.
pub const MAXWELL_ACCURACY_OPT_IN: &str = "Show finite results labelled \"accuracy not met\"";

/// A one-shot request from the panel to the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaxwellPanelAction {
    /// Frame the whole domain through the shared view-command seam.
    Fit,
    /// Restore the default Maxwell scene.
    Reset,
    /// Switch the projection to 2D without changing mode or case.
    SwitchTo2d,
}

/// Panel-local state: the draft scene, the last rejected edit and the Advanced
/// disclosure state.
#[derive(Debug, Clone, Default)]
pub struct MaxwellPanelState {
    pub draft: MaxwellScene,
    pub error: Option<String>,
    pub advanced: bool,
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

    /// Refresh the draft from the committed scene. Called on mode entry and
    /// after Reset, never every frame, so a dirty draft is not overwritten.
    pub fn sync_from(&mut self, state: &State) {
        self.draft = state.maxwell.clone();
        self.error = None;
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

    if commit_now && action.is_none() {
        panel.commit(state);
    }
    action
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
