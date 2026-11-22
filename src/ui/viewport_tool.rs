//! Issue #228: the viewport-edge click tool.
//!
//! The strip at the viewport edge names what the next viewport click will do.
//! Everything it shows is derived from the real viewmodel state plus the
//! session-only arming choices in [`Controls`]; it never owns a second,
//! independent "selected tool" flag. A prerequisite that is off becomes an
//! explicit enable-and-place action instead of a selected tool that cannot act.
//!
//! Placement is either one-shot (one click, then Inspect) or sticky (the tool
//! stays selected across clicks). `Escape` always returns to Inspect and
//! cancels any armed placement. Tool buttons are ordinary focusable egui
//! buttons, so Tab/Shift+Tab and Enter/Space select them without repurposing a
//! user binding.

use eframe::egui;

use crate::interaction::{Bindings, InteractionAction};
use crate::source_scene::FieldMode;
use crate::ui::menu::chord_shortcut_label;
use crate::ui::sidebar::Controls;
use crate::view_command::{
    CLICK_HINT_PARTICLES_NEED_3D, CLICK_HINT_PARTICLES_OFF, CLICK_HINT_STREAMLINES_OFF,
};
use crate::viewmodel::{LeftClickAction, State};

/// Why both placement tools are inert outside a formula field: clicks never
/// seed or burst in the source, induction, materials, Maxwell or Ode modes.
pub const CLICK_HINT_NO_SEEDING: &str = "Clicks do not seed in this field mode";
/// Why the probe tool is inert: the Scalar view (which owns point probes) does
/// not apply.
pub const CLICK_HINT_PROBES_UNAVAILABLE: &str = "Probes are unavailable in this view";
/// Why every vector click tool is inert while a scalar plot is showing.
pub const CLICK_HINT_PLOT_MODE: &str = "Vector click tools are unavailable in plot mode";

/// One tool the viewport strip offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewportTool {
    /// Clicks do nothing placed-related.
    Inspect,
    /// The next click plants one scalar point probe.
    PlaceProbe,
    /// The next click plants one click streamline seed.
    SeedStreamline,
    /// The next click adds one particle burst.
    AddParticles,
}

impl ViewportTool {
    /// Every tool, in the order the strip shows them.
    pub const ALL: [ViewportTool; 4] = [
        ViewportTool::Inspect,
        ViewportTool::PlaceProbe,
        ViewportTool::SeedStreamline,
        ViewportTool::AddParticles,
    ];

    /// The stable button label.
    pub fn label(self) -> &'static str {
        match self {
            ViewportTool::Inspect => "Inspect",
            ViewportTool::PlaceProbe => "Place probe",
            ViewportTool::SeedStreamline => "Seed streamline",
            ViewportTool::AddParticles => "Add particles",
        }
    }

    /// The one-line explanation, also the button tooltip when ready.
    pub fn tooltip(self) -> &'static str {
        match self {
            ViewportTool::Inspect => {
                "The next viewport click places nothing; an existing seed can still be selected"
            }
            ViewportTool::PlaceProbe => "The next viewport click plants one point probe",
            ViewportTool::SeedStreamline => "The next viewport click plants one streamline seed",
            ViewportTool::AddParticles => "The next viewport click adds one particle burst",
        }
    }
}

/// Whether one tool can act right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolAvailability {
    /// The tool is ready to select.
    Ready,
    /// A prerequisite is off; selecting this entry enables it and arms one
    /// placement. The payload is the short reason.
    EnableThenPlace(&'static str),
    /// The tool cannot be used here; the payload is the specific reason.
    Disabled(&'static str),
}

impl ToolAvailability {
    /// Whether the entry accepts a click.
    pub fn is_interactive(self) -> bool {
        !matches!(self, ToolAvailability::Disabled(_))
    }

    /// The specific reason shown in the tooltip, when there is one.
    pub fn reason(self) -> Option<&'static str> {
        match self {
            ToolAvailability::Ready => None,
            ToolAvailability::EnableThenPlace(reason) | ToolAvailability::Disabled(reason) => {
                Some(reason)
            }
        }
    }
}

/// One stripped-down option in the derived tool table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolOption {
    pub tool: ViewportTool,
    pub availability: ToolAvailability,
}

/// The pure inputs to tool derivation. Tests build this directly; the app
/// derives it from [`State`] and [`Controls`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolInputs {
    /// The vector viewport is showing (not a scalar plot).
    pub vector_mode: bool,
    /// The field mode accepts click seeds and bursts (Formula).
    pub seedable_mode: bool,
    pub streamlines: bool,
    pub particles: bool,
    /// The active field has a third component, so particles have somewhere to
    /// ride (the 3D view or a 2D plane slice).
    pub particles_capable: bool,
    /// The Scalar view (which owns point probes) applies.
    pub probes_supported: bool,
    pub left_click_action: LeftClickAction,
    /// The Scalar section's armed "Place probe" toggle.
    pub scalar_place_probe: bool,
    /// The strip's explicit Inspect choice.
    pub inspect: bool,
    /// Whether a successful placement keeps the tool selected.
    pub sticky: bool,
}

/// The derived strip: the effective selected tool, every option, and the
/// placement mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolView {
    pub selected: ViewportTool,
    pub options: [ToolOption; 4],
    pub sticky: bool,
}

/// Derive the strip and the effective click tool from real state.
pub fn derive(input: ToolInputs) -> ToolView {
    let seed = if !input.vector_mode {
        ToolAvailability::Disabled(CLICK_HINT_PLOT_MODE)
    } else if !input.seedable_mode {
        ToolAvailability::Disabled(CLICK_HINT_NO_SEEDING)
    } else if input.streamlines {
        ToolAvailability::Ready
    } else {
        ToolAvailability::EnableThenPlace(CLICK_HINT_STREAMLINES_OFF)
    };
    let particles = if !input.vector_mode {
        ToolAvailability::Disabled(CLICK_HINT_PLOT_MODE)
    } else if !input.seedable_mode {
        ToolAvailability::Disabled(CLICK_HINT_NO_SEEDING)
    } else if !input.particles_capable {
        ToolAvailability::Disabled(CLICK_HINT_PARTICLES_NEED_3D)
    } else if input.particles {
        ToolAvailability::Ready
    } else {
        ToolAvailability::EnableThenPlace(CLICK_HINT_PARTICLES_OFF)
    };
    let probe = if !input.vector_mode {
        ToolAvailability::Disabled(CLICK_HINT_PLOT_MODE)
    } else if input.probes_supported {
        ToolAvailability::Ready
    } else {
        ToolAvailability::Disabled(CLICK_HINT_PROBES_UNAVAILABLE)
    };

    let options = [
        ToolOption {
            tool: ViewportTool::Inspect,
            availability: ToolAvailability::Ready,
        },
        ToolOption {
            tool: ViewportTool::PlaceProbe,
            availability: probe,
        },
        ToolOption {
            tool: ViewportTool::SeedStreamline,
            availability: seed,
        },
        ToolOption {
            tool: ViewportTool::AddParticles,
            availability: particles,
        },
    ];
    let availability = |tool: ViewportTool| {
        options
            .iter()
            .find(|option| option.tool == tool)
            .map(|option| option.availability)
            .unwrap_or(ToolAvailability::Disabled(CLICK_HINT_PLOT_MODE))
    };

    // An armed probe is the click that is really about to happen; otherwise an
    // explicit Inspect wins, and only a Ready action can be the effective
    // tool. This is what makes "Add particles selected while Particles is off"
    // impossible: the gated action falls back to Inspect while the strip still
    // offers the enable-and-place entry.
    let selected = if input.scalar_place_probe {
        // The armed probe is the click that is really queued. When its path is
        // gone the strip says Inspect rather than advertising the left-click
        // action that this click will not perform.
        if probe == ToolAvailability::Ready {
            ViewportTool::PlaceProbe
        } else {
            ViewportTool::Inspect
        }
    } else if input.inspect {
        ViewportTool::Inspect
    } else {
        match input.left_click_action {
            LeftClickAction::SeedStreamline
                if availability(ViewportTool::SeedStreamline) == ToolAvailability::Ready =>
            {
                ViewportTool::SeedStreamline
            }
            LeftClickAction::AddParticles
                if availability(ViewportTool::AddParticles) == ToolAvailability::Ready =>
            {
                ViewportTool::AddParticles
            }
            _ => ViewportTool::Inspect,
        }
    };

    ToolView {
        selected,
        options,
        sticky: input.sticky,
    }
}

/// Derive the strip from the live viewmodel and the session-only controls.
pub fn derive_for(state: &State, controls: &Controls) -> ToolView {
    derive(ToolInputs {
        vector_mode: state.is_vector_mode(),
        seedable_mode: state.field_mode == FieldMode::Formula,
        streamlines: state.streamlines,
        particles: state.particles,
        particles_capable: state.active_dimension() == 3,
        probes_supported: state.scalar_view_applicable(),
        left_click_action: state.left_click_action,
        scalar_place_probe: controls.scalar_place_probe,
        inspect: controls.viewport_inspect,
        sticky: controls.viewport_sticky,
    })
}

/// Apply one strip choice to the session controls. Enabling a prerequisite is
/// deliberately separate from placing: the next viewport click performs the
/// single armed placement, and a prerequisite that cannot be enabled changes
/// nothing.
pub fn activate(tool: ViewportTool, view: &ToolView, controls: &mut Controls) {
    let availability = view
        .options
        .iter()
        .find(|option| option.tool == tool)
        .map(|option| option.availability)
        .unwrap_or(ToolAvailability::Disabled(CLICK_HINT_PLOT_MODE));
    match tool {
        ViewportTool::Inspect => inspect(controls),
        ViewportTool::PlaceProbe => {
            if availability.is_interactive() {
                controls.viewport_inspect = false;
                controls.scalar_place_probe = true;
                controls.place_seed = false;
                controls.viewport_armed_one_shot = false;
            }
        }
        ViewportTool::SeedStreamline => match availability {
            ToolAvailability::Ready => {
                controls.viewport_inspect = false;
                controls.left_click_action = LeftClickAction::SeedStreamline;
                controls.scalar_place_probe = false;
                controls.viewport_armed_one_shot = false;
            }
            ToolAvailability::EnableThenPlace(_) => {
                controls.streamlines = true;
                controls.viewport_inspect = false;
                controls.left_click_action = LeftClickAction::SeedStreamline;
                controls.scalar_place_probe = false;
                controls.viewport_armed_one_shot = true;
            }
            ToolAvailability::Disabled(_) => {}
        },
        ViewportTool::AddParticles => match availability {
            ToolAvailability::Ready => {
                controls.viewport_inspect = false;
                controls.left_click_action = LeftClickAction::AddParticles;
                controls.scalar_place_probe = false;
                controls.viewport_armed_one_shot = false;
            }
            ToolAvailability::EnableThenPlace(_) => {
                controls.particles = true;
                controls.viewport_inspect = false;
                controls.left_click_action = LeftClickAction::AddParticles;
                controls.scalar_place_probe = false;
                controls.viewport_armed_one_shot = true;
            }
            ToolAvailability::Disabled(_) => {}
        },
    }
}

/// Return to Inspect and cancel every armed placement. Used by the strip's
/// Inspect entry and by Escape.
pub fn inspect(controls: &mut Controls) {
    controls.viewport_inspect = true;
    controls.scalar_place_probe = false;
    controls.place_seed = false;
    controls.viewport_armed_one_shot = false;
}

/// Whether one successful placement should return to Inspect. Sticky mode
/// keeps the tool; a one-shot choice or an enable-and-place arming returns to
/// Inspect and clears the arming in the caller.
pub fn placement_is_one_shot(controls: &Controls) -> bool {
    controls.viewport_armed_one_shot || !controls.viewport_sticky
}

/// Draw the strip inside `ui`. Kept separate from [`show`] so tests can focus
/// the buttons without a full app frame.
pub fn show_in_ui(
    ui: &mut egui::Ui,
    view: &ToolView,
    controls: &mut Controls,
    bindings: &Bindings,
) {
    ui.horizontal_wrapped(|ui| {
        ui.label(format!("Next click: {}", view.selected.label()));
        for option in &view.options {
            let selected = view.selected == option.tool;
            let label = match option.availability {
                ToolAvailability::EnableThenPlace(_) => {
                    format!("{} (enable & place)", option.tool.label())
                }
                _ => option.tool.label().to_owned(),
            };
            let shortcut = shortcut_hint(option.tool, bindings);
            let hover = match option.availability {
                ToolAvailability::Ready => match shortcut {
                    Some(shortcut) => format!("{} ({shortcut})", option.tool.tooltip()),
                    None => option.tool.tooltip().to_owned(),
                },
                ToolAvailability::EnableThenPlace(reason) => {
                    format!("{reason} — click to enable it and place once")
                }
                ToolAvailability::Disabled(reason) => reason.to_owned(),
            };
            let response = ui.add_enabled(
                option.availability.is_interactive(),
                egui::Button::selectable(selected, label),
            );
            let response = response.on_hover_text(hover);
            if response.clicked() {
                activate(option.tool, view, controls);
            }
        }
        ui.separator();
        ui.label("Mode");
        for (sticky, label, hover) in [
            (false, "One-shot", "Place once, then return to Inspect"),
            (
                true,
                "Keep placing",
                "Keep the tool selected for every click until changed",
            ),
        ] {
            if ui
                .selectable_label(controls.viewport_sticky == sticky, label)
                .on_hover_text(hover)
                .clicked()
            {
                controls.viewport_sticky = sticky;
            }
        }
    });
}

/// The key label for a tool's existing binding, if it has one. The strip only
/// surfaces the binding; it never assigns or repurposes a chord.
fn shortcut_hint(tool: ViewportTool, bindings: &Bindings) -> Option<String> {
    let action = match tool {
        ViewportTool::SeedStreamline => Some(InteractionAction::SeedStreamline),
        ViewportTool::AddParticles => Some(InteractionAction::AddParticles),
        ViewportTool::Inspect | ViewportTool::PlaceProbe => None,
    }?;
    bindings
        .chord(action)
        .map(chord_shortcut_label)
        .filter(|label| !label.is_empty())
}

/// Paint the strip at the viewport's top edge. The central viewport records its
/// rect every frame, so the overlay follows resizes in both wide and compact
/// layouts.
pub fn show(
    ctx: &egui::Context,
    viewport: egui::Rect,
    view: &ToolView,
    controls: &mut Controls,
    bindings: &Bindings,
) {
    if !view.options.iter().any(|option| {
        matches!(
            option.tool,
            ViewportTool::SeedStreamline | ViewportTool::AddParticles
        ) && option.availability.is_interactive()
    }) && view.selected == ViewportTool::Inspect
    {
        // A scalar plot has no vector click tools; the strip would only say
        // Inspect. Keep the viewport clean there.
        return;
    }
    egui::Area::new(egui::Id::unique("viewport_tool_strip"))
        .order(egui::Order::Foreground)
        .fixed_pos(egui::pos2(viewport.center().x, viewport.top() + 8.0))
        .pivot(egui::Align2::CENTER_TOP)
        .show(ctx, |ui| {
            ui.set_max_width((viewport.width() - 24.0).max(120.0));
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                show_in_ui(ui, view, controls, bindings);
            });
        });
}
