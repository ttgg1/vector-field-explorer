//! Task 4: the responsive Maxwell panel, the sixth adapter selector and the
//! help rows. Widget assertions use the same headless-accesskit helpers as
//! `material_panel_test.rs`; rail/sheet and action checks drive a real `VfApp`.

use eframe::egui;

use crate::support;

use vector_field_explorer::VfApp;
use vector_field_explorer::maxwell::{
    ConductorOutline, MAXWELL_SWITCH_2D_WARNING, MaxwellCase, MaxwellScene, MaxwellStop,
};
use vector_field_explorer::source_scene::FieldMode;
use vector_field_explorer::ui::formula_bar::{self, FormulaBarState};
use vector_field_explorer::ui::layout::SheetState;
use vector_field_explorer::ui::maxwell_panel::{
    self, MAXWELL_CONDUCTOR_NOTE, MAXWELL_ONLY_NOTE, MaxwellPanelState,
};
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

/// Every painted text run, recursively through shape groups.
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

fn panel_frame(
    ctx: &egui::Context,
    width: f32,
    height: f32,
    state: &mut State,
    panel: &mut MaxwellPanelState,
    compact: bool,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    run_frame(ctx, width, height, events, |ui| {
        egui::CentralPanel::default().show(ui, |ui| {
            let _ = maxwell_panel::show(ui, panel, state, compact);
        });
    })
}

fn conductor(radius: f64) -> MaxwellScene {
    MaxwellScene {
        case: MaxwellCase::Conductor2D,
        outline: ConductorOutline::Circle { radius },
        ..MaxwellScene::default()
    }
}

fn conductor_state(mode_3d: bool) -> State {
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Maxwell);
    state.set_mode(mode_3d);
    assert!(state.set_maxwell_scene(conductor(1.0)));
    state.refresh_maxwell_sampling();
    state
}

#[test]
fn maxwell_panel_exposes_case_geometry_and_actions() {
    for (width, height, compact) in [(1200.0, 900.0, false), (390.0, 720.0, true)] {
        let ctx = context();
        let mut state = State::new(true);
        state.set_field_mode(FieldMode::Maxwell);
        state.refresh_maxwell_sampling();
        let mut panel = MaxwellPanelState::default();
        panel.sync_from(&state);
        let out = panel_frame(
            &ctx,
            width,
            height,
            &mut state,
            &mut panel,
            compact,
            Vec::new(),
        );
        let nodes = nodes(&out);
        for label in [
            "Maxwell",
            "Case",
            "Parallel plates",
            "E0 (display)",
            "B0 (display)",
            "Separation",
            "Fit",
            "Reset Maxwell section",
        ] {
            assert!(has(&nodes, label), "{width} pt is missing {label}");
        }
        assert_eq!(
            find(&nodes, "Fit").expect("Fit button").role,
            egui::Role::Button
        );
    }
}

#[test]
fn conductor_controls_only_appear_for_conductor_case() {
    let ctx = context();
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Maxwell);
    state.refresh_maxwell_sampling();
    let mut panel = MaxwellPanelState::default();
    panel.sync_from(&state);

    let out = panel_frame(
        &ctx,
        1200.0,
        900.0,
        &mut state,
        &mut panel,
        false,
        Vec::new(),
    );
    let parallel_nodes = nodes(&out);
    for label in ["Circle", "Square", "Elements", "Applied x", "Applied y"] {
        assert!(
            !has(&parallel_nodes, label),
            "parallel plates must not show the conductor control {label}"
        );
    }
    let texts = painted_texts(&out).join("\n");
    assert!(
        !texts.contains(MAXWELL_CONDUCTOR_NOTE),
        "the conductor note belongs only to Conductor2D"
    );

    assert!(state.set_maxwell_scene(conductor(1.0)));
    state.refresh_maxwell_sampling();
    panel.sync_from(&state);
    let out = panel_frame(
        &ctx,
        1200.0,
        900.0,
        &mut state,
        &mut panel,
        false,
        Vec::new(),
    );
    let nodes = nodes(&out);
    for label in [
        "Conductor2D",
        "Circle",
        "Square",
        "Elements",
        "Applied x",
        "Applied y",
        "Radius",
    ] {
        assert!(has(&nodes, label), "Conductor2D is missing {label}");
    }
    let texts = painted_texts(&out).join("\n");
    assert!(
        texts.contains(MAXWELL_CONDUCTOR_NOTE),
        "the infinite-cylinder applicability note is painted: {texts}"
    );
}

#[test]
fn maxwell_wide_and_compact_panels_reflow() {
    for (width, height, compact) in [(1200.0, 900.0, false), (390.0, 720.0, true)] {
        let ctx = context();
        let mut state = conductor_state(false);
        let mut panel = MaxwellPanelState::default();
        panel.sync_from(&state);
        panel.advanced = true;
        let out = panel_frame(
            &ctx,
            width,
            height,
            &mut state,
            &mut panel,
            compact,
            Vec::new(),
        );
        let nodes = nodes(&out);
        for label in [
            "Maxwell",
            "Conductor2D",
            "Circle",
            "Square",
            "Elements",
            "Applied x",
            "Applied y",
            "Fit",
            "Reset Maxwell section",
            "Advanced",
            "Assumptions",
        ] {
            assert!(has(&nodes, label), "{width} pt is missing {label}");
        }
        for node in &nodes {
            assert!(
                node.rect.min.x >= -1.0 && node.rect.max.x <= width + 1.0,
                "{width} pt: {:?} {:?} leaves the window",
                node.label,
                node.rect
            );
        }
    }
}

#[test]
fn invalid_maxwell_edit_retains_last_valid_scene() {
    let ctx = context();
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Maxwell);
    state.refresh_maxwell_sampling();
    let valid = state.maxwell.clone();
    let sampling = state.maxwell_sampling.clone();
    let mut panel = MaxwellPanelState::default();
    panel.sync_from(&state);
    panel.draft.outer_radius = panel.draft.radius;

    assert!(!panel.commit(&mut state), "outer_radius <= radius rejects");
    assert_eq!(state.maxwell, valid, "the last valid scene stays committed");
    assert_eq!(
        state.maxwell_sampling, sampling,
        "a rejected edit leaves the current sampling alone"
    );
    assert!(
        panel
            .error
            .as_deref()
            .unwrap_or("")
            .contains("outer radius"),
        "the panel keeps an actionable error: {:?}",
        panel.error
    );

    let out = panel_frame(
        &ctx,
        1200.0,
        900.0,
        &mut state,
        &mut panel,
        false,
        Vec::new(),
    );
    let text = painted_texts(&out).join("\n");
    assert!(
        text.contains("outer radius must be greater than the inner radius"),
        "the rejected value's message is visible: {text}"
    );
}

#[test]
fn accuracy_optin_only_for_accuracy_not_met() {
    let ctx = context();
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Maxwell);
    state.refresh_maxwell_sampling();
    assert_eq!(
        state.maxwell_sampling.as_ref().map(|s| s.accuracy),
        Some(MaxwellStop::TargetMet),
        "the default analytic case meets its target"
    );
    let mut panel = MaxwellPanelState::default();
    panel.sync_from(&state);
    panel.advanced = true;
    assert!(!panel.draft.allow_inaccurate, "the opt-in starts off");

    let out = panel_frame(
        &ctx,
        1200.0,
        900.0,
        &mut state,
        &mut panel,
        false,
        Vec::new(),
    );
    let checkbox = "Show finite results labelled \"accuracy not met\"";
    assert!(
        find(&nodes(&out), checkbox)
            .expect("accuracy opt-in")
            .disabled,
        "the opt-in is disabled unless a finite result misses its target"
    );

    state.maxwell_sampling.as_mut().expect("sampling").accuracy = MaxwellStop::AccuracyNotMet;
    let out = panel_frame(
        &ctx,
        1200.0,
        900.0,
        &mut state,
        &mut panel,
        false,
        Vec::new(),
    );
    assert!(
        !find(&nodes(&out), checkbox)
            .expect("accuracy opt-in")
            .disabled,
        "a finite accuracy-not-met result enables the opt-in"
    );
    let texts = painted_texts(&out).join("\n");
    assert!(
        texts.contains("accuracy not met"),
        "the status labels the missed target: {texts}"
    );

    state.maxwell_sampling.as_mut().expect("sampling").accuracy = MaxwellStop::Unavailable;
    let out = panel_frame(
        &ctx,
        1200.0,
        900.0,
        &mut state,
        &mut panel,
        false,
        Vec::new(),
    );
    assert!(
        find(&nodes(&out), checkbox)
            .expect("accuracy opt-in")
            .disabled,
        "an unavailable result cannot be opted into"
    );
}

#[test]
fn switch_to_2d_action_sets_2d_and_preserves_mode() {
    let ctx = context();
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    app.state.set_field_mode(FieldMode::Maxwell);
    app.state.set_mode(true);
    assert!(app.state.set_maxwell_scene(conductor(1.0)));
    app.layout.maxwell_expanded = true;
    app.orbit.target = glam::Vec3::new(1.5, -2.0, 0.5);
    app.orbit.yaw = 1.1;
    app.orbit.pitch = 0.6;
    app.orbit.distance = 4.5;
    app.orbit.ortho_zoom = 2.5;
    let orbit = app.orbit;
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
    let out = render(&mut app, Vec::new());
    let switch = find(&nodes(&out), "Switch to 2D")
        .expect("Switch to 2D")
        .rect
        .center();
    render(&mut app, pointer(switch, true));
    render(&mut app, pointer(switch, false));

    assert!(!app.state.mode_3d, "the action switches to the 2D view");
    assert_eq!(
        app.state.field_mode,
        FieldMode::Maxwell,
        "mode is preserved"
    );
    assert_eq!(
        app.state.maxwell.case,
        MaxwellCase::Conductor2D,
        "the case is untouched"
    );
    assert_eq!(app.orbit, orbit, "the camera is preserved");
}

#[test]
fn maxwell_advanced_warning_is_only_current_scene() {
    let ctx = context();
    let mut state = conductor_state(true);
    let mut panel = MaxwellPanelState::default();
    panel.sync_from(&state);
    panel.advanced = true;
    let out = panel_frame(
        &ctx,
        1200.0,
        900.0,
        &mut state,
        &mut panel,
        false,
        Vec::new(),
    );
    let first = painted_texts(&out).join("\n");
    assert!(
        first.contains(MAXWELL_SWITCH_2D_WARNING),
        "the current 3D applicability warning is shown: {first}"
    );

    panel.draft.case = MaxwellCase::ParallelPlates;
    assert!(panel.commit(&mut state));
    assert!(state.maxwell_sampling.is_none());
    let out = panel_frame(
        &ctx,
        1200.0,
        900.0,
        &mut state,
        &mut panel,
        false,
        Vec::new(),
    );
    let stale = painted_texts(&out).join("\n");
    assert!(
        !stale.contains(MAXWELL_SWITCH_2D_WARNING),
        "a prior scene's warning is never shown as current: {stale}"
    );
}

#[test]
fn maxwell_mode_disables_features_and_cas() {
    let ctx = context();
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Maxwell);
    let mut controls = Controls::default();
    let out = run_frame(&ctx, 400.0, 1400.0, Vec::new(), |ui| {
        egui::CentralPanel::default().show(ui, |ui| {
            sidebar::show(ui, &mut state, &mut controls);
        });
    });
    let painted = nodes(&out);
    let streamlines = find(&painted, "Streamlines").expect("Streamlines control");
    assert!(
        streamlines.disabled,
        "feature controls are disabled in Maxwell"
    );
    assert!(
        has(&painted, MAXWELL_ONLY_NOTE),
        "the exact Maxwell features note is shown"
    );
    assert!(
        !has(&painted, "Coloring"),
        "Coloring is hidden in Maxwell mode"
    );
    assert!(has(&painted, "Domain"), "Domain stays available");

    #[cfg(not(target_arch = "wasm32"))]
    {
        use vector_field_explorer::ui::cas_panel::CasPanelState;
        let mut cas = CasPanelState::default();
        let out = run_frame(&ctx, 400.0, 900.0, Vec::new(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                vector_field_explorer::ui::cas_panel::show(ui, &mut state, &mut cas);
            });
        });
        assert!(
            has(
                &nodes(&out),
                "CAS edits the formula field; switch to Formula."
            ),
            "CAS keeps its exact disabled message in Maxwell"
        );
    }
}

#[test]
fn maxwell_help_rows_state_the_limits() {
    let text = maxwell_panel::maxwell_help_rows()
        .iter()
        .map(|row| format!("{} {}", row.input, row.action))
        .collect::<Vec<_>>()
        .join(" ");
    for needle in [
        "canonical",
        "ideal",
        "surface-current",
        "infinite-cylinder",
        "BEM",
        "no general solver",
        "accuracy not met",
        "display",
    ] {
        assert!(text.contains(needle), "help is missing {needle}");
    }
}

#[test]
fn formula_bar_selects_maxwell_and_hides_the_formula_editor() {
    let ctx = context();
    let mut state = State::new(true);
    let mut bar = FormulaBarState::default();
    let out = run_frame(&ctx, 1200.0, 200.0, Vec::new(), |ui| {
        formula_bar::show(ui, &mut state, &mut bar, false);
    });
    let selector = find(&nodes(&out), "Maxwell")
        .expect("Maxwell selector")
        .rect;
    run_frame(
        &ctx,
        1200.0,
        200.0,
        pointer(selector.center(), true),
        |ui| formula_bar::show(ui, &mut state, &mut bar, false),
    );
    let out = run_frame(
        &ctx,
        1200.0,
        200.0,
        pointer(selector.center(), false),
        |ui| formula_bar::show(ui, &mut state, &mut bar, false),
    );
    assert_eq!(state.field_mode, FieldMode::Maxwell);
    let nodes = nodes(&out);
    assert!(
        !has(&nodes, "Field formula"),
        "the formula editor is hidden"
    );
    assert!(has(&nodes, "Maxwell scene summary"));
    assert!(
        nodes
            .iter()
            .any(|node| node.label.contains("Parallel plates")),
        "the selected case shows"
    );
    let open = find(&nodes, "Open Maxwell")
        .expect("Open Maxwell button")
        .rect;
    run_frame(&ctx, 1200.0, 200.0, pointer(open.center(), true), |ui| {
        formula_bar::show(ui, &mut state, &mut bar, false)
    });
    run_frame(&ctx, 1200.0, 200.0, pointer(open.center(), false), |ui| {
        formula_bar::show(ui, &mut state, &mut bar, false)
    });
    assert!(bar.open_maxwell, "the button requests the panel");
}

#[test]
fn open_maxwell_reaches_the_wide_rail_or_compact_sheet() {
    let ctx = context();
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    app.state.set_field_mode(FieldMode::Maxwell);
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
    app.bar.open_maxwell = true;
    render(&mut app, 1200.0);
    assert!(
        app.layout.maxwell_expanded,
        "the wide request expands the Maxwell rail"
    );

    app.layout.maxwell_expanded = false;
    app.bar.open_maxwell = true;
    render(&mut app, 390.0);
    assert_eq!(
        app.sheet,
        SheetState::Maxwell,
        "the compact request opens the Maxwell sheet"
    );
}
