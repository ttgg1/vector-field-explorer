//! The compact simulation transport strip (ticket 26).
//!
//! Rendered as one row directly under the formula bar, not as a new panel, and
//! only while the view has a clock consumer (particles on in 3D today). The
//! strip owns no state: every control edits [`SimClock`], the app's one time
//! source.
//!
//! The button and time labels take a `compact` flag so ticket 53 can switch to
//! icon-only buttons and a shorter readout without changing this module's
//! shape. v0.1.0 always renders the full row.

use eframe::egui;

use crate::sim_clock::{MAX_SPEED, MIN_SPEED, SimClock, should_animate};
use crate::viewmodel::State;

/// Play/Pause label for the current state. Compact mode is icon-only.
pub fn play_label(playing: bool, compact: bool) -> &'static str {
    match (playing, compact) {
        (true, true) => "⏸",
        (false, true) => "▶",
        (true, false) => "⏸ Pause",
        (false, false) => "▶ Play",
    }
}

/// Reset-time label. Compact mode is icon-only.
pub fn reset_label(compact: bool) -> &'static str {
    if compact { "↺" } else { "↺ Reset time" }
}

/// The simulated time readout, one decimal place: `t = 12.4 s` (or the shorter
/// compact `t = 12.4`). Non-finite or negative times cannot reach the clock,
/// but the formatter still renders a harmless zero for a corrupt mirror.
pub fn time_label(time: f32, compact: bool) -> String {
    let time = if time.is_finite() { time.max(0.0) } else { 0.0 };
    if compact {
        format!("t = {time:.1}")
    } else {
        format!("t = {time:.1} s")
    }
}

/// Draw the strip and apply every edit to `clock`.
pub fn show(ui: &mut egui::Ui, clock: &mut SimClock, compact: bool) {
    ui.horizontal(|ui| {
        if ui
            .button(play_label(clock.playing, compact))
            .on_hover_text("Play or pause the simulation (Space)")
            .clicked()
        {
            clock.toggle_playing();
        }
        if ui
            .button(reset_label(compact))
            .on_hover_text("Reset simulation time to 0; the particle cloud stays")
            .clicked()
        {
            clock.reset_time();
        }
        ui.label(time_label(clock.time, compact))
            .on_hover_text("Simulated time since the last reset");

        // The slider edits a local copy so `set_speed` stays the one clamp.
        // Its range is exactly the clock's contract (0.1x..=3.0x).
        let mut speed = clock.speed;
        let slider = egui::Slider::new(&mut speed, MIN_SPEED..=MAX_SPEED)
            .max_decimals(1)
            .suffix("×");
        let response = if compact {
            ui.add(slider)
        } else {
            ui.add(slider.text("Speed"))
        };
        let response = response.on_hover_text("How fast simulated time runs");
        if response.changed() {
            clock.set_speed(speed);
        }
    });
}

/// Keep the repaint loop alive only while something is animating. A paused or
/// consumer-free view requests nothing, so egui can go idle.
pub fn request_particle_repaint(ctx: &egui::Context, state: &State, clock: &SimClock) {
    if should_animate(state, clock) {
        ctx.request_repaint();
    }
}

/// Consume a Space press as play/pause when the transport is visible and no
/// text field owns the keyboard. Consuming the key also keeps a focused widget
/// from activating it. Returns whether the clock was toggled.
pub fn toggle_on_space(ctx: &egui::Context, state: &State, clock: &mut SimClock) -> bool {
    if !crate::sim_clock::transport_visible(state) || ctx.text_edit_focused() {
        return false;
    }
    let pressed = ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Space));
    if pressed {
        clock.toggle_playing();
    }
    pressed
}
