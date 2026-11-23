//! Pure motion tokens and reduced-motion resolution (issue #289).
//!
//! `progress` is the one clamped ease-out sampler every transition shares.
//! `effective_mode` resolves the persisted setting against the platform signal.
//! The final tests drive the real app frame so a hidden surface cannot keep a
//! mid-flight transition requesting paint forever (AC4). The closing tests
//! cover issue #310: a panel moved to a viewport edge must keep its saved
//! resting position across motion-enabled hide/reopen cycles (B5).

use eframe::egui::{self, vec2};

use crate::support;

use vector_field_explorer::VfApp;
use vector_field_explorer::source_scene::FieldMode;
use vector_field_explorer::ui::motion::{
    MotionMode, MotionTokens, MotionTransition, effective_mode, needs_repaint, progress,
};
use vector_field_explorer::ui::viewport_panel::{ViewportPanelState, show_slide};

const VIEWPORT: egui::Vec2 = vec2(1280.0, 1000.0);

/// Drive one real app frame with a known frame time so `stable_dt` produces a
/// deterministic transition tick through `advance_transition`. Both `time` and
/// `predicted_dt` are supplied so `stable_dt` is the requested delta whether or
/// not egui believes the previous frame requested an immediate repaint.
fn app_frame(ctx: &egui::Context, app: &mut VfApp, time_s: f64, dt_s: f32) {
    let mut frame = eframe::Frame::_new_kittest();
    let mut out = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, VIEWPORT)),
            time: Some(time_s),
            predicted_dt: dt_s,
            ..Default::default()
        },
        |ui| eframe::App::ui(app, ui, &mut frame),
    );
    out.textures_delta.clear();
}

#[test]
fn progress_samples_the_ease_out_curve_and_clamps() {
    assert_eq!(progress(0.0, 100.0), 0.0);
    let mid = progress(50.0, 100.0);
    assert!(mid > 0.0 && mid < 1.0, "mid progress was {mid}");
    assert_eq!(progress(100.0, 100.0), 1.0);
    assert_eq!(progress(500.0, 100.0), 1.0, "elapsed past the end clamps");
    assert_eq!(progress(-50.0, 100.0), 0.0, "negative elapsed clamps");
}

#[test]
fn progress_handles_zero_and_non_finite_duration_without_nan() {
    assert_eq!(progress(10.0, 0.0), 1.0, "zero duration is immediate");
    assert_eq!(progress(10.0, f32::NAN), 1.0);
    assert_eq!(progress(10.0, f32::INFINITY), 1.0);
    assert_eq!(progress(f32::NAN, 100.0), 0.0);
    assert!(progress(10.0, 100.0).is_finite());
}

#[test]
fn default_tokens_are_bounded_and_ordered() {
    let tokens = MotionTokens::default();
    assert!(
        tokens.open_slide_ms > tokens.hover_press_ms && tokens.hover_press_ms > 0.0,
        "open/slide is longer than hover/press; both are positive"
    );
    assert!(tokens.open_slide_ms <= 500.0 && tokens.hover_press_ms <= 500.0);
    assert!(tokens.slide_distance_pt > 0.0 && tokens.slide_distance_pt <= 24.0);
}

#[test]
fn transition_starts_at_zero_settles_at_one_and_requests_no_idle_repaint() {
    let duration = MotionTokens::default().open_slide_ms;
    let mut transition = MotionTransition::new(duration);
    assert!(!transition.is_active());
    assert!(
        !needs_repaint(&transition),
        "a fresh, idle transition is inert"
    );

    transition.start();
    assert_eq!(transition.tick(0.0, MotionMode::Full), 0.0);
    assert_eq!(transition.progress(), 0.0);

    // Mid-flight: active and requesting repaint.
    let halfway = transition.tick(duration / 2.0, MotionMode::Full);
    assert!(halfway > 0.0 && halfway < 1.0, "mid progress was {halfway}");
    assert!(transition.is_active());
    assert!(needs_repaint(&transition));

    // Reaching the end: no longer active, no further frames requested.
    let end = transition.tick(duration / 2.0 + 10.0, MotionMode::Full);
    assert_eq!(end, 1.0);
    assert_eq!(transition.progress(), 1.0);
    assert!(!transition.is_active());
    assert!(
        !needs_repaint(&transition),
        "settled transitions never request idle frames"
    );

    // Extra ticks past the end stay settled and stay inert.
    assert_eq!(transition.tick(1000.0, MotionMode::Full), 1.0);
    assert!(!needs_repaint(&transition));
}

#[test]
fn transition_is_immediate_under_reduced_mode() {
    let mut transition = MotionTransition::new(MotionTokens::default().open_slide_ms);
    transition.start();
    assert_eq!(transition.tick(0.0, MotionMode::Reduced), 1.0);
    assert_eq!(transition.progress(), 1.0);
    assert!(!transition.is_active());
    assert!(!needs_repaint(&transition));

    // A mode change mid-flight also jumps straight to the end state.
    transition.start();
    transition.tick(10.0, MotionMode::Full);
    assert!(transition.is_active());
    assert_eq!(transition.tick(10.0, MotionMode::Reduced), 1.0);
    assert!(!transition.is_active());
}

#[test]
fn transition_restart_resets_to_zero() {
    let mut transition = MotionTransition::new(100.0);
    transition.start();
    transition.tick(100.0, MotionMode::Full);
    assert_eq!(transition.progress(), 1.0);
    transition.start();
    assert_eq!(transition.progress(), 0.0);
    assert!(!transition.is_active());
}

#[test]
fn effective_mode_is_reduced_when_either_signal_is_set() {
    assert_eq!(effective_mode(false, false), MotionMode::Full);
    assert_eq!(effective_mode(true, false), MotionMode::Reduced);
    assert_eq!(effective_mode(false, true), MotionMode::Reduced);
    assert_eq!(effective_mode(true, true), MotionMode::Reduced);
}

/// B1 regression: hiding a surface mid-flight must settle its transition so a
/// closed panel never keeps requesting repaint (AC4). Drives the real app frame
/// rather than the pure tick-to-settle path, which cannot catch a caller that
/// forgets to settle on the invisible path.
#[test]
fn hiding_typeset_mid_flight_settles_and_stops_repaint() {
    let ctx = support::context();
    let mut app = VfApp::new();
    let half_s = (MotionTokens::default().open_slide_ms / 1000.0 / 2.0) as f64;

    // Rising edge: opening the panel starts its slide.
    app.formula_typeset.open = true;
    app_frame(&ctx, &mut app, half_s, half_s as f32);
    assert!(
        app.typeset_transition.is_active(),
        "the visible panel is mid-flight after a half-duration tick"
    );
    assert!(needs_repaint(&app.typeset_transition));

    // Falling edge: hiding the panel mid-flight must settle the transition.
    app.formula_typeset.open = false;
    app_frame(&ctx, &mut app, half_s * 2.0, half_s as f32);
    assert!(
        !app.typeset_transition.is_active(),
        "a hidden panel's transition must not stay in flight"
    );
    assert!(
        !needs_repaint(&app.typeset_transition),
        "a hidden panel must never request another frame"
    );
}

/// B1 regression for the legend: switching to a non-vector field mode mid-slide
/// must settle the legend transition and stop repaint requests.
#[test]
fn hiding_legend_mid_flight_settles_and_stops_repaint() {
    let ctx = support::context();
    let mut app = VfApp::new();
    let half_s = (MotionTokens::default().open_slide_ms / 1000.0 / 2.0) as f64;

    // The legend is visible by default in vector mode. Drive a half-duration
    // frame so its transition is in flight.
    assert!(app.state.is_vector_mode());
    app_frame(&ctx, &mut app, half_s, half_s as f32);
    assert!(app.legend_transition.is_active());
    assert!(needs_repaint(&app.legend_transition));

    // Switching to ODE hides the legend mid-flight; it must settle.
    assert!(app.state.set_field_mode(FieldMode::Ode));
    app_frame(&ctx, &mut app, half_s * 2.0, half_s as f32);
    assert!(
        !app.legend_transition.is_active(),
        "a hidden legend's transition must not stay in flight"
    );
    assert!(
        !needs_repaint(&app.legend_transition),
        "a hidden legend must never request another frame"
    );
}

// --- B5: saved resting position must survive animated reopen at a viewport edge ---

/// Drive one frame of the shared floating-panel seam with a known pointer
/// event set. `show_slide` is the same renderer both the legend and the typeset
/// panel use; exercising it here keeps the regression at the shared root cause.
fn slide_frame(
    ctx: &egui::Context,
    viewport: egui::Rect,
    id: egui::Id,
    placement: &mut ViewportPanelState,
    slide: egui::Vec2,
    events: Vec<egui::Event>,
) -> egui::Rect {
    let mut rect = egui::Rect::NOTHING;
    let mut out = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(viewport),
            events,
            ..Default::default()
        },
        |ui| {
            rect = show_slide(
                ui.ctx(),
                viewport,
                id,
                egui::pos2(40.0, 40.0),
                placement,
                slide,
                |ui| {
                    let _ = ui.button("Measure field");
                },
            )
            .response
            .rect;
        },
    );
    out.textures_delta.clear();
    rect
}

fn pointer(pos: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(pos),
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        },
    ]
}

/// B5 regression (issue #310): a panel dragged to the bottom edge and reopened
/// with motion must settle at exactly its saved position. The painted slide is
/// constrained to the viewport; subtracting that shortened slide from the
/// constrained rect must not shift the stored resting position.
#[test]
fn bottom_edge_reopen_keeps_the_saved_resting_position() {
    let ctx = support::context();
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, VIEWPORT);
    let distance = MotionTokens::default().slide_distance_pt;
    let id = egui::Id::unique("issue310_bottom_panel");
    let mut placement = ViewportPanelState::default();

    // Two warm-up frames give the Area a measured size.
    slide_frame(&ctx, viewport, id, &mut placement, egui::Vec2::ZERO, vec![]);
    let resting = slide_frame(&ctx, viewport, id, &mut placement, egui::Vec2::ZERO, vec![]);

    // Move the panel to the bottom edge through its normal header drag.
    let start = resting.center_top();
    let end = start + egui::vec2(0.0, viewport.max.y - resting.bottom() + 4.0);
    slide_frame(
        &ctx,
        viewport,
        id,
        &mut placement,
        egui::Vec2::ZERO,
        pointer(start, true),
    );
    slide_frame(
        &ctx,
        viewport,
        id,
        &mut placement,
        egui::Vec2::ZERO,
        vec![egui::Event::PointerMoved(end)],
    );
    slide_frame(
        &ctx,
        viewport,
        id,
        &mut placement,
        egui::Vec2::ZERO,
        pointer(end, false),
    );
    let saved = slide_frame(&ctx, viewport, id, &mut placement, egui::Vec2::ZERO, vec![]);
    assert_eq!(
        saved.bottom() as f64,
        viewport.max.y as f64,
        "panel is pinned to the bottom edge"
    );

    // A full open/reopen cycle: slide in from below, then settle at rest.
    let _ = slide_frame(
        &ctx,
        viewport,
        id,
        &mut placement,
        egui::vec2(0.0, distance),
        vec![],
    );
    let _ = slide_frame(
        &ctx,
        viewport,
        id,
        &mut placement,
        egui::vec2(0.0, distance * 0.5),
        vec![],
    );
    let settled = slide_frame(&ctx, viewport, id, &mut placement, egui::Vec2::ZERO, vec![]);
    assert_eq!(
        settled.min, saved.min,
        "the reopened panel must settle at its saved position: saved={saved:?}, settled={settled:?}"
    );
}

/// Repeated reopen must not accumulate drift, and a genuine drag must still
/// write a new saved position (the fix may not freeze the placement).
#[test]
fn repeated_reopen_has_no_drift_but_dragging_still_repositions() {
    let ctx = support::context();
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, VIEWPORT);
    let distance = MotionTokens::default().slide_distance_pt;
    let id = egui::Id::unique("issue310_repeat_panel");
    let mut placement = ViewportPanelState::default();

    slide_frame(&ctx, viewport, id, &mut placement, egui::Vec2::ZERO, vec![]);
    let resting = slide_frame(&ctx, viewport, id, &mut placement, egui::Vec2::ZERO, vec![]);
    let start = resting.center_top();
    let end = start + egui::vec2(0.0, viewport.max.y - resting.bottom() + 4.0);
    slide_frame(
        &ctx,
        viewport,
        id,
        &mut placement,
        egui::Vec2::ZERO,
        pointer(start, true),
    );
    slide_frame(
        &ctx,
        viewport,
        id,
        &mut placement,
        egui::Vec2::ZERO,
        vec![egui::Event::PointerMoved(end)],
    );
    slide_frame(
        &ctx,
        viewport,
        id,
        &mut placement,
        egui::Vec2::ZERO,
        pointer(end, false),
    );
    let saved = slide_frame(&ctx, viewport, id, &mut placement, egui::Vec2::ZERO, vec![]);

    for cycle in 0..4 {
        let _ = slide_frame(
            &ctx,
            viewport,
            id,
            &mut placement,
            egui::vec2(0.0, distance),
            vec![],
        );
        let _ = slide_frame(
            &ctx,
            viewport,
            id,
            &mut placement,
            egui::vec2(0.0, distance * 0.5),
            vec![],
        );
        let settled = slide_frame(&ctx, viewport, id, &mut placement, egui::Vec2::ZERO, vec![]);
        assert_eq!(
            settled.min, saved.min,
            "reopen cycle {cycle} drifted: saved={saved:?}, settled={settled:?}"
        );
    }

    // A real drag still moves the panel; the position is not frozen.
    let start = saved.center_top();
    let end = start + egui::vec2(-40.0, -60.0);
    slide_frame(
        &ctx,
        viewport,
        id,
        &mut placement,
        egui::Vec2::ZERO,
        pointer(start, true),
    );
    slide_frame(
        &ctx,
        viewport,
        id,
        &mut placement,
        egui::Vec2::ZERO,
        vec![egui::Event::PointerMoved(end)],
    );
    slide_frame(
        &ctx,
        viewport,
        id,
        &mut placement,
        egui::Vec2::ZERO,
        pointer(end, false),
    );
    let moved = slide_frame(&ctx, viewport, id, &mut placement, egui::Vec2::ZERO, vec![]);
    assert_ne!(
        moved.min, saved.min,
        "intentional dragging must still reposition the panel"
    );
    assert!(
        moved.top() < saved.top() - 40.0,
        "the drag moved the panel upward: {moved:?}"
    );
}

/// One real app frame with caller-supplied pointer events and a known clock.
fn app_frame_events(
    ctx: &egui::Context,
    app: &mut VfApp,
    time_s: f64,
    dt_s: f32,
    events: Vec<egui::Event>,
) {
    let mut frame = eframe::Frame::_new_kittest();
    let mut out = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, VIEWPORT)),
            time: Some(time_s),
            predicted_dt: dt_s,
            events,
            ..Default::default()
        },
        |ui| eframe::App::ui(app, ui, &mut frame),
    );
    out.textures_delta.clear();
}

/// Settle a transition with a few full-duration frames so the slide reaches
/// progress 1 and the painted rect reflects the stored resting position.
fn settle_app(ctx: &egui::Context, app: &mut VfApp, mut time_s: f64) -> f64 {
    let dt = MotionTokens::default().open_slide_ms / 1000.0;
    for _ in 0..3 {
        time_s += dt as f64;
        app_frame_events(ctx, app, time_s, dt, vec![]);
    }
    time_s
}

/// Drive an animated reopen with small time steps so the transition actually
/// spends frames mid-flight (progress strictly between 0 and 1). Only a
/// mid-flight slide paints the panel past the viewport's constrained bottom
/// edge, which is the state that can pollute the stored resting position.
fn reopen_with_motion(ctx: &egui::Context, app: &mut VfApp, mut time_s: f64) -> f64 {
    let full = MotionTokens::default().open_slide_ms / 1000.0;
    let dt = full / 4.0;
    for _ in 0..6 {
        time_s += dt as f64;
        app_frame_events(ctx, app, time_s, dt, vec![]);
    }
    settle_app(ctx, app, time_s)
}

/// AC: move the quantitative legend to the bottom edge through its normal
/// drag control, hide and reopen with motion enabled, and confirm it settles
/// at the saved location on every reopen without accumulating drift.
#[test]
fn legend_reopen_from_bottom_edge_settles_at_saved_location() {
    let ctx = support::context();
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    let mut time_s = 0.0_f64;
    let legend_rect = || {
        ctx.memory(|memory| memory.area_rect(egui::Id::unique("viewport_color_legend")))
            .expect("production legend area")
    };

    // Warm up, then drag the legend down to the bottom edge via its header.
    for _ in 0..3 {
        time_s += 1.0 / 60.0;
        app_frame_events(&ctx, &mut app, time_s, 1.0 / 60.0, vec![]);
    }
    let before = legend_rect();
    let start = before.center_top();
    let end = start + egui::vec2(0.0, VIEWPORT.y - before.bottom() + 4.0);
    time_s += 1.0 / 60.0;
    app_frame_events(&ctx, &mut app, time_s, 1.0 / 60.0, pointer(start, true));
    time_s += 1.0 / 60.0;
    app_frame_events(
        &ctx,
        &mut app,
        time_s,
        1.0 / 60.0,
        vec![egui::Event::PointerMoved(end)],
    );
    time_s += 1.0 / 60.0;
    app_frame_events(&ctx, &mut app, time_s, 1.0 / 60.0, pointer(end, false));
    time_s = settle_app(&ctx, &mut app, time_s);
    let saved = legend_rect();
    assert!(
        saved.bottom() > VIEWPORT.y - 60.0,
        "legend was dragged near the constrained bottom: {saved:?}"
    );

    for cycle in 0..3 {
        // Hide via the normal field-mode control, then reopen with motion.
        assert!(app.state.set_field_mode(FieldMode::Ode));
        time_s = settle_app(&ctx, &mut app, time_s);
        assert!(app.state.set_field_mode(FieldMode::Formula));
        // Drive the slide-in with real mid-flight frames, then settle.
        time_s = reopen_with_motion(&ctx, &mut app, time_s);
        let settled = legend_rect();
        assert!(
            settled.min.distance(saved.min) <= 1.0,
            "legend reopen cycle {cycle} drifted beyond pixel rounding: saved={saved:?}, settled={settled:?}"
        );
    }
}

/// AC: same hide/reopen path for the typeset formula panel through its own
/// open/close control, with motion enabled.
#[test]
fn typeset_reopen_from_bottom_edge_settles_at_saved_location() {
    let ctx = support::context();
    let mut app = VfApp::new();
    let mut time_s = 0.0_f64;
    let typeset_rect = || {
        ctx.memory(|memory| memory.area_rect(egui::Id::unique("Typeset formula")))
            .expect("production typeset area")
    };

    app.formula_typeset.open = true;
    app.layout.help_visible = false;
    for _ in 0..4 {
        time_s += 1.0 / 60.0;
        app_frame_events(&ctx, &mut app, time_s, 1.0 / 60.0, vec![]);
    }
    let before = typeset_rect();
    let start = egui::pos2(before.left() + 4.0, before.center().y);
    let end = start + egui::vec2(0.0, VIEWPORT.y - before.bottom() + 4.0);
    time_s += 1.0 / 60.0;
    app_frame_events(&ctx, &mut app, time_s, 1.0 / 60.0, pointer(start, true));
    time_s += 1.0 / 60.0;
    app_frame_events(
        &ctx,
        &mut app,
        time_s,
        1.0 / 60.0,
        vec![egui::Event::PointerMoved(end)],
    );
    time_s += 1.0 / 60.0;
    app_frame_events(&ctx, &mut app, time_s, 1.0 / 60.0, pointer(end, false));
    time_s = settle_app(&ctx, &mut app, time_s);
    let saved = typeset_rect();
    assert!(
        saved.bottom() > VIEWPORT.y - 60.0,
        "typeset panel was dragged near the constrained bottom: {saved:?}"
    );

    for cycle in 0..3 {
        app.formula_typeset.open = false;
        time_s = settle_app(&ctx, &mut app, time_s);
        app.formula_typeset.open = true;
        time_s = reopen_with_motion(&ctx, &mut app, time_s);
        let settled = typeset_rect();
        assert!(
            settled.min.distance(saved.min) <= 1.0,
            "typeset reopen cycle {cycle} drifted beyond pixel rounding: saved={saved:?}, settled={settled:?}"
        );
    }
}
