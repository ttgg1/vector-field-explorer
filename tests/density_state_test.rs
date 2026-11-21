//! Issue #96 slice B1: document-state custom densities and their edit history.
//!
//! The model owns validation, the dirty flag and the on-demand report cache;
//! these tests never touch the UI, renderer or field sampling.

use glam::Vec3;
use vector_field_explorer::appearance_profile::AppearanceProfile;
use vector_field_explorer::custom_density::{
    DensityFamily, DensitySpec, DensitySupport, SupportKind,
};
use vector_field_explorer::edit_history::{EditHistory, EditSnapshot};
use vector_field_explorer::persistence::ViewRecord;
use vector_field_explorer::scene_document::SceneCamera;
use vector_field_explorer::viewmodel::{MAX_DENSITIES, State};

fn support() -> DensitySupport {
    DensitySupport {
        kind: SupportKind::Box,
        center: Vec3::ZERO,
        axis: Vec3::Z,
        size: Vec3::splat(2.0),
    }
}

fn spec(name: &str, family: DensityFamily, formula: &str) -> DensitySpec {
    DensitySpec {
        name: name.to_string(),
        family,
        support: support(),
        formula: formula.to_string(),
        samples: 8,
        softening: 0.0,
    }
}

fn volume(name: &str) -> DensitySpec {
    spec(name, DensityFamily::VolumeCharge, "1")
}

fn snapshot(state: &State) -> EditSnapshot {
    EditSnapshot::new(
        ViewRecord::capture(state),
        state.parameters.clone(),
        SceneCamera::default(),
        AppearanceProfile::capture("", state),
    )
}

#[test]
fn commit_rejects_duplicate_names_and_keeps_the_previous_list() {
    let mut state = State::new(true);
    let committed = vec![volume("first")];
    state
        .commit_densities(committed.clone())
        .expect("a valid list commits");
    state.dirty = false;

    let error = state
        .commit_densities(vec![volume("same"), volume("same")])
        .expect_err("duplicate names are rejected");
    assert!(
        error.to_lowercase().contains("duplicate") || error.to_lowercase().contains("unique"),
        "the message names the duplicate rule: {error}"
    );
    assert_eq!(
        state.densities(),
        committed.as_slice(),
        "a rejected list leaves the committed list untouched"
    );
    assert!(!state.dirty, "a rejected list never dirties the scene");
}

#[test]
fn commit_is_a_no_op_for_an_equal_list() {
    let mut state = State::new(true);
    let list = vec![
        volume("one"),
        spec("current", DensityFamily::SteadyCurrent, "0,0,1"),
    ];
    state.commit_densities(list.clone()).expect("valid list");
    state.dirty = false;

    state
        .commit_densities(list.clone())
        .expect("an equal list is a validated no-op");
    assert!(!state.dirty, "an equal list does not request a rebuild");
    assert_eq!(state.densities(), list.as_slice());
}

#[test]
fn density_cap_is_enforced_with_a_message() {
    let mut state = State::new(true);
    let at_cap: Vec<DensitySpec> = (0..MAX_DENSITIES)
        .map(|index| volume(&format!("density {index}")))
        .collect();
    state
        .commit_densities(at_cap.clone())
        .expect("the cap itself is accepted");

    let mut over = at_cap;
    over.push(volume("one too many"));
    let error = state
        .commit_densities(over)
        .expect_err("the cap rejects an extra density");
    assert!(
        error.contains(&MAX_DENSITIES.to_string()),
        "the message names the cap: {error}"
    );
    assert!(
        error.to_lowercase().contains("densit"),
        "the message names the document record: {error}"
    );
    assert_eq!(state.densities().len(), MAX_DENSITIES);
}

#[test]
fn reset_densities_clears_without_dirtying_when_already_empty() {
    let mut state = State::new(true);
    state.dirty = false;
    state.reset_densities();
    assert!(!state.dirty, "an empty reset is a no-op");

    state
        .commit_densities(vec![volume("first")])
        .expect("a valid density commits");
    state.dirty = false;
    state.reset_densities();
    assert!(state.densities().is_empty());
    assert!(state.dirty, "a real clear requests one rebuild");
}

#[test]
fn density_edits_are_one_history_entry_each() {
    let mut state = State::new(true);
    let mut history = EditHistory::new();

    history.begin(snapshot(&state));
    state
        .commit_densities(vec![volume("first")])
        .expect("the first edit is valid");
    assert!(history.commit(&snapshot(&state)), "one entry is recorded");
    assert_eq!(history.len(), 1);

    // An equal list is not an edit: it records no second entry.
    history.begin(snapshot(&state));
    state
        .commit_densities(vec![volume("first")])
        .expect("an equal list is a no-op");
    assert!(!history.commit(&snapshot(&state)));
    assert_eq!(history.len(), 1, "the no-op adds no history entry");

    history.begin(snapshot(&state));
    state
        .commit_densities(vec![volume("first"), volume("second")])
        .expect("the second edit is valid");
    assert!(history.commit(&snapshot(&state)), "one more entry");
    assert_eq!(history.len(), 2);
}
