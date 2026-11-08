//! Plane-by-plane inspection (ticket 23): the pure seam for selecting one
//! sampled slice of the 3D cube and dimming the rest.
//!
//! The app samples the full cube and then colours each arrow by whether its
//! grid index on the chosen axis matches the selected plane. Matching is by
//! index (recovered from the arrow's position), never by float equality, so a
//! sample that is a rounding error away from the plane coordinate still
//! belongs to it.

use glam::{UVec3, Vec3};

use crate::domain::Domain;

/// Which grid axis a plane is perpendicular to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaneAxis {
    X,
    Y,
    Z,
}

impl PlaneAxis {
    /// The axes in UI order.
    pub const ALL: [PlaneAxis; 3] = [Self::X, Self::Y, Self::Z];

    /// Lower-case axis name, used by the slider label.
    pub fn label(self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Y => "y",
            Self::Z => "z",
        }
    }

    /// Samples along this axis in `grid`.
    pub fn count(self, grid: UVec3) -> u32 {
        match self {
            Self::X => grid.x,
            Self::Y => grid.y,
            Self::Z => grid.z,
        }
    }

    /// Index of the middle sample on this axis; 0 for a one-slice axis.
    pub fn middle_index(self, grid: UVec3) -> u32 {
        self.count(grid).saturating_sub(1) / 2
    }

    /// The component of `p` along this axis.
    fn component(self, p: Vec3) -> f32 {
        match self {
            Self::X => p.x,
            Self::Y => p.y,
            Self::Z => p.z,
        }
    }
}

/// One selected slice of the sampled grid. `index` is a grid index, not a
/// world coordinate; [`plane_coordinate`] turns it into one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaneView {
    pub enabled: bool,
    pub axis: PlaneAxis,
    pub index: u32,
}

impl Default for PlaneView {
    fn default() -> Self {
        Self { enabled: false, axis: PlaneAxis::Z, index: 0 }
    }
}

impl PlaneView {
    /// The default selection for `grid`: z, disabled, middle index.
    pub fn middle(grid: UVec3) -> Self {
        Self {
            enabled: false,
            axis: PlaneAxis::Z,
            index: PlaneAxis::Z.middle_index(grid),
        }
    }

    /// The same selection with `index` clamped into `grid`'s axis; a
    /// one-slice axis collapses to index 0.
    pub fn clamped(self, grid: UVec3) -> Self {
        let count = self.axis.count(grid);
        Self {
            index: if count <= 1 { 0 } else { self.index.min(count - 1) },
            ..self
        }
    }

    /// Whether this selection actually splits the grid: enabled and the axis
    /// has more than one sample.
    pub fn active(self, grid: UVec3) -> bool {
        self.enabled && self.axis.count(grid) > 1
    }

    /// Samples along the selected axis in `grid`.
    pub fn count(self, grid: UVec3) -> u32 {
        self.axis.count(grid)
    }
}

/// How far a dimmed colour moves toward neutral grey, 0..1.
pub const DIM_FACTOR: f32 = 0.75;
/// The neutral grey dimmed colours lerp toward (8-bit).
pub const DIM_GREY: f32 = 128.0;
/// Opacity of the off-plane arrows: translucent enough to see the cube, solid
/// enough to keep its shape.
pub const DIM_ALPHA: f32 = 0.2;

/// World coordinate of sample `index` on `axis`, matching
/// [`Domain::grid_points`] exactly (endpoints included, one slice at the
/// midpoint).
pub fn plane_coordinate(domain: &Domain, grid: UVec3, axis: PlaneAxis, index: u32) -> f32 {
    let n = axis.count(grid);
    let lo = axis.component(domain.min);
    let size = axis.component(domain.domain_size());
    if n <= 1 {
        return lo + size * 0.5;
    }
    let i = index.min(n - 1) as f32;
    lo + size * (i / (n - 1) as f32)
}

/// Whether `position` belongs to `plane`'s slice, by recovering its grid index
/// from the coordinate rather than comparing floats.
pub fn on_plane(position: Vec3, domain: &Domain, grid: UVec3, plane: &PlaneView) -> bool {
    let n = plane.axis.count(grid);
    if n <= 1 {
        return true;
    }
    let lo = plane.axis.component(domain.min);
    let size = plane.axis.component(domain.domain_size());
    let index = if size.is_finite() && size > 0.0 {
        let t = (plane.axis.component(position) - lo) / size;
        (t * (n - 1) as f32).round().clamp(0.0, (n - 1) as f32) as u32
    } else {
        0
    };
    index == plane.index.min(n - 1)
}

/// Lerp an arrow colour three quarters of the way toward neutral grey.
pub fn dim_color(color: [u8; 3]) -> [u8; 3] {
    let lerp = |channel: u8| {
        let c = channel as f32;
        (c * (1.0 - DIM_FACTOR) + DIM_GREY * DIM_FACTOR).round().clamp(0.0, 255.0) as u8
    };
    [lerp(color[0]), lerp(color[1]), lerp(color[2])]
}
