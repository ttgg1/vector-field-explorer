//! Task 3: the responsive ODE workbench panel, its charts, the fourth
//! formula-bar selector and the ODE help rows. Widget assertions use the same
//! headless-accesskit helpers as `induction_panel_test.rs`; the app checks
//! drive real `VfApp` frames at both layout widths.

use eframe::egui;

use crate::support;

use vector_field_explorer::ode::{OdeChart, OdeSource};
use vector_field_explorer::source_scene::FieldMode;
use vector_field_explorer::ui::formula_bar::{self, FormulaBarState};
use vector_field_explorer::ui::layout::{DEFAULT_ODE_WIDTH, LayoutState, SheetState};
use vector_field_explorer::ui::ode_panel::{self, OdePanelState, ode_help_rows};
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

fn checkbox<'a>(nodes: &'a [WidgetNode], label: &str) -> Option<&'a WidgetNode> {
    nodes
        .iter()
        .find(|node| node.role == egui::Role::CheckBox && node.label == label)
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

/// Draw the panel the way the wide rail (300 pt) or the compact sheet does.
fn panel_frame(
    ctx: &egui::Context,
    width: f32,
    height: f32,
    events: Vec<egui::Event>,
    state: &mut State,
    panel: &mut OdePanelState,
    compact: bool,
) -> egui::FullOutput {
    run_frame(ctx, width, height, events, |ui| {
        if compact {
            egui::CentralPanel::default().show(ui, |ui| {
                let _ = ode_panel::show(ui, panel, state, true);
            });
        } else {
            egui::Panel::left("ode_test_panel")
                .exact_size(300.0)
                .resizable(false)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("ode_test_body")
                        .show(ui, |ui| {
                            let _ = ode_panel::show(ui, panel, state, false);
                        });
                });
        }
    })
}

fn click_panel(
    ctx: &egui::Context,
    width: f32,
    height: f32,
    pos: egui::Pos2,
    state: &mut State,
    panel: &mut OdePanelState,
    compact: bool,
) {
    for pressed in [true, false] {
        panel_frame(
            ctx,
            width,
            height,
            pointer(pos, pressed),
            state,
            panel,
            compact,
        );
    }
}

#[test]
fn ode_panel_exposes_equations_seeds_and_actions() {
    let ctx = context();
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Ode);
    let mut panel = OdePanelState::default();
    let out = panel_frame(
        &ctx,
        1200.0,
        900.0,
        Vec::new(),
        &mut state,
        &mut panel,
        false,
    );
    let painted = nodes(&out);
    assert!(has(&painted, "ODE"));
    assert!(has(&painted, "Equations"));
    assert!(has(&painted, "Initial conditions"));
    assert!(has(&painted, "Step"));
    assert!(has(&painted, "Fit"));
    assert!(has(&painted, "Reset ODE section"));
    assert!(
        find(&painted, "Step").expect("Step button").disabled,
        "Step is disabled without a solution"
    );
    assert!(
        !find(&painted, "Reset ODE section")
            .expect("Reset button")
            .disabled,
        "Reset stays available without a solution"
    );
}

#[test]
fn ode_equation_edit_commits_valid_scene_only() {
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Ode);
    let original = state.ode.clone();
    let mut panel = OdePanelState::default();

    panel.draft.equations[1] = "y+".into();
    assert!(!panel.commit(&mut state));
    assert_eq!(state.ode, original, "invalid equations keep the old scene");
    assert!(
        panel
            .error
            .as_deref()
            .is_some_and(|error| error.contains("equation")),
        "the rejected equation is reported: {:?}",
        panel.error
    );

    panel.draft.equations[1] = "-x - 0.1*y".into();
    assert!(panel.commit(&mut state));
    assert_eq!(state.ode.equations[1], "-x - 0.1*y");
    assert!(panel.error.is_none());
}

#[test]
fn current_field_switch_adopts_the_durable_field_dimension() {
    let ctx = context();
    let mut state = State::new(true);
    assert!(state.set_formula("y, -x, 0".into()));
    assert_eq!(state.field.dimension, 3, "the durable field is 3D");
    state.set_field_mode(FieldMode::Ode);
    let mut panel = OdePanelState::default();
    assert_eq!(panel.draft.dimension, 2, "the draft starts in 2D");

    let width = 1200.0;
    let height = 900.0;
    let out = panel_frame(
        &ctx,
        width,
        height,
        Vec::new(),
        &mut state,
        &mut panel,
        false,
    );
    let current_field = find(&nodes(&out), "Current field")
        .expect("Current field selector")
        .rect
        .center();
    click_panel(
        &ctx,
        width,
        height,
        current_field,
        &mut state,
        &mut panel,
        false,
    );

    assert_eq!(
        panel.draft.dimension, 3,
        "selecting the current field adopts its dimension"
    );
    assert_eq!(state.ode.source, OdeSource::CurrentField);
    assert!(state.ode_error.is_none(), "{:?}", state.ode_error);
    assert_eq!(
        state
            .ode_solution
            .as_ref()
            .map(|solution| solution.dimension),
        Some(3),
        "the solved scene is three dimensional"
    );

    let equations = find(
        &nodes(&panel_frame(
            &ctx,
            width,
            height,
            Vec::new(),
            &mut state,
            &mut panel,
            false,
        )),
        "Equations",
    )
    .expect("Equations selector")
    .rect
    .center();
    click_panel(
        &ctx, width, height, equations, &mut state, &mut panel, false,
    );
    assert_eq!(state.ode.source, OdeSource::Equations);
    assert_eq!(
        panel.draft.dimension, 3,
        "switching back to equations keeps the draft dimension"
    );
}

#[test]
fn ode_initial_conditions_are_bounded_and_distinct() {
    let ctx = context();
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Ode);
    let mut panel = OdePanelState::default();
    let width = 1200.0;
    let height = 2400.0;

    for _ in 0..7 {
        let out = panel_frame(
            &ctx,
            width,
            height,
            Vec::new(),
            &mut state,
            &mut panel,
            false,
        );
        let add = find(&nodes(&out), "Add initial condition")
            .expect("Add initial condition")
            .rect
            .center();
        click_panel(&ctx, width, height, add, &mut state, &mut panel, false);
    }
    assert_eq!(state.ode.seeds.len(), 8, "eight seeds are accepted");
    let seeds = state.ode.seeds.clone();
    for (index, seed) in seeds.iter().enumerate() {
        assert!(
            seeds[..index].iter().all(|earlier| earlier != seed),
            "seeds are distinct: {seeds:?}"
        );
    }

    let out = panel_frame(
        &ctx,
        width,
        height,
        Vec::new(),
        &mut state,
        &mut panel,
        false,
    );
    let painted = nodes(&out);
    let add = find(&painted, "Add initial condition").expect("Add initial condition");
    assert!(add.disabled, "the ninth seed cannot be added");
    click_panel(
        &ctx,
        width,
        height,
        add.rect.center(),
        &mut state,
        &mut panel,
        false,
    );
    assert_eq!(state.ode.seeds.len(), 8, "the bound holds at eight");

    let remove = nodes(&panel_frame(
        &ctx,
        width,
        height,
        Vec::new(),
        &mut state,
        &mut panel,
        false,
    ))
    .into_iter()
    .find(|node| node.label == "Remove")
    .expect("Remove button")
    .rect
    .center();
    click_panel(&ctx, width, height, remove, &mut state, &mut panel, false);
    assert_eq!(state.ode.seeds.len(), 7, "a seed row can be removed");
}

#[test]
fn time_series_variables_reject_all_false() {
    let ctx = context();
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Ode);
    let mut panel = OdePanelState::default();
    let width = 1200.0;
    let height = 1600.0;

    let time_series = find(
        &nodes(&panel_frame(
            &ctx,
            width,
            height,
            Vec::new(),
            &mut state,
            &mut panel,
            false,
        )),
        "Time series",
    )
    .expect("Time series selector")
    .rect
    .center();
    click_panel(
        &ctx,
        width,
        height,
        time_series,
        &mut state,
        &mut panel,
        false,
    );
    assert_eq!(state.ode_chart, OdeChart::TimeSeries);

    for label in ["x", "y"] {
        let out = panel_frame(
            &ctx,
            width,
            height,
            Vec::new(),
            &mut state,
            &mut panel,
            false,
        );
        let pos = checkbox(&nodes(&out), label)
            .unwrap_or_else(|| panic!("{label} checkbox"))
            .rect
            .center();
        click_panel(&ctx, width, height, pos, &mut state, &mut panel, false);
    }
    assert!(
        state.ode_variables.iter().any(|enabled| *enabled),
        "at least one variable stays selected"
    );
    let painted = nodes(&panel_frame(
        &ctx,
        width,
        height,
        Vec::new(),
        &mut state,
        &mut panel,
        false,
    ));
    assert!(
        has(&painted, "Select at least one variable."),
        "the all-off attempt is explained inline"
    );
}

#[test]
fn ode_compact_and_wide_panels_reflow() {
    for (width, compact, panel_width) in [(1200.0, false, 300.0), (390.0, true, 390.0)] {
        let ctx = context();
        let mut state = State::new(true);
        state.set_field_mode(FieldMode::Ode);
        let mut panel = OdePanelState::default();
        let out = panel_frame(
            &ctx,
            width,
            900.0,
            Vec::new(),
            &mut state,
            &mut panel,
            compact,
        );
        let painted = nodes(&out);
        for label in [
            "ODE",
            "Equations",
            "Initial conditions",
            "Total time",
            "Phase portrait",
            "Time series",
            "Step",
            "Fit",
            "Reset ODE section",
            "Advanced",
        ] {
            assert!(has(&painted, label), "{width} pt is missing {label}");
        }
        for node in &painted {
            assert!(
                node.rect.max.x <= panel_width + 1.0 && node.rect.min.x >= -1.0,
                "{width} pt: `{}` leaves the {panel_width} pt panel: {:?}",
                node.label,
                node.rect
            );
            assert!(
                node.rect.width() <= panel_width + 1.0,
                "{width} pt: `{}` is wider than the panel: {:?}",
                node.label,
                node.rect
            );
        }
    }
}

#[test]
fn ode_chart_switch_commits_state() {
    let ctx = context();
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Ode);
    let mut panel = OdePanelState::default();
    let width = 1200.0;
    let height = 1200.0;

    let time_series = find(
        &nodes(&panel_frame(
            &ctx,
            width,
            height,
            Vec::new(),
            &mut state,
            &mut panel,
            false,
        )),
        "Time series",
    )
    .expect("Time series selector")
    .rect
    .center();
    click_panel(
        &ctx,
        width,
        height,
        time_series,
        &mut state,
        &mut panel,
        false,
    );
    assert_eq!(state.ode_chart, OdeChart::TimeSeries);

    let phase = find(
        &nodes(&panel_frame(
            &ctx,
            width,
            height,
            Vec::new(),
            &mut state,
            &mut panel,
            false,
        )),
        "Phase portrait",
    )
    .expect("Phase portrait selector")
    .rect
    .center();
    click_panel(&ctx, width, height, phase, &mut state, &mut panel, false);
    assert_eq!(state.ode_chart, OdeChart::Phase);
}

#[test]
fn ode_mode_disables_features_and_cas() {
    let ctx = context();
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Ode);

    let mut controls = Controls::default();
    let output = run_frame(&ctx, 400.0, 2400.0, Vec::new(), |ui| {
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
        "Features controls are disabled in Ode"
    );
    assert!(
        has(
            &painted,
            "Streamlines, particles and click seeding stay off in Ode.",
        ),
        "the Ode Features note is shown"
    );

    #[cfg(not(target_arch = "wasm32"))]
    {
        use vector_field_explorer::ui::cas_panel::{CasPanelState, Operation};
        let mut panel = CasPanelState::default();
        panel.input = "x".into();
        panel.operation = Operation::Gradient;
        panel.refresh(2);
        let output = run_frame(&ctx, 460.0, 1200.0, Vec::new(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                vector_field_explorer::ui::cas_panel::show(ui, &mut state, &mut panel);
            });
        });
        let painted = nodes(&output);
        let apply = find(&painted, "Replace field").expect("CAS Replace field button");
        assert!(apply.disabled, "CAS Apply is disabled in Ode");
        assert!(
            has(&painted, "CAS edits the formula field; switch to Formula."),
            "the shared CAS scope message is shown"
        );
    }
}

#[test]
fn formula_bar_selects_ode_and_hides_the_editor() {
    let ctx = context();
    let mut state = State::new(true);
    let mut bar = FormulaBarState::default();
    let mut output = run_frame(&ctx, 1200.0, 200.0, Vec::new(), |ui| {
        formula_bar::show(ui, &mut state, &mut bar, false);
    });
    let selector = find(&nodes(&output), "ODE")
        .expect("ODE selector")
        .rect
        .center();
    for pressed in [true, false] {
        output = run_frame(&ctx, 1200.0, 200.0, pointer(selector, pressed), |ui| {
            formula_bar::show(ui, &mut state, &mut bar, false);
        });
    }
    assert_eq!(state.field_mode, FieldMode::Ode);
    let painted = nodes(&output);
    assert!(!has(&painted, "Field formula"), "the editor is hidden");
    assert!(has(&painted, "ODE scene summary"));
    assert!(has(&painted, "Open ODE"));
}

#[test]
fn open_ode_reaches_the_wide_rail_or_compact_sheet() {
    let ctx = context();
    let mut app = vector_field_explorer::VfApp::new();
    app.layout.help_visible = false;
    app.state.set_field_mode(FieldMode::Ode);
    let mut frame = eframe::Frame::_new_kittest();
    let mut render = |app: &mut vector_field_explorer::VfApp, width: f32, events| {
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, 900.0),
                )),
                events,
                ..Default::default()
            },
            |ui| eframe::App::ui(app, ui, &mut frame),
        );
        out.textures_delta.clear();
        out
    };

    let mut out = render(&mut app, 1200.0, Vec::new());
    for _ in 0..3 {
        out = render(&mut app, 1200.0, Vec::new());
    }
    let open = find(&nodes(&out), "Open ODE")
        .expect("Open ODE button on the wide bar")
        .rect
        .center();
    render(&mut app, 1200.0, pointer(open, true));
    render(&mut app, 1200.0, pointer(open, false));
    assert!(
        app.layout.ode_expanded,
        "the wide button expands the ODE rail"
    );

    app.layout.ode_expanded = false;
    let mut out = render(&mut app, 390.0, Vec::new());
    for _ in 0..3 {
        out = render(&mut app, 390.0, Vec::new());
    }
    let open = find(&nodes(&out), "Open ODE")
        .expect("Open ODE button on the compact bar")
        .rect
        .center();
    render(&mut app, 390.0, pointer(open, true));
    render(&mut app, 390.0, pointer(open, false));
    assert_eq!(
        app.sheet,
        SheetState::Ode,
        "the compact button opens the ODE sheet"
    );
}

#[test]
fn ode_help_rows_state_the_model_limits() {
    let text = ode_help_rows()
        .iter()
        .map(|row| format!("{} {}", row.input, row.action))
        .collect::<Vec<_>>()
        .join(" ");
    for needle in [
        "fixed-step RK4",
        "no private timer",
        "bounded",
        "phase portrait",
        "current formula field",
    ] {
        assert!(text.contains(needle), "ODE help is missing {needle}");
    }
}

#[test]
fn ode_layout_defaults_are_collapsed_and_sanitized() {
    let layout = LayoutState::default();
    assert!(
        !layout.ode_expanded,
        "the ODE panel starts collapsed to its rail"
    );
    assert_eq!(layout.ode_width, DEFAULT_ODE_WIDTH);

    let mut layout = LayoutState {
        ode_width: f32::NAN,
        ..Default::default()
    }
    .sanitized();
    assert_eq!(layout.ode_width, DEFAULT_ODE_WIDTH);
    layout.toggle_ode();
    assert!(layout.ode_expanded);

    let mut sheet = SheetState::default();
    sheet.open(SheetState::Ode);
    assert!(sheet.shows(SheetState::Ode));
}
