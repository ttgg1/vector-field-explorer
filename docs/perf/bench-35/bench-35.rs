//! Ticket 35 temporary CPU benchmark harness. Not committed.
//!
//! Run: cargo run --release --example bench35
//!
//! Std-only. Times the public particle/sampling seams at realistic v0.1.0
//! counts with `Instant`, reports medians of repeated runs, and counts heap
//! allocations with a counting global allocator.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use glam::{UVec3, Vec3};
use vector_field_explorer::colormap::Colormap;
use vector_field_explorer::domain::Domain;
use vector_field_explorer::field::FieldExpr;
use vector_field_explorer::particles::{
    self, AdvectSettings, ParticleAppearance, ParticleColorMode, Particles,
};
use vector_field_explorer::renderer::{
    ArrowInstanceGPU, combined_instances, sort_scene_back_to_front, sorted_lines_back_to_front,
};
use vector_field_explorer::sampling::{ViewOptions, sample_field};

struct Counting;
static ALLOCS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn alloc_reset() {
    ALLOCS.store(0, Ordering::Relaxed);
    BYTES.store(0, Ordering::Relaxed);
}
fn alloc_read() -> (u64, u64) {
    (ALLOCS.load(Ordering::Relaxed), BYTES.load(Ordering::Relaxed))
}

fn median_ns(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mid = samples.len() / 2;
    if samples.len() % 2 == 0 {
        (samples[mid - 1] + samples[mid]) / 2.0
    } else {
        samples[mid]
    }
}

fn pump(count: usize, trail: u32) -> (Particles, ParticleAppearance) {
    let domain = Domain::default();
    let mut p = Particles::default();
    particles::spawn(&mut p, &domain, count, 3);
    let appearance = ParticleAppearance {
        trail_len: trail,
        ..ParticleAppearance::default()
    };
    // Fill the trail ring to its steady length so the timed calls measure the
    // steady-state path, not the first frames.
    let field = FieldExpr::parse("(x, y, z)").unwrap();
    for _ in 0..40 {
        particles::advect(
            &mut p,
            &field,
            &domain,
            AdvectSettings {
                dt: 1.0 / 60.0,
                trail_len: trail,
                loop_on_stagnation: true,
                stagnation_seconds: 0.5,
            },
        );
    }
    (p, appearance)
}

fn time_call<F: FnMut()>(rounds: usize, mut f: F) -> (f64, (u64, u64)) {
    let mut samples = Vec::with_capacity(rounds);
    let mut alloc = (0, 0);
    for _ in 0..rounds {
        alloc_reset();
        let t = Instant::now();
        f();
        let ns = t.elapsed().as_nanos() as f64;
        let used = alloc_read();
        samples.push(ns);
        alloc = used;
    }
    (median_ns(samples), alloc)
}

/// Split a `(u, v[, w])` formula the way `FieldExpr::parse` does, so this
/// harness can bind each component once and price the current re-bind.
fn split_components(src: &str) -> Vec<String> {
    let body = src.trim();
    let body = if body.starts_with('(') && body.ends_with(')') {
        &body[1..body.len() - 1]
    } else {
        body
    };
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (i, ch) in body.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(body[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(body[start..].trim().to_string());
    parts
}

fn main() {
    let field3 = FieldExpr::parse("(x, y, z)").unwrap();
    let field_exp = FieldExpr::parse(
        "(3*x*z/(x^2+y^2+z^2+0.05)^2.5, 3*y*z/(x^2+y^2+z^2+0.05)^2.5, \
         (3*z^2-(x^2+y^2+z^2))/(x^2+y^2+z^2+0.05)^2.5)",
    )
    .unwrap();
    let domain = Domain::default();
    let rounds = 21;

    println!("== eval3 alone (ns/call) ==");
    for (name, field) in [("radial", &field3), ("magnetic_dipole", &field_exp)] {
        let points: Vec<Vec3> = (0..4000)
            .map(|i| {
                let f = i as f32 / 4000.0;
                Vec3::new(-1.0 + 2.0 * f, 0.5 * f - 0.25, 0.25 * f)
            })
            .collect();
        let (ns, _) = time_call(rounds, || {
            let mut acc = 0.0f32;
            for p in &points {
                acc += field.eval3(*p).map_or(0.0, |v| v.length());
            }
            std::hint::black_box(acc);
        });
        println!("{name},{:.1},{:.3}", ns / 4000.0, ns / 1e6);
    }

    println!("== bind-once comparison on the 3D radial field (ns per eval3) ==");
    {
        let parts = split_components("(x, y, z)");
        let bound: Vec<Box<dyn Fn(f64, f64, f64) -> f64>> = parts
            .iter()
            .map(|part| {
                let expr: meval::Expr = part.parse().unwrap();
                Box::new(expr.bind3("x", "y", "z").unwrap())
                    as Box<dyn Fn(f64, f64, f64) -> f64>
            })
            .collect();
        let points: Vec<Vec3> = (0..4000)
            .map(|i| {
                let f = i as f32 / 4000.0;
                Vec3::new(-1.0 + 2.0 * f, 0.5 * f - 0.25, 0.25 * f)
            })
            .collect();
        let (ns, alloc) = time_call(rounds, || {
            let mut acc = 0.0f64;
            for p in &points {
                let (x, y, z) = (p.x as f64, p.y as f64, p.z as f64);
                acc += bound[0](x, y, z) + bound[1](x, y, z) + bound[2](x, y, z);
            }
            std::hint::black_box(acc);
        });
        println!("bound_once,{:.1},{:.3},allocs,{},bytes,{}", ns / 4000.0, ns / 1e6, alloc.0, alloc.1);

        let manual = |x: f64, y: f64, z: f64| (x, y, z);
        let (ns, _) = time_call(rounds, || {
            let mut acc = 0.0f64;
            for p in &points {
                let (x, y, z) = (p.x as f64, p.y as f64, p.z as f64);
                let (u, v, w) = manual(x, y, z);
                acc += u + v + w;
            }
            std::hint::black_box(acc);
        });
        println!("hand_closure,{:.1},{:.3}", ns / 4000.0, ns / 1e6);
    }

    println!("== advect (ns/particle, total ms, allocs, alloc_bytes) ==");
    for n in [500, 2000, 4000] {
        for trail in [0, 8, 32] {
            let (mut cloud, _appearance) = pump(n, trail);
            let settings = AdvectSettings {
                dt: 1.0 / 60.0,
                trail_len: trail,
                loop_on_stagnation: true,
                stagnation_seconds: 0.5,
            };
            let (ns, alloc) = time_call(rounds, || {
                particles::advect(&mut cloud, &field3, &domain, settings);
            });
            println!(
                "{n},{trail},{:.1},{:.3},{},{}",
                ns / n as f64,
                ns / 1e6,
                alloc.0,
                alloc.1
            );
        }
    }

    println!("== advect magnetic_dipole (ns/particle, total ms) ==");
    for n in [500, 2000, 4000] {
        let (mut cloud, _) = pump(n, 0);
        let settings = AdvectSettings {
            dt: 1.0 / 60.0,
            trail_len: 0,
            loop_on_stagnation: true,
            stagnation_seconds: 0.5,
        };
        let (ns, _) = time_call(rounds, || {
            particles::advect(&mut cloud, &field_exp, &domain, settings);
        });
        println!("{n},{:.1},{:.3}", ns / n as f64, ns / 1e6);
    }

    println!("== dot_instances magnitude (ns/particle, total ms, allocs, bytes) ==");
    for n in [500, 2000, 4000] {
        let (cloud, appearance) = pump(n, 0);
        let speeds = particles::speed_range(&cloud);
        let (ns, alloc) = time_call(rounds, || {
            let dots = particles::dot_instances(
                &cloud,
                &field3,
                &domain,
                Colormap::Viridis,
                (0.0, 2.0),
                &appearance,
                speeds,
                0.01,
            );
            std::hint::black_box(dots);
        });
        println!(
            "{n},{:.1},{:.3},{},{}",
            ns / n as f64,
            ns / 1e6,
            alloc.0,
            alloc.1
        );
    }

    println!("== dot_instances speed (ns/particle, total ms) ==");
    for n in [500, 2000, 4000] {
        let (cloud, mut appearance) = pump(n, 0);
        appearance.color_mode = ParticleColorMode::Speed;
        let speeds = particles::speed_range(&cloud);
        let (ns, _) = time_call(rounds, || {
            let dots = particles::dot_instances(
                &cloud,
                &field3,
                &domain,
                Colormap::Viridis,
                (0.0, 2.0),
                &appearance,
                speeds,
                0.01,
            );
            std::hint::black_box(dots);
        });
        println!("{n},{:.1},{:.3}", ns / n as f64, ns / 1e6);
    }

    println!("== trail_instances (ns/particle, total ms, segments, allocs, bytes) ==");
    for n in [500, 2000, 4000] {
        for trail in [8, 32] {
            let (mut cloud, mut appearance) = pump(n, trail);
            appearance.trail_len = trail;
            let speeds = particles::speed_range(&cloud);
            let mut segments = 0usize;
            let (ns, alloc) = time_call(rounds, || {
                let trails = particles::trail_instances(
                    &cloud,
                    &field3,
                    Colormap::Viridis,
                    (0.0, 2.0),
                    &appearance,
                    speeds,
                    0.1,
                );
                segments = trails.len();
                std::hint::black_box(trails);
            });
            println!(
                "{n},{trail},{:.1},{:.3},{segments},{},{}",
                ns / n as f64,
                ns / 1e6,
                alloc.0,
                alloc.1
            );
            // Keep the cloud alive for the next round.
            particles::advect(
                &mut cloud,
                &field3,
                &domain,
                AdvectSettings {
                    dt: 1.0 / 60.0,
                    trail_len: trail,
                    ..Default::default()
                },
            );
        }
    }

    println!("== sample_field (ns/sample, total ms, samples) ==");
    for (label, grid) in [
        ("2d_default_24x24", UVec3::new(24, 24, 1)),
        ("3d_default_16^3", UVec3::splat(16)),
        ("3d_max_40^3", UVec3::splat(40)),
        ("2d_max_200x200", UVec3::new(200, 200, 1)),
    ] {
        let view = ViewOptions::default();
        let (ns, alloc) = time_call(rounds, || {
            let arrows = sample_field(&field3, &domain, grid, &view);
            std::hint::black_box(arrows);
        });
        let count = grid.x as f64 * grid.y as f64 * grid.z as f64;
        println!(
            "{label},{:.1},{:.3},{count},{},{}",
            ns / count,
            ns / 1e6,
            alloc.0,
            alloc.1
        );
    }

    println!("== frame CPU pipeline worst case (4000 particles, trail 32, 4096 arrows) ==");
    let (mut cloud, mut appearance) = pump(4000, 32);
    appearance.trail_len = 32;
    // A synthetic 3D arrow set at the 16^3 default sampling.
    let arrows: Vec<ArrowInstanceGPU> = (0..4096)
        .map(|i| ArrowInstanceGPU {
            origin: [i as f32 * 0.001, 0.0, 0.0, 1.0],
            dir_len: [0.0, 0.0, 1.0, 0.2],
            color: [0.2, 0.5, 0.8, 1.0],
        })
        .collect();
    let view = glam::Mat4::look_at_rh(Vec3::new(0.0, 0.0, 4.0), Vec3::ZERO, Vec3::Y);
    let (ns, alloc) = time_call(rounds, || {
        particles::advect(
            &mut cloud,
            &field3,
            &domain,
            AdvectSettings {
                dt: 1.0 / 60.0,
                trail_len: 32,
                loop_on_stagnation: true,
                stagnation_seconds: 0.5,
            },
        );
        let speeds = particles::speed_range(&cloud);
        let dots = particles::dot_instances(
            &cloud,
            &field3,
            &domain,
            Colormap::Viridis,
            (0.0, 2.0),
            &appearance,
            speeds,
            0.01,
        );
        let trails = particles::trail_instances(
            &cloud,
            &field3,
            Colormap::Viridis,
            (0.0, 2.0),
            &appearance,
            speeds,
        0.1,
        );
        let mut combined = combined_instances(&arrows, &dots);
        sort_scene_back_to_front(&mut combined, &view);
        let sorted_trails = sorted_lines_back_to_front(&trails, &view);
        std::hint::black_box((combined, sorted_trails));
    });
    println!(
        "worst_case_total,{:.3},allocs,{},bytes,{}",
        ns / 1e6,
        alloc.0,
        alloc.1
    );
}
