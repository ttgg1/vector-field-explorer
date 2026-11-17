//! Selected-slice source-glyph composition (issue 26 / #55).
//!
//! The pure glyph model is pinned by `source_glyph_test`; these tests cover
//! the app seam: glyphs join the composed reference/flow/click lines on the
//! current plane, and only while the source adapter is active.

use glam::{DVec3, UVec3, Vec3};
use vector_field_explorer::VfApp;
use vector_field_explorer::domain::Domain;
use vector_field_explorer::plane::{PlaneAxis, PlaneFrame, PlaneView};
use vector_field_explorer::sampling::ViewOptions;
use vector_field_explorer::source_glyph::source_glyphs;
use vector_field_explorer::source_scene::{
    FieldMode, SourceEntry, SourceKindTag, SourceScene, sample_source_scene,
};
use vector_field_explorer::sources::Source;

fn plane(domain: &Domain, index: u32) -> PlaneFrame {
    PlaneFrame::new(
        domain,
        PlaneView {
            enabled: true,
            axis: PlaneAxis::Z,
            index,
        },
        UVec3::new(5, 5, 5),
    )
}

fn one_point(z: f64) -> SourceScene {
    let mut scene = SourceScene::default();
    scene.entries.push(SourceEntry {
        enabled: true,
        source: Source {
            // Offset in x so the projected field is nonzero even at the plane
            // centre directly above the source; a centred charge's normal
            // component would otherwise be dropped by the magnitude guard.
            center: DVec3::new(0.25, 0.0, z),
            axis: DVec3::Z,
            kind: SourceKindTag::PointCharge.default_source().kind,
        },
    });
    scene
}

#[test]
fn slice_sampling_matches_the_3d_samples_projected_onto_the_plane() {
    let domain = Domain::new(Vec3::splat(-2.0), Vec3::splat(2.0));
    let frame = plane(&domain, 2);
    let scene = one_point(0.5);
    let view = ViewOptions::default();
    let world = sample_source_scene(&scene, &domain, UVec3::new(3, 3, 3), None, &view);
    let slice = sample_source_scene(&scene, &domain, UVec3::new(3, 3, 3), Some(&frame), &view);
    assert_eq!(slice.arrows.len(), 9);
    for arrow in &slice.arrows {
        let world_point = frame.to_world(arrow.position);
        let expected = world
            .arrows
            .iter()
            .find(|candidate| (candidate.position - world_point).length() < 1e-5)
            .expect("same sample point exists in the 3D pass");
        // The 3D pass publishes a normalized direction plus a separate
        // magnitude, so rebuild the unnormalized vector before projecting it.
        let projected = frame.project(expected.dir * expected.mag);
        assert!((arrow.dir - projected.normalize()).length() < 1e-4);
        assert!((arrow.mag - projected.length()).abs() < 1e-4);
    }
}

#[test]
fn plane_change_updates_glyphs_and_samples_together() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Sources);
    app.state.set_mode(false);
    // Put the source exactly on the index-4 slice so the on-slice case is
    // exercised independently of the app's default density and domain.
    app.state.set_plane_index(4);
    let on_plane = app
        .state
        .plane_frame()
        .expect("a 2D source slice has a plane frame");
    let z = f64::from(on_plane.origin.z);
    app.state.set_source_scene(one_point(z));
    app.state.set_plane_index(0);
    app.test_harness().advance_simulation();
    let off_lines = app.test_harness().source_glyph_line_count();
    let off_arrows = app.state.source_sampling().unwrap().arrows.clone();
    assert!(off_lines > 0, "off-slice glyphs still draw dashed lines");
    assert!(!off_arrows.is_empty());

    app.state.set_plane_index(4);
    app.test_harness().advance_simulation();
    let on_lines = app.test_harness().source_glyph_line_count();
    let on_arrows = app.state.source_sampling().unwrap().arrows.clone();
    assert!(on_lines > 0, "on-slice glyphs draw solid lines");
    assert!(app.test_harness().viewport_line_count() >= on_lines);
    assert!(!on_arrows.is_empty());
    assert_ne!(
        off_arrows[0].mag, on_arrows[0].mag,
        "samples are from the new plane"
    );

    let glyphs = source_glyphs(
        &app.state.sources,
        &app.state.scene_domain(),
        app.state.plane_frame().as_ref(),
    );
    assert_eq!(glyphs.len(), 1);
    assert!(
        !glyphs[0].off_slice,
        "the moved plane now carries the source glyph"
    );
}

#[test]
fn formula_mode_keeps_the_existing_scene_and_emits_no_source_glyphs() {
    let mut app = VfApp::new();
    app.state.set_source_scene(one_point(0.0));
    app.state.set_field_mode(FieldMode::Sources);
    app.test_harness().advance_simulation();
    assert!(app.test_harness().source_glyph_line_count() > 0);
    app.state.set_field_mode(FieldMode::Formula);
    app.test_harness().advance_simulation();
    assert_eq!(app.test_harness().source_glyph_line_count(), 0);
    assert!(app.state.source_sampling().is_none());
    assert!(
        !app.state.sources.entries.is_empty(),
        "switching adapters preserves the durable source scene"
    );
}
