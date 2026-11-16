//! Pure configuration/identity evidence for the depth-equipped plot renderer.
//!
//! These tests never acquire a wgpu device: they exercise the production
//! extent, draw-plan, pipeline-policy, prepared-identity and capture-preflight
//! seams that the real pipeline creation, callback paint and capture consume.

use eframe::egui::{PaintCallbackInfo, Rect, pos2, vec2};
use eframe::wgpu;
use vector_field_explorer::plot::PlotKind;
use vector_field_explorer::plot_renderer::capture::{CaptureLimits, validate_capture_extent};
use vector_field_explorer::plot_renderer::{
    PlotDraw, PlotFormatCapabilities, PlotPipeline, PlotTargetKey, plot_draw_order,
    plot_pipeline_policy, plot_target_extent, prepared_matches,
};

fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect::from_min_size(pos2(x, y), vec2(width, height))
}

fn key(extent: [u32; 2]) -> PlotTargetKey {
    PlotTargetKey {
        extent,
        format: wgpu::TextureFormat::Bgra8UnormSrgb,
        composite_samples: 1,
    }
}

fn features(
    allowed_usages: wgpu::TextureUsages,
    flags: wgpu::TextureFormatFeatureFlags,
) -> wgpu::TextureFormatFeatures {
    wgpu::TextureFormatFeatures {
        allowed_usages,
        flags,
    }
}

#[test]
fn plot_extent_uses_viewport_rounding_and_proportional_caps() {
    // Nonzero origins, fractional DPI and a clipped rect must match egui's own
    // rounded/clamped viewport pixels exactly; only then can the target and the
    // composite viewport agree.
    let fixtures = [
        (rect(37.25, 18.75, 412.5, 233.25), 1.0, [800, 600]),
        (rect(10.3, 4.7, 300.6, 200.2), 1.5, [900, 700]),
        (rect(-20.0, -10.0, 400.0, 300.0), 2.0, [500, 400]),
        (rect(2.5, 3.5, 100.0, 50.0), 0.618, [640, 480]),
    ];
    for (viewport, pixels_per_point, screen_size) in fixtures {
        let info = PaintCallbackInfo {
            viewport,
            clip_rect: viewport,
            pixels_per_point,
            screen_size_px: screen_size,
        };
        let pixels = info.viewport_in_pixels();
        let expected = [pixels.width_px as u32, pixels.height_px as u32];
        assert_eq!(
            plot_target_extent(viewport, pixels_per_point, screen_size, 4096),
            Some(expected),
            "{viewport:?} at {pixels_per_point} ppp"
        );
    }

    // A zero-area or fully clipped viewport has no target at all.
    for viewport in [
        rect(1.0, 1.0, 0.0, 10.0),
        rect(1.0, 1.0, 10.0, 0.0),
        rect(0.1, 0.1, 0.2, 0.2),
        rect(50.0, 50.0, 20.0, 20.0),
    ] {
        assert_eq!(plot_target_extent(viewport, 1.0, [10, 10], 4096), None);
    }

    // Device capping scales both axes by the same factor, never stretching one.
    let capped = plot_target_extent(rect(0.0, 0.0, 2000.0, 1000.0), 1.0, [2000, 1000], 800);
    assert_eq!(capped, Some([800, 400]));
    let capped = plot_target_extent(rect(0.0, 0.0, 1000.0, 2000.0), 1.0, [1000, 2000], 500);
    assert_eq!(capped, Some([250, 500]));
    // An already-small target is left alone by the cap.
    let unchanged = plot_target_extent(rect(0.0, 0.0, 320.0, 200.0), 1.0, [320, 200], 4096);
    assert_eq!(unchanged, Some([320, 200]));
}

#[test]
fn plot_draw_policy_and_preparation_identity() {
    // M2: the vector kind has no scalar plot draw plan and cannot fall back to
    // vector geometry through the plot pass.
    assert!(plot_draw_order(PlotKind::VectorField).is_empty());
    assert_eq!(
        plot_draw_order(PlotKind::Surface3D),
        &[PlotDraw::Mesh, PlotDraw::References]
    );
    assert_eq!(
        plot_draw_order(PlotKind::Curve2D),
        &[PlotDraw::References, PlotDraw::Curve]
    );

    // The production policy drives actual pipeline creation: mesh is CCW/back
    // culled opaque with depth testing and writes.
    let mesh = plot_pipeline_policy(PlotPipeline::Mesh);
    assert_eq!(mesh.depth, Some((wgpu::CompareFunction::LessEqual, true)));
    assert_eq!(mesh.cull, Some(wgpu::Face::Back));
    assert_eq!(mesh.front_face, wgpu::FrontFace::Ccw);
    assert_eq!(mesh.blend, None);

    // References test depth but never write it, and keep the existing line blend.
    let references = plot_pipeline_policy(PlotPipeline::References);
    assert_eq!(
        references.depth,
        Some((wgpu::CompareFunction::LessEqual, false))
    );
    assert_eq!(references.cull, None);
    assert_eq!(references.front_face, wgpu::FrontFace::Ccw);
    assert_eq!(
        references.blend,
        Some(vector_field_explorer::renderer::LINE_BLEND)
    );

    // The curve tests and writes depth so it overlays its same-plane reference.
    let curve = plot_pipeline_policy(PlotPipeline::Curve);
    assert_eq!(curve.depth, Some((wgpu::CompareFunction::LessEqual, true)));
    assert_eq!(curve.cull, None);
    assert_eq!(
        curve.blend,
        Some(vector_field_explorer::renderer::LINE_BLEND)
    );

    // Composite has no depth state, replaces (no blend) and takes the incoming
    // target's sample count rather than the single-sample plot attachments.
    let composite = plot_pipeline_policy(PlotPipeline::Composite);
    assert_eq!(composite.depth, None);
    assert_eq!(composite.cull, None);
    assert_eq!(composite.blend, None);
    assert_eq!(PlotPipeline::Composite.sample_count(4), 4);
    assert_eq!(PlotPipeline::Composite.sample_count(1), 1);
    assert_eq!(PlotPipeline::Mesh.sample_count(4), 1);
    assert_eq!(PlotPipeline::References.sample_count(4), 1);
    assert_eq!(PlotPipeline::Curve.sample_count(4), 1);

    // Paint only accepts the preparation identity it was recorded with.
    let prepared_key = key([640, 480]);
    assert!(!prepared_matches(None, 7, prepared_key));
    assert!(prepared_matches(Some((7, prepared_key)), 7, prepared_key));
    assert!(!prepared_matches(Some((7, prepared_key)), 8, prepared_key));
    assert!(!prepared_matches(
        Some((7, prepared_key)),
        7,
        key([320, 240])
    ));
    let mut other_format = prepared_key;
    other_format.format = wgpu::TextureFormat::Rgba8Unorm;
    assert!(!prepared_matches(Some((7, prepared_key)), 7, other_format));
    let mut other_samples = prepared_key;
    other_samples.composite_samples = 4;
    assert!(!prepared_matches(Some((7, prepared_key)), 7, other_samples));
    // A released/resized renderer has no identity at all and so cannot paint a
    // stale generation.
    let released: Option<(u64, PlotTargetKey)> = None;
    assert!(!prepared_matches(released, 7, prepared_key));
}

#[test]
fn plot_format_and_capture_preflight_reject_unsupported() {
    let required_color = wgpu::TextureUsages::RENDER_ATTACHMENT
        | wgpu::TextureUsages::TEXTURE_BINDING
        | wgpu::TextureUsages::COPY_SRC;
    let required_depth = wgpu::TextureUsages::RENDER_ATTACHMENT;
    let multisampled = wgpu::TextureFormatFeatureFlags::FILTERABLE
        | wgpu::TextureFormatFeatureFlags::MULTISAMPLE_X4;

    let supported = PlotFormatCapabilities {
        color: features(required_color, multisampled),
        depth: features(required_depth, wgpu::TextureFormatFeatureFlags::empty()),
    };
    assert!(supported.validate_plot(1).is_ok());
    assert!(supported.validate_plot(4).is_ok());
    assert!(supported.validate_plot(8).unwrap_err().contains("sample"));

    // Missing color or depth usages, and a non-filterable sampled color target,
    // must fail at installation rather than panic on first draw.
    let missing_copy = PlotFormatCapabilities {
        color: features(required_color - wgpu::TextureUsages::COPY_SRC, multisampled),
        depth: features(required_depth, wgpu::TextureFormatFeatureFlags::empty()),
    };
    assert!(missing_copy.validate_plot(1).is_err());
    let missing_texture = PlotFormatCapabilities {
        color: features(
            required_color - wgpu::TextureUsages::TEXTURE_BINDING,
            multisampled,
        ),
        depth: features(required_depth, wgpu::TextureFormatFeatureFlags::empty()),
    };
    assert!(missing_texture.validate_plot(1).is_err());
    let not_filterable = PlotFormatCapabilities {
        color: features(
            required_color,
            wgpu::TextureFormatFeatureFlags::MULTISAMPLE_X4,
        ),
        depth: features(required_depth, wgpu::TextureFormatFeatureFlags::empty()),
    };
    assert!(not_filterable.validate_plot(1).is_err());
    let missing_depth = PlotFormatCapabilities {
        color: features(required_color, multisampled),
        depth: features(
            wgpu::TextureUsages::COPY_DST,
            wgpu::TextureFormatFeatureFlags::empty(),
        ),
    };
    assert!(missing_depth.validate_plot(1).is_err());

    // Capture does not need a sampled color target but does need render + copy.
    let capture_format = PlotFormatCapabilities {
        color: features(
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            wgpu::TextureFormatFeatureFlags::empty(),
        ),
        depth: features(required_depth, wgpu::TextureFormatFeatureFlags::empty()),
    };
    assert!(capture_format.validate_capture().is_ok());
    let capture_no_copy = PlotFormatCapabilities {
        color: features(
            wgpu::TextureUsages::RENDER_ATTACHMENT,
            wgpu::TextureFormatFeatureFlags::empty(),
        ),
        depth: features(required_depth, wgpu::TextureFormatFeatureFlags::empty()),
    };
    assert!(capture_no_copy.validate_capture().is_err());

    let limits = CaptureLimits {
        max_texture_dimension_2d: 4096,
        max_buffer_size: 32 << 20,
    };
    let format = wgpu::TextureFormat::Rgba8Unorm;
    assert!(validate_capture_extent(format, [640, 480], limits).is_ok());
    // Invalid dimensions are rejected before any texture/allocation.
    assert!(validate_capture_extent(format, [0, 480], limits).is_err());
    assert!(validate_capture_extent(format, [640, 0], limits).is_err());
    assert!(validate_capture_extent(format, [5000, 480], limits).is_err());
    // A supported format with a readback larger than the device limit errors.
    assert!(validate_capture_extent(format, [4096, 4096], limits).is_err());
    // Only formats the PNG path understands are accepted.
    assert!(validate_capture_extent(wgpu::TextureFormat::Rgba16Float, [64, 64], limits).is_err());
    // Arithmetic overflow in the aligned row stride is an error, not a wrap.
    let unbounded = CaptureLimits {
        max_texture_dimension_2d: u32::MAX,
        max_buffer_size: u64::MAX,
    };
    assert!(validate_capture_extent(format, [u32::MAX, 1], unbounded).is_err());
}
