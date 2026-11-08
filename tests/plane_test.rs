//! Ticket 23: the pure plane-selection seam.
//!
//! `PlaneView` names one sampled slice of the cube; the app highlights the
//! arrows whose grid index on that axis matches it and dims the rest. These
//! tests pin the pure pieces: the plane coordinate, index-from-position
//! matching, clamping and the dim colour/alpha. No GPU and no window.

use glam::{UVec3, Vec3};
use vector_field_explorer::domain::Domain;
use vector_field_explorer::plane::{
    DIM_ALPHA, DIM_FACTOR, DIM_GREY, PlaneAxis, PlaneView, dim_color, on_plane, plane_coordinate,
};

const EPS: f32 = 1e-5;

fn grid(n: u32) -> UVec3 {
    UVec3::new(n, n, n)
}

#[test]
fn default_plane_is_z_and_disabled() {
    let plane = PlaneView::default();
    assert_eq!(plane.axis, PlaneAxis::Z, "the default axis is z");
    assert!(!plane.enabled, "plane-by-plane starts off");
    assert_eq!(PlaneAxis::ALL, [PlaneAxis::X, PlaneAxis::Y, PlaneAxis::Z]);
    assert_eq!(PlaneAxis::X.label(), "x");
    assert_eq!(PlaneAxis::Y.label(), "y");
    assert_eq!(PlaneAxis::Z.label(), "z");
}

#[test]
fn middle_plane_sits_at_the_middle_index() {
    let plane = PlaneView::middle(grid(24));
    assert_eq!(plane.axis, PlaneAxis::Z);
    assert!(!plane.enabled);
    assert_eq!(plane.index, 11, "(24 - 1) / 2 = 11");
    assert_eq!(PlaneAxis::X.middle_index(grid(5)), 2);
    assert_eq!(PlaneAxis::Y.middle_index(UVec3::new(5, 8, 3)), 3, "y count 8 -> index 3");
    assert_eq!(PlaneAxis::Z.middle_index(UVec3::new(5, 8, 1)), 0, "a one-slice axis sits at 0");
}

#[test]
fn axis_count_reads_the_grid_dimension() {
    let g = UVec3::new(6, 7, 8);
    assert_eq!(PlaneAxis::X.count(g), 6);
    assert_eq!(PlaneAxis::Y.count(g), 7);
    assert_eq!(PlaneAxis::Z.count(g), 8);
    let plane = PlaneView { enabled: true, axis: PlaneAxis::Y, index: 3 };
    assert_eq!(plane.count(g), 7);
    assert!(plane.active(g), "enabled with more than one slice is active");
    assert!(!plane.active(UVec3::new(6, 1, 8)), "a single-slice axis is inert");
    assert!(!PlaneView { enabled: false, ..plane }.active(g), "disabled is inert");
}

#[test]
fn plane_coordinate_matches_the_sampling_grid() {
    let domain = Domain::default();
    assert!((plane_coordinate(&domain, grid(3), PlaneAxis::Z, 0) - -1.0).abs() < EPS);
    assert!((plane_coordinate(&domain, grid(3), PlaneAxis::Z, 1) - 0.0).abs() < EPS);
    assert!((plane_coordinate(&domain, grid(3), PlaneAxis::Z, 2) - 1.0).abs() < EPS);
    assert!(
        (plane_coordinate(&domain, grid(3), PlaneAxis::X, 2) - 1.0).abs() < EPS,
        "the axis selects which dimension is read",
    );
    assert!(
        (plane_coordinate(&domain, UVec3::new(3, 3, 1), PlaneAxis::Z, 0) - 0.0).abs() < EPS,
        "a one-slice axis sits at the domain midpoint",
    );
    let custom = Domain::new(Vec3::new(-2.0, 0.0, 1.0), Vec3::new(2.0, 3.0, 5.0));
    assert!((plane_coordinate(&custom, grid(5), PlaneAxis::Z, 2) - 3.0).abs() < EPS);
    assert!((plane_coordinate(&custom, grid(5), PlaneAxis::X, 0) - -2.0).abs() < EPS);
    assert!((plane_coordinate(&custom, grid(5), PlaneAxis::X, 4) - 2.0).abs() < EPS);
}

#[test]
fn plane_coordinate_clamps_the_index() {
    let domain = Domain::default();
    assert!((plane_coordinate(&domain, grid(5), PlaneAxis::Z, 99) - 1.0).abs() < EPS);
}

#[test]
fn on_plane_matches_by_grid_index_not_float_equality() {
    let domain = Domain::default();
    let g = grid(5);
    let plane = PlaneView { enabled: true, axis: PlaneAxis::Z, index: 2 };
    let z = plane_coordinate(&domain, g, PlaneAxis::Z, 2);
    assert!(on_plane(Vec3::new(0.0, 0.0, z), &domain, g, &plane), "the exact sample is on the plane");
    assert!(
        on_plane(Vec3::new(0.2, -0.3, z + 1e-4), &domain, g, &plane),
        "a float-near sample still belongs to the same index",
    );
    let off = plane_coordinate(&domain, g, PlaneAxis::Z, 3);
    assert!(!on_plane(Vec3::new(0.0, 0.0, off), &domain, g, &plane), "the next sample is off-plane");
    let between = (z + off) * 0.5;
    assert!(
        !on_plane(Vec3::new(0.0, 0.0, between), &domain, g, &plane),
        "a point halfway between samples rounds to the next index, not this one",
    );
}

#[test]
fn on_plane_reads_the_axis_it_is_given() {
    let domain = Domain::default();
    let g = grid(5);
    let plane = PlaneView { enabled: true, axis: PlaneAxis::X, index: 0 };
    assert!(on_plane(Vec3::new(-1.0, 0.3, -0.2), &domain, g, &plane), "x = min is index 0");
    assert!(!on_plane(Vec3::new(0.0, 0.3, -0.2), &domain, g, &plane), "x = 0 is the middle index");
}

#[test]
fn a_one_slice_axis_puts_every_point_on_the_plane() {
    let domain = Domain::default();
    let g = UVec3::new(5, 5, 1);
    let plane = PlaneView { enabled: true, axis: PlaneAxis::Z, index: 0 };
    assert!(on_plane(Vec3::new(0.3, -0.4, 0.0), &domain, g, &plane));
    assert!(on_plane(Vec3::new(0.3, -0.4, 0.9), &domain, g, &plane));
}

#[test]
fn clamped_keeps_the_index_inside_the_axis() {
    let g = UVec3::new(5, 7, 9);
    let plane = PlaneView { enabled: true, axis: PlaneAxis::Y, index: 99 };
    assert_eq!(plane.clamped(g).index, 6, "clamped to the last y sample");
    let plane = PlaneView { enabled: true, axis: PlaneAxis::Y, index: 0 };
    assert_eq!(plane.clamped(g).index, 0);
    let flat = PlaneView { enabled: true, axis: PlaneAxis::Z, index: 4 };
    assert_eq!(flat.clamped(UVec3::new(5, 5, 1)).index, 0, "a one-slice axis collapses to 0");
}

#[test]
fn dim_color_lerps_three_quarters_toward_neutral_grey() {
    assert_eq!(DIM_GREY, 128.0);
    assert!((DIM_FACTOR - 0.75).abs() < EPS);
    assert_eq!(dim_color([255, 0, 0]), [160, 96, 96]);
    assert_eq!(dim_color([0, 255, 0]), [96, 160, 96]);
    assert_eq!(dim_color([128, 128, 128]), [128, 128, 128], "grey stays grey");
    assert_eq!(dim_color([0, 0, 0]), [96, 96, 96], "black lifts toward the grey");
}

#[test]
fn dim_alpha_is_translucent_but_visible() {
    assert!(
        (0.15..=0.25).contains(&DIM_ALPHA),
        "the dimmed pass targets alpha 0.15..=0.25, got {DIM_ALPHA}",
    );
}
