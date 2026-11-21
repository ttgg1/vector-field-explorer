//! Post-epic performance baseline harness (#18 task a).
//!
//! Run through `scripts/bench-perf-baseline.sh`; a normal `cargo test` returns
//! immediately unless `VFE_PERF_BASELINE=1`, matching the other bench modules.
//! This file only measures shipped public APIs and writes nothing.

use std::hint::black_box;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use eframe::wgpu;
use glam::Vec3;
use vector_field_explorer::arrow_style::ArrowStyle;
use vector_field_explorer::cli::{self, CliFlags, CliOptions, Mode};
use vector_field_explorer::custom_density::{
    DensityFamily, DensitySpec, DensitySupport, SupportKind, TimeLaw,
};
use vector_field_explorer::field_probe::{GaussianSurface, SurfaceKind};
use vector_field_explorer::maxwell::MaxwellScene;
use vector_field_explorer::maxwell_wave::WaveSpec;
use vector_field_explorer::plot::PlotKind;
use vector_field_explorer::presets::presets;
use vector_field_explorer::renderer::sorted_lines_back_to_front;
use vector_field_explorer::scalar::ScalarContext;
use vector_field_explorer::source_presets::{load_source_preset, source_preset};
use vector_field_explorer::source_scene::FieldMode;
use vector_field_explorer::sources::FieldFamily;
use vector_field_explorer::viewmodel::State;

/// Timed CPU iterations after the discarded warm-up.
const CPU_SAMPLES: usize = 30;
/// CPU warm-up iterations, never reported.
const CPU_WARMUP: usize = 5;
/// Timed offscreen GPU frames after the discarded warm-up.
const GPU_SAMPLES: usize = 15;
/// GPU warm-up frames, never reported.
const GPU_WARMUP: usize = 3;

/// Linear-interpolated percentile (R-7): the input must be sorted ascending.
fn percentile(sorted: &[f64], percent: f64) -> Option<f64> {
    let last = sorted.len().checked_sub(1)?;
    let rank = percent.clamp(0.0, 1.0) * last as f64;
    let low = rank.floor() as usize;
    let high = rank.ceil() as usize;
    let low_value = sorted[low];
    if low == high {
        return Some(low_value);
    }
    let fraction = rank - low as f64;
    Some(low_value + (sorted[high] - low_value) * fraction)
}

/// Run the whole baseline only when `enabled`; the clock is never consulted
/// before that gate, so a normal test run allocates and measures nothing.
fn run_baseline_with_gate(enabled: bool, mut now: impl FnMut() -> Instant) -> bool {
    if !enabled {
        return false;
    }

    // Cold-start proxy: a fresh 2D state and its first geometry build. This is
    // not application startup (no window, GPU, font or UI initialisation).
    let cold_started = now();
    let cold = State::new(false);
    black_box(cold.snapshot());
    let cold_ms = cold_started.elapsed().as_secs_f64() * 1000.0;
    println!("PERF cold_start_ms={cold_ms:.3}");

    let mut scenes = build_scenes();
    for scene in &mut scenes {
        let samples = time_iterations(&mut now, CPU_WARMUP, CPU_SAMPLES, || scene.state.snapshot());
        report(scene.name, "snapshot", &samples);

        let samples = time_iterations(&mut now, CPU_WARMUP, CPU_SAMPLES, || scene.state.sample());
        report(scene.name, "sample", &samples);

        match scene.extra {
            Extra::None => {}
            Extra::Scalar => {
                // Advance past the scalar key's 100 ms clock bucket so every
                // timed refresh rebuilds instead of hitting the one-entry
                // cache the warm-up just filled.
                let base = scene.state.time;
                let mut step = 0u32;
                let samples = time_iterations(&mut now, CPU_WARMUP, CPU_SAMPLES, || {
                    step = step.wrapping_add(1);
                    scene.state.time = base + step as f32 * 0.15;
                    black_box(scene.state.refresh_scalar_view(true));
                });
                report(scene.name, "scalar_view", &samples);
            }
            Extra::Density => {
                // The density report cache is recomputed on every refresh.
                let samples = time_iterations(&mut now, CPU_WARMUP, CPU_SAMPLES, || {
                    scene.state.refresh_density_reports();
                });
                report(scene.name, "continuity_report", &samples);
            }
            Extra::Probe => {
                // The cached source sampling is what the Sources snapshot turns
                // into instances; price it separately from the flux report.
                let samples = time_iterations(&mut now, CPU_WARMUP, CPU_SAMPLES, || {
                    scene.state.refresh_source_sampling();
                });
                report(scene.name, "source_sample", &samples);
                // Move the surface a little each iteration so the report cache
                // cannot answer from the previous geometry. Build every
                // replacement outside the clock, so `probe_report` prices the
                // update+refresh path (validation, dirty mark, uncached flux)
                // rather than sphere construction.
                let surfaces: Vec<GaussianSurface> = (1..=CPU_WARMUP + CPU_SAMPLES)
                    .map(|step| sphere(Vec3::new(step as f32 * 0.001, 0.0, 0.0), 0.5, 48))
                    .collect();
                let mut step = 0usize;
                let samples = time_iterations(&mut now, CPU_WARMUP, CPU_SAMPLES, || {
                    let surface = surfaces[step];
                    step += 1;
                    scene
                        .state
                        .update_probe_surface(0, surface)
                        .expect("benchmark probe surface must stay valid");
                    scene.state.refresh_probe_reports(true);
                });
                report(scene.name, "probe_report", &samples);
            }
            Extra::Maxwell => {
                let samples = time_iterations(&mut now, CPU_WARMUP, CPU_SAMPLES, || {
                    scene.state.refresh_maxwell_sampling();
                });
                report(scene.name, "maxwell_sample", &samples);
            }
            Extra::StyleRebuild => {
                let styles = [ArrowStyle::Cone, ArrowStyle::Flat];
                let mut step = 0usize;
                let samples = time_iterations(&mut now, CPU_WARMUP, CPU_SAMPLES, || {
                    step = step.wrapping_add(1);
                    scene.state.set_arrow_style(styles[step % styles.len()]);
                    black_box(scene.state.snapshot())
                });
                report(scene.name, "style_rebuild", &samples);
            }
            Extra::TrailSort => {
                // Issue #245: `snapshot`/`sample` never build or sort trails;
                // the app's per-frame assembly does. Prepare the real scene
                // once outside the clock, then price only the painter-order
                // copy over that scene's real trail list. Scramble it once so
                // the measurement is not flattered by an already-sorted input
                // (the adaptive stable sort would otherwise take its best
                // case; live trails arrive in chronological, effectively
                // arbitrary depth order).
                // The harness options leave the simulation duration at zero,
                // which builds no trail history. Preadvance one second at the
                // shipped fixed step so the measurement sees the scene's real
                // ~96k-segment trail list (4000 particles x 24 segments).
                let mut options = scene.options.clone();
                if options.duration_seconds <= 0.0 {
                    options.duration_seconds = 1.0;
                }
                if options.fixed_step_seconds <= 0.0 {
                    options.fixed_step_seconds = 0.01;
                }
                let frame =
                    cli::output_frame(&options).expect("benchmark output frame must be valid");
                let prepared = cli::prepare_scene_with_state(&options, frame, scene.state.clone())
                    .expect("benchmark worst-particles scene must prepare");
                let view = prepared.frame.view;
                let trails = shuffled(prepared.scene.trails.as_ref());
                assert!(
                    !trails.is_empty(),
                    "trail_sort scene must actually contain trail segments"
                );
                let samples = time_iterations(&mut now, CPU_WARMUP, CPU_SAMPLES, || {
                    black_box(sorted_lines_back_to_front(&trails, &view))
                });
                report(scene.name, "trail_sort", &samples);
            }
        }
    }

    let gpu_requested = std::env::var("VFE_PERF_GPU").as_deref() == Ok("1");
    let (gpu_name, gpu_backend) = if gpu_requested {
        match gpu_adapter_info() {
            Some((name, backend)) => {
                run_gpu_scenes(&scenes, &mut now);
                (name, backend)
            }
            None => {
                println!(
                    "PERF gpu=skipped reason=no_headless_adapter (CPU baseline only, no \
                     offscreen frame number is fabricated)"
                );
                ("none".to_string(), "none".to_string())
            }
        }
    } else {
        ("not_probed".to_string(), "not_probed".to_string())
    };

    let build = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    println!(
        "PERF host={} gpu={} backend={} build={} date={}",
        token(&cpu_name()),
        token(&gpu_name),
        token(&gpu_backend),
        build,
        today_utc()
    );
    true
}

/// Extra per-scene work kept separate from `snapshot`/`sample`.
#[derive(Clone, Copy)]
enum Extra {
    None,
    Scalar,
    Density,
    Probe,
    Maxwell,
    /// Toggle the arrow family between rebuilds: a style-only change that must
    /// reuse the previous sampling pass (issue #240 hit path).
    StyleRebuild,
    /// Time the per-frame trail painter sort (#245); the snapshot path leaves
    /// trails empty, so this prepares the scene once and sorts its real list.
    TrailSort,
}

struct Scene {
    name: &'static str,
    state: State,
    options: CliOptions,
    extra: Extra,
}

fn build_scenes() -> Vec<Scene> {
    let mut scenes = Vec::new();

    let state = State::new(false);
    scenes.push(Scene {
        name: "default-2d",
        options: cli_options(Mode::TwoD, 24, false, 0, 0),
        state,
        extra: Extra::None,
    });

    // Warm-cache hit path: the style toggle does not change the sampling key,
    // so this number is a style-only rebuild, not a cold sampling pass.
    scenes.push(Scene {
        name: "style-rebuild",
        options: cli_options(Mode::TwoD, 24, false, 0, 0),
        state: State::new(false),
        extra: Extra::StyleRebuild,
    });

    let mut state = State::new(true);
    state.set_density(16);
    scenes.push(Scene {
        name: "densest-3d",
        options: cli_options(Mode::ThreeD, 16, false, 0, 0),
        state,
        extra: Extra::None,
    });

    let mut state = State::new(true);
    let magnetic_dipole = presets()
        .iter()
        .find(|preset| preset.name == "Magnetic dipole")
        .expect("the shipped Magnetic dipole preset exists")
        .formula;
    state.set_formula(magnetic_dipole.to_string());
    scenes.push(Scene {
        name: "dipole-3d",
        options: cli_options(Mode::ThreeD, 16, false, 0, 0),
        state,
        extra: Extra::None,
    });

    let mut state = State::new(true);
    state.set_density(40);
    scenes.push(Scene {
        name: "grid-stress",
        options: cli_options(Mode::ThreeD, 40, false, 0, 0),
        state,
        extra: Extra::None,
    });

    let mut state = State::new(true);
    state.set_particles(true);
    state.set_particle_count(4000);
    state.set_particle_appearance(vector_field_explorer::particles::ParticleAppearance {
        trail_len: 32,
        ..Default::default()
    });
    scenes.push(Scene {
        name: "worst-particles",
        options: cli_options(Mode::ThreeD, 16, true, 4000, 32),
        state,
        extra: Extra::TrailSort,
    });

    let mut state = State::new(true);
    state.set_plot_kind(PlotKind::Surface3D);
    state.set_scalar_source(ScalarContext::Surface, "sin(x)*cos(y)".to_string());
    state.set_show_scalar_heatmap(true);
    state
        .set_scalar_grid_side(160)
        .expect("160 is the shipped scalar grid maximum");
    state.refresh_scalar_view(true);
    scenes.push(Scene {
        name: "scalar-heatmap",
        options: cli_options(Mode::ThreeD, 16, false, 0, 0),
        state,
        extra: Extra::Scalar,
    });

    let mut state = State::new(true);
    state
        .commit_densities(vec![harmonic_density()])
        .expect("the benchmark harmonic density is valid");
    state.refresh_density_reports();
    scenes.push(Scene {
        name: "custom-density",
        options: cli_options(Mode::ThreeD, 16, false, 0, 0),
        state,
        extra: Extra::Density,
    });

    let mut state = State::new(true);
    let preset = source_preset("Coulomb point").expect("the shipped Coulomb preset exists");
    assert!(load_source_preset(preset, &mut state));
    state.refresh_source_sampling();
    state
        .add_probe_surface(sphere(Vec3::ZERO, 0.5, 48))
        .expect("the benchmark Gaussian sphere is valid");
    state.set_probe_family(FieldFamily::Electric);
    state.refresh_probe_reports(true);
    scenes.push(Scene {
        name: "field-probe",
        options: cli_options(Mode::ThreeD, 16, false, 0, 0),
        state,
        extra: Extra::Probe,
    });

    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Maxwell);
    state.set_maxwell_scene(MaxwellScene::default());
    assert!(state.set_maxwell_wave(Some(WaveSpec {
        amplitude: 1.0,
        wavelength: 2.0,
        direction: Vec3::X,
        polarization: Vec3::Y,
        phase: 0.0,
    })));
    state.refresh_maxwell_sampling();
    scenes.push(Scene {
        name: "plane-wave",
        options: cli_options(Mode::ThreeD, 16, false, 0, 0),
        state,
        extra: Extra::Maxwell,
    });

    scenes
}

fn cli_options(
    mode: Mode,
    density: u32,
    particles: bool,
    particle_count: u32,
    trail_len: u32,
) -> CliOptions {
    cli::resolve_config(
        None,
        CliFlags {
            output: Some("perf-baseline.png".into()),
            width: Some(640),
            height: Some(400),
            mode: Some(mode),
            density: Some(density),
            particles: Some(particles),
            particle_count: Some(particle_count),
            trail_len: Some(trail_len),
            references_grid_and_box: Some(false),
            references_world_axes: Some(false),
            ..Default::default()
        },
    )
    .expect("the benchmark CLI options are valid")
}

fn sphere(center: Vec3, radius: f32, segments: u32) -> GaussianSurface {
    GaussianSurface {
        kind: SurfaceKind::Sphere,
        center,
        axis: Vec3::Z,
        size: Vec3::splat(2.0 * radius),
        segments,
    }
}

fn harmonic_density() -> DensitySpec {
    DensitySpec {
        name: "harmonic-current".to_string(),
        family: DensityFamily::SteadyCurrent,
        support: DensitySupport {
            kind: SupportKind::Box,
            center: Vec3::ZERO,
            axis: Vec3::Z,
            size: Vec3::splat(2.0),
        },
        // J = (x, 0, 0) cos(t); rho = -sin(t) makes div J + d rho/dt = 0.
        formula: "x,0,0".to_string(),
        samples: 24,
        softening: 0.0,
        time_law: TimeLaw::Harmonic {
            omega: 1.0,
            phase: 0.0,
        },
        charge_formula: Some("-sin(t)".to_string()),
    }
}

/// Deterministic shuffle so the trail-sort measurement does not benefit from
/// an already-sorted list. The sort cost depends on the key multiset, not on
/// this particular order; live trails are effectively arbitrary in depth.
fn shuffled<T: Copy>(items: &[T]) -> Vec<T> {
    let mut out = items.to_vec();
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    for index in (1..out.len()).rev() {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let pick = ((state >> 33) as usize) % (index + 1);
        out.swap(index, pick);
    }
    out
}

fn time_iterations<T>(
    now: &mut impl FnMut() -> Instant,
    warmup: usize,
    samples: usize,
    mut run: impl FnMut() -> T,
) -> Vec<f64> {
    for _ in 0..warmup {
        black_box(run());
    }
    let mut micros = Vec::with_capacity(samples);
    for _ in 0..samples {
        let started = now();
        let value = run();
        black_box(value);
        micros.push(started.elapsed().as_secs_f64() * 1_000_000.0);
    }
    micros
}

fn report(scene: &str, metric: &str, samples: &[f64]) {
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let p50 = percentile(&sorted, 0.50).expect("reported samples are non-empty");
    let p95 = percentile(&sorted, 0.95).expect("reported samples are non-empty");
    let p99 = percentile(&sorted, 0.99).expect("reported samples are non-empty");
    println!(
        "PERF {scene} {metric} p50_us={p50:.3} p95_us={p95:.3} p99_us={p99:.3} n={}",
        sorted.len()
    );
}

fn gpu_adapter_info() -> Option<(String, String)> {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
    descriptor.backends.remove(wgpu::Backends::NOOP);
    let instance = wgpu::Instance::new(descriptor);
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        compatible_surface: None,
        ..Default::default()
    }))
    .ok()?;
    let info = adapter.get_info();
    Some((info.name, format!("{:?}", info.backend)))
}

fn run_gpu_scenes(scenes: &[Scene], now: &mut impl FnMut() -> Instant) {
    for scene in scenes {
        let frame = match cli::output_frame(&scene.options) {
            Ok(frame) => frame,
            Err(error) => {
                println!("PERF {} gpu_frame skipped reason=frame:{error}", scene.name);
                continue;
            }
        };
        let prepared =
            match cli::prepare_scene_with_state(&scene.options, frame, scene.state.clone()) {
                Ok(prepared) => prepared,
                Err(error) => {
                    println!(
                        "PERF {} gpu_frame skipped reason=prepare:{error}",
                        scene.name
                    );
                    continue;
                }
            };
        let mut session = match cli::HeadlessRenderSession::for_scene(&prepared) {
            Ok(session) => session,
            Err(error) => {
                println!(
                    "PERF {} gpu_frame skipped reason=session:{error}",
                    scene.name
                );
                continue;
            }
        };
        match gpu_frames(&mut session, &prepared, now) {
            Ok(samples) => report(scene.name, "gpu_frame", &samples),
            Err(error) => println!(
                "PERF {} gpu_frame skipped reason=render:{error}",
                scene.name
            ),
        }
    }
}

fn gpu_frames(
    session: &mut cli::HeadlessRenderSession,
    prepared: &cli::PreparedScene,
    now: &mut impl FnMut() -> Instant,
) -> Result<Vec<f64>, cli::CliError> {
    let width = prepared.frame.width;
    let height = prepared.frame.height;
    let mut render = || {
        session.render_frame(
            &prepared.scene,
            prepared.objects.as_ref(),
            prepared.object_style,
            prepared.frame.view_proj,
            prepared.frame.eye,
            width,
            height,
            prepared.frame.background_linear,
        )
    };
    for _ in 0..GPU_WARMUP {
        black_box(render()?);
    }
    let mut micros = Vec::with_capacity(GPU_SAMPLES);
    for _ in 0..GPU_SAMPLES {
        let started = now();
        let pixels = render()?;
        black_box(pixels);
        micros.push(started.elapsed().as_secs_f64() * 1_000_000.0);
    }
    Ok(micros)
}

fn cpu_name() -> String {
    if let Ok(text) = std::fs::read_to_string("/proc/cpuinfo") {
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("model name")
                && let Some((_, value)) = rest.split_once(':')
            {
                let value = value.trim();
                if !value.is_empty() {
                    return value.to_string();
                }
            }
        }
    }
    std::env::consts::ARCH.to_string()
}

fn token(value: &str) -> String {
    let token: String = value
        .trim()
        .chars()
        .map(|character| {
            if character.is_whitespace() || character == '=' {
                '_'
            } else {
                character
            }
        })
        .collect();
    if token.is_empty() {
        "unknown".to_string()
    } else {
        token
    }
}

fn today_utc() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let (year, month, day) = civil_from_days((seconds / 86_400) as i64);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Howard Hinnant's `civil_from_days`, with 1970-01-01 as day zero.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_prime + 2) / 5 + 1) as u32;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    } as u32;
    let year = if month <= 2 { year + 1 } else { year };
    (year, month, day)
}

#[test]
fn percentile_helper_handles_odd_even_and_single_lengths() {
    assert_eq!(percentile(&[], 0.5), None);

    let one = [7.5];
    assert_eq!(percentile(&one, 0.0), Some(7.5));
    assert_eq!(percentile(&one, 0.5), Some(7.5));
    assert_eq!(percentile(&one, 0.95), Some(7.5));
    assert_eq!(percentile(&one, 0.99), Some(7.5));
    assert_eq!(percentile(&one, 1.0), Some(7.5));

    let odd = [1.0, 2.0, 3.0];
    assert_eq!(percentile(&odd, 0.5), Some(2.0));
    assert!((percentile(&odd, 0.95).unwrap() - 2.9).abs() < 1e-12);
    assert!((percentile(&odd, 0.99).unwrap() - 2.98).abs() < 1e-12);

    let even = [1.0, 2.0, 3.0, 4.0];
    assert_eq!(percentile(&even, 0.5), Some(2.5));
    assert!((percentile(&even, 0.95).unwrap() - 3.85).abs() < 1e-12);
    assert_eq!(percentile(&even, 0.0), Some(1.0));
    assert_eq!(percentile(&even, 1.0), Some(4.0));

    let hundred: Vec<f64> = (1..=100).map(f64::from).collect();
    let p50 = percentile(&hundred, 0.5).unwrap();
    let p95 = percentile(&hundred, 0.95).unwrap();
    let p99 = percentile(&hundred, 0.99).unwrap();
    assert!((p50 - 50.5).abs() < 1e-12);
    assert!((p95 - 95.05).abs() < 1e-12);
    assert!((p99 - 99.01).abs() < 1e-12);
    assert!(p50 <= p95 && p95 <= p99, "percentiles must be ordered");
}

#[test]
fn gate_unset_returns_before_touching_the_clock() {
    let mut clock_calls = 0;
    let ran = run_baseline_with_gate(false, || {
        clock_calls += 1;
        Instant::now()
    });
    assert!(!ran, "the unset gate must not run the harness");
    assert_eq!(clock_calls, 0, "the unset gate must not read the clock");
}

#[test]
fn perf_baseline() {
    if std::env::var("VFE_PERF_BASELINE").as_deref() != Ok("1") {
        return;
    }
    assert!(run_baseline_with_gate(true, Instant::now));
}
