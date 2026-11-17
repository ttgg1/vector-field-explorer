//! Task 4 ODE persistence, malformed-payload fallback and global reset.

use vector_field_explorer::VfApp;
use vector_field_explorer::ode::{OdeChart, OdeScene, OdeSeed, OdeSource};
use vector_field_explorer::persistence::ViewRecord;
use vector_field_explorer::source_scene::FieldMode;
use vector_field_explorer::ui::sidebar::Controls;
use vector_field_explorer::viewmodel::State;

#[test]
fn ode_scene_round_trips_all_fields() {
    let mut state = State::new(true);
    state.ode = OdeScene {
        source: OdeSource::Equations,
        dimension: 3,
        equations: ["-0.5 * y".into(), "2.0 * x".into(), "-0.1 * z".into()],
        seeds: vec![
            OdeSeed {
                position: [1.5, -0.25, 0.75],
            },
            OdeSeed {
                position: [0.0, 1.0, -1.0],
            },
        ],
        duration: 7.5,
    };
    state.ode_chart = OdeChart::TimeSeries;
    state.ode_variables = [true, false, true];
    state.field_mode = FieldMode::Ode;

    let record = ViewRecord::capture(&state);
    let mut restored = State::new(false);
    let mut controls = Controls::new();
    record.restore(&mut restored, &mut controls);

    assert_eq!(restored.ode, state.ode);
    assert_eq!(restored.field_mode, FieldMode::Ode);
    assert_eq!(restored.ode_chart, OdeChart::TimeSeries);
    assert_eq!(restored.ode_variables, [true, false, true]);
    // Restore is lazy: no solver runs during persistence restore.
    assert!(restored.ode_solution.is_none());
    restored.refresh_ode_if_dirty();
    assert!(restored.ode_solution.is_some());
    assert!(restored.ode_error.is_none());
}

#[test]
fn malformed_ode_payload_falls_back_lazily() {
    let record = ViewRecord {
        field_mode: FieldMode::Ode,
        ode_seeds: vec![[f64::NAN, 0.0, 0.0]],
        ..ViewRecord::default()
    };

    let mut state = State::new(false);
    let mut controls = Controls::new();
    record.restore(&mut state, &mut controls);

    assert_eq!(state.field_mode, FieldMode::Ode);
    assert_eq!(state.ode, OdeScene::default());
    assert!(state.ode_error.is_some());
    assert!(state.ode_solution.is_none());
    // The fallback scene is still pending, so the first Ode frame solves it.
    state.refresh_ode_if_dirty();
    assert!(state.ode_solution.is_some());
}

#[test]
fn legacy_record_leaves_ode_unselected_and_error_free() {
    let record = ViewRecord {
        field_mode: FieldMode::Formula,
        ..ViewRecord::default()
    };

    let mut state = State::new(false);
    let mut controls = Controls::new();
    record.restore(&mut state, &mut controls);

    assert_eq!(state.field_mode, FieldMode::Formula);
    assert_eq!(state.ode, OdeScene::default());
    assert!(state.ode_error.is_none());
    assert!(state.ode_solution.is_none());
}

#[test]
fn global_reset_clears_ode_fields() {
    let mut state = State::new(true);
    state.ode = OdeScene {
        duration: 3.0,
        ..OdeScene::default()
    };
    state.ode_chart = OdeChart::TimeSeries;
    state.ode_variables = [false, true, false];
    state.reset_ode();
    assert_eq!(state.ode, OdeScene::default());
    assert_eq!(state.ode_chart, OdeChart::Phase);
    assert_eq!(state.ode_variables, [true, true, false]);
    assert!(state.ode_error.is_none());
}

#[test]
fn global_reset_returns_to_formula_and_clears_the_ode_loop_period() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Ode);
    app.clock.loop_period = Some(3.0);
    app.test_harness().reset_all();
    assert_eq!(app.state.field_mode, FieldMode::Formula);
    assert_eq!(app.state.ode, OdeScene::default());
    assert_eq!(app.clock.loop_period, None);
}
