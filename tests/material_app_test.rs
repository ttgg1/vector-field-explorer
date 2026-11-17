//! Task 2 Materials app integration: mode branch, state sampling, plane reuse,
//! arrows-only scenes and harness seams.

use glam::Vec3;

use vector_field_explorer::VfApp;
use vector_field_explorer::material::{
    MaterialCase, MaterialFamily, MaterialScene, MaterialView, material_field,
};
use vector_field_explorer::plane::{PlaneAxis, PlaneView};
use vector_field_explorer::source_scene::FieldMode;

#[test]
fn materials_mode_is_static_arrows_and_outline_only() {
    let mut app = VfApp::new();
    app.state.particles = true;
    app.state.set_field_mode(FieldMode::Materials);
    app.test_harness().advance_simulation();
    let scene = app.test_harness().test_assemble_viewport_scene();
    assert!(app.test_harness().material_arrow_count() > 0);
    assert!(app.test_harness().material_line_count() > 0);
    assert!(scene.trails.is_empty() && scene.dots.is_empty() && scene.combined.is_empty());
    assert!(!app.state.has_time_dependent_consumer());
    assert!(app.state.particle_target().is_none());
}

#[test]
fn materials_2d_sampling_is_a_projection_of_the_3d_case() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Materials);
    app.state.set_mode(true);
    app.test_harness().advance_simulation();
    let world = app.state.material_sampling.clone().unwrap();
    app.state.set_mode(false);
    app.test_harness().advance_simulation();
    let slice = app.state.material_sampling.clone().unwrap();
    assert!(!slice.arrows.is_empty());
    let frame = app
        .state
        .plane_frame()
        .expect("2D Materials has a plane frame");
    for arrow in &slice.arrows {
        assert!(arrow.position.z.abs() <= 1e-6);
        let world_point = frame.to_world(arrow.position);
        let projected = frame.project(material_field(&app.state.material, world_point).unwrap());
        assert!((arrow.dir - projected.normalize()).length() <= 1e-4);
        assert!((arrow.mag - projected.length()).abs() <= 1e-4);
    }
    assert_eq!(world.family, slice.family);
    assert_eq!(world.view, slice.view);
}

#[test]
fn material_view_switches_without_mixing_families() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Materials);
    for view in [
        MaterialView::Total,
        MaterialView::Applied,
        MaterialView::Response,
    ] {
        app.state
            .set_material_scene(MaterialScene {
                view,
                ..MaterialScene::default()
            })
            .unwrap();
        app.test_harness().advance_simulation();
        let sampling = app.state.material_sampling.as_ref().unwrap();
        assert_eq!(sampling.family, MaterialFamily::Electric);
        assert_eq!(sampling.view, view);
        assert!(sampling.arrows.iter().all(|a| a.color == [255, 176, 64]));
    }
}

#[test]
fn invalid_edit_retains_last_sampling_and_sets_error() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Materials);
    app.test_harness().advance_simulation();
    let before = app.state.material_sampling.clone().unwrap();
    let bad = MaterialScene {
        material_epsilon_r: f64::NAN,
        ..MaterialScene::default()
    };
    assert!(app.state.set_material_scene(bad).is_err());
    assert!(app.state.material_error.is_some());
    app.test_harness().advance_simulation();
    assert_eq!(
        app.state.material_sampling.as_ref().unwrap().arrows.len(),
        before.arrows.len()
    );
}

#[test]
fn material_edit_rebuilds_only_sampling_and_preserves_other_scenes() {
    let mut app = VfApp::new();
    app.state.set_formula("y, -x, 0".into());
    app.state.set_field_mode(FieldMode::Materials);
    app.test_harness().advance_simulation();
    let before = app.state.material_sampling.clone().unwrap();
    app.test_harness().advance_simulation();
    assert_eq!(app.state.material_sampling.as_ref().unwrap(), &before);
    let changed = MaterialScene {
        radius: 0.5,
        ..MaterialScene::default()
    };
    app.state.set_material_scene(changed).unwrap();
    app.test_harness().advance_simulation();
    assert_ne!(app.state.material_sampling.as_ref().unwrap(), &before);
    app.state.set_field_mode(FieldMode::Formula);
    assert_eq!(app.state.formula, "y, -x, 0");
}

#[test]
fn plane_change_and_reset_use_the_existing_frame_contract() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Materials);
    app.state.set_mode(false);
    app.state.set_plane(PlaneView {
        enabled: true,
        axis: PlaneAxis::Z,
        index: 0,
    });
    app.test_harness().advance_simulation();
    let first = app.state.material_sampling.clone().unwrap();
    app.state.set_plane_index(4);
    app.test_harness().advance_simulation();
    let second = app.state.material_sampling.clone().unwrap();
    assert_ne!(first.arrows, second.arrows);
    app.state.reset_material();
    assert_eq!(app.state.material, MaterialScene::default());
    assert!(app.state.material_sampling.is_none());
}

#[test]
fn material_outline_reaches_the_vector_scene_without_reference_lines() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Materials);
    app.state.set_mode(true);
    app.test_harness().advance_simulation();
    let outline = app.test_harness().material_line_count();
    assert!(outline > 0, "the sphere outline must reach the scene");
    assert!(app.test_harness().viewport_line_count() > outline);
}

#[test]
fn empty_domain_returns_a_visible_warning_rather_than_arrows() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Materials);
    app.state.set_mode(true);
    app.state.domain = vector_field_explorer::Domain::new(Vec3::ZERO, Vec3::ZERO);
    app.state.mark_dirty();
    app.test_harness().advance_simulation();
    assert_eq!(app.test_harness().material_arrow_count(), 0);
    let harness = app.test_harness();
    let error = harness.material_sampling_error().unwrap_or("");
    assert!(error.contains("produces no samples"), "{error:?}");
}

#[test]
fn materials_reset_and_mode_switch_preserve_other_modes() {
    let mut app = VfApp::new();
    app.state.set_formula("y, -x, 0".into());
    let formula = app.state.formula.clone();
    let sources = app.state.sources.clone();
    let induction = app.state.induction.clone();
    let ode = app.state.ode.clone();
    app.state.set_field_mode(FieldMode::Materials);
    app.test_harness().advance_simulation();
    assert_eq!(app.state.formula, formula);
    assert_eq!(app.state.sources, sources);
    assert_eq!(app.state.induction, induction);
    assert_eq!(app.state.ode, ode);
    app.state.reset_material();
    app.state.set_field_mode(FieldMode::Formula);
    assert_eq!(app.state.formula, formula);
    assert_eq!(app.state.sources, sources);
    assert_eq!(app.state.induction, induction);
    assert_eq!(app.state.ode, ode);
}

#[test]
fn magnetic_material_case_draws_only_the_magnetic_family() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Materials);
    app.state
        .set_material_scene(MaterialScene {
            case: MaterialCase::PermeableSphere,
            ..MaterialScene::default()
        })
        .unwrap();
    app.test_harness().advance_simulation();
    let sampling = app.state.material_sampling.as_ref().unwrap();
    assert_eq!(sampling.family, MaterialFamily::Magnetic);
    assert!(sampling.arrows.iter().all(|a| a.color == [64, 196, 255]));
    assert!(app.test_harness().material_scene().is_some());
}
