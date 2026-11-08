//! Sampling: field + domain grid + view options → arrow instance data.

use glam::UVec3;

use crate::colormap::Colormap;
use crate::domain::Domain;
use crate::field::FieldExpr;

/// How arrows are colored: either a fixed magnitude range, or auto-fit to
/// the magnitudes actually sampled.
#[derive(Debug, Clone, PartialEq)]
pub struct ViewOptions {
    pub colormap: Colormap,
    pub color_range: Option<(f32, f32)>,
}

impl Default for ViewOptions {
    fn default() -> Self {
        Self { colormap: Colormap::Viridis, color_range: None }
    }
}

/// One sampled arrow: position, unit direction, true magnitude, color.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArrowData {
    pub position: glam::Vec3,
    /// Unit direction of the field at `position`.
    pub dir: glam::Vec3,
    /// Field magnitude (unchanged by any later direction normalization).
    pub mag: f32,
    pub color: [u8; 3],
}

/// (min, max) magnitude over sampled arrows; empty slice → (0.0, 0.0).
pub fn magnitude_range(arrows: &[ArrowData]) -> (f32, f32) {
    if arrows.is_empty() {
        return (0.0, 0.0);
    }
    let (mut min, mut max) = (f32::INFINITY, f32::NEG_INFINITY);
    for a in arrows {
        min = min.min(a.mag);
        max = max.max(a.mag);
    }
    (min, max)
}

/// Sample `field` over `domain.grid_points(grid)`, skipping degenerate
/// arrows (non-finite field values, or nearly zero-length directions).
pub fn sample_field(field: &FieldExpr, domain: &Domain, grid: UVec3, view: &ViewOptions) -> Vec<ArrowData> {
    let mut sampled: Vec<(glam::Vec3, glam::Vec3, f32)> = Vec::new();
    for p in domain.grid_points(grid) {
        let Some(v) = field.eval3(p) else { continue };
        let mag = v.length();
        if mag < 1e-6 || !mag.is_finite() {
            continue;
        }
        sampled.push((p, v / mag, mag));
    }

    let (cmin, cmax) = view.color_range.unwrap_or_else(|| {
        let (mut min, mut max) = (f32::INFINITY, f32::NEG_INFINITY);
        for &(_, _, m) in &sampled {
            min = min.min(m);
            max = max.max(m);
        }
        (min, max)
    });
    sampled
        .into_iter()
        .map(|(position, dir, mag)| ArrowData {
            position,
            dir,
            mag,
            color: view.colormap.colormap(mag, cmin, cmax),
        })
        .collect()
}

