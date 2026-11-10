use vector_field_explorer::field::FieldExpr;
use vector_field_explorer::ui::cas_panel::{CasPanelState, Operation, ResultKind, Variable};
use vector_field_explorer::viewmodel::State;

#[test]
fn previews_operations_with_explicit_result_kind_and_copy_payload() {
    let mut panel = CasPanelState::default();
    panel.input = "x*y".into();
    panel.operation = Operation::Differentiate;
    panel.variable = Variable::X;
    panel.refresh(2);
    assert_eq!(panel.preview.as_ref().unwrap().formula, "y");
    assert_eq!(panel.preview.as_ref().unwrap().kind, ResultKind::Scalar);
    assert_eq!(panel.copy_text(), Some("y"));

    panel.input = "(x, y)".into();
    panel.operation = Operation::Curl;
    panel.refresh(2);
    assert_eq!(panel.preview.as_ref().unwrap().kind, ResultKind::Scalar);
    assert!(!panel.can_apply(2));
    panel.input = "(x*y, y*z, z*x)".into();
    panel.refresh(3);
    assert_eq!(panel.preview.as_ref().unwrap().kind, ResultKind::Vector);
    assert_eq!(
        panel.preview.as_ref().unwrap().formula.matches(',').count(),
        2
    );

    panel.input = "(x*y, x^2)".into();
    panel.operation = Operation::Differentiate;
    panel.variable = Variable::X;
    panel.refresh(2);
    assert_eq!(panel.preview.as_ref().unwrap().kind, ResultKind::Vector);
    assert!(panel.preview.as_ref().unwrap().formula.contains("y"));

    panel.input = "(x*y, x+y)".into();
    panel.operation = Operation::Divergence;
    panel.refresh(2);
    assert_eq!(panel.preview.as_ref().unwrap().kind, ResultKind::Scalar);
    assert!(!panel.can_apply(2));
}

#[test]
fn only_explicit_current_vector_apply_mutates_the_field() {
    let mut panel = CasPanelState::default();
    let mut state = State::new(false);
    state.snapshot_if_dirty();
    let original = state.formula.clone();
    panel.input = "x*y".into();
    panel.operation = Operation::Gradient;
    panel.refresh(2);
    assert_eq!(state.formula, original);
    assert!(panel.apply(&mut state, 2));
    assert_eq!(state.formula, "(y, x)");
    assert_eq!(FieldExpr::parse(&state.formula).unwrap().dimension, 2);
    assert!(state.snapshot_if_dirty().is_some());

    panel.input = "x".into();
    panel.operation = Operation::Differentiate;
    panel.refresh(2);
    assert!(!panel.can_apply(2));
    assert!(!panel.apply(&mut state, 2));
}

#[test]
fn edits_mode_switch_and_failures_invalidate_apply_without_mutation() {
    let mut panel = CasPanelState::default();
    let mut state = State::new(false);
    state.snapshot_if_dirty();
    let original = state.formula.clone();
    panel.input = "x".into();
    panel.operation = Operation::Gradient;
    panel.refresh(2);
    assert!(panel.can_apply(2));
    panel.input.push_str(" +");
    assert!(!panel.can_apply(2));
    panel.refresh(2);
    assert!(panel.error.is_some());
    assert!(!panel.apply(&mut state, 2));
    assert_eq!(state.formula, original);
    panel.input = "x".into();
    panel.refresh(2);
    assert!(!panel.can_apply(3));
    assert!(!panel.apply(&mut state, 3));
    assert_eq!(state.formula, original);
    assert!(state.snapshot_if_dirty().is_none());
}
