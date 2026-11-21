//! Native present-path measurement driver (#18 task b).
//!
//! `main` constructs the app with a [`PresentConfig`](crate::perf_present::PresentConfig)
//! only when `VFE_PERF_PRESENT=1`. This module then measures the real
//! wall-clock interval between successive `present_frame` calls, prints the two
//! bounded `PERF present` lines once the window completes, and asks the
//! viewport to close itself. Every other run carries `None` and pays one branch
//! per frame.

use std::io::Write as _;
use std::path::Path;
use std::time::{Duration, Instant};

use super::*;
use crate::perf_present::{
    INNER_SIZE, PresentAggregator, PresentConfig, WARMUP_SECONDS, deadline_seconds, host_line,
    host_name,
};

/// One active present measurement.
pub(super) struct PresentRun {
    config: PresentConfig,
    aggregator: PresentAggregator,
    printed: bool,
    /// Whether the forced logical inner size was requested this run.
    size_requested: bool,
    /// Wall-clock start, used for the independent safety deadline.
    started: Instant,
    /// Previous [`present_frame`](VfApp::present_frame) call. The first call
    /// only seeds it; later calls feed the real elapsed interval.
    last_update: Option<Instant>,
}

impl PresentRun {
    pub(super) fn new(config: PresentConfig) -> Self {
        let aggregator = PresentAggregator::new(WARMUP_SECONDS, config.seconds);
        Self {
            config,
            aggregator,
            printed: false,
            size_requested: false,
            started: Instant::now(),
            last_update: None,
        }
    }

    /// Warm-up + window + slack, independent of frame production.
    fn deadline(&self) -> Instant {
        self.started + Duration::from_secs_f64(deadline_seconds(self.config.seconds))
    }
}

/// Write exactly the given lines to an optional output file.
fn write_out(path: Option<&Path>, lines: &[String]) {
    let Some(path) = path else {
        return;
    };
    match std::fs::File::create(path).and_then(|mut file| {
        for line in lines {
            writeln!(file, "{line}")?;
        }
        Ok(())
    }) {
        Ok(()) => log::info!("present measurement written to {}", path.display()),
        Err(error) => {
            log::warn!(
                "present measurement could not be written to {}: {error}",
                path.display()
            );
        }
    }
}

impl VfApp {
    /// Enforce the wall-clock safety deadline even when eframe runs the hidden
    /// `logic` path and never calls `ui` (for example an occluded window).
    pub(super) fn present_deadline_check(&mut self, ctx: &egui::Context) {
        let Some(present) = self.present.as_mut() else {
            return;
        };
        if present.printed {
            return;
        }
        let now = Instant::now();
        let deadline = present.deadline();
        if now < deadline {
            // A timer keeps the hidden path alive until the deadline.
            ctx.request_repaint_after(deadline.saturating_duration_since(now));
            return;
        }

        let scene = present.config.scene.clone();
        let line = format!("PERF present {scene} error=deadline_exceeded");
        println!("{line}");
        write_out(present.config.out.as_deref(), std::slice::from_ref(&line));
        present.printed = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    /// Feed the next presented frame into the active measurement, if any.
    ///
    /// Called at the end of [`eframe::App::ui`], *before* the current frame is
    /// presented. The sample is therefore the raw wall-clock interval between
    /// successive update calls, which includes the previous frame's present
    /// and swapchain cost; GPU submit cost inside the current frame is only
    /// visible through the following interval. The first call only seeds the
    /// clock.
    pub(super) fn present_frame(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.present_deadline_check(ctx);
        let Some(present) = self.present.as_mut() else {
            return;
        };
        if present.printed {
            return;
        }

        let now = Instant::now();

        // eframe restores the previous window geometry from egui memory even
        // with `persist_window` off, so request the documented logical inner
        // size explicitly on the first measured frame. The resize settles well
        // inside the 2 s warm-up.
        if !present.size_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                INNER_SIZE[0] as f32,
                INNER_SIZE[1] as f32,
            )));
            present.size_requested = true;
        }

        let previous = present.last_update.replace(now);
        if let Some(previous) = previous {
            present
                .aggregator
                .push(now.duration_since(previous).as_secs_f64());
        }
        if !present.aggregator.done() {
            // eframe idles without input; a measurement needs every frame.
            ctx.request_repaint();
            return;
        }

        let scene = present.config.scene.clone();
        let out = present.config.out.clone();
        let Some(report) = present.aggregator.report() else {
            let line = format!("PERF present {scene} error=no_frames");
            println!("{line}");
            write_out(out.as_deref(), std::slice::from_ref(&line));
            present.printed = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        };

        let pixels_per_point = ctx.pixels_per_point();
        let screen = ctx.input(|input| input.viewport_rect()).size();
        let width = (screen.x * pixels_per_point).round().max(0.0) as u32;
        let height = (screen.y * pixels_per_point).round().max(0.0) as u32;
        let (gpu, backend) = frame
            .wgpu_render_state()
            .map(|render_state| {
                let info = render_state.adapter.get_info();
                (info.name, format!("{:?}", info.backend))
            })
            .unwrap_or_else(|| ("no_adapter".to_string(), "none".to_string()));
        let build = if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        };

        let measurement = report.measurement_line(&scene);
        let host = host_line(&host_name(), &gpu, &backend, width, height, build);
        println!("{measurement}");
        println!("{host}");
        write_out(out.as_deref(), &[measurement, host]);
        present.printed = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}
