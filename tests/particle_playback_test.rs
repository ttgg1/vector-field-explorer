//! Ticket 26: one simulation clock drives the particle cloud, with a compact
//! transport (Play/Pause, reset time, speed) and a repaint only while
//! something is actually animating.
//!
//! The clock is pure; the UI seams (`request_particle_repaint`,
//! `toggle_on_space`) are exercised on headless egui contexts so the repaint
//! and text-focus rules are pinned without a window.

use eframe::egui;

use vector_field_explorer::camera::OrbitCamera;
use vector_field_explorer::domain::Domain;
use vector_field_explorer::field::FieldExpr;
use vector_field_explorer::fly_camera::FlyCamera;
use vector_field_explorer::particles::{Particles, advect, count, spawn};
use vector_field_explorer::render::Camera;
use vector_field_explorer::sim_clock::{
    MAX_SPEED, MAX_STEP, MIN_SPEED, SimClock, has_clock_consumer, should_animate,
    transport_visible,
};
use vector_field_explorer::ui::layout::LayoutState;
use vector_field_explorer::ui::menu::{self, MenuAction, UiState};
use vector_field_explorer::ui::sidebar::Controls;
use vector_field_explorer::ui::transport::{
    self, play_label, request_particle_repaint, time_label, toggle_on_space,
};
use vector_field_explorer::viewmodel::State;

fn close(actual: f32, expected: f32) -> bool {
    (actual - expected).abs() < 1e-6
}

/// A 3D state with particles on: the one v0.1.0 clock consumer.
fn animating_state() -> State {
    let mut state = State::new(true);
    state.set_particles(true);
    state
}

// --- Clock ----------------------------------------------------------------

#[test]
fn a_fresh_clock_is_playing_at_normal_speed() {
    let clock = SimClock::default();
    assert_eq!(clock.time, 0.0, "time starts at zero");
    assert!(clock.playing, "a fresh session runs");
    assert_eq!(clock.speed, 1.0, "speed starts at 1x");
    assert_eq!(clock.step_dt(), 0.0, "no step before the first advance");
}

#[test]
fn advance_clamps_rejects_and_accepts_the_whole_range() {
    let mut clock = SimClock::default();

    clock.advance(-1.0);
    assert_eq!(clock.step_dt(), 0.0, "a negative dt cannot rewind");
    assert_eq!(clock.time, 0.0);

    clock.advance(f32::NAN);
    assert_eq!(clock.step_dt(), 0.0, "NaN is ignored");
    assert_eq!(clock.time, 0.0);

    clock.advance(f32::INFINITY);
    assert_eq!(clock.step_dt(), 0.0, "infinity is ignored");
    assert_eq!(clock.time, 0.0);

    clock.advance(0.0);
    assert_eq!(clock.step_dt(), 0.0, "a zero frame is no motion");

    clock.advance(0.001);
    assert!(close(clock.step_dt(), 0.001), "a tiny dt passes through");
    assert!(close(clock.time, 0.001), "and advances time by the same step");

    clock.advance(10.0);
    assert_eq!(clock.step_dt(), MAX_STEP, "a huge dt clamps to MAX_STEP");
    assert!(
        close(clock.time, 0.001 + MAX_STEP),
        "time advances by the clamped step, not the raw frame: {}",
        clock.time,
    );
}

#[test]
fn the_clock_holds_without_a_consumer_and_resumes_from_the_held_time() {
    let mut clock = SimClock::default();
    let mut state = State::new(true);

    // Particles off: no consumer, so the frame is held, not accumulated.
    clock.advance_for(&state, 0.05);
    assert_eq!(clock.time, 0.0, "hidden time does not accumulate");
    assert_eq!(clock.step_dt(), 0.0, "and no consumer step is produced");

    // Enabling particles resumes from the held time, not from zero.
    state.set_particles(true);
    clock.advance_for(&state, 0.05);
    assert!(close(clock.time, 0.05), "the consumer resumes from the held time");

    // A 2D view hides the consumer again; time holds where it was.
    state.set_mode(false);
    clock.advance_for(&state, 0.05);
    assert!(close(clock.time, 0.05), "2D holds the clock");
    assert_eq!(clock.step_dt(), 0.0);

    // Returning to 3D resumes from the held time.
    state.set_mode(true);
    clock.advance_for(&state, 0.05);
    assert!(close(clock.time, 0.10), "3D resumes from the held time");
}

#[test]
fn pause_stops_time_and_does_not_accumulate() {
    let mut clock = SimClock::default();
    clock.advance(0.05);
    assert!(close(clock.time, 0.05));

    clock.set_playing(false);
    clock.advance(0.05);
    clock.advance(0.05);
    clock.advance(5.0);
    assert_eq!(clock.time, 0.05, "paused frames never advance the clock");
    assert_eq!(clock.step_dt(), 0.0, "no consumer step while paused");

    clock.set_playing(true);
    clock.advance(0.05);
    assert!(
        close(clock.time, 0.1),
        "resuming adds one normal step, never the paused gap: {}",
        clock.time,
    );
    assert!(close(clock.step_dt(), 0.05), "the first frame after Play is a normal step");
}

#[test]
fn speed_scales_the_simulated_step() {
    let mut clock = SimClock::default();
    clock.set_speed(2.0);
    clock.advance(0.02);
    assert!(close(clock.step_dt(), 0.04), "2x speed doubles the step");
    assert!(close(clock.time, 0.04), "and time follows the simulated step");

    clock.reset_time();
    clock.set_speed(MIN_SPEED);
    clock.advance(0.05);
    assert!(close(clock.step_dt(), 0.005), "half speed halves the step");

    clock.set_speed(100.0);
    assert_eq!(clock.speed, MAX_SPEED, "the speed clamps to the slider's top");
    clock.set_speed(-1.0);
    assert_eq!(clock.speed, MIN_SPEED, "and to the slider's bottom");
    clock.set_speed(f32::NAN);
    assert_eq!(clock.speed, 1.0, "a non-finite speed falls back to 1x");
}

#[test]
fn reset_time_zeroes_the_clock_and_keeps_playback_settings() {
    let mut clock = SimClock::default();
    clock.set_speed(2.5);
    clock.advance(0.05);

    clock.reset_time();
    assert_eq!(clock.time, 0.0, "reset time returns to zero");
    assert_eq!(clock.step_dt(), 0.0, "and clears the last step");
    assert!(clock.playing, "reset time does not pause");
    assert_eq!(clock.speed, 2.5, "reset time does not change speed");

    clock.set_playing(false);
    clock.reset_time();
    assert!(!clock.playing, "resetting time never silently starts playback");
}

#[test]
fn reset_restores_the_fresh_clock() {
    let mut clock = SimClock::default();
    clock.set_speed(3.0);
    clock.advance(0.05);
    clock.set_playing(false);

    clock.reset();
    assert_eq!(clock.time, 0.0);
    assert!(clock.playing);
    assert_eq!(clock.speed, 1.0);
    assert_eq!(clock.step_dt(), 0.0);
}

// --- Predicates and repaint ----------------------------------------------

#[test]
fn should_animate_needs_playing_and_a_particles_3d_consumer() {
    let clock = SimClock::default();
    let mut paused = SimClock::default();
    paused.set_playing(false);

    let mut state = State::new(true);
    assert!(!should_animate(&state, &clock), "particles off: nothing consumes the clock");
    assert!(!has_clock_consumer(&state));

    state.set_particles(true);
    assert!(should_animate(&state, &clock), "particles on in 3D animates");
    assert!(has_clock_consumer(&state));
    assert!(!should_animate(&state, &paused), "paused never animates");

    state.set_mode(false);
    assert!(!should_animate(&state, &clock), "2D has no particle cloud");
    assert!(!has_clock_consumer(&state));
}

#[test]
fn transport_is_visible_only_with_a_clock_consumer() {
    let mut static_3d = State::new(true);
    assert!(
        !transport_visible(&static_3d),
        "a static field with particles off shows no transport chrome",
    );
    static_3d.set_particles(true);
    assert!(transport_visible(&static_3d), "particles on in 3D show the transport");

    let mut flat = State::new(false);
    flat.set_particles(true);
    assert!(!transport_visible(&flat), "a 2D view never shows the transport");
    assert!(!has_clock_consumer(&flat), "the consumer predicate agrees");
}

/// A context that has served egui's initial repaint request, so a later
/// `has_requested_repaint` reports only what the test itself asked for.
fn quiet_context() -> egui::Context {
    let ctx = egui::Context::default();
    // A fresh context has egui's initial multi-pass repaint pending (and the
    // first pass installs fonts); three empty passes leave it genuinely idle.
    for _ in 0..3 {
        let mut output = ctx.run_ui(egui::RawInput::default(), |_| {});
        output.textures_delta.clear();
    }
    assert!(!ctx.has_requested_repaint(), "the baseline context is idle");
    ctx
}

#[test]
fn repaint_is_requested_only_while_animating() {
    let state = animating_state();
    let clock = SimClock::default();

    let ctx = quiet_context();
    request_particle_repaint(&ctx, &state, &clock);
    assert!(ctx.has_requested_repaint(), "an animating cloud keeps the loop alive");

    let mut paused = SimClock::default();
    paused.set_playing(false);
    let ctx = quiet_context();
    request_particle_repaint(&ctx, &state, &paused);
    assert!(!ctx.has_requested_repaint(), "paused requests no repaint");

    let off = State::new(true);
    let ctx = quiet_context();
    request_particle_repaint(&ctx, &off, &clock);
    assert!(!ctx.has_requested_repaint(), "particles off requests no repaint");
}

// --- Particle wiring ------------------------------------------------------

#[test]
fn pause_keeps_the_cloud_and_play_resumes_from_the_same_positions() {
    let domain = Domain::default();
    let field = FieldExpr::parse("(1, 0, 0)").expect("pump field parses");
    let mut cloud = Particles::default();
    spawn(&mut cloud, &domain, 32, 3);
    let mut clock = SimClock::default();

    clock.advance(0.05);
    advect(&mut cloud, &field, &domain, clock.step_dt());
    let paused_at = cloud.pos.clone();
    assert_eq!(count(&cloud), 32);

    clock.set_playing(false);
    for _ in 0..20 {
        clock.advance(0.05);
        advect(&mut cloud, &field, &domain, clock.step_dt());
    }
    assert_eq!(count(&cloud), 32, "paused frames never clear the cloud");
    assert_eq!(cloud.pos, paused_at, "paused frames never move the cloud");

    clock.set_playing(true);
    clock.advance(0.05);
    advect(&mut cloud, &field, &domain, clock.step_dt());
    assert_ne!(cloud.pos, paused_at, "play resumes from the paused positions");
    let step = cloud.pos[0].x - paused_at[0].x;
    assert!(
        step > 0.0 && step < 0.1,
        "the first frame after Play is one normal step, not a jump: {step}",
    );
}

// --- Space shortcut -------------------------------------------------------

/// What one Space press did: whether playback toggled and whether the transport
/// consumed the key from the event stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SpaceOutcome {
    toggled: bool,
    consumed: bool,
}

/// One egui pass with a Space press (and an optional focused text field), and
/// what the real transport shortcut did with it.
fn run_space(state: &State, clock: &mut SimClock, focus_text: bool) -> SpaceOutcome {
    let ctx = egui::Context::default();
    if focus_text {
        let mut text = String::new();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.text_edit_singleline(&mut text).request_focus();
        });
        output.textures_delta.clear();
    }
    let mut input = egui::RawInput::default();
    input.events.push(egui::Event::Key {
        key: egui::Key::Space,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    });
    let mut outcome = None;
    let mut output = ctx.run_pass(input, |ctx| {
        if outcome.is_none() {
            let toggled = toggle_on_space(ctx, state, clock);
            let key_left = ctx.input(|input| {
                input.events.iter().any(|event| {
                    matches!(
                        event,
                        egui::Event::Key { key: egui::Key::Space, pressed: true, .. }
                    )
                })
            });
            outcome = Some(SpaceOutcome { toggled, consumed: !key_left });
        }
    });
    output.textures_delta.clear();
    outcome.unwrap_or(SpaceOutcome { toggled: false, consumed: false })
}

#[test]
fn space_toggles_only_when_visible_and_never_while_typing() {
    let state = animating_state();
    let mut clock = SimClock::default();

    let visible = run_space(&state, &mut clock, false);
    assert!(visible.toggled, "Space toggles a visible transport");
    assert!(visible.consumed, "and consumes the key");
    assert!(!clock.playing, "and pauses it");

    let again = run_space(&state, &mut clock, false);
    assert!(again.toggled, "Space toggles back");
    assert!(clock.playing, "and plays it");

    let typing = run_space(&state, &mut clock, true);
    assert!(!typing.toggled, "typing owns Space");
    assert!(!typing.consumed, "the text field still receives its Space");
    assert!(clock.playing, "the focused formula field is untouched");

    let hidden = State::new(true);
    let mut hidden_clock = SimClock::default();
    let hidden_outcome = run_space(&hidden, &mut hidden_clock, false);
    assert!(!hidden_outcome.toggled, "no consumer, no shortcut");
    assert!(!hidden_outcome.consumed, "and the key is left alone");
    assert!(hidden_clock.playing, "a hidden transport never starts playback");

    let mut flat = State::new(false);
    flat.set_particles(true);
    let mut flat_clock = SimClock::default();
    let flat_outcome = run_space(&flat, &mut flat_clock, false);
    assert!(!flat_outcome.toggled, "2D hides the transport");
    assert!(!flat_outcome.consumed);
    assert!(flat_clock.playing);
}

#[test]
fn space_is_left_to_a_focused_formula_field() {
    let state = animating_state();
    let mut clock = SimClock::default();

    let outcome = run_space(&state, &mut clock, true);

    assert!(!outcome.toggled, "a focused formula field keeps Space for typing");
    assert!(!outcome.consumed, "the transport leaves the key in the event stream");
    assert!(clock.playing, "playback is unchanged");
}

// --- Controls, reset and help --------------------------------------------

#[test]
fn transport_labels_reflect_state_and_stay_compact_ready() {
    assert!(play_label(true, false).contains("Pause"), "playing shows Pause");
    assert!(play_label(false, false).contains("Play"), "paused shows Play");
    assert_eq!(play_label(true, true), "⏸", "compact mode is icon-only");
    assert_eq!(play_label(false, true), "▶");
    assert_eq!(time_label(12.4, false), "t = 12.4 s", "the readout is compact");
    assert_eq!(time_label(12.4, true), "t = 12.4", "with a shorter compact label");
    assert_eq!((MIN_SPEED, MAX_SPEED), (0.1, 3.0));
}

/// One accesskit node with just the fields the transport test compares.
struct Node {
    role: egui::Role,
    label: Option<String>,
    value: Option<String>,
    rect: egui::Rect,
}

fn nodes(output: &egui::FullOutput) -> Vec<Node> {
    let Some(update) = output.platform_output.accesskit_update.as_ref() else {
        return Vec::new();
    };
    update
        .nodes
        .iter()
        .filter_map(|(_, node)| {
            let bounds = node.bounds()?;
            Some(Node {
                role: node.role(),
                label: node.label().map(str::to_owned),
                value: node.value().map(str::to_owned),
                rect: egui::Rect::from_min_max(
                    egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                    egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                ),
            })
        })
        .collect()
}

fn has_label(nodes: &[Node], label: &str) -> bool {
    nodes.iter().any(|node| {
        node.label.as_deref() == Some(label) || node.value.as_deref() == Some(label)
    })
}

fn button_rect(nodes: &[Node], label: &str) -> egui::Rect {
    nodes
        .iter()
        .find(|node| node.role == egui::Role::Button && node.label.as_deref() == Some(label))
        .unwrap_or_else(|| panic!("the transport exposes a `{label}` button"))
        .rect
}

#[test]
fn the_transport_strip_shows_play_reset_time_and_speed_and_toggles() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut clock = SimClock::default();
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(960.0, 600.0));
    let frame = |ctx: &egui::Context, clock: &mut SimClock, events: Vec<egui::Event>| {
        let mut output = ctx.run_ui(
            egui::RawInput { screen_rect: Some(screen), events, ..Default::default() },
            |ui| transport::show(ui, clock, false),
        );
        output.textures_delta.clear();
        output
    };

    let output = frame(&ctx, &mut clock, Vec::new());
    let ui_nodes = nodes(&output);
    assert!(has_label(&ui_nodes, "⏸ Pause"), "the strip offers Pause while playing");
    assert!(has_label(&ui_nodes, "↺ Reset time"), "and a reset-time button");
    assert!(has_label(&ui_nodes, "t = 0.0 s"), "with the simulated time readout");

    let pause = button_rect(&ui_nodes, "⏸ Pause");
    let click = |pos: egui::Pos2, pressed: bool| {
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    };
    let _ = frame(&ctx, &mut clock, click(pause.center(), true));
    let _ = frame(&ctx, &mut clock, click(pause.center(), false));
    assert!(!clock.playing, "clicking Pause stops the clock");
    let output = frame(&ctx, &mut clock, Vec::new());
    assert!(has_label(&nodes(&output), "▶ Play"), "the button now reads Play");

    clock.time = 0.8;
    let reset = button_rect(&nodes(&output), "↺ Reset time");
    let _ = frame(&ctx, &mut clock, click(reset.center(), true));
    let _ = frame(&ctx, &mut clock, click(reset.center(), false));
    assert_eq!(clock.time, 0.0, "reset time returns the clock to zero");
    assert!(!clock.playing, "reset time does not resume playback");
}

#[test]
fn reset_all_controls_restores_a_fresh_clock() {
    let mut clock = SimClock::default();
    clock.set_speed(2.5);
    clock.advance(0.05);
    clock.set_playing(false);

    let mut layout = LayoutState::default();
    let mut orbit = OrbitCamera::default();
    let mut fly: Option<FlyCamera> = None;
    let mut camera = Camera::default();
    let mut state = State::new(false);
    let mut controls = Controls::new();
    let effect = menu::apply_action(
        MenuAction::ResetAllControls,
        &mut UiState {
            layout: &mut layout,
            orbit: &mut orbit,
            fly: &mut fly,
            camera: &mut camera,
            state: &mut state,
            controls: &mut controls,
            clock: &mut clock,
        },
    );
    assert_eq!(effect, menu::SideEffect::None);
    assert_eq!(clock.time, 0.0, "reset controls returns time to zero");
    assert!(clock.playing, "and starts playing");
    assert_eq!(clock.speed, 1.0, "and restores 1x speed");
    assert_eq!(clock.step_dt(), 0.0);
}

#[test]
fn features_reset_leaves_the_transport_clock_alone() {
    let mut clock = SimClock::default();
    clock.set_speed(2.7);
    clock.set_playing(false);

    let mut controls = Controls::new();
    controls.reset_features();

    assert_eq!(clock.speed, 2.7, "speed belongs to the transport, not Features");
    assert!(!clock.playing, "and Features reset never resumes playback");
}
