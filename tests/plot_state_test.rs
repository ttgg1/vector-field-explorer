use glam::Vec3;
use std::sync::Arc;
use vector_field_explorer::{
    Colormap, Domain,
    plot::{CurveEdges, PlotKind, SurfaceMesh},
    plot_scene::{PlotGeometry, build_plot_scene},
    plot_state::{PlotSettings, SampledPlot},
};
use vector_field_explorer::{
    parameters::{Parameter, ParameterSet},
    plot_examples::EXAMPLES,
    scalar::ScalarContext,
    viewmodel::State,
};

fn separation_parameter(value: f64) -> ParameterSet {
    ParameterSet {
        version: 1,
        entries: vec![Parameter {
            name: "d".into(),
            value,
            min: None,
            max: None,
            step: 0.1,
        }],
    }
}

#[test]
fn parameterized_surface_install_rejects_before_touching_any_state() {
    let mut s = State::new(false);
    s.set_plot_kind(PlotKind::Surface3D);
    assert!(s.plot_scene().bounds.is_some(), "the old mesh is painted");
    let before_parameters = s.parameters.clone();
    let before_source = s.plot.surface.source.clone();
    let before_samples = s.plot.surface.samples.clone().expect("sampled mesh");
    let before_generation = s.plot.generation;

    let error = s
        .apply_parameterized_surface(separation_parameter(1.0), "d +".into())
        .expect_err("the parameter-aware surface parser rejects the draft");

    assert!(
        error.contains("parse") || error.contains("error"),
        "the parser error is actionable: {error}"
    );
    assert_eq!(
        s.parameters, before_parameters,
        "no parameter was committed"
    );
    assert_eq!(s.plot.surface.source, before_source, "source unchanged");
    assert!(
        Arc::ptr_eq(
            &before_samples,
            s.plot.surface.samples.as_ref().expect("mesh unchanged")
        ),
        "the old mesh stays under its old source"
    );
    assert_eq!(s.plot.generation, before_generation, "no scene rebuild");
    assert!(
        s.plot.surface.error.is_none(),
        "a rejected install does not overwrite the retained status"
    );
}

#[test]
fn parameterized_surface_install_accepts_d_and_rebuilds_the_mesh() {
    let mut s = State::new(false);
    s.set_plot_kind(PlotKind::Surface3D);
    assert!(s.plot_scene().bounds.is_some());
    let before_samples = s.plot.surface.samples.clone().expect("sampled mesh");

    s.apply_parameterized_surface(separation_parameter(1.5), "d*x".into())
        .expect("the parameter-aware parser accepts d");

    assert_eq!(s.parameters.value("d"), Some(1.5));
    assert_eq!(s.plot.surface.source, "d*x");
    assert!(s.plot.surface.accepted.is_some());
    assert!(s.plot.surface.samples.is_none(), "the mesh is invalidated");
    assert!(s.plot_scene().bounds.is_some(), "the mesh rebuilds from d");
    assert!(
        !Arc::ptr_eq(
            &before_samples,
            s.plot.surface.samples.as_ref().expect("the rebuilt mesh")
        ),
        "the accepted d is baked into a fresh mesh"
    );
}

#[test]
fn parameter_edits_reparse_and_invalidate_the_installed_surface() {
    let mut s = State::new(false);
    s.set_plot_kind(PlotKind::Surface3D);
    s.apply_parameterized_surface(separation_parameter(1.5), "d*x".into())
        .expect("install the parameter-aware surface");
    assert!(s.plot_scene().bounds.is_some());
    assert_eq!(
        s.plot
            .surface
            .accepted
            .as_ref()
            .expect("accepted expression")
            .eval_at(1.0, 0.0, 0.0),
        Some(1.5)
    );
    let before_samples = s.plot.surface.samples.clone().expect("sampled mesh");

    let mut next = s.parameters.clone();
    next.entries[0].value = 3.0;
    s.commit_parameters(next)
        .expect("the ordinary Variables commit path");

    assert_eq!(s.parameters.value("d"), Some(3.0));
    assert_eq!(s.plot.surface.source, "d*x", "the source stays editable");
    let scene = s.plot_scene();
    assert!(scene.bounds.is_some(), "the mesh rebuilds for the new d");
    assert_eq!(
        s.plot
            .surface
            .accepted
            .as_ref()
            .expect("reparsed expression")
            .eval_at(1.0, 0.0, 0.0),
        Some(3.0),
        "the accepted expression follows the committed parameter"
    );
    assert!(
        !Arc::ptr_eq(
            &before_samples,
            s.plot.surface.samples.as_ref().expect("fresh mesh")
        ),
        "the old parameter value's mesh is invalidated"
    );
}

#[test]
fn active_invalid_edit_keeps_last_valid_but_destination_clears() {
    let mut s = State::new(true);
    s.set_plot_kind(PlotKind::Curve2D);
    assert!(s.plot_scene().bounds.is_some());
    let samples = s.plot.curve.samples.clone().unwrap();
    s.set_active_source("x +".into());
    let status = s.plot.curve.error.clone().unwrap();
    assert_eq!(s.plot_scene().status, status);
    assert!(Arc::ptr_eq(
        &samples,
        s.plot.curve.samples.as_ref().unwrap()
    ));
    s.set_plot_kind(PlotKind::Surface3D);
    s.set_plot_kind(PlotKind::Curve2D);
    assert!(s.plot_scene().bounds.is_none());
    assert_eq!(s.plot_scene().status, status);
}

#[test]
fn sources_and_projection_are_independent() {
    let mut s = State::new(true);
    s.set_formula("(x, y, z)".into());
    s.set_plot_kind(PlotKind::Curve2D);
    s.set_active_source("x+1".into());
    s.set_mode(true);
    s.set_active_source("x+y".into());
    s.set_mode(false);
    assert_eq!(s.active_source(), "x+1");
    s.set_plot_kind(PlotKind::VectorField);
    assert_eq!(s.formula, "(x, y, z)");
    assert_eq!(s.field.dimension, 3);
    assert!(s.mode_3d);
    assert_eq!(s.plot.surface.source, "x+y");
}

#[test]
fn kind_switches_preserve_the_vector_slice_index() {
    let mut s = State::new(true);
    let middle = s.plane.index;
    assert!(middle > 0);
    s.set_plot_kind(PlotKind::Curve2D);
    assert_eq!(
        s.plane.index, middle,
        "a three-component field keeps its plane grid in 2D, so the selected slice survives"
    );
    s.set_plot_kind(PlotKind::VectorField);
    assert_eq!(
        s.plane.index, middle,
        "returning to 3D keeps the same slice"
    );
}

#[test]
fn settings_sanitize_at_seam() {
    let mut s = State::new(false);
    s.set_plot_samples(usize::MAX);
    s.set_plot_resolution(u32::MAX);
    assert_eq!(s.plot.settings.curve_samples, 4097);
    assert_eq!(s.plot.settings.surface_resolution, 96);
    s.set_plot_width(f32::NAN);
    assert_eq!(s.plot.settings.line_width, 0.01);
    s.set_plot_width(1e6);
    assert_eq!(s.plot.settings.line_width, 0.2);
    s.set_plot_width(1.0);
    assert_eq!(s.plot.settings.line_width, 0.2);
    s.set_plot_width(0.0001);
    assert_eq!(s.plot.settings.line_width, 0.001);
    s.set_plot_width(0.001);
    assert_eq!(s.plot.settings.line_width, 0.001);
    s.set_plot_width(0.2);
    assert_eq!(s.plot.settings.line_width, 0.2);
    s.set_plot_width(0.01);
    assert_eq!(s.plot.settings.line_width, 0.01);
    s.set_plot_height_range(3., -2.);
    assert_eq!(
        (s.plot.settings.height_min, s.plot.settings.height_max),
        (-2., 3.)
    );
    s.set_plot_height_range(2., 2.);
    assert_eq!(
        (s.plot.settings.height_min, s.plot.settings.height_max),
        (2., 2.)
    );
    s.set_plot_height_range(f32::NAN, 2.);
    assert_eq!(
        (s.plot.settings.height_min, s.plot.settings.height_max),
        (0., 1.)
    );
    assert!(!s.set_plot_height_range(0., 1.));
}

#[test]
fn examples_use_real_setters() {
    let mut s = State::new(false);
    for example in EXAMPLES {
        s.select_plot_example(example);
        assert_eq!(s.plot.kind, example.kind);
        assert_eq!(s.active_source(), example.source);
        assert!(s.plot_scene().bounds.is_some(), "{}", example.name);
    }
    s.set_scalar_source(ScalarContext::Curve, "x+2".into());
    assert_eq!(s.plot.curve.source, "x+2");
}

#[test]
fn sampling_cache_tracks_domain_and_ignores_palette_and_height_range() {
    let mut s = State::new(false);
    s.set_plot_kind(PlotKind::Surface3D);
    s.plot_scene();
    let first = s.plot.surface.samples.clone().unwrap();
    let generation = s.plot.generation;
    s.set_colormap(Colormap::Mono);
    s.set_plot_auto_height(false);
    s.set_plot_height_range(-1., 1.);
    assert!(s.plot_scene().generation > generation);
    assert!(Arc::ptr_eq(
        &first,
        s.plot.surface.samples.as_ref().unwrap()
    ));
    let domain = s.domain;
    s.set_domain(domain.min - Vec3::Z, domain.max + Vec3::Z);
    s.plot_scene();
    assert!(Arc::ptr_eq(
        &first,
        s.plot.surface.samples.as_ref().unwrap()
    ));
    s.set_domain(domain.min - Vec3::X, domain.max);
    s.plot_scene();
    assert!(!Arc::ptr_eq(
        &first,
        s.plot.surface.samples.as_ref().unwrap()
    ));
}

#[test]
fn references_and_time_rebuild_without_resampling_scalar_geometry() {
    let mut s = State::new(false);
    s.set_plot_kind(PlotKind::Curve2D);
    s.plot_scene();
    let samples = s.plot.curve.samples.clone().unwrap();
    let generation = s.plot.generation;
    s.set_show_grid(!s.show_grid);
    s.set_show_coordinate_system(!s.show_coordinate_system);
    let scene = s.plot_scene();
    assert!(scene.generation > generation);
    assert!(Arc::ptr_eq(
        &samples,
        s.plot.curve.samples.as_ref().unwrap()
    ));
    let generation = s.plot.generation;
    s.set_time(3.0);
    s.plot_scene();
    assert_eq!(s.plot.generation, generation);
    assert!(Arc::ptr_eq(
        &samples,
        s.plot.curve.samples.as_ref().unwrap()
    ));
}

#[test]
fn restore_invalid_drafts_clears_prior_samples_and_reset_preserves_other_draft() {
    let mut s = State::new(true);
    s.set_plot_kind(PlotKind::Curve2D);
    s.plot_scene();
    s.restore_plot_drafts("".into(), "x+y".into());
    assert!(s.plot_scene().bounds.is_none());
    assert!(s.plot.curve.error.is_some());
    s.reset_plot_section();
    assert_eq!(s.plot.curve.source, "0.5*x^2");
    assert_eq!(s.plot.surface.source, "x+y");
    assert_eq!(s.formula, "(x, y, z)");
}

#[test]
fn restore_matching_sources_reparses_valid_and_empty_drafts() {
    let mut s = State::new(false);
    s.set_plot_kind(PlotKind::Curve2D);
    s.set_active_source("".into());
    s.set_scalar_source(ScalarContext::Surface, "x+y".into());
    s.restore_plot_drafts("".into(), "x+y".into());
    assert!(s.plot.curve.error.is_some());
    assert!(s.plot.curve.accepted.is_none());
    assert!(s.plot.surface.accepted.is_some());
    assert!(s.plot.surface.samples.is_none());
    assert_eq!(s.plot.surface.source, "x+y");
}

#[test]
fn scalar_noops_keep_generation_clean_and_examples_advance_epoch() {
    let mut s = State::new(false);
    s.set_plot_kind(PlotKind::Curve2D);
    s.snapshot_if_dirty();
    let generation = s.plot.generation;
    let epoch = s.plot.transition_epoch;
    assert!(!s.set_plot_kind(PlotKind::Curve2D));
    assert!(!s.set_plot_samples(s.plot.settings.curve_samples));
    assert!(!s.set_plot_resolution(s.plot.settings.surface_resolution));
    assert!(!s.set_plot_width(s.plot.settings.line_width));
    assert!(!s.set_plot_auto_height(s.plot.settings.auto_height));
    assert!(!s.set_plot_height_range(s.plot.settings.height_min, s.plot.settings.height_max));
    assert!(!s.reset_plot_section());
    assert!(!s.dirty);
    assert_eq!(s.plot.generation, generation);
    assert_eq!(s.plot.transition_epoch, epoch);
    s.select_plot_example(&EXAMPLES[0]);
    let epoch = s.plot.transition_epoch;
    s.select_plot_example(&EXAMPLES[0]);
    assert_eq!(s.plot.transition_epoch, epoch + 1);
}

#[test]
fn reset_surface_preserves_curve_and_vector_and_restores_settings() {
    let mut s = State::new(true);
    s.set_plot_kind(PlotKind::Curve2D);
    s.set_active_source("x+7".into());
    s.set_plot_kind(PlotKind::Surface3D);
    s.set_active_source("x-y".into());
    s.set_plot_samples(2);
    s.set_plot_resolution(2);
    s.set_plot_width(1.0);
    assert_eq!(s.plot.settings.line_width, 0.2);
    s.set_plot_auto_height(false);
    s.set_plot_height_range(-4., 4.);
    assert!(s.reset_plot_section());
    let defaults = vector_field_explorer::plot_state::PlotState::default();
    assert_eq!(s.plot.surface.source, defaults.surface.source);
    assert_eq!(s.plot.curve.source, "x+7");
    assert_eq!(s.formula, "(x, y, z)");
    assert_eq!(
        s.plot.settings.curve_samples,
        defaults.settings.curve_samples
    );
    assert_eq!(
        s.plot.settings.surface_resolution,
        defaults.settings.surface_resolution
    );
    assert_eq!(s.plot.settings.line_width, defaults.settings.line_width);
    assert_eq!(s.plot.settings.auto_height, defaults.settings.auto_height);
    assert_eq!(
        (s.plot.settings.height_min, s.plot.settings.height_max),
        (0., 1.)
    );
}

#[test]
fn curve_y_span_invalidates_samples_but_unchanged_setters_do_not() {
    let mut s = State::new(false);
    s.set_plot_kind(PlotKind::Curve2D);
    s.plot_scene();
    let first = s.plot.curve.samples.clone().unwrap();
    let epoch = s.plot.transition_epoch;
    let generation = s.plot.generation;
    s.snapshot_if_dirty();
    assert!(!s.set_plot_samples(s.plot.settings.curve_samples));
    assert!(!s.set_active_source(s.active_source().into()));
    assert!(!s.dirty);
    assert_eq!(s.plot.generation, generation);
    s.set_active_source("x".into());
    assert_eq!(s.plot.transition_epoch, epoch);
    s.plot_scene();
    let edited = s.plot.curve.samples.clone().unwrap();
    assert!(!Arc::ptr_eq(&first, &edited));
    let domain = s.domain;
    s.set_domain(domain.min - Vec3::Y, domain.max);
    s.plot_scene();
    assert!(!Arc::ptr_eq(
        &edited,
        s.plot.curve.samples.as_ref().unwrap()
    ));
}

#[test]
fn scene_bounds_and_signed_colors_ignore_holes() {
    let mesh = SurfaceMesh {
        positions: vec![
            Vec3::new(0., 0., -1.),
            Vec3::new(1., 0., 1.),
            Vec3::new(0., 1., 0.),
            Vec3::splat(99.),
        ],
        normals: vec![Vec3::Z; 4],
        indices: vec![0, 1, 2],
        ..Default::default()
    };
    let scene = build_plot_scene(
        PlotKind::Surface3D,
        Some(&SampledPlot::Surface(mesh)),
        &Domain::default(),
        &PlotSettings::default(),
        Colormap::Mono,
        false,
        false,
        7,
    );
    let bounds = scene.bounds.unwrap();
    assert_eq!(bounds.max.z, 1.);
    assert_eq!(scene.height_range, (-1., 1.));
    assert_eq!(scene.generation, 7);
    assert!(scene.references.is_empty());
    let PlotGeometry::Surface(vertices, indices) = scene.geometry else {
        panic!()
    };
    assert_eq!(&*indices, &[0, 1, 2]);
    assert_eq!(vertices[0].color, [0., 0., 0., 1.]);
    assert_eq!(vertices[1].color, [1., 1., 1., 1.]);
}

#[test]
fn curve_segments_only_and_constant_height_gray() {
    let a = Vec3::new(-1., 2., 0.);
    let b = Vec3::new(1., 2., 0.);
    let samples = SampledPlot::Curve {
        slots: vec![Some(a), Some(b), None, Some(Vec3::splat(99.))],
        edges: CurveEdges {
            segments: vec![[a, b]],
            ..Default::default()
        },
    };
    let scene = build_plot_scene(
        PlotKind::Curve2D,
        Some(&samples),
        &Domain::default(),
        &PlotSettings::default(),
        Colormap::Mono,
        false,
        true,
        3,
    );
    assert_eq!(scene.height_range, (2., 2.));
    assert_eq!(scene.bounds.unwrap().max.y, 2.);
    assert_eq!(scene.references.len(), 2);
    let PlotGeometry::Curve(lines) = scene.geometry else {
        panic!()
    };
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].color, [128. / 255., 128. / 255., 128. / 255., 1.]);
}

#[test]
fn empty_mismatched_and_invalid_samples_are_safe() {
    let invalid = SampledPlot::Surface(SurfaceMesh {
        positions: vec![Vec3::ZERO],
        normals: vec![Vec3::Z],
        indices: vec![0, 1, 2],
        ..Default::default()
    });
    for samples in [None, Some(&invalid)] {
        let scene = build_plot_scene(
            PlotKind::Surface3D,
            samples,
            &Domain::default(),
            &PlotSettings::default(),
            Colormap::Viridis,
            false,
            false,
            9,
        );
        assert!(scene.bounds.is_none());
        assert!(!scene.status.is_empty());
        let PlotGeometry::Surface(vertices, indices) = scene.geometry else {
            panic!()
        };
        assert!(vertices.is_empty() && indices.is_empty());
    }
    let scene = build_plot_scene(
        PlotKind::Curve2D,
        Some(&invalid),
        &Domain::default(),
        &PlotSettings::default(),
        Colormap::Mono,
        false,
        false,
        1,
    );
    assert!(scene.bounds.is_none());
    assert_eq!(scene.kind, PlotKind::Curve2D);
}

#[test]
fn references_follow_scalar_kind_and_visibility() {
    for (kind, grid, axes, count) in [
        (PlotKind::Curve2D, false, false, 0),
        (PlotKind::Curve2D, true, false, 22),
        (PlotKind::Curve2D, false, true, 2),
        (PlotKind::Surface3D, false, true, 3),
        (PlotKind::Surface3D, true, false, 78),
    ] {
        let scene = build_plot_scene(
            kind,
            None,
            &Domain::default(),
            &PlotSettings::default(),
            Colormap::Mono,
            grid,
            axes,
            0,
        );
        assert_eq!(scene.references.len(), count);
    }
}

#[test]
fn manual_range_clamps_signed_heights_without_changing_geometry() {
    let a = Vec3::new(0., -2., 0.);
    let b = Vec3::new(1., 0., 0.);
    let c = Vec3::new(2., 2., 0.);
    let samples = SampledPlot::Curve {
        slots: vec![Some(a), Some(b), Some(c)],
        edges: CurveEdges {
            segments: vec![[a, b], [b, c]],
            ..Default::default()
        },
    };
    let settings = PlotSettings {
        auto_height: false,
        height_min: 0.5,
        height_max: -0.5,
        ..Default::default()
    };
    let scene = build_plot_scene(
        PlotKind::Curve2D,
        Some(&samples),
        &Domain::default(),
        &settings,
        Colormap::Mono,
        false,
        false,
        0,
    );
    assert_eq!(scene.height_range, (-0.5, 0.5));
    assert_eq!(scene.bounds.unwrap().min.y, -2.);
    let PlotGeometry::Curve(lines) = scene.geometry else {
        panic!()
    };
    assert_eq!(lines[0].color, [0., 0., 0., 1.]);
    assert_eq!(lines[1].color, [1., 1., 1., 1.]);
}

#[test]
fn degenerate_surface_triangles_do_not_create_bounds() {
    let mesh = SurfaceMesh {
        positions: vec![Vec3::ZERO, Vec3::X, Vec3::X * 2.0],
        normals: vec![Vec3::Z; 3],
        indices: vec![0, 1, 2],
        ..Default::default()
    };
    let scene = build_plot_scene(
        PlotKind::Surface3D,
        Some(&SampledPlot::Surface(mesh)),
        &Domain::default(),
        &PlotSettings::default(),
        Colormap::Mono,
        false,
        false,
        0,
    );
    assert!(scene.bounds.is_none());
    let PlotGeometry::Surface(vertices, indices) = scene.geometry else {
        panic!()
    };
    assert!(vertices.is_empty() && indices.is_empty());
}
