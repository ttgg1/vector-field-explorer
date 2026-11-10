//! Formula parsing and evaluation coverage.
//!
//! Moved out of `src/field.rs`'s inline module (ticket 13) without changing a
//! test or an assertion: every case here already exercised the public
//! `FieldExpr` API.

use glam::{Vec2, Vec3};
use vector_field_explorer::field::{FieldError, FieldExpr};

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
