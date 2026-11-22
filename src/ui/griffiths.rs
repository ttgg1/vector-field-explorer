//! Issue #223 phase 1: the optional Griffiths onboarding branch.
//!
//! The branch is deliberately separate from the ordered quick tour. It offers
//! four selectable activities from the welcome card and the Help window. Only
//! the charged-ring activity (#224) ships in this slice; the other routes stay
//! selectable but report a safe, clearly labelled placeholder without touching
//! the scene.

use eframe::egui;

use crate::ui::ring_lesson::RingLessonState;

/// The picker's window title and accessible label.
pub const GRIFFITHS_PICKER_TITLE: &str = "Try a Griffiths problem";
/// The ring route's control label, also used by the picker and tests.
pub const GRIFFITHS_RING_LABEL: &str =
    "Griffiths 2.5 · Charged ring: where is the axial field strongest?";

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
            Self::Shell => "Griffiths 2.11 · Charged shell (not available yet)",
            Self::Plane => "Griffiths §3.2 · Grounded plane (not available yet)",
            Self::Wire => "Griffiths 5.14(a) · Current-carrying wire (not available yet)",
        }
    }

    /// Whether this route has a shipped lesson in this build.
    pub const fn available(self) -> bool {
        matches!(self, Self::Ring)
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
                        action = GriffithsAction::OpenRing;
                    } else {
                        action = GriffithsAction::Placeholder(route);
                    }
                }
                ui.small(route.locator());
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
