//! Streamline tracing coverage: RK4 integration, jittered seed grids, the
//! camera-ray and mode-aware click seeds, and click bursts.
//!
//! Moved out of `src/streamlines.rs`'s inline module (ticket 13). Every test
//! body is unchanged; only the imports moved from `super::*` / `crate::` to
//! `vector_field_explorer::`.

use glam::{Vec2, Vec3};
use vector_field_explorer::camera::{OrbitCamera, ViewCamera};
use vector_field_explorer::domain::Domain;
use vector_field_explorer::field::FieldExpr;
use vector_field_explorer::streamlines::{
    CLICK_BURST_REACH, CLICK_BURST_SEEDS, StreamlineOpts, burst_seeds, click_seed, ray_seed,
    seed_grid, trace, trace_at,
};

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
    let seed = ray_seed(ViewCamera::Orbit(&camera), &domain, 1.0, Vec2::ZERO).expect("a centre click describes a seed");
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
        let seed = ray_seed(ViewCamera::Orbit(&camera), &domain, 1.0, ndc_xy).expect("these clicks land on the box");
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
        let seed = ray_seed(ViewCamera::Orbit(&camera), &domain, 1.6, Vec2::ZERO).expect("a centre click describes a seed");
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
        let seed = ray_seed(ViewCamera::Orbit(&camera), &domain, 1.0, ndc_xy).expect("these clicks land on the box");
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
    let seed = ray_seed(ViewCamera::Orbit(&wide), &domain, 1.0, Vec2::new(1.0, -1.0)).expect("a missed click describes a point");
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
    assert!(ray_seed(ViewCamera::Orbit(&broken), &domain, 1.0, Vec2::ZERO).is_none(), "an infinite orbit gives no seed");
    let unusable = OrbitCamera { yaw: 0.8, pitch: 0.9, distance: f32::NAN, target: Vec3::ZERO, fov_y: 55.0_f32.to_radians(), ..OrbitCamera::default() };
    assert!(ray_seed(ViewCamera::Orbit(&unusable), &domain, 1.0, Vec2::ZERO).is_none(), "a NaN orbit gives no seed");
    let squashed = OrbitCamera { yaw: 0.8, pitch: 1.55, distance: 0.5, target: Vec3::ZERO, fov_y: 55.0_f32.to_radians(), ..OrbitCamera::default() };
    assert!(ray_seed(ViewCamera::Orbit(&squashed), &domain, 1.0, Vec2::ZERO).is_some(), "a pole-on orbit still describes a ray");
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
        let seed = click_seed(ViewCamera::Orbit(&camera), &domain, aspect, ndc, Some(plane_size))
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
    let seed = click_seed(ViewCamera::Orbit(&camera), &domain, 1.0, Vec2::ZERO, Some(2.7))
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
        let direct = ray_seed(ViewCamera::Orbit(&camera), &domain, 1.3, ndc).expect("the ray lands on the box");
        let via = click_seed(ViewCamera::Orbit(&camera), &domain, 1.3, ndc, None).expect("a 3D click describes a seed");
        assert_close(via, direct, "3D click seeding uses the camera-ray seed");
    }
}

#[test]
fn click_seed_degenerate_camera_gives_no_seed() {
    // A broken orbit describes no usable ray in either mode: no seed, not
    // a NaN point the tracer and renderer would choke on.
    let domain = Domain::default();
    let broken = OrbitCamera { yaw: 0.8, pitch: 0.9, distance: f32::NAN, target: Vec3::ZERO, fov_y: 55.0_f32.to_radians(), ..OrbitCamera::default() };
    assert!(click_seed(ViewCamera::Orbit(&broken), &domain, 1.0, Vec2::ZERO, None).is_none(), "3D: no seed from a broken camera");
    assert!(click_seed(ViewCamera::Orbit(&broken), &domain, 1.0, Vec2::ZERO, Some(2.7)).is_none(), "2D: no seed from a broken camera");
    assert!(click_seed(ViewCamera::Orbit(&OrbitCamera::default()), &domain, 1.0, Vec2::ZERO, Some(0.0)).is_some(),
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

#[test]
fn trace_at_varies_with_t() {
    let f = FieldExpr::parse("(cos(t), sin(t), 0)").expect("parses");
    let domain = Domain::default();
    let opts = StreamlineOpts { max_len: 10, step_scale: 0.1, both_directions: false };
    let a = trace_at(&f, Vec3::new(0.0, 0.0, 0.0), &domain, &opts, 0.0);
    let b = trace_at(&f, Vec3::new(0.0, 0.0, 0.0), &domain, &opts, 1.0);
    // Direction changes with t, so traces differ materially.
    assert!(a != b, "traces should differ materially");
    // At least one coordinate differs by more than 1e-3.
    let mut maxdiff = 0.0f32;
    for (pa, pb) in a.iter().zip(b.iter()) {
        maxdiff = maxdiff.max((pa.x - pb.x).abs().max((pa.y - pb.y).abs()));
    }
    assert!(maxdiff > 1e-3, "maxdiff = {}", maxdiff);
}

#[test]
fn trace_equals_t_zero() {
    let f = FieldExpr::parse("(cos(t), 0, 0)").expect("parses");
    let domain = Domain::default();
    let opts = StreamlineOpts { max_len: 10, step_scale: 0.1, both_directions: false };
    let a = trace(&f, Vec3::new(0.0, 0.0, 0.0), &domain, &opts);
    let b = trace_at(&f, Vec3::new(0.0, 0.0, 0.0), &domain, &opts, 0.0);
    assert_eq!(a, b);
}
