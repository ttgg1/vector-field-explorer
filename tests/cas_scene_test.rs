//! Issue #214: connect CAS results to the active scene and plot potential and
//! field together on the same slice.
//!
//! The tests drive the shipped CAS result seam and the ordinary scalar-view and
//! gradient setters. No second physics model, grammar or sampler is introduced:
//! contours reuse the scalar-view sampler and arrows reuse the #93
//! potential-gradient layer, both bound to the current parameters, time and
//! slice.

use eframe::egui::{self, vec2};
use glam::Vec3;

use crate::support;
use crate::support::storage::MemoryStorage;

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
    CasPanelState, Operation, ResultKind, SceneTarget, Variable, show_scalar_preview,
};
use vector_field_explorer::ui::cas_preview::{scalar_slice_heatmap, slice_preview_key};
use vector_field_explorer::viewmodel::State;

fn app_output(ctx: &egui::Context, app: &mut VfApp, events: Vec<egui::Event>) -> egui::FullOutput {
    let mut native = eframe::Frame::_new_kittest();
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(1280.0, 6000.0),
            )),
            events,
            ..Default::default()
        },
        |ui| eframe::App::ui(app, ui, &mut native),
    );
    output.textures_delta.clear();
    output
}

fn idle(ctx: &egui::Context, app: &mut VfApp) {
    let _ = app_output(ctx, app, Vec::new());
}

fn render_scalar_preview_texture(
    ctx: &egui::Context,
    state: &State,
    panel: &mut CasPanelState,
    formula: &str,
) -> Option<egui::TextureId> {
    let mut texture = None;
    for _ in 0..2 {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    vec2(460.0, 900.0),
                )),
                ..Default::default()
            },
            |ui| {
                show_scalar_preview(ui, state, panel, formula);
                texture = panel.preview_texture().map(|handle| handle.id());
            },
        );
        output.textures_delta.clear();
    }
    texture
}

fn button_bounds(output: &egui::FullOutput, label: &str) -> Option<egui::Rect> {
    output
        .platform_output
        .accesskit_update
        .as_ref()?
        .nodes
        .iter()
        .find_map(|(_, node)| {
            (node.role() == egui::Role::Button && node.label() == Some(label))
                .then(|| {
                    node.bounds().map(|r| {
                        egui::Rect::from_min_max(
                            egui::pos2(r.x0 as f32, r.y0 as f32),
                            egui::pos2(r.x1 as f32, r.y1 as f32),
                        )
                    })
                })
                .flatten()
        })
}

fn click_app_button(ctx: &egui::Context, app: &mut VfApp, label: &str) {
    let output = app_output(ctx, app, Vec::new());
    let pos = button_bounds(&output, label)
        .unwrap_or_else(|| panic!("the shipped panel offers the {label:?} button"))
        .center();
    app_output(
        ctx,
        app,
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    app_output(
        ctx,
        app,
        vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
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
    assert!(
        NATIVE_SYMBOLIC_NOTE
            .to_ascii_lowercase()
            .contains("finite-difference"),
        "the note separates the finite-difference overlay from native symbolic work"
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
    assert_eq!(state.plot.surface.source, before_source);
    let contour = &state.overlays().as_slice()[0];
    assert_eq!(contour.formula, result);
    assert!(contour.active_contour);
    assert_eq!(
        state.formula, before_formula,
        "the vector source is untouched"
    );
    assert_eq!(
        state.parameters, before_parameters,
        "plotting does not edit parameters"
    );
    assert_eq!(state.scalar_contour_levels(), 0);
    assert!(
        !state.scalar_overlay_lines().is_empty(),
        "CAS contours are visible"
    );
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
    assert_eq!(panel.take_scene_edit(), Some("add linked CAS overlay"));
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
fn cas_contour_plot_and_remove_keep_an_identical_manual_layer() {
    let ctx = support::context();
    ctx.enable_accesskit();
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    app.layout.controls_width = 460.0;
    app.layout.cas_expanded = true;
    app.layout.cas_width = 460.0;
    app.state.set_formula("(0, 0)".into());
    app.cas = scalar_panel("x*y", Operation::Differentiate, Variable::X);
    let expression = app.cas.preview.as_ref().unwrap().formula.clone();
    app.state
        .set_scalar_source(ScalarContext::Surface, expression.clone());
    app.controls.scalar_contour_levels = 3;
    app.controls.scalar_grid_side = 24;
    for _ in 0..4 {
        idle(&ctx, &mut app);
    }
    app.state.refresh_scalar_view(true);

    assert_eq!(app.state.scalar_contour_levels(), 3);
    assert_eq!(app.state.scalar_grid_side(), 24);
    let manual_lines = app.state.scalar_overlay_lines().len();
    assert!(manual_lines > 0);
    click_app_button(&ctx, &mut app, "Plot contours");
    let id = app.state.overlays().as_slice()[0].id;
    let together = app.state.scalar_overlay_lines().len();
    assert!(
        together > manual_lines,
        "both contour layers should be drawn"
    );
    assert_eq!(app.state.plot.surface.source, expression);
    assert_eq!(app.state.scalar_contour_levels(), 3);
    assert_eq!(app.state.scalar_grid_side(), 24);

    click_app_button(&ctx, &mut app, &format!("Remove {id}"));
    assert!(app.state.overlays().is_empty());
    assert_eq!(app.state.plot.surface.source, expression);
    assert_eq!(app.state.scalar_contour_levels(), 3);
    assert_eq!(app.state.scalar_grid_side(), 24);
    assert_eq!(app.state.scalar_overlay_lines().len(), manual_lines);

    assert!(app.undo_document(&ctx));
    idle(&ctx, &mut app);
    app.state.refresh_scalar_view(true);
    assert_eq!(app.state.overlays().as_slice()[0].id, id);
    assert_eq!(app.state.scalar_overlay_lines().len(), together);
    assert_eq!(app.state.scalar_contour_levels(), 3);
    assert_eq!(app.state.scalar_grid_side(), 24);
    assert!(app.redo_document(&ctx));
    idle(&ctx, &mut app);
    app.state.refresh_scalar_view(true);
    assert!(app.state.overlays().is_empty());
    assert_eq!(app.state.scalar_overlay_lines().len(), manual_lines);
    assert_eq!(app.state.scalar_contour_levels(), 3);
    assert_eq!(app.state.scalar_grid_side(), 24);
}

#[test]
fn a_manual_gradient_stays_hidden_when_the_manual_scalar_source_is_unavailable() {
    let mut state = State::new(false);
    state.set_formula("(0, 0)".into());
    state.set_scalar_source(ScalarContext::Surface, "x".into());
    let frame = state.scalar_view_frame();
    let gradient =
        vector_field_explorer::scalar_analysis::potential_field_spec("x".into(), frame).unwrap();
    state.set_gradient(Some(gradient)).unwrap();
    state.refresh_scalar_analysis(true);
    assert!(state.overlays().is_empty());
    assert!(!state.scalar_overlay_lines().is_empty());

    state.set_scalar_source(ScalarContext::Surface, "z".into());
    assert!(!state.scalar_view_applicable());
    assert!(state.scalar_overlay_lines().is_empty());
}

#[test]
fn cas_potential_draws_with_an_unavailable_manual_scalar_source() {
    let ctx = support::context();
    ctx.enable_accesskit();
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    app.layout.cas_expanded = true;
    app.layout.cas_width = 460.0;
    app.state.set_formula("(0, 0)".into());
    app.state
        .set_scalar_source(ScalarContext::Surface, "z".into());
    app.cas = scalar_panel("x*y", Operation::Differentiate, Variable::X);
    for _ in 0..4 {
        idle(&ctx, &mut app);
    }

    assert!(!app.state.scalar_view_applicable());
    click_app_button(&ctx, &mut app, "Plot contours");
    let contour = app.state.overlays().as_slice()[0].id;
    let contour_lines = app.state.scalar_overlay_lines().len();
    assert!(contour_lines > 0);
    click_app_button(&ctx, &mut app, "Add linked overlay");
    assert!(
        app.state.gradient.is_some(),
        "the CAS potential is installed"
    );
    let combined = app.state.scalar_overlay_lines().len();
    assert!(combined > contour_lines, "both CAS scene layers are drawn");

    click_app_button(&ctx, &mut app, &format!("Remove {contour}"));
    assert_eq!(app.state.plot.surface.source, "z");
    assert!(!app.state.scalar_view_applicable());
    assert_eq!(
        app.state.scalar_overlay_lines().len(),
        combined - contour_lines
    );
}

#[test]
fn closed_cas_panel_contours_follow_committed_inputs_in_scene_snapshots() {
    let ctx = support::context();
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    app.layout.cas_expanded = false;
    app.layout.controls_expanded = false;
    app.sheet.close();
    app.state.set_formula("(0, 0)".into());
    app.state
        .set_scalar_source(ScalarContext::Surface, "z".into());
    app.state
        .commit_parameters(ParameterSet {
            entries: vec![Parameter {
                name: "d".into(),
                value: 1.0,
                min: None,
                max: None,
                step: 0.1,
            }],
            ..ParameterSet::default()
        })
        .unwrap();
    app.clock.playing = false;
    app.clock.time = 0.25;
    // Persisted linked rows can bind named parameters even though the
    // built-in symbolic parser only derives expressions in x, y, z and t.
    app.state
        .add_linked_overlay(
            "x + (d+t)*y",
            vector_field_explorer::scalar_overlay::OverlayLayer::Contours,
        )
        .unwrap();
    idle(&ctx, &mut app);

    let geometry = |state: &State| {
        state
            .snapshot()
            .lines
            .iter()
            .map(|line| (line.a, line.b))
            .collect::<Vec<_>>()
    };
    let initial = geometry(&app.state);
    assert!(!initial.is_empty());
    assert!(!app.state.scalar_view_applicable());
    app.clock.time = 0.26;
    idle(&ctx, &mut app);
    assert_eq!(
        geometry(&app.state),
        initial,
        "time within the sample interval reuses geometry"
    );

    app.clock.time = 0.65;
    idle(&ctx, &mut app);
    let later = geometry(&app.state);
    assert!(!later.is_empty());
    assert_ne!(
        later, initial,
        "the committed clock changes the contour slope"
    );

    let mut parameters = app.state.parameters.clone();
    parameters.entries[0].value = 2.0;
    app.state.commit_parameters(parameters).unwrap();
    idle(&ctx, &mut app);
    let edited = geometry(&app.state);
    assert!(!edited.is_empty());
    assert_ne!(edited, later, "committing d changes the contour slope");

    let min = Vec3::new(app.state.domain.min.x, app.state.domain.min.y, -1.0);
    let max = Vec3::new(app.state.domain.max.x, app.state.domain.max.y, 3.0);
    assert!(app.state.set_domain(min, max));
    idle(&ctx, &mut app);
    let moved = geometry(&app.state);
    assert!(!moved.is_empty());
    assert_ne!(
        moved[0].0[2], edited[0].0[2],
        "the contour follows the moved slice"
    );

    app.state
        .commit_parameters(ParameterSet::default())
        .unwrap();
    idle(&ctx, &mut app);
    assert!(
        geometry(&app.state).is_empty(),
        "an unavailable binding clears the scene layer"
    );
    assert!(app.state.overlays().as_slice()[0].active_contour);
}

#[test]
fn each_cas_scene_action_is_one_undo_entry() {
    let ctx = support::context();
    ctx.enable_accesskit();
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    app.layout.controls_width = 460.0;
    app.layout.cas_expanded = true;
    app.layout.cas_width = 460.0;
    app.state.set_formula("(0, 0)".into());
    app.state
        .set_scalar_source(ScalarContext::Surface, "x".into());
    app.state.set_gradient(None).expect("clear gradient");
    app.cas = scalar_panel("x*y", Operation::Differentiate, Variable::X);
    let before_source = app.state.plot.surface.source.clone();
    let before_gradient = app.state.gradient.clone();
    for _ in 0..4 {
        idle(&ctx, &mut app);
    }
    let base = app.history_len();

    // Drive the shipped expanded CAS body: one click per scene action.
    click_app_button(&ctx, &mut app, "Plot contours");
    assert_eq!(app.state.plot.surface.source, before_source);
    let contour = &app.state.overlays().as_slice()[0];
    assert_eq!(contour.formula, "y");
    assert!(contour.active_contour);
    assert_eq!(app.state.gradient, before_gradient);
    assert_eq!(
        app.history_len(),
        base + 1,
        "the contour click is exactly one undo entry"
    );

    click_app_button(&ctx, &mut app, "Add linked overlay");
    assert!(app.state.gradient.is_some(), "the overlay is installed");
    assert_eq!(app.state.plot.surface.source, before_source);
    assert!(app.state.overlays().as_slice()[0].active_contour);
    assert_eq!(
        app.history_len(),
        base + 2,
        "the overlay click is exactly one undo entry"
    );

    // Each undo restores only the view changed by the preceding action.
    assert!(app.undo_document(&ctx), "one undo is available");
    assert_eq!(app.state.gradient, before_gradient);
    assert_eq!(
        app.state.plot.surface.source, before_source,
        "undoing the overlay does not edit the manual source"
    );
    assert_eq!(app.history_len(), base + 1);
    assert!(app.state.overlays().as_slice()[0].active_contour);

    assert!(
        app.undo_document(&ctx),
        "the contour action is still undoable"
    );
    assert_eq!(app.state.plot.surface.source, before_source);
    assert!(app.state.overlays().is_empty());
    assert_eq!(app.history_len(), base);
}

#[test]
fn the_thumbnail_samples_the_active_slice_time_and_parameters() {
    let expr = ScalarExpr::parse("x", ScalarContext::Surface).expect("surface expr");
    let xy = scalar_slice_heatmap(&expr, &xy_slice(0.0), 0.0, [0.0, 1.0], Colormap::Viridis)
        .expect("slice image");
    let higher = scalar_slice_heatmap(&expr, &xy_slice(1.0), 0.0, [0.0, 1.0], Colormap::Viridis)
        .expect("slice image");
    assert_eq!(xy.width, higher.width);
    let parameters = ParameterSet::default();
    let key = slice_preview_key(
        "x",
        &parameters,
        &xy_slice(0.0),
        0.0,
        [0.0, 1.0],
        Colormap::Viridis,
    );
    assert_ne!(
        key,
        slice_preview_key(
            "x",
            &parameters,
            &xy_slice(1.0),
            0.0,
            [0.0, 1.0],
            Colormap::Viridis
        )
    );
    assert_ne!(
        key,
        slice_preview_key(
            "x",
            &parameters,
            &xy_slice(0.0),
            0.5,
            [0.0, 1.0],
            Colormap::Viridis
        )
    );

    // A committed-parameter edit changes what the painter samples, so it must
    // change the cache identity even when the formula, slice and clock do not.
    let mut separated = parameters.clone();
    separated.entries.push(Parameter {
        name: "d".into(),
        value: 1.0,
        min: None,
        max: None,
        step: 0.1,
    });
    let before = slice_preview_key(
        "d*x",
        &separated,
        &xy_slice(0.0),
        0.0,
        [0.0, 1.0],
        Colormap::Viridis,
    );
    separated.entries[0].value = 2.0;
    let after = slice_preview_key(
        "d*x",
        &separated,
        &xy_slice(0.0),
        0.0,
        [0.0, 1.0],
        Colormap::Viridis,
    );
    assert_ne!(before, after, "the key covers the committed parameters");
}

#[test]
fn a_parameter_only_edit_rebuilds_the_scalar_thumbnail() {
    let ctx = support::context();
    let mut state = State::new(false);
    let mut parameters = ParameterSet::default();
    parameters.entries.push(Parameter {
        name: "d".into(),
        value: 1.0,
        min: None,
        max: None,
        step: 0.1,
    });
    state.commit_parameters(parameters).expect("commit d = 1");
    let mut panel = CasPanelState::default();
    let formula = "d*x";
    let frame = state.scalar_view_frame();
    let time = f64::from(state.time);
    let first_key = slice_preview_key(
        formula,
        &state.parameters,
        &frame,
        time,
        [-1.0, 1.0],
        state.colormap,
    );

    let first = render_scalar_preview_texture(&ctx, &state, &mut panel, formula)
        .expect("the first thumbnail paints");

    // Change only the committed parameter; the formula, frame and clock stay put.
    let mut next = state.parameters.clone();
    next.entries[0].value = 2.0;
    state.commit_parameters(next).expect("commit d = 2");
    let second_key = slice_preview_key(
        formula,
        &state.parameters,
        &frame,
        time,
        [-1.0, 1.0],
        state.colormap,
    );
    assert_ne!(
        first_key, second_key,
        "the cache key must cover the committed parameters the painter samples"
    );

    let second = render_scalar_preview_texture(&ctx, &state, &mut panel, formula)
        .expect("the second thumbnail paints");
    assert_ne!(
        first, second,
        "a parameter-only edit must rebuild the thumbnail instead of reusing the old texture"
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

#[test]
fn workspace_round_trip_preserves_manual_settings_and_explicit_cas_ownership() {
    let mut app = VfApp::new();
    app.state.set_formula("(0, 0)".into());
    app.state
        .set_scalar_source(ScalarContext::Surface, "y".into());
    app.state.set_scalar_contour_levels(3).unwrap();
    app.state.set_scalar_grid_side(24).unwrap();
    app.state.refresh_scalar_view(true);
    let manual_lines = app.state.scalar_overlay_lines().len();
    let mut panel = scalar_panel("x*y", Operation::Differentiate, Variable::X);
    panel
        .apply_scene(&mut app.state, 2, SceneTarget::Contours)
        .unwrap();
    let cas_id = app.state.overlays().as_slice()[0].id;
    let together = app.state.scalar_overlay_lines().len();
    assert!(manual_lines > 0 && together > manual_lines);

    let mut storage = MemoryStorage::default();
    eframe::App::save(&mut app, &mut storage);
    let mut reopened = VfApp::new_with_storage(Some(&storage));
    assert_eq!(reopened.state.plot.surface.source, "y");
    assert_eq!(reopened.state.scalar_contour_levels(), 3);
    assert_eq!(reopened.state.scalar_grid_side(), 24);
    assert_eq!(reopened.controls.scalar_contour_levels, 3);
    assert_eq!(reopened.controls.scalar_grid_side, 24);
    assert_eq!(reopened.state.overlays().as_slice()[0].id, cas_id);
    reopened.state.refresh_scalar_view(true);
    assert_eq!(reopened.state.scalar_overlay_lines().len(), together);

    let stored = storage.values.get_mut(eframe::APP_KEY).unwrap();
    assert!(stored.contains("active_contour:true,"));
    let current = stored.clone();
    *stored = stored
        .replace("active_contour:true,", "")
        .replace("scalar_grid_side:24,", "")
        .replace("scalar_contour_levels:3,", "");
    let mut legacy = VfApp::new_with_storage(Some(&storage));
    assert_eq!(legacy.state.plot.surface.source, "y");
    assert_eq!(
        legacy.state.scalar_contour_levels(),
        State::new(false).scalar_contour_levels()
    );
    assert_eq!(
        legacy.state.scalar_grid_side(),
        State::new(false).scalar_grid_side()
    );
    assert_eq!(legacy.state.overlays().as_slice()[0].id, cas_id);
    assert!(legacy.state.scalar_overlay_lines().is_empty());
    legacy.state.refresh_scalar_view(true);
    let legacy_manual_lines = legacy.state.scalar_overlay_lines().len();
    assert!(legacy_manual_lines > 0);
    assert!(legacy.state.remove_overlay(cas_id).is_some());
    assert_eq!(
        legacy.state.scalar_overlay_lines().len(),
        legacy_manual_lines
    );

    // Out-of-range stored numbers are bounded by the same limits as the live
    // controls while the valid overlay record still restores.
    storage.values.insert(
        eframe::APP_KEY.into(),
        current
            .replace("scalar_grid_side:24,", "scalar_grid_side:0,")
            .replace("scalar_contour_levels:3,", "scalar_contour_levels:999,"),
    );
    let repaired = VfApp::new_with_storage(Some(&storage));
    assert_eq!(
        repaired.state.scalar_grid_side(),
        vector_field_explorer::viewmodel::MIN_SCALAR_GRID_SIDE,
    );
    assert_eq!(
        repaired.state.scalar_contour_levels(),
        vector_field_explorer::scalar_view::MAX_CONTOUR_LEVELS as u32,
    );
    assert_eq!(repaired.state.overlays().as_slice()[0].id, cas_id);
    assert!(!repaired.state.scalar_overlay_lines().is_empty());
}
