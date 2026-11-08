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
/// viewport almost all of its width, wide enough for the rotated label's cap
/// height plus breathing room (ticket 22: 26 → 32 for 14 pt type).
pub const RAIL_WIDTH: f32 = 32.0;

/// Font size of the rotated rail label, points. Readable at a glance while
/// still fitting the rotated cap height inside [`RAIL_WIDTH`].
pub const RAIL_LABEL_SIZE: f32 = 14.0;

/// Bottom slice of the rail reserved for the chevron, points. The rotated
/// label is centred in the band above it so the two never collide.
pub const RAIL_CHEVRON_ZONE: f32 = 24.0;

/// Default expanded width of the left Controls panel, points.
pub const DEFAULT_CONTROLS_WIDTH: f32 = 260.0;

/// Default expanded width of the right Presets panel, points.
pub const DEFAULT_PRESETS_WIDTH: f32 = 240.0;

/// Narrowest an expanded panel may be dragged to.
pub const MIN_PANEL_WIDTH: f32 = 180.0;

/// Widest an expanded panel may be dragged to.
pub const MAX_PANEL_WIDTH: f32 = 460.0;

/// Which edge a rail sits on: decides the label's reading direction and the
/// chevron's outward direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RailSide {
    Left,
    Right,
}

/// Rotation of the rail label in radians, clockwise on screen (the
/// `epaint::TextShape` convention).
///
/// Convention (ticket 22): the **left** rail reads bottom-to-top (negative,
/// counter-clockwise) and the **right** rail reads top-to-bottom (positive,
/// clockwise). Each label is therefore 90° from upright text and the pair is a
/// vertical mirror of each other. The chevron direction is independent: it
/// always points out toward the viewport the rail expands into.
pub fn rail_text_angle(side: RailSide) -> f32 {
    match side {
        RailSide::Left => -std::f32::consts::FRAC_PI_2,
        RailSide::Right => std::f32::consts::FRAC_PI_2,
    }
}

/// The glyph on a rail chevron: outward means rightwards on the left edge and
/// leftwards on the right edge.
pub fn rail_chevron(side: RailSide) -> &'static str {
    match side {
        RailSide::Left => "›",
        RailSide::Right => "‹",
    }
}

/// The part of the rail the label may use: everything above the chevron zone.
pub fn rail_label_band(rail: egui::Rect) -> egui::Rect {
    egui::Rect::from_min_max(
        rail.min,
        egui::pos2(rail.max.x, (rail.max.y - RAIL_CHEVRON_ZONE).max(rail.min.y)),
    )
}

/// Centre of the rail chevron: horizontally centred, inside the bottom zone.
pub fn rail_chevron_pos(rail: egui::Rect) -> egui::Pos2 {
    egui::pos2(rail.center().x, rail.bottom() - RAIL_CHEVRON_ZONE / 2.0)
}

/// Tooltip shown over a collapsed rail. Unchanged by ticket 22.
pub fn rail_tooltip(label: &str, shortcut: &str) -> String {
    format!("Show {label} ({shortcut})")
}

/// Placement of the single rotated rail label: the `pos` pivot handed to
/// `epaint::TextShape` and its clockwise rotation. Pure so centring and
/// containment are testable without a GPU or a font atlas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RailLabel {
    /// Upper-left corner of the unrotated galley; `TextShape`'s rotation pivot.
    pub pos: egui::Pos2,
    /// Clockwise rotation in radians, see [`rail_text_angle`].
    pub angle: f32,
}

impl RailLabel {
    /// Centre the rotated label in the rail's label band (above the chevron).
    ///
    /// `TextShape` rotates about `pos`, so the pivot is backed out of the
    /// rotated bounding box's centre to land that centre on the band's centre.
    pub fn place(rail: egui::Rect, galley_size: egui::Vec2, side: RailSide) -> Self {
        let angle = rail_text_angle(side);
        let rotated = egui::Rect::from_min_size(egui::Pos2::ZERO, galley_size)
            .rotate_bb(egui::emath::Rot2::from_angle(angle));
        let centre = rail_label_band(rail).center();
        Self {
            pos: centre - rotated.center().to_vec2(),
            angle,
        }
    }

    /// Visual bounding box of the rotated label, in rail coordinates.
    pub fn visual_bounds(&self, galley_size: egui::Vec2) -> egui::Rect {
        egui::Rect::from_min_size(egui::Pos2::ZERO, galley_size)
            .rotate_bb(egui::emath::Rot2::from_angle(self.angle))
            .translate(self.pos.to_vec2())
    }
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

/// Draw a collapsed panel's rail as a clickable vertical strip: one rotated
/// label reading bottom-to-top on the left and top-to-bottom on the right, a
/// chevron at the bottom pointing out toward the collapsed panel, and a
/// tooltip naming the shortcut. Returns true when clicked (the caller expands
/// and pins the panel).
pub fn rail(ui: &mut egui::Ui, side: RailSide, label: &str, shortcut: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(ui.available_size(), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&response);
        ui.painter().rect_filled(rect, 0.0, visuals.bg_fill);

        let font = egui::FontId::proportional(RAIL_LABEL_SIZE);
        let color = visuals.text_color();
        let galley = ui
            .painter()
            .layout_no_wrap(label.to_owned(), font.clone(), color);
        let placed = RailLabel::place(rect, galley.size(), side);
        ui.painter()
            .add(egui::epaint::TextShape::new(placed.pos, galley, color).with_angle(placed.angle));
        ui.painter().text(
            rail_chevron_pos(rect),
            egui::Align2::CENTER_CENTER,
            rail_chevron(side),
            font,
            color,
        );
    }
    response
        .on_hover_text(rail_tooltip(label, shortcut))
        .clicked()
}
