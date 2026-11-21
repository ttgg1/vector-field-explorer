//! Issue #93 slice A: scalar comparison series and potential-gradient model.
//!
//! Pure model tests: bounded comparison sets, central/one-sided finite
//! differences with an honest error estimate, hole preservation, the
//! invalidating cache and persistence of series/gradient settings.

use glam::{UVec3, Vec3};
use vector_field_explorer::animation_scene::AnimationSnapshot;
use vector_field_explorer::camera::OrbitCamera;
use vector_field_explorer::color_ramp::Rgb;
use vector_field_explorer::domain::Domain;
use vector_field_explorer::parameters::{Parameter, ParameterSet};
use vector_field_explorer::particles::Particles;
use vector_field_explorer::persistence::ViewRecord;
use vector_field_explorer::plane::{PlaneFrame, PlaneView};
use vector_field_explorer::renderer::LineInstanceGPU;
use vector_field_explorer::scalar_analysis::{
    ComparisonSeries, ComparisonSet, GradientCache, GradientCacheKey, GradientSpec,
    MAX_COMPARISON_SERIES, MAX_GRADIENT_SAMPLES, MAX_SERIES_LABEL_BYTES, MAX_STEP, MIN_STEP,
    SeriesId, SeriesRole, SeriesStyle, sample_gradient,
};
use vector_field_explorer::scalar_view::UNSUPPORTED_Z_REASON;
use vector_field_explorer::screenshot::scene_to_svg;
use vector_field_explorer::source_scene::ELECTRIC_COLOR;
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
    assert!(
        spec("x*x", SeriesRole::Scalar, MIN_STEP, 0.0)
            .validate()
            .is_ok()
    );
    assert!(
        spec("x*x", SeriesRole::Scalar, MAX_STEP, 0.0)
            .validate()
            .is_ok()
    );
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

// ---------------------------------------------------------------------------
// Issue #93 slice B: comparison-curve and gradient-arrow geometry.
// ---------------------------------------------------------------------------

fn rgba(color: [u8; 3]) -> [f32; 4] {
    [
        f32::from(color[0]) / 255.0,
        f32::from(color[1]) / 255.0,
        f32::from(color[2]) / 255.0,
        1.0,
    ]
}

fn line_length(line: &LineInstanceGPU) -> f32 {
    let a = Vec3::new(line.a[0], line.a[1], line.a[2]);
    let b = Vec3::new(line.b[0], line.b[1], line.b[2]);
    (b - a).length()
}

fn with_color(lines: &[LineInstanceGPU], color: [f32; 4]) -> Vec<&LineInstanceGPU> {
    lines.iter().filter(|line| line.color == color).collect()
}

#[test]
fn hidden_section_does_not_sample_the_gradient() {
    let mut state = State::new(false);
    state
        .set_scalar_contour_levels(0)
        .expect("0 hides contours");
    let frame = state.scalar_view_frame();
    state
        .set_gradient(Some(GradientSpec {
            role: SeriesRole::Scalar,
            expr: "x".into(),
            frame,
            step: 1e-3,
            max_error: 1.0,
        }))
        .expect("valid gradient");

    // A hidden panel performs no sampling and paints no overlay fed by one.
    state.refresh_scalar_analysis(false);
    assert_eq!(state.scalar_analysis_resamples(), 0);
    assert!(state.scalar_overlay_lines().is_empty());

    state.refresh_scalar_analysis(true);
    assert_eq!(state.scalar_analysis_resamples(), 1);
    assert!(!state.scalar_overlay_lines().is_empty());

    // An unchanged key (same formula, frame, domain, parameters and the same
    // 100 ms clock bucket) is a cache hit.
    state.refresh_scalar_analysis(true);
    assert_eq!(state.scalar_analysis_resamples(), 1);
    state.set_time(0.05);
    state.refresh_scalar_analysis(true);
    assert_eq!(
        state.scalar_analysis_resamples(),
        1,
        "the same 100 ms bucket does not resample"
    );

    // The next bucket invalidates the cache through the #92 10 Hz rule.
    state.set_time(0.2);
    state.refresh_scalar_analysis(true);
    assert_eq!(state.scalar_analysis_resamples(), 2);
}

#[test]
fn comparison_curves_render_in_their_own_styles() {
    let mut state = State::new(false);
    state
        .set_scalar_contour_levels(0)
        .expect("0 hides contours");
    let solid_style = SeriesStyle {
        color: Rgb([250, 10, 20]),
        dashed: false,
    };
    let dashed_style = SeriesStyle {
        color: Rgb([10, 250, 20]),
        dashed: true,
    };
    let mut set = ComparisonSet::new();
    set.push(ComparisonSeries::new(
        SeriesId(0),
        "solid".into(),
        "x".into(),
        solid_style,
    ))
    .expect("room for the first series");
    set.push(ComparisonSeries::new(
        SeriesId(1),
        "dashed".into(),
        "x".into(),
        dashed_style,
    ))
    .expect("room for the second series");
    state.set_comparison_series(set).expect("valid set");

    state.refresh_scalar_analysis(true);
    let lines = state.scalar_overlay_lines();
    let solid = with_color(&lines, rgba(solid_style.color.0));
    let dashed = with_color(&lines, rgba(dashed_style.color.0));
    assert!(!solid.is_empty(), "the solid series paints lines");
    assert!(!dashed.is_empty(), "the dashed series paints lines");
    assert!(
        dashed.len() < solid.len(),
        "dashing leaves gaps instead of painting every edge"
    );
    let solid_total: f32 = solid.iter().map(|line| line_length(line)).sum();
    let dashed_total: f32 = dashed.iter().map(|line| line_length(line)).sum();
    assert!(
        dashed_total < solid_total * 0.9,
        "the dashed pattern covers less of the curve: {dashed_total} vs {solid_total}"
    );
    let solid_len = line_length(solid[0]);
    assert!(
        dashed
            .iter()
            .all(|line| line_length(line) <= solid_len + 1e-6),
        "no dash is longer than one sampled edge"
    );

    // The curve samples the shared plane locally: for `f(x) = x` the first
    // edge lies on the 45° line through the frame origin.
    let first = solid[0];
    assert!(
        (first.a[0] - first.a[1]).abs() < 1e-6 && (first.b[0] - first.b[1]).abs() < 1e-6,
        "f(x) = x maps onto the frame diagonal: {first:?}"
    );

    // Independent styles: distinct colours on the same geometry.
    assert_ne!(
        solid[0].color, dashed[0].color,
        "each series keeps its own colour"
    );
}

#[test]
fn unparseable_comparison_series_surfaces_an_error_and_paints_nothing() {
    let mut state = State::new(false);
    state
        .set_scalar_contour_levels(0)
        .expect("0 hides contours");
    let mut set = ComparisonSet::new();
    set.push(ComparisonSeries::new(
        SeriesId(7),
        "bad series".into(),
        "a*x + q".into(),
        SeriesStyle::default(),
    ))
    .expect("slice-A accepts any non-empty expression text");
    state
        .set_comparison_series(set)
        .expect("slice-A validation is unchanged");

    state.refresh_scalar_analysis(true);
    let error = state
        .scalar_analysis_error()
        .expect("the parse failure is surfaced in the panel");
    assert!(
        error.contains("bad series"),
        "the message names the series: {error}"
    );
    assert!(
        error.to_lowercase().contains("parse"),
        "the message reports a parse failure: {error}"
    );
    assert!(
        state.scalar_overlay_lines().is_empty(),
        "an unparseable series paints no geometry"
    );
    assert_eq!(
        state.comparison_series.as_slice()[0].expr,
        "a*x + q",
        "the rejected text stays in the committed draft for editing"
    );
}

#[test]
fn unusable_gradient_samples_are_omitted_and_counted() {
    let mut state = State::new(false);
    state
        .set_scalar_contour_levels(0)
        .expect("0 hides contours");
    let frame = state.scalar_view_frame();
    state
        .set_gradient(Some(GradientSpec {
            role: SeriesRole::Scalar,
            // Undefined for x < 0: those samples are holes and never filled.
            expr: "sqrt(x)".into(),
            frame,
            step: 1e-3,
            max_error: 1e9,
        }))
        .expect("valid gradient");

    state.refresh_scalar_analysis(true);
    let counts = state.scalar_analysis_counts();
    assert!(counts.arrows > 0, "the defined half paints arrows");
    assert!(counts.hidden > 0, "the hole half is reported as hidden");
    assert!(counts.one_sided > 0, "domain edges use one-sided stencils");
    assert_eq!(
        counts.arrows + counts.hidden,
        MAX_GRADIENT_SAMPLES,
        "every sample is either drawn or reported hidden"
    );

    let lines = state.scalar_overlay_lines();
    assert!(!lines.is_empty(), "usable samples paint arrow glyphs");
    assert!(
        lines
            .iter()
            .all(|line| line.a.iter().chain(line.b.iter()).all(|v| v.is_finite())),
        "a hole is never replaced by fabricated geometry"
    );
    assert!(
        lines.iter().all(|line| line.a[0] > 0.0 && line.b[0] > 0.0),
        "the undefined x < 0 half is omitted"
    );
}

#[test]
fn gradient_arrows_carry_the_electric_label_colour_only_for_that_role() {
    let mut state = State::new(false);
    state
        .set_scalar_contour_levels(0)
        .expect("0 hides contours");
    state.set_scalar_text_color([0.9, 0.8, 0.7, 1.0]);
    let frame = state.scalar_view_frame();
    let make = |role| GradientSpec {
        role,
        expr: "x".into(),
        frame,
        step: 1e-3,
        max_error: 1.0,
    };

    state
        .set_gradient(Some(make(SeriesRole::Scalar)))
        .expect("valid scalar gradient");
    state.refresh_scalar_analysis(true);
    let scalar_lines = state.scalar_overlay_lines();
    assert!(!scalar_lines.is_empty());
    assert!(
        scalar_lines
            .iter()
            .all(|line| line.color == state.scalar_text_color()),
        "a generic gradient uses the scalar text colour and is never called electric"
    );

    state
        .set_gradient(Some(make(SeriesRole::ElectricPotential)))
        .expect("valid potential gradient");
    state.refresh_scalar_analysis(true);
    let electric_lines = state.scalar_overlay_lines();
    assert!(!electric_lines.is_empty());
    assert!(
        electric_lines
            .iter()
            .all(|line| line.color == rgba(ELECTRIC_COLOR)),
        "E = -grad V uses the electric-family colour"
    );
    assert!(
        electric_lines
            .iter()
            .all(|line| line.color != state.scalar_text_color()),
        "the electric role does not reuse the scalar text colour"
    );
}

#[test]
fn comparison_and_gradient_geometry_do_not_change_arrows_mapping_or_probes() {
    let mut state = State::new(false);
    state
        .set_scalar_contour_levels(0)
        .expect("0 hides contours");
    state
        .add_point_probe(Vec3::new(0.1, 0.2, 0.0), None)
        .expect("room for one probe");
    let arrows_before = state.sample();
    let mapping_before = state.color_mapping();
    let probes_before = state.probes.clone();

    let mut set = ComparisonSet::new();
    set.push(ComparisonSeries::new(
        SeriesId(0),
        "reference".into(),
        "x".into(),
        SeriesStyle::default(),
    ))
    .expect("room for one series");
    state.set_comparison_series(set).expect("valid set");
    let frame = state.scalar_view_frame();
    state
        .set_gradient(Some(GradientSpec {
            role: SeriesRole::ElectricPotential,
            expr: "x".into(),
            frame,
            step: 1e-3,
            max_error: 1.0,
        }))
        .expect("valid gradient");
    state.refresh_scalar_analysis(true);
    assert!(!state.scalar_overlay_lines().is_empty());

    assert_eq!(
        state.sample(),
        arrows_before,
        "comparison/gradient geometry never feeds vector arrows"
    );
    assert_eq!(
        state.color_mapping(),
        mapping_before,
        "comparison/gradient geometry never feeds the colour mapping"
    );
    assert_eq!(
        state.probes, probes_before,
        "comparison/gradient geometry never feeds probes"
    );
}

#[test]
fn svg_export_keeps_curves_and_gradient_as_lines() {
    let mut state = State::new(false);
    state
        .set_scalar_contour_levels(0)
        .expect("0 hides contours");
    let style = SeriesStyle {
        color: Rgb([31, 119, 180]),
        dashed: false,
    };
    let mut set = ComparisonSet::new();
    set.push(ComparisonSeries::new(
        SeriesId(0),
        "curve".into(),
        "x".into(),
        style,
    ))
    .expect("room for one series");
    state.set_comparison_series(set).expect("valid set");
    let frame = state.scalar_view_frame();
    state
        .set_gradient(Some(GradientSpec {
            role: SeriesRole::ElectricPotential,
            expr: "x".into(),
            frame,
            step: 1e-3,
            max_error: 1.0,
        }))
        .expect("valid gradient");
    state.refresh_scalar_analysis(true);

    let scene = state.snapshot();
    let svg = scene_to_svg(&scene, glam::Mat4::IDENTITY, 256, 256, [0.0, 0.0, 0.0, 1.0]);
    assert!(svg.contains("<line"), "the line blueprint survives");
    assert!(
        svg.contains("#1f77b4"),
        "the comparison curve keeps its own style colour in SVG"
    );
    assert!(
        svg.contains("#ffb040"),
        "the electric gradient keeps the electric colour in SVG"
    );
    assert!(
        !svg.contains("heatmap") && !svg.contains("<polygon class=\"heatmap"),
        "scalar analysis geometry never becomes filled heatmap cells"
    );
}

#[test]
fn capture_and_animation_include_the_overlay() {
    let mut state = State::new(false);
    state
        .set_scalar_contour_levels(0)
        .expect("0 hides contours");
    let style = SeriesStyle {
        color: Rgb([200, 30, 60]),
        dashed: false,
    };
    let mut set = ComparisonSet::new();
    set.push(ComparisonSeries::new(
        SeriesId(0),
        "curve".into(),
        "x".into(),
        style,
    ))
    .expect("room for one series");
    state.set_comparison_series(set).expect("valid set");
    let frame = state.scalar_view_frame();
    state
        .set_gradient(Some(GradientSpec {
            role: SeriesRole::ElectricPotential,
            expr: "x".into(),
            frame,
            step: 1e-3,
            max_error: 1.0,
        }))
        .expect("valid gradient");
    state.refresh_scalar_analysis(true);

    // The PNG capture path paints the app's cached scene lines.
    let scene = state.snapshot();
    assert!(
        !with_color(&scene.lines, rgba(style.color.0)).is_empty(),
        "the screenshot scene carries the comparison curve"
    );
    assert!(
        !with_color(&scene.lines, rgba(ELECTRIC_COLOR)).is_empty(),
        "the screenshot scene carries the gradient arrows"
    );

    // The animation snapshot composes the same line collection.
    let mut snapshot = AnimationSnapshot::from_parts(
        state,
        Particles::default(),
        OrbitCamera::default(),
        None,
        [0.0, 0.0, 0.0, 1.0],
    );
    let frame = snapshot
        .frame_at(0.0, 0.0, 1.0, true)
        .expect("animation frame");
    assert!(
        !with_color(&frame.scene.lines, rgba(style.color.0)).is_empty(),
        "the animation frame carries the comparison curve"
    );
    assert!(
        !with_color(&frame.scene.lines, rgba(ELECTRIC_COLOR)).is_empty(),
        "the animation frame carries the gradient arrows"
    );
}
