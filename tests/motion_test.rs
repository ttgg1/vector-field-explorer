//! Pure motion tokens and reduced-motion resolution (issue #289).
//!
//! `progress` is the one clamped ease-out sampler every transition shares.
//! `effective_mode` resolves the persisted setting against the platform signal.
//! The final tests drive the real app frame so a hidden surface cannot keep a
//! mid-flight transition requesting paint forever (AC4).

use eframe::egui::{self, vec2};

use crate::support;

use vector_field_explorer::VfApp;
use vector_field_explorer::source_scene::FieldMode;
use vector_field_explorer::ui::motion::{
    MotionMode, MotionTokens, MotionTransition, effective_mode, needs_repaint, progress,
};

const VIEWPORT: egui::Vec2 = vec2(1280.0, 1000.0);

/// Drive one real app frame with a known frame time so `stable_dt` produces a
/// deterministic transition tick through `advance_transition`. Both `time` and
/// `predicted_dt` are supplied so `stable_dt` is the requested delta whether or
/// not egui believes the previous frame requested an immediate repaint.
fn app_frame(ctx: &egui::Context, app: &mut VfApp, time_s: f64, dt_s: f32) {
    let mut frame = eframe::Frame::_new_kittest();
    let mut out = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, VIEWPORT)),
            time: Some(time_s),
            predicted_dt: dt_s,
            ..Default::default()
        },
        |ui| eframe::App::ui(app, ui, &mut frame),
    );
    out.textures_delta.clear();
}

#[test]
fn progress_samples_the_ease_out_curve_and_clamps() {
    assert_eq!(progress(0.0, 100.0), 0.0);
    let mid = progress(50.0, 100.0);
    assert!(mid > 0.0 && mid < 1.0, "mid progress was {mid}");
    assert_eq!(progress(100.0, 100.0), 1.0);
    assert_eq!(progress(500.0, 100.0), 1.0, "elapsed past the end clamps");
    assert_eq!(progress(-50.0, 100.0), 0.0, "negative elapsed clamps");
}

#[test]
fn progress_handles_zero_and_non_finite_duration_without_nan() {
    assert_eq!(progress(10.0, 0.0), 1.0, "zero duration is immediate");
    assert_eq!(progress(10.0, f32::NAN), 1.0);
    assert_eq!(progress(10.0, f32::INFINITY), 1.0);
    assert_eq!(progress(f32::NAN, 100.0), 0.0);
    assert!(progress(10.0, 100.0).is_finite());
}

#[test]
fn default_tokens_are_bounded_and_ordered() {
    let tokens = MotionTokens::default();
    assert!(
        tokens.open_slide_ms > tokens.hover_press_ms && tokens.hover_press_ms > 0.0,
        "open/slide is longer than hover/press; both are positive"
    );
    assert!(tokens.open_slide_ms <= 500.0 && tokens.hover_press_ms <= 500.0);
    assert!(tokens.slide_distance_pt > 0.0 && tokens.slide_distance_pt <= 24.0);
}

#[test]
fn transition_starts_at_zero_settles_at_one_and_requests_no_idle_repaint() {
    let duration = MotionTokens::default().open_slide_ms;
    let mut transition = MotionTransition::new(duration);
    assert!(!transition.is_active());
    assert!(
        !needs_repaint(&transition),
        "a fresh, idle transition is inert"
    );

    transition.start();
    assert_eq!(transition.tick(0.0, MotionMode::Full), 0.0);
    assert_eq!(transition.progress(), 0.0);

    // Mid-flight: active and requesting repaint.
    let halfway = transition.tick(duration / 2.0, MotionMode::Full);
    assert!(halfway > 0.0 && halfway < 1.0, "mid progress was {halfway}");
    assert!(transition.is_active());
    assert!(needs_repaint(&transition));

    // Reaching the end: no longer active, no further frames requested.
    let end = transition.tick(duration / 2.0 + 10.0, MotionMode::Full);
    assert_eq!(end, 1.0);
    assert_eq!(transition.progress(), 1.0);
    assert!(!transition.is_active());
    assert!(
        !needs_repaint(&transition),
        "settled transitions never request idle frames"
    );

    // Extra ticks past the end stay settled and stay inert.
    assert_eq!(transition.tick(1000.0, MotionMode::Full), 1.0);
    assert!(!needs_repaint(&transition));
}

#[test]
fn transition_is_immediate_under_reduced_mode() {
    let mut transition = MotionTransition::new(MotionTokens::default().open_slide_ms);
    transition.start();
    assert_eq!(transition.tick(0.0, MotionMode::Reduced), 1.0);
    assert_eq!(transition.progress(), 1.0);
    assert!(!transition.is_active());
    assert!(!needs_repaint(&transition));

    // A mode change mid-flight also jumps straight to the end state.
    transition.start();
    transition.tick(10.0, MotionMode::Full);
    assert!(transition.is_active());
    assert_eq!(transition.tick(10.0, MotionMode::Reduced), 1.0);
    assert!(!transition.is_active());
}

#[test]
fn transition_restart_resets_to_zero() {
    let mut transition = MotionTransition::new(100.0);
    transition.start();
    transition.tick(100.0, MotionMode::Full);
    assert_eq!(transition.progress(), 1.0);
    transition.start();
    assert_eq!(transition.progress(), 0.0);
    assert!(!transition.is_active());
}

#[test]
fn effective_mode_is_reduced_when_either_signal_is_set() {
    assert_eq!(effective_mode(false, false), MotionMode::Full);
    assert_eq!(effective_mode(true, false), MotionMode::Reduced);
    assert_eq!(effective_mode(false, true), MotionMode::Reduced);
    assert_eq!(effective_mode(true, true), MotionMode::Reduced);
}

/// B1 regression: hiding a surface mid-flight must settle its transition so a
/// closed panel never keeps requesting repaint (AC4). Drives the real app frame
/// rather than the pure tick-to-settle path, which cannot catch a caller that
/// forgets to settle on the invisible path.
#[test]
fn hiding_typeset_mid_flight_settles_and_stops_repaint() {
    let ctx = support::context();
    let mut app = VfApp::new();
    let half_s = (MotionTokens::default().open_slide_ms / 1000.0 / 2.0) as f64;

    // Rising edge: opening the panel starts its slide.
    app.formula_typeset.open = true;
    app_frame(&ctx, &mut app, half_s, half_s as f32);
    assert!(
        app.typeset_transition.is_active(),
        "the visible panel is mid-flight after a half-duration tick"
    );
    assert!(needs_repaint(&app.typeset_transition));

    // Falling edge: hiding the panel mid-flight must settle the transition.
    app.formula_typeset.open = false;
    app_frame(&ctx, &mut app, half_s * 2.0, half_s as f32);
    assert!(
        !app.typeset_transition.is_active(),
        "a hidden panel's transition must not stay in flight"
    );
    assert!(
        !needs_repaint(&app.typeset_transition),
        "a hidden panel must never request another frame"
    );
}

/// B1 regression for the legend: switching to a non-vector field mode mid-slide
/// must settle the legend transition and stop repaint requests.
#[test]
fn hiding_legend_mid_flight_settles_and_stops_repaint() {
    let ctx = support::context();
    let mut app = VfApp::new();
    let half_s = (MotionTokens::default().open_slide_ms / 1000.0 / 2.0) as f64;

    // The legend is visible by default in vector mode. Drive a half-duration
    // frame so its transition is in flight.
    assert!(app.state.is_vector_mode());
    app_frame(&ctx, &mut app, half_s, half_s as f32);
    assert!(app.legend_transition.is_active());
    assert!(needs_repaint(&app.legend_transition));

    // Switching to ODE hides the legend mid-flight; it must settle.
    assert!(app.state.set_field_mode(FieldMode::Ode));
    app_frame(&ctx, &mut app, half_s * 2.0, half_s as f32);
    assert!(
        !app.legend_transition.is_active(),
        "a hidden legend's transition must not stay in flight"
    );
    assert!(
        !needs_repaint(&app.legend_transition),
        "a hidden legend must never request another frame"
    );
}
