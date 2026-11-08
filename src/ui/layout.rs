//! Layout state and the collapsed-panel rails (ticket 14).
//!
//! The two side panels can collapse to a narrow vertical rail. The rail is the
//! only restore affordance (ADR 0001 rejected hide-only toggles and
//! hover-peek), so a collapsed panel still occupies [`RAIL_WIDTH`] points and
//! clicking the rail pins the panel open.
//!
//! [`LayoutState`] is the persisted layout record: visibility, expanded widths
//! and help-overlay visibility. It carries serde defaults so an absent or
//! partial record still yields a usable desktop (`serde(default)` on every
//! field, plus [`LayoutState::sanitized`] for corrupt numbers).

use eframe::egui;
use serde::{Deserialize, Serialize};

/// Width of a collapsed panel's edge rail, points. Narrow enough to leave the
/// viewport almost all of its width, wide enough for a stacked label + chevron.
pub const RAIL_WIDTH: f32 = 26.0;

/// Default expanded width of the left Controls panel, points.
pub const DEFAULT_CONTROLS_WIDTH: f32 = 260.0;

/// Default expanded width of the right Presets panel, points.
pub const DEFAULT_PRESETS_WIDTH: f32 = 240.0;

/// Narrowest an expanded panel may be dragged to.
pub const MIN_PANEL_WIDTH: f32 = 180.0;

/// Widest an expanded panel may be dragged to.
pub const MAX_PANEL_WIDTH: f32 = 460.0;

/// Which edge a rail sits on; it only decides the chevron direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RailSide {
    Left,
    Right,
}

/// Persisted layout: which panels are expanded, how wide they are, and whether
/// the help overlay is up. First launch expands both panels and shows help.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutState {
    /// Controls panel (left) expanded rather than collapsed to its rail.
    pub controls_expanded: bool,
    /// Presets panel (right) expanded rather than collapsed to its rail.
    pub presets_expanded: bool,
    /// Expanded width of the Controls panel, points; kept while collapsed so
    /// re-expanding restores the user's width.
    pub controls_width: f32,
    /// Expanded width of the Presets panel, points.
    pub presets_width: f32,
    /// Whether the viewport's help overlay is visible.
    pub help_visible: bool,
}

impl Default for LayoutState {
    fn default() -> Self {
        Self {
            controls_expanded: true,
            presets_expanded: true,
            controls_width: DEFAULT_CONTROLS_WIDTH,
            presets_width: DEFAULT_PRESETS_WIDTH,
            help_visible: true,
        }
    }
}

impl LayoutState {
    /// Clamp stored widths into the draggable range; a corrupt (non-finite)
    /// width falls back to the default rather than producing a zero-width
    /// panel. Called on load so bad storage never bricks the layout.
    pub fn sanitized(mut self) -> Self {
        self.controls_width = clamp_width(self.controls_width, DEFAULT_CONTROLS_WIDTH);
        self.presets_width = clamp_width(self.presets_width, DEFAULT_PRESETS_WIDTH);
        self
    }

    /// Back to first-launch layout: both panels expanded at default widths and
    /// the help overlay visible.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Show the Controls panel (or its rail).
    pub fn toggle_controls(&mut self) {
        self.controls_expanded = !self.controls_expanded;
    }

    /// Show the Presets panel (or its rail).
    pub fn toggle_presets(&mut self) {
        self.presets_expanded = !self.presets_expanded;
    }

    /// Flip the help-overlay setting.
    pub fn toggle_help(&mut self) {
        self.help_visible = !self.help_visible;
    }
}

/// Clamp a panel width into [`MIN_PANEL_WIDTH`]..=[`MAX_PANEL_WIDTH`]; a
/// non-finite width falls back to `fallback`.
pub fn clamp_width(width: f32, fallback: f32) -> f32 {
    if width.is_finite() {
        width.clamp(MIN_PANEL_WIDTH, MAX_PANEL_WIDTH)
    } else {
        fallback
    }
}

/// Draw a collapsed panel's rail as a clickable vertical strip: the label
/// stacked one character per row, a chevron pointing out toward the collapsed
/// panel, and a tooltip naming the shortcut. Returns true when clicked (the
/// caller expands and pins the panel).
pub fn rail(ui: &mut egui::Ui, side: RailSide, label: &str, shortcut: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(ui.available_size(), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&response);
        ui.painter().rect_filled(rect, 0.0, visuals.bg_fill);

        let font = egui::FontId::proportional(11.0);
        let line = 13.0;
        let mut y = rect.top() + 8.0;
        for ch in label.chars() {
            ui.painter().text(
                egui::pos2(rect.center().x, y),
                egui::Align2::CENTER_TOP,
                ch,
                font.clone(),
                visuals.text_color(),
            );
            y += line;
        }
        let chevron = match side {
            RailSide::Left => "›",
            RailSide::Right => "‹",
        };
        ui.painter().text(
            egui::pos2(rect.center().x, rect.bottom() - 12.0),
            egui::Align2::CENTER_CENTER,
            chevron,
            font,
            visuals.text_color(),
        );
    }
    response
        .on_hover_text(format!("Show {label} ({shortcut})"))
        .clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_launch_expands_both_panels_and_shows_help() {
        let layout = LayoutState::default();
        assert!(layout.controls_expanded, "the Controls panel starts expanded");
        assert!(layout.presets_expanded, "the Presets panel starts expanded");
        assert!(layout.help_visible, "the help overlay is visible on first run");
        assert_eq!(layout.controls_width, DEFAULT_CONTROLS_WIDTH);
        assert_eq!(layout.presets_width, DEFAULT_PRESETS_WIDTH);
        const { assert!(RAIL_WIDTH < MIN_PANEL_WIDTH, "the rail stays narrower than any expanded panel"); }
        assert!((16.0..=40.0).contains(&RAIL_WIDTH), "the rail is a slim strip");
    }

    #[test]
    fn toggles_flip_visibility_and_back() {
        let mut layout = LayoutState::default();
        layout.toggle_controls();
        assert!(!layout.controls_expanded, "one click collapses the Controls panel");
        layout.toggle_controls();
        assert!(layout.controls_expanded, "a second click pins it open again");
        layout.toggle_presets();
        assert!(!layout.presets_expanded);
        layout.toggle_presets();
        assert!(layout.presets_expanded);
        layout.toggle_help();
        assert!(!layout.help_visible, "dismissing help hides the overlay");
        layout.toggle_help();
        assert!(layout.help_visible);
    }

    #[test]
    fn reset_restores_first_launch_layout() {
        let mut layout = LayoutState {
            controls_expanded: false,
            presets_expanded: false,
            controls_width: 400.0,
            presets_width: 350.0,
            help_visible: false,
        };
        layout.reset();
        assert_eq!(layout, LayoutState::default(), "reset layout returns every knob");
        assert!(layout.controls_expanded, "reset re-expands rather than hiding");
    }

    #[test]
    fn widths_clamp_and_corrupt_widths_fall_back() {
        assert_eq!(clamp_width(10.0, 250.0), MIN_PANEL_WIDTH);
        assert_eq!(clamp_width(10_000.0, 250.0), MAX_PANEL_WIDTH);
        assert_eq!(clamp_width(212.5, 250.0), 212.5, "a sane width passes through");
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(clamp_width(bad, 250.0), 250.0, "{bad} falls back to the default");
        }
        let corrupt = LayoutState {
            controls_width: f32::NAN,
            presets_width: 5000.0,
            ..Default::default()
        }
        .sanitized();
        assert_eq!(corrupt.controls_width, DEFAULT_CONTROLS_WIDTH);
        assert_eq!(corrupt.presets_width, MAX_PANEL_WIDTH);
        assert!(corrupt.controls_expanded && corrupt.presets_expanded, "visibility survives");
    }
}
