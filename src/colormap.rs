//! Color ramps mapping scalar magnitudes to RGB colors.
//!
//! Magnitudes are normalized against a `[min, max]` range and sampled from
//! the selected colormap; out-of-range magnitudes clamp to the ramp ends.

/// Colormaps available for viewing vector field magnitudes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Colormap {
    /// Perceptually uniform viridis ramp (dark purple to yellow).
    Viridis,
    /// Rainbow-like turbo ramp (blue to red).
    Turbo,
    /// Grayscale ramp from black at the minimum to white at the maximum.
    Mono,
}

impl Colormap {
    pub const ALL: [Colormap; 3] = [Colormap::Viridis, Colormap::Turbo, Colormap::Mono];

    /// Human-readable name shown in the colormap picker.
    pub fn label(&self) -> &'static str {
        match self {
            Colormap::Viridis => "Viridis",
            Colormap::Turbo => "Turbo",
            Colormap::Mono => "Mono",
        }
    }

    /// Maps `magnitude` to an RGB triple given the displayed `[min, max]`
    /// range. Magnitudes outside the range clamp to the ramp ends; an empty
    /// range (`max == min`) yields a constant mid gray.
    /// Maps `magnitude` to an RGB triple given the displayed `[min, max]`
    /// range. Magnitudes outside the range clamp to the ramp ends; an empty
    /// range (`max <= min`) yields a constant mid gray.
    pub fn colormap(&self, magnitude: f32, min: f32, max: f32) -> [u8; 3] {
        if max <= min {
            return [128, 128, 128];
        }
        let t = ((magnitude - min) / (max - min)).clamp(0.0, 1.0);
        match self {
            Colormap::Mono => {
                let v = (255.0 * t + 0.5) as u8;
                [v, v, v]
            }
            Colormap::Viridis => colorous::VIRIDIS.eval_continuous(t as f64).as_array(),
            Colormap::Turbo => colorous::TURBO.eval_continuous(t as f64).as_array(),
        }
    }
}

