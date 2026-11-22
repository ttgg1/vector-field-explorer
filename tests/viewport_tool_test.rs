//! Issue #228: the viewport-edge click tool is derived from real state, supports
//! one-shot and sticky placement, enable-and-place, Escape and keyboard
//! selection, and keeps one placement click to exactly one action.

use eframe::egui;

use crate::support;

use vector_field_explorer::VfApp;
use vector_field_explorer::interaction::Bindings;
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
            (node.label() == Some(label))
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

fn frame_with_width(
    ctx: &egui::Context,
    app: &mut VfApp,
    width: f32,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let mut native = eframe::Frame::_new_kittest();
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, 900.0),
            )),
            events,
            ..Default::default()
        },
        |ui| eframe::App::ui(app, ui, &mut native),
    );
    output.textures_delta.clear();
    output
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
