//! Issue #226: the Griffiths §3.2 grounded-plane image-charge route.
//!
//! The route loads the shipped `MaxwellCase::ImagePlane` scene in a vertical
//! slice and wraps the shipped `maxwell_field` plus one pure normalized
//! `k = 1` potential helper in a prediction-gated lesson. It adds no physical
//! case, quadrature, tolerance or durable probe surface; the trial image and
//! both readings are session-only.

use eframe::egui;
use glam::Vec3;

use crate::support;
use vector_field_explorer::VfApp;
use vector_field_explorer::maxwell::{
    IMAGE_PLANE_BOUNDARY_TOLERANCE, MaxwellCase, MaxwellScene, image_plane_potential, maxwell_field,
};
use vector_field_explorer::plane::PlaneAxis;
use vector_field_explorer::source_scene::FieldMode;
use vector_field_explorer::ui::griffiths::{
    GRIFFITHS_PICKER_TITLE, GRIFFITHS_PLANE_LABEL, GriffithsRoute,
};
use vector_field_explorer::ui::menu::MenuAction;
use vector_field_explorer::ui::plane_lesson::{
    PLANE_BOUNDARY_NOTE, PLANE_CAS_EXPRESSION, PLANE_CAS_PROVENANCE, PLANE_HALF_SPACE_NOTE,
    PLANE_HINT, PLANE_IMAGE_NOTE, PLANE_LESSON_TITLE, PLANE_PROMPT, PLANE_QUESTION,
    PLANE_SAME_SIGN_FAIL_NOTE, PLANE_SINGULARITY_NOTE, PLANE_UNITS_NOTE,
    PlaneImageLocationPrediction, PlaneImageSignPrediction, PlaneImageTrial, PlaneLessonState,
    boundary_readings, plane_field_reading,
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

fn plane() -> MaxwellScene {
    MaxwellScene {
        case: MaxwellCase::ImagePlane,
        ..MaxwellScene::default()
    }
}

fn correct_trial() -> PlaneImageTrial {
    PlaneImageTrial {
        sign: PlaneImageSignPrediction::NegativeOpposite,
        location: PlaneImageLocationPrediction::MirrorBelow,
    }
}

fn same_sign_trial() -> PlaneImageTrial {
    PlaneImageTrial {
        sign: PlaneImageSignPrediction::SameSign,
        location: PlaneImageLocationPrediction::MirrorBelow,
    }
}

#[test]
fn the_plane_route_is_selectable_from_the_picker_and_replayable_from_help() {
    assert!(GriffithsRoute::Plane.available());
    assert_eq!(GriffithsRoute::Plane.label(), GRIFFITHS_PLANE_LABEL);
    let mut h = Harness::new(1200.0, 800.0);
    h.idle();
    h.app.open_griffiths_branch();
    h.idle();
    let text = texts(&h.frame(Vec::new())).join("\n");
    assert!(
        text.contains(GRIFFITHS_PICKER_TITLE) && text.contains(GRIFFITHS_PLANE_LABEL),
        "the picker offers the plane route: {text}"
    );
    assert!(
        !GRIFFITHS_PLANE_LABEL.contains("not available"),
        "the plane route is labelled as available: {GRIFFITHS_PLANE_LABEL}"
    );
    click_label(&mut h, GRIFFITHS_PLANE_LABEL);
    h.idle();
    assert!(
        h.app.plane_lesson_active_for_test(),
        "selecting the plane route opens the activity"
    );
    assert_eq!(
        h.app.state.field_mode,
        FieldMode::Maxwell,
        "the route loads the shipped Maxwell case"
    );
    assert_eq!(h.app.state.maxwell.case, MaxwellCase::ImagePlane);
    assert!(
        !h.app.state.mode_3d,
        "the activity opens a 2D slice of the image-plane scene"
    );
    assert_eq!(
        h.app.state.plane.axis,
        PlaneAxis::Y,
        "the slice normal is y, so the viewport is the vertical x-z plane"
    );
    assert!(
        h.app.state.maxwell_sampling.is_some(),
        "the shipped scene is sampled for the slice"
    );

    h.app.exit_griffiths_lesson();
    assert!(!h.app.plane_lesson_active_for_test());

    h.app
        .apply_menu_action_for_test(MenuAction::ShowHelpTable, &h.ctx);
    h.idle();
    click_label(&mut h, "Try a Griffiths problem");
    h.idle();
    click_label(&mut h, GRIFFITHS_PLANE_LABEL);
    h.idle();
    assert!(
        h.app.plane_lesson_active_for_test(),
        "Help can replay the plane route"
    );
    assert!(
        !h.app.plane_lesson_revealed_for_test(),
        "Help replay starts a fresh prediction gate"
    );
}

#[test]
fn the_prompt_precedes_the_reveal_and_states_the_construction() {
    let mut h = Harness::new(1200.0, 1600.0);
    h.idle();
    h.app.open_griffiths_branch();
    h.app.start_griffiths_route(GriffithsRoute::Plane);
    h.idle();

    let text = texts(&h.frame(Vec::new())).join("\n");
    assert!(
        text.contains(PLANE_QUESTION),
        "the question is visible: {text}"
    );
    assert!(text.contains(PLANE_PROMPT), "the prompt is visible: {text}");
    assert!(text.contains(PLANE_HINT), "the hint is visible: {text}");
    assert!(
        !text.contains(PLANE_IMAGE_NOTE),
        "the mirror construction is withheld before the prediction: {text}"
    );
    assert!(
        !text.contains("Physical scene"),
        "the physical result is withheld before the prediction: {text}"
    );

    assert!(!h.app.reveal_plane_for_test(), "the reveal stays gated");
    h.app.predict_plane_for_test(
        PlaneImageSignPrediction::NegativeOpposite,
        PlaneImageLocationPrediction::MirrorBelow,
    );
    assert!(h.app.reveal_plane_for_test());
    h.idle();
    let text = texts(&h.frame(Vec::new())).join("\n");
    assert!(
        text.contains("Grounded-plane image result"),
        "the reveal shows the result heading: {text}"
    );
    assert!(
        text.contains(PLANE_IMAGE_NOTE),
        "the reveal shows the mirror-position negative image: {text}"
    );
    assert!(
        text.contains(PLANE_UNITS_NOTE),
        "the result states the normalized units: {text}"
    );
    assert!(
        text.contains(PLANE_HALF_SPACE_NOTE),
        "the result states the valid half-space: {text}"
    );
    assert!(
        text.contains(PLANE_BOUNDARY_NOTE),
        "the result states the boundary potential rule: {text}"
    );
    assert!(
        text.contains(PLANE_SINGULARITY_NOTE),
        "the result excludes both singularities: {text}"
    );
    assert!(
        text.contains(PLANE_CAS_PROVENANCE),
        "the result offers the analytic expression with provenance: {text}"
    );
}

#[test]
fn the_correct_image_has_zero_boundary_potential_at_multiple_lateral_positions() {
    let scene = plane();
    for lateral in [0.5_f32, 1.0, 2.0] {
        let potential = image_plane_potential(&scene, Vec3::new(lateral, 0.0, 0.0))
            .expect("the boundary potential is defined at z = 0");
        assert!(
            potential.abs() <= IMAGE_PLANE_BOUNDARY_TOLERANCE,
            "the correct image gives V = 0 at lateral {lateral}: {potential}"
        );
    }

    let readings = boundary_readings(&scene, correct_trial());
    assert_eq!(readings.len(), 3, "three lateral positions are tested");
    for reading in readings {
        assert!(
            reading.zero_within_tolerance,
            "the correct image is zero within tolerance at lateral {}: {}",
            reading.lateral, reading.potential
        );
        assert!(reading.potential.abs() <= IMAGE_PLANE_BOUNDARY_TOLERANCE);
    }
}

#[test]
fn a_same_sign_image_fails_the_boundary_potential_test() {
    let scene = plane();
    let readings = boundary_readings(&scene, same_sign_trial());
    assert!(
        readings
            .iter()
            .all(|reading| !reading.zero_within_tolerance),
        "a same-sign image is visibly nonzero at every tested lateral position: {readings:?}"
    );
    assert!(
        readings.iter().all(|reading| reading.potential > 1e-6),
        "the same-sign boundary potential is clearly nonzero: {readings:?}"
    );

    let mut h = Harness::new(1200.0, 1600.0);
    h.idle();
    h.app.open_griffiths_branch();
    h.app.start_griffiths_route(GriffithsRoute::Plane);
    h.app.predict_plane_for_test(
        PlaneImageSignPrediction::SameSign,
        PlaneImageLocationPrediction::MirrorBelow,
    );
    assert!(h.app.reveal_plane_for_test());
    h.idle();
    let text = texts(&h.frame(Vec::new())).join("\n");
    assert!(
        text.contains(PLANE_SAME_SIGN_FAIL_NOTE),
        "the lesson states that the same-sign trial fails: {text}"
    );
}

#[test]
fn the_limiting_field_just_above_the_plane_has_no_tangential_component() {
    let scene = plane();
    let field = maxwell_field(&scene, Vec3::new(0.4, 0.0, 1e-3))
        .expect("the field is finite just above the plane");
    assert!(
        field.x.abs() < 2e-3 && field.y.abs() < 1e-12,
        "the tangential component vanishes in the boundary limit: {field:?}"
    );
    assert!(field.z < -1.0, "the normal component is finite: {field:?}");

    let reading = plane_field_reading(&scene, 0.4, 1e-3);
    let sampled = reading.field.expect("the vertical-slice field is finite");
    assert!(sampled.x.abs() < 2e-3 && sampled.y.abs() < 1e-12);
}

#[test]
fn the_valid_half_space_and_both_singularities_are_excluded() {
    let scene = plane();
    assert_eq!(
        maxwell_field(&scene, Vec3::new(0.4, 0.0, -0.5)).unwrap(),
        Vec3::ZERO,
        "the shipped field is zero below the grounded plane"
    );
    assert!(
        image_plane_potential(&scene, Vec3::new(0.4, 0.0, -0.5)).is_err(),
        "the analytic potential is not defined below the valid half-space"
    );
    for singular in [Vec3::new(0.0, 0.0, 1.0), Vec3::new(0.0, 0.0, -1.0)] {
        assert!(
            image_plane_potential(&scene, singular).is_err(),
            "the potential excludes the singularity at {singular:?}"
        );
    }
    assert!(
        maxwell_field(&scene, Vec3::new(0.0, 0.0, 1.0)).is_err(),
        "the shipped field excludes the source singularity"
    );
}

#[test]
fn the_analytic_potential_formula_reaches_the_cas_input_with_provenance() {
    let mut h = Harness::new(1200.0, 1600.0);
    h.idle();
    h.app.open_griffiths_branch();
    h.app.start_griffiths_route(GriffithsRoute::Plane);
    h.app.predict_plane_for_test(
        PlaneImageSignPrediction::NegativeOpposite,
        PlaneImageLocationPrediction::MirrorBelow,
    );
    assert!(h.app.reveal_plane_for_test());
    h.idle();
    let text = texts(&h.frame(Vec::new())).join("\n");
    assert!(
        text.contains(PLANE_CAS_EXPRESSION),
        "the analytic potential expression is shown: {text}"
    );

    h.app.send_plane_formula_to_cas_for_test();
    assert_eq!(h.app.cas.input, PLANE_CAS_EXPRESSION);
    assert!(
        PLANE_CAS_EXPRESSION.contains("sqrt")
            && PLANE_CAS_EXPRESSION.contains("y-h")
            && PLANE_CAS_EXPRESSION.contains("y+h"),
        "the expression writes the source and image distances in the CAS slice: \
         {PLANE_CAS_EXPRESSION}"
    );
}

#[test]
fn back_exit_restores_the_starting_scene_and_keep_exploring_adopts_the_plane() {
    let mut h = Harness::new(1200.0, 800.0);
    h.idle();
    let start_document = h.app.capture_document_for_test();
    let start_orbit = h.app.orbit;
    let start_camera = (h.app.camera.position, h.app.camera.scale);
    let start_mode_3d = h.app.state.mode_3d;
    let start_plane = h.app.state.plane;

    h.app.open_griffiths_branch();
    h.app.start_griffiths_route(GriffithsRoute::Plane);
    h.idle();
    assert!(h.app.plane_lesson_active_for_test());
    assert_eq!(h.app.state.field_mode, FieldMode::Maxwell);
    assert_eq!(h.app.state.maxwell.case, MaxwellCase::ImagePlane);
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
    assert_eq!(
        h.app.state.plane, start_plane,
        "Back/Exit restores the slice selection"
    );
    assert!(!h.app.plane_lesson_active_for_test());

    h.app.open_griffiths_branch();
    h.app.start_griffiths_route(GriffithsRoute::Plane);
    h.app.keep_griffiths_scene();
    assert!(!h.app.plane_lesson_active_for_test());
    assert!(h.app.griffiths_start_snapshot_is_none_for_test());
    assert_eq!(h.app.state.field_mode, FieldMode::Maxwell);
    assert_eq!(
        h.app.state.maxwell.case,
        MaxwellCase::ImagePlane,
        "Keep exploring adopts the shipped plane scene"
    );
    assert!(!h.app.state.mode_3d);
    assert_eq!(
        h.app.state.plane.axis,
        PlaneAxis::Y,
        "Keep exploring adopts the vertical slice"
    );
}

#[test]
fn the_plane_lesson_stays_usable_in_wide_and_compact_layouts() {
    for (width, height) in [(1200.0, 800.0), (360.0, 640.0)] {
        let mut h = Harness::new(width, height);
        h.idle();
        h.app.open_griffiths_branch();
        h.idle();
        let text = texts(&h.frame(Vec::new())).join("\n");
        assert!(
            text.contains(GRIFFITHS_PICKER_TITLE) && text.contains(GRIFFITHS_PLANE_LABEL),
            "{width} pt: the plane route is reachable from the picker: {text}"
        );
        if width > 400.0 {
            click_label(&mut h, GRIFFITHS_PLANE_LABEL);
        } else {
            // The seeded compact frame still has the quick-tour welcome card
            // above the picker; the route itself is the subject here.
            h.app.start_griffiths_route(GriffithsRoute::Plane);
        }
        h.idle();
        assert!(
            h.app.plane_lesson_active_for_test(),
            "{width} pt: the plane route opens"
        );
        h.app.predict_plane_for_test(
            PlaneImageSignPrediction::NegativeOpposite,
            PlaneImageLocationPrediction::MirrorBelow,
        );
        h.app.reveal_plane_for_test();
        h.idle();
        let text = texts(&h.frame(Vec::new())).join("\n");
        assert!(
            text.contains(PLANE_LESSON_TITLE)
                && text.contains("Grounded-plane image result")
                && text.contains("Boundary potential"),
            "{width} pt: the same labelled result stays usable: {text}"
        );
        assert!(
            text.contains("Field just above") && text.contains("lateral position"),
            "{width} pt: the field and lateral controls remain labelled: {text}"
        );
    }
}

#[test]
fn a_fresh_lesson_state_does_not_touch_the_scene_or_a_durable_probe() {
    let scene = plane();
    let mut lesson = PlaneLessonState::default();
    assert!(!lesson.active());
    lesson.activate(&scene);
    assert!(lesson.active());
    assert!(!lesson.revealed());
    assert_eq!(lesson.sign_prediction(), None);
    assert_eq!(lesson.location_prediction(), None);
    lesson.deactivate();
    assert!(!lesson.active());
}
