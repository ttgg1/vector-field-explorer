//! Sampling-domain coverage.
//!
//! Moved out of `src/domain.rs`'s inline module (ticket 13) without changing a
//! test or an assertion: the grid is reached through the public `Domain` API.

use glam::{UVec3, Vec3};
use vector_field_explorer::domain::Domain;

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
