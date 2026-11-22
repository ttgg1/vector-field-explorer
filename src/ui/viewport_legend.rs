//! Issue #230: the colour legend painted next to the viewport.
//!
//! The overlay is a second view of the sidebar's existing colour mapping, not a
//! second source of truth. It reads the same `ColorMapping`/`ColorStats` seams
//! and writes through the same `State` setters, so an edit in either place is
//! visible in the other on the next frame.

use eframe::egui;

use crate::color_mapping::{ColorQuantity, ColorStats};
use crate::material::{MaterialFamily, material_family};
use crate::maxwell::{MaxwellFamily, maxwell_family};
use crate::source_scene::{FieldMode, SourceDisplay};
use crate::ui::sidebar::Controls;
use crate::viewmodel::State;

/// Header text. Also the accessible name of the collapse control.
pub const LEGEND_TITLE: &str = "Colour legend";

/// The app's honest unit convention. Colours use normalized teaching units,
/// never SI.
pub const PANEL_UNITS_NOTE: &str = "Normalized teaching units (not SI)";

pub const BOTH_NOTE: &str = "Both: one live colour per physics family, set in Appearance. No single magnitude ramp; \
     E and B are distinct labelled scales";

/// A pure, testable description of what the legend shows this frame.
#[derive(Debug, Clone, PartialEq)]
pub struct LegendSnapshot {
    /// Short quantity label: `F`, `E`, `B`, or `E + B` for Both.
    pub quantity: &'static str,
    /// Longer quantity/family explanation under the label.
    pub quantity_detail: String,
    /// Unit convention.
    pub units: &'static str,
    /// Human-readable effective range, e.g. `0.50 - 4.00`.
    pub range: String,
    /// Effective minimum (legend seam).
    pub min: f32,
    /// Effective maximum (legend seam).
    pub max: f32,
    /// Whether the range is currently frozen.
    pub locked: bool,
    /// `Auto` or `Locked`.
    pub state_label: &'static str,
    /// Active arrow-length mode label.
    pub arrow_mode: &'static str,
    /// One-line meaning of the arrow-length mode.
    pub arrow_meaning: &'static str,
    /// Effective family colours for the Sources `Both` display.
    pub family_scales: Option<[(&'static str, [u8; 3]); 2]>,
    /// Whether one numeric ramp describes the colours.
    pub numeric_ramp: bool,
    /// Whether lock/edit/unlock controls are meaningful.
    pub lockable: bool,
}

impl LegendSnapshot {
    /// Derive the overlay's display state from the same seams the sidebar uses.
    pub fn new(state: &State, stats: &ColorStats) -> Self {
        let mapping = state.color_mapping();
        let legend = mapping.legend(stats);
        let both =
            state.field_mode == FieldMode::Sources && state.sources.display == SourceDisplay::Both;
        let directional = state.color_quantity == ColorQuantity::Direction2d;
        let family_scales =
            both.then(|| crate::ui::sources_panel::both_legend(state.source_family_colors()));
        let (quantity, quantity_detail) = quantity_labels(state);
        let range = if directional {
            format!("{:.0}\u{b0} - {:.0}\u{b0}", legend.min, legend.max)
        } else {
            format!("{:.2} - {:.2}", legend.min, legend.max)
        };
        Self {
            quantity,
            quantity_detail,
            units: PANEL_UNITS_NOTE,
            range,
            min: legend.min,
            max: legend.max,
            locked: !state.auto_fit,
            state_label: if both {
                "Family colours"
            } else if state.auto_fit {
                "Auto"
            } else {
                "Locked"
            },
            arrow_mode: state.arrow_length_mode.label(),
            arrow_meaning: state.arrow_length_mode.tooltip(),
            family_scales,
            numeric_ramp: !both,
            lockable: !both
                && !directional
                && legend.min.is_finite()
                && legend.max.is_finite()
                && legend.min < legend.max,
        }
    }
}

fn quantity_labels(state: &State) -> (&'static str, String) {
    let quantity = state.color_quantity.label();
    let (short, family) = match state.field_mode {
        FieldMode::Formula | FieldMode::Ode => ("F", "Formula field"),
        FieldMode::Induction => ("E", "Electric field"),
        FieldMode::Sources => match state.sources.display {
            SourceDisplay::Electric => ("E", "Electric field"),
            SourceDisplay::Magnetic => ("B", "Magnetic field"),
            SourceDisplay::Both => (
                "E + B",
                "Electric and magnetic fields (one live colour per family)",
            ),
        },
        FieldMode::Materials => match material_family(state.material.case) {
            MaterialFamily::Electric => ("E", "Electric field"),
            MaterialFamily::Magnetic => ("B", "Magnetic field"),
        },
        FieldMode::Maxwell => match maxwell_family(state.maxwell.case) {
            MaxwellFamily::Electric => ("E", "Electric field"),
            MaxwellFamily::Magnetic => ("B", "Magnetic field"),
        },
    };
    let both =
        state.field_mode == FieldMode::Sources && state.sources.display == SourceDisplay::Both;
    if both {
        (short, family.to_owned())
    } else {
        (short, format!("{family} \u{b7} {quantity}"))
    }
}

/// The overlay's content width. Compact keeps a smaller cap and always leaves
/// most of the viewport uncovered.
pub fn panel_width(viewport: egui::Rect, compact: bool) -> f32 {
    let cap = (viewport.width() - 24.0).max(1.0);
    let desired: f32 = if compact { 180.0 } else { 250.0 };
    let compact_cap = (viewport.width() * 0.6).max(1.0);
    desired
        .min(cap)
        .min(if compact { compact_cap } else { cap })
}

/// Result of painting the legend: its actual rectangle and whether the body is
/// open this frame.
#[derive(Debug, Clone, Copy)]
pub struct LegendFrame {
    pub rect: egui::Rect,
    pub body_visible: bool,
}

/// Paint the legend at the viewport's top-right.
///
/// All controls are ordinary egui widgets (collapsing header, buttons and
/// `DragValue` rows), so tab focus and activation follow the app's normal
/// keyboard handling.
pub fn show(
    ctx: &egui::Context,
    viewport: egui::Rect,
    state: &mut State,
    controls: &mut Controls,
    stats: &ColorStats,
    compact: bool,
) -> LegendFrame {
    let snapshot = LegendSnapshot::new(state, stats);
    let width = panel_width(viewport, compact);
    let anchor = egui::pos2(viewport.right() - 12.0, viewport.top() + 12.0);
    let mut body_visible = false;

    let response = egui::Area::new(egui::Id::unique("viewport_color_legend"))
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::RIGHT_TOP)
        .fixed_pos(anchor)
        .show(ctx, |ui| {
            ui.set_width(width);
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                egui::CollapsingHeader::new(LEGEND_TITLE)
                    .id_salt("viewport_color_legend_body")
                    .default_open(!compact)
                    .show(ui, |ui| {
                        body_visible = true;
                        legend_body(ui, state, controls, stats, &snapshot);
                    });
            });
        });

    LegendFrame {
        rect: response.response.rect,
        body_visible,
    }
}

fn legend_body(
    ui: &mut egui::Ui,
    state: &mut State,
    controls: &mut Controls,
    stats: &ColorStats,
    snapshot: &LegendSnapshot,
) {
    ui.horizontal(|ui| {
        ui.strong(snapshot.quantity);
        ui.label(&snapshot.quantity_detail);
    });
    if snapshot.family_scales.is_none() {
        ui.label(format!("Units: {}", snapshot.units));
    }

    if let Some(scales) = snapshot.family_scales {
        ui.label(BOTH_NOTE);
        for (label, rgb) in scales {
            ui.colored_label(egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]), label);
        }
    } else if snapshot.numeric_ramp {
        ui.label(format!("Range: {}", snapshot.range));
        ui.label(snapshot.state_label);
        lock_controls(ui, state, controls, stats, snapshot);
    }

    ui.separator();
    ui.label(format!("Arrow length: {}", snapshot.arrow_mode))
        .on_hover_text(snapshot.arrow_meaning);
    ui.small(snapshot.arrow_meaning);
}

fn lock_controls(
    ui: &mut egui::Ui,
    state: &mut State,
    controls: &mut Controls,
    stats: &ColorStats,
    snapshot: &LegendSnapshot,
) {
    if !snapshot.lockable {
        ui.small("Locking is unavailable for this quantity.");
        return;
    }
    if state.auto_fit {
        if ui
            .button("Lock range")
            .on_hover_text("Commit the effective range and turn auto-fit off")
            .clicked()
        {
            state.freeze_color_range(stats);
            sync_range_mirrors(state, controls);
        }
        return;
    }

    let mut min = state.color_min;
    let mut max = state.color_max;
    let changed = ui
        .horizontal(|ui| {
            ui.label("Min");
            let min_changed = ui
                .add(egui::DragValue::new(&mut min).speed(0.01))
                .on_hover_text("Magnitude at the colour ramp's low end")
                .changed();
            ui.label("Max");
            let max_changed = ui
                .add(egui::DragValue::new(&mut max).speed(0.01))
                .on_hover_text("Magnitude at the colour ramp's high end")
                .changed();
            min_changed || max_changed
        })
        .inner;
    if changed {
        state.set_color_range(min, max);
        // `set_color_range` may swap the bounds; mirror the effective state.
        sync_range_mirrors(state, controls);
    }
    if ui
        .button("Unlock")
        .on_hover_text("Return to auto-fitting the colour range")
        .clicked()
    {
        state.set_auto_fit(true);
        sync_range_mirrors(state, controls);
    }
}

/// Keep the sidebar mirrors in step with an overlay mutation in the same frame.
fn sync_range_mirrors(state: &State, controls: &mut Controls) {
    controls.auto_fit = state.auto_fit;
    controls.color_min = state.color_min;
    controls.color_max = state.color_max;
}
