//! Task 3: projected slice flow and scene-aware streamline tracing.
//!
//! A slice is traced through the local plane frame, so positions stay on the
//! `z == 0` slice; world mode and ordinary two-component 2D keep their old
//! paths.

use glam::{UVec3, Vec3};
use vector_field_explorer::domain::Domain;
use vector_field_explorer::field::FieldExpr;
use vector_field_explorer::flow::{Flow, PlaneFlow};
use vector_field_explorer::plane::{PlaneAxis, PlaneFrame, PlaneView};
use vector_field_explorer::viewmodel::State;

#[test]
fn plane_flow_projects_out_of_plane_velocity() {
    let field = FieldExpr::parse("1, 2, 3").unwrap();
    let domain = Domain::new(Vec3::splat(-1.0), Vec3::splat(1.0));
    let frame = PlaneFrame::new(
        &domain,
        PlaneView {
            axis: PlaneAxis::Z,
            index: 2,
            ..Default::default()
        },
        UVec3::splat(5),
    );
    let flow = PlaneFlow {
        field: &field,
        frame,
    };
    let v = flow.eval(Vec3::ZERO, 0.0).unwrap();
    assert_eq!(v, Vec3::new(1.0, 2.0, 0.0));
}

#[test]
fn plane_streamlines_never_leave_the_slice() {
    let mut state = State::new(false);
    state.set_formula("y, -x, 1".into());
    state.set_streamlines(true);
    let lines = state.streamlines_snapshot(&state.sample());
    assert!(!lines.is_empty());
    assert!(
        lines
            .iter()
            .all(|line| line.a[2].abs() < 1e-5 && line.b[2].abs() < 1e-5)
    );
}

#[test]
fn click_streamlines_stay_on_the_slice() {
    let mut state = State::new(false);
    state.set_formula("y, -x, 1".into());
    state.set_streamlines(true);
    let range = state.color_range(&state.sample());
    let lines = state.click_streamlines_snapshot(range, &[Vec3::ZERO]);
    assert!(!lines.is_empty());
    assert!(
        lines
            .iter()
            .all(|line| line.a[2].abs() < 1e-5 && line.b[2].abs() < 1e-5)
    );
}

#[test]
fn world_streamlines_are_unchanged() {
    let mut state = State::new(true);
    state.set_formula("0, 0, 1".into());
    state.set_streamlines(true);
    let lines = state.streamlines_snapshot(&state.sample());
    assert!(!lines.is_empty());
    assert!(
        lines
            .iter()
            .any(|line| line.a[2].abs() > 1e-4 || line.b[2].abs() > 1e-4)
    );
}

#[test]
fn ordinary_2d_streamlines_seed_one_xy_layer() {
    let mut state = State::new(false);
    state.set_streamlines(true);
    let z = state.scene_domain().center().z;
    let lines = state.streamlines_snapshot(&state.sample());
    assert!(!lines.is_empty());
    assert!(
        lines
            .iter()
            .all(|line| (line.a[2] - z).abs() < 1e-5 && (line.b[2] - z).abs() < 1e-5)
    );
}
