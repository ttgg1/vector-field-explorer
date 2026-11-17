use std::fs;

use tempfile::tempdir;
use vector_field_explorer::animation::{
    ANIMATION_MAX_BYTES, ANIMATION_MAX_DURATION, ANIMATION_MAX_FPS, ANIMATION_MAX_FRAMES,
    ANIMATION_MAX_PIXELS, ANIMATION_MIN_DURATION, ANIMATION_MIN_FPS, AnimationError,
    AnimationFormat, AnimationSpec, atomic_replace, frame_count, frame_times,
    gif_delay_centiseconds,
};

fn gif_spec() -> AnimationSpec {
    AnimationSpec {
        start_time: 0.0,
        duration: 1.0,
        fps: 12,
        width: 320,
        height: 240,
        loop_gif: true,
        format: AnimationFormat::Gif,
    }
}

#[test]
fn frame_count_and_times_do_not_duplicate_the_endpoint() {
    let spec = AnimationSpec {
        start_time: 2.0,
        duration: 0.5,
        fps: 10,
        width: 320,
        height: 240,
        loop_gif: true,
        format: AnimationFormat::Gif,
    };
    assert_eq!(frame_count(&spec).unwrap(), 5);
    let times = frame_times(&spec).unwrap();
    assert_eq!(times.len(), 5);
    for (actual, expected) in times.iter().zip([2.0, 2.1, 2.2, 2.3, 2.4]) {
        assert!((actual - expected).abs() < 1e-6);
    }
}

#[test]
fn animation_limits_reject_bad_specs() {
    let base = gif_spec();
    let mutations: [fn(&mut AnimationSpec); 7] = [
        |s: &mut AnimationSpec| s.fps = 0,
        |s: &mut AnimationSpec| s.fps = 31,
        |s: &mut AnimationSpec| s.duration = 0.0,
        |s: &mut AnimationSpec| s.duration = 61.0,
        |s: &mut AnimationSpec| s.start_time = f32::NAN,
        |s: &mut AnimationSpec| s.width = 0,
        |s: &mut AnimationSpec| {
            s.width = 4096;
            s.height = 4096;
        },
    ];
    for mutate in mutations {
        let mut spec = base.clone();
        mutate(&mut spec);
        assert!(
            matches!(spec.validate(), Err(AnimationError::InvalidSpec(_))),
            "{spec:?} must be rejected"
        );
    }
    let odd = AnimationSpec {
        format: AnimationFormat::Mp4,
        width: 321,
        ..base.clone()
    };
    assert!(matches!(
        odd.validate(),
        Err(AnimationError::InvalidSpec(_))
    ));
    let odd_gif = AnimationSpec {
        width: 321,
        ..base.clone()
    };
    assert!(odd_gif.validate().is_ok(), "GIF may use an odd width");
    let mut too_long = base.clone();
    too_long.duration = 60.0;
    too_long.fps = 30;
    assert_eq!(frame_count(&too_long).unwrap(), 1800);
}

#[test]
fn constants_and_defaults_match_the_design() {
    assert_eq!(ANIMATION_MIN_FPS, 1);
    assert_eq!(ANIMATION_MAX_FPS, 30);
    assert_eq!(ANIMATION_MAX_FRAMES, 1800);
    assert_eq!(ANIMATION_MIN_DURATION, 0.1);
    assert_eq!(ANIMATION_MAX_DURATION, 60.0);
    assert_eq!(ANIMATION_MAX_PIXELS, 8_388_608);
    assert_eq!(ANIMATION_MAX_BYTES, 512 * 1024 * 1024);

    let spec = AnimationSpec::default();
    assert_eq!(spec.start_time, 0.0);
    assert_eq!(spec.duration, 4.0);
    assert_eq!(spec.fps, 12);
    assert_eq!((spec.width, spec.height), (1280, 720));
    assert!(spec.loop_gif);
    assert_eq!(spec.format, AnimationFormat::Gif);
}

#[test]
fn boundary_values_validate_and_the_pixel_budget_is_exact() {
    let mut boundary = gif_spec();
    boundary.duration = ANIMATION_MIN_DURATION;
    boundary.fps = ANIMATION_MIN_FPS;
    boundary.width = 4000;
    boundary.height = ANIMATION_MAX_PIXELS as u32 / 4000;
    assert!(boundary.validate().is_ok());
    assert_eq!(frame_count(&boundary).unwrap(), 1);

    boundary.duration = ANIMATION_MAX_DURATION;
    boundary.fps = ANIMATION_MAX_FPS;
    boundary.width = 2048;
    boundary.height = 1024;
    assert!(boundary.validate().is_ok());
    assert_eq!(
        frame_count(&boundary).unwrap(),
        (ANIMATION_MAX_DURATION * ANIMATION_MAX_FPS as f32).ceil() as usize
    );

    boundary.width = 4096;
    boundary.height = 2048;
    assert!(boundary.validate().is_ok());
    boundary.height = 2049;
    assert!(matches!(
        boundary.validate(),
        Err(AnimationError::InvalidSpec(_))
    ));

    let mut nonfinite_duration = gif_spec();
    nonfinite_duration.duration = f32::INFINITY;
    assert!(matches!(
        nonfinite_duration.validate(),
        Err(AnimationError::InvalidSpec(_))
    ));
}

#[test]
fn fractional_durations_do_not_gain_a_duplicate_endpoint_frame() {
    let spec = AnimationSpec {
        duration: 0.1,
        fps: 30,
        ..AnimationSpec::default()
    };
    assert_eq!(frame_count(&spec).unwrap(), 3);
    let times = frame_times(&spec).unwrap();
    assert!((times[2] - (1.0 / 15.0)).abs() < 1e-6);
    assert!(times.iter().all(|time| *time < 0.1));

    let mut two_tenths = spec;
    two_tenths.duration = 0.2;
    assert_eq!(frame_count(&two_tenths).unwrap(), 6);
    let times = frame_times(&two_tenths).unwrap();
    assert!(times.iter().all(|time| *time < 0.2));
}

#[test]
fn gif_delay_quantizes_predictably() {
    assert_eq!(gif_delay_centiseconds(1), 100);
    assert_eq!(gif_delay_centiseconds(12), 8);
    assert_eq!(gif_delay_centiseconds(30), 3);
}

#[test]
fn frame_times_reject_invalid_specs() {
    let mut spec = gif_spec();
    spec.fps = 0;
    assert!(matches!(
        frame_times(&spec),
        Err(AnimationError::InvalidSpec(_))
    ));
    assert!(matches!(
        frame_count(&spec),
        Err(AnimationError::InvalidSpec(_))
    ));
}

#[test]
fn atomic_replace_overwrites_an_existing_destination() {
    let dir = tempdir().unwrap();
    let destination = dir.path().join("output.gif");
    let temporary = dir.path().join(".output.gif.tmp");
    fs::write(&destination, b"old").unwrap();
    fs::write(&temporary, b"new").unwrap();

    atomic_replace(&temporary, &destination).unwrap();

    assert_eq!(fs::read(&destination).unwrap(), b"new");
    assert!(!temporary.exists());
}
