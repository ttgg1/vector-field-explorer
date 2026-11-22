//! Issue #225: the Griffiths 2.11 charged-shell route.
//!
//! The route wraps the shipped Hollow sphere Maxwell case and the #219
//! `GaussShellState`/`compute_reading` seam in a prediction-gated lesson. It
//! adds no physical case, quadrature or durable probe surface.

use eframe::egui;

use crate::support;
use vector_field_explorer::VfApp;
use vector_field_explorer::maxwell::{MaxwellCase, MaxwellScene};
use vector_field_explorer::source_scene::FieldMode;
use vector_field_explorer::sources::FieldFamily;
use vector_field_explorer::ui::gauss_shell::{
    GAUSS_SHELL_INSIDE_NOTE, GAUSS_SHELL_INTERFACE_NOTE, GaussShellReadingKind,
};
use vector_field_explorer::ui::griffiths::{
    GRIFFITHS_PICKER_TITLE, GRIFFITHS_SHELL_LABEL, GriffithsRoute,
};
use vector_field_explorer::ui::menu::MenuAction;
use vector_field_explorer::ui::shell_lesson::{
    SHELL_BOUNDARY_NOTE, SHELL_HINT, SHELL_PROMPT, SHELL_QUESTION, ShellFieldPrediction,
    ShellFluxPrediction, ShellLessonState,
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

fn hollow(radius: f64) -> MaxwellScene {
    MaxwellScene {
        case: MaxwellCase::HollowSphere,
        radius,
        ..MaxwellScene::default()
    }
}

#[test]
fn the_shell_route_is_selectable_from_the_picker_and_replayable_from_help() {
    assert!(GriffithsRoute::Shell.available());
    let mut h = Harness::new(1200.0, 800.0);
    h.idle();
    h.app.open_griffiths_branch();
    h.idle();
    let out = h.frame(Vec::new());
    let text = texts(&out).join("\n");
    assert!(
        text.contains(GRIFFITHS_PICKER_TITLE) && text.contains(GRIFFITHS_SHELL_LABEL),
        "the picker offers the shell route: {text}"
    );
    click_label(&mut h, GRIFFITHS_SHELL_LABEL);
    h.idle();
    assert!(
        h.app.shell_lesson_active_for_test(),
        "selecting the shell route opens the activity"
    );
    assert_eq!(
        h.app.state.field_mode,
        FieldMode::Maxwell,
        "the route loads the shipped Maxwell case"
    );
    assert_eq!(h.app.state.maxwell.case, MaxwellCase::HollowSphere);

    h.app.exit_griffiths_lesson();
    assert!(!h.app.shell_lesson_active_for_test());

    h.app
        .apply_menu_action_for_test(MenuAction::ShowHelpTable, &h.ctx);
    h.idle();
    click_label(&mut h, "Try a Griffiths problem");
    h.idle();
    click_label(&mut h, GRIFFITHS_SHELL_LABEL);
    h.idle();
    assert!(
        h.app.shell_lesson_active_for_test(),
        "Help can replay the shell route"
    );
}

#[test]
fn the_prompt_precedes_the_reveal_and_the_hint_separates_shell_radius_from_measuring_radius() {
    let mut h = Harness::new(1200.0, 800.0);
    h.idle();
    h.app.open_griffiths_branch();
    h.app.start_griffiths_route(GriffithsRoute::Shell);
    h.idle();

    let out = h.frame(Vec::new());
    let text = texts(&out).join("\n");
    assert!(
        text.contains(SHELL_QUESTION),
        "the question is visible: {text}"
    );
    assert!(text.contains(SHELL_PROMPT), "the prompt is visible: {text}");
    assert!(
        text.contains(SHELL_HINT),
        "the first hint is visible: {text}"
    );
    assert!(
        SHELL_HINT.contains("R is the shell") && SHELL_HINT.contains("r is the measuring"),
        "the first hint distinguishes the shell radius R from the measuring radius r"
    );
    assert!(
        !text.contains("Radial field E =") && !text.contains("Flux Phi ="),
        "no measured result is shown before the prediction: {text}"
    );

    assert!(!h.app.reveal_shell_for_test(), "the reveal stays gated");
    h.app.predict_shell_for_test(
        ShellFieldPrediction::ZeroInside,
        ShellFluxPrediction::ZeroInside,
    );
    assert!(h.app.reveal_shell_for_test());
    h.idle();
    let text = texts(&h.frame(Vec::new())).join("\n");
    assert!(
        text.contains("Radial field E ="),
        "the reveal shows E: {text}"
    );
    assert!(
        text.contains("Surface area A ="),
        "the reveal shows A: {text}"
    );
    assert!(text.contains("Flux Phi ="), "the reveal shows Phi: {text}");
    assert!(
        text.contains("Enclosed charge") && text.contains("Outward normals"),
        "the reveal adds enclosed charge and outward normals: {text}"
    );
}

#[test]
fn inside_reports_zero_field_and_flux_and_the_ideal_boundary_is_excluded() {
    let scene = hollow(1.0);
    let mut lesson = ShellLessonState::default();
    lesson.activate(&scene);
    assert!(lesson.set_probe_radius(0.5));
    let report = lesson
        .report_for(&scene, FieldFamily::Electric, 0.0)
        .clone();
    assert!(matches!(
        report.at_radius.kind,
        GaussShellReadingKind::Interior { .. }
    ));
    assert_eq!(report.at_radius.field(), Some(0.0));
    let (flux, error) = report
        .at_radius
        .flux_estimate()
        .expect("the interior flux is a finite estimate");
    assert_eq!(flux, 0.0);
    assert_eq!(error, 0.0);
    assert!(report.at_radius.area().is_some());
    assert!(GAUSS_SHELL_INSIDE_NOTE.contains("zero"));

    assert!(lesson.set_probe_radius(scene.radius));
    let boundary = lesson
        .report_for(&scene, FieldFamily::Electric, 0.0)
        .at_radius
        .clone();
    assert!(
        matches!(boundary.kind, GaussShellReadingKind::Excluded { .. }),
        "r = R is flagged as an excluded boundary sample: {:?}",
        boundary.kind
    );
    assert!(boundary.field().is_none());
    assert!(GAUSS_SHELL_INTERFACE_NOTE.contains("excluded"));
}

#[test]
fn the_excluded_boundary_reading_explains_why_no_field_is_reported() {
    let mut h = Harness::new(1200.0, 800.0);
    h.idle();
    h.app.open_griffiths_branch();
    h.app.start_griffiths_route(GriffithsRoute::Shell);
    h.app.predict_shell_for_test(
        ShellFieldPrediction::ZeroInside,
        ShellFluxPrediction::ZeroInside,
    );
    assert!(h.app.reveal_shell_for_test());
    let shell_radius = h.app.state.maxwell.radius;
    assert!(h.app.set_shell_probe_for_test(shell_radius));
    h.idle();

    let rendered = texts(&h.frame(Vec::new()));
    let text = rendered.join("\n");
    assert!(
        text.contains("Excluded boundary sample"),
        "the boundary sample is visibly excluded: {text}"
    );
    assert!(
        rendered.iter().any(|line| line == SHELL_BOUNDARY_NOTE),
        "the excluded path states the boundary explanation on its own line: {text}"
    );
}

#[test]
fn outside_doubling_r_quarters_the_field_and_keeps_the_flux_within_uncertainty() {
    let scene = hollow(1.0);
    let mut lesson = ShellLessonState::default();
    lesson.activate(&scene);
    assert!(lesson.set_probe_radius(1.5));
    let report = lesson
        .report_for(&scene, FieldFamily::Electric, 0.0)
        .clone();

    let first_field = report.at_radius.field().expect("exterior r field");
    let second_field = report.at_double.field().expect("exterior 2r field");
    let first_area = report.at_radius.area().expect("exterior r area");
    let second_area = report.at_double.area().expect("exterior 2r area");
    assert!(
        (first_field / second_field - 4.0).abs() < 1e-3,
        "doubling r quarters the field: {first_field} / {second_field}"
    );
    assert!(
        (second_area / first_area - 4.0).abs() < 1e-3,
        "doubling r quadruples the area: {second_area} / {first_area}"
    );
    let (flux_first, error_first) = report.at_radius.flux_estimate().expect("r flux");
    let (flux_second, error_second) = report.at_double.flux_estimate().expect("2r flux");
    let difference = (flux_first - flux_second).abs();
    let uncertainty = error_first + error_second;
    assert!(
        difference <= uncertainty + 1e-9,
        "flux is equal within quadrature uncertainty: {difference} <= {uncertainty}"
    );
}

#[test]
fn pinning_inside_and_outside_then_returning_completes_and_help_replay_resets() {
    let mut h = Harness::new(1200.0, 800.0);
    h.idle();
    h.app.open_griffiths_branch();
    h.app.start_griffiths_route(GriffithsRoute::Shell);
    h.app.predict_shell_for_test(
        ShellFieldPrediction::ZeroInside,
        ShellFluxPrediction::ZeroInside,
    );
    assert!(h.app.reveal_shell_for_test());
    assert!(!h.app.shell_lesson_completed_for_test());

    assert!(h.app.set_shell_probe_for_test(0.5));
    assert!(h.app.pin_shell_radius_for_test());
    assert!(!h.app.shell_comparison_ready_for_test());
    assert!(h.app.set_shell_probe_for_test(1.5));
    assert!(h.app.pin_shell_radius_for_test());
    assert!(h.app.shell_comparison_ready_for_test());

    h.app.exit_griffiths_lesson();
    assert!(
        h.app.shell_lesson_completed_for_test(),
        "returning after comparing inside/outside completes the activity"
    );

    h.app.open_griffiths_branch();
    h.idle();
    let text = texts(&h.frame(Vec::new())).join("\n");
    assert!(
        text.contains("Charged shell activity completed"),
        "the picker keeps the completed state visible after returning: {text}"
    );
    assert!(
        text.contains(GRIFFITHS_SHELL_LABEL),
        "the completed shell route is still offered for replay: {text}"
    );

    h.app.start_griffiths_route(GriffithsRoute::Shell);
    assert!(
        !h.app.shell_lesson_completed_for_test(),
        "Help replay starts a fresh activity"
    );
    assert!(!h.app.shell_lesson_revealed_for_test());
}

#[test]
fn back_exit_restores_the_starting_scene_and_keep_exploring_adopts_the_shell() {
    let mut h = Harness::new(1200.0, 800.0);
    h.idle();
    let start_document = h.app.capture_document_for_test();
    let start_orbit = h.app.orbit;
    let start_camera = (h.app.camera.position, h.app.camera.scale);

    h.app.open_griffiths_branch();
    h.app.start_griffiths_route(GriffithsRoute::Shell);
    h.idle();
    assert!(h.app.shell_lesson_active_for_test());
    assert_eq!(h.app.state.field_mode, FieldMode::Maxwell);
    assert_eq!(h.app.state.maxwell.case, MaxwellCase::HollowSphere);
    h.app.exit_griffiths_lesson();
    assert_eq!(
        h.app.capture_document_for_test().view,
        start_document.view,
        "Back/Exit restores the starting scene"
    );
    assert_eq!(h.app.orbit, start_orbit, "Back/Exit restores the camera");
    assert_eq!((h.app.camera.position, h.app.camera.scale), start_camera);

    h.app.open_griffiths_branch();
    h.app.start_griffiths_route(GriffithsRoute::Shell);
    h.app.keep_griffiths_scene();
    assert!(!h.app.shell_lesson_active_for_test());
    assert!(h.app.griffiths_start_snapshot_is_none_for_test());
    assert_eq!(h.app.state.field_mode, FieldMode::Maxwell);
    assert_eq!(
        h.app.state.maxwell.case,
        MaxwellCase::HollowSphere,
        "Keep exploring adopts the shipped shell scene"
    );
}

#[test]
fn the_shell_lesson_stays_usable_in_wide_and_compact_layouts() {
    for (width, height) in [(1200.0, 800.0), (360.0, 640.0)] {
        let mut h = Harness::new(width, height);
        h.idle();
        h.app.open_griffiths_branch();
        h.idle();
        let text = texts(&h.frame(Vec::new())).join("\n");
        assert!(
            text.contains(GRIFFITHS_PICKER_TITLE) && text.contains(GRIFFITHS_SHELL_LABEL),
            "{width} pt: the shell route is reachable from the picker: {text}"
        );
        if width > 400.0 {
            click_label(&mut h, GRIFFITHS_SHELL_LABEL);
        } else {
            // The seeded compact frame still has the quick-tour welcome card,
            // which paints above the picker; the real welcome path dismisses
            // the tour first (TourAction::Griffiths). The route itself is the
            // subject here, so start it directly after proving its label draws.
            h.app.start_griffiths_route(GriffithsRoute::Shell);
        }
        h.idle();
        assert!(
            h.app.shell_lesson_active_for_test(),
            "{width} pt: the shell route opens"
        );
        h.app.predict_shell_for_test(
            ShellFieldPrediction::ZeroInside,
            ShellFluxPrediction::ZeroInside,
        );
        h.app.reveal_shell_for_test();
        h.idle();
        let text = texts(&h.frame(Vec::new())).join("\n");
        assert!(
            text.contains("Field and flux result") && text.contains("Surface area A ="),
            "{width} pt: the same labelled result stays usable: {text}"
        );
        assert!(
            text.contains("Flux Phi =") && text.contains("measuring radius r"),
            "{width} pt: E/A/Phi and the r control remain labelled: {text}"
        );
    }
}
