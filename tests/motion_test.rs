//! Pure motion tokens and reduced-motion resolution (issue #289).
//!
//! `progress` is the one clamped ease-out sampler every transition shares.
//! `effective_mode` resolves the persisted setting against the platform signal.

use vector_field_explorer::ui::motion::{
    MotionMode, MotionTokens, MotionTransition, effective_mode, needs_repaint, progress,
};

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
