//! Task 7: real scalar-plot controls, formula mirroring, ordinary example and
//! reset transitions, and the vector-only / SVG guards.
//!
//! These tests drive the owning widgets through headless egui with accesskit,
//! following `button_sizing_test.rs` / `responsive_test.rs` conventions. They
//! assert the controls that are actually painted, not a replica of the rules.

use std::sync::Arc;

use eframe::egui;

use crate::support;

use vector_field_explorer::{
    parameters::{Parameter, ParameterSet},
    plot::PlotKind,
    presets::presets,
    screenshot::SCALAR_SVG_MESSAGE,
    ui::{
        cas_panel::{CasPanelState, Operation},
        formula_bar::{self, FormulaBarState},
        help_overlay, menu,
        plot_panel::{self, PlotPanelAction},
        presets::PresetPanel,
        top_bar,
    },
    viewmodel::State,
};

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

/// A real Plot panel, at either responsive width, driven through its widget path.
struct PlotHarness {
    ctx: egui::Context,
    state: State,
    compact: bool,
    width: f32,
    height: f32,
    action: Option<PlotPanelAction>,
}

impl PlotHarness {
    fn new(width: f32, compact: bool, kind: PlotKind) -> Self {
        let mut state = State::new(false);
        state.set_plot_kind(kind);
        Self {
            ctx: context(),
            state,
            compact,
            width,
            height: 900.0,
            action: None,
        }
    }

    fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        let state = &mut self.state;
        let compact = self.compact;
        let action = &mut self.action;
        run_frame(&self.ctx, self.width, self.height, events, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                if let Some(next) = plot_panel::show(ui, state, compact) {
                    *action = Some(next);
                }
            });
        })
    }

    fn idle(&mut self) -> egui::FullOutput {
        for _ in 0..3 {
            self.frame(Vec::new());
        }
        self.frame(Vec::new())
    }

    fn click(&mut self, rect: egui::Rect) {
        self.frame(pointer(rect.center(), true));
        self.frame(pointer(rect.center(), false));
        self.idle();
    }

    fn click_label(&mut self, label: &str) {
        let out = self.idle();
        let rect = find(&nodes(&out), label)
            .unwrap_or_else(|| panic!("missing reachable widget {label:?}"))
            .rect;
        self.click(rect);
    }
}

#[test]
fn plot_controls_wide_and_compact_reflow() {
    for (width, compact) in [(1200.0, false), (390.0, true)] {
        let mut h = PlotHarness::new(width, compact, PlotKind::Curve2D);
        let out = h.idle();
        let painted = nodes(&out);

        for label in ["Vector field", "Curve y=f(x)", "Surface z=f(x,y)"] {
            assert!(
                has(&painted, label),
                "{label:?} must be reachable at {width}"
            );
        }
        for label in [
            "Example",
            "Samples",
            "Line width (world units)",
            "Color",
            "Range",
            "Fit plot",
            "Height y",
        ] {
            assert!(
                has(&painted, label),
                "curve control {label:?} missing at {width}"
            );
        }
        assert!(
            !has(&painted, "Resolution"),
            "curve hides surface resolution"
        );

        h.click_label("Advanced");
        let advanced = nodes(&h.idle());
        assert!(
            has(&advanced, "Reset plot section"),
            "the Advanced area exposes the plot-section reset"
        );
        assert!(
            !has(&advanced, "Resolution"),
            "surface-only resolution stays out of curve mode"
        );

        h.click_label("Surface z=f(x,y)");
        let surface = nodes(&h.idle());
        for label in ["Resolution", "Color", "Viridis", "Fit plot", "Height z"] {
            assert!(
                has(&surface, label),
                "surface control {label:?} missing at {width}"
            );
        }
        for label in ["Samples", "Line width (world units)"] {
            assert!(
                !has(&surface, label),
                "curve-only {label:?} must not appear in surface mode"
            );
        }

        // Empty geometry disables Fit with a visible explanation rather than a
        // dead-looking enabled button.
        h.state.plot.surface.accepted = None;
        h.state.plot.surface.samples = None;
        let empty = nodes(&h.idle());
        assert!(
            find(&empty, "Fit plot").is_some_and(|node| node.disabled),
            "Fit plot is disabled without finite geometry at {width}"
        );
        assert!(
            empty
                .iter()
                .any(|node| node.label.contains("finite geometry")),
            "the disabled Fit plot explains why at {width}"
        );
    }
}

#[test]
fn formula_edits_and_projection_use_saved_context() {
    let ctx = context();
    let mut state = State::new(false);
    state.set_formula("(y, -x)".into());
    state.set_plot_kind(PlotKind::Curve2D);
    let mut bar = FormulaBarState::default();
    let width = 1200.0;

    let draw = |ctx: &egui::Context, state: &mut State, bar: &mut FormulaBarState| {
        run_frame(ctx, width, 300.0, Vec::new(), |ui| {
            formula_bar::show(ui, state, bar, false);
        })
    };

    draw(&ctx, &mut state, &mut bar);
    assert_eq!(bar.text, "0.5*x^2", "the bar mirrors the curve draft");
    assert_ne!(
        bar.text, state.formula,
        "the stored vector formula must not overwrite the scalar mirror"
    );

    let out = draw(&ctx, &mut state, &mut bar);
    let painted = nodes(&out);
    assert!(has(&painted, "y ="), "curve mode shows the fixed y = label");
    assert!(
        !has(&painted, "z ="),
        "curve mode never shows the surface label"
    );

    // Focus the real text field, select all, then type a new scalar source.
    let field = painted
        .iter()
        .find(|node| node.role == egui::Role::TextInput)
        .expect("the formula bar exposes its real text field")
        .rect;
    let events = pointer(field.center(), true);
    run_frame(&ctx, width, 300.0, events, |ui| {
        formula_bar::show(ui, &mut state, &mut bar, false);
    });
    run_frame(&ctx, width, 300.0, pointer(field.center(), false), |ui| {
        formula_bar::show(ui, &mut state, &mut bar, false);
    });
    let mut select_and_type = vec![
        egui::Event::ModifiersChanged(egui::Modifiers::COMMAND),
        egui::Event::Key {
            key: egui::Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        },
        egui::Event::Text("x^3".into()),
    ];
    select_and_type.push(egui::Event::Key {
        key: egui::Key::A,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: egui::Modifiers::COMMAND,
    });
    run_frame(&ctx, width, 300.0, select_and_type, |ui| {
        formula_bar::show(ui, &mut state, &mut bar, false);
    });
    draw(&ctx, &mut state, &mut bar);
    assert_eq!(
        state.plot.curve.source, "x^3",
        "typing edits the active scalar source"
    );
    assert_eq!(
        state.formula, "(y, -x)",
        "the vector formula is untouched by a scalar edit"
    );
    assert_eq!(bar.text, "x^3");

    // A 3D click in plot mode selects the surface context; its draft and label
    // become active without reinterpreting the curve text.
    let out = nodes(&draw(&ctx, &mut state, &mut bar));
    let three_d = find(&out, "3D view").expect("projection toggle").rect;
    run_frame(&ctx, width, 300.0, pointer(three_d.center(), true), |ui| {
        formula_bar::show(ui, &mut state, &mut bar, false);
    });
    run_frame(&ctx, width, 300.0, pointer(three_d.center(), false), |ui| {
        formula_bar::show(ui, &mut state, &mut bar, false);
    });
    let out = nodes(&draw(&ctx, &mut state, &mut bar));
    assert_eq!(state.plot.kind, PlotKind::Surface3D);
    assert_eq!(bar.text, state.plot.surface.source);
    assert_eq!(
        state.plot.curve.source, "x^3",
        "the curve draft is preserved"
    );
    assert!(has(&out, "z ="), "surface mode shows the fixed z = label");

    // An external kind/source change is mirrored on the next real frame.
    state.set_plot_kind(PlotKind::Curve2D);
    state.set_scalar_source(
        vector_field_explorer::scalar::ScalarContext::Curve,
        "2*x".into(),
    );
    bar.text = "(y, -x)".into();
    draw(&ctx, &mut state, &mut bar);
    assert_eq!(bar.text, "2*x");
}

#[test]
fn examples_and_resets_take_real_path() {
    // A catalogue example goes through the ordinary kind/source transaction.
    let mut h = PlotHarness::new(1200.0, false, PlotKind::Curve2D);
    let before = h.state.plot.transition_epoch;
    let out = nodes(&h.idle());
    let combo = find(&out, "Harmonic oscillator potential")
        .expect("the example selector starts on the matching catalogue entry")
        .rect;
    h.click(combo);
    h.click_label("Gaussian wave packet");
    assert_eq!(h.state.plot.kind, PlotKind::Curve2D);
    assert_eq!(h.state.plot.curve.source, "exp(-x^2)*cos(8*x)");
    assert!(h.state.plot.transition_epoch > before);

    // Custom is derived from the current text, not a stored second copy.
    h.state.set_active_source("sin(x)".into());
    let out = nodes(&h.idle());
    assert!(has(&out, "Custom"), "an edited source selects Custom");

    // Plot-section reset restores plot settings and the active scalar draft
    // only, preserving the vector formula.
    h.state.set_formula("(y, -x)".into());
    h.state.set_plot_samples(900);
    h.state.set_plot_width(0.1);
    h.state.set_plot_auto_height(false);
    h.state.set_plot_height_range(-2.0, 3.0);
    h.click_label("Advanced");
    let out = nodes(&h.idle());
    let reset = find(&out, "Reset plot section")
        .expect("the plot-section reset is a real button")
        .rect;
    h.click(reset);
    assert_eq!(h.state.plot.curve.source, "0.5*x^2");
    assert_eq!(h.state.plot.settings.curve_samples, 513);
    assert_eq!(h.state.plot.settings.line_width, 0.01);
    assert!(h.state.plot.settings.auto_height);
    assert_eq!(
        h.state.formula, "(y, -x)",
        "the plot-section reset preserves the vector formula"
    );

    // Global reset is dispatched through the real Edit menu, not just a setter.
    let mut app = vector_field_explorer::VfApp::new();
    app.state.set_plot_kind(PlotKind::Surface3D);
    app.state.set_formula("(y, -x)".into());
    app.state.set_active_source("sin(2*x)*cos(y)".into());
    app.state.set_plot_resolution(80);
    app.state.set_plot_width(0.05);
    let mut harness = AppHarness::new(1280.0, 900.0);
    harness.app = app;
    harness.idle();
    harness.click_label("Edit");
    harness.click_label("Reset all controls");
    harness.idle();
    assert_eq!(harness.app.state.plot.kind, PlotKind::VectorField);
    assert_eq!(harness.app.state.plot.surface.source, "exp(-(x^2+y^2))");
    assert_eq!(harness.app.state.plot.settings.surface_resolution, 64);
    assert_eq!(harness.app.state.formula, "(y, -x)");
    assert!(
        harness.app.bar.text == "(y, -x)",
        "the bar follows the reset vector editor"
    );
}

#[test]
fn vector_only_controls_and_svg_explain_disabled_state() {
    // Vector-only values survive scalar mode and are explained, not silently
    // rewritten. Grid/axes and the camera remain available.
    let mut h = PlotHarness::new(1200.0, false, PlotKind::Curve2D);
    h.state.set_particles(true);
    h.state.set_show_grid(false);
    h.state.set_show_coordinate_system(false);
    let out = nodes(&h.idle());
    assert!(has(&out, "Vector field"));
    assert!(has(&out, "Fit plot"));

    let mut app = vector_field_explorer::VfApp::new();
    app.state.set_plot_kind(PlotKind::Curve2D);
    app.controls.particles = true;
    app.presets = PresetPanel::new();
    let mut harness = AppHarness::new(1280.0, 900.0);
    harness.app = app;
    let out = nodes(&harness.idle());
    assert!(
        out.iter().any(|node| node.label.contains("Vector-only")),
        "scalar mode explains the disabled vector controls"
    );
    assert!(
        find(&out, "Show grid").is_some_and(|node| !node.disabled),
        "grid remains available while plotting"
    );
    assert!(
        harness.app.controls.particles,
        "the hidden/disabled vector values are retained"
    );

    // A vector preset explicitly returns to vector mode before its formula.
    let name = presets()[0].name;
    harness.app.state.set_plot_kind(PlotKind::Surface3D);
    assert!(harness.app.presets.load_field(name, &mut harness.app.state));
    assert_eq!(harness.app.state.plot.kind, PlotKind::VectorField);

    // CAS Apply is inert in scalar mode even when called directly.
    let mut cas = CasPanelState::default();
    cas.input = "x^2+y^2".into();
    cas.operation = Operation::Gradient;
    cas.refresh(2);
    let mut state = State::new(false);
    state.set_plot_kind(PlotKind::Curve2D);
    let formula = state.formula.clone();
    assert!(
        !cas.apply(&mut state, 2),
        "direct CAS apply must reject scalar mode"
    );
    assert_eq!(state.formula, formula);

    // Copy formula reads the active scalar editor, not the preserved vector
    // text.
    state.set_active_source("sin(x)".into());
    let mut layout = vector_field_explorer::ui::layout::LayoutState::default();
    let mut orbit = vector_field_explorer::camera::OrbitCamera::default();
    let mut fly = None;
    let mut camera = vector_field_explorer::render::Camera::default();
    let mut controls = vector_field_explorer::ui::sidebar::Controls::new();
    let mut clock = vector_field_explorer::sim_clock::SimClock::default();
    let effect = menu::apply_action(
        menu::MenuAction::CopyFormula,
        &mut menu::UiState {
            layout: &mut layout,
            orbit: &mut orbit,
            fly: &mut fly,
            camera: &mut camera,
            state: &mut state,
            controls: &mut controls,
            clock: &mut clock,
        },
    );
    assert_eq!(
        effect,
        menu::SideEffect::CopyText("sin(x)".into()),
        "Copy formula reads the active scalar source"
    );

    // The exact scalar SVG message is shared by both kinds; the rendered menu,
    // button and help rows all carry it.
    for kind in [PlotKind::Curve2D, PlotKind::Surface3D] {
        assert_eq!(
            top_bar::svg_disabled_reason(kind, false),
            Some(SCALAR_SVG_MESSAGE)
        );
    }
    assert_eq!(
        top_bar::svg_disabled_reason(PlotKind::VectorField, true),
        Some("SVG export is 2D only; use PNG in 3D")
    );
    assert!(
        help_overlay::scalar_help_rows()
            .iter()
            .any(|row| row.action == SCALAR_SVG_MESSAGE),
        "the help table states the exact scalar SVG reason"
    );
    for needle in [
        "backface",
        "holes",
        "world units",
        "epsilon 0.1",
        "dimensionless",
    ] {
        assert!(
            help_overlay::scalar_help_rows()
                .iter()
                .any(|row| row.action.to_lowercase().contains(needle)),
            "scalar help mentions {needle:?}"
        );
    }

    let ctx = context();
    let mut style = vector_field_explorer::ui::style_panel::AppStyle::default();
    let screenshot = vector_field_explorer::screenshot::Screenshot::new();
    let mut open = false;
    let mut out = run_frame(&ctx, 1280.0, 300.0, Vec::new(), |ui| {
        let _ = top_bar::show_with_plot_kind(
            ui,
            &mut style,
            &screenshot,
            &mut open,
            false,
            PlotKind::Curve2D,
            None,
        );
    });
    out.textures_delta.clear();
    let painted = nodes(&out);
    assert!(
        find(&painted, "Screenshot as SVG").is_some_and(|node| node.disabled),
        "the scalar SVG button is visibly disabled"
    );
    assert!(
        !menu::SHORTCUTS
            .iter()
            .any(|shortcut| shortcut.label.to_lowercase().contains("fit")),
        "Fit plot adds no keyboard shortcut"
    );
}

#[test]
fn analytic_dipole_route_loads_d_only_after_a_successful_commit() {
    let mut h = PlotHarness::new(1200.0, false, PlotKind::Surface3D);
    h.state.set_plot_resolution(5);
    h.idle();
    assert!(h.state.parameters.value("d").is_none());
    let before_source = h.state.plot.surface.source.clone();

    h.click_label("Analytic dipole potential");
    h.click_label("Use analytic potential");
    let out = h.idle();

    assert_eq!(h.state.parameters.value("d"), Some(1.0));
    assert_ne!(h.state.plot.surface.source, before_source);
    assert!(h.state.plot.surface.source.contains('d'));
    assert!(h.state.plot.surface.accepted.is_some());
    let Some(vector_field_explorer::plot_state::SampledPlot::Surface(mesh)) =
        h.state.plot.surface.samples.as_deref()
    else {
        panic!("missing sampled dipole");
    };
    assert!(mesh.diagnostics.rejected_cells > 0 && !mesh.indices.is_empty());
    assert!(h.state.plot_scene().bounds.is_some());
    assert!(
        nodes(&out).iter().any(|node| node.label.contains("Loaded")),
        "success is reported only after the mesh exists: {:?}",
        nodes(&out).iter().map(|n| &n.label).collect::<Vec<_>>()
    );
    let mut parameters = h.state.parameters.clone();
    parameters.entries[0].value = 2.0;
    h.state.commit_parameters(parameters).unwrap();
    let painted = nodes(&h.idle());
    assert!(painted.iter().any(|n| n.label.contains("At load: d = 1")));
}

#[test]
fn analytic_dipole_route_undrawable_candidate_rolls_back() {
    let mut h = PlotHarness::new(1200.0, false, PlotKind::Surface3D);
    // Normal dipole: both poles project onto the single cell's center.
    h.state.set_plot_resolution(2);
    h.idle();
    h.click_label("Analytic dipole potential");
    h.click_label("z");
    assert_failed_dipole_rollback(&mut h);
}

#[test]
fn analytic_dipole_route_failure_changes_nothing_and_has_no_loaded_status() {
    let mut h = PlotHarness::new(1200.0, false, PlotKind::Surface3D);
    // The parameter cap is already full, so adding the separation `d` cannot
    // commit. The route must reject the whole action before touching anything.
    let mut parameters = ParameterSet::default();
    for index in 0..32 {
        parameters.entries.push(Parameter {
            name: format!("p{index}"),
            value: 1.0,
            min: None,
            max: None,
            step: 0.1,
        });
    }
    h.state
        .commit_parameters(parameters)
        .expect("a full-but-valid set");
    h.idle();
    h.click_label("Analytic dipole potential");
    assert_failed_dipole_rollback(&mut h);
}

fn assert_failed_dipole_rollback(h: &mut PlotHarness) {
    let before_parameters = h.state.parameters.clone();
    let before_source = h.state.plot.surface.source.clone();
    let before_samples = h.state.plot.surface.samples.clone().expect("sampled mesh");
    let before_generation = h.state.plot.generation;
    let accepted = |h: &PlotHarness| {
        h.state
            .plot
            .surface
            .accepted
            .as_ref()
            .unwrap()
            .eval(0.2, 0.3)
    };
    let before_value = accepted(h);
    let before_error = h.state.plot.surface.error.clone();

    h.click_label("Use analytic potential");
    let out = h.idle();
    let painted = nodes(&out);

    assert_eq!(
        h.state.parameters, before_parameters,
        "no parameter changed"
    );
    assert_eq!(
        h.state.plot.surface.source, before_source,
        "source unchanged"
    );
    assert_eq!(accepted(h), before_value);
    assert_eq!(h.state.plot.surface.error, before_error);
    assert!(
        Arc::ptr_eq(
            &before_samples,
            h.state
                .plot
                .surface
                .samples
                .as_ref()
                .expect("mesh unchanged")
        ),
        "the old mesh is still the one on screen"
    );
    assert_eq!(
        h.state.plot.generation, before_generation,
        "no scene rebuild was requested"
    );
    assert!(
        painted
            .iter()
            .any(|node| node.label.contains("Could not load")
                && node.label.contains("Nothing was changed")),
        "an actionable failure is shown: {:?}",
        painted.iter().map(|n| &n.label).collect::<Vec<_>>()
    );
    assert!(
        !painted.iter().any(|node| node.label.contains("Loaded")),
        "failure never claims success"
    );
}

/// Full application chrome, so menu and status paths are the production ones.
struct AppHarness {
    ctx: egui::Context,
    app: vector_field_explorer::VfApp,
    width: f32,
    height: f32,
}

impl AppHarness {
    fn new(width: f32, height: f32) -> Self {
        let ctx = context();
        let mut app = vector_field_explorer::VfApp::new();
        app.layout.help_visible = false;
        Self {
            ctx,
            app,
            width,
            height,
        }
    }

    fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        let mut eframe_frame = eframe::Frame::_new_kittest();
        let app = &mut self.app;
        run_frame(&self.ctx, self.width, self.height, events, |ui| {
            eframe::App::ui(app, ui, &mut eframe_frame)
        })
    }

    fn idle(&mut self) -> egui::FullOutput {
        for _ in 0..3 {
            self.frame(Vec::new());
        }
        self.frame(Vec::new())
    }

    fn click_label(&mut self, label: &str) {
        let out = self.idle();
        let rect = find(&nodes(&out), label)
            .unwrap_or_else(|| panic!("missing app widget {label:?}"))
            .rect;
        self.frame(pointer(rect.center(), true));
        self.frame(pointer(rect.center(), false));
    }
}
