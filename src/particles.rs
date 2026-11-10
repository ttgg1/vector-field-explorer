//! Advected particle dots: live per-frame state, not a dirty-flag cache.
//!
//! Arrows and streamlines are a cache the viewmodel seam rebuilds only when a
//! control is marked dirty. Particles are different: the app advances the whole
//! cloud by the frame's stable dt every frame, so motion reads as flow rather
//! than as a static sampling. Each particle takes one RK4 step through the
//! field per [`advect`] call — the same 4-evaluation step the streamline tracer
//! uses, but with the simulation clock's speed-scaled frame step instead of a
//! fixed arc-length budget (ticket 26).
//!
//! The cloud's size is constant between target changes: a particle that sits
//! outside the domain, hits a zero-velocity point, turns non-finite or steps
//! out of the domain is respawned inside it, so the on-screen dot count never
//! drifts and never exceeds its cap. The target follows the density
//! (`density` x [`PARTICLES_PER_DENSITY`], capped at [`MAX_PARTICLES`]) until
//! the user sets an explicit count (ticket 30); [`resize`] trims or extends
//! the cloud in place when that target changes.
//!
//! Spawn jitter is deterministic: an inline Park/Miller LCG (no `rand`
//! dependency), so a fresh session puts the dots in the same places.

use glam::Vec3;
use serde::{Deserialize, Serialize};

use crate::colormap::Colormap;
use crate::domain::Domain;
use crate::field::FieldExpr;
use crate::renderer::{ArrowInstanceGPU, LineInstanceGPU, PARTICLE_KIND};

/// Hard cap on the particle cloud, whatever the density slider asks for.
pub const MAX_PARTICLES: usize = 4000;
/// Particles the app offers per unit of the density slider.
pub const PARTICLES_PER_DENSITY: usize = 16;
/// Spawn jitter is inset this much of each half extent, so a spawned particle
/// is strictly inside the domain and a first advect step cannot clip it.
pub const SPAWN_INSET: f32 = 0.02;
/// Dot radius as a fraction of the grid spacing, so a denser grid keeps dots
/// the same relative size (and dots stay visible in the 3D view). The user's
/// particle size multiplies this base fraction; the fraction itself never
/// changes (ticket 30).
pub const DOT_SPACING_FRACTION: f32 = 0.3;
/// Particles one viewport click adds around the picked spot (ticket 06), on
/// top of the ambient cloud.
pub const CLICK_BURST_PARTICLES: usize = 64;
/// Radius of that burst, as a fraction of the domain's smallest positive
/// extent, so a click reads as a local cluster at any domain size.
pub const CLICK_SPAWN_REACH: f32 = 0.1;
/// Most trail points one particle may keep. The requested trail length is
/// clamped to this, and the total point budget below can shrink it further
/// as the cloud grows.
pub const MAX_TRAIL_LEN: u32 = 32;
/// Total stored trail points across the cloud. 4000 particles therefore get
/// at most 25 points each (24 segments), so a dense cloud cannot blow the
/// line-instance budget up by 32x (ticket 48).
pub const MAX_TRAIL_SEGMENTS: usize = 100_000;
/// A particle moving at or below this fraction of the domain's smallest
/// positive extent per simulated second counts as stagnant: less than 1% of
/// the box width per second is effectively standing still. Speed rather than
/// per-step displacement keeps the verdict independent of the frame's `dt`
/// (ticket 48 fix round).
pub const STAGNATION_SPEED_EPSILON: f32 = 1e-2;

/// Particle opacity bounds and default (ticket 48).
pub const MIN_PARTICLE_OPACITY: f32 = 0.05;
pub const MAX_PARTICLE_OPACITY: f32 = 1.0;
pub const DEFAULT_PARTICLE_OPACITY: f32 = 1.0;
/// Requested trail points: `0` turns trails off.
pub const MIN_TRAIL_LEN: u32 = 0;
pub const DEFAULT_TRAIL_LEN: u32 = 0;
/// Per-segment alpha multiplier toward the tail; `1` means no fade.
pub const MIN_TRAIL_FADE: f32 = 0.0;
pub const MAX_TRAIL_FADE: f32 = 1.0;
pub const DEFAULT_TRAIL_FADE: f32 = 0.65;
/// Trail width multiplier on [`DOT_SPACING_FRACTION`].
pub const MIN_TRAIL_WIDTH: f32 = 0.05;
pub const MAX_TRAIL_WIDTH: f32 = 2.0;
pub const DEFAULT_TRAIL_WIDTH: f32 = 1.0;
/// Stagnation loop threshold, simulated seconds.
pub const MIN_STAGNATION_SECONDS: f32 = 0.1;
pub const MAX_STAGNATION_SECONDS: f32 = 5.0;
pub const DEFAULT_STAGNATION_SECONDS: f32 = 0.5;
/// Historical particle default (Material Teal). This is the serialized fallback
/// for records that predate the field, not the app-chrome accent (`#4cc9f0`).
pub const DEFAULT_PARTICLE_FIXED_COLOR: [u8; 4] = [0, 150, 136, 255];

/// How a particle's dot and trail get their colour (ticket 48).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParticleColorMode {
    /// The field's magnitude, ramped over the shared magnitude range. The
    /// default, matching the arrows and streamlines.
    #[default]
    Magnitude,
    /// The particle's own numerical speed, auto-ranged to the current cloud.
    Speed,
    /// One fixed colour for every dot and trail segment.
    Fixed,
}

impl ParticleColorMode {
    /// Every mode, in selector order.
    pub const ALL: [ParticleColorMode; 3] = [
        ParticleColorMode::Magnitude,
        ParticleColorMode::Speed,
        ParticleColorMode::Fixed,
    ];

    /// Short label for the selector.
    pub fn label(self) -> &'static str {
        match self {
            ParticleColorMode::Magnitude => "Magnitude",
            ParticleColorMode::Speed => "Speed",
            ParticleColorMode::Fixed => "Fixed",
        }
    }

    /// One-line tooltip explanation.
    pub fn tooltip(self) -> &'static str {
        match self {
            ParticleColorMode::Magnitude => "Colour by the field magnitude, like the arrows",
            ParticleColorMode::Speed => "Colour by each particle's speed, auto-ranged to the cloud",
            ParticleColorMode::Fixed => "Paint every particle the chosen colour",
        }
    }
}

/// The per-particle appearance knobs (ticket 48): opacity, colouring, trail
/// history and stagnation looping. Egui-free on purpose; the sidebar converts
/// [`fixed_color`](ParticleAppearance::fixed_color) to `Color32` at the UI edge
/// and persistence stores the same four bytes.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ParticleAppearance {
    /// Dot and trail alpha, `0.05..=1.0`.
    pub opacity: f32,
    /// How dots and trails choose their RGB.
    pub color_mode: ParticleColorMode,
    /// Opaque RGBA for [`ParticleColorMode::Fixed`]. The UI edits RGB only and
    /// sanitizing pins alpha to 255; alpha is ignored at render.
    pub fixed_color: [u8; 4],
    /// Requested trail points, `0..=32`; `0` turns trails off.
    pub trail_len: u32,
    /// Per-segment alpha multiplier toward the tail, `0.0..=1.0`.
    pub trail_fade: f32,
    /// Trail width multiplier on [`DOT_SPACING_FRACTION`], `0.05..=2.0`.
    pub trail_width: f32,
    /// Re-spawn a particle that has stagnated for
    /// [`stagnation_seconds`](ParticleAppearance::stagnation_seconds).
    pub loop_on_stagnation: bool,
    /// How long a particle may sit still before looping, `0.1..=5.0` s.
    pub stagnation_seconds: f32,
}

impl Default for ParticleAppearance {
    fn default() -> Self {
        Self {
            opacity: DEFAULT_PARTICLE_OPACITY,
            color_mode: ParticleColorMode::Magnitude,
            fixed_color: DEFAULT_PARTICLE_FIXED_COLOR,
            trail_len: DEFAULT_TRAIL_LEN,
            trail_fade: DEFAULT_TRAIL_FADE,
            trail_width: DEFAULT_TRAIL_WIDTH,
            loop_on_stagnation: true,
            stagnation_seconds: DEFAULT_STAGNATION_SECONDS,
        }
    }
}

impl ParticleAppearance {
    /// Clamp every field into its documented range; a non-finite float falls
    /// back to its default. The sidebar and persistence share this one rule.
    pub fn sanitized(self) -> Self {
        let defaults = ParticleAppearance::default();
        Self {
            opacity: finite_clamped(
                self.opacity,
                MIN_PARTICLE_OPACITY,
                MAX_PARTICLE_OPACITY,
                defaults.opacity,
            ),
            color_mode: self.color_mode,
            // The picker edits RGB only; alpha is ignored at render, so the
            // stored colour is always opaque (ticket 48 fix round).
            fixed_color: [self.fixed_color[0], self.fixed_color[1], self.fixed_color[2], 255],
            trail_len: self.trail_len.min(MAX_TRAIL_LEN),
            trail_fade: finite_clamped(
                self.trail_fade,
                MIN_TRAIL_FADE,
                MAX_TRAIL_FADE,
                defaults.trail_fade,
            ),
            trail_width: finite_clamped(
                self.trail_width,
                MIN_TRAIL_WIDTH,
                MAX_TRAIL_WIDTH,
                defaults.trail_width,
            ),
            loop_on_stagnation: self.loop_on_stagnation,
            stagnation_seconds: finite_clamped(
                self.stagnation_seconds,
                MIN_STAGNATION_SECONDS,
                MAX_STAGNATION_SECONDS,
                defaults.stagnation_seconds,
            ),
        }
    }
}

/// Clamp a finite value into `lo..=hi`; a non-finite value takes `fallback`.
fn finite_clamped(value: f32, lo: f32, hi: f32, fallback: f32) -> f32 {
    if value.is_finite() { value.clamp(lo, hi) } else { fallback }
}

/// Park/Miller LCG constants: 64-bit multiplicative congruential, all u64
/// arithmetic wraps (debug-build overflow panics otherwise).
const LCG_MULTIPLIER: u64 = 6364136223846793005;
const LCG_INCREMENT: u64 = 1442695040888963407;
/// Starting seed of a fresh cloud. The seed is an implementation detail of the
/// reproducible jitter, not a behaviour the tests pin: any non-zero seed works,
/// because every advect step is far smaller than the [`SPAWN_INSET`] margin.
const SPAWN_SEED: u64 = 0x123456789ABCDEF0;
/// NaN for [`rk4_step`]'s "the flow vanished here" sentinel: the f32 NaN bit
/// pattern, spelled as bits because [`f32`] has no `nan` constant in scope here.
const NAN_F32: f32 = f32::from_bits(0x7fc00000);
/// Bits of one LCG output used per axis of a spawn draw (21 bits).
const LCG_SLICE: u64 = 0x1FFFFF;
/// Scale that maps a [`LCG_SLICE`]-bit integer onto [-1, 1). A power of two
/// (2 / 2^21), so the draw stays exact in f32.
const UNIT_SCALE: f32 = 2.0 / 2097152.0;

/// A cloud of advected points. Positions are world-space; colouring and dot
/// size are derived when the dots are drawn, so the state stays small and
/// survives a colormap, size or speed change without rebuilding it.
#[derive(Clone)]
pub struct Particles {
    pub pos: Vec<Vec3>,
    /// Trail history per particle, oldest first and newest last (ticket 48).
    /// Always aligned with [`pos`](Particles::pos).
    trails: Vec<Vec<Vec3>>,
    /// Simulated seconds each particle has been stagnant (ticket 48).
    stuck: Vec<f32>,
    /// Last step's numerical speed per particle, `|q - p| / dt` (ticket 48).
    speeds: Vec<f32>,
    /// The cloud's whole pseudo-random state (see [`lcg_next`]).
    rng: u64,
}

impl Default for Particles {
    /// An empty cloud at the fixed seed: the first [`spawn`] draws are
    /// reproducible across runs. This replaces the hand-written `new()`
    /// (clippy's `custom-constructor`): callers spell `Particles::default()`.
    fn default() -> Particles {
        Particles {
            pos: Vec::new(),
            trails: Vec::new(),
            stuck: Vec::new(),
            speeds: Vec::new(),
            rng: SPAWN_SEED,
        }
    }
}

impl Particles {
    /// A cloud holding exactly `pos` at the default pseudo-random seed.
    ///
    /// Test seam (ticket 13): integration tests in `tests/` must place points
    /// outside the domain to exercise the respawn path, but the RNG state stays
    /// private so no caller can desynchronise the deterministic jitter. Kept
    /// `#[doc(hidden)]` because it is not part of the app's public contract.
    #[doc(hidden)]
    pub fn with_positions(pos: Vec<Vec3>) -> Particles {
        let trails = pos.iter().map(|point| vec![*point]).collect();
        let len = pos.len();
        Particles {
            pos,
            trails,
            stuck: vec![0.0; len],
            speeds: vec![0.0; len],
            ..Particles::default()
        }
    }

    /// Read-only view of the per-particle history parallel to
    /// [`pos`](Particles::pos): trails (oldest first), stagnation times and
    /// last speeds.
    ///
    /// Test seam (ticket 48 fix round): the integration tests need to observe
    /// the ring, stuck counters and speeds, but the fields stay private so
    /// production callers cannot desynchronise the parallel vectors. This is
    /// the single read-only window; there is deliberately no setter.
    #[doc(hidden)]
    pub fn history(&self) -> (&[Vec<Vec3>], &[f32], &[f32]) {
        (&self.trails, &self.stuck, &self.speeds)
    }

    /// Keep every parallel vector the same length as `pos`: older test-built
    /// clouds (or a caller that assigns `pos` directly) may arrive without
    /// trails/stuck/speeds. New points get a one-point trail and zero times.
    fn align_parallel(&mut self) {
        let len = self.pos.len();
        self.trails.truncate(len);
        self.stuck.truncate(len);
        self.speeds.truncate(len);
        self.trails.resize_with(len, Vec::new);
        self.stuck.resize(len, 0.0);
        self.speeds.resize(len, 0.0);
        for (point, trail) in self.pos.iter().zip(self.trails.iter_mut()) {
            if trail.is_empty() {
                trail.push(*point);
            }
        }
    }

    /// Truncate `pos`, `trails`, `stuck` and `speeds` together so a rollback
    /// (e.g. undoing a click burst) cannot desynchronise the parallel vectors
    /// and leave stale speeds or trail history behind.
    pub fn truncate(&mut self, len: usize) {
        self.pos.truncate(len);
        self.trails.truncate(len);
        self.stuck.truncate(len);
        self.speeds.truncate(len);
    }
}

/// How many particles a density setting offers, capped at [`MAX_PARTICLES`] so
/// the dot buffer stays bounded whatever the slider is set to.
pub fn target_count(density: u32) -> usize {
    (density as usize * PARTICLES_PER_DENSITY).min(MAX_PARTICLES)
}

/// The count the cloud should hold for a density setting and an explicit user
/// count (ticket 30): `0` keeps today's Auto behaviour ([`target_count`]),
/// any other value overrides it, and both respect [`MAX_PARTICLES`].
pub fn effective_particle_count(density: u32, particle_count: u32) -> u32 {
    if particle_count == 0 {
        target_count(density) as u32
    } else {
        particle_count.min(MAX_PARTICLES as u32)
    }
}

/// How many particles the cloud currently holds.
pub fn count(particles: &Particles) -> usize {
    particles.pos.len()
}

/// How many trail points each particle may keep for a cloud of
/// `particle_count` and a requested `trail_len` (ticket 48).
///
/// `trail_len` counts stored points, so segments are one fewer. The total is
/// capped at [`MAX_TRAIL_SEGMENTS`] points across the cloud: 4000 particles
/// with 32 requested get 25 points each (24 segments), not 128 000. Zero
/// particles or zero requested points mean no trail history at all.
pub fn effective_trail_len(particle_count: usize, requested: u32) -> usize {
    let requested = requested.min(MAX_TRAIL_LEN) as usize;
    if particle_count == 0 || requested == 0 {
        0
    } else {
        requested.min(MAX_TRAIL_SEGMENTS / particle_count)
    }
}

/// Empty the cloud *and* rewind its pseudo-random stream to [`SPAWN_SEED`]: the
/// app calls this when the particle toggle turns off, so a later enable respawns
/// from a clean state — the same jitter a fresh session draws — rather than
/// continuing a stream the previous cloud already advanced. Trail history,
/// stagnation times and speeds reset with the positions.
pub fn clear(particles: &mut Particles) {
    particles.pos.clear();
    particles.trails.clear();
    particles.stuck.clear();
    particles.speeds.clear();
    particles.rng = SPAWN_SEED;
}

/// Inclusive axis-aligned containment (the `Domain` type has no `contains`
/// method), the same rule the streamline tracer uses.
fn inside(domain: &Domain, p: Vec3) -> bool {
    p.x >= domain.min.x && p.x <= domain.max.x
        && p.y >= domain.min.y && p.y <= domain.max.y
        && p.z >= domain.min.z && p.z <= domain.max.z
}

/// Field flow at `p`. A missing or non-finite evaluation reads as the zero
/// vector, which [`advect`] treats as zero velocity and respawns.
fn flow_at(field: &FieldExpr, p: Vec3) -> Vec3 {
    match field.eval3(p) {
        Some(v) if v.is_finite() => v,
        _ => Vec3::splat(0.0),
    }
}

/// One step of the cloud's pseudo-random stream: the LCG state itself, kept in
/// one function so every caller advances the same way.
fn lcg_next(state: u64) -> u64 {
    state.wrapping_mul(LCG_MULTIPLIER).wrapping_add(LCG_INCREMENT)
}

/// Three independent 21-bit slices of one LCG output mapped onto [-1, 1): one
/// per axis, so a single draw fills a box instead of walking a stream.
fn unit_triple(z: u64) -> (f32, f32, f32) {
    let a = ((z >> 42) & LCG_SLICE) as u32 as f32;
    let b = ((z >> 21) & LCG_SLICE) as u32 as f32;
    let c = (z & LCG_SLICE) as u32 as f32;
    (a * UNIT_SCALE - 1.0, b * UNIT_SCALE - 1.0, c * UNIT_SCALE - 1.0)
}

/// A point in the domain's inset box: `center + jitter * band`, where `band`
/// is half the axis extent shrunk by [`SPAWN_INSET`]. A degenerate axis
/// (zero extent, as the z axis of a flat 2D domain is) pins that axis at its
/// coordinate, so 2D spawns sit on the domain's plane.
fn jittered_point(center: Vec3, band: Vec3, z: u64) -> Vec3 {
    let (ux, uy, uz) = unit_triple(z);
    center + Vec3::new(ux, uy, uz) * band
}

/// The domain's spawn box, split into the center and per-axis band every
/// respawn reuses.
struct SpawnBox {
    center: Vec3,
    band: Vec3,
}

fn spawn_box(domain: &Domain) -> SpawnBox {
    let inset = 0.5 * (1.0 - SPAWN_INSET);
    SpawnBox {
        center: (domain.min + domain.max) * 0.5,
        band: (domain.max - domain.min) * inset,
    }
}

/// One RK4 step through the field, time step `h`: 4 evaluations
/// (`k1 = p + f(p)h/2`, `k2 = p + f(k1)h/2`, `k3 = p + f(k2)h`,
/// `next = p + (f(p) + 2f(k1) + 2f(k2) + f(k3))h/6`) — the same walk as the
/// streamline tracer's `integrate`, but with a per-frame time step rather than
/// a fixed arc-length one. The NaN sentinel marks zero or non-finite velocity,
/// which [`advect`] respawns.
fn rk4_step(field: &FieldExpr, p: Vec3, h: f32) -> Vec3 {
    let f0 = flow_at(field, p);
    let mag = f0.length();
    if mag <= 0.0 || mag.is_nan() {
        return Vec3::splat(NAN_F32);
    }
    let k1 = p + f0 * (h * 0.5);
    let f1 = flow_at(field, k1);
    let k2 = p + f1 * (h * 0.5);
    let f2 = flow_at(field, k2);
    let k3 = p + f2 * h;
    let f3 = flow_at(field, k3);
    p + (f0 + f1 * 2.0 + f2 * 2.0 + f3) * (h / 6.0)
}

/// Fill the cloud with `count` jittered points inside `domain`. The `dimension`
/// selects the spawn volume: `3` jitters through the domain's whole inset box,
/// so the orbit view's cloud has depth, while any other value pins the points
/// to the domain's z-midpoint plane — the flat slice the pan/zoom view draws
/// (and what a genuinely flat 2D domain gives either way). Capped at
/// [`MAX_PARTICLES`] and drawn from the cloud's stream (so a respawn mid-
/// [`advect`] and a fresh spawn agree).
pub fn spawn(particles: &mut Particles, domain: &Domain, count: usize, dimension: usize) {
    particles.align_parallel();
    let region = spawn_box(domain);
    let band = if dimension == 3 { region.band } else { region.band * Vec3::new(1.0, 1.0, 0.0) };
    let mut state = particles.rng;
    particles.pos.reserve((particles.pos.len() + count).min(MAX_PARTICLES) - particles.pos.len());
    for _ in 0..count {
        if particles.pos.len() >= MAX_PARTICLES {
            break;
        }
        state = lcg_next(state);
        let point = jittered_point(region.center, band, state);
        particles.pos.push(point);
        particles.trails.push(vec![point]);
        particles.stuck.push(0.0);
        particles.speeds.push(0.0);
    }
    particles.rng = state;
}

/// Bring the cloud to exactly `target` particles (ticket 30): trim from the
/// end when the target is lower, spawn only the difference when it is higher.
/// Existing particles keep their order and positions, and new points come
/// from the cloud's own stream, so which particles a raise adds is
/// deterministic for a given history (a repeated raise from the same trimmed
/// cloud replays the same additions; it is not a fixed prefix of a longer
/// spawn). Never exceeds [`MAX_PARTICLES`]. The app calls this with the
/// viewmodel's per-frame target; it must not be called while particles are off.
pub fn resize(particles: &mut Particles, domain: &Domain, target: usize, dimension: usize) {
    particles.align_parallel();
    let target = target.min(MAX_PARTICLES);
    if particles.pos.len() > target {
        particles.pos.truncate(target);
        particles.trails.truncate(target);
        particles.stuck.truncate(target);
        particles.speeds.truncate(target);
    } else if particles.pos.len() < target {
        spawn(particles, domain, target - particles.pos.len(), dimension);
    }
}

/// Apply a target only when it changed since the last frame, returning the
/// target now in force (ticket 30). This is the app's per-frame policy: a
/// stable target leaves the cloud alone, so a click burst that sits above the
/// configured count survives; `None` (particles off / 2D) spawns nothing and
/// leaves the cloud for the caller's clear branch.
pub fn retarget(
    particles: &mut Particles,
    domain: &Domain,
    previous: Option<usize>,
    target: Option<usize>,
    dimension: usize,
) -> Option<usize> {
    if target != previous
        && let Some(target) = target
    {
        resize(particles, domain, target, dimension);
    }
    target
}

/// Add a `count`-point burst around `center`: the click-seeded counterpart of
/// [`spawn`]. The cluster radius is [`CLICK_SPAWN_REACH`] of the domain's
/// smallest positive extent, so the dots land where the user pointed rather
/// than across the whole box; `dimension` follows [`spawn`] (`3` jitters the
/// full box, anything else keeps z at the clicked plane). Draws from the
/// cloud's own stream, so the same click replays the same dots. Returns how
/// many points were actually added: zero for a click off the domain or an
/// empty request, and never past [`MAX_PARTICLES`].
pub fn spawn_at(
    particles: &mut Particles,
    domain: &Domain,
    center: Vec3,
    count: usize,
    dimension: usize,
) -> usize {
    particles.align_parallel();
    if count == 0 || !center.is_finite() || !inside(domain, center) {
        return 0;
    }
    let size = domain.domain_size();
    let smallest = [size.x, size.y, size.z]
        .into_iter()
        .filter(|extent| *extent > 0.0)
        .fold(f32::INFINITY, f32::min);
    let reach = if smallest.is_finite() { CLICK_SPAWN_REACH * smallest } else { 0.0 };
    let band = Vec3::new(reach, reach, if dimension == 3 { reach } else { 0.0 });
    let mut state = particles.rng;
    let mut added = 0;
    for _ in 0..count {
        if particles.pos.len() >= MAX_PARTICLES {
            break;
        }
        state = lcg_next(state);
        let p = jittered_point(center, band, state);
        let point = Vec3::new(
            p.x.clamp(domain.min.x, domain.max.x),
            p.y.clamp(domain.min.y, domain.max.y),
            p.z.clamp(domain.min.z, domain.max.z),
        );
        particles.pos.push(point);
        particles.trails.push(vec![point]);
        particles.stuck.push(0.0);
        particles.speeds.push(0.0);
        added += 1;
    }
    particles.rng = state;
    added
}

/// The per-frame advection settings (ticket 48). `dt` is the
/// [`SimClock`](crate::sim_clock::SimClock)'s clamped, speed-scaled frame step;
/// there is deliberately no separate speed multiplier (ticket 26 ruling).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AdvectSettings {
    /// Simulated seconds to advance. Non-finite, zero or negative is a no-op.
    pub dt: f32,
    /// Requested trail points, `0..=32`; `0` keeps no history.
    pub trail_len: u32,
    /// Re-spawn a particle once it has stagnated for `stagnation_seconds`.
    pub loop_on_stagnation: bool,
    /// Stagnation threshold in simulated seconds.
    pub stagnation_seconds: f32,
}

impl Default for AdvectSettings {
    fn default() -> Self {
        Self {
            dt: 0.0,
            trail_len: DEFAULT_TRAIL_LEN,
            loop_on_stagnation: true,
            stagnation_seconds: DEFAULT_STAGNATION_SECONDS,
        }
    }
}

/// Append `point` to `trail`, keeping at most `effective` points (newest last).
/// With trails off the history collapses to the newest point only.
fn push_trail_point(trail: &mut Vec<Vec3>, point: Vec3, effective: usize) {
    trail.push(point);
    if effective == 0 {
        trail.clear();
        trail.push(point);
    } else if trail.len() > effective {
        let drop = trail.len() - effective;
        trail.drain(0..drop);
    }
}

/// The smallest positive domain extent, or `None` for a degenerate box. The
/// stagnation threshold scales with this so it reads the same at any size.
fn smallest_extent(domain: &Domain) -> Option<f32> {
    let size = domain.domain_size();
    [size.x, size.y, size.z]
        .into_iter()
        .filter(|extent| *extent > 0.0 && extent.is_finite())
        .fold(None, |smallest: Option<f32>, extent| {
            Some(smallest.map_or(extent, |value| value.min(extent)))
        })
}

/// Process one particle's advection outcome for the stagnation loop, returning
/// `true` when the particle respawned. Shared by the moving and stuck paths.
/// The verdict is on speed (world units per simulated second), so it does not
/// change when the frame's `dt` changes.
#[allow(clippy::too_many_arguments)]
fn stagnated_respawn(
    particles: &mut Particles,
    index: usize,
    speed: f32,
    dt: f32,
    stagnation_speed: f32,
    settings: AdvectSettings,
    region: &SpawnBox,
    state: u64,
) -> bool {
    if speed <= stagnation_speed {
        particles.stuck[index] += dt;
    } else {
        particles.stuck[index] = 0.0;
    }
    if !settings.loop_on_stagnation || particles.stuck[index] < settings.stagnation_seconds {
        return false;
    }
    let point = jittered_point(region.center, region.band, state);
    particles.pos[index] = point;
    particles.trails[index].clear();
    particles.trails[index].push(point);
    particles.stuck[index] = 0.0;
    particles.speeds[index] = 0.0;
    true
}

/// Advect every particle by one RK4 step of `settings.dt` simulated seconds —
/// the [`SimClock`](crate::sim_clock::SimClock)'s clamped, speed-scaled frame
/// step — respawning the ones that leave the domain, hit zero velocity or turn
/// non-finite. The cloud keeps its size; a zero or non-finite step is a no-op,
/// so a paused clock leaves every particle exactly where it was.
///
/// Each successful step appends the new position to that particle's trail and
/// trims the oldest points to [`effective_trail_len`]; a stagnant particle
/// (speed at or below [`STAGNATION_SPEED_EPSILON`] of the smallest extent per
/// simulated second) accumulates simulated `dt` and, when looping is on,
/// respawns once it reaches the threshold. A respawn resets that particle's
/// trail, stuck time and speed. An exact zero-velocity point still takes the
/// pre-existing RK4 NaN respawn immediately, in both loop toggle states.
pub fn advect(
    particles: &mut Particles,
    field: &FieldExpr,
    domain: &Domain,
    settings: AdvectSettings,
) {
    particles.align_parallel();
    let dt = settings.dt;
    if !(dt.is_finite() && dt > 0.0) {
        return;
    }
    let region = spawn_box(domain);
    let effective = effective_trail_len(particles.pos.len(), settings.trail_len);
    let stagnation_speed =
        smallest_extent(domain).map_or(0.0, |extent| STAGNATION_SPEED_EPSILON * extent);
    let threshold = finite_clamped(
        settings.stagnation_seconds,
        MIN_STAGNATION_SECONDS,
        MAX_STAGNATION_SECONDS,
        DEFAULT_STAGNATION_SECONDS,
    );
    let settings = AdvectSettings { stagnation_seconds: threshold, ..settings };
    let mut state = particles.rng;
    for i in 0..particles.pos.len() {
        state = lcg_next(state);
        let p = particles.pos[i];
        let q = if inside(domain, p) { rk4_step(field, p, dt) } else { Vec3::splat(NAN_F32) };
        if !q.is_finite() || !inside(domain, q) {
            let point = jittered_point(region.center, region.band, state);
            particles.pos[i] = point;
            particles.trails[i].clear();
            particles.trails[i].push(point);
            particles.stuck[i] = 0.0;
            particles.speeds[i] = 0.0;
            continue;
        }
        let displacement = (q - p).length();
        let speed = displacement / dt;
        particles.pos[i] = q;
        particles.speeds[i] = speed;
        push_trail_point(&mut particles.trails[i], q, effective);
        stagnated_respawn(
            particles,
            i,
            speed,
            dt,
            stagnation_speed,
            settings,
            &region,
            state,
        );
    }
    particles.rng = state;
}

/// The observed speed range of the current cloud (ticket 48). `Speed`
/// colouring auto-ranges to this each frame; an empty or all-equal cloud falls
/// back to `0..1` so the ramp still has span.
pub fn speed_range(particles: &Particles) -> (f32, f32) {
    // Defensive: never read past the aligned prefix, so a desynchronised cloud
    // (pos shorter or longer than speeds) cannot read stale speeds.
    let n = particles.pos.len().min(particles.speeds.len());
    let mut lo = f32::INFINITY;
    let mut hi = f32::NEG_INFINITY;
    for speed in &particles.speeds[..n] {
        if speed.is_finite() {
            lo = lo.min(*speed);
            hi = hi.max(*speed);
        }
    }
    if lo.is_finite() && hi.is_finite() && hi > lo { (lo, hi) } else { (0.0, 1.0) }
}

/// The colour one dot or trail segment gets (ticket 48), in RGBA `0..1`.
/// `alpha` is applied by the caller; the RGB rules are shared so dots and
/// trails always agree for a given mode.
#[allow(clippy::too_many_arguments)]
fn particle_rgb(
    mode: ParticleColorMode,
    field: &FieldExpr,
    point: Vec3,
    speed: f32,
    colormap: Colormap,
    color_range: (f32, f32),
    speed_range: (f32, f32),
    appearance: &ParticleAppearance,
) -> [f32; 3] {
    match mode {
        ParticleColorMode::Magnitude => {
            let mag = match field.eval3(point) {
                Some(v) if v.is_finite() => v.length(),
                _ => 0.0,
            };
            let ramp = colormap.colormap(mag, color_range.0, color_range.1);
            [
                ramp[0] as f32 / 255.0,
                ramp[1] as f32 / 255.0,
                ramp[2] as f32 / 255.0,
            ]
        }
        ParticleColorMode::Speed => {
            let ramp = colormap.colormap(speed, speed_range.0, speed_range.1);
            [
                ramp[0] as f32 / 255.0,
                ramp[1] as f32 / 255.0,
                ramp[2] as f32 / 255.0,
            ]
        }
        ParticleColorMode::Fixed => {
            let [r, g, b, _] = appearance.fixed_color;
            [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]
        }
    }
}

/// Draw the cloud: one GPU dot instance per particle. RGB follows
/// [`ParticleAppearance::color_mode`] (magnitude, speed or fixed) and alpha is
/// the particle opacity; radius is `radius` world units. Dots reuse the arrow
/// instance layout — the dot shader reads `origin` as the center and
/// `dir_len.w` as the radius.
#[allow(clippy::too_many_arguments)]
pub fn dot_instances(
    particles: &Particles,
    field: &FieldExpr,
    domain: &Domain,
    colormap: Colormap,
    color_range: (f32, f32),
    appearance: &ParticleAppearance,
    speed_range: (f32, f32),
    radius: f32,
) -> Vec<ArrowInstanceGPU> {
    let appearance = appearance.sanitized();
    let alpha = appearance.opacity;
    let radius = if radius.is_finite() { radius.max(0.0) } else { 0.0 };
    let mut dots = Vec::with_capacity(particles.pos.len().min(MAX_PARTICLES));
    for i in 0..particles.pos.len().min(MAX_PARTICLES) {
        let p = particles.pos[i];
        if !inside(domain, p) {
            continue;
        }
        let speed = particles.speeds.get(i).copied().unwrap_or(0.0);
        let [r, g, b] = particle_rgb(
            appearance.color_mode,
            field,
            p,
            speed,
            colormap,
            color_range,
            speed_range,
            &appearance,
        );
        dots.push(ArrowInstanceGPU {
            origin: [p.x, p.y, p.z, PARTICLE_KIND],
            dir_len: [0.0, 0.0, 1.0, radius],
            color: [r, g, b, alpha],
        });
    }
    dots
}

/// One line segment per consecutive trail point, newest segment first-faded
/// (ticket 48). The newest segment has age 0 and alpha `opacity`; each older
/// segment multiplies by `trail_fade`. Width is
/// `trail_width * DOT_SPACING_FRACTION * spacing`, so a wider grid gives
/// proportionally wider trails. Trails are skipped entirely when the requested
/// length is below two points or the cloud is empty, and the whole result is
/// hard-capped at [`MAX_TRAIL_SEGMENTS`].
pub fn trail_instances(
    particles: &Particles,
    field: &FieldExpr,
    colormap: Colormap,
    color_range: (f32, f32),
    appearance: &ParticleAppearance,
    speed_range: (f32, f32),
    spacing: f32,
) -> Vec<LineInstanceGPU> {
    let appearance = appearance.sanitized();
    let effective = effective_trail_len(particles.pos.len(), appearance.trail_len);
    if effective < 2 {
        return Vec::new();
    }
    let spacing = if spacing.is_finite() { spacing.max(0.0) } else { 0.0 };
    let width = appearance.trail_width * DOT_SPACING_FRACTION * spacing;
    let alpha = appearance.opacity.clamp(0.0, 1.0);
    let fade = appearance.trail_fade.clamp(0.0, 1.0);
    let mut trails = Vec::new();
    for i in 0..particles.pos.len().min(MAX_PARTICLES) {
        let Some(trail) = particles.trails.get(i) else {
            continue;
        };
        let start = trail.len().saturating_sub(effective);
        let points = &trail[start..];
        if points.len() < 2 {
            continue;
        }
        let speed = particles.speeds.get(i).copied().unwrap_or(0.0);
        let midpoint = points[points.len() - 2].lerp(points[points.len() - 1], 0.5);
        let [r, g, b] = particle_rgb(
            appearance.color_mode,
            field,
            midpoint,
            speed,
            colormap,
            color_range,
            speed_range,
            &appearance,
        );
        let segments = points.len() - 1;
        for j in 0..segments {
            if trails.len() >= MAX_TRAIL_SEGMENTS {
                return trails;
            }
            let age = (segments - 1 - j) as i32;
            let segment_alpha = (alpha * fade.powi(age)).clamp(0.0, 1.0);
            trails.push(LineInstanceGPU {
                a: [points[j].x, points[j].y, points[j].z, 1.0],
                b: [points[j + 1].x, points[j + 1].y, points[j + 1].z, width],
                color: [r, g, b, segment_alpha],
            });
        }
    }
    trails
}

/// The billboard quad's six corners in dot-local space: a camera-facing square
/// with half-extent 1, in triangle-list order (two triangles, no index buffer).
/// The dot shader mirrors this table; its fragment keeps only the unit circle
/// inscribed in the square, so a dot reads as a round disc rather than the old
/// octahedron's diamond (ticket 29).
pub const DOT_QUAD_CORNERS: [[f32; 2]; 6] = [
    [-1.0, -1.0],
    [1.0, -1.0],
    [1.0, 1.0],
    [-1.0, -1.0],
    [1.0, 1.0],
    [-1.0, 1.0],
];

/// World-space corners of one dot's camera-facing billboard: `origin` offset by
/// `right * x + up * y` scaled by the instance's world-space `radius` (the
/// `dir_len.w` the dot shader reads). This is the CPU mirror of the vertex
/// shader's billboard expansion, so the geometry is checkable without a GPU.
pub fn dot_billboard_corners(origin: Vec3, radius: f32, right: Vec3, up: Vec3) -> [Vec3; 6] {
    DOT_QUAD_CORNERS.map(|[x, y]| origin + (right * x + up * y) * radius)
}

