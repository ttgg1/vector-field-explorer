//! Pure motion tokens and reduced-motion resolution (issue #289).
//!
//! `progress` is the one clamped ease-out sampler every transition shares.
//! `effective_mode` resolves the persisted setting against the platform signal.

use vector_field_explorer::ui::motion::{
    MotionMode, MotionTokens, effective_mode, progress,
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
}

#[test]
fn effective_mode_is_reduced_when_either_signal_is_set() {
    assert_eq!(effective_mode(false, false), MotionMode::Full);
    assert_eq!(effective_mode(true, false), MotionMode::Reduced);
    assert_eq!(effective_mode(false, true), MotionMode::Reduced);
    assert_eq!(effective_mode(true, true), MotionMode::Reduced);
}
