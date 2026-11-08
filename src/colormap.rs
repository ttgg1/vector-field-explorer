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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viridis_endpoints() {
        assert_eq!(Colormap::Viridis.colormap(0.0, 0.0, 1.0), [68, 1, 84]);
        assert_eq!(Colormap::Viridis.colormap(1.0, 0.0, 1.0), [253, 231, 37]);
    }

    #[test]
    fn mono_endpoints() {
        assert_eq!(Colormap::Mono.colormap(0.0, 0.0, 1.0), [0, 0, 0]);
        assert_eq!(Colormap::Mono.colormap(1.0, 0.0, 1.0), [255, 255, 255]);
    }

    #[test]
    fn mono_midpoint_is_gray() {
        let c = Colormap::Mono.colormap(0.5, 0.0, 1.0);
        assert_eq!(c, [128, 128, 128]);
    }

    #[test]
    fn turbo_interior_differs_from_endpoints() {
        let lo = Colormap::Turbo.colormap(0.0, 0.0, 1.0);
        let mid = Colormap::Turbo.colormap(0.5, 0.0, 1.0);
        let hi = Colormap::Turbo.colormap(1.0, 0.0, 1.0);
        assert_ne!(mid, lo);
        assert_ne!(mid, hi);
    }

    #[test]
    fn out_of_range_clamps() {
        assert_eq!(Colormap::Viridis.colormap(-1.0, 0.0, 1.0), [68, 1, 84]);
        assert_eq!(Colormap::Viridis.colormap(2.0, 0.0, 1.0), [253, 231, 37]);
        assert_eq!(Colormap::Mono.colormap(-5.0, 0.0, 1.0), [0, 0, 0]);
        assert_eq!(Colormap::Mono.colormap(5.0, 0.0, 1.0), [255, 255, 255]);
    }

    #[test]
    fn offset_range_normalizes() {
        // Same ramp endpoints sampled over a non-zero-based range.
        assert_eq!(Colormap::Viridis.colormap(2.0, 2.0, 3.0), [68, 1, 84]);
        assert_eq!(Colormap::Viridis.colormap(3.0, 2.0, 3.0), [253, 231, 37]);
    }

    #[test]
    fn degenerate_range_is_mid_gray() {
        assert_eq!(Colormap::Viridis.colormap(0.0, 1.5, 1.5), [128, 128, 128]);
        assert_eq!(Colormap::Turbo.colormap(2.0, 0.0, 0.0), [128, 128, 128]);
        assert_eq!(Colormap::Mono.colormap(-0.5, 0.0, 0.0), [128, 128, 128]);
    }

    #[test]
    fn negative_magnitudes_map() {
        // Range spanning negative magnitudes, sampled at both ends.
        let lo = Colormap::Mono.colormap(-2.0, -2.0, 2.0);
        let hi = Colormap::Mono.colormap(2.0, -2.0, 2.0);
        assert_eq!(lo, [0, 0, 0]);
        assert_eq!(hi, [255, 255, 255]);
    }

    #[test]
    fn all_covers_every_variant_with_unique_labels() {
        assert_eq!(
            Colormap::ALL,
            [Colormap::Viridis, Colormap::Turbo, Colormap::Mono]
        );
        for (i, &c) in Colormap::ALL.iter().enumerate() {
            assert!(!c.label().is_empty());
            assert!(Colormap::ALL[..i].iter().all(|o| o.label() != c.label()));
        }
    }
}
