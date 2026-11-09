//! Streamline tracing: RK4 integration of a field's flow lines.
//!
//! A streamline starts at a seed point and follows the field with adaptive
//! RK4 steps (`h = step_scale / |f|`, so fast regions cover more distance per
//! point). It abandons where the field is zero or non-finite or where it exits
//! the domain. In the 2D view the field is a `Vec3` with `z == 0`; RK4 uses
//! the full `Vec3` (harmless for a planar field).
//!
//! Seeds are a jittered grid (ticket ruling): a regular lattice over the view
//! with a seeded, deterministic per-point jitter so streamlines are spread
//! evenly but not on a rigid lattice — reproducible across rebuilds, stable
//! under camera changes. The 3D view additionally seeds from a camera ray
//! ([`ray_seed`], ticket 06): a click lands on the domain box where the user
//! points, at the camera's focal plane.

use glam::{Vec2, Vec3, Vec4};

use crate::camera::ViewCamera;
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

/// Streamline seeds one click produces: the picked point plus a small
/// surrounding cluster (a "burst" — ticket 06).
pub const CLICK_BURST_SEEDS: usize = 5;
/// Radius of that cluster, as a fraction of the domain's smallest positive
/// extent, so a click reads as a local spray at any zoom of the domain.
pub const CLICK_BURST_REACH: f32 = 0.08;

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
/// zero. With `both_directions` the result is one continuous ordered path
/// running farthest-upstream → seed → farthest-downstream: the seed is
/// counted exactly once, so no zero-length segment can reach the renderer.
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
    // seed itself is counted once, at the seam. The upstream walk runs
    // seed-outward, so reverse everything past the seed to lay the polyline
    // farthest-backward first, then append the downstream walk (which starts
    // at the seed).
    let back = integrate(field, start, domain, cap.div_ceil(2), step_scale, -1.0);
    let forward = integrate(field, start, domain, cap + 1 - back.len(), step_scale, 1.0);
    let mut line: Vec<Vec3> = Vec::new();
    line.extend(back[1..].iter().rev().copied());
    line.extend(forward);
    line
}

/// Seed point for a click at `ndc_xy` in the 3D view: unproject the click with
/// the active [`ViewCamera`] and land it on the domain box.
///
/// The seed sits where the user points: on the focal plane through the view's
/// focus point (the orbit target, or the domain centre while flying),
/// perpendicular to the view direction, clamped to the box's near/far crossing
/// along the ray. Clamping to that crossing (rather than taking the bare entry
/// point) is what gives a traced streamline room to run: a near-face entry would
/// sit in front of the field with only the box's depth ahead of it, while a
/// centre click then seeds exactly the point the user aimed at.
///
/// A click whose ray misses the box falls back to a shell entry on a sphere
/// about the focus at half the eye-to-focus distance (or, for a ray that misses
/// that sphere too, the point of closest approach projected out to it), which
/// `trace` then drops: clicking empty space draws nothing. Returns `None` only
/// when the camera does not describe a finite ray.
pub fn ray_seed(
    camera: ViewCamera<'_>,
    domain: &Domain,
    aspect: f32,
    ndc_xy: Vec2,
) -> Option<Vec3> {
    let aspect = if aspect.is_finite() && aspect > 0.0 { aspect } else { 1.0 };
    let (origin, direction) = camera.ndc_ray(aspect, ndc_xy);
    let span = direction.length();
    if !(origin.is_finite() && direction.is_finite()) || !span.is_finite() || span <= 0.0 {
        return None;
    }
    let view = direction * (1.0 / span);
    // Orbit views aim at their target; a fly camera has no target, so point at
    // the centre of the field the user is exploring.
    let focus = match camera {
        ViewCamera::Orbit(orbit) => orbit.target,
        ViewCamera::Fly(_) => (domain.min + domain.max) * 0.5,
    };

    // Ray/box crossing (the slab test). `t` is in units of the raw, unnormalized
    // `direction`, so `t == 1` is the far plane; a ray parallel to an axis gives
    // +/-inf bounds, which the min/max reduction of the three axes ignores unless
    // the ray is entirely outside a slab (then the crossing is empty).
    let reciprocal = Vec3::splat(1.0) / direction;
    let t_enter = (domain.min - origin) * reciprocal;
    let t_exit = (domain.max - origin) * reciprocal;
    let t_in = Vec3::min(t_enter, t_exit).max_element().max(0.0);
    let t_out = Vec3::max(t_enter, t_exit).min_element();
    if t_in <= t_out {
        // Depth of the focal plane — the plane through the focus point,
        // perpendicular to the view axis — measured along the clicked ray,
        // then clamped into the box crossing so an extreme click still seeds a
        // usable point. A centre click lands exactly on the focus.
        let axis = (focus - origin).normalize_or_zero();
        let denom = view.dot(axis);
        let t_world = if denom.abs() < 1e-6 { 0.0 } else { (focus - origin).dot(axis) / denom };
        let t_focal = t_world / span;
        return Some(origin + direction * t_focal.clamp(t_in, t_out));
    }

    // Box missed: the shell fallback. `s` below is in world units along the unit
    // view direction, the sphere being half the eye-to-focus distance.
    let to_focus = focus - origin;
    let shell = to_focus.length() * 0.5;
    if !(to_focus.is_finite()) || !shell.is_finite() || shell <= 0.0 {
        return None;
    }
    let along = to_focus.dot(view);
    let lateral = (to_focus - view * along).length();
    if lateral <= shell {
        // Entry into the shell, on the near side of the focus.
        return Some(origin + view * (along - (shell * shell - lateral * lateral).sqrt()));
    }
    // The ray misses the shell as well: project its point of closest approach
    // radially out onto it, so the seed still reads as "the direction clicked".
    let nearest = origin + view * along;
    let outward = nearest - focus;
    if !outward.is_finite() || outward.length() <= 0.0 {
        return None;
    }
    Some(focus + outward.normalize() * shell)
}

/// The mode-aware click-to-seed entry point: where a click at `ndc_xy` plants a
/// seed in the current view.
///
/// `ortho_plane_size == None` is the 3D view, and the seed is exactly
/// [`ray_seed`]'s focal-plane placement on the camera ray. `Some(plane_size)` is
/// the 2D view — the same orbit camera run through its orthographic projection
/// — and the seed is where that click's ortho ray crosses the domain's z
/// midpoint, the plane the 2D grid is sampled on. The seed is not clamped to the
/// domain: a click on empty space returns a point the tracer then drops, so
/// "clicked nothing" draws nothing.
///
/// Returns `None` only when the camera or the aspect does not describe a usable
/// ray.
pub fn click_seed(
    camera: ViewCamera<'_>,
    domain: &Domain,
    aspect: f32,
    ndc_xy: Vec2,
    ortho_plane_size: Option<f32>,
) -> Option<Vec3> {
    let aspect = if aspect.is_finite() && aspect > 0.0 { aspect } else { 1.0 };
    let Some(plane_size) = ortho_plane_size else {
        return ray_seed(camera, domain, aspect, ndc_xy);
    };
    let plane_size = if plane_size.is_finite() && plane_size > 0.0 { plane_size } else { 1.0 };
    let inv_vp = camera.vp_matrix(aspect, Some(plane_size)).inverse();
    if !inv_vp.is_finite() {
        return None;
    }
    let unproject = |z: f32| {
        let p = inv_vp * Vec4::new(ndc_xy.x, ndc_xy.y, z, 1.0);
        Vec3::new(p.x, p.y, p.z) / p.w
    };
    // The ortho click's ray is the near→far segment through the NDC point; only
    // its direction matters for landing it on the plane.
    let near = unproject(-1.0);
    let far = unproject(1.0);
    let direction = far - near;
    if !(near.is_finite() && direction.is_finite()) || direction.length_squared() <= 0.0 {
        return None;
    }
    let dz = direction.z;
    if !dz.is_finite() || dz.abs() < 1e-9 {
        return None;
    }
    let z_mid = (domain.min.z + domain.max.z) * 0.5;
    let seed = near + direction * ((z_mid - near.z) / dz);
    seed.is_finite().then_some(seed)
}

/// The seeds one click plants: the clicked point first, then `count - 1`
/// deterministic jittered neighbours inside a small cluster around it.
///
/// `dimension == 3` jitters the full box, so a 3D burst has depth; any other
/// value leaves z at the clicked coordinate, keeping a 2D burst on its plane.
/// The offsets are keyed on the clicked point's bits (splitmix64), so the same
/// click redraws the same burst across frames while different clicks differ.
/// Returns an empty vector for `count == 0` or a click outside the domain.
pub fn burst_seeds(center: Vec3, domain: &Domain, count: usize, dimension: usize) -> Vec<Vec3> {
    if count == 0 || !inside(domain, center) {
        return Vec::new();
    }
    let size = domain.domain_size();
    let smallest = [size.x, size.y, size.z]
        .into_iter()
        .filter(|extent| *extent > 0.0)
        .fold(f32::INFINITY, f32::min);
    let reach = if smallest.is_finite() { CLICK_BURST_REACH * smallest } else { 0.0 };
    let salt = (center.x.to_bits() as u64)
        ^ ((center.y.to_bits() as u64) << 21)
        ^ ((center.z.to_bits() as u64) << 42);
    let mut seeds = Vec::with_capacity(count);
    seeds.push(center);
    for i in 1..count {
        let (ux, uy, uz) = unit_triple(rand_stream(salt, i as u64));
        let dz = if dimension == 3 { uz } else { 0.0 };
        let p = center + Vec3::new(ux, uy, dz) * reach;
        seeds.push(Vec3::new(
            p.x.clamp(domain.min.x, domain.max.x),
            p.y.clamp(domain.min.y, domain.max.y),
            p.z.clamp(domain.min.z, domain.max.z),
        ));
    }
    seeds
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

/// Three independent 21-bit slices of a splitmix64 output mapped onto [-1, 1):
/// one per axis, so a click-burst offset fills a small box in one draw.
fn unit_triple(z: u64) -> (f32, f32, f32) {
    const SLICE: u64 = 0x1FFFFF;
    const SCALE: f32 = 2.0 / 2097152.0;
    let a = ((z >> 42) & SLICE) as u32 as f32;
    let b = ((z >> 21) & SLICE) as u32 as f32;
    let c = (z & SLICE) as u32 as f32;
    (a * SCALE - 1.0, b * SCALE - 1.0, c * SCALE - 1.0)
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

