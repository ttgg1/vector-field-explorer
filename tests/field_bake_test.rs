//! P3 slice 1a: CPU baked field model contract tests.
//!
//! These tests cover the model and lifecycle only. No default particle or
//! streamline call site switches to the bake in this slice, so the existing
//! byte-identical reference path stays in force.

use glam::Vec3;
use vector_field_explorer::appearance_profile::AppearanceProfile;
use vector_field_explorer::domain::Domain;
use vector_field_explorer::edit_history::EditSnapshot;
use vector_field_explorer::field::FieldExpr;
use vector_field_explorer::field_bake::{
    BakeAccuracyProbes, BakeAxis, BakeError, BakeResolution, BakeSpec, BakedField,
    MAX_LATTICE_BYTES,
};
use vector_field_explorer::flow::{BakedFlow, Flow, PlaneFlow};
use vector_field_explorer::parameters::{Parameter, ParameterSet};
use vector_field_explorer::particles::{self, AdvectSettings, Particles};
use vector_field_explorer::persistence::ViewRecord;
use vector_field_explorer::plane::{PlaneAxis, PlaneFrame, PlaneView};
use vector_field_explorer::scene_document::SceneCamera;
use vector_field_explorer::source_scene::FieldMode;
use vector_field_explorer::ui::sidebar::Controls;
use vector_field_explorer::viewmodel::State;

fn spec(axis: BakeAxis, resolution: u32, formula: &str) -> BakeSpec {
    BakeSpec {
        formula: formula.to_string(),
        parameters: ParameterSet::default(),
        domain: Domain::default(),
        axis,
        resolution,
        field_mode: FieldMode::Formula,
        plane_frame: None,
    }
}

fn volume_spec(resolution: u32, formula: &str) -> BakeSpec {
    spec(BakeAxis::Volume3d, resolution, formula)
}

fn plane_spec(resolution: u32, formula: &str) -> BakeSpec {
    spec(BakeAxis::Plane2d, resolution, formula)
}

fn assert_close(actual: Vec3, expected: Vec3, tolerance: f32) {
    for (a, e) in [
        (actual.x, expected.x),
        (actual.y, expected.y),
        (actual.z, expected.z),
    ] {
        assert!(
            (a - e).abs() <= tolerance,
            "component differs: baked {actual:?}, exact {expected:?}"
        );
    }
}

fn assert_positions_bit_equal(left: &[Vec3], right: &[Vec3]) {
    assert_eq!(left.len(), right.len(), "position count differs");
    for (a, b) in left.iter().zip(right) {
        assert_eq!(
            a.to_array().map(f32::to_bits),
            b.to_array().map(f32::to_bits),
            "particle position differs"
        );
    }
}

fn assert_lines_bit_equal(
    left: &[vector_field_explorer::renderer::LineInstanceGPU],
    right: &[vector_field_explorer::renderer::LineInstanceGPU],
) {
    assert_eq!(left.len(), right.len(), "line count differs");
    for (a, b) in left.iter().zip(right) {
        assert_eq!(
            a.a.map(f32::to_bits),
            b.a.map(f32::to_bits),
            "line start differs"
        );
        assert_eq!(
            a.b.map(f32::to_bits),
            b.b.map(f32::to_bits),
            "line end differs"
        );
        assert_eq!(
            a.color.map(f32::to_bits),
            b.color.map(f32::to_bits),
            "line colour differs"
        );
    }
}

#[test]
fn linear_field_is_reproduced_at_interior_points() {
    let formula = "(z + 2*y, x - z, y + 1)";
    let field = FieldExpr::parse(formula).unwrap();
    let baked = BakedField::build(volume_spec(24, formula), &field).unwrap();

    let probes = [
        Vec3::new(-0.9, -0.7, -0.4),
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(0.3, 0.6, -0.2),
        Vec3::new(0.95, 0.8, 0.7),
    ];
    for point in probes {
        let exact = field.eval3(point).unwrap();
        let sampled = baked.sample(point).expect("interior point samples");
        assert_close(sampled, exact, 1e-4);
    }

    // A linear field has zero divergence, and the reported residual agrees.
    let report = baked.accuracy_report(BakeAccuracyProbes {
        interpolation: Some(&field),
        divergence_free: Some(&field),
    });
    assert_eq!(report.resolution, 24);
    let interpolation = report.interpolation_max_error.expect("measured");
    assert!(
        interpolation <= 1e-4,
        "linear error {interpolation} is not f32-tight"
    );
    let residual = report.divergence_residual.expect("measured");
    assert!(
        residual <= 1e-3,
        "linear divergence residual {residual} is too large"
    );
}

#[test]
fn quadratic_field_error_is_bounded_and_reported() {
    let formula = "(x*x + y*y + z*z, x*y, y*z)";
    let field = FieldExpr::parse(formula).unwrap();
    let resolution = 24;
    let baked = BakedField::build(volume_spec(resolution, formula), &field).unwrap();

    let report = baked.accuracy_report(BakeAccuracyProbes {
        interpolation: Some(&field),
        divergence_free: None,
    });
    assert_eq!(report.resolution, resolution);
    let error = report
        .interpolation_max_error
        .expect("a supplied reference is measured");
    assert!(error > 0.0, "a quadratic field cannot be exact");
    // Cell width is 2/23; the linear-interpolation error of x² is h²/4 ≈ 0.0019.
    assert!(
        error < 0.02,
        "quadratic error {error} exceeds the documented bound"
    );
    assert_eq!(
        report.divergence_residual, None,
        "an unmeasured residual must stay absent, never fabricated"
    );

    // No reference means no measured number at all.
    let unmeasured = baked.accuracy_report(BakeAccuracyProbes::default());
    assert_eq!(unmeasured.interpolation_max_error, None);
    assert_eq!(unmeasured.divergence_residual, None);
}

#[test]
fn outside_domain_samples_return_none() {
    let formula = "(x, y)";
    let field = FieldExpr::parse(formula).unwrap();
    let baked = BakedField::build(plane_spec(32, formula), &field).unwrap();

    assert!(baked.sample(Vec3::new(2.0, 0.0, 0.0)).is_none());
    assert!(baked.sample(Vec3::new(0.0, -1.5, 0.0)).is_none());
    assert!(baked.sample(Vec3::new(f32::NAN, 0.0, 0.0)).is_none());
    // A genuine 2D bake ignores z exactly like `FieldExpr`'s two components.
    assert!(baked.sample(Vec3::new(0.0, 0.0, 5.0)).is_some());
}

#[test]
fn extreme_resolution_is_refused_without_overflow() {
    let formula = "(x, y, z)";
    let field = FieldExpr::parse(formula).unwrap();
    for axis in [BakeAxis::Volume3d, BakeAxis::Plane2d] {
        let error = BakedField::build(spec(axis, u32::MAX, formula), &field)
            .expect_err("u32::MAX cannot be a valid lattice");
        assert!(
            matches!(error, BakeError::Memory { .. }),
            "{axis:?}: extreme resolution must be refused as Memory, got {error:?}"
        );
    }
}

#[test]
fn plane_resolution_bounds_are_inclusive() {
    let formula = "(x, y)";
    let field = FieldExpr::parse(formula).unwrap();
    assert!(BakedField::build(plane_spec(32, formula), &field).is_ok());
    assert!(BakedField::build(plane_spec(256, formula), &field).is_ok());

    for (got, min, max) in [(31, 32, 256), (257, 32, 256)] {
        let error = BakedField::build(plane_spec(got, formula), &field).unwrap_err();
        assert_eq!(
            error,
            BakeError::Resolution {
                axis: BakeAxis::Plane2d,
                min,
                max,
                got,
            }
        );
    }
}

#[test]
fn non_finite_domain_extent_is_empty() {
    let formula = "(x, y)";
    let field = FieldExpr::parse(formula).unwrap();
    let mut spec = plane_spec(32, formula);
    spec.domain = Domain::new(Vec3::splat(-1.0), Vec3::new(f32::INFINITY, 1.0, 1.0));
    assert!(matches!(
        BakedField::build(spec, &field),
        Err(BakeError::EmptyDomain)
    ));
}

#[test]
fn volume_sample_outside_domain_z_is_none() {
    let formula = "(x, y, z)";
    let field = FieldExpr::parse(formula).unwrap();
    let baked = BakedField::build(volume_spec(16, formula), &field).unwrap();
    assert!(baked.sample(Vec3::new(0.0, 0.0, 0.0)).is_some());
    assert!(
        baked.sample(Vec3::new(0.0, 0.0, 1.5)).is_none(),
        "a volume bake must not clamp a query outside the z box"
    );
}

#[test]
fn time_dependent_field_refuses_to_bake() {
    let formula = "(t, 0, 0)";
    let field = FieldExpr::parse(formula).unwrap();
    let error = BakedField::build(volume_spec(16, formula), &field).unwrap_err();
    assert!(
        matches!(error, BakeError::TimeDependent),
        "expected TimeDependent, got {error:?}"
    );
    assert_eq!(error.to_string(), "cannot bake a time-dependent field");
}

#[test]
fn non_finite_corner_propagates_as_none_never_nan_motion() {
    // `1/(x+1)` is infinite at the domain's x = -1 corner, so that lattice
    // node has no finite value.
    let formula = "(1/(x + 1), 0, 0)";
    let field = FieldExpr::parse(formula).unwrap();
    let baked = BakedField::build(volume_spec(16, formula), &field).unwrap();

    let bad = baked.sample(Vec3::new(-1.0, -1.0, -1.0));
    assert_eq!(bad, None, "a non-finite corner never becomes finite motion");
    assert_eq!(
        baked.sample(Vec3::new(-0.98, -0.98, -0.98)),
        None,
        "the cell containing the corner is refused, not NaN-interpolated"
    );
    let good = baked.sample(Vec3::new(0.5, 0.5, 0.5));
    assert!(good.is_some_and(|v| v.is_finite()));
}

#[test]
fn same_spec_and_field_produce_identical_lattice_bytes() {
    let formula = "(sin(x) + y, cos(z) - x, x*y*z)";
    let field = FieldExpr::parse(formula).unwrap();
    let first = BakedField::build(volume_spec(20, formula), &field).unwrap();
    let second = BakedField::build(volume_spec(20, formula), &field).unwrap();
    assert_eq!(first.lattice().len(), second.lattice().len());
    for (a, b) in first.lattice().iter().zip(second.lattice()) {
        assert_eq!(
            a.to_array().map(f32::to_bits),
            b.to_array().map(f32::to_bits),
            "same input must produce the same lattice bytes"
        );
    }
    assert_eq!(first.spec(), second.spec());
}

#[test]
fn non_formula_field_mode_is_refused_by_build_and_rebuild() {
    let formula = "(x, y, z)";
    let field = FieldExpr::parse(formula).unwrap();
    let mut sources = volume_spec(16, formula);
    sources.field_mode = FieldMode::Sources;
    assert_eq!(
        BakedField::build(sources, &field).unwrap_err(),
        BakeError::UnsupportedMode {
            mode: FieldMode::Sources
        }
    );

    let mut state = State::new(false);
    state.set_field_mode(FieldMode::Sources);
    assert_eq!(
        state.rebuild_field_bake(32).unwrap_err(),
        BakeError::UnsupportedMode {
            mode: FieldMode::Sources
        }
    );
    assert!(state.field_bake().is_none());
}

#[test]
fn maxwell_refresh_clears_a_bake_from_a_bypassed_mode_write() {
    let mut state = State::new(false);
    state.rebuild_field_bake(32).unwrap();
    assert!(state.field_bake().is_some());

    // A direct `field_mode` write bypasses the setter's invalidation.
    state.force_field_mode_for_tests(FieldMode::Maxwell);
    state.refresh_maxwell_sampling();
    assert!(
        state.field_bake().is_none(),
        "a Maxwell refresh must not keep a formula bake"
    );
}

#[test]
fn memory_cap_rejects_an_over_cap_resolution_and_axis() {
    assert_eq!(MAX_LATTICE_BYTES, 3_145_728);
    // 64³ × 12 bytes is exactly the ADR cap and remains allowed.
    let formula = "(x, y, z)";
    let field = FieldExpr::parse(formula).unwrap();
    assert!(BakedField::build(volume_spec(64, formula), &field).is_ok());

    // The budget is checked before the per-axis resolution bound, so a caller
    // asking for a far larger allocation is refused without allocating.
    let error = BakedField::build(volume_spec(256, formula), &field).unwrap_err();
    assert!(
        matches!(error, BakeError::Memory { .. }),
        "expected Memory, got {error:?}"
    );

    // The validated resolution type never clamps: 15 is refused, not raised to
    // 16, and a zero-extent domain is refused as empty.
    assert!(matches!(
        BakeResolution::new(BakeAxis::Volume3d, 15),
        Err(BakeError::Resolution { .. })
    ));
    let mut empty = volume_spec(16, formula);
    empty.domain = Domain::new(Vec3::ZERO, Vec3::ZERO);
    assert!(matches!(
        BakedField::build(empty, &field),
        Err(BakeError::EmptyDomain)
    ));
}

#[test]
fn rebuild_is_invalidated_by_each_field_affecting_change_only() {
    let mut state = State::new(false);
    assert!(state.field_bake().is_none());
    state.rebuild_field_bake(32).unwrap();
    assert!(state.field_bake().is_some());

    // Unrelated display state preserves the bake.
    state.set_arrows(false);
    state.set_show_grid(false);
    assert!(state.field_bake().is_some());

    state.set_formula("(x*x, y)".into());
    assert!(state.field_bake().is_none(), "formula change invalidates");
    state.rebuild_field_bake(32).unwrap();

    let parameters = ParameterSet {
        entries: vec![Parameter {
            name: "k".into(),
            value: 2.0,
            step: 1.0,
            ..Parameter::default()
        }],
        ..ParameterSet::default()
    };
    state.commit_parameters(parameters).unwrap();
    assert!(state.field_bake().is_none(), "parameter change invalidates");
    state.rebuild_field_bake(32).unwrap();

    state.set_domain(Vec3::splat(-2.0), Vec3::splat(2.0));
    assert!(state.field_bake().is_none(), "domain change invalidates");
    state.rebuild_field_bake(32).unwrap();

    state.set_mode(true);
    assert!(state.field_bake().is_none(), "mode change invalidates");
    state.rebuild_field_bake(32).unwrap();

    state.set_field_mode(FieldMode::Sources);
    assert!(
        state.field_bake().is_none(),
        "field mode change invalidates"
    );

    let mut plane = State::new(false);
    plane.set_formula("(z, x, y)".into());
    assert!(plane.plane_frame().is_some(), "3D field in 2D is a slice");
    plane.rebuild_field_bake(32).unwrap();
    plane.set_plane_axis(PlaneAxis::X);
    assert!(
        plane.field_bake().is_none(),
        "plane-frame change invalidates"
    );
}

#[test]
fn bake_is_runtime_only_and_absent_from_capture_restore_and_history() {
    let mut state = State::new(false);
    assert!(state.field_bake().is_none());

    // The edit-history document snapshot is identical with and without a bake
    // on otherwise-identical state: the bake is not document state.
    let without_bake = EditSnapshot::new(
        ViewRecord::capture(&state),
        state.parameters.clone(),
        SceneCamera::default(),
        AppearanceProfile::capture("runtime-only", &state),
    );
    state.rebuild_field_bake(32).unwrap();
    assert!(state.field_bake().is_some());
    let with_bake = EditSnapshot::new(
        ViewRecord::capture(&state),
        state.parameters.clone(),
        SceneCamera::default(),
        AppearanceProfile::capture("runtime-only", &state),
    );
    assert_eq!(without_bake, with_bake);

    // A `ViewRecord` never carries the lattice; restoring one clears the
    // runtime bake rather than reviving a value the record cannot describe.
    let view = ViewRecord::capture(&state);
    let mut restored = State::new(false);
    restored.rebuild_field_bake(32).unwrap();
    view.restore(&mut restored, &mut Controls::new());
    assert!(restored.field_bake().is_none());
}

#[test]
fn clearing_the_bake_leaves_existing_particle_and_streamline_results_identical() {
    let mut state = State::new(true);
    state.set_formula("(-y, x, z)".into());
    state.set_density(16);
    state.set_streamlines(true);
    let positions = vec![
        Vec3::new(-0.5, -0.5, 0.0),
        Vec3::new(0.25, -0.1, 0.4),
        Vec3::new(0.6, 0.3, -0.2),
        Vec3::new(-0.2, 0.7, 0.1),
    ];
    let settings = AdvectSettings {
        dt: 0.02,
        time: 0.0,
        trail_len: 8,
        loop_on_stagnation: false,
        stagnation_seconds: 0.5,
    };

    let mut before = Particles::with_positions(positions.clone());
    particles::advect(
        &mut before,
        &state.scene_flow(),
        &state.scene_domain(),
        settings,
    );
    let lines_before = state.streamlines_snapshot(&state.sample());

    state.rebuild_field_bake(32).unwrap();
    let mut during = Particles::with_positions(positions.clone());
    particles::advect(
        &mut during,
        &state.scene_flow(),
        &state.scene_domain(),
        settings,
    );
    let lines_during = state.streamlines_snapshot(&state.sample());

    state.clear_field_bake();
    assert!(state.field_bake().is_none());
    let mut after = Particles::with_positions(positions);
    particles::advect(
        &mut after,
        &state.scene_flow(),
        &state.scene_domain(),
        settings,
    );
    let lines_after = state.streamlines_snapshot(&state.sample());

    // Slice 1a never routes these call sites through the bake.
    assert_positions_bit_equal(&before.pos, &during.pos);
    assert_positions_bit_equal(&before.pos, &after.pos);
    assert_lines_bit_equal(&lines_before, &lines_during);
    assert_lines_bit_equal(&lines_before, &lines_after);
}

#[test]
fn baked_flow_matches_plane_flow_projection_and_stationary_rule() {
    // The field varies with z across the selected plane, so sampling the local
    // `(u, v, 0)` point instead of lifting it to world would be visible.
    let formula = "(z, 0, 0)";
    let field = FieldExpr::parse(formula).unwrap();
    let baked = BakedField::build(volume_spec(24, formula), &field).unwrap();
    let domain = Domain::default();
    let frame = PlaneFrame::new(
        &domain,
        PlaneView {
            axis: PlaneAxis::Z,
            index: 3,
            ..Default::default()
        },
        glam::UVec3::splat(5),
    );
    assert!(
        frame.origin.z != 0.0,
        "the selected plane must sit off z = 0 for the lift to matter"
    );

    let plane_flow = PlaneFlow {
        field: &field,
        frame,
    };
    let baked_flow = BakedFlow {
        baked: &baked,
        plane: Some(frame),
    };
    for point in [Vec3::ZERO, Vec3::new(0.5, -0.25, 0.0)] {
        let world = frame.to_world(point);
        let expected = plane_flow.eval(point, 0.0).unwrap();
        assert!(
            expected.x.abs() > 0.1,
            "the lifted sample must carry the plane's z into x"
        );
        let actual = baked_flow.eval(point, 0.0).unwrap();
        assert_close(actual, expected, 1e-4);
        // Lifting/sampling/projecting keeps the result in-plane.
        assert_eq!(actual.z, 0.0);
        assert!(world.is_finite());
    }
    assert!(baked_flow.zero_velocity_is_stationary());
    assert!(
        !BakedFlow {
            baked: &baked,
            plane: None,
        }
        .zero_velocity_is_stationary()
    );
}
