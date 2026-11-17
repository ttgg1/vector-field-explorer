//! Real-widget tests for the animation export menu, dialog and disabled
//! reasons at desktop and phone widths.

use eframe::egui;
use vector_field_explorer::VfApp;
use vector_field_explorer::induction::InductionScene;
use vector_field_explorer::plot::PlotKind;
use vector_field_explorer::source_scene::FieldMode;

const FORMULA_REASON: &str = "Animation needs a time-dependent formula or moving particles.";
const INDUCTION_REASON: &str =
    "Set a nonzero amplitude and frequency to animate this induction case.";
const GENERAL_REASON: &str =
    "Animation export supports time-dependent Formula fields, Induction and particles.";

struct Harness {
    ctx: egui::Context,
    app: VfApp,
    width: f32,
}

impl Harness {
    fn new(width: f32) -> Self {
        let ctx = crate::support::context();
        ctx.enable_accesskit();
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let mut app = VfApp::new();
        app.layout.help_visible = false;
        app.state.set_formula("sin(t), 0, 0".into());
        Self { ctx, app, width }
    }

    fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        let mut frame = eframe::Frame::_new_kittest();
        let mut out = self.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(self.width, 1000.0),
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
            self.frame(vec![]);
        }
        self.frame(vec![])
    }

    fn click(&mut self, pos: egui::Pos2) -> egui::FullOutput {
        self.frame(pointer(pos, true));
        self.frame(pointer(pos, false))
    }

    fn pick(&mut self, label: &str) {
        let out = self.idle();
        let rect = bounds(&out, label).unwrap_or_else(|| panic!("missing widget {label}"));
        self.click(rect.center());
    }

    fn menu_opener(&self) -> &'static str {
        if self.width <= vector_field_explorer::ui::layout::COMPACT_WIDTH {
            vector_field_explorer::ui::top_bar::HAMBURGER_LABEL
        } else {
            "File"
        }
    }

    fn open_animation_dialog_from_menu(&mut self) {
        let opener = self.menu_opener();
        self.pick(opener);
        self.pick("Export animation…");
    }
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

fn node<'a>(out: &'a egui::FullOutput, label: &str) -> Option<&'a egui::accesskit::Node> {
    out.platform_output
        .accesskit_update
        .as_ref()?
        .nodes
        .iter()
        .map(|(_, node)| node)
        .find(|node| node.label() == Some(label) || node.value() == Some(label))
}

fn bounds(out: &egui::FullOutput, label: &str) -> Option<egui::Rect> {
    let node = node(out, label)?;
    let rect = node.bounds()?;
    Some(egui::Rect::from_min_max(
        egui::pos2(rect.x0 as f32, rect.y0 as f32),
        egui::pos2(rect.x1 as f32, rect.y1 as f32),
    ))
}

fn button_disabled(out: &egui::FullOutput, label: &str) -> bool {
    node(out, label).is_some_and(|node| node.is_disabled())
}

fn texts(out: &egui::FullOutput) -> Vec<String> {
    fn visit(shape: &egui::Shape, out: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(text) => out.push(text.galley.text().to_owned()),
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    visit(shape, out);
                }
            }
            _ => {}
        }
    }
    let mut out_texts = Vec::new();
    for shape in &out.shapes {
        visit(&shape.shape, &mut out_texts);
    }
    out_texts
}

fn assert_has(texts: &[String], needle: &str) {
    assert!(
        texts.iter().any(|text| text == needle),
        "missing {needle:?}; painted text was {texts:?}"
    );
}

fn static_induction(app: &mut VfApp) {
    app.state.set_field_mode(FieldMode::Induction);
    assert!(app.state.set_induction_scene(InductionScene {
        amplitude: 0.0,
        frequency: 0.0,
        ..InductionScene::default()
    }));
}

fn animated_induction(app: &mut VfApp) {
    app.state.set_field_mode(FieldMode::Induction);
    assert!(app.state.set_induction_scene(InductionScene {
        amplitude: 1.0,
        frequency: 0.25,
        ..InductionScene::default()
    }));
}

#[test]
fn menu_opens_the_dialog_with_fields_and_frame_count_at_wide_and_compact() {
    for width in [1200.0, 390.0] {
        let mut h = Harness::new(width);
        h.open_animation_dialog_from_menu();
        let out = h.idle();
        let painted = texts(&out);

        assert!(h.app.animation.open, "the menu must open the dialog");
        assert_has(&painted, "Export animation");
        assert_has(&painted, "Format");
        assert_has(&painted, "Duration");
        assert_has(&painted, "FPS");
        assert_has(&painted, "Width");
        assert_has(&painted, "Height");
        assert_has(&painted, "Frames: 48");
        assert!(
            !painted.iter().any(|text| text.contains("Start time")),
            "the dialog must not expose a start-time field: {painted:?}"
        );
        assert!(
            !button_disabled(&out, "Start"),
            "a time-dependent formula may be animated at width {width}"
        );
    }
}

#[test]
fn frame_count_tracks_duration_and_fps() {
    for width in [1200.0, 390.0] {
        let mut h = Harness::new(width);
        h.app.animation.open_for_viewport((1200, 800));
        h.app.animation.duration = 1.0;
        h.app.animation.fps = 4;
        let out = h.idle();
        assert_has(&texts(&out), "Frames: 4");
    }
}

#[test]
fn static_formula_names_the_exact_reason_and_disables_start() {
    let mut h = Harness::new(1200.0);
    h.app.state.set_formula("1, 0, 0".into());
    h.app.animation.open_for_viewport((1200, 800));
    let out = h.idle();
    assert_has(&texts(&out), FORMULA_REASON);
    assert!(button_disabled(&out, "Start"));
}

#[test]
fn static_induction_names_the_exact_reason_and_enables_start_when_animated() {
    for width in [1200.0, 390.0] {
        let mut h = Harness::new(width);
        static_induction(&mut h.app);
        h.app.animation.open_for_viewport((1200, 800));
        let out = h.idle();
        assert_has(&texts(&out), INDUCTION_REASON);
        assert!(button_disabled(&out, "Start"));

        animated_induction(&mut h.app);
        let out = h.idle();
        assert!(!texts(&out).iter().any(|text| text == INDUCTION_REASON));
        assert!(!button_disabled(&out, "Start"));
    }
}

#[test]
fn unsupported_modes_name_the_general_reason_and_disable_start() {
    for width in [1200.0, 390.0] {
        for mode in [FieldMode::Sources, FieldMode::Materials, FieldMode::Ode] {
            let mut h = Harness::new(width);
            h.app.state.set_field_mode(mode);
            h.app.animation.open_for_viewport((1200, 800));
            let out = h.idle();
            assert_has(&texts(&out), GENERAL_REASON);
            assert!(
                button_disabled(&out, "Start"),
                "{mode:?} must not start an animation at width {width}"
            );
        }

        let mut h = Harness::new(width);
        h.app.state.plot.kind = PlotKind::Curve2D;
        h.app.animation.open_for_viewport((1200, 800));
        let out = h.idle();
        assert_has(&texts(&out), GENERAL_REASON);
        assert!(
            button_disabled(&out, "Start"),
            "scalar plots are not animatable at width {width}"
        );
    }
}

#[test]
fn the_menu_item_is_disabled_for_a_static_induction_scene() {
    for width in [1200.0, 390.0] {
        let mut h = Harness::new(width);
        static_induction(&mut h.app);
        let opener = h.menu_opener();
        h.pick(opener);
        let out = h.idle();
        assert!(
            button_disabled(&out, "Export animation…"),
            "the menu item must be disabled with its reason at width {width}"
        );
        assert!(!h.app.animation.open);
    }
}

#[test]
fn a_pending_screenshot_blocks_starting_an_animation() {
    let mut h = Harness::new(1200.0);
    h.app.screenshot.request();
    h.app.animation.open_for_viewport((1200, 800));
    let out = h.idle();
    assert_has(
        &texts(&out),
        "Wait for the current screenshot capture to finish.",
    );
    assert!(button_disabled(&out, "Start"));
}

#[test]
fn a_running_animation_blocks_screenshot_requests() {
    use vector_field_explorer::animation::AnimationSpec;
    use vector_field_explorer::animation_job::AnimationJob;

    let mut h = Harness::new(1200.0);
    h.app.animation_job = Some(AnimationJob::fake(AnimationSpec::default(), 4));
    h.app.animation.running = true;
    let ctx = h.ctx.clone();
    h.app.test_harness().menu_svg_export(&ctx);
    assert_eq!(
        h.app.screenshot.toast_text(),
        Some("Wait for the animation export to finish.")
    );
}

#[test]
fn a_terminal_job_restores_the_previous_play_state() {
    use vector_field_explorer::animation::AnimationSpec;
    use vector_field_explorer::animation_job::{AnimationJob, AnimationJobState};

    let mut app = VfApp::new();
    app.clock.set_playing(true);
    let mut cancelled = AnimationJob::fake(AnimationSpec::default(), 2);
    cancelled.cancel();
    cancelled.pump_for_test(1).expect("cancel drains");
    assert_eq!(cancelled.state(), AnimationJobState::Cancelled);
    app.test_begin_animation_job(cancelled);
    assert!(!app.clock.playing, "starting an export pauses playback");

    app.test_sync_animation_job();
    assert!(
        app.clock.playing,
        "cancelling restores the previous play state"
    );
    assert!(app.animation_job.is_none());

    let mut failed = AnimationJob::fake_with_renderer_error(AnimationSpec::default(), 2, 0);
    let _ = failed.pump_for_test(usize::MAX);
    app.test_begin_animation_job(failed);
    assert!(!app.clock.playing);
    app.test_sync_animation_job();
    assert!(
        app.clock.playing,
        "failure restores the previous play state"
    );
    assert!(app.animation_job.is_none());
}

#[test]
fn a_failed_start_is_retryable_but_invalid_fields_still_block() {
    let mut h = Harness::new(1200.0);
    h.app.animation.open_for_viewport((1200, 800));
    h.app.animation.error = Some("ffmpeg video export could not start".into());
    let out = h.idle();
    assert!(
        !button_disabled(&out, "Start"),
        "a previous sink/open failure must not permanently disable Start"
    );

    assert!(
        h.app.test_prepare_animation_start(),
        "a valid retry clears the failure and captures"
    );
    assert!(h.app.animation.error.is_none());

    h.app.animation.width = 10_000;
    h.app.animation.height = 10_000;
    let out = h.idle();
    assert!(
        button_disabled(&out, "Start"),
        "an invalid field keeps Start blocked"
    );
    assert!(
        !h.app.test_prepare_animation_start(),
        "invalid fields fail the same prepare step"
    );
    assert!(
        h.app
            .animation
            .error
            .as_deref()
            .is_some_and(|error| error.contains("budget")),
        "the validation error is still shown: {:?}",
        h.app.animation.error
    );
}
