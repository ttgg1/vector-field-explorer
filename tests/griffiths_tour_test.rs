//! Issue #223 phase 1: the optional Griffiths branch from the quick-tour
//! welcome card and the Help window, plus its safe four-route picker.
//!
//! All four routes ship in this slice: the charged ring (#224), charged shell
//! (#225), grounded plane (#226) and surface-current wire (#227).

use eframe::egui;

use crate::support;
use vector_field_explorer::VfApp;
use vector_field_explorer::interaction::Bindings;
use vector_field_explorer::source_scene::FieldMode;
use vector_field_explorer::ui::griffiths::{
    GRIFFITHS_PICKER_TITLE, GRIFFITHS_RING_LABEL, GriffithsRoute,
};
use vector_field_explorer::ui::help_overlay::help_window_with_bindings;
use vector_field_explorer::ui::menu::MenuAction;

struct Harness {
    ctx: egui::Context,
    app: VfApp,
    width: f32,
    height: f32,
}

impl Harness {
    fn new(width: f32, height: f32) -> Self {
        let ctx = support::context();
        ctx.enable_accesskit();
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let mut app = VfApp::new();
        app.layout.help_visible = false;
        Self {
            ctx,
            app,
            width,
            height,
        }
    }

    fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        let mut frame = eframe::Frame::_new_kittest();
        let mut out = self.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(self.width, self.height),
                )),
                events,
                ..Default::default()
            },
            |ui| eframe::App::ui(&mut self.app, ui, &mut frame),
        );
        out.textures_delta.clear();
        out
    }

    fn idle(&mut self) -> egui::FullOutput {
        for _ in 0..3 {
            self.frame(Vec::new());
        }
        self.frame(Vec::new())
    }

    fn click(&mut self, pos: egui::Pos2) -> egui::FullOutput {
        let pointer = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        self.frame(vec![egui::Event::PointerMoved(pos), pointer(true)]);
        self.frame(vec![pointer(false)])
    }
}

fn texts(out: &egui::FullOutput) -> Vec<String> {
    out.shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::Text(shape) => Some(shape.galley.text().to_owned()),
            _ => None,
        })
        .collect()
}

fn node_rect(out: &egui::FullOutput, label: &str) -> Option<egui::Rect> {
    out.platform_output
        .accesskit_update
        .as_ref()?
        .nodes
        .iter()
        .find_map(|(_, node)| {
            (node.label() == Some(label) || node.value() == Some(label)).then(|| {
                let b = node.bounds().expect("widget bounds");
                egui::Rect::from_min_max(
                    egui::pos2(b.x0 as f32, b.y0 as f32),
                    egui::pos2(b.x1 as f32, b.y1 as f32),
                )
            })
        })
}

fn click_label(h: &mut Harness, label: &str) -> egui::FullOutput {
    let out = h.frame(Vec::new());
    let rect = node_rect(&out, label)
        .unwrap_or_else(|| panic!("`{label}` is a reachable button: {:?}", texts(&out)));
    h.click(rect.center())
}

#[test]
fn the_welcome_card_offers_the_optional_griffiths_branch() {
    let mut h = Harness::new(1200.0, 800.0);
    h.app
        .apply_menu_action_for_test(MenuAction::StartTour, &h.ctx);
    h.idle();
    let out = h.frame(Vec::new());
    assert!(
        node_rect(&out, "Try a Griffiths problem").is_some(),
        "the welcome card keeps the quick tour and adds the optional branch: {:?}",
        texts(&out)
    );

    let _ = click_label(&mut h, "Try a Griffiths problem");
    h.idle();
    let out = h.frame(Vec::new());
    let text = texts(&out).join("\n");
    assert!(
        h.app.griffiths_picker_open_for_test(),
        "the branch opens the route picker"
    );
    assert!(
        text.contains(GRIFFITHS_PICKER_TITLE),
        "the picker is visible: {text}"
    );
    for route in GriffithsRoute::ALL {
        assert!(
            text.contains(route.label()),
            "route {:?} is listed with its own label: {text}",
            route
        );
    }
    assert!(
        !text.contains("not available yet"),
        "every route has a shipped lesson: {text}"
    );
    assert!(
        GriffithsRoute::ALL.iter().all(|route| route.available()),
        "the picker offers all four shipped routes"
    );
}

#[test]
fn help_offers_the_branch_and_can_replay_it_after_a_lesson() {
    let mut h = Harness::new(1200.0, 800.0);
    h.app
        .apply_menu_action_for_test(MenuAction::ShowHelpTable, &h.ctx);
    h.idle();
    let out = h.frame(Vec::new());
    assert!(
        node_rect(&out, "Try a Griffiths problem").is_some(),
        "the Help window exposes the branch: {:?}",
        texts(&out)
    );

    // Open the ring route once and leave it. Re-entry through Help must offer
    // the branch again and start a fresh session.
    click_label(&mut h, "Try a Griffiths problem");
    h.idle();
    assert!(h.app.griffiths_picker_open_for_test());
    click_label(&mut h, GRIFFITHS_RING_LABEL);
    h.idle();
    assert!(h.app.ring_lesson_active_for_test());
    h.app.exit_griffiths_lesson();
    assert!(!h.app.ring_lesson_active_for_test());

    h.app
        .apply_menu_action_for_test(MenuAction::ShowHelpTable, &h.ctx);
    h.idle();
    let out = h.frame(Vec::new());
    let rect = node_rect(&out, "Try a Griffiths problem")
        .expect("the Help entry survives after first launch");
    h.click(rect.center());
    h.idle();
    click_label(&mut h, GRIFFITHS_RING_LABEL);
    h.idle();
    assert!(
        h.app.ring_lesson_active_for_test(),
        "the ring lesson is replayable from Help"
    );
}

#[test]
fn the_branch_and_ring_route_work_in_wide_and_compact_layouts() {
    for (width, height) in [(1200.0, 800.0), (360.0, 640.0)] {
        let mut h = Harness::new(width, height);
        h.idle();
        h.app.open_griffiths_branch();
        h.idle();
        let out = h.frame(Vec::new());
        let text = texts(&out).join("\n");
        assert!(
            text.contains(GRIFFITHS_PICKER_TITLE) && text.contains(GRIFFITHS_RING_LABEL),
            "{width} pt: the picker and ring route are reachable: {text}"
        );
        click_label(&mut h, GRIFFITHS_RING_LABEL);
        assert!(
            h.app.ring_lesson_active_for_test(),
            "{width} pt: the ring route opens"
        );
        assert_eq!(
            h.app.state.field_mode,
            FieldMode::Sources,
            "{width} pt: the ring scene is loaded through the Sources adapter"
        );
    }
}

#[test]
fn the_legacy_help_window_has_no_dead_griffiths_control() {
    // `help_window_with_bindings` cannot route the branch action, so it must
    // not draw the picker button at all. The app uses `help_window_with_branch`.
    let ctx = support::context();
    ctx.enable_accesskit();
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    let mut open = true;
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000.0, 4000.0),
            )),
            ..Default::default()
        },
        |ui| help_window_with_bindings(ui.ctx(), &mut open, &Bindings::default_bindings()),
    );
    output.textures_delta.clear();
    for _ in 0..3 {
        let mut next = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000.0, 4000.0),
                )),
                ..Default::default()
            },
            |ui| help_window_with_bindings(ui.ctx(), &mut open, &Bindings::default_bindings()),
        );
        next.textures_delta.clear();
        output = next;
    }
    let text = texts(&output).join("\n");
    assert!(
        text.contains("3D view"),
        "the legacy help window still renders its table: {text}"
    );
    assert!(
        !text.contains("Try a Griffiths problem"),
        "the legacy help window has no inert Griffiths control: {text}"
    );
}

#[test]
fn all_four_routes_are_available_and_the_plane_route_opens_its_lesson() {
    let mut h = Harness::new(1200.0, 800.0);
    h.idle();
    assert!(
        GriffithsRoute::ALL.iter().all(|route| route.available()),
        "the four-route picker has no unavailable route in this build"
    );
    h.app.open_griffiths_branch();
    h.app.start_griffiths_route(GriffithsRoute::Plane);
    assert!(
        !h.app.griffiths_picker_open_for_test(),
        "selecting the plane route closes the picker instead of showing a placeholder"
    );
    assert_eq!(
        h.app.state.maxwell.case,
        vector_field_explorer::maxwell::MaxwellCase::ImagePlane,
        "the plane route loads the shipped image-plane case"
    );
    h.idle();
    let text = texts(&h.frame(Vec::new())).join("\n");
    assert!(
        text.contains("Grounded plane"),
        "the plane activity is visible: {text}"
    );
    assert!(
        !text.contains("not available yet"),
        "no placeholder is shown for a shipped route: {text}"
    );
}
