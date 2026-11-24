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

#[test]
fn platform_preference_settles_widget_easing_and_panel_slides_on_the_first_frame() {
    let ctx = support::context();
    let mut app = VfApp::new();
    assert!(!app.style.reduced_motion);
    let dt = 0.01;

    app.test_set_platform_reduced_motion(true);
    app.formula_typeset.open = true;
    app_frame(&ctx, &mut app, dt, dt as f32);
    assert_eq!(app.typeset_transition.progress(), 1.0);
    assert_eq!(app.legend_transition.progress(), 1.0);
    assert_eq!(ctx.style_of(egui::Theme::Dark).animation_time, 0.0);

    app.test_set_platform_reduced_motion(false);
    app_frame(&ctx, &mut app, dt * 2.0, dt as f32);
    assert!(ctx.style_of(egui::Theme::Dark).animation_time > 0.0);

    app.style.reduced_motion = true;
    app.formula_typeset.open = false;
    app_frame(&ctx, &mut app, dt * 3.0, dt as f32);
    app.formula_typeset.open = true;
    app_frame(&ctx, &mut app, dt * 4.0, dt as f32);
    assert_eq!(app.typeset_transition.progress(), 1.0);
    assert_eq!(ctx.style_of(egui::Theme::Dark).animation_time, 0.0);
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
    slide_frame_sized(ctx, viewport, id, placement, slide, events, NO_EXTRA_HEIGHT)
}

/// No extra content: the fixed-content regressions use the same widget tree as
/// the reflow test so a measured-size change is the only difference.
const NO_EXTRA_HEIGHT: f32 = 0.0;

/// Like [`slide_frame`], but the contents can reflow between frames by changing
/// `content_height`. This models a panel whose measured size changes while the
/// open slide is in flight (viewport resize, collapsing header, text rewrap).
fn slide_frame_sized(
    ctx: &egui::Context,
    viewport: egui::Rect,
    id: egui::Id,
    placement: &mut ViewportPanelState,
    slide: egui::Vec2,
    events: Vec<egui::Event>,
    content_height: f32,
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
                    ui.add_space(content_height);
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

/// Move a shared-seam panel with a real pointer drag. `from` and `to` are the
/// grab and release points; the caller chooses them for an edge pin or a free
/// reposition. Returns the frame at the final position.
fn drag_panel(
    frame: &mut impl FnMut(egui::Vec2, Vec<egui::Event>) -> egui::Rect,
    from: egui::Pos2,
    to: egui::Pos2,
) -> egui::Rect {
    frame(egui::Vec2::ZERO, pointer(from, true));
    frame(egui::Vec2::ZERO, vec![egui::Event::PointerMoved(to)]);
    frame(egui::Vec2::ZERO, pointer(to, false));
    frame(egui::Vec2::ZERO, vec![])
}

/// Fractional-offset regression: egui rounds the painted Area position to
/// whole pixels (`round_area_position`), but a reconstructed slide offset is
/// unrounded. Near — but not against — the bottom edge, consecutive mid-flight
/// frames with fractional offsets can therefore quantize the painted top and
/// then subtract the raw offset, accumulating error in the saved rest. This
/// uses real eased timing (fractional elapsed samples) and no dragging.
#[test]
fn fractional_animated_reopen_does_not_shift_saved_rest() {
    let ctx = support::context();
    ctx.set_pixels_per_point(1.25);
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, VIEWPORT);
    let id = egui::Id::unique("issue310_fractional_panel");
    let mut placement = ViewportPanelState::default();
    let distance = MotionTokens::default().slide_distance_pt;
    let duration = MotionTokens::default().open_slide_ms;

    // Warm up, then drag to a spot 10pt short of the fully clamped bottom.
    let mut frame = |slide, events| slide_frame(&ctx, viewport, id, &mut placement, slide, events);
    frame(egui::Vec2::ZERO, vec![]);
    let resting = frame(egui::Vec2::ZERO, vec![]);
    let gap = 10.0_f32;
    let start = resting.center_top();
    let end = start + egui::vec2(0.0, viewport.max.y - resting.bottom() - gap);
    let saved = drag_panel(&mut frame, start, end);
    assert!(
        saved.bottom() < viewport.max.y - 2.0 && saved.bottom() > viewport.max.y - gap - 4.0,
        "panel sits near but off the bottom clamp: {saved:?}"
    );

    // A persistent fractional slide (no settle frame) is the purest form of
    // the feedback risk: each frame quantizes the painted top, then subtracts
    // the raw offset from the stored rest.
    let tiny = egui::vec2(0.0, 0.4_f32);
    for _ in 0..20 {
        frame(tiny, vec![]);
    }
    let after_hold = frame(egui::Vec2::ZERO, vec![]);
    assert_eq!(
        after_hold.min, saved.min,
        "a persistent fractional slide must not shift the saved rest: \
         saved={saved:?}, after={after_hold:?}"
    );

    for cycle in 0..4 {
        // Real eased progress at fractional elapsed times produces fractional
        // offsets (e.g. 0.4, 0.7) rather than the integral 12/6 offsets.
        for elapsed in [0.15, 0.35, 0.6, 0.85].map(|f| f * duration) {
            let p = progress(elapsed, duration);
            frame(egui::vec2(0.0, distance * (1.0 - p)), vec![]);
        }
        let settled = frame(egui::Vec2::ZERO, vec![]);
        assert_eq!(
            settled.min, saved.min,
            "fractional reopen cycle {cycle} shifted the saved rest: \
             saved={saved:?}, settled={settled:?}"
        );
    }
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

    // Two warm-up frames give the Area a measured size, then drag to the edge.
    let mut frame = |slide, events| slide_frame(&ctx, viewport, id, &mut placement, slide, events);
    frame(egui::Vec2::ZERO, vec![]);
    let resting = frame(egui::Vec2::ZERO, vec![]);
    let start = resting.center_top();
    let end = start + egui::vec2(0.0, viewport.max.y - resting.bottom() + 4.0);
    let saved = drag_panel(&mut frame, start, end);
    assert_eq!(
        saved.bottom() as f64,
        viewport.max.y as f64,
        "panel is pinned to the bottom edge"
    );

    // A full open/reopen cycle: slide in from below, then settle at rest.
    frame(egui::vec2(0.0, distance), vec![]);
    frame(egui::vec2(0.0, distance * 0.5), vec![]);
    let settled = frame(egui::Vec2::ZERO, vec![]);
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

    let mut frame = |slide, events| slide_frame(&ctx, viewport, id, &mut placement, slide, events);
    frame(egui::Vec2::ZERO, vec![]);
    let resting = frame(egui::Vec2::ZERO, vec![]);
    let start = resting.center_top();
    let end = start + egui::vec2(0.0, viewport.max.y - resting.bottom() + 4.0);
    let saved = drag_panel(&mut frame, start, end);

    for cycle in 0..4 {
        frame(egui::vec2(0.0, distance), vec![]);
        frame(egui::vec2(0.0, distance * 0.5), vec![]);
        let settled = frame(egui::Vec2::ZERO, vec![]);
        assert_eq!(
            settled.min, saved.min,
            "reopen cycle {cycle} drifted: saved={saved:?}, settled={settled:?}"
        );
    }

    // A real drag still moves the panel; the position is not frozen.
    let start = saved.center_top();
    let end = start + egui::vec2(-40.0, -60.0);
    let moved = drag_panel(&mut frame, start, end);
    assert_ne!(
        moved.min, saved.min,
        "intentional dragging must still reposition the panel"
    );
    assert!(
        moved.top() < saved.top() - 40.0,
        "the drag moved the panel upward: {moved:?}"
    );
}

/// B5 follow-up: egui's `Area::begin` constrains the painted position using the
/// *previous* frame's measured size, while `Area::end` replaces that size with
/// the newly measured content size. When contents reflow mid-slide, recovering
/// the applied slide with the new size inverts the wrong constraint and drags
/// the saved rest off the bottom edge. The inverse must use the size egui used.
#[test]
fn reflow_during_slide_keeps_the_saved_rest_and_post_resize_drag_works() {
    let ctx = support::context();
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, VIEWPORT);
    let distance = MotionTokens::default().slide_distance_pt;
    let id = egui::Id::unique("issue310_reflow_panel");
    let mut placement = ViewportPanelState::default();
    let tall = 160.0_f32; // total measured height ~= 184 with the button
    let short = 40.0_f32;

    let mut frame =
        |vp, slide, events, h| slide_frame_sized(&ctx, vp, id, &mut placement, slide, events, h);

    // Warm up with tall content, then drag to the bottom edge.
    frame(viewport, egui::Vec2::ZERO, vec![], tall);
    let resting = frame(viewport, egui::Vec2::ZERO, vec![], tall);
    let start = resting.center_top();
    let end = start + egui::vec2(0.0, viewport.max.y - resting.bottom() + 4.0);
    let saved = drag_panel(
        &mut |slide, events| frame(viewport, slide, events, tall),
        start,
        end,
    );
    assert_eq!(
        saved.bottom() as f64,
        viewport.max.y as f64,
        "tall panel pinned to bottom"
    );

    // Reopen with a mid-flight slide, then reflow to short content before it
    // settles. egui constrained with the tall size, so the saved rest must stay.
    let _ = frame(viewport, egui::vec2(0.0, distance), vec![], tall);
    let _ = frame(viewport, egui::vec2(0.0, distance * 0.5), vec![], short);
    let settled = frame(viewport, egui::Vec2::ZERO, vec![], short);
    assert_eq!(
        settled.min, saved.min,
        "reflow mid-slide must not shift the saved rest: saved={saved:?}, settled={settled:?}"
    );

    // Shrink the viewport so the short panel is re-clamped to a new bottom.
    let small = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(VIEWPORT.x, 600.0));
    let resized = frame(small, egui::Vec2::ZERO, vec![], short);
    assert!(
        resized.bottom() <= small.max.y + 1.0,
        "panel fits the shrunk viewport: {resized:?}"
    );
    let rest_after_resize = frame(small, egui::Vec2::ZERO, vec![], short);

    // A real drag after the resize still repositions and the new spot holds.
    let start = rest_after_resize.center_top();
    let end = start + egui::vec2(-30.0, -80.0);
    let moved = drag_panel(
        &mut |slide, events| frame(small, slide, events, short),
        start,
        end,
    );
    assert!(
        moved.top() < rest_after_resize.top() - 40.0,
        "post-resize drag moved the panel: {moved:?}"
    );

    // A reopen after the drag must settle on the dragged position, not drift.
    let _ = frame(small, egui::vec2(0.0, distance), vec![], short);
    let settled = frame(small, egui::Vec2::ZERO, vec![], short);
    assert!(
        settled.min.distance(moved.min) <= 1.0,
        "reopen after resize+drag drifted: moved={moved:?}, settled={settled:?}"
    );
}

/// One real app frame with caller-supplied pointer events and a known clock.
fn app_frame_events(
    ctx: &egui::Context,
    app: &mut VfApp,
    time_s: f64,
    dt_s: f32,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
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
    out
}

/// Locate a labelled control in a real app frame, then click it through real
/// pointer events. This is the normal entry point users take, not a direct
/// mutation of the underlying app state.
fn click_app_control(ctx: &egui::Context, app: &mut VfApp, time_s: &mut f64, label: &str) {
    let dt = 1.0_f32 / 60.0;
    let dts = dt as f64;
    *time_s += dts;
    let out = app_frame_events(ctx, app, *time_s, dt, vec![]);
    let rect = out
        .platform_output
        .accesskit_update
        .as_ref()
        .expect("accesskit enabled")
        .nodes
        .iter()
        .find_map(|(_, node)| {
            (node.label() == Some(label)).then(|| {
                let b = node.bounds().expect("control bounds");
                egui::Rect::from_min_max(
                    egui::pos2(b.x0 as f32, b.y0 as f32),
                    egui::pos2(b.x1 as f32, b.y1 as f32),
                )
            })
        })
        .unwrap_or_else(|| panic!("real control {label:?} is not in the app frame"));
    let center = rect.center();
    // A move frame first matches the real control path and settles hover state.
    *time_s += dts;
    app_frame_events(
        ctx,
        app,
        *time_s,
        dt,
        vec![egui::Event::PointerMoved(center)],
    );
    *time_s += dts;
    app_frame_events(ctx, app, *time_s, dt, pointer(center, true));
    *time_s += dts;
    app_frame_events(ctx, app, *time_s, dt, pointer(center, false));
}

/// Drag a floating panel's Area down to the bottom of the constrained viewport
/// through real pointer events. `grab` selects a point inside the panel that is
/// draggable (the legend header vs. the typeset panel's left edge).
fn drag_to_bottom(
    ctx: &egui::Context,
    app: &mut VfApp,
    time_s: &mut f64,
    area_id: egui::Id,
    grab: impl Fn(egui::Rect) -> egui::Pos2,
) {
    let dt = 1.0_f32 / 60.0;
    let dts = dt as f64;
    let rect = || {
        ctx.memory(|memory| memory.area_rect(area_id))
            .expect("production panel area")
    };
    let before = rect();
    let start = grab(before);
    let end = start + egui::vec2(0.0, VIEWPORT.y - before.bottom() + 4.0);
    *time_s += dts;
    app_frame_events(ctx, app, *time_s, dt, pointer(start, true));
    *time_s += dts;
    app_frame_events(ctx, app, *time_s, dt, vec![egui::Event::PointerMoved(end)]);
    *time_s += dts;
    app_frame_events(ctx, app, *time_s, dt, pointer(end, false));
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
/// drag control, then hide and reopen it by clicking the real Formula/ODE
/// field-mode labels. Each motion reopen must settle at the saved location
/// without accumulating drift.
#[test]
fn legend_reopen_from_bottom_edge_settles_at_saved_location() {
    let ctx = support::context();
    ctx.enable_accesskit();
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    let mut time_s = 0.0_f64;
    let legend_id = egui::Id::unique("viewport_color_legend");
    let legend_rect = || {
        ctx.memory(|memory| memory.area_rect(legend_id))
            .expect("production legend area")
    };

    for _ in 0..3 {
        time_s += 1.0 / 60.0;
        app_frame_events(&ctx, &mut app, time_s, 1.0 / 60.0, vec![]);
    }
    drag_to_bottom(&ctx, &mut app, &mut time_s, legend_id, |r| r.center_top());
    time_s = settle_app(&ctx, &mut app, time_s);
    let saved = legend_rect();
    assert!(
        saved.bottom() > VIEWPORT.y - 60.0,
        "legend was dragged near the constrained bottom: {saved:?}"
    );

    for cycle in 0..3 {
        // Normal controls: click the real ODE selector to hide the legend.
        click_app_control(&ctx, &mut app, &mut time_s, "ODE");
        assert_eq!(
            app.state.field_mode,
            FieldMode::Ode,
            "ODE label click hid the legend"
        );
        time_s = settle_app(&ctx, &mut app, time_s);
        // Click the real Formula selector to reopen it with motion.
        click_app_control(&ctx, &mut app, &mut time_s, "Formula");
        assert_eq!(
            app.state.field_mode,
            FieldMode::Formula,
            "Formula label click reopened the legend"
        );
        time_s = reopen_with_motion(&ctx, &mut app, time_s);
        let settled = legend_rect();
        assert!(
            settled.min.distance(saved.min) <= 1.0,
            "legend reopen cycle {cycle} drifted beyond pixel rounding: saved={saved:?}, settled={settled:?}"
        );
    }
}

/// AC: same hide/reopen path for the typeset formula panel through its own
/// real toggle control, with motion enabled. The panel is opened, moved to the
/// bottom edge, closed and reopened again using only pointer clicks.
#[test]
fn typeset_reopen_from_bottom_edge_settles_at_saved_location() {
    let ctx = support::context();
    ctx.enable_accesskit();
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    let mut time_s = 0.0_f64;
    let typeset_id = egui::Id::unique("Typeset formula");
    let typeset_rect = || {
        ctx.memory(|memory| memory.area_rect(typeset_id))
            .expect("production typeset area")
    };

    // Open through the real floating toggle strip control. Give the overlay
    // Area a few frames to publish its button before clicking it.
    for _ in 0..3 {
        time_s += 1.0 / 60.0;
        app_frame_events(&ctx, &mut app, time_s, 1.0 / 60.0, vec![]);
    }
    click_app_control(&ctx, &mut app, &mut time_s, "Show typeset formula");
    assert!(app.formula_typeset.open, "toggle click opened the panel");
    for _ in 0..3 {
        time_s += 1.0 / 60.0;
        app_frame_events(&ctx, &mut app, time_s, 1.0 / 60.0, vec![]);
    }
    drag_to_bottom(&ctx, &mut app, &mut time_s, typeset_id, |r| {
        egui::pos2(r.left() + 4.0, r.center().y)
    });
    time_s = settle_app(&ctx, &mut app, time_s);
    let saved = typeset_rect();
    assert!(
        saved.bottom() > VIEWPORT.y - 60.0,
        "typeset panel was dragged near the constrained bottom: {saved:?}"
    );

    for cycle in 0..3 {
        // Normal controls: the toggle button hides, then reopens the panel.
        click_app_control(&ctx, &mut app, &mut time_s, "Hide typeset formula");
        assert!(!app.formula_typeset.open, "toggle click hid the panel");
        time_s = settle_app(&ctx, &mut app, time_s);
        click_app_control(&ctx, &mut app, &mut time_s, "Show typeset formula");
        assert!(app.formula_typeset.open, "toggle click reopened the panel");
        time_s = reopen_with_motion(&ctx, &mut app, time_s);
        let settled = typeset_rect();
        assert!(
            settled.min.distance(saved.min) <= 1.0,
            "typeset reopen cycle {cycle} drifted beyond pixel rounding: saved={saved:?}, settled={settled:?}"
        );
    }
}
