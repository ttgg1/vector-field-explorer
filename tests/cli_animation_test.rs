//! Native CLI animation configuration, validation and frame loop.
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use vector_field_explorer::animation::{
    AnimationFormat, AnimationSpec, FrameSink, frame_count, frame_times,
};
use vector_field_explorer::cli;

struct FakeSession {
    calls: Arc<AtomicUsize>,
    frames: Arc<AtomicUsize>,
}

impl cli::AnimationRenderSession for FakeSession {
    fn render_animation_frame(
        &mut self,
        _frame: &vector_field_explorer::animation_scene::AnimationFrame,
        width: u32,
        height: u32,
        _clear: [f64; 4],
    ) -> Result<Vec<u8>, cli::CliError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let index = self.frames.fetch_add(1, Ordering::SeqCst) as u8;
        Ok(vec![index; width as usize * height as usize * 4])
    }
}

struct RecordingSink {
    frames: Arc<Mutex<Vec<Vec<u8>>>>,
    finished: Arc<AtomicBool>,
}

impl FrameSink for RecordingSink {
    fn push_frame(
        &mut self,
        rgba: &[u8],
        _width: u32,
        _height: u32,
    ) -> Result<(), vector_field_explorer::animation::AnimationError> {
        self.frames.lock().unwrap().push(rgba.to_vec());
        Ok(())
    }

    fn bytes_written(&self) -> u64 {
        0
    }

    fn finish(self: Box<Self>) -> Result<(), vector_field_explorer::animation::AnimationError> {
        self.finished.store(true, Ordering::SeqCst);
        Ok(())
    }
}

fn base_flags(output: &Path) -> cli::CliFlags {
    cli::CliFlags {
        output: Some(output.to_path_buf()),
        width: Some(8),
        height: Some(8),
        density: Some(4),
        formula: Some("(1,0)".into()),
        references_grid_and_box: Some(false),
        references_world_axes: Some(false),
        ..Default::default()
    }
}

fn animation_options(
    output: &Path,
    format: &str,
    duration: f64,
    fps: u32,
    loop_gif: bool,
) -> cli::CliOptions {
    let mut flags = base_flags(output);
    flags.animation_format = Some(format.into());
    flags.animation_fps = Some(fps);
    flags.animation_loop = loop_gif;
    flags.duration = Some(duration);
    cli::resolve_config(None, flags).unwrap()
}

fn temp_root(label: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("vfe-cli-animation-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

#[test]
fn animation_flags_round_trip_config() {
    let flags = cli::parse_flags(
        [
            "vfe",
            "--animation-format",
            "gif",
            "--animation-fps",
            "15",
            "--animation-loop",
            "-o",
            "plot.gif",
        ]
        .map(OsString::from),
    )
    .unwrap();
    let options = cli::resolve_config(None, flags).unwrap();
    assert_eq!(options.animation_format, Some(AnimationFormat::Gif));
    assert_eq!(options.animation_fps, 15);
    assert!(options.animation_loop);
    assert_eq!(options.duration_seconds, 4.0, "animation duration default");

    let config = cli::parse_config(
        r#"{"output":"plot.webm","animation_format":"webm","animation_fps":24,"animation_loop":false,"duration_seconds":2.5}"#,
    )
    .unwrap();
    let options = cli::resolve_config(Some(config), cli::CliFlags::default()).unwrap();
    assert_eq!(options.animation_format, Some(AnimationFormat::Webm));
    assert_eq!(options.animation_fps, 24);
    assert!(!options.animation_loop);
    assert_eq!(options.duration_seconds, 2.5);

    let error = cli::parse_config(r#"{"animation_format":"avi"}"#)
        .and_then(|config| cli::resolve_config(Some(config), cli::CliFlags::default()))
        .unwrap_err();
    assert!(matches!(error, cli::CliError::Validation(_)));
    assert!(error.to_string().contains("gif, mp4, webm"));
}

#[test]
fn animation_requires_matching_extension_and_even_video_dimensions() {
    let root = temp_root("extension");
    let mut mismatch = base_flags(&root.join("plot.gif"));
    mismatch.animation_format = Some("mp4".into());
    mismatch.duration = Some(0.5);
    let error = cli::resolve_config(None, mismatch).unwrap_err();
    assert!(matches!(error, cli::CliError::Validation(_)));
    assert!(error.to_string().contains("expected .mp4"), "{error}");

    for (format, extension) in [("gif", "GIF"), ("mp4", "MP4"), ("webm", "WebM")] {
        let path = root.join(format!("plot.{extension}"));
        let options = animation_options(&path, format, 0.5, 12, false);
        assert_eq!(options.output, path);
    }

    let odd = animation_options(&root.join("odd.mp4"), "mp4", 0.5, 12, false);
    let mut odd = odd;
    odd.width = 9;
    let error = cli::validate_effective(&odd).unwrap_err();
    assert!(
        error.to_string().contains("even width and height"),
        "{error}"
    );

    let mut even = animation_options(&root.join("even.mp4"), "mp4", 0.5, 12, false);
    even.width = 10;
    even.height = 8;
    cli::validate_effective(&even).unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn animation_loop_is_gif_only() {
    let root = temp_root("loop");
    animation_options(&root.join("loop.gif"), "gif", 0.5, 12, true);

    for (format, extension) in [("mp4", "mp4"), ("webm", "webm")] {
        let error = cli::resolve_config(
            None,
            cli::CliFlags {
                animation_format: Some(format.into()),
                animation_loop: true,
                duration: Some(0.5),
                ..base_flags(&root.join(format!("loop.{extension}")))
            },
        )
        .unwrap_err();
        assert!(matches!(error, cli::CliError::Validation(_)));
        assert!(error.to_string().contains("animation_loop"), "{error}");
        assert!(error.to_string().contains("GIF"), "{error}");
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn still_export_ignores_animation_fields() {
    let plain_flags = cli::CliFlags {
        output: Some("still.svg".into()),
        width: Some(40),
        height: Some(30),
        density: Some(4),
        duration: Some(0.5),
        particles: Some(false),
        references_grid_and_box: Some(false),
        references_world_axes: Some(false),
        ..Default::default()
    };
    let plain = cli::resolve_config(None, plain_flags.clone()).unwrap();
    let config =
        cli::parse_config(r#"{"animation_fps":25,"animation_loop":true,"duration_seconds":0.5}"#)
            .unwrap();
    let with_fields = cli::resolve_config(Some(config), plain_flags).unwrap();
    assert_eq!(with_fields.animation_format, None);
    assert_eq!(with_fields.duration_seconds, 0.5);
    assert_eq!(with_fields.animation_fps, 25);

    let plain_scene = cli::prepare_scene(&plain, cli::output_frame(&plain).unwrap()).unwrap();
    let ignored_scene =
        cli::prepare_scene(&with_fields, cli::output_frame(&with_fields).unwrap()).unwrap();
    assert_eq!(
        cli::scene_to_cli_svg(&plain_scene).unwrap(),
        cli::scene_to_cli_svg(&ignored_scene).unwrap()
    );

    let only_fps = cli::resolve_config(
        Some(cli::parse_config(r#"{"animation_fps":25}"#).unwrap()),
        cli::CliFlags {
            output: Some("still.svg".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        only_fps.duration_seconds, 0.0,
        "still duration keeps its zero default"
    );
}

#[test]
fn animation_uses_one_headless_session() {
    let root = temp_root("one-session");
    let output = root.join("one.gif");
    let options = animation_options(&output, "gif", 0.5, 4, false);
    let opened = Arc::new(AtomicUsize::new(0));
    let frames = Arc::new(AtomicUsize::new(0));
    let result = cli::run_animation_export_with(&options, {
        let opened = Arc::clone(&opened);
        let frames = Arc::clone(&frames);
        move || {
            opened.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(FakeSession {
                calls: Arc::new(AtomicUsize::new(0)),
                frames,
            }) as Box<dyn cli::AnimationRenderSession>)
        }
    });
    result.unwrap();
    assert_eq!(opened.load(Ordering::SeqCst), 1, "one session per export");
    assert_eq!(
        frames.load(Ordering::SeqCst),
        frame_count(&options.animation_spec().unwrap().unwrap()).unwrap()
    );
    assert!(output.is_file(), "GIF output was not written");
    assert!(
        !root.join(".one.gif.tmp").exists(),
        "temporary file left behind"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn cli_animation_formula_and_particles_frame_count() {
    let root = temp_root("frames");
    let mut options = animation_options(&root.join("frames.gif"), "gif", 0.5, 6, false);
    options.mode = cli::Mode::ThreeD;
    options.formula = "(sin(t),0,0)".into();
    options.particles = true;
    options.start_time_seconds = 0.25;
    options.fixed_step_seconds = 0.05;
    options.width = 16;
    options.height = 16;

    let spec: AnimationSpec = options.animation_spec().unwrap().unwrap();
    assert_eq!(spec.start_time, 0.25);
    assert_eq!(frame_count(&spec).unwrap(), 3);
    assert_eq!(
        frame_times(&spec).unwrap(),
        vec![0.25, 0.25 + 1.0 / 6.0, 0.25 + 2.0 / 6.0]
    );

    let before = cli::prepare_animation_snapshot(&options).unwrap();
    assert!(
        !before.particles.pos.is_empty(),
        "particles were not spawned"
    );
    let before_positions = before.particles.pos.clone();

    let mut snapshot = before;
    let frames = Arc::new(Mutex::new(Vec::new()));
    let finished = Arc::new(AtomicBool::new(false));
    let sink = RecordingSink {
        frames: Arc::clone(&frames),
        finished: Arc::clone(&finished),
    };
    let mut session = FakeSession {
        calls: Arc::new(AtomicUsize::new(0)),
        frames: Arc::new(AtomicUsize::new(0)),
    };
    cli::render_animation_frames(
        &spec,
        &mut snapshot,
        &mut session,
        Box::new(sink),
        &vector_field_explorer::export_provenance::ExportProvenance::default(),
    )
    .unwrap();

    let frames = frames.lock().unwrap();
    assert_eq!(frames.len(), 3);
    assert!(finished.load(Ordering::SeqCst));
    assert_eq!(frames[0][0], 0, "first fake frame");
    assert_eq!(frames[2][0], 2, "third fake frame");
    assert_ne!(
        snapshot.particles.pos, before_positions,
        "particles must advance between frames"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn invalid_animation_leaves_no_output_and_reports_the_error() {
    let root = temp_root("invalid");
    for (label, mutate, expected) in [
        (
            "fps",
            Box::new(|options: &mut cli::CliOptions| options.animation_fps = 0)
                as Box<dyn Fn(&mut cli::CliOptions)>,
            "fps 0 is outside 1..=30",
        ),
        (
            "duration",
            Box::new(|options: &mut cli::CliOptions| options.duration_seconds = 0.01),
            "duration 0.01 s is outside 0.1..=60 s",
        ),
        (
            "nonfinite",
            Box::new(|options: &mut cli::CliOptions| options.start_time_seconds = f64::NAN),
            "start_time_seconds must be finite",
        ),
        (
            "pixels",
            Box::new(|options: &mut cli::CliOptions| {
                options.width = 4096;
                options.height = 2049;
            }),
            "pixels; the budget is 8388608",
        ),
    ] {
        let output = root.join(format!("{label}.gif"));
        let mut options = animation_options(&output, "gif", 0.5, 12, false);
        mutate(&mut options);
        let error = cli::run_export(options).unwrap_err();
        assert!(
            matches!(error, cli::CliError::Validation(_)),
            "{label}: {error:?}"
        );
        assert!(error.to_string().contains(expected), "{label}: {error}");
        assert!(!output.exists(), "{label}: output must not exist");
        assert!(
            !root.join(format!(".{label}.gif.tmp")).exists(),
            "{label}: temporary file must not exist"
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn missing_video_encoder_is_an_actionable_external_error() {
    let root = temp_root("missing-ffmpeg");
    let output = root.join("video.mp4");
    let options = animation_options(&output, "mp4", 0.5, 12, false);
    let spec = options.animation_spec().unwrap().unwrap();
    let program = root.join("does-not-exist-ffmpeg");
    let error = match cli::animation_sink_with_program(&spec, &output, &program) {
        Ok(_) => panic!("missing encoder must fail"),
        Err(error) => error,
    };
    assert!(matches!(error, cli::CliError::External(_)), "{error:?}");
    assert!(error.to_string().contains("Install ffmpeg"));
    assert!(error.to_string().contains("VFE_FFMPEG"));
    assert!(!output.exists());
    assert!(!root.join(".video.mp4.tmp").exists());
    std::fs::remove_dir_all(root).unwrap();
}

/// Real displayless adapter smoke; invoked only when `VFE_GPU_RUNTIME` is set.
#[test]
fn native_animation_gif_smoke() {
    if std::env::var_os("VFE_GPU_RUNTIME").is_none() {
        return;
    }
    let root = temp_root("gpu-smoke");
    let output = root.join("smoke.gif");
    let options = animation_options(&output, "gif", 0.3, 10, true);
    cli::run_export(options).unwrap();
    let bytes = std::fs::read(&output).unwrap();
    assert!(bytes.starts_with(b"GIF89a") || bytes.starts_with(b"GIF87a"));
    let mut decoder = gif::DecodeOptions::new();
    decoder.set_color_output(gif::ColorOutput::RGBA);
    let mut decoder = decoder
        .read_info(std::io::Cursor::new(bytes))
        .expect("decoded smoke GIF");
    assert_eq!((decoder.width(), decoder.height()), (8, 8));
    let mut frames = 0;
    while decoder.read_next_frame().unwrap().is_some() {
        frames += 1;
    }
    assert_eq!(frames, 3);
    std::fs::remove_dir_all(root).unwrap();
}
