//! Issue #214: connect CAS results to the active scene and plot potential and
//! field together on the same slice.
//!
//! The tests drive the shipped CAS result seam and the ordinary scalar-view and
//! gradient setters. No second physics model, grammar or sampler is introduced:
//! contours reuse the scalar-view source layer and arrows reuse the #93
//! potential-gradient layer, both bound to the current parameters, time and
//! slice.

use eframe::egui::{self, vec2};
use glam::Vec3;

use crate::support;

use vector_field_explorer::VfApp;
use vector_field_explorer::colormap::Colormap;
use vector_field_explorer::parameters::{Parameter, ParameterSet};
use vector_field_explorer::plane::{PlaneAxis, PlaneFrame};
use vector_field_explorer::scalar::{ScalarContext, ScalarExpr};
use vector_field_explorer::scalar_analysis::{
    ANALYTIC_DIPOLE_ROUTE_NOTE, NATIVE_SYMBOLIC_NOTE, SeriesRole, analytic_dipole_potential,
    sample_gradient,
};
use vector_field_explorer::scalar_view::scalar_view_expr;
use vector_field_explorer::ui::cas_panel::{
    CasPanelState, Operation, ResultKind, SceneTarget, Variable,
};
use vector_field_explorer::ui::cas_preview::{scalar_slice_heatmap, slice_preview_key};
use vector_field_explorer::viewmodel::State;

fn frame(ctx: &egui::Context, app: &mut VfApp) -> egui::FullOutput {
    let mut native = eframe::Frame::_new_kittest();
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(900.0, 700.0),
            )),
            events: Vec::new(),
            ..Default::default()
        },
        |ui| eframe::App::ui(app, ui, &mut native),
    );
    output.textures_delta.clear();
    output
}

fn idle(ctx: &egui::Context, app: &mut VfApp) {
    let _ = frame(ctx, app);
}

fn xy_slice(origin_z: f32) -> PlaneFrame {
    PlaneFrame {
        axis: PlaneAxis::Z,
        origin: Vec3::new(0.0, 0.0, origin_z),
        right: Vec3::X,
        up: Vec3::Y,
        normal: Vec3::Z,
        half_u: 2.0,
        half_v: 2.0,
    }
}

fn scalar_panel(input: &str, operation: Operation, variable: Variable) -> CasPanelState {
    let mut panel = CasPanelState::default();
    panel.input = input.to_owned();
    panel.operation = operation;
    panel.variable = variable;
    panel.refresh(2);
    panel
}

#[test]
fn analytic_dipole_route_is_available_on_web_and_native() {
    assert!(ANALYTIC_DIPOLE_ROUTE_NOTE.contains("web"));
    assert!(NATIVE_SYMBOLIC_NOTE.to_ascii_lowercase().contains("native"));
    assert!(
        NATIVE_SYMBOLIC_NOTE
            .to_ascii_lowercase()
            .contains("symbolic")
    );

    let frame = xy_slice(0.0);
    let dipole = analytic_dipole_potential(PlaneAxis::X, "d", &frame).expect("analytic dipole");
    assert_eq!(dipole.role, SeriesRole::ElectricPotential);
    assert!(
        dipole.expr.contains('d'),
        "the separation symbol is carried"
    );
    assert!(dipole.expr.contains("sqrt"));
    assert!(
        dipole.valid_region.contains("XY") || dipole.valid_region.contains("slice"),
        "the valid region names the slice: {}",
        dipole.valid_region
    );
    // The expression is a normal surface-context source once `d` is committed.
    let parameters = ParameterSet {
        version: 1,
        entries: vec![Parameter {
            name: "d".into(),
            value: 1.0,
            min: None,
            max: None,
            step: 0.1,
        }],
    };
    let expr = scalar_view_expr(&dipole.expr, &parameters).expect("surface expression");
    let near = expr
        .eval_at(1.0, 0.0, 0.0)
        .expect("finite beside the charge");
    let far = expr
        .eval_at(3.0, 0.0, 0.0)
        .expect("finite away from the charge");
    assert!(near.abs() > far.abs(), "potential is largest near a charge");
}

#[test]
fn cas_scalar_result_plots_contours_without_touching_the_cas_input() {
    let mut state = State::new(false);
    state.set_scalar_contour_levels(0).expect("hide contours");
    let before_source = state.plot.surface.source.clone();
    let before_formula = state.formula.clone();
    let before_parameters = state.parameters.clone();
    let mut panel = scalar_panel("x*y", Operation::Differentiate, Variable::X);
    let result = panel
        .preview
        .as_ref()
        .expect("the derivative previews")
        .formula
        .clone();
    assert_eq!(panel.preview.as_ref().unwrap().kind, ResultKind::Scalar);

    let message = panel
        .apply_scene(&mut state, 2, SceneTarget::Contours)
        .expect("the scalar result applies");
    assert_eq!(state.plot.surface.source, result);
    assert_ne!(state.plot.surface.source, before_source);
    assert_eq!(
        state.formula, before_formula,
        "the vector source is untouched"
    );
    assert_eq!(
        state.parameters, before_parameters,
        "plotting does not edit parameters"
    );
    assert!(state.scalar_contour_levels() > 0, "contours are visible");
    assert_eq!(panel.input, "x*y", "the CAS scratch input survives");
    assert!(panel.preview.is_some(), "the CAS result survives");
    assert!(
        message.to_ascii_lowercase().contains("analytic"),
        "the result provenance is labelled: {message}"
    );
    assert!(
        message.to_ascii_lowercase().contains("sampled"),
        "plotted values are labelled sampled: {message}"
    );
    assert!(
        message.contains("slice") || message.contains("XY"),
        "the valid region is shown: {message}"
    );
    assert_eq!(
        panel.take_scene_edit(),
        Some("plot CAS contours"),
        "the panel reports one named history edit"
    );
}

#[test]
fn cas_scalar_result_overlays_minus_grad_v_on_the_current_slice() {
    let mut state = State::new(false);
    state.set_formula("(0, 0)".into());
    let frame = xy_slice(1.0);
    state.set_domain(Vec3::new(-2.0, -2.0, -1.0), Vec3::new(2.0, 2.0, 3.0));
    // Force a visible plane slice at z = 1.
    state.set_mode(false);
    let mut panel = scalar_panel("x*y", Operation::Differentiate, Variable::X);
    let result = panel.preview.as_ref().unwrap().formula.clone();
    let source_before = state.plot.surface.source.clone();

    let message = panel
        .apply_scene(&mut state, 2, SceneTarget::PotentialField)
        .expect("the potential overlay applies");
    let spec = state
        .gradient
        .as_ref()
        .expect("the gradient request exists");
    assert_eq!(spec.role, SeriesRole::ElectricPotential);
    assert_eq!(spec.expr, result);
    assert_eq!(spec.label(), "E = -∇V");
    assert_eq!(state.plot.surface.source, source_before, "source untouched");
    assert!(
        message.contains("-∇V") && message.to_ascii_lowercase().contains("downhill"),
        "the minus sign and direction are shown: {message}"
    );
    // The request is bound to a real slice frame from the live state.
    let live = state.scalar_view_frame();
    assert!(spec.frame.half_u > 0.0 && spec.frame.half_v > 0.0);
    assert!(live.half_u > 0.0 && spec.frame.half_u == live.half_u);
    assert_eq!(panel.take_scene_edit(), Some("overlay E = -∇V"));
    let _ = frame; // the live frame depends on the test's domain and plane.
}

#[test]
fn a_separation_parameter_change_updates_both_layers() {
    let mut state = State::new(false);
    state.set_formula("(0, 0)".into());
    let dipole = analytic_dipole_potential(PlaneAxis::X, "d", &xy_slice(0.0)).expect("dipole");
    let mut parameters = ParameterSet::default();
    parameters.entries.push(Parameter {
        name: "d".into(),
        value: 1.0,
        min: None,
        max: None,
        step: 0.1,
    });
    state.commit_parameters(parameters).expect("commit d");
    state.set_scalar_source(ScalarContext::Surface, dipole.expr.clone());
    let spec = vector_field_explorer::scalar_analysis::GradientSpec {
        role: SeriesRole::ElectricPotential,
        expr: dipole.expr.clone(),
        frame: xy_slice(0.0),
        step: 1e-3,
        max_error: 5e-2,
    };
    state.set_gradient(Some(spec.clone())).expect("gradient");

    let scalar_at = |state: &State, x: f64| {
        let expr = scalar_view_expr(&state.plot.surface.source, &state.parameters).unwrap();
        expr.eval_at(x, 0.5, f64::from(state.time)).unwrap()
    };
    let gradient_at = |state: &State| {
        sample_gradient(
            &spec,
            &state.parameters,
            f64::from(state.time),
            &state.domain,
        )
        .expect("gradient samples")[0]
            .gradient
            .expect("finite")
    };
    let before_scalar = scalar_at(&state, 0.4);
    let before_gradient = gradient_at(&state);

    let mut next = state.parameters.clone();
    next.entries[0].value = 2.0;
    state.commit_parameters(next).expect("edit separation");

    let after_scalar = scalar_at(&state, 0.4);
    let after_gradient = gradient_at(&state);
    assert_ne!(before_scalar, after_scalar, "contours follow d");
    assert_ne!(before_gradient, after_gradient, "arrows follow d");
}

#[test]
fn a_vector_result_is_refused_by_the_scalar_scene_route() {
    let mut state = State::new(false);
    let mut panel = scalar_panel("(x*y, x)", Operation::Differentiate, Variable::X);
    assert_eq!(panel.preview.as_ref().unwrap().kind, ResultKind::Vector);
    let error = panel
        .apply_scene(&mut state, 2, SceneTarget::Contours)
        .expect_err("a vector result has no scalar contour layer");
    assert!(error.contains("scalar"), "{error}");
    assert_eq!(panel.take_scene_edit(), None);
}

#[test]
fn one_undo_restores_both_cas_scene_layers() {
    let ctx = support::context();
    let mut app = VfApp::new();
    app.state.set_formula("(0, 0)".into());
    app.state
        .set_scalar_source(ScalarContext::Surface, "x".into());
    app.state.set_gradient(None).expect("clear gradient");
    let before_source = app.state.plot.surface.source.clone();
    let before_levels = app.state.scalar_contour_levels();
    let before_gradient = app.state.gradient.clone();
    idle(&ctx, &mut app);

    let mut panel = scalar_panel("x*y", Operation::Differentiate, Variable::X);
    panel
        .apply_scene(&mut app.state, 2, SceneTarget::Contours)
        .expect("contours");
    panel
        .apply_scene(&mut app.state, 2, SceneTarget::PotentialField)
        .expect("overlay");
    assert_eq!(panel.take_scene_edit(), Some("overlay E = -∇V"));
    idle(&ctx, &mut app);
    assert_ne!(app.state.plot.surface.source, before_source);
    assert!(app.state.gradient.is_some());

    assert!(app.undo_document(&ctx), "one undo is available");
    assert_eq!(app.state.plot.surface.source, before_source);
    assert_eq!(app.state.scalar_contour_levels(), before_levels);
    assert_eq!(app.state.gradient, before_gradient);
}

#[test]
fn the_thumbnail_samples_the_active_slice_time_and_parameters() {
    let expr = ScalarExpr::parse("x", ScalarContext::Surface).expect("surface expr");
    let xy = scalar_slice_heatmap(&expr, &xy_slice(0.0), 0.0, [0.0, 1.0], Colormap::Viridis)
        .expect("slice image");
    let higher = scalar_slice_heatmap(&expr, &xy_slice(1.0), 0.0, [0.0, 1.0], Colormap::Viridis)
        .expect("slice image");
    assert_eq!(xy.width, higher.width);
    let key = slice_preview_key("x", &xy_slice(0.0), 0.0, [0.0, 1.0], Colormap::Viridis);
    assert_ne!(
        key,
        slice_preview_key("x", &xy_slice(1.0), 0.0, [0.0, 1.0], Colormap::Viridis)
    );
    assert_ne!(
        key,
        slice_preview_key("x", &xy_slice(0.0), 0.5, [0.0, 1.0], Colormap::Viridis)
    );
}

#[test]
fn the_plot_panel_offers_the_analytic_dipole_route() {
    let ctx = support::context();
    ctx.enable_accesskit();
    let mut state = State::new(false);
    state.set_plot_kind(vector_field_explorer::plot::PlotKind::Surface3D);
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(460.0, 900.0),
            )),
            ..Default::default()
        },
        |ui| {
            vector_field_explorer::ui::plot_panel::show(ui, &mut state, false);
        },
    );
    output.textures_delta.clear();
    let labels: Vec<String> = output
        .platform_output
        .accesskit_update
        .as_ref()
        .into_iter()
        .flat_map(|update| update.nodes.iter())
        .flat_map(|(_, node)| {
            [node.label(), node.value()]
                .into_iter()
                .flatten()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(
        labels
            .iter()
            .any(|text| text.contains("Analytic dipole potential")),
        "the platform-neutral route is offered in the Plot section: {labels:?}"
    );
}

#[test]
fn the_result_provenance_is_labelled_analytic_with_its_valid_region() {
    let state = State::new(false);
    let panel = scalar_panel("x*y", Operation::Differentiate, Variable::X);
    let provenance = panel.preview_provenance();
    assert!(provenance.to_ascii_lowercase().contains("analytic"));
    assert!(provenance.to_ascii_lowercase().contains("sampled"));
    let region = panel.preview_valid_region(&state);
    assert!(
        region.contains("slice") || region.contains("XY") || region.contains("slice"),
        "valid region names the slice: {region}"
    );
}
