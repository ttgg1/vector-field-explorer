//! Formula parsing and evaluation coverage.
//!
//! Moved out of `src/field.rs`'s inline module (ticket 13) without changing a
//! test or an assertion: every case here already exercised the public
//! `FieldExpr` API.

use glam::{DVec3, Vec2, Vec3};
use vector_field_explorer::field::{FieldError, FieldExpr};
use vector_field_explorer::parameters::{Parameter, ParameterSet};

fn parameters(entries: &[(&str, f64)]) -> ParameterSet {
    ParameterSet {
        version: 1,
        entries: entries
            .iter()
            .map(|(name, value)| Parameter {
                name: (*name).to_string(),
                value: *value,
                min: None,
                max: None,
                step: 1.0,
            })
            .collect(),
    }
}

#[test]
fn parse_two_component() {
    let f = FieldExpr::parse("(sin(y), x*cos(y))").expect("should parse");
    assert_eq!(f.dimension, 2);
    let v = f.eval2(Vec2::new(1.0, 0.0)).expect("should evaluate");
    assert!(v.x.abs() < 1e-6, "u = {}', expected ~0", v.x);
    assert!((v.y - 1.0).abs() < 1e-6, "v = {}, expected ~1", v.y);
}

#[test]
fn parse_three_component() {
    let f = FieldExpr::parse("(x, -y, z)").expect("should parse");
    assert_eq!(f.dimension, 3);
}

#[test]
fn three_component_evaluated_in_two_d() {
    let f = FieldExpr::parse("(x, -y, z)").expect("should parse");
    let v = f.eval2(Vec2::new(1.0, 0.0)).expect("should evaluate");
    assert!((v.x - 1.0).abs() < 1e-6, "u = {}, expected ~1", v.x);
    assert!(v.y.abs() < 1e-6, "v = {}, expected ~0", v.y);
}

#[test]
fn top_level_comma_only_must_not_split() {
    let f = FieldExpr::parse("(max(0,1), x)").expect("inner commas must not split");
    assert_eq!(f.dimension, 2);
}

#[test]
fn err_on_invalid() {
    let err = FieldExpr::parse("(x, sin(").unwrap_err();
    match err {
        FieldError::Parse(msg) => {
            assert!(msg.contains("sin("), "message `{msg}` must contain `sin(`");
        }
    }
}

#[test]
fn eval_nan_becomes_none() {
    let f = FieldExpr::parse("(1/x, 0)").expect("should parse");
    let v = f.eval2(Vec2::new(0.0, 0.0));
    assert!(v.is_none());
}

#[test]
fn unknown_variable_not_evaluated() {
    let f = FieldExpr::parse("(w, 0)").expect("should parse");
    assert!(f.eval2(Vec2::new(1.0, 1.0)).is_none());
    assert!(f.eval3(Vec3::new(1.0, 1.0, 1.0)).is_none());
}

#[test]
fn eval3_at_differs_from_eval3_for_a_t_formula() {
    let f = FieldExpr::parse("(cos(t), sin(t))").expect("should parse");
    let at_zero = f.eval3(Vec3::new(0.0, 0.0, 0.0)).expect("eval3");
    let at_one = f.eval3_at(Vec3::new(0.0, 0.0, 0.0), 1.0).expect("eval3_at");
    assert!((at_zero.x - at_one.x).abs() > 1e-6 || (at_zero.y - at_one.y).abs() > 1e-6);
    assert!(f.is_time_dependent());
}

#[test]
fn eval3_at_f64_matches_the_f32_boundary_and_rejects_nonfinite() {
    let f = FieldExpr::parse("(t + x, sin(t) * y, z - t)").expect("time-dependent field");
    assert!(f.is_time_dependent());
    let point = Vec3::new(0.25, -1.5, 2.0);
    let time = 0.5f32;
    let f32_value = f.eval3_at(point, time).expect("f32 boundary");
    let f64_value = f
        .eval3_at_f64(
            DVec3::new(point.x as f64, point.y as f64, point.z as f64),
            time as f64,
        )
        .expect("f64 boundary");
    let close = |a: f32, b: f32| (a - b).abs() <= 8.0 * f32::EPSILON * a.abs().max(1.0);
    assert!(close(f32_value.x, f64_value.x));
    assert!(close(f32_value.y, f64_value.y));
    assert!(close(f32_value.z, f64_value.z));

    let singular = FieldExpr::parse("(1 / (t - 1), 0, 0)").expect("singular field");
    assert!(
        singular.eval3_at_f64(DVec3::ZERO, 1.0).is_none(),
        "non-finite components must be rejected"
    );
}

#[test]
fn eval3_equals_t_zero() {
    let f = FieldExpr::parse("(cos(t), sin(t)*x)").expect("should parse");
    let a = f.eval3(Vec3::new(2.0, 0.0, 0.0)).expect("eval3");
    let b = f.eval3_at(Vec3::new(2.0, 0.0, 0.0), 0.0).expect("eval3_at");
    assert!((a.x - b.x).abs() < 1e-6);
    assert!((a.y - b.y).abs() < 1e-6);
}

#[test]
fn both_components_see_t_in_2d_and_3d() {
    let f3 = FieldExpr::parse("(t, x, z+t)").expect("3d");
    let a = f3.eval3_at(Vec3::new(1.0, 2.0, 3.0), 5.0).expect("3d t");
    assert!((a.x - 5.0).abs() < 1e-6);
    assert!((a.z - 8.0).abs() < 1e-6);

    let f2 = FieldExpr::parse("(t, x)").expect("2d");
    let b = f2.eval3_at(Vec3::new(1.0, 2.0, 0.0), 5.0).expect("2d t");
    assert!((b.x - 5.0).abs() < 1e-6);
}

#[test]
fn is_time_dependent_only_for_free_t() {
    assert!(FieldExpr::parse("(t, x)").unwrap().is_time_dependent());
    assert!(!FieldExpr::parse("(t(x), x)").unwrap().is_time_dependent());
    assert!(!FieldExpr::parse("(x, y)").unwrap().is_time_dependent());
    let f = FieldExpr::parse("(w, 0)").unwrap();
    assert!(f.eval3(Vec3::new(1.0, 1.0, 1.0)).is_none());
}

#[test]
fn parameterised_formula_evaluates_with_the_committed_value() {
    let set = parameters(&[("k", 2.5)]);
    let f = FieldExpr::parse_with_parameters("(k*x, y)", &set).expect("parameter binds");
    let v = f.eval2(Vec2::new(2.0, 1.0)).expect("should evaluate");
    assert!((v.x - 5.0).abs() < 1e-6, "u = {}, expected 5", v.x);
    assert!((v.y - 1.0).abs() < 1e-6, "v = {}, expected 1", v.y);
    assert!(
        !f.is_time_dependent(),
        "a substituted value cannot make the field time-dependent"
    );
}

#[test]
fn changing_the_parameter_set_changes_the_result() {
    let first = FieldExpr::parse_with_parameters("(k*x, 0)", &parameters(&[("k", 2.0)]))
        .expect("first set binds");
    let second = FieldExpr::parse_with_parameters("(k*x, 0)", &parameters(&[("k", 3.0)]))
        .expect("second set binds");
    let point = Vec2::new(1.5, 0.0);
    assert!((first.eval2(point).unwrap().x - 3.0).abs() < 1e-6);
    assert!((second.eval2(point).unwrap().x - 4.5).abs() < 1e-6);
}

#[test]
fn environment_names_and_function_calls_are_never_rewritten() {
    // The set is invalid on purpose: even a raw set that names coordinates or
    // builtins must not shadow them in the formula environment.
    let set = parameters(&[("x", 7.0), ("sin", 9.0), ("t", 11.0)]);
    let f = FieldExpr::parse_with_parameters("(x, sin(x)+t)", &set).expect("parse");
    let v = f.eval3_at(Vec3::new(2.0, 0.0, 0.0), 0.5).expect("evaluate");
    assert!((v.x - 2.0).abs() < 1e-6, "x is the coordinate");
    assert!(
        (v.y - (2.0f32.sin() + 0.5)).abs() < 1e-6,
        "sin stays a function and t stays the clock: {}",
        v.y
    );
    assert!(
        f.is_time_dependent(),
        "the substituted component keeps free t"
    );
}

#[test]
fn an_identifier_followed_by_a_parenthesis_is_a_function_call() {
    let set = parameters(&[("k", 2.0)]);
    let error = FieldExpr::parse_with_parameters("(k(x), 0)", &set)
        .expect_err("a parameter name in call position is not substituted");
    let message = error.to_string();
    assert!(
        message.contains("function") && message.contains('k'),
        "the error reports the unresolved call: {message}"
    );
}

#[test]
fn a_free_unknown_value_name_errors_and_names_the_component() {
    let set = parameters(&[("k", 2.0)]);
    let error = FieldExpr::parse_with_parameters("(x, k*w)", &set)
        .expect_err("a free unknown value name is rejected");
    let message = error.to_string();
    assert!(
        message.contains("k*w"),
        "the error names the component: {message}"
    );
    assert!(
        message.contains('w'),
        "the error names the free identifier: {message}"
    );
    assert!(
        !message.to_lowercase().contains("parsed successfully"),
        "the error must not claim the formula parsed: {message}"
    );
}
