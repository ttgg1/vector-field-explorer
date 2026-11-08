//! Domain-matched reference geometry (ticket 23).
//!
//! App-side decoration only: the wireframe cube, face grids and axes are folded
//! into the line collection by `app::compose_lines`, never counted as field
//! geometry and never owned by the viewmodel snapshot.
//!
//! 3D sampling draws the 12 domain box edges, light grid lines on the three
//! far faces (the min-x/min-y/min-z planes, which face away from the default
//! orbit eye), and coloured X/Y/Z axes through the domain centre. Flat sampling
//! draws one grid on the domain's mid-z plane, which is exactly where
//! `Domain::grid_points` puts a single z slice.

use glam::Vec3;

use crate::domain::Domain;
use crate::renderer::LineInstanceGPU;

/// Subdivisions per face direction; 10 gives the 11 lines that matched the old
/// flat grid's 21-line-per-axis density.
pub const REFERENCE_DIVISIONS: u32 = 10;

/// Colour of the 12 domain box edges.
pub const EDGE_COLOR: [f32; 4] = [0.38, 0.42, 0.48, 1.0];
/// Colour of the subdivided face grid.
pub const GRID_COLOR: [f32; 4] = [0.24, 0.26, 0.30, 1.0];
/// X axis (red-ish), Y axis (green-ish), Z axis (blue-ish).
pub const AXIS_X_COLOR: [f32; 4] = [0.86, 0.26, 0.26, 1.0];
pub const AXIS_Y_COLOR: [f32; 4] = [0.30, 0.80, 0.35, 1.0];
pub const AXIS_Z_COLOR: [f32; 4] = [0.30, 0.45, 0.95, 1.0];

/// Line width of the face grid, world units.
pub const GRID_WIDTH: f32 = 0.006;
/// Line width of the box edges.
pub const EDGE_WIDTH: f32 = 0.012;
/// Line width of the coloured axes.
pub const AXIS_WIDTH: f32 = 0.014;

/// Reference lines for `domain`: a wireframe cube with subdivided back faces
/// and coloured axes when sampling runs three-dimensional, one flat grid on
/// the domain's mid-z plane otherwise.
pub fn reference_lines(domain: &Domain, sampling_3d: bool) -> Vec<LineInstanceGPU> {
    if sampling_3d { cube_lines(domain) } else { plane_lines(domain) }
}

fn push(lines: &mut Vec<LineInstanceGPU>, a: Vec3, b: Vec3, color: [f32; 4], width: f32) {
    lines.push(LineInstanceGPU {
        a: [a.x, a.y, a.z, 1.0],
        b: [b.x, b.y, b.z, width],
        color,
    });
}

/// Sample `k` of `divisions` between `lo` and `hi`, inclusive on both ends.
fn lerp(lo: f32, hi: f32, k: u32) -> f32 {
    lo + (hi - lo) * (k as f32 / REFERENCE_DIVISIONS as f32)
}

fn cube_lines(domain: &Domain) -> Vec<LineInstanceGPU> {
    let min = domain.min;
    let max = domain.max;
    let mut lines = Vec::new();

    // 12 box edges.
    for y in [min.y, max.y] {
        for z in [min.z, max.z] {
            push(&mut lines, Vec3::new(min.x, y, z), Vec3::new(max.x, y, z), EDGE_COLOR, EDGE_WIDTH);
        }
    }
    for x in [min.x, max.x] {
        for z in [min.z, max.z] {
            push(&mut lines, Vec3::new(x, min.y, z), Vec3::new(x, max.y, z), EDGE_COLOR, EDGE_WIDTH);
        }
    }
    for x in [min.x, max.x] {
        for y in [min.y, max.y] {
            push(&mut lines, Vec3::new(x, y, min.z), Vec3::new(x, y, max.z), EDGE_COLOR, EDGE_WIDTH);
        }
    }

    // Light grids on the three far faces. Each face gets one line per
    // subdivision along each of its two in-plane directions.
    for k in 0..=REFERENCE_DIVISIONS {
        let t = lerp(min.y, max.y, k);
        push(&mut lines, Vec3::new(min.x, t, min.z), Vec3::new(min.x, t, max.z), GRID_COLOR, GRID_WIDTH);
        let t = lerp(min.z, max.z, k);
        push(&mut lines, Vec3::new(min.x, min.y, t), Vec3::new(min.x, max.y, t), GRID_COLOR, GRID_WIDTH);

        let t = lerp(min.x, max.x, k);
        push(&mut lines, Vec3::new(t, min.y, min.z), Vec3::new(t, min.y, max.z), GRID_COLOR, GRID_WIDTH);
        let t = lerp(min.z, max.z, k);
        push(&mut lines, Vec3::new(min.x, min.y, t), Vec3::new(max.x, min.y, t), GRID_COLOR, GRID_WIDTH);

        let t = lerp(min.x, max.x, k);
        push(&mut lines, Vec3::new(t, min.y, min.z), Vec3::new(t, max.y, min.z), GRID_COLOR, GRID_WIDTH);
        let t = lerp(min.y, max.y, k);
        push(&mut lines, Vec3::new(min.x, t, min.z), Vec3::new(max.x, t, min.z), GRID_COLOR, GRID_WIDTH);
    }

    // Coloured axes through the domain centre.
    let centre = (min + max) * 0.5;
    push(&mut lines, Vec3::new(min.x, centre.y, centre.z), Vec3::new(max.x, centre.y, centre.z), AXIS_X_COLOR, AXIS_WIDTH);
    push(&mut lines, Vec3::new(centre.x, min.y, centre.z), Vec3::new(centre.x, max.y, centre.z), AXIS_Y_COLOR, AXIS_WIDTH);
    push(&mut lines, Vec3::new(centre.x, centre.y, min.z), Vec3::new(centre.x, centre.y, max.z), AXIS_Z_COLOR, AXIS_WIDTH);

    lines
}

fn plane_lines(domain: &Domain) -> Vec<LineInstanceGPU> {
    let min = domain.min;
    let max = domain.max;
    let z = (min.z + max.z) * 0.5;
    let mut lines = Vec::new();

    for k in 0..=REFERENCE_DIVISIONS {
        let y = lerp(min.y, max.y, k);
        push(&mut lines, Vec3::new(min.x, y, z), Vec3::new(max.x, y, z), GRID_COLOR, GRID_WIDTH);
        let x = lerp(min.x, max.x, k);
        push(&mut lines, Vec3::new(x, min.y, z), Vec3::new(x, max.y, z), GRID_COLOR, GRID_WIDTH);
    }

    let centre = (min + max) * 0.5;
    push(&mut lines, Vec3::new(min.x, centre.y, z), Vec3::new(max.x, centre.y, z), AXIS_X_COLOR, AXIS_WIDTH);
    push(&mut lines, Vec3::new(centre.x, min.y, z), Vec3::new(centre.x, max.y, z), AXIS_Y_COLOR, AXIS_WIDTH);

    lines
}
