//! Field sampling coverage: grid points → coloured arrow data.
//!
//! Moved out of `src/sampling.rs`'s inline module (ticket 13) without changing a
//! test or an assertion: `sample_field`, `magnitude_range` and `ViewOptions` are
//! all public.

use glam::{UVec3, Vec3};
use vector_field_explorer::color_mapping::{
    ColorMapping, ColorPalette, ColorQuantity,
};
use vector_field_explorer::color_ramp::ScaleMode;
use vector_field_explorer::colormap::Colormap;
use vector_field_explorer::domain::Domain;
use vector_field_explorer::field::FieldExpr;
use vector_field_explorer::plane::{PlaneAxis, PlaneFrame, PlaneView};
use vector_field_explorer::sampling::{
    ViewOptions, magnitude_range, sample_field, sample_field_at, sample_plane_at,
};

fn opts(colormap: Colormap, color_range: Option<(f32, f32)>) -> ViewOptions {
    ViewOptions { colormap, color_range, color_mapping: None }
}

fn mapping(
    colormap: Colormap,
    mode: ScaleMode,
    quantity: ColorQuantity,
    manual_range: Option<(f32, f32)>,
) -> ColorMapping {
    ColorMapping {
        palette: ColorPalette::Builtin(colormap),
        mode,
        quantity,
        manual_range,
    }
}

fn opts_with_mapping(
    colormap: Colormap,
    color_range: Option<(f32, f32)>,
    color_mapping: ColorMapping,
) -> ViewOptions {
    ViewOptions { colormap, color_range, color_mapping: Some(color_mapping) }
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

#[test]
fn legacy_mapping_is_byte_identical_to_no_mapping() {
    let f = FieldExpr::parse("x, 0, 0").unwrap();
    let domain = Domain::new(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0));
    let grid = UVec3::new(5, 1, 1);

    let legacy = sample_field_at(&f, &domain, grid, &opts(Colormap::Turbo, None), 0.0);
    let mapped = sample_field_at(
        &f,
        &domain,
        grid,
        &opts_with_mapping(
            Colormap::Turbo,
            None,
            mapping(
                Colormap::Turbo,
                ScaleMode::Linear,
                ColorQuantity::Magnitude,
                None,
            ),
        ),
        0.0,
    );
    assert_eq!(legacy, mapped, "a legacy-equivalent mapping paints the same bytes");
    assert!(!legacy.is_empty(), "the parity check is not vacuous");
}

#[test]
fn legacy_manual_range_parity_includes_out_of_range_values() {
    let f = FieldExpr::parse("x, 0, 0").unwrap();
    let domain = Domain::new(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0));
    let grid = UVec3::new(5, 1, 1);
    let range = Some((0.25, 0.5));

    let legacy = sample_field_at(
        &f,
        &domain,
        grid,
        &opts(Colormap::Viridis, range),
        0.0,
    );
    let mapped = sample_field_at(
        &f,
        &domain,
        grid,
        &opts_with_mapping(
            Colormap::Viridis,
            range,
            mapping(
                Colormap::Viridis,
                ScaleMode::Linear,
                ColorQuantity::Magnitude,
                range,
            ),
        ),
        0.0,
    );
    assert_eq!(
        legacy, mapped,
        "out-of-range magnitudes clamp through the builtin colormap exactly"
    );
    assert!(
        legacy.iter().any(|a| a.mag < 0.25 || a.mag > 0.5),
        "the sample set really crosses the manual range"
    );
}

#[test]
fn component_quantity_uses_a_zero_centred_range() {
    let f = FieldExpr::parse("x, 0, 0").unwrap();
    let domain = Domain::new(Vec3::splat(-1.0), Vec3::splat(1.0));
    let grid = UVec3::new(2, 1, 1);

    let magnitude = sample_field_at(
        &f,
        &domain,
        grid,
        &opts_with_mapping(
            Colormap::Turbo,
            None,
            mapping(
                Colormap::Turbo,
                ScaleMode::Linear,
                ColorQuantity::Magnitude,
                None,
            ),
        ),
        0.0,
    );
    let component = sample_field_at(
        &f,
        &domain,
        grid,
        &opts_with_mapping(
            Colormap::Turbo,
            None,
            mapping(
                Colormap::Turbo,
                ScaleMode::Linear,
                ColorQuantity::ComponentX,
                None,
            ),
        ),
        0.0,
    );
    assert_eq!(magnitude.len(), 2);
    assert_eq!(component.len(), 2);
    assert_eq!(
        magnitude[0].color, magnitude[1].color,
        "both ends have the same magnitude"
    );
    assert_ne!(
        component[0].color, component[1].color,
        "the signed component differs across the domain"
    );
    assert_eq!(
        component[0].color,
        Colormap::Turbo.from_unit(0.0),
        "the negative end lands at the bottom of the zero-centred range"
    );
    assert_eq!(
        component[1].color,
        Colormap::Turbo.from_unit(1.0),
        "the positive end lands at the top of the zero-centred range"
    );
}

#[test]
fn plane_direction_uses_the_projected_local_vector() {
    // Axis-X plane: local x = -world z, local y = world y. This world vector
    // projects to local (0, 1), i.e. 90 degrees, while its raw world angle is
    // atan2(1, -1) = 135 degrees. Using the drawn (projected) vector gives
    // t == 0.25.
    let field = FieldExpr::parse("-1, 1, 0").unwrap();
    let domain = Domain::new(Vec3::splat(-1.0), Vec3::splat(1.0));
    let frame = PlaneFrame::new(
        &domain,
        PlaneView {
            axis: PlaneAxis::X,
            index: 1,
            ..Default::default()
        },
        UVec3::splat(2),
    );
    let view = opts_with_mapping(
        Colormap::Turbo,
        None,
        mapping(
            Colormap::Turbo,
            ScaleMode::Linear,
            ColorQuantity::Direction2d,
            None,
        ),
    );
    let arrows = sample_plane_at(&field, &frame, 2, &view, 0.0);
    assert!(!arrows.is_empty(), "the projected field is non-degenerate");
    let expected = Colormap::Turbo.from_unit(0.25);
    for arrow in &arrows {
        assert_eq!(
            arrow.color, expected,
            "direction colour comes from the projected local dir {:?}",
            arrow.dir
        );
    }
}

#[test]
fn manual_range_is_honoured_and_auto_uses_the_sample_set() {
    let constant = FieldExpr::parse("1, 0, 0").unwrap();
    let domain = Domain::new(Vec3::splat(-1.0), Vec3::splat(1.0));
    let manual = sample_field_at(
        &constant,
        &domain,
        UVec3::new(2, 1, 1),
        &opts_with_mapping(
            Colormap::Mono,
            None,
            mapping(
                Colormap::Mono,
                ScaleMode::Linear,
                ColorQuantity::Magnitude,
                Some((0.0, 2.0)),
            ),
        ),
        0.0,
    );
    assert_eq!(manual.len(), 2);
    assert!(
        manual.iter().all(|a| a.color == [128, 128, 128]),
        "magnitude 1 in a manual 0..2 range is Mono's midpoint"
    );

    let varying = FieldExpr::parse("x, 0, 0").unwrap();
    let positive = Domain::new(Vec3::new(1.0, -1.0, -1.0), Vec3::new(2.0, 1.0, 1.0));
    let auto = sample_field_at(
        &varying,
        &positive,
        UVec3::new(2, 1, 1),
        &opts_with_mapping(
            Colormap::Mono,
            None,
            mapping(
                Colormap::Mono,
                ScaleMode::Linear,
                ColorQuantity::Magnitude,
                None,
            ),
        ),
        0.0,
    );
    assert_eq!(auto.len(), 2);
    for arrow in &auto {
        let t = (arrow.mag - 1.0) / (2.0 - 1.0);
        assert_eq!(
            arrow.color,
            Colormap::Mono.from_unit(t),
            "auto range fits the sampled magnitudes 1..2"
        );
    }
}

#[test]
fn non_finite_or_collapsed_ranges_fall_back_to_mid_grey() {
    let constant = FieldExpr::parse("1, 0, 0").unwrap();
    let domain = Domain::new(Vec3::splat(-1.0), Vec3::splat(1.0));
    let grid = UVec3::new(2, 1, 1);

    let non_finite = sample_field_at(
        &constant,
        &domain,
        grid,
        &opts_with_mapping(
            Colormap::Turbo,
            None,
            mapping(
                Colormap::Turbo,
                ScaleMode::Linear,
                ColorQuantity::Magnitude,
                Some((f32::NAN, 1.0)),
            ),
        ),
        0.0,
    );
    assert!(non_finite.iter().all(|a| a.color == [128, 128, 128]));

    let collapsed = sample_field_at(
        &constant,
        &domain,
        grid,
        &opts_with_mapping(
            Colormap::Turbo,
            None,
            mapping(
                Colormap::Turbo,
                ScaleMode::Linear,
                ColorQuantity::Magnitude,
                Some((1.0, 1.0)),
            ),
        ),
        0.0,
    );
    assert!(collapsed.iter().all(|a| a.color == [128, 128, 128]));

    let collapsed_auto = sample_field_at(
        &constant,
        &domain,
        grid,
        &opts_with_mapping(
            Colormap::Turbo,
            None,
            mapping(
                Colormap::Turbo,
                ScaleMode::Linear,
                ColorQuantity::Magnitude,
                None,
            ),
        ),
        0.0,
    );
    assert!(
        collapsed_auto.iter().all(|a| a.color == [128, 128, 128]),
        "a constant sample set collapses the auto range"
    );
}
