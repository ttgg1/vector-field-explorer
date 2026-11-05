//! Axis-aligned sampling domain for the vector field grid.
//!
//! The domain is an axis-aligned box `[min, max]` on all three axes; 2D
//! callers simply keep one sample along the z axis.

use glam::{UVec3, Vec3};

/// An axis-aligned sampling box, `min` and `max` inclusive.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Domain {
    pub min: Vec3,
    pub max: Vec3,
}

impl Domain {
    pub fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }

    /// Extents of the box along each axis.
    pub fn domain_size(&self) -> Vec3 {
        self.max - self.min
    }

    /// Samples `dims` points per axis covering the box inclusive on both
    /// ends. An axis with a single sample sits at that axis's midpoint.
    /// An empty iterator results if any dimension is zero.
    pub fn grid_points(&self, dims: UVec3) -> impl Iterator<Item = Vec3> {
        GridPoints {
            domain: *self,
            dims,
            index: 0,
            total: if dims.x.min(dims.y).min(dims.z) == 0 {
                0
            } else {
                dims.x as u64 * dims.y as u64 * dims.z as u64
            },
        }
    }
}

impl Default for Domain {
    fn default() -> Self {
        Self::new(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0))
    }
}

/// Iterator over `Domain::grid_points`, x-major then y then z.
pub struct GridPoints {
    domain: Domain,
    dims: UVec3,
    index: u64,
    total: u64,
}

impl GridPoints {
    /// Position along one axis: `n == 0` is never sampled; `n == 1`
    /// returns the axis midpoint; otherwise endpoints are included.
    fn axis_point(n: u32, i: u32, lo: f32, size: f32) -> f32 {
        if n <= 1 {
            lo + size * 0.5
        } else {
            lo + size * (i as f32 / (n - 1) as f32)
        }
    }
}

impl Iterator for GridPoints {
    type Item = Vec3;

    fn next(&mut self) -> Option<Vec3> {
        if self.index >= self.total {
            return None;
        }
        let flat = self.index;
        self.index += 1;
        let i = flat as u32;
        let xi = i % self.dims.x;
        let yi = (i / self.dims.x) % self.dims.y;
        let zi = i / (self.dims.x * self.dims.y);
        let size = self.domain.domain_size();
        let lo = self.domain.min;
        Some(Vec3::new(
            Self::axis_point(self.dims.x, xi, lo.x, size.x),
            Self::axis_point(self.dims.y, yi, lo.y, size.y),
            Self::axis_point(self.dims.z, zi, lo.z, size.z),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{Vec3, UVec3};

    #[test]
    fn default_is_unit_cube() {
        let d = Domain::default();
        assert_eq!(d.min, Vec3::new(-1.0, -1.0, -1.0));
        assert_eq!(d.max, Vec3::new(1.0, 1.0, 1.0));
    }

    #[test]
    fn new_ranges_and_size() {
        let d = Domain::new(Vec3::new(0.0, -2.0, -0.5), Vec3::new(4.0, 2.0, 0.5));
        assert_eq!(d.domain_size(), Vec3::new(4.0, 4.0, 1.0));
    }

    #[test]
    fn line_grid_samples_axis_endpoints_only() {
        let d = Domain::default();
        let pts: Vec<Vec3> = d.grid_points(UVec3::new(2, 1, 1)).collect();
        assert_eq!(pts.len(), 2, "dims (2,1,1) must produce exactly 2 points");
        let xs: Vec<f32> = pts.iter().map(|p| p.x).collect();
        assert!(xs.contains(&-1.0), "x must include min endpoint -1.0");
        assert!(xs.contains(&1.0), "x must include max endpoint 1.0");
        for p in &pts {
            assert_eq!(p.y, 0.0, "single y sample must sit at the axis midpoint");
            assert_eq!(p.z, 0.0, "single z sample must sit at the axis midpoint");
        }
    }

    #[test]
    fn cube_grid_count_and_endpoints_included() {
        let d = Domain::default();
        let pts: Vec<Vec3> = d.grid_points(UVec3::new(3, 3, 3)).collect();
        assert_eq!(pts.len(), 27, "dims (3,3,3) must produce 27 points");
        for axis_min in [
            pts.iter().any(|p| p.x == -1.0),
            pts.iter().any(|p| p.y == -1.0),
            pts.iter().any(|p| p.z == -1.0),
        ] {
            assert!(axis_min, "each axis must include its -1.0 endpoint");
        }
        for axis_max in [
            pts.iter().any(|p| p.x == 1.0),
            pts.iter().any(|p| p.y == 1.0),
            pts.iter().any(|p| p.z == 1.0),
        ] {
            assert!(axis_max, "each axis must include its 1.0 endpoint");
        }
    }

    #[test]
    fn zero_dim_grid_is_empty() {
        let d = Domain::default();
        assert_eq!(d.grid_points(UVec3::new(2, 0, 1)).count(), 0);
    }
}
