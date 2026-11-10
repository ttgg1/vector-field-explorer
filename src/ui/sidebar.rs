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
use crate::colormap::Colormap;
use crate::domain::Domain;
use crate::particles::{
    MAX_PARTICLES, MAX_TRAIL_LEN, MIN_PARTICLE_OPACITY, MAX_PARTICLE_OPACITY,
    MIN_TRAIL_FADE, MAX_TRAIL_FADE, MIN_TRAIL_WIDTH, MAX_TRAIL_WIDTH,
    MIN_STAGNATION_SECONDS, MAX_STAGNATION_SECONDS, ParticleAppearance, ParticleColorMode,
};
use crate::plane::{DIM_ALPHA, PlaneAxis, PlaneView, plane_coordinate};
use crate::view_command::{ClickActionPlan, click_action_plan};
use crate::viewmodel::{
    ArrowLengthMode,
    LeftClickAction,
    State,
    StreamlineSeedMode,
    DEFAULT_DENSITY_2D,
    DEFAULT_MAGNITUDE_SCALE,
    DEFAULT_PARTICLE_SIZE,
    DEFAULT_PROPORTIONAL_EXPONENT,
    MIN_DENSITY,
    MAX_DENSITY_2D,
    MAX_DENSITY_3D,
    MIN_ARROW_SCALE,
    MAX_ARROW_SCALE,
    MIN_ARROW_OPACITY,
    MAX_ARROW_OPACITY,
    MIN_PLANE_DIM_OPACITY,
    MAX_PLANE_DIM_OPACITY,
    MIN_MAGNITUDE_SCALE,
    MAX_MAGNITUDE_SCALE,
    MIN_PARTICLE_SIZE,
    MAX_PARTICLE_SIZE,
    MIN_PROPORTIONAL_EXPONENT,
    MAX_PROPORTIONAL_EXPONENT,
    MIN_DOMAIN_EXTENT,
    MAX_DOMAIN_EXTENT,
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
        matches!(
            self,
            Section::Display | Section::Camera | Section::Features
        )
    }
}

/// The effective domain box as the Domain header's summary line, e.g.
/// `x -1…1 · y -1…1 · z -1…1`. `include_z` is false when the view is 2D, so
/// the header never advertises an extent the user cannot see or sample (the
/// row list uses the same gate). Numbers are rounded to at
/// most two decimals and integers lose their trailing `.00`, which keeps the
/// line short enough for the panel's 180 pt minimum.
pub fn domain_summary(domain: &Domain, include_z: bool) -> String {
    let axis = |name: &str, lo: f32, hi: f32| {
        format!("{name} {}…{}", extent_text(lo), extent_text(hi))
    };
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
/// without a window; `is_3d` is the sampling dimension, not raw `mode_3d`, so a
/// flat formula in 3D mode still hides z.
pub fn domain_axes(is_3d: bool) -> &'static [DomainAxis] {
    if is_3d {
        &[DomainAxis::X, DomainAxis::Y, DomainAxis::Z]
    } else {
        &[DomainAxis::X, DomainAxis::Y]
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
    if view_3d { ArrowStyleSet::ThreeD } else { ArrowStyleSet::TwoD }
}

/// Whether the Sides slider applies to the active view's selected family.
pub fn sides_control_enabled(
    view_3d: bool,
    style_3d: ArrowStyle,
    style_2d: ArrowStyle2D,
) -> bool {
    if view_3d { style_3d.uses_sides() } else { style_2d.uses_sides() }
}

/// Whether the Head length/head width sliders apply to the active view's
/// selected family. Cylinder and Line have no head, so those controls are
/// disabled rather than silently ignored.
pub fn head_control_enabled(
    view_3d: bool,
    style_3d: ArrowStyle,
    style_2d: ArrowStyle2D,
) -> bool {
    if view_3d { style_3d.uses_head() } else { style_2d.uses_head() }
}

/// Whether the seed-mode selector applies. With Streamlines off both seed
/// sources are hidden and clicks are not stored, so the selector would be a
/// dead control; it is disabled rather than silently ignored.
pub fn seed_mode_enabled(streamlines: bool) -> bool {
    streamlines
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
    if count == 0 { "Auto".to_owned() } else { count.to_string() }
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
    pub arrow_style: ArrowStyle,
    pub arrow_style_2d: ArrowStyle2D,
    pub arrow_params: ArrowParams,
    pub colormap: Colormap,
    pub auto_fit: bool,
    pub color_min: f32,
    pub color_max: f32,
    pub streamlines: bool,
    /// Which streamline seed sources are shown while the Feature is on.
    pub seed_mode: StreamlineSeedMode,
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
            arrow_style: ArrowStyle::Cone,
            arrow_style_2d: ArrowStyle2D::Flat,
            arrow_params: ArrowParams::default(),
            colormap: Colormap::Viridis,
            auto_fit: true,
            color_min: 0.0,
            color_max: 1.0,
            streamlines: false,
            seed_mode: StreamlineSeedMode::Auto,
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
        }
    }

    /// Restore the Display section's defaults (density, arrow length mode,
    /// both scales, arrow and off-plane opacity, the proportional contrast,
    /// arrow visibility/style/appearance, plane-by-plane).
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
        self.left_click_action = defaults.left_click_action;
        self.particles = defaults.particles;
        self.particle_size = defaults.particle_size;
        self.particle_count = defaults.particle_count;
        self.particle_appearance = defaults.particle_appearance;
    }

    /// Restore the Coloring section's defaults (viridis, auto-fit, 0..1).
    pub fn reset_coloring(&mut self) {
        let defaults = Controls::new();
        self.colormap = defaults.colormap;
        self.auto_fit = defaults.auto_fit;
        self.color_min = defaults.color_min;
        self.color_max = defaults.color_max;
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
        self.fly = false;
    }
}

/// The same defaults as [`Controls::new`], so the type honours clippy's
/// `new_without_default`: the app can reach its starting knobs either way.
impl Default for Controls {
    fn default() -> Controls { Controls::new() }
}

use crate::ui::layout;

/// Draw the sidebar and hand every edit to the viewmodel setters.
pub fn show(ui: &mut egui::Ui, state: &mut State, controls: &mut Controls) {
    show_compact(ui, state, controls, false)
}

/// Compact-mode entry: applies the 44 pt row/touch-target floor, then draws.
pub fn show_compact(ui: &mut egui::Ui, state: &mut State, controls: &mut Controls, compact: bool) {
    if compact {
        ui.spacing_mut().interact_size.y = layout::COMPACT_ROW_HEIGHT;
    }
    show_inner(ui, state, controls)
}

fn show_inner(ui: &mut egui::Ui, state: &mut State, controls: &mut Controls) {
    // The density ceiling keys to the dimension being sampled (the ledger
    // ruling): n³ only when the mode is 3D and the field carries z.
    let sampling_3d = state.is_3d();
    let view_3d = state.mode_3d;
    let max_density = if sampling_3d { MAX_DENSITY_3D } else { MAX_DENSITY_2D };
    let grid = state.grid();
    let domain = state.domain;

    // The plane mirror follows the seam (which clamps/recentres it on grid
    // changes) except while its slider is mid-drag: the in-flight value has to
    // survive the frame so the slider does not snap back. The seam is only
    // handed the new index when the drag ends, so dragging never resamples.
    if !controls.plane_drag {
        controls.plane = state.plane;
    }
    let mut commit_plane = false;

    section(ui, Section::Display, controls, None, |ui, controls| {
        ui.horizontal(|ui| {
            ui.label(if sampling_3d { "Density (n³)" } else { "Density (n²)" });
            ui.add(egui::DragValue::new(&mut controls.density).range(MIN_DENSITY..=max_density))
                .on_hover_text("Samples per axis; 3D sampling makes it the cube of this count");
        });
        ui.checkbox(&mut controls.arrows, "Show arrows")
            .on_hover_text("Hide the arrow field without hiding streamlines or particles");
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
            egui::Slider::new(&mut controls.arrow_opacity, MIN_ARROW_OPACITY..=MAX_ARROW_OPACITY)
                .text("Arrow opacity")
                .custom_formatter(|value, _| format!("{:.0}%", value * 100.0)),
        )
        .on_hover_text(
            "Opacity of the whole arrow field; 100% is fully solid. Dense fields can be \
             de-emphasised without hiding them",
        );
        ui.horizontal(|ui| {
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
        ui.add_enabled_ui(proportional_exponent_enabled(controls.arrow_length_mode), |ui| {
            ui.add(
                egui::Slider::new(
                    &mut controls.proportional_exponent,
                    MIN_PROPORTIONAL_EXPONENT..=MAX_PROPORTIONAL_EXPONENT,
                )
                .text("Proportional contrast"),
            );
        })
        .response
        .on_hover_text("How strongly arrow length follows magnitude; 1 = linear")
        .on_disabled_hover_text("Only Proportional arrow length follows magnitude");
        ui.horizontal(|ui| {
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
                    sides_control_enabled(view_3d, controls.arrow_style, controls.arrow_style_2d),
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
        if sampling_3d {
            commit_plane |= plane_controls(ui, controls, &domain, grid);
        }
    });

    // Camera (ticket 65): view controls sit between Display and Features. The
    // fly toggle keys to the sampled dimension (`is_3d`), the same gate as
    // `toggle_fly`: a 2D formula in 3D mode must stay inert. The app mirrors
    // the real camera into `controls.fly` each frame, so C, Esc, a 2D switch,
    // Reset/Fit and presets all land here on the next frame.
    section(ui, Section::Camera, controls, None, |ui, controls| {
        ui.add_enabled_ui(sampling_3d, |ui| {
            ui.checkbox(&mut controls.fly, "Fly camera")
                .on_hover_text("Free flight in the 3D view (C); Esc releases the mouse");
        })
        .response
        .on_disabled_hover_text("3D only");
        if controls.fly && sampling_3d {
            ui.label("Flying — Esc to release");
        }
    });

    section(ui, Section::Features, controls, None, |ui, controls| {
        ui.checkbox(&mut controls.streamlines, "Streamlines").on_hover_text(
            "Auto adds grid seeds; Click shows only click seeds. Off hides both and disables click seeding.",
        );
        ui.add_enabled_ui(seed_mode_enabled(controls.streamlines), |ui| {
            ui.horizontal(|ui| {
                ui.label("Seed");
                for mode in StreamlineSeedMode::ALL {
                    ui.selectable_value(&mut controls.seed_mode, mode, mode.label())
                        .on_hover_text(mode.tooltip());
                }
            });
        })
        .response
        .on_disabled_hover_text(
            "Turn Streamlines on to choose Auto or Click seeding",
        );
        // Ticket 60: the left click action stays visible even while its feature
        // is off; the click is then a hinted no-op rather than a dead control.
        ui.horizontal(|ui| {
            ui.label("Left click");
            for action in LeftClickAction::ALL {
                let hover = match click_action_plan(
                    action,
                    controls.streamlines,
                    controls.particles,
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
            .on_hover_text("Animate dots carried by the field (3D view)");
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
                    ui.horizontal(|ui| {
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
                                ui.color_edit_button_srgb(&mut fixed)
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

    section(ui, Section::Coloring, controls, None, |ui, controls| {
        ui.horizontal(|ui| {
            ui.label("Colormap");
            for colormap in Colormap::ALL {
                ui.selectable_value(&mut controls.colormap, colormap, colormap.label())
                    .on_hover_text("Color map for field magnitude");
            }
        });
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
    });

    // The Domain editor renders only the axes the current sampling dimension
    // can use; reversed drags are handed over as-is and the seam's per-axis
    // swap normalizes them (mirrored back below so the widgets show the
    // effective box). The header summary shows the box while collapsed and
    // omits z in 2D like the row list.
    section(
        ui,
        Section::Domain,
        controls,
        Some(domain_summary(&state.domain, sampling_3d)),
        |ui, controls| {
            // Only the axes the current sampling dimension can use: the z row
            // vanishes in 2D rather than sitting greyed out (ticket 50). The
            // stored z survives for the next 3D switch; the reset below still
            // resets all three axes.
            for axis in domain_axes(sampling_3d) {
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
    apply(state, controls);
}

/// Draw the plane-by-plane controls (3D sampling only) and report whether the
/// selection should be committed to the seam this frame. The slider commits on
/// `drag_stopped` (or a click/keyboard change), not on every dragged frame.
fn plane_controls(
    ui: &mut egui::Ui,
    controls: &mut Controls,
    domain: &Domain,
    grid: glam::UVec3,
) -> bool {
    let mut commit = ui
        .checkbox(&mut controls.plane.enabled, "Plane by plane")
        .on_hover_text("Highlight one sampled slice and fade the rest of the cube")
        .changed();
    if !controls.plane.enabled {
        return commit;
    }

    ui.horizontal(|ui| {
        ui.label("Axis");
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
            .text("Plane")
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
    ui.add(
        egui::Slider::new(
            &mut controls.plane_dim_opacity,
            MIN_PLANE_DIM_OPACITY..=MAX_PLANE_DIM_OPACITY,
        )
        .text("Off-plane opacity")
        .custom_formatter(|value, _| format!("{:.0}%", value * 100.0)),
    )
    .on_hover_text("Opacity of arrows outside the selected plane");
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

/// The title row every section header shares: the strong title and the
/// generic reset button in the right-hand corner. The Domain header wraps this
/// in a vertical layout to add its summary line.
fn section_header_line(ui: &mut egui::Ui, section: Section, reset: &mut bool) {
    ui.label(egui::RichText::new(section.title()).strong());
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        // Full-size button, not `small_button`: the section action gets a
        // normal click target. (In egui 0.37 `small_button` only trims
        // vertical padding, so this is about target size, not type size.)
        *reset = ui
            .button("Reset")
            .on_hover_text(format!("Reset {} to defaults", section.title()))
            .clicked();
    });
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
    state.set_colormap(controls.colormap);
    state.set_auto_fit(controls.auto_fit);
    state.set_color_range(controls.color_min, controls.color_max);
    state.set_arrows(controls.arrows);
    state.set_arrow_style(controls.arrow_style);
    state.set_arrow_style_2d(controls.arrow_style_2d);
    state.set_arrow_params(controls.arrow_params);
    state.set_streamlines(controls.streamlines);
    state.set_seed_mode(controls.seed_mode);
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

    controls.density = state.density;
    controls.arrow_scale = state.arrow_scale;
    controls.arrow_opacity = state.arrow_opacity;
    controls.plane_dim_opacity = state.plane_dim_opacity;
    controls.arrow_length_mode = state.arrow_length_mode;
    controls.magnitude_scale = state.magnitude_scale;
    controls.proportional_exponent = state.proportional_exponent;
    controls.colormap = state.colormap;
    controls.auto_fit = state.auto_fit;
    controls.color_min = state.color_min;
    controls.color_max = state.color_max;
    controls.arrows = state.arrows;
    controls.arrow_style = state.arrow_style;
    controls.arrow_style_2d = state.arrow_style_2d;
    controls.arrow_params = state.arrow_params;
    controls.streamlines = state.streamlines;
    controls.seed_mode = state.seed_mode;
    controls.left_click_action = state.left_click_action;
    controls.particles = state.particles;
    controls.particle_size = state.particle_size;
    controls.particle_count = state.particle_count;
    controls.particle_appearance = state.particle_appearance;
    controls.domain_min = state.domain.min;
    controls.domain_max = state.domain.max;
}
