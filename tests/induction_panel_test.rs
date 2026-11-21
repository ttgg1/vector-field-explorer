//! Task 3: the responsive Induction panel, the adapter selector and the help
//! rows. Widget assertions use the same headless-accesskit helpers as
//! `plot_ui_test.rs`; the rail check drives a real `VfApp` frame.

use eframe::egui;

use crate::support;

use vector_field_explorer::VfApp;
use vector_field_explorer::source_scene::FieldMode;
use vector_field_explorer::ui::formula_bar::{self, FormulaBarState};
use vector_field_explorer::ui::help_overlay::induction_help_rows;
use vector_field_explorer::ui::induction_panel::{self, InductionPanelState};
use vector_field_explorer::ui::layout::SheetState;
use vector_field_explorer::ui::sidebar::{self, Controls};
use vector_field_explorer::viewmodel::State;

struct WidgetNode {
    role: egui::Role,
    label: String,
    rect: egui::Rect,
    disabled: bool,
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
                role: node.role(),
                label,
                rect: egui::Rect::from_min_max(
                    egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                    egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                ),
                disabled: node.is_disabled(),
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

fn run_frame(
    ctx: &egui::Context,
    width: f32,
    height: f32,
    events: Vec<egui::Event>,
    draw: impl FnMut(&mut egui::Ui),
) -> egui::FullOutput {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, height),
            )),
            events,
            ..Default::default()
        },
        draw,
    );
    output.textures_delta.clear();
    output
}

#[test]
fn induction_panel_wide_and_compact_expose_all_controls() {
    for (width, compact) in [(1200.0, false), (390.0, true)] {
        let ctx = context();
        let mut panel = InductionPanelState::default();
        let mut state = State::new(true);
        state.set_field_mode(FieldMode::Induction);
        let output = run_frame(&ctx, width, 900.0, Vec::new(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let _ = induction_panel::show(ui, &mut panel, &mut state, compact);
            });
        });
        let nodes = nodes(&output);
        for label in [
            "Current loop",
            "Finite solenoid",
            "Amplitude",
            "Frequency",
            "Both",
            "Fit",
        ] {
            assert!(has(&nodes, label), "{width} pt is missing {label}");
        }
        assert!(has(&nodes, "Radius"));
        assert!(has(&nodes, "Advanced"));
        assert_eq!(
            find(&nodes, "Fit").expect("Fit button").role,
            egui::Role::Button
        );
    }
}

#[test]
fn formula_bar_selects_induction_and_hides_the_formula_editor() {
    let ctx = context();
    let mut state = State::new(true);
    let mut bar = FormulaBarState::default();
    let mut output = run_frame(&ctx, 1200.0, 200.0, Vec::new(), |ui| {
        formula_bar::show(ui, &mut state, &mut bar, false);
    });
    let induction = find(&nodes(&output), "Induction")
        .expect("Induction selector")
        .rect;
    run_frame(
        &ctx,
        1200.0,
        200.0,
        pointer(induction.center(), true),
        |ui| {
            formula_bar::show(ui, &mut state, &mut bar, false);
        },
    );
    output = run_frame(
        &ctx,
        1200.0,
        200.0,
        pointer(induction.center(), false),
        |ui| {
            formula_bar::show(ui, &mut state, &mut bar, false);
        },
    );
    assert_eq!(state.field_mode, FieldMode::Induction);
    let nodes = nodes(&output);
    assert!(
        !has(&nodes, "Field formula"),
        "the formula editor is hidden"
    );
    assert!(has(&nodes, "Induction scene summary"));
}

#[test]
fn open_induction_button_requests_the_panel() {
    let ctx = context();
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Induction);
    let mut bar = FormulaBarState::default();
    let output = run_frame(&ctx, 1200.0, 200.0, Vec::new(), |ui| {
        formula_bar::show(ui, &mut state, &mut bar, false);
    });
    let button = find(&nodes(&output), "Open induction")
        .expect("Open induction button")
        .rect
        .center();
    run_frame(&ctx, 1200.0, 200.0, pointer(button, true), |ui| {
        formula_bar::show(ui, &mut state, &mut bar, false);
    });
    run_frame(&ctx, 1200.0, 200.0, pointer(button, false), |ui| {
        formula_bar::show(ui, &mut state, &mut bar, false);
    });
    assert!(bar.open_induction, "the button requests the panel");
}

#[test]
fn edits_validate_and_keep_the_last_valid_scene() {
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Induction);
    let valid = state.induction.clone();
    let mut panel = InductionPanelState::default();
    panel.scene.radius = -1.0;
    assert!(!panel.commit(&mut state));
    assert_eq!(state.induction, valid, "invalid geometry is not committed");
    assert!(panel.error.as_deref().unwrap_or("").contains("radius"));
    panel.scene = valid.clone();
    panel.scene.amplitude = 2.5;
    assert!(panel.commit(&mut state));
    assert_eq!(state.induction.amplitude, 2.5);
    assert!(panel.error.is_none());
}

#[test]
fn help_explains_assumptions_clock_and_arrow_scope() {
    let text = induction_help_rows()
        .iter()
        .map(|row| format!("{} {}", row.input, row.action))
        .collect::<Vec<_>>()
        .join(" ");
    for needle in [
        "quasistatic",
        "vector potential",
        "shared clock",
        "arrows only",
        "arbitrary B",
    ] {
        assert!(text.contains(needle), "help is missing {needle}");
    }
}

#[test]
fn features_controls_are_disabled_in_induction_mode() {
    let ctx = context();
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Induction);
    let mut controls = Controls::default();
    let output = run_frame(&ctx, 400.0, 1200.0, Vec::new(), |ui| {
        egui::CentralPanel::default().show(ui, |ui| {
            sidebar::show(
                ui,
                &mut state,
                &mut controls,
                &mut vector_field_explorer::seed_layout_library::SeedLayoutLibrary::default(),
            );
        });
    });
    let painted = nodes(&output);
    let streamlines = find(&painted, "Streamlines").expect("Streamlines control");
    assert!(
        streamlines.disabled,
        "streamline controls are disabled in Induction"
    );
    assert!(
        has(
            &painted,
            "Arrows only: streamlines, particles and click seeding stay off in Induction.",
        ),
        "Induction features explanation is shown"
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn cas_apply_refuses_induction_mode() {
    use vector_field_explorer::ui::cas_panel::{CasPanelState, Operation};
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Induction);
    let formula = state.formula.clone();
    let mut panel = CasPanelState::default();
    panel.input = "x^2+y^2+z^2".into();
    panel.operation = Operation::Gradient;
    panel.refresh(3);
    assert!(!panel.apply(&mut state, 3));
    assert_eq!(state.formula, formula);
}

#[test]
fn app_applies_the_open_induction_request_in_both_layouts() {
    let ctx = context();
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    app.state.set_field_mode(FieldMode::Induction);
    let mut frame = eframe::Frame::_new_kittest();
    let mut render = |app: &mut VfApp, width: f32| {
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, 900.0),
                )),
                ..Default::default()
            },
            |ui| eframe::App::ui(app, ui, &mut frame),
        );
        out.textures_delta.clear();
        out
    };
    app.bar.open_induction = true;
    render(&mut app, 1200.0);
    assert!(
        app.layout.induction_expanded,
        "wide request expands the rail"
    );

    app.layout.induction_expanded = false;
    app.bar.open_induction = true;
    render(&mut app, 390.0);
    assert_eq!(
        app.sheet,
        SheetState::Induction,
        "compact request opens the induction sheet"
    );
}

#[test]
fn induction_rail_expands_a_real_app_frame() {
    let ctx = context();
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    app.state.set_field_mode(FieldMode::Induction);
    let mut frame = eframe::Frame::_new_kittest();
    let mut render = |app: &mut VfApp, events: Vec<egui::Event>| {
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 900.0),
                )),
                events,
                ..Default::default()
            },
            |ui| eframe::App::ui(app, ui, &mut frame),
        );
        out.textures_delta.clear();
        out
    };
    let mut out = render(&mut app, Vec::new());
    for _ in 0..3 {
        out = render(&mut app, Vec::new());
    }
    let rail = rail_bounds(&out, "Induction").expect("Induction rail");
    render(&mut app, pointer(rail.center(), true));
    let out = render(&mut app, pointer(rail.center(), false));
    assert!(app.layout.induction_expanded, "the rail expands the panel");
    assert!(
        has(&nodes(&out), "Reset induction section"),
        "the expanded rail paints the induction body"
    );
}

fn rail_bounds(out: &egui::FullOutput, label: &str) -> Option<egui::Rect> {
    fn find(shape: &egui::Shape, label: &str) -> Option<egui::Rect> {
        match shape {
            egui::Shape::Text(text) if text.galley.text() == label && text.angle != 0.0 => Some(
                text.galley
                    .mesh_bounds
                    .rotate_bb(egui::emath::Rot2::from_angle(text.angle))
                    .translate(text.pos.to_vec2()),
            ),
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| find(shape, label)),
            _ => None,
        }
    }
    out.shapes
        .iter()
        .find_map(|shape| find(&shape.shape, label))
}
