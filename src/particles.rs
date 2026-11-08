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
use crate::renderer::ArrowInstanceGPU;

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
            origin: [p.x, p.y, p.z, 1.0],
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

#[cfg(test)]
mod tests {
    use super::{CLICK_BURST_PARTICLES, CLICK_SPAWN_REACH, MAX_PARTICLES, PARTICLES_PER_DENSITY, Particles, advect, clear, count, dot_instances, spawn, spawn_at, target_count};
    use glam::Vec3;
    use crate::colormap::Colormap;
    use crate::domain::Domain;
    use crate::field::FieldExpr;

    /// The cloud's positions, for the assertions below.
    fn positions(cloud: &Particles) -> Vec<Vec3> {
        cloud.pos.to_vec()
    }

    #[test]
    fn spawn_fills_requested_count() {
        let domain = Domain::new(Vec3::new(-1.0, -0.5, 0.5), Vec3::new(1.0, 0.5, 0.5));
        let mut cloud = Particles::default();
        spawn(&mut cloud, &domain, 37, 2);
        let ps = positions(&cloud);
        assert_eq!(ps.len(), 37, "spawn fills exactly the requested count");
        for p in &ps {
            assert!(
                p.x >= domain.min.x && p.x <= domain.max.x
                    && p.y >= domain.min.y && p.y <= domain.max.y
                    && p.z >= domain.min.z && p.z <= domain.max.z,
                "inside the domain",
            );
            assert!(p.x >= -0.99 + 1e-5 && p.x <= 0.99, "inset from the x edges");
            assert!(p.y >= -0.495 + 1e-5 && p.y <= 0.495, "inset from the y edges");
            assert_eq!(p.z, 0.5, "a degenerate z axis pins the plane midpoint");
        }
    }

    #[test]
    fn spawn_3d_mode_jitters_the_full_depth() {
        // 3D mode: particles are seeded throughout the domain's depth, not on
        // a plane — a small cloud's z spread covers most of the box, and the
        // same inset as 2D mode keeps it off the domain's skin.
        let domain = Domain::default();
        let mut cloud = Particles::default();
        spawn(&mut cloud, &domain, 64, 3);
        let ps = positions(&cloud);
        assert_eq!(ps.len(), 64, "count is honoured");
        let mut z_min = f32::INFINITY;
        let mut z_max = f32::NEG_INFINITY;
        for p in ps.iter() {
            z_min = z_min.min(p.z);
            z_max = z_max.max(p.z);
        }
        assert!(z_min > domain.min.z && z_max < domain.max.z, "inset from the domain faces");
        assert!(z_max - z_min > 0.8 * (domain.max.z - domain.min.z), "seeds span the domain's depth");
        let mut x_min = f32::INFINITY;
        let mut x_max = f32::NEG_INFINITY;
        for p in ps.iter() {
            x_min = x_min.min(p.x);
            x_max = x_max.max(p.x);
        }
        assert!(x_max - x_min > 0.8 * (domain.max.x - domain.min.x), "and the domain's width");
    }

    #[test]
    fn spawn_2d_mode_pins_the_plane() {
        // 2D mode keeps particles on the domain's z-midpoint plane, so the
        // existing 2D behaviour is unchanged by the dimension parameter.
        let domain = Domain::new(Vec3::new(-2.0, -2.0, 0.5), Vec3::new(2.0, 2.0, 0.5));
        let mut cloud = Particles::default();
        spawn(&mut cloud, &domain, 32, 2);
        let ps = positions(&cloud);
        assert!(ps.iter().all(|p| p.z == 0.5), "flat 2D domains pin z at the plane");
    }

    #[test]
    fn spawn_3d_mode_of_a_flat_domain_is_a_2d_mode() {
        // A domain that is itself flat degenerates the same way in either mode
        // (the band is zero on that axis), so 3D mode cannot escape the box.
        let domain = Domain::new(Vec3::new(-1.0, -1.0, 0.5), Vec3::new(1.0, 1.0, 0.5));
        let mut cloud = Particles::default();
        spawn(&mut cloud, &domain, 32, 3);
        let ps = positions(&cloud);
        assert!(ps.iter().all(|p| p.z == 0.5), "a zero-depth domain stays a plane in 3D mode");
    }

    #[test]
    fn spawn_at_clusters_around_the_click() {
        // A click burst adds a small cloud centred on the picked spot, not
        // across the whole domain: the dots appear where the user pointed.
        let domain = Domain::default();
        let center = Vec3::new(0.25, -0.1, 0.3);
        let mut cloud = Particles::default();
        let added = spawn_at(&mut cloud, &domain, center, CLICK_BURST_PARTICLES, 3);
        assert_eq!(added, CLICK_BURST_PARTICLES, "the click adds its whole burst");
        let ps = positions(&cloud);
        assert_eq!(ps.len(), CLICK_BURST_PARTICLES);
        let reach = CLICK_SPAWN_REACH * 2.0; // the default domain's smallest extent
        for p in &ps {
            assert!((*p - center).length() <= reach * 3.0_f32.sqrt() + 1e-5, "within the click radius: {p:?}");
            assert!(
                p.x >= domain.min.x && p.x <= domain.max.x
                    && p.y >= domain.min.y && p.y <= domain.max.y
                    && p.z >= domain.min.z && p.z <= domain.max.z,
                "inside the domain: {p:?}",
            );
        }
    }

    #[test]
    fn spawn_at_is_deterministic_and_2d_pins_the_plane() {
        // The burst draws from the cloud's own stream, so replaying a click
        // (or rebuilding) lands the same dots; 2D clicks stay on their plane.
        let domain = Domain::default();
        let center = Vec3::new(-0.2, 0.4, 0.0);
        let mut a = Particles::default();
        let mut b = Particles::default();
        spawn_at(&mut a, &domain, center, CLICK_BURST_PARTICLES, 3);
        spawn_at(&mut b, &domain, center, CLICK_BURST_PARTICLES, 3);
        assert_eq!(positions(&a), positions(&b), "the same click draws the same burst");
        let mut flat = Particles::default();
        let plane = Domain::new(Vec3::new(-1.0, -1.0, 0.5), Vec3::new(1.0, 1.0, 0.5));
        spawn_at(&mut flat, &plane, Vec3::new(0.1, 0.2, 0.5), 24, 2);
        assert!(positions(&flat).iter().all(|p| p.z == 0.5), "2D bursts stay on the plane");
    }

    #[test]
    fn spawn_at_respects_the_cap_and_the_domain() {
        // A click can never push the cloud past its instance budget, and a
        // click that landed off the field adds nothing.
        let domain = Domain::default();
        let mut full = Particles::default();
        spawn(&mut full, &domain, MAX_PARTICLES, 3);
        assert_eq!(spawn_at(&mut full, &domain, Vec3::ZERO, CLICK_BURST_PARTICLES, 3), 0);
        assert_eq!(count(&full), MAX_PARTICLES, "the cap holds");
        let mut cloud = Particles::default();
        assert_eq!(spawn_at(&mut cloud, &domain, Vec3::new(5.0, 0.0, 0.0), 10, 3), 0,
            "an off-field click adds nothing");
        assert!(positions(&cloud).is_empty());
    }

    #[test]
    fn spawn_caps_at_particle_cap() {
        let domain = Domain::default();
        let mut cloud = Particles::default();
        spawn(&mut cloud, &domain, MAX_PARTICLES + 250, 3);
        assert_eq!(count(&cloud), MAX_PARTICLES, "never more than the instance budget");
    }

    #[test]
    fn target_count_scales_with_density() {
        assert_eq!(target_count(1), PARTICLES_PER_DENSITY);
        assert_eq!(target_count(200), 200 * PARTICLES_PER_DENSITY, "the 2D density cap still fits");
        assert_eq!(target_count(10_000), MAX_PARTICLES, "bounded by the particle cap");
    }

    #[test]
    fn spawn_is_reproducible() {
        let domain = Domain::default();
        let mut a = Particles::default();
        let mut b = Particles::default();
        spawn(&mut a, &domain, 37, 3);
        spawn(&mut b, &domain, 37, 3);
        assert_eq!(positions(&a), positions(&b), "jitter is deterministic across runs");
    }

    #[test]
    fn pump_field_speed_scales_the_step() {
        // Pinned binding test: a pump field advances every particle by dt x speed
        // (within RK4 slack), never past the domain, whatever the spawn jitter.
        let field = FieldExpr::parse("(1, 0)").expect("pump field parses");
        let domain = Domain::default();
        let mut particles = Particles::default();
        spawn(&mut particles, &domain, 9, 3);
        let dt = 0.016;
        let speed = 1.0;
        let before = positions(&particles);
        advect(&mut particles, &field, &domain, dt, speed);
        let after = positions(&particles);
        assert_eq!(before.len(), after.len(), "advect never changes the count");
        for i in 0..after.len() {
            let p = after[i];
            assert!(
                p.x >= domain.min.x && p.x <= domain.max.x
                    && p.y >= domain.min.y && p.y <= domain.max.y
                    && p.z >= domain.min.z && p.z <= domain.max.z,
                "still inside",
            );
            assert!(
                (p.x - before[i].x) >= dt * speed * 0.5,
                "advanced along the flow by dt x speed (within RK4 slack)",
            );
            assert!(p.x <= 1.0, "never past the domain edge");
        }
    }

    #[test]
    fn speed_zero_leaves_particles_put() {
        let field = FieldExpr::parse("(1, 0)").expect("pump field parses");
        let domain = Domain::default();
        let mut particles = Particles::default();
        spawn(&mut particles, &domain, 11, 3);
        let before = positions(&particles);
        advect(&mut particles, &field, &domain, 0.016, 0.0);
        let ps = positions(&particles);
        assert_eq!(ps.len(), 11, "the count is constant");
        for (i, p) in ps.iter().enumerate() {
            assert_eq!(
                *p,
                before[i],
                "a speed of 0 leaves every particle exactly at its previous position",
            );
            assert!(
                p.x >= domain.min.x && p.x <= domain.max.x
                    && p.y >= domain.min.y && p.y <= domain.max.y
                    && p.z >= domain.min.z && p.z <= domain.max.z,
                "inside",
            );
        }
    }

    #[test]
    fn outside_particle_respawns_inside() {
        let field = FieldExpr::parse("(1, 0)").expect("pump field parses");
        let domain = Domain::default();
        // Clippy's `field_reassign_with_default` form: the only field we vary
        // is `pos`; the LCG seed stays the Default one.
        let mut particles = Particles {
            pos: vec![Vec3::splat(5.0), Vec3::new(5.0, 0.0, 0.0)],
            ..Default::default()
        };
        advect(&mut particles, &field, &domain, 0.016, 1.0);
        let ps = positions(&particles);
        assert_eq!(ps.len(), 2, "respawning keeps the count");
        for p in &ps {
            assert!(
                p.x >= domain.min.x && p.x <= domain.max.x
                    && p.y >= domain.min.y && p.y <= domain.max.y
                    && p.z >= domain.min.z && p.z <= domain.max.z,
                "respawned inside",
            );
        }
    }

    #[test]
    fn zero_velocity_particle_respawns() {
        let field = FieldExpr::parse("(0, 0)").expect("zero field parses");
        let domain = Domain::default();
        let mut particles = Particles {
            pos: vec![Vec3::splat(0.0)],
            ..Default::default()
        };
        advect(&mut particles, &field, &domain, 0.016, 1.0);
        let ps = positions(&particles);
        assert_eq!(ps.len(), 1, "the count is constant");
        let p = ps[0];
        assert!(
            p.x >= domain.min.x && p.x <= domain.max.x
                && p.y >= domain.min.y && p.y <= domain.max.y
                && p.z >= domain.min.z && p.z <= domain.max.z,
            "inside",
        );
    }

    #[test]
    fn count_constant_under_vortex_advection() {
        // Pinned binding test: the count never drifts or spikes past the named
        // cap over 100 advect steps, and every particle stays inside.
        let field = FieldExpr::parse("(-y, x)").expect("vortex parses");
        let domain = Domain::new(Vec3::new(-1.0, -1.0, 0.0), Vec3::new(1.0, 1.0, 0.0));
        let mut particles = Particles::default();
        let density = 100;
        spawn(&mut particles, &domain, target_count(density), 2);
        let base = count(&particles);
        assert!(base <= MAX_PARTICLES, "spawned count respects the cap");
        let dt = 0.016;
        for _ in 0..100 {
            advect(&mut particles, &field, &domain, dt, 2.0);
            assert_eq!(count(&particles), base, "count constant while pumping a vortex");
            assert!(count(&particles) <= MAX_PARTICLES, "and never exceeds the cap");
        }
        for p in &particles.pos {
            assert!(
                p.x >= domain.min.x && p.x <= domain.max.x
                    && p.y >= domain.min.y && p.y <= domain.max.y
                    && p.z >= domain.min.z && p.z <= domain.max.z,
                "every particle is inside after 100 steps",
            );
        }
    }

    #[test]
    fn respawn_stays_reproducible() {
        let field = FieldExpr::parse("(1, 0)").expect("pump field parses");
        let domain = Domain::default();
        let mut a = Particles::default();
        let mut b = Particles::default();
        a.pos = vec![Vec3::splat(5.0), Vec3::splat(-5.0), Vec3::new(0.0, 0.0, 9.0)];
        b.pos = vec![Vec3::splat(5.0), Vec3::splat(-5.0), Vec3::new(0.0, 0.0, 9.0)];
        advect(&mut a, &field, &domain, 0.016, 1.0);
        advect(&mut b, &field, &domain, 0.016, 1.0);
        assert_eq!(positions(&a), positions(&b), "respawns come from the same seeded stream");
    }

    #[test]
    fn dots_count_matches_particles() {
        let field = FieldExpr::parse("(2, 0)").expect("constant field parses");
        let domain = Domain::default();
        let mut cloud = Particles::default();
        spawn(&mut cloud, &domain, 64, 3);
        let dots = dot_instances(&cloud, &field, &domain, Colormap::Viridis, (0.0, 2.0), 0.05);
        assert_eq!(dots.len(), count(&cloud), "one dot instance per particle");
        assert!(dots.len() <= MAX_PARTICLES, "inside the instance budget");
        for (d, p) in dots.iter().zip(positions(&cloud).iter()) {
            assert_eq!(d.origin, [p.x, p.y, p.z, 1.0], "the dot is centered on the particle");
        }
    }

    #[test]
    fn dot_radius_follows_grid_spacing() {
        let field = FieldExpr::parse("(1, 0)").expect("pump field parses");
        let domain = Domain::default();
        let mut cloud = Particles::default();
        spawn(&mut cloud, &domain, 16, 3);
        let radius = 2.0;
        let dots = dot_instances(&cloud, &field, &domain, Colormap::Turbo, (0.0, 1.0), radius);
        assert_eq!(dots.len(), 16);
        for d in &dots {
            assert_eq!(d.dir_len[3], radius, "radius in dir_len.w: the dot shader reads it there");
        }
    }

    #[test]
    fn dots_colored_by_field_magnitude() {
        // Dots share the arrows' color rule: the colormap sampled at the
        // field's magnitude, so a constant field paints every dot the same.
        let field = FieldExpr::parse("(2, 0)").expect("constant field parses");
        let domain = Domain::default();
        let mut cloud = Particles::default();
        spawn(&mut cloud, &domain, 8, 3);
        let bright = dot_instances(&cloud, &field, &domain, Colormap::Mono, (0.0, 2.0), 0.1);
        for d in &bright {
            assert_eq!(d.color, [1.0, 1.0, 1.0, 1.0], "mono ramp at its maximum: white");
        }
        let dim = dot_instances(&cloud, &field, &domain, Colormap::Mono, (0.0, 8.0), 0.1);
        let quarter = 64_f32 / 255.0;
        for (d, e) in dim.iter().zip(bright.iter()) {
            assert_eq!(d.color, [quarter, quarter, quarter, 1.0], "same field, a quarter of a wider ramp");
            assert_eq!(d.origin, e.origin, "only the color moves with the range");
        }
    }

    #[test]
    fn clear_empties_the_cloud() {
        let field = FieldExpr::parse("(1, 0)").expect("pump field parses");
        let domain = Domain::default();
        let mut cloud = Particles::default();
        spawn(&mut cloud, &domain, 24, 3);
        clear(&mut cloud);
        assert_eq!(count(&cloud), 0, "turning the toggle off empties the cloud");
        assert!(dot_instances(&cloud, &field, &domain, Colormap::Viridis, (0.0, 1.0), 0.05).is_empty(),
            "no dots painted for an empty cloud");
    }

    #[test]
    fn spawn_clear_spawn_is_deterministic() {
        let domain = Domain::default();
        let mut cloud = Particles::default();
        spawn(&mut cloud, &domain, 24, 3);
        let first = positions(&cloud);
        clear(&mut cloud);
        spawn(&mut cloud, &domain, 24, 3);
        assert_eq!(positions(&cloud), first,
            "a toggle off/on cycle respawns from the fixed seed, not the advanced stream");
    }
}
