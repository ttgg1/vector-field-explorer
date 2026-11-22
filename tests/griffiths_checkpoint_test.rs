//! Issue #235: explicit per-step controls in the Griffiths activities.
//!
//! The activities expose the current checkpoint, an honest one-step skip, a
//! voluntary hint and a temporary "Explore from here" excursion. Returning from
//! exploration lets the learner restore the checkpoint or keep their variation.

use eframe::egui;

use crate::support;
use vector_field_explorer::VfApp;
use vector_field_explorer::ui::griffiths::{
    GRIFFITHS_EXPLORE_FROM_HERE, GRIFFITHS_KEEP_VARIATION, GRIFFITHS_RESUME_CHECKPOINT,
    GRIFFITHS_RETURN_TO_ACTIVITY, GRIFFITHS_SHOW_HINT, GRIFFITHS_SKIP_STEP, GriffithsCheckpoint,
    GriffithsRoute,
};
use vector_field_explorer::ui::menu::MenuAction;
use vector_field_explorer::ui::ring_lesson::{RING_HINT, RingPrediction};
use vector_field_explorer::ui::wire_lesson::{WIRE_DIRECTION_HINT, WIRE_ORIENTATION_HINT};

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

    fn start_route(&mut self, route: GriffithsRoute) {
        self.idle();
        self.app.open_griffiths_branch();
        self.app.start_griffiths_route(route);
        self.idle();
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
        .unwrap_or_else(|| panic!("`{label}` is a reachable control: {:?}", texts(&out)));
    h.click(rect.center())
}

#[test]
fn skip_advances_exactly_one_checkpoint_without_revealing_the_result() {
    let mut h = Harness::new(1200.0, 800.0);
    h.start_route(GriffithsRoute::Ring);

    assert_eq!(
        h.app.griffiths_checkpoint_for_test(),
        Some(GriffithsCheckpoint::Predict)
    );
    let out = h.frame(Vec::new());
    let text = texts(&out).join("\n");
    assert!(
        text.contains("Checkpoint 1 of 3 · Make your prediction"),
        "the current checkpoint is explicit text: {text}"
    );
    assert!(
        node_rect(&out, GRIFFITHS_SKIP_STEP).is_some(),
        "Skip this step is offered while a prediction is still open"
    );
    assert!(
        !text.contains("Axial field result"),
        "no result before the prediction step: {text}"
    );

    click_label(&mut h, GRIFFITHS_SKIP_STEP);
    h.idle();

    assert_eq!(
        h.app.griffiths_checkpoint_for_test(),
        Some(GriffithsCheckpoint::Reveal)
    );
    assert!(
        !h.app.ring_lesson_revealed_for_test(),
        "skipping the prediction step never reveals the solution"
    );
    let out = h.frame(Vec::new());
    let text = texts(&out).join("\n");
    assert!(
        text.contains("Checkpoint 2 of 3 · Reveal the result"),
        "the checkpoint advanced by exactly one step: {text}"
    );
    assert!(
        !text.contains("Axial field result"),
        "the result stays withheld after skipping the prediction: {text}"
    );
    assert!(
        node_rect(&out, GRIFFITHS_SKIP_STEP).is_none(),
        "there is no further honest step to skip before the reveal"
    );

    click_label(&mut h, "Reveal result");
    h.idle();
    assert_eq!(
        h.app.griffiths_checkpoint_for_test(),
        Some(GriffithsCheckpoint::Result)
    );
    let text = texts(&h.frame(Vec::new())).join("\n");
    assert!(
        text.contains("Checkpoint 3 of 3 · Read the result"),
        "the result step is explicit after the reveal: {text}"
    );
}

#[test]
fn hints_are_voluntary_and_do_not_name_the_answer_before_reveal() {
    let mut h = Harness::new(1200.0, 800.0);
    h.start_route(GriffithsRoute::Ring);

    let out = h.frame(Vec::new());
    let text = texts(&out).join("\n");
    assert!(
        !text.contains(RING_HINT),
        "the contextual hint stays hidden until requested: {text}"
    );
    assert!(
        node_rect(&out, GRIFFITHS_SHOW_HINT).is_some(),
        "Show hint is offered beside the step controls"
    );

    click_label(&mut h, GRIFFITHS_SHOW_HINT);
    h.idle();
    let text = texts(&h.frame(Vec::new())).join("\n");
    assert!(
        text.contains(RING_HINT),
        "Show hint reveals the lesson's existing contextual hint: {text}"
    );
    assert!(
        !text.contains("Axial field result"),
        "the hint does not reveal the result: {text}"
    );
    assert_eq!(
        h.app.griffiths_checkpoint_for_test(),
        Some(GriffithsCheckpoint::Predict),
        "asking for a hint does not advance the checkpoint"
    );

    // The wire lesson exposes both of its existing hints through the same
    // voluntary control, and they remain pre-reveal safe.
    h.app.exit_griffiths_lesson();
    h.start_route(GriffithsRoute::Wire);
    let text = texts(&h.frame(Vec::new())).join("\n");
    assert!(
        !text.contains(WIRE_DIRECTION_HINT) && !text.contains(WIRE_ORIENTATION_HINT),
        "both wire hints are hidden by default: {text}"
    );
    click_label(&mut h, GRIFFITHS_SHOW_HINT);
    h.idle();
    let text = texts(&h.frame(Vec::new())).join("\n");
    assert!(
        text.contains(WIRE_DIRECTION_HINT) && text.contains(WIRE_ORIENTATION_HINT),
        "the wire hints appear together: {text}"
    );
    assert!(
        !text.contains("Signed circulation"),
        "the wire hint does not reveal the measured circulation: {text}"
    );
}

#[test]
fn explore_then_resume_restores_the_checkpoint_scene_and_lesson_state() {
    let mut h = Harness::new(1200.0, 800.0);
    h.start_route(GriffithsRoute::Ring);
    h.app.set_ring_probe_for_test(0.75);
    h.app.predict_ring_for_test(RingPrediction::PartwayOut);
    h.idle();
    assert_eq!(
        h.app.griffiths_checkpoint_for_test(),
        Some(GriffithsCheckpoint::Reveal)
    );
    let saved_view = h.app.capture_document_for_test().view;
    let saved_orbit = h.app.orbit;
    let saved_camera = (h.app.camera.position, h.app.camera.scale);

    click_label(&mut h, GRIFFITHS_EXPLORE_FROM_HERE);
    h.idle();
    assert!(
        h.app.griffiths_exploration_active_for_test(),
        "Explore from here starts a temporary excursion"
    );
    assert!(
        !h.app.griffiths_return_prompt_for_test(),
        "the excursion starts free, without a modal"
    );
    let text = texts(&h.frame(Vec::new())).join("\n");
    assert!(
        text.contains("Checkpoint 2 of 3 · Reveal the result"),
        "the saved checkpoint stays visible while exploring: {text}"
    );
    assert!(
        node_rect(&h.frame(Vec::new()), GRIFFITHS_RETURN_TO_ACTIVITY).is_some(),
        "a return control is reachable during the excursion"
    );

    // Ordinary free exploration: the learner changes the scene and the camera.
    h.app.state.formula = "sin(x)".to_owned();
    h.app.camera.position = [0.25, -0.5];
    h.app.camera.scale = 2.0;
    h.app.orbit.yaw = 0.3;
    h.idle();

    click_label(&mut h, GRIFFITHS_RETURN_TO_ACTIVITY);
    h.idle();
    assert!(h.app.griffiths_return_prompt_for_test());
    let text = texts(&h.frame(Vec::new())).join("\n");
    assert!(
        text.contains(GRIFFITHS_RESUME_CHECKPOINT) && text.contains(GRIFFITHS_KEEP_VARIATION),
        "return offers both explicit choices: {text}"
    );
    assert!(
        text.contains("restores the scene") && text.contains("keeps your current"),
        "the prompt says which scene each choice keeps: {text}"
    );

    click_label(&mut h, GRIFFITHS_RESUME_CHECKPOINT);
    h.idle();
    assert!(!h.app.griffiths_exploration_active_for_test());
    assert_eq!(
        h.app.griffiths_checkpoint_for_test(),
        Some(GriffithsCheckpoint::Reveal),
        "the checkpoint is restored"
    );
    assert_eq!(
        h.app.capture_document_for_test().view,
        saved_view,
        "Resume at checkpoint restores the scene saved at the checkpoint"
    );
    assert_eq!(h.app.orbit, saved_orbit, "Resume restores the camera");
    assert_eq!(
        (h.app.camera.position, h.app.camera.scale),
        saved_camera,
        "Resume restores the camera scale and pan"
    );
    assert!(
        !h.app.ring_lesson_revealed_for_test(),
        "the restored checkpoint is still pre-reveal"
    );
}

#[test]
fn keep_my_variation_retains_current_scene_and_lesson_edits() {
    let mut h = Harness::new(1200.0, 800.0);
    h.start_route(GriffithsRoute::Ring);
    h.app.predict_ring_for_test(RingPrediction::PartwayOut);
    h.app.set_ring_probe_for_test(0.75);
    h.idle();

    click_label(&mut h, GRIFFITHS_EXPLORE_FROM_HERE);
    h.idle();
    h.app.state.formula = "sin(x)".to_owned();
    h.app.set_ring_probe_for_test(-1.5);
    h.idle();

    click_label(&mut h, GRIFFITHS_RETURN_TO_ACTIVITY);
    h.idle();
    click_label(&mut h, GRIFFITHS_KEEP_VARIATION);
    h.idle();

    assert!(!h.app.griffiths_exploration_active_for_test());
    assert!(
        h.app.ring_lesson_active_for_test(),
        "the activity resumes with the learner's variation"
    );
    assert_eq!(
        h.app.state.formula, "sin(x)",
        "Keep my variation retains the explored scene"
    );
    let reading = h
        .app
        .ring_lesson_reading_for_test()
        .expect("the ring lesson remains active");
    assert!(
        (reading.z + 1.5).abs() < 1e-9,
        "Keep my variation retains the lesson edit, got z = {}",
        reading.z
    );
    assert!(
        !h.app.ring_lesson_revealed_for_test(),
        "the variation is still at the pre-reveal checkpoint"
    );
}

#[test]
fn choose_controls_are_reachable_and_exit_or_adopt_works_on_both_layouts() {
    for (width, height) in [(1200.0, 800.0), (390.0, 640.0)] {
        let mut h = Harness::new(width, height);
        h.idle();
        let start_view = h.app.capture_document_for_test().view;
        let start_orbit = h.app.orbit;
        let start_camera = (h.app.camera.position, h.app.camera.scale);

        h.app.open_griffiths_branch();
        h.app.start_griffiths_route(GriffithsRoute::Ring);
        h.idle();
        let out = h.frame(Vec::new());
        let text = texts(&out).join("\n");
        assert!(
            text.contains("Checkpoint 1 of 3") && text.contains(GRIFFITHS_SHOW_HINT),
            "{width} x {height}: checkpoint and hint controls are visible: {text}"
        );
        assert!(
            node_rect(&out, GRIFFITHS_SKIP_STEP).is_some()
                && node_rect(&out, GRIFFITHS_EXPLORE_FROM_HERE).is_some(),
            "{width} x {height}: skip and explore controls are reachable"
        );

        // A full temporary excursion and resume stays usable at this width.
        click_label(&mut h, GRIFFITHS_EXPLORE_FROM_HERE);
        h.idle();
        assert!(h.app.griffiths_exploration_active_for_test());
        click_label(&mut h, GRIFFITHS_RETURN_TO_ACTIVITY);
        h.idle();
        click_label(&mut h, GRIFFITHS_RESUME_CHECKPOINT);
        h.idle();
        assert_eq!(
            h.app.griffiths_checkpoint_for_test(),
            Some(GriffithsCheckpoint::Predict),
            "{width} x {height}: resume returns to the same checkpoint"
        );

        // Back / Exit still restores the starting workspace.
        h.app.exit_griffiths_lesson();
        assert_eq!(
            h.app.capture_document_for_test().view,
            start_view,
            "{width} x {height}: Back / Exit restores the starting scene"
        );
        assert_eq!(
            h.app.orbit, start_orbit,
            "{width} x {height}: camera restored"
        );
        assert_eq!(
            (h.app.camera.position, h.app.camera.scale),
            start_camera,
            "{width} x {height}: camera pan restored"
        );

        // Keep exploring still adopts the activity scene.
        h.app.open_griffiths_branch();
        h.app.start_griffiths_route(GriffithsRoute::Ring);
        h.idle();
        h.app.keep_griffiths_scene();
        assert!(
            h.app.griffiths_start_snapshot_is_none_for_test(),
            "{width} x {height}: adopting drops the starting snapshot"
        );
        assert!(
            h.app.state.field_mode == vector_field_explorer::source_scene::FieldMode::Sources,
            "{width} x {height}: the ring scene is adopted"
        );
    }
}

#[test]
fn quick_tour_is_unchanged_and_help_replays_a_fresh_checkpoint() {
    let mut h = Harness::new(1200.0, 800.0);
    h.app
        .apply_menu_action_for_test(MenuAction::StartTour, &h.ctx);
    h.idle();
    let out = h.frame(Vec::new());
    assert!(
        node_rect(&out, "Try a Griffiths problem").is_some(),
        "the quick tour keeps its welcome card and optional branch"
    );

    h.app.exit_griffiths_lesson();
    h.app
        .apply_menu_action_for_test(MenuAction::ShowHelpTable, &h.ctx);
    h.idle();
    click_label(&mut h, "Try a Griffiths problem");
    h.idle();
    h.app.start_griffiths_route(GriffithsRoute::Ring);
    h.idle();
    assert_eq!(
        h.app.griffiths_checkpoint_for_test(),
        Some(GriffithsCheckpoint::Predict),
        "Help replay starts a fresh prediction checkpoint"
    );
    assert!(!h.app.ring_lesson_revealed_for_test());
}
