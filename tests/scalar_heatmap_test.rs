//! Issue #92 slice B2: offscreen scalar heatmap data, upload seam and ordering.
//!
//! Pure CPU tests: no adapter/device. The adapter-gated render probe lives in
//! `tests/heatmap_gpu_test.rs` and skips with a clear message when no device is
//! available.

use std::sync::Arc;

use glam::{UVec3, Vec3};
use vector_field_explorer::{
    animation_scene::AnimationSnapshot,
    camera::OrbitCamera,
    color_mapping::{ColorMapping, ColorPalette, ColorQuantity},
    color_ramp::ScaleMode,
    colormap::Colormap,
    domain::Domain,
    particles::Particles,
    persistence::ViewRecord,
    plane::{PlaneAxis, PlaneFrame, PlaneView},
    renderer::{
        DrawPass, HeatmapGPU, HeatmapVertex, LineInstanceGPU, MAX_HEATMAP_SIDE, SceneGPU,
        heatmap_legend, heatmap_upload_required,
    },
    scalar::ScalarContext,
    scalar_view::{ScalarGrid, sample_grid, scalar_view_expr},
    screenshot::scene_to_svg,
    source_scene::FieldMode,
    ui::sidebar::Controls,
    viewmodel::State,
};

fn expr(source: &str) -> vector_field_explorer::scalar::ScalarExpr {
    scalar_view_expr(
        source,
        &vector_field_explorer::parameters::ParameterSet::default(),
    )
    .expect("valid test formula")
}

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

fn mapping(palette: Colormap, manual_range: Option<(f32, f32)>) -> ColorMapping {
    ColorMapping {
        palette: ColorPalette::Builtin(palette),
        mode: ScaleMode::Linear,
        quantity: ColorQuantity::Magnitude,
        manual_range,
    }
}

fn grid(source: &str, side: u32) -> ScalarGrid {
    sample_grid(&expr(source), xy_frame(), side, side, 0.0).expect("valid grid")
}

#[test]
fn heatmap_quad_covers_the_sampled_plane_extent() {
    // Both plane axes: the quad must follow the frame basis, not hardcoded XY.
    for axis in [PlaneAxis::X, PlaneAxis::Y, PlaneAxis::Z] {
        let frame = PlaneFrame::new(
            &Domain::new(Vec3::new(-2.0, -1.0, -3.0), Vec3::new(2.0, 1.0, 3.0)),
            PlaneView {
                enabled: true,
                axis,
                index: 1,
            },
            UVec3::splat(4),
        );
        let grid = sample_grid(&expr("x + y"), frame, 9, 9, 0.0).expect("valid grid");
        let heatmap =
            HeatmapGPU::from_grid(&grid, &mapping(Colormap::Viridis, None)).expect("finite grid");
        assert_eq!(heatmap.vertices.len(), 6);
        assert_eq!(heatmap.size, [9, 9]);

        let expected = [
            frame.to_world(Vec3::new(-frame.half_u, -frame.half_v, 0.0)),
            frame.to_world(Vec3::new(frame.half_u, -frame.half_v, 0.0)),
            frame.to_world(Vec3::new(frame.half_u, frame.half_v, 0.0)),
            frame.to_world(Vec3::new(-frame.half_u, frame.half_v, 0.0)),
        ];
        let corners: Vec<Vec3> = heatmap
            .vertices
            .iter()
            .map(|vertex: &HeatmapVertex| Vec3::from_array(vertex.position))
            .collect();
        for corner in expected {
            assert!(
                corners
                    .iter()
                    .any(|candidate| corner.abs_diff_eq(*candidate, 1e-5)),
                "quad must touch {corner:?}; got {corners:?}"
            );
        }

        // Texture row 0 is the grid's first (world-bottom) row: the bottom
        // corners sample v = 0 and the top corners v = height / MAX.
        let uv_at = |target: Vec3| {
            heatmap
                .vertices
                .iter()
                .find(|vertex| Vec3::from_array(vertex.position).abs_diff_eq(target, 1e-5))
                .expect("corner vertex")
                .uv
        };
        assert_eq!(
            uv_at(expected[0])[1],
            0.0,
            "bottom row maps to texture row 0"
        );
        assert!(
            (uv_at(expected[3])[1] - 9.0 / MAX_HEATMAP_SIDE as f32).abs() < 1e-6,
            "top row maps to the last uploaded row"
        );
    }
}

#[test]
fn heatmap_colours_follow_the_active_palette_and_manual_range() {
    let grid = grid("x", 8);
    let active = mapping(Colormap::Viridis, Some((-1.0, 1.0)));
    let heatmap = HeatmapGPU::from_grid(&grid, &active).expect("finite grid");

    // Cell (0, 0) averages the first two x samples: (-1 + -5/7) / 2 = -6/7,
    // so t = (value + 1) / 2 = 1/14 under the manual range.
    let expected = active.palette.sample_unit(1.0 / 14.0);
    assert_eq!(&heatmap.pixels[0..3], &expected);
    assert_eq!(heatmap.pixels[3], 255, "painted texels are opaque");

    // The next cell averages (-5/7 + -3/7) / 2 = -4/7, so t = 3/14.
    let expected_high = active.palette.sample_unit(3.0 / 14.0);
    let offset = 4;
    assert_eq!(&heatmap.pixels[offset..offset + 3], &expected_high);

    // A manual range is honoured even with auto-fit disabled.
    let wider = HeatmapGPU::from_grid(&grid, &mapping(Colormap::Viridis, Some((-2.0, 2.0))))
        .expect("finite grid");
    assert_ne!(&wider.pixels[0..3], &expected);

    // Pin a literal palette byte for the same known cell under Mono at
    // t = 1/14: round(255 / 14) = 18 in every channel. The other expectations
    // may route through `sample_unit`; this one cannot drift with it.
    let mono = HeatmapGPU::from_grid(&grid, &mapping(Colormap::Mono, Some((-1.0, 1.0))))
        .expect("finite grid");
    assert_eq!(&mono.pixels[0..3], &[18, 18, 18]);
}

#[test]
fn heatmaps_colour_by_magnitude_even_when_arrows_use_direction() {
    let grid = grid("x", 8);
    let magnitude = mapping(Colormap::Viridis, None);
    let direction = ColorMapping {
        quantity: ColorQuantity::Direction2d,
        ..magnitude.clone()
    };

    let magnitude_heatmap =
        HeatmapGPU::from_grid(&grid, &magnitude).expect("finite magnitude heatmap");
    let direction_heatmap =
        HeatmapGPU::from_grid(&grid, &direction).expect("finite direction heatmap");

    // A filled cell holds one scalar, not an angle: the heatmap must paint the
    // magnitude ramp while the arrow mapping keeps `Direction2d` untouched.
    assert_eq!(direction_heatmap.pixels, magnitude_heatmap.pixels);
    let magnitude_legend = heatmap_legend(&grid, &magnitude).expect("finite legend");
    let direction_legend = heatmap_legend(&grid, &direction).expect("finite legend");
    assert_eq!(direction_legend, magnitude_legend);
}

#[test]
fn clipped_values_use_the_endpoint_colour_and_report_clipping() {
    let grid = grid("x", 8);
    let mapping = mapping(Colormap::Mono, Some((0.0, 1.0)));
    let heatmap = HeatmapGPU::from_grid(&grid, &mapping).expect("finite grid");

    // Cell (0, 0) averages -6/7: below the manual minimum, so the low endpoint.
    assert_eq!(&heatmap.pixels[0..4], &[0, 0, 0, 255]);
    let legend = heatmap_legend(&grid, &mapping).expect("finite legend");
    assert_eq!((legend.min, legend.max), (0.0, 1.0));
    assert!(legend.clipped_low > 0, "low clipping must be reported");
    assert_eq!(legend.clipped_high, 0);
}

#[test]
fn heatmap_is_absent_when_the_scalar_view_is_unsupported_or_hidden() {
    let mut state = State::new(false);
    assert_eq!(state.field_mode, FieldMode::Formula);

    state.set_scalar_source(ScalarContext::Surface, "z".into());
    state.set_show_scalar_heatmap(true);
    assert!(
        state.scalar_heatmap().is_none(),
        "an unsupported formula has no heatmap"
    );

    state.set_scalar_source(ScalarContext::Surface, "x".into());
    let _ = state.refresh_scalar_view(true);
    assert!(state.scalar_heatmap().is_some());
    state.set_show_scalar_heatmap(false);
    assert!(
        state.scalar_heatmap().is_none(),
        "hiding the heatmap drops it from the scene"
    );
}

#[test]
fn heatmap_toggle_round_trips_and_legacy_records_default_hidden() {
    let mut state = State::new(false);
    assert!(!state.show_scalar_heatmap());
    assert!(!ViewRecord::capture(&state).show_scalar_heatmap);

    state.set_show_scalar_heatmap(true);
    let record = ViewRecord::capture(&state);
    assert!(record.show_scalar_heatmap);

    let mut restored = State::new(false);
    record.restore(&mut restored, &mut Controls::new());
    assert!(restored.show_scalar_heatmap());

    // An older envelope/scene has no key; the container default is hidden.
    let legacy: ViewRecord = serde_json::from_str("{}").expect("legacy defaults");
    assert!(!legacy.show_scalar_heatmap);
}

#[test]
fn heatmap_upload_is_skipped_for_an_unchanged_identity() {
    assert!(!heatmap_upload_required(Some(7), Some(7)));
    assert!(heatmap_upload_required(Some(7), Some(8)));
    assert!(heatmap_upload_required(None, Some(1)));
    assert!(!heatmap_upload_required(Some(7), None));
    assert!(!heatmap_upload_required(None, None));
}

#[test]
fn svg_export_excludes_the_heatmap_but_keeps_lines() {
    let line = LineInstanceGPU {
        a: [-0.5, 0.0, 0.0, 1.0],
        b: [0.5, 0.0, 0.0, 0.05],
        color: [0.9, 0.2, 0.1, 1.0],
    };
    let heatmap = Arc::new(
        HeatmapGPU::from_grid(&grid("x", 8), &mapping(Colormap::Turbo, None)).expect("finite grid"),
    );
    let mut scene = SceneGPU {
        mode_3d: false,
        lines: Arc::new(vec![line]),
        ..Default::default()
    };
    let without = scene_to_svg(&scene, glam::Mat4::IDENTITY, 64, 64, [0.0, 0.0, 0.0, 1.0]);
    scene.heatmap = Some(heatmap);
    let with = scene_to_svg(&scene, glam::Mat4::IDENTITY, 64, 64, [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(with, without, "SVG ignores the heatmap fill");
    assert!(with.contains("<line"), "the line blueprint survives");
    assert!(!with.contains("heatmap"), "no heatmap element is emitted");
}

#[test]
fn scene_without_heatmap_is_byte_identical() {
    let scene = SceneGPU {
        mode_3d: false,
        lines: Arc::new(vec![LineInstanceGPU {
            a: [-0.5, 0.0, 0.0, 1.0],
            b: [0.5, 0.0, 0.0, 0.05],
            color: [0.9, 0.2, 0.1, 1.0],
        }]),
        ..Default::default()
    };
    assert!(scene.heatmap.is_none());
    assert_eq!(
        scene.draw_passes(),
        vec![DrawPass::Lines { instances: 1 }],
        "default scenes keep the historical draw plan"
    );
    let baseline = scene_to_svg(&scene, glam::Mat4::IDENTITY, 32, 32, [0.0; 4]);
    let mut copy = scene.clone();
    copy.heatmap = Some(Arc::new(
        HeatmapGPU::from_grid(&grid("x", 8), &mapping(Colormap::Mono, None)).expect("finite grid"),
    ));
    assert_eq!(
        scene_to_svg(&copy, glam::Mat4::IDENTITY, 32, 32, [0.0; 4]),
        baseline
    );
}

#[test]
fn heatmap_texture_stays_within_the_texel_budget() {
    let grid = grid("x", MAX_HEATMAP_SIDE);
    let heatmap =
        HeatmapGPU::from_grid(&grid, &mapping(Colormap::Mono, None)).expect("finite grid");
    assert_eq!(heatmap.size, [MAX_HEATMAP_SIDE, MAX_HEATMAP_SIDE]);
    assert_eq!(
        heatmap.pixels.len(),
        (MAX_HEATMAP_SIDE * MAX_HEATMAP_SIDE * 4) as usize
    );
    assert_eq!(heatmap.vertices.len(), 6);
    assert!(heatmap.size[0] <= MAX_HEATMAP_SIDE && heatmap.size[1] <= MAX_HEATMAP_SIDE);
}

#[test]
fn animation_snapshot_carries_the_heatmap() {
    let mut state = State::new(false);
    state.set_scalar_source(ScalarContext::Surface, "x".into());
    state.set_show_scalar_heatmap(true);
    state.set_scalar_grid_side(16).expect("valid grid side");
    assert!(state.refresh_scalar_view(true).is_some());

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
        frame.scene.heatmap.is_some(),
        "the exported frame must carry the same heatmap field"
    );
}

#[test]
fn animation_frames_keep_the_heatmap_across_the_clock() {
    let mut state = State::new(false);
    state.set_scalar_source(ScalarContext::Surface, "sin(t)*x".into());
    state.set_show_scalar_heatmap(true);
    state.set_scalar_grid_side(16).expect("valid grid side");
    assert!(state.refresh_scalar_view(true).is_some());

    let mut snapshot = AnimationSnapshot::from_parts(
        state,
        Particles::default(),
        OrbitCamera::default(),
        None,
        [0.0, 0.0, 0.0, 1.0],
    );
    // The regression reported against B2: an animated formula must refresh the
    // scalar cache for each exported clock step, not drop the heatmap after
    // the initial static frame.
    let steps = [
        (0.0, 0.0),
        (0.25, 0.25),
        (0.5, 0.25),
        (0.75, 0.25),
        (1.0, 0.25),
    ];
    for (index, (time, dt)) in steps.into_iter().enumerate() {
        let frame = snapshot
            .frame_at(time, dt, 1.0, index == 0)
            .unwrap_or_else(|error| panic!("animation frame at t={time}: {error:?}"));
        assert!(
            frame.scene.heatmap.is_some(),
            "frame at t={time} must carry the refreshed time-dependent heatmap"
        );
    }
}

#[test]
fn heatmap_draws_before_lines_and_arrows() {
    let heatmap = Arc::new(
        HeatmapGPU::from_grid(&grid("x", 8), &mapping(Colormap::Mono, None)).expect("finite grid"),
    );
    let line = LineInstanceGPU {
        a: [-0.5, 0.0, 0.0, 1.0],
        b: [0.5, 0.0, 0.0, 0.05],
        color: [1.0; 4],
    };
    let arrow = vector_field_explorer::renderer::ArrowInstanceGPU {
        origin: [0.0, 0.0, 0.0, 1.0],
        dir_len: [1.0, 0.0, 0.0, 1.0],
        color: [1.0; 4],
    };
    let scene = SceneGPU {
        mode_3d: false,
        heatmap: Some(heatmap),
        lines: Arc::new(vec![line]),
        arrows: Arc::new(vec![arrow]),
        ..Default::default()
    };
    let passes = scene.draw_passes();
    assert_eq!(passes[0], DrawPass::Heatmap { vertices: 6 });
    assert_eq!(passes[1], DrawPass::Lines { instances: 1 });
    assert!(matches!(passes[2], DrawPass::Arrow2D { .. }));
}
