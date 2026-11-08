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
use super::renderer::{ArrowInstanceGPU, LineInstanceGPU, SceneGPU};
use super::sampling::{ArrowData, ViewOptions, magnitude_range, sample_field};
use super::streamlines::{CLICK_BURST_SEEDS, StreamlineOpts, burst_seeds, seed_grid, trace};

use std::sync::Arc;

use glam::{UVec3, Vec2, Vec3};

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
/// Bounds of the particle and streamline speed slider.
pub const MIN_SPEED: f32 = 0.1;
pub const MAX_SPEED: f32 = 3.0;
/// Line width streamlines are painted with, world units.
pub const STREAMLINE_WIDTH: f32 = 0.012;
/// Seed jitter as a fraction of a seed-grid cell (`seed_grid` caps it below
/// half a cell so lattice neighbours never swap).
pub const STREAMLINE_JITTER: f32 = 0.45;

/// The starting field for each mode: the Pump preset (2D) and the 3D radial
/// preset, so a fresh app shows something immediately.
pub const FORMULA_2D: &str = "(x, y)";
pub const FORMULA_3D: &str = "(x, y, z)";

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
    pub normalize: bool,
    pub colormap: Colormap,
    pub auto_fit: bool,
    pub color_min: f32,
    pub color_max: f32,
    pub arrows: bool,
    pub streamlines: bool,
    pub particles: bool,
    pub speed: f32,
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
        State {
            mode_3d,
            formula: formula.to_string(),
            field,
            parse_error: String::new(),
            density: if mode_3d { DEFAULT_DENSITY_3D } else { DEFAULT_DENSITY_2D },
            domain: Domain::default(),
            arrow_scale: 1.0,
            normalize: false,
            colormap: Colormap::Viridis,
            auto_fit: true,
            color_min: 0.0,
            color_max: 1.0,
            arrows: true,
            streamlines: false,
            particles: false,
            speed: 1.0,
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
                self.field = field;
                self.parse_error = String::new();
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
        self.mode_3d = mode_3d;
        self.mark_dirty()
    }

    /// Samples per axis, clamped to what this mode can afford.
    pub fn set_density(self: &mut State, density: u32) -> bool {
        let max = if self.is_3d() { MAX_DENSITY_3D } else { MAX_DENSITY_2D };
        let density = density.clamp(MIN_DENSITY, max);
        if self.density == density {
            return false;
        }
        self.density = density;
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
        self.domain = domain;
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

    /// Draw every arrow the same length, keeping only its direction.
    pub fn set_normalize(self: &mut State, normalize: bool) -> bool {
        if self.normalize == normalize {
            return false;
        }
        self.normalize = normalize;
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

    /// Stream tracing, not yet drawn.
    pub fn set_streamlines(self: &mut State, on: bool) -> bool {
        if self.streamlines == on {
            return false;
        }
        self.streamlines = on;
        self.mark_dirty()
    }

    /// Particle tracing, not yet drawn.
    pub fn set_particles(self: &mut State, on: bool) -> bool {
        if self.particles == on {
            return false;
        }
        self.particles = on;
        self.mark_dirty()
    }

    /// How fast traces move, a multiple of the grid spacing per second.
    pub fn set_speed(self: &mut State, speed: f32) -> bool {
        let speed = speed.clamp(MIN_SPEED, MAX_SPEED);
        if self.speed == speed {
            return false;
        }
        self.speed = speed;
        self.mark_dirty()
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
    /// lengths: with `normalize` off, lengths are proportional to magnitude
    /// and the longest arrow is exactly `arrow_scale` grid cells; with it on
    /// every arrow is exactly `arrow_scale` cells.
    pub fn arrow_instances(self: &State, arrows: &[ArrowData]) -> Vec<ArrowInstanceGPU> {
        let unit = self.arrow_scale * self.spacing();
        let max_mag = arrows.iter().fold(0.0_f32, |m, a| m.max(a.mag));
        arrows
            .iter()
            .map(|a| {
                let len = if self.normalize || max_mag <= 0.0 { unit } else { unit * (a.mag / max_mag) };
                ArrowInstanceGPU {
                    origin: [a.position.x, a.position.y, a.position.z, 1.0],
                    dir_len: [a.dir.x, a.dir.y, a.dir.z, len],
                    color: [
                        a.color[0] as f32 / 255.0,
                        a.color[1] as f32 / 255.0,
                        a.color[2] as f32 / 255.0,
                        1.0,
                    ],
                }
            })
            .collect()
    }

    /// Line instances for the 2D streamlines: a jittered seed grid (deterministic
    /// across rebuilds, salt-keyed to the sampling grid so a density change
    /// reseeds), each seed RK4-traced through the field, one instance per
    /// segment coloured like the arrows it crosses — same magnitude range as
    /// the arrows. The speed slider sets the per-line point budget and the
    /// grid spacing sets the step scale, so streamlines advance at a
    /// spacing-multiple per second comparable with arrows and particles.
    /// Both views trace the same RK4 [`streamlines::trace`] through the same
    /// 3D-sampled field: the pan/zoom view uses the jittered grid of the view
    /// plane, while the orbit view uses the same grid at the domain's mid-depth
    /// so its lines run with the depth they cross. [`streamlines::ray_seed`]
    /// turns a click into a seed inside the domain for the orbit view's click-to-seed.
    pub fn streamlines_snapshot(self: &State, sampled: &[ArrowData]) -> Vec<LineInstanceGPU> {
        if !self.streamlines {
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

    /// The tracing parameters both seed sources share: the speed slider sets
    /// the point budget (bounded inside the tracer's 400-point cap) and the
    /// grid spacing sets the step scale, so lines advance at a comparable
    /// spacing-multiple per second in either view.
    fn streamline_opts(self: &State) -> StreamlineOpts {
        let speed = if self.speed.is_finite() { self.speed.clamp(MIN_SPEED, MAX_SPEED) } else { 1.0 };
        StreamlineOpts {
            max_len: (120.0 * speed).round() as usize,
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
    /// Unlike [`streamlines_snapshot`] this ignores the streamline toggle — a
    /// click is an explicit request to see the flow there. Pure; the app owns
    /// the (bounded) list of clicks.
    pub fn click_streamlines_snapshot(
        self: &State,
        color_range: (f32, f32),
        clicks: &[Vec3],
    ) -> Vec<LineInstanceGPU> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colormap::Colormap;
    use glam::Vec3;

    #[test]
    fn new_state_starts_at_default_density() {
        let s2 = State::new(false);
        assert_eq!(s2.density, DEFAULT_DENSITY_2D);
        assert_eq!(s2.field.dimension, 2);
        assert_eq!(s2.formula, FORMULA_2D);
        assert_eq!(s2.parse_error, "");
        assert!(s2.dirty, "a fresh state must ask for a scene");
        let s3 = State::new(true);
        assert_eq!(s3.density, DEFAULT_DENSITY_3D);
        assert_eq!(s3.field.dimension, 3);
    }

    #[test]
    fn domain_extrema_swap_makes_valid_box() {
        let low = Vec3::new(1.0, -5.0, 3.0);
        let high = Vec3::new(-2.0, 7.0, 0.0);
        let d = domain_between(low, high);
        assert_eq!(d.min, Vec3::new(-2.0, -5.0, 0.0));
        assert_eq!(d.max, Vec3::new(1.0, 7.0, 3.0));
        let size = d.domain_size();
        assert!(size.x > 0.0 && size.y > 0.0 && size.z > 0.0, "swapped box must not be empty");
        let mut s = State::new(false);
        let changed = s.set_domain(low, high);
        assert!(changed, "domain actually moved");
        assert_eq!(s.domain.min, d.min);
    }

    #[test]
    fn density_clamped_per_mode() {
        let mut s2 = State::new(false);
        assert!(s2.set_density(2), "below floor clamps up");
        assert_eq!(s2.density, MIN_DENSITY);
        assert!(s2.set_density(999), "above ceiling clamps down");
        assert_eq!(s2.density, MAX_DENSITY_2D);
        assert!(!s2.set_density(1000), "already at the cap");
        let mut s3 = State::new(true);
        assert!(s3.set_density(1), "below floor clamps up");
        assert_eq!(s3.density, MIN_DENSITY);
        assert!(s3.set_density(999), "above ceiling clamps down");
        assert_eq!(s3.density, MAX_DENSITY_3D);
        assert!(!s3.set_density(40), "already at the cap");
    }

    #[test]
    fn garbage_formula_keeps_field_and_reports_error() {
        let mut s = State::new(false);
        let before = s.field.clone();
        let probe = Vec3::new(0.5, -0.25, 0.0);
        assert!(s.set_formula(")(".to_string()), "an invalid edit still marks the state dirty");
        assert!(!s.parse_error.is_empty(), "the parse failure is reported");
        assert!(
            s.field.eval3(probe) == before.eval3(probe),
            "the last good field must stay in place"
        );
        assert!(s.dirty);
    }

    #[test]
    fn valid_formula_replaces_field_and_clears_error() {
        let mut s = State::new(false);
        assert!(s.set_formula(")(".to_string()), "invalid edit reports…");
        assert!(!s.parse_error.is_empty(), "…and sets an error");
        assert!(s.set_formula("(-y, x)".to_string()), "a valid replacement applies");
        assert_eq!(s.parse_error, "", "the error clears");
        assert_eq!(s.field.dimension, 2);
        assert_eq!(s.field.eval3(Vec3::new(2.0, 1.0, 0.0)), Some(Vec3::new(-1.0, 2.0, 0.0)));
        assert!(s.dirty, "a valid replacement marks the scene dirty");
    }

    #[test]
    fn unchanged_setters_report_false_and_stay_clean() {
        let mut s = State::new(false);
        assert!(s.snapshot_if_dirty().is_some(), "fresh state snapshots once");
        assert!(!s.dirty, "rebuild consumes the dirty flag");
        assert!(!s.set_colormap(s.colormap));
        assert!(!s.set_density(s.density));
        assert!(!s.set_formula(s.formula.clone()), "same text is not a change");
        assert!(!s.set_domain(s.domain.min, s.domain.max));
        assert!(!s.set_arrow_scale(s.arrow_scale));
        assert!(!s.set_normalize(s.normalize));
        assert!(!s.set_mode(s.mode_3d));
        assert!(!s.dirty, "a failed setter must not re-dirty the state");
        assert!(s.snapshot_if_dirty().is_none(), "no rebuild when nothing changed");
    }

    #[test]
    fn snapshot_rebuilds_only_when_dirty() {
        let mut s = State::new(false);
        assert!(s.snapshot_if_dirty().is_some());
        assert!(s.set_colormap(Colormap::Turbo), "a real change marks dirty");
        assert!(s.snapshot_if_dirty().is_some(), "a real change rebuilds once");
        assert!(s.snapshot_if_dirty().is_none(), "and only once");
    }

    #[test]
    fn pump_snapshot_has_the_whole_grid() {
        let s = State::new(false);
        let arrows = s.sample();
        assert_eq!(arrows.len(), 576, "24x24 pump grid: 576 sampled arrows");
        let scene = s.snapshot();
        assert_eq!(scene.arrows.len(), 576, "the scene carries the whole grid");
        assert!(scene.lines.is_empty(), "streamlines join in Task 9");
        assert!(scene.dots.is_empty(), "particles join in Task 10");
    }

    #[test]
    fn normalized_arrows_share_one_length() {
        let mut s = State::new(false);
        assert!(s.set_normalize(true), "normalize toggle marks dirty");
        let inst = s.arrow_instances(&s.sample());
        let expected = s.arrow_scale * s.spacing();
        assert!(expected > 0.0);
        for a in &inst {
            assert!((a.dir_len[3] - expected).abs() < 1e-5, "every normalized arrow is arrow_scale cells long");
        }
    }

    #[test]
    fn speaking_arrows_are_magnitude_proportional() {
        let s = State::new(false);
        let arrows = s.sample();
        let inst = s.arrow_instances(&arrows);
        let lengths: Vec<f32> = inst.iter().map(|a| a.dir_len[3]).collect();
        let expected_max = s.arrow_scale * s.spacing();
        let max_len = lengths.iter().fold(0.0_f32, |m, l| m.max(*l));
        assert!((max_len - expected_max).abs() < 1e-5, "longest arrow is exactly arrow_scale cells");
        for l in &lengths {
            assert!(*l <= expected_max + 1e-5, "no arrow exceeds arrow_scale cells");
        }
        let by_mag: Vec<(f32, f32)> = arrows
            .iter()
            .zip(lengths)
            .map(|(a, l)| (a.mag, l))
            .collect();
        let mut sorted = by_mag.clone();
        sorted.sort_by(|(a, _), (b, _)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        assert!(sorted.len() > 1, "need at least two arrows to compare");
        for i in 1..sorted.len() {
            assert!(
                sorted[i - 1].1 <= sorted[i].1 + 1e-5,
                "length must follow magnitude (speaking arrows)",
            );
        }
    }

    #[test]
    fn color_range_follows_auto_fit() {
        let mut s = State::new(false);
        let v = s.view_options();
        assert!(v.color_range.is_none(), "auto-fit lets the sampler fit colors");
        assert_eq!(v.colormap, Colormap::Viridis);
        assert!(s.set_auto_fit(false), "auto-fit off marks dirty");
        assert!(s.set_color_range(2.0, -3.0), "bounds swap and apply");
        assert_eq!(s.color_min, -3.0);
        assert_eq!(s.color_max, 2.0);
        assert_eq!(s.view_options().color_range, Some((-3.0, 2.0)));
    }

    #[test]
    fn density_clamps_to_dimension_being_sampled() {
        let mut s = State::new(false);
        assert!(s.set_mode(true), "switching to 3D mode marks dirty");
        assert!(s.set_density(MAX_DENSITY_2D), "a flat field still affords the 2D cap");
        assert_eq!(s.density, MAX_DENSITY_2D, "cap keys to the sampled dimension, not the mode");
        assert_eq!(s.grid().z, 1, "a 2D field still samples flat in 3D mode");
    }

    #[test]
    fn plane_size_scales_with_largest_extent() {
        let mut s = State::new(false);
        let size = s.plane_size();
        assert!((size - 2.7).abs() < 1e-4, "default 2-unit domain times the margin");
        assert!(
            s.set_domain(Vec3::new(-2.0, 0.0, 0.0), Vec3::new(2.0, 1.0, 0.0)),
            "a wider-than-tall domain applies",
        );
        let grown = s.plane_size();
        assert!((grown - 5.4).abs() < 1e-4, "follows the x extent once it is largest");
    }

    #[test]
    fn snapshot_reports_active_mode() {
        let mut s = State::new(false);
        assert!(s.set_mode(true), "3D mode on a 2D field marks dirty");
        let scene = s.snapshot();
        assert_eq!(
            scene.arrows.len(),
            (DEFAULT_DENSITY_2D * DEFAULT_DENSITY_2D) as usize,
            "the flat sample count, not the cubic one",
        );
        let flat = scene.arrows.iter().all(|a| a.origin[2] == 0.0);
        assert!(flat, "every arrow origin sits on the z = 0 plane when sampling runs flat");
    }

    #[test]
    fn two_d_streamlines_snapshot_emits_line_segments() {
        let mut s = State::new(false);
        assert!(s.set_streamlines(true), "enabling streamlines marks the scene dirty");
        let scene = s.snapshot();
        assert!(!scene.lines.is_empty(), "enabled streamlines paint at least one line segment");
        assert!(scene.dots.is_empty(), "particles still wait for their task");
    }

    #[test]
    fn streamlines_run_in_both_modes_and_off_by_default() {
        let mut s3 = State::new(true);
        assert!(s3.set_streamlines(true), "the toggle still works in 3D");
        assert!(!s3.snapshot().lines.is_empty(), "the orbit view gets 3D streamlines (ticket 6)");
        let s2 = State::new(false);
        assert!(s2.snapshot().lines.is_empty(), "streamlines are off at the defaults");
    }

    #[test]
    fn speed_slider_changes_streamline_length() {
        let mut slow = State::new(false);
        assert!(slow.set_formula("(-y, x)".to_string()), "switch the plane to a vortex");
        assert!(slow.set_streamlines(true));
        assert!(slow.set_speed(0.1), "crawling speed gives each line a small budget");
        let mut fast = State::new(false);
        assert!(fast.set_formula("(-y, x)".to_string()));
        assert!(fast.set_streamlines(true));
        assert!(fast.set_speed(3.0), "top speed grows the point budget per line");
        let slow_lines = slow.snapshot().lines.len();
        let fast_lines = fast.snapshot().lines.len();
        assert!(fast_lines > slow_lines, "the budget, and so the length, follows the speed slider");
    }

    #[test]
    fn streamline_seeds_survive_identical_rebuilds() {
        let mut s = State::new(false);
        assert!(s.set_formula("(-y, x)".to_string()));
        assert!(s.set_streamlines(true));
        let first = s.snapshot().lines.len();
        let second = s.snapshot().lines.len();
        let third = s.snapshot().lines.len();
        assert_eq!(first, second, "seeds are deterministic: rebuilds agree");
        assert_eq!(second, third, "and again, with no state change in between");
    }

    /// The payload of every line instance, for equality checks
    /// (`LineInstanceGPU` has no `PartialEq` of its own).
    fn segments(lines: &[crate::renderer::LineInstanceGPU]) -> Vec<([f32; 4], [f32; 4], [f32; 4])> {
        lines.iter().map(|l| (l.a, l.b, l.color)).collect()
    }

    #[test]
    fn click_streamlines_seed_a_burst_without_the_grid_toggle() {
        // A click is an explicit request: it plants a burst even with the
        // streamline grid toggle off, and the burst stays on the 2D plane.
        let mut s = State::new(false);
        assert!(s.set_formula("(1, 0)".to_string()), "a field that flows at the origin");
        assert!(!s.streamlines, "the grid seeds are off at the defaults");
        let lines = s.click_streamlines_snapshot((0.0, 1.0), &[Vec3::ZERO]);
        assert!(!lines.is_empty(), "the click seeds a burst anyway");
        assert!(
            lines.iter().all(|l| l.a[2] == 0.0 && l.b[2] == 0.0),
            "the 2D burst stays on the click's plane",
        );
        let starts: Vec<[f32; 3]> = lines.iter().map(|l| [l.a[0], l.a[1], l.a[2]]).collect();
        let unique = starts.iter().fold(Vec::new(), |mut acc: Vec<[f32; 3]>, p| {
            if !acc.contains(p) {
                acc.push(*p);
            }
            acc
        });
        assert!(unique.len() > 1, "the burst traces several seeds, not one: {} starts", unique.len());
    }

    #[test]
    fn click_streamlines_are_deterministic_and_bounded_by_the_clicks() {
        // Same clicks redraw the same bursts; each extra click adds its own
        // geometry; and a click off the field contributes nothing.
        let mut s = State::new(true);
        assert!(s.set_formula("(-y, x, 0.5)".to_string()), "a depth-carrying field");
        let clicks = [Vec3::new(0.1, 0.1, 0.0), Vec3::new(-0.2, 0.3, 0.1)];
        let first = s.click_streamlines_snapshot((0.0, 1.5), &clicks);
        let second = s.click_streamlines_snapshot((0.0, 1.5), &clicks);
        assert_eq!(segments(&first), segments(&second), "rebuilds redraw the same bursts");
        let one = s.click_streamlines_snapshot((0.0, 1.5), &clicks[..1]);
        assert!(first.len() > one.len(), "each click adds its own burst");
        assert!(s.click_streamlines_snapshot((0.0, 1.5), &[]).is_empty(), "no clicks, no click lines");
        assert!(
            s.click_streamlines_snapshot((0.0, 1.5), &[Vec3::new(9.0, 9.0, 9.0)]).is_empty(),
            "a click off the field draws nothing",
        );
    }
}
