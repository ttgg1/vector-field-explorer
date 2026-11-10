//! One global simulation clock: the only time source for animated features.
//!
//! Particles are the first v0.1.0 consumer (ticket 26); time-dependent fields
//! and plots will read the same clock later. The clock is transient — it is
//! never persisted — so a fresh session and `Reset all controls` both start at
//! time 0, playing, at 1x.
//!
//! The frame step is clamped to [`MAX_STEP`] so a long idle gap or a window
//! drag cannot make the cloud jump, and a paused clock produces a zero step
//! rather than accumulating the missed time. `advance` exposes the
//! speed-scaled step through [`SimClock::step_dt`] for consumers.

use crate::viewmodel::State;

/// Longest simulated step one frame may take, seconds (1/20 s). A frame slower
/// than 20 fps advances only this far, so a stall cannot teleport particles.
pub const MAX_STEP: f32 = 1.0 / 20.0;

/// Slowest and fastest transport speed, a multiple of real time.
pub const MIN_SPEED: f32 = 0.1;
pub const MAX_SPEED: f32 = 3.0;
/// Speed a fresh clock and `Reset all controls` use.
pub const DEFAULT_SPEED: f32 = 1.0;

/// The transport's state: elapsed simulated time, whether it runs, and its
/// time multiplier. The per-frame [`step_dt`](SimClock::step_dt) is private so
/// consumers cannot advance the cloud with a step the clock did not produce.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SimClock {
    /// Simulated seconds elapsed, speed-scaled.
    pub time: f32,
    /// Whether time advances on `advance`.
    pub playing: bool,
    /// Time multiplier in [`MIN_SPEED`]..=[`MAX_SPEED`].
    pub speed: f32,
    /// Optional loop period: when `Some`, `advance` wraps `time` into
    /// `[0, period)` with a floor-modulo. `None` means no wrapping.
    pub loop_period: Option<f32>,
    step: f32,
}

impl Default for SimClock {
    fn default() -> SimClock {
        SimClock {
            time: 0.0,
            playing: true,
            speed: DEFAULT_SPEED,
            loop_period: None,
            step: 0.0,
        }
    }
}

impl SimClock {
    /// Advance one frame from the raw frame time `dt`, seconds.
    ///
    /// Non-finite, negative and zero values are ignored; a huge `dt` clamps to
    /// [`MAX_STEP`]. The simulated step is the clamped frame time times the
    /// speed, and time advances by exactly that step. Paused clocks keep their
    /// time and report a zero step, so resuming never replays missed frames.
    pub fn advance(&mut self, dt: f32) {
        self.step = if !self.playing || !dt.is_finite() || dt <= 0.0 {
            0.0
        } else {
            dt.min(MAX_STEP) * sanitized_speed(self.speed)
        };
        self.time += self.step;
        // Wrap into [0, period) with a floor-modulo when a period is set.
        if let Some(period) = self.loop_period
            && period.is_finite()
            && period > 0.0
        {
            self.time = self.time - period * (self.time / period).floor();
        }
    }

    /// Advance one frame only while [`should_animate`] holds. With no clock
    /// consumer (particles off or the 2D view) the clock *holds*: time stays
    /// where it was and the step is cleared, so re-enabling the consumer
    /// resumes from the held time rather than replaying or accumulating the
    /// hidden frames. Paused frames hold the same way.
    pub fn advance_for(&mut self, state: &State, dt: f32) {
        if should_animate(state, self) {
            self.advance(dt);
        } else {
            self.step = 0.0;
        }
    }

    /// The speed-scaled step this frame's `advance` produced, seconds. Zero
    /// while paused or after a rejected frame time.
    pub fn step_dt(&self) -> f32 {
        self.step
    }

    /// Start or pause playback. Any change clears the last step so the next
    /// consumer frame advances exactly once from the new state.
    pub fn set_playing(&mut self, playing: bool) {
        self.playing = playing;
        self.step = 0.0;
    }

    pub fn toggle_playing(&mut self) {
        self.set_playing(!self.playing);
    }

    /// Clamp the transport speed into [`MIN_SPEED`]..=[`MAX_SPEED`]; a
    /// non-finite value falls back to 1x.
    pub fn set_speed(&mut self, speed: f32) {
        self.speed = sanitized_speed(speed);
    }

    /// Set the loop period. A non-finite or non-positive value clears the loop
    /// (sets it to `None`).
    pub fn set_loop_period(&mut self, period: f32) {
        self.loop_period = if period.is_finite() && period > 0.0 {
            Some(period)
        } else {
            None
        };
    }

    /// Return time to zero without touching playback or speed. The particle
    /// cloud is untouched; the transport's reset-time button calls this.
    pub fn reset_time(&mut self) {
        self.time = 0.0;
        self.step = 0.0;
    }

    /// Restore a fresh clock: time 0, playing, 1x. `Reset all controls` uses
    /// this, matching a fresh session.
    pub fn reset(&mut self) {
        *self = SimClock::default();
    }
}

fn sanitized_speed(speed: f32) -> f32 {
    if speed.is_finite() {
        speed.clamp(MIN_SPEED, MAX_SPEED)
    } else {
        DEFAULT_SPEED
    }
}

/// Whether the current view has a v0.1.0 clock consumer: the particle cloud
/// exists only in the 3D view. Ticket 39 extends this with time-dependent
/// fields and ticket 38 with plots.
pub fn has_clock_consumer(state: &State) -> bool {
    (state.particles && state.mode_3d) || state.field.is_time_dependent()
}

/// Whether the transport strip should be shown. Today that is exactly
/// [`has_clock_consumer`]; a static field with particles off keeps its chrome
/// minimal.
pub fn transport_visible(state: &State) -> bool {
    has_clock_consumer(state)
}

/// Whether this frame should keep the repaint loop alive: playing *and* at
/// least one consumer exists. Paused or consumer-free frames stop the loop.
pub fn should_animate(state: &State, clock: &SimClock) -> bool {
    clock.playing && has_clock_consumer(state)
}
