//! Issue #89 slice A: validated named-parameter records and the atomic commit
//! seam. No parser or UI binding is exercised here.

use glam::Vec2;
use vector_field_explorer::parameters::{MAX_NAME_LEN, MAX_PARAMETERS, Parameter, ParameterSet};
use vector_field_explorer::viewmodel::State;

fn parameter(name: &str, value: f64) -> Parameter {
    Parameter {
        name: name.to_string(),
        value,
        min: None,
        max: None,
        step: 1.0,
    }
}

fn set(entries: Vec<Parameter>) -> ParameterSet {
    ParameterSet {
        version: 1,
        entries,
    }
}

#[test]
fn reserved_names_rejected() {
    // Pin the whole reserved list: coordinates/time, meval's constants, and
    // every builtin function in the locked `meval` 0.2.0 context.
    let reserved = [
        "x", "y", "z", "t", "pi", "e", "sqrt", "exp", "ln", "abs", "sin", "cos", "tan", "asin",
        "acos", "atan", "sinh", "cosh", "tanh", "asinh", "acosh", "atanh", "floor", "ceil",
        "round", "signum", "atan2", "max", "min",
    ];
    assert_eq!(reserved.len(), 29, "the pinned meval list is complete");
    for name in reserved {
        let error = set(vec![parameter(name, 1.0)])
            .validate()
            .expect_err(&format!("`{name}` must be reserved"));
        assert!(
            error.contains(name),
            "the error names the offending parameter: {error}"
        );
        assert!(
            error.to_lowercase().contains("reserved"),
            "the error states the reserved-name rule: {error}"
        );
    }
}

#[test]
fn duplicate_and_malformed_names_rejected() {
    let duplicate = set(vec![parameter("gain", 1.0), parameter("gain", 2.0)]);
    assert!(
        duplicate.validate().unwrap_err().contains("gain"),
        "duplicate names name the parameter"
    );

    for name in ["has space", "1leading", "", "dash-name", "dot.name", "é"] {
        let error = set(vec![parameter(name, 1.0)])
            .validate()
            .expect_err(&format!("`{name}` is not a valid name"));
        assert!(
            error.to_lowercase().contains("name"),
            "the error states the naming rule: {error}"
        );
    }

    let too_long = "a".repeat(MAX_NAME_LEN + 1);
    assert!(set(vec![parameter(&too_long, 1.0)]).validate().is_err());
    let longest = "a".repeat(MAX_NAME_LEN);
    assert!(
        set(vec![parameter(&longest, 1.0)]).validate().is_ok(),
        "a name of exactly MAX_NAME_LEN ASCII bytes is allowed"
    );

    // Names are case-sensitive, so only exact duplicates collide.
    assert!(
        set(vec![parameter("gain", 1.0), parameter("Gain", 2.0)])
            .validate()
            .is_ok()
    );
}

#[test]
fn bounds_and_step_rejected() {
    let mut reversed = parameter("reversed", 1.0);
    reversed.min = Some(2.0);
    reversed.max = Some(1.0);
    let error = set(vec![reversed]).validate().unwrap_err();
    assert!(error.contains("reversed"), "the error names the parameter");
    assert!(
        error.to_lowercase().contains("min"),
        "the error states the bound rule: {error}"
    );

    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let error = set(vec![parameter("value", value)])
            .validate()
            .expect_err("a non-finite value is rejected");
        assert!(error.contains("value") && error.to_lowercase().contains("finite"));
    }

    let mut bad_min = parameter("low", 1.0);
    bad_min.min = Some(f64::NAN);
    assert!(set(vec![bad_min]).validate().is_err());

    let mut bad_max = parameter("high", 1.0);
    bad_max.max = Some(f64::INFINITY);
    assert!(set(vec![bad_max]).validate().is_err());

    for step in [0.0, -0.5, f64::NAN, f64::INFINITY] {
        let mut entry = parameter("step", 1.0);
        entry.step = step;
        let error = set(vec![entry])
            .validate()
            .expect_err("step must be finite and positive");
        assert!(
            error.contains("step") && error.to_lowercase().contains("step"),
            "the error names the parameter and the step rule: {error}"
        );
    }

    let mut below = parameter("bounded", 0.5);
    below.min = Some(1.0);
    below.max = Some(2.0);
    assert!(
        set(vec![below]).validate().is_err(),
        "a value below the lower bound is rejected"
    );

    let mut above = parameter("bounded", 2.5);
    above.min = Some(1.0);
    above.max = Some(2.0);
    assert!(
        set(vec![above]).validate().is_err(),
        "a value above the upper bound is rejected"
    );

    let mut inclusive = parameter("bounded", 2.0);
    inclusive.min = Some(1.0);
    inclusive.max = Some(2.0);
    assert!(
        set(vec![inclusive]).validate().is_ok(),
        "the bounds are inclusive"
    );
}

#[test]
fn entry_cap_enforced() {
    let entries = |count: usize| {
        (0..count)
            .map(|index| parameter(&format!("p{index}"), index as f64))
            .collect::<Vec<_>>()
    };
    assert!(
        set(entries(MAX_PARAMETERS)).validate().is_ok(),
        "exactly MAX_PARAMETERS entries are allowed"
    );
    let error = set(entries(MAX_PARAMETERS + 1))
        .validate()
        .expect_err("one entry over the cap is rejected");
    assert!(
        error.contains(&MAX_PARAMETERS.to_string()),
        "the error states the cap: {error}"
    );
}

#[test]
fn invalid_commit_is_atomic() {
    let mut state = State::new(false);
    let previous = state.parameters.clone();
    let generation = state.plot.generation;
    let ode_epoch = state.ode_epoch;
    state.dirty = false;

    let invalid = set(vec![parameter("x", 1.0)]);
    let error = state
        .commit_parameters(invalid)
        .expect_err("a reserved name is rejected");
    assert!(error.contains("x"));
    assert_eq!(
        state.parameters, previous,
        "a rejected commit keeps the previous set"
    );
    assert!(!state.dirty, "a rejected commit does not dirty the scene");
    assert_eq!(
        state.plot.generation, generation,
        "a rejected commit leaves the generation counter alone"
    );
    assert_eq!(
        state.ode_epoch, ode_epoch,
        "a rejected commit leaves dependent caches alone"
    );
    assert_eq!(state.parameter_value("x"), None);
}

#[test]
fn valid_commit_replaces_once_and_marks_dirty() {
    let mut state = State::new(false);
    state.dirty = false;

    let next = ParameterSet {
        version: 1,
        entries: vec![Parameter {
            name: "gain".to_string(),
            value: 2.5,
            min: Some(0.0),
            max: Some(5.0),
            step: 0.5,
        }],
    };
    state
        .commit_parameters(next.clone())
        .expect("a valid set commits");
    assert_eq!(state.parameters, next);
    assert!(state.dirty, "an accepted commit dirties the scene");
    assert_eq!(state.parameter_value("gain"), Some(2.5));
    assert_eq!(state.parameter_value("missing"), None);
}

#[test]
fn equal_commit_is_a_no_op() {
    let mut state = State::new(false);

    let next = set(vec![parameter("gain", 2.5)]);
    state
        .commit_parameters(next.clone())
        .expect("a valid set commits");
    assert!(
        state.snapshot_if_dirty().is_some(),
        "the first commit consumes a fresh scene"
    );
    assert!(!state.dirty, "the scene rebuild clears the dirty flag");

    state
        .commit_parameters(next)
        .expect("an equal set is accepted");
    assert!(
        !state.dirty,
        "committing an equal set does not dirty the scene"
    );
    assert!(
        state.snapshot_if_dirty().is_none(),
        "committing an equal set requests no rebuild"
    );
}

fn u(state: &State) -> f32 {
    state
        .field
        .eval2(Vec2::new(1.0, 0.0))
        .expect("the field evaluates")
        .x
}

#[test]
fn committing_a_changed_value_re_evaluates_the_existing_formula() {
    let mut state = State::new(false);
    state.set_formula("(gain*x, 0)".to_string());
    assert!(
        !state.parse_error.is_empty(),
        "before the parameter is committed the formula cannot bind"
    );

    state
        .commit_parameters(set(vec![parameter("gain", 2.5)]))
        .expect("the set commits");
    assert_eq!(state.parse_error, "", "committing binds the formula");
    assert!((u(&state) - 2.5).abs() < 1e-6);

    assert!(state.snapshot_if_dirty().is_some());
    state
        .commit_parameters(set(vec![parameter("gain", 4.0)]))
        .expect("a changed value commits");
    assert!(
        (u(&state) - 4.0).abs() < 1e-6,
        "the existing formula re-evaluates with the new value"
    );
    assert!(
        state.snapshot_if_dirty().is_some(),
        "a changed value requests exactly one rebuild"
    );
}

#[test]
fn formula_referencing_a_removed_parameter_keeps_the_last_valid_field() {
    let mut state = State::new(false);
    state
        .commit_parameters(set(vec![parameter("gain", 2.5)]))
        .expect("the set commits");
    state.set_formula("(gain*x, 0)".to_string());
    assert_eq!(state.parse_error, "");
    assert!((u(&state) - 2.5).abs() < 1e-6);

    state
        .commit_parameters(set(Vec::new()))
        .expect("the empty set is valid");
    assert_eq!(
        state.parameters,
        set(Vec::new()),
        "the new set stays committed"
    );
    assert!(
        state.parse_error.contains("gain"),
        "the parse error names the free parameter: {}",
        state.parse_error
    );
    assert!(
        (u(&state) - 2.5).abs() < 1e-6,
        "the previous valid field is kept"
    );

    state.set_formula("(x, 0)".to_string());
    assert_eq!(
        state.parse_error, "",
        "a formula that binds clears the error"
    );
    assert!((u(&state) - 1.0).abs() < 1e-6);
}

#[test]
fn rejected_sets_leave_field_and_parse_error_untouched() {
    let mut state = State::new(false);
    state
        .commit_parameters(set(vec![parameter("gain", 2.5)]))
        .expect("the set commits");
    state.set_formula("(gain*x, 0)".to_string());
    assert_eq!(state.parse_error, "");
    let previous = state.parameters.clone();
    state.dirty = false;

    let error = state
        .commit_parameters(set(vec![parameter("x", 1.0)]))
        .expect_err("a reserved name is rejected");
    assert!(error.contains("x"));
    assert_eq!(state.parameters, previous, "the previous set is kept");
    assert_eq!(state.parse_error, "", "the parse error is untouched");
    assert!((u(&state) - 2.5).abs() < 1e-6, "the field is untouched");
    assert!(!state.dirty, "a rejected set does not dirty the scene");
}
