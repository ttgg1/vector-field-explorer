//! Task 2: projected slice sampling, local references, fit and click.
//!
//! The slice scene is flat and local: positions are `(u, v, 0)` in the
//! selected plane's frame, directions are the field projected onto that frame,
//! and the camera/click seam receives the local domain rather than the world
//! box.

use glam::{UVec3, Vec2, Vec3};
use vector_field_explorer::camera::{OrbitCamera, ViewCamera};
use vector_field_explorer::domain::Domain;
use vector_field_explorer::field::FieldExpr;
use vector_field_explorer::plane::{PlaneAxis, PlaneFrame, PlaneView};
use vector_field_explorer::reference::{AXIS_Y_COLOR, AXIS_Z_COLOR, reference_plane_lines};
use vector_field_explorer::sampling::{ViewOptions, sample_plane_at};
use vector_field_explorer::streamlines::click_seed;
use vector_field_explorer::view_command::{ViewCommand, ViewContext, apply_view_command};
use vector_field_explorer::viewmodel::PLANE_MARGIN;

#[test]
fn slice_sampling_projects_onto_plane_basis() {
    let field = FieldExpr::parse("1, 2, 3").unwrap();
    let domain = Domain::new(Vec3::splat(-1.0), Vec3::splat(1.0));
    let frame = PlaneFrame::new(
        &domain,
        PlaneView {
            axis: PlaneAxis::Z,
            index: 1,
            ..Default::default()
        },
        UVec3::splat(3),
    );
    let arrows = sample_plane_at(&field, &frame, 3, &ViewOptions::default(), 0.0);
    assert!(!arrows.is_empty());
    assert!(arrows.iter().all(|a| a.position.z == 0.0));
    assert!((arrows[0].mag - 5f32.sqrt()).abs() < 1e-5);
    assert!(arrows[0].dir.z == 0.0);
    assert!((arrows[0].dir.x - 1.0 / 5f32.sqrt()).abs() < 1e-5);
}

#[test]
fn slice_sampling_omits_a_purely_normal_field() {
    let field = FieldExpr::parse("0, 0, 1").unwrap();
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
    assert!(sample_plane_at(&field, &frame, 9, &ViewOptions::default(), 0.0).is_empty());
}

#[test]
fn plane_reference_axes_use_world_axis_colors() {
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
    let lines = reference_plane_lines(&frame, true, true);
    assert!(lines.iter().any(|line| line.color == AXIS_Z_COLOR));
    assert!(lines.iter().any(|line| line.color == AXIS_Y_COLOR));
    assert!(
        lines
            .iter()
            .all(|line| line.a[2].abs() < 1e-6 && line.b[2].abs() < 1e-6)
    );
}

#[test]
fn plane_fit_uses_local_extents() {
    let domain = Domain::new(Vec3::new(-10.0, -2.0, -1.0), Vec3::new(10.0, 2.0, 1.0));
    let frame = PlaneFrame::new(
        &domain,
        PlaneView {
            axis: PlaneAxis::X,
            index: 2,
            ..Default::default()
        },
        UVec3::splat(5),
    );
    let view = ViewContext::new(false, frame.local_domain(), 1.0);
    let camera = apply_view_command(OrbitCamera::default(), view, ViewCommand::Fit);
    assert!(camera.ortho_zoom > 0.0);
    assert!(camera.target.x.abs() < 1e-6);
    assert!(camera.target.y.abs() < 1e-6);
    assert!(camera.target.z.abs() < 1e-6);
}

#[test]
fn plane_click_lands_on_the_local_slice() {
    let domain = Domain::new(Vec3::new(-5.0, -2.0, -2.0), Vec3::new(5.0, 2.0, 2.0));
    let frame = PlaneFrame::new(
        &domain,
        PlaneView {
            axis: PlaneAxis::X,
            index: 3,
            ..Default::default()
        },
        UVec3::splat(5),
    );
    let bounds = frame.local_domain();
    let camera = OrbitCamera::default();
    let seed = click_seed(
        ViewCamera::Orbit(&camera),
        &bounds,
        1.0,
        Vec2::ZERO,
        Some(bounds.domain_size().x.max(bounds.domain_size().y) * PLANE_MARGIN),
    )
    .expect("a centred click must hit the local plane");
    assert!(seed.z.abs() < 1e-5);
    assert!(bounds.min.x <= seed.x && seed.x <= bounds.max.x);
    assert!(bounds.min.y <= seed.y && seed.y <= bounds.max.y);
}
