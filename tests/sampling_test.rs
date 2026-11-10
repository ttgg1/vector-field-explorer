//! Field sampling coverage: grid points → coloured arrow data.
//!
//! Moved out of `src/sampling.rs`'s inline module (ticket 13) without changing a
//! test or an assertion: `sample_field`, `magnitude_range` and `ViewOptions` are
//! all public.

use glam::{UVec3, Vec3};
use vector_field_explorer::colormap::Colormap;
use vector_field_explorer::domain::Domain;
use vector_field_explorer::field::FieldExpr;
use vector_field_explorer::sampling::{ViewOptions, magnitude_range, sample_field, sample_field_at};

fn opts(colormap: Colormap, color_range: Option<(f32, f32)>) -> ViewOptions {
    ViewOptions { colormap, color_range }
}

#[test]
fn constant_field_fills_grid() {
    let f = FieldExpr::parse("1, 0, 0").unwrap();
    let arrows = sample_field(&f, &Domain::default(), UVec3::new(2, 2, 1), &opts(Colormap::Mono, None));
    assert_eq!(arrows.len(), 4);
    for a in &arrows {
        assert!(a.dir.distance(Vec3::X) < 1e-5, "dir must be unit +X");
        assert_eq!(a.mag, 1.0);
    }
}

#[test]
fn sample_field_at_varies_with_t() {
    let f = FieldExpr::parse("(cos(t), 0, 0)").expect("parses");
    let domain = Domain::default();
    let grid = UVec3::new(4, 4, 1);
    let view = opts(Colormap::Mono, None);
    let a = sample_field_at(&f, &domain, grid, &view, 0.0);
    let b = sample_field_at(&f, &domain, grid, &view, 1.0);
    assert_ne!(a, b);
}

#[test]
fn sample_field_equals_t_zero() {
    let f = FieldExpr::parse("(cos(t), 0, 0)").expect("parses");
    let domain = Domain::default();
    let grid = UVec3::new(4, 4, 1);
    let view = opts(Colormap::Mono, None);
    let a = sample_field(&f, &domain, grid, &view);
    let b = sample_field_at(&f, &domain, grid, &view, 0.0);
    assert_eq!(a, b);
}

#[test]
fn magnitude_range_of_constant_field() {
    let f = FieldExpr::parse("1, 0, 0").unwrap();
    let arrows = sample_field(&f, &Domain::default(), UVec3::new(2, 2, 1), &opts(Colormap::Mono, None));
    assert_eq!(magnitude_range(&arrows), (1.0, 1.0));
}

#[test]
fn sample_skips_nan_arrows() {
    // 1/x explodes at x == 0; that grid point must be dropped, not crash
    let f = FieldExpr::parse("1/x, 0, 0").unwrap();
    let arrows = sample_field(&f, &Domain::default(), UVec3::new(5, 1, 1), &opts(Colormap::Mono, None));
    assert_eq!(arrows.len(), 4, "the x=0 arrow is dropped");
    assert!(arrows.iter().all(|a| a.position.x.abs() > 1e-6));
}

#[test]
fn all_zero_field_yields_no_arrows() {
    // zero-length direction is degenerate - the vector stays empty
    let f = FieldExpr::parse("0, 0, 0").unwrap();
    let arrows = sample_field(&f, &Domain::default(), UVec3::new(2, 2, 1), &opts(Colormap::Mono, None));
    assert!(arrows.is_empty());
    assert_eq!(magnitude_range(&arrows), (0.0, 0.0));
}

#[test]
fn color_follows_provided_range() {
    // Plan ruling: the plan's default symmetric domain makes |x| equal at
    // both ends, so "left darker than right" is untestable there. The
    // intended behavior (color follows magnitude through the provided
    // range) is preserved with an asymmetric sampled domain.
    let f = FieldExpr::parse("x, 0, 0").unwrap();
    let domain = Domain::new(Vec3::new(-0.5, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0));
    let arrows = sample_field(&f, &domain, UVec3::new(2, 1, 1), &opts(Colormap::Mono, Some((0.0, 1.0))));
    assert_eq!(arrows.len(), 2);
    let (left, right) = (&arrows[0], &arrows[1]);
    assert!(left.mag < right.mag, "mags {} < {}", left.mag, right.mag);
    for c in 0..3 {
        assert!(left.color[c] < right.color[c], "left arrow must be darker");
    }
}
