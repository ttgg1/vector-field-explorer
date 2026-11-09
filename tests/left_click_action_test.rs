// Ticket 60: the Left click action enum, its State/Controls plumbing, and the
// pure click-action decision that each mouse click and keyboard shortcut uses.

use vector_field_explorer::ui::sidebar::{Controls, apply};
use vector_field_explorer::view_command::{
    CLICK_HINT_PARTICLES_NEED_3D, CLICK_HINT_PARTICLES_OFF, CLICK_HINT_STREAMLINES_OFF,
    ClickActionPlan, click_action_plan,
};
use vector_field_explorer::viewmodel::{
    ClickOutcome, LeftClickAction, State, click_outcome,
};

#[test]
fn left_click_action_defaults_to_seed_streamline() {
    assert_eq!(LeftClickAction::default(), LeftClickAction::SeedStreamline);
    assert_eq!(
        LeftClickAction::ALL,
        [LeftClickAction::SeedStreamline, LeftClickAction::AddParticles],
        "the selector offers exactly the two choices",
    );
    assert_eq!(
        LeftClickAction::ALL.map(LeftClickAction::label),
        ["Seed streamline", "Add particles"],
        "the two labels the sidebar shows",
    );
    for action in LeftClickAction::ALL {
        assert!(!action.tooltip().is_empty(), "{action:?} needs a tooltip");
    }
    assert_eq!(State::new(false).left_click_action, LeftClickAction::SeedStreamline);
    assert_eq!(State::new(true).left_click_action, LeftClickAction::SeedStreamline);
}

#[test]
fn the_setter_marks_dirty_only_when_the_action_changes() {
    let mut state = State::new(false);
    assert!(state.snapshot_if_dirty().is_some(), "consume the fresh scene");
    assert!(state.set_left_click_action(LeftClickAction::AddParticles));
    assert_eq!(state.left_click_action, LeftClickAction::AddParticles);
    assert!(state.snapshot_if_dirty().is_some(), "a change rebuilds the scene once");
    assert!(
        !state.set_left_click_action(LeftClickAction::AddParticles),
        "setting the same action is a no-op",
    );
    assert!(state.snapshot_if_dirty().is_none(), "and asks for no rebuild");
}

#[test]
fn the_controls_mirror_defaults_and_resets_to_seed_streamline() {
    let controls = Controls::new();
    let state = State::new(false);
    assert_eq!(controls.left_click_action, state.left_click_action);
    assert_eq!(controls.left_click_action, LeftClickAction::SeedStreamline);

    let mut controls = Controls::new();
    let mut state = State::new(false);
    controls.left_click_action = LeftClickAction::AddParticles;
    apply(&mut state, &mut controls);
    assert_eq!(state.left_click_action, LeftClickAction::AddParticles);
    assert_eq!(controls.left_click_action, state.left_click_action, "the mirror follows");

    controls.reset_features();
    assert_eq!(
        controls.left_click_action,
        LeftClickAction::SeedStreamline,
        "Reset Features restores the default",
    );
}

#[test]
fn reset_all_controls_restores_seed_streamline() {
    // Edit > Reset all controls replaces the mirror with `Controls::new()` and
    // hands it to the same `apply` seam.
    let mut state = State::new(true);
    assert!(state.set_left_click_action(LeftClickAction::AddParticles));
    let mut controls = Controls::new();
    apply(&mut state, &mut controls);
    assert_eq!(state.left_click_action, LeftClickAction::SeedStreamline);
}

#[test]
fn seed_streamline_click_plans_one_seed_and_no_burst() {
    assert_eq!(
        click_action_plan(LeftClickAction::SeedStreamline, true, true, true),
        ClickActionPlan::SeedStreamline,
        "Streamlines on seeds, whatever particles are doing",
    );
    assert_eq!(
        click_action_plan(LeftClickAction::SeedStreamline, false, true, true),
        ClickActionPlan::Ignore(CLICK_HINT_STREAMLINES_OFF),
        "Streamlines off is a no-op with a hint, not a particle burst",
    );
}

#[test]
fn add_particles_click_plans_one_burst_and_no_seed() {
    assert_eq!(
        click_action_plan(LeftClickAction::AddParticles, false, true, true),
        ClickActionPlan::AddParticles,
        "Particles on in 3D adds the existing burst, even with Streamlines off",
    );
    assert_eq!(
        click_action_plan(LeftClickAction::AddParticles, true, false, true),
        ClickActionPlan::Ignore(CLICK_HINT_PARTICLES_OFF),
        "Particles off is a no-op with a hint",
    );
    assert_eq!(
        click_action_plan(LeftClickAction::AddParticles, true, true, false),
        ClickActionPlan::Ignore(CLICK_HINT_PARTICLES_NEED_3D),
        "the 2D view has no particle cloud",
    );
}

#[test]
fn the_no_op_hints_are_pinned() {
    assert_eq!(CLICK_HINT_STREAMLINES_OFF, "Streamlines are off");
    assert_eq!(CLICK_HINT_PARTICLES_OFF, "Particles are off");
    assert_eq!(CLICK_HINT_PARTICLES_NEED_3D, "Particles need the 3D view");
}

#[test]
fn double_click_still_resets_and_drag_still_does_not_seed() {
    assert_eq!(
        click_outcome(true, true, false),
        ClickOutcome::ResetView,
        "double-click reset is unchanged by the new action selector",
    );
    assert_eq!(
        click_outcome(true, false, true),
        ClickOutcome::None,
        "a drag never seeds or bursts",
    );
}
