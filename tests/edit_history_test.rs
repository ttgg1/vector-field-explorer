//! Issue #86 slice A1: the pure bounded edit-history model.
//!
//! Exercises transaction semantics, the redo tail, eviction at
//! [`MAX_HISTORY`], the undo-free simulation scope and snapshot equality. No
//! UI, input, persistence or renderer code is involved.

use vector_field_explorer::appearance_profile::{AppearanceProfile, builtins};
use vector_field_explorer::edit_history::{
    EditHistory, EditScope, EditSnapshot, MAX_HISTORY,
};
use vector_field_explorer::parameters::ParameterSet;
use vector_field_explorer::persistence::ViewRecord;
use vector_field_explorer::scene_document::SceneCamera;

fn snapshot(
    view: ViewRecord,
    parameters: ParameterSet,
    camera: SceneCamera,
    appearance: AppearanceProfile,
) -> EditSnapshot {
    EditSnapshot::new(view, parameters, camera, appearance)
}

fn default_appearance() -> AppearanceProfile {
    let mut appearance = builtins()
        .into_iter()
        .next()
        .expect("the built-in default look exists");
    appearance.name = "history sentinel".to_owned();
    appearance
}

fn default_snapshot() -> EditSnapshot {
    snapshot(
        ViewRecord::default(),
        ParameterSet::default(),
        SceneCamera::default(),
        default_appearance(),
    )
}

#[test]
fn record_then_undo_returns_the_previous_value() {
    let mut history = EditHistory::new();
    assert!(history.is_empty());
    assert!(!history.can_undo());

    assert!(history.record(1u32, &2));
    assert_eq!(history.len(), 1);
    assert!(history.can_undo());
    assert!(!history.can_redo());

    assert_eq!(history.undo(2), Some(1));
    assert_eq!(history.len(), 0);
    assert!(!history.can_undo());
    assert!(history.can_redo());
    assert_eq!(history.undo(1), None);
}

#[test]
fn redo_returns_the_value_that_was_undone() {
    let mut history = EditHistory::new();
    assert!(history.record(1u32, &2));
    assert_eq!(history.undo(2), Some(1));
    assert_eq!(history.redo_len(), 1);
    assert!(history.can_redo());

    assert_eq!(history.redo(1), Some(2));
    assert_eq!(history.len(), 1);
    assert_eq!(history.redo_len(), 0);
    assert!(!history.can_redo());
}

#[test]
fn a_new_record_drops_the_redo_tail() {
    let mut history = EditHistory::new();
    assert!(history.record(1u32, &2));
    assert!(history.record(2, &3));
    assert_eq!(history.undo(3), Some(2));
    assert_eq!(history.redo_len(), 1);

    assert!(history.record(2, &4));
    assert_eq!(history.redo_len(), 0);
    assert!(!history.can_redo());
    assert_eq!(history.redo(4), None);
    assert_eq!(history.len(), 2);

    // The abandoned future (3) is gone; undo walks the recorded branch.
    assert_eq!(history.undo(4), Some(2));
    assert_eq!(history.undo(2), Some(1));
}

#[test]
fn commit_without_a_change_records_nothing() {
    let mut history = EditHistory::new();
    history.begin(7u32);
    assert!(!history.commit(&7));
    assert_eq!(history.len(), 0);
    assert!(!history.can_undo());
    assert!(!history.is_recording());
}

#[test]
fn commit_without_begin_records_nothing() {
    let mut history = EditHistory::new();
    assert!(!history.is_recording());
    assert!(!history.commit(&5));
    assert_eq!(history.len(), 0);
    assert!(!history.can_undo());
}

#[test]
fn begin_twice_keeps_the_first_snapshot() {
    let mut history = EditHistory::new();
    history.begin(1u32);
    history.begin(2);
    assert!(history.commit(&3));

    assert_eq!(history.len(), 1);
    assert_eq!(history.undo(3), Some(1));
}

#[test]
fn cancel_discards_the_pending_snapshot() {
    let mut history = EditHistory::new();
    assert!(history.record(1u32, &2));
    assert_eq!(history.len(), 1);

    history.begin(9);
    assert!(history.is_recording());
    history.cancel();
    assert!(!history.is_recording());
    assert_eq!(history.len(), 1);

    // A commit after cancellation has no pending snapshot.
    assert!(!history.commit(&10));
    assert_eq!(history.len(), 1);
}

#[test]
fn history_is_bounded_and_evicts_the_oldest_entry() {
    let mut history = EditHistory::new();
    for value in 0..70u32 {
        assert!(history.record(value, &(value + 1)));
    }

    assert_eq!(history.len(), MAX_HISTORY);

    // The newest 64 pre-edit snapshots survive: 6..70 in reverse.
    let mut current = 70u32;
    for expected in (6..70).rev() {
        current = history
            .undo(current)
            .unwrap_or_else(|| panic!("undo should return {expected}"));
        assert_eq!(current, expected);
    }
    assert_eq!(history.undo(current), None);
}

#[test]
fn undo_and_redo_at_the_ends_are_no_ops() {
    let mut history = EditHistory::<u32>::new();
    assert!(history.is_empty());
    assert!(!history.can_undo());
    assert!(!history.can_redo());
    assert_eq!(history.undo(1), None);
    assert_eq!(history.redo(1), None);
    assert_eq!(history.len(), 0);
    assert_eq!(history.redo_len(), 0);

    assert!(history.record(1u32, &2));
    assert!(history.can_undo());
    assert_eq!(history.redo(2), None);
    assert_eq!(history.len(), 1);
    assert_eq!(history.redo_len(), 0);
}

#[test]
fn redo_respects_the_history_bound() {
    let mut history = EditHistory::new();
    for value in 0..70u32 {
        assert!(history.record(value, &(value + 1)));
    }
    assert_eq!(history.len(), MAX_HISTORY);

    let mut current = 70u32;
    while let Some(previous) = history.undo(current) {
        current = previous;
    }
    assert_eq!(history.redo_len(), MAX_HISTORY);

    let mut redone = 0usize;
    while let Some(next) = history.redo(current) {
        current = next;
        redone += 1;
        assert!(
            history.len() <= MAX_HISTORY,
            "redo never pushes undo past the bound"
        );
    }
    assert_eq!(redone, MAX_HISTORY);
    assert_eq!(history.len(), MAX_HISTORY);
    assert_eq!(history.redo_len(), 0);
}

#[test]
fn simulation_scope_is_the_only_undo_free_scope() {
    assert_eq!(
        EditScope::ALL,
        [
            EditScope::Color,
            EditScope::Camera,
            EditScope::Simulation,
            EditScope::All,
        ]
    );
    assert_eq!(EditScope::Color.label(), "Color");
    assert_eq!(EditScope::Camera.label(), "Camera");
    assert_eq!(EditScope::Simulation.label(), "Simulation");
    assert_eq!(EditScope::All.label(), "All");

    assert!(EditScope::Color.records_history());
    assert!(EditScope::Camera.records_history());
    assert!(!EditScope::Simulation.records_history());
    assert!(EditScope::All.records_history());
}

#[test]
fn snapshot_equality_covers_every_field() {
    let equal = default_snapshot();
    assert_eq!(equal, default_snapshot());

    let view = ViewRecord {
        formula: "(x, y, 0)".to_owned(),
        ..ViewRecord::default()
    };
    let changed_view = snapshot(
        view,
        ParameterSet::default(),
        SceneCamera::default(),
        default_appearance(),
    );
    assert_ne!(equal, changed_view);

    let parameters = ParameterSet {
        entries: vec![Default::default()],
        ..ParameterSet::default()
    };
    let changed_parameters = snapshot(
        ViewRecord::default(),
        parameters,
        SceneCamera::default(),
        default_appearance(),
    );
    assert_ne!(equal, changed_parameters);

    let camera = SceneCamera {
        yaw: SceneCamera::default().yaw + 0.5,
        ..SceneCamera::default()
    };
    let changed_camera = snapshot(
        ViewRecord::default(),
        ParameterSet::default(),
        camera,
        default_appearance(),
    );
    assert_ne!(equal, changed_camera);

    let appearance = AppearanceProfile {
        name: "another sentinel".to_owned(),
        ..default_appearance()
    };
    let changed_appearance = snapshot(
        ViewRecord::default(),
        ParameterSet::default(),
        SceneCamera::default(),
        appearance,
    );
    assert_ne!(equal, changed_appearance);
}
