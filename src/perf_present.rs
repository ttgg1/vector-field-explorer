//! Bounded native present-path measurement for the documented frame floors
//! (#18 task b).
//!
//! The accepted floors in `docs/perf/particle-performance.md` describe
//! *presented* rates, so the offscreen proxy in `tests/perf_baseline_test.rs`
//! cannot prove them. This module holds the native-only env configuration, the
//! pure frame-interval aggregator and the named-scene seam;
//! `src/app/present.rs` drives them from the real `eframe` present loop. A
//! normal run never reads any of this: `main` opts in only when
//! `VFE_PERF_PRESENT=1`, and [`parse_config`] returns `None` for every other
//! value.

use std::path::PathBuf;

use crate::viewmodel::State;

/// Logical inner size forced for a present run (the documented floor size).
pub const INNER_SIZE: [u32; 2] = [1280, 800];

/// Seconds discarded before aggregation.
pub const WARMUP_SECONDS: f64 = 2.0;

/// Default measurement window after the warm-up.
pub const DEFAULT_SECONDS: f64 = 10.0;

/// Shortest accepted measurement window.
pub const MIN_SECONDS: f64 = 5.0;

/// Longest accepted measurement window.
pub const MAX_SECONDS: f64 = 60.0;

/// Slack after warm-up + measurement window before the wall-clock safety
/// deadline gives up and closes the window.
pub const DEADLINE_SLACK_SECONDS: f64 = 5.0;

/// The wall-clock limit for a run of `duration_seconds`: warm-up + window +
/// [`DEADLINE_SLACK_SECONDS`].
pub fn deadline_seconds(duration_seconds: f64) -> f64 {
    WARMUP_SECONDS + duration_seconds + DEADLINE_SLACK_SECONDS
}

/// The named scenes the harness can apply through existing state seams.
pub const SUPPORTED_SCENES: [&str; 2] = ["default-2d", "densest-3d"];

/// A validated present-mode request.
///
/// Build these through [`parse_config`]/[`PresentConfig::from_env`] or
/// [`PresentConfig::default`]; [`apply_scene`] only accepts
/// [`SUPPORTED_SCENES`].
#[derive(Debug, Clone, PartialEq)]
pub struct PresentConfig {
    /// Scene name, one of [`SUPPORTED_SCENES`].
    pub scene: String,
    /// Measurement window in seconds, already clamped to
    /// [`MIN_SECONDS`]..=[`MAX_SECONDS`].
    pub seconds: f64,
    /// Optional file that receives exactly the two `PERF present` lines.
    pub out: Option<PathBuf>,
}

impl Default for PresentConfig {
    fn default() -> Self {
        Self {
            scene: "default-2d".to_string(),
            seconds: DEFAULT_SECONDS,
            out: None,
        }
    }
}

impl PresentConfig {
    /// Read the process environment. `Ok(None)` means the mode is off.
    pub fn from_env() -> Result<Option<Self>, String> {
        parse_config(|key| std::env::var(key).ok().filter(|value| !value.is_empty()))
    }
}

/// Parse the configuration from an injected variable lookup.
///
/// Only the exact value `1` enables the mode, so `VFE_PERF_PRESENT=0` (or any
/// other value) stays inert and no default behaviour changes. Invalid seconds
/// or an unknown scene are hard errors rather than silent fallbacks: a
/// benchmark that measured something other than the requested scene would be
/// worse than no record.
pub fn parse_config(get: impl Fn(&str) -> Option<String>) -> Result<Option<PresentConfig>, String> {
    if get("VFE_PERF_PRESENT").as_deref() != Some("1") {
        return Ok(None);
    }

    let scene = get("VFE_PERF_SCENE").unwrap_or_else(|| "default-2d".to_string());
    if !SUPPORTED_SCENES.contains(&scene.as_str()) {
        return Err(format!(
            "unknown VFE_PERF_SCENE {scene:?}; supported scenes: {}",
            SUPPORTED_SCENES.join(", ")
        ));
    }

    let seconds = match get("VFE_PERF_SECONDS") {
        Some(raw) => raw
            .parse::<f64>()
            .map_err(|error| format!("invalid VFE_PERF_SECONDS {raw:?}: {error}"))?,
        None => DEFAULT_SECONDS,
    };
    if !seconds.is_finite() {
        return Err("VFE_PERF_SECONDS must be finite".to_string());
    }

    Ok(Some(PresentConfig {
        scene,
        seconds: seconds.clamp(MIN_SECONDS, MAX_SECONDS),
        out: get("VFE_PERF_OUT")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from),
    }))
}

/// Apply one named scene through the shipped state constructors/setters.
///
/// `default-2d` is the ordinary 2D default (density 24); `densest-3d` is the
/// shipped 3D default grid (density 16, 4096 arrows) used by the performance
/// record. No solver or scene schema is involved.
pub fn apply_scene(state: &mut State, scene: &str) -> Result<(), String> {
    match scene {
        "default-2d" => {
            *state = State::new(false);
            Ok(())
        }
        "densest-3d" => {
            *state = State::new(true);
            // Deliberately pin the documented "densest shipped preset" grid:
            // the 3D default is 16 (4096 arrows), as recorded in
            // docs/perf/particle-performance.md.
            state.set_density(16);
            Ok(())
        }
        _ => Err(format!(
            "unknown scene {scene:?}; supported scenes: {}",
            SUPPORTED_SCENES.join(", ")
        )),
    }
}

/// Accumulates raw frame-to-frame intervals over a bounded window, discarding
/// the warm-up.
#[derive(Debug, Clone)]
pub struct PresentAggregator {
    warmup_seconds: f64,
    duration_seconds: f64,
    elapsed_seconds: f64,
    samples_ms: Vec<f64>,
}

impl PresentAggregator {
    /// A fresh window; `duration_seconds` is measured after the warm-up.
    pub fn new(warmup_seconds: f64, duration_seconds: f64) -> Self {
        Self {
            warmup_seconds,
            duration_seconds,
            elapsed_seconds: 0.0,
            samples_ms: Vec::new(),
        }
    }

    /// Feed one frame's delta in seconds. Non-finite and non-positive deltas
    /// are ignored. Returns whether the measurement window is complete.
    pub fn push(&mut self, dt_seconds: f64) -> bool {
        if !dt_seconds.is_finite() || dt_seconds <= 0.0 {
            return self.done();
        }
        self.elapsed_seconds += dt_seconds;
        // The sample belongs to the measured window only once the accumulated
        // clock has passed the warm-up. Keeping whole frame deltas makes the
        // p50/p95/p99 values describe real frames, not a truncated one.
        if self.elapsed_seconds > self.warmup_seconds {
            self.samples_ms.push(dt_seconds * 1000.0);
        }
        self.done()
    }

    /// Whether the whole warm-up plus measurement window has elapsed.
    pub fn done(&self) -> bool {
        self.elapsed_seconds >= self.warmup_seconds + self.duration_seconds
    }

    /// The measured statistics, or `None` when no frame was kept.
    pub fn report(&self) -> Option<PresentReport> {
        PresentReport::from_deltas_ms(&self.samples_ms)
    }

    /// Build statistics directly from frame deltas in milliseconds (sorted
    /// internally). Exposed for tests and the reporting path.
    pub fn report_from_deltas_ms(samples: &[f64]) -> Option<PresentReport> {
        PresentReport::from_deltas_ms(samples)
    }
}

/// Aggregated frame-delta statistics for one scene.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PresentReport {
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub fps_p50: f64,
    pub n: usize,
    /// Sum of the kept frame deltas, seconds.
    pub window_seconds: f64,
}

impl PresentReport {
    /// Linear-interpolated percentiles (R-7) over the kept frame deltas.
    fn from_deltas_ms(samples: &[f64]) -> Option<Self> {
        if samples.is_empty() {
            return None;
        }
        let mut sorted = samples.to_vec();
        sorted.sort_by(f64::total_cmp);
        let p50 = percentile(&sorted, 0.50)?;
        let p95 = percentile(&sorted, 0.95)?;
        let p99 = percentile(&sorted, 0.99)?;
        let window_seconds = samples.iter().sum::<f64>() / 1000.0;
        Some(Self {
            p50_ms: p50,
            p95_ms: p95,
            p99_ms: p99,
            fps_p50: if p50 > 0.0 { 1000.0 / p50 } else { 0.0 },
            n: samples.len(),
            window_seconds,
        })
    }

    /// The bounded measurement line printed once per run.
    pub fn measurement_line(&self, scene: &str) -> String {
        format!(
            "PERF present {scene} p50_ms={:.3} p95_ms={:.3} p99_ms={:.3} \
             fps_p50={:.1} n={} window_s={:.1}",
            self.p50_ms, self.p95_ms, self.p99_ms, self.fps_p50, self.n, self.window_seconds
        )
    }
}

/// R-7 linear interpolation; `sorted` must be ascending and non-empty.
fn percentile(sorted: &[f64], percent: f64) -> Option<f64> {
    let last = sorted.len().checked_sub(1)?;
    let rank = percent.clamp(0.0, 1.0) * last as f64;
    let low = rank.floor() as usize;
    let high = rank.ceil() as usize;
    let low_value = sorted[low];
    if low == high {
        return Some(low_value);
    }
    Some(low_value + (sorted[high] - low_value) * (rank - low as f64))
}

/// The bounded host line printed once per run. Tokens contain no spaces so a
/// baseline parser can split them on whitespace.
pub fn host_line(
    host: &str,
    gpu: &str,
    backend: &str,
    width: u32,
    height: u32,
    build: &str,
) -> String {
    format!(
        "PERF present host={} gpu={} backend={} size={width}x{height} build={}",
        token(host),
        token(gpu),
        token(backend),
        token(build)
    )
}

/// Best-effort CPU name for the host line; never fails.
pub fn host_name() -> String {
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
