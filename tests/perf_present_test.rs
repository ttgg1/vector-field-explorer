//! Present-path floor harness (#18 task b): pure env parsing, frame-delta
//! aggregation, named-scene application and the inert-by-default guarantee.

use std::collections::HashMap;

use vector_field_explorer::perf_present::{
    DEFAULT_SECONDS, INNER_SIZE, PresentAggregator, PresentConfig, WARMUP_SECONDS, apply_scene,
    deadline_seconds, host_line, parse_config,
};
use vector_field_explorer::viewmodel::State;

use crate::support::storage::MemoryStorage;

fn lookup(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let map: HashMap<String, String> = vars
        .iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect();
    move |key| map.get(key).cloned()
}

#[test]
fn absent_env_var_leaves_the_mode_inert() {
    assert!(
        parse_config(|_| None)
            .expect("absent mode is not an error")
            .is_none()
    );
    // A normal app never opts into the harness; only `main` builds a present
    // run from an explicit config.
    let app = vector_field_explorer::VfApp::new();
    assert!(!app.present_mode_active());
}

#[test]
fn only_the_exact_enabling_value_turns_the_mode_on() {
    for value in ["0", "yes", "true", ""] {
        let config = parse_config(lookup(&[("VFE_PERF_PRESENT", value)]))
            .expect("a non-1 value is simply inert");
        assert!(config.is_none(), "{value:?} must not enable the mode");
    }
    let config = parse_config(lookup(&[("VFE_PERF_PRESENT", "1")]))
        .expect("=1 enables the mode")
        .expect("a config is produced");
    assert_eq!(config.scene, "default-2d");
    assert_eq!(config.seconds, DEFAULT_SECONDS);
    assert_eq!(config.out, None);
}

#[test]
fn scene_seconds_and_output_are_parsed() {
    let config = parse_config(lookup(&[
        ("VFE_PERF_PRESENT", "1"),
        ("VFE_PERF_SCENE", "densest-3d"),
        ("VFE_PERF_SECONDS", "12.5"),
        ("VFE_PERF_OUT", "/tmp/present.txt"),
    ]))
    .expect("well-formed values parse")
    .expect("enabled");
    assert_eq!(config.scene, "densest-3d");
    assert_eq!(config.seconds, 12.5);
    assert_eq!(
        config.out.as_deref(),
        Some(std::path::Path::new("/tmp/present.txt"))
    );
}

#[test]
fn seconds_defaults_and_clamps_into_the_bounded_window() {
    let low = parse_config(lookup(&[
        ("VFE_PERF_PRESENT", "1"),
        ("VFE_PERF_SECONDS", "1"),
    ]))
    .expect("below the floor clamps")
    .expect("enabled");
    assert_eq!(low.seconds, 5.0);

    let high = parse_config(lookup(&[
        ("VFE_PERF_PRESENT", "1"),
        ("VFE_PERF_SECONDS", "500"),
    ]))
    .expect("above the ceiling clamps")
    .expect("enabled");
    assert_eq!(high.seconds, 60.0);

    let missing = parse_config(lookup(&[("VFE_PERF_PRESENT", "1")]))
        .expect("missing seconds defaults")
        .expect("enabled");
    assert_eq!(missing.seconds, DEFAULT_SECONDS);
}

#[test]
fn invalid_seconds_and_unknown_scenes_are_errors() {
    let bad_seconds = parse_config(lookup(&[
        ("VFE_PERF_PRESENT", "1"),
        ("VFE_PERF_SECONDS", "soon"),
    ]));
    assert!(bad_seconds.is_err());

    let nan_seconds = parse_config(lookup(&[
        ("VFE_PERF_PRESENT", "1"),
        ("VFE_PERF_SECONDS", "NaN"),
    ]));
    assert!(nan_seconds.is_err());

    let unknown_scene = parse_config(lookup(&[
        ("VFE_PERF_PRESENT", "1"),
        ("VFE_PERF_SCENE", "not-a-scene"),
    ]));
    let message = unknown_scene.expect_err("an unknown scene is rejected");
    assert!(message.contains("not-a-scene"), "{message}");
    assert!(message.contains("densest-3d"), "{message}");
}

#[test]
fn the_documented_floor_scenes_apply_through_state_seams() {
    let mut state = State::new(true);
    apply_scene(&mut state, "default-2d").expect("default-2d is supported");
    assert!(!state.mode_3d);
    assert_eq!(state.density, 24);

    let mut state = State::new(false);
    apply_scene(&mut state, "densest-3d").expect("densest-3d is supported");
    assert!(state.mode_3d);
    assert_eq!(state.density, 16);

    assert!(apply_scene(&mut state, "missing").is_err());
}

#[test]
fn percentiles_use_r7_linear_interpolation() {
    let samples: Vec<f64> = (1..=100).map(f64::from).collect();
    let report = PresentAggregator::report_from_deltas_ms(&samples).expect("non-empty");
    assert_eq!(report.n, 100);
    assert!((report.p50_ms - 50.5).abs() < 1e-9);
    assert!((report.p95_ms - 95.05).abs() < 1e-9);
    assert!((report.p99_ms - 99.01).abs() < 1e-9);
    assert!((report.fps_p50 - 1000.0 / 50.5).abs() < 1e-9);
    assert_eq!(report.window_seconds, 5.05);
}

#[test]
fn warm_up_is_discarded_before_aggregation() {
    let mut aggregator = PresentAggregator::new(WARMUP_SECONDS, 5.0);
    // Four 0.5 s frames fill exactly the 2 s warm-up and must not be samples.
    for _ in 0..4 {
        let done = aggregator.push(0.5);
        assert!(!done);
    }
    // Five 1 s frames cover the 5 s measurement window.
    for _ in 0..5 {
        let done = aggregator.push(1.0);
        assert!(!done || aggregator.report().is_some());
    }
    assert!(aggregator.done());
    let report = aggregator.report().expect("measured frames");
    assert_eq!(report.n, 5);
    assert!((report.p50_ms - 1000.0).abs() < 1e-9);
    assert!((report.window_seconds - 5.0).abs() < 1e-9);
}

#[test]
fn non_finite_and_non_positive_deltas_are_ignored() {
    let mut aggregator = PresentAggregator::new(0.0, 1.0);
    assert!(!aggregator.push(f64::NAN));
    assert!(!aggregator.push(f64::INFINITY));
    assert!(!aggregator.push(0.0));
    assert!(!aggregator.push(-1.0));
    assert!(aggregator.report().is_none());
    assert!(aggregator.push(1.0));
    assert_eq!(aggregator.report().expect("one sample").n, 1);
}

#[test]
fn the_reported_inner_size_is_the_documented_floor_size() {
    assert_eq!(INNER_SIZE, [1280, 800]);
}

#[test]
fn host_line_is_bounded_and_tokenised() {
    let line = host_line(
        "AMD Ryzen 9 7950X 16-Core Processor",
        "AMD Radeon RX 9070 XT (RADV GFX1201)",
        "Vulkan",
        1280,
        800,
        "release",
    );
    assert_eq!(
        line,
        "PERF present host=AMD_Ryzen_9_7950X_16-Core_Processor \
         gpu=AMD_Radeon_RX_9070_XT_(RADV_GFX1201) backend=Vulkan size=1280x800 build=release"
    );
}

#[test]
fn config_defaults_are_the_documented_ones() {
    let config = PresentConfig::default();
    assert_eq!(config.scene, "default-2d");
    assert_eq!(config.seconds, DEFAULT_SECONDS);
    assert_eq!(WARMUP_SECONDS, 2.0);
}

#[test]
fn the_wall_clock_deadline_adds_the_documented_slack() {
    assert_eq!(deadline_seconds(10.0), 17.0);
    assert_eq!(deadline_seconds(60.0), 67.0);
}

#[test]
fn a_measurement_save_writes_no_storage() {
    let mut app = vector_field_explorer::VfApp::new_present(PresentConfig::default());
    let mut storage = MemoryStorage::default();
    eframe::App::save(&mut app, &mut storage);
    assert!(
        storage.values.is_empty(),
        "a present measurement must not persist settings: {:?}",
        storage.values
    );
}

#[test]
fn a_normal_app_save_still_writes_settings() {
    let mut app = vector_field_explorer::VfApp::new();
    let mut storage = MemoryStorage::default();
    eframe::App::save(&mut app, &mut storage);
    assert!(
        !storage.values.is_empty(),
        "the inert path keeps the ordinary save"
    );
}
