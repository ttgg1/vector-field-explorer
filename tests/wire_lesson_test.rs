//! Issue #227: the Griffiths 5.14(a) surface-current wire route.
//!
//! The route wraps the shipped `MaxwellCase::InfiniteWire` field and the
//! shipped `field_probe::circulation` loop in a prediction-gated transverse
//! slice. It adds no physical case, quadrature, tolerance or durable probe
//! surface; the movable loop and its orientation are session-only.

use eframe::egui;

use crate::support;
use vector_field_explorer::VfApp;
use vector_field_explorer::maxwell::{MaxwellCase, MaxwellScene};
use vector_field_explorer::source_scene::FieldMode;
use vector_field_explorer::sources::FieldFamily;
use vector_field_explorer::ui::griffiths::{
    GRIFFITHS_PICKER_TITLE, GRIFFITHS_WIRE_LABEL, GriffithsRoute,
};
use vector_field_explorer::ui::menu::MenuAction;
use vector_field_explorer::ui::wire_lesson::{
    WIRE_ASSUMPTION_NOTE, WIRE_DIRECTION_HINT, WIRE_INTERFACE_NOTE, WIRE_LESSON_TITLE,
    WIRE_ORIENTATION_HINT, WIRE_PROMPT, WIRE_QUESTION, WIRE_UNITS_NOTE, WireDirectionPrediction,
    WireInsidePrediction, WireLessonState, WireOrientation, WireReadingKind,
};

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

fn wire(radius: f64) -> MaxwellScene {
    MaxwellScene {
        case: MaxwellCase::InfiniteWire,
        radius,
        ..MaxwellScene::default()
    }
}

#[test]
fn the_wire_route_is_selectable_from_the_picker_and_replayable_from_help() {
    assert!(GriffithsRoute::Wire.available());
    assert_eq!(GriffithsRoute::Wire.label(), GRIFFITHS_WIRE_LABEL);
    let mut h = Harness::new(1200.0, 800.0);
    h.idle();
    h.app.open_griffiths_branch();
    h.idle();
    let out = h.frame(Vec::new());
    let text = texts(&out).join("\n");
    assert!(
        text.contains(GRIFFITHS_PICKER_TITLE) && text.contains(GRIFFITHS_WIRE_LABEL),
        "the picker offers the wire route: {text}"
    );
    assert!(
        !GRIFFITHS_WIRE_LABEL.contains("not available"),
        "the wire route is labelled as available: {GRIFFITHS_WIRE_LABEL}"
    );
    click_label(&mut h, GRIFFITHS_WIRE_LABEL);
    h.idle();
    assert!(
        h.app.wire_lesson_active_for_test(),
        "selecting the wire route opens the activity"
    );
    assert_eq!(
        h.app.state.field_mode,
        FieldMode::Maxwell,
        "the route loads the shipped Maxwell case"
    );
    assert_eq!(h.app.state.maxwell.case, MaxwellCase::InfiniteWire);
    assert!(
        !h.app.state.mode_3d,
        "the activity opens a transverse slice looking down the wire axis"
    );
    let text = texts(&h.frame(Vec::new())).join("\n");
    assert!(
        text.contains(WIRE_LESSON_TITLE),
        "the activity window is visible: {text}"
    );

    h.app.exit_griffiths_lesson();
    assert!(!h.app.wire_lesson_active_for_test());

    h.app
        .apply_menu_action_for_test(MenuAction::ShowHelpTable, &h.ctx);
    h.idle();
    click_label(&mut h, "Try a Griffiths problem");
    h.idle();
    click_label(&mut h, GRIFFITHS_WIRE_LABEL);
    h.idle();
    assert!(
        h.app.wire_lesson_active_for_test(),
        "Help can replay the wire route"
    );
    assert!(
        !h.app.wire_lesson_revealed_for_test(),
        "Help replay starts a fresh prediction gate"
    );
}

#[test]
fn the_prompt_precedes_the_reveal_and_the_hints_anchor_the_current_and_normal() {
    let mut h = Harness::new(1200.0, 1600.0);
    h.idle();
    h.app.open_griffiths_branch();
    h.app.start_griffiths_route(GriffithsRoute::Wire);
    h.idle();

    let out = h.frame(Vec::new());
    let text = texts(&out).join("\n");
    assert!(
        text.contains(WIRE_QUESTION),
        "the question is visible: {text}"
    );
    assert!(text.contains(WIRE_PROMPT), "the prompt is visible: {text}");
    assert!(
        text.contains(WIRE_DIRECTION_HINT),
        "the direction hint is visible: {text}"
    );
    assert!(
        text.contains(WIRE_ORIENTATION_HINT),
        "the orientation hint is visible: {text}"
    );
    assert!(
        WIRE_DIRECTION_HINT.contains("+z") && WIRE_DIRECTION_HINT.contains("right-hand"),
        "the direction hint anchors the shipped +z current and the right-hand rule"
    );
    assert!(
        WIRE_ORIENTATION_HINT.contains("normal") && WIRE_ORIENTATION_HINT.contains("circulation"),
        "the orientation hint anchors the loop normal and the signed circulation"
    );
    assert!(
        text.contains(WIRE_ASSUMPTION_NOTE),
        "the problem card states the ideal surface-current assumption before any result: {text}"
    );
    assert!(
        !text.contains("|B| at loop") && !text.contains("Signed circulation"),
        "no measured result is shown before the prediction: {text}"
    );

    assert!(!h.app.reveal_wire_for_test(), "the reveal stays gated");
    h.app.predict_wire_for_test(
        WireDirectionPrediction::CounterClockwise,
        WireInsidePrediction::BothZero,
    );
    assert!(h.app.reveal_wire_for_test());
    h.idle();
    let text = texts(&h.frame(Vec::new())).join("\n");
    assert!(text.contains("|B| at loop"), "the reveal shows |B|: {text}");
    assert!(
        text.contains("Signed circulation"),
        "the reveal shows the signed circulation: {text}"
    );
    assert!(
        text.contains(WIRE_UNITS_NOTE),
        "the reveal labels normalized amplitudes: {text}"
    );
    assert!(
        text.contains(WIRE_ASSUMPTION_NOTE),
        "the problem card states the ideal surface-current assumption: {text}"
    );
    assert!(
        text.contains(WIRE_INTERFACE_NOTE),
        "the interface handling is stated: {text}"
    );
}

#[test]
fn inside_the_shell_reports_zero_field_and_zero_circulation() {
    let scene = wire(1.0);
    let mut lesson = WireLessonState::default();
    lesson.activate(&scene);
    assert!(lesson.set_loop_radius(0.5));
    let report = lesson
        .report_for(&scene, FieldFamily::Magnetic, 0.0)
        .clone();
    assert!(
        matches!(report.at_radius.kind, WireReadingKind::Interior { .. }),
        "r < a is classified as interior: {:?}",
        report.at_radius.kind
    );
    assert_eq!(report.at_radius.field(), Some(0.0));
    let (circulation, error) = report
        .at_radius
        .circulation_estimate()
        .expect("the interior circulation is a finite estimate");
    assert_eq!(circulation, 0.0);
    assert_eq!(error, 0.0);
}

#[test]
fn the_ideal_interface_is_excluded_and_explained() {
    let scene = wire(1.0);
    let mut lesson = WireLessonState::default();
    lesson.activate(&scene);
    assert!(lesson.set_loop_radius(scene.radius));
    let report = lesson
        .report_for(&scene, FieldFamily::Magnetic, 0.0)
        .clone();
    assert!(
        matches!(report.at_radius.kind, WireReadingKind::Excluded { .. }),
        "r = a is flagged as an excluded boundary sample: {:?}",
        report.at_radius.kind
    );
    assert!(report.at_radius.field().is_none());
    assert!(report.at_radius.circulation_estimate().is_none());
    assert!(WIRE_INTERFACE_NOTE.contains("excluded"));
}

#[test]
fn the_excluded_boundary_reading_is_visible_in_the_lesson() {
    let mut h = Harness::new(1200.0, 800.0);
    h.idle();
    h.app.open_griffiths_branch();
    h.app.start_griffiths_route(GriffithsRoute::Wire);
    h.app.predict_wire_for_test(
        WireDirectionPrediction::CounterClockwise,
        WireInsidePrediction::BothZero,
    );
    assert!(h.app.reveal_wire_for_test());
    let shell_radius = h.app.state.maxwell.radius;
    assert!(h.app.set_wire_loop_for_test(shell_radius));
    h.idle();

    let rendered = texts(&h.frame(Vec::new()));
    let text = rendered.join("\n");
    assert!(
        text.contains("Excluded boundary sample"),
        "the boundary sample is visibly excluded: {text}"
    );
    assert!(
        rendered.iter().any(|line| line == WIRE_INTERFACE_NOTE),
        "the excluded path states the interface explanation on its own line: {text}"
    );
}

#[test]
fn outside_doubling_the_loop_radius_halves_b_and_preserves_the_circulation() {
    let scene = wire(1.0);
    let mut lesson = WireLessonState::default();
    lesson.activate(&scene);
    assert!(lesson.set_loop_radius(1.5));
    let report = lesson
        .report_for(&scene, FieldFamily::Magnetic, 0.0)
        .clone();

    let first_field = report.at_radius.field().expect("exterior r field");
    let second_field = report.at_double.field().expect("exterior 2r field");
    assert!(
        (first_field / second_field - 2.0).abs() < 1e-3,
        "doubling the loop radius halves |B|: {first_field} / {second_field}"
    );
    let (circulation_first, error_first) = report
        .at_radius
        .circulation_estimate()
        .expect("r circulation");
    let (circulation_second, error_second) = report
        .at_double
        .circulation_estimate()
        .expect("2r circulation");
    let difference = (circulation_first - circulation_second).abs();
    let uncertainty = error_first + error_second;
    assert!(
        difference <= uncertainty + 1e-9,
        "the circulation is unchanged within quadrature uncertainty: \
         {difference} <= {uncertainty}"
    );
    assert!(
        circulation_first.abs() > 1.0,
        "the exterior circulation is nonzero in normalized units: {circulation_first}"
    );
}

#[test]
fn reversing_only_the_loop_orientation_flips_the_signed_circulation_and_keeps_the_field() {
    let scene = wire(1.0);
    let mut lesson = WireLessonState::default();
    lesson.activate(&scene);
    assert!(lesson.set_loop_radius(1.5));
    let positive = lesson
        .report_for(&scene, FieldFamily::Magnetic, 0.0)
        .clone();
    lesson.set_orientation(WireOrientation::NegativeNormal);
    let negative = lesson
        .report_for(&scene, FieldFamily::Magnetic, 0.0)
        .clone();

    let (positive_circulation, positive_error) = positive
        .at_radius
        .circulation_estimate()
        .expect("positive circulation");
    let (negative_circulation, negative_error) = negative
        .at_radius
        .circulation_estimate()
        .expect("negative circulation");
    assert!(
        (positive_circulation + negative_circulation).abs()
            <= positive_error + negative_error + 1e-9,
        "reversing orientation reverses the signed integral: \
         {positive_circulation} vs {negative_circulation}"
    );
    assert!(
        positive_circulation.signum() != negative_circulation.signum(),
        "the two orientations report opposite signs"
    );
    assert_eq!(
        positive.at_radius.field(),
        negative.at_radius.field(),
        "the field magnitude at the loop is orientation independent"
    );
    assert_eq!(
        positive.field_vector, negative.field_vector,
        "reversing the loop normal does not change the field vector"
    );

    // The viewport arrows come from the shipped Maxwell sampling, which the
    // lesson orientation never touches. Flip the orientation through the app
    // and compare the actual sampled arrows.
    let mut h = Harness::new(1200.0, 800.0);
    h.idle();
    h.app.open_griffiths_branch();
    h.app.start_griffiths_route(GriffithsRoute::Wire);
    h.idle();
    let arrows_before = h
        .app
        .state
        .maxwell_sampling
        .as_ref()
        .expect("the wire scene samples arrows")
        .arrows
        .clone();
    assert!(!arrows_before.is_empty());
    h.app
        .set_wire_orientation_for_test(WireOrientation::NegativeNormal);
    let arrows_after = h
        .app
        .state
        .maxwell_sampling
        .as_ref()
        .expect("the wire scene still samples arrows")
        .arrows
        .clone();
    assert_eq!(
        arrows_before, arrows_after,
        "reversing the loop orientation leaves the field arrows unchanged"
    );
}

#[test]
fn back_exit_restores_the_starting_scene_and_keep_exploring_adopts_the_wire() {
    let mut h = Harness::new(1200.0, 800.0);
    h.idle();
    let start_document = h.app.capture_document_for_test();
    let start_orbit = h.app.orbit;
    let start_camera = (h.app.camera.position, h.app.camera.scale);
    let start_mode_3d = h.app.state.mode_3d;

    h.app.open_griffiths_branch();
    h.app.start_griffiths_route(GriffithsRoute::Wire);
    h.idle();
    assert!(h.app.wire_lesson_active_for_test());
    assert_eq!(h.app.state.field_mode, FieldMode::Maxwell);
    assert_eq!(h.app.state.maxwell.case, MaxwellCase::InfiniteWire);
    h.app.exit_griffiths_lesson();
    assert_eq!(
        h.app.capture_document_for_test().view,
        start_document.view,
        "Back/Exit restores the starting scene"
    );
    assert_eq!(h.app.orbit, start_orbit, "Back/Exit restores the camera");
    assert_eq!((h.app.camera.position, h.app.camera.scale), start_camera);
    assert_eq!(
        h.app.state.mode_3d, start_mode_3d,
        "Back/Exit restores the projection"
    );
    assert!(!h.app.wire_lesson_active_for_test());

    h.app.open_griffiths_branch();
    h.app.start_griffiths_route(GriffithsRoute::Wire);
    h.app.keep_griffiths_scene();
    assert!(!h.app.wire_lesson_active_for_test());
    assert!(h.app.griffiths_start_snapshot_is_none_for_test());
    assert_eq!(h.app.state.field_mode, FieldMode::Maxwell);
    assert_eq!(
        h.app.state.maxwell.case,
        MaxwellCase::InfiniteWire,
        "Keep exploring adopts the shipped wire scene"
    );
    assert!(
        !h.app.state.mode_3d,
        "Keep exploring adopts the transverse slice"
    );
}

#[test]
fn the_wire_lesson_stays_usable_in_wide_and_compact_layouts() {
    for (width, height) in [(1200.0, 800.0), (360.0, 640.0)] {
        let mut h = Harness::new(width, height);
        h.idle();
        h.app.open_griffiths_branch();
        h.idle();
        let text = texts(&h.frame(Vec::new())).join("\n");
        assert!(
            text.contains(GRIFFITHS_PICKER_TITLE) && text.contains(GRIFFITHS_WIRE_LABEL),
            "{width} pt: the wire route is reachable from the picker: {text}"
        );
        if width > 400.0 {
            click_label(&mut h, GRIFFITHS_WIRE_LABEL);
        } else {
            // The seeded compact frame still has the quick-tour welcome card
            // above the picker; the route itself is the subject here.
            h.app.start_griffiths_route(GriffithsRoute::Wire);
        }
        h.idle();
        assert!(
            h.app.wire_lesson_active_for_test(),
            "{width} pt: the wire route opens"
        );
        h.app.predict_wire_for_test(
            WireDirectionPrediction::CounterClockwise,
            WireInsidePrediction::BothZero,
        );
        h.app.reveal_wire_for_test();
        h.idle();
        let text = texts(&h.frame(Vec::new())).join("\n");
        assert!(
            text.contains("B and circulation result") && text.contains("|B| at loop"),
            "{width} pt: the same labelled result stays usable: {text}"
        );
        assert!(
            text.contains("Signed circulation") && text.contains("loop radius r"),
            "{width} pt: B, circulation and the radius control remain labelled: {text}"
        );
    }
}
