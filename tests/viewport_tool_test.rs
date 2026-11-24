//! Issue #228: the viewport-edge click tool is derived from real state, supports
//! one-shot and sticky placement, enable-and-place, Escape and keyboard
//! selection, and keeps one placement click to exactly one action.

use eframe::egui;

use crate::support;

use vector_field_explorer::VfApp;
use vector_field_explorer::interaction::Bindings;
use vector_field_explorer::scalar_view::MAX_PROBES;
use vector_field_explorer::ui::sidebar::Controls;
use vector_field_explorer::ui::viewport_tool::{self, ToolAvailability, ToolInputs, ViewportTool};
use vector_field_explorer::view_command::{CLICK_HINT_PARTICLES_OFF, CLICK_HINT_STREAMLINES_OFF};
use vector_field_explorer::viewmodel::{LeftClickAction, State};

fn inputs() -> ToolInputs {
    ToolInputs {
        vector_mode: true,
        seedable_mode: true,
        streamlines: true,
        particles: false,
        particles_capable: true,
        probes_supported: true,
        left_click_action: LeftClickAction::SeedStreamline,
        scalar_place_probe: false,
        inspect: false,
        sticky: true,
    }
}

fn option(view: &viewport_tool::ToolView, tool: ViewportTool) -> viewport_tool::ToolOption {
    view.options
        .iter()
        .copied()
        .find(|option| option.tool == tool)
        .expect("every tool is listed")
}

#[test]
fn a_ready_seed_tool_is_selected_from_the_real_action() {
    let view = viewport_tool::derive(inputs());
    assert_eq!(view.selected, ViewportTool::SeedStreamline);
    assert_eq!(
        option(&view, ViewportTool::SeedStreamline).availability,
        ToolAvailability::Ready
    );
    assert_eq!(
        option(&view, ViewportTool::Inspect).availability,
        ToolAvailability::Ready
    );
    assert!(view.sticky);
}

#[test]
fn add_particles_selected_while_particles_is_off_is_never_the_effective_tool() {
    let mut input = inputs();
    input.left_click_action = LeftClickAction::AddParticles;
    input.particles = false;
    let view = viewport_tool::derive(input);
    assert_eq!(
        view.selected,
        ViewportTool::Inspect,
        "the documented mismatch must be impossible: Add particles is not shown as selected while off"
    );
    assert_eq!(
        option(&view, ViewportTool::AddParticles).availability,
        ToolAvailability::EnableThenPlace(CLICK_HINT_PARTICLES_OFF),
        "the off prerequisite is offered as an explicit enable-and-place action"
    );

    input.particles = true;
    let view = viewport_tool::derive(input);
    assert_eq!(view.selected, ViewportTool::AddParticles);
    assert_eq!(
        option(&view, ViewportTool::AddParticles).availability,
        ToolAvailability::Ready
    );
}

#[test]
fn streamlines_off_is_an_enable_and_place_option_not_a_selected_tool() {
    let mut input = inputs();
    input.streamlines = false;
    let view = viewport_tool::derive(input);
    assert_eq!(view.selected, ViewportTool::Inspect);
    assert_eq!(
        option(&view, ViewportTool::SeedStreamline).availability,
        ToolAvailability::EnableThenPlace(CLICK_HINT_STREAMLINES_OFF)
    );
}

#[test]
fn an_ordinary_2d_field_disables_particles_with_a_specific_reason() {
    let mut input = inputs();
    input.left_click_action = LeftClickAction::AddParticles;
    input.particles_capable = false;
    let view = viewport_tool::derive(input);
    assert_eq!(view.selected, ViewportTool::Inspect);
    assert_eq!(
        option(&view, ViewportTool::AddParticles).availability,
        ToolAvailability::Disabled("Particles need a 3D field")
    );
}

#[test]
fn modes_that_never_seed_disable_both_placement_tools() {
    let mut input = inputs();
    input.seedable_mode = false;
    let view = viewport_tool::derive(input);
    assert_eq!(view.selected, ViewportTool::Inspect);
    for tool in [ViewportTool::SeedStreamline, ViewportTool::AddParticles] {
        assert!(matches!(
            option(&view, tool).availability,
            ToolAvailability::Disabled(_)
        ));
    }
}

#[test]
fn plot_mode_disables_every_vector_tool() {
    let mut input = inputs();
    input.vector_mode = false;
    let view = viewport_tool::derive(input);
    for tool in [
        ViewportTool::PlaceProbe,
        ViewportTool::SeedStreamline,
        ViewportTool::AddParticles,
    ] {
        assert!(matches!(
            option(&view, tool).availability,
            ToolAvailability::Disabled(_)
        ));
    }
}

#[test]
fn an_armed_probe_or_inspect_choice_wins_over_the_left_click_action() {
    let mut input = inputs();
    input.scalar_place_probe = true;
    assert_eq!(
        viewport_tool::derive(input).selected,
        ViewportTool::PlaceProbe
    );

    input.scalar_place_probe = false;
    input.inspect = true;
    assert_eq!(viewport_tool::derive(input).selected, ViewportTool::Inspect);

    input.probes_supported = false;
    input.scalar_place_probe = true;
    assert_eq!(
        viewport_tool::derive(input).selected,
        ViewportTool::Inspect,
        "an armed probe cannot be selected when the view cannot place probes"
    );
}

#[test]
fn a_stale_armed_probe_selects_inspect_instead_of_advertising_a_seed_tool() {
    // The probe arm is the click that is really queued. If its path is gone it
    // is unavailable, so the strip must not claim a seed action that the click
    // will not perform.
    let mut input = inputs();
    input.scalar_place_probe = true;
    input.probes_supported = false;
    input.streamlines = true;
    input.particles = true;
    for action in [
        LeftClickAction::SeedStreamline,
        LeftClickAction::AddParticles,
    ] {
        input.left_click_action = action;
        let view = viewport_tool::derive(input);
        assert_eq!(
            view.selected,
            ViewportTool::Inspect,
            "a stale armed probe must not advertise {action:?}"
        );
    }
}

#[test]
fn the_default_placement_mode_keeps_placing() {
    assert!(
        Controls::new().viewport_sticky,
        "issue #228 preserves the historical repeated-click default"
    );
    let state = State::new(false);
    assert!(
        viewport_tool::derive_for(&state, &Controls::new()).sticky,
        "the strip inherits the Keep placing default"
    );
}

#[test]
fn the_sticky_choice_is_carried_through_derivation() {
    let mut input = inputs();
    input.sticky = false;
    assert!(!viewport_tool::derive(input).sticky);
}

// ---------------------------------------------------------------- app paths

fn frame(ctx: &egui::Context, app: &mut VfApp, events: Vec<egui::Event>) {
    let mut native = eframe::Frame::_new_kittest();
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 800.0),
            )),
            events,
            ..Default::default()
        },
        |ui| eframe::App::ui(app, ui, &mut native),
    );
    output.textures_delta.clear();
}

fn key(key: egui::Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

fn viewport() -> egui::Rect {
    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 400.0))
}

fn ready_seed_app() -> VfApp {
    let mut app = VfApp::new();
    app.state.set_formula("y, -x, 1".into());
    app.state.set_streamlines(true);
    app.controls.streamlines = true;
    app.controls.viewport_inspect = false;
    app.controls.left_click_action = LeftClickAction::SeedStreamline;
    app
}

#[test]
fn one_shot_placement_performs_one_action_then_returns_to_inspect() {
    let mut app = ready_seed_app();
    app.controls.viewport_sticky = false;
    let rect = viewport();
    let aspect = 1.0;

    app.test_seed_click(rect.center(), rect, aspect, 0.0);
    assert_eq!(app.test_harness().click_seed_count(), 1);
    assert!(app.controls.viewport_inspect, "one-shot returns to Inspect");

    app.test_seed_click(rect.center() + egui::vec2(80.0, 0.0), rect, aspect, 0.0);
    assert_eq!(
        app.test_harness().click_seed_count(),
        1,
        "a second click after the one-shot placement does nothing placed-related"
    );
}

#[test]
fn sticky_placement_keeps_the_tool_across_two_clicks() {
    let mut app = ready_seed_app();
    app.controls.viewport_sticky = true;
    let rect = viewport();
    let aspect = 1.0;

    app.test_seed_click(rect.center(), rect, aspect, 0.0);
    app.test_seed_click(rect.center() + egui::vec2(80.0, 0.0), rect, aspect, 0.0);
    assert_eq!(app.test_harness().click_seed_count(), 2);
    assert!(
        !app.controls.viewport_inspect,
        "sticky keeps the tool selected"
    );
}

/// Select the strip's Place probe the way its button does, so these tests cover
/// the tool the user actually picks rather than a raw control flag.
fn probe_app(sticky: bool) -> VfApp {
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    app.controls.viewport_sticky = sticky;
    let view = viewport_tool::derive_for(&app.state, &app.controls);
    viewport_tool::activate(ViewportTool::PlaceProbe, &view, &mut app.controls);
    app
}

fn probe_viewport() -> egui::Rect {
    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 700.0))
}

#[test]
fn sticky_probe_placement_stays_armed_for_a_second_probe() {
    let mut app = probe_app(true);
    let rect = probe_viewport();
    let aspect = rect.width() / rect.height();

    app.test_seed_click(rect.center(), rect, aspect, 0.0);
    assert_eq!(app.state.probes.points.len(), 1, "the first click places");
    assert!(
        app.controls.scalar_place_probe,
        "Keep placing leaves the probe armed for the next click"
    );
    assert!(
        !app.controls.viewport_inspect,
        "Keep placing does not fall back to Inspect"
    );

    app.test_seed_click(rect.center() + egui::vec2(40.0, 0.0), rect, aspect, 0.0);
    assert_eq!(
        app.state.probes.points.len(),
        2,
        "the second sticky click places another probe"
    );
}

#[test]
fn one_shot_probe_placement_arms_once_then_inspects() {
    let mut app = probe_app(false);
    let rect = probe_viewport();
    let aspect = rect.width() / rect.height();

    app.test_seed_click(rect.center(), rect, aspect, 0.0);
    assert_eq!(app.state.probes.points.len(), 1, "the first click places");
    assert!(
        !app.controls.scalar_place_probe,
        "one-shot disarms after a successful placement"
    );
    assert!(
        app.controls.viewport_inspect,
        "one-shot returns to Inspect after one placement"
    );

    app.test_seed_click(rect.center() + egui::vec2(40.0, 0.0), rect, aspect, 0.0);
    assert_eq!(
        app.state.probes.points.len(),
        1,
        "the second click places nothing"
    );
}

#[test]
fn a_sidebar_probe_arm_stays_one_shot_under_the_sticky_default() {
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    assert!(
        app.controls.viewport_sticky,
        "the strip default is Keep placing"
    );
    // The Scalar section's "Place probe" checkbox sets this arming flag.
    app.controls.scalar_place_probe = true;
    app.controls.viewport_armed_one_shot = true;

    let rect = probe_viewport();
    let aspect = rect.width() / rect.height();
    app.test_seed_click(rect.center(), rect, aspect, 0.0);
    assert_eq!(app.state.probes.points.len(), 1, "the checkbox places once");
    assert!(
        !app.controls.scalar_place_probe,
        "the standalone checkbox disarms after one probe"
    );
    assert!(
        app.controls.viewport_inspect,
        "the standalone checkbox returns to Inspect after one probe"
    );

    app.test_seed_click(rect.center() + egui::vec2(40.0, 0.0), rect, aspect, 0.0);
    assert_eq!(
        app.state.probes.points.len(),
        1,
        "the sticky default does not keep the sidebar checkbox armed"
    );
}

#[test]
fn a_cap_rejected_sidebar_arm_clears_the_one_shot_marker() {
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    for _ in 0..MAX_PROBES {
        app.state
            .add_point_probe(glam::Vec3::new(0.1, 0.1, 0.0), None)
            .expect("room for the capped test probes");
    }
    app.controls.scalar_place_probe = true;
    app.controls.viewport_armed_one_shot = true;

    let rect = probe_viewport();
    let aspect = rect.width() / rect.height();
    app.test_seed_click(rect.center(), rect, aspect, 0.0);
    assert_eq!(
        app.state.probes.points.len(),
        MAX_PROBES,
        "the cap rejects the placement"
    );
    assert!(
        app.controls.scalar_message.contains("at most 8"),
        "the cap rejection is reported: {:?}",
        app.controls.scalar_message
    );
    assert!(
        !app.controls.scalar_place_probe,
        "a cap rejection disarms the probe"
    );
    assert!(
        !app.controls.viewport_armed_one_shot,
        "the cancelled one-shot marker must not leak"
    );

    // The next click is not forced to Inspect/one-shot by the stale arm.
    app.state.set_formula("y, -x, 1".into());
    app.state.set_streamlines(true);
    app.controls.streamlines = true;
    app.controls.left_click_action = LeftClickAction::SeedStreamline;
    app.controls.viewport_inspect = false;
    app.test_seed_click(rect.center(), rect, aspect, 0.0);
    assert_eq!(
        app.test_harness().click_seed_count(),
        1,
        "the next click seeds normally"
    );
    assert!(
        !app.controls.viewport_inspect,
        "the stale marker does not force the next placement to Inspect"
    );
}

#[test]
fn an_outside_domain_probe_click_keeps_the_arm_for_a_retry() {
    let mut app = probe_app(true);
    let rect = probe_viewport();
    let aspect = rect.width() / rect.height();

    app.test_seed_click(rect.left_top() + egui::vec2(1.0, 1.0), rect, aspect, 0.0);
    assert!(
        app.state.probes.points.is_empty(),
        "an outside-domain pick places nothing"
    );
    assert!(
        app.controls.scalar_place_probe,
        "the miss stays armed so the user can retry"
    );
    assert!(
        app.controls
            .scalar_message
            .contains("inside the scene domain"),
        "the miss is reported: {:?}",
        app.controls.scalar_message
    );

    app.test_seed_click(rect.center(), rect, aspect, 0.0);
    assert_eq!(
        app.state.probes.points.len(),
        1,
        "the retry places the probe"
    );
}

#[test]
fn escape_returns_to_inspect_and_cancels_an_armed_one_shot() {
    let ctx = support::context();
    let mut app = ready_seed_app();
    app.layout.help_visible = false;
    app.controls.viewport_sticky = false;
    frame(&ctx, &mut app, Vec::new());

    frame(&ctx, &mut app, vec![key(egui::Key::Escape)]);
    assert!(app.controls.viewport_inspect, "Escape selects Inspect");

    let rect = viewport();
    app.test_seed_click(rect.center(), rect, 1.0, 0.0);
    assert_eq!(
        app.test_harness().click_seed_count(),
        0,
        "the cancelled one-shot places nothing"
    );
}

#[test]
fn enable_and_place_turns_the_prerequisite_on_and_places_once() {
    let ctx = support::context();
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    app.state.set_formula("y, -x, 1".into());
    app.state.set_streamlines(false);
    app.controls.streamlines = false;
    app.controls.viewport_sticky = true;

    let view = viewport_tool::derive_for(&app.state, &app.controls);
    assert_eq!(view.selected, ViewportTool::Inspect);
    assert_eq!(
        option(&view, ViewportTool::SeedStreamline).availability,
        ToolAvailability::EnableThenPlace(CLICK_HINT_STREAMLINES_OFF)
    );
    viewport_tool::activate(ViewportTool::SeedStreamline, &view, &mut app.controls);
    assert!(app.controls.streamlines, "the prerequisite is turned on");
    assert!(app.controls.viewport_armed_one_shot);

    frame(&ctx, &mut app, Vec::new());
    assert!(app.state.streamlines, "the seam enabled Streamlines");

    let rect = viewport();
    app.test_seed_click(rect.center(), rect, 1.0, 0.0);
    assert_eq!(app.test_harness().click_seed_count(), 1);
    app.test_seed_click(rect.center() + egui::vec2(80.0, 0.0), rect, 1.0, 0.0);
    assert_eq!(
        app.test_harness().click_seed_count(),
        1,
        "enable-and-place arms exactly one placement even in sticky mode"
    );
}

#[test]
fn enable_and_place_adds_one_particle_burst_then_inspects() {
    let ctx = support::context();
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    app.state.set_formula("x, y, 1".into());
    app.controls.particles = false;
    app.controls.left_click_action = LeftClickAction::AddParticles;
    app.controls.viewport_sticky = true;

    let view = viewport_tool::derive_for(&app.state, &app.controls);
    assert_eq!(
        option(&view, ViewportTool::AddParticles).availability,
        ToolAvailability::EnableThenPlace(CLICK_HINT_PARTICLES_OFF)
    );
    viewport_tool::activate(ViewportTool::AddParticles, &view, &mut app.controls);
    assert!(app.controls.particles, "Particles is enabled");
    frame(&ctx, &mut app, Vec::new());
    assert!(app.state.particles);

    let rect = viewport();
    app.test_seed_click(rect.center(), rect, 1.0, 0.0);
    let after_first = app.test_harness().particle_snapshot().0.len();
    assert!(after_first > 0, "the armed click adds a burst");
    assert!(
        app.controls.viewport_inspect,
        "enable-and-place is one shot"
    );
    app.test_seed_click(rect.center() + egui::vec2(80.0, 0.0), rect, 1.0, 0.0);
    assert_eq!(
        app.test_harness().particle_snapshot().0.len(),
        after_first,
        "the second click places nothing more"
    );
}

#[test]
fn enable_and_place_does_nothing_when_the_prerequisite_cannot_be_enabled() {
    let mut app = VfApp::new();
    app.state.set_formula("x, y".into());
    app.controls.particles = false;
    app.controls.left_click_action = LeftClickAction::AddParticles;

    let view = viewport_tool::derive_for(&app.state, &app.controls);
    assert_eq!(
        option(&view, ViewportTool::AddParticles).availability,
        ToolAvailability::Disabled("Particles need a 3D field")
    );
    viewport_tool::activate(ViewportTool::AddParticles, &view, &mut app.controls);
    assert!(!app.controls.particles, "no prerequisite is invented");

    let rect = viewport();
    app.test_seed_click(rect.center(), rect, 1.0, 0.0);
    assert_eq!(app.test_harness().click_seed_count(), 0);
    assert!(app.test_harness().particle_snapshot().0.is_empty());
}

// ------------------------------------------------------------- layout and keys

struct WidgetNode {
    label: String,
}

fn nodes(output: &egui::FullOutput) -> Vec<WidgetNode> {
    let Some(update) = output.platform_output.accesskit_update.as_ref() else {
        return Vec::new();
    };
    update
        .nodes
        .iter()
        .map(|(_, node)| {
            let label = node
                .label()
                .or_else(|| node.value())
                .unwrap_or_default()
                .to_owned();
            WidgetNode { label }
        })
        .collect()
}

fn node_rects(output: &egui::FullOutput, label: &str) -> Vec<egui::Rect> {
    let Some(update) = output.platform_output.accesskit_update.as_ref() else {
        return Vec::new();
    };
    update
        .nodes
        .iter()
        .filter_map(|(_, node)| {
            (node.label() == Some(label) || node.value() == Some(label))
                .then_some(node.bounds())
                .flatten()
        })
        .map(|bounds| {
            egui::Rect::from_min_max(
                egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
            )
        })
        .collect()
}

fn pointer(pos: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
    pointer_button(pos, egui::PointerButton::Primary, pressed)
}

fn pointer_button(pos: egui::Pos2, button: egui::PointerButton, pressed: bool) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(pos),
        egui::Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: egui::Modifiers::NONE,
        },
    ]
}

fn frame_with_width(
    ctx: &egui::Context,
    app: &mut VfApp,
    width: f32,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    frame_with_size(ctx, app, width, 900.0, events)
}

fn frame_with_size(
    ctx: &egui::Context,
    app: &mut VfApp,
    width: f32,
    height: f32,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let mut native = eframe::Frame::_new_kittest();
    frame_with_native(ctx, app, &mut native, egui::vec2(width, height), events)
}

fn frame_with_native(
    ctx: &egui::Context,
    app: &mut VfApp,
    native: &mut eframe::Frame,
    size: egui::Vec2,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            events,
            ..Default::default()
        },
        |ui| eframe::App::ui(app, ui, native),
    );
    output.textures_delta.clear();
    output
}

#[cfg(not(target_arch = "wasm32"))]
fn gpu_test_frame() -> Option<eframe::Frame> {
    use eframe::{egui_wgpu, wgpu};

    if std::env::var_os("VFE_VIEWPORT_GPU_PROBE").as_deref() != Some(std::ffi::OsStr::new("1")) {
        eprintln!("GPU not requested; set VFE_VIEWPORT_GPU_PROBE=1 on a GPU host");
        return None;
    }

    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
    descriptor.backends.remove(wgpu::Backends::NOOP);
    let instance = wgpu::Instance::new(descriptor);
    let mut native = eframe::Frame::_new_kittest();
    native.wgpu_render_state = Some(
        pollster::block_on(egui_wgpu::RenderState::create(
            &egui_wgpu::WgpuConfiguration::default(),
            &instance,
            None,
            egui_wgpu::RendererOptions::default(),
        ))
        .expect("real headless wgpu render state"),
    );
    Some(native)
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn secondary_click_opens_the_vector_viewport_gesture_menu() {
    let Some(mut native) = gpu_test_frame() else {
        return;
    };
    let ctx = support::context();
    ctx.enable_accesskit();
    let mut app = ready_seed_app();
    app.layout.help_visible = false;
    let mut frame = |events| {
        frame_with_native(
            &ctx,
            &mut app,
            &mut native,
            egui::vec2(1200.0, 900.0),
            events,
        )
    };
    for _ in 0..3 {
        frame(vec![]);
    }

    let field = egui::pos2(450.0, 450.0);
    frame(pointer_button(field, egui::PointerButton::Secondary, true));
    frame(pointer_button(field, egui::PointerButton::Secondary, false));
    let out = frame(vec![]);
    assert!(
        !node_rects(&out, "Fit view F").is_empty(),
        "an unconsumed secondary click opens the viewport menu with its shortcut hint"
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn dismissing_the_viewport_menu_does_not_also_seed_or_reset() {
    let Some(mut native) = gpu_test_frame() else {
        return;
    };
    let ctx = support::context();
    ctx.enable_accesskit();
    let mut app = ready_seed_app();
    app.layout.help_visible = false;
    app.controls.viewport_sticky = false;
    app.orbit.yaw += 0.4;
    let pose = app.orbit;
    let mut frame = |app: &mut VfApp, events| {
        frame_with_native(&ctx, app, &mut native, egui::vec2(1200.0, 900.0), events)
    };
    for _ in 0..3 {
        frame(&mut app, vec![]);
    }
    let menu_anchor = egui::pos2(600.0, 320.0);
    let secondary = egui::PointerButton::Secondary;
    frame(&mut app, pointer_button(menu_anchor, secondary, true));
    frame(&mut app, pointer_button(menu_anchor, secondary, false));
    let menu = frame(&mut app, vec![]);
    assert!(!node_rects(&menu, "Fit view F").is_empty());

    let outside = egui::pos2(450.0, 450.0);
    frame(&mut app, pointer(outside, true));
    frame(&mut app, pointer(outside, false));
    let out = frame(&mut app, vec![]);
    assert!(node_rects(&out, "Fit view F").is_empty());
    assert_eq!(app.test_harness().click_seed_count(), 0, "dismissal only");
    assert_eq!(app.orbit, pose);
    assert!(!app.controls.viewport_inspect);

    frame(&mut app, pointer(outside, true));
    frame(&mut app, pointer(outside, false));
    assert_eq!(app.test_harness().click_seed_count(), 1);
    assert!(app.controls.viewport_inspect);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn navigating_the_viewport_menu_does_not_apply_camera_keys() {
    let Some(mut native) = gpu_test_frame() else {
        return;
    };
    let ctx = support::context();
    ctx.enable_accesskit();
    let mut app = ready_seed_app();
    app.layout.help_visible = false;
    let mut frame = |app: &mut VfApp, events| {
        frame_with_native(&ctx, app, &mut native, egui::vec2(1200.0, 900.0), events)
    };
    for _ in 0..3 {
        frame(&mut app, vec![]);
    }
    let field = egui::pos2(450.0, 450.0);
    let secondary = egui::PointerButton::Secondary;
    frame(&mut app, pointer_button(field, secondary, true));
    frame(&mut app, pointer_button(field, secondary, false));
    let menu = frame(&mut app, vec![]);
    assert!(!node_rects(&menu, "Fit view F").is_empty());

    app.orbit.target.x = 0.2;
    let pose = app.orbit;
    frame(&mut app, vec![key(egui::Key::ArrowDown)]);
    assert_eq!(app.orbit, pose, "menu navigation does not pan the field");
    frame(&mut app, vec![]);
    frame(&mut app, vec![key(egui::Key::Enter)]);
    assert_eq!(app.orbit.target.x, 0.0, "Enter selects Fit");
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn viewport_gesture_menu_routes_view_and_panel_actions_in_wide_and_compact() {
    use vector_field_explorer::camera::OrbitCamera;
    use vector_field_explorer::ui::layout::SheetState;

    let Some(mut native) = gpu_test_frame() else {
        return;
    };

    for width in [1200.0, 500.0] {
        let ctx = support::context();
        ctx.enable_accesskit();
        let mut app = ready_seed_app();
        app.layout.help_visible = false;
        app.layout.controls_expanded = false;
        app.layout.appearance_expanded = false;
        app.style.reduced_motion = true;
        app.state.set_mode(true);
        app.orbit.yaw += 0.4;
        app.orbit.distance = 30.0;
        let tilted = app.orbit.yaw;
        let field = egui::pos2(width * 0.5, 370.0);
        let mut frame = |app: &mut VfApp, events| {
            frame_with_native(&ctx, app, &mut native, egui::vec2(width, 900.0), events)
        };
        for _ in 0..3 {
            frame(&mut app, vec![]);
        }

        let mut choose = |app: &mut VfApp, label| {
            frame(
                app,
                pointer_button(field, egui::PointerButton::Secondary, true),
            );
            frame(
                app,
                pointer_button(field, egui::PointerButton::Secondary, false),
            );
            let menu = frame(app, vec![]);
            let entry = node_rects(&menu, label)
                .into_iter()
                .min_by(|a, b| {
                    a.center()
                        .distance_sq(field)
                        .total_cmp(&b.center().distance_sq(field))
                })
                .unwrap_or_else(|| panic!("width {width}: gesture menu entry {label}"));
            frame(app, pointer(entry.center(), true));
            frame(app, pointer(entry.center(), false));
            let out = frame(app, vec![]);
            assert!(
                node_rects(&out, "Fit view F").is_empty(),
                "selection closes the menu"
            );
        };
        choose(&mut app, "Fit view F");
        assert!(app.orbit.distance < 30.0, "Fit frames the domain");
        assert_eq!(app.orbit.yaw, tilted, "Fit keeps the current orbit angle");

        choose(&mut app, "Reset view R");
        assert_eq!(app.orbit.yaw, OrbitCamera::default().yaw);
        assert_eq!(app.orbit.distance, OrbitCamera::default().distance);

        choose(&mut app, "Controls");
        if width == 500.0 {
            assert_eq!(app.sheet, SheetState::Controls);
        } else {
            assert!(app.layout.controls_expanded);
        }

        choose(&mut app, "Appearance");
        if width == 500.0 {
            assert_eq!(app.sheet, SheetState::Appearance);
        } else {
            assert!(app.layout.appearance_expanded);
        }
        assert_eq!(
            app.test_harness().click_seed_count(),
            0,
            "gesture menu actions do not place the armed streamline seed"
        );
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn viewport_secondary_drag_and_consumed_secondary_clicks_do_not_open_the_gesture_menu() {
    use vector_field_explorer::fly_camera::FlyCamera;
    use vector_field_explorer::plot::PlotKind;

    let Some(mut native) = gpu_test_frame() else {
        return;
    };
    let ctx = support::context();
    ctx.enable_accesskit();
    let mut app = ready_seed_app();
    app.layout.help_visible = false;
    app.style.reduced_motion = true;
    app.state.set_mode(true);
    let field = egui::pos2(450.0, 450.0);
    let secondary = egui::PointerButton::Secondary;
    let mut frame = |app: &mut VfApp, events| {
        frame_with_native(&ctx, app, &mut native, egui::vec2(1200.0, 900.0), events)
    };
    for _ in 0..3 {
        frame(&mut app, vec![]);
    }

    let orbit = app.orbit;
    let end = field + egui::vec2(60.0, 30.0);
    frame(&mut app, pointer_button(field, secondary, true));
    frame(&mut app, vec![egui::Event::PointerMoved(end)]);
    frame(&mut app, pointer_button(end, secondary, false));
    let out = frame(&mut app, vec![]);
    assert_ne!(app.orbit.yaw, orbit.yaw, "secondary drag still orbits");
    assert!(node_rects(&out, "Fit view F").is_empty());

    let legend = ctx
        .memory(|memory| memory.area_rect(egui::Id::unique("viewport_color_legend")))
        .expect("production legend area");
    let legend_header = legend.right_top() + egui::vec2(-3.0, 3.0);
    let mut assert_no_menu = |app: &mut VfApp, pos, reason| {
        frame(app, pointer_button(pos, secondary, true));
        frame(app, pointer_button(pos, secondary, false));
        let out = frame(app, vec![]);
        assert!(node_rects(&out, "Fit view F").is_empty(), "{reason}");
    };
    assert_no_menu(
        &mut app,
        legend_header,
        "the floating legend consumes the gesture",
    );

    app.fly = Some(FlyCamera::from_orbit(&app.orbit, 1.0));
    assert_no_menu(&mut app, field, "captured fly input has no viewport menu");
    assert!(app.fly.is_some(), "the secondary click leaves fly active");

    app.state.set_plot_kind(PlotKind::Curve2D);
    assert_no_menu(&mut app, field, "scalar plot input has no vector menu");
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn dragging_the_app_legend_does_not_orbit_or_place_an_armed_seed() {
    let Some(mut native) = gpu_test_frame() else {
        return;
    };
    let ctx = support::context();
    let mut app = ready_seed_app();
    app.layout.help_visible = false;
    app.controls.viewport_sticky = false;
    let frame = |app: &mut VfApp, native: &mut eframe::Frame, events| {
        frame_with_native(&ctx, app, native, egui::vec2(1200.0, 900.0), events);
    };
    let legend_rect = || {
        ctx.memory(|memory| memory.area_rect(egui::Id::unique("viewport_color_legend")))
            .expect("production legend area")
    };
    for _ in 0..3 {
        frame(&mut app, &mut native, vec![]);
    }
    assert_eq!(
        viewport_tool::derive_for(&app.state, &app.controls).selected,
        ViewportTool::SeedStreamline
    );
    let before = legend_rect();
    let pose = app.orbit;
    let seeds = app.test_harness().click_seed_count();
    let start = before.right_top() + egui::vec2(-3.0, 3.0);
    let end = start + egui::vec2(-100.0, 75.0);
    for events in [
        pointer(start, true),
        vec![egui::Event::PointerMoved(end)],
        pointer(end, false),
        vec![],
    ] {
        frame(&mut app, &mut native, events);
        assert_eq!(app.orbit, pose);
        assert_eq!(app.test_harness().click_seed_count(), seeds);
        assert_eq!(
            viewport_tool::derive_for(&app.state, &app.controls).selected,
            ViewportTool::SeedStreamline
        );
    }
    assert!(legend_rect().left() < before.left() - 50.0);

    // The same production viewport must still accept camera and placement input
    // off the legend; a frame that took the renderer fallback would fail here.
    let field = egui::pos2(450.0, 450.0);
    let pan_end = field + egui::vec2(35.0, 15.0);
    frame(&mut app, &mut native, pointer(field, true));
    frame(
        &mut app,
        &mut native,
        vec![egui::Event::PointerMoved(pan_end)],
    );
    frame(&mut app, &mut native, pointer(pan_end, false));
    assert_ne!(
        app.orbit, pose,
        "an unobstructed viewport drag moves the camera"
    );
    assert_eq!(app.test_harness().click_seed_count(), seeds);
    frame(&mut app, &mut native, pointer(field, true));
    frame(&mut app, &mut native, pointer(field, false));
    assert_eq!(app.test_harness().click_seed_count(), seeds + 1);
    assert!(app.controls.viewport_inspect, "one-shot placement disarms");
}

#[test]
fn the_strip_is_present_in_wide_and_compact_layouts_and_clicks_place_once() {
    for width in [1200.0, 500.0] {
        let ctx = support::context();
        ctx.enable_accesskit();
        let mut app = ready_seed_app();
        app.layout.help_visible = false;
        app.controls.viewport_sticky = false;
        let _out = frame_with_width(&ctx, &mut app, width, Vec::new());
        let _ = frame_with_width(&ctx, &mut app, width, Vec::new());
        let out = frame_with_width(&ctx, &mut app, width, Vec::new());
        let nodes = nodes(&out);
        let labels: Vec<&str> = nodes.iter().map(|node| node.label.as_str()).collect();
        assert!(
            labels.iter().any(|label| label.starts_with("Next click:")),
            "width {width}: the viewport edge names the effective tool"
        );
        for label in ["Inspect", "Place probe", "Seed streamline"] {
            assert!(
                labels.contains(&label),
                "width {width}: missing tool {label}"
            );
        }
        assert!(
            labels
                .iter()
                .any(|label| label.starts_with("Add particles")),
            "width {width}: the particles tool is offered with its prerequisite reason"
        );

        let rect = viewport();
        app.test_seed_click(rect.center(), rect, 1.0, 0.0);
        assert_eq!(
            app.test_harness().click_seed_count(),
            1,
            "width {width}: one placement click performs exactly one action"
        );
    }
}

#[test]
fn a_full_frame_strip_click_selects_seed_streamline_and_one_placement_follows() {
    let ctx = support::context();
    ctx.enable_accesskit();
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    app.state.set_formula("y, -x, 1".into());
    app.state.set_streamlines(true);
    app.controls.streamlines = true;
    app.controls.viewport_inspect = true;
    app.controls.viewport_sticky = false;

    let _ = frame_with_width(&ctx, &mut app, 1200.0, Vec::new());
    let _ = frame_with_width(&ctx, &mut app, 1200.0, Vec::new());
    let out = frame_with_width(&ctx, &mut app, 1200.0, Vec::new());
    let button = node_rects(&out, "Seed streamline")
        .into_iter()
        .min_by(|a, b| a.center().y.total_cmp(&b.center().y))
        .expect("the strip offers Seed streamline in the full app frame");

    let _ = frame_with_width(&ctx, &mut app, 1200.0, pointer(button.center(), true));
    let _ = frame_with_width(&ctx, &mut app, 1200.0, pointer(button.center(), false));
    assert_eq!(
        app.controls.left_click_action,
        LeftClickAction::SeedStreamline,
        "the full-frame strip click selects the tool"
    );
    assert!(
        !app.controls.viewport_inspect,
        "selecting a tool leaves Inspect"
    );
    assert_eq!(
        app.test_harness().click_seed_count(),
        0,
        "selecting a tool in the strip places nothing by itself"
    );
    // The headless frame returns before the wgpu viewport body, so the
    // placement click enters through the documented production seam.
    let _ = frame_with_width(&ctx, &mut app, 1200.0, Vec::new());
    let rect = viewport();
    app.test_seed_click(rect.center(), rect, 1.0, 0.0);
    assert_eq!(
        app.test_harness().click_seed_count(),
        1,
        "one placement click after selecting the tool performs exactly one action"
    );
}

#[test]
fn a_one_shot_sidebar_probe_survives_changing_the_left_click_action() {
    let ctx = support::context();
    ctx.enable_accesskit();
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    assert!(
        app.controls.viewport_sticky,
        "the default placement mode is Keep placing"
    );

    let width = 1200.0;
    let height = 4000.0;
    let mut out = frame_with_size(&ctx, &mut app, width, height, Vec::new());
    for _ in 0..3 {
        out = frame_with_size(&ctx, &mut app, width, height, Vec::new());
    }

    // 1. Open the Scalar section.
    let header = node_rects(&out, "Scalar view")
        .into_iter()
        .min_by(|a, b| a.left().total_cmp(&b.left()))
        .expect("the Scalar view header is exposed");
    for pressed in [true, false] {
        let _ = frame_with_size(
            &ctx,
            &mut app,
            width,
            height,
            pointer(header.center(), pressed),
        );
    }
    for _ in 0..20 {
        out = frame_with_size(&ctx, &mut app, width, height, Vec::new());
    }

    // 2. Check the standalone Place probe checkbox, which is a one-shot arm.
    let checkbox = node_rects(&out, "Place probe")
        .into_iter()
        .min_by(|a, b| a.left().total_cmp(&b.left()))
        .expect("the Scalar Place probe checkbox is exposed");
    for pressed in [true, false] {
        let _ = frame_with_size(
            &ctx,
            &mut app,
            width,
            height,
            pointer(checkbox.center(), pressed),
        );
    }
    assert!(app.controls.scalar_place_probe, "the checkbox arms a probe");
    assert!(
        app.controls.viewport_armed_one_shot,
        "the checkbox arms exactly one placement"
    );

    // 3. Change the sidebar left-click action. This must not convert the
    // one-shot probe arm into a sticky probe tool.
    out = frame_with_size(&ctx, &mut app, width, height, Vec::new());
    let add_particles = node_rects(&out, "Add particles")
        .into_iter()
        .min_by(|a, b| a.left().total_cmp(&b.left()))
        .expect("the sidebar Add particles selector is exposed");
    for pressed in [true, false] {
        let _ = frame_with_size(
            &ctx,
            &mut app,
            width,
            height,
            pointer(add_particles.center(), pressed),
        );
    }
    assert_eq!(
        app.controls.left_click_action,
        LeftClickAction::AddParticles,
        "the sidebar selector changed the left-click action"
    );
    assert!(app.controls.scalar_place_probe, "the probe stays armed");
    assert!(
        app.controls.viewport_armed_one_shot,
        "changing the left-click action must preserve the one-shot probe marker"
    );

    // 4. One click places exactly one probe, applies no seed or particle, and
    // returns to Inspect. A second click places nothing.
    let rect = probe_viewport();
    let aspect = rect.width() / rect.height();
    app.test_seed_click(rect.center(), rect, aspect, 0.0);
    assert_eq!(app.state.probes.points.len(), 1, "one probe is placed");
    assert_eq!(
        app.test_harness().click_seed_count(),
        0,
        "the armed probe click does not also seed"
    );
    assert!(
        app.test_harness().particle_snapshot().0.is_empty(),
        "the armed probe click does not also add a particle burst"
    );
    assert!(
        !app.controls.scalar_place_probe,
        "the one-shot probe disarms after one placement"
    );
    assert!(
        app.controls.viewport_inspect,
        "the strip returns to Inspect after the one-shot placement"
    );

    app.test_seed_click(rect.center() + egui::vec2(40.0, 0.0), rect, aspect, 0.0);
    assert_eq!(
        app.state.probes.points.len(),
        1,
        "a second click places no second probe"
    );
}

#[test]
fn tool_buttons_are_keyboard_focusable_and_enter_selects() {
    let ctx = support::context();
    let mut state = State::new(false);
    state.set_formula("y, -x, 1".into());
    state.set_streamlines(true);
    let mut controls = Controls::new();
    controls.streamlines = true;
    let bindings = Bindings::default_bindings();
    let view = viewport_tool::derive_for(&state, &controls);

    fn pass(
        ctx: &egui::Context,
        view: &viewport_tool::ToolView,
        controls: &mut Controls,
        bindings: &Bindings,
        events: Vec<egui::Event>,
    ) {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 600.0),
                )),
                events,
                ..Default::default()
            },
            |ui| viewport_tool::show_in_ui(ui, view, controls, bindings),
        );
        output.textures_delta.clear();
    }

    pass(&ctx, &view, &mut controls, &bindings, Vec::new());
    pass(
        &ctx,
        &view,
        &mut controls,
        &bindings,
        vec![key(egui::Key::Tab)],
    );
    pass(
        &ctx,
        &view,
        &mut controls,
        &bindings,
        vec![key(egui::Key::Enter)],
    );
    let view = viewport_tool::derive_for(&state, &controls);
    assert_eq!(
        view.selected,
        ViewportTool::Inspect,
        "Tab focuses the first tool button and Enter selects Inspect"
    );
}
