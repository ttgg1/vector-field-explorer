//! Issue #83 slice B1: pure colour-mapping model, quantity rules, legend
//! descriptor, clipping counts and the State/persistence seam.

use glam::Vec3;
use vector_field_explorer::color_mapping::{
    ClipCounts, ClipSide, ColorMapping, ColorPalette, ColorQuantity, ColorSample, ColorStats,
};
use vector_field_explorer::color_ramp::{
    ColorRamp, ColorStop, ColorStyle, RampKind, Rgb, ScaleMode,
};
use vector_field_explorer::colormap::Colormap;
use vector_field_explorer::viewmodel::State;

fn make_stats(min_magnitude: f32, max_magnitude: f32, max_abs_component: f32) -> ColorStats {
    ColorStats {
        min_magnitude,
        max_magnitude,
        max_abs_component,
    }
}

fn sample(magnitude: f32, dir: Vec3) -> ColorSample {
    ColorSample { magnitude, dir }
}

fn make_mapping(
    palette: ColorPalette,
    mode: ScaleMode,
    quantity: ColorQuantity,
    manual_range: Option<(f32, f32)>,
) -> ColorMapping {
    ColorMapping {
        palette,
        mode,
        quantity,
        manual_range,
    }
}

fn custom_ramp() -> ColorRamp {
    ColorRamp {
        stops: vec![
            ColorStop {
                position: 0.0,
                color: Rgb([10, 20, 30]),
            },
            ColorStop {
                position: 1.0,
                color: Rgb([200, 210, 220]),
            },
        ],
        reversed: false,
        kind: RampKind::Sequential,
    }
}

fn cyclic_ramp() -> ColorRamp {
    ColorRamp {
        stops: vec![
            ColorStop {
                position: 0.0,
                color: Rgb([0, 0, 0]),
            },
            ColorStop {
                position: 0.5,
                color: Rgb([255, 255, 255]),
            },
            ColorStop {
                position: 1.0,
                color: Rgb([0, 0, 0]),
            },
        ],
        reversed: false,
        kind: RampKind::Cyclic,
    }
}

#[test]
fn stats_from_samples_cover_magnitudes_and_components() {
    let samples = [
        sample(2.0, Vec3::X),
        sample(3.0, Vec3::Y),
        sample(1.0, -Vec3::Z),
    ];
    let stats = ColorStats::from_samples(&samples);
    assert_eq!(stats.min_magnitude, 1.0);
    assert_eq!(stats.max_magnitude, 3.0);
    assert_eq!(stats.max_abs_component, 3.0);
}

#[test]
fn empty_stats_are_zero() {
    let stats = ColorStats::from_samples(&[]);
    assert_eq!(stats.min_magnitude, 0.0);
    assert_eq!(stats.max_magnitude, 0.0);
    assert_eq!(stats.max_abs_component, 0.0);
}

#[test]
fn magnitude_auto_range_uses_the_sample_stats() {
    let mapping = make_mapping(
        ColorPalette::Builtin(Colormap::Viridis),
        ScaleMode::Linear,
        ColorQuantity::Magnitude,
        None,
    );
    assert_eq!(
        mapping.effective_range(&make_stats(1.5, 4.5, 9.0)),
        (1.5, 4.5)
    );
    assert_eq!(
        mapping.effective_range(&make_stats(1.5, 4.5, 9.0)),
        (1.5, 4.5),
        "magnitude ignores the component bound"
    );
}

#[test]
fn builtin_linear_bytes_match_colormap_exactly() {
    for colormap in Colormap::ALL {
        let mapping = make_mapping(
            ColorPalette::Builtin(colormap),
            ScaleMode::Linear,
            ColorQuantity::Magnitude,
            None,
        );
        let stats = make_stats(-2.0, 3.5, 3.5);
        let (min, max) = mapping.effective_range(&stats);
        for value in [-9.0, -2.0, -1.25, 0.0, 0.25, 1.0, 3.5, 11.0] {
            let mapped = mapping.sample(&sample(value, Vec3::X), &stats);
            assert_eq!(
                mapped.rgb,
                colormap.colormap(value, min, max),
                "{colormap:?} at {value} must stay byte-identical"
            );
        }
    }
}

#[test]
fn custom_ramp_overrides_the_builtin_palette() {
    let mapping = make_mapping(
        ColorPalette::Custom(custom_ramp()),
        ScaleMode::Linear,
        ColorQuantity::Magnitude,
        None,
    );
    let stats = make_stats(0.0, 1.0, 1.0);
    assert_eq!(
        mapping.sample(&sample(0.0, Vec3::X), &stats).rgb,
        [10, 20, 30]
    );
    assert_eq!(
        mapping.sample(&sample(1.0, Vec3::X), &stats).rgb,
        [200, 210, 220]
    );
    assert_ne!(
        mapping.sample(&sample(0.0, Vec3::X), &stats).rgb,
        Colormap::Viridis.colormap(0.0, 0.0, 1.0),
        "the custom ramp is not the selected builtin"
    );
}

#[test]
fn component_ranges_are_symmetric_and_values_are_signed() {
    let mapping = make_mapping(
        ColorPalette::Builtin(Colormap::Mono),
        ScaleMode::Linear,
        ColorQuantity::ComponentX,
        None,
    );
    let stats = make_stats(0.0, 4.0, 2.5);
    assert_eq!(mapping.effective_range(&stats), (-2.5, 2.5));

    let positive = mapping.sample(&sample(1.25, Vec3::X), &stats);
    let negative = mapping.sample(&sample(1.25, -Vec3::X), &stats);
    assert_eq!(positive.rgb, Colormap::Mono.colormap(1.25, -2.5, 2.5));
    assert_eq!(negative.rgb, Colormap::Mono.colormap(-1.25, -2.5, 2.5));
    assert_ne!(positive.rgb, negative.rgb);

    for (quantity, dir) in [
        (ColorQuantity::ComponentX, Vec3::X),
        (ColorQuantity::ComponentY, Vec3::Y),
        (ColorQuantity::ComponentZ, Vec3::Z),
    ] {
        let mapping = make_mapping(
            ColorPalette::Builtin(Colormap::Mono),
            ScaleMode::Linear,
            quantity,
            None,
        );
        assert_eq!(
            mapping.sample(&sample(1.25, dir), &stats).rgb,
            positive.rgb,
            "{quantity:?} reads its own axis"
        );
    }
}

#[test]
fn direction_uses_the_atan2_angle_and_ignores_the_range() {
    let mapping = make_mapping(
        ColorPalette::Builtin(Colormap::Mono),
        ScaleMode::Linear,
        ColorQuantity::Direction2d,
        Some((100.0, 200.0)),
    );
    let stats = make_stats(0.0, 1.0, 1.0);

    let right = mapping.sample(&sample(7.0, Vec3::X), &stats);
    let up = mapping.sample(&sample(7.0, Vec3::Y), &stats);
    let left = mapping.sample(&sample(7.0, -Vec3::X), &stats);
    let down = mapping.sample(&sample(7.0, -Vec3::Y), &stats);

    assert_eq!(right.rgb, [0, 0, 0]);
    assert_eq!(up.rgb, [64, 64, 64]);
    assert_eq!(left.rgb, [128, 128, 128]);
    assert_eq!(down.rgb, [191, 191, 191]);
    for mapped in [right, up, left, down] {
        assert_eq!(mapped.clipped, None);
    }

    let other_range = make_mapping(
        ColorPalette::Builtin(Colormap::Mono),
        ScaleMode::Linear,
        ColorQuantity::Direction2d,
        Some((-5.0, -1.0)),
    );
    assert_eq!(
        other_range.sample(&sample(7.0, Vec3::Y), &stats).rgb,
        up.rgb,
        "the manual range never changes a direction colour"
    );
}

#[test]
fn direction_is_unavailable_in_the_3d_view() {
    assert!(ColorQuantity::Direction2d.available(false).is_ok());
    assert_eq!(
        ColorQuantity::Direction2d.available(true).unwrap_err(),
        "Direction colouring is available in the 2D plane view only."
    );
    assert!(ColorQuantity::Magnitude.available(true).is_ok());
    assert!(ColorQuantity::ComponentZ.available(true).is_ok());
}

#[test]
fn signed_log_falls_back_with_an_explanation_when_the_range_does_not_span_zero() {
    let mapping = make_mapping(
        ColorPalette::Builtin(Colormap::Viridis),
        ScaleMode::SignedLog { half_width: 0.5 },
        ColorQuantity::Magnitude,
        None,
    );
    let stats = make_stats(0.0, 4.0, 4.0);
    assert_eq!(mapping.effective_range(&stats), (0.0, 4.0));
    let explanation = mapping.explanation(&stats).expect("disabled mode explains");
    assert!(
        explanation.contains("zero"),
        "the explanation names the missing zero span: {explanation}"
    );
    let mapped = mapping.sample(&sample(2.0, Vec3::X), &stats);
    assert_eq!(
        mapped.rgb,
        Colormap::Viridis.colormap(2.0, 0.0, 4.0),
        "disabled signed log falls back to linear on the same range"
    );
    assert_eq!(mapped.clipped, None);
    assert_eq!(
        mapping.legend(&stats).ticks,
        vec![0.0, 1.0, 2.0, 3.0, 4.0],
        "disabled signed log uses linear legend ticks"
    );
}

#[test]
fn usable_signed_log_maps_zero_to_the_ramp_centre() {
    let mapping = make_mapping(
        ColorPalette::Builtin(Colormap::Mono),
        ScaleMode::SignedLog { half_width: 1.0 },
        ColorQuantity::Magnitude,
        None,
    );
    let stats = make_stats(-10.0, 10.0, 10.0);
    assert_eq!(mapping.explanation(&stats), None);
    let centre = mapping.sample(&sample(0.0, Vec3::X), &stats);
    assert_eq!(centre.rgb, [128, 128, 128]);
    assert_eq!(centre.clipped, None);
    assert_eq!(
        mapping.sample(&sample(10.0, Vec3::X), &stats).rgb,
        [255, 255, 255]
    );
}

#[test]
fn clipping_reports_the_side_and_counts_samples() {
    let mapping = make_mapping(
        ColorPalette::Builtin(Colormap::Mono),
        ScaleMode::Linear,
        ColorQuantity::Magnitude,
        None,
    );
    let stats = make_stats(0.0, 1.0, 1.0);
    let samples = [
        sample(-0.5, Vec3::X),
        sample(0.0, Vec3::X),
        sample(0.5, Vec3::X),
        sample(1.0, Vec3::X),
        sample(2.0, Vec3::X),
    ];

    assert_eq!(
        mapping.sample(&samples[0], &stats).clipped,
        Some(ClipSide::Low)
    );
    assert_eq!(mapping.sample(&samples[1], &stats).clipped, None);
    assert_eq!(mapping.sample(&samples[2], &stats).clipped, None);
    assert_eq!(mapping.sample(&samples[3], &stats).clipped, None);
    assert_eq!(
        mapping.sample(&samples[4], &stats).clipped,
        Some(ClipSide::High)
    );
    assert_eq!(
        mapping.clip_counts(&samples, &stats),
        ClipCounts { low: 1, high: 1 }
    );
}

#[test]
fn direction_clip_counts_are_zero() {
    let mapping = make_mapping(
        ColorPalette::Builtin(Colormap::Mono),
        ScaleMode::Linear,
        ColorQuantity::Direction2d,
        None,
    );
    let stats = make_stats(0.0, 1.0, 1.0);
    let samples = [sample(-5.0, Vec3::X), sample(9.0, Vec3::Y)];
    assert_eq!(
        mapping.clip_counts(&samples, &stats),
        ClipCounts { low: 0, high: 0 }
    );
}

#[test]
fn legend_ticks_match_the_effective_range_and_mode() {
    let linear = make_mapping(
        ColorPalette::Builtin(Colormap::Turbo),
        ScaleMode::Linear,
        ColorQuantity::Magnitude,
        None,
    );
    let legend = linear.legend(&make_stats(2.0, 6.0, 6.0));
    assert_eq!(legend.kind, RampKind::Sequential);
    assert_eq!((legend.min, legend.max), (2.0, 6.0));
    assert_eq!(legend.ticks, vec![2.0, 3.0, 4.0, 5.0, 6.0]);
    assert!(!legend.cyclic);

    let signed = make_mapping(
        ColorPalette::Builtin(Colormap::Turbo),
        ScaleMode::SignedLog { half_width: 1.0 },
        ColorQuantity::Magnitude,
        None,
    );
    let legend = signed.legend(&make_stats(-4.0, 4.0, 4.0));
    assert_eq!(legend.ticks, vec![-4.0, -1.0, 0.0, 1.0, 4.0]);
    assert!(!legend.cyclic);

    let cyclic = make_mapping(
        ColorPalette::Custom(cyclic_ramp()),
        ScaleMode::Linear,
        ColorQuantity::Magnitude,
        None,
    );
    let legend = cyclic.legend(&make_stats(0.0, 1.0, 1.0));
    assert_eq!(legend.kind, RampKind::Cyclic);
    assert!(legend.cyclic);

    let direction = make_mapping(
        ColorPalette::Builtin(Colormap::Viridis),
        ScaleMode::Linear,
        ColorQuantity::Direction2d,
        None,
    );
    let legend = direction.legend(&make_stats(0.0, 1.0, 1.0));
    assert_eq!(legend.kind, RampKind::Cyclic);
    assert_eq!((legend.min, legend.max), (0.0, 360.0));
    assert_eq!(legend.ticks, vec![0.0, 90.0, 180.0, 270.0, 360.0]);
    assert!(legend.cyclic);
}

#[test]
fn legend_ticks_match_color_style_for_every_usable_mode() {
    // Linear parity.
    let linear = make_mapping(
        ColorPalette::Builtin(Colormap::Mono),
        ScaleMode::Linear,
        ColorQuantity::Magnitude,
        Some((-2.0, 3.0)),
    );
    let style = ColorStyle {
        ramp: ColorRamp::default(),
        mode: ScaleMode::Linear,
    };
    assert_eq!(
        linear.legend(&make_stats(0.0, 1.0, 1.0)).ticks,
        style.legend_ticks(-2.0, 3.0)
    );

    // Usable signed-log parity.
    let signed_mode = ScaleMode::SignedLog { half_width: 0.75 };
    let signed = make_mapping(
        ColorPalette::Builtin(Colormap::Mono),
        signed_mode,
        ColorQuantity::Magnitude,
        Some((-4.0, 4.0)),
    );
    let style = ColorStyle {
        ramp: ColorRamp::default(),
        mode: signed_mode,
    };
    assert_eq!(
        signed.legend(&make_stats(0.0, 1.0, 1.0)).ticks,
        style.legend_ticks(-4.0, 4.0)
    );

    // A disabled signed log uses the linear tick recipe.
    let disabled = make_mapping(
        ColorPalette::Builtin(Colormap::Mono),
        ScaleMode::SignedLog { half_width: 0.5 },
        ColorQuantity::Magnitude,
        Some((0.0, 3.0)),
    );
    let style = ColorStyle {
        ramp: ColorRamp::default(),
        mode: ScaleMode::Linear,
    };
    assert_eq!(
        disabled.legend(&make_stats(0.0, 1.0, 1.0)).ticks,
        style.legend_ticks(0.0, 3.0)
    );
}

#[test]
fn invalid_ranges_keep_finite_legend_bounds_and_no_ticks() {
    let mapping = make_mapping(
        ColorPalette::Builtin(Colormap::Mono),
        ScaleMode::Linear,
        ColorQuantity::Magnitude,
        Some((5.0, 5.0)),
    );
    let legend = mapping.legend(&make_stats(0.0, 1.0, 1.0));
    assert!(legend.ticks.is_empty());
    assert!(legend.min.is_finite());
    assert!(legend.max.is_finite());

    let mapping = make_mapping(
        ColorPalette::Builtin(Colormap::Mono),
        ScaleMode::Linear,
        ColorQuantity::Magnitude,
        Some((f32::NAN, 5.0)),
    );
    let legend = mapping.legend(&make_stats(0.0, 1.0, 1.0));
    assert!(legend.ticks.is_empty());
    assert!(legend.min.is_finite());
    assert!(legend.max.is_finite());
}

#[test]
fn non_finite_samples_are_mid_gray() {
    let magnitude = make_mapping(
        ColorPalette::Builtin(Colormap::Viridis),
        ScaleMode::Linear,
        ColorQuantity::Magnitude,
        None,
    );
    let stats = make_stats(0.0, 1.0, 1.0);
    assert_eq!(
        magnitude.sample(&sample(f32::NAN, Vec3::X), &stats).rgb,
        [128, 128, 128]
    );
    assert_eq!(
        magnitude
            .sample(&sample(f32::INFINITY, Vec3::X), &stats)
            .rgb,
        [128, 128, 128]
    );
    assert_eq!(
        magnitude
            .sample(&sample(0.5, Vec3::X), &make_stats(f32::NAN, 1.0, 1.0))
            .rgb,
        [128, 128, 128]
    );

    let direction = make_mapping(
        ColorPalette::Builtin(Colormap::Viridis),
        ScaleMode::Linear,
        ColorQuantity::Direction2d,
        None,
    );
    assert_eq!(
        direction
            .sample(&sample(1.0, Vec3::new(f32::NAN, 0.0, 0.0)), &stats)
            .rgb,
        [128, 128, 128]
    );
}

#[test]
fn validate_rejects_a_custom_palette_with_an_invalid_ramp() {
    let invalid = ColorRamp {
        stops: vec![ColorStop {
            position: 0.0,
            color: Rgb([0, 0, 0]),
        }],
        reversed: false,
        kind: RampKind::Sequential,
    };
    let mapping = make_mapping(
        ColorPalette::Custom(invalid),
        ScaleMode::Linear,
        ColorQuantity::Magnitude,
        None,
    );
    assert_eq!(
        mapping.validate().unwrap_err(),
        "ramp needs at least 2 stops, got 1",
        "the rejection names the violated ramp rule"
    );
}

#[test]
fn builtin_palettes_always_validate() {
    for colormap in Colormap::ALL {
        let mapping = make_mapping(
            ColorPalette::Builtin(colormap),
            ScaleMode::Linear,
            ColorQuantity::Magnitude,
            None,
        );
        assert!(
            mapping.validate().is_ok(),
            "{colormap:?} is always a valid palette"
        );
    }
}

#[test]
fn validate_rejects_non_finite_manual_range_bounds() {
    for (min, max) in [
        (f32::NAN, 1.0),
        (0.0, f32::NAN),
        (f32::INFINITY, 1.0),
        (0.0, f32::INFINITY),
        (f32::NEG_INFINITY, 1.0),
        (0.0, f32::NEG_INFINITY),
    ] {
        let mapping = make_mapping(
            ColorPalette::Builtin(Colormap::Viridis),
            ScaleMode::Linear,
            ColorQuantity::Magnitude,
            Some((min, max)),
        );
        assert_eq!(
            mapping.validate().unwrap_err(),
            "manual colour range bounds must be finite",
            "({min}, {max}) must be rejected as non-finite"
        );
    }
}

#[test]
fn validate_rejects_a_reversed_manual_range_and_accepts_a_collapsed_one() {
    let reversed = make_mapping(
        ColorPalette::Builtin(Colormap::Viridis),
        ScaleMode::Linear,
        ColorQuantity::Magnitude,
        Some((2.0, 1.0)),
    );
    assert_eq!(
        reversed.validate().unwrap_err(),
        "manual colour range min must be at most max"
    );

    let collapsed = make_mapping(
        ColorPalette::Builtin(Colormap::Viridis),
        ScaleMode::Linear,
        ColorQuantity::Magnitude,
        Some((1.0, 1.0)),
    );
    assert!(
        collapsed.validate().is_ok(),
        "min == max is a usable frozen point"
    );
}

#[test]
fn validate_never_mutates_the_mapping() {
    let valid = make_mapping(
        ColorPalette::Custom(custom_ramp()),
        ScaleMode::SignedLog { half_width: 0.5 },
        ColorQuantity::ComponentY,
        Some((1.0, 2.0)),
    );
    let before = valid.clone();
    assert!(valid.validate().is_ok());
    assert_eq!(valid, before, "a successful validation is read-only");

    let invalid = make_mapping(
        ColorPalette::Custom(ColorRamp {
            stops: Vec::new(),
            reversed: false,
            kind: RampKind::Sequential,
        }),
        ScaleMode::Linear,
        ColorQuantity::Magnitude,
        Some((2.0, 1.0)),
    );
    let before = invalid.clone();
    assert!(invalid.validate().is_err());
    assert_eq!(invalid, before, "a rejected validation is read-only");
}

#[test]
fn palette_kind_is_sequential_for_builtins_and_follows_custom_ramps() {
    for colormap in Colormap::ALL {
        assert_eq!(
            ColorPalette::Builtin(colormap).kind(),
            RampKind::Sequential,
            "{colormap:?} is a sequential legend ramp"
        );
    }
    for kind in RampKind::ALL {
        let ramp = ColorRamp {
            kind,
            ..custom_ramp()
        };
        assert_eq!(ColorPalette::Custom(ramp).kind(), kind);
    }
}

#[test]
fn tiny_negative_direction_angle_uses_the_first_stop_colour() {
    let mapping = make_mapping(
        ColorPalette::Custom(custom_ramp()),
        ScaleMode::Linear,
        ColorQuantity::Direction2d,
        None,
    );
    let stats = make_stats(0.0, 1.0, 1.0);
    let mapped = mapping.sample(&sample(1.0, Vec3::new(1.0, -1e-12, 0.0)), &stats);
    assert_eq!(
        mapped.rgb,
        [10, 20, 30],
        "an angle rounding up to t == 1.0 folds back to the 0.0 seam"
    );
}

#[test]
fn set_mode_folds_direction_colouring_back_to_magnitude_in_3d() {
    let mut state = State::new(false);
    assert!(state.set_color_quantity(ColorQuantity::Direction2d));
    assert!(state.set_mode(true));
    assert_eq!(
        state.color_quantity,
        ColorQuantity::Magnitude,
        "the 3D mode change also resets the unusable quantity"
    );

    let mut flat = State::new(false);
    assert!(flat.set_color_quantity(ColorQuantity::Direction2d));
    assert!(!flat.set_mode(false));
    assert_eq!(
        flat.color_quantity,
        ColorQuantity::Direction2d,
        "the 2D view keeps direction colouring"
    );
}

#[test]
fn state_color_mapping_reflects_the_selected_fields() {
    let mut state = State::new(false);
    assert_eq!(
        state.color_mapping(),
        ColorMapping {
            palette: ColorPalette::Builtin(Colormap::Viridis),
            mode: ScaleMode::Linear,
            quantity: ColorQuantity::Magnitude,
            manual_range: None,
        }
    );

    let ramp = custom_ramp();
    assert!(state.set_color_ramp(Some(ramp.clone())));
    assert!(state.set_color_mode(ScaleMode::SignedLog { half_width: 0.5 }));
    assert!(state.set_color_quantity(ColorQuantity::ComponentY));
    assert!(state.set_auto_fit(false));
    assert!(state.set_color_range(1.0, 2.0));
    assert_eq!(
        state.color_mapping(),
        ColorMapping {
            palette: ColorPalette::Custom(ramp),
            mode: ScaleMode::SignedLog { half_width: 0.5 },
            quantity: ColorQuantity::ComponentY,
            manual_range: Some((1.0, 2.0)),
        }
    );
}

#[test]
fn color_setters_validate_guard_noops_and_dirty_once() {
    let mut state = State::new(false);
    state.dirty = false;

    assert!(!state.set_color_mode(ScaleMode::Linear));
    assert!(!state.dirty, "an equal mode commit is not dirty");
    assert!(state.set_color_mode(ScaleMode::SignedLog { half_width: 0.5 }));
    assert!(state.dirty);
    state.dirty = false;
    assert!(!state.set_color_mode(ScaleMode::SignedLog { half_width: 0.5 }));
    assert!(!state.dirty);

    assert!(!state.set_color_quantity(ColorQuantity::Magnitude));
    assert!(!state.dirty, "an equal quantity commit is not dirty");
    assert!(state.set_color_quantity(ColorQuantity::ComponentX));
    assert!(state.dirty);
    state.dirty = false;
    assert!(!state.set_color_quantity(ColorQuantity::ComponentX));
    assert!(!state.dirty);

    let ramp = custom_ramp();
    assert!(state.set_color_ramp(Some(ramp.clone())));
    assert!(state.dirty);
    state.dirty = false;
    assert!(!state.set_color_ramp(Some(ramp.clone())));
    assert!(!state.dirty, "an equal ramp commit is not dirty");

    let invalid = ColorRamp {
        stops: Vec::new(),
        reversed: false,
        kind: RampKind::Sequential,
    };
    state.dirty = false;
    assert!(!state.set_color_ramp(Some(invalid)));
    assert!(!state.dirty, "a rejected ramp does not dirty");
    assert_eq!(state.color_ramp, Some(ramp));
}

#[test]
fn direction_quantity_is_rejected_by_the_setter_in_3d() {
    let mut state = State::new(true);
    state.dirty = false;
    assert!(!state.set_color_quantity(ColorQuantity::Direction2d));
    assert!(!state.dirty);
    assert_eq!(state.color_quantity, ColorQuantity::Magnitude);

    let mut flat = State::new(false);
    assert!(flat.set_color_quantity(ColorQuantity::Direction2d));
}

#[test]
fn selecting_a_builtin_clears_a_custom_ramp() {
    let mut state = State::new(false);
    assert!(state.set_color_ramp(Some(custom_ramp())));
    state.dirty = false;
    assert!(state.set_colormap(Colormap::Viridis));
    assert!(state.color_ramp.is_none(), "the custom override is cleared");
    assert!(state.dirty);

    state.dirty = false;
    assert!(!state.set_colormap(Colormap::Viridis));
    assert!(!state.dirty, "the same builtin with no override is a no-op");

    assert!(state.set_color_ramp(Some(custom_ramp())));
    state.dirty = false;
    assert!(state.set_colormap(Colormap::Turbo));
    assert!(state.color_ramp.is_none());
    assert!(state.dirty);
}

#[test]
fn freezes_commit_the_current_effective_range_once() {
    let mut state = State::new(false);
    state.dirty = false;
    let stats = make_stats(2.0, 6.0, 6.0);
    assert!(state.freeze_color_range(&stats));
    assert!(!state.auto_fit);
    assert_eq!((state.color_min, state.color_max), (2.0, 6.0));
    assert!(state.dirty);

    state.dirty = false;
    assert!(!state.freeze_color_range(&stats));
    assert!(!state.dirty, "freezing at the committed range is a no-op");

    state.color_quantity = ColorQuantity::Direction2d;
    state.auto_fit = true;
    state.dirty = false;
    assert!(!state.freeze_color_range(&stats));
    assert!(!state.dirty, "direction has no freezable range");
}

#[test]
fn is_legacy_equivalent_only_accepts_the_default_builtin_path() {
    let default = make_mapping(
        ColorPalette::Builtin(Colormap::Viridis),
        ScaleMode::Linear,
        ColorQuantity::Magnitude,
        None,
    );
    assert!(default.is_legacy_equivalent(Colormap::Viridis));
    assert!(
        !default.is_legacy_equivalent(Colormap::Turbo),
        "a different builtin is not the legacy path"
    );

    assert!(
        !make_mapping(
            ColorPalette::Custom(custom_ramp()),
            ScaleMode::Linear,
            ColorQuantity::Magnitude,
            None,
        )
        .is_legacy_equivalent(Colormap::Viridis),
        "a custom palette is not the legacy path"
    );
    assert!(
        !make_mapping(
            ColorPalette::Builtin(Colormap::Viridis),
            ScaleMode::SignedLog { half_width: 0.5 },
            ColorQuantity::Magnitude,
            None,
        )
        .is_legacy_equivalent(Colormap::Viridis),
        "a non-linear mode is not the legacy path"
    );
    for quantity in [
        ColorQuantity::ComponentX,
        ColorQuantity::ComponentY,
        ColorQuantity::ComponentZ,
        ColorQuantity::Direction2d,
    ] {
        assert!(
            !make_mapping(
                ColorPalette::Builtin(Colormap::Viridis),
                ScaleMode::Linear,
                quantity,
                None,
            )
            .is_legacy_equivalent(Colormap::Viridis),
            "{quantity:?} is not the legacy path"
        );
    }
    assert!(
        !make_mapping(
            ColorPalette::Builtin(Colormap::Viridis),
            ScaleMode::Linear,
            ColorQuantity::Magnitude,
            Some((0.0, 1.0)),
        )
        .is_legacy_equivalent(Colormap::Viridis),
        "a manual range is not the legacy path"
    );
}

#[test]
fn view_options_forward_only_a_non_legacy_mapping() {
    let mut state = State::new(false);
    assert!(
        state.view_options().color_mapping.is_none(),
        "the default Viridis/Magnitude/Linear path stays legacy"
    );

    assert!(state.set_color_quantity(ColorQuantity::ComponentX));
    assert_eq!(
        state
            .view_options()
            .color_mapping
            .expect("a component quantity is non-legacy")
            .quantity,
        ColorQuantity::ComponentX
    );

    assert!(state.set_color_quantity(ColorQuantity::Magnitude));
    assert!(state.set_auto_fit(false));
    assert!(state.set_color_range(1.0, 2.0));
    assert!(
        state.view_options().color_mapping.is_some(),
        "a manual range is not legacy-equivalent"
    );

    assert!(state.set_auto_fit(true));
    assert!(state.set_color_ramp(Some(custom_ramp())));
    assert!(
        state.view_options().color_mapping.is_some(),
        "a custom palette is not legacy-equivalent"
    );
    assert!(state.set_colormap(Colormap::Viridis));
    assert!(
        state.view_options().color_mapping.is_none(),
        "clearing the custom ramp returns to the legacy path"
    );
}
