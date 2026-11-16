//! The Plot section: the discoverable scalar-editor controls shared by the wide
//! Controls panel and the compact bottom sheet.
//!
//! The widgets edit [`State`] through its existing setters. The only action that
//! needs the app (the Task 6 Fit plot seam) is reported as
//! [`PlotPanelAction::FitPlot`]; everything else is plain view-model state.

use eframe::egui;

use crate::colormap::Colormap;
use crate::plot::PlotKind;
use crate::plot_examples::EXAMPLES;
use crate::viewmodel::State;

/// An app-level action the panel cannot run without a viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlotPanelAction {
    /// Frame the currently rendered plot with the plot-only camera.
    FitPlot,
}

/// The explicit kind choices, in selector order.
pub const PLOT_KINDS: [PlotKind; 3] = [
    PlotKind::VectorField,
    PlotKind::Curve2D,
    PlotKind::Surface3D,
];

/// The user-facing label for a kind. The scalar labels include their fixed
/// left-hand side so the choice is self-explanatory.
pub fn kind_label(kind: PlotKind) -> &'static str {
    match kind {
        PlotKind::VectorField => "Vector field",
        PlotKind::Curve2D => "Curve y=f(x)",
        PlotKind::Surface3D => "Surface z=f(x,y)",
    }
}

/// The fixed, non-editable left-hand side the formula bar shows for a scalar
/// kind; vector mode has no assignment label.
pub fn source_label(kind: PlotKind) -> Option<&'static str> {
    match kind {
        PlotKind::VectorField => None,
        PlotKind::Curve2D => Some("y ="),
        PlotKind::Surface3D => Some("z ="),
    }
}

/// The editable field's hint/placeholder: the allowed independent variables,
/// deliberately distinct from the vector component syntax.
pub fn source_hint(kind: PlotKind) -> &'static str {
    match kind {
        PlotKind::VectorField => "(x, y)",
        PlotKind::Curve2D => "x only, e.g. 0.5*x^2",
        PlotKind::Surface3D => "x and y, e.g. exp(-(x^2+y^2))",
    }
}

/// The help text attached to the source field.
pub fn source_help(kind: PlotKind) -> &'static str {
    match kind {
        PlotKind::VectorField => "Vector field: comma-separated components, e.g. (-y, x)",
        PlotKind::Curve2D => "Curve y=f(x): use x only; y is the plotted height",
        PlotKind::Surface3D => "Surface z=f(x,y): use x and y; z is the plotted height",
    }
}

/// The catalogue entry currently matching the active draft, if any. `Custom`
/// is the absence of a match, derived from the text rather than stored beside
/// it so the two can never disagree.
pub fn active_example_name(state: &State) -> Option<&'static str> {
    let kind = state.plot.kind;
    let source = state.active_source();
    EXAMPLES
        .iter()
        .find(|example| example.kind == kind && example.source == source)
        .map(|example| example.name)
}

/// Whether Fit plot has finite rendered geometry to frame. Sampling is cached
/// in the state, so this is the same scene the viewport will paint.
fn fit_available(state: &mut State) -> bool {
    if state.is_vector_mode() {
        return false;
    }
    state.plot_scene().bounds.is_some()
}

/// Draw the Plot section. `compact` applies finger-sized row spacing without
/// changing which controls exist.
pub fn show(ui: &mut egui::Ui, state: &mut State, compact: bool) -> Option<PlotPanelAction> {
    if compact {
        ui.spacing_mut().interact_size.y = crate::ui::layout::TOUCH_TARGET;
    }
    let mut action = None;
    egui::CollapsingHeader::new("Plot")
        .default_open(true)
        .show(ui, |ui| {
            kind_selector(ui, state);
            if state.is_vector_mode() {
                ui.weak("Choose Curve or Surface to plot a scalar expression in the formula bar.");
                return;
            }
            source_help_line(ui, state.plot.kind);
            example_selector(ui, state);
            match state.plot.kind {
                PlotKind::Curve2D => curve_controls(ui, state),
                // Surface's one control (resolution) lives in Advanced.
                PlotKind::Surface3D | PlotKind::VectorField => {}
            }
            color_controls(ui, state);
            range_controls(ui, state);
            fit_control(ui, state, &mut action);
            advanced(ui, state);
        });
    action
}

fn kind_selector(ui: &mut egui::Ui, state: &mut State) {
    ui.horizontal_wrapped(|ui| {
        ui.label("Kind");
        for kind in PLOT_KINDS {
            if ui
                .selectable_label(state.plot.kind == kind, kind_label(kind))
                .on_hover_text(match kind {
                    PlotKind::VectorField => "Show the vector field and its controls",
                    PlotKind::Curve2D => "Plot y=f(x) with the saved curve draft",
                    PlotKind::Surface3D => "Plot z=f(x,y) with the saved surface draft",
                })
                .clicked()
            {
                state.set_plot_kind(kind);
            }
        }
    });
}

fn source_help_line(ui: &mut egui::Ui, kind: PlotKind) {
    ui.label(egui::RichText::new(source_help(kind)).small().weak());
}

fn example_selector(ui: &mut egui::Ui, state: &mut State) {
    let selected = active_example_name(state).unwrap_or("Custom");
    let mut picked = None;
    ui.horizontal(|ui| {
        ui.label("Example");
        egui::ComboBox::from_id_salt("plot_example")
            .selected_text(selected)
            .show_ui(ui, |ui| {
                ui.selectable_label(selected == "Custom", "Custom")
                    .on_hover_text("Keep the expression currently typed in the formula bar");
                for example in EXAMPLES {
                    if ui
                        .selectable_label(selected == example.name, example.name)
                        .on_hover_text(format!(
                            "Load `{}` through the ordinary edit path",
                            example.source
                        ))
                        .clicked()
                    {
                        picked = Some(example.name);
                    }
                }
            });
    });
    if let Some(name) = picked
        && let Some(example) = EXAMPLES.iter().find(|example| example.name == name)
    {
        state.select_plot_example(example);
    }
}

fn curve_controls(ui: &mut egui::Ui, state: &mut State) {
    ui.horizontal(|ui| {
        ui.label("Samples");
        let mut samples = state.plot.settings.curve_samples;
        if ui
            .add(egui::DragValue::new(&mut samples).range(2..=4097))
            .on_hover_text("Curve samples across the inclusive x domain")
            .changed()
        {
            state.set_plot_samples(samples);
        }
    });
    ui.horizontal(|ui| {
        ui.label("Line width (world units)");
        let mut width = state.plot.settings.line_width;
        if ui
            .add(
                egui::DragValue::new(&mut width)
                    .speed(0.001)
                    .range(0.001..=0.2)
                    .max_decimals(3),
            )
            .on_hover_text("Curve width in world units, not screen pixels")
            .changed()
        {
            state.set_plot_width(width);
        }
    });
}

fn color_controls(ui: &mut egui::Ui, state: &mut State) {
    ui.horizontal_wrapped(|ui| {
        ui.label("Color");
        for colormap in Colormap::ALL {
            if ui
                .selectable_label(state.colormap == colormap, colormap.label())
                .on_hover_text("Colormap shared with the vector field")
                .clicked()
            {
                state.set_colormap(colormap);
            }
        }
    });
}

fn range_controls(ui: &mut egui::Ui, state: &mut State) {
    ui.horizontal_wrapped(|ui| {
        ui.label("Range");
        if ui
            .selectable_label(state.plot.settings.auto_height, "Auto")
            .on_hover_text("Fit the height colours to the finite rendered geometry")
            .clicked()
        {
            state.set_plot_auto_height(true);
        }
        if ui
            .selectable_label(!state.plot.settings.auto_height, "Manual")
            .on_hover_text("Clamp height colours without changing the plotted heights")
            .clicked()
        {
            state.set_plot_auto_height(false);
        }
    });
    ui.label(match state.plot.kind {
        PlotKind::Surface3D => "Height z",
        _ => "Height y",
    });
    if !state.plot.settings.auto_height {
        let mut min = state.plot.settings.height_min;
        let mut max = state.plot.settings.height_max;
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label("Minimum");
            changed |= ui
                .add(egui::DragValue::new(&mut min).speed(0.01))
                .on_hover_text("Height colour minimum; colours only, never geometry")
                .changed();
            ui.label("Maximum");
            changed |= ui
                .add(egui::DragValue::new(&mut max).speed(0.01))
                .on_hover_text("Height colour maximum; colours only, never geometry")
                .changed();
        });
        if changed {
            state.set_plot_height_range(min, max);
        }
    }
}

fn fit_control(ui: &mut egui::Ui, state: &mut State, action: &mut Option<PlotPanelAction>) {
    let available = fit_available(state);
    let response = ui
        .add_enabled(available, egui::Button::new("Fit plot"))
        .on_hover_text("Frame the finite plot bounds with the plot camera (no shortcut)");
    let response = if available {
        response
    } else {
        response.on_disabled_hover_text("Fit plot needs finite plot geometry")
    };
    if response.clicked() {
        *action = Some(PlotPanelAction::FitPlot);
    }
    if !available {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            "Fit plot is unavailable until the plot has finite geometry.",
        );
    }
}

fn advanced(ui: &mut egui::Ui, state: &mut State) {
    egui::CollapsingHeader::new("Advanced")
        .default_open(false)
        .show(ui, |ui| {
            if state.plot.kind == PlotKind::Surface3D {
                ui.horizontal(|ui| {
                    ui.label("Resolution");
                    let mut resolution = state.plot.settings.surface_resolution;
                    if ui
                        .add(egui::DragValue::new(&mut resolution).range(2..=96))
                        .on_hover_text("Vertices per surface axis (square grid)")
                        .changed()
                    {
                        state.set_plot_resolution(resolution);
                    }
                });
            }
            if ui
                .button("Reset plot section")
                .on_hover_text(
                    "Restore this plot's settings and scalar draft; the vector formula is preserved",
                )
                .clicked()
            {
                state.reset_plot_section();
            }
        });
}
