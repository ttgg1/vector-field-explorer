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

    /// The centre of the box, the point every camera framing command targets.
    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
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

