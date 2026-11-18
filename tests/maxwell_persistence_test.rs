//! Task 5 Maxwell persistence: full durable-scene round-trip, legacy defaults,
//! malformed fallback without promotion, lazy restore without a BEM solve and
//! reset ownership.

use vector_field_explorer::VfApp;
use vector_field_explorer::maxwell::{ConductorOutline, MaxwellCase, MaxwellScene, MaxwellView};
use vector_field_explorer::persistence::{AppSettings, ViewRecord, save};
use vector_field_explorer::source_scene::FieldMode;
use vector_field_explorer::ui::sidebar::Controls;
use vector_field_explorer::viewmodel::State;

use crate::support::storage::MemoryStorage;

fn custom_scene() -> MaxwellScene {
    MaxwellScene {
        case: MaxwellCase::Conductor2D,
        e0: 2.5,
        b0: -1.25,
        separation: 0.75,
        radius: 2.5,
        outer_radius: 4.5,
        charge: 0.25,
        charge_height: 1.5,
        outline: ConductorOutline::Square { half_extent: 0.9 },
        applied: [0.0, 2.0],
        elements: 64,
        allow_inaccurate: true,
        view: MaxwellView::Field,
    }
}

#[test]
fn maxwell_scene_round_trips_every_field_without_sampling() {
    let mut state = State::new(false);
    state.set_field_mode(FieldMode::Maxwell);
    let scene = custom_scene();
    assert!(state.set_maxwell_scene(scene.clone()));

    let record = ViewRecord::capture(&state);
    assert_eq!(record.maxwell_case, "conductor_2d");
    assert_eq!(record.maxwell_e0, 2.5);
    assert_eq!(record.maxwell_b0, -1.25);
    assert_eq!(record.maxwell_separation, 0.75);
    assert_eq!(record.maxwell_radius, 2.5);
    assert_eq!(record.maxwell_outer_radius, 4.5);
    assert_eq!(record.maxwell_charge, 0.25);
    assert_eq!(record.maxwell_charge_height, 1.5);
    assert_eq!(record.maxwell_outline, "square:0.9");
    assert_eq!(record.maxwell_applied, [0.0, 2.0]);
    assert_eq!(record.maxwell_elements, 64);
    assert!(record.maxwell_allow_inaccurate);

    let json = serde_json::to_string(&record).unwrap();
    let back: ViewRecord = serde_json::from_str(&json).unwrap();
    let mut restored = State::new(true);
    back.restore(&mut restored, &mut Controls::new());

    assert_eq!(restored.field_mode, FieldMode::Maxwell);
    assert_eq!(restored.maxwell, scene);
    assert_eq!(restored.maxwell.view, scene.view);
    assert!(
        restored.maxwell_sampling.is_none(),
        "restore never samples synchronously",
    );
    assert!(restored.maxwell_error.is_none());
}

#[test]
fn restoring_a_conductor_scene_does_not_solve_bem() {
    let mut state = State::new(false);
    state.set_field_mode(FieldMode::Maxwell);
    assert!(state.set_maxwell_scene(custom_scene()));
    let mut storage = MemoryStorage::default();
    save(
        &mut storage,
        &AppSettings {
            view: ViewRecord::capture(&state),
            ..Default::default()
        },
    );

    let mut app = VfApp::new_with_storage(Some(&storage));
    assert_eq!(app.state.field_mode, FieldMode::Maxwell);
    assert_eq!(app.state.maxwell, custom_scene());
    assert!(
        app.state.maxwell_sampling.is_none(),
        "restore must be lazy: the first frame owns sampling and the BEM solve",
    );
    assert!(app.state.maxwell_error.is_none());
    assert_eq!(
        app.test_harness().maxwell_bem_solves(),
        0,
        "restore must not solve the 2D conductor BEM",
    );
}

#[test]
fn absent_maxwell_fields_default_without_promotion_or_error() {
    let legacy: ViewRecord = serde_json::from_str("{}").unwrap();
    let mut state = State::new(false);
    let error = legacy.restore_maxwell_fields(&mut state);

    assert!(error.is_none());
    assert_eq!(state.maxwell, MaxwellScene::default());
    assert!(state.maxwell_sampling.is_none());
    assert!(state.maxwell_error.is_none());
    assert_eq!(state.field_mode, FieldMode::Formula);

    legacy.restore(&mut state, &mut Controls::new());
    assert_eq!(state.field_mode, FieldMode::Formula);
    assert_eq!(state.maxwell, MaxwellScene::default());
    assert!(state.maxwell_error.is_none());
}

#[test]
fn malformed_maxwell_fields_fall_back_and_only_report_for_a_stored_mode() {
    let text = r#"{
        "field_mode": "Maxwell",
        "maxwell_case": "bogus_case",
        "maxwell_outline": "ellipse:1",
        "maxwell_elements": 31
    }"#;
    let record: ViewRecord = serde_json::from_str(text).unwrap();
    let mut state = State::new(true);
    let error = record
        .restore_maxwell_fields(&mut state)
        .expect("a malformed Maxwell payload reports its reason");
    assert!(error.to_lowercase().contains("maxwell"), "{error}");
    assert_eq!(state.maxwell, MaxwellScene::default());
    assert!(state.maxwell_sampling.is_none());
    assert_eq!(state.maxwell_error.as_deref(), Some(error.as_str()));
    assert_eq!(
        state.field_mode,
        FieldMode::Formula,
        "restore_maxwell_fields never promotes the mode",
    );

    let mut restored = State::new(true);
    record.restore(&mut restored, &mut Controls::new());
    assert_eq!(restored.field_mode, FieldMode::Formula, "no promotion");
    assert_eq!(restored.maxwell, MaxwellScene::default());
    assert!(restored.maxwell_sampling.is_none());
    assert!(
        restored
            .maxwell_error
            .as_deref()
            .is_some_and(|message| message.to_lowercase().contains("maxwell")),
        "{:?}",
        restored.maxwell_error,
    );

    // The same broken payload without the stored mode stays silent.
    let text = r#"{
        "maxwell_case": "bogus_case",
        "maxwell_outline": "ellipse:1",
        "maxwell_elements": 31
    }"#;
    let record: ViewRecord = serde_json::from_str(text).unwrap();
    let mut state = State::new(false);
    record.restore(&mut state, &mut Controls::new());
    assert_eq!(state.field_mode, FieldMode::Formula);
    assert_eq!(state.maxwell, MaxwellScene::default());
    assert!(state.maxwell_error.is_none());
}

#[test]
fn global_reset_clears_maxwell_and_returns_to_formula() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Maxwell);
    app.state.set_mode(false);
    assert!(app.state.set_maxwell_scene(custom_scene()));
    app.state.refresh_maxwell_sampling();
    assert!(app.state.maxwell_sampling.is_some());
    app.state.maxwell_error = Some("retained error".into());

    app.test_harness().reset_all();

    assert_eq!(app.state.field_mode, FieldMode::Formula);
    assert_eq!(app.state.maxwell, MaxwellScene::default());
    assert!(app.state.maxwell_sampling.is_none());
    assert!(app.state.maxwell_error.is_none());
}

#[test]
fn maxwell_section_reset_leaves_other_modes_untouched() {
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Maxwell);
    assert!(state.set_maxwell_scene(custom_scene()));
    state.maxwell_error = Some("retained error".into());
    let sources_before = state.sources.clone();
    let induction_before = state.induction.clone();
    let ode_before = state.ode.clone();
    let material_before = state.material.clone();

    state.reset_maxwell();

    assert_eq!(state.field_mode, FieldMode::Maxwell);
    assert_eq!(state.maxwell, MaxwellScene::default());
    assert!(state.maxwell_sampling.is_none());
    assert!(state.maxwell_error.is_none());
    assert_eq!(state.sources, sources_before);
    assert_eq!(state.induction, induction_before);
    assert_eq!(state.ode, ode_before);
    assert_eq!(state.material, material_before);
}
