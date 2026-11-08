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

/// Outer width of a collapsed panel's edge rail, points, including the rail
/// panel frame's margins. Narrow enough to leave the viewport almost all of
/// its width, wide enough for the rotated 14 pt label.
///
/// This is the **outer** panel width, not the label band: the panel is shown
/// with [`rail_frame`], which takes [`RAIL_FRAME_MARGIN`] on each side, and
/// egui reserves one more point on the fixed edge for the separator line. The
/// real content band is therefore `32 - 2×4 - 1 = 23` pt (measured by the
/// layout tests), which clears the 14 pt label's 16 pt line box with room to
/// spare. The default side-panel frame's 8 pt margins would leave only 15 pt —
/// narrower than the line box — which is why the rail overrides the frame.
pub const RAIL_WIDTH: f32 = 32.0;

/// Horizontal inner margin of the rail panel frame, points, each side.
/// Replaces the default `Frame::side_top_panel` 8 pt horizontal margin, which
/// leaves a content band narrower than the rotated label's line box. The
/// vertical 2 pt padding stays at the default.
pub const RAIL_FRAME_MARGIN: i8 = 4;

/// Font size of the rotated rail label, points. Readable at a glance while
/// still fitting the rotated line box inside the real rail band.
pub const RAIL_LABEL_SIZE: f32 = 14.0;

/// Bottom slice of the rail reserved for the chevron, points. The rotated
/// label is centred in the band above it so the two never collide.
pub const RAIL_CHEVRON_ZONE: f32 = 24.0;

/// Frame for a collapsed rail panel. The rail's fill and separator match every
/// other side panel; only the horizontal inner margin is tightened so the
/// label band is wide enough for 14 pt type (see [`RAIL_WIDTH`]).
pub fn rail_frame(style: &egui::Style) -> egui::Frame {
    egui::Frame::side_top_panel(style).inner_margin(egui::Margin::symmetric(RAIL_FRAME_MARGIN, 2))
}

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

/// The glyph on an expanded panel's collapse button: the mirror of
/// [`rail_chevron`], pointing inward toward the edge the panel collapses to.
pub fn collapse_chevron(side: RailSide) -> &'static str {
    match side {
        RailSide::Left => "‹",
        RailSide::Right => "›",
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
    /// Centre the rotated label's **ink** in the rail's label band (above the
    /// chevron).
    ///
    /// `ink` is the unrotated glyph bounding box relative to the galley origin
    /// (`egui::Galley::mesh_bounds`), not the galley's line box: the line box
    /// carries the font's full ascent/descent and is taller than the visible
    /// glyphs, so centring it would both pad the label asymmetrically and make
    /// containment depend on font metrics rather than on what the user sees.
    /// `TextShape` rotates about `pos`, so the pivot is backed out of the
    /// rotated ink box's centre to land that centre on the band's centre.
    pub fn place(rail: egui::Rect, ink: egui::Rect, side: RailSide) -> Self {
        let angle = rail_text_angle(side);
        let rotated = ink.rotate_bb(egui::emath::Rot2::from_angle(angle));
        let centre = rail_label_band(rail).center();
        Self {
            pos: centre - rotated.center().to_vec2(),
            angle,
        }
    }

    /// Visual bounding box of the rotated label ink, in rail coordinates.
    pub fn visual_bounds(&self, ink: egui::Rect) -> egui::Rect {
        ink.rotate_bb(egui::emath::Rot2::from_angle(self.angle))
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
        let placed = RailLabel::place(rect, galley.mesh_bounds, side);
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

/// Geometry of a shown rail panel: the click target / label band
/// ([`Self::content_rect`]) and the full panel footprint including the frame
/// margins and separator allowance ([`Self::outer_rect`]).
#[derive(Debug, Clone, Copy)]
pub struct RailPanel {
    /// Whether the rail was clicked this frame.
    pub clicked: bool,
    /// The panel's outer rect; its width is exactly [`RAIL_WIDTH`].
    pub outer_rect: egui::Rect,
    /// The panel's real content rect: the click target, the tooltip's hover
    /// area and the space the rotated label must fit. Narrower than
    /// [`RAIL_WIDTH`] by the frame margins.
    pub content_rect: egui::Rect,
}

/// Show a collapsed panel's rail as a side panel: the one place the rail's
/// panel id, exact outer size and frame are configured. Both the app and the
/// layout tests call this, so the tested geometry cannot drift from the
/// shipped panel (the ticket-22 review found the tests asserting against a
/// synthetic `RAIL_WIDTH` rect instead of the real content band).
pub fn rail_panel(
    ui: &mut egui::Ui,
    id_salt: impl egui::AsIdSalt,
    side: RailSide,
    label: &str,
    shortcut: &str,
) -> RailPanel {
    let panel = match side {
        RailSide::Left => egui::Panel::left(id_salt),
        RailSide::Right => egui::Panel::right(id_salt),
    };
    let mut content_rect = egui::Rect::NOTHING;
    let response = panel
        .exact_size(RAIL_WIDTH)
        .frame(rail_frame(ui.style()))
        .show(ui, |ui| {
            content_rect = ui.max_rect();
            rail(ui, side, label, shortcut)
        });
    RailPanel {
        clicked: response.inner,
        outer_rect: response.response.rect,
        content_rect,
    }
}

/// A slim collapse chevron in the header of an expanded panel. Clicking it
/// collapses the panel back to its rail — the symmetric affordance to the
/// rail's expand click, alongside Ctrl+B / Ctrl+P.
///
/// `small_button` is deliberate: in egui 0.37 it does **not** shrink the type
/// (it only trims 2 pt of vertical padding), so the chevron glyph already
/// paints at the body/button size. Keeping the compact padding leaves the
/// panel header slim; the chevron's horizontal button padding still gives it a
/// comfortable click target and its tooltip is unchanged.
pub fn collapse_button(ui: &mut egui::Ui, side: RailSide, shortcut: &str) -> bool {
    let tooltip = match side {
        RailSide::Left => format!("Collapse Controls ({shortcut})"),
        RailSide::Right => format!("Collapse Presets ({shortcut})"),
    };
    let mut clicked = false;
    ui.horizontal(|ui| {
        clicked = ui
            .small_button(collapse_chevron(side))
            .on_hover_text(tooltip)
            .clicked();
    });
    clicked
}
