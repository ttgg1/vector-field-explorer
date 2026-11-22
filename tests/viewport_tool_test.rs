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
