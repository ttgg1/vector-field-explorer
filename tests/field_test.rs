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
