//! Issue #92 slice A: scalar view model, bounds, contours, probes and cache.
//!
//! Pure model tests: no UI, renderer or shader. The contract is
//! `docs/superpowers/specs/2026-11-18-scalar-views-contract.md`.

use glam::{UVec3, Vec3};
use vector_field_explorer::{
    domain::Domain,
    parameters::{Parameter, ParameterSet},
    persistence::ViewRecord,
    plane::{PlaneAxis, PlaneFrame, PlaneView},
    scalar::{ScalarContext, ScalarExpr},
    scalar_view::{
        LineProbe, MAX_CONTOUR_LEVELS, MAX_GRID_SIDE, MAX_LINE_SAMPLES, MAX_PROBES, MIN_GRID_SIDE,
        PointProbe, ProbeSet, ScalarGrid, ScalarSample, ScalarViewError, UNSUPPORTED_Z_REASON,
        contour_segments, sample_grid, sample_line, scalar_view_expr,
    },
    ui::sidebar::Controls,
    viewmodel::State,
};

fn expr(source: &str) -> ScalarExpr {
    ScalarExpr::parse(source, ScalarContext::Surface).expect("valid test formula")
}

/// The XY frame of the box `[-1, 1]^2`, matching the default domain.
fn xy_frame() -> PlaneFrame {
    PlaneFrame::new(
        &Domain::new(Vec3::new(-1.0, -1.0, 0.0), Vec3::new(1.0, 1.0, 0.0)),
        PlaneView {
            enabled: false,
            axis: PlaneAxis::Z,
            index: 0,
        },
        UVec3::splat(2),
    )
}

fn value(grid: &ScalarGrid, i: u32, j: u32) -> f32 {
    grid.sample(i, j).value.expect("finite test sample")
}

fn parameter(name: &str, value: f64) -> Parameter {
    Parameter {
        name: name.to_string(),
        value,
        min: None,
        max: None,
        step: 1.0,
    }
}

fn parameter_set(name: &str, value: f64) -> ParameterSet {
    ParameterSet {
        version: 1,
        entries: vec![parameter(name, value)],
    }
}

#[test]
fn constant_field_grid_is_uniform_and_finite() {
    let grid = sample_grid(&expr("2.5"), xy_frame(), 9, 8, 0.0).expect("valid grid");
    assert_eq!((grid.width, grid.height), (9, 8));
    assert_eq!(grid.samples.len(), 9 * 8);
    assert!(grid.samples.iter().all(|sample| sample.value == Some(2.5)));
    assert!(
        grid.samples
            .iter()
            .all(|sample| sample.position.is_finite())
    );
    assert_eq!(grid.painted_cells(), 8 * 7);
    assert_eq!(grid.unpainted_cells(), 0);
}

#[test]
fn linear_field_grid_matches_analytic_corners() {
    let grid = sample_grid(&expr("x + 2*y"), xy_frame(), 9, 9, 0.0).expect("valid grid");
    assert!((value(&grid, 0, 0) - -3.0).abs() < 1e-6);
    assert!((value(&grid, 8, 0) - -1.0).abs() < 1e-6);
    assert!((value(&grid, 0, 8) - 1.0).abs() < 1e-6);
    assert!((value(&grid, 8, 8) - 3.0).abs() < 1e-6);
    // Plane-local (u, v) maps to the world position on the frame.
    assert!((grid.sample(0, 0).position - Vec3::new(-1.0, -1.0, 0.0)).length() < 1e-6);
    assert!((grid.sample(8, 8).position - Vec3::new(1.0, 1.0, 0.0)).length() < 1e-6);
}

#[test]
fn saddle_field_grid_is_antisymmetric() {
    let grid = sample_grid(&expr("x*y"), xy_frame(), 9, 9, 0.0).expect("valid grid");
    for j in 0..9 {
        for i in 0..9 {
            let a = value(&grid, i, j);
            let mirrored_u = value(&grid, 8 - i, j);
            let mirrored_v = value(&grid, i, 8 - j);
            assert!(
                (a + mirrored_u).abs() < 1e-6,
                "({i},{j}) {a} vs u-mirror {mirrored_u}"
            );
            assert!(
                (a + mirrored_v).abs() < 1e-6,
                "({i},{j}) {a} vs v-mirror {mirrored_v}"
            );
        }
    }
    assert!(value(&grid, 4, 4).abs() < 1e-6, "the saddle centre is zero");
}

#[test]
fn pole_samples_are_none_and_cells_stay_unpainted() {
    let grid = sample_grid(&expr("1/x"), xy_frame(), 9, 9, 0.0).expect("valid grid");
    // The ninth column sits exactly on the pole at u == 0.
    assert_eq!(grid.sample(4, 0).value, None);
    assert_eq!(grid.sample(4, 8).value, None);
    // The two cell columns touching the pole cannot be painted.
    for j in 0..8 {
        assert!(!grid.cell_is_painted(3, j));
        assert!(!grid.cell_is_painted(4, j));
    }
    assert_eq!(grid.unpainted_cells(), 2 * 8);
    assert_eq!(grid.painted_cells(), 8 * 8 - 2 * 8);
}

#[test]
fn domain_edges_do_not_fabricate_samples() {
    let grid = sample_grid(&expr("sqrt(x)"), xy_frame(), 9, 9, 0.0).expect("valid grid");
    assert_eq!(
        grid.sample(0, 4).value,
        None,
        "the invalid edge stays absent"
    );
    assert!(grid.sample(8, 4).value.is_some(), "the finite edge is kept");
    assert!(value(&grid, 8, 4) > 0.99);
    // No null sample receives a neighbouring value.
    for j in 0..9 {
        assert_eq!(grid.sample(0, j).value, None);
    }
}

#[test]
fn animated_samples_track_the_shared_clock() {
    let expr = expr("x + t");
    let first = sample_grid(&expr, xy_frame(), 9, 9, 0.0).expect("valid grid");
    let second = sample_grid(&expr, xy_frame(), 9, 9, 0.75).expect("valid grid");
    assert_eq!(first.time, 0.0);
    assert_eq!(second.time, 0.75);
    for j in 0..9 {
        for i in 0..9 {
            let delta = value(&second, i, j) - value(&first, i, j);
            assert!(
                (delta - 0.75).abs() < 1e-6,
                "sample ({i},{j}) moved {delta}"
            );
        }
    }
}

#[test]
fn grid_and_line_bounds_are_enforced_before_allocation() {
    assert_eq!(MAX_GRID_SIDE, 160);
    assert_eq!(MIN_GRID_SIDE, 8);
    assert_eq!(MAX_LINE_SAMPLES, 512);
    assert_eq!(MAX_PROBES, 8);
    assert_eq!(MAX_CONTOUR_LEVELS, 16);

    let expr = expr("x");
    for (width, height) in [(MIN_GRID_SIDE - 1, 8), (8, MIN_GRID_SIDE - 1), (8, 161)] {
        let error = sample_grid(&expr, xy_frame(), width, height, 0.0)
            .expect_err("an out-of-range grid must be rejected");
        assert!(
            matches!(error, ScalarViewError::GridSize { .. }),
            "got {error:?}"
        );
    }

    let mismatch = ScalarGrid::new(xy_frame(), 8, 8, 0.0, vec![])
        .expect_err("a short sample vector must be rejected");
    assert!(matches!(mismatch, ScalarViewError::SampleCount { .. }));

    for count in [1, MAX_LINE_SAMPLES + 1] {
        let error = sample_line(
            &expr,
            &xy_frame(),
            Vec3::new(-1.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            count,
            0.0,
        )
        .expect_err("an out-of-range line must be rejected");
        assert!(
            matches!(error, ScalarViewError::LineSamples { .. }),
            "got {error:?}"
        );
    }

    sample_grid(&expr, xy_frame(), MIN_GRID_SIDE, MAX_GRID_SIDE, 0.0)
        .expect("the inclusive bounds are accepted");

    let mut probes = ProbeSet::default();
    for _ in 0..MAX_PROBES {
        probes
            .push_point(PointProbe::new(Vec3::ZERO, None).expect("finite probe"))
            .expect("room below the shared cap");
    }
    let over = probes
        .push_line(LineProbe::new(Vec3::ZERO, Vec3::X, None, Vec::new()).expect("finite line"))
        .expect_err("the shared probe cap is enforced");
    assert!(matches!(
        over,
        ScalarViewError::ProbeLimit { max: MAX_PROBES }
    ));
}

#[test]
fn contour_levels_must_be_finite_strictly_increasing_and_bounded() {
    let grid = sample_grid(&expr("x"), xy_frame(), 9, 9, 0.0).expect("valid grid");
    for levels in [
        vec![f32::NAN],
        vec![f32::INFINITY],
        vec![0.5, 0.5],
        vec![0.5, 0.25],
        vec![0.0; MAX_CONTOUR_LEVELS + 1],
    ] {
        let error =
            contour_segments(&grid, &levels).expect_err("an invalid level set must be rejected");
        assert!(
            matches!(error, ScalarViewError::ContourLevels { .. }),
            "got {error:?}"
        );
    }
    assert!(
        contour_segments(&grid, &[])
            .expect("no levels is valid")
            .is_empty()
    );
    let mut bounded = Vec::new();
    for n in 0..MAX_CONTOUR_LEVELS {
        bounded.push(n as f32 * 0.05);
    }
    contour_segments(&grid, &bounded).expect("the level cap is inclusive");
}

#[test]
fn contours_emit_nothing_across_missing_corners() {
    let frame = xy_frame();
    let mut samples = Vec::new();
    for j in 0..8 {
        for i in 0..8 {
            let value = if i == 7 && j == 7 { 1.0 } else { 0.0 };
            samples.push(ScalarSample {
                position: Vec3::new(i as f32 / 3.5 - 1.0, j as f32 / 3.5 - 1.0, 0.0),
                value: Some(value),
            });
        }
    }
    let complete = ScalarGrid::new(frame, 8, 8, 0.0, samples.clone()).expect("valid grid");
    assert_eq!(
        contour_segments(&complete, &[0.5])
            .expect("valid levels")
            .len(),
        1,
        "only the single rising cell emits a segment"
    );

    let mut holed = samples;
    holed[7 * 8 + 7].value = None;
    let holed = ScalarGrid::new(frame, 8, 8, 0.0, holed).expect("valid grid");
    assert!(
        contour_segments(&holed, &[0.5])
            .expect("valid levels")
            .is_empty(),
        "a cell with a missing corner emits no segment"
    );
}

#[test]
fn probes_are_document_state_and_restore_without_dirtying() {
    let point = PointProbe::new(Vec3::new(0.25, -0.5, 0.0), None).expect("valid probe");
    let line = LineProbe::new(
        Vec3::new(-1.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        None,
        Vec::new(),
    )
    .expect("valid probe");

    let mut state = State::new(false);
    state
        .probes
        .push_point(point.clone())
        .expect("room for a probe");
    state
        .probes
        .push_line(line.clone())
        .expect("room for a probe");
    let record = ViewRecord::capture(&state);
    assert_eq!(record.probes.points, vec![point.clone()]);
    assert_eq!(record.probes.lines, vec![line.clone()]);

    // Direct assignment is the startup seam: probes alone never dirty.
    let mut direct = State::new(false);
    direct.dirty = false;
    direct.probes = record.probes.clone();
    assert_eq!(direct.probes.points, vec![point.clone()]);
    assert!(!direct.dirty, "assigning probes leaves dirty alone");

    // The enclosing view restore asks for its documented single rebuild and
    // does not request a second one because of the probes.
    let mut restored = State::new(false);
    record.restore(&mut restored, &mut Controls::new());
    assert_eq!(restored.probes.points, vec![point]);
    assert_eq!(restored.probes.lines, vec![line]);
    assert!(restored.dirty, "restore asks for its one rebuild");
    assert!(restored.snapshot_if_dirty().is_some());
    assert!(
        restored.snapshot_if_dirty().is_none(),
        "the restored probes do not re-dirty the scene"
    );
}

#[test]
fn parameter_edit_invalidates_cached_records() {
    let mut state = State::new(false);
    state.set_scalar_source(ScalarContext::Surface, "k*x".into());
    state
        .commit_parameters(parameter_set("k", 1.0))
        .expect("valid parameter set");

    let first = state
        .refresh_scalar_view(true)
        .expect("the first visible refresh samples")
        .grid
        .sample(0, 0)
        .value
        .expect("finite sample");
    assert_eq!(state.scalar_view_cache().resamples(), 1);
    let again = state
        .refresh_scalar_view(true)
        .expect("the cache is warm")
        .grid
        .sample(0, 0)
        .value
        .expect("finite sample");
    assert_eq!(again, first);
    assert_eq!(
        state.scalar_view_cache().resamples(),
        1,
        "no extra resample"
    );

    state
        .commit_parameters(parameter_set("k", 2.0))
        .expect("valid parameter set");
    let changed = state
        .refresh_scalar_view(true)
        .expect("the parameter edit invalidates the cache")
        .grid
        .sample(0, 0)
        .value
        .expect("finite sample");
    assert!((changed - 2.0 * first).abs() < 1e-6, "{changed} vs {first}");
    assert_eq!(
        state.scalar_view_cache().resamples(),
        2,
        "invalidation causes exactly one resample"
    );
}

#[test]
fn line_probe_samples_preserve_invalidity() {
    let grid = sample_line(
        &expr("1/x"),
        &xy_frame(),
        Vec3::new(-1.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        511,
        0.0,
    )
    .expect("valid line");
    assert_eq!(grid.len(), 511);
    assert!(grid.iter().all(|sample| sample.position.is_finite()));
    let missing = grid.iter().filter(|sample| sample.value.is_none()).count();
    assert_eq!(missing, 1, "only the pole is absent");
    assert_eq!(grid[255].value, None);

    let probe = LineProbe::new(
        Vec3::new(-1.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        None,
        grid.clone(),
    )
    .expect("valid probe");
    assert_eq!(probe.samples, grid);
}

#[test]
fn linear_field_contour_segments_lie_at_the_level() {
    let grid = sample_grid(&expr("x"), xy_frame(), 9, 9, 0.0).expect("valid grid");
    let segments = contour_segments(&grid, &[0.25]).expect("valid levels");
    assert!(!segments.is_empty());
    for segment in &segments {
        assert_eq!(segment.level, 0.25);
        assert!(
            (segment.start.x - 0.25).abs() < 1e-5,
            "start {} is off the level",
            segment.start.x
        );
        assert!(
            (segment.end.x - 0.25).abs() < 1e-5,
            "end {} is off the level",
            segment.end.x
        );
    }
}

#[test]
fn hidden_panel_samples_nothing_and_animated_refresh_is_throttled() {
    let mut state = State::new(false);
    state.set_scalar_source(ScalarContext::Surface, "x + t".into());

    assert!(
        state.refresh_scalar_view(false).is_none(),
        "a hidden panel with no cache samples nothing"
    );
    assert_eq!(state.scalar_view_cache().resamples(), 0);

    state.set_time(0.0);
    let first = state
        .refresh_scalar_view(true)
        .expect("showing the panel samples once immediately");
    assert_eq!(first.grid.time, 0.0);
    assert_eq!(state.scalar_view_cache().resamples(), 1);

    state.set_time(0.05);
    let throttled = state
        .refresh_scalar_view(true)
        .expect("the warm cache is returned");
    assert_eq!(
        throttled.grid.time, 0.0,
        "a refresh inside the 100 ms bucket reuses the record"
    );
    assert_eq!(state.scalar_view_cache().resamples(), 1);

    state.set_time(0.11);
    let next = state
        .refresh_scalar_view(true)
        .expect("the next clock bucket samples");
    assert!((next.grid.time - 0.11).abs() < 1e-6);
    assert_eq!(state.scalar_view_cache().resamples(), 2);

    state.set_time(0.4);
    assert!(
        state.refresh_scalar_view(false).is_some(),
        "the cache is retained"
    );
    assert_eq!(
        state.scalar_view_cache().resamples(),
        2,
        "a hidden panel performs no sampling"
    );
    state
        .refresh_scalar_view(true)
        .expect("showing it again samples the pending change once");
    assert_eq!(state.scalar_view_cache().resamples(), 3);
}

#[test]
fn z_formula_reports_the_scalar_unsupported_reason() {
    assert_eq!(UNSUPPORTED_Z_REASON, "scalar views support x/y formulas");
    for source in ["z", "x + z", "exp(z)"] {
        let error = scalar_view_expr(source, &ParameterSet::default())
            .expect_err("a z formula is unsupported");
        assert_eq!(error.to_string(), UNSUPPORTED_Z_REASON, "source {source}");
        assert!(matches!(error, ScalarViewError::UnsupportedZ));
    }

    let mut state = State::new(false);
    state.set_scalar_source(ScalarContext::Surface, "z + x".into());
    assert!(state.refresh_scalar_view(true).is_none());
    assert_eq!(
        state.scalar_view_cache().error(),
        Some(UNSUPPORTED_Z_REASON)
    );
}
