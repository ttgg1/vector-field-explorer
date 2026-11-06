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

#[cfg(test)]
mod tests {
    use super::{sample_field, magnitude_range, ViewOptions};
    use crate::colormap::Colormap;
    use crate::domain::Domain;
    use crate::field::FieldExpr;
    use glam::{UVec3, Vec3};

    fn opts(colormap: Colormap, color_range: Option<(f32, f32)>) -> ViewOptions {
        ViewOptions { colormap, color_range }
    }

    #[test]
    fn constant_field_fills_grid() {
        let f = FieldExpr::parse("1, 0, 0").unwrap();
        let arrows = sample_field(&f, &Domain::default(), UVec3::new(2, 2, 1), &opts(Colormap::Mono, None));
        assert_eq!(arrows.len(), 4);
        for a in &arrows {
            assert!(a.dir.distance(Vec3::X) < 1e-5, "dir must be unit +X");
            assert_eq!(a.mag, 1.0);
        }
    }

    #[test]
    fn magnitude_range_of_constant_field() {
        let f = FieldExpr::parse("1, 0, 0").unwrap();
        let arrows = sample_field(&f, &Domain::default(), UVec3::new(2, 2, 1), &opts(Colormap::Mono, None));
        assert_eq!(magnitude_range(&arrows), (1.0, 1.0));
    }

    #[test]
    fn sample_skips_nan_arrows() {
        // 1/x explodes at x == 0; that grid point must be dropped, not crash
        let f = FieldExpr::parse("1/x, 0, 0").unwrap();
        let arrows = sample_field(&f, &Domain::default(), UVec3::new(5, 1, 1), &opts(Colormap::Mono, None));
        assert_eq!(arrows.len(), 4, "the x=0 arrow is dropped");
        assert!(arrows.iter().all(|a| a.position.x.abs() > 1e-6));
    }

    #[test]
    fn all_zero_field_yields_no_arrows() {
        // zero-length direction is degenerate - the vector stays empty
        let f = FieldExpr::parse("0, 0, 0").unwrap();
        let arrows = sample_field(&f, &Domain::default(), UVec3::new(2, 2, 1), &opts(Colormap::Mono, None));
        assert!(arrows.is_empty());
        assert_eq!(magnitude_range(&arrows), (0.0, 0.0));
    }

    #[test]
    fn color_follows_provided_range() {
        // Plan ruling: the plan's default symmetric domain makes |x| equal at
        // both ends, so "left darker than right" is untestable there. The
        // intended behavior (color follows magnitude through the provided
        // range) is preserved with an asymmetric sampled domain.
        let f = FieldExpr::parse("x, 0, 0").unwrap();
        let domain = Domain::new(Vec3::new(-0.5, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0));
        let arrows = sample_field(&f, &domain, UVec3::new(2, 1, 1), &opts(Colormap::Mono, Some((0.0, 1.0))));
        assert_eq!(arrows.len(), 2);
        let (left, right) = (&arrows[0], &arrows[1]);
        assert!(left.mag < right.mag, "mags {} < {}", left.mag, right.mag);
        for c in 0..3 {
            assert!(left.color[c] < right.color[c], "left arrow must be darker");
        }
    }
}
