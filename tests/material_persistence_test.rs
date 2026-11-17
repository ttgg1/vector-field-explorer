//! Task 4 Materials persistence: full field round-trip, legacy defaults,
//! malformed fallback, restore without sampling, reset ownership and the
//! restored-panel draft sync.

use vector_field_explorer::VfApp;
use vector_field_explorer::material::{MaterialCase, MaterialScene, MaterialView};
use vector_field_explorer::persistence::{AppSettings, ViewRecord, save};
use vector_field_explorer::plane::PlaneAxis;
use vector_field_explorer::source_scene::FieldMode;
use vector_field_explorer::ui::sidebar::Controls;
use vector_field_explorer::viewmodel::State;

use crate::support::storage::MemoryStorage;

fn custom_scene() -> MaterialScene {
    MaterialScene {
        case: MaterialCase::PermeableBoundary,
        background_epsilon_r: 2.5,
        background_mu_r: 1.25,
        material_epsilon_r: 7.5,
        material_mu_r: 3.5,
        radius: 1.75,
        interface_axis: PlaneAxis::X,
        interface_offset: -0.5,
        applied: [0.25, -1.5, 2.0],
        view: MaterialView::Response,
    }
}

#[test]
fn material_scene_round_trips_every_field_without_sampling() {
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Materials);
    let scene = custom_scene();
    state.set_material_scene(scene.clone()).unwrap();

    let record = ViewRecord::capture(&state);
    assert_eq!(record.material_case, "permeable_boundary");
    assert_eq!(record.material_background_epsilon_r, 2.5);
    assert_eq!(record.material_background_mu_r, 1.25);
    assert_eq!(record.material_material_epsilon_r, 7.5);
    assert_eq!(record.material_material_mu_r, 3.5);
    assert_eq!(record.material_radius, 1.75);
    assert_eq!(record.material_interface_axis, "x");
    assert_eq!(record.material_interface_offset, -0.5);
    assert_eq!(record.material_applied, [0.25, -1.5, 2.0]);
    assert_eq!(record.material_view, "response");

    let json = serde_json::to_string(&record).unwrap();
    let back: ViewRecord = serde_json::from_str(&json).unwrap();
    let mut restored = State::new(false);
    back.restore(&mut restored, &mut Controls::new());

    assert_eq!(restored.field_mode, FieldMode::Materials);
    assert_eq!(restored.material, scene);
    assert!(
        restored.material_sampling.is_none(),
        "restore never samples synchronously",
    );
    assert!(restored.material_error.is_none());
}

#[test]
fn absent_material_fields_default_to_the_dielectric_sphere_without_promotion() {
    let legacy: ViewRecord = serde_json::from_str("{}").unwrap();
    let mut state = State::new(false);
    let error = legacy.restore_material_fields(&mut state);

    assert!(error.is_none());
    assert_eq!(state.material, MaterialScene::default());
    assert!(state.material_sampling.is_none());
    assert!(state.material_error.is_none());
    assert_eq!(state.field_mode, FieldMode::Formula);

    legacy.restore(&mut state, &mut Controls::new());
    assert_eq!(state.field_mode, FieldMode::Formula);
    assert_eq!(state.material, MaterialScene::default());
    assert!(state.material_error.is_none());
}

#[test]
fn malformed_material_fields_fall_back_and_report_only_for_stored_materials_mode() {
    let text = r#"{
        "field_mode": "Materials",
        "material_case": "bogus_case",
        "material_background_epsilon_r": 0.5,
        "material_background_mu_r": 1.0,
        "material_material_epsilon_r": 4.0,
        "material_material_mu_r": 1.0,
        "material_radius": -2.0,
        "material_interface_axis": "w",
        "material_interface_offset": 0.0,
        "material_applied": [1.0, 0.0, 0.0],
        "material_view": "total"
    }"#;
    let record: ViewRecord = serde_json::from_str(text).unwrap();
    let mut state = State::new(true);
    record.restore(&mut state, &mut Controls::new());

    assert_eq!(state.material, MaterialScene::default());
    assert!(state.material_sampling.is_none());
    assert_eq!(state.field_mode, FieldMode::Formula);
    let error = state
        .material_error
        .as_deref()
        .expect("a stored Materials record reports its malformed payload");
    assert!(error.to_lowercase().contains("material"), "{error}");

    // The same broken payload without the stored Materials mode is silent and
    // must never promote Materials.
    let text = r#"{
        "material_case": "bogus_case",
        "material_radius": -2.0,
        "material_interface_axis": "w"
    }"#;
    let record: ViewRecord = serde_json::from_str(text).unwrap();
    let mut state = State::new(false);
    record.restore(&mut state, &mut Controls::new());
    assert_eq!(state.material, MaterialScene::default());
    assert_eq!(state.field_mode, FieldMode::Formula);
    assert!(state.material_error.is_none());
}

#[test]
fn restore_material_fields_validates_and_stores_without_sampling() {
    let mut state = State::new(false);
    state.set_field_mode(FieldMode::Formula);
    let record: ViewRecord = serde_json::from_value(serde_json::json!({
        "material_case": "permeable_sphere",
        "material_background_epsilon_r": 1.5,
        "material_background_mu_r": 2.0,
        "material_material_epsilon_r": 3.0,
        "material_material_mu_r": 12.0,
        "material_radius": 2.25,
        "material_interface_axis": "y",
        "material_interface_offset": 1.0,
        "material_applied": [0.0, 2.0, 0.0],
        "material_view": "applied"
    }))
    .unwrap();

    let error = record.restore_material_fields(&mut state);
    assert!(error.is_none());
    assert_eq!(state.field_mode, FieldMode::Formula, "no mode promotion");
    assert_eq!(state.material.case, MaterialCase::PermeableSphere);
    assert_eq!(state.material.radius, 2.25);
    assert_eq!(state.material.interface_axis, PlaneAxis::Y);
    assert_eq!(state.material.view, MaterialView::Applied);
    assert!(state.material_sampling.is_none());
    assert!(state.material_error.is_none());
}

#[test]
fn global_reset_clears_materials_and_returns_to_formula() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Materials);
    app.state.set_material_scene(custom_scene()).unwrap();
    app.test_harness().advance_simulation();
    assert!(app.state.material_sampling.is_some());

    let mut invalid = app.state.material.clone();
    invalid.material_epsilon_r = 0.5;
    assert!(app.state.set_material_scene(invalid).is_err());
    assert!(app.state.material_error.is_some());

    app.test_harness().reset_all();

    assert_eq!(app.state.field_mode, FieldMode::Formula);
    assert_eq!(app.state.material, MaterialScene::default());
    assert!(app.state.material_sampling.is_none());
    assert!(app.state.material_error.is_none());
}

#[test]
fn materials_section_reset_only_restores_the_default_scene() {
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Materials);
    state.set_material_scene(custom_scene()).unwrap();
    state.refresh_material_sampling();
    assert!(state.material_sampling.is_some());

    let sources_before = state.sources.clone();
    let induction_before = state.induction.clone();
    let ode_before = state.ode.clone();

    state.reset_material();

    assert_eq!(state.field_mode, FieldMode::Materials);
    assert_eq!(state.material, MaterialScene::default());
    assert!(state.material_sampling.is_none());
    assert!(state.material_error.is_none());
    assert_eq!(state.sources, sources_before);
    assert_eq!(state.induction, induction_before);
    assert_eq!(state.ode, ode_before);
}

#[test]
fn restored_materials_record_syncs_the_panel_draft() {
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Materials);
    let scene = MaterialScene {
        case: MaterialCase::PermeableSphere,
        radius: 2.25,
        applied: [0.0, 0.0, -2.0],
        view: MaterialView::Applied,
        ..MaterialScene::default()
    };
    state.set_material_scene(scene.clone()).unwrap();

    let mut storage = MemoryStorage::default();
    save(
        &mut storage,
        &AppSettings {
            view: ViewRecord::capture(&state),
            ..Default::default()
        },
    );

    let app = VfApp::new_with_storage(Some(&storage));
    assert_eq!(app.state.field_mode, FieldMode::Materials);
    assert_eq!(app.state.material, scene);
    assert_eq!(
        app.material.draft, scene,
        "the panel draft must not show the stale default after restore",
    );
    assert!(app.material.error.is_none());
}
