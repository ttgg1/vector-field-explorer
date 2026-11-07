//! Streamline tracing: RK4 integration of a field's flow lines.
//!
//! A streamline starts at a seed point and follows the field with adaptive
//! RK4 steps (`h = step_scale / |f|`, so fast regions cover more distance per
//! point). It abandons where the field is zero or non-finite or where it exits
//! the domain. In the 2D view the field is a `Vec3` with `z == 0`; RK4 uses
//! the full `Vec3` (harmless for a planar field). See `docs/superpowers/plans/`
//! task 9.
//!
//! Seeds are a jittered grid (ticket ruling): a regular lattice over the view
//! with a seeded, deterministic per-point jitter so streamlines are spread
//! evenly but not on a rigid lattice — reproducible across rebuilds, stable
//! under camera changes. Click-to-seed is ticket 6.

use glam::Vec3;

use crate::domain::Domain;
use crate::field::FieldExpr;

/// Tracing parameters (see plan task 9).
pub struct StreamlineOpts {
    /// Requested point budget per line (capped at [`MAX_STREAMLINE_POINTS`]).
    pub max_len: usize,
    /// Target arc-length advanced per RK4 step, world units.
    pub step_scale: f32,
    /// Trace upstream and downstream from the seed.
    pub both_directions: bool,
}

/// Hard cap on points per streamline, regardless of the requested budget.
pub const MAX_STREAMLINE_POINTS: usize = 400;

/// Inclusive axis-aligned containment (the `Domain` type has no `contains`).
fn inside(domain: &Domain, p: Vec3) -> bool {
    p.x >= domain.min.x && p.x <= domain.max.x
        && p.y >= domain.min.y && p.y <= domain.max.y
        && p.z >= domain.min.z && p.z <= domain.max.z
}

/// Field flow at `p`, negated for `sign < 0` (upstream tracing). Missing or
/// non-finite evaluations read as the zero vector, which ends an integration.
fn flow_at(field: &FieldExpr, p: Vec3, sign: f32) -> Vec3 {
    match field.eval3(p) {
        Some(v) if v.is_finite() => v * sign,
        _ => Vec3::splat(0.0),
    }
}

/// One-direction RK4 walk: 4 field evaluations per step
/// (`k1 = p + f(p)h/2`, `k2 = p + f(k1)h/2`, `k3 = p + f(k2)h`,
/// `next = p + (f(p) + 2f(k1) + 2f(k2) + f(k3))h/6`) with the time step
/// `h = step_scale / |f(p)|`, so each step advances about `step_scale` of
/// arc length. Stops at the budget, where the flow vanishes, or on a step
/// that leaves the domain or turns non-finite. Always returns at least
/// `[start]`.
fn integrate(field: &FieldExpr, start: Vec3, domain: &Domain, budget: usize, step_scale: f32, sign: f32) -> Vec<Vec3> {
    let mut line: Vec<Vec3> = vec![start];
    let mut p = start;
    while line.len() < budget {
        let f0 = flow_at(field, p, sign);
        let mag = f0.length();
        if mag <= 0.0 {
            break;
        }
        let h = step_scale / mag;
        let k1 = p + f0 * (h * 0.5);
        let f1 = flow_at(field, k1, sign);
        let k2 = p + f1 * (h * 0.5);
        let f2 = flow_at(field, k2, sign);
        let k3 = p + f2 * h;
        let f3 = flow_at(field, k3, sign);
        let next = p + (f0 + f1 * 2.0 + f2 * 2.0 + f3) * (h / 6.0);
        if !next.is_finite() || !inside(domain, next) {
            break;
        }
        line.push(next);
        p = next;
    }
    line
}

/// Trace a streamline from `start`. Returns points along the line (possibly
/// just `[start]`), empty when the seed is outside the domain or the budget is
/// zero.
pub fn trace(field: &FieldExpr, start: Vec3, domain: &Domain, opts: &StreamlineOpts) -> Vec<Vec3> {
    let cap = opts.max_len.min(MAX_STREAMLINE_POINTS);
    if cap == 0 || !inside(domain, start) {
        return Vec::new();
    }
    let step_scale = if opts.step_scale.is_finite() && opts.step_scale > 0.0 {
        opts.step_scale
    } else {
        0.05
    };
    if !opts.both_directions {
        return integrate(field, start, domain, cap, step_scale, 1.0);
    }
    // Spend the budget half upstream (rounded up), the rest downstream; the
    // seed itself is counted once, at the seam.
    let back = integrate(field, start, domain, cap.div_ceil(2), step_scale, -1.0);
    let mut line: Vec<Vec3> = Vec::new();
    for p in back[..back.len() - 1].iter().rev() {
        line.push(*p);
    }
    line.extend(integrate(field, start, domain, cap + 1 - back.len(), step_scale, 1.0));
    line
}

/// Jittered grid of seeds over the domain's xy plane at the plane's z midpoint:
/// `per_axis × per_axis` lattice points spanning the domain extents (endpoints
/// included for `per_axis > 1`, like `Domain::grid_points`), each displaced by
/// at most `jitter × cell size` in a seeded, deterministic pseudo-random
/// direction (splitmix64 keyed on `salt` and lattice index), clamped back into
/// the domain.
/// Splitmix64 stream value for `(salt, index)` — deterministic across
/// rebuilds and machines, no dependency on `rand`. All u64 ops wrap
/// (debug-build overflow panics otherwise).
fn rand_stream(salt: u64, index: u64) -> u64 {
    let z = salt.wrapping_add((index + 1).wrapping_mul(0x9E3779B97F4A7C15));
    let z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    let z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

/// Two independent 24-bit slices of a splitmix64 output mapped onto [-1, 1).
fn unit_pair(z: u64) -> (f32, f32) {
    let a = ((z >> 32) & 0xFFFFFF) as u32 as f32;
    let b = (z & 0xFFFFFF) as u32 as f32;
    (a * (2.0 / 16777216.0) - 1.0, b * (2.0 / 16777216.0) - 1.0)
}

pub fn seed_grid(domain: &Domain, per_axis: u32, jitter: f32, salt: u64) -> Vec<Vec3> {
    if per_axis == 0 {
        return Vec::new();
    }
    // Jitter past 0.5 x cell per axis would let lattice neighbours cross.
    let jitter = if jitter.is_finite() { jitter.clamp(0.0, 0.45) } else { 0.0 };
    let size = domain.domain_size();
    let cell_x = if per_axis == 1 { 0.0 } else { size.x / (per_axis - 1) as f32 };
    let cell_y = if per_axis == 1 { 0.0 } else { size.y / (per_axis - 1) as f32 };
    let z = (domain.min.z + domain.max.z) * 0.5;
    let mut seeds = Vec::new();
    for iy in 0..per_axis {
        for ix in 0..per_axis {
            let index = (iy * per_axis + ix) as u64;
            let (ux, uy) = unit_pair(rand_stream(salt, index));
            let bx = if per_axis == 1 { domain.min.x + size.x * 0.5 } else { domain.min.x + ix as f32 * cell_x };
            let by = if per_axis == 1 { domain.min.y + size.y * 0.5 } else { domain.min.y + iy as f32 * cell_y };
            let x = (bx + ux * jitter * cell_x).max(domain.min.x).min(domain.max.x);
            let y = (by + uy * jitter * cell_y).max(domain.min.y).min(domain.max.y);
            seeds.push(Vec3::new(x, y, z));
        }
    }
    seeds
}

#[cfg(test)]
mod tests {
    use super::{trace, seed_grid, StreamlineOpts};
    use glam::{Vec2, Vec3};
    use crate::domain::Domain;
    use crate::field::FieldExpr;

    /// Inclusive axis-aligned containment (Domain has no `contains` method).
    fn inside(d: &Domain, p: Vec3) -> bool {
        p.x >= d.min.x && p.x <= d.max.x
            && p.y >= d.min.y && p.y <= d.max.y
            && p.z >= d.min.z && p.z <= d.max.z
    }

    #[test]
    fn constant_field_traces_straight() {
        let f = FieldExpr::parse("(2, 0)").expect("constant field parses");
        let d = Domain::new(Vec3::new(-10.0, -10.0, -1.0), Vec3::new(10.0, 10.0, 1.0));
        let opts = StreamlineOpts { max_len: 50, step_scale: 0.2, both_directions: false };
        let line = trace(&f, Vec3::new(-2.0, 3.0, 0.0), &d, &opts);
        assert_eq!(line.len(), 50, "budget is spent in straight advance");
        for i in 0..line.len() {
            assert_eq!(line[i].y, 3.0, "constant (2,0) field never moves y");
            assert_eq!(line[i].z, 0.0, "planar field stays on z = 0");
            if i > 0 {
                assert!(line[i].x > line[i - 1].x, "advance follows the flow (+x)");
            }
        }
        // Adaptive step: h = step_scale / |f| = 0.1 is the time step, so the
        // RK4 increment over it is |f| * h = step_scale = 0.2 of advance.
        let dx = line[1].x - line[0].x;
        assert!((dx - 0.2).abs() < 1e-5, "advance per step is step_scale");
        for i in 2..line.len() {
            let step = line[i].x - line[i - 1].x;
            assert!((step - dx).abs() < 1e-5, "constant field steps evenly");
        }
    }

    #[test]
    fn zero_field_makes_no_advance() {
        let f = FieldExpr::parse("(0, 0)").expect("zero field parses");
        let d = Domain::new(Vec3::new(-1.0, -1.0, 0.0), Vec3::new(1.0, 1.0, 0.0));
        let opts = StreamlineOpts { max_len: 50, step_scale: 0.2, both_directions: true };
        let line = trace(&f, Vec3::new(0.4, -0.2, 0.0), &d, &opts);
        assert_eq!(line.len(), 1, "seed only: the flow vanishes here");
        assert!(line[0].is_finite(), "no NaN creeps into the seed");
    }

    #[test]
    fn seed_outside_domain_traces_nothing() {
        let f = FieldExpr::parse("(2, 0)").expect("constant field parses");
        let d = Domain::new(Vec3::new(-1.0, -1.0, 0.0), Vec3::new(1.0, 1.0, 0.0));
        let opts = StreamlineOpts { max_len: 50, step_scale: 0.2, both_directions: false };
        let line = trace(&f, Vec3::new(5.0, 0.0, 0.0), &d, &opts);
        assert!(line.is_empty(), "a seed outside the domain draws nothing");
    }

    #[test]
    fn vortex_streamline_stays_round() {
        // f = (-y, x): integral curves are circles about the origin; RK4 error
        // must not spiral the line out of a tight radius band.
        let f = FieldExpr::parse("(-y, x)").expect("vortex field parses");
        let d = Domain::new(Vec3::new(-2.0, -2.0, 0.0), Vec3::new(2.0, 2.0, 0.0));
        let opts = StreamlineOpts { max_len: 400, step_scale: 0.05, both_directions: true };
        let line = trace(&f, Vec3::new(0.5, 0.0, 0.0), &d, &opts);
        assert!(line.len() >= 50 && line.len() <= 400, "budget-bounded, never capped past 400");
        for p in &line {
            let r = Vec2::new(p.x, p.y).length();
            assert!((0.45..=0.55).contains(&r), "radius conserved near the seed's 0.5");
        }
    }

    #[test]
    fn jittered_seeds_are_reproducible() {
        let d = Domain::new(Vec3::new(-1.0, -1.0, 0.0), Vec3::new(1.0, 1.0, 0.0));
        let a = seed_grid(&d, 8, 0.4, 42);
        assert_eq!(a, seed_grid(&d, 8, 0.4, 42), "same salt, same seeds across rebuilds");
        assert_eq!(a.len(), 64, "8x8 grid of seeds");

        for s in &a {
            assert!(inside(&d, *s), "jittered seeds are clamped into the domain");
            assert_eq!(s.z, 0.0, "2D seeds sit on the plane midpoint");
        }

        // Jitter is bounded by jitter x cell: adjacent lattice points (cell 2/7)
        // can approach no closer than (1 - 2 x 0.4) x cell.
        let cell = 2.0 / 7.0;
        for i in 0..a.len() {
            for j in i + 1..a.len() {
                assert!(a[i].distance(a[j]) > cell * 0.1, "seeds stay spread apart");
            }
        }

        // The jitter actually moved most seeds off the rigid lattice.
        let mut moved = 0;
        for s in &a {
            let gx = ((s.x + 1.0) / cell).round() * cell - 1.0;
            if (s.x - gx).abs() > 1e-3 {
                moved += 1;
            }
        }
        assert!(moved > 32, "jittered, not a rigid lattice");

        // Different salt keys a different stream.
        assert!(a != seed_grid(&d, 8, 0.4, 43), "salt keys the jitter");
    }
}
