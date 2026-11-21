//! Slice B: the Maxwell learning/residual panel. Widget assertions use the same
//! headless-accesskit helpers as `maxwell_panel_test.rs`; the numeric checks are
//! driven through the panel's press-only `show` seam.

use eframe::egui;

use crate::support;

use vector_field_explorer::maxwell::{
    solve_bem, ConductorOutline, MaxwellCase, MaxwellScene, MAXWELL_BEM_PENDING_WARNING,
    MAXWELL_BEM_STALE_WARNING,
};
use vector_field_explorer::maxwell_learning::{spec, EquationForm, MaxwellEquation};
use vector_field_explorer::maxwell_residual::TermStatus;
use vector_field_explorer::ui::maxwell_learning_panel::{
    MaxwellLearningPanel, MAXWELL_LEARNING_LIMITATION, MAXWELL_POINTWISE_NOTE,
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
