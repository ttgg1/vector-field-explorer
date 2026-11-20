//! Issue #83 slice B1: the pure colour-mapping model.
//!
//! A [`ColorMapping`] combines a palette (a builtin [`Colormap`] or a custom
//! [`ColorRamp`]), a [`ScaleMode`] and a [`ColorQuantity`] with an optional
//! frozen range. Sampling, legend ticks and clipping counts all derive from the
//! same effective range, so the renderer, legend and export cannot disagree.
//!
//! This module is deliberately pure: it does not touch sampling, arrows,
//! particles, the renderer, export or any UI widget.

use crate::color_ramp::{ColorRamp, ColorStyle, RampKind, ScaleMode};
use crate::colormap::Colormap;

use glam::Vec3;

/// Mid grey used whenever a value cannot be mapped to a colour.
const MID_GRAY: [u8; 3] = [128, 128, 128];

/// What the colour ramp represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum ColorQuantity {
    /// Vector magnitude.
    #[default]
    Magnitude,
    /// Signed X component.
    ComponentX,
    /// Signed Y component.
    ComponentY,
    /// Signed Z component.
    ComponentZ,
    /// Direction in the 2D plane, encoded as an angle.
    Direction2d,
}

impl ColorQuantity {
    /// Every quantity, in display order.
    pub const ALL: [Self; 5] = [
        Self::Magnitude,
        Self::ComponentX,
        Self::ComponentY,
        Self::ComponentZ,
        Self::Direction2d,
    ];

    /// Human-readable name shown in the UI.
    pub fn label(self) -> &'static str {
        match self {
            Self::Magnitude => "Magnitude",
            Self::ComponentX => "X component",
            Self::ComponentY => "Y component",
            Self::ComponentZ => "Z component",
            Self::Direction2d => "Direction (2D)",
        }
    }

    /// Checks that this quantity can be used in the current view.
    ///
    /// Direction colouring has no meaningful 3D encoding, so it is rejected in
    /// the 3D view rather than silently projected.
    pub fn available(self, is_3d: bool) -> Result<(), &'static str> {
        match (self, is_3d) {
            (Self::Direction2d, true) => {
                Err("Direction colouring is available in the 2D plane view only.")
            }
            _ => Ok(()),
        }
    }
}

/// A palette: a builtin colormap or a validated custom ramp.
#[derive(Debug, Clone, PartialEq)]
pub enum ColorPalette {
    /// One of the shipped [`Colormap`] ramps.
    Builtin(Colormap),
    /// A user-editable [`ColorRamp`].
    Custom(ColorRamp),
}

impl ColorPalette {
    /// Validates the palette's own contents.
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Builtin(_) => Ok(()),
            Self::Custom(ramp) => ramp.validate(),
        }
    }

    /// The semantic kind used by the legend.
    pub fn kind(&self) -> RampKind {
        match self {
            Self::Builtin(_) => RampKind::Sequential,
            Self::Custom(ramp) => ramp.kind,
        }
    }

    /// Samples the palette at a normalized position in `0.0..=1.0`.
    pub fn sample_unit(&self, t: f32) -> [u8; 3] {
        match self {
            Self::Builtin(colormap) => colormap.from_unit(t),
            Self::Custom(ramp) => ramp.sample(t).0,
        }
    }
}

/// One field sample as seen by the colour mapper.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorSample {
    /// Vector magnitude.
    pub magnitude: f32,
    /// Unit direction.
    pub dir: Vec3,
}

/// Aggregate bounds over a group of samples.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ColorStats {
    /// Smallest finite magnitude, or `0.0` for an empty/fully non-finite set.
    pub min_magnitude: f32,
    /// Largest finite magnitude, or `0.0` for an empty/fully non-finite set.
    pub max_magnitude: f32,
    /// Largest absolute signed component value.
    pub max_abs_component: f32,
}

impl ColorStats {
    /// Computes the bounds used by auto-fit ranges.
    ///
    /// Non-finite magnitudes and directions are ignored. An empty or fully
    /// non-finite input yields all-zero bounds.
    pub fn from_samples(samples: &[ColorSample]) -> Self {
        let mut min_magnitude = f32::INFINITY;
        let mut max_magnitude = f32::NEG_INFINITY;
        let mut max_abs_component: f32 = 0.0;
        let mut any_magnitude = false;

        for sample in samples {
            if sample.magnitude.is_finite() {
                any_magnitude = true;
                min_magnitude = min_magnitude.min(sample.magnitude);
                max_magnitude = max_magnitude.max(sample.magnitude);
                let dir = sample.dir;
                if dir.x.is_finite() && dir.y.is_finite() && dir.z.is_finite() {
                    let magnitude = sample.magnitude.abs();
                    let component = dir.x.abs().max(dir.y.abs()).max(dir.z.abs()) * magnitude;
                    max_abs_component = max_abs_component.max(component);
                }
            }
        }

        if any_magnitude {
            Self {
                min_magnitude,
                max_magnitude,
                max_abs_component,
            }
        } else {
            Self::default()
        }
    }
}

/// Which side of the effective range a value fell outside.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ClipSide {
    /// Below the effective minimum.
    Low,
    /// Above the effective maximum.
    High,
}

/// A sampled colour plus whether the value was clipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MappedColor {
    /// The mapped sRGB bytes.
    pub rgb: [u8; 3],
    /// The side the value was clipped on, if any.
    pub clipped: Option<ClipSide>,
}

/// The legend description derived from the effective mapping.
#[derive(Debug, Clone, PartialEq)]
pub struct ColorLegend {
    /// Semantic ramp kind.
    pub kind: RampKind,
    /// Effective minimum, or `0.0` for direction colouring.
    pub min: f32,
    /// Effective maximum, or `360.0` for direction colouring.
    pub max: f32,
    /// Legend tick values.
    pub ticks: Vec<f32>,
    /// Whether the ramp wraps.
    pub cyclic: bool,
}

/// Counts of samples below and above the effective range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ClipCounts {
    /// Samples below the minimum.
    pub low: usize,
    /// Samples above the maximum.
    pub high: usize,
}

/// The complete, effective colour mapping.
#[derive(Debug, Clone, PartialEq)]
pub struct ColorMapping {
    /// Builtin or custom palette.
    pub palette: ColorPalette,
    /// Value-to-ramp scaling.
    pub mode: ScaleMode,
    /// What the colour represents.
    pub quantity: ColorQuantity,
    /// A frozen range; `None` auto-fits to [`ColorStats`].
    pub manual_range: Option<(f32, f32)>,
}

impl ColorMapping {
    /// Validates the palette, then the mode, then the frozen range.
    ///
    /// The mapping is never mutated; a rejected value leaves the caller's
    /// copy untouched.
    pub fn validate(&self) -> Result<(), String> {
        self.palette.validate()?;
        self.mode.validate()?;
        if let Some((min, max)) = self.manual_range {
            if !min.is_finite() || !max.is_finite() {
                return Err("manual colour range bounds must be finite".to_owned());
            }
            if min > max {
                return Err("manual colour range min must be at most max".to_owned());
            }
        }
        Ok(())
    }

    /// The range sampling and the legend actually use.
    ///
    /// A manual range always wins. Otherwise magnitude auto-fits to the sample
    /// magnitudes, components auto-fit symmetrically around zero, and direction
    /// uses the unused `0.0..1.0` placeholder.
    pub fn effective_range(&self, stats: &ColorStats) -> (f32, f32) {
        if let Some(range) = self.manual_range {
            return range;
        }
        match self.quantity {
            ColorQuantity::Magnitude => (stats.min_magnitude, stats.max_magnitude),
            ColorQuantity::ComponentX | ColorQuantity::ComponentY | ColorQuantity::ComponentZ => {
                (-stats.max_abs_component, stats.max_abs_component)
            }
            ColorQuantity::Direction2d => (0.0, 1.0),
        }
    }

    /// Maps one sample to a colour.
    ///
    /// Direction uses the wrapped `atan2(y, x)` angle and ignores the range.
    /// A non-finite value or unusable range yields mid grey. A signed log whose
    /// range does not strictly span zero falls back to linear normalization on
    /// the same range; [`Self::explanation`] reports why.
    pub fn sample(&self, sample: &ColorSample, stats: &ColorStats) -> MappedColor {
        if self.quantity == ColorQuantity::Direction2d {
            return self.sample_direction(sample);
        }

        let (min, max) = self.effective_range(stats);
        let value = self.value(sample);
        if !value.is_finite() || !min.is_finite() || !max.is_finite() || min >= max {
            return MappedColor {
                rgb: MID_GRAY,
                clipped: None,
            };
        }

        let clipped = if value < min {
            Some(ClipSide::Low)
        } else if value > max {
            Some(ClipSide::High)
        } else {
            None
        };

        let rgb = match &self.palette {
            ColorPalette::Builtin(colormap)
                if self.effective_mode(min, max) == ScaleMode::Linear =>
            {
                colormap.colormap(value, min, max)
            }
            _ => match self.effective_mode(min, max).normalize(value, min, max) {
                Some(t) => self.palette.sample_unit(t),
                None => MID_GRAY,
            },
        };
        MappedColor { rgb, clipped }
    }

    /// The legend descriptor for the effective mapping.
    pub fn legend(&self, stats: &ColorStats) -> ColorLegend {
        if self.quantity == ColorQuantity::Direction2d {
            return ColorLegend {
                kind: RampKind::Cyclic,
                min: 0.0,
                max: 360.0,
                ticks: vec![0.0, 90.0, 180.0, 270.0, 360.0],
                cyclic: true,
            };
        }

        let (min, max) = self.effective_range(stats);
        let kind = self.palette.kind();
        let cyclic = kind == RampKind::Cyclic;
        let range_valid = min.is_finite() && max.is_finite() && min < max;
        let min = if min.is_finite() { min } else { 0.0 };
        let max = if max.is_finite() { max } else { 0.0 };
        let ticks = if range_valid {
            ColorStyle {
                ramp: ColorRamp::default(),
                mode: self.effective_mode(min, max),
            }
            .legend_ticks(min, max)
        } else {
            Vec::new()
        };
        ColorLegend {
            kind,
            min,
            max,
            ticks,
            cyclic,
        }
    }

    /// Counts samples below and above the effective range.
    ///
    /// Direction colouring never clips. Values that cannot be evaluated
    /// (non-finite value or range) are ignored.
    pub fn clip_counts(&self, samples: &[ColorSample], stats: &ColorStats) -> ClipCounts {
        if self.quantity == ColorQuantity::Direction2d {
            return ClipCounts::default();
        }
        let (min, max) = self.effective_range(stats);
        if !min.is_finite() || !max.is_finite() || min >= max {
            return ClipCounts::default();
        }

        let mut counts = ClipCounts::default();
        for sample in samples {
            let value = self.value(sample);
            if !value.is_finite() {
                continue;
            }
            if value < min {
                counts.low += 1;
            } else if value > max {
                counts.high += 1;
            }
        }
        counts
    }

    /// Why a non-linear mode was disabled, if it was.
    ///
    /// `None` means the effective mode is usable as requested. A signed log is
    /// usable only when the range is finite and strictly spans zero and its
    /// half-width is valid.
    pub fn explanation(&self, stats: &ColorStats) -> Option<String> {
        let (min, max) = self.effective_range(stats);
        match self.mode {
            ScaleMode::Linear => None,
            ScaleMode::SignedLog { .. } if self.signed_log_usable(min, max) => None,
            ScaleMode::SignedLog { .. } if min < 0.0 && max > 0.0 => Some(
                "Signed log colouring has an invalid half-width; using linear scaling.".to_owned(),
            ),
            ScaleMode::SignedLog { .. } => Some(
                "Signed log colouring needs a range that spans zero; using linear scaling."
                    .to_owned(),
            ),
        }
    }

    fn sample_direction(&self, sample: &ColorSample) -> MappedColor {
        let dir = sample.dir;
        if !dir.x.is_finite() || !dir.y.is_finite() || !dir.z.is_finite() {
            return MappedColor {
                rgb: MID_GRAY,
                clipped: None,
            };
        }
        let angle = dir.y.atan2(dir.x);
        let t = (angle / std::f32::consts::TAU).rem_euclid(1.0);
        // A tiny negative angle can round up to exactly 1.0 after
        // `rem_euclid`; fold it back to the seam so a cyclic ramp shows its
        // first-stop colour at ~0° instead of blinking to the far end.
        let t = if t >= 1.0 { 0.0 } else { t };
        MappedColor {
            rgb: self.palette.sample_unit(t),
            clipped: None,
        }
    }

    fn value(&self, sample: &ColorSample) -> f32 {
        match self.quantity {
            ColorQuantity::Magnitude => sample.magnitude,
            ColorQuantity::ComponentX => sample.dir.x * sample.magnitude,
            ColorQuantity::ComponentY => sample.dir.y * sample.magnitude,
            ColorQuantity::ComponentZ => sample.dir.z * sample.magnitude,
            ColorQuantity::Direction2d => f32::NAN,
        }
    }

    fn signed_log_usable(&self, min: f32, max: f32) -> bool {
        let ScaleMode::SignedLog { half_width } = self.mode else {
            return true;
        };
        min.is_finite()
            && max.is_finite()
            && min < 0.0
            && max > 0.0
            && half_width.is_finite()
            && half_width > 0.0
    }

    fn effective_mode(&self, min: f32, max: f32) -> ScaleMode {
        if self.signed_log_usable(min, max) {
            self.mode
        } else {
            ScaleMode::Linear
        }
    }
}
