use std::sync::Arc;

use vector_field_explorer::{
    plot::PlotKind,
    plot_scene::PlotScene,
    sim_clock::{SimClock, has_clock_consumer, should_animate, transport_visible},
    viewmodel::State,
};

fn scalar(source: &str, kind: PlotKind) -> State {
    let mut state = State::new(false);
    state.set_plot_kind(kind);
    state.set_active_source(source.into());
    state
}

fn scene(state: &mut State) -> Arc<PlotScene> {
    state.plot_scene()
}

#[test]
fn static_scalar_has_no_clock_consumer() {
    let state = scalar("x^2", PlotKind::Curve2D);
    assert!(!state.has_time_dependent_consumer());
    assert!(!has_clock_consumer(&state));
    assert!(!transport_visible(&state));
    assert!(!should_animate(&state, &SimClock::default()));
}

#[test]
fn time_dependent_scalar_consumes_the_clock_only_while_playing() {
    let state = scalar("sin(t)*x", PlotKind::Curve2D);
    assert!(state.has_time_dependent_consumer());
    assert!(has_clock_consumer(&state));
    assert!(transport_visible(&state));

    let mut clock = SimClock::default();
    assert!(should_animate(&state, &clock));
    clock.set_playing(false);
    assert!(!should_animate(&state, &clock));
    assert!(
        transport_visible(&state),
        "pause does not hide the transport"
    );

    let surface = scalar("x+y+t", PlotKind::Surface3D);
    assert!(has_clock_consumer(&surface));
}

#[test]
fn animated_scalar_wraps_time_at_the_loop_period() {
    let state = scalar("sin(t)*x", PlotKind::Curve2D);
    let mut clock = SimClock::default();
    clock.set_loop_period(2.0);
    clock.time = 1.98;
    clock.advance_for(&state, 0.2);
    assert!((clock.time - 0.03).abs() < 1e-6, "got {}", clock.time);
}

#[test]
fn time_changes_bump_generation_only_for_a_valid_time_dependent_draft() {
    let mut animated = scalar("sin(t)*x", PlotKind::Curve2D);
    let _ = scene(&mut animated);
    let before = animated.plot.generation;
    assert!(animated.set_time(0.5));
    assert!(animated.plot.generation > before);

    let mut stationary = scalar("x^2", PlotKind::Curve2D);
    let _ = scene(&mut stationary);
    let before = stationary.plot.generation;
    assert!(stationary.set_time(0.5));
    assert_eq!(stationary.plot.generation, before);

    let mut invalid = scalar("sin(t)*x", PlotKind::Curve2D);
    let _ = scene(&mut invalid);
    invalid.set_active_source("sin(t)*".into());
    let before = invalid.plot.generation;
    invalid.set_time(0.75);
    assert_eq!(invalid.plot.generation, before);
}

#[test]
fn sampled_at_tracks_the_frame_and_freezes_on_invalid_edits() {
    let mut animated = scalar("sin(t)*x", PlotKind::Curve2D);
    let _ = scene(&mut animated);
    assert_eq!(animated.plot.curve.sampled_at, Some(0.0));
    animated.set_time(0.5);
    let _ = scene(&mut animated);
    assert_eq!(animated.plot.curve.sampled_at, Some(0.5));

    let mut stationary = scalar("x^2", PlotKind::Curve2D);
    let _ = scene(&mut stationary);
    stationary.set_time(0.5);
    let _ = scene(&mut stationary);
    assert_eq!(stationary.plot.curve.sampled_at, Some(0.0));

    let mut invalid = scalar("sin(t)*x", PlotKind::Curve2D);
    let _ = scene(&mut invalid);
    invalid.set_active_source("sin(t)*".into());
    invalid.set_time(0.5);
    let frozen = scene(&mut invalid);
    assert_eq!(invalid.plot.curve.sampled_at, Some(0.0));
    assert!(invalid.plot.curve.error.is_some());
    assert!(frozen.status.contains("scalar parse error"));
}

#[test]
fn invalid_time_dependent_edit_keeps_the_transport_and_clock_alive() {
    let mut state = scalar("sin(t)*x", PlotKind::Curve2D);
    let _ = scene(&mut state);
    state.set_active_source("sin(t)*".into());
    assert!(state.plot.curve.error.is_some());
    assert!(state.plot.curve.accepted.is_some());
    assert!(state.plot.curve.samples.is_some());
    assert!(has_clock_consumer(&state));
    assert!(transport_visible(&state));
    assert!(should_animate(&state, &SimClock::default()));
}
