//! Ticket 23: the domain-matched reference grid.
//!
//! `reference_lines` is app-side decoration (never counted as field geometry)
//! but it must follow the sampling domain: a wireframe cube with subdivided
//! back faces and coloured axes in 3D, and one flat grid on the mid-z sampling
//! plane in 2D. These tests pin the counts, bounds, axis colours and the plane
//! the flat grid sits on.

use glam::Vec3;
use vector_field_explorer::domain::Domain;
use vector_field_explorer::reference::{
    AXIS_X_COLOR, AXIS_Y_COLOR, AXIS_Z_COLOR, EDGE_COLOR, REFERENCE_DIVISIONS, reference_lines,
};
use vector_field_explorer::renderer::LineInstanceGPU;

const EPS: f32 = 1e-5;

fn endpoints(lines: &[LineInstanceGPU]) -> impl Iterator<Item = Vec3> + '_ {
    lines
        .iter()
        .flat_map(|line| [Vec3::new(line.a[0], line.a[1], line.a[2]), Vec3::new(line.b[0], line.b[1], line.b[2])])
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() <= EPS
}

fn same_color(a: [f32; 4], b: [f32; 4]) -> bool {
    (0..4).all(|i| close(a[i], b[i]))
}

/// Lines that run from one domain bound to the other along `axis`, through the
/// domain centre on the other two axes.
fn axis_line<'a>(
    lines: &'a [LineInstanceGPU],
    axis: usize,
    domain: &Domain,
    color: [f32; 4],
) -> Vec<&'a LineInstanceGPU> {
    let centre = (domain.min + domain.max) * 0.5;
    lines
        .iter()
        .filter(|line| same_color(line.color, color))
        .filter(|line| {
            let a = Vec3::new(line.a[0], line.a[1], line.a[2]);
            let b = Vec3::new(line.b[0], line.b[1], line.b[2]);
            let mut lo = [0.0_f32; 3];
            let mut hi = [0.0_f32; 3];
            for i in 0..3 {
                lo[i] = a[i].min(b[i]);
                hi[i] = a[i].max(b[i]);
            }
            let spans = close(lo[axis], domain.min[axis]) && close(hi[axis], domain.max[axis]);
            let centred = (0..3).filter(|i| *i != axis).all(|i| {
                close(lo[i], centre[i]) && close(hi[i], centre[i])
            });
            spans && centred
        })
        .collect()
}

#[test]
fn three_d_reference_has_twelve_box_edges_subdivided_faces_and_three_axes() {
    let domain = Domain::default();
    let lines = reference_lines(&domain, true);
    let expected = 12 + 3 * 2 * (REFERENCE_DIVISIONS as usize + 1) + 3;
    assert_eq!(
        lines.len(),
        expected,
        "12 edges + 3 back faces x 2 directions x {} lines + 3 axes",
        REFERENCE_DIVISIONS as usize + 1,
    );
    assert_eq!(REFERENCE_DIVISIONS, 10, "matches the old 21-line-per-axis density");
}

#[test]
fn three_d_reference_stays_inside_the_domain_and_draws_every_bound() {
    let domain = Domain::new(Vec3::new(-2.0, 0.0, 1.0), Vec3::new(2.0, 3.0, 5.0));
    let lines = reference_lines(&domain, true);
    for p in endpoints(&lines) {
        assert!(p.x >= domain.min.x - EPS && p.x <= domain.max.x + EPS, "x bound: {p:?}");
        assert!(p.y >= domain.min.y - EPS && p.y <= domain.max.y + EPS, "y bound: {p:?}");
        assert!(p.z >= domain.min.z - EPS && p.z <= domain.max.z + EPS, "z bound: {p:?}");
    }
    // A wireframe box: at least one line lies on each of the six bounds.
    let edge_lines: Vec<&LineInstanceGPU> =
        lines.iter().filter(|line| same_color(line.color, EDGE_COLOR)).collect();
    assert_eq!(edge_lines.len(), 12, "the box edges are the only edge-coloured lines");
    for (axis, bound) in [
        (0, domain.min.x),
        (0, domain.max.x),
        (1, domain.min.y),
        (1, domain.max.y),
        (2, domain.min.z),
        (2, domain.max.z),
    ] {
        assert!(
            edge_lines.iter().any(|line| {
                let a = [line.a[0], line.a[1], line.a[2]];
                let b = [line.b[0], line.b[1], line.b[2]];
                close(a[axis], bound) && close(b[axis], bound)
            }),
            "a box edge lies on axis {axis} bound {bound}",
        );
    }
}

#[test]
fn three_d_reference_axes_are_coloured_and_centred() {
    let domain = Domain::default();
    let lines = reference_lines(&domain, true);
    for (axis, color, name) in [
        (0usize, AXIS_X_COLOR, "x"),
        (1, AXIS_Y_COLOR, "y"),
        (2, AXIS_Z_COLOR, "z"),
    ] {
        let axes = axis_line(&lines, axis, &domain, color);
        assert_eq!(axes.len(), 1, "exactly one {name} axis line");
        assert!(same_color(axes[0].color, color), "the {name} axis carries its own colour");
    }
    // Red-ish X, green-ish Y, blue-ish Z: the dominant channel is the axis.
    const {
        assert!(AXIS_X_COLOR[0] > AXIS_X_COLOR[1] && AXIS_X_COLOR[0] > AXIS_X_COLOR[2]);
        assert!(AXIS_Y_COLOR[1] > AXIS_Y_COLOR[0] && AXIS_Y_COLOR[1] > AXIS_Y_COLOR[2]);
        assert!(AXIS_Z_COLOR[2] > AXIS_Z_COLOR[0] && AXIS_Z_COLOR[2] > AXIS_Z_COLOR[1]);
    }
}

#[test]
fn two_d_reference_is_one_flat_grid_on_the_mid_z_plane() {
    let domain = Domain::default();
    let lines = reference_lines(&domain, false);
    let expected = 2 * (REFERENCE_DIVISIONS as usize + 1) + 2;
    assert_eq!(lines.len(), expected, "{} grid lines per direction + 2 axes", REFERENCE_DIVISIONS + 1);
    for p in endpoints(&lines) {
        assert!(close(p.z, 0.0), "the default domain's mid-z is 0, got {p:?}");
        assert!(p.x >= domain.min.x - EPS && p.x <= domain.max.x + EPS, "x bound: {p:?}");
        assert!(p.y >= domain.min.y - EPS && p.y <= domain.max.y + EPS, "y bound: {p:?}");
    }
    let custom = Domain::new(Vec3::new(-1.0, -1.0, 1.0), Vec3::new(1.0, 1.0, 5.0));
    for p in endpoints(&reference_lines(&custom, false)) {
        assert!(close(p.z, 3.0), "the flat grid follows the domain's mid-z, got {p:?}");
    }
}

#[test]
fn two_d_reference_has_coloured_x_and_y_axes_through_the_centre() {
    let domain = Domain::default();
    let lines = reference_lines(&domain, false);
    assert_eq!(axis_line(&lines, 0, &domain, AXIS_X_COLOR).len(), 1, "one x axis");
    assert_eq!(axis_line(&lines, 1, &domain, AXIS_Y_COLOR).len(), 1, "one y axis");
    assert!(same_color(axis_line(&lines, 0, &domain, AXIS_X_COLOR)[0].color, AXIS_X_COLOR));
    assert!(same_color(axis_line(&lines, 1, &domain, AXIS_Y_COLOR)[0].color, AXIS_Y_COLOR));
    assert!(
        !lines.iter().any(|line| same_color(line.color, AXIS_Z_COLOR)),
        "a flat grid has no z axis",
    );
}

#[test]
fn two_d_reference_covers_the_whole_domain_grid() {
    // Every grid line runs from one bound to the other on its in-plane axis,
    // so the grid reaches every sampled coordinate.
    let domain = Domain::default();
    let lines = reference_lines(&domain, false);
    let grid_lines: Vec<&LineInstanceGPU> =
        lines.iter().filter(|line| !same_color(line.color, AXIS_X_COLOR) && !same_color(line.color, AXIS_Y_COLOR)).collect();
    for k in 0..=REFERENCE_DIVISIONS {
        let t = -1.0 + 2.0 * k as f32 / REFERENCE_DIVISIONS as f32;
        assert!(
            grid_lines.iter().any(|line| close(line.a[1], t) && close(line.b[1], t)),
            "a horizontal grid line sits at y = {t}",
        );
        assert!(
            grid_lines.iter().any(|line| close(line.a[0], t) && close(line.b[0], t)),
            "a vertical grid line sits at x = {t}",
        );
    }
}
