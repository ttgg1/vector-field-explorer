//! Domain-matched reference geometry (ticket 23).
//!
//! App-side decoration only: the wireframe cube, face grids and axes are folded
//! into the line collection by `app::compose_lines`, never counted as field
//! geometry and never owned by the viewmodel snapshot.
//!
//! 3D sampling draws light grid lines on the three far faces first, then the
//! 12 domain box edges over them (so coincident segments are not overdrawn by
//! the thinner grid), and the coloured X/Y/Z axes through the domain centre
//! last. Flat sampling draws one grid on the domain's mid-z plane, which is
//! exactly where `Domain::grid_points` puts a single z slice, then its axes.

use glam::Vec3;

use crate::domain::Domain;
use crate::plane::PlaneFrame;
use crate::renderer::LineInstanceGPU;
use crate::viewport_style::ViewportStyle;

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
    reference_lines_with(domain, sampling_3d, &ViewportStyle::default())
}

/// Reference lines for `domain` drawn with `style`. The legacy
/// [`reference_lines`] delegates here with the default style, so its output is
/// unchanged.
pub fn reference_lines_with(
    domain: &Domain,
    sampling_3d: bool,
    style: &ViewportStyle,
) -> Vec<LineInstanceGPU> {
    reference_lines_visible_with(domain, sampling_3d, true, true, style)
}

/// Selected domain decoration, with the grid and box preceding the axes.
pub(crate) fn reference_lines_visible(
    domain: &Domain,
    sampling_3d: bool,
    show_grid: bool,
    show_coordinate_system: bool,
) -> Vec<LineInstanceGPU> {
    reference_lines_visible_with(
        domain,
        sampling_3d,
        show_grid,
        show_coordinate_system,
        &ViewportStyle::default(),
    )
}

fn reference_lines_visible_with(
    domain: &Domain,
    sampling_3d: bool,
    show_grid: bool,
    show_coordinate_system: bool,
    style: &ViewportStyle,
) -> Vec<LineInstanceGPU> {
    let mut lines = Vec::new();
    if sampling_3d {
        if show_grid {
            cube_grid_and_box(domain, &mut lines, style);
        }
        if show_coordinate_system {
            cube_axes(domain, &mut lines, style);
        }
    } else {
        if show_grid {
            plane_grid(domain, &mut lines, style);
        }
        if show_coordinate_system {
            plane_axes(domain, &mut lines, style);
        }
    }
    lines
}

/// Reference lines for one selected slice, in the frame's local `(u, v, 0)`
/// coordinates: a flat grid and the two in-plane axes. The axes take the
/// world-axis colour of `frame.right` and `frame.up`, so an X slice shows the
/// Z and Y axes while a Z slice shows X and Y. Deliberately no cube: the slice
/// scene is flat.
pub fn reference_plane_lines(
    frame: &PlaneFrame,
    show_grid: bool,
    show_coordinate_system: bool,
) -> Vec<LineInstanceGPU> {
    reference_plane_lines_with(
        frame,
        show_grid,
        show_coordinate_system,
        &ViewportStyle::default(),
    )
}

/// Reference lines for one selected slice drawn with `style`. The legacy
/// [`reference_plane_lines`] delegates here with the default style.
pub fn reference_plane_lines_with(
    frame: &PlaneFrame,
    show_grid: bool,
    show_coordinate_system: bool,
    style: &ViewportStyle,
) -> Vec<LineInstanceGPU> {
    let mut lines = Vec::new();
    let half_u = frame.half_u;
    let half_v = frame.half_v;
    if show_grid {
        for k in 0..=REFERENCE_DIVISIONS {
            let v = lerp(-half_v, half_v, k);
            push(
                &mut lines,
                Vec3::new(-half_u, v, 0.0),
                Vec3::new(half_u, v, 0.0),
                style.grid_color,
                style.grid_width,
            );
            let u = lerp(-half_u, half_u, k);
            push(
                &mut lines,
                Vec3::new(u, -half_v, 0.0),
                Vec3::new(u, half_v, 0.0),
                style.grid_color,
                style.grid_width,
            );
        }
    }
    if show_coordinate_system {
        push(
            &mut lines,
            Vec3::new(-half_u, 0.0, 0.0),
            Vec3::new(half_u, 0.0, 0.0),
            world_axis_color(frame.right, style),
            style.axis_width,
        );
        push(
            &mut lines,
            Vec3::new(0.0, -half_v, 0.0),
            Vec3::new(0.0, half_v, 0.0),
            world_axis_color(frame.up, style),
            style.axis_width,
        );
    }
    lines
}

/// The colour of the world axis a frame basis vector points along, ignoring
/// the sign (the X and Y slice bases point along `-Z` and `-X`).
fn world_axis_color(basis: Vec3, style: &ViewportStyle) -> [f32; 4] {
    let abs = basis.abs();
    if abs.x >= abs.y && abs.x >= abs.z {
        style.axis_x_color
    } else if abs.y >= abs.z {
        style.axis_y_color
    } else {
        style.axis_z_color
    }
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

fn cube_grid_and_box(domain: &Domain, lines: &mut Vec<LineInstanceGPU>, style: &ViewportStyle) {
    let min = domain.min;
    let max = domain.max;

    // Light grids on the three far faces, drawn first so the thicker box
    // edges and axes land on top where they coincide. Each face gets one line
    // per subdivision along each of its two in-plane directions.
    for k in 0..=REFERENCE_DIVISIONS {
        let t = lerp(min.y, max.y, k);
        push(
            lines,
            Vec3::new(min.x, t, min.z),
            Vec3::new(min.x, t, max.z),
            style.grid_color,
            style.grid_width,
        );
        let t = lerp(min.z, max.z, k);
        push(
            lines,
            Vec3::new(min.x, min.y, t),
            Vec3::new(min.x, max.y, t),
            style.grid_color,
            style.grid_width,
        );

        let t = lerp(min.x, max.x, k);
        push(
            lines,
            Vec3::new(t, min.y, min.z),
            Vec3::new(t, min.y, max.z),
            style.grid_color,
            style.grid_width,
        );
        let t = lerp(min.z, max.z, k);
        push(
            lines,
            Vec3::new(min.x, min.y, t),
            Vec3::new(max.x, min.y, t),
            style.grid_color,
            style.grid_width,
        );

        let t = lerp(min.x, max.x, k);
        push(
            lines,
            Vec3::new(t, min.y, min.z),
            Vec3::new(t, max.y, min.z),
            style.grid_color,
            style.grid_width,
        );
        let t = lerp(min.y, max.y, k);
        push(
            lines,
            Vec3::new(min.x, t, min.z),
            Vec3::new(max.x, t, min.z),
            style.grid_color,
            style.grid_width,
        );
    }

    // 12 box edges, after the face grid so they overdraw the thinner grid
    // lines they share.
    for y in [min.y, max.y] {
        for z in [min.z, max.z] {
            push(
                lines,
                Vec3::new(min.x, y, z),
                Vec3::new(max.x, y, z),
                style.box_color,
                style.box_width,
            );
        }
    }
    for x in [min.x, max.x] {
        for z in [min.z, max.z] {
            push(
                lines,
                Vec3::new(x, min.y, z),
                Vec3::new(x, max.y, z),
                style.box_color,
                style.box_width,
            );
        }
    }
    for x in [min.x, max.x] {
        for y in [min.y, max.y] {
            push(
                lines,
                Vec3::new(x, y, min.z),
                Vec3::new(x, y, max.z),
                style.box_color,
                style.box_width,
            );
        }
    }
}

fn cube_axes(domain: &Domain, lines: &mut Vec<LineInstanceGPU>, style: &ViewportStyle) {
    let min = domain.min;
    let max = domain.max;
    let centre = (min + max) * 0.5;
    push(
        lines,
        Vec3::new(min.x, centre.y, centre.z),
        Vec3::new(max.x, centre.y, centre.z),
        style.axis_x_color,
        style.axis_width,
    );
    push(
        lines,
        Vec3::new(centre.x, min.y, centre.z),
        Vec3::new(centre.x, max.y, centre.z),
        style.axis_y_color,
        style.axis_width,
    );
    push(
        lines,
        Vec3::new(centre.x, centre.y, min.z),
        Vec3::new(centre.x, centre.y, max.z),
        style.axis_z_color,
        style.axis_width,
    );
}

fn plane_grid(domain: &Domain, lines: &mut Vec<LineInstanceGPU>, style: &ViewportStyle) {
    let min = domain.min;
    let max = domain.max;
    let z = (min.z + max.z) * 0.5;

    for k in 0..=REFERENCE_DIVISIONS {
        let y = lerp(min.y, max.y, k);
        push(
            lines,
            Vec3::new(min.x, y, z),
            Vec3::new(max.x, y, z),
            style.grid_color,
            style.grid_width,
        );
        let x = lerp(min.x, max.x, k);
        push(
            lines,
            Vec3::new(x, min.y, z),
            Vec3::new(x, max.y, z),
            style.grid_color,
            style.grid_width,
        );
    }
}

fn plane_axes(domain: &Domain, lines: &mut Vec<LineInstanceGPU>, style: &ViewportStyle) {
    let min = domain.min;
    let max = domain.max;
    let z = (min.z + max.z) * 0.5;
    let centre = (min + max) * 0.5;
    push(
        lines,
        Vec3::new(min.x, centre.y, z),
        Vec3::new(max.x, centre.y, z),
        style.axis_x_color,
        style.axis_width,
    );
    push(
        lines,
        Vec3::new(centre.x, min.y, z),
        Vec3::new(centre.x, max.y, z),
        style.axis_y_color,
        style.axis_width,
    );
}
