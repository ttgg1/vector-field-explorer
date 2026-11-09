//! Particle-cloud coverage: spawn jitter, click bursts, advection, respawns and
//! dot instances.
//!
//! Moved out of `src/particles.rs`'s inline module (ticket 13). Every test body
//! is unchanged except `outside_particle_respawns_inside` and
//! `zero_velocity_particle_respawns`, which now seed the cloud through the
//! `#[doc(hidden)]` `Particles::with_positions` constructor (same default RNG
//! seed as before) because the private `rng` field blocks a struct-update
//! literal outside the crate. Otherwise only the imports moved, from `super::*`
//! / `crate::` to `vector_field_explorer::`.

use glam::Vec3;
use vector_field_explorer::colormap::Colormap;
use vector_field_explorer::domain::Domain;
use vector_field_explorer::field::FieldExpr;
use vector_field_explorer::particles::{
    CLICK_BURST_PARTICLES, CLICK_SPAWN_REACH, DOT_SPACING_FRACTION, MAX_PARTICLES,
    PARTICLES_PER_DENSITY, Particles, advect, clear, count, dot_instances, effective_particle_count,
    resize, retarget, spawn, spawn_at, target_count,
};
use vector_field_explorer::viewmodel::State;

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
fn pump_field_step_scales_the_move() {
    // Pinned binding test: a pump field advances every particle by the clock's
    // step (within RK4 slack), never past the domain, whatever the spawn jitter.
    let field = FieldExpr::parse("(1, 0)").expect("pump field parses");
    let domain = Domain::default();
    let mut particles = Particles::default();
    spawn(&mut particles, &domain, 9, 3);
    let step = 0.016;
    let before = positions(&particles);
    advect(&mut particles, &field, &domain, step);
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
            (p.x - before[i].x) >= step * 0.5,
            "advanced along the flow by the step (within RK4 slack)",
        );
        assert!(p.x <= 1.0, "never past the domain edge");
    }
}

#[test]
fn zero_step_leaves_particles_put() {
    let field = FieldExpr::parse("(1, 0)").expect("pump field parses");
    let domain = Domain::default();
    let mut particles = Particles::default();
    spawn(&mut particles, &domain, 11, 3);
    let before = positions(&particles);
    advect(&mut particles, &field, &domain, 0.0);
    let ps = positions(&particles);
    assert_eq!(ps.len(), 11, "the count is constant");
    for (i, p) in ps.iter().enumerate() {
        assert_eq!(
            *p,
            before[i],
            "a zero step leaves every particle exactly at its previous position",
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
    // The only field we vary is `pos`; the LCG seed stays the Default one, via
    // the `#[doc(hidden)]` test seam (the RNG state itself stays private).
    let mut particles = Particles::with_positions(vec![
        Vec3::splat(5.0),
        Vec3::new(5.0, 0.0, 0.0),
    ]);
    advect(&mut particles, &field, &domain, 0.016);
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
    let mut particles = Particles::with_positions(vec![Vec3::splat(0.0)]);
    advect(&mut particles, &field, &domain, 0.016);
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
    let step = 0.016 * 2.0;
    for _ in 0..100 {
        advect(&mut particles, &field, &domain, step);
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
    advect(&mut a, &field, &domain, 0.016);
    advect(&mut b, &field, &domain, 0.016);
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
        assert_eq!(d.origin[..3], [p.x, p.y, p.z], "the dot is centered on the particle");
        assert_eq!(
            d.origin[3], 0.0,
            "origin.w tags a dot as a particle (ticket 62's combined stream)",
        );
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

// --- Ticket 30: explicit count and size ---------------------------------

#[test]
fn effective_count_is_auto_until_an_explicit_count_overrides_it() {
    assert_eq!(
        effective_particle_count(8, 0),
        128,
        "Auto is density x 16 (8 x 16 here)",
    );
    assert_eq!(
        effective_particle_count(8, 100),
        100,
        "a non-zero count overrides the density-derived value",
    );
    assert_eq!(
        effective_particle_count(8, 5000),
        MAX_PARTICLES as u32,
        "an explicit count still respects the particle cap",
    );
    assert_eq!(effective_particle_count(0, 0), 0, "zero density with Auto asks for nothing");
    assert_eq!(
        effective_particle_count(10_000, 0),
        MAX_PARTICLES as u32,
        "Auto is capped too",
    );
}

#[test]
fn a_lower_target_trims_the_cloud_and_keeps_the_survivors_in_place() {
    let domain = Domain::default();
    let mut cloud = Particles::default();
    spawn(&mut cloud, &domain, 120, 3);
    let before = positions(&cloud);

    resize(&mut cloud, &domain, 100, 3);

    assert_eq!(count(&cloud), 100, "the cloud shrinks to the target");
    assert_eq!(positions(&cloud), before[..100], "the surviving particles never move");
}

#[test]
fn a_higher_target_spawns_only_the_difference_without_moving_existing_particles() {
    let domain = Domain::default();
    let mut cloud = Particles::default();
    spawn(&mut cloud, &domain, 100, 3);
    let before = positions(&cloud);

    resize(&mut cloud, &domain, 180, 3);

    let after = positions(&cloud);
    assert_eq!(after.len(), 180, "the cloud grows to the target");
    assert_eq!(&after[..100], before.as_slice(), "existing particles keep their positions");
    for p in &after[100..] {
        assert!(
            p.x >= domain.min.x && p.x <= domain.max.x
                && p.y >= domain.min.y && p.y <= domain.max.y
                && p.z >= domain.min.z && p.z <= domain.max.z,
            "the new particles spawn inside the domain",
        );
    }
}

#[test]
fn resize_to_the_same_count_is_a_no_op() {
    let domain = Domain::default();
    let mut cloud = Particles::default();
    spawn(&mut cloud, &domain, 37, 3);
    let before = positions(&cloud);

    resize(&mut cloud, &domain, 37, 3);

    assert_eq!(positions(&cloud), before, "nothing is trimmed or spawned");
}

#[test]
fn resize_caps_at_the_particle_budget() {
    let domain = Domain::default();
    let mut cloud = Particles::default();
    spawn(&mut cloud, &domain, 10, 3);

    resize(&mut cloud, &domain, MAX_PARTICLES + 500, 3);

    assert_eq!(count(&cloud), MAX_PARTICLES, "the instance budget still bounds the cloud");
}

#[test]
fn dot_instances_write_the_scaled_state_radius_into_dir_len_w() {
    let mut state = State::new(true);
    assert!(state.set_particle_size(2.0));
    let field = FieldExpr::parse("(1, 0)").expect("pump field parses");
    let domain = Domain::default();
    let mut cloud = Particles::default();
    spawn(&mut cloud, &domain, 16, 3);

    let radius = state.particle_radius(state.spacing());
    let dots = dot_instances(&cloud, &field, &domain, Colormap::Viridis, (0.0, 1.0), radius);

    assert_eq!(dots.len(), 16);
    for d in &dots {
        assert_eq!(d.dir_len[3], radius, "the dot shader reads the radius from dir_len.w");
    }
    assert!(
        radius > DOT_SPACING_FRACTION * state.spacing(),
        "size 2 doubles the radius over the base fraction",
    );
}

#[test]
fn retarget_applies_a_changed_target_but_preserves_a_click_burst_above_it() {
    let domain = Domain::default();
    let mut cloud = Particles::default();
    spawn(&mut cloud, &domain, 128, 3);
    let added = spawn_at(&mut cloud, &domain, Vec3::ZERO, CLICK_BURST_PARTICLES, 3);
    assert_eq!(added, CLICK_BURST_PARTICLES);
    let burst = positions(&cloud);
    assert_eq!(burst.len(), 128 + CLICK_BURST_PARTICLES);

    let kept = retarget(&mut cloud, &domain, Some(128), Some(128), 3);
    assert_eq!(kept, Some(128), "a stable target is still the target in force");
    assert_eq!(positions(&cloud), burst, "a click burst above the count survives a stable frame");

    let applied = retarget(&mut cloud, &domain, Some(128), Some(100), 3);
    assert_eq!(applied, Some(100));
    assert_eq!(count(&cloud), 100, "a changed, lower target trims the cloud");
}

#[test]
fn retarget_spawns_the_difference_when_the_target_rises() {
    let domain = Domain::default();
    let mut cloud = Particles::default();
    spawn(&mut cloud, &domain, 100, 3);
    let before = positions(&cloud);

    let applied = retarget(&mut cloud, &domain, Some(100), Some(180), 3);

    assert_eq!(applied, Some(180));
    assert_eq!(count(&cloud), 180);
    assert_eq!(&positions(&cloud)[..100], before.as_slice(), "existing particles stay put");
}

#[test]
fn retarget_spawns_from_an_empty_cloud_on_the_first_target() {
    let domain = Domain::default();
    let mut cloud = Particles::default();

    let applied = retarget(&mut cloud, &domain, None, Some(64), 3);

    assert_eq!(applied, Some(64), "launching with particles on applies the first target");
    assert_eq!(count(&cloud), 64);
}

#[test]
fn retarget_off_spawns_nothing_and_leaves_the_cloud_for_the_clear_branch() {
    let domain = Domain::default();
    let mut cloud = Particles::default();
    spawn(&mut cloud, &domain, 64, 3);
    let before = positions(&cloud);

    let applied = retarget(&mut cloud, &domain, Some(64), None, 3);

    assert_eq!(applied, None, "off has no target");
    assert_eq!(positions(&cloud), before, "particles off spawns nothing");
}
