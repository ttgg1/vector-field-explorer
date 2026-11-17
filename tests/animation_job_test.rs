//! GUI animation job state machine, exercised with a fake renderer and sink.
//!
//! The job owns the detached snapshot, spec, next frame index, cancellation
//! flag and state. These tests pin one-frame-at-a-time progress, every failure
//! path, cancellation, banner application and temp-cleanup semantics without a
//! GPU or an app frame.

use std::sync::Arc;

use gif::DecodeOptions;
use tempfile::tempdir;

use vector_field_explorer::VfApp;
use vector_field_explorer::animation::{AnimationError, AnimationSpec, gif_sink};
use vector_field_explorer::animation_job::{AnimationEligibility, AnimationJob, AnimationJobState};
use vector_field_explorer::animation_scene::AnimationFrame;
use vector_field_explorer::export_provenance::ExportProvenance;
use vector_field_explorer::induction::InductionScene;
use vector_field_explorer::source_scene::FieldMode;
use vector_field_explorer::viewmodel::State;

fn spec() -> AnimationSpec {
    AnimationSpec {
        duration: 0.25,
        fps: 4,
        width: 32,
        height: 24,
        ..AnimationSpec::default()
    }
}

#[test]
fn job_renders_every_frame_and_finishes() {
    let mut job = AnimationJob::fake(spec(), 2);
    assert_eq!(job.state(), AnimationJobState::Preparing);
    assert_eq!(job.frames_total(), 2);
    job.pump_for_test(usize::MAX).expect("job completes");
    assert_eq!(job.state(), AnimationJobState::Done);
    assert_eq!(job.frames_done(), 2);
    assert!(job.fake_probe().finished());
    assert!(!job.fake_probe().dropped());
}

#[test]
fn pump_renders_at_most_one_frame_per_call() {
    let mut job = AnimationJob::fake(spec(), 3);
    job.pump_for_test(1).expect("one frame");
    assert_eq!(job.frames_done(), 1);
    assert_eq!(job.state(), AnimationJobState::RenderingFrame(1));

    job.pump_for_test(1).expect("one frame");
    assert_eq!(job.frames_done(), 2);
    assert_eq!(job.state(), AnimationJobState::RenderingFrame(2));

    job.pump_for_test(1).expect("last frame");
    assert_eq!(job.frames_done(), 3);
    assert_eq!(job.state(), AnimationJobState::Encoding);

    job.pump_for_test(1).expect("sink finishes");
    assert_eq!(job.state(), AnimationJobState::Done);
}

#[test]
fn cancel_stops_without_finishing_the_sink() {
    let mut job = AnimationJob::fake(AnimationSpec::default(), 10);
    job.cancel();
    job.pump_for_test(1).expect("cancel drains");
    assert_eq!(job.state(), AnimationJobState::Cancelled);
    let probe = job.fake_probe();
    assert!(!probe.finished(), "cancel must never finish the sink");
    assert!(probe.dropped(), "cancel drops the sink for temp cleanup");
}

#[test]
fn cancel_during_encoding_stops_before_the_sink_finishes() {
    let mut job = AnimationJob::fake(spec(), 4);
    job.pump_for_test(3).expect("three frames");
    assert_eq!(job.frames_done(), 3);
    job.cancel();
    job.pump_for_test(1).expect("cancel drains");
    assert_eq!(job.state(), AnimationJobState::Cancelled);
    assert_eq!(job.frames_done(), 3);
    let probe = job.fake_probe();
    assert!(!probe.finished());
    assert!(probe.dropped());
}

#[test]
fn renderer_error_fails_the_job_and_drops_the_sink() {
    let mut job = AnimationJob::fake_with_renderer_error(spec(), 4, 1);
    let error = job
        .pump_for_test(usize::MAX)
        .expect_err("the fake renderer fails");
    assert_eq!(
        error,
        AnimationError::Encoder("fake renderer failed".into())
    );
    assert_eq!(job.state(), AnimationJobState::Failed);
    assert_eq!(job.error(), Some("fake renderer failed"));
    assert_eq!(job.frames_done(), 1);
    let probe = job.fake_probe();
    assert!(!probe.finished());
    assert!(probe.dropped());
}

#[test]
fn sink_error_fails_the_job_and_drops_the_sink() {
    let mut job = AnimationJob::fake_with_sink_error(spec(), 4, 1);
    let error = job
        .pump_for_test(usize::MAX)
        .expect_err("the fake sink fails");
    assert_eq!(error, AnimationError::Io("fake sink failed".into()));
    assert_eq!(job.state(), AnimationJobState::Failed);
    assert_eq!(job.error(), Some("fake sink failed"));
    assert_eq!(job.frames_done(), 1);
    let probe = job.fake_probe();
    assert!(!probe.finished());
    assert!(probe.dropped());
}

#[test]
fn provenance_is_baked_into_every_frame() {
    let provenance = ExportProvenance {
        warning_lines: vec!["Induction - test warning".into()],
        ..ExportProvenance::default()
    };
    let mut job = AnimationJob::fake_with_provenance(spec(), 2, provenance);
    job.pump_for_test(usize::MAX).expect("job completes");

    let frames = job.fake_probe().frames();
    assert_eq!(frames.len(), 2);
    // The fake renderer paints a uniform mid-grey frame; the banner strip is
    // always darker, so a base-coloured pixel in the bottom rows means the
    // provenance was not applied.
    let spec = spec();
    let y = spec.height - 2;
    for (index, frame) in frames.iter().enumerate() {
        let offset = ((y * spec.width + 2) * 4) as usize;
        assert_ne!(
            &frame[offset..offset + 4],
            &[40, 40, 40, 255],
            "frame {index} was pushed without its banner"
        );
    }
}

#[test]
fn formula_eligibility_names_the_missing_animation_source() {
    let mut state = State::new(false);
    state.set_formula("1, 0, 0".into());
    let eligibility = AnimationEligibility::from_state(&state);
    assert!(!eligibility.allowed);
    assert!(!eligibility.animated);
    assert_eq!(
        eligibility.reason,
        Some("Animation needs a time-dependent formula or moving particles.")
    );

    state.set_formula("sin(t), 0, 0".into());
    let eligibility = AnimationEligibility::from_state(&state);
    assert!(eligibility.allowed);
    assert!(eligibility.animated);
    assert_eq!(eligibility.reason, None);
}

#[test]
fn particle_only_formula_is_animatable() {
    let mut state = State::new(false);
    state.set_mode(true);
    state.set_formula("(x, y, z)".into());
    state.set_particles(true);
    assert!(state.particle_target().is_some());
    let eligibility = AnimationEligibility::from_state(&state);
    assert!(eligibility.allowed, "{eligibility:?}");
    assert!(eligibility.animated);
}

#[test]
fn induction_eligibility_requires_a_nonzero_amplitude_and_frequency() {
    let mut state = State::new(false);
    state.set_field_mode(FieldMode::Induction);
    let static_scene = InductionScene {
        amplitude: 0.0,
        frequency: 0.0,
        ..InductionScene::default()
    };
    assert!(state.set_induction_scene(static_scene));
    let eligibility = AnimationEligibility::from_state(&state);
    assert!(!eligibility.allowed);
    assert_eq!(
        eligibility.reason,
        Some("Set a nonzero amplitude and frequency to animate this induction case.")
    );

    assert!(state.set_induction_scene(InductionScene {
        amplitude: 1.0,
        frequency: 0.25,
        ..InductionScene::default()
    }));
    let eligibility = AnimationEligibility::from_state(&state);
    assert!(eligibility.allowed);
    assert!(eligibility.animated);
}

#[test]
fn unsupported_modes_share_the_general_reason() {
    let general =
        "Animation export supports time-dependent Formula fields, Induction and particles.";
    for mode in [FieldMode::Sources, FieldMode::Materials, FieldMode::Ode] {
        let mut state = State::new(false);
        state.set_field_mode(mode);
        let eligibility = AnimationEligibility::from_state(&state);
        assert!(!eligibility.allowed, "{mode:?} must not animate");
        assert_eq!(eligibility.reason, Some(general), "{mode:?}");
    }
}

#[test]
fn fake_probe_is_shareable() {
    let job = AnimationJob::fake(spec(), 1);
    let probe = job.fake_probe();
    assert_eq!(Arc::strong_count(&probe), 3);
    assert!(probe.frames().is_empty());
}

struct SolidRenderer;

impl vector_field_explorer::animation_job::AnimationRenderer for SolidRenderer {
    fn render(
        &mut self,
        _frame: AnimationFrame,
        width: u32,
        height: u32,
        _clear: [f64; 4],
    ) -> Result<Vec<u8>, AnimationError> {
        Ok(vec![50; width as usize * height as usize * 4])
    }
}

#[test]
fn real_snapshot_and_gif_sink_finish_through_the_job() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("animation.gif");
    let spec = AnimationSpec {
        duration: 0.5,
        fps: 4,
        width: 32,
        height: 24,
        ..AnimationSpec::default()
    };
    let mut app = VfApp::new();
    app.state.set_formula("sin(t), 0, 0".into());
    let snapshot = app.animation_snapshot().expect("snapshot captures");
    let provenance = ExportProvenance {
        warning_lines: vec!["Formula - test warning".into()],
        ..ExportProvenance::default()
    };
    let sink = gif_sink(&spec, &path).expect("GIF sink opens");
    let mut job =
        AnimationJob::new(spec.clone(), snapshot, sink, Some(provenance)).expect("job builds");
    let mut renderer = SolidRenderer;
    let mut steps = 0;
    while !job.state().is_terminal() {
        job.pump(&mut renderer).expect("job step");
        steps += 1;
        assert!(steps < 16, "the job must finish");
    }
    assert_eq!(job.state(), AnimationJobState::Done);
    assert_eq!(job.frames_done(), 2);

    let file = std::fs::File::open(&path).expect("GIF exists");
    let mut options = DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::RGBA);
    let mut decoder = options.read_info(file).expect("GIF decodes");
    let mut frames = 0;
    while decoder.read_next_frame().unwrap().is_some() {
        frames += 1;
    }
    assert_eq!(frames, 2);
    assert!(
        dir.path()
            .read_dir()
            .unwrap()
            .filter_map(|entry| entry.ok())
            .all(|entry| !entry.file_name().to_string_lossy().ends_with(".tmp")),
        "the streaming sink left no temporary file"
    );
}

#[test]
fn a_completion_without_an_outstanding_request_is_rejected() {
    let mut job = AnimationJob::fake(spec(), 2);
    let error = job
        .finish_rendered_frame(Ok(vec![0; 32 * 24 * 4]))
        .expect_err("a stale readback must not push a frame");
    assert!(matches!(error, AnimationError::InvalidSpec(_)), "{error:?}");
    assert_eq!(job.frames_done(), 0);
}

#[test]
fn clearing_a_stale_browser_readback_disconnects_its_sender() {
    let mut app = VfApp::new();
    let (tx, rx) = std::sync::mpsc::channel();
    {
        let mut harness = app.test_harness();
        harness.plant_animation_readback(rx);
        assert!(
            harness.animation_readback_present(),
            "the stale receiver must be planted"
        );
        harness.clear_animation_readback();
        assert!(
            !harness.animation_readback_present(),
            "the production clearing path must drop the receiver"
        );
    }
    assert!(
        tx.send(Ok(Vec::new())).is_err(),
        "the stale callback must not reach a newer job"
    );
}

#[test]
fn the_animation_byte_cap_reports_its_limit() {
    use vector_field_explorer::animation_job::check_animation_byte_cap;

    check_animation_byte_cap(16, 16).expect("the boundary is allowed");
    let error = check_animation_byte_cap(17, 16).expect_err("over the synthetic cap");
    assert_eq!(
        error,
        AnimationError::Encoder("animation output exceeds the 16-byte limit".into())
    );
}
