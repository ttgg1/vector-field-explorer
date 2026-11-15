use glam::Vec3;
use vector_field_explorer::{
    Colormap, Domain,
    plot::{CurveEdges, PlotKind, SurfaceMesh},
    plot_scene::{PlotGeometry, build_plot_scene},
    plot_state::{PlotSettings, SampledPlot},
};

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
