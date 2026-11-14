use eframe::wgpu;
use vector_field_explorer::{cli, renderer};

fn prepared() -> cli::PreparedScene {
    let options = cli::resolve_config(
        None,
        cli::CliFlags {
            output: Some("plot.png".into()),
            density: Some(4),
            width: Some(129),
            height: Some(97),
            references_grid_and_box: Some(false),
            references_world_axes: Some(false),
            ..Default::default()
        },
    )
    .unwrap();
    cli::prepare_scene(&options, cli::output_frame(&options).unwrap()).unwrap()
}

#[test]
fn actual_map_callback_error_retains_stage_and_cause() {
    let cause = wgpu::BufferAsyncError.to_string();
    let error = renderer::readback_map_status(Err(wgpu::BufferAsyncError)).unwrap_err();
    assert!(matches!(error, renderer::ReadbackError::Map(_)));
    let error = cli::offscreen_error_to_cli(renderer::OffscreenError::Readback(error));
    assert!(matches!(error, cli::CliError::Readback(_)));
    assert!(error.to_string().contains("map"));
    assert!(error.to_string().contains(&cause));

    // This is the native viewport's string boundary, not a new GUI error type.
    let mut screenshot = vector_field_explorer::screenshot::Screenshot::new();
    screenshot.request();
    assert!(screenshot.take_request());
    screenshot.fail(error.to_string());
    assert!(screenshot.toast_text().unwrap().contains(&cause));
    screenshot.request();
    assert!(
        screenshot.take_request(),
        "failure must reopen capture slot"
    );
}

#[test]
fn native_readback_errors_keep_original_causes_and_setup_classification() {
    use std::error::Error;
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    drop(tx);
    for (error, stage) in [
        (
            renderer::ReadbackError::Poll(wgpu::PollError::Timeout),
            "poll",
        ),
        (
            renderer::ReadbackError::CallbackLost(rx.recv().unwrap_err()),
            "receive",
        ),
    ] {
        let cause = error.source().unwrap().to_string();
        let error = cli::offscreen_error_to_cli(renderer::OffscreenError::Readback(error));
        assert!(matches!(error, cli::CliError::Readback(_)));
        assert!(error.to_string().contains(stage));
        assert!(error.to_string().contains(&cause));
    }
    let error = cli::offscreen_error_to_cli(renderer::OffscreenError::Readback(
        renderer::ReadbackError::Decode("unsupported channel format".into()),
    ));
    assert!(matches!(error, cli::CliError::Readback(_)));
    assert!(error.to_string().contains("RGBA decode"));
    assert!(error.to_string().contains("unsupported channel format"));
    let error = cli::offscreen_error_to_cli(renderer::OffscreenError::Setup(
        "invalid target format".into(),
    ));
    assert!(matches!(error, cli::CliError::Render(_)));
    assert!(error.to_string().contains("setup/submission"));
    assert!(error.to_string().contains("invalid target format"));
}

#[test]
fn invalid_output_dimensions_fail_before_adapter_or_allocation() {
    for (width, height) in [
        (0, 97),
        (129, 0),
        (4097, 1),
        (1, 4097),
        (u32::MAX, u32::MAX),
    ] {
        let mut scene = prepared();
        scene.frame.width = width;
        scene.frame.height = height;
        assert!(matches!(
            cli::render_offscreen(&scene),
            Err(cli::CliError::Validation(_))
        ));
    }
}

#[test]
fn gpu_limit_validation_checks_texture_and_padded_pixel_bytes_without_gpu() {
    let scene = prepared();
    let mut limits = wgpu::Limits::default();
    cli::validate_gpu_limits(&scene, &limits).unwrap();
    limits.max_texture_dimension_2d = 128;
    let error = cli::validate_gpu_limits(&scene, &limits).unwrap_err();
    assert!(matches!(error, cli::CliError::Device(_)));
    assert!(error.to_string().contains("texture limit 128"));

    limits = wgpu::Limits::default();
    let padded_bytes = u64::from(vector_field_explorer::screenshot::padded_row_bytes(129)) * 97;
    limits.max_buffer_size = padded_bytes;
    cli::validate_gpu_limits(&scene, &limits).unwrap();
    limits.max_buffer_size -= 1;
    let error = cli::validate_gpu_limits(&scene, &limits).unwrap_err();
    assert!(matches!(error, cli::CliError::Device(_)));
    assert!(
        error
            .to_string()
            .contains(&format!("readback needs {padded_bytes} bytes"))
    );

    let mut edge = scene;
    for (width, height) in [(1, 1), (4096, 4096)] {
        edge.frame.width = width;
        edge.frame.height = height;
        cli::validate_gpu_limits(&edge, &wgpu::Limits::default()).unwrap();
    }
    for (width, height) in [(0, 1), (1, 0), (4097, 4096), (u32::MAX, u32::MAX)] {
        edge.frame.width = width;
        edge.frame.height = height;
        assert!(matches!(
            cli::validate_gpu_limits(&edge, &wgpu::Limits::default()),
            Err(cli::CliError::Validation(_))
        ));
    }
}

#[test]
fn gpu_limit_validation_checks_each_actual_storage_collection_without_gpu() {
    let arrow = renderer::ArrowInstanceGPU {
        origin: [0.0, 0.0, 0.0, renderer::ARROW_KIND],
        dir_len: [1.0, 0.0, 0.0, 1.0],
        color: [1.0; 4],
    };
    let line = renderer::LineInstanceGPU {
        a: [0.0; 4],
        b: [1.0; 4],
        color: [1.0; 4],
    };
    for collection in ["arrows", "combined", "lines", "trails", "dots"] {
        let mut scene = prepared();
        scene.scene = renderer::SceneGPU {
            mode_3d: collection == "combined",
            ..Default::default()
        };
        match collection {
            "arrows" => scene.scene.arrows = std::sync::Arc::new(vec![arrow; 2]),
            "combined" => scene.scene.combined = std::sync::Arc::new(vec![arrow; 2]),
            "lines" => scene.scene.lines = std::sync::Arc::new(vec![line; 2]),
            "trails" => scene.scene.trails = std::sync::Arc::new(vec![line; 2]),
            "dots" => scene.scene.dots = std::sync::Arc::new(vec![arrow; 2]),
            _ => unreachable!(),
        }
        let bytes = 2 * size_of::<renderer::ArrowInstanceGPU>() as u64;
        let mut limits = wgpu::Limits {
            max_storage_buffer_binding_size: bytes,
            ..Default::default()
        };
        cli::validate_gpu_limits(&scene, &limits).unwrap();
        limits.max_storage_buffer_binding_size -= 1;
        let error = cli::validate_gpu_limits(&scene, &limits).unwrap_err();
        assert!(matches!(error, cli::CliError::Device(_)));
        let label = if matches!(collection, "arrows" | "combined") {
            "arrows/combined"
        } else {
            collection
        };
        assert!(
            error
                .to_string()
                .contains(&format!("{label} storage buffer")),
            "{error}"
        );
    }

    // Distinguish the storage max-buffer bound from the earlier readback bound:
    // a 1x1 frame needs 256 readback bytes, but these arrows need 768 bytes.
    let mut scene = prepared();
    scene.frame.width = 1;
    scene.frame.height = 1;
    let bytes = (scene.scene.arrows.len() * size_of::<renderer::ArrowInstanceGPU>()) as u64;
    assert!(bytes > 256);
    let limits = wgpu::Limits {
        max_buffer_size: bytes - 1,
        ..Default::default()
    };
    let error = cli::validate_gpu_limits(&scene, &limits).unwrap_err();
    assert!(matches!(error, cli::CliError::Device(_)));
    assert!(error.to_string().contains("arrows/combined storage buffer"));
}

#[test]
fn gpu_limit_validation_checks_storage_and_uniform_binding_limits_without_gpu() {
    for limits in [
        wgpu::Limits {
            max_storage_buffers_per_shader_stage: 0,
            ..Default::default()
        },
        wgpu::Limits {
            max_uniform_buffers_per_shader_stage: 0,
            ..Default::default()
        },
        wgpu::Limits {
            max_uniform_buffer_binding_size: size_of::<renderer::Globals>() as u64 - 1,
            ..Default::default()
        },
    ] {
        let error = cli::validate_gpu_limits(&prepared(), &limits).unwrap_err();
        assert!(matches!(error, cli::CliError::Device(_)));
        assert!(error.to_string().contains("storage/uniform binding limits"));
    }
}

#[test]
fn device_acquisition_invokes_request_once_and_preserves_original_failure() {
    let calls = std::cell::Cell::new(0);
    let error = cli::acquire_device_with(|| {
        calls.set(calls.get() + 1);
        Err::<(), _>(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "driver device initialization denied",
        ))
    })
    .unwrap_err();
    assert_eq!(calls.get(), 1);
    assert!(matches!(error, cli::CliError::Device(_)));
    assert!(
        error
            .to_string()
            .contains("headless PNG device request failed")
    );
    assert!(
        error
            .to_string()
            .contains("driver device initialization denied")
    );

    let device = cli::acquire_device_with(|| {
        calls.set(calls.get() + 1);
        Ok::<_, std::io::Error>(37)
    })
    .unwrap();
    assert_eq!(device, 37);
    assert_eq!(calls.get(), 2, "success must invoke exactly once too");
}

// The outer process never mutates shared environment. The shell script selects
// this worker with display variables removed and explicitly requires GPU proof.
#[test]
fn displayless_gpu_worker() {
    if std::env::var_os("VFE_GPU_RUNTIME").is_none() {
        return;
    }
    assert!(std::env::var_os("DISPLAY").is_none());
    assert!(std::env::var_os("WAYLAND_DISPLAY").is_none());
    let _ = env_logger::builder().is_test(true).try_init();
    for mode in [cli::Mode::TwoD, cli::Mode::ThreeD] {
        let mut scene = prepared();
        if mode == cli::Mode::ThreeD {
            let options = cli::resolve_config(
                None,
                cli::CliFlags {
                    output: Some("plot.png".into()),
                    mode: Some(mode),
                    density: Some(4),
                    width: Some(129),
                    height: Some(97),
                    references_grid_and_box: Some(false),
                    references_world_axes: Some(false),
                    ..Default::default()
                },
            )
            .unwrap();
            scene = cli::prepare_scene(&options, cli::output_frame(&options).unwrap()).unwrap();
        }
        let pixels = cli::render_offscreen(&scene).expect("real displayless adapter required");
        assert_eq!(pixels.len(), 129 * 97 * 4);
        assert!(
            pixels
                .as_chunks::<4>()
                .0
                .iter()
                .any(|p| *p != scene.frame.background_srgb)
        );
        let image = image::RgbaImage::from_raw(129, 97, pixels).unwrap();
        let mut encoded = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        let decoded = image::load_from_memory(encoded.get_ref()).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (129, 97));
    }

    // Identity projection places the unlit flat arrow's shaft across the
    // centre pixel, well away from its triangle edges. No references overlap.
    let options = cli::resolve_config(
        Some(cli::parse_config(r#"{"background":[73,131,207,109]}"#).unwrap()),
        cli::CliFlags {
            output: Some("color.png".into()),
            width: Some(129),
            height: Some(97),
            ..Default::default()
        },
    )
    .unwrap();
    let mut frame = cli::output_frame(&options).unwrap();
    frame.view_proj = glam::Mat4::IDENTITY;
    let color = [0.25, 0.5, 0.75, 0.5];
    let scene = cli::PreparedScene {
        frame,
        scene: renderer::SceneGPU {
            mode_3d: false,
            arrows: std::sync::Arc::new(vec![renderer::ArrowInstanceGPU {
                origin: [-0.5, 0.0, 0.5, renderer::ARROW_KIND],
                dir_len: [1.0, 0.0, 0.0, 1.0],
                color,
            }]),
            ..Default::default()
        },
    };
    let pixels = cli::render_offscreen(&scene).unwrap();
    assert_eq!(&pixels[..4], &[73, 131, 207, 109], "sRGB clear/readback");
    // ARROW2D_BLEND: RGB SrcAlpha/OneMinusSrcAlpha, alpha
    // One/OneMinusSrcAlpha. Blending occurs in linear space, then the sRGB
    // attachment encodes RGB (but not alpha) on storage.
    assert_eq!(renderer::ARROW2D_BLEND, wgpu::BlendState::ALPHA_BLENDING);
    let alpha = f64::from(color[3]);
    let mut expected = [0; 4];
    for channel in 0..3 {
        let linear = f64::from(color[channel]) * alpha
            + scene.frame.background_linear[channel] * (1.0 - alpha);
        let srgb = if linear <= 0.0031308 {
            12.92 * linear
        } else {
            1.055 * linear.powf(1.0 / 2.4) - 0.055
        };
        expected[channel] = (srgb * 255.0).round() as u8;
    }
    expected[3] =
        ((alpha + scene.frame.background_linear[3] * (1.0 - alpha)) * 255.0).round() as u8;
    let centre = (48 * 129 + 64) * 4;
    assert_eq!(
        &pixels[centre..centre + 4],
        &expected,
        "linear alpha blend then sRGB encoding"
    );
    eprintln!(
        "GPU runtime: 2D/3D dimensions and field ink; exact clear {:?}, blended {:?}",
        &pixels[..4],
        expected
    );

    let mut edge = scene;
    edge.frame.width = 1;
    edge.frame.height = 1;
    edge.scene = renderer::SceneGPU::default();
    assert_eq!(cli::render_offscreen(&edge).unwrap(), [73, 131, 207, 109]);
}

#[test]
fn no_adapter_worker() {
    if std::env::var_os("VFE_GPU_NO_ADAPTER").is_none() {
        return;
    }
    let error = cli::render_offscreen(&prepared()).unwrap_err();
    assert!(matches!(error, cli::CliError::Adapter(_)), "{error:?}");
    assert!(
        error
            .to_string()
            .contains("no usable headless wgpu adapter")
    );
    assert!(error.to_string().contains("no CPU fallback"));
}
