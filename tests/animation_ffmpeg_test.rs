use std::path::Path;

use tempfile::tempdir;
#[cfg(unix)]
use vector_field_explorer::animation::FrameSink;
#[cfg(unix)]
use vector_field_explorer::animation::frame_count;
use vector_field_explorer::animation::{
    AnimationError, AnimationFormat, AnimationSpec, ffmpeg_args, video_sink_with_program,
};
use vector_field_explorer::export_provenance::ExportProvenance;

fn spec(format: AnimationFormat) -> AnimationSpec {
    AnimationSpec {
        start_time: 0.0,
        duration: 0.3,
        fps: 10,
        width: 4,
        height: 4,
        loop_gif: false,
        format,
    }
}

fn provenance() -> ExportProvenance {
    ExportProvenance {
        warning_lines: vec![
            "Sources - softened teaching model - a = 0.01".to_string(),
            "Unavailable: 2".to_string(),
        ],
        ..ExportProvenance::default()
    }
}

fn args_for(format: AnimationFormat) -> Vec<String> {
    ffmpeg_args(&spec(format), Path::new("/tmp/output.video"), &provenance())
}

fn contains(haystack: &[String], needle: &str) -> bool {
    haystack.iter().any(|argument| argument == needle)
}

#[test]
fn ffmpeg_args_select_the_right_codec_and_metadata() {
    let mp4 = args_for(AnimationFormat::Mp4);
    assert!(contains(&mp4, "libx264"));
    assert!(contains(&mp4, "yuv420p"));
    assert!(contains(&mp4, "+faststart"));
    assert!(contains(&mp4, "-vf"));
    assert!(contains(&mp4, "vflip"));
    assert!(contains(&mp4, "title=Vector Field Explorer animation"));
    assert!(contains(
        &mp4,
        "comment=Sources - softened teaching model - a = 0.01; Unavailable: 2"
    ));
    assert_eq!(mp4.last().unwrap(), "/tmp/output.video");
    assert_eq!(
        &mp4[..],
        [
            "-y",
            "-loglevel",
            "error",
            "-f",
            "rawvideo",
            "-pixel_format",
            "rgba",
            "-video_size",
            "4x4",
            "-framerate",
            "10",
            "-i",
            "pipe:0",
            "-vf",
            "vflip",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-movflags",
            "+faststart",
            "-metadata",
            "title=Vector Field Explorer animation",
            "-metadata",
            "comment=Sources - softened teaching model - a = 0.01; Unavailable: 2",
            "-f",
            "mp4",
            "/tmp/output.video",
        ]
    );

    let webm = args_for(AnimationFormat::Webm);
    assert!(contains(&webm, "libvpx-vp9"));
    assert!(!contains(&webm, "libx264"));
    assert!(contains(&webm, "yuv420p"));
    assert!(!contains(&webm, "+faststart"));
    assert_eq!(
        &webm[..],
        [
            "-y",
            "-loglevel",
            "error",
            "-f",
            "rawvideo",
            "-pixel_format",
            "rgba",
            "-video_size",
            "4x4",
            "-framerate",
            "10",
            "-i",
            "pipe:0",
            "-vf",
            "vflip",
            "-c:v",
            "libvpx-vp9",
            "-pix_fmt",
            "yuv420p",
            "-metadata",
            "title=Vector Field Explorer animation",
            "-metadata",
            "comment=Sources - softened teaching model - a = 0.01; Unavailable: 2",
            "-f",
            "webm",
            "/tmp/output.video",
        ]
    );
}

#[test]
fn ffmpeg_metadata_is_sanitized_to_one_line() {
    let provenance = ExportProvenance {
        warning_lines: vec!["line one\nline two\u{7}".to_string()],
        ..ExportProvenance::default()
    };
    let args = ffmpeg_args(
        &spec(AnimationFormat::Mp4),
        Path::new("out.mp4"),
        &provenance,
    );
    let comment = args
        .iter()
        .find(|argument| argument.starts_with("comment="))
        .expect("comment metadata");
    assert_eq!(comment, "comment=line one; line two");
    assert!(!comment.contains('\n'));
}

#[test]
fn formula_video_without_warnings_gets_no_fake_comment() {
    let args = ffmpeg_args(
        &spec(AnimationFormat::Mp4),
        Path::new("out.mp4"),
        &ExportProvenance::default(),
    );
    assert!(contains(&args, "title=Vector Field Explorer animation"));
    assert!(!args.iter().any(|argument| argument.starts_with("comment=")));
}

#[test]
fn video_rejects_gif_and_odd_or_missing_programs() {
    let dir = tempdir().unwrap();
    let output = dir.path().join("out.mp4");
    let gif = spec(AnimationFormat::Gif);
    assert!(matches!(
        video_sink_with_program(&gif, &output, &provenance(), Path::new("ffmpeg")),
        Err(AnimationError::InvalidSpec(_))
    ));

    let odd = AnimationSpec {
        width: 3,
        ..spec(AnimationFormat::Mp4)
    };
    assert!(matches!(
        video_sink_with_program(&odd, &output, &provenance(), Path::new("ffmpeg")),
        Err(AnimationError::InvalidSpec(_))
    ));

    let missing = dir.path().join("no-such-ffmpeg");
    let error = match video_sink_with_program(
        &spec(AnimationFormat::Mp4),
        &output,
        &provenance(),
        &missing,
    ) {
        Ok(_) => panic!("a missing program must not start a video sink"),
        Err(error) => error,
    };
    assert!(matches!(error, AnimationError::External(_)));
    assert!(error.to_string().contains("ffmpeg"));
    assert!(error.to_string().contains("VFE_FFMPEG"));
}

#[cfg(unix)]
mod unix_fake_ffmpeg {
    use std::fs;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use super::*;

    /// Write a fake program and fully release the write handle before any
    /// spawn can see it. `Text file busy` is the ETXTBSY fork/exec race: a
    /// concurrent test's child can inherit this process's still-open write
    /// handle and hold the file busy briefly. Syncing and dropping here
    /// removes this writer's side; [`open_video_sink`] retries the remaining
    /// cross-thread window, mirroring `cas_backend::spawn_with_retry`.
    fn script(directory: &Path, body: &str) -> PathBuf {
        let path = directory.join("fake-ffmpeg.sh");
        let mut file = fs::File::create(&path).unwrap();
        file.write_all(body.as_bytes()).unwrap();
        file.flush().unwrap();
        file.sync_all().unwrap();
        drop(file);
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).unwrap();
        path
    }

    /// Open a sink against a freshly written fake program. A concurrent
    /// test's fork can still hold the script busy for a moment, so retry the
    /// transient ETXTBSY failure with a short backoff before giving up.
    fn open_video_sink(
        spec: &AnimationSpec,
        output: &Path,
        provenance: &ExportProvenance,
        program: &Path,
    ) -> Box<dyn FrameSink> {
        for attempt in 0..5 {
            match video_sink_with_program(spec, output, provenance, program) {
                Ok(sink) => return sink,
                Err(AnimationError::External(message))
                    if message.contains("Text file busy") && attempt < 4 =>
                {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("could not open the fake ffmpeg sink: {error}"),
            }
        }
        unreachable!("the final attempt either opens the sink or panics")
    }

    fn frame(spec: &AnimationSpec) -> Vec<u8> {
        vec![128u8; (spec.width * spec.height * 4) as usize]
    }

    fn temporary_siblings(directory: &Path) -> Vec<String> {
        directory
            .read_dir()
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".tmp"))
            .collect()
    }

    #[test]
    fn fake_ffmpeg_receives_every_rgba_byte_and_output_is_renamed() {
        let dir = tempdir().unwrap();
        let captured = dir.path().join("stdin.bin");
        let output = dir.path().join("out.mp4");
        let program = script(
            dir.path(),
            &format!(
                "#!/bin/sh\ncat > '{}'\nfor last; do :; done\nprintf 'dummy-video' > \"$last\"\n",
                captured.display()
            ),
        );
        let spec = spec(AnimationFormat::Mp4);

        let mut sink = open_video_sink(&spec, &output, &provenance(), &program);
        for _ in 0..frame_count(&spec).unwrap() {
            sink.push_frame(&frame(&spec), spec.width, spec.height)
                .unwrap();
        }
        assert_eq!(
            sink.bytes_written(),
            u64::from(spec.width) * u64::from(spec.height) * 4 * 3
        );
        sink.finish().unwrap();

        assert_eq!(
            fs::read(&captured).unwrap().len(),
            (spec.width * spec.height * 4 * 3) as usize
        );
        assert_eq!(fs::read(&output).unwrap(), b"dummy-video");
        assert!(temporary_siblings(dir.path()).is_empty());
    }

    #[test]
    fn a_stalled_encoder_fails_without_blocking_the_calling_thread() {
        let dir = tempdir().unwrap();
        let output = dir.path().join("out.mp4");
        // This encoder never reads stdin, so its pipe fills.
        let program = script(dir.path(), "#!/bin/sh\nsleep 2\n");
        let spec = AnimationSpec {
            width: 256,
            height: 256,
            duration: 1.0,
            fps: 10,
            ..spec(AnimationFormat::Mp4)
        };

        let mut sink = open_video_sink(&spec, &output, &provenance(), &program);
        let frame = frame(&spec);
        let started = std::time::Instant::now();
        let mut failure = None;
        for _ in 0..frame_count(&spec).unwrap() {
            if let Err(error) = sink.push_frame(&frame, spec.width, spec.height) {
                failure = Some(error);
                break;
            }
        }
        let elapsed = started.elapsed();
        let failure = failure.expect("a stalled encoder must fail visibly, not accept frames");
        assert!(
            matches!(
                failure,
                AnimationError::External(_) | AnimationError::Encoder(_) | AnimationError::Io(_)
            ),
            "the failure names the encoder problem: {failure:?}"
        );
        assert!(
            elapsed < Duration::from_millis(500),
            "the failure is bounded, got {elapsed:?}"
        );
        assert!(
            !output.exists(),
            "a failed export leaves no destination file"
        );
    }

    #[test]
    fn dropping_video_before_finish_preserves_the_output_and_cleans_its_stage() {
        let dir = tempdir().unwrap();
        let output = dir.path().join("out.mp4");
        fs::write(&output, b"previous video").unwrap();
        let program = script(
            dir.path(),
            "#!/bin/sh\ncat >/dev/null\nfor last; do :; done\nprintf 'unfinished job' > \"$last\"\n",
        );
        let spec = spec(AnimationFormat::Mp4);
        let mut sink = open_video_sink(&spec, &output, &provenance(), &program);
        sink.push_frame(&frame(&spec), spec.width, spec.height)
            .unwrap();
        drop(sink);

        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while !temporary_siblings(dir.path()).is_empty() {
            assert!(
                std::time::Instant::now() < deadline,
                "abandoned stage was not removed"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(fs::read(output).unwrap(), b"previous video");
    }

    #[test]
    fn normal_video_jobs_publish_decodable_mp4_and_webm_with_every_frame() {
        use std::process::Command;
        use vector_field_explorer::VfApp;
        use vector_field_explorer::animation_job::{
            AnimationJob, AnimationJobState, AnimationRenderer,
        };
        use vector_field_explorer::animation_scene::AnimationFrame;

        if std::env::var_os("VFE_ANIMATION_FFMPEG_PROBE").as_deref()
            != Some(std::ffi::OsStr::new("1"))
        {
            eprintln!("Set VFE_ANIMATION_FFMPEG_PROBE=1 to exercise installed ffmpeg and ffprobe");
            return;
        }

        struct SolidFrames;
        impl AnimationRenderer for SolidFrames {
            fn render(
                &mut self,
                _: AnimationFrame,
                width: u32,
                height: u32,
                _: [f64; 4],
            ) -> Result<Vec<u8>, AnimationError> {
                Ok(vec![100; (width * height * 4) as usize])
            }
        }

        let dir = tempdir().unwrap();
        for (format, extension) in [
            (AnimationFormat::Mp4, "mp4"),
            (AnimationFormat::Webm, "webm"),
        ] {
            let output = dir.path().join(format!("output.{extension}"));
            fs::write(&output, b"previous export").unwrap();
            let spec = AnimationSpec {
                width: 64,
                height: 64,
                ..spec(format)
            };
            let scheduled_frames = frame_count(&spec).unwrap();
            let mut app = VfApp::new();
            app.state.set_formula("sin(t), 0, 0".into());
            let sink = open_video_sink(&spec, &output, &provenance(), Path::new("ffmpeg"));
            let mut job =
                AnimationJob::new(spec, app.animation_snapshot().unwrap(), sink, None).unwrap();
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            while !job.state().is_terminal() {
                job.pump(&mut SolidFrames).unwrap();
                assert!(
                    std::time::Instant::now() < deadline,
                    "video job did not finish"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
            assert_eq!(job.state(), AnimationJobState::Done);
            assert_eq!(job.frames_done(), scheduled_frames);

            let probe = Command::new("ffprobe")
                .args([
                    "-v",
                    "error",
                    "-count_frames",
                    "-select_streams",
                    "v:0",
                    "-show_entries",
                    "stream=nb_read_frames",
                    "-of",
                    "default=nokey=1:noprint_wrappers=1",
                ])
                .arg(&output)
                .output()
                .expect("ffprobe must be installed for this probe");
            assert!(
                probe.status.success(),
                "{}",
                String::from_utf8_lossy(&probe.stderr)
            );
            let decoded = String::from_utf8(probe.stdout).unwrap();
            assert_eq!(decoded.trim().parse(), Ok(scheduled_frames));
            assert!(temporary_siblings(dir.path()).is_empty());
        }
    }

    #[test]
    fn nonzero_ffmpeg_exit_surfaces_stderr_and_removes_the_temp_file() {
        let dir = tempdir().unwrap();
        let output = dir.path().join("out.webm");
        let program = script(
            dir.path(),
            "#!/bin/sh\ncat > /dev/null\necho 'fake ffmpeg exploded' >&2\nexit 7\n",
        );
        let spec = spec(AnimationFormat::Webm);

        let mut sink = open_video_sink(&spec, &output, &provenance(), &program);
        sink.push_frame(&frame(&spec), spec.width, spec.height)
            .unwrap();
        let error = sink.finish().expect_err("nonzero exit must fail");
        assert!(matches!(error, AnimationError::External(_)));
        assert!(error.to_string().contains("fake ffmpeg exploded"));
        assert!(!output.exists());
        assert!(temporary_siblings(dir.path()).is_empty());
    }
}
