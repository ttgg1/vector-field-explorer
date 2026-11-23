//! Central motion vocabulary for restrained UI motion (issue #289, bounded slice).
//!
//! One small, pure module: bounded duration tokens, a deterministic clamped
//! ease-out sampler, and reduced-motion resolution. Nothing here touches egui,
//! the scene or the renderer, so every branch is directly unit-testable. No
//! ad-hoc timing values belong in callers — they all read [`MotionTokens`].

/// Bounded durations (milliseconds) and the single easing curve shared by every
/// transition. Values are constants in disguise: callers use [`Default`] rather
/// than inventing their own timings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionTokens {
    /// Open/slide transition for the compact sheet and floating panels.
    pub open_slide_ms: f32,
    /// Hover/press feedback for buttons and toggles.
    pub hover_press_ms: f32,
}

impl Default for MotionTokens {
    fn default() -> Self {
        Self {
            open_slide_ms: 140.0,
            hover_press_ms: 80.0,
        }
    }
}

/// Resolve a transition's normalized progress (0..=1) at `elapsed_ms` using one
/// quadratic ease-out. Pure and deterministic.
///
/// Non-finite or non-positive durations and non-finite elapsed values settle
/// immediately rather than producing `NaN`: a corrupt or degenerate timing must
/// never leak an invalid offset into layout. Negative elapsed clamps to 0.
pub fn progress(elapsed_ms: f32, duration_ms: f32) -> f32 {
    if !duration_ms.is_finite() || duration_ms <= 0.0 || !elapsed_ms.is_finite() {
        // A non-finite elapsed means "no usable clock sample": treat it as the
        // immediate end state so a transition never sticks part-way.
        return if !elapsed_ms.is_finite() { 0.0 } else { 1.0 };
    }
    let t = elapsed_ms.clamp(0.0, duration_ms) / duration_ms;
    1.0 - (1.0 - t) * (1.0 - t)
}

/// How a transition behaves this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionMode {
    /// Animate over the token duration.
    Full,
    /// Jump straight to the end state on the first frame.
    Reduced,
}

/// Resolve the effective mode from the persisted user setting and the platform
/// reduced-motion preference. Either signal disables motion; the platform
/// preference is an additional automatic gate, not an override of the setting.
pub fn effective_mode(setting_reduced: bool, platform_reduced: bool) -> MotionMode {
    if setting_reduced || platform_reduced {
        MotionMode::Reduced
    } else {
        MotionMode::Full
    }
}
