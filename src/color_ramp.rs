//! Validated custom colour ramps and value scaling.
//!
//! This is the pure model behind the custom-gradient UI: a [`ColorRamp`] is a
//! list of validated colour stops, a [`ScaleMode`] turns values into ramp
//! positions, and [`ColorStyle`] combines them for sampling and legend ticks.
//! It does not touch the renderer, persistence or the existing [`Colormap`]
//! behaviour.

use crate::colormap::Colormap;

/// The mid grey used when a value cannot be normalized.
const MID_GRAY: Rgb = Rgb([128, 128, 128]);

/// An sRGB colour with 8-bit channels.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct Rgb(pub [u8; 3]);

impl Rgb {
    /// Parses exactly six hexadecimal digits with an optional leading `#`.
    ///
    /// Input is ASCII case-insensitive. Anything else (wrong length, non-hex
    /// bytes, empty input or a bare `#`) returns `None` and never panics.
    pub fn from_hex(text: &str) -> Option<Self> {
        let digits = text.strip_prefix('#').unwrap_or(text);
        let bytes = digits.as_bytes();
        if bytes.len() != 6 {
            return None;
        }
        let mut channels = [0u8; 3];
        for (index, channel) in channels.iter_mut().enumerate() {
            let high = hex_digit(bytes[index * 2])?;
            let low = hex_digit(bytes[index * 2 + 1])?;
            *channel = high * 16 + low;
        }
        Some(Self(channels))
    }

    /// Returns the canonical lowercase `#rrggbb` form.
    pub fn to_hex(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0[0], self.0[1], self.0[2])
    }
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// One colour stop in a [`ColorRamp`].
#[derive(
    Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize,
)]
pub struct ColorStop {
    /// Normalized ramp position, in `0.0..=1.0`.
    pub position: f32,
    /// Colour sampled at this position.
    pub color: Rgb,
}

/// The semantic use of a [`ColorRamp`].
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize,
)]
pub enum RampKind {
    /// Ordered low-to-high ramp.
    #[default]
    Sequential,
    /// Two-sided ramp with a meaningful centre.
    Diverging,
    /// Wrapping ramp whose endpoints match.
    Cyclic,
}

impl RampKind {
    /// Every ramp kind, in display order.
    pub const ALL: [RampKind; 3] = [
        RampKind::Sequential,
        RampKind::Diverging,
        RampKind::Cyclic,
    ];

    /// Human-readable name shown in the UI.
    pub fn label(&self) -> &'static str {
        match self {
            RampKind::Sequential => "Sequential",
            RampKind::Diverging => "Diverging",
            RampKind::Cyclic => "Cyclic",
        }
    }
}

/// A validated list of colour stops plus reversal and semantic kind.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ColorRamp {
    /// Stops ordered by strictly increasing [`ColorStop::position`].
    pub stops: Vec<ColorStop>,
    /// Whether sampling reads the ramp back to front.
    pub reversed: bool,
    /// Semantic use of the ramp.
    pub kind: RampKind,
}

impl ColorRamp {
    /// Smallest accepted number of stops.
    pub const MIN_STOPS: usize = 2;
    /// Largest accepted number of stops.
    pub const MAX_STOPS: usize = 16;

    /// Checks every structural rule and returns the first failure.
    ///
    /// Rejections name the rule that failed; [`Self::sanitized`] turns an
    /// invalid ramp into the default instead of repairing fields.
    pub fn validate(&self) -> Result<(), String> {
        let count = self.stops.len();
        if count < Self::MIN_STOPS {
            return Err(format!(
                "ramp needs at least {} stops, got {count}",
                Self::MIN_STOPS
            ));
        }
        if count > Self::MAX_STOPS {
            return Err(format!(
                "ramp accepts at most {} stops, got {count}",
                Self::MAX_STOPS
            ));
        }

        let mut previous = f32::NEG_INFINITY;
        for stop in &self.stops {
            if !stop.position.is_finite() {
                return Err("stop positions must be finite".to_owned());
            }
            if !(0.0..=1.0).contains(&stop.position) {
                return Err("stop positions must lie in 0.0..=1.0".to_owned());
            }
            if stop.position <= previous {
                return Err("stop positions must be strictly increasing".to_owned());
            }
            previous = stop.position;
        }

        let first = &self.stops[0];
        let last = &self.stops[count - 1];
        if first.position != 0.0 {
            return Err("first stop position must be exactly 0.0".to_owned());
        }
        if last.position != 1.0 {
            return Err("last stop position must be exactly 1.0".to_owned());
        }

        match self.kind {
            RampKind::Sequential => {}
            RampKind::Diverging => {
                if count < 3 {
                    return Err("diverging ramps need at least 3 stops".to_owned());
                }
                if !self.stops.iter().any(|stop| stop.position == 0.5) {
                    return Err(
                        "diverging ramps need a stop at exactly 0.5".to_owned()
                    );
                }
                if first.color == last.color {
                    return Err(
                        "diverging ramps need different endpoint colours".to_owned()
                    );
                }
            }
            RampKind::Cyclic => {
                if count < 3 {
                    return Err("cyclic ramps need at least 3 stops".to_owned());
                }
                if first.color != last.color {
                    return Err(
                        "cyclic ramps need identical endpoint colours".to_owned()
                    );
                }
            }
        }

        Ok(())
    }

    /// Returns `self` when valid, otherwise the default ramp.
    ///
    /// Individual fields are never repaired.
    pub fn sanitized(self) -> Self {
        if self.validate().is_ok() {
            self
        } else {
            Self::default()
        }
    }

    /// Samples the ramp at `t`, clamping to `0.0..=1.0` and treating NaN as 0.
    ///
    /// Reversal samples `1.0 - t`. Stop positions return their exact colour;
    /// between stops each byte channel is interpolated linearly in `f64` and
    /// rounded. Invalid ramps still sample without panicking.
    pub fn sample(&self, t: f32) -> Rgb {
        let t = if t.is_nan() {
            0.0
        } else {
            t.clamp(0.0, 1.0)
        };
        let t = if self.reversed { 1.0 - t } else { t };

        let Some(first) = self.stops.first() else {
            return Rgb([0, 0, 0]);
        };
        let Some(last) = self.stops.last() else {
            return Rgb([0, 0, 0]);
        };
        if let Some(stop) = self.stops.iter().find(|stop| stop.position == t) {
            return stop.color;
        }
        if t <= first.position {
            return first.color;
        }
        for pair in self.stops.windows(2) {
            let (low, high) = (pair[0], pair[1]);
            if t <= high.position {
                let span = high.position - low.position;
                if span <= 0.0 {
                    return high.color;
                }
                let fraction = f64::from(t - low.position) / f64::from(span);
                return Rgb([
                    lerp_channel(low.color.0[0], high.color.0[0], fraction),
                    lerp_channel(low.color.0[1], high.color.0[1], fraction),
                    lerp_channel(low.color.0[2], high.color.0[2], fraction),
                ]);
            }
        }
        last.color
    }

    /// Seeds an editable approximation of a builtin [`Colormap`].
    ///
    /// `Viridis`/`Turbo` use nine stops at `0.0, 0.125, ..., 1.0` sampled from
    /// [`Colormap::colormap`]; they are an approximation, matching exactly at
    /// both endpoints but not between stops. `Mono` is a linear byte ramp, so
    /// it is seeded with just two stops (`#000000` at `0.0`, `#ffffff` at
    /// `1.0`). That seed is structurally exact, but interior samples can differ
    /// from [`Colormap::Mono`] by one LSB at f32 half-step boundaries: the
    /// legacy reference rounds `(255.0 * t + 0.5)` in f32, whereas
    /// [`Self::sample`] interpolates and rounds in f64.
    pub fn builtin(colormap: Colormap) -> Self {
        let stops = match colormap {
            Colormap::Mono => vec![
                ColorStop {
                    position: 0.0,
                    color: Rgb([0, 0, 0]),
                },
                ColorStop {
                    position: 1.0,
                    color: Rgb([255, 255, 255]),
                },
            ],
            Colormap::Viridis | Colormap::Turbo => (0..=8u8)
                .map(|step| {
                    let position = f32::from(step) / 8.0;
                    ColorStop {
                        position,
                        color: Rgb(colormap.colormap(position, 0.0, 1.0)),
                    }
                })
                .collect(),
        };
        Self {
            stops,
            reversed: false,
            kind: RampKind::Sequential,
        }
    }
}

impl Default for ColorRamp {
    /// A two-stop black-to-white sequential ramp.
    fn default() -> Self {
        Self {
            stops: vec![
                ColorStop {
                    position: 0.0,
                    color: Rgb([0, 0, 0]),
                },
                ColorStop {
                    position: 1.0,
                    color: Rgb([255, 255, 255]),
                },
            ],
            reversed: false,
            kind: RampKind::Sequential,
        }
    }
}

fn lerp_channel(low: u8, high: u8, fraction: f64) -> u8 {
    let value = f64::from(low) + (f64::from(high) - f64::from(low)) * fraction;
    value.round().clamp(0.0, 255.0) as u8
}

/// Maps a raw value into `0.0..=1.0` against a finite `min < max` range.
#[derive(
    Debug, Clone, Copy, PartialEq, Default, serde::Serialize, serde::Deserialize,
)]
pub enum ScaleMode {
    /// Uniform mapping of the value range onto the ramp.
    #[default]
    Linear,
    /// Symmetric logarithmic mapping for ranges that span zero.
    SignedLog {
        /// Positive half-width of the linear region, in value units.
        half_width: f32,
    },
}

impl ScaleMode {
    /// Checks the mode's parameters.
    pub fn validate(&self) -> Result<(), String> {
        match *self {
            Self::Linear => Ok(()),
            Self::SignedLog { half_width } => {
                if !half_width.is_finite() {
                    return Err("signed log half_width must be finite".to_owned());
                }
                if half_width <= 0.0 {
                    return Err(
                        "signed log half_width must be greater than 0.0".to_owned()
                    );
                }
                if half_width > 1.0e6 {
                    return Err(
                        "signed log half_width must be at most 1.0e6".to_owned()
                    );
                }
                Ok(())
            }
        }
    }

    /// Normalizes `value` against `min..max`, or returns `None` when the
    /// inputs cannot be mapped.
    ///
    /// All three values must be finite and `min < max`. Signed log mode also
    /// requires a range that strictly spans zero (`min < 0.0 < max`). Signed
    /// log always maps `0.0` (including `-0.0`) to exactly `0.5`; when an
    /// endpoint scale underflows to zero, non-zero values of that sign clip to
    /// the corresponding extreme (`0.0` or `1.0`) instead of becoming NaN.
    pub fn normalize(&self, value: f32, min: f32, max: f32) -> Option<f32> {
        if !value.is_finite() || !min.is_finite() || !max.is_finite() || min >= max {
            return None;
        }
        let normalized = match *self {
            Self::Linear => {
                let span = f64::from(max) - f64::from(min);
                (f64::from(value) - f64::from(min)) / span
            }
            Self::SignedLog { half_width } => {
                if min >= 0.0 || max <= 0.0 {
                    return None;
                }
                if !half_width.is_finite() || half_width <= 0.0 {
                    return None;
                }
                let half_width = f64::from(half_width);
                let scaled =
                    |v: f64| v.signum() * (1.0 + v.abs() / half_width).ln();
                let value = f64::from(value);
                if value == 0.0 {
                    0.5
                } else {
                    let negative = value < 0.0;
                    let endpoint_scale = if negative {
                        scaled(f64::from(min))
                    } else {
                        scaled(f64::from(max))
                    };
                    if endpoint_scale == 0.0 {
                        // The endpoint scale underflowed; clip by sign.
                        if negative { 0.0 } else { 1.0 }
                    } else if negative {
                        0.5 - 0.5 * scaled(value) / endpoint_scale
                    } else {
                        0.5 + 0.5 * scaled(value) / endpoint_scale
                    }
                }
            }
        };
        let clamped = normalized.clamp(0.0, 1.0);
        if clamped.is_finite() {
            Some(clamped as f32)
        } else {
            None
        }
    }
}

/// A ramp plus the scaling that positions raw values on it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ColorStyle {
    /// Ramp sampled with normalized positions.
    pub ramp: ColorRamp,
    /// Mapping from raw values to ramp positions.
    pub mode: ScaleMode,
}

impl ColorStyle {
    /// Validates the ramp first, then the scale mode.
    pub fn validate(&self) -> Result<(), String> {
        self.ramp.validate()?;
        self.mode.validate()
    }

    /// Samples the ramp for `value` in `min..max`.
    ///
    /// Returns mid grey `[128, 128, 128]` exactly when normalization is
    /// impossible, preserving the existing empty-range colour.
    pub fn sample(&self, value: f32, min: f32, max: f32) -> Rgb {
        match self.mode.normalize(value, min, max) {
            Some(t) => self.ramp.sample(t),
            None => MID_GRAY,
        }
    }

    /// Returns at most nine strictly increasing legend ticks.
    ///
    /// Linear mode yields five evenly spaced values; signed log yields
    /// `[min, -half_width, 0.0, half_width, max]` sorted and deduplicated.
    /// Signed-log ticks are clipped to `min..=max`, so a `half_width` outside
    /// the range cannot emit out-of-range ticks. Both endpoints are retained
    /// for valid ranges; invalid ranges yield an empty vector.
    pub fn legend_ticks(&self, min: f32, max: f32) -> Vec<f32> {
        if !min.is_finite() || !max.is_finite() || min >= max {
            return Vec::new();
        }
        match self.mode {
            ScaleMode::Linear => {
                let mut ticks: Vec<f32> = (0..=4)
                    .map(|step| {
                        let fraction = f64::from(step) / 4.0;
                        let value = f64::from(min)
                            + (f64::from(max) - f64::from(min)) * fraction;
                        value as f32
                    })
                    .collect();
                ticks.dedup();
                ticks
            }
            ScaleMode::SignedLog { half_width } => {
                if !half_width.is_finite() || half_width <= 0.0 {
                    return Vec::new();
                }
                let mut ticks = vec![min, -half_width, 0.0, half_width, max];
                ticks.retain(|tick| *tick >= min && *tick <= max);
                ticks.sort_by(f32::total_cmp);
                ticks.dedup();
                ticks
            }
        }
    }
}

impl Default for ColorStyle {
    /// Builtin Viridis with linear scaling.
    fn default() -> Self {
        Self {
            ramp: ColorRamp::builtin(Colormap::Viridis),
            mode: ScaleMode::Linear,
        }
    }
}
