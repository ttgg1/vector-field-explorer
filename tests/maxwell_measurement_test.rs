//! Issue #217: each Maxwell statement form opens the measurement that actually
//! checks it. The table-driven mapping and incompatibility explanations are pure;
//! the application coverage drives a real `VfApp` through the normal panel and
//! Field probes entry points.

use eframe::egui;

use crate::support;

use vector_field_explorer::VfApp;
use vector_field_explorer::maxwell::{MaxwellCase, MaxwellScene};
use vector_field_explorer::maxwell_learning::{EquationForm, MaxwellEquation};
use vector_field_explorer::maxwell_wave::WaveSpec;
use vector_field_explorer::sources::FieldFamily;
use vector_field_explorer::ui::maxwell_learning_panel::{
    MAXWELL_PARTIAL_MEASUREMENT_NOTE, MaxwellLearningPanel, MeasurementKind,
};

struct WidgetNode {
    label: String,
    rect: egui::Rect,
}

fn nodes(output: &egui::FullOutput) -> Vec<WidgetNode> {
    let Some(update) = output.platform_output.accesskit_update.as_ref() else {
        return Vec::new();
    };
    update
        .nodes
        .iter()
        .filter_map(|(_, node)| {
            let bounds = node.bounds()?;
            let label = node
                .label()
                .or_else(|| node.value())
                .unwrap_or_default()
                .to_owned();
            Some(WidgetNode {
                label,
                rect: egui::Rect::from_min_max(
                    egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                    egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                ),
            })
        })
        .collect()
}

fn find<'a>(nodes: &'a [WidgetNode], label: &str) -> Option<&'a WidgetNode> {
    nodes.iter().find(|node| node.label == label)
}

fn painted_texts(output: &egui::FullOutput) -> Vec<String> {
    fn collect(shape: &egui::Shape, out: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(text) => out.push(text.galley.text().to_owned()),
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect(shape, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut out);
    }
    out
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

fn context() -> egui::Context {
    let ctx = support::context();
    ctx.enable_accesskit();
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    ctx
}

struct Harness {
    ctx: egui::Context,
    app: VfApp,
    frame: eframe::Frame,
    width: f32,
    height: f32,
}

impl Harness {
    fn new(scene: MaxwellScene) -> Self {
        Self::with_size(scene, 1200.0, 2000.0)
    }

    fn compact(scene: MaxwellScene) -> Self {
        Self::with_size(scene, 360.0, 640.0)
    }

    fn with_size(scene: MaxwellScene, width: f32, height: f32) -> Self {
        let ctx = context();
        let mut app = VfApp::new();
        app.layout.help_visible = false;
        app.state
            .set_field_mode(vector_field_explorer::source_scene::FieldMode::Maxwell);
        let _ = app.state.set_maxwell_scene(scene);
        app.state.refresh_maxwell_sampling();
        app.maxwell.sync_from(&app.state);
        app.layout.controls_expanded = true;
        app.layout.maxwell_expanded = true;
        Self {
            ctx,
            app,
            frame: eframe::Frame::_new_kittest(),
            width,
            height,
        }
    }

    fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        let mut out = self.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(self.width, self.height),
                )),
                events,
                ..Default::default()
            },
            |ui| eframe::App::ui(&mut self.app, ui, &mut self.frame),
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

    fn click(&mut self, label: &str) {
        let out = self.frame(Vec::new());
        let rect = find(&nodes(&out), label)
            .unwrap_or_else(|| panic!("missing clickable {label}"))
            .rect;
        self.frame(pointer(rect.center(), true));
        self.frame(pointer(rect.center(), false));
        self.idle();
    }

    /// Scroll the open compact sheet until `label` is fully inside the window,
    /// then press it. The Maxwell sheet is taller than a phone screen, so a
    /// real user scrolls to reach the measurement row.
    fn scroll_click(&mut self, label: &str) {
        for _ in 0..40 {
            let out = self.idle();
            let node = find(&nodes(&out), label)
                .unwrap_or_else(|| panic!("missing scrollable {label}"))
                .rect;
            // Keep the row above the sheet's bottom tab bar, whose overlay
            // would otherwise receive the click.
            if node.min.y >= 0.0 && node.max.y <= self.height * 0.7 {
                self.click(label);
                return;
            }
            let pos = egui::pos2(self.width / 2.0, self.height * 0.6);
            self.frame(vec![
                egui::Event::PointerMoved(pos),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -120.0),
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Wheel,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
        }
        panic!("{label} never scrolled into the compact sheet");
    }
}

fn scene(case: MaxwellCase) -> MaxwellScene {
    MaxwellScene {
        case,
        ..MaxwellScene::default()
    }
}

fn wave_spec() -> WaveSpec {
    WaveSpec {
        amplitude: 1.5,
        wavelength: 2.0,
        direction: glam::Vec3::X,
        polarization: glam::Vec3::Y,
        phase: 0.25,
    }
}

#[test]
fn every_equation_and_form_maps_to_the_matching_measurement() {
    for equation in MaxwellEquation::ALL {
        for (form, expected_kind) in [
            (EquationForm::Integral, None),
            (EquationForm::Differential, Some(MeasurementKind::Point)),
        ] {
            let panel = MaxwellLearningPanel {
                equation,
                form,
                ..MaxwellLearningPanel::default()
            };
            let request = panel.measurement_request();
            assert_eq!(request.equation, equation);
            let expected_kind = expected_kind.unwrap_or(match equation {
                MaxwellEquation::GaussElectric | MaxwellEquation::GaussMagnetic => {
                    MeasurementKind::Surface
                }
                MaxwellEquation::Faraday | MaxwellEquation::AmpereMaxwell => MeasurementKind::Loop,
            });
            assert_eq!(
                request.kind,
                expected_kind,
                "{} / {form:?}",
                equation.title()
            );
            let expected_family = match equation {
                MaxwellEquation::GaussElectric | MaxwellEquation::Faraday => FieldFamily::Electric,
                MaxwellEquation::GaussMagnetic | MaxwellEquation::AmpereMaxwell => {
                    FieldFamily::Magnetic
                }
            };
            assert_eq!(
                request.family,
                expected_family,
                "{} / {form:?}",
                equation.title()
            );
            assert!(!request.button_label().is_empty());
        }
    }
}

#[test]
fn integral_magnetic_gauss_is_a_b_flux_surface_and_differential_is_a_point() {
    let integral = MaxwellLearningPanel {
        equation: MaxwellEquation::GaussMagnetic,
        form: EquationForm::Integral,
        ..MaxwellLearningPanel::default()
    }
    .measurement_request();
    assert_eq!(integral.kind, MeasurementKind::Surface);
    assert_eq!(integral.family, FieldFamily::Magnetic);

    let differential = MaxwellLearningPanel {
        equation: MaxwellEquation::GaussMagnetic,
        form: EquationForm::Differential,
        ..MaxwellLearningPanel::default()
    }
    .measurement_request();
    assert_eq!(differential.kind, MeasurementKind::Point);
}

#[test]
fn compatible_integral_laws_explain_the_partial_measurement() {
    let panel = MaxwellLearningPanel {
        equation: MaxwellEquation::GaussMagnetic,
        form: EquationForm::Integral,
        ..MaxwellLearningPanel::default()
    };
    assert!(
        panel
            .measurement_explanation(&scene(MaxwellCase::Solenoid))
            .is_none(),
        "a magnetic case can measure a B-flux surface"
    );
    assert!(MAXWELL_PARTIAL_MEASUREMENT_NOTE.contains("sampled integral"));
    assert!(MAXWELL_PARTIAL_MEASUREMENT_NOTE.contains("right-hand side"));
}

#[test]
fn incompatible_case_names_the_active_family_and_the_next_step() {
    let panel = MaxwellLearningPanel {
        equation: MaxwellEquation::GaussMagnetic,
        form: EquationForm::Integral,
        ..MaxwellLearningPanel::default()
    };
    let explanation = panel
        .measurement_explanation(&scene(MaxwellCase::ParallelPlates))
        .expect("an electric case cannot measure a B-flux surface");
    assert!(explanation.contains("supplies E only"), "{explanation}");
    assert!(
        explanation.contains("magnetic") && explanation.contains("solenoid"),
        "the explanation names the compatible family and an example: {explanation}"
    );
    assert!(
        explanation.contains("Gauss's law for electricity"),
        "the explanation names the alternative law: {explanation}"
    );
}

#[test]
fn applying_an_integral_law_adds_or_reuses_the_matching_family_probe() {
    let mut h = Harness::new(scene(MaxwellCase::Solenoid));
    h.app.maxwell.learning.equation = MaxwellEquation::GaussMagnetic;
    h.app.maxwell.learning.form = EquationForm::Integral;
    let scene_before = h.app.state.maxwell.clone();
    let orbit = h.app.orbit;

    h.click("Maxwell equations");
    let open_texts = painted_texts(&h.frame(Vec::new())).join("\n");
    assert!(
        open_texts.contains(MAXWELL_PARTIAL_MEASUREMENT_NOTE),
        "the integral statement names what the partial measurement establishes: {open_texts}"
    );
    h.click("Open flux surface");

    assert_eq!(
        h.app.state.probe_family(),
        FieldFamily::Magnetic,
        "the action selects the law's family"
    );
    assert_eq!(
        h.app.state.probe_surfaces().len(),
        1,
        "the action reaches one oriented surface"
    );
    assert_eq!(
        h.app.state.probe_surfaces()[0].kind,
        vector_field_explorer::field_probe::SurfaceKind::Sphere
    );
    assert_eq!(
        h.app.state.maxwell, scene_before,
        "the Maxwell scene itself is unchanged"
    );
    assert_eq!(h.app.orbit, orbit, "the camera is unchanged");
    let report = h
        .app
        .state
        .probe_surface_report(0)
        .expect("the open Field probes section reports the existing IntegralStatus");
    assert_eq!(
        report.family,
        FieldFamily::Magnetic,
        "the readout is the B-flux report for the requested family"
    );

    // Applying again reuses the existing surface rather than piling up probes.
    h.click("Open flux surface");
    assert_eq!(
        h.app.state.probe_surfaces().len(),
        1,
        "a second action reuses the existing surface"
    );
}

#[test]
fn wave_display_allows_both_guided_flux_families() {
    for (case, equation, family) in [
        (
            MaxwellCase::Solenoid,
            MaxwellEquation::GaussElectric,
            FieldFamily::Electric,
        ),
        (
            MaxwellCase::ParallelPlates,
            MaxwellEquation::GaussMagnetic,
            FieldFamily::Magnetic,
        ),
    ] {
        let mut h = Harness::new(scene(case));
        assert!(h.app.state.set_maxwell_wave(Some(wave_spec())));
        h.app.state.refresh_maxwell_sampling();
        h.app.maxwell.learning.equation = equation;
        h.app.maxwell.learning.form = EquationForm::Integral;

        h.click("Maxwell equations");
        let texts = painted_texts(&h.frame(Vec::new())).join("\n");
        assert!(
            !texts.contains("supplies") && !texts.contains("cannot measure"),
            "the wave display supplies both families, so no case-only explanation is shown: {texts}"
        );

        h.click("Open flux surface");
        assert_eq!(
            h.app.state.probe_family(),
            family,
            "the guided measurement selects the requested family"
        );
        assert_eq!(
            h.app.state.probe_surfaces().len(),
            1,
            "the request reaches one oriented surface"
        );
        let report = h
            .app
            .state
            .probe_surface_report(0)
            .expect("the open Field probes section reports the integrated wave field");
        assert!(
            matches!(
                report.status,
                vector_field_explorer::field_probe::IntegralStatus::Estimate { .. }
            ),
            "the wave supplies the requested family instead of a case-scoped refusal: {:?}",
            report.status
        );
    }
}

#[test]
fn case_scoped_measurement_refusal_remains_without_a_wave() {
    let mut h = Harness::new(scene(MaxwellCase::ParallelPlates));
    h.app.maxwell.learning.equation = MaxwellEquation::GaussMagnetic;
    h.app.maxwell.learning.form = EquationForm::Integral;

    h.click("Maxwell equations");
    h.click("Open flux surface");
    let texts = painted_texts(&h.frame(Vec::new())).join("\n");
    assert!(texts.contains("supplies E only"), "{texts}");
    assert!(h.app.state.probe_surfaces().is_empty());

    assert!(h.app.state.set_maxwell_wave(Some(wave_spec())));
    let texts = painted_texts(&h.frame(Vec::new())).join("\n");
    assert!(!texts.contains("supplies E only"), "{texts}");
}

#[test]
fn compact_maxwell_measurement_opens_the_controls_sheet_and_shows_the_readout() {
    use vector_field_explorer::ui::layout::{SheetSize, SheetState};

    let mut h = Harness::compact(scene(MaxwellCase::Solenoid));
    h.app.maxwell.learning.equation = MaxwellEquation::GaussMagnetic;
    h.app.maxwell.learning.form = EquationForm::Integral;
    // The compact layout shows one panel at a time. Open the Maxwell sheet and
    // expand it so the measurement button is on screen, exactly as a user
    // would before pressing it.
    h.app.sheet.open(SheetState::Maxwell);
    h.app.sheet_expanded = SheetSize::Expanded;
    h.idle();
    assert_eq!(
        h.app.sheet,
        SheetState::Maxwell,
        "the Maxwell sheet is the live compact surface"
    );

    h.scroll_click("Maxwell equations");
    h.scroll_click("Open flux surface");

    assert_eq!(
        h.app.sheet,
        SheetState::Controls,
        "the measurement opens the Controls sheet"
    );
    assert_eq!(
        h.app.state.probe_surfaces().len(),
        1,
        "the action reaches one oriented surface on the compact path"
    );
    let out = h.frame(Vec::new());
    let labels = nodes(&out);
    assert!(
        find(&labels, "Field probes").is_some(),
        "the Controls sheet reveals the Field probes body"
    );
    assert!(
        labels.iter().any(|node| node.label.contains("samples)")),
        "the flux readout is visible in the Controls sheet: {:?}",
        labels.iter().map(|node| &node.label).collect::<Vec<_>>()
    );
    assert!(
        h.app.state.probe_surface_report(0).is_some(),
        "the readout is the existing IntegralStatus report"
    );
}

#[test]
fn incompatible_action_is_explained_and_mutates_nothing() {
    let mut h = Harness::new(scene(MaxwellCase::ParallelPlates));
    h.app.maxwell.learning.equation = MaxwellEquation::GaussMagnetic;
    h.app.maxwell.learning.form = EquationForm::Integral;
    let family_before = h.app.state.probe_family();
    let surfaces_before = h.app.state.probe_surfaces().len();
    let loops_before = h.app.state.probe_loops().len();

    h.click("Maxwell equations");
    h.click("Open flux surface");
    let out = h.frame(Vec::new());
    let texts = painted_texts(&out).join("\n");

    assert_eq!(
        h.app.state.probe_family(),
        family_before,
        "an incompatible action does not change the family"
    );
    assert_eq!(h.app.state.probe_surfaces().len(), surfaces_before);
    assert_eq!(h.app.state.probe_loops().len(), loops_before);
    assert!(
        texts.contains("supplies E only"),
        "the actionable explanation is visible: {texts}"
    );
    assert!(
        texts.contains("Ideal solenoid"),
        "the explanation names a compatible case: {texts}"
    );
}

#[test]
fn differential_action_highlights_the_point_and_names_unavailable_terms() {
    let mut h = Harness::new(scene(MaxwellCase::ParallelPlates));
    h.app.maxwell.learning.equation = MaxwellEquation::GaussElectric;
    h.app.maxwell.learning.form = EquationForm::Differential;
    h.app.maxwell.learning.probe = [0.25, -0.1, 0.05];
    let surfaces_before = h.app.state.probe_surfaces().len();
    let loops_before = h.app.state.probe_loops().len();

    h.click("Maxwell equations");
    h.click("Check at probe point");
    let out = h.frame(Vec::new());
    let texts = painted_texts(&out).join("\n");

    assert!(
        h.app.maxwell.learning.report.is_some(),
        "the differential action runs the existing pointwise check"
    );
    assert_eq!(
        h.app.state.probe_surfaces().len(),
        surfaces_before,
        "no integral surface is claimed for a differential check"
    );
    assert_eq!(h.app.state.probe_loops().len(), loops_before);
    assert!(
        texts.contains("Sampled point (highlighted)"),
        "the sampled point is highlighted in the readout: {texts}"
    );
    assert!(
        texts.contains("Unsupported: rho"),
        "unavailable terms are still named: {texts}"
    );
    assert!(
        !texts.contains("flux estimate"),
        "a differential check never claims an integral estimate: {texts}"
    );
}
