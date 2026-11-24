//! Issue #287: the active formula shown as typeset math in a floating,
//! movable panel over the vector-field view.
//!
//! The tests drive the shipped seams: the formula bar editor stays the input
//! surface, `cas_tex::linear_to_tex` + `math_view::paint_tex_or_linear` do the
//! typesetting, `viewport_panel::show` owns dragging/clamping, and every
//! assertion goes through the app's normal UI entry point.

use eframe::egui::{self, vec2};

use crate::support;

use vector_field_explorer::VfApp;
use vector_field_explorer::parameters::{Parameter, ParameterSet};
use vector_field_explorer::source_scene::FieldMode;
use vector_field_explorer::ui::formula_typeset_panel::{
    FormulaTypesetPanelState, TOGGLE_HIDE, TOGGLE_SHOW, composed_formula, toggle_button,
};
use vector_field_explorer::viewmodel::State;

const VIEWPORT: egui::Rect =
    egui::Rect::from_min_max(egui::pos2(12.0, 72.0), egui::pos2(1252.0, 932.0));

fn context() -> egui::Context {
    let ctx = support::context();
    ctx.enable_accesskit();
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    ctx
}

fn frame_at(
    ctx: &egui::Context,
    size: egui::Vec2,
    events: Vec<egui::Event>,
    draw: impl FnMut(&mut egui::Ui),
) -> egui::FullOutput {
    let mut out = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            events,
            ..Default::default()
        },
        draw,
    );
    out.textures_delta.clear();
    out
}

fn app_output(
    ctx: &egui::Context,
    app: &mut VfApp,
    size: egui::Vec2,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let mut native = eframe::Frame::_new_kittest();
    frame_at(ctx, size, events, |ui| {
        eframe::App::ui(app, ui, &mut native)
    })
}

fn app_idle(ctx: &egui::Context, app: &mut VfApp, size: egui::Vec2, frames: usize) {
    for _ in 0..frames {
        let _ = app_output(ctx, app, size, Vec::new());
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

fn nodes(out: &egui::FullOutput) -> Vec<(egui::Role, String)> {
    out.platform_output
        .accesskit_update
        .as_ref()
        .map(|update| {
            update
                .nodes
                .iter()
                .map(|(_, node)| {
                    (
                        node.role(),
                        node.value()
                            .or_else(|| node.label())
                            .unwrap_or_default()
                            .to_owned(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn node_rect(out: &egui::FullOutput, label: &str) -> Option<egui::Rect> {
    out.platform_output
        .accesskit_update
        .as_ref()?
        .nodes
        .iter()
        .find_map(|(_, node)| {
            (node.label() == Some(label) || node.value() == Some(label))
                .then(|| node.bounds())
                .flatten()
                .map(|b| {
                    egui::Rect::from_min_max(
                        egui::pos2(b.x0 as f32, b.y0 as f32),
                        egui::pos2(b.x1 as f32, b.y1 as f32),
                    )
                })
        })
}

fn click_app_button(ctx: &egui::Context, app: &mut VfApp, label: &str) {
    let size = vec2(1280.0, 1000.0);
    let out = app_output(ctx, app, size, Vec::new());
    let rect = out
        .platform_output
        .accesskit_update
        .as_ref()
        .and_then(|update| {
            update.nodes.iter().find_map(|(_, node)| {
                (node.role() == egui::Role::Button && node.label() == Some(label))
                    .then(|| node.bounds())
                    .flatten()
                    .map(|b| {
                        egui::Rect::from_min_max(
                            egui::pos2(b.x0 as f32, b.y0 as f32),
                            egui::pos2(b.x1 as f32, b.y1 as f32),
                        )
                    })
            })
        })
        .unwrap_or_else(|| panic!("`{label}` button reachable: {:?}", texts(&out)));
    click_app_at(ctx, app, size, rect.center());
}

fn click_app_at(ctx: &egui::Context, app: &mut VfApp, size: egui::Vec2, pos: egui::Pos2) {
    let _ = app_output(ctx, app, size, vec![egui::Event::PointerMoved(pos)]);
    let _ = app_output(
        ctx,
        app,
        size,
        vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    let _ = app_output(
        ctx,
        app,
        size,
        vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
}

#[test]
fn standalone_toggle_area_click_opens_the_panel() {
    let ctx = context();
    let mut panel = FormulaTypesetPanelState::default();
    for _ in 0..3 {
        let _ = frame_at(&ctx, vec2(1280.0, 1000.0), Vec::new(), |ui| {
            vector_field_explorer::ui::formula_typeset_panel::toggle_area(
                ui.ctx(),
                VIEWPORT,
                &mut panel,
            );
        });
    }
    let out = frame_at(&ctx, vec2(1280.0, 1000.0), Vec::new(), |ui| {
        vector_field_explorer::ui::formula_typeset_panel::toggle_area(
            ui.ctx(),
            VIEWPORT,
            &mut panel,
        );
    });
    let pos = node_rect(&out, TOGGLE_SHOW).expect("toggle").center();
    let press = egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    };
    let _ = frame_at(
        &ctx,
        vec2(1280.0, 1000.0),
        vec![egui::Event::PointerMoved(pos)],
        |ui| {
            vector_field_explorer::ui::formula_typeset_panel::toggle_area(
                ui.ctx(),
                VIEWPORT,
                &mut panel,
            );
        },
    );
    let _ = frame_at(&ctx, vec2(1280.0, 1000.0), vec![press], |ui| {
        vector_field_explorer::ui::formula_typeset_panel::toggle_area(
            ui.ctx(),
            VIEWPORT,
            &mut panel,
        );
    });
    let release = egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    };
    let _ = frame_at(&ctx, vec2(1280.0, 1000.0), vec![release], |ui| {
        vector_field_explorer::ui::formula_typeset_panel::toggle_area(
            ui.ctx(),
            VIEWPORT,
            &mut panel,
        );
    });
    assert!(panel.open, "a click on the floating toggle opens the panel");
}

#[test]
fn toggle_button_opens_and_closes_without_disabling_the_formula_editor() {
    let ctx = context();
    let mut state = FormulaTypesetPanelState::default();
    let mut bar = vector_field_explorer::ui::formula_bar::FormulaBarState::default();
    let mut app_state = State::new(false);
    app_state.set_formula("(-y, x)".into());
    bar.text = app_state.active_source().to_owned();

    let out = frame_at(&ctx, vec2(1280.0, 1000.0), Vec::new(), |ui| {
        toggle_button(ui, &mut state);
        vector_field_explorer::ui::formula_bar::show(ui, &mut app_state, &mut bar, false);
    });
    assert!(
        nodes(&out)
            .iter()
            .any(|(role, text)| *role == egui::Role::Button && text == TOGGLE_SHOW),
        "the formula bar area offers an accessible open control"
    );
    assert!(
        nodes(&out).iter().any(|(_, text)| text == "(-y, x)"),
        "the editor still shows the active formula"
    );

    state.open = true;
    let out = frame_at(&ctx, vec2(1280.0, 1000.0), Vec::new(), |ui| {
        toggle_button(ui, &mut state);
        vector_field_explorer::ui::formula_bar::show(ui, &mut app_state, &mut bar, false);
    });
    assert!(
        nodes(&out)
            .iter()
            .any(|(role, text)| *role == egui::Role::Button && text == TOGGLE_HIDE),
        "an open panel offers a close control"
    );
    assert!(
        nodes(&out).iter().any(|(_, text)| text == "(-y, x)"),
        "opening the panel never replaces the editable formula field"
    );
}

#[test]
fn three_component_formula_typesets_and_follows_the_editor() {
    let ctx = context();
    let mut app = VfApp::new();
    app.formula_typeset.open = true;
    app.bar.text = "(-y, x, sin(t))".into();
    app.state.set_formula("(-y, x, sin(t))".into());
    app_idle(&ctx, &mut app, vec2(1280.0, 1000.0), 3);

    let out = app_output(&ctx, &mut app, vec2(1280.0, 1000.0), Vec::new());
    let shown = texts(&out).join("\n");
    assert!(
        shown.contains("Active formula"),
        "the panel distinguishes the active formula from a sampled value: {shown}"
    );
    assert!(
        shown.contains("-y") && shown.contains("\\sin(t)") || shown.contains("sin(t)"),
        "three components are shown: {shown}"
    );

    // The editor remains the input surface: a new formula is reflected on the
    // next frame through the same app entry point.
    app.bar.text = "(x, -y, 0)".into();
    app.state.set_formula("(x, -y, 0)".into());
    let next = app_output(&ctx, &mut app, vec2(1280.0, 1000.0), Vec::new());
    let shown = texts(&next).join("\n");
    assert!(
        shown.contains("-y"),
        "the updated formula is shown: {shown}"
    );
    assert!(
        !nodes(&next).iter().any(|(_, t)| t == "(-y, x, sin(t))"),
        "the previous formula is gone"
    );
}

#[test]
fn parameter_and_time_edits_reflect_in_the_panel() {
    let mut params = ParameterSet::default();
    params.entries.push(Parameter {
        name: "k".into(),
        value: 2.0,
        ..Parameter::default()
    });
    let formula = "(k*y, -k*x, 0)";
    let mut state = State::new(false);
    state.parameters = params.clone();
    state.set_formula(formula.into());
    let first = composed_formula(formula, &state);
    assert!(
        first.iter().any(|line| line.contains("2")),
        "the committed parameter value is substituted: {first:?}"
    );

    params.entries[0].value = 5.0;
    state.parameters = params.clone();
    let edited = composed_formula(formula, &state);
    assert_ne!(first, edited, "a parameter edit changes the shown formula");
    assert!(
        edited.iter().any(|line| line.contains("5")),
        "the new parameter value appears: {edited:?}"
    );

    state.set_formula("(sin(t), 0, 0)".into());
    state.set_time(1.5);
    let timed = composed_formula("(sin(t), 0, 0)", &state);
    assert!(
        timed.iter().any(|line| line.contains("1.5")),
        "the shared clock time is substituted: {timed:?}"
    );
}

#[test]
fn fallback_is_labelled_never_claimed_as_typeset() {
    let ctx = context();
    let mut app = VfApp::new();
    app.formula_typeset.open = true;
    // Unsupported notation for the TeX seam: a bracketed component.
    app.bar.text = "([x], y, 0)".into();
    app.state.set_formula("([x], y, 0)".into());
    app_idle(&ctx, &mut app, vec2(1280.0, 1000.0), 3);
    let out = app_output(&ctx, &mut app, vec2(1280.0, 1000.0), Vec::new());
    let shown = texts(&out).join("\n");
    assert!(
        shown.contains("linear fallback") && shown.lines().any(|line| line == "[x]"),
        "a failed conversion states the fallback: {shown}"
    );
}

#[test]
fn header_drag_does_not_reach_the_field_and_reset_restores_default() {
    let ctx = context();
    let mut panel = FormulaTypesetPanelState {
        open: true,
        ..Default::default()
    };
    let mut app_state = State::new(false);
    let bar = vector_field_explorer::ui::formula_bar::FormulaBarState {
        text: "(-y, x, 0)".into(),
        ..Default::default()
    };
    app_state.set_formula("(-y, x, 0)".into());

    let mut field_dragged = false;
    // One frame through the real overlay seam, with a click-and-drag field
    // beneath it. Returns the Area rect so placement can be compared exactly.
    fn frame(
        ctx: &egui::Context,
        panel: &mut FormulaTypesetPanelState,
        formula: &str,
        state: &State,
        field_dragged: &mut bool,
        events: Vec<egui::Event>,
    ) -> egui::Rect {
        let native = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(1280.0, 1000.0),
            )),
            events,
            ..Default::default()
        };
        let mut rect = egui::Rect::ZERO;
        let mut out = ctx.run_ui(native, |ui| {
            let (_, field) = ui.allocate_exact_size(VIEWPORT.size(), egui::Sense::click_and_drag());
            *field_dragged |= field.dragged();
            let response = vector_field_explorer::ui::formula_typeset_panel::show(
                ui.ctx(),
                VIEWPORT,
                panel,
                formula,
                state,
            );
            rect = response.response.rect;
        });
        out.textures_delta.clear();
        rect
    }

    let pointer = |pos: egui::Pos2, pressed: bool| {
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    };

    let _ = frame(
        &ctx,
        &mut panel,
        &bar.text,
        &app_state,
        &mut field_dragged,
        Vec::new(),
    );
    let _ = frame(
        &ctx,
        &mut panel,
        &bar.text,
        &app_state,
        &mut field_dragged,
        Vec::new(),
    );
    let before = frame(
        &ctx,
        &mut panel,
        &bar.text,
        &app_state,
        &mut field_dragged,
        Vec::new(),
    );
    let start = before.right_top() + egui::vec2(-3.0, 3.0);
    let end = start + egui::vec2(-100.0, 75.0);
    frame(
        &ctx,
        &mut panel,
        &bar.text,
        &app_state,
        &mut field_dragged,
        pointer(start, true),
    );
    frame(
        &ctx,
        &mut panel,
        &bar.text,
        &app_state,
        &mut field_dragged,
        vec![egui::Event::PointerMoved(end)],
    );
    assert!(
        !field_dragged,
        "a drag on the panel header must not pan or orbit the field"
    );
    frame(
        &ctx,
        &mut panel,
        &bar.text,
        &app_state,
        &mut field_dragged,
        pointer(end, false),
    );
    let moved = frame(
        &ctx,
        &mut panel,
        &bar.text,
        &app_state,
        &mut field_dragged,
        Vec::new(),
    );
    assert_ne!(
        before.min, moved.min,
        "the panel followed the header drag: {before:?} -> {moved:?}"
    );
    assert!(
        VIEWPORT.contains_rect(moved),
        "the panel stays inside the viewport: {moved:?}"
    );

    panel.reset_position();
    let restored = frame(
        &ctx,
        &mut panel,
        &bar.text,
        &app_state,
        &mut field_dragged,
        Vec::new(),
    );
    assert_eq!(
        before.min, restored.min,
        "reset restores the default placement"
    );
}

#[test]
fn wide_and_compact_layouts_keep_panel_and_editor_inside_the_viewport() {
    for size in [vec2(1280.0, 900.0), vec2(390.0, 760.0)] {
        let ctx = context();
        let mut app = VfApp::new();
        app.formula_typeset.open = true;
        app.bar.text = "(-y, x, 0)".into();
        app.state.set_formula("(-y, x, 0)".into());
        app_idle(&ctx, &mut app, size, 3);
        let out = app_output(&ctx, &mut app, size, Vec::new());
        let nodes = nodes(&out);
        assert!(
            nodes.iter().any(|(_, t)| t == "Typeset formula"),
            "the panel is reachable at {size:?}"
        );
        assert!(
            nodes
                .iter()
                .any(|(role, t)| *role == egui::Role::TextInput && t == "(-y, x, 0)"),
            "the formula editor stays usable at {size:?}"
        );
        for (role, text) in &nodes {
            if ["Typeset formula", "Close typeset formula", "Reset position"]
                .contains(&text.as_str())
            {
                let rect = node_rect(&out, text).expect("headings have bounds");
                assert!(
                    size.x >= rect.max.x && size.y >= rect.max.y,
                    "{role:?} {text} stays inside {size:?}: {rect:?}"
                );
            }
        }
    }
}

#[test]
fn app_wire_control_opens_and_closes_the_panel() {
    let ctx = context();
    let mut app = VfApp::new();
    app_idle(&ctx, &mut app, vec2(1280.0, 1000.0), 3);
    let out = app_output(&ctx, &mut app, vec2(1280.0, 1000.0), Vec::new());
    assert!(
        nodes(&out)
            .iter()
            .any(|(role, t)| *role == egui::Role::Button && t == TOGGLE_SHOW),
        "the app exposes the wire control"
    );
    click_app_button(&ctx, &mut app, TOGGLE_SHOW);
    assert!(app.formula_typeset.open, "the app toggle opens the panel");
    app_idle(&ctx, &mut app, vec2(1280.0, 1000.0), 3);
    let out = app_output(&ctx, &mut app, vec2(1280.0, 1000.0), Vec::new());
    assert!(
        nodes(&out).iter().any(|(_, t)| t == "Typeset formula"),
        "the panel is visible after toggling"
    );
    click_app_button(&ctx, &mut app, "Close typeset formula");
    let out = app_output(&ctx, &mut app, vec2(1280.0, 1000.0), Vec::new());
    assert!(
        !app.formula_typeset.open,
        "the close control dismisses the panel"
    );
    assert!(
        !nodes(&out).iter().any(|(_, t)| t == "Typeset formula"),
        "the dismissed panel is gone"
    );
}

/// F1 regression: in a non-Formula vector mode the formula bar short-circuits
/// before its mirror, so `bar.text` stays empty/stale. The panel must never
/// present that as the active formula; it must show the authoritative committed
/// source with a label making clear the bar is not the live editor here.
#[test]
fn non_formula_vector_mode_never_shows_empty_or_stale_formula_as_active() {
    let ctx = context();
    let mut app = VfApp::new();
    // A distinct committed formula so a stale bar can be detected.
    app.state.set_formula("(-y, x, 0)".into());
    // The bar mirror never ran in this mode: leave it at its empty default.
    app.bar.text.clear();
    assert!(app.state.set_field_mode(FieldMode::Sources));
    assert!(app.state.is_vector_mode());
    app.formula_typeset.open = true;
    app_idle(&ctx, &mut app, vec2(1280.0, 1000.0), 3);

    let out = app_output(&ctx, &mut app, vec2(1280.0, 1000.0), Vec::new());
    let shown = texts(&out).join("\n");
    // The committed formula is present, not an empty or stale bar string.
    assert!(
        shown.contains("-y") && shown.contains(" x") || shown.contains("-y, x"),
        "the authoritative committed formula is shown in Sources mode: {shown}"
    );
    // The separate reference must not be mistaken for an equation of Sources.
    assert!(
        shown.contains("Active equation unavailable for this mode")
            && shown.contains("Inactive Formula reference"),
        "the Sources equation is unavailable and the Formula reference is inactive: {shown}"
    );
    // The bar editor itself still holds the empty draft; it is untouched.
    assert!(
        app.bar.text.is_empty(),
        "the formula bar draft is not overwritten by the panel"
    );
}

/// F2 regression: a committed value smaller than 0.00005 must not be rendered
/// as `0`. Full-precision formatting, matching the field evaluator's
/// `format!("{value}")` substitution seam, keeps it non-zero and consistent.
#[test]
fn tiny_substituted_value_matches_field_evaluator_precision() {
    let mut params = ParameterSet::default();
    params.entries.push(Parameter {
        name: "k".into(),
        value: 0.00003,
        ..Parameter::default()
    });
    let formula = "(k*x, 0, 0)";
    let mut state = State::new(false);
    state.parameters = params.clone();
    state.set_formula(formula.into());
    let composed = composed_formula(formula, &state);
    let field_literal = format!("{}", 0.00003_f64);
    assert!(
        composed.iter().any(|line| line.contains(&field_literal)),
        "the panel uses the same full precision as the field evaluator ({field_literal}): {composed:?}"
    );
    assert!(
        !composed
            .iter()
            .any(|line| line.trim_start_matches("(").starts_with("0,")),
        "a sub-0.00005 value is not collapsed to zero: {composed:?}"
    );
}

#[test]
fn wrapped_vector_keeps_nested_component_parentheses() {
    let state = State::new(false);
    for formula in ["(sin((x+y)), (x+y), 0)", "sin((x+y)), (x+y), 0"] {
        assert_eq!(
            composed_formula(formula, &state),
            ["sin((x+y))", "(x+y)", "0"]
        );
    }
    assert_eq!(composed_formula("(sin(x))", &state), ["(sin(x))"]);
    assert_eq!(composed_formula("(x), (y)", &state), ["(x)", "(y)"]);
}

#[test]
fn a_fraction_typesets_in_the_scrollable_equation_panel() {
    let ctx = context();
    let mut app = VfApp::new();
    let formula = "(x+1)/(y+2), y, 0";
    let size = vec2(1280.0, 1000.0);
    app.bar.text = formula.to_owned();
    app.state.set_formula(formula.to_owned());
    assert!(app.state.parse_error.is_empty());
    assert_eq!(app.state.field.dimension, 3);
    app_idle(&ctx, &mut app, size, 3);
    click_app_button(&ctx, &mut app, TOGGLE_SHOW);
    app_idle(&ctx, &mut app, size, 3);
    let output = app_output(&ctx, &mut app, size, Vec::new());
    let shown = texts(&output).join("\n");
    assert!(app.formula_typeset.open && shown.contains("Active formula"));
    assert!(
        !shown.contains("linear fallback"),
        "a supported fraction should typeset: {shown}"
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn a_replaced_cas_vector_typesets_in_the_floating_equation_panel() {
    use vector_field_explorer::ui::cas_panel::Operation;

    let ctx = context();
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    app.layout.cas_expanded = true;
    app.layout.cas_width = 460.0;
    app.state.set_mode(true);
    app.cas.input = "x*y".to_owned();
    app.cas.operation = Operation::Gradient;
    let size = vec2(1280.0, 1000.0);
    app_idle(&ctx, &mut app, size, 3);
    assert_eq!(app.cas.copy_text(), Some("(y, x, 0)"));

    click_app_button(&ctx, &mut app, "Replace field");
    assert_eq!(app.state.active_source(), "(y, x, 0)");
    assert!(app.state.parse_error.is_empty());
    assert_eq!(app.state.field.dimension, 3);
    click_app_button(&ctx, &mut app, TOGGLE_SHOW);
    app_idle(&ctx, &mut app, size, 3);
    let output = app_output(&ctx, &mut app, size, Vec::new());
    let shown = texts(&output).join("\n");
    assert!(app.formula_typeset.open && shown.contains("Active formula"));
    assert!(
        !shown.contains("linear fallback"),
        "the accepted CAS vector should typeset: {shown}"
    );
}

#[test]
fn equation_panel_shows_the_committed_typed_ode_rhs() {
    let ctx = context();
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    app.state.set_formula("(91*x, 92*y, 93*z)".into());
    let mut scene = app.state.ode.clone();
    scene.dimension = 3;
    scene.equations = ["2*y".into(), "-x".into(), "t-z".into()];
    app.state
        .set_ode_scene(scene)
        .expect("valid typed RHS fixture");
    let wide = vec2(1280.0, 1000.0);
    app_idle(&ctx, &mut app, wide, 3);
    click_app_label(&ctx, &mut app, wide, "ODE");
    assert_eq!(app.state.field_mode, FieldMode::Ode);
    click_app_button(&ctx, &mut app, "Open ODE");
    app_idle(&ctx, &mut app, wide, 3);
    replace_editor_text(&ctx, &mut app, wide, "-x", "-3*x");
    assert_eq!(app.state.ode.equations[1], "-3*x");
    click_app_button(&ctx, &mut app, TOGGLE_SHOW);
    app_idle(&ctx, &mut app, wide, 3);
    assert!(app.formula_typeset.open);
    let out = app_output(&ctx, &mut app, wide, Vec::new());
    let painted = texts(&out);
    let shown = painted.join("\n");
    for expected in [
        "Live ODE RHS · 3D",
        "produce the trajectory",
        "integration time",
        "dx/dt = 2*y",
        "dy/dt = -3*x",
        "dz/dt = t-z",
    ] {
        assert!(shown.contains(expected), "missing {expected:?}: {shown}");
    }
    for dormant in ["91", "92", "93"] {
        assert!(
            !painted.iter().any(|text| text == dormant),
            "dormant Formula coefficient {dormant}: {shown}"
        );
    }
}

fn click_app_label(ctx: &egui::Context, app: &mut VfApp, size: egui::Vec2, label: &str) {
    let out = app_output(ctx, app, size, Vec::new());
    let pos = node_rect(&out, label)
        .unwrap_or_else(|| panic!("{label:?} reachable: {:?}", nodes(&out)))
        .center();
    click_app_at(ctx, app, size, pos);
}

fn replace_editor_text(
    ctx: &egui::Context,
    app: &mut VfApp,
    size: egui::Vec2,
    previous: &str,
    replacement: &str,
) {
    click_app_label(ctx, app, size, previous);
    let key = |key, modifiers| egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    };
    let _ = app_output(
        ctx,
        app,
        size,
        vec![
            key(egui::Key::A, egui::Modifiers::COMMAND),
            egui::Event::Paste(replacement.into()),
            key(egui::Key::Enter, egui::Modifiers::NONE),
        ],
    );
}

#[test]
fn equation_panel_labels_the_previous_typed_rhs_after_a_rejected_edit() {
    let ctx = context();
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    let wide = vec2(1280.0, 1000.0);
    app_idle(&ctx, &mut app, wide, 3);
    click_app_label(&ctx, &mut app, wide, "ODE");
    click_app_button(&ctx, &mut app, "Open ODE");
    app_idle(&ctx, &mut app, wide, 3);
    let solved = app.state.ode_solution.clone().expect("initial solution");
    let committed = app.state.ode.clone();

    replace_editor_text(&ctx, &mut app, wide, "-x", "missing_axis(");
    assert!(
        app.state.ode_error.is_some(),
        "the ODE editor rejects the draft"
    );
    assert_eq!(app.state.ode, committed);
    assert!(std::sync::Arc::ptr_eq(
        app.state.ode_solution.as_ref().unwrap(),
        &solved
    ));

    click_app_button(&ctx, &mut app, TOGGLE_SHOW);
    app_idle(&ctx, &mut app, wide, 3);
    let out = app_output(&ctx, &mut app, wide, Vec::new());
    let shown = texts(&out).join("\n");
    assert!(shown.contains("Last committed ODE RHS"), "{shown}");
    assert!(shown.contains("dy/dt = -x"), "{shown}");
    assert!(
        shown.contains("trajectory comes from the last valid solve"),
        "{shown}"
    );
    assert!(!shown.contains("Live ODE RHS"), "{shown}");
    assert!(!shown.contains("dy/dt = missing_axis"), "{shown}");
    click_app_button(&ctx, &mut app, "Open in Formula");
    assert_eq!(app.state.field_mode, FieldMode::Formula);
    assert_eq!(app.state.active_source(), "(y, -x)");
}

#[test]
fn compact_equation_panel_shows_only_the_two_active_ode_derivatives() {
    let ctx = context();
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    let compact = vec2(640.0, 900.0);
    app_idle(&ctx, &mut app, compact, 3);
    assert_eq!(
        app.layout_mode,
        vector_field_explorer::ui::layout::LayoutMode::Compact
    );
    click_app_label(&ctx, &mut app, compact, "ODE");
    assert_eq!(app.state.field_mode, FieldMode::Ode);
    click_app_label(&ctx, &mut app, compact, TOGGLE_SHOW);
    app_idle(&ctx, &mut app, compact, 3);
    assert!(app.formula_typeset.open);
    let shown = texts(&app_output(&ctx, &mut app, compact, Vec::new())).join("\n");
    assert!(shown.contains("Live ODE RHS · 2D"), "{shown}");
    assert!(
        shown.contains("dx/dt = y") && shown.contains("dy/dt = -x"),
        "{shown}"
    );
    assert!(!shown.contains("dz/dt"), "{shown}");
}

#[test]
fn current_field_ode_reports_an_unavailable_equation_and_the_last_valid_trajectory() {
    let ctx = context();
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    let source = "(t, -3*x, 0)";
    app.state.set_formula(source.into());
    let wide = vec2(1280.0, 1000.0);
    app_idle(&ctx, &mut app, wide, 3);
    click_app_label(&ctx, &mut app, wide, "ODE");
    click_app_button(&ctx, &mut app, "Open ODE");
    app_idle(&ctx, &mut app, wide, 3);
    click_app_label(&ctx, &mut app, wide, "Current field");
    assert_eq!(
        app.state.ode.source,
        vector_field_explorer::ode::OdeSource::CurrentField
    );
    let solution = app
        .state
        .ode_solution
        .clone()
        .expect("valid Current field solve");
    let sample = solution
        .sample_at(0, 1.0)
        .expect("trajectory at one second");
    assert!(
        (sample.position[0] - 1.5).abs() < 0.001,
        "dx/dt=t integrates from the default seed x=1"
    );
    app.clock.set_playing(false);
    app.clock.seek(7.25);
    click_app_button(&ctx, &mut app, TOGGLE_SHOW);
    app_idle(&ctx, &mut app, wide, 3);
    assert_eq!(app.state.time, 7.25);
    let valid = texts(&app_output(&ctx, &mut app, wide, Vec::new()));

    click_app_label(&ctx, &mut app, wide, "Formula");
    replace_editor_text(&ctx, &mut app, wide, source, "invalid_formula_marker(");
    assert!(!app.state.parse_error.is_empty());
    click_app_label(&ctx, &mut app, wide, "ODE");
    app_idle(&ctx, &mut app, wide, 3);
    assert!(app.formula_typeset.open);
    assert!(
        app.state.ode_error.is_some(),
        "Current field rejects the Formula edit"
    );
    assert!(std::sync::Arc::ptr_eq(
        app.state.ode_solution.as_ref().unwrap(),
        &solution
    ));
    let rejected = texts(&app_output(&ctx, &mut app, wide, Vec::new()));

    for output in [&valid, &rejected] {
        assert!(
            output
                .iter()
                .any(|text| text.contains("Equation unavailable for ODE using Current field")),
            "exact RHS unavailable; valid paint={valid:?}, after rejected edit={rejected:?}"
        );
        assert!(
            !output
                .iter()
                .any(|text| text.contains("Committed vector formula")
                    || text.contains("linear fallback")),
            "the Equation panel must not show a substituted or rejected Formula: {output:?}"
        );
    }
    assert!(!valid.iter().any(|text| text == "7.25"), "{valid:?}");
    assert!(
        !rejected
            .iter()
            .any(|text| text == "invalid_formula_marker("),
        "{rejected:?}"
    );
    let last_solve = "The trajectory comes from the last valid solve.";
    assert!(!valid.iter().any(|text| text == last_solve), "{valid:?}");
    assert!(
        rejected.iter().any(|text| text == last_solve),
        "{rejected:?}"
    );
}

#[test]
fn open_in_formula_transfers_the_committed_ode_rhs() {
    let ctx = context();
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    let mut scene = app.state.ode.clone();
    scene.equations = ["y+t".into(), "-2*x".into(), "z".into()];
    app.state.set_ode_scene(scene).expect("valid typed RHS");
    let size = vec2(1280.0, 1000.0);
    app_idle(&ctx, &mut app, size, 3);
    click_app_label(&ctx, &mut app, size, "ODE");
    click_app_button(&ctx, &mut app, TOGGLE_SHOW);
    app_idle(&ctx, &mut app, size, 3);
    app.clock.set_playing(false);
    app.clock.seek(0.75);
    let shown = texts(&app_output(&ctx, &mut app, size, Vec::new())).join("\n");
    assert!(shown.contains("shared playback time for t"), "{shown}");
    assert!(shown.contains("The trajectory stays in ODE"), "{shown}");
    click_app_button(&ctx, &mut app, "Open in Formula");
    assert_eq!(app.state.field_mode, FieldMode::Formula);
    assert_eq!(app.state.active_source(), "(y+t, -2*x)");
    assert_eq!(app.state.field.dimension, 2);
    assert!(app.state.parse_error.is_empty());
    assert_eq!(app.state.time, 0.75);
    for (point, time) in [
        (glam::Vec3::new(2.0, 3.0, 9.0), app.state.time),
        (glam::Vec3::new(-1.0, 4.0, 9.0), 1.25),
    ] {
        let expected = glam::Vec3::new(point.y + time, -2.0 * point.x, 0.0);
        assert_eq!(app.state.field.eval3_at(point, time), Some(expected));
    }
}

#[test]
fn ode_transfer_does_not_bind_a_formula_parameter_to_the_rhs() {
    let ctx = context();
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    app.state
        .commit_parameters(ParameterSet {
            entries: vec![Parameter {
                name: "gain".into(),
                value: 2.0,
                step: 1.0,
                ..Parameter::default()
            }],
            ..ParameterSet::default()
        })
        .expect("valid Formula parameters");
    let mut scene = app.state.ode.clone();
    scene.equations[0] = "gain*x".into();
    // The solver may retain the finite seed when an RHS cannot be evaluated.
    // Formula could bind gain, which would make this a different model.
    app.state
        .set_ode_scene(scene)
        .expect("ODE retains the finite seed");
    let size = vec2(1280.0, 1000.0);
    app_idle(&ctx, &mut app, size, 3);
    click_app_label(&ctx, &mut app, size, "ODE");
    click_app_button(&ctx, &mut app, TOGGLE_SHOW);
    app_idle(&ctx, &mut app, size, 3);
    let shown = texts(&app_output(&ctx, &mut app, size, Vec::new())).join("\n");
    assert!(
        shown.contains("These ODE equations cannot be opened in Formula."),
        "{shown}"
    );
    assert!(!shown.contains("Open in Formula"), "{shown}");
}

#[test]
fn committed_ode_rhs_transfers_without_a_solved_trajectory() {
    for solve_error in [None, Some("trajectory solve failed".to_owned())] {
        let ctx = context();
        let mut app = VfApp::new();
        app.layout.help_visible = false;
        let mut scene = app.state.ode.clone();
        scene.equations = ["y+t".into(), "-2*x".into(), "0".into()];
        app.state.set_ode_scene(scene).expect("valid typed RHS");
        let size = vec2(1280.0, 1000.0);
        app_idle(&ctx, &mut app, size, 3);
        click_app_label(&ctx, &mut app, size, "ODE");
        app_idle(&ctx, &mut app, size, 3);

        // Fixture: the scene is still committed, but its runtime trajectory
        // is unavailable. The Equation launcher and transfer are normal clicks.
        app.state.ode_solution = None;
        let status = if solve_error.is_some() {
            "Trajectory unavailable. The ODE workbench reports an error."
        } else {
            "No solved trajectory is available."
        };
        app.state.ode_error = solve_error;
        click_app_button(&ctx, &mut app, TOGGLE_SHOW);
        app_idle(&ctx, &mut app, size, 3);
        assert!(app.state.ode_solution.is_none());

        let shown = texts(&app_output(&ctx, &mut app, size, Vec::new())).join("\n");
        assert!(shown.contains("Committed ODE RHS · 2D"), "{shown}");
        assert!(shown.contains("dx/dt = y+t"), "{shown}");
        assert!(shown.contains(status), "{shown}");
        click_app_button(&ctx, &mut app, "Open in Formula");
        assert_eq!(app.state.field_mode, FieldMode::Formula);
        assert_eq!(app.state.active_source(), "(y+t, -2*x)");
        assert!(app.state.parse_error.is_empty());
    }
}
