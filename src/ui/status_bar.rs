//! Bottom status bar: the formula parse state (or a transient hint) on the
//! left, and a compact live readout of frame timing and scene composition on
//! the right.
//!
//! Counts are snapshot-derived, never maintained separately: arrows, particles
//! and trail segments come from the built scene, while streamlines stay the
//! app's field-geometry segment count (the scene's line collection also holds
//! the decorative backdrop). Width decides how much of the readout is worth
//! drawing through [`visible_stats`]; the pure [`format_status`] and
//! [`StatsMask`] seam is stable for ticket 53's compact layout.

use eframe::egui;

use crate::app::StatusBarState;

/// How long a transient hint stays live, in seconds of frame time.
pub const HINT_LIFETIME: f32 = 2.5;

/// Parse-error colour, matching the old status bar's red.
const PARSE_ERROR_COLOR: egui::Color32 = egui::Color32::from_rgb(230, 70, 70);

/// Which optional readouts fit at a given width. FPS, frame time, arrow count
/// and streamline count are never masked.
///
/// This is the public contract ticket 53's compact status bar consumes:
/// [`visible_stats`] returns a strict subset of the full mask as width shrinks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatsMask {
    pub sim_time: bool,
    pub density: bool,
    pub trails: bool,
    pub particles: bool,
}

impl StatsMask {
    /// Every optional readout.
    pub const FULL: StatsMask = StatsMask {
        sim_time: true,
        density: true,
        trails: true,
        particles: true,
    };
}

/// The readouts that fit in `width` points. Priority (drops first to last):
/// simulation time → density → trail segments → particle count. The first four
/// readouts (FPS, frame ms, arrows, streamlines) are always present.
pub fn visible_stats(width: f32) -> StatsMask {
    StatsMask {
        sim_time: width >= 960.0,
        density: width >= 700.0,
        trails: width >= 520.0,
        particles: width >= 360.0,
    }
}

/// Smoothed frame-rate meter fed from egui's stable frame time.
///
/// The first valid sample seeds the average with `1/dt`; later samples use a
/// 90/10 exponential moving average so the readout does not flicker. Non-finite
/// and non-positive frame times are ignored, leaving the last good value.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FpsMeter {
    ema: f32,
}

impl FpsMeter {
    /// Current smoothed frames per second; zero until the first valid sample.
    pub fn fps(&self) -> f32 {
        self.ema
    }

    /// Current smoothed frame time in milliseconds; zero until the first valid
    /// sample.
    pub fn frame_ms(&self) -> f32 {
        if self.ema > 0.0 { 1000.0 / self.ema } else { 0.0 }
    }

    /// Consume one frame's stable delta time, seconds.
    pub fn update(&mut self, dt: f32) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        let sample = 1.0 / dt;
        if self.ema <= 0.0 {
            self.ema = sample;
        } else {
            self.ema = 0.9 * self.ema + 0.1 * sample;
        }
    }
}

/// Advance a transient hint by `elapsed` seconds of frame time since the hint
/// was set, clearing it once [`HINT_LIFETIME`] has passed.
///
/// Pure: the caller owns the accumulated frame time, because an
/// `Option<String>` has nowhere to store a countdown and no wall clock may
/// enter the status bar. A non-finite or negative elapsed time is ignored.
///
/// Caller contract: set [`StatusBarState::hint`] only on the triggering event,
/// never every frame. The app restarts the accumulated lifetime when the hint
/// text changes, so re-setting the same text each frame would make the message
/// flicker (live 2.5 s, gone, live again) instead of staying put.
pub fn advance_hint(hint: &mut Option<String>, elapsed: f32) {
    if hint.is_none() || !elapsed.is_finite() || elapsed < 0.0 {
        return;
    }
    if elapsed >= HINT_LIFETIME {
        *hint = None;
    }
}

/// Compact SI-style count: `999`, `1.2k`, `12.4k`, `1.2M`.
pub fn compact_count(count: usize) -> String {
    if count >= 1_000_000 {
        format!("{:.1}M", count as f64 / 1_000_000.0)
    } else if count >= 1_000 {
        format!("{:.1}k", count as f64 / 1_000.0)
    } else {
        count.to_string()
    }
}

/// `count` with the right singular/plural noun, using [`compact_count`].
fn count_label(count: usize, singular: &str, plural: &str) -> String {
    let noun = if count == 1 { singular } else { plural };
    format!("{} {noun}", compact_count(count))
}

/// Format the right-hand stats readout for `mask`.
///
/// The full form is
/// `60 fps · 16.7 ms | 1.2k arrows · 240 streamlines · 800 particles · 1.2k
/// trail segs | n³=32 | t 12.4 s`, with `n²` in the 2D view, `t` omitted when
/// the clock is not visible, and a `fly | ` prefix while the fly camera is
/// active.
pub fn format_status(status: &StatusBarState, mask: StatsMask) -> String {
    let mut out = String::new();
    if status.flying {
        out.push_str("fly | ");
    }
    out.push_str(&format!("{:.0} fps · {:.1} ms", status.fps, status.frame_ms));
    out.push_str(" | ");
    out.push_str(&count_label(status.arrow_count, "arrow", "arrows"));
    out.push_str(" · ");
    out.push_str(&count_label(status.streamline_count, "streamline", "streamlines"));
    if mask.particles {
        out.push_str(" · ");
        out.push_str(&count_label(status.particle_count, "particle", "particles"));
    }
    if mask.trails {
        out.push_str(" · ");
        out.push_str(&count_label(status.trail_count, "trail seg", "trail segs"));
    }
    if mask.density {
        let exponent = if status.is_3d { "³" } else { "²" };
        out.push_str(&format!(" | n{exponent}={}", status.density));
    }
    if mask.sim_time
        && let Some(time) = status.sim_time
    {
        out.push_str(&format!(" | t {time:.1} s"));
    }
    out
}

/// Draw the status bar: the parse error wins on the left, otherwise a live
/// transient hint or "ready"; the right side is the stats readout reduced to
/// what fits the current width.
pub fn show(ui: &mut egui::Ui, status: &StatusBarState) {
    // The mask is a window contract (ticket 53 pins the thresholds), so feed it
    // the full egui content width rather than this panel's inner budget: the
    // bottom panel's own margins make `available_width()` ~16 pt narrower and
    // would drop the 960 pt "all readouts" case at a 960 pt window.
    let width = ui.ctx().content_rect().width();
    let mask = visible_stats(width);
    ui.horizontal(|ui| {
        if !status.parse_message.is_empty() {
            ui.colored_label(PARSE_ERROR_COLOR, status.parse_message.as_str());
        } else if let Some(hint) = status.hint.as_deref() {
            ui.weak(hint);
        } else {
            ui.weak("ready");
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.weak(format_status(status, mask));
        });
    });
}
