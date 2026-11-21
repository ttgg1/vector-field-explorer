//! Issue #93 slice A: scalar comparison series and potential-gradient model.
//!
//! Pure model tests: bounded comparison sets, central/one-sided finite
//! differences with an honest error estimate, hole preservation, the
//! invalidating cache and persistence of series/gradient settings.

use glam::{UVec3, Vec3};
use vector_field_explorer::color_ramp::Rgb;
use vector_field_explorer::domain::Domain;
use vector_field_explorer::parameters::{Parameter, ParameterSet};
use vector_field_explorer::persistence::ViewRecord;
use vector_field_explorer::plane::{PlaneFrame, PlaneView};
use vector_field_explorer::scalar_analysis::{
    sample_gradient, ComparisonSeries, ComparisonSet, GradientCache, GradientCacheKey,
    GradientSpec, SeriesId, SeriesRole, SeriesStyle, MAX_COMPARISON_SERIES, MAX_GRADIENT_SAMPLES,
    MAX_SERIES_LABEL_BYTES, MAX_STEP, MIN_STEP,
};
use vector_field_explorer::scalar_view::UNSUPPORTED_Z_REASON;
use vector_field_explorer::ui::sidebar::Controls;
use vector_field_explorer::viewmodel::State;

fn domain(half: f32) -> Domain {
    Domain::new(Vec3::splat(-half), Vec3::splat(half))
}

fn xy_frame(half: f32) -> PlaneFrame {
    PlaneFrame::new(
        &domain(half),
        PlaneView::middle(UVec3::new(16, 16, 1)),
        UVec3::new(16, 16, 1),
    )
}

fn spec(expr: &str, role: SeriesRole, step: f32, max_error: f32) -> GradientSpec {
    GradientSpec {
        role,
        expr: expr.to_owned(),
        frame: xy_frame(1.0),
        step,
        max_error,
    }
}

fn axis(index: u32, count: u32, half: f32) -> f32 {
    (index as f32 / (count - 1) as f32 * 2.0 - 1.0) * half
}

fn sample_at(
    samples: &[vector_field_explorer::scalar_analysis::GradientSample],
    i: u32,
    j: u32,
) -> &vector_field_explorer::scalar_analysis::GradientSample {
    &samples[(j as usize) * 160 + i as usize]
}

fn assert_close(actual: f32, expected: f32, tolerance: f32, message: &str) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{message}: expected {expected}, got {actual}"
    );
}

#[test]
fn constant_potential_has_zero_gradient() {
    let spec = spec("3.0", SeriesRole::ElectricPotential, 1e-3, 0.0);
    let samples = sample_gradient(&spec, &ParameterSet::default(), 0.0, &domain(1.0))
        .expect("constant potential samples");
    assert_eq!(samples.len(), MAX_GRADIENT_SAMPLES);
    for sample in &samples {
        let gradient = sample.gradient.expect("constant is defined everywhere");
        assert_close(gradient.length(), 0.0, 1e-5, "gradient is zero");
        assert_eq!(sample.error_estimate, 0.0, "exact constant has no error");
        assert!(sample.usable, "zero error is inside the budget");
    }
}

#[test]
fn linear_potential_gradient_is_exact() {
    let spec = spec("x + 2*y", SeriesRole::ElectricPotential, 1e-3, 1e-6);
    let samples = sample_gradient(&spec, &ParameterSet::default(), 0.0, &domain(1.0))
        .expect("linear potential samples");
    let interior = sample_at(&samples, 80, 80);
    let field = interior.gradient.expect("linear is defined everywhere");
    // ElectricPotential samples are the display field E = -∇V.
    assert_close(field.x, -1.0, 1e-4, "E_x = -d/dx");
    assert_close(field.y, -2.0, 1e-4, "E_y = -d/dy");
    assert_close(field.z, 0.0, 1e-6, "no out-of-plane component");
    assert!(
        interior.error_estimate <= 1e-5,
        "linear difference is exact"
    );
    assert!(!interior.one_sided, "interior uses the central stencil");
}

#[test]
fn sampled_gradient_always_derives_from_the_spec_text() {
    // The old API accepted a caller-parsed `ScalarExpr` beside the spec, so a
    // spec whose `expr` was "x" could be sampled with a separately parsed
    // "x + 2*y". Sampling now parses `spec.expr` through the #92 seam, making the
    // spec text the only possible source.
    let spec = spec("x", SeriesRole::Scalar, 1e-3, 1e-6);
    let samples = sample_gradient(&spec, &ParameterSet::default(), 0.0, &domain(1.0))
        .expect("spec-derived expression samples");
    let interior = sample_at(&samples, 80, 80);
    let gradient = interior.gradient.expect("linear scalar is defined");
    assert_close(gradient.x, 1.0, 1e-4, "sampled from spec.expr");
    assert_close(gradient.y, 0.0, 1e-4, "no separately parsed expression");
}

#[test]
fn electric_potential_samples_are_the_negated_gradient() {
    let parameters = ParameterSet::default();
    let test_domain = domain(1.0);
    let scalar_spec = spec("x + 2*y", SeriesRole::Scalar, 1e-3, 1e-5);
    let potential_spec = spec("x + 2*y", SeriesRole::ElectricPotential, 1e-3, 1e-5);
    let scalar =
        sample_gradient(&scalar_spec, &parameters, 0.0, &test_domain).expect("scalar samples");
    let potential = sample_gradient(&potential_spec, &parameters, 0.0, &test_domain)
        .expect("potential samples");

    let scalar_sample = sample_at(&scalar, 80, 80);
    let potential_sample = sample_at(&potential, 80, 80);
    let gradient = scalar_sample.gradient.expect("scalar is defined");
    let field = potential_sample.gradient.expect("potential is defined");
    assert_close(gradient.x, 1.0, 1e-4, "∇f x component");
    assert_close(gradient.y, 2.0, 1e-4, "∇f y component");
    assert_close(field.x, -1.0, 1e-4, "E_x = -∇V_x");
    assert_close(field.y, -2.0, 1e-4, "E_y = -∇V_y");
    assert_close(
        gradient.length(),
        field.length(),
        1e-5,
        "negation preserves magnitude",
    );
    assert_eq!(
        scalar_sample.error_estimate, potential_sample.error_estimate,
        "negation does not change the error indicator"
    );
    assert_eq!(
        scalar_sample.usable, potential_sample.usable,
        "negation does not change usability"
    );
    assert_eq!(
        scalar_sample.one_sided, potential_sample.one_sided,
        "negation does not change the stencil"
    );
}

#[test]
fn quadratic_potential_gradient_matches_the_analytic_derivative() {
    let spec = spec("x*x + y*y", SeriesRole::Scalar, 1e-2, 1e-3);
    let samples = sample_gradient(&spec, &ParameterSet::default(), 0.0, &domain(1.0))
        .expect("quadratic samples");
    for (i, j) in [(80, 80), (120, 40), (40, 120)] {
        let u = axis(i, 160, 1.0);
        let v = axis(j, 160, 1.0);
        let gradient = sample_at(&samples, i, j)
            .gradient
            .expect("quadratic is defined everywhere");
        assert_close(gradient.x, 2.0 * u, 1e-3, "analytic d/dx");
        assert_close(gradient.y, 2.0 * v, 1e-3, "analytic d/dy");
    }
}

#[test]
fn pole_samples_report_none_and_no_error_fabrication() {
    let spec = spec("1 / (x + 1)", SeriesRole::Scalar, 1e-3, 0.0);
    let samples = sample_gradient(&spec, &ParameterSet::default(), 0.0, &domain(1.0))
        .expect("sampling preserves the pole as a hole");
    let mut holes = 0;
    for j in 0..160 {
        let sample = sample_at(&samples, 0, j);
        assert!(
            sample.gradient.is_none(),
            "the pole on the inward stencil is a hole"
        );
        assert_eq!(
            sample.error_estimate, 0.0,
            "a missing sample fabricates no error"
        );
        assert!(!sample.usable, "a missing sample is never usable");
        holes += 1;
    }
    assert!(holes > 0, "the boundary column is sampled");
    assert!(
        samples.iter().any(|sample| sample.gradient.is_some()),
        "finite interior samples are preserved"
    );
}

#[test]
fn domain_boundary_uses_inward_one_sided_differences_and_flags_it() {
    let spec = spec("x + 2*y", SeriesRole::Scalar, 1e-3, 1e-6);
    let samples = sample_gradient(&spec, &ParameterSet::default(), 0.0, &domain(1.0))
        .expect("linear samples");
    let left = sample_at(&samples, 0, 80);
    let right = sample_at(&samples, 159, 80);
    let bottom = sample_at(&samples, 80, 0);
    let top = sample_at(&samples, 80, 159);
    let interior = sample_at(&samples, 80, 80);
    for edge in [left, right, bottom, top] {
        assert!(
            edge.one_sided,
            "an exact domain edge uses a one-sided stencil"
        );
        let gradient = edge.gradient.expect("linear edge is defined");
        assert_close(gradient.x, 1.0, 1e-4, "edge d/dx");
        assert_close(gradient.y, 2.0, 1e-4, "edge d/dy");
    }
    assert!(!interior.one_sided, "the interior stays central");
}

#[test]
fn step_refinement_shrinks_the_reported_error_quadratically_for_sin() {
    let coarse = spec("sin(x)", SeriesRole::Scalar, MAX_STEP, 1.0);
    let fine = spec("sin(x)", SeriesRole::Scalar, MAX_STEP / 2.0, 1.0);
    let coarse_samples = sample_gradient(&coarse, &ParameterSet::default(), 0.0, &domain(1.0))
        .expect("coarse sin samples");
    let fine_samples = sample_gradient(&fine, &ParameterSet::default(), 0.0, &domain(1.0))
        .expect("fine sin samples");
    let mean = |samples: &[vector_field_explorer::scalar_analysis::GradientSample]| {
        let mut total = 0.0f64;
        let mut count = 0u32;
        for sample in samples.iter().filter(|sample| !sample.one_sided) {
            if sample.gradient.is_some() {
                total += f64::from(sample.error_estimate);
                count += 1;
            }
        }
        total / f64::from(count)
    };
    let coarse_error = mean(&coarse_samples);
    let fine_error = mean(&fine_samples);
    assert!(coarse_error > 0.0, "coarse error is not fabricated as zero");
    assert!(
        fine_error > 0.0,
        "fine error stays an indicator, not an exact bound"
    );
    let ratio = coarse_error / fine_error;
    assert!(
        ratio > 3.0,
        "halving h shrinks central-difference disagreement roughly fourfold, got {ratio}"
    );
}

#[test]
fn generic_scalar_gradient_is_never_labelled_as_an_electric_field() {
    assert_eq!(SeriesRole::Scalar.label(), "∇f");
    assert_eq!(SeriesRole::ElectricPotential.label(), "E = -∇V");
    let generic = spec("x*x", SeriesRole::Scalar, 1e-2, 1e-2);
    assert_eq!(generic.label(), "∇f");
    assert!(!generic.label().contains("electric"));
    let potential = spec("x*x", SeriesRole::ElectricPotential, 1e-2, 1e-2);
    assert_eq!(potential.label(), "E = -∇V");
}

#[test]
fn comparison_series_are_bounded_labelled_and_independently_styled() {
    let mut set = ComparisonSet::new();
    for index in 0..MAX_COMPARISON_SERIES {
        set.push(ComparisonSeries::new(
            SeriesId(index as u32),
            format!("series {index}"),
            "x".to_owned(),
            SeriesStyle {
                color: Rgb([20 + index as u8 * 30, 10, 40]),
                dashed: index % 2 == 1,
            },
        ))
        .expect("room for the bounded series");
    }
    assert_eq!(set.len(), MAX_COMPARISON_SERIES);
    assert_eq!(set.validate(), Ok(()));
    let colors: Vec<_> = set.iter().map(|series| series.style.color).collect();
    for (index, color) in colors.iter().enumerate() {
        for other in &colors[index + 1..] {
            assert_ne!(color, other, "each series keeps its own colour");
        }
    }
    assert!(set.iter().any(|series| series.style.dashed));
    assert!(set.iter().any(|series| !series.style.dashed));

    let fifth = ComparisonSeries::new(
        SeriesId(99),
        "fifth".to_owned(),
        "x".to_owned(),
        SeriesStyle::default(),
    );
    assert!(
        set.push(fifth).is_err(),
        "the cap is checked before mutation"
    );
    assert_eq!(
        set.len(),
        MAX_COMPARISON_SERIES,
        "a rejected push changes nothing"
    );

    let duplicate = ComparisonSeries::new(
        SeriesId(0),
        "duplicate".to_owned(),
        "x".to_owned(),
        SeriesStyle::default(),
    );
    assert!(
        set.push(duplicate).is_err(),
        "duplicate ids are rejected by the set"
    );

    let empty_label = ComparisonSeries::new(
        SeriesId(1),
        String::new(),
        "x".to_owned(),
        SeriesStyle::default(),
    );
    assert!(empty_label.validate().is_err(), "empty labels are rejected");
    let long_label = ComparisonSeries::new(
        SeriesId(1),
        "x".repeat(MAX_SERIES_LABEL_BYTES + 1),
        "x".to_owned(),
        SeriesStyle::default(),
    );
    assert!(
        long_label.validate().is_err(),
        "over-long labels are rejected"
    );
    let empty_expr = ComparisonSeries::new(
        SeriesId(1),
        "label".to_owned(),
        String::new(),
        SeriesStyle::default(),
    );
    assert!(
        empty_expr.validate().is_err(),
        "empty expressions are rejected"
    );

    let corrupt = ComparisonSet::from_series(vec![
        ComparisonSeries::new(
            SeriesId(0),
            String::new(),
            "x".to_owned(),
            SeriesStyle::default(),
        ),
        ComparisonSeries::new(
            SeriesId(1),
            "ok".to_owned(),
            "x".to_owned(),
            SeriesStyle::default(),
        ),
        ComparisonSeries::new(
            SeriesId(1),
            "duplicate".to_owned(),
            "x".to_owned(),
            SeriesStyle::default(),
        ),
        ComparisonSeries::new(
            SeriesId(2),
            "x".repeat(MAX_SERIES_LABEL_BYTES + 8),
            "x".to_owned(),
            SeriesStyle::default(),
        ),
        ComparisonSeries::new(
            SeriesId(3),
            "dropped".to_owned(),
            String::new(),
            SeriesStyle::default(),
        ),
        ComparisonSeries::new(
            SeriesId(4),
            "extra".to_owned(),
            "x".to_owned(),
            SeriesStyle::default(),
        ),
    ]);
    let repaired = corrupt.sanitized();
    assert!(repaired.len() <= MAX_COMPARISON_SERIES);
    assert_eq!(repaired.validate(), Ok(()));
    for series in repaired.iter() {
        assert!(!series.label.is_empty(), "sanitizing repairs labels");
        assert!(
            series.label.len() <= MAX_SERIES_LABEL_BYTES,
            "sanitizing truncates labels"
        );
        assert!(!series.expr.is_empty(), "empty expressions are dropped");
    }
}

#[test]
fn series_and_gradient_settings_restore_without_dirtying() {
    let mut set = ComparisonSet::new();
    set.push(ComparisonSeries::new(
        SeriesId(3),
        "potential".to_owned(),
        "x*x + y*y".to_owned(),
        SeriesStyle {
            color: Rgb([200, 120, 20]),
            dashed: true,
        },
    ))
    .expect("room");
    let gradient = spec("x*x + y*y", SeriesRole::ElectricPotential, 1e-2, 0.05);

    let mut source = State::new(false);
    source.comparison_series = set.clone();
    source.gradient = Some(gradient.clone());
    let record = ViewRecord::capture(&source);
    assert_eq!(record.comparison_series, set);
    assert_eq!(record.gradient, Some(gradient.clone()));

    // Direct assignment is the startup seam: these fields alone never dirty.
    let mut direct = State::new(false);
    direct.dirty = false;
    direct.comparison_series = set.clone();
    direct.gradient = Some(gradient.clone());
    assert!(
        !direct.dirty,
        "assigning series/gradient leaves dirty alone"
    );

    let mut restored = State::new(false);
    record.restore(&mut restored, &mut Controls::new());
    assert_eq!(restored.comparison_series, set);
    assert_eq!(restored.gradient, Some(gradient));
    assert!(restored.dirty, "restore asks for its one rebuild");
    assert!(restored.snapshot_if_dirty().is_some());
    assert!(
        restored.snapshot_if_dirty().is_none(),
        "restored series/gradient do not re-dirty the scene"
    );

    let legacy = ViewRecord::default();
    assert!(legacy.comparison_series.is_empty());
    assert!(legacy.gradient.is_none());
}

#[test]
fn invalid_or_non_finite_steps_are_rejected_before_sampling() {
    for step in [
        f32::NAN,
        f32::INFINITY,
        0.0,
        MIN_STEP / 2.0,
        MAX_STEP * 2.0,
        -MIN_STEP,
    ] {
        let candidate = spec("x*x", SeriesRole::Scalar, step, 0.0);
        assert!(candidate.validate().is_err(), "step {step} is rejected");
        let error = sample_gradient(&candidate, &ParameterSet::default(), 0.0, &domain(1.0))
            .expect_err("an invalid step never samples");
        assert!(
            error.contains("step"),
            "the reason names the invalid step: {error}"
        );
    }
    for max_error in [f32::NAN, f32::NEG_INFINITY, -1.0] {
        let candidate = spec("x*x", SeriesRole::Scalar, 1e-2, max_error);
        assert!(
            candidate.validate().is_err(),
            "max_error {max_error} is rejected"
        );
    }
    assert!(spec("x*x", SeriesRole::Scalar, MIN_STEP, 0.0)
        .validate()
        .is_ok());
    assert!(spec("x*x", SeriesRole::Scalar, MAX_STEP, 0.0)
        .validate()
        .is_ok());
}

#[test]
fn comparison_series_do_not_reach_arrows_colours_or_probes() {
    let mut state = State::new(false);
    state.dirty = false;
    let arrows = state.arrows;
    let colormap = state.colormap;
    let color_ramp = state.color_ramp.clone();
    let color_mode = state.color_mode;
    let color_quantity = state.color_quantity;
    let color_min = state.color_min;
    let color_max = state.color_max;
    let probes = state.probes.clone();
    let vector_mode_3d = state.mode_3d;

    let mut set = ComparisonSet::new();
    set.push(ComparisonSeries::new(
        SeriesId(1),
        "comparison".to_owned(),
        "x".to_owned(),
        SeriesStyle {
            color: Rgb([10, 200, 30]),
            dashed: false,
        },
    ))
    .expect("room");
    state
        .set_comparison_series(set.clone())
        .expect("valid series");
    assert!(state.dirty, "a real series edit dirties once");
    assert_eq!(state.comparison_series, set);

    state.dirty = false;
    state
        .set_comparison_series(set.clone())
        .expect("unchanged series are a no-op");
    assert!(!state.dirty, "an unchanged series does not dirty");

    let gradient = spec("x*x", SeriesRole::ElectricPotential, 1e-2, 0.1);
    state
        .set_gradient(Some(gradient.clone()))
        .expect("valid gradient");
    assert!(state.dirty, "a real gradient edit dirties once");
    state.dirty = false;
    state
        .set_gradient(Some(gradient))
        .expect("unchanged gradient is a no-op");
    assert!(!state.dirty, "an unchanged gradient does not dirty");

    assert_eq!(state.arrows, arrows);
    assert_eq!(state.colormap, colormap);
    assert_eq!(state.color_ramp, color_ramp);
    assert_eq!(state.color_mode, color_mode);
    assert_eq!(state.color_quantity, color_quantity);
    assert_eq!(state.color_min, color_min);
    assert_eq!(state.color_max, color_max);
    assert_eq!(state.probes, probes);
    assert_eq!(state.mode_3d, vector_mode_3d);
}

#[test]
fn editing_parameters_clock_or_source_invalidates_the_gradient_cache() {
    let spec = spec("x*x + y*y", SeriesRole::Scalar, 1e-2, 0.1);
    let parameters = ParameterSet::default();
    let test_domain = domain(1.0);
    let key = GradientCacheKey::new(&spec, &parameters, &test_domain, 0.0);
    let samples = sample_gradient(&spec, &parameters, 0.0, &test_domain).expect("samples");
    let mut cache = GradientCache::default();
    cache.store(key.clone(), Ok(samples));
    assert!(cache.record_matches(&key));
    assert_eq!(
        cache.samples().expect("stored samples").len(),
        MAX_GRADIENT_SAMPLES
    );

    // The clock is quantised to the #92 10 Hz interval.
    assert!(cache.record_matches(&GradientCacheKey::new(
        &spec,
        &parameters,
        &test_domain,
        0.09
    )));
    assert!(!cache.record_matches(&GradientCacheKey::new(
        &spec,
        &parameters,
        &test_domain,
        0.11
    )));

    let mut edited = parameters.clone();
    edited.entries.push(Parameter {
        name: "a".to_owned(),
        value: 2.0,
        step: 0.1,
        ..Default::default()
    });
    assert!(!cache.record_matches(&GradientCacheKey::new(&spec, &edited, &test_domain, 0.0)));

    let mut changed_source = spec.clone();
    changed_source.expr = "x - y".to_owned();
    assert!(!cache.record_matches(&GradientCacheKey::new(
        &changed_source,
        &parameters,
        &test_domain,
        0.0
    )));

    let changed_domain = domain(2.0);
    assert!(!cache.record_matches(&GradientCacheKey::new(
        &spec,
        &parameters,
        &changed_domain,
        0.0
    )));

    let mut changed_frame = spec.clone();
    changed_frame.frame = xy_frame(2.0);
    assert!(!cache.record_matches(&GradientCacheKey::new(
        &changed_frame,
        &parameters,
        &test_domain,
        0.0
    )));

    let mut changed_step = spec.clone();
    changed_step.step = 1e-3;
    assert!(!cache.record_matches(&GradientCacheKey::new(
        &changed_step,
        &parameters,
        &test_domain,
        0.0
    )));

    let mut changed_role = spec.clone();
    changed_role.role = SeriesRole::ElectricPotential;
    assert!(
        !cache.record_matches(&GradientCacheKey::new(
            &changed_role,
            &parameters,
            &test_domain,
            0.0
        )),
        "the field sign differs, so the role is part of the cache identity"
    );
}

#[test]
fn curve_only_or_z_formulas_report_the_scalar_unsupported_reason() {
    // `ScalarExpr` does not retain its parse context and every Curve-valid
    // formula is also Surface-valid, so the observable #92 rejection is the
    // explicit z reason (and the parser message for any other bad source).
    let z_spec = spec("x + z", SeriesRole::Scalar, 1e-2, 0.1);
    let error = sample_gradient(&z_spec, &ParameterSet::default(), 0.0, &domain(1.0))
        .expect_err("a z formula is unsupported");
    assert_eq!(error, UNSUPPORTED_Z_REASON);

    let bad_spec = spec("x + unknown_name", SeriesRole::Scalar, 1e-2, 0.1);
    let error = sample_gradient(&bad_spec, &ParameterSet::default(), 0.0, &domain(1.0))
        .expect_err("an unbound formula is unsupported");
    assert!(
        error.contains("allowed variables: x, y"),
        "the #92 parse reason names the surface variables: {error}"
    );
}

#[test]
fn samples_over_the_error_budget_are_marked_unusable() {
    let strict = spec("sin(x)", SeriesRole::Scalar, MAX_STEP, 0.0);
    let samples =
        sample_gradient(&strict, &ParameterSet::default(), 0.0, &domain(1.0)).expect("sin samples");
    for sample in samples.iter().filter(|sample| sample.gradient.is_some()) {
        assert!(
            sample.error_estimate > 0.0,
            "a finite sin sample reports the refinement disagreement"
        );
        assert!(
            !sample.usable,
            "a sample over a zero budget is marked unusable"
        );
    }

    let generous = spec("sin(x)", SeriesRole::Scalar, MAX_STEP, f32::MAX);
    let samples = sample_gradient(&generous, &ParameterSet::default(), 0.0, &domain(1.0))
        .expect("sin samples");
    assert!(
        samples
            .iter()
            .all(|sample| sample.gradient.is_none() || sample.usable),
        "a generous budget trusts every defined sample"
    );
}
