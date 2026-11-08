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
//! under camera changes. The 3D view additionally seeds from a camera ray
//! ([`ray_seed`], ticket 06): a click lands on the domain box where the user
//! points, at the camera's focal plane.

use glam::{Vec2, Vec3, Vec4};

use crate::camera::OrbitCamera;
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
/// [`OrbitCamera::ndc_ray`] and land it on the domain box.
///
/// The seed sits where the user points: on the focal plane through the camera's
/// `target`, perpendicular to the view direction, clamped to the box's near/far
/// crossing along the ray. Clamping to that crossing (rather than taking the
/// bare entry point) is what gives a traced streamline room to run: a near-face
/// entry would sit in front of the field with only the box's depth ahead of it,
/// while a centre click then seeds exactly the target the user aimed at.
///
/// A click whose ray misses the box falls back to a shell entry on a sphere
/// about the target at half the eye-to-target distance (or, for a ray that
/// misses that sphere too, the point of closest approach projected out to it),
/// which `trace` then drops: clicking empty space draws nothing. Returns `None`
/// only when the camera does not describe a finite ray.
pub fn ray_seed(camera: OrbitCamera, domain: &Domain, aspect: f32, ndc_xy: Vec2) -> Option<Vec3> {
    let aspect = if aspect.is_finite() && aspect > 0.0 { aspect } else { 1.0 };
    let (origin, direction) = camera.ndc_ray(aspect, ndc_xy);
    let span = direction.length();
    if !(origin.is_finite() && direction.is_finite()) || !span.is_finite() || span <= 0.0 {
        return None;
    }
    let view = direction * (1.0 / span);

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
        // Depth of the focal plane — the plane through the camera's target,
        // perpendicular to the eye-to-target axis — measured along the clicked
        // ray, then clamped into the box crossing so an extreme click still
        // seeds a usable point. A centre click lands exactly on the target.
        let axis = (camera.target - camera.position()).normalize_or_zero();
        let denom = view.dot(axis);
        let t_world = if denom.abs() < 1e-6 { 0.0 } else { (camera.target - origin).dot(axis) / denom };
        let t_focal = t_world / span;
        return Some(origin + direction * t_focal.clamp(t_in, t_out));
    }

    // Box missed: the shell fallback. `s` below is in world units along the unit
    // view direction, the sphere being half the eye-to-target distance.
    let to_target = camera.target - origin;
    let shell = to_target.length() * 0.5;
    if !(to_target.is_finite()) || !shell.is_finite() || shell <= 0.0 {
        return None;
    }
    let along = to_target.dot(view);
    let lateral = (to_target - view * along).length();
    if lateral <= shell {
        // Entry into the shell, on the near side of the target.
        return Some(origin + view * (along - (shell * shell - lateral * lateral).sqrt()));
    }
    // The ray misses the shell as well: project its point of closest approach
    // radially out onto it, so the seed still reads as "the direction clicked".
    let nearest = origin + view * along;
    let outward = nearest - camera.target;
    if !outward.is_finite() || outward.length() <= 0.0 {
        return None;
    }
    Some(camera.target + outward.normalize() * shell)
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
    camera: OrbitCamera,
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

#[cfg(test)]
mod tests {
    use super::{CLICK_BURST_REACH, CLICK_BURST_SEEDS, burst_seeds, click_seed, ray_seed, seed_grid, trace, StreamlineOpts};
    use glam::{Vec2, Vec3};
    use crate::camera::OrbitCamera;
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
    fn vortex_stays_bounded() {
        // Binding plan-task-9 test: vortex (-y, x, 0) on [-1,1]², traced from
        // (1, 0) with `both_directions`, keeps every point of the polyline at
        // |pos| <= 1.3 and spends at least 50 points. Plus sequence validity:
        // no consecutive duplicates (a zero-length segment would normalise a
        // zero vector in the shader) and every consecutive chord turns CCW —
        // one continuous farthest-backward -> seed -> farthest-forward ribbon,
        // not a reversed leg with a seed pasted in the middle.
        let f = FieldExpr::parse("(-y, x)").expect("vortex field parses");
        let d = Domain::new(Vec3::new(-1.0, -1.0, 0.0), Vec3::new(1.0, 1.0, 0.0));
        let opts = StreamlineOpts { max_len: 200, step_scale: 0.05, both_directions: true };
        let line = trace(&f, Vec3::new(1.0, 0.0, 0.0), &d, &opts);
        assert!(line.len() >= 50, "both directions spend the budget: >= 50 points");
        for p in &line {
            assert!(p.is_finite(), "no NaN anywhere on the polyline");
            assert!(p.length() <= 1.3, "every point bounded: the circle at radius 1 stays well inside 1.3");
        }
        for i in 1..line.len() {
            let chord = line[i] - line[i - 1];
            assert!(chord.length() > 1e-3, "no duplicate points: every segment is real (steps are ~0.05)");
            if i > 1 {
                let prev = line[i - 1] - line[i - 2];
                assert!(prev.cross(chord).z > 1e-6, "ordered path: consecutive chords keep turning CCW");
            }
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

    // --- the 3D click-to-seed path (ticket 06) --------------------------------

    /// Asserts two points coincide, to the tolerance the unprojection's f32
    /// noise allows.
    fn assert_close(actual: Vec3, expected: Vec3, what: &str) {
        let gap = (actual - expected).length();
        assert!(gap < 1e-3, "{what}: |{actual:?} - {expected:?}| = {gap}");
    }

    /// A view direction for the focal-plane assertions below.
    fn view_of(camera: OrbitCamera) -> Vec3 {
        let (_, direction) = camera.ndc_ray(1.0, Vec2::ZERO);
        direction * (1.0 / direction.length())
    }

    #[test]
    fn ray_seed_inside_domain() {
        // Pinned ticket-06 test: a centre click (ndc 0,0) at the default orbit
        // aimed at the origin of a [-1,1]^3 domain unprojects to a point inside
        // that domain, and tracing the depth-carrying vortex (-y, x, 0.5) from
        // it spends the point budget (>= 50 points) rather than dying at once.
        // This is what pulls streamlines into the 3D view at all.
        let camera = OrbitCamera::default();
        let domain = Domain::default();
        let seed = ray_seed(camera, &domain, 1.0, Vec2::ZERO).expect("a centre click describes a seed");
        assert!(inside(&domain, seed), "the seed lands inside the domain: {seed:?}");

        let field = FieldExpr::parse("(-y, x, 0.5)").expect("depth-carrying vortex parses");
        let opts = StreamlineOpts { max_len: 200, step_scale: 0.02, both_directions: true };
        let line = trace(&field, seed, &domain, &opts);
        assert!(line.len() >= 50, "the ray-seeded streamline spends the budget: {} points", line.len());
    }

    #[test]
    fn ray_seed_stays_on_the_clicked_ray() {
        // The seed never wanders off the ray the click describes: it is
        // collinear with the unprojected direction and in front of the eye.
        let camera = OrbitCamera::default();
        let domain = Domain::default();
        let clicks = [
            Vec2::ZERO,
            Vec2::new(0.4, -0.25),
            Vec2::new(-0.35, 0.3),
            Vec2::new(0.15, 0.15),
            Vec2::new(-0.45, -0.4),
        ];
        for ndc_xy in clicks {
            let (origin, direction) = camera.ndc_ray(1.0, ndc_xy);
            let seed = ray_seed(camera, &domain, 1.0, ndc_xy).expect("these clicks land on the box");
            let offset = seed - origin;
            assert!(direction.cross(offset).length() <= 1e-2 * direction.length(), "seed on the clicked ray");
            assert!(direction.dot(offset) >= 0.0, "seed in front of the eye");
            assert!(inside(&domain, seed), "the seed stays inside the domain: {seed:?}");
        }
    }
    #[test]
    fn ray_seed_seeds_the_aimed_point_and_the_focal_plane() {
        // A centre click seeds the point the camera is aimed at (the orbit
        // target), whatever the orbit angles and zoom, and that seed lies on
        // the focal plane through the target, perpendicular to the view.
        let domain = Domain::default();
        let field = FieldExpr::parse("(-y, x, 0.5)").expect("depth-carrying vortex parses");
        let opts = StreamlineOpts { max_len: 200, step_scale: 0.02, both_directions: true };

        let mut orbited = OrbitCamera::default();
        orbited.orbit(1.4, -0.45);
        let mut zoomed = OrbitCamera::default();
        zoomed.zoom(0.25);
        let mut pulled = OrbitCamera::default();
        pulled.zoom(4.0);

        for camera in [OrbitCamera::default(), orbited, zoomed, pulled] {
            let seed = ray_seed(camera, &domain, 1.6, Vec2::ZERO).expect("a centre click describes a seed");
            assert_close(seed, camera.target, "a centre click seeds the point the camera is aimed at");
            let view = view_of(camera);
            assert!((seed - camera.target).dot(view).abs() < 1e-3, "the seed lies on the focal plane");
            let line = trace(&field, seed, &domain, &opts);
            assert!(line.len() >= 50, "the click spends the budget: {} points", line.len());
        }
    }

    #[test]
    fn ray_seed_interior_clicks_run_the_line_through_the_box() {
        // Off-centre clicks that hit the box's interior (not its skin) put the
        // seed on the same focal plane; the traced line still runs the field's
        // full depth through the box.
        let domain = Domain::default();
        let field = FieldExpr::parse("(-y, x, 0.5)").expect("depth-carrying vortex parses");
        let opts = StreamlineOpts { max_len: 200, step_scale: 0.02, both_directions: true };
        let camera = OrbitCamera::default();
        let view = view_of(camera);
        for ndc_xy in [Vec2::new(0.2, -0.15), Vec2::new(-0.3, 0.35)] {
            let seed = ray_seed(camera, &domain, 1.0, ndc_xy).expect("these clicks land on the box");
            assert!(inside(&domain, seed), "seed inside the domain: {seed:?}");
            assert!((seed - camera.target).dot(view).abs() < 1e-3, "interior click stays on the focal plane");
            let line = trace(&field, seed, &domain, &opts);
            assert!(line.len() >= 50, "the click spends the budget: {} points", line.len());
        }
    }

    #[test]
    fn ray_seed_click_off_the_field_traces_nothing() {
        // A wide-frustum corner click whose ray misses the box falls back to a
        // shell point outside the domain, which `trace` then drops: clicking
        // empty space draws nothing, and the caller still gets a finite point.
        let domain = Domain::default();
        let field = FieldExpr::parse("(-y, x, 0.5)").expect("depth-carrying vortex parses");
        let opts = StreamlineOpts { max_len: 200, step_scale: 0.02, both_directions: true };
        let wide = OrbitCamera { yaw: 0.8, pitch: 0.9, distance: 3.2, target: Vec3::ZERO, fov_y: 1.5, ..OrbitCamera::default() };
        let seed = ray_seed(wide, &domain, 1.0, Vec2::new(1.0, -1.0)).expect("a missed click describes a point");
        assert!(seed.is_finite(), "a missed click describes a finite point: {seed:?}");
        assert!(!inside(&domain, seed), "the shell fallback sits off the domain: {seed:?}");
        assert!(
            trace(&field, seed, &domain, &opts).is_empty(),
            "a click off the field traces nothing",
        );
    }

    #[test]
    fn ray_seed_degenerate_view_gives_no_seed() {
        // A camera that does not describe a finite ray (a non-finite orbit
        // distance) yields no seed rather than a NaN point the renderer would
        // choke on.
        let domain = Domain::default();
        let broken = OrbitCamera { yaw: 0.8, pitch: 0.9, distance: f32::INFINITY, target: Vec3::ZERO, fov_y: 55.0_f32.to_radians(), ..OrbitCamera::default() };
        assert!(ray_seed(broken, &domain, 1.0, Vec2::ZERO).is_none(), "an infinite orbit gives no seed");
        let unusable = OrbitCamera { yaw: 0.8, pitch: 0.9, distance: f32::NAN, target: Vec3::ZERO, fov_y: 55.0_f32.to_radians(), ..OrbitCamera::default() };
        assert!(ray_seed(unusable, &domain, 1.0, Vec2::ZERO).is_none(), "a NaN orbit gives no seed");
        let squashed = OrbitCamera { yaw: 0.8, pitch: 1.55, distance: 0.5, target: Vec3::ZERO, fov_y: 55.0_f32.to_radians(), ..OrbitCamera::default() };
        assert!(ray_seed(squashed, &domain, 1.0, Vec2::ZERO).is_some(), "a pole-on orbit still describes a ray");
    }

    // --- the mode-aware click entry point (ticket 06) ------------------------

    /// The NDC a world point projects to under `vp`, for the round-trip test.
    fn project(vp: glam::Mat4, p: Vec3) -> Vec2 {
        let clip = vp * glam::Vec4::new(p.x, p.y, p.z, 1.0);
        Vec2::new(clip.x / clip.w, clip.y / clip.w)
    }

    #[test]
    fn click_seed_2d_lands_on_the_midplane_under_the_click() {
        // The 2D view is the same orbit camera run orthographically. A click
        // maps onto the plane the 2D grid is sampled on (the domain's z
        // midpoint) and, crucially, projects back to exactly the NDC clicked:
        // the seed is under the pointer, not beside it.
        let camera = OrbitCamera::default();
        let domain = Domain::default();
        let aspect = 1.6;
        let plane_size = 2.7;
        let vp = camera.vp_matrix(aspect, Some(plane_size));
        for ndc in [Vec2::ZERO, Vec2::new(0.4, -0.2), Vec2::new(-0.6, 0.5), Vec2::new(0.9, 0.9)] {
            let seed = click_seed(camera, &domain, aspect, ndc, Some(plane_size))
                .expect("a 2D click describes a seed");
            let z_mid = (domain.min.z + domain.max.z) * 0.5;
            assert!((seed.z - z_mid).abs() < 1e-4, "the seed sits on the domain's z-midpoint plane: {seed:?}");
            let back = project(vp, seed);
            assert!((back - ndc).length() < 1e-3, "the seed projects back under the click: {back:?} vs {ndc:?}");
        }
    }

    #[test]
    fn click_seed_2d_follows_the_domain_midplane() {
        // A domain whose z midpoint is not the camera target's z still seeds on
        // that midpoint plane: the click is an orthographic ray and the plane
        // picks the point along it.
        let camera = OrbitCamera::default();
        let domain = Domain::new(Vec3::new(-1.0, -1.0, 0.5), Vec3::new(1.0, 1.0, 0.5));
        let seed = click_seed(camera, &domain, 1.0, Vec2::ZERO, Some(2.7))
            .expect("a 2D click describes a seed");
        assert!((seed.z - 0.5).abs() < 1e-4, "the seed follows the shifted midplane: {seed:?}");
    }

    #[test]
    fn click_seed_3d_is_the_camera_ray_seed() {
        // In 3D mode the entry point is exactly the tested camera-ray seed:
        // one mode-aware function, no second placement rule to drift.
        let camera = OrbitCamera::default();
        let domain = Domain::default();
        for ndc in [Vec2::ZERO, Vec2::new(0.35, -0.3), Vec2::new(-0.5, 0.45)] {
            let direct = ray_seed(camera, &domain, 1.3, ndc).expect("the ray lands on the box");
            let via = click_seed(camera, &domain, 1.3, ndc, None).expect("a 3D click describes a seed");
            assert_close(via, direct, "3D click seeding uses the camera-ray seed");
        }
    }

    #[test]
    fn click_seed_degenerate_camera_gives_no_seed() {
        // A broken orbit describes no usable ray in either mode: no seed, not
        // a NaN point the tracer and renderer would choke on.
        let domain = Domain::default();
        let broken = OrbitCamera { yaw: 0.8, pitch: 0.9, distance: f32::NAN, target: Vec3::ZERO, fov_y: 55.0_f32.to_radians(), ..OrbitCamera::default() };
        assert!(click_seed(broken, &domain, 1.0, Vec2::ZERO, None).is_none(), "3D: no seed from a broken camera");
        assert!(click_seed(broken, &domain, 1.0, Vec2::ZERO, Some(2.7)).is_none(), "2D: no seed from a broken camera");
        assert!(click_seed(OrbitCamera::default(), &domain, 1.0, Vec2::ZERO, Some(0.0)).is_some(),
            "a zero plane size still describes the centre ray");
    }

    // --- the click burst (ticket 06) -----------------------------------------

    #[test]
    fn burst_seeds_cluster_around_the_click() {
        // One click becomes a small cluster: the click itself plus jittered
        // neighbours, all inside the domain and within the burst reach.
        let domain = Domain::default();
        let center = Vec3::new(0.25, -0.1, 0.3);
        let seeds = burst_seeds(center, &domain, CLICK_BURST_SEEDS, 3);
        assert_eq!(seeds.len(), CLICK_BURST_SEEDS, "one seed per burst member");
        assert_eq!(seeds[0], center, "the click itself leads the burst");
        let reach = CLICK_BURST_REACH * 2.0; // the default domain's smallest extent
        for seed in &seeds {
            assert!(inside(&domain, *seed), "burst seeds stay in the domain: {seed:?}");
            assert!((*seed - center).length() <= reach * 3.0_f32.sqrt() + 1e-5, "inside the burst reach: {seed:?}");
        }
        assert!(seeds[1..].iter().any(|s| *s != center), "the cluster actually spreads");
    }

    #[test]
    fn burst_seeds_are_deterministic_and_2d_pins_the_plane() {
        // The same click redraws the same burst (rebuilds must not shimmer),
        // a different click draws a different one, and 2D bursts stay on the
        // clicked plane instead of jittering out of it.
        let domain = Domain::default();
        let center = Vec3::new(-0.2, 0.4, 0.0);
        let a = burst_seeds(center, &domain, CLICK_BURST_SEEDS, 3);
        let b = burst_seeds(center, &domain, CLICK_BURST_SEEDS, 3);
        assert_eq!(a, b, "the same click draws the same burst");
        let c = burst_seeds(Vec3::new(0.3, 0.2, -0.4), &domain, CLICK_BURST_SEEDS, 3);
        assert_ne!(a[1..], c[1..], "a different click draws a different burst");
        let flat = burst_seeds(center, &domain, CLICK_BURST_SEEDS, 2);
        assert!(flat.iter().all(|s| s.z == center.z), "2D bursts stay on the clicked plane: {flat:?}");
    }

    #[test]
    fn burst_seeds_skip_clicks_off_the_field() {
        // A click outside the domain has nothing to trace from, and an empty
        // request asks for no seeds: both yield an empty burst.
        let domain = Domain::default();
        assert!(burst_seeds(Vec3::new(5.0, 0.0, 0.0), &domain, 4, 3).is_empty(), "off the domain seeds nothing");
        assert!(burst_seeds(Vec3::ZERO, &domain, 0, 3).is_empty(), "no burst for an empty request");
    }
}
