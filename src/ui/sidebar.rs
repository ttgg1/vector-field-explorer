//! Left sidebar: every scene knob the widgets expose — sampling density,
//! arrow length modes and magnitude scale, colormap and color range, streamlines
//! and particles, and the domain extrema. Transport speed lives in the
//! strip under the formula bar (ticket 26), not here.
//!
//! Sections follow the agreed order (tickets 14 and 65): **Display → Camera →
//! Features → Coloring → Domain**. Display, Camera and Features start open,
//! the two less-used sections start collapsed, and every section has a
//! reset-to-defaults button in its header. Camera hosts the fly toggle and is
//! the future home of FOV/near-far/projection settings.
//!
//! The widgets edit a sidebar-local mirror ([`Controls`], like the formula
//! bar's [`FormulaBarState`]); `show` hands those values to the viewmodel
//! setters once per frame, and the setters own clamping, the min<max swaps
//! and the dirty flag. After applying them the mirror is refreshed from the
//! state, so the next frame's widgets start from the effective values: a
//! density the mode clamped down, extrema the seam swapped.
//!
//! Ruling (ledger): the density clamp keys to the dimension BEING SAMPLED
//! — 4..=200 while sampling runs flat, 4..=40 only when sampling runs
//! n³ — so the widget's range keys to `State::is_3d()` and reuses the
//! seam's own constants rather than re-inventing bounds. The seam still
//! clamps; the UI only stays consistent with it.

use eframe::egui;

use crate::arrow_style::{
    ArrowParams, ArrowStyle, ArrowStyle2D, MAX_HEAD_LENGTH, MAX_HEAD_WIDTH, MAX_SHAFT_RADIUS,
    MAX_SIDES, MIN_HEAD_LENGTH, MIN_HEAD_WIDTH, MIN_SHAFT_RADIUS, MIN_SIDES,
};
use crate::color_mapping::{ClipCounts, ColorMapping, ColorQuantity, ColorStats};
use crate::color_ramp::{ColorRamp, ColorStop, RampKind, Rgb, ScaleMode};
use crate::colormap::Colormap;
use crate::domain::Domain;
use crate::parameters::{MAX_PARAMETERS, PARAMETER_VERSION, Parameter, ParameterSet};
use crate::particles::{
    MAX_PARTICLE_OPACITY, MAX_PARTICLES, MAX_STAGNATION_SECONDS, MAX_TRAIL_FADE, MAX_TRAIL_LEN,
    MAX_TRAIL_WIDTH, MIN_PARTICLE_OPACITY, MIN_STAGNATION_SECONDS, MIN_TRAIL_FADE, MIN_TRAIL_WIDTH,
    ParticleAppearance, ParticleColorMode,
};
use crate::plane::{DIM_ALPHA, PlaneAxis, PlaneFrame, PlaneView, plane_coordinate};
use crate::scalar_view::{MAX_CONTOUR_LEVELS, MAX_GRID_SIDE, scalar_view_expr};
use crate::seed_layout::{
    MAX_GENERATED_PER_APPLICATION, MAX_SEEDS, SeedAnchor, SeedLayout, SeedPoint, circle_seeds,
    line_seeds, region_seeds,
};
use crate::seed_layout_library::{DEFAULT_LAYOUT_NAME, SeedLayoutDocument, SeedLayoutProfile};
use crate::source_scene::{FieldMode, SourceDisplay};
use crate::view_command::{ClickActionPlan, click_action_plan};
use crate::viewmodel::{
    ArrowLengthMode, DEFAULT_DENSITY_2D, DEFAULT_MAGNITUDE_SCALE, DEFAULT_PARTICLE_SIZE,
    DEFAULT_PROPORTIONAL_EXPONENT, DEFAULT_SCALAR_CONTOUR_LEVELS, DEFAULT_SCALAR_GRID_SIDE,
    LeftClickAction, MAX_ARROW_OPACITY, MAX_ARROW_SCALE, MAX_DENSITY_2D, MAX_DENSITY_3D,
    MAX_DOMAIN_EXTENT, MAX_MAGNITUDE_SCALE, MAX_PARTICLE_SIZE, MAX_PLANE_DIM_OPACITY,
    MAX_PROPORTIONAL_EXPONENT, MAX_STREAMLINES, MIN_ARROW_OPACITY, MIN_ARROW_SCALE, MIN_DENSITY,
    MIN_DOMAIN_EXTENT, MIN_MAGNITUDE_SCALE, MIN_PARTICLE_SIZE, MIN_PLANE_DIM_OPACITY,
    MIN_PROPORTIONAL_EXPONENT, MIN_SCALAR_GRID_SIDE, State, StreamlineSeedMode,
    clamp_domain_extent,
};
use glam::Vec3;

/// The Controls panel's sections, in display order (ticket 14 ruling).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Display,
    Camera,
    Features,
    Coloring,
    Domain,
}

/// The sections in the order they appear in the panel.
pub const SECTIONS: [Section; 5] = [
    Section::Display,
    Section::Camera,
    Section::Features,
    Section::Coloring,
    Section::Domain,
];

impl Section {
    /// Header text.
    pub fn title(self) -> &'static str {
        match self {
            Section::Display => "Display",
            Section::Camera => "Camera",
            Section::Features => "Features",
            Section::Coloring => "Coloring",
            Section::Domain => "Domain",
        }
    }

    /// Display, Camera and Features are the everyday controls, so they start
    /// open; Coloring and Domain keep their space until the user asks for
    /// them. Camera starts open so the fly toggle is discoverable (ticket 65
    /// ruling).
    pub fn default_open(self) -> bool {
        matches!(self, Section::Display | Section::Camera | Section::Features)
    }
}

/// The effective domain box as the Domain header's summary line, e.g.
/// `x -1…1 · y -1…1 · z -1…1`. `include_z` is false when the view is 2D, so
/// the header never advertises an extent the user cannot see or sample (the
/// row list uses the same gate). Numbers are rounded to at
/// most two decimals and integers lose their trailing `.00`, which keeps the
/// line short enough for the panel's 180 pt minimum.
pub fn domain_summary(domain: &Domain, include_z: bool) -> String {
    let axis =
        |name: &str, lo: f32, hi: f32| format!("{name} {}…{}", extent_text(lo), extent_text(hi));
    let mut parts = vec![
        axis("x", domain.min.x, domain.max.x),
        axis("y", domain.min.y, domain.max.y),
    ];
    if include_z {
        parts.push(axis("z", domain.min.z, domain.max.z));
    }
    parts.join(" · ")
}

/// One number in [`domain_summary`]: an integer when it rounds to one, else
/// at most two decimals with trailing zeros trimmed. Non-finite values cannot
/// reach the state, but the summary still renders something harmless if one
/// appears in a mirror mid-edit.
fn extent_text(value: f32) -> String {
    if !value.is_finite() {
        return "0".to_string();
    }
    let rounded = if value.abs() < 0.005 {
        0.0
    } else {
        (value * 100.0).round() / 100.0
    };
    if rounded.fract() == 0.0 {
        format!("{}", rounded as i64)
    } else {
        let mut text = format!("{rounded:.2}");
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
        text
    }
}

/// One axis of the sampled domain box, in editor row order. The z row exists
/// only while sampling runs three-dimensionally (ticket 50): the value stays
/// stored for the next 3D switch, but the 2D editor does not advertise it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainAxis {
    X,
    Y,
    Z,
}

impl DomainAxis {
    /// The lowercase label the row and its tooltips use.
    pub fn label(self) -> &'static str {
        match self {
            DomainAxis::X => "x",
            DomainAxis::Y => "y",
            DomainAxis::Z => "z",
        }
    }
}

/// The rows the Domain editor renders. Pure so the 2D/3D gating is testable
/// without a window. `three_component` is whether the field itself has a z
/// component, not the sampling mode: a 3D field in 2D keeps its z rows, while a
/// flat formula in 3D mode hides z.
pub fn domain_axes(three_component: bool) -> &'static [DomainAxis] {
    if three_component {
        &[DomainAxis::X, DomainAxis::Y, DomainAxis::Z]
    } else {
        &[DomainAxis::X, DomainAxis::Y]
    }
}

/// How much of the plane selector to show: hidden for a two-component field,
/// axis/index only for a 2D slice, and the full 3D plane-by-plane controls in
/// the 3D view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaneControlMode {
    Hidden,
    SliceOnly,
    Full,
}

/// The single plane-selector visibility rule. A plane needs a three-component
/// field to have a third axis; only the 3D view may also dim the rest of the
/// cube and toggle the mode.
pub fn plane_control_mode(field_dimension: usize, mode_3d: bool) -> PlaneControlMode {
    if field_dimension != 3 {
        PlaneControlMode::Hidden
    } else if mode_3d {
        PlaneControlMode::Full
    } else {
        PlaneControlMode::SliceOnly
    }
}

/// Whether the Magnitude-scale control applies. Only `Magnitude` uses it, so
/// the widget is disabled everywhere else: no dead control that looks
/// adjustable but changes nothing. Pure so the rule is testable without a
/// window, matching the sidebar's other seams.
pub fn magnitude_scale_enabled(mode: ArrowLengthMode) -> bool {
    mode == ArrowLengthMode::Magnitude
}

/// Whether the shared arrow length scale applies. `Magnitude` lengths are the
/// field magnitude times the magnitude scale alone, so the arrow scale is
/// disabled there rather than silently ignored.
pub fn arrow_scale_enabled(mode: ArrowLengthMode) -> bool {
    mode != ArrowLengthMode::Magnitude
}

/// Whether the Proportional contrast slider applies. Only Proportional length
/// follows magnitude, so the other modes disable the control rather than
/// offering an exponent that changes nothing (ticket 57).
pub fn proportional_exponent_enabled(mode: ArrowLengthMode) -> bool {
    mode == ArrowLengthMode::Proportional
}

/// Which arrow-style family set the sidebar offers for the active view
/// (ticket 61). The sets are independent: switching views only swaps which
/// selector is shown, never which family the other view uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrowStyleSet {
    /// The 3D families in [`ArrowStyle::ALL`].
    ThreeD,
    /// The 2D families in [`ArrowStyle2D::ALL`].
    TwoD,
}

impl ArrowStyleSet {
    /// The selector's label.
    pub fn label(self) -> &'static str {
        match self {
            ArrowStyleSet::ThreeD => "Arrow style",
            ArrowStyleSet::TwoD => "2D arrow style",
        }
    }
}

/// The arrow set the sidebar shows for the active view: 3D offers the 3D
/// families, 2D offers the 2D families. Pure so the view swap is testable
/// without a window.
pub fn arrow_style_set(view_3d: bool) -> ArrowStyleSet {
    if view_3d {
        ArrowStyleSet::ThreeD
    } else {
        ArrowStyleSet::TwoD
    }
}

/// Whether the Sides slider applies to the active view's selected family.
pub fn sides_control_enabled(view_3d: bool, style_3d: ArrowStyle, style_2d: ArrowStyle2D) -> bool {
    if view_3d {
        style_3d.uses_sides()
    } else {
        style_2d.uses_sides()
    }
}

/// Whether the Head length/head width sliders apply to the active view's
/// selected family. Cylinder and Line have no head, so those controls are
/// disabled rather than silently ignored.
pub fn head_control_enabled(view_3d: bool, style_3d: ArrowStyle, style_2d: ArrowStyle2D) -> bool {
    if view_3d {
        style_3d.uses_head()
    } else {
        style_2d.uses_head()
    }
}

/// Whether the seed-mode selector applies. With Streamlines off both seed
/// sources are hidden and clicks are not stored, so the selector would be a
/// dead control; it is disabled rather than silently ignored.
pub fn seed_mode_enabled(streamlines: bool) -> bool {
    streamlines
}

/// Whether the explicit streamline-count control applies: the explicit count
/// only replaces Auto's density-derived lattice, so Click-only seeding (which
/// paints click bursts, not a count of lines) leaves it disabled.
pub fn streamline_count_enabled(streamlines: bool, seed_mode: StreamlineSeedMode) -> bool {
    streamlines && seed_mode == StreamlineSeedMode::Auto
}

/// Whether the explicit-seed editor belongs in the Flow/Features section: it
/// only edits formula streamlines, not the source, induction, materials,
/// Maxwell or Ode modes that own their own seeding.
pub fn seed_layout_visible(field_mode: FieldMode, streamlines: bool, vector_mode: bool) -> bool {
    vector_mode && streamlines && field_mode == FieldMode::Formula
}

/// Whether the Variables editor belongs in the Controls panel (issue #89 slice
/// C): it edits the formula field's named parameters, so it hides with the
/// other formula-only vector sections (sources, induction, materials, Maxwell,
/// Ode) and never appears while plotting.
pub fn variables_visible(field_mode: FieldMode, vector_mode: bool) -> bool {
    vector_mode && field_mode == FieldMode::Formula
}

/// Whether the automatic streamline-count control applies. An explicit layout
/// replaces the count and the density grid, so the control is disabled until
/// the layout is cleared.
pub fn streamline_count_enabled_with_layout(
    streamlines: bool,
    seed_mode: StreamlineSeedMode,
    layout_empty: bool,
) -> bool {
    streamline_count_enabled(streamlines, seed_mode) && layout_empty
}

/// The explicit-seed readout above the layout editor.
pub fn seed_layout_count_label(count: usize) -> String {
    match count {
        0 => "No explicit seeds".to_owned(),
        1 => "1 explicit seed".to_owned(),
        count => format!("{count} explicit seeds"),
    }
}

/// Shown beside the disabled streamline-count control while explicit seeds
/// replace the automatic grid.
pub const SEED_LAYOUT_DENSITY_INACTIVE_NOTE: &str =
    "Explicit seeds replace the automatic grid; count is inactive until Clear all.";

/// The streamline-count drag value's text: `0` is the Auto sentinel, any other
/// value shows its number so the widget can both display "Auto" and accept a
/// typed count.
pub fn streamline_count_label(count: u32) -> String {
    if count == 0 {
        "Auto".to_owned()
    } else {
        count.to_string()
    }
}

/// Explained in place of the Coloring widgets when the Both source display
/// owns the colours: fixed family colours replace the colormap, so there is no
/// shared magnitude range to choose.
pub const BOTH_COLORING_NOTE: &str = "Both uses fixed family colours (E amber, B cyan); choose Electric or Magnetic to use a colormap.";

/// Why the Direction quantity is disabled in the 3D view.
pub const DIRECTION_3D_EXPLANATION: &str =
    "Direction colouring is available in 2D; it is disabled in 3D";

/// Smallest half-width the signed-log DragValue accepts.
pub const MIN_SIGNED_LOG_HALF_WIDTH: f32 = 1.0e-6;

/// Largest half-width the signed-log DragValue accepts (matching
/// [`ScaleMode::validate`]).
pub const MAX_SIGNED_LOG_HALF_WIDTH: f32 = 1.0e6;

/// Whether the Coloring section renders the colormap, auto-fit and range
/// widgets. Sources in the `Both` display paint fixed family colours instead,
/// so those controls would be silently inert; single-family source displays
/// and Formula mode use the normal colormap path.
pub fn coloring_controls_visible(field_mode: FieldMode, display: SourceDisplay) -> bool {
    !(field_mode == FieldMode::Sources && display == SourceDisplay::Both)
}

/// Whether the Scalar view section exists for the current field mode (issue
/// #92 slice B1). It is a formula-field overlay; source/ODE/material modes own
/// their own scenes and cannot host a surface scalar.
pub fn scalar_section_visible(field_mode: FieldMode) -> bool {
    field_mode == FieldMode::Formula
}

/// Why the Scalar view cannot sample the current scene, if it cannot. `None`
/// means the controls are usable; the z reason is the model's explicit
/// [`crate::scalar_view::UNSUPPORTED_Z_REASON`]. The positive case is the same
/// [`State::scalar_view_applicable`] predicate that gates overlay composition.
pub fn scalar_view_unsupported_reason(state: &State) -> Option<String> {
    if state.scalar_view_applicable() {
        return None;
    }
    if state.field_mode != FieldMode::Formula {
        return Some("scalar views need the formula field".to_owned());
    }
    if !state.is_vector_mode() {
        return Some("scalar views need a vector-field view".to_owned());
    }
    scalar_view_expr(&state.plot.surface.source, &state.parameters)
        .err()
        .map(|error| error.to_string())
}

/// One probe row's value text: an out-of-domain probe, an invalid evaluation,
/// or the finite sampled value.
fn probe_row_label(
    index: usize,
    probe: &crate::scalar_view::PointProbe,
    domain: &Domain,
) -> String {
    let inside = probe.position.cmpge(domain.min).all() && probe.position.cmple(domain.max).all();
    let status = if !inside {
        "outside the field".to_owned()
    } else {
        match probe.value {
            Some(value) => format!("{value:.3}"),
            None => "invalid".to_owned(),
        }
    };
    format!("Probe {}: {status}", index + 1)
}

/// Whether the particle size slider applies. With Particles off there are no
/// dots, so the slider is disabled rather than silently ignored (ticket 30).
pub fn particle_size_enabled(particles: bool) -> bool {
    particles
}

/// Whether the fixed-colour picker applies (ticket 48): only
/// [`ParticleColorMode::Fixed`] reads it.
pub fn particle_fixed_color_enabled(mode: ParticleColorMode) -> bool {
    mode == ParticleColorMode::Fixed
}

/// The RGB the fixed-colour picker edits. Alpha is not user-editable and is
/// ignored at render, so it never enters the picker (ticket 48 fix round).
pub fn fixed_color_rgb(color: [u8; 4]) -> [u8; 3] {
    [color[0], color[1], color[2]]
}

/// Store the picker's RGB as an opaque colour: the vec4 alpha stays 255 so a
/// previous translucent value can never darken the shown RGB.
pub fn fixed_color_opaque(rgb: [u8; 3]) -> [u8; 4] {
    [rgb[0], rgb[1], rgb[2], 255]
}

/// Whether the trail fade control applies: trails need at least two stored
/// points, so length 0 or 1 leaves the slider disabled (ticket 48).
pub fn trail_fade_enabled(trail_len: u32) -> bool {
    trail_len >= 2
}

/// Whether the trail width control applies: with no drawn trail it would be a
/// dead knob, so it shares the fade's length gate (ticket 48).
pub fn trail_width_enabled(trail_len: u32) -> bool {
    trail_len >= 2
}

/// Whether the stagnation threshold applies: looping off leaves a stuck
/// particle in place, so the seconds control is disabled (ticket 48).
pub fn stagnation_seconds_enabled(loop_on_stagnation: bool) -> bool {
    loop_on_stagnation
}

/// The particle-count drag value's text: `0` is the Auto sentinel, any other
/// value shows its number so the widget can both display "Auto" and accept a
/// typed count (ticket 30).
pub fn particle_count_label(count: u32) -> String {
    if count == 0 {
        "Auto".to_owned()
    } else {
        count.to_string()
    }
}

/// Render the signed-log half-width for its [`egui::DragValue`].
///
/// The valid range reaches down to `1e-6`, which fixed decimal places would
/// round to `0.000`; small magnitudes use scientific notation (still accepted
/// by the widget's default parser) so the minimum stays legible.
fn signed_log_half_width_display(value: f32) -> String {
    let magnitude = value.abs();
    if magnitude != 0.0 && magnitude < 1.0e-2 {
        format!("{value:e}")
    } else {
        format!("{value:.3}")
    }
}

/// Clamp a typed signed-log half-width into the validated range.
fn sanitized_half_width(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(MIN_SIGNED_LOG_HALF_WIDTH, MAX_SIGNED_LOG_HALF_WIDTH)
    } else {
        1.0
    }
}

/// One legend value, rendered as degrees for direction colouring.
fn legend_value(value: f32, directional: bool) -> String {
    if directional {
        format!("{value:.0}\u{b0}")
    } else {
        format!("{value:.2}")
    }
}

/// Paint the gradient bar and its range/ticks/clip summary.
///
/// The bar is sampled directly from the active palette in 64 slices, so it
/// shows the palette rather than clipped sample values.
fn color_legend(ui: &mut egui::Ui, mapping: &ColorMapping, stats: &ColorStats, clips: ClipCounts) {
    let directional = mapping.quantity == ColorQuantity::Direction2d;
    let legend = mapping.legend(stats);
    let desired = egui::vec2(ui.available_width().max(1.0), 14.0);
    let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    const SLICES: usize = 64;
    for slice in 0..SLICES {
        let t0 = slice as f32 / SLICES as f32;
        let t1 = (slice + 1) as f32 / SLICES as f32;
        let rgb = mapping.palette.sample_unit((t0 + t1) * 0.5);
        let slice_rect = egui::Rect::from_min_max(
            egui::pos2(rect.left() + t0 * rect.width(), rect.top()),
            egui::pos2(rect.left() + t1 * rect.width(), rect.bottom()),
        );
        painter.rect_filled(
            slice_rect,
            0.0,
            egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]),
        );
    }

    ui.horizontal_wrapped(|ui| {
        for tick in &legend.ticks {
            ui.label(legend_value(*tick, directional));
        }
    });
    ui.label(format!(
        "Range: {} - {}",
        legend_value(legend.min, directional),
        legend_value(legend.max, directional)
    ));
    // A cyclic ramp wraps instead of clipping, so the below/above caps and
    // counts would be misleading (and direction colouring never clips).
    if !legend.cyclic {
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("< {}", legend_value(legend.min, directional)));
            ui.label(format!("> {}", legend_value(legend.max, directional)));
            ui.label(format!("{} below, {} above", clips.low, clips.high));
        });
    }
}

/// A hex field's in-progress text, kept in egui's temporary data so an
/// unparseable edit stays visible instead of snapping back to the stop colour.
#[derive(Clone)]
struct PaletteHexDraft {
    text: String,
    /// The stop colour this text was seeded from; a different stop colour means
    /// the ramp was reseeded and the draft must follow it.
    color: Rgb,
}

/// Draw one editable hex field bound to `stop`, reporting whether the current
/// text is unparseable. Invalid text never changes the stop.
fn palette_hex_field(ui: &mut egui::Ui, index: usize, stop: &mut ColorStop) -> bool {
    let id = ui.make_persistent_id(("palette_stop_hex", index));
    let mut draft = ui
        .data_mut(|data| data.get_temp::<PaletteHexDraft>(id))
        .unwrap_or_else(|| PaletteHexDraft {
            text: stop.color.to_hex(),
            color: stop.color,
        });
    if draft.color != stop.color {
        draft = PaletteHexDraft {
            text: stop.color.to_hex(),
            color: stop.color,
        };
    }
    ui.add(
        egui::TextEdit::singleline(&mut draft.text)
            .id(id)
            .desired_width(76.0)
            .char_limit(20),
    )
    .on_hover_text("Stop colour as #rrggbb");
    if let Some(color) = Rgb::from_hex(&draft.text) {
        if color != stop.color {
            stop.color = color;
        }
        draft.color = color;
    }
    let invalid = Rgb::from_hex(&draft.text).is_none();
    ui.data_mut(|data| data.insert_temp(id, draft));
    invalid
}

/// Insert a stop at the midpoint of the widest existing gap, sampling the
/// interval's colour so positions stay strictly increasing.
fn palette_add_stop(ramp: &mut ColorRamp) {
    let mut best = 0;
    let mut widest = f32::NEG_INFINITY;
    for (index, pair) in ramp.stops.windows(2).enumerate() {
        let gap = pair[1].position - pair[0].position;
        if gap > widest {
            widest = gap;
            best = index;
        }
    }
    let left = ramp.stops[best];
    debug_assert!(ramp.stops.len() >= 2);
    let right = ramp.stops[best + 1];
    let position = 0.5 * (left.position + right.position);
    // `ColorRamp::sample` honours `reversed`, but stop positions are always
    // specified in ramp order, so sample the interval unreversed.
    let color = ColorRamp {
        stops: vec![left, right],
        reversed: false,
        kind: ramp.kind,
    }
    .sample(position);
    ramp.stops.insert(best + 1, ColorStop { position, color });
}

/// The collapsed custom-palette editor inside Coloring: stop rows, reversal,
/// kind and the portable text buffer. `effective` is the ramp editing starts
/// from and the fallback for exporting when no draft exists.
fn custom_palette_controls(
    ui: &mut egui::Ui,
    controls: &mut Controls,
    effective: &ColorRamp,
    custom_active: bool,
    builtin_label: &str,
) {
    egui::CollapsingHeader::new("Custom palette")
        .id_salt("custom_palette")
        .default_open(false)
        .show(ui, |ui| {
            if controls.palette_working.is_some() {
                let mut remove_index = None;
                let mut add_requested = false;
                {
                    let ramp = controls
                        .palette_working
                        .as_mut()
                        .expect("the working ramp was checked above");
                    let count = ramp.stops.len();
                    for (index, stop) in ramp.stops.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::DragValue::new(&mut stop.position)
                                    .speed(0.001)
                                    .range(0.0..=1.0)
                                    .fixed_decimals(3),
                            )
                            .on_hover_text("Stop position in 0.0..=1.0");
                            let invalid = palette_hex_field(ui, index, stop);
                            if invalid {
                                ui.label("invalid hex");
                            }
                            if ui
                                .add_enabled(
                                    count > ColorRamp::MIN_STOPS,
                                    egui::Button::new("Remove"),
                                )
                                .on_disabled_hover_text("A ramp needs at least two stops")
                                .clicked()
                            {
                                remove_index = Some(index);
                            }
                        });
                    }
                    if let Some(index) = remove_index {
                        ramp.stops.remove(index);
                    }
                    if ui
                        .add_enabled(
                            ramp.stops.len() < ColorRamp::MAX_STOPS,
                            egui::Button::new("Add stop"),
                        )
                        .on_hover_text("Insert a stop in the widest gap")
                        .on_disabled_hover_text("A ramp accepts at most 16 stops")
                        .clicked()
                    {
                        add_requested = true;
                    }
                    if add_requested {
                        palette_add_stop(ramp);
                    }
                    ui.checkbox(&mut ramp.reversed, "Reversed")
                        .on_hover_text("Read the ramp back to front");
                    button_row(ui, |ui| {
                        ui.label("Kind");
                        for kind in RampKind::ALL {
                            ui.selectable_value(&mut ramp.kind, kind, kind.label())
                                .on_hover_text("How the ramp is meant to be read");
                        }
                    });
                }
                let validation = controls
                    .palette_working
                    .as_ref()
                    .expect("the working ramp was checked above")
                    .validate();
                if let Err(reason) = &validation {
                    let error_color = ui.visuals().error_fg_color;
                    ui.colored_label(error_color, reason);
                }
                if ui
                    .add_enabled(validation.is_ok(), egui::Button::new("Apply palette"))
                    .on_disabled_hover_text("Fix the palette error before applying")
                    .clicked()
                {
                    controls.palette_apply_requested = true;
                }
                if ui
                    .button("Revert to built-in")
                    .on_hover_text("Discard the custom palette and use the built-in colormap")
                    .clicked()
                {
                    controls.palette_working = None;
                    controls.palette_apply_requested = true;
                }
            } else {
                let edit_label = if custom_active {
                    "Edit custom palette"
                } else {
                    "Edit current palette"
                };
                if ui
                    .button(edit_label)
                    .on_hover_text("Start from the palette currently in use")
                    .clicked()
                {
                    controls.palette_working = Some(effective.clone());
                }
                ui.label(if custom_active {
                    "Custom palette".to_owned()
                } else {
                    format!("Built-in: {builtin_label}")
                });
            }

            if ui
                .button("Export text")
                .on_hover_text("Write the draft (or active palette) as versioned JSON")
                .clicked()
            {
                let ramp = controls.palette_working.as_ref().unwrap_or(effective);
                controls.palette_text = crate::palette_io::export_ramp(ramp);
            }
            ui.add(
                egui::TextEdit::multiline(&mut controls.palette_text)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            )
            .on_hover_text("Portable palette JSON; paste one here to import it");
            ui.horizontal(|ui| {
                if ui
                    .button("Apply text")
                    .on_hover_text("Parse the text and make it the draft")
                    .clicked()
                {
                    match crate::palette_io::import_ramp(&controls.palette_text) {
                        Ok(ramp) => {
                            controls.palette_working = Some(ramp);
                            controls.palette_apply_requested = true;
                        }
                        Err(error) => controls.palette_message = error,
                    }
                }
                if ui
                    .add_enabled(
                        !controls.palette_text.is_empty(),
                        egui::Button::new("Copy text"),
                    )
                    .on_hover_text("Copy the palette JSON to the clipboard")
                    .on_disabled_hover_text("Export or paste a palette first")
                    .clicked()
                {
                    ui.ctx().copy_text(controls.palette_text.clone());
                }
            });
            if !controls.palette_message.is_empty() {
                ui.label(&controls.palette_message);
            }
        });
}

/// Session-only generator inputs for the Flow section's "Seed layout" area.
#[derive(Debug, Clone, PartialEq)]
pub struct SeedGeneratorControls {
    pub line_from: Vec3,
    pub line_to: Vec3,
    pub line_count: u32,
    pub circle_center: Vec3,
    pub circle_axis: PlaneAxis,
    pub circle_radius: f32,
    pub circle_count: u32,
    pub region_min: Vec3,
    pub region_max: Vec3,
    pub region_count: u32,
}

impl Default for SeedGeneratorControls {
    fn default() -> Self {
        Self {
            line_from: Vec3::new(-1.0, 0.0, 0.0),
            line_to: Vec3::new(1.0, 0.0, 0.0),
            line_count: 8,
            circle_center: Vec3::ZERO,
            circle_axis: PlaneAxis::Z,
            circle_radius: 1.0,
            circle_count: 8,
            region_min: Vec3::splat(-1.0),
            region_max: Vec3::splat(1.0),
            region_count: 16,
        }
    }
}

/// Session-only Variables editor drafts (issue #89 slice C). `drafts` is the
/// working copy the row widgets edit; `synced` is the committed set it was
/// seeded from, so an external commit (preset restore, Edit > Reset) refreshes
/// the drafts without cloning the set on every unchanged frame. `name` is the
/// Add-parameter text field and `error` keeps the model's last rejection.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VariablesControls {
    pub drafts: Vec<Parameter>,
    pub name: String,
    pub error: String,
    synced: ParameterSet,
}

impl VariablesControls {
    /// Follow the committed set only when it actually changed: a rejected
    /// edit leaves the committed set equal to `synced`, so its drafts (and the
    /// other rows' values) survive.
    fn sync(&mut self, committed: &ParameterSet) {
        if self.synced != *committed {
            self.synced = committed.clone();
            self.drafts = self.synced.entries.clone();
            self.error.clear();
        }
    }

    /// Commit the drafts through the atomic model seam. A rejected draft set
    /// keeps the drafts and records the model's message; an accepted one
    /// re-seeds the drafts from the committed set.
    fn commit(&mut self, state: &mut State) -> bool {
        let next = ParameterSet {
            version: PARAMETER_VERSION,
            entries: self.drafts.clone(),
        };
        // A no-op edit must still clear a stale inline error without asking the model to rebuild.
        if next == state.parameters {
            self.error.clear();
            return true;
        }
        match state.commit_parameters(next) {
            Ok(()) => {
                self.synced = state.parameters.clone();
                self.drafts = self.synced.entries.clone();
                self.error.clear();
                true
            }
            Err(message) => {
                self.error = message;
                false
            }
        }
    }

    /// The section's Reset: commit an empty set and clear the session drafts.
    fn reset(&mut self, state: &mut State) {
        self.drafts.clear();
        self.name.clear();
        let _ = self.commit(state);
    }
}

/// Commit the session Variables drafts through the atomic model seam. The
/// widget path and the headless tests share this one handoff.
#[doc(hidden)]
pub fn commit_variables(state: &mut State, controls: &mut Controls) -> bool {
    controls.variables.commit(state)
}

/// The knobs the widgets edit between frames. Starts at the same defaults as
/// a fresh 2D [`State`] so an untouched first sidebar frame never marks the
/// scene dirty (and the status bar counts the scene the user actually sees).
#[derive(Debug, Clone)]
pub struct Controls {
    pub density: u32,
    pub arrow_scale: f32,
    /// Global arrow de-emphasis (ticket 36).
    pub arrow_opacity: f32,
    /// Plane-mode off-plane opacity (ticket 36); the plane controls own the
    /// widget, but the value persists with the other Display knobs.
    pub plane_dim_opacity: f32,
    pub arrow_length_mode: ArrowLengthMode,
    pub magnitude_scale: f32,
    /// The Proportional contrast exponent (ticket 57); disabled outside
    /// Proportional.
    pub proportional_exponent: f32,
    /// Show arrows (ticket 45); unchecking hides only the arrow instances.
    pub arrows: bool,
    /// Show the plane/face grid and domain box.
    pub show_grid: bool,
    /// Show the colored reference axes, independently of field lines.
    pub show_coordinate_system: bool,
    pub arrow_style: ArrowStyle,
    pub arrow_style_2d: ArrowStyle2D,
    pub arrow_params: ArrowParams,
    pub colormap: Colormap,
    /// What the colour ramp represents (issue #83).
    pub color_quantity: ColorQuantity,
    /// How raw values map onto the ramp.
    pub color_mode: ScaleMode,
    /// UI-only mirror of the scale choice: `true` builds
    /// [`ScaleMode::SignedLog`] in [`apply`].
    pub signed_log: bool,
    /// Half-width used when [`signed_log`] is on.
    pub signed_log_half_width: f32,
    /// Cached auto-fit bounds supplied by the app each frame.
    pub color_stats: ColorStats,
    /// Cached clip counts supplied by the app each frame.
    pub color_clips: ClipCounts,
    /// One-shot request to commit the effective range and turn auto-fit off.
    pub freeze_requested: bool,
    pub auto_fit: bool,
    pub color_min: f32,
    pub color_max: f32,
    /// Session-only custom palette draft (issue #83 slice C2). `None` until
    /// the user starts editing the current effective ramp.
    pub palette_working: Option<ColorRamp>,
    /// Portable versioned-JSON buffer for palette import/export.
    pub palette_text: String,
    /// Last palette success/error text shown under the editor.
    pub palette_message: String,
    /// One-shot request consumed by [`apply`] to install the draft (or clear
    /// the override when the draft is `None`).
    pub palette_apply_requested: bool,
    /// Session-only named-parameter drafts for the Variables section.
    pub variables: VariablesControls,
    pub streamlines: bool,
    /// Which streamline seed sources are shown while the Feature is on.
    pub seed_mode: StreamlineSeedMode,
    /// Explicit streamline seed count; `0` means Auto (density-derived).
    pub streamline_count: u32,
    /// Session-only explicit-seed editor state (issue #87 slice S2). None of
    /// this is document state; the committed seeds live in `State`.
    pub seed_generators: SeedGeneratorControls,
    /// The last generator/placement rejection, shown inline under the editor.
    pub seed_error: String,
    /// Armed "Place seed" toggle: the next viewport click plants one seed.
    pub place_seed: bool,
    /// Index of the seed selected for numeric editing and viewport dragging.
    pub selected_seed: Option<usize>,
    /// Index of the seed whose viewport drag is in progress.
    pub seed_drag: Option<usize>,
    /// Whether the in-progress seed gesture has actually moved; a click that
    /// only selects a seed must not snap or record it.
    pub seed_drag_moved: bool,
    /// The save-current name draft for the named seed-layout library.
    pub seed_layout_name: String,
    /// The cached portable seed-layout export, refreshed by Export and after
    /// every successful library mutation.
    pub seed_layout_text: String,
    /// The import draft for the portable seed-layout document.
    pub seed_layout_import: String,
    /// The last named-layout result or rejection, kept distinct from
    /// [`seed_error`] (generator and placement failures).
    pub seed_layout_message: String,
    /// The user layout awaiting the second delete click.
    pub pending_layout_delete: Option<String>,
    /// What one explicit left click does (ticket 60).
    pub left_click_action: LeftClickAction,
    pub particles: bool,
    /// Dot radius multiplier on the grid spacing (ticket 30).
    pub particle_size: f32,
    /// Explicit particle count; `0` means Auto (density x 16) (ticket 30).
    pub particle_count: u32,
    /// Per-particle appearance: opacity, colour mode, trails and stagnation
    /// looping (ticket 48).
    pub particle_appearance: ParticleAppearance,
    pub domain_min: Vec3,
    pub domain_max: Vec3,
    /// Plane-by-plane mirror (ticket 23). The index is only handed to the
    /// seam when the slider is released, so a drag never resamples the field.
    pub plane: PlaneView,
    /// True while the plane slider is being dragged: the mirror must keep the
    /// in-flight value instead of following the state.
    pub plane_drag: bool,
    /// The Camera section's fly toggle (ticket 65). The app mirrors the active
    /// camera into this every frame and reconciles a user edit through
    /// `fly_camera::sync_fly_mirror`; the sidebar itself never owns fly state.
    pub fly: bool,
    pub sensitivity: f32,
    /// Scalar-view grid side (issue #92 slice B1): 16..=160 in steps of 8.
    pub scalar_grid_side: u32,
    /// Explicit contour-level count; `0` hides contours.
    pub scalar_contour_levels: u32,
    /// Armed "Place probe" toggle: the next viewport click plants one probe.
    pub scalar_place_probe: bool,
    /// The last Scalar view rejection, shown inline under the controls.
    pub scalar_message: String,
}

impl Controls {
    /// The knobs at the viewmodel defaults: density 24 (the 2D default — the
    /// mode toggle re-instances anyway), arrow scale 1, viridis, auto-fit
    /// range, traces off, the default domain box.
    pub fn new() -> Controls {
        let domain = Domain::default();
        Controls {
            density: DEFAULT_DENSITY_2D,
            arrow_scale: 1.0,
            arrow_opacity: MAX_ARROW_OPACITY,
            plane_dim_opacity: DIM_ALPHA,
            arrow_length_mode: ArrowLengthMode::Proportional,
            magnitude_scale: DEFAULT_MAGNITUDE_SCALE,
            proportional_exponent: DEFAULT_PROPORTIONAL_EXPONENT,
            arrows: true,
            show_grid: true,
            show_coordinate_system: true,
            arrow_style: ArrowStyle::Cone,
            arrow_style_2d: ArrowStyle2D::Flat,
            arrow_params: ArrowParams::default(),
            colormap: Colormap::Viridis,
            color_quantity: ColorQuantity::Magnitude,
            color_mode: ScaleMode::Linear,
            signed_log: false,
            signed_log_half_width: 1.0,
            color_stats: ColorStats::default(),
            color_clips: ClipCounts::default(),
            freeze_requested: false,
            auto_fit: true,
            color_min: 0.0,
            color_max: 1.0,
            palette_working: None,
            palette_text: String::new(),
            palette_message: String::new(),
            palette_apply_requested: false,
            variables: VariablesControls::default(),
            streamlines: false,
            seed_mode: StreamlineSeedMode::Auto,
            streamline_count: 0,
            seed_generators: SeedGeneratorControls::default(),
            seed_error: String::new(),
            place_seed: false,
            selected_seed: None,
            seed_drag: None,
            seed_drag_moved: false,
            seed_layout_name: String::new(),
            seed_layout_text: String::new(),
            seed_layout_import: String::new(),
            seed_layout_message: String::new(),
            pending_layout_delete: None,
            left_click_action: LeftClickAction::SeedStreamline,
            particles: false,
            particle_size: DEFAULT_PARTICLE_SIZE,
            particle_count: 0,
            particle_appearance: ParticleAppearance::default(),
            domain_min: domain.min,
            domain_max: domain.max,
            plane: PlaneView::default(),
            plane_drag: false,
            fly: false,
            sensitivity: 1.0,
            scalar_grid_side: DEFAULT_SCALAR_GRID_SIDE,
            scalar_contour_levels: DEFAULT_SCALAR_CONTOUR_LEVELS,
            scalar_place_probe: false,
            scalar_message: String::new(),
        }
    }

    /// Restore the Display section's defaults (density, arrow length mode,
    /// both scales, arrow and off-plane opacity, the proportional contrast,
    /// arrow and reference visibility/style/appearance, plane-by-plane).
    pub fn reset_display(&mut self) {
        let defaults = Controls::new();
        self.density = defaults.density;
        self.arrow_scale = defaults.arrow_scale;
        self.arrow_opacity = defaults.arrow_opacity;
        self.plane_dim_opacity = defaults.plane_dim_opacity;
        self.arrow_length_mode = defaults.arrow_length_mode;
        self.magnitude_scale = defaults.magnitude_scale;
        self.proportional_exponent = defaults.proportional_exponent;
        self.arrows = defaults.arrows;
        self.show_grid = defaults.show_grid;
        self.show_coordinate_system = defaults.show_coordinate_system;
        self.arrow_style = defaults.arrow_style;
        self.arrow_style_2d = defaults.arrow_style_2d;
        self.arrow_params = defaults.arrow_params;
        self.plane = defaults.plane;
        self.plane_drag = false;
    }

    /// Restore the Features section's defaults (traces off, Auto seed mode).
    /// Speed is not a Features control since ticket 26: it belongs to the
    /// transport's clock, so this reset leaves it alone.
    pub fn reset_features(&mut self) {
        let defaults = Controls::new();
        self.streamlines = defaults.streamlines;
        self.seed_mode = defaults.seed_mode;
        self.streamline_count = defaults.streamline_count;
        self.left_click_action = defaults.left_click_action;
        self.particles = defaults.particles;
        self.particle_size = defaults.particle_size;
        self.particle_count = defaults.particle_count;
        self.particle_appearance = defaults.particle_appearance;
    }

    /// Restore the Coloring section's defaults (viridis, magnitude, linear,
    /// auto-fit, 0..1, cleared cached/one-shot state).
    ///
    /// Resetting the built-in to viridis also clears an active custom palette
    /// on the next [`apply`] whenever the built-in actually changed: `apply`
    /// calls `set_colormap`, which discards the override. An override survives
    /// the reset only when the built-in was already viridis, so the selection
    /// does not move.
    pub fn reset_coloring(&mut self) {
        let defaults = Controls::new();
        self.colormap = defaults.colormap;
        self.color_quantity = defaults.color_quantity;
        self.color_mode = defaults.color_mode;
        self.signed_log = defaults.signed_log;
        self.signed_log_half_width = defaults.signed_log_half_width;
        self.color_stats = defaults.color_stats;
        self.color_clips = defaults.color_clips;
        self.freeze_requested = defaults.freeze_requested;
        self.auto_fit = defaults.auto_fit;
        self.color_min = defaults.color_min;
        self.color_max = defaults.color_max;
        self.palette_working = defaults.palette_working;
        self.palette_text = defaults.palette_text;
        self.palette_message = defaults.palette_message;
        self.palette_apply_requested = defaults.palette_apply_requested;
    }

    /// Restore the Domain section's defaults (the unit cube).
    pub fn reset_domain(&mut self) {
        let defaults = Controls::new();
        self.domain_min = defaults.domain_min;
        self.domain_max = defaults.domain_max;
    }

    /// Restore the Camera section's defaults: leave fly mode (which releases
    /// the pointer capture). Fly speed is not stored here — dropping the
    /// `FlyCamera` already restores `DEFAULT_SPEED` on the next entry.
    pub fn reset_camera(&mut self) {
        let defaults = Controls::new();
        self.fly = defaults.fly;
        self.sensitivity = defaults.sensitivity;
    }
}

/// The collapsed-by-default Scalar view section (issue #92 slice B1): the
/// bounded grid/contour controls, the point-probe list and the click-arming
/// toggle. It refreshes the scalar cache only while its body is open.
fn scalar_view_section(ui: &mut egui::Ui, state: &mut State, controls: &mut Controls) {
    egui::CollapsingHeader::new("Scalar view")
        .id_salt("scalar_view_section")
        .default_open(false)
        .show(ui, |ui| {
            let _ = state.refresh_scalar_view(true);
            let text = ui.visuals().text_color().to_array();
            state.set_scalar_text_color([
                f32::from(text[0]) / 255.0,
                f32::from(text[1]) / 255.0,
                f32::from(text[2]) / 255.0,
                1.0,
            ]);

            if let Some(reason) = scalar_view_unsupported_reason(state) {
                ui.label(&reason);
                controls.scalar_place_probe = false;
                // Keep the toggle visible but disabled so the section explains
                // why no heatmap can be painted instead of silently hiding a
                // persisted setting.
                let mut show_heatmap = state.show_scalar_heatmap();
                ui.add_enabled(
                    false,
                    egui::Checkbox::new(&mut show_heatmap, "Show heatmap"),
                )
                .on_hover_text(reason);
                return;
            }

            ui.horizontal(|ui| {
                ui.label("Grid resolution");
                ui.add(
                    egui::Slider::new(
                        &mut controls.scalar_grid_side,
                        MIN_SCALAR_GRID_SIDE..=MAX_GRID_SIDE,
                    )
                    .step_by(8.0),
                )
                .on_hover_text("Samples per grid side; 16..=160 in steps of 8");
            });
            ui.horizontal(|ui| {
                ui.label("Contour levels");
                ui.add(egui::Slider::new(
                    &mut controls.scalar_contour_levels,
                    0..=MAX_CONTOUR_LEVELS as u32,
                ))
                .on_hover_text("Explicit marching-squares levels; 0 hides contours");
            });

            let mut show_heatmap = state.show_scalar_heatmap();
            if ui
                .checkbox(&mut show_heatmap, "Show heatmap")
                .on_hover_text("Fill painted cells with the active colour mapping")
                .changed()
            {
                state.set_show_scalar_heatmap(show_heatmap);
            }
            if show_heatmap {
                heatmap_legend_line(ui, state);
            }

            ui.checkbox(&mut controls.scalar_place_probe, "Place probe")
                .on_hover_text("Click the viewport plane to add one point probe");

            let mut remove = None;
            let domain = state.scene_domain();
            for (index, probe) in state.probes.points.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(probe_row_label(index, probe, &domain));
                    if ui
                        .button("Remove")
                        .on_hover_text("Remove this point probe")
                        .clicked()
                    {
                        remove = Some(index);
                    }
                });
            }
            if let Some(index) = remove
                && state.remove_point_probe(index)
            {
                controls.scalar_message.clear();
            }

            if !controls.scalar_message.is_empty() {
                ui.label(&controls.scalar_message);
            }
        });
}

/// Palette strip, effective range and clipping note for the painted heatmap
/// (issue #92 slice B2). Derived from the same record/colour mapping as the
/// GPU texture so the legend cannot describe a different field.
fn heatmap_legend_line(ui: &mut egui::Ui, state: &State) {
    let Some(legend) = state.scalar_heatmap_legend() else {
        ui.label("No finite heatmap cells on this slice");
        return;
    };
    let mapping = state.color_mapping();
    const STEPS: usize = 64;
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().min(220.0), 10.0),
        egui::Sense::hover(),
    );
    let painter = ui.painter();
    for step in 0..STEPS {
        let t = step as f32 / (STEPS - 1) as f32;
        let rgb = mapping.palette.sample_unit(t);
        let left = rect.left() + rect.width() * step as f32 / STEPS as f32;
        let right = rect.left() + rect.width() * (step + 1) as f32 / STEPS as f32;
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(left, rect.top()),
                egui::pos2(right + 0.5, rect.bottom()),
            ),
            0.0,
            egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]),
        );
    }
    ui.label(format!("{:.4} .. {:.4}", legend.min, legend.max));
    if legend.clipped_low > 0 || legend.clipped_high > 0 {
        ui.label(format!(
            "Clipped to the ramp endpoints: {} low, {} high",
            legend.clipped_low, legend.clipped_high
        ));
    }
}

/// The same defaults as [`Controls::new`], so the type honours clippy's
/// `new_without_default`: the app can reach its starting knobs either way.
impl Default for Controls {
    fn default() -> Controls {
        Controls::new()
    }
}

use crate::ui::layout;

/// Draw the sidebar and hand every edit to the viewmodel setters.
/// One row of three world-coordinate drag values.
fn seed_vec3_row(ui: &mut egui::Ui, label: &str, value: &mut Vec3) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::DragValue::new(&mut value.x).speed(0.05).prefix("x "));
        ui.add(egui::DragValue::new(&mut value.y).speed(0.05).prefix("y "));
        ui.add(egui::DragValue::new(&mut value.z).speed(0.05).prefix("z "));
    });
}

/// The local frame a stored anchor belongs to, re-clamped against the current
/// plane grid.
fn seed_anchor_frame(state: &State, anchor: &SeedAnchor) -> PlaneFrame {
    let grid = state.plane_grid();
    let plane = PlaneView {
        enabled: true,
        axis: anchor.axis,
        index: anchor.index,
    };
    PlaneFrame::new(&state.domain, plane.clamped(grid), grid)
}

/// The collapsed-by-default "Seed layout" editor inside Flow.
fn seed_layout_editor(
    ui: &mut egui::Ui,
    state: &mut State,
    controls: &mut Controls,
    layouts: &mut crate::seed_layout_library::SeedLayoutLibrary,
) {
    let count = state.seed_layout().len();
    ui.horizontal(|ui| {
        ui.label(seed_layout_count_label(count))
            .on_hover_text(format!("At most {MAX_SEEDS} explicit seeds"));
        if ui
            .add_enabled(count > 0, egui::Button::new("Clear all"))
            .clicked()
        {
            state.reset_seed_layout();
            controls.selected_seed = None;
            controls.seed_error.clear();
        }
    });
    ui.checkbox(&mut controls.place_seed, "Place seed")
        .on_hover_text("The next viewport click plants one explicit seed on the active plane");
    controls.selected_seed = controls.selected_seed.filter(|index| *index < count);
    controls.seed_drag = controls.seed_drag.filter(|index| *index < count);
    if !controls.seed_error.is_empty() {
        ui.label(egui::RichText::new(&controls.seed_error).color(ui.visuals().error_fg_color));
    }

    let max_generated = MAX_GENERATED_PER_APPLICATION as u32;
    let mut generated = None;
    {
        let generators = &mut controls.seed_generators;
        ui.separator();
        ui.label("Line");
        seed_vec3_row(ui, "From", &mut generators.line_from);
        seed_vec3_row(ui, "To", &mut generators.line_to);
        let from = generators.line_from;
        let to = generators.line_to;
        ui.horizontal(|ui| {
            ui.label("Count");
            ui.add(
                egui::DragValue::new(&mut generators.line_count)
                    .range(2..=max_generated)
                    .speed(1.0),
            );
            if ui.button("Add line").clicked() {
                let domain = state.scene_domain();
                generated = Some(line_seeds(&domain, from, to, generators.line_count));
            }
        });

        ui.separator();
        ui.label("Circle");
        seed_vec3_row(ui, "Centre", &mut generators.circle_center);
        ui.horizontal(|ui| {
            ui.label("Plane");
            for axis in PlaneAxis::ALL {
                ui.selectable_value(&mut generators.circle_axis, axis, axis.label());
            }
        });
        let center = generators.circle_center;
        let axis = generators.circle_axis;
        ui.horizontal(|ui| {
            ui.label("Radius");
            ui.add(
                egui::DragValue::new(&mut generators.circle_radius)
                    .range(0.0001..=1.0e6)
                    .speed(0.05),
            );
            ui.label("Count");
            ui.add(
                egui::DragValue::new(&mut generators.circle_count)
                    .range(3..=max_generated)
                    .speed(1.0),
            );
            if ui.button("Add circle").clicked() {
                let domain = state.scene_domain();
                generated = Some(circle_seeds(
                    &domain,
                    center,
                    axis,
                    generators.circle_radius,
                    generators.circle_count,
                ));
            }
        });

        ui.separator();
        ui.label("Region");
        seed_vec3_row(ui, "Min", &mut generators.region_min);
        seed_vec3_row(ui, "Max", &mut generators.region_max);
        let min = generators.region_min;
        let max = generators.region_max;
        ui.horizontal(|ui| {
            ui.label("Count");
            ui.add(
                egui::DragValue::new(&mut generators.region_count)
                    .range(1..=max_generated)
                    .speed(1.0),
            );
            if ui.button("Add region").clicked() {
                let domain = state.scene_domain();
                generated = Some(region_seeds(&domain, min, max, generators.region_count));
            }
        });
    }
    if let Some(result) = generated {
        match result {
            Ok(points) => match state.apply_generated_seeds(&points) {
                Ok(()) => controls.seed_error.clear(),
                Err(error) => controls.seed_error = error.to_string(),
            },
            Err(error) => controls.seed_error = error.to_string(),
        }
    }

    if count > 0 {
        egui::CollapsingHeader::new("Seeds")
            .default_open(false)
            .show(ui, |ui| {
                let mut next: Option<SeedLayout> = None;
                let seeds = state.seed_layout().seeds.clone();
                for (index, seed) in seeds.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let selected = controls.selected_seed == Some(index);
                        if ui
                            .selectable_label(selected, format!("Seed {}", index + 1))
                            .clicked()
                        {
                            controls.selected_seed = Some(index);
                        }
                        let mut edited = *seed;
                        let changed = if let Some(anchor) = seed.anchor {
                            let frame = seed_anchor_frame(state, &anchor);
                            let local = frame.to_local(seed.position);
                            let mut u = local.x;
                            let mut v = local.y;
                            let u_changed = ui
                                .add(egui::DragValue::new(&mut u).speed(0.01).prefix("u "))
                                .changed();
                            let v_changed = ui
                                .add(egui::DragValue::new(&mut v).speed(0.01).prefix("v "))
                                .changed();
                            if u_changed || v_changed {
                                edited = SeedPoint {
                                    position: frame.to_world(Vec3::new(u, v, 0.0)),
                                    anchor: Some(SeedAnchor { u, v, ..anchor }),
                                };
                                true
                            } else {
                                ui.label(format!(
                                    "world ({:.2}, {:.2}, {:.2})",
                                    seed.position.x, seed.position.y, seed.position.z
                                ));
                                false
                            }
                        } else {
                            let mut position = seed.position;
                            let changed = ui
                                .add(
                                    egui::DragValue::new(&mut position.x)
                                        .speed(0.01)
                                        .prefix("x "),
                                )
                                .changed()
                                | ui.add(
                                    egui::DragValue::new(&mut position.y)
                                        .speed(0.01)
                                        .prefix("y "),
                                )
                                .changed()
                                | ui.add(
                                    egui::DragValue::new(&mut position.z)
                                        .speed(0.01)
                                        .prefix("z "),
                                )
                                .changed();
                            if changed {
                                edited = SeedPoint {
                                    position,
                                    anchor: None,
                                };
                            }
                            changed
                        };
                        if changed {
                            let mut layout = state.seed_layout().clone();
                            layout.seeds[index] = edited;
                            next = Some(layout);
                        }
                        if ui.button("Delete").clicked() {
                            let mut layout = state.seed_layout().clone();
                            layout.seeds.remove(index);
                            next = Some(layout);
                            controls.selected_seed = None;
                        }
                    });
                }
                if let Some(layout) = next {
                    match state.commit_seed_layout(layout) {
                        Ok(()) => {
                            controls.seed_error.clear();
                            if controls
                                .selected_seed
                                .is_some_and(|index| index >= state.seed_layout().len())
                            {
                                controls.selected_seed = None;
                            }
                        }
                        Err(error) => controls.seed_error = error,
                    }
                }
            });
    }

    named_seed_layouts(ui, state, controls, layouts);
}

/// The "Named layouts" library inside the Seed layout editor. Apply replaces
/// the committed seeds through the one existing history seam; saving, deleting
/// and importing mutate only settings-side library data and never dirty the
/// document.
fn named_seed_layouts(
    ui: &mut egui::Ui,
    state: &mut State,
    controls: &mut Controls,
    layouts: &mut crate::seed_layout_library::SeedLayoutLibrary,
) {
    egui::CollapsingHeader::new("Named layouts")
        .id_salt("named_seed_layouts")
        .default_open(false)
        .show(ui, |ui| {
            let domain = state.scene_domain();
            let all = layouts.all(&domain);
            let mut apply: Option<SeedLayoutProfile> = None;
            let mut delete: Option<String> = None;
            for profile in &all {
                let builtin = profile.name == DEFAULT_LAYOUT_NAME;
                ui.horizontal_wrapped(|ui| {
                    ui.label(named_layout_row_label(profile));
                    if ui
                        .button("Apply")
                        .on_hover_text("Replace the explicit seeds with this layout")
                        .clicked()
                    {
                        apply = Some(profile.clone());
                    }
                    if !builtin {
                        let armed = controls.pending_layout_delete.as_deref()
                            == Some(profile.name.as_str());
                        let label = if armed { "Confirm delete" } else { "Delete" };
                        if ui
                            .button(label)
                            .on_hover_text("Delete this user layout")
                            .clicked()
                        {
                            delete = Some(profile.name.clone());
                        }
                    }
                });
            }

            if let Some(profile) = apply {
                apply_named_layout(state, controls, &profile);
            } else if let Some(name) = delete {
                delete_named_layout(controls, layouts, &name);
            }

            ui.horizontal_wrapped(|ui| {
                ui.label("Name");
                ui.add(
                    egui::TextEdit::singleline(&mut controls.seed_layout_name)
                        .hint_text("Layout name")
                        .desired_width(180.0),
                );
            });
            if ui
                .button("Save current")
                .on_hover_text("Save the committed explicit seeds under this name")
                .clicked()
            {
                save_named_layout(state, controls, layouts);
            }

            ui.horizontal_wrapped(|ui| {
                if ui.button("Export text").clicked() {
                    controls.seed_layout_text = layouts.export_text();
                }
                let can_copy = !layouts.is_empty();
                if ui
                    .add_enabled(can_copy, egui::Button::new("Copy"))
                    .on_hover_text("Copy the user layout document")
                    .clicked()
                {
                    if controls.seed_layout_text.is_empty() {
                        controls.seed_layout_text = layouts.export_text();
                    }
                    ui.ctx().copy_text(controls.seed_layout_text.clone());
                    controls.seed_layout_message = "Copied layout text".to_owned();
                }
            });
            ui.add(
                egui::TextEdit::multiline(&mut controls.seed_layout_text)
                    .desired_rows(2)
                    .desired_width(f32::INFINITY)
                    .interactive(false),
            );

            ui.label("Import layouts");
            ui.add(
                egui::TextEdit::multiline(&mut controls.seed_layout_import)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY)
                    .hint_text("Paste a seed layout document"),
            );
            if ui
                .button("Apply text")
                .on_hover_text("Merge the document into the user layouts atomically")
                .clicked()
            {
                import_named_layouts(state, controls, layouts);
            }

            if !controls.seed_layout_message.is_empty() {
                ui.label(&controls.seed_layout_message);
            }
        });
}

fn named_layout_row_label(profile: &SeedLayoutProfile) -> String {
    if profile.name == DEFAULT_LAYOUT_NAME {
        return format!("{DEFAULT_LAYOUT_NAME} (generated grid)");
    }
    match profile.seeds.len() {
        1 => format!("{} (1 seed)", profile.name),
        count => format!("{} ({count} seeds)", profile.name),
    }
}

fn apply_named_layout(state: &mut State, controls: &mut Controls, profile: &SeedLayoutProfile) {
    match state.commit_seed_layout(profile.seeds.clone()) {
        Ok(()) => {
            controls.seed_error.clear();
            controls.selected_seed = None;
            controls.seed_drag = None;
            controls.place_seed = false;
            controls.pending_layout_delete = None;
            controls.seed_layout_message = format!("Applied layout {:?}", profile.name);
        }
        Err(reason) => controls.seed_layout_message = reason,
    }
}

fn save_named_layout(
    state: &State,
    controls: &mut Controls,
    layouts: &mut crate::seed_layout_library::SeedLayoutLibrary,
) {
    let name = controls.seed_layout_name.trim().to_owned();
    let replaces = layouts.user().iter().any(|profile| profile.name == name);
    let profile = SeedLayoutProfile {
        name: name.clone(),
        seeds: state.seed_layout().clone(),
    };
    match layouts.upsert(profile, &state.scene_domain()) {
        Ok(()) => {
            let action = if replaces { "Replaced" } else { "Saved" };
            controls.seed_layout_name.clear();
            controls.pending_layout_delete = None;
            controls.seed_layout_text = layouts.export_text();
            controls.seed_layout_message = format!("{action} layout {name:?}");
        }
        Err(error) => controls.seed_layout_message = error,
    }
}

fn delete_named_layout(
    controls: &mut Controls,
    layouts: &mut crate::seed_layout_library::SeedLayoutLibrary,
    name: &str,
) {
    if controls.pending_layout_delete.as_deref() != Some(name) {
        controls.pending_layout_delete = Some(name.to_owned());
        controls.seed_layout_message = format!("Click Delete again to remove {name:?}");
        return;
    }

    controls.pending_layout_delete = None;
    if layouts.remove(name) {
        controls.seed_layout_text = layouts.export_text();
        controls.seed_layout_message = format!("Deleted layout {name:?}");
    } else {
        controls.seed_layout_message = format!("Layout {name:?} was not found");
    }
}

fn import_named_layouts(
    state: &State,
    controls: &mut Controls,
    layouts: &mut crate::seed_layout_library::SeedLayoutLibrary,
) {
    let before = layouts.len();
    match layouts.import_text(&controls.seed_layout_import, &state.scene_domain()) {
        Ok(after) => {
            let added = after.saturating_sub(before);
            if added == 0
                && SeedLayoutDocument::from_json(&controls.seed_layout_import)
                    .is_ok_and(|document| document.layouts.is_empty())
            {
                // An empty document changes nothing, so leave the cached
                // export and any armed delete exactly as they were.
                controls.seed_layout_message = "Nothing to import".to_owned();
                return;
            }
            let message = match added {
                0 => "Import replaced existing layouts; no new layouts imported".to_owned(),
                1 => "Imported 1 new layout".to_owned(),
                count => format!("Imported {count} new layouts"),
            };
            controls.pending_layout_delete = None;
            controls.seed_layout_text = layouts.export_text();
            controls.seed_layout_message = message;
        }
        Err(error) => controls.seed_layout_message = error,
    }
}

pub fn show(
    ui: &mut egui::Ui,
    state: &mut State,
    controls: &mut Controls,
    layouts: &mut crate::seed_layout_library::SeedLayoutLibrary,
) {
    let _ = show_with_plot_action(ui, state, controls, layouts, false);
}

/// Wide entry that also reports a Plot action the app must run (Fit plot).
pub fn show_with_plot_action(
    ui: &mut egui::Ui,
    state: &mut State,
    controls: &mut Controls,
    layouts: &mut crate::seed_layout_library::SeedLayoutLibrary,
    compact: bool,
) -> Option<crate::ui::plot_panel::PlotPanelAction> {
    show_inner(ui, state, controls, layouts, compact)
}

/// Compact-mode entry: applies the compact row floor and spacing, then draws.
pub fn show_compact(
    ui: &mut egui::Ui,
    state: &mut State,
    controls: &mut Controls,
    layouts: &mut crate::seed_layout_library::SeedLayoutLibrary,
    compact: bool,
) {
    let _ = show_compact_with_plot_action(ui, state, controls, layouts, compact);
}

/// Compact entry that also reports a Plot action the app must run.
pub fn show_compact_with_plot_action(
    ui: &mut egui::Ui,
    state: &mut State,
    controls: &mut Controls,
    layouts: &mut crate::seed_layout_library::SeedLayoutLibrary,
    compact: bool,
) -> Option<crate::ui::plot_panel::PlotPanelAction> {
    if compact {
        ui.spacing_mut().interact_size.y = layout::COMPACT_ROW_HEIGHT;
        ui.spacing_mut().item_spacing.y = layout::COMPACT_ITEM_SPACING;
    }
    show_inner(ui, state, controls, layouts, compact)
}

fn show_inner(
    ui: &mut egui::Ui,
    state: &mut State,
    controls: &mut Controls,
    layouts: &mut crate::seed_layout_library::SeedLayoutLibrary,
    compact: bool,
) -> Option<crate::ui::plot_panel::PlotPanelAction> {
    // The Plot section is drawn first and may switch kind within this frame;
    // every later scalar/vector gate reads the resulting state, not a stale
    // pre-click snapshot.
    let plot_action = crate::ui::plot_panel::show(ui, state, compact);

    // The density ceiling keys to the dimension being sampled (the ledger
    // ruling): n³ only when the mode is 3D and the field carries z.
    let sampling_3d = state.is_3d();
    let view_3d = state.mode_3d;
    let field_dimension = state.field.dimension;
    let plane_mode = plane_control_mode(field_dimension, view_3d);
    let max_density = if sampling_3d {
        MAX_DENSITY_3D
    } else {
        MAX_DENSITY_2D
    };
    let domain = state.domain;
    let vector_mode = state.is_vector_mode();
    let coloring_visible = coloring_controls_visible(state.field_mode, state.sources.display);
    let color_mapping = state.color_mapping();
    let color_explanation = color_mapping.explanation(&controls.color_stats);
    let freeze_range = color_mapping.effective_range(&controls.color_stats);
    let palette_effective = state
        .color_ramp
        .clone()
        .unwrap_or_else(|| ColorRamp::builtin(state.colormap));
    let palette_custom_active = state.color_ramp.is_some();
    let palette_builtin_label = state.colormap.label();

    // The plane mirror follows the seam (which clamps/recentres it on grid
    // changes) except while its slider is mid-drag: the in-flight value has to
    // survive the frame so the slider does not snap back. The seam is only
    // handed the new index when the drag ends, so dragging never resamples.
    if !controls.plane_drag {
        controls.plane = state.plane;
    }
    let mut commit_plane = false;

    section(ui, Section::Display, controls, None, |ui, controls| {
        if !vector_mode {
            ui.label(
                "Vector-only display controls are disabled while plotting; their values are retained.",
            );
        }
        ui.add_enabled_ui(vector_mode, |ui| {
            ui.horizontal(|ui| {
                ui.label(if sampling_3d {
                    "Density (n³)"
                } else {
                    "Density (n²)"
                });
                ui.add(
                    egui::DragValue::new(&mut controls.density).range(MIN_DENSITY..=max_density),
                )
                .on_hover_text("Samples per axis; 3D sampling makes it the cube of this count");
            });
            ui.checkbox(&mut controls.arrows, "Show arrows")
                .on_hover_text("Hide the arrow field without hiding streamlines or particles");
        });
        ui.checkbox(&mut controls.show_grid, "Show grid")
            .on_hover_text("Show the plane/face grid and domain box");
        ui.checkbox(
            &mut controls.show_coordinate_system,
            "Show coordinate system",
        )
        .on_hover_text(
            "Show colored X/Y axes (and Z in 3D); these reference axes are not field lines",
        );
        ui.add_enabled_ui(vector_mode, |ui| {
            let arrow_scale_on = arrow_scale_enabled(controls.arrow_length_mode);
            let arrow_scale_text = if arrow_scale_on {
                "Arrow length (grid cells)"
            } else {
                "Arrow length (Proportional/Uniform)"
            };
            let arrow_scale_hover = if arrow_scale_on {
                "Arrow length as a multiple of one grid cell"
            } else {
                "Disabled in Magnitude mode; use Magnitude scale instead"
            };
            ui.add_enabled_ui(arrow_scale_on, |ui| {
                ui.add(
                    egui::Slider::new(&mut controls.arrow_scale, MIN_ARROW_SCALE..=MAX_ARROW_SCALE)
                        .text(arrow_scale_text),
                );
                ui.horizontal(|ui| {
                    ui.label("Exact length");
                    ui.add(
                        egui::DragValue::new(&mut controls.arrow_scale)
                            .speed(0.01)
                            .range(MIN_ARROW_SCALE..=MAX_ARROW_SCALE)
                            .max_decimals(2),
                    );
                });
            })
            .response
            .on_hover_text(arrow_scale_hover)
            .on_disabled_hover_text("Disabled in Magnitude mode; use Magnitude scale instead");
            ui.add(
                egui::Slider::new(
                    &mut controls.arrow_opacity,
                    MIN_ARROW_OPACITY..=MAX_ARROW_OPACITY,
                )
                .text("Arrow opacity")
                .custom_formatter(|value, _| format!("{:.0}%", value * 100.0)),
            )
            .on_hover_text(
                "Opacity of the whole arrow field; 100% is fully solid. Dense fields can be \
             de-emphasised without hiding them",
            );
            button_row(ui, |ui| {
                ui.label("Length mode");
                for mode in ArrowLengthMode::ALL {
                    ui.selectable_value(&mut controls.arrow_length_mode, mode, mode.label())
                        .on_hover_text(mode.tooltip());
                }
            });
            ui.add_enabled_ui(magnitude_scale_enabled(controls.arrow_length_mode), |ui| {
                ui.horizontal(|ui| {
                    ui.label("Magnitude scale");
                    ui.add(
                        egui::DragValue::new(&mut controls.magnitude_scale)
                            .speed(0.01)
                            .range(MIN_MAGNITUDE_SCALE..=MAX_MAGNITUDE_SCALE)
                            .max_decimals(2),
                    );
                });
            })
            .response
            .on_hover_text("World length per unit of field magnitude");
            ui.add_enabled_ui(
                proportional_exponent_enabled(controls.arrow_length_mode),
                |ui| {
                    ui.add(
                        egui::Slider::new(
                            &mut controls.proportional_exponent,
                            MIN_PROPORTIONAL_EXPONENT..=MAX_PROPORTIONAL_EXPONENT,
                        )
                        .text("Proportional contrast"),
                    );
                },
            )
            .response
            .on_hover_text("How strongly arrow length follows magnitude; 1 = linear")
            .on_disabled_hover_text("Only Proportional arrow length follows magnitude");
            button_row(ui, |ui| {
                let set = arrow_style_set(view_3d);
                ui.label(set.label());
                match set {
                    ArrowStyleSet::ThreeD => {
                        for style in ArrowStyle::ALL {
                            ui.selectable_value(&mut controls.arrow_style, style, style.label())
                                .on_hover_text(style.tooltip());
                        }
                    }
                    ArrowStyleSet::TwoD => {
                        for style in ArrowStyle2D::ALL {
                            ui.selectable_value(&mut controls.arrow_style_2d, style, style.label())
                                .on_hover_text(style.tooltip());
                        }
                    }
                }
            });
            egui::CollapsingHeader::new("Arrow appearance")
                .default_open(false)
                .show(ui, |ui| {
                    let head_enabled = head_control_enabled(
                        view_3d,
                        controls.arrow_style,
                        controls.arrow_style_2d,
                    );
                    ui.add_enabled(
                        head_enabled,
                        egui::Slider::new(
                            &mut controls.arrow_params.head_length,
                            MIN_HEAD_LENGTH..=MAX_HEAD_LENGTH,
                        )
                        .text("Head length"),
                    )
                    .on_hover_text("Arrow head length as a fraction of the total arrow length")
                    .on_disabled_hover_text("This arrow family has no arrowhead");
                    ui.add_enabled(
                        head_enabled,
                        egui::Slider::new(
                            &mut controls.arrow_params.head_width,
                            MIN_HEAD_WIDTH..=MAX_HEAD_WIDTH,
                        )
                        .text("Head width"),
                    )
                    .on_hover_text("Arrow head width as a fraction of the total arrow length")
                    .on_disabled_hover_text("This arrow family has no arrowhead");
                    ui.add(
                        egui::Slider::new(
                            &mut controls.arrow_params.shaft_radius,
                            MIN_SHAFT_RADIUS..=MAX_SHAFT_RADIUS,
                        )
                        .text("Shaft radius"),
                    )
                    .on_hover_text("Shaft radius as a fraction of the total arrow length");
                    ui.add_enabled(
                        sides_control_enabled(
                            view_3d,
                            controls.arrow_style,
                            controls.arrow_style_2d,
                        ),
                        egui::Slider::new(&mut controls.arrow_params.sides, MIN_SIDES..=MAX_SIDES)
                            .text("Sides"),
                    )
                    .on_hover_text("Radial side count for the active arrow family")
                    .on_disabled_hover_text("This arrow family has a fixed silhouette");
                    if ui
                        .button("Reset arrow appearance")
                        .on_hover_text("Restore the default head and shaft geometry")
                        .clicked()
                    {
                        controls.arrow_params = ArrowParams::default();
                    }
                });
            if plane_mode != PlaneControlMode::Hidden {
                commit_plane |=
                    plane_controls(ui, controls, &domain, state.plane_grid(), plane_mode);
            }
        });
    });

    // Camera (ticket 65): view controls sit between Display and Features. The
    // fly toggle keys to the sampled dimension (`is_3d`), the same gate as
    // `toggle_fly`: a 2D formula in 3D mode must stay inert. The app mirrors
    // the real camera into `controls.fly` each frame, so C, Esc, a 2D switch,
    // Reset/Fit and presets all land here on the next frame.
    section(ui, Section::Camera, controls, None, |ui, controls| {
        if compact {
            controls.fly = false;
            ui.label("Fly camera is unavailable in Compact")
                .on_hover_text("Fly camera is available in Wide layouts only");
        } else {
            if !vector_mode {
                ui.label("Fly camera is vector-only; the plot has its own navigation.");
            }
            ui.add_enabled_ui(sampling_3d && vector_mode, |ui| {
                ui.checkbox(&mut controls.fly, "Fly camera")
                    .on_hover_text("Free flight in the 3D view (C); Esc releases the mouse");
                // Sensitivity slider
                ui.add(
                    egui::Slider::new(&mut controls.sensitivity, 0.1..=5.0)
                        .text("Look Sensitivity"),
                )
                .on_hover_text("Change mouse look sensitivity for fly mode (default 1)")
            })
            .response
            .on_disabled_hover_text("3D only");
        }
        if controls.fly && sampling_3d {
            ui.label("Flying — Esc to release");
        }
    });

    if state.field_mode == crate::source_scene::FieldMode::Sources {
        ui.label("Streamlines and particles use the formula field; switch to Formula to use them.");
    } else {
        let induction_mode = state.field_mode == crate::source_scene::FieldMode::Induction;
        let ode_mode = state.field_mode == crate::source_scene::FieldMode::Ode;
        let materials_mode = state.field_mode == crate::source_scene::FieldMode::Materials;
        let maxwell_mode = state.field_mode == crate::source_scene::FieldMode::Maxwell;
        if induction_mode {
            ui.label(
                "Arrows only: streamlines, particles and click seeding stay off in Induction.",
            );
        } else if ode_mode {
            ui.label("Streamlines, particles and click seeding stay off in Ode.");
        } else if materials_mode {
            ui.label(crate::ui::material_panel::MATERIALS_ONLY_NOTE);
        } else if maxwell_mode {
            ui.label(crate::ui::maxwell_panel::MAXWELL_ONLY_NOTE);
        }
        ui.add_enabled_ui(
            !(induction_mode || ode_mode || materials_mode || maxwell_mode),
            |ui| {
            section(ui, Section::Features, controls, None, |ui, controls| {
                if !vector_mode {
                    ui.label("Streamlines and particles are vector-only; their values are retained.");
                }
                ui.add_enabled_ui(vector_mode, |ui| {
            ui.checkbox(&mut controls.streamlines, "Streamlines").on_hover_text(
                "Auto adds grid seeds; Click shows only click seeds. Off hides both and disables click seeding.",
            );
            ui.add_enabled_ui(seed_mode_enabled(controls.streamlines), |ui| {
                button_row(ui, |ui| {
                    ui.label("Seed");
                    for mode in StreamlineSeedMode::ALL {
                        ui.selectable_value(&mut controls.seed_mode, mode, mode.label())
                            .on_hover_text(mode.tooltip());
                    }
                });
            })
            .response
            .on_disabled_hover_text("Turn Streamlines on to choose Auto or Click seeding");
            // The explicit count only replaces Auto's density-derived lattice:
            // Click-only seeding paints click bursts instead, and an explicit
            // layout replaces the count itself, so each case is disabled rather
            // than silently ignored.
            let explicit_layout = !state.seed_layout().is_empty();
            ui.add_enabled_ui(
                streamline_count_enabled_with_layout(
                    controls.streamlines,
                    controls.seed_mode,
                    !explicit_layout,
                ),
                |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Streamline count");
                        ui.add(
                            egui::DragValue::new(&mut controls.streamline_count)
                                .range(0..=MAX_STREAMLINES)
                                .speed(1.0)
                                .custom_formatter(|value, _| {
                                    streamline_count_label(value.max(0.0).round() as u32)
                                }),
                        )
                        .on_hover_text(
                            "0 = Auto (density-derived); a value seeds exactly that many lines.",
                        );
                    });
                },
            )
            .response
            .on_disabled_hover_text(if explicit_layout {
                SEED_LAYOUT_DENSITY_INACTIVE_NOTE
            } else {
                "Click-only seeding uses click bursts; turn Streamlines on and choose Auto to set a count"
            });
            if explicit_layout {
                ui.label(SEED_LAYOUT_DENSITY_INACTIVE_NOTE);
            }
            // Ticket 60: the left click action stays visible even while its feature
            // is off; the click is then a hinted no-op rather than a dead control.
            button_row(ui, |ui| {
                ui.label("Left click");
                for action in LeftClickAction::ALL {
                    let hover = match click_action_plan(
                        action,
                        controls.streamlines,
                        controls.particles && state.field.dimension == 3,
                        view_3d,
                    ) {
                        ClickActionPlan::Ignore(reason) => {
                            format!("{} — {reason}", action.tooltip())
                        }
                        _ => action.tooltip().to_owned(),
                    };
                    ui.selectable_value(&mut controls.left_click_action, action, action.label())
                        .on_hover_text(hover);
                }
            });
            ui.checkbox(&mut controls.particles, "Particles")
                .on_hover_text(
                    "Animate dots carried by the field; 2D needs a three-component field and \
                     shows the selected slice",
                );
            ui.horizontal(|ui| {
                ui.label("Particle count");
                ui.add(
                    egui::DragValue::new(&mut controls.particle_count)
                        .range(0..=MAX_PARTICLES as u32)
                        .speed(1.0)
                        .custom_formatter(|value, _| {
                            particle_count_label(value.max(0.0).round() as u32)
                        }),
                )
                .on_hover_text("0 = Auto (density × 16); a value overrides it.");
            });
            // Ticket 48: appearance lives in its own nested subsection so the
            // Features body stays a pair of feature toggles. Size (ticket 30) moves
            // in here rather than staying as a second, duplicate control.
            egui::CollapsingHeader::new("Particle appearance")
                .default_open(false)
                .show(ui, |ui| {
                    ui.add_enabled_ui(controls.particles, |ui| {
                        let appearance = &mut controls.particle_appearance;
                        ui.add_enabled_ui(particle_size_enabled(controls.particles), |ui| {
                            ui.horizontal(|ui| {
                                ui.label("Size");
                                ui.add(
                                    egui::Slider::new(
                                        &mut controls.particle_size,
                                        MIN_PARTICLE_SIZE..=MAX_PARTICLE_SIZE,
                                    )
                                    .fixed_decimals(2),
                                )
                                .on_hover_text(
                                    "Dot radius relative to the grid spacing (0.25–3.0, default 1.0)",
                                );
                            });
                        });
                        ui.add(
                            egui::Slider::new(
                                &mut appearance.opacity,
                                MIN_PARTICLE_OPACITY..=MAX_PARTICLE_OPACITY,
                            )
                            .text("Opacity"),
                        )
                        .on_hover_text("Dot and trail alpha (0.05–1.0, default 1.0)");
                        button_row(ui, |ui| {
                            ui.label("Colour");
                            for mode in ParticleColorMode::ALL {
                                ui.selectable_value(
                                    &mut appearance.color_mode,
                                    mode,
                                    mode.label(),
                                )
                                .on_hover_text(mode.tooltip());
                            }
                        });
                        let mut fixed = fixed_color_rgb(appearance.fixed_color);
                        let picker = ui.add_enabled_ui(
                            particle_fixed_color_enabled(appearance.color_mode),
                            |ui| {
                                ui.horizontal(|ui| {
                                    ui.label("Fixed colour");
                                    super::style_panel::color_button(ui, |ui| ui.color_edit_button_srgb(&mut fixed))
                                })
                                .inner
                            },
                        );
                        if picker.inner.changed() {
                            appearance.fixed_color = fixed_color_opaque(fixed);
                        }
                        picker.response.on_hover_text(
                            "Colour used by the Fixed mode (default teal)",
                        ).on_disabled_hover_text(
                            "Choose Fixed mode to pick a colour",
                        );
                        ui.add(
                            egui::Slider::new(
                                &mut appearance.trail_len,
                                0..=MAX_TRAIL_LEN,
                            )
                            .integer()
                            .text("Trail length"),
                        )
                        .on_hover_text("Stored trail points per particle; 0 = off (0–32)");
                        ui.add_enabled(
                            trail_fade_enabled(appearance.trail_len),
                            egui::Slider::new(
                                &mut appearance.trail_fade,
                                MIN_TRAIL_FADE..=MAX_TRAIL_FADE,
                            )
                            .text("Trail fade"),
                        )
                        .on_hover_text(
                            "Alpha multiplier toward the tail (0–1, default 0.65); 1 = no fade",
                        )
                        .on_disabled_hover_text("Turn trails on (length 2 or more) to set the fade");
                        ui.add_enabled(
                            trail_width_enabled(appearance.trail_len),
                            egui::Slider::new(
                                &mut appearance.trail_width,
                                MIN_TRAIL_WIDTH..=MAX_TRAIL_WIDTH,
                            )
                            .text("Trail width"),
                        )
                        .on_hover_text(
                            "Trail width relative to the dot spacing (0.05–2.0, default 1.0)",
                        )
                        .on_disabled_hover_text("Turn trails on (length 2 or more) to set the width");
                        egui::CollapsingHeader::new("Advanced")
                            .default_open(false)
                            .show(ui, |ui| {
                                ui.checkbox(
                                    &mut appearance.loop_on_stagnation,
                                    "Loop when stuck",
                                )
                                .on_hover_text(
                                    "Re-spawn a particle that stops moving (default on)",
                                );
                                ui.add_enabled(
                                    stagnation_seconds_enabled(
                                        appearance.loop_on_stagnation,
                                    ),
                                    egui::Slider::new(
                                        &mut appearance.stagnation_seconds,
                                        MIN_STAGNATION_SECONDS..=MAX_STAGNATION_SECONDS,
                                    )
                                    .text("Stuck time (s)"),
                                )
                                .on_hover_text(
                                    "How long a particle may sit still before it loops (0.1–5.0 s, default 0.5)",
                                )
                                .on_disabled_hover_text(
                                    "Turn Loop when stuck on to set the stuck time",
                                );
                            });
                    })
                    .response
                    .on_disabled_hover_text("Turn Particles on to change their appearance");
                });
            });
            if seed_layout_visible(state.field_mode, controls.streamlines, vector_mode) {
                egui::CollapsingHeader::new("Seed layout")
                    .default_open(false)
                    .show(ui, |ui| seed_layout_editor(ui, state, controls, layouts));
            } else {
                // The editor is the only place the user can arm placement. If
                // it disappears (Streamlines off or leaving the formula/vector
                // context), a stale armed flag must not plant a seed later.
                controls.place_seed = false;
            }
        });
        });
    }

    // Issue #89 slice C: the named-parameter editor lives beside the formula
    // sections and only exists for the formula field in a vector view.
    if variables_visible(state.field_mode, vector_mode) {
        variables_section(ui, state, controls);
    }

    if state.field_mode != crate::source_scene::FieldMode::Ode
        && state.field_mode != crate::source_scene::FieldMode::Materials
        && state.field_mode != crate::source_scene::FieldMode::Maxwell
    {
        section(ui, Section::Coloring, controls, None, |ui, controls| {
            if !coloring_visible {
                ui.label(BOTH_COLORING_NOTE);
                return;
            }
            if !vector_mode {
                ui.label(
                    "Vector magnitude range is disabled while plotting; plot colours live in Plot.",
                );
            }
            ui.add_enabled_ui(vector_mode, |ui| {
                button_row(ui, |ui| {
                    ui.label("Colormap");
                    for colormap in Colormap::ALL {
                        let response =
                            ui.selectable_value(&mut controls.colormap, colormap, colormap.label());
                        // Choosing a built-in clears any custom override. The
                        // request covers the same-colormap click, where the
                        // value does not change and `set_colormap` would be
                        // skipped to preserve the override.
                        if response.clicked() && palette_custom_active {
                            controls.palette_working = None;
                            controls.palette_apply_requested = true;
                        }
                        response.on_hover_text("Color map for field magnitude");
                    }
                });
                button_row(ui, |ui| {
                    ui.label("Quantity");
                    for quantity in ColorQuantity::ALL {
                        let enabled = quantity != ColorQuantity::Direction2d || !sampling_3d;
                        ui.add_enabled_ui(enabled, |ui| {
                            ui.selectable_value(
                                &mut controls.color_quantity,
                                quantity,
                                quantity.label(),
                            )
                        })
                        .response
                        .on_hover_text("What the colour ramp represents")
                        .on_disabled_hover_text(DIRECTION_3D_EXPLANATION);
                    }
                });
                if sampling_3d {
                    ui.label(DIRECTION_3D_EXPLANATION);
                }
                button_row(ui, |ui| {
                    ui.label("Scale");
                    ui.selectable_value(&mut controls.signed_log, false, "Linear")
                        .on_hover_text("Uniform mapping across the colour range");
                    ui.selectable_value(&mut controls.signed_log, true, "Signed log")
                        .on_hover_text("Logarithmic either side of zero");
                });
                if controls.signed_log {
                    ui.horizontal(|ui| {
                        ui.label("Signed log half-width");
                        ui.add(
                            egui::DragValue::new(&mut controls.signed_log_half_width)
                                .speed(0.05)
                                .range(MIN_SIGNED_LOG_HALF_WIDTH..=MAX_SIGNED_LOG_HALF_WIDTH)
                                .custom_formatter(|value, _| {
                                    signed_log_half_width_display(value as f32)
                                }),
                        )
                        .on_hover_text(
                            "Width of the linear region around zero (finite, > 0, at most 1e6)",
                        );
                    });
                }
                if let Some(explanation) = &color_explanation {
                    ui.label(explanation);
                }
                let freeze_usable = controls.color_quantity != ColorQuantity::Direction2d
                    && freeze_range.0.is_finite()
                    && freeze_range.1.is_finite()
                    && freeze_range.0 < freeze_range.1;
                if ui
                    .add_enabled(freeze_usable, egui::Button::new("Freeze current range"))
                    .on_hover_text("Commit the effective range and turn auto-fit off")
                    .on_disabled_hover_text(
                        "Freezing needs a usable range and is unavailable for direction colouring",
                    )
                    .clicked()
                {
                    controls.freeze_requested = true;
                }
                color_legend(
                    ui,
                    &color_mapping,
                    &controls.color_stats,
                    controls.color_clips,
                );
                ui.checkbox(&mut controls.auto_fit, "Auto-fit color range")
                    .on_hover_text("Fit the color range to the sampled magnitudes");
                if !controls.auto_fit {
                    ui.horizontal(|ui| {
                        ui.label("Magnitude range:");
                        ui.add(egui::DragValue::new(&mut controls.color_min).speed(0.01))
                            .on_hover_text("Magnitude at the color ramp's low end");
                        ui.label("to");
                        ui.add(egui::DragValue::new(&mut controls.color_max).speed(0.01))
                            .on_hover_text("Magnitude at the color ramp's high end");
                    });
                }
                custom_palette_controls(
                    ui,
                    controls,
                    &palette_effective,
                    palette_custom_active,
                    palette_builtin_label,
                );
            });
        });
    }

    // Issue #92 slice B1: the scalar overlay section sits after Coloring. A
    // collapsed section must not sample; the open body performs the one
    // visible refresh.
    state.refresh_scalar_view(false);
    if scalar_section_visible(state.field_mode) {
        scalar_view_section(ui, state, controls);
    }

    // The Domain editor renders only the axes the current sampling dimension
    // can use; reversed drags are handed over as-is and the seam's per-axis
    // swap normalizes them (mirrored back below so the widgets show the
    // effective box). The header summary shows the box while collapsed and
    // omits z in 2D like the row list.
    section(
        ui,
        Section::Domain,
        controls,
        Some(domain_summary(&state.domain, field_dimension == 3)),
        |ui, controls| {
            // Only the axes the current sampling dimension can use: the z row
            // vanishes in 2D rather than sitting greyed out (ticket 50). The
            // stored z survives for the next 3D switch; the reset below still
            // resets all three axes.
            for axis in domain_axes(field_dimension == 3) {
                let (min, max) = match axis {
                    DomainAxis::X => (&mut controls.domain_min.x, &mut controls.domain_max.x),
                    DomainAxis::Y => (&mut controls.domain_min.y, &mut controls.domain_max.y),
                    DomainAxis::Z => (&mut controls.domain_min.z, &mut controls.domain_max.z),
                };
                domain_axis_row(ui, axis.label(), min, max);
            }
            ui.add_space(ui.spacing().item_spacing.y);
            if ui
                .button("Reset domain")
                .on_hover_text("Restore x, y, z to -1…1")
                .clicked()
            {
                // Same seam as the generic header button, so there is one
                // reset path.
                controls.reset_domain();
            }
        },
    );

    // Hand the whole mirror to the seam (below); the rebuild that the dirty
    // flag asks for happens in app.rs right after this panel, so this frame's
    // snapshot and status-bar count already reflect the edits. The plane index
    // is separate: it commits on toggle/axis changes and on slider release
    // only (ticket 23), so a drag never re-samples the field.
    if commit_plane {
        state.set_plane(controls.plane);
        controls.plane = state.plane;
    }
    if vector_mode {
        apply(state, controls);
    } else {
        apply_plot_mode(state, controls);
    }
    plot_action
}

/// Scalar-mode hand-off: only the controls shared by plots (grid/axes, domain,
/// palette) reach the seam. Vector magnitude/arrow/feature setters stay out so
/// a scalar frame can never overwrite the values scalar mode retains, and the
/// palette is taken from the Plot panel rather than a stale vector mirror.
fn apply_plot_mode(state: &mut State, controls: &mut Controls) {
    state.set_show_grid(controls.show_grid);
    state.set_show_coordinate_system(controls.show_coordinate_system);
    apply_scalar_controls(state, controls);
    let domain_min = Vec3::new(
        extent_or_previous(controls.domain_min.x, state.domain.min.x),
        extent_or_previous(controls.domain_min.y, state.domain.min.y),
        extent_or_previous(controls.domain_min.z, state.domain.min.z),
    );
    let domain_max = Vec3::new(
        extent_or_previous(controls.domain_max.x, state.domain.max.x),
        extent_or_previous(controls.domain_max.y, state.domain.max.y),
        extent_or_previous(controls.domain_max.z, state.domain.max.z),
    );
    state.set_domain(domain_min, domain_max);

    // Mirrors only: the Plot panel already called the palette setter directly.
    controls.colormap = state.colormap;
    controls.show_grid = state.show_grid;
    controls.show_coordinate_system = state.show_coordinate_system;
    controls.domain_min = state.domain.min;
    controls.domain_max = state.domain.max;
}

/// Commit the two bounded Scalar view settings through their validated State
/// setters and mirror the effective values back. A changed successful commit
/// clears an earlier rejection; a rejection is surfaced in the message slot and
/// leaves the committed value untouched.
fn apply_scalar_controls(state: &mut State, controls: &mut Controls) {
    let mut committed = false;
    let mut rejected = false;

    let grid_side_changed = controls.scalar_grid_side != state.scalar_grid_side();
    if let Err(error) = state.set_scalar_grid_side(controls.scalar_grid_side) {
        controls.scalar_message = error.to_string();
        rejected = true;
    } else if grid_side_changed {
        committed = true;
    }

    let levels_changed = controls.scalar_contour_levels != state.scalar_contour_levels();
    if let Err(error) = state.set_scalar_contour_levels(controls.scalar_contour_levels) {
        controls.scalar_message = error.to_string();
        rejected = true;
    } else if levels_changed {
        committed = true;
    }

    if committed && !rejected {
        controls.scalar_message.clear();
    }
    controls.scalar_grid_side = state.scalar_grid_side();
    controls.scalar_contour_levels = state.scalar_contour_levels();
}

/// Draw the plane selector and report whether the selection should be
/// committed to the seam this frame. `Full` keeps the 3D plane-by-plane mode
/// with its enable toggle and off-plane opacity; `SliceOnly` is the 2D slice
/// selector (axis and position only). The slider commits on `drag_stopped` (or
/// a click/keyboard change), not on every dragged frame.
fn plane_controls(
    ui: &mut egui::Ui,
    controls: &mut Controls,
    domain: &Domain,
    grid: glam::UVec3,
    mode: PlaneControlMode,
) -> bool {
    if mode == PlaneControlMode::Hidden {
        return false;
    }
    let full = mode == PlaneControlMode::Full;
    let mut commit = false;
    if full {
        commit = ui
            .checkbox(&mut controls.plane.enabled, "Plane by plane")
            .on_hover_text("Highlight one sampled slice and fade the rest of the cube")
            .changed();
        if !controls.plane.enabled {
            return commit;
        }
    }

    button_row(ui, |ui| {
        ui.label(if full { "Axis" } else { "Slice axis" });
        for axis in PlaneAxis::ALL {
            commit |= ui
                .selectable_value(&mut controls.plane.axis, axis, axis.label().to_uppercase())
                .on_hover_text("Axis the selected plane is perpendicular to")
                .changed();
        }
    });

    let count = controls.plane.axis.count(grid);
    // An axis switch may leave the old index past the new axis's last sample.
    controls.plane.index = controls.plane.index.min(count.saturating_sub(1));
    if count <= 1 {
        ui.label(format!(
            "{} = {:.2}  (1 / 1) — one slice",
            controls.plane.axis.label(),
            plane_coordinate(domain, grid, controls.plane.axis, controls.plane.index),
        ));
        return commit;
    }

    let response = ui.add(
        egui::Slider::new(&mut controls.plane.index, 0..=count - 1)
            .text(if full { "Plane" } else { "Position" })
            .integer(),
    );
    if response.drag_started() {
        controls.plane_drag = true;
    }
    if response.drag_stopped() {
        controls.plane_drag = false;
        commit = true;
    }
    // A click on the track or a keyboard step is already a completed change;
    // only a live drag waits for the release.
    if response.changed() && !response.dragged() {
        commit = true;
    }
    let index = controls.plane.index;
    ui.label(format!(
        "{} = {:.2}  ({} / {})",
        controls.plane.axis.label(),
        plane_coordinate(domain, grid, controls.plane.axis, index),
        index + 1,
        count,
    ));
    if full {
        ui.add(
            egui::Slider::new(
                &mut controls.plane_dim_opacity,
                MIN_PLANE_DIM_OPACITY..=MAX_PLANE_DIM_OPACITY,
            )
            .text("Off-plane opacity")
            .custom_formatter(|value, _| format!("{:.0}%", value * 100.0)),
        )
        .on_hover_text("Opacity of arrows outside the selected plane");
    }
    commit
}

/// One axis's min/max row: the axis label names the component, both drag
/// values are bounded by the seam's single extent range, and the tooltips name
/// the world-unit meaning. [`domain_axes`] decides which rows exist.
fn domain_axis_row(ui: &mut egui::Ui, axis: &str, min: &mut f32, max: &mut f32) {
    ui.horizontal(|ui| {
        ui.label(format!("{axis}:"));
        ui.add(
            egui::DragValue::new(min)
                .speed(0.01)
                .range(MIN_DOMAIN_EXTENT..=MAX_DOMAIN_EXTENT)
                .max_decimals(2),
        )
        .on_hover_text(format!("Minimum {axis} of the sampled box, world units"));
        ui.label("to");
        ui.add(
            egui::DragValue::new(max)
                .speed(0.01)
                .range(MIN_DOMAIN_EXTENT..=MAX_DOMAIN_EXTENT)
                .max_decimals(2),
        )
        .on_hover_text(format!("Maximum {axis} of the sampled box, world units"));
    })
    .response
    .on_hover_text(format!("Sampled {axis} extent, world units"));
}

/// The Variables section: collapsed by default, seeded from the committed set,
/// and rendered only where [`variables_visible`] allows it.
fn variables_section(ui: &mut egui::Ui, state: &mut State, controls: &mut Controls) {
    controls.variables.sync(&state.parameters);
    egui::CollapsingHeader::new("Variables")
        .default_open(false)
        .show(ui, |ui| variables_editor(ui, state, controls));
}

/// One optional min/max bound: a checkbox toggles the blank/unbounded state
/// and a positive-width DragValue edits the value while it is bounded. The
/// returned flag reports a widget change that should commit.
fn optional_variable_bound(ui: &mut egui::Ui, label: &str, bound: &mut Option<f64>) -> bool {
    let mut changed = false;
    ui.label(label);
    let mut unbounded = bound.is_none();
    if ui.checkbox(&mut unbounded, "unbounded").changed() {
        *bound = if unbounded { None } else { Some(0.0) };
        changed = true;
    }
    if let Some(value) = bound {
        changed |= ui
            .add(egui::DragValue::new(value).speed(0.1))
            .on_hover_text("Inclusive bound; leave unbounded for no limit")
            .changed();
    }
    changed
}

/// Draw the draft rows and hand every edit to the atomic model seam. Drafts
/// stay session-only on [`Controls`]; a rejected commit keeps them and shows
/// the model's message inline.
fn variables_editor(ui: &mut egui::Ui, state: &mut State, controls: &mut Controls) {
    let variables = &mut controls.variables;
    let mut edited = false;
    let mut delete: Option<usize> = None;

    let row_count = variables.drafts.len();
    for index in 0..row_count {
        let entry = &mut variables.drafts[index];
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(&entry.name).strong());
            ui.label("Value");
            edited |= ui
                .add(egui::DragValue::new(&mut entry.value).speed(0.1))
                .on_hover_text("The value formula references bind to")
                .changed();
            if ui
                .button("Delete")
                .on_hover_text("Remove this parameter from the committed set")
                .clicked()
            {
                delete = Some(index);
            }
        });
        ui.horizontal_wrapped(|ui| {
            edited |= optional_variable_bound(ui, "Min", &mut entry.min);
            edited |= optional_variable_bound(ui, "Max", &mut entry.max);
            ui.label("Step");
            edited |= ui
                .add(
                    egui::DragValue::new(&mut entry.step)
                        .speed(0.05)
                        .range(0.0001..=1.0e9),
                )
                .on_hover_text("Positive increment used by the drag value")
                .changed();
        });
        // A blank bound is the unbounded state; a typed/derived non-finite
        // value must never reach the draft as NaN.
        if entry.min.is_some_and(|value| !value.is_finite()) {
            entry.min = None;
            edited = true;
        }
        if entry.max.is_some_and(|value| !value.is_finite()) {
            entry.max = None;
            edited = true;
        }
    }

    if let Some(index) = delete {
        variables.drafts.remove(index);
        edited = true;
    }

    let add = ui
        .horizontal_wrapped(|ui| {
            ui.label("New");
            ui.add(
                egui::TextEdit::singleline(&mut variables.name)
                    .hint_text("name")
                    .desired_width(90.0),
            );
            let enabled =
                !variables.name.trim().is_empty() && variables.drafts.len() < MAX_PARAMETERS;
            ui.add_enabled(enabled, egui::Button::new("Add parameter"))
                .on_hover_text("Validate and add a named parameter")
                .clicked()
        })
        .inner;
    if add {
        let name = variables.name.trim().to_owned();
        variables.drafts.push(Parameter {
            name,
            value: 0.0,
            min: None,
            max: None,
            step: 1.0,
        });
        if variables.commit(state) {
            variables.name.clear();
        } else {
            // The model rejected the set: keep the name draft for correction
            // but do not leave the invalid row in the editor.
            variables.drafts.pop();
        }
    }

    if ui
        .button("Reset variables")
        .on_hover_text("Commit an empty parameter set")
        .clicked()
    {
        variables.reset(state);
    } else if edited {
        variables.commit(state);
    }

    if !variables.error.is_empty() {
        ui.add(
            egui::Label::new(
                egui::RichText::new(&variables.error).color(ui.visuals().error_fg_color),
            )
            .wrap(),
        );
    }
}

/// One collapsible section with a reset-to-defaults button in its header.
/// `summary`, when present, is a second header line that stays visible while
/// the section is collapsed (the Domain box needs to be readable without
/// opening it).
fn section(
    ui: &mut egui::Ui,
    section: Section,
    controls: &mut Controls,
    summary: Option<String>,
    body: impl FnOnce(&mut egui::Ui, &mut Controls),
) {
    let id = ui.make_persistent_id(("controls_section", section.title()));
    let mut reset = false;
    egui::collapsing_header::CollapsingState::load_with_default_open(
        ui.ctx(),
        id,
        section.default_open(),
    )
    .show_header(ui, |ui| match summary {
        Some(summary) => {
            ui.vertical(|ui| {
                section_header_line(ui, section, &mut reset);
                ui.add(egui::Label::new(summary).wrap()).on_hover_text(
                    "Current sampled box, world units; drag the rows below to change it",
                );
            });
        }
        None => section_header_line(ui, section, &mut reset),
    })
    .body(|ui| body(ui, controls));

    if reset {
        match section {
            Section::Display => controls.reset_display(),
            Section::Camera => controls.reset_camera(),
            Section::Features => controls.reset_features(),
            Section::Coloring => controls.reset_coloring(),
            Section::Domain => controls.reset_domain(),
        }
    }
}

/// Wrap button choices against the visible panel even when a non-button
/// editor has requested a wider minimum for the surrounding body.
fn button_row(ui: &mut egui::Ui, contents: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal_wrapped(|ui| {
        ui.set_max_width(
            ui.available_width()
                .min((ui.clip_rect().right() - ui.cursor().left()).max(0.0)),
        );
        contents(ui);
    });
}

/// The title row every section header shares: the strong title and the
/// generic reset button in the right-hand corner. The Domain header wraps this
/// in a vertical layout to add its summary line.
fn section_header_line(ui: &mut egui::Ui, section: Section, reset: &mut bool) {
    ui.label(egui::RichText::new(section.title()).strong());
    // Non-button editors can request a wider panel than its visible clip.
    // Anchor the action to the visible remainder without resetting the parent
    // header's cursor (which has already allocated the collapse toggle).
    let mut rect = ui.available_rect_before_wrap();
    rect.max.x = rect.max.x.min(ui.clip_rect().right()).max(rect.min.x);
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
        |ui| {
            // Full-size button, not `small_button`: the section action gets a
            // normal click target. (In egui 0.37 `small_button` only trims
            // vertical padding, so this is about target size, not type size.)
            *reset = ui
                .button("Reset")
                .on_hover_text(format!("Reset {} to defaults", section.title()))
                .clicked();
        },
    );
}

/// Keep a dragged extent honest before the seam sees it: NaN has no direction,
/// so the previous effective value stays (the box must not jump); every other
/// value passes through the one bounded clamp.
fn extent_or_previous(value: f32, previous: f32) -> f32 {
    if value.is_nan() {
        previous
    } else {
        clamp_domain_extent(value)
    }
}

/// The whole edit→seam handoff, kept pure (no egui types) so `show` stays
/// thin and the wiring is testable without a window (the spec's testing rule,
/// and the ledger ruling that the sidebar must call seam setters, never
/// mutate scene state). Every seam setter owns its own clamping / swap rule
/// and marks the state dirty only if the value actually moved; afterwards the
/// mirror is refreshed from the effective state, so next frame's widgets
/// start from truth (clamped density, swapped extrema, …).
///
/// Plane-by-plane is deliberately absent: its index commits on slider release
/// (ticket 23), not once per frame, so it has its own handoff in `show`.
pub fn apply(state: &mut State, controls: &mut Controls) {
    state.set_density(controls.density);
    state.set_arrow_scale(controls.arrow_scale);
    state.set_arrow_opacity(controls.arrow_opacity);
    state.set_plane_dim_opacity(controls.plane_dim_opacity);
    state.set_arrow_length_mode(controls.arrow_length_mode);
    state.set_magnitude_scale(controls.magnitude_scale);
    state.set_proportional_exponent(controls.proportional_exponent);
    state.set_color_quantity(controls.color_quantity);
    let half_width = sanitized_half_width(controls.signed_log_half_width);
    controls.signed_log_half_width = half_width;
    let mode = if controls.signed_log {
        ScaleMode::SignedLog { half_width }
    } else {
        ScaleMode::Linear
    };
    if mode.validate().is_err() {
        state.set_color_mode(ScaleMode::Linear);
    } else {
        state.set_color_mode(mode);
    }
    // A custom override survives frames where the built-in selection is
    // unchanged: `set_colormap` unconditionally clears the override, so it is
    // only called when the selection actually moved (or when no override is
    // active). Choosing a built-in that differs from the current selection
    // therefore still clears the override.
    if controls.colormap != state.colormap || state.color_ramp.is_none() {
        state.set_colormap(controls.colormap);
    }
    state.set_auto_fit(controls.auto_fit);
    state.set_color_range(controls.color_min, controls.color_max);
    if controls.palette_apply_requested {
        controls.palette_apply_requested = false;
        match controls.palette_working.clone() {
            Some(ramp) => match ramp.validate() {
                Ok(()) => {
                    state.set_color_ramp(Some(ramp.clone()));
                    controls.palette_working = Some(ramp);
                    controls.palette_message = "Custom palette applied".to_owned();
                }
                Err(reason) => controls.palette_message = reason,
            },
            None => {
                state.set_color_ramp(None);
                controls.palette_message =
                    format!("Reverted to built-in {}", state.colormap.label());
            }
        }
    }
    if controls.freeze_requested {
        state.freeze_color_range(&controls.color_stats);
        controls.freeze_requested = false;
    }
    state.set_arrows(controls.arrows);
    state.set_show_grid(controls.show_grid);
    state.set_show_coordinate_system(controls.show_coordinate_system);
    state.set_arrow_style(controls.arrow_style);
    state.set_arrow_style_2d(controls.arrow_style_2d);
    state.set_arrow_params(controls.arrow_params);
    state.set_streamlines(controls.streamlines);
    state.set_seed_mode(controls.seed_mode);
    state.set_streamline_count(controls.streamline_count);
    state.set_left_click_action(controls.left_click_action);
    state.set_particles(controls.particles);
    state.set_particle_size(controls.particle_size);
    state.set_particle_count(controls.particle_count);
    state.set_particle_appearance(controls.particle_appearance);
    // Domain extents are bounded here, before `set_domain`'s per-axis swap,
    // so the UI's `.range` and this handoff share one clamp.
    let domain_min = Vec3::new(
        extent_or_previous(controls.domain_min.x, state.domain.min.x),
        extent_or_previous(controls.domain_min.y, state.domain.min.y),
        extent_or_previous(controls.domain_min.z, state.domain.min.z),
    );
    let domain_max = Vec3::new(
        extent_or_previous(controls.domain_max.x, state.domain.max.x),
        extent_or_previous(controls.domain_max.y, state.domain.max.y),
        extent_or_previous(controls.domain_max.z, state.domain.max.z),
    );
    state.set_domain(domain_min, domain_max);
    apply_scalar_controls(state, controls);

    controls.density = state.density;
    controls.arrow_scale = state.arrow_scale;
    controls.arrow_opacity = state.arrow_opacity;
    controls.plane_dim_opacity = state.plane_dim_opacity;
    controls.arrow_length_mode = state.arrow_length_mode;
    controls.magnitude_scale = state.magnitude_scale;
    controls.proportional_exponent = state.proportional_exponent;
    controls.colormap = state.colormap;
    controls.color_quantity = state.color_quantity;
    controls.color_mode = state.color_mode;
    match state.color_mode {
        ScaleMode::Linear => controls.signed_log = false,
        ScaleMode::SignedLog { half_width } => {
            controls.signed_log = true;
            controls.signed_log_half_width = half_width;
        }
    }
    controls.auto_fit = state.auto_fit;
    controls.color_min = state.color_min;
    controls.color_max = state.color_max;
    controls.arrows = state.arrows;
    controls.show_grid = state.show_grid;
    controls.show_coordinate_system = state.show_coordinate_system;
    controls.arrow_style = state.arrow_style;
    controls.arrow_style_2d = state.arrow_style_2d;
    controls.arrow_params = state.arrow_params;
    controls.streamlines = state.streamlines;
    controls.seed_mode = state.seed_mode;
    controls.streamline_count = state.streamline_count;
    controls.left_click_action = state.left_click_action;
    controls.particles = state.particles;
    controls.particle_size = state.particle_size;
    controls.particle_count = state.particle_count;
    controls.particle_appearance = state.particle_appearance;
    controls.domain_min = state.domain.min;
    controls.domain_max = state.domain.max;
}
