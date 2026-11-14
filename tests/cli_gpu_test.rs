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
