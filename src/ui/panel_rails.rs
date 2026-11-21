//! Ordered, independently toggled side rails. Callers own panel bodies and state.

use eframe::egui;

use super::layout::{self, RailLabel, RailSide};

/// A panel toggle. IDs must be unique within the strip and stable across reorder.
pub struct RailEntry<'a> {
    pub id: &'static str,
    pub label: &'static str,
    pub shortcut: Option<&'a str>,
    pub expanded: &'a mut bool,
}

/// Actual widget geometry, including entries outside the scroll viewport.
pub struct RailEntryResponse {
    pub id: &'static str,
    pub response: egui::Response,
    pub label_bounds: egui::Rect,
}

pub struct RailStripResponse {
    pub outer_rect: egui::Rect,
    pub entries: Vec<RailEntryResponse>,
}

/// Show a 32pt strip in descriptor order, scrolling rather than truncating labels.
/// Include the caller's panel generation in `id_salt` when resetting panel IDs.
pub fn show_rail_strip(
    ui: &mut egui::Ui,
    id_salt: impl egui::AsIdSalt,
    side: RailSide,
    entries: &mut [RailEntry<'_>],
) -> RailStripResponse {
    let panel = match side {
        RailSide::Left => egui::Panel::left(id_salt),
        RailSide::Right => egui::Panel::right(id_salt),
    };
    let mut responses = Vec::with_capacity(entries.len());
    let shown = panel
        .exact_size(layout::RAIL_WIDTH)
        .frame(layout::rail_frame(ui.style()))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("rail-entries")
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for entry in entries {
                        let font = egui::FontId::proportional(layout::RAIL_LABEL_SIZE);
                        let galley = ui.painter().layout_no_wrap(
                            entry.label.to_owned(),
                            font,
                            egui::Color32::PLACEHOLDER,
                        );
                        let (_, rect) = ui.allocate_space(egui::vec2(
                            ui.available_width(),
                            layout::rail_entry_height(&galley),
                        ));
                        let placed = RailLabel::place(rect, galley.mesh_bounds, side);
                        let label_bounds = placed.visual_bounds(galley.mesh_bounds);
                        // The shared painter creates its interaction from the
                        // caller's scope id; pushing the stable entry id here
                        // keeps the strip's widget ids independent of order.
                        let response = ui
                            .push_id(entry.id, |ui| {
                                let response = layout::rail_entry_interaction(ui, rect);
                                // Toggle before painting so the click frame
                                // paints the new selection and chevron instead
                                // of lagging one frame.
                                if response.clicked() {
                                    *entry.expanded = !*entry.expanded;
                                }
                                layout::paint_rail_entry_state(
                                    ui,
                                    rect,
                                    &response,
                                    side,
                                    entry.label,
                                    entry.shortcut,
                                    *entry.expanded,
                                )
                            })
                            .inner;
                        responses.push(RailEntryResponse {
                            id: entry.id,
                            response,
                            label_bounds,
                        });
                    }
                });
        });
    RailStripResponse {
        outer_rect: shown.response.rect,
        entries: responses,
    }
}

/// Fit already-sanitized expanded-panel requests without changing stored widths.
/// Minimums come first; remaining space is proportional to requested excess.
pub fn fit_panel_widths(requested: &[f32], budget: f32) -> Vec<f32> {
    if requested.is_empty() {
        return Vec::new();
    }
    let budget = if budget.is_finite() {
        budget.max(0.0)
    } else {
        0.0
    };
    let total: f32 = requested.iter().sum();
    if total <= budget {
        return requested.to_vec();
    }
    let minimum = layout::MIN_PANEL_WIDTH;
    let minimum_total = minimum * requested.len() as f32;
    if budget <= minimum_total {
        return vec![budget / requested.len() as f32; requested.len()];
    }
    let fraction = (budget - minimum_total) / (total - minimum_total);
    requested
        .iter()
        .map(|width| minimum + (width - minimum) * fraction)
        .collect()
}
