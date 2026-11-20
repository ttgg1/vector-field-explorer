//! Color-ramp and scale foundation (issue #83 slice A).
//!
//! Exercises the pure, panic-free model that a later slice will wire into the
//! sidebar, legends and sampling. No UI or renderer code is involved.

use vector_field_explorer::colormap::Colormap;
use vector_field_explorer::color_ramp::{
    ColorRamp, ColorStop, ColorStyle, RampKind, Rgb, ScaleMode,
};

fn stop(position: f32, hex: &str) -> ColorStop {
    ColorStop {
        position,
        color: Rgb::from_hex(hex).expect("valid test fixture hex"),
    }
}

fn ramp(stops: Vec<ColorStop>, kind: RampKind) -> ColorRamp {
    ColorRamp {
        stops,
        reversed: false,
        kind,
    }
}

fn black_to_white() -> ColorRamp {
    ramp(
        vec![stop(0.0, "#000000"), stop(1.0, "#ffffff")],
        RampKind::Sequential,
    )
}

#[test]
fn hex_round_trip_and_canonical_output() {
    let rgb = Rgb::from_hex("#1A2b3C").unwrap();
    assert_eq!(rgb, Rgb([0x1a, 0x2b, 0x3c]));
    assert_eq!(rgb.to_hex(), "#1a2b3c");

    let bare = Rgb::from_hex("00FF7f").unwrap();
    assert_eq!(bare, Rgb([0, 255, 127]));
    assert_eq!(bare.to_hex(), "#00ff7f");

    assert_eq!(Rgb([0, 0, 0]).to_hex(), "#000000");
    assert_eq!(Rgb([255, 255, 255]).to_hex(), "#ffffff");
}

#[test]
fn malformed_hex_is_rejected() {
    for bad in [
        "", "#", "12345", "1234567", "#12345", "#1234567", "#gg0000", "#12 456",
        "#-12345", "#0x1234", " ##1234", "éééééé",
    ] {
        assert_eq!(Rgb::from_hex(bad), None, "{bad:?} should not parse");
    }
}

#[test]
fn rgb_default_is_black_and_serde_round_trips() {
    assert_eq!(Rgb::default(), Rgb([0, 0, 0]));

    let json = serde_json::to_string(&Rgb([1, 2, 3])).unwrap();
    assert_eq!(json, "[1,2,3]");
    let back: Rgb = serde_json::from_str(&json).unwrap();
    assert_eq!(back, Rgb([1, 2, 3]));
}

#[test]
fn ramp_kind_defaults_labels_and_all() {
    assert_eq!(RampKind::default(), RampKind::Sequential);
    assert_eq!(RampKind::Sequential.label(), "Sequential");
    assert_eq!(RampKind::Diverging.label(), "Diverging");
    assert_eq!(RampKind::Cyclic.label(), "Cyclic");
    assert_eq!(RampKind::ALL.len(), 3);
    for (i, kind) in RampKind::ALL.iter().enumerate() {
        assert!(RampKind::ALL[..i].iter().all(|other| other != kind));
        assert!(!kind.label().is_empty());
    }
}

#[test]
fn color_ramp_default_is_valid_and_sequential() {
    let default = ColorRamp::default();
    assert_eq!(default.kind, RampKind::Sequential);
    assert!(!default.reversed);
    assert!(default.validate().is_ok());
    assert_eq!(default.stops.len(), 2);
    assert_eq!(default.stops[0].position, 0.0);
    assert_eq!(default.stops[1].position, 1.0);
}

#[test]
fn ramp_serde_round_trips() {
    let original = ramp(
        vec![
            stop(0.0, "#000000"),
            stop(0.5, "#ff0000"),
            stop(1.0, "#ffffff"),
        ],
        RampKind::Diverging,
    );
    let json = serde_json::to_string(&original).unwrap();
    let back: ColorRamp = serde_json::from_str(&json).unwrap();
    assert_eq!(back, original);

    let mode = ScaleMode::SignedLog { half_width: 2.5 };
    let json = serde_json::to_string(&mode).unwrap();
    let back: ScaleMode = serde_json::from_str(&json).unwrap();
    assert_eq!(back, mode);
}

#[test]
fn stop_count_bounds_are_enforced() {
    assert_eq!(ColorRamp::MIN_STOPS, 2);
    assert_eq!(ColorRamp::MAX_STOPS, 16);

    let one = ramp(vec![stop(0.0, "#000000")], RampKind::Sequential);
    assert!(one.validate().unwrap_err().contains("at least"));

    let mut too_many = Vec::new();
    for i in 0..=ColorRamp::MAX_STOPS {
        too_many.push(stop(i as f32 / ColorRamp::MAX_STOPS as f32, "#000000"));
    }
    let too_many = ramp(too_many, RampKind::Sequential);
    assert_eq!(too_many.stops.len(), ColorRamp::MAX_STOPS + 1);
    assert!(too_many.validate().unwrap_err().contains("at most"));

    let sixteen: Vec<ColorStop> = (0..ColorRamp::MAX_STOPS)
        .map(|i| stop(i as f32 / (ColorRamp::MAX_STOPS - 1) as f32, "#000000"))
        .collect();
    let sixteen = ramp(sixteen, RampKind::Sequential);
    assert_eq!(sixteen.stops.len(), ColorRamp::MAX_STOPS);
    assert!(sixteen.validate().is_ok());
}

#[test]
fn unsorted_and_duplicate_positions_are_rejected() {
    let unsorted = ramp(
        vec![
            stop(0.0, "#000000"),
            stop(0.6, "#ff0000"),
            stop(0.4, "#00ff00"),
            stop(1.0, "#ffffff"),
        ],
        RampKind::Sequential,
    );
    assert!(
        unsorted
            .validate()
            .unwrap_err()
            .contains("strictly increasing")
    );

    let duplicate = ramp(
        vec![
            stop(0.0, "#000000"),
            stop(0.5, "#ff0000"),
            stop(0.5, "#00ff00"),
            stop(1.0, "#ffffff"),
        ],
        RampKind::Sequential,
    );
    assert!(
        duplicate
            .validate()
            .unwrap_err()
            .contains("strictly increasing")
    );
}

#[test]
fn out_of_range_and_non_finite_positions_are_rejected() {
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let invalid = ramp(
            vec![
                stop(0.0, "#000000"),
                stop(bad, "#ff0000"),
                stop(1.0, "#ffffff"),
            ],
            RampKind::Sequential,
        );
        assert!(
            invalid.validate().unwrap_err().contains("finite"),
            "position {bad} should be rejected as non-finite"
        );
    }

    for bad in [-0.1, 1.1] {
        let invalid = ramp(
            vec![
                stop(0.0, "#000000"),
                stop(bad, "#ff0000"),
                stop(1.0, "#ffffff"),
            ],
            RampKind::Sequential,
        );
        assert!(
            invalid.validate().unwrap_err().contains("0.0..=1.0"),
            "position {bad} should be rejected as out of range"
        );
    }
}

#[test]
fn endpoints_must_be_exactly_zero_and_one() {
    let no_first = ramp(
        vec![stop(0.25, "#000000"), stop(1.0, "#ffffff")],
        RampKind::Sequential,
    );
    assert!(no_first.validate().unwrap_err().contains("0.0"));

    let no_last = ramp(
        vec![stop(0.0, "#000000"), stop(0.75, "#ffffff")],
        RampKind::Sequential,
    );
    assert!(no_last.validate().unwrap_err().contains("1.0"));
}

#[test]
fn diverging_kind_needs_three_stops_a_centre_and_distinct_ends() {
    let valid = ramp(
        vec![
            stop(0.0, "#0000ff"),
            stop(0.5, "#ffffff"),
            stop(1.0, "#ff0000"),
        ],
        RampKind::Diverging,
    );
    assert!(valid.validate().is_ok());

    let too_few = ramp(
        vec![stop(0.0, "#0000ff"), stop(1.0, "#ff0000")],
        RampKind::Diverging,
    );
    assert!(too_few.validate().unwrap_err().contains("at least 3"));

    let no_centre = ramp(
        vec![
            stop(0.0, "#0000ff"),
            stop(0.4, "#ffffff"),
            stop(1.0, "#ff0000"),
        ],
        RampKind::Diverging,
    );
    assert!(no_centre.validate().unwrap_err().contains("0.5"));

    let same_ends = ramp(
        vec![
            stop(0.0, "#0000ff"),
            stop(0.5, "#ffffff"),
            stop(1.0, "#0000ff"),
        ],
        RampKind::Diverging,
    );
    assert!(
        same_ends
            .validate()
            .unwrap_err()
            .contains("different endpoint")
    );
}

#[test]
fn cyclic_kind_needs_three_stops_and_identical_ends() {
    let valid = ramp(
        vec![
            stop(0.0, "#ff0000"),
            stop(0.5, "#00ff00"),
            stop(1.0, "#ff0000"),
        ],
        RampKind::Cyclic,
    );
    assert!(valid.validate().is_ok());

    let too_few = ramp(
        vec![stop(0.0, "#ff0000"), stop(1.0, "#ff0000")],
        RampKind::Cyclic,
    );
    assert!(too_few.validate().unwrap_err().contains("at least 3"));

    let different_ends = ramp(
        vec![
            stop(0.0, "#ff0000"),
            stop(0.5, "#00ff00"),
            stop(1.0, "#0000ff"),
        ],
        RampKind::Cyclic,
    );
    assert!(
        different_ends
            .validate()
            .unwrap_err()
            .contains("identical endpoint")
    );
}

#[test]
fn stop_positions_return_exact_colours() {
    let ramp = ramp(
        vec![
            stop(0.0, "#ff0000"),
            stop(0.5, "#00ff00"),
            stop(1.0, "#0000ff"),
        ],
        RampKind::Sequential,
    );
    assert_eq!(ramp.sample(0.0), Rgb([255, 0, 0]));
    assert_eq!(ramp.sample(0.5), Rgb([0, 255, 0]));
    assert_eq!(ramp.sample(1.0), Rgb([0, 0, 255]));
}

#[test]
fn interpolation_rounds_each_channel() {
    let gray = black_to_white();
    assert_eq!(gray.sample(0.5), Rgb([128, 128, 128]));
    assert_eq!(gray.sample(0.25), Rgb([64, 64, 64]));
    assert_eq!(gray.sample(0.75), Rgb([191, 191, 191]));

    let colour = ramp(
        vec![
            stop(0.0, "#ff0000"),
            stop(0.5, "#00ff00"),
            stop(1.0, "#0000ff"),
        ],
        RampKind::Sequential,
    );
    assert_eq!(colour.sample(0.25), Rgb([128, 128, 0]));
}

#[test]
fn reversed_samples_mirror_and_stay_exact() {
    let mut ramp = black_to_white();
    ramp.reversed = true;
    assert_eq!(ramp.sample(0.0), Rgb([255, 255, 255]));
    assert_eq!(ramp.sample(1.0), Rgb([0, 0, 0]));
    assert_eq!(ramp.sample(0.25), Rgb([191, 191, 191]));
    assert_eq!(ramp.sample(0.5), Rgb([128, 128, 128]));
    assert_eq!(ramp.sample(0.75), Rgb([64, 64, 64]));
}

#[test]
fn sample_clamps_and_treats_nan_as_zero() {
    let ramp = black_to_white();
    assert_eq!(ramp.sample(-5.0), Rgb([0, 0, 0]));
    assert_eq!(ramp.sample(5.0), Rgb([255, 255, 255]));
    assert_eq!(ramp.sample(f32::NEG_INFINITY), Rgb([0, 0, 0]));
    assert_eq!(ramp.sample(f32::INFINITY), Rgb([255, 255, 255]));
    assert_eq!(ramp.sample(f32::NAN), Rgb([0, 0, 0]));

    let mut reversed = ramp;
    reversed.reversed = true;
    assert_eq!(reversed.sample(f32::NAN), Rgb([255, 255, 255]));
}

// The two-stop Mono seed is structurally exact, but the legacy reference
// computes `(255.0 * t + 0.5) as u8` in f32 while `ColorRamp::sample`
// interpolates and rounds in f64. At f32 half-step boundaries the f32 product
// rounds up to exactly 178.5/229.5 while the f64 product stays just below, so
// only t = 0.7 and t = 0.9 differ, each by one channel value. This is the
// reference's arithmetic, not the stop choice.
#[test]
fn mono_builtin_stays_within_one_lsb_of_the_colormap() {
    let ramp = ColorRamp::builtin(Colormap::Mono);
    assert_eq!(ramp.stops.len(), 2);
    assert_eq!(ramp.kind, RampKind::Sequential);
    assert_eq!(ramp.stops[0].position, 0.0);
    assert_eq!(ramp.stops[0].color, Rgb([0, 0, 0]));
    assert_eq!(ramp.stops[1].position, 1.0);
    assert_eq!(ramp.stops[1].color, Rgb([255, 255, 255]));
    assert!(ramp.validate().is_ok());

    for t in [0.0, 1.0] {
        assert_eq!(
            ramp.sample(t),
            Rgb(Colormap::Mono.colormap(t, 0.0, 1.0)),
            "endpoint t={t}"
        );
    }

    for k in 0..=100 {
        let t = k as f32 / 100.0;
        let got = ramp.sample(t);
        let want = Colormap::Mono.colormap(t, 0.0, 1.0);
        for channel in 0..3 {
            let delta = (got.0[channel] as i16 - want[channel] as i16).abs();
            assert!(
                delta <= 1,
                "t={t} channel {channel}: {} differs from {} by {delta}",
                got.to_hex(),
                Rgb(want).to_hex()
            );
        }
    }
}

#[test]
fn builtin_matches_viridis_and_turbo_at_both_endpoints() {
    for colormap in [Colormap::Viridis, Colormap::Turbo] {
        let ramp = ColorRamp::builtin(colormap);
        assert_eq!(ramp.stops.len(), 9);
        assert_eq!(ramp.sample(0.0), Rgb(colormap.colormap(0.0, 0.0, 1.0)));
        assert_eq!(ramp.sample(1.0), Rgb(colormap.colormap(1.0, 0.0, 1.0)));
        for (k, stop) in ramp.stops.iter().enumerate() {
            let t = k as f32 / 8.0;
            assert_eq!(stop.position, t);
            assert_eq!(stop.color, Rgb(colormap.colormap(t, 0.0, 1.0)));
        }
    }
}

#[test]
fn linear_scaling_normalizes_and_clamps() {
    let mode = ScaleMode::Linear;
    assert_eq!(mode.normalize(0.0, 0.0, 10.0), Some(0.0));
    assert_eq!(mode.normalize(5.0, 0.0, 10.0), Some(0.5));
    assert_eq!(mode.normalize(10.0, 0.0, 10.0), Some(1.0));
    assert_eq!(mode.normalize(-5.0, 0.0, 10.0), Some(0.0));
    assert_eq!(mode.normalize(50.0, 0.0, 10.0), Some(1.0));
    assert_eq!(mode.normalize(2.0, 2.0, 4.0), Some(0.0));
}

#[test]
fn empty_and_non_finite_ranges_return_none() {
    let mode = ScaleMode::Linear;
    assert_eq!(mode.normalize(0.0, 1.0, 1.0), None);
    assert_eq!(mode.normalize(0.0, 2.0, 1.0), None);
    assert_eq!(mode.normalize(f32::NAN, 0.0, 1.0), None);
    assert_eq!(mode.normalize(0.0, f32::NAN, 1.0), None);
    assert_eq!(mode.normalize(0.0, 0.0, f32::INFINITY), None);

    let signed = ScaleMode::SignedLog { half_width: 1.0 };
    assert_eq!(signed.normalize(f32::NAN, -1.0, 1.0), None);
    assert_eq!(signed.normalize(0.0, -1.0, f32::NAN), None);
}

#[test]
fn scale_mode_validate_rejects_invalid_half_widths() {
    assert!(ScaleMode::Linear.validate().is_ok());
    for bad in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1.0e6 + 1.0] {
        let mode = ScaleMode::SignedLog { half_width: bad };
        assert!(mode.validate().is_err(), "half_width {bad} should be invalid");
    }
    assert!(
        ScaleMode::SignedLog { half_width: 1.0e6 }
            .validate()
            .is_ok()
    );
    assert!(
        ScaleMode::SignedLog { half_width: 0.25 }
            .validate()
            .is_ok()
    );
}

#[test]
fn signed_log_symmetry_and_endpoints() {
    let mode = ScaleMode::SignedLog { half_width: 1.0 };
    assert_eq!(mode.normalize(0.0, -10.0, 10.0), Some(0.5));
    assert_eq!(mode.normalize(-10.0, -10.0, 10.0), Some(0.0));
    assert_eq!(mode.normalize(10.0, -10.0, 10.0), Some(1.0));

    for k in 1..20 {
        let v = k as f32 * 0.5;
        let lo = mode.normalize(-v, -10.0, 10.0).unwrap();
        let hi = mode.normalize(v, -10.0, 10.0).unwrap();
        assert!((lo + hi - 1.0).abs() < 1.0e-6, "v={v}: {lo} + {hi}");
    }
}

#[test]
fn signed_log_is_monotone_and_continuous() {
    let mode = ScaleMode::SignedLog { half_width: 0.5 };
    let mut previous = f32::NEG_INFINITY;
    for k in 0..=400 {
        let v = -10.0 + k as f32 * 0.05;
        let t = mode.normalize(v, -10.0, 10.0).unwrap();
        assert!(t.is_finite());
        assert!((0.0..=1.0).contains(&t));
        assert!(t >= previous - 1.0e-6, "not monotone at v={v}");
        previous = t;
    }

    for edge in [-0.5f32, 0.0, 0.5] {
        let eps = 1.0e-4;
        let lo = mode.normalize(edge - eps, -10.0, 10.0).unwrap();
        let at = mode.normalize(edge, -10.0, 10.0).unwrap();
        let hi = mode.normalize(edge + eps, -10.0, 10.0).unwrap();
        assert!((lo - at).abs() < 1.0e-3, "jump below {edge}");
        assert!((hi - at).abs() < 1.0e-3, "jump above {edge}");
    }
}

#[test]
fn signed_log_rejects_ranges_that_do_not_span_zero() {
    let mode = ScaleMode::SignedLog { half_width: 1.0 };
    assert_eq!(mode.normalize(0.5, 0.0, 1.0), None);
    assert_eq!(mode.normalize(-0.5, -1.0, 0.0), None);
    assert_eq!(mode.normalize(0.5, 1.0, 2.0), None);
    assert_eq!(mode.normalize(0.5, -2.0, -1.0), None);
}

#[test]
fn signed_log_zero_is_centred_even_when_the_range_underflows() {
    let mode = ScaleMode::SignedLog { half_width: 1.0e6 };
    let (min, max) = (-1.0e-30, 1.0e-30);

    assert_eq!(mode.normalize(0.0, min, max), Some(0.5));
    assert_eq!(mode.normalize(-0.0, min, max), Some(0.5));

    // The endpoint scales underflow to zero; non-zero values clip instead of
    // inheriting a 0/0 NaN.
    assert_eq!(mode.normalize(min, min, max), Some(0.0));
    assert_eq!(mode.normalize(max, min, max), Some(1.0));
    assert_eq!(mode.normalize(-1.0e-31, min, max), Some(0.0));
    assert_eq!(mode.normalize(1.0e-31, min, max), Some(1.0));
}

#[test]
fn signed_log_legend_ticks_stay_inside_the_range() {
    let style = |half_width| ColorStyle {
        ramp: black_to_white(),
        mode: ScaleMode::SignedLog { half_width },
    };

    let inside = style(2.0).legend_ticks(-10.0, 10.0);
    assert_eq!(inside, vec![-10.0, -2.0, 0.0, 2.0, 10.0]);

    let larger = style(20.0).legend_ticks(-10.0, 10.0);
    assert_eq!(larger, vec![-10.0, 0.0, 10.0]);

    let small_range = style(1.0).legend_ticks(-0.5, 0.5);
    assert_eq!(small_range, vec![-0.5, 0.0, 0.5]);

    for half_width in [0.001, 0.5, 1.0, 2.0, 20.0, 1.0e6] {
        let ticks = style(half_width).legend_ticks(-10.0, 10.0);
        assert!(ticks.len() <= 9, "half_width {half_width}");
        assert_eq!(ticks[0], -10.0, "half_width {half_width}");
        assert_eq!(*ticks.last().unwrap(), 10.0, "half_width {half_width}");
        assert!(
            ticks.windows(2).all(|pair| pair[0] < pair[1]),
            "half_width {half_width}: {ticks:?}"
        );
        assert!(
            ticks.iter().all(|tick| (-10.0..=10.0).contains(tick)),
            "half_width {half_width}: {ticks:?}"
        );
    }
}

#[test]
fn color_style_default_and_validation_order() {
    let style = ColorStyle::default();
    assert_eq!(style.mode, ScaleMode::Linear);
    assert_eq!(style.ramp, ColorRamp::builtin(Colormap::Viridis));
    assert!(style.validate().is_ok());

    let bad_ramp = ColorStyle {
        ramp: ramp(vec![stop(0.0, "#000000")], RampKind::Sequential),
        mode: ScaleMode::Linear,
    };
    assert!(bad_ramp.validate().is_err());

    let bad_mode = ColorStyle {
        ramp: black_to_white(),
        mode: ScaleMode::SignedLog { half_width: 0.0 },
    };
    assert!(bad_mode.validate().is_err());
}

#[test]
fn color_style_samples_the_ramp_and_uses_mid_gray_on_empty_ranges() {
    let style = ColorStyle {
        ramp: ColorRamp::builtin(Colormap::Mono),
        mode: ScaleMode::Linear,
    };
    assert_eq!(style.sample(0.0, 0.0, 1.0), Rgb([0, 0, 0]));
    assert_eq!(style.sample(1.0, 0.0, 1.0), Rgb([255, 255, 255]));
    assert_eq!(style.sample(0.5, 0.0, 1.0), Rgb([128, 128, 128]));

    // Empty, inverted and non-finite ranges preserve the mid-gray fallback.
    assert_eq!(style.sample(0.5, 1.0, 1.0), Rgb([128, 128, 128]));
    assert_eq!(style.sample(0.5, 2.0, 1.0), Rgb([128, 128, 128]));
    assert_eq!(style.sample(f32::NAN, 0.0, 1.0), Rgb([128, 128, 128]));
}

#[test]
fn signed_log_style_sampling_is_centred() {
    let style = ColorStyle {
        ramp: ramp(
            vec![
                stop(0.0, "#0000ff"),
                stop(0.5, "#ffffff"),
                stop(1.0, "#ff0000"),
            ],
            RampKind::Diverging,
        ),
        mode: ScaleMode::SignedLog { half_width: 1.0 },
    };
    assert_eq!(style.sample(0.0, -1.0, 1.0), Rgb([255, 255, 255]));
}

#[test]
fn linear_legend_ticks_are_five_sorted_values_with_endpoints() {
    let style = ColorStyle::default();
    let ticks = style.legend_ticks(0.0, 10.0);
    assert_eq!(ticks.len(), 5);
    assert_eq!(ticks[0], 0.0);
    assert_eq!(ticks[4], 10.0);
    assert!(ticks.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(ticks.iter().all(|tick| tick.is_finite()));
}

#[test]
fn signed_log_legend_ticks_are_bounded_sorted_and_deduplicated() {
    let style = ColorStyle {
        ramp: black_to_white(),
        mode: ScaleMode::SignedLog { half_width: 1.0 },
    };

    let ticks = style.legend_ticks(-2.0, 3.0);
    assert_eq!(ticks, vec![-2.0, -1.0, 0.0, 1.0, 3.0]);

    let deduped = style.legend_ticks(-1.0, 1.0);
    assert_eq!(deduped, vec![-1.0, 0.0, 1.0]);

    let offset = style.legend_ticks(-0.25, 4.0);
    assert!(offset.len() <= 9);
    assert!(offset.contains(&-0.25));
    assert_eq!(*offset.last().unwrap(), 4.0);
    assert!(offset.windows(2).all(|pair| pair[0] < pair[1]));
}

#[test]
fn legend_ticks_are_empty_for_invalid_ranges() {
    let style = ColorStyle::default();
    for (min, max) in [
        (1.0, 1.0),
        (2.0, 1.0),
        (f32::NAN, 1.0),
        (0.0, f32::INFINITY),
    ] {
        assert!(style.legend_ticks(min, max).is_empty(), "{min}..{max}");
    }
}

#[test]
fn sanitized_returns_default_instead_of_repairing() {
    let invalid = ramp(vec![stop(0.0, "#000000")], RampKind::Sequential);
    assert!(invalid.validate().is_err());
    assert_eq!(invalid.clone().sanitized(), ColorRamp::default());

    let valid = black_to_white();
    assert_eq!(valid.clone().sanitized(), valid);
}

#[test]
fn extreme_inputs_never_panic_and_stay_finite_or_none() {
    let extremes = [
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::MIN,
        f32::MAX,
        f32::MIN_POSITIVE,
        -f32::MIN_POSITIVE,
        0.0,
        -0.0,
        1.0,
        -1.0,
    ];

    let ramps = [
        black_to_white(),
        ColorRamp::builtin(Colormap::Viridis),
        ColorRamp::builtin(Colormap::Mono),
        ramp(vec![stop(0.0, "#000000")], RampKind::Sequential),
    ];
    let modes = [
        ScaleMode::Linear,
        ScaleMode::SignedLog { half_width: 1.0 },
        ScaleMode::SignedLog { half_width: 0.0 },
        ScaleMode::SignedLog {
            half_width: f32::NAN,
        },
    ];

    for ramp in &ramps {
        for value in extremes {
            let _ = ramp.sample(value);
        }
        for mode in &modes {
            let style = ColorStyle {
                ramp: ramp.clone(),
                mode: *mode,
            };
            for value in extremes {
                for min in extremes {
                    for max in extremes {
                        if let Some(t) = mode.normalize(value, min, max) {
                            assert!(t.is_finite(), "non-finite t for {value} in {min}..{max}");
                            assert!((0.0..=1.0).contains(&t));
                        }
                        let _ = style.sample(value, min, max);
                        let ticks = style.legend_ticks(min, max);
                        assert!(ticks.len() <= 9);
                        assert!(ticks.iter().all(|tick| tick.is_finite()));
                        assert!(ticks.windows(2).all(|pair| pair[0] < pair[1]));
                    }
                }
            }
        }
    }
}
