//! The viewmodel seam: the app's whole visible state and the pure snapshot
//! builder that turns it into the GPU scene.
//!
//! Spec ruling: exactly ONE new seam — pure functions state→SceneGPU and
//! input→camera actions, so the UI layer stays thin and the wiring logic is
//! testable without a window. No GPU types beyond the plain `SceneGPU`
//! instance layouts.

use super::camera::OrbitCamera;
use super::colormap::Colormap;
use super::domain::Domain;
use super::field::FieldExpr;
use super::plane::{DIM_ALPHA, PlaneAxis, PlaneView, dim_color, on_plane};
use super::renderer::{ArrowInstanceGPU, LineInstanceGPU, SceneGPU};
use super::sampling::{ArrowData, ViewOptions, magnitude_range, sample_field};
use super::streamlines::{CLICK_BURST_SEEDS, StreamlineOpts, burst_seeds, seed_grid, trace};
use crate::arrow_style::{ArrowParams, ArrowStyle, ArrowStyle2D};

use std::sync::Arc;

use glam::{UVec3, Vec2, Vec3};
use serde::{Deserialize, Serialize};

/// Number of samples along one axis of a 2D grid.
pub const DEFAULT_DENSITY_2D: u32 = 24;
/// Number of samples along one axis of the default 3D grid.
pub const DEFAULT_DENSITY_3D: u32 = 16;
/// Density the sidebar clamps the 2D grid to.
pub const MAX_DENSITY_2D: u32 = 200;
/// Density the sidebar clamps the 3D grid to.
pub const MAX_DENSITY_3D: u32 = 40;
/// The smallest density the sidebar offers.
pub const MIN_DENSITY: u32 = 4;
/// The 2D plane is the domain this much wider, so the field fills the window.
pub const PLANE_MARGIN: f32 = 1.35;
/// Bounds of the arrow-length slider.
pub const MIN_ARROW_SCALE: f32 = 0.1;
pub const MAX_ARROW_SCALE: f32 = 4.0;
/// Bounds and default of the magnitude scale: world units per unit of field
/// magnitude in [`ArrowLengthMode::Magnitude`]. Bounded here so the widget and
/// the seam share one range; the resulting arrow length itself is not clamped.
pub const MIN_MAGNITUDE_SCALE: f32 = 0.01;
pub const MAX_MAGNITUDE_SCALE: f32 = 10.0;
pub const DEFAULT_MAGNITUDE_SCALE: f32 = 1.0;
/// Bounds of one domain extent, world units. The editor keeps the sampled box
/// finite and in a range where a drag move is still meaningful; the seam's
/// [`clamp_domain_extent`] is the single place both the widgets and the
/// per-frame handoff take the bound from.
pub const MIN_DOMAIN_EXTENT: f32 = -100.0;
pub const MAX_DOMAIN_EXTENT: f32 = 100.0;
/// Line width streamlines are painted with, world units.
pub const STREAMLINE_WIDTH: f32 = 0.012;
/// Fixed point budget per streamline. Streamlines are not clock consumers, so
/// their length no longer follows a speed slider; the transport's clock speed
/// only animates particles (ticket 26).
pub const STREAMLINE_MAX_LEN: usize = 120;
/// Seed jitter as a fraction of a seed-grid cell (`seed_grid` caps it below
/// half a cell so lattice neighbours never swap).
pub const STREAMLINE_JITTER: f32 = 0.45;

/// The starting field for each mode: the Pump preset (2D) and the 3D radial
/// preset, so a fresh app shows something immediately.
pub const FORMULA_2D: &str = "(x, y)";
pub const FORMULA_3D: &str = "(x, y, z)";

/// How a sampled field magnitude becomes an arrow's world-space length
/// (ticket 33). Replaces the old `normalize` bool; persisted records that still
/// carry one migrate through [`ArrowLengthMode::from_normalize`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ArrowLengthMode {
    /// Length follows magnitude, normalised so the longest arrow is exactly
    /// `arrow_scale` grid cells. The default.
    #[default]
    Proportional,
    /// Every arrow is exactly `arrow_scale` grid cells; only direction
    /// survives.
    Uniform,
    /// Length is the sampled magnitude itself in world units, multiplied by
    /// `magnitude_scale`, so fields of different magnitude can be compared
    /// directly. The arrow scale does not apply in this mode.
    Magnitude,
}

impl ArrowLengthMode {
    /// Every mode, in the order the Display selector shows them.
    pub const ALL: [ArrowLengthMode; 3] = [
        ArrowLengthMode::Proportional,
        ArrowLengthMode::Uniform,
        ArrowLengthMode::Magnitude,
    ];

    /// Short label for the mode selector.
    pub fn label(self) -> &'static str {
        match self {
            ArrowLengthMode::Proportional => "Proportional",
            ArrowLengthMode::Uniform => "Uniform",
            ArrowLengthMode::Magnitude => "Magnitude",
        }
    }

    /// One-line explanation, short enough for a tooltip.
    pub fn tooltip(self) -> &'static str {
        match self {
            ArrowLengthMode::Proportional => {
                "Length follows magnitude; the longest arrow is the arrow length scale"
            }
            ArrowLengthMode::Uniform => {
                "Every arrow has the same length, keeping only its direction"
            }
            ArrowLengthMode::Magnitude => "Length is the field magnitude in world units",
        }
    }

    /// The mode a legacy persisted `normalize` bool means: `true` painted every
    /// arrow the same length, `false` kept the magnitude proportion.
    pub fn from_normalize(normalize: bool) -> Self {
        if normalize { ArrowLengthMode::Uniform } else { ArrowLengthMode::Proportional }
    }
}

/// How streamlines get their seeds (ticket 50). The Feature toggle still gates
/// both sources; this mode only picks which seeds the on state paints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum StreamlineSeedMode {
    /// The automatic jittered grid seeds plus any click seeds. The default, so
    /// a left click is never dead.
    #[default]
    Auto,
    /// Click seeds only; the automatic grid seeds are hidden.
    Click,
}

impl StreamlineSeedMode {
    /// Every mode, in the order the sidebar selector shows them.
    pub const ALL: [StreamlineSeedMode; 2] =
        [StreamlineSeedMode::Auto, StreamlineSeedMode::Click];

    /// Short label for the selector.
    pub fn label(self) -> &'static str {
        match self {
            StreamlineSeedMode::Auto => "Auto",
            StreamlineSeedMode::Click => "Click",
        }
    }

    /// One-line explanation, short enough for a tooltip.
    pub fn tooltip(self) -> &'static str {
        match self {
            StreamlineSeedMode::Auto => "Show the automatic grid seeds and accept clicks",
            StreamlineSeedMode::Click => "Show only the seeds planted by clicks",
        }
    }
}

/// The state the user can touch: what field, how dense, how coloured, how
/// projected, from where.
pub struct State {
    pub mode_3d: bool,
    pub formula: String,
    pub field: FieldExpr,
    pub parse_error: String,
    pub density: u32,
    pub domain: Domain,
    pub arrow_scale: f32,
    pub arrow_length_mode: ArrowLengthMode,
    pub magnitude_scale: f32,
    pub colormap: Colormap,
    pub auto_fit: bool,
    pub color_min: f32,
    pub color_max: f32,
    pub arrows: bool,
    /// The 3D arrow family (ticket 45); it never selects the 2D arrow.
    pub arrow_style: ArrowStyle,
    /// The 2D arrow family (ticket 61); independent of [`State::arrow_style`].
    pub arrow_style_2d: ArrowStyle2D,
    /// The head/shaft proportions shared by every 3D family.
    pub arrow_params: ArrowParams,
    pub streamlines: bool,
    /// Which seeds the on state paints (ticket 50): the automatic grid, the
    /// click bursts, or both.
    pub seed_mode: StreamlineSeedMode,
    pub particles: bool,
    /// Which sampled slice plane-by-plane mode highlights (ticket 23).
    pub plane: PlaneView,
    pub dirty: bool,
}

impl State {
    /// Flag the state as needing a fresh scene, and report that it did.
    pub fn mark_dirty(self: &mut State) -> bool {
        self.dirty = true;
        true
    }

    /// Sampling runs three-dimensional only when the mode is 3D and the
    /// field actually carries three components.
    pub fn is_3d(self: &State) -> bool {
        self.mode_3d && self.field.dimension == 3
    }

    /// A fresh state for a 2D or 3D field at the defaults.
    pub fn new(mode_3d: bool) -> State {
        let formula = if mode_3d { FORMULA_3D } else { FORMULA_2D };
        let field = FieldExpr::parse(formula).expect("default formula must parse");
        let density = if mode_3d { DEFAULT_DENSITY_3D } else { DEFAULT_DENSITY_2D };
        let grid = if mode_3d && field.dimension == 3 {
            UVec3::splat(density)
        } else {
            UVec3::new(density, density, 1)
        };
        State {
            mode_3d,
            formula: formula.to_string(),
            field,
            parse_error: String::new(),
            density,
            domain: Domain::default(),
            arrow_scale: 1.0,
            arrow_length_mode: ArrowLengthMode::Proportional,
            magnitude_scale: DEFAULT_MAGNITUDE_SCALE,
            colormap: Colormap::Viridis,
            auto_fit: true,
            color_min: 0.0,
            color_max: 1.0,
            arrows: true,
            arrow_style: ArrowStyle::Cone,
            arrow_style_2d: ArrowStyle2D::Flat,
            arrow_params: ArrowParams::default(),
            streamlines: false,
            seed_mode: StreamlineSeedMode::Auto,
            particles: false,
            plane: PlaneView::middle(grid),
            dirty: true,
        }
    }

    /// The formula the field bar holds. Re-parsing it marks the scene dirty
    /// when the field actually changed; a formula that does not parse is
    /// reported in [`parse_error`] and leaves [`field`] at the last one that
    /// parsed, so the viewport keeps showing the previous field.
    pub fn set_formula(self: &mut State, formula: String) -> bool {
        if self.formula == formula {
            return false;
        }
        let parsed = FieldExpr::parse(formula.as_str());
        self.formula = formula;
        match parsed {
            Ok(field) => {
                let previous_grid = self.grid();
                self.field = field;
                self.parse_error = String::new();
                self.sync_plane(previous_grid);
                self.mark_dirty()
            }
            Err(error) => {
                // Once per formula change, never per frame: the status bar
                // still shows the message; this is the stderr/console trail.
                log::debug!("field parse failed: {error}");
                self.parse_error = error.to_string();
                self.mark_dirty()
            }
        }
    }

    /// The projection: an orthographic front view for 2D, the orbiting
    /// perspective camera for 3D.
    pub fn set_mode(self: &mut State, mode_3d: bool) -> bool {
        if self.mode_3d == mode_3d {
            return false;
        }
        let previous_grid = self.grid();
        self.mode_3d = mode_3d;
        self.sync_plane(previous_grid);
        self.mark_dirty()
    }

    /// Samples per axis, clamped to what this mode can afford.
    pub fn set_density(self: &mut State, density: u32) -> bool {
        let max = if self.is_3d() { MAX_DENSITY_3D } else { MAX_DENSITY_2D };
        let density = density.clamp(MIN_DENSITY, max);
        if self.density == density {
            return false;
        }
        let previous_grid = self.grid();
        self.density = density;
        self.sync_plane(previous_grid);
        self.mark_dirty()
    }

    /// The domain to explore, with the corners normalised so that min is
    /// actually the minimum: dragging the sliders past each other swaps them
    /// rather than producing an empty domain.
    pub fn set_domain(self: &mut State, min: Vec3, max: Vec3) -> bool {
        let domain = domain_between(min, max);
        if self.domain == domain {
            return false;
        }
        let previous_grid = self.grid();
        self.domain = domain;
        self.sync_plane(previous_grid);
        self.mark_dirty()
    }

    /// How long to draw arrows, as a multiple of the grid spacing.
    pub fn set_arrow_scale(self: &mut State, scale: f32) -> bool {
        let scale = scale.clamp(MIN_ARROW_SCALE, MAX_ARROW_SCALE);
        if self.arrow_scale == scale {
            return false;
        }
        self.arrow_scale = scale;
        self.mark_dirty()
    }

    /// Choose how magnitude becomes arrow length.
    pub fn set_arrow_length_mode(self: &mut State, mode: ArrowLengthMode) -> bool {
        if self.arrow_length_mode == mode {
            return false;
        }
        self.arrow_length_mode = mode;
        self.mark_dirty()
    }

    /// World length per unit of field magnitude, used only by
    /// [`ArrowLengthMode::Magnitude`]. The value is clamped into
    /// [`MIN_MAGNITUDE_SCALE`]..=[`MAX_MAGNITUDE_SCALE`] and a NaN is rejected
    /// (the previous scale stays), but the resulting arrow length is never
    /// clamped: the user chose the scale and the camera can still fit the
    /// domain around an extreme arrow.
    pub fn set_magnitude_scale(self: &mut State, scale: f32) -> bool {
        if scale.is_nan() {
            return false;
        }
        let scale = scale.clamp(MIN_MAGNITUDE_SCALE, MAX_MAGNITUDE_SCALE);
        if self.magnitude_scale == scale {
            return false;
        }
        self.magnitude_scale = scale;
        self.mark_dirty()
    }

    /// The palette that maps magnitude to colour.
    pub fn set_colormap(self: &mut State, colormap: Colormap) -> bool {
        if self.colormap == colormap {
            return false;
        }
        self.colormap = colormap;
        self.mark_dirty()
    }

    /// Whether to colour by the field's own magnitude range, or by one the
    /// user set.
    pub fn set_auto_fit(self: &mut State, auto_fit: bool) -> bool {
        if self.auto_fit == auto_fit {
            return false;
        }
        self.auto_fit = auto_fit;
        self.mark_dirty()
    }

    /// A magnitude range the user set, used when [`auto_fit`] is off. The
    /// bounds are swapped if given the wrong way round.
    pub fn set_color_range(self: &mut State, min: f32, max: f32) -> bool {
        let lo = min.min(max);
        let hi = min.max(max);
        if self.color_min == lo && self.color_max == hi {
            return false;
        }
        self.color_min = lo;
        self.color_max = hi;
        self.mark_dirty()
    }

    /// The field's arrows.
    pub fn set_arrows(self: &mut State, arrows: bool) -> bool {
        if self.arrows == arrows {
            return false;
        }
        self.arrows = arrows;
        self.mark_dirty()
    }

    /// Choose the 3D arrow shape family. The 2D family is independent.
    pub fn set_arrow_style(self: &mut State, style: ArrowStyle) -> bool {
        if self.arrow_style == style {
            return false;
        }
        self.arrow_style = style;
        self.mark_dirty()
    }

    /// Choose the 2D arrow family (ticket 61). The 3D family is independent.
    pub fn set_arrow_style_2d(self: &mut State, style: ArrowStyle2D) -> bool {
        if self.arrow_style_2d == style {
            return false;
        }
        self.arrow_style_2d = style;
        self.mark_dirty()
    }

    /// Set the head/shaft proportions, sanitized through the one clamp the
    /// persistence restore and the sidebar share.
    pub fn set_arrow_params(self: &mut State, params: ArrowParams) -> bool {
        let params = params.sanitized();
        if self.arrow_params == params {
            return false;
        }
        self.arrow_params = params;
        self.mark_dirty()
    }

    /// Stream tracing, not yet drawn.
    pub fn set_streamlines(self: &mut State, on: bool) -> bool {
        if self.streamlines == on {
            return false;
        }
        self.streamlines = on;
        self.mark_dirty()
    }

    /// Choose which seeds the on state paints (ticket 50). The mode only swaps
    /// what is drawn or stored; the Feature toggle still hides both.
    pub fn set_seed_mode(self: &mut State, mode: StreamlineSeedMode) -> bool {
        if self.seed_mode == mode {
            return false;
        }
        self.seed_mode = mode;
        self.mark_dirty()
    }

    /// Whether a viewport click may add to the app's click-seed list. The list
    /// itself is app-owned, but the gate belongs to the same Feature toggle
    /// that hides the click lines: off hides both and refuses new seeds, so
    /// re-enabling reproduces the previous clicks. Both modes accept clicks.
    pub fn accepts_click_seed(self: &State) -> bool {
        self.streamlines
    }

    /// Particle tracing, not yet drawn.
    pub fn set_particles(self: &mut State, on: bool) -> bool {
        if self.particles == on {
            return false;
        }
        self.particles = on;
        self.mark_dirty()
    }

    /// The plane-by-plane selection, clamped into the current grid. The index
    /// is a user choice once the grid has more than one slice, so toggling the
    /// mode keeps it; only a grid change recentres it (see [`sync_plane`]).
    pub fn set_plane(self: &mut State, plane: PlaneView) -> bool {
        let plane = plane.clamped(self.grid());
        if self.plane == plane {
            return false;
        }
        self.plane = plane;
        self.mark_dirty()
    }

    /// Toggle plane-by-plane inspection.
    pub fn set_plane_enabled(self: &mut State, enabled: bool) -> bool {
        let plane = PlaneView { enabled, ..self.plane };
        self.set_plane(plane)
    }

    /// Choose the axis the selected plane is perpendicular to. The index is
    /// clamped into the new axis (a one-slice axis collapses to 0).
    pub fn set_plane_axis(self: &mut State, axis: PlaneAxis) -> bool {
        let plane = PlaneView { axis, ..self.plane };
        self.set_plane(plane)
    }

    /// Step the selected plane along its axis; out-of-range values clamp.
    pub fn set_plane_index(self: &mut State, index: u32) -> bool {
        let plane = PlaneView { index, ..self.plane };
        self.set_plane(plane)
    }

    /// Whether plane-by-plane dimming applies: 3D sampling, the mode is on,
    /// and the chosen axis really has more than one slice. 2D mode and a flat
    /// field in 3D mode are therefore untouched.
    pub fn plane_active(self: &State) -> bool {
        self.is_3d() && self.plane.active(self.grid())
    }

    /// Keep the plane index valid after the sampling grid changes shape:
    /// collapse a one-slice axis to 0, recentre a plane that no longer exists,
    /// and — leaving a one-slice axis — start at the middle plane, because
    /// index 0 on a single slice was never a choice. Called by every setter
    /// that changes the grid (density, mode, formula dimension, domain) with
    /// the grid that was in force before the change.
    fn sync_plane(self: &mut State, previous_grid: UVec3) {
        let grid = self.grid();
        let count = self.plane.axis.count(grid);
        let previous_count = self.plane.axis.count(previous_grid);
        if count <= 1 {
            self.plane.index = 0;
        } else if previous_count <= 1 || self.plane.index >= count {
            self.plane.index = self.plane.axis.middle_index(grid);
        }
    }

    /// The world half-height the 2D orthographic window covers: the domain's
    /// larger horizontal extent, padded so the field fills the window.
    pub fn plane_size(self: &State) -> f32 {
        let size = self.domain.domain_size();
        size.x.max(size.y) * PLANE_MARGIN
    }

    /// The sampling grid for the density: `n`×`n` (one z slice) in 2D mode,
    /// `n`×`n`×`n` when sampling runs three-dimensional.
    pub fn grid(self: &State) -> UVec3 {
        let n = self.density.max(MIN_DENSITY);
        if self.is_3d() {
            UVec3::new(n, n, n)
        } else {
            UVec3::new(n, n, 1)
        }
    }

    /// The colour settings projected onto what the sampler consumes: with
    /// auto-fit on the range is `None` so the sampler fits the sampled
    /// magnitudes itself.
    pub fn view_options(self: &State) -> ViewOptions {
        ViewOptions {
            colormap: self.colormap,
            color_range: if self.auto_fit { None } else { Some((self.color_min, self.color_max)) },
        }
    }

    /// Resample the field with this state's grid and colour settings.
    pub fn sample(self: &State) -> Vec<ArrowData> {
        sample_field(&self.field, &self.domain, self.grid(), &self.view_options())
    }

    /// Size of one grid cell along the tightest axis: arrow lengths are
    /// multiples of this, so a denser grid keeps arrows the same relative
    /// size instead of piling them up.
    pub fn spacing(self: &State) -> f32 {
        let grid = self.grid();
        let size = self.domain.domain_size();
        let cell = |extent, n| if n <= 1 { extent } else { extent / (n - 1) as f32 };
        let tightest = cell(size.x, grid.x).min(cell(size.y, grid.y));
        if grid.z > 1 {
            tightest.min(cell(size.z, grid.z))
        } else {
            tightest
        }
    }

    /// The colour range the painted field geometry shares: the state's explicit
    /// bounds when auto-fit is off, otherwise the magnitudes of `sampled`. The
    /// streamlines built from `sampled` and the app's live dot cloud both ask
    /// for this, so dots always ramp the same colours as the arrows and
    /// streamline segments next to them.
    pub fn color_range(self: &State, sampled: &[ArrowData]) -> (f32, f32) {
        self.view_options().color_range.unwrap_or_else(|| magnitude_range(sampled))
    }

    /// Convert sampled arrows into GPU instances at this state's arrow
    /// lengths. `arrow_scale` is the length multiplier for
    /// [`ArrowLengthMode::Proportional`] (longest arrow exactly `arrow_scale`
    /// grid cells, shorter ones following their magnitude) and
    /// [`ArrowLengthMode::Uniform`] (every arrow exactly `arrow_scale` cells).
    /// With [`ArrowLengthMode::Magnitude`] the length is instead
    /// `magnitude_scale * mag` in world units — `arrow_scale` does not apply,
    /// and the result is deliberately not clamped, so a field with a huge
    /// magnitude produces a huge arrow and the camera does the fitting.
    ///
    /// Plane-by-plane mode (ticket 23) keeps the selected slice's colormap
    /// colour at full alpha and lerps every other arrow toward grey at
    /// [`DIM_ALPHA`]. Sampling still produced the whole cube, so stepping the
    /// plane only rebuilds these instances, never the field.
    pub fn arrow_instances(self: &State, arrows: &[ArrowData]) -> Vec<ArrowInstanceGPU> {
        let unit = self.arrow_scale * self.spacing();
        let max_mag = arrows.iter().fold(0.0_f32, |m, a| m.max(a.mag));
        let plane_active = self.plane_active();
        let grid = self.grid();
        let domain = self.domain;
        arrows
            .iter()
            .map(|a| {
                let len = match self.arrow_length_mode {
                    ArrowLengthMode::Uniform => unit,
                    ArrowLengthMode::Proportional => {
                        if max_mag <= 0.0 { unit } else { unit * (a.mag / max_mag) }
                    }
                    ArrowLengthMode::Magnitude => a.mag * self.magnitude_scale,
                };
                let (rgb, alpha) =
                    if plane_active && !on_plane(a.position, &domain, grid, &self.plane) {
                        (dim_color(a.color), DIM_ALPHA)
                    } else {
                        (a.color, 1.0)
                    };
                ArrowInstanceGPU {
                    origin: [a.position.x, a.position.y, a.position.z, 1.0],
                    dir_len: [a.dir.x, a.dir.y, a.dir.z, len],
                    color: [
                        rgb[0] as f32 / 255.0,
                        rgb[1] as f32 / 255.0,
                        rgb[2] as f32 / 255.0,
                        alpha,
                    ],
                }
            })
            .collect()
    }

    /// Line instances for the 2D streamlines: a jittered seed grid (deterministic
    /// across rebuilds, salt-keyed to the sampling grid so a density change
    /// reseeds), each seed RK4-traced through the field, one instance per
    /// segment coloured like the arrows it crosses — same magnitude range as
    /// the arrows. [`STREAMLINE_MAX_LEN`] sets the per-line point budget and
    /// the grid spacing sets the step scale. Streamlines are a static trace,
    /// not clock consumers, so the transport speed never changes their length.
    /// Both views trace the same RK4 [`streamlines::trace`] through the same
    /// 3D-sampled field: the pan/zoom view uses the jittered grid of the view
    /// plane, while the orbit view uses the same grid at the domain's mid-depth
    /// so its lines run with the depth they cross. [`streamlines::ray_seed`]
    /// turns a click into a seed inside the domain for the orbit view's click-to-seed.
    pub fn streamlines_snapshot(self: &State, sampled: &[ArrowData]) -> Vec<LineInstanceGPU> {
        // Off hides both seed sources; Click hides only the automatic grid.
        if !self.streamlines || self.seed_mode == StreamlineSeedMode::Click {
            return Vec::new();
        }
        // Half the grid density seeds the plane: enough lines to read the flow
        // without tracing one per sampled point.
        let axis = (self.density / 2).clamp(4, 24);
        let grid = self.grid();
        let salt = grid.x as u64 ^ ((grid.y as u64) << 16);
        let seeds = seed_grid(&self.domain, axis, STREAMLINE_JITTER, salt);
        let opts = self.streamline_opts();
        let range = self.color_range(sampled);
        let mut lines: Vec<LineInstanceGPU> = Vec::new();
        for seed in seeds {
            self.push_line(&mut lines, seed, &opts, range);
        }
        lines
    }

    /// The tracing parameters both seed sources share: a fixed point budget
    /// ([`STREAMLINE_MAX_LEN`], bounded inside the tracer's own 400-point cap)
    /// and a step scale from the grid spacing. Streamlines are static, so no
    /// transport or particle setting changes these.
    fn streamline_opts(self: &State) -> StreamlineOpts {
        StreamlineOpts {
            max_len: STREAMLINE_MAX_LEN,
            step_scale: 0.5 * self.spacing(),
            both_directions: true,
        }
    }

    /// Trace one seed and push its coloured segments: one instance per segment,
    /// coloured at the segment's start by the same [`color_range`] the arrows
    /// use. Shared by the grid seeds and the click bursts so both paint
    /// identically.
    fn push_line(
        self: &State,
        lines: &mut Vec<LineInstanceGPU>,
        seed: Vec3,
        opts: &StreamlineOpts,
        range: (f32, f32),
    ) {
        let (lo, hi) = range;
        let path = trace(&self.field, seed, &self.domain, opts);
        for i in 0..path.len().saturating_sub(1) {
            let pa = path[i];
            let pb = path[i + 1];
            let mag = match self.field.eval3(pa) {
                Some(v) => v.length(),
                None => 0.0,
            };
            let rgb = self.colormap.colormap(mag, lo, hi);
            lines.push(LineInstanceGPU {
                a: [pa.x, pa.y, pa.z, 1.0],
                b: [pb.x, pb.y, pb.z, STREAMLINE_WIDTH],
                color: [
                    rgb[0] as f32 / 255.0,
                    rgb[1] as f32 / 255.0,
                    rgb[2] as f32 / 255.0,
                    1.0,
                ],
            });
        }
    }

    /// Line instances for the click-seeded streamlines: every click expands to
    /// a deterministic [`burst_seeds`] cluster (3D depth in the orbit view, on
    /// the clicked plane in 2D), each seed traced exactly like a grid seed.
    /// Both seed modes accept clicks, but the Feature toggle gates them like
    /// the grid: off hides every click line and the app stores no new seeds.
    /// Pure; the app owns the (bounded) list of clicks.
    pub fn click_streamlines_snapshot(
        self: &State,
        color_range: (f32, f32),
        clicks: &[Vec3],
    ) -> Vec<LineInstanceGPU> {
        if !self.streamlines {
            return Vec::new();
        }
        let opts = self.streamline_opts();
        let dimension = if self.mode_3d { 3 } else { 2 };
        let mut lines: Vec<LineInstanceGPU> = Vec::new();
        for click in clicks {
            for seed in burst_seeds(*click, &self.domain, CLICK_BURST_SEEDS, dimension) {
                self.push_line(&mut lines, seed, &opts, color_range);
            }
        }
        lines
    }

    /// The whole GPU scene for this state: arrows plus streamlines —
    /// particle tasks add `dots` later. Pure: no GPU resources touched, so
    /// the UI layer just stores the result; the field is sampled once and
    /// reused for the arrows and the streamline colour range.
    pub fn snapshot(self: &State) -> SceneGPU {
        let sampled = self.sample();
        let arrows = if self.arrows { self.arrow_instances(&sampled) } else { Vec::new() };
        SceneGPU {
            arrows: Arc::new(arrows),
            lines: Arc::new(self.streamlines_snapshot(&sampled)),
            dots: Arc::new(Vec::new()),
            // Ticket 62: the app builds and sorts the per-frame combined
            // arrow + particle stream, because dots advect every frame. The
            // viewmodel's arrows stay frame-invariant in sampling order.
            combined: Arc::new(Vec::new()),
            arrow_style: self.arrow_style,
            arrow_style_2d: self.arrow_style_2d,
            arrow_params: self.arrow_params,
            mode_3d: self.mode_3d,
        }
    }

    /// Rebuild the scene only when the state has been marked dirty since the
    /// last rebuild: `Some` means the caller must replace its scene, `None`
    /// means the last snapshot is still current.
    pub fn snapshot_if_dirty(self: &mut State) -> Option<SceneGPU> {
        if !self.dirty {
            return None;
        }
        self.dirty = false;
        Some(self.snapshot())
    }
}

/// The axis-aligned box between two corners, swapping per axis so `min`
/// really is the minimum (brief Step 2: "swap if violated").
pub fn domain_between(min: Vec3, max: Vec3) -> Domain {
    Domain::new(
        Vec3::new(min.x.min(max.x), min.y.min(max.y), min.z.min(max.z)),
        Vec3::new(min.x.max(max.x), min.y.max(max.y), min.z.max(max.z)),
    )
}

/// Clamp one domain extent into [`MIN_DOMAIN_EXTENT`]..=[`MAX_DOMAIN_EXTENT`].
///
/// This is the single bound the domain editor and its per-frame handoff use,
/// so the widgets and the seam cannot drift apart. Infinities clamp to the
/// nearer edge; NaN has no direction and becomes `0`. The call site keeps the
/// *previous* extent for NaN instead (a dragged box should not jump to zero),
/// but the pure function still promises a finite, in-range result.
pub fn clamp_domain_extent(value: f32) -> f32 {
    if value.is_nan() {
        0.0
    } else {
        value.clamp(MIN_DOMAIN_EXTENT, MAX_DOMAIN_EXTENT)
    }
}

/// Radians of orbit per point of left-drag in the 3D view.
pub const ORBIT_SENSITIVITY: f32 = 0.005;

/// One frame of viewport mouse input, in logical points and egui's zoom
/// convention (`zoom > 1` means the user asked to get closer). `drag` is the
/// orbit channel, `pan` the pan channel; [`drag_channel`] maps the held buttons
/// onto them (ticket 18), then each mode maps the channels onto its own
/// navigation (see [`apply_camera_input`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewGesture {
    /// Orbit-channel drag delta, points (3D: orbit; 2D: pan).
    pub drag: Vec2,
    /// Pan-channel drag delta, points (pans both views).
    pub pan: Vec2,
    /// Wheel/pinch zoom factor; 1.0 is no change.
    pub zoom: f32,
    /// Viewport height in points, used to scale a pan like the picture
    /// under the pointer.
    pub viewport_height: f32,
}

impl Default for ViewGesture {
    fn default() -> Self {
        Self {
            drag: Vec2::ZERO,
            pan: Vec2::ZERO,
            zoom: 1.0,
            viewport_height: 1.0,
        }
    }
}

/// Pure camera action for one frame of mouse input.
///
/// 3D: a primary drag orbits ([`ORBIT_SENSITIVITY`] rad/point), a secondary or
/// middle drag pans the orbit target in the view plane, and the wheel/pinch
/// factor scales the orbit distance (clamped by the camera).
///
/// 2D: a drag pans the fixed front view along **world X/Y** (the view is
/// looking down -Z, so screen axes are world axes); the wheel/pinch factor
/// multiplies the ortho zoom (clamped). Panning moves the target exactly with
/// the pointer: one screen point of drag is one screen point of world motion at
/// the current visible height, so the scene sticks to the cursor.
///
/// Ticket 14 supersedes ticket 02's "2D ignores gestures" rule: the 2D view is
/// now navigable. Returns the new camera.
pub fn apply_camera_input(
    camera: OrbitCamera,
    mode_3d: bool,
    plane_size: f32,
    gesture: ViewGesture,
) -> OrbitCamera {
    let mut camera = camera;
    let zoom = if gesture.zoom.is_finite() && gesture.zoom > 0.0 {
        gesture.zoom
    } else {
        1.0
    };
    let height = if gesture.viewport_height.is_finite() && gesture.viewport_height > 0.0 {
        gesture.viewport_height
    } else {
        1.0
    };
    let plane_size = if plane_size.is_finite() && plane_size > 0.0 {
        plane_size
    } else {
        1.0
    };

    if mode_3d {
        camera.orbit(gesture.drag.x * ORBIT_SENSITIVITY, gesture.drag.y * ORBIT_SENSITIVITY);
        // egui reports >1 for "closer"; the orbit camera's zoom is a distance
        // multiplier, so invert it.
        camera.zoom(1.0 / zoom);
        let world_per_point =
            2.0 * camera.distance * (camera.fov_y * 0.5).tan() / height;
        camera.pan(-gesture.pan.x * world_per_point, gesture.pan.y * world_per_point);
    } else {
        // The 2D view is a fixed front view (camera::OrbitCamera::front_view),
        // so its screen basis is world X (right) and world Y (up) regardless of
        // the orbit angles. A NaN zoom falls back to 1 before scaling the pan
        // (review M-3).
        let ortho_zoom = if camera.ortho_zoom.is_finite() && camera.ortho_zoom > 0.0 {
            camera.ortho_zoom
        } else {
            1.0
        };
        let world_per_point = (plane_size / ortho_zoom) / height;
        // Both buttons pan in 2D: a left drag is the 2D navigation gesture.
        let pan = gesture.drag + gesture.pan;
        camera.target.x -= pan.x * world_per_point;
        camera.target.y += pan.y * world_per_point;
        camera.zoom_2d(zoom);
    }
    camera
}

/// Which pointer buttons are dragging this frame, plus the Shift modifier.
/// Plain data so the button→gesture mapping is testable without a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DragButtons {
    pub primary: bool,
    pub secondary: bool,
    pub middle: bool,
    pub shift: bool,
}

/// The camera channel a drag feeds. [`ViewGesture`] carries one delta per
/// channel, so the mapping decides which field gets the pointer delta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragChannel {
    /// Orbit the 3D view.
    Orbit,
    /// Pan the view (both modes).
    Pan,
}

/// Map the held pointer buttons onto a camera channel (ticket 18).
///
/// 3D: left-drag and right-drag orbit; middle-drag and Shift+right-drag pan
/// (the trackpad/no-middle-button fallback). 2D: every button pans — the fixed
/// front view has nothing to orbit. A middle button is the most explicit pan,
/// then the secondary button, then the primary one.
///
/// The wasm build (ticket 16) reuses this mapping and only has to suppress the
/// browser context menu on the secondary button.
pub fn drag_channel(mode_3d: bool, buttons: DragButtons) -> Option<DragChannel> {
    if buttons.middle {
        return Some(DragChannel::Pan);
    }
    if buttons.secondary {
        return Some(if mode_3d && !buttons.shift {
            DragChannel::Orbit
        } else {
            DragChannel::Pan
        });
    }
    if buttons.primary {
        return Some(if mode_3d { DragChannel::Orbit } else { DragChannel::Pan });
    }
    None
}

/// What a completed pointer gesture in the viewport should do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickOutcome {
    /// The gesture never became a drag and was a single click: seed the flow.
    Seed,
    /// A double-click: reset the view, never seed.
    ResetView,
    /// No click (a drag, a hover, or nothing).
    None,
}

/// Decide what one viewport gesture means. egui reports `clicked` for every
/// completed click including the second half of a double-click, so the
/// double-click check comes first: the reset wins and the click never seeds.
pub fn click_outcome(clicked: bool, double_clicked: bool, dragged: bool) -> ClickOutcome {
    if double_clicked {
        ClickOutcome::ResetView
    } else if clicked && !dragged {
        ClickOutcome::Seed
    } else {
        ClickOutcome::None
    }
}

/// What a captured (pointer-locked) viewport gesture does. Capture is
/// FPS-modal, so there is deliberately no reset variant: every click seeds at
/// the crosshair, and the second click of a double-click seeds again. A drag
/// never seeds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapturedClickOutcome {
    /// Seed at the viewport centre (NDC `(0, 0)`).
    SeedCentre,
    /// No click (a drag, a hover, or nothing).
    Ignore,
}

/// Decide a captured viewport gesture. Unlike [`click_outcome`], a double
/// click cannot become [`ClickOutcome::ResetView`]: reset belongs to orbit
/// mode, so a captured second click seeds again.
pub fn captured_click_outcome(clicked: bool, dragged: bool) -> CapturedClickOutcome {
    if clicked && !dragged {
        CapturedClickOutcome::SeedCentre
    } else {
        CapturedClickOutcome::Ignore
    }
}

/// How long after a single click a second click still counts as a double-click,
/// in seconds. Matches egui's default `max_double_click_delay`: the app uses it
/// to take back the first click's seed when a double-click resets the view.
pub const DOUBLE_CLICK_WINDOW: f64 = 0.3;

/// The seed the last single click planted, so a double-click can remove it
/// again: a double-click resets the view and must not seed the flow.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LastClickSeed {
    pub seed: Vec3,
    /// Particle count after any ambient spawn but before the click's burst.
    pub particles_before: usize,
    /// egui's frame time when the click happened.
    pub time: f64,
    /// Whether [`State::accepts_click_seed`] let this click into the seed list.
    /// An off-state click still bursts particles, but a double-click must not
    /// delete a retained seed that happens to sit at the same point.
    pub stored: bool,
}

/// What [`undo_click_seed`] did: the seed list after the undo, whether the
/// click's seed was removed, and the particle count to truncate back to when
/// the click's burst should go too.
#[derive(Debug, Clone, PartialEq)]
pub struct ClickSeedUndo {
    pub seeds: Vec<Vec3>,
    pub removed: bool,
    pub particles: Option<usize>,
}

/// Take back the burst the previous single click planted when it turns out to
/// be the first half of a double-click. Pure: the app owns the seed list, the
/// particle cloud and the recompose step.
///
/// A click older than [`DOUBLE_CLICK_WINDOW`] is not a double-click and is left
/// alone. Inside the window, for a stored click exactly the last occurrence of
/// its seed is removed and `particles` reports the pre-click count (when the
/// cloud still has at least that many points, so a truncation is safe).
///
/// An unstored click (`LastClickSeed::stored == false`, e.g. a click made while
/// Streamlines was off) removes nothing: the deterministic pick can coincide
/// with a retained seed, and the off-state promise is that retained clicks stay
/// recoverable. Its particle burst is still rolled back when safe.
pub fn undo_click_seed(
    seeds: &[Vec3],
    last: Option<LastClickSeed>,
    now: f64,
    particle_count: usize,
) -> ClickSeedUndo {
    let unchanged = |seeds: &[Vec3]| ClickSeedUndo {
        seeds: seeds.to_vec(),
        removed: false,
        particles: None,
    };
    let Some(last) = last else {
        return unchanged(seeds);
    };
    if !now.is_finite() || !last.time.is_finite() || now - last.time > DOUBLE_CLICK_WINDOW {
        return unchanged(seeds);
    }
    let rollback = if particle_count >= last.particles_before {
        Some(last.particles_before)
    } else {
        None
    };
    if !last.stored {
        return ClickSeedUndo {
            seeds: seeds.to_vec(),
            removed: false,
            particles: rollback,
        };
    }
    let mut seeds = seeds.to_vec();
    let Some(index) = seeds.iter().rposition(|seed| *seed == last.seed) else {
        return unchanged(&seeds);
    };
    seeds.remove(index);
    ClickSeedUndo {
        seeds,
        removed: true,
        particles: rollback,
    }
}

