//! Issue #223 phase 1: the optional Griffiths onboarding branch.
//!
//! The branch is deliberately separate from the ordered quick tour. It offers
//! four selectable activities from the welcome card and the Help window. The
//! charged-ring (#224), charged-shell (#225), grounded-plane (#226) and
//! surface-current wire (#227) lessons all ship; the picker keeps its safe
//! placeholder path for any future unavailable route.

use eframe::egui;

use crate::ui::plane_lesson::PlaneLessonState;
use crate::ui::ring_lesson::RingLessonState;
use crate::ui::shell_lesson::ShellLessonState;
use crate::ui::wire_lesson::WireLessonState;

/// The picker's window title and accessible label.
pub const GRIFFITHS_PICKER_TITLE: &str = "Try a Griffiths problem";
/// The ring route's control label, also used by the picker and tests.
pub const GRIFFITHS_RING_LABEL: &str =
    "Griffiths 2.5 · Charged ring: where is the axial field strongest?";
/// The shell route's control label, also used by the picker and tests.
pub const GRIFFITHS_SHELL_LABEL: &str =
    "Griffiths 2.11 · Charged shell: field and flux across the shell";
/// The grounded-plane route's control label, also used by the picker and tests.
pub const GRIFFITHS_PLANE_LABEL: &str =
    "Griffiths §3.2 · Grounded plane: what image charge makes V = 0?";
/// The wire route's control label, also used by the picker and tests.
pub const GRIFFITHS_WIRE_LABEL: &str =
    "Griffiths 5.14(a) · Current-carrying wire: B and circulation";
/// The picker summary kept after a completed shell activity.
pub const GRIFFITHS_SHELL_COMPLETED_NOTE: &str = "Charged shell activity completed this session; choose it again to replay with a fresh prediction.";
/// Reveal the activity's existing contextual hint on request.
pub const GRIFFITHS_SHOW_HINT: &str = "Show hint";
/// Hide the contextual hint again.
pub const GRIFFITHS_HIDE_HINT: &str = "Hide hint";
/// Advance exactly one pre-reveal step without revealing the result.
pub const GRIFFITHS_SKIP_STEP: &str = "Skip this step";
/// Temporarily leave the activity at its current checkpoint.
pub const GRIFFITHS_EXPLORE_FROM_HERE: &str = "Explore from here";
/// Return to the pending resume/keep-variation choice.
pub const GRIFFITHS_RETURN_TO_ACTIVITY: &str = "Return to activity";
/// Restore the scene and lesson state saved at the checkpoint.
pub const GRIFFITHS_RESUME_CHECKPOINT: &str = "Resume at checkpoint";
/// Keep the learner's current scene and lesson edits.
pub const GRIFFITHS_KEEP_VARIATION: &str = "Keep my variation";

/// The meaningful step of the active activity. Everything here is derived from
/// the lesson's session state, so it cannot drift from the prediction/reveal
/// gate the learner sees.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GriffithsCheckpoint {
    /// Choosing a prediction; nothing has been revealed.
    Predict,
    /// A prediction or an explicit skip is recorded; the result awaits reveal.
    Reveal,
    /// The solution is revealed and being read.
    Result,
}

impl GriffithsCheckpoint {
    /// The visible checkpoint text. It names the step, not only a highlight.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Predict => "Checkpoint 1 of 3 · Make your prediction",
            Self::Reveal => "Checkpoint 2 of 3 · Reveal the result",
            Self::Result => "Checkpoint 3 of 3 · Read the result",
        }
    }
}

/// The active route and checkpoint, if an activity is open.
pub fn active_checkpoint(state: &GriffithsState) -> Option<(GriffithsRoute, GriffithsCheckpoint)> {
    let route = if state.lesson.active() {
        GriffithsRoute::Ring
    } else if state.shell.active() {
        GriffithsRoute::Shell
    } else if state.plane.active() {
        GriffithsRoute::Plane
    } else if state.wire.active() {
        GriffithsRoute::Wire
    } else {
        return None;
    };
    Some((route, checkpoint_for(route, state)))
}

/// The checkpoint for one route, or `Predict` when it is not active.
pub fn checkpoint_for(route: GriffithsRoute, state: &GriffithsState) -> GriffithsCheckpoint {
    match route {
        GriffithsRoute::Ring => state.lesson.checkpoint(),
        GriffithsRoute::Shell => state.shell.checkpoint(),
        GriffithsRoute::Plane => state.plane.checkpoint(),
        GriffithsRoute::Wire => state.wire.checkpoint(),
    }
}

/// One optional activity in the Griffiths branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GriffithsRoute {
    /// Griffiths 2.5, the charged ring's axial field (#224).
    Ring,
    /// Griffiths 2.11, field and flux across a charged shell (#225).
    Shell,
    /// Griffiths §3.2, the grounded-plane image charge (#226).
    Plane,
    /// Griffiths 5.14(a), circulation in a surface-current wire (#227).
    Wire,
}

impl GriffithsRoute {
    /// Every route in picker order.
    pub const ALL: [Self; 4] = [Self::Ring, Self::Shell, Self::Plane, Self::Wire];

    /// The stable route key, safe for tests and future routing.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Ring => "ring",
            Self::Shell => "shell",
            Self::Plane => "plane",
            Self::Wire => "wire",
        }
    }

    /// The edition and locator shown with the activity.
    pub const fn locator(self) -> &'static str {
        match self {
            Self::Ring => "Griffiths, Introduction to Electrodynamics, 4th ed., Problem 2.5",
            Self::Shell => "Griffiths, Introduction to Electrodynamics, 4th ed., Problem 2.11",
            Self::Plane => "Griffiths, Introduction to Electrodynamics, 4th ed., §3.2",
            Self::Wire => "Griffiths, Introduction to Electrodynamics, 4th ed., Problem 5.14(a)",
        }
    }

    /// The picker label. Unimplemented routes say so on the button itself.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Ring => GRIFFITHS_RING_LABEL,
            Self::Shell => GRIFFITHS_SHELL_LABEL,
            Self::Plane => GRIFFITHS_PLANE_LABEL,
            Self::Wire => GRIFFITHS_WIRE_LABEL,
        }
    }

    /// Whether this route has a shipped lesson in this build.
    pub const fn available(self) -> bool {
        matches!(self, Self::Ring | Self::Shell | Self::Plane | Self::Wire)
    }
}

/// Session-only branch state. The app owns the scene/camera snapshot.
#[derive(Debug, Clone, Default)]
pub struct GriffithsState {
    /// Whether the four-route picker is up.
    pub picker_open: bool,
    /// An unavailable route the learner selected; the picker explains it.
    pub placeholder: Option<GriffithsRoute>,
    /// The ring activity's session state.
    pub lesson: RingLessonState,
    /// The shell activity's session state and completion flag.
    pub shell: ShellLessonState,
    /// Whether the shell activity was completed in this branch session. This
    /// survives returning to the picker or Help and is cleared by the next
    /// shell start so a replay begins uncompleted.
    pub shell_completed: bool,
    /// The grounded-plane activity's session state.
    pub plane: PlaneLessonState,
    /// The wire activity's session state.
    pub wire: WireLessonState,
}

impl GriffithsState {
    /// Open the route picker over the current scene.
    pub fn open(&mut self) {
        self.picker_open = true;
        self.placeholder = None;
    }

    /// Close the picker without selecting a route.
    pub fn close(&mut self) {
        self.picker_open = false;
        self.placeholder = None;
    }

    /// Clear an unavailable-route message while leaving the picker reachable.
    pub fn close_placeholder(&mut self) -> bool {
        self.placeholder.take().is_some()
    }
}

/// What the app should do after a picker interaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GriffithsAction {
    None,
    /// Open the charged-ring lesson.
    OpenRing,
    /// Open the charged-shell lesson.
    OpenShell,
    /// Open the grounded-plane image-charge lesson.
    OpenPlane,
    /// Open the surface-current wire lesson.
    OpenWire,
    /// A route without a shipped lesson was selected.
    Placeholder(GriffithsRoute),
    /// Close the picker and return to the current scene.
    Dismiss,
}

/// Draw the four-route picker. The app owns the scene/camera snapshot and the
/// actual route transition.
pub fn show_picker(ctx: &egui::Context, state: &mut GriffithsState) -> GriffithsAction {
    if !state.picker_open {
        return GriffithsAction::None;
    }
    let width = (ctx.content_rect().width() - 32.0).clamp(300.0, 460.0);
    let mut action = GriffithsAction::None;
    egui::Window::new(GRIFFITHS_PICKER_TITLE)
        .collapsible(false)
        .resizable(false)
        .default_width(width)
        .max_width(width)
        .show(ctx, |ui| {
            ui.label(
                "Optional practice from Griffiths, Introduction to Electrodynamics (4th ed.). \
                 The quick interface tour stays available from Help > Start or restart tour.",
            );
            ui.add_space(6.0);
            for route in GriffithsRoute::ALL {
                let label = route.label();
                let response = ui.add_sized(
                    [ui.available_width(), 26.0],
                    egui::Button::new(label).wrap(),
                );
                if response.clicked() {
                    if route.available() {
                        action = match route {
                            GriffithsRoute::Shell => GriffithsAction::OpenShell,
                            GriffithsRoute::Plane => GriffithsAction::OpenPlane,
                            GriffithsRoute::Wire => GriffithsAction::OpenWire,
                            _ => GriffithsAction::OpenRing,
                        };
                    } else {
                        action = GriffithsAction::Placeholder(route);
                    }
                }
                ui.small(route.locator());
                if route == GriffithsRoute::Shell && state.shell_completed {
                    ui.small(GRIFFITHS_SHELL_COMPLETED_NOTE);
                }
                ui.add_space(2.0);
            }
            if let Some(route) = state.placeholder {
                ui.separator();
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    format!(
                        "The {} lesson is not available yet. Your scene is unchanged; \
                         close this message to keep exploring or choose another route.",
                        route.label()
                    ),
                );
                if ui.button("Close message").clicked() {
                    state.close_placeholder();
                }
            }
            ui.separator();
            if ui.button("Close").clicked() {
                action = GriffithsAction::Dismiss;
            }
        });
    action
}

/// What the app should do after an exploration overlay interaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GriffithsExplorationAction {
    None,
    /// Ask the learner whether to resume or keep the variation.
    Return,
    /// Restore the snapshot saved at the checkpoint.
    Resume,
    /// Keep the current scene and lesson edits.
    KeepVariation,
}

/// Draw the temporary-exploration banner, or the resume/keep choice once the
/// learner returns. The app owns the snapshot and lesson state.
pub fn show_exploration(
    ctx: &egui::Context,
    route: GriffithsRoute,
    checkpoint: GriffithsCheckpoint,
    return_prompt: bool,
) -> GriffithsExplorationAction {
    let width = (ctx.content_rect().width() - 32.0).clamp(300.0, 460.0);
    let mut action = GriffithsExplorationAction::None;
    if return_prompt {
        egui::Window::new("Return to the activity?")
            .order(egui::Order::Foreground)
            .collapsible(false)
            .resizable(false)
            .default_width(width)
            .max_width(width)
            .show(ctx, |ui| {
                ui.label(format!("Exploring from {}.", route.label()));
                ui.strong(checkpoint.label());
                ui.add_space(4.0);
                ui.label(
                    "Resume at checkpoint restores the scene, camera and lesson state \
                     saved when you chose Explore from here.",
                );
                ui.label(
                    "Keep my variation keeps your current scene and lesson edits and \
                     discards only the saved checkpoint.",
                );
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .button(GRIFFITHS_RESUME_CHECKPOINT)
                        .on_hover_text("Restore the scene and lesson saved at this checkpoint")
                        .clicked()
                    {
                        action = GriffithsExplorationAction::Resume;
                    }
                    if ui
                        .button(GRIFFITHS_KEEP_VARIATION)
                        .on_hover_text("Keep your current scene and lesson edits")
                        .clicked()
                    {
                        action = GriffithsExplorationAction::KeepVariation;
                    }
                });
            });
    } else {
        egui::Window::new("Exploring from checkpoint")
            .order(egui::Order::Foreground)
            .collapsible(false)
            .resizable(false)
            .default_width(width)
            .max_width(width)
            .show(ctx, |ui| {
                ui.label(format!("{} · {}", route.label(), checkpoint.label()));
                ui.small(
                    "The activity is paused at this checkpoint; the scene is yours to edit. \
                     Return when you want to resume or keep your variation.",
                );
                if ui.button(GRIFFITHS_RETURN_TO_ACTIVITY).clicked() {
                    action = GriffithsExplorationAction::Return;
                }
            });
    }
    action
}
