//! Advected particle dots: live per-frame state, not a dirty-flag cache.
//!
//! Arrows and streamlines are a cache the viewmodel seam rebuilds only when a
//! control is marked dirty. Particles are different: the app advances the whole
//! cloud by the frame's stable dt every frame, so motion reads as flow rather
//! than as a static sampling. Each particle takes one RK4 step through the
//! field per [`advect`] call — the same 4-evaluation step the streamline tracer
//! uses, but with a time step derived from the frame (`dt * speed`) instead of
//! from a fixed arc-length budget.
//!
//! The cloud's size is constant: a particle that sits outside the domain, hits
//! a zero-velocity point, turns non-finite or steps out of the domain is
//! respawned inside it, so the on-screen dot count never drifts and never
//! exceeds its cap (`density` x [`PARTICLES_PER_DENSITY`], capped at
//! [`MAX_PARTICLES`]).
//!
//! Spawn jitter is deterministic: an inline Park/Miller LCG (no `rand`
//! dependency), so a fresh session puts the dots in the same places.
//! See `docs/superpowers/plans/` task 10 / ticket 05.

use glam::Vec3;

use crate::colormap::Colormap;
use crate::domain::Domain;
use crate::field::FieldExpr;
use crate::renderer::{ArrowInstanceGPU, PARTICLE_KIND};

/// Hard cap on the particle cloud, whatever the density slider asks for.
pub const MAX_PARTICLES: usize = 4000;
/// Particles the app offers per unit of the density slider.
pub const PARTICLES_PER_DENSITY: usize = 16;
/// Spawn jitter is inset this much of each half extent, so a spawned particle
/// is strictly inside the domain and a first advect step cannot clip it.
pub const SPAWN_INSET: f32 = 0.02;
/// Dot radius as a fraction of the grid spacing, so a denser grid keeps dots
/// the same relative size (and dots stay visible in the 3D view).
pub const DOT_SPACING_FRACTION: f32 = 0.3;
/// Particles one viewport click adds around the picked spot (ticket 06), on
/// top of the ambient cloud.
pub const CLICK_BURST_PARTICLES: usize = 64;
/// Radius of that burst, as a fraction of the domain's smallest positive
/// extent, so a click reads as a local cluster at any domain size.
pub const CLICK_SPAWN_REACH: f32 = 0.1;

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
/// survives a colormap or speed change without rebuilding it.
#[derive(Clone)]
pub struct Particles {
    pub pos: Vec<Vec3>,
    /// The cloud's whole pseudo-random state (see [`lcg_next`]).
    rng: u64,
}

impl Default for Particles {
    /// An empty cloud at the fixed seed: the first [`spawn`] draws are
    /// reproducible across runs. This replaces the hand-written `new()`
    /// (clippy's `custom-constructor`): callers spell `Particles::default()`.
    fn default() -> Particles {
        Particles { pos: Vec::new(), rng: SPAWN_SEED }
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
        Particles { pos, ..Particles::default() }
    }
}

/// How many particles a density setting offers, capped at [`MAX_PARTICLES`] so
/// the dot buffer stays bounded whatever the slider is set to.
pub fn target_count(density: u32) -> usize {
    (density as usize * PARTICLES_PER_DENSITY).min(MAX_PARTICLES)
}

/// How many particles the cloud currently holds.
pub fn count(particles: &Particles) -> usize {
    particles.pos.len()
}

/// Empty the cloud *and* rewind its pseudo-random stream to [`SPAWN_SEED`]: the
/// app calls this when the particle toggle turns off, so a later enable respawns
/// from a clean state — the same jitter a fresh session draws — rather than
/// continuing a stream the previous cloud already advanced.
pub fn clear(particles: &mut Particles) {
    particles.pos.clear();
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
    let region = spawn_box(domain);
    let band = if dimension == 3 { region.band } else { region.band * Vec3::new(1.0, 1.0, 0.0) };
    let mut state = particles.rng;
    particles.pos.reserve((particles.pos.len() + count).min(MAX_PARTICLES) - particles.pos.len());
    for _ in 0..count {
        if particles.pos.len() >= MAX_PARTICLES {
            break;
        }
        state = lcg_next(state);
        particles.pos.push(jittered_point(region.center, band, state));
    }
    particles.rng = state;
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
        particles.pos.push(Vec3::new(
            p.x.clamp(domain.min.x, domain.max.x),
            p.y.clamp(domain.min.y, domain.max.y),
            p.z.clamp(domain.min.z, domain.max.z),
        ));
        added += 1;
    }
    particles.rng = state;
    added
}

/// Advect every particle by one RK4 step for the frame's `dt` (seconds) scaled
/// by the speed slider, respawning the ones that leave the domain, hit zero
/// velocity or turn non-finite. The cloud keeps its size.
pub fn advect(particles: &mut Particles, field: &FieldExpr, domain: &Domain, dt: f32, speed: f32) {
    if !(dt.is_finite() && dt > 0.0) {
        return;
    }
    let step = if speed.is_finite() { speed.max(0.0) } else { 1.0 };
    let h = dt * step;
    let region = spawn_box(domain);
    let mut state = particles.rng;
    let mut next = Vec::with_capacity(particles.pos.len());
    for i in 0..particles.pos.len() {
        state = lcg_next(state);
        let p = particles.pos[i];
        let q = if inside(domain, p) { rk4_step(field, p, h) } else { Vec3::splat(NAN_F32) };
        next.push(if q.is_finite() && inside(domain, q) { q } else { jittered_point(region.center, region.band, state) });
    }
    particles.pos = next;
    particles.rng = state;
}

/// Draw the cloud: one GPU dot instance per particle, coloured by the field's
/// magnitude at that particle exactly like the arrows and streamline segments
/// are (the [`color_range`] is the range those share), radius `radius` world
/// units. Dots reuse the arrow instance layout — the dot shader reads
/// `origin` as the center and `dir_len.w` as the radius.
pub fn dot_instances(
    particles: &Particles,
    field: &FieldExpr,
    domain: &Domain,
    colormap: Colormap,
    color_range: (f32, f32),
    radius: f32,
) -> Vec<ArrowInstanceGPU> {
    let (lo, hi) = color_range;
    let radius = if radius.is_finite() { radius.max(0.0) } else { 0.0 };
    let mut dots = Vec::with_capacity(particles.pos.len().min(MAX_PARTICLES));
    for i in 0..particles.pos.len().min(MAX_PARTICLES) {
        let p = particles.pos[i];
        if !inside(domain, p) {
            continue;
        }
        let mag = match field.eval3(p) {
            Some(v) if v.is_finite() => v.length(),
            _ => 0.0,
        };
        let ramp = colormap.colormap(mag, lo, hi);
        dots.push(ArrowInstanceGPU {
            origin: [p.x, p.y, p.z, PARTICLE_KIND],
            dir_len: [0.0, 0.0, 1.0, radius],
            color: [ramp[0] as f32 / 255.0, ramp[1] as f32 / 255.0, ramp[2] as f32 / 255.0, 1.0],
        });
    }
    dots
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

