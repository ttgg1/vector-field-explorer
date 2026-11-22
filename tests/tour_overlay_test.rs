//! Guided-tour overlay, keyboard and Help-entry coverage (#99 slice B).
//!
//! The model and stop table are covered by `tests/tour_test.rs`. These tests
//! drive the real app frame so the overlay, the session-only progress value,
//! the normal layout toggles and the keyboard routing are all exercised
//! together.

use eframe::egui;

use crate::support;
use vector_field_explorer::VfApp;
use vector_field_explorer::tour::{AdvanceRule, TOUR};
use vector_field_explorer::ui::menu::MenuAction;
use vector_field_explorer::ui::tour_overlay::advance_ready;

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
        // The persistent first-launch help overlay would add unrelated text to
        // every card assertion. It is independent of the tour.
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

    fn key(&mut self, key: egui::Key) -> egui::FullOutput {
        self.frame(vec![egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }])
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

    fn start(&mut self) {
        self.app
            .apply_menu_action_for_test(MenuAction::StartTour, &self.ctx);
    }
}

/// Move the live progress to the stop with `id` without painting a frame; the
/// next frame's prepare phase sees the changed index, exactly like Next/Back.
fn go_to(h: &mut Harness, id: &str) {
    let progress = h.app.tour.as_mut().expect("the tour is active");
    let target = TOUR
        .iter()
        .position(|stop| stop.id == id)
        .expect("the stop exists in the tour table");
    while progress.index() < target {
        progress.advance();
    }
    while progress.index() > target {
        progress.back();
    }
}

fn index(h: &Harness) -> usize {
    h.app
        .tour
        .as_ref()
        .expect("the tour is active or finished")
        .index()
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

fn text_input_value(out: &egui::FullOutput, value: &str) -> Option<egui::Rect> {
    out.platform_output
        .accesskit_update
        .as_ref()?
        .nodes
        .iter()
        .find_map(|(_, node)| {
            (node.role() == egui::Role::TextInput && node.value() == Some(value)).then(|| {
                let b = node.bounds().expect("text input bounds");
                egui::Rect::from_min_max(
                    egui::pos2(b.x0 as f32, b.y0 as f32),
                    egui::pos2(b.x1 as f32, b.y1 as f32),
                )
            })
        })
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

#[test]
fn flux_stop_anchors_to_the_real_integral_surface_control_in_wide_and_compact_layouts() {
    use vector_field_explorer::source_scene::FieldMode;

    for (width, height) in [(1200.0, 800.0), (360.0, 640.0)] {
        let mut h = Harness::new(width, height);
        // The app's real default is the Formula adapter, which cannot supply a
        // probe field. The probes stop must reach a probe-capable adapter
        // through the shipped mode switch without the test forcing Sources.
        assert_eq!(
            h.app.state.field_mode,
            FieldMode::Formula,
            "{width} pt: the test starts from the shipped default adapter"
        );
        h.start();
        h.idle();
        go_to(&mut h, "probes");
        // The tour opens the Controls panel/sheet, switches the adapter and
        // reveals the Field probes body; the scroll request then brings the
        // Add sphere row into view.
        for _ in 0..8 {
            h.frame(Vec::new());
        }
        let out = h.frame(Vec::new());
        assert_eq!(
            h.app.state.field_mode,
            FieldMode::Sources,
            "{width} pt: the probes stop switches to a probe-capable adapter"
        );
        let add = node_rect(&out, "Add sphere")
            .unwrap_or_else(|| panic!("{width} pt: the tour reveals the real Add sphere control"));
        let rect = h
            .app
            .tour_highlight_rect_for_test()
            .unwrap_or_else(|| panic!("{width} pt: the flux anchor resolves"));
        assert!(
            rect.width() > 0.0 && rect.height() > 0.0,
            "{width} pt: the highlight is non-empty"
        );
        assert!(
            (rect.center() - add.center()).length() < 2.0,
            "{width} pt: the highlight {rect:?} covers the Add sphere control {add:?}"
        );
    }
}

#[test]
fn probe_stop_explains_when_only_the_scalar_plot_view_is_available() {
    use vector_field_explorer::plot::PlotKind;

    let mut h = Harness::new(1200.0, 800.0);
    // The scalar plot view has no vector field, and switching the field adapter
    // alone cannot restore one. The card must name the next step instead of
    // presenting a dead Field probes section.
    h.app.state.set_plot_kind(PlotKind::Curve2D);
    h.start();
    h.idle();
    go_to(&mut h, "probes");
    let out = h.frame(Vec::new());
    let text = texts(&out).join("\n");
    assert!(
        text.contains("Switch back to the vector view to use Field probes."),
        "the card explains how to reach the probes: {text}"
    );
    assert!(
        h.app.tour_highlight_rect_for_test().is_none(),
        "an unavailable Field probes body produces no anchor"
    );
}

#[test]
fn an_absent_integral_control_does_not_fall_back_to_the_whole_panel() {
    let mut h = Harness::new(1200.0, 800.0);
    h.app
        .state
        .set_field_mode(vector_field_explorer::source_scene::FieldMode::Sources);
    h.start();
    h.idle();
    go_to(&mut h, "probes");
    for _ in 0..8 {
        h.frame(Vec::new());
    }
    assert!(
        h.app.tour_highlight_rect_for_test().is_some(),
        "the control is visible first"
    );

    // Closing the Controls panel leaves the control absent. The tour must not
    // fabricate a highlight from the panel rectangle or reopen the panel.
    h.app.layout.toggle_controls();
    for _ in 0..4 {
        h.frame(Vec::new());
    }
    assert!(
        h.app.tour_highlight_rect_for_test().is_none(),
        "an absent integral control produces no anchor"
    );
    assert!(
        !h.app.layout.controls_expanded,
        "the user's close is respected for the rest of the stop"
    );
}

#[test]
fn starting_the_tour_changes_only_the_probe_adapter_and_records_one_undoable_switch() {
    use vector_field_explorer::source_scene::FieldMode;

    let mut h = Harness::new(1200.0, 800.0);
    h.idle();

    let document = h.app.capture_document_for_test();
    assert_eq!(
        document.view.field_mode,
        FieldMode::Formula,
        "the default document starts in Formula"
    );
    let history = h.app.history_len();
    let redo = h.app.history_redo_len();

    h.start();
    assert_eq!(index(&h), 0);
    for _ in 0..TOUR.len() {
        h.frame(Vec::new());
        h.app.tour.as_mut().expect("tour progress").advance();
    }
    h.frame(Vec::new());
    assert!(
        h.app
            .tour
            .as_ref()
            .is_some_and(|progress| progress.is_finished()),
        "advancing past the last stop finishes the session tour"
    );

    let after = h.app.capture_document_for_test();
    // The probes stop deliberately switches to a probe-capable adapter through
    // the shipped `set_field_mode` seam. Nothing else in the view record may
    // change: the formula, domain, probes and every durable scene stay exact.
    let mut expected_view = document.view.clone();
    expected_view.field_mode = FieldMode::Sources;
    assert_eq!(
        after.view, expected_view,
        "the only view change is the documented probes-step adapter switch"
    );
    assert_eq!(
        after.parameters, document.parameters,
        "the tour must not mutate named parameters"
    );
    assert_eq!(
        after.appearance, document.appearance,
        "the tour must not mutate appearance"
    );
    // The adapter switch re-frames the camera for the new scene; the pose is
    // otherwise untouched.
    let mut expected_camera = document.camera.clone();
    expected_camera.scale = after.camera.scale;
    expected_camera.ortho_zoom = after.camera.ortho_zoom;
    assert_eq!(
        after.camera, expected_camera,
        "the adapter switch may re-frame but must not orbit or move the camera"
    );
    assert_eq!(
        h.app.history_len(),
        history + 1,
        "the one deliberate adapter switch is the only undo entry"
    );
    assert_eq!(
        h.app.history_redo_len(),
        redo,
        "walking the tour forward records no redo entry"
    );
}

#[test]
fn next_and_back_follow_the_stop_table() {
    let mut h = Harness::new(1200.0, 800.0);
    h.start();
    h.idle();
    assert_eq!(index(&h), 0);
    assert_eq!(
        h.app.tour.as_ref().unwrap().current().unwrap().id,
        TOUR[0].id
    );

    h.key(egui::Key::Enter);
    assert_eq!(index(&h), 1);
    assert_eq!(
        h.app.tour.as_ref().unwrap().current().unwrap().id,
        TOUR[1].id
    );

    h.key(egui::Key::ArrowRight);
    assert_eq!(index(&h), 2);
    assert_eq!(
        h.app.tour.as_ref().unwrap().current().unwrap().id,
        TOUR[2].id
    );

    // The fourth stop waits on `Changed("camera_fit")`; navigation still lands
    // on it, but Enter alone cannot move past it in this test.
    h.key(egui::Key::Enter);
    assert_eq!(index(&h), 3);
    assert_eq!(
        h.app.tour.as_ref().unwrap().current().unwrap().id,
        TOUR[3].id
    );

    h.key(egui::Key::ArrowLeft);
    assert_eq!(index(&h), 2);
    assert_eq!(
        h.app.tour.as_ref().unwrap().current().unwrap().id,
        TOUR[2].id
    );

    h.key(egui::Key::ArrowLeft);
    assert_eq!(index(&h), 1);
    assert_eq!(
        h.app.tour.as_ref().unwrap().current().unwrap().id,
        TOUR[1].id
    );
}

#[test]
fn escape_dismisses_and_the_menu_restarts_at_the_first_stop() {
    let mut h = Harness::new(1200.0, 800.0);
    h.start();
    h.idle();
    h.key(egui::Key::Enter);
    assert_eq!(index(&h), 1);

    let out = h.key(egui::Key::Escape);
    assert!(
        h.app.tour.as_ref().unwrap().is_finished(),
        "Escape ends the tour for the session"
    );
    assert_eq!(h.app.tour.as_ref().unwrap().current(), None);
    assert!(
        !texts(&out).iter().any(|text| text == TOUR[1].title),
        "the card is gone after dismissal"
    );

    h.app
        .apply_menu_action_for_test(MenuAction::StartTour, &h.ctx);
    assert_eq!(index(&h), 0);
    assert!(!h.app.tour.as_ref().unwrap().is_finished());
    assert_eq!(
        h.app.tour.as_ref().unwrap().current().unwrap().id,
        TOUR[0].id
    );
}

#[test]
fn changed_rule_advances_only_after_the_control_changes() {
    let rule = AdvanceRule::Changed("play");
    assert!(
        !advance_ready(rule, Some("false"), Some("false")),
        "an unchanged value keeps the stop"
    );
    assert!(
        advance_ready(rule, Some("false"), Some("true")),
        "a real change satisfies the stop"
    );
    assert!(
        !advance_ready(rule, None, Some("true")),
        "no snapshot is not a change"
    );
    assert!(
        !advance_ready(rule, Some("false"), None),
        "an unavailable value cannot satisfy the stop"
    );
    assert!(
        advance_ready(AdvanceRule::Next, None, None),
        "explicit Next stops are always ready"
    );
}

#[test]
fn opening_a_collapsed_panel_uses_the_normal_layout_toggle() {
    let mut h = Harness::new(1200.0, 800.0);
    h.app.layout.presets_expanded = false;
    h.idle();

    let mut expected = h.app.layout.clone();
    expected.toggle_presets();

    h.start();
    h.frame(Vec::new());
    h.key(egui::Key::Enter);
    assert_eq!(index(&h), 1, "the Presets stop is current");
    h.frame(Vec::new());

    assert!(
        h.app.layout.presets_expanded,
        "the tour opens the collapsed target panel"
    );
    assert_eq!(
        h.app.layout, expected,
        "the tour must go through LayoutState::toggle_presets, not a parallel path"
    );
}

#[test]
fn resolved_anchor_paints_a_highlight() {
    let mut h = Harness::new(1200.0, 800.0);
    h.start();
    h.idle();
    go_to(&mut h, "presets");
    h.frame(Vec::new());

    let rect = h
        .app
        .tour_highlight_rect_for_test()
        .expect("a resolved anchor paints a highlight");
    assert!(
        rect.width() > 0.0 && rect.height() > 0.0,
        "the highlight must cover the real panel rect, got {rect:?}"
    );
}

#[test]
fn closing_the_target_panel_keeps_it_closed_for_the_rest_of_the_stop() {
    let mut h = Harness::new(1200.0, 800.0);
    h.app.layout.presets_expanded = false;
    h.start();
    h.idle();

    go_to(&mut h, "presets");
    h.frame(Vec::new());
    assert!(
        h.app.layout.presets_expanded,
        "the tour opens the stop's panel on its first frame"
    );

    // The user closes the panel the tour opened. It must stay closed: the tour
    // only opens a target once per stop, never every frame.
    h.app.layout.toggle_presets();
    assert!(!h.app.layout.presets_expanded);
    for _ in 0..3 {
        h.frame(Vec::new());
    }
    assert!(
        !h.app.layout.presets_expanded,
        "closing the panel must not have it yanked back open while the stop is current"
    );
    assert_eq!(
        h.app.tour.as_ref().unwrap().current().unwrap().id,
        "presets",
        "the same stop is still current"
    );

    // A later stop that needs the same panel opens it again.
    go_to(&mut h, "saved_scenes");
    h.frame(Vec::new());
    assert!(
        h.app.layout.presets_expanded,
        "the next stop that needs Presets opens it again"
    );
}

#[test]
fn closing_the_compact_sheet_keeps_it_closed_for_the_rest_of_the_stop() {
    use vector_field_explorer::ui::layout::SheetState;

    let mut h = Harness::new(360.0, 640.0);
    h.start();
    h.idle();

    go_to(&mut h, "presets");
    h.frame(Vec::new());
    assert_eq!(
        h.app.sheet,
        SheetState::Presets,
        "compact mode opens the stop's sheet"
    );

    h.app.sheet.close();
    for _ in 0..3 {
        h.frame(Vec::new());
    }
    assert_eq!(
        h.app.sheet,
        SheetState::None,
        "the tour must not force the closed sheet open every frame"
    );

    go_to(&mut h, "saved_scenes");
    h.frame(Vec::new());
    assert_eq!(
        h.app.sheet,
        SheetState::Presets,
        "the next stop that needs Presets opens the sheet again"
    );
}

#[test]
fn camera_stop_continues_only_after_fit_domain() {
    let mut h = Harness::new(1200.0, 800.0);
    h.start();
    h.idle();
    go_to(&mut h, "viewport");
    h.frame(Vec::new());
    let viewport_index = index(&h);

    // Orbiting is exploration; the card asks for Fit domain to continue.
    h.app.orbit.yaw += 0.4;
    h.frame(Vec::new());
    h.key(egui::Key::Enter);
    assert_eq!(
        index(&h),
        viewport_index,
        "a free orbit must not continue the viewport stop"
    );

    // Fit domain produces the framing change the card names.
    h.key(egui::Key::F);
    h.key(egui::Key::Enter);
    assert_eq!(
        index(&h),
        viewport_index + 1,
        "Fit domain continues the viewport stop"
    );
}

#[test]
fn unresolved_anchor_still_shows_the_card_without_a_highlight() {
    let mut h = Harness::new(1200.0, 800.0);
    h.start();
    h.idle();

    // The Maxwell panel body only exists in the Maxwell adapter. The tour must
    // not switch adapters or invent a rectangle; it shows the card plain.
    while h.app.tour.as_ref().unwrap().current().unwrap().id != "maxwell" {
        h.app.tour.as_mut().unwrap().advance();
    }
    assert_ne!(
        h.app.state.field_mode,
        vector_field_explorer::source_scene::FieldMode::Maxwell
    );
    let out = h.frame(Vec::new());

    assert!(
        texts(&out)
            .iter()
            .any(|text| text == "Maxwell teaching case"),
        "the card still renders from the stop table"
    );
    assert!(
        h.app.tour_highlight_rect_for_test().is_none(),
        "an unresolved anchor must not fabricate a highlight"
    );
    let nodes = out
        .platform_output
        .accesskit_update
        .as_ref()
        .expect("accesskit is enabled for this harness");
    assert!(
        nodes
            .nodes
            .iter()
            .any(|(_, node)| node.role() == egui::Role::Dialog),
        "the card is exposed as a dialog"
    );
    assert!(
        nodes.nodes.iter().any(|(_, node)| {
            node.label() == Some("Maxwell teaching case")
                || node.value() == Some("Maxwell teaching case")
        }),
        "the card dialog carries its title"
    );
}

#[test]
fn compact_layout_keeps_the_card_on_screen() {
    let mut h = Harness::new(360.0, 640.0);
    h.start();
    h.idle();

    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(360.0, 640.0));
    for _ in 0..TOUR.len() {
        h.frame(Vec::new());
        let card = h
            .app
            .tour_card_rect_for_test()
            .expect("the card draws in the compact layout");
        assert!(
            screen.contains_rect(card),
            "card {card:?} must stay inside compact screen {screen:?}"
        );
        assert!(
            card.width() <= screen.width(),
            "the card must not be wider than a phone screen"
        );
        h.app.tour.as_mut().unwrap().advance();
    }
}

#[test]
fn tour_keys_do_not_reach_viewport_handlers_but_text_editing_still_works() {
    let mut h = Harness::new(1200.0, 800.0);
    h.start();
    h.idle();
    let orbit = h.app.orbit;

    h.key(egui::Key::ArrowRight);
    assert_eq!(index(&h), 1, "Right advances the tour instead of panning");
    assert_eq!(h.app.orbit, orbit, "the viewport must not pan");

    h.key(egui::Key::ArrowLeft);
    assert_eq!(index(&h), 0, "Left goes back instead of panning");
    assert_eq!(h.app.orbit, orbit, "the viewport must not pan back");

    // Focus the real formula editor through the accesskit tree, then confirm
    // tour keys keep their normal text-editing meaning.
    let out = h.idle();
    let formula = h.app.state.formula.clone();
    let rect = text_input_value(&out, &formula).expect("the formula editor is live");
    h.click(rect.center());
    h.idle();
    assert!(
        h.ctx.text_edit_focused(),
        "the formula editor owns the keyboard"
    );

    let before = index(&h);
    let formula_before = h.app.state.formula.clone();
    h.key(egui::Key::ArrowRight);
    assert_eq!(index(&h), before, "text editing keeps Right");
    assert_eq!(h.app.state.formula, formula_before);
    h.key(egui::Key::ArrowLeft);
    assert_eq!(index(&h), before, "text editing keeps Left");
    assert_eq!(h.app.state.formula, formula_before);
}

#[test]
fn help_menu_entry_is_labelled_start_or_restart_truthfully() {
    let help = vector_field_explorer::ui::menu::MENUS
        .iter()
        .find(|menu| menu.title == "Help")
        .expect("the Help menu exists");
    let item = help
        .items
        .iter()
        .find(|item| item.action == MenuAction::StartTour)
        .expect("Help carries the tour entry");
    assert_eq!(item.label, "Start or restart tour");
    assert!(item.label.contains("Start"));
    assert!(item.label.contains("restart"));

    let mut h = Harness::new(1200.0, 800.0);
    h.start();
    assert_eq!(index(&h), 0);
    h.app.tour.as_mut().unwrap().dismiss();
    h.app
        .apply_menu_action_for_test(MenuAction::StartTour, &h.ctx);
    assert_eq!(index(&h), 0, "the entry restarts a finished tour");
    assert!(!h.app.tour.as_ref().unwrap().is_finished());
}

#[test]
fn card_placement_avoids_the_target_and_stays_on_screen() {
    use vector_field_explorer::ui::tour_overlay::place_card;
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 800.0));
    let size = egui::vec2(340.0, 230.0);
    let targets = [
        egui::Rect::from_min_size(egui::pos2(0.0, 40.0), egui::vec2(260.0, 720.0)),
        egui::Rect::from_min_size(egui::pos2(940.0, 40.0), egui::vec2(260.0, 720.0)),
        egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1200.0, 86.0)),
        egui::Rect::from_min_size(egui::pos2(0.0, 760.0), egui::vec2(1200.0, 40.0)),
        egui::Rect::from_center_size(egui::pos2(600.0, 400.0), egui::vec2(480.0, 320.0)),
    ];
    for target in targets {
        let card = place_card(Some(target), screen, size);
        assert!(
            screen.contains_rect(card),
            "card {card:?} must stay on screen for target {target:?}"
        );
        assert!(
            !card.intersects(target),
            "card {card:?} must not cover target {target:?}"
        );
    }

    let unresolved = place_card(None, screen, size);
    assert!(screen.contains_rect(unresolved));
}
