//! Ticket 23: the domain-matched reference grid.
//!
//! `reference_lines` is app-side decoration (never counted as field geometry)
//! but it must follow the sampling domain: a wireframe cube with subdivided
//! back faces and coloured axes in 3D, and one flat grid on the mid-z sampling
//! plane in 2D. These tests pin the counts, bounds, axis colours and the plane
//! the flat grid sits on.

/// Issue #300: the object-aware line shader reads body depth through this role
/// tag in `LineInstanceGPU::a.w`. It is `pub(crate)` in the renderer, so the
/// integration test pins the packed ABI value directly (see shaders.line.wgsl).
const GRID_ROLE_TAG: f32 = -1.0;

const LINE_SHADER: &str = include_str!("../src/shaders.line.wgsl");

use glam::{UVec3, Vec3};
use vector_field_explorer::app::{
    compose_lines, compose_lines_with_frame, compose_lines_with_frame_style,
};
use vector_field_explorer::domain::Domain;
use vector_field_explorer::plane::{PlaneAxis, PlaneFrame, PlaneView};
use vector_field_explorer::reference::{
    AXIS_X_COLOR, AXIS_Y_COLOR, AXIS_Z_COLOR, EDGE_COLOR, GRID_COLOR, REFERENCE_DIVISIONS,
    reference_lines, reference_plane_lines, reference_plane_lines_with, reference_lines_with,
};
use vector_field_explorer::renderer::LineInstanceGPU;
use vector_field_explorer::viewport_style::ViewportStyle;
use vector_field_explorer::viewmodel::State;

const EPS: f32 = 1e-5;

fn endpoints(lines: &[LineInstanceGPU]) -> impl Iterator<Item = Vec3> + '_ {
    lines.iter().flat_map(|line| {
        [
            Vec3::new(line.a[0], line.a[1], line.a[2]),
            Vec3::new(line.b[0], line.b[1], line.b[2]),
        ]
    })
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
            let centred = (0..3)
                .filter(|i| *i != axis)
                .all(|i| close(lo[i], centre[i]) && close(hi[i], centre[i]));
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
    assert_eq!(
        REFERENCE_DIVISIONS, 10,
        "matches the old 21-line-per-axis density"
    );
}

#[test]
fn three_d_reference_stays_inside_the_domain_and_draws_every_bound() {
    let domain = Domain::new(Vec3::new(-2.0, 0.0, 1.0), Vec3::new(2.0, 3.0, 5.0));
    let lines = reference_lines(&domain, true);
    for p in endpoints(&lines) {
        assert!(
            p.x >= domain.min.x - EPS && p.x <= domain.max.x + EPS,
            "x bound: {p:?}"
        );
        assert!(
            p.y >= domain.min.y - EPS && p.y <= domain.max.y + EPS,
            "y bound: {p:?}"
        );
        assert!(
            p.z >= domain.min.z - EPS && p.z <= domain.max.z + EPS,
            "z bound: {p:?}"
        );
    }
    // A wireframe box: at least one line lies on each of the six bounds.
    let edge_lines: Vec<&LineInstanceGPU> = lines
        .iter()
        .filter(|line| same_color(line.color, EDGE_COLOR))
        .collect();
    assert_eq!(
        edge_lines.len(),
        12,
        "the box edges are the only edge-coloured lines"
    );
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
fn face_grid_is_drawn_before_the_box_edges_and_axes_last() {
    // Later lines overdraw earlier ones where they coincide, so the thin face
    // grid must come first, the thicker box edges on top of it, and the
    // coloured axes last.
    let domain = Domain::default();
    let lines = reference_lines(&domain, true);
    let last_grid = lines
        .iter()
        .rposition(|line| same_color(line.color, GRID_COLOR))
        .expect("the faces contribute grid lines");
    let first_edge = lines
        .iter()
        .position(|line| same_color(line.color, EDGE_COLOR))
        .expect("the cube contributes edge lines");
    assert!(
        last_grid < first_edge,
        "every face-grid line precedes the box edges"
    );
    let last_edge = lines
        .iter()
        .rposition(|line| same_color(line.color, EDGE_COLOR))
        .expect("the cube contributes edge lines");
    for (color, name) in [
        (AXIS_X_COLOR, "x"),
        (AXIS_Y_COLOR, "y"),
        (AXIS_Z_COLOR, "z"),
    ] {
        let axis = lines
            .iter()
            .position(|line| same_color(line.color, color))
            .unwrap_or_else(|| panic!("the {name} axis is drawn"));
        assert!(
            axis > last_edge,
            "the {name} axis is drawn after the box edges"
        );
    }

    let flat = reference_lines(&domain, false);
    let last_flat_grid = flat
        .iter()
        .rposition(|line| same_color(line.color, GRID_COLOR))
        .expect("the plane contributes grid lines");
    let first_flat_axis = flat
        .iter()
        .position(|line| same_color(line.color, AXIS_X_COLOR))
        .expect("the plane contributes axes");
    assert!(
        last_flat_grid < first_flat_axis,
        "the 2D grid precedes its axes"
    );
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
        assert!(
            same_color(axes[0].color, color),
            "the {name} axis carries its own colour"
        );
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
    assert_eq!(
        lines.len(),
        expected,
        "{} grid lines per direction + 2 axes",
        REFERENCE_DIVISIONS + 1
    );
    for p in endpoints(&lines) {
        assert!(
            close(p.z, 0.0),
            "the default domain's mid-z is 0, got {p:?}"
        );
        assert!(
            p.x >= domain.min.x - EPS && p.x <= domain.max.x + EPS,
            "x bound: {p:?}"
        );
        assert!(
            p.y >= domain.min.y - EPS && p.y <= domain.max.y + EPS,
            "y bound: {p:?}"
        );
    }
    let custom = Domain::new(Vec3::new(-1.0, -1.0, 1.0), Vec3::new(1.0, 1.0, 5.0));
    for p in endpoints(&reference_lines(&custom, false)) {
        assert!(
            close(p.z, 3.0),
            "the flat grid follows the domain's mid-z, got {p:?}"
        );
    }
}

#[test]
fn two_d_reference_has_coloured_x_and_y_axes_through_the_centre() {
    let domain = Domain::default();
    let lines = reference_lines(&domain, false);
    assert_eq!(
        axis_line(&lines, 0, &domain, AXIS_X_COLOR).len(),
        1,
        "one x axis"
    );
    assert_eq!(
        axis_line(&lines, 1, &domain, AXIS_Y_COLOR).len(),
        1,
        "one y axis"
    );
    assert!(same_color(
        axis_line(&lines, 0, &domain, AXIS_X_COLOR)[0].color,
        AXIS_X_COLOR
    ));
    assert!(same_color(
        axis_line(&lines, 1, &domain, AXIS_Y_COLOR)[0].color,
        AXIS_Y_COLOR
    ));
    assert!(
        !lines
            .iter()
            .any(|line| same_color(line.color, AXIS_Z_COLOR)),
        "a flat grid has no z axis",
    );
}

#[test]
fn two_d_reference_covers_the_whole_domain_grid() {
    // Every grid line runs from one bound to the other on its in-plane axis,
    // so the grid reaches every sampled coordinate.
    let domain = Domain::default();
    let lines = reference_lines(&domain, false);
    let grid_lines: Vec<&LineInstanceGPU> = lines
        .iter()
        .filter(|line| {
            !same_color(line.color, AXIS_X_COLOR) && !same_color(line.color, AXIS_Y_COLOR)
        })
        .collect();
    for k in 0..=REFERENCE_DIVISIONS {
        let t = -1.0 + 2.0 * k as f32 / REFERENCE_DIVISIONS as f32;
        assert!(
            grid_lines
                .iter()
                .any(|line| close(line.a[1], t) && close(line.b[1], t)),
            "a horizontal grid line sits at y = {t}",
        );
        assert!(
            grid_lines
                .iter()
                .any(|line| close(line.a[0], t) && close(line.b[0], t)),
            "a vertical grid line sits at x = {t}",
        );
    }
}

fn sentinel(color: [f32; 4], x: f32) -> LineInstanceGPU {
    LineInstanceGPU {
        a: [x, 12.0, 13.0, 1.0],
        b: [x + 1.0, 12.0, 13.0, 0.123],
        color,
    }
}

fn same_lines(actual: &[LineInstanceGPU], expected: &[LineInstanceGPU]) {
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected) {
        assert_eq!(actual.a, expected.a);
        assert_eq!(actual.b, expected.b);
        assert_eq!(actual.color, expected.color);
    }
}

#[test]
fn reference_visibility_composes_selected_geometry_before_unchanged_flow_and_clicks() {
    let domain = Domain::new(Vec3::new(-4.0, 2.0, 6.0), Vec3::new(8.0, 10.0, 14.0));
    let flow = [sentinel(AXIS_X_COLOR, 101.0), sentinel(GRID_COLOR, 103.0)];
    let clicks = [sentinel(GRID_COLOR, 105.0), sentinel(AXIS_X_COLOR, 107.0)];
    for (sampling_3d, grid_count, axes) in [
        (
            true,
            6 * (REFERENCE_DIVISIONS as usize + 1) + 12,
            &[AXIS_X_COLOR, AXIS_Y_COLOR, AXIS_Z_COLOR][..],
        ),
        (
            false,
            2 * (REFERENCE_DIVISIONS as usize + 1),
            &[AXIS_X_COLOR, AXIS_Y_COLOR][..],
        ),
    ] {
        for (show_grid, show_coordinate_system) in
            [(true, true), (true, false), (false, true), (false, false)]
        {
            let lines = compose_lines(
                &flow,
                &clicks,
                &domain,
                sampling_3d,
                show_grid,
                show_coordinate_system,
            );
            let reference_count = if show_grid { grid_count } else { 0 }
                + if show_coordinate_system {
                    axes.len()
                } else {
                    0
                };
            assert_eq!(lines.len(), reference_count + flow.len() + clicks.len());
            let reference = &lines[..reference_count];
            same_lines(&lines[reference_count..reference_count + flow.len()], &flow);
            same_lines(&lines[reference_count + flow.len()..], &clicks);
            if show_grid {
                let grid_lines = if sampling_3d {
                    grid_count - 12
                } else {
                    grid_count
                };
                assert!(
                    reference[..grid_lines]
                        .iter()
                        .all(|line| line.color == GRID_COLOR)
                );
                if sampling_3d {
                    assert!(
                        reference[grid_lines..grid_count]
                            .iter()
                            .all(|line| line.color == EDGE_COLOR)
                    );
                } else {
                    assert!(
                        reference[..grid_count]
                            .iter()
                            .all(|line| line.a[2] == 10.0 && line.b[2] == 10.0)
                    );
                }
            }
            if show_coordinate_system {
                for (line, color) in reference[reference_count - axes.len()..].iter().zip(axes) {
                    assert_eq!(line.color, *color);
                }
            }
            for line in reference {
                for point in [line.a, line.b] {
                    for (axis, coordinate) in point.iter().enumerate().take(3) {
                        assert!(*coordinate >= domain.min[axis] && *coordinate <= domain.max[axis]);
                    }
                }
            }
            if show_grid && show_coordinate_system {
                same_lines(reference, &reference_lines(&domain, sampling_3d));
            }
        }
    }
}

#[test]
fn reference_visibility_uses_effective_sampling_dimension_for_flat_formula_in_3d_mode() {
    let mut state = State::new(true);
    assert!(state.set_formula("(y, -x)".to_owned()));
    assert!(state.mode_3d);
    assert!(!state.is_3d());
    let lines = compose_lines(&[], &[], &state.domain, state.is_3d(), true, true);
    assert_eq!(lines.len(), 2 * (REFERENCE_DIVISIONS as usize + 1) + 2);
    assert!(
        lines
            .iter()
            .all(|line| line.a[2] == 0.0 && line.b[2] == 0.0)
    );
}

#[test]
fn reference_visibility_recomposition_preserves_real_grid_and_click_segments() {
    let mut state = State::new(false);
    state.set_streamlines(true);
    let flow = state
        .snapshot_if_dirty()
        .expect("initial grid snapshot")
        .lines;
    let click_seeds = [Vec3::ZERO];
    let clicks = state.click_streamlines_snapshot((0.0, 1.0), &click_seeds);
    assert!(
        !clicks.is_empty(),
        "a seeded click contributes field segments"
    );

    state.set_show_grid(false);
    state.set_show_coordinate_system(false);
    let hidden = compose_lines(
        &flow,
        &clicks,
        &state.domain,
        state.is_3d(),
        state.show_grid,
        state.show_coordinate_system,
    );
    assert_eq!(hidden.len(), flow.len() + clicks.len());
    same_lines(&hidden[..flow.len()], &flow);
    same_lines(&hidden[flow.len()..], &clicks);

    let undone = compose_lines(
        &flow,
        &[],
        &state.domain,
        state.is_3d(),
        state.show_grid,
        state.show_coordinate_system,
    );
    same_lines(&undone, &flow);

    state.set_show_coordinate_system(true);
    let rebuilt = state
        .snapshot_if_dirty()
        .expect("visibility dirties the scene");
    let visible = compose_lines(
        &rebuilt.lines,
        &[],
        &state.domain,
        state.is_3d(),
        state.show_grid,
        state.show_coordinate_system,
    );
    assert_eq!(visible.len(), rebuilt.lines.len() + 2);
    assert_eq!(visible[0].color, AXIS_X_COLOR);
    assert_eq!(visible[1].color, AXIS_Y_COLOR);
    same_lines(&visible[2..], &rebuilt.lines);
}

#[test]
fn plane_frame_composition_is_local_flat_and_keeps_flow_and_clicks_after_references() {
    let domain = Domain::new(Vec3::new(-2.0, -1.0, -3.0), Vec3::new(2.0, 1.0, 3.0));
    let frame = PlaneFrame::new(
        &domain,
        PlaneView {
            axis: PlaneAxis::X,
            index: 2,
            ..Default::default()
        },
        UVec3::splat(5),
    );
    let flow = [sentinel(AXIS_X_COLOR, 101.0), sentinel(GRID_COLOR, 103.0)];
    let clicks = [sentinel(GRID_COLOR, 105.0)];

    let lines = compose_lines_with_frame(&flow, &clicks, &domain, false, Some(frame), true, true);
    let reference_count = 2 * (REFERENCE_DIVISIONS as usize + 1) + 2;
    assert_eq!(lines.len(), reference_count + flow.len() + clicks.len());

    let reference = &lines[..reference_count];
    assert!(
        reference
            .iter()
            .all(|line| line.a[2].abs() < 1e-6 && line.b[2].abs() < 1e-6),
        "a plane-frame reference is flat in local z, never a cube"
    );
    assert!(
        reference.iter().all(|line| line.color == GRID_COLOR
            || line.color == AXIS_Z_COLOR
            || line.color == AXIS_Y_COLOR),
        "only the local grid and its two world-axis-coloured axes"
    );
    same_lines(&lines[reference_count..reference_count + flow.len()], &flow);
    same_lines(&lines[reference_count + flow.len()..], &clicks);
}

/// A non-default record whose axis colours are tagged by channel so an axis
/// swap is visible immediately.
fn styled() -> ViewportStyle {
    let mut style = ViewportStyle::dark_laboratory();
    style.background = [0.1, 0.2, 0.3];
    style.grid_color = [0.14, 0.24, 0.34, 1.0];
    style.box_color = [0.44, 0.54, 0.64, 1.0];
    style.axis_x_color = [0.91, 0.11, 0.12, 1.0];
    style.axis_y_color = [0.11, 0.92, 0.13, 1.0];
    style.axis_z_color = [0.12, 0.13, 0.93, 1.0];
    style.grid_width = 0.004;
    style.box_width = 0.01;
    style.axis_width = 0.02;
    style
}

#[test]
fn styled_axes_follow_world_plane() {
    let domain = Domain::new(Vec3::new(-2.0, -1.0, -3.0), Vec3::new(2.0, 1.0, 3.0));
    let style = styled();

    let cube = reference_lines_with(&domain, true, &style);
    for (axis, color, name) in [
        (0usize, style.axis_x_color, "x"),
        (1, style.axis_y_color, "y"),
        (2, style.axis_z_color, "z"),
    ] {
        let axes = axis_line(&cube, axis, &domain, color);
        assert_eq!(axes.len(), 1, "exactly one styled {name} axis");
        assert_eq!(
            axes[0].b[3], style.axis_width,
            "the {name} axis takes the styled width"
        );
    }
    assert!(
        cube.iter()
            .filter(|line| same_color(line.color, style.box_color))
            .count()
            == 12,
        "the box takes the styled colour"
    );

    // X/Y/Z slices rotate the in-plane basis; each axis keeps the world-axis
    // colour of its basis vector, and the grid takes the styled colour/width.
    for (axis, expected_right, expected_up) in [
        (PlaneAxis::Z, style.axis_x_color, style.axis_y_color),
        (PlaneAxis::X, style.axis_z_color, style.axis_y_color),
        (PlaneAxis::Y, style.axis_x_color, style.axis_z_color),
    ] {
        let frame = PlaneFrame::new(
            &domain,
            PlaneView {
                axis,
                index: 2,
                ..Default::default()
            },
            UVec3::splat(5),
        );
        let lines = reference_plane_lines_with(&frame, true, true, &style);
        let axis_colors: Vec<[f32; 4]> = lines
            .iter()
            .filter(|line| !same_color(line.color, style.grid_color))
            .map(|line| line.color)
            .collect();
        assert_eq!(
            axis_colors,
            vec![expected_right, expected_up],
            "{axis:?} slice keeps the world-axis colour of its basis vectors"
        );
        for line in &lines {
            if same_color(line.color, style.grid_color) {
                assert_eq!(line.b[3], style.grid_width, "grid width follows the style");
            } else {
                assert_eq!(line.b[3], style.axis_width, "axis width follows the style");
            }
        }
    }
}

#[test]
fn default_style_is_byte_identical_to_legacy() {
    let domain = Domain::new(Vec3::new(-2.0, -1.0, -3.0), Vec3::new(2.0, 1.0, 3.0));
    let default = ViewportStyle::default();
    for sampling_3d in [true, false] {
        same_lines(
            &reference_lines(&domain, sampling_3d),
            &reference_lines_with(&domain, sampling_3d, &default),
        );
    }

    for axis in [PlaneAxis::X, PlaneAxis::Y, PlaneAxis::Z] {
        let frame = PlaneFrame::new(
            &domain,
            PlaneView {
                axis,
                index: 2,
                ..Default::default()
            },
            UVec3::splat(5),
        );
        same_lines(
            &reference_plane_lines(&frame, true, true),
            &reference_plane_lines_with(&frame, true, true, &default),
        );
    }
}

// --- Issue #84 A2a: the Reference layer scales widths and alpha ------------

#[test]
fn reference_width_and_opacity_apply() {
    use vector_field_explorer::layer_style::{LayerId, LayerOverride};

    let mut state = State::new(false);
    let base = state.viewport_style;
    let mut styles = *state.layer_styles();
    let reference = LayerId::ALL
        .iter()
        .position(|layer| *layer == LayerId::Reference)
        .expect("Reference is listed");
    styles.layers[reference] = LayerOverride {
        opacity: Some(0.5),
        width: Some(2.0),
        color: None,
    };
    state
        .commit_layer_styles(styles)
        .expect("the reference style is valid");

    let style = state.reference_viewport_style();
    assert_eq!(style.background, base.background, "background is never scaled");
    assert_eq!(style.grid_width, base.grid_width * 2.0);
    assert_eq!(style.box_width, base.box_width * 2.0);
    assert_eq!(style.axis_width, base.axis_width * 2.0);
    for color in [
        style.grid_color,
        style.box_color,
        style.axis_x_color,
        style.axis_y_color,
        style.axis_z_color,
    ] {
        assert_eq!(color[3], 0.5, "every reference alpha is halved");
    }
    assert_eq!(style.grid_color[..3], base.grid_color[..3], "RGB untouched");

    let lines = compose_lines_with_frame_style(
        &[],
        &[],
        &state.scene_domain(),
        false,
        state.plane_frame(),
        true,
        true,
        &style,
    );
    assert!(!lines.is_empty(), "the flat grid and axes are painted");
    let grid = base.grid_width * 2.0;
    let axis = base.axis_width * 2.0;
    for line in &lines {
        assert!(
            close(line.b[3], grid) || close(line.b[3], axis),
            "width is the scaled grid or axis width, got {}",
            line.b[3]
        );
        assert!(close(line.color[3], 0.5), "emitted alpha is halved");
    }

    let neutral = reference_lines(&Domain::default(), false);
    for line in &neutral {
        assert_eq!(line.color[3], 1.0, "the legacy default stays opaque");
    }
}

// --- Issue #300: only face-grid lines are object-occluded -------------------

fn tagged(line: &LineInstanceGPU) -> bool {
    line.a[3] == GRID_ROLE_TAG
}

#[test]
fn only_face_grid_lines_carry_the_grid_role_tag() {
    // The object-aware line pipeline depth-tests every GRID_TAG line against
    // the solid-body depth attachment. Scene-boundary edges are not face grid:
    // tagging them would hide the domain box behind solid bodies.
    let domain = Domain::default();
    let lines = reference_lines(&domain, true);
    let face_grid: Vec<_> = lines
        .iter()
        .filter(|line| same_color(line.color, GRID_COLOR))
        .collect();
    let box_edges: Vec<_> = lines
        .iter()
        .filter(|line| same_color(line.color, EDGE_COLOR))
        .collect();
    let axes: Vec<_> = lines
        .iter()
        .filter(|line| {
            same_color(line.color, AXIS_X_COLOR)
                || same_color(line.color, AXIS_Y_COLOR)
                || same_color(line.color, AXIS_Z_COLOR)
        })
        .collect();

    assert_eq!(face_grid.len(), 6 * (REFERENCE_DIVISIONS as usize + 1));
    assert_eq!(box_edges.len(), 12);
    assert_eq!(axes.len(), 3);
    assert!(
        face_grid.iter().all(|line| tagged(line)),
        "face-grid lines are depth-tested against objects"
    );
    assert!(
        box_edges.iter().all(|line| !tagged(line)),
        "domain-box edges keep the visible painter behaviour"
    );
    assert!(
        axes.iter().all(|line| !tagged(line)),
        "axes are never object-occluded grid lines"
    );
}

#[test]
fn flat_reference_grid_keeps_its_role_tag_and_axes_stay_painter_lines() {
    let domain = Domain::default();
    let lines = reference_lines(&domain, false);
    let grid: Vec<_> = lines
        .iter()
        .filter(|line| same_color(line.color, GRID_COLOR))
        .collect();
    let axes: Vec<_> = lines
        .iter()
        .filter(|line| same_color(line.color, AXIS_X_COLOR) || same_color(line.color, AXIS_Y_COLOR))
        .collect();
    assert_eq!(grid.len(), 2 * (REFERENCE_DIVISIONS as usize + 1));
    assert_eq!(axes.len(), 2);
    assert!(grid.iter().all(|line| tagged(line)));
    assert!(axes.iter().all(|line| !tagged(line)));
}

/// Object depth must be read with a NEAREST non-comparison sampler. wgpu 30's
/// GLES/WebGL2 backend returns 0 from `textureLoad` on a depth texture (issue
/// gfx-rs/wgpu#10878), so the packed role tag must be sampled portably instead.
#[test]
fn grid_depth_is_read_with_a_portable_nearest_sampler() {
    assert!(
        !LINE_SHADER.contains("textureLoad(body_depth"),
        "depth-texture textureLoad is unsupported on GLES/WebGL2"
    );
    assert!(
        LINE_SHADER.contains("textureSampleLevel(body_depth"),
        "grid depth must be read through a NEAREST non-comparison sampler"
    );
    assert!(
        LINE_SHADER.contains("@group(1) @binding(1) var body_depth_sampler: sampler;"),
        "the grid depth bind group needs a separate non-comparison sampler binding"
    );
}
