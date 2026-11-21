//! Slice B: the Maxwell learning/residual panel. Widget assertions use the same
//! headless-accesskit helpers as `maxwell_panel_test.rs`; the numeric checks are
//! driven through the panel's press-only `show` seam.

use eframe::egui;

use crate::support;

use vector_field_explorer::maxwell::{
    ConductorOutline, MAXWELL_BEM_PENDING_WARNING, MAXWELL_BEM_STALE_WARNING, MaxwellCase,
    MaxwellScene, solve_bem,
};
use vector_field_explorer::maxwell_learning::{EquationForm, MaxwellEquation, spec};
use vector_field_explorer::maxwell_residual::TermStatus;
use vector_field_explorer::ui::maxwell_learning_panel::{
    MAXWELL_LEARNING_LIMITATION, MAXWELL_POINTWISE_NOTE, MAXWELL_REPORT_STALE, MaxwellLearningPanel,
};
use vector_field_explorer::ui::maxwell_panel::{self, MaxwellPanelState};
use vector_field_explorer::viewmodel::State;

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

fn has(nodes: &[WidgetNode], label: &str) -> bool {
    find(nodes, label).is_some()
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

fn frame(
    ctx: &egui::Context,
    panel: &mut MaxwellLearningPanel,
    scene: &mut MaxwellScene,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    frame_with_bem(ctx, panel, scene, None, events)
}

fn frame_with_bem(
    ctx: &egui::Context,
    panel: &mut MaxwellLearningPanel,
    scene: &mut MaxwellScene,
    bem: Option<&vector_field_explorer::maxwell::BemSolution>,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1100.0, 2400.0),
            )),
            events,
            ..Default::default()
        },
        |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                panel.show(ui, scene, bem);
            });
        },
    );
    output.textures_delta.clear();
    output
}

fn maxwell_panel_frame(
    ctx: &egui::Context,
    state: &mut State,
    panel: &mut MaxwellPanelState,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1100.0, 2400.0),
            )),
            events,
            ..Default::default()
        },
        |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let _ = maxwell_panel::show(ui, panel, state, false);
            });
        },
    );
    output.textures_delta.clear();
    output
}

/// Click a widget inside the full Maxwell panel, then leave the section open
/// for the caller's assertion frame.
fn click_in_maxwell_panel(
    ctx: &egui::Context,
    state: &mut State,
    panel: &mut MaxwellPanelState,
    label: &str,
) {
    let out = maxwell_panel_frame(ctx, state, panel, Vec::new());
    let rect = find(&nodes(&out), label)
        .unwrap_or_else(|| panic!("missing clickable {label}"))
        .rect;
    maxwell_panel_frame(ctx, state, panel, pointer(rect.center(), true));
    maxwell_panel_frame(ctx, state, panel, pointer(rect.center(), false));
}

fn click(
    ctx: &egui::Context,
    panel: &mut MaxwellLearningPanel,
    scene: &mut MaxwellScene,
    label: &str,
) {
    let out = frame(ctx, panel, scene, Vec::new());
    let rect = find(&nodes(&out), label)
        .unwrap_or_else(|| panic!("missing clickable {label}"))
        .rect;
    frame(ctx, panel, scene, pointer(rect.center(), true));
    frame(ctx, panel, scene, pointer(rect.center(), false));
}

fn magnetic_scene() -> MaxwellScene {
    MaxwellScene {
        case: MaxwellCase::Solenoid,
        ..MaxwellScene::default()
    }
}

fn solenoid_scene() -> MaxwellScene {
    MaxwellScene {
        case: MaxwellCase::Solenoid,
        b0: 1.0,
        radius: 1.0,
        ..MaxwellScene::default()
    }
}

fn check_gauss_magnetic(panel: &mut MaxwellLearningPanel, scene: &mut MaxwellScene) {
    panel.equation = MaxwellEquation::GaussMagnetic;
    panel.check(scene, None);
}

#[test]
fn panel_lists_every_equation_and_both_forms() {
    let ctx = context();
    let mut scene = MaxwellScene::default();
    let mut panel = MaxwellLearningPanel::default();
    let out = frame(&ctx, &mut panel, &mut scene, Vec::new());
    let nodes = nodes(&out);
    for equation in MaxwellEquation::ALL {
        assert!(
            has(&nodes, equation.title()),
            "missing equation row {}",
            equation.title()
        );
    }
    for form in ["Integral", "Differential"] {
        assert!(has(&nodes, form), "missing form selector {form}");
    }
    for equation in MaxwellEquation::ALL {
        panel.equation = equation;
        let out = frame(&ctx, &mut panel, &mut scene, Vec::new());
        let texts = painted_texts(&out).join("\n");
        let record = spec(equation);
        assert!(
            texts.contains(record.integral),
            "missing integral statement for {}",
            equation.title()
        );
        assert!(
            texts.contains(record.differential),
            "missing differential statement for {}",
            equation.title()
        );
    }
    panel.equation = MaxwellEquation::GaussElectric;
    assert_eq!(
        panel.equation,
        MaxwellEquation::GaussElectric,
        "default equation"
    );
    assert_eq!(panel.form, EquationForm::Differential, "default form");
}

#[test]
fn selecting_an_equation_shows_its_symbols_and_assumptions() {
    let ctx = context();
    let mut scene = MaxwellScene::default();
    let mut panel = MaxwellLearningPanel::default();
    click(
        &ctx,
        &mut panel,
        &mut scene,
        MaxwellEquation::Faraday.title(),
    );
    assert_eq!(panel.equation, MaxwellEquation::Faraday);
    let out = frame(&ctx, &mut panel, &mut scene, Vec::new());
    let texts = painted_texts(&out).join("\n");
    for symbol in ["curl E", "dB/dt", "Phi_B"] {
        assert!(
            texts.contains(symbol),
            "Faraday symbols are missing {symbol}"
        );
    }
    assert!(
        texts.contains("Quasistatic"),
        "Faraday assumptions state its limit: {texts}"
    );
}

#[test]
fn gauss_magnetic_residual_is_reported_against_a_zero_rhs() {
    let ctx = context();
    let mut scene = magnetic_scene();
    let mut panel = MaxwellLearningPanel::default();
    check_gauss_magnetic(&mut panel, &mut scene);
    let report = panel.report.as_ref().expect("Gauss magnetic is checkable");
    assert_eq!(report.residual.equation, MaxwellEquation::GaussMagnetic);
    assert_eq!(
        report.residual.rhs,
        Some(0.0),
        "explicit zero right-hand side"
    );
    assert_eq!(report.residual.status, TermStatus::Analytic);
    let normalized = report.residual.normalized.expect("normalized residual");
    assert!(normalized.is_finite(), "normalized residual stays finite");
    let out = frame(&ctx, &mut panel, &mut scene, Vec::new());
    let texts = painted_texts(&out).join("\n");
    assert!(
        texts.contains("rhs 0"),
        "the zero right-hand side is shown: {texts}"
    );
    assert!(
        texts.contains("Numerical"),
        "the divergence comes from a central difference"
    );
    assert!(
        texts.contains(MAXWELL_POINTWISE_NOTE),
        "the readout states the check is the pointwise differential form: {texts}"
    );
}

#[test]
fn solenoid_interior_divergence_matches_the_analytic_zero() {
    // Ideal infinite solenoid: B = b0 z-hat exactly inside, so div B is
    // analytically zero at any interior probe. Every stencil offset stays
    // inside the radius, so the central difference must return zero, not merely
    // a finite value.
    let ctx = context();
    let mut scene = MaxwellScene {
        case: MaxwellCase::Solenoid,
        b0: 1.0,
        radius: 1.0,
        ..MaxwellScene::default()
    };
    let mut panel = MaxwellLearningPanel {
        equation: MaxwellEquation::GaussMagnetic,
        probe: [0.25, -0.1, 0.05],
        ..MaxwellLearningPanel::default()
    };
    panel.check(&scene, None);
    let report = panel
        .report
        .as_ref()
        .expect("solenoid interior is checkable");
    assert_eq!(
        report.residual.lhs,
        Some(0.0),
        "uniform interior B has an exactly zero h-based divergence"
    );
    let normalized = report.residual.normalized.expect("normalized residual");
    assert!(
        normalized <= 1e-12,
        "analytic div B = 0 within the f32/central-difference floor, got {normalized:.3e}"
    );
    let out = frame(&ctx, &mut panel, &mut scene, Vec::new());
    let texts = painted_texts(&out).join("\n");
    assert!(
        texts.contains("Numerical"),
        "the analytic zero comes from the central-difference stencil: {texts}"
    );
}

#[test]
fn missing_time_derivative_terms_are_reported_unsupported_not_zero() {
    let ctx = context();
    let scene = MaxwellScene::default();
    let mut panel = MaxwellLearningPanel {
        equation: MaxwellEquation::Faraday,
        ..MaxwellLearningPanel::default()
    };
    panel.check(&scene, None);
    let report = panel.report.as_ref().expect("report is kept");
    assert_eq!(
        report.residual.status,
        TermStatus::Unsupported("db_dt"),
        "the missing time derivative is named, not replaced by zero"
    );
    assert_eq!(report.residual.normalized, None);
    assert!(
        report.residual.explanation.contains("db_dt"),
        "the exact symbol appears in the explanation"
    );
    let mut scene = scene;
    let out = frame(&ctx, &mut panel, &mut scene, Vec::new());
    let texts = painted_texts(&out).join("\n");
    assert!(
        texts.contains("Unsupported: db_dt"),
        "per-term chip: {texts}"
    );
    assert!(
        texts.contains(MAXWELL_LEARNING_LIMITATION),
        "the partial-check limitation is always visible"
    );
}

#[test]
fn non_finite_probe_point_is_reported_singular_and_never_nan() {
    let ctx = context();
    let mut scene = magnetic_scene();
    let mut panel = MaxwellLearningPanel {
        equation: MaxwellEquation::GaussMagnetic,
        probe: [f64::NAN, 0.0, 0.0],
        ..MaxwellLearningPanel::default()
    };
    panel.check(&scene, None);
    let report = panel.report.as_ref().expect("a singular report is kept");
    assert_eq!(
        report.residual.status,
        TermStatus::Singular("div_b"),
        "a non-finite probe is singular with the offending symbol"
    );
    assert_eq!(report.residual.normalized, None);
    let out = frame(&ctx, &mut panel, &mut scene, Vec::new());
    let texts = painted_texts(&out).join("\n");
    assert!(!texts.contains("NaN"), "no NaN is ever painted: {texts}");
    assert!(
        !texts.contains("inf"),
        "no infinity is ever painted: {texts}"
    );
    assert!(texts.contains("Singular: div_b"), "singular chip: {texts}");
}

#[test]
fn conductor_case_without_a_solution_skips_the_check_with_the_existing_warning() {
    let ctx = context();
    let mut scene = MaxwellScene {
        case: MaxwellCase::Conductor2D,
        outline: ConductorOutline::Circle { radius: 1.0 },
        ..MaxwellScene::default()
    };
    let mut panel = MaxwellLearningPanel {
        equation: MaxwellEquation::GaussMagnetic,
        ..MaxwellLearningPanel::default()
    };
    let out = frame(&ctx, &mut panel, &mut scene, Vec::new());
    let texts = painted_texts(&out).join("\n");
    assert!(
        texts.contains(MAXWELL_BEM_PENDING_WARNING),
        "the existing pending warning is shown: {texts}"
    );
    click(&ctx, &mut panel, &mut scene, "Check residual");
    assert!(
        panel.report.is_none(),
        "no residual is computed without a matching BEM solution"
    );
    assert_eq!(panel.message.as_deref(), Some(MAXWELL_BEM_PENDING_WARNING));
}

#[test]
fn conductor_case_with_a_stale_solution_skips_the_check_with_the_stale_warning() {
    let ctx = context();
    let solved = MaxwellScene {
        case: MaxwellCase::Conductor2D,
        elements: 64,
        ..MaxwellScene::default()
    };
    let solution = solve_bem(&solved).expect("circle BEM");
    let mut stale = MaxwellScene {
        case: MaxwellCase::Conductor2D,
        elements: 128,
        ..MaxwellScene::default()
    };
    assert!(
        !solution.matches_scene(&stale),
        "the cached solve belongs to a different panel count"
    );

    let mut panel = MaxwellLearningPanel {
        equation: MaxwellEquation::GaussMagnetic,
        ..MaxwellLearningPanel::default()
    };
    let out = frame_with_bem(&ctx, &mut panel, &mut stale, Some(&solution), Vec::new());
    let texts = painted_texts(&out).join("\n");
    assert!(
        texts.contains(MAXWELL_BEM_STALE_WARNING),
        "the stale warning is painted for a non-matching solution: {texts}"
    );

    panel.check(&stale, Some(&solution));
    assert!(
        panel.report.is_none(),
        "a stale BEM solution never runs a residual check"
    );
    assert_eq!(panel.message.as_deref(), Some(MAXWELL_BEM_STALE_WARNING));
}

#[test]
fn residual_check_runs_only_on_press() {
    let ctx = context();
    let mut scene = magnetic_scene();
    let mut panel = MaxwellLearningPanel::default();
    check_gauss_magnetic(&mut panel, &mut scene);
    let original = panel.report.clone();
    frame(&ctx, &mut panel, &mut scene, Vec::new());
    frame(&ctx, &mut panel, &mut scene, Vec::new());
    assert_eq!(
        panel.report, original,
        "a plain frame never re-runs the bounded residual work"
    );

    let mut fresh = MaxwellLearningPanel {
        equation: MaxwellEquation::GaussMagnetic,
        ..MaxwellLearningPanel::default()
    };
    let mut scene = magnetic_scene();
    frame(&ctx, &mut fresh, &mut scene, Vec::new());
    assert!(fresh.report.is_none(), "no check before a press");
    click(&ctx, &mut fresh, &mut scene, "Check residual");
    assert!(fresh.report.is_some(), "the button runs the check once");
}

#[test]
fn report_clears_when_the_committed_case_changes() {
    let ctx = context();
    let mut scene = solenoid_scene();
    let mut panel = MaxwellLearningPanel {
        equation: MaxwellEquation::GaussMagnetic,
        ..MaxwellLearningPanel::default()
    };
    panel.check(&scene, None);
    assert!(panel.report.is_some(), "the check produced a report");

    scene.case = MaxwellCase::ParallelPlates;
    let out = frame(&ctx, &mut panel, &mut scene, Vec::new());
    assert!(
        panel.report.is_none(),
        "a report from another committed case is never kept"
    );
    assert_eq!(
        panel.message.as_deref(),
        Some(MAXWELL_REPORT_STALE),
        "the panel says why the result disappeared"
    );
    let texts = painted_texts(&out).join("\n");
    assert!(
        texts.contains(MAXWELL_REPORT_STALE),
        "the stale explanation is painted: {texts}"
    );
}

#[test]
fn report_clears_when_a_geometry_parameter_changes() {
    let ctx = context();
    let mut scene = solenoid_scene();
    let mut panel = MaxwellLearningPanel {
        equation: MaxwellEquation::GaussMagnetic,
        ..MaxwellLearningPanel::default()
    };
    panel.check(&scene, None);
    assert!(panel.report.is_some(), "the check produced a report");

    scene.radius = 1.5;
    frame(&ctx, &mut panel, &mut scene, Vec::new());
    assert!(
        panel.report.is_none(),
        "a geometry edit invalidates the sampled report"
    );
    assert_eq!(panel.message.as_deref(), Some(MAXWELL_REPORT_STALE));
}

#[test]
fn report_clears_after_a_scene_reset_or_restore() {
    let mut state = State::new(true);
    assert!(state.set_maxwell_scene(solenoid_scene()));
    let mut panel = MaxwellPanelState::default();
    panel.sync_from(&state);
    panel.learning.equation = MaxwellEquation::GaussMagnetic;
    panel.learning.probe = [0.25, 0.0, 0.05];
    panel.learning.check(state.maxwell_scene(), None);
    assert!(
        panel.learning.report.is_some(),
        "the committed scene produced a report"
    );

    state.reset_maxwell();
    panel.sync_from(&state);
    assert!(
        panel.learning.report.is_none(),
        "Reset clears the learning report through sync_from"
    );
    assert!(
        panel.learning.message.is_none(),
        "Reset also clears any stale explanation"
    );
    assert_eq!(
        panel.learning.equation,
        MaxwellEquation::GaussMagnetic,
        "sync_from keeps the equation selection"
    );
    assert_eq!(
        panel.learning.probe,
        [0.25, 0.0, 0.05],
        "sync_from keeps the probe coordinates"
    );

    // The restore entry path uses the same sync seam: a newly committed scene
    // must not leave the previous scene's report behind.
    panel.learning.equation = MaxwellEquation::GaussElectric;
    panel.learning.check(state.maxwell_scene(), None);
    assert!(panel.learning.report.is_some());
    assert!(state.set_maxwell_scene(solenoid_scene()));
    panel.sync_from(&state);
    assert!(
        panel.learning.report.is_none(),
        "a restored committed scene clears the previous report"
    );
}

#[test]
fn report_names_the_committed_scene_and_probe() {
    let ctx = context();
    let mut scene = solenoid_scene();
    let mut panel = MaxwellLearningPanel {
        equation: MaxwellEquation::GaussMagnetic,
        probe: [0.25, -0.1, 0.05],
        ..MaxwellLearningPanel::default()
    };
    panel.check(&scene, None);
    let out = frame(&ctx, &mut panel, &mut scene, Vec::new());
    let texts = painted_texts(&out).join("\n");
    assert!(
        texts.contains("Committed scene: Ideal solenoid"),
        "the report names the committed case: {texts}"
    );
    assert!(
        texts.contains("(0.250, -0.100, 0.050)"),
        "the report names the probe coordinates: {texts}"
    );
    assert!(
        texts.contains("Numerical"),
        "the existing term chips are still painted: {texts}"
    );
}

#[test]
fn a_rejected_draft_keeps_the_report_labelled_with_the_on_screen_scene() {
    let ctx = context();
    let mut state = State::new(true);
    assert!(state.set_maxwell_scene(solenoid_scene()));
    let mut panel = MaxwellPanelState::default();
    panel.sync_from(&state);
    panel.learning.equation = MaxwellEquation::GaussMagnetic;
    panel.learning.probe = [0.25, -0.1, 0.05];
    panel.learning.check(state.maxwell_scene(), None);
    assert!(
        panel.learning.report.is_some(),
        "the check produced a report"
    );

    // Reject a draft that names a different case; the committed solenoid stays
    // on screen, and the report must keep describing that scene.
    panel.draft.case = MaxwellCase::ParallelPlates;
    panel.draft.separation = 0.0;
    assert!(!panel.commit(&mut state), "the draft is invalid");
    assert_eq!(
        state.maxwell_scene().case,
        MaxwellCase::Solenoid,
        "the committed scene is unchanged"
    );

    click_in_maxwell_panel(&ctx, &mut state, &mut panel, "Maxwell equations");
    let out = maxwell_panel_frame(&ctx, &mut state, &mut panel, Vec::new());
    let texts = painted_texts(&out).join("\n");
    assert!(
        texts.contains("Committed scene: Ideal solenoid"),
        "the report is labelled with the committed scene, not the draft: {texts}"
    );
    assert!(
        texts.contains("(0.250, -0.100, 0.050)"),
        "the report keeps the probe it was computed for: {texts}"
    );
    assert!(
        !texts.contains("Committed scene: Parallel plates"),
        "a rejected draft cannot misattribute the report: {texts}"
    );
}

#[test]
fn switching_the_equation_form_keeps_the_report() {
    let ctx = context();
    let mut scene = solenoid_scene();
    let mut panel = MaxwellLearningPanel {
        equation: MaxwellEquation::GaussMagnetic,
        probe: [0.25, -0.1, 0.05],
        ..MaxwellLearningPanel::default()
    };
    panel.check(&scene, None);
    assert!(panel.report.is_some(), "the check produced a report");

    click(&ctx, &mut panel, &mut scene, "Integral");
    assert_eq!(panel.form, EquationForm::Integral);
    assert!(
        panel.report.is_some(),
        "the display form is not part of the report provenance"
    );
    let out = frame(&ctx, &mut panel, &mut scene, Vec::new());
    let texts = painted_texts(&out).join("\n");
    assert!(
        texts.contains("Committed scene: Ideal solenoid"),
        "the report stays painted for the Integral form: {texts}"
    );
    assert!(
        texts.contains("Selected form: Integral"),
        "the selected form is still visible: {texts}"
    );

    click(&ctx, &mut panel, &mut scene, "Differential");
    assert_eq!(panel.form, EquationForm::Differential);
    assert!(
        panel.report.is_some(),
        "switching back keeps the same report"
    );
    let out = frame(&ctx, &mut panel, &mut scene, Vec::new());
    let texts = painted_texts(&out).join("\n");
    assert!(
        texts.contains("Committed scene: Ideal solenoid"),
        "the report stays painted for the Differential form: {texts}"
    );
    assert!(
        texts.contains("Selected form: Differential"),
        "the selected form is still visible: {texts}"
    );
}

#[test]
fn changing_the_equation_clears_the_report_via_the_staleness_check() {
    let ctx = context();
    let mut scene = solenoid_scene();
    let mut panel = MaxwellLearningPanel {
        equation: MaxwellEquation::GaussMagnetic,
        ..MaxwellLearningPanel::default()
    };
    panel.check(&scene, None);
    assert!(panel.report.is_some(), "the check produced a report");

    click(
        &ctx,
        &mut panel,
        &mut scene,
        MaxwellEquation::Faraday.title(),
    );
    assert_eq!(panel.equation, MaxwellEquation::Faraday);
    assert!(
        panel.report.is_none(),
        "the equation selector goes through the staleness check"
    );
    assert_eq!(panel.message.as_deref(), Some(MAXWELL_REPORT_STALE));
    let out = frame(&ctx, &mut panel, &mut scene, Vec::new());
    let texts = painted_texts(&out).join("\n");
    assert!(
        texts.contains(MAXWELL_REPORT_STALE),
        "the stale explanation is painted after the equation edit: {texts}"
    );
    assert!(
        !texts.contains("Committed scene:"),
        "the old report header is gone: {texts}"
    );
    assert!(
        texts.contains("No residual check run yet."),
        "the panel states that no check describes the new equation: {texts}"
    );
}

#[test]
fn a_successful_recheck_replaces_the_stale_message_with_a_fresh_report() {
    let ctx = context();
    let mut scene = solenoid_scene();
    let mut panel = MaxwellLearningPanel {
        equation: MaxwellEquation::GaussMagnetic,
        probe: [0.25, -0.1, 0.05],
        ..MaxwellLearningPanel::default()
    };
    panel.check(&scene, None);
    assert!(panel.report.is_some());

    scene.case = MaxwellCase::ParallelPlates;
    frame(&ctx, &mut panel, &mut scene, Vec::new());
    assert!(panel.report.is_none(), "the scene edit staled the report");
    assert_eq!(panel.message.as_deref(), Some(MAXWELL_REPORT_STALE));

    click(&ctx, &mut panel, &mut scene, "Check residual");
    assert!(
        panel.report.is_some(),
        "the press evaluates the current committed scene"
    );
    assert_ne!(
        panel.message.as_deref(),
        Some(MAXWELL_REPORT_STALE),
        "the fresh check replaces the stale explanation"
    );
    let out = frame(&ctx, &mut panel, &mut scene, Vec::new());
    let texts = painted_texts(&out).join("\n");
    assert!(
        !texts.contains(MAXWELL_REPORT_STALE),
        "the stale text is gone after a successful recheck: {texts}"
    );
    assert!(
        texts.contains("Committed scene: Parallel plates"),
        "the report header names the newly checked scene: {texts}"
    );
}

#[test]
fn probe_label_never_paints_negative_zero() {
    let ctx = context();
    let mut scene = solenoid_scene();
    let mut panel = MaxwellLearningPanel {
        equation: MaxwellEquation::GaussMagnetic,
        probe: [-0.0, 0.0, -0.0],
        ..MaxwellLearningPanel::default()
    };
    panel.check(&scene, None);
    let out = frame(&ctx, &mut panel, &mut scene, Vec::new());
    let texts = painted_texts(&out).join("\n");
    assert!(
        texts.contains("(0.000, 0.000, 0.000)"),
        "negative zero is normalized in the report label: {texts}"
    );
    assert!(
        !texts.contains("-0.000"),
        "the report label never paints negative zero: {texts}"
    );
}

#[test]
fn plain_frames_do_not_re_evaluate_after_a_check() {
    let ctx = context();
    let mut scene = magnetic_scene();
    let mut panel = MaxwellLearningPanel::default();
    click(&ctx, &mut panel, &mut scene, "Check residual");
    let report = panel.report.clone().expect("the button ran the check");
    assert_eq!(report.inputs.scene, scene, "the report records its scene");
    assert_eq!(
        report.inputs.equation, panel.equation,
        "the report records its equation"
    );
    assert_eq!(
        report.inputs.probe, panel.probe,
        "the report records its probe"
    );
    let message = panel.message.clone();
    let report_address = panel.report.as_ref().map(|report| report as *const _);
    let painted = painted_texts(&frame(&ctx, &mut panel, &mut scene, Vec::new())).join("\n");
    assert!(
        painted.contains("Committed scene: Ideal solenoid"),
        "the checked report is painted: {painted}"
    );

    // No evaluation-count seam exists for this panel, so an exact same-input
    // re-evaluation would be value-identical to the stored report. The honest
    // observable proxy is that the stored report is not replaced (allocation
    // identity) and the painted readout is byte-identical across plain frames.
    for _ in 0..3 {
        let frame_texts =
            painted_texts(&frame(&ctx, &mut panel, &mut scene, Vec::new())).join("\n");
        assert_eq!(
            frame_texts, painted,
            "plain frames paint the same readout without re-running the check"
        );
        assert_eq!(
            panel.report.as_ref().map(|report| report as *const _),
            report_address,
            "plain frames do not replace the stored report"
        );
    }
    assert_eq!(
        panel.report,
        Some(report),
        "plain frames never re-run the bounded residual work"
    );
    assert_eq!(
        panel.message, message,
        "plain frames never add a stale message for an unchanged scene"
    );

    // A panel that has never been checked must still paint no report after a
    // plain frame; an auto-evaluating regression would create and paint one.
    let mut fresh = MaxwellLearningPanel {
        equation: MaxwellEquation::GaussMagnetic,
        ..MaxwellLearningPanel::default()
    };
    let mut fresh_scene = magnetic_scene();
    let fresh_out = frame(&ctx, &mut fresh, &mut fresh_scene, Vec::new());
    let fresh_texts = painted_texts(&fresh_out).join("\n");
    assert!(
        fresh.report.is_none(),
        "a plain frame never checks on its own"
    );
    assert!(
        fresh_texts.contains("No residual check run yet."),
        "the unchecked panel states that no check has run: {fresh_texts}"
    );
    assert!(
        !fresh_texts.contains("Committed scene:"),
        "no report header is painted before a press: {fresh_texts}"
    );
}

#[test]
fn panel_never_claims_a_full_time_dependent_solution() {
    let ctx = context();
    let scene = MaxwellScene::default();
    let mut panel = MaxwellLearningPanel {
        equation: MaxwellEquation::Faraday,
        ..MaxwellLearningPanel::default()
    };
    panel.check(&scene, None);
    let mut scene = scene;
    let out = frame(&ctx, &mut panel, &mut scene, Vec::new());
    let texts = painted_texts(&out).join("\n");
    assert!(
        texts.contains(MAXWELL_LEARNING_LIMITATION),
        "the static/quasistatic partial-check statement is always visible"
    );
    assert!(
        texts.contains("static or quasistatic"),
        "the limitation names the active regime"
    );
    assert!(
        texts.contains("partial check, not a full Maxwell verification"),
        "the panel never claims a full verification"
    );
    let explanation = &panel.report.as_ref().expect("report").residual.explanation;
    assert!(
        explanation.contains("does not solve the coupled time-dependent Maxwell system"),
        "the model explanation is shown verbatim"
    );
    assert!(
        !texts.contains("full time-dependent solution"),
        "no full-solution claim is painted"
    );
}
