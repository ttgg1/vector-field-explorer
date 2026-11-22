//! Issue #219: the guided Gauss-law investigation for the charged spherical
//! shell. The numeric tests drive the pure `compute_reading` seam; the panel
//! tests use the same headless-accesskit helpers as `maxwell_panel_test.rs`.

use eframe::egui;

use crate::support;

use vector_field_explorer::maxwell::{MaxwellCase, MaxwellScene};
use vector_field_explorer::source_scene::FieldMode;
use vector_field_explorer::sources::FieldFamily;
use vector_field_explorer::ui::gauss_shell::{
    compute_reading, GaussShellReadingKind, GaussShellState, GAUSS_SHELL_INSIDE_NOTE,
    GAUSS_SHELL_INTERFACE_NOTE, GAUSS_SHELL_QUESTION,
};
use vector_field_explorer::ui::layout::SheetState;
use vector_field_explorer::ui::maxwell_panel::{self, MaxwellPanelState};
use vector_field_explorer::viewmodel::State;
use vector_field_explorer::VfApp;

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

fn panel_frame(
    ctx: &egui::Context,
    width: f32,
    height: f32,
    state: &mut State,
    panel: &mut MaxwellPanelState,
    compact: bool,
    events: Vec<egui::Event>,
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
        |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let _ = maxwell_panel::show(ui, panel, state, compact);
            });
        },
    );
    output.textures_delta.clear();
    output
}

fn hollow(radius: f64) -> MaxwellScene {
    MaxwellScene {
        case: MaxwellCase::HollowSphere,
        radius,
        e0: 1.0,
        ..MaxwellScene::default()
    }
}

fn close(left: f64, right: f64, tolerance: f64) -> bool {
    (left - right).abs() <= tolerance
}

#[test]
fn exterior_r_and_double_r_show_quarter_field_fourfold_area_and_equal_flux() {
    let scene = hollow(1.0);
    let first = compute_reading(&scene, 1.5, FieldFamily::Electric, 0.0);
    let second = compute_reading(&scene, 3.0, FieldFamily::Electric, 0.0);

    let field_first = first.field().expect("exterior r field");
    let field_second = second.field().expect("exterior 2r field");
    let area_first = first.area().expect("exterior r area");
    let area_second = second.area().expect("exterior 2r area");

    assert!(
        close(field_first / field_second, 4.0, 1e-3),
        "doubling r quarters the radial field: {field_first} / {field_second}"
    );
    assert!(
        close(area_second / area_first, 4.0, 1e-3),
        "doubling r quadruples the surface area: {area_second} / {area_first}"
    );

    let (flux_first, error_first) = first.flux_estimate().expect("r flux estimate");
    let (flux_second, error_second) = second.flux_estimate().expect("2r flux estimate");
    let difference = (flux_first - flux_second).abs();
    let uncertainty = error_first + error_second;
    assert!(
        difference <= uncertainty + 1e-9,
        "flux is equal within quadrature uncertainty: |{flux_first} - {flux_second}| = \
         {difference} <= {uncertainty}"
    );
    assert!(
        difference < 1e-6,
        "the shipped midpoint rule resolves this radial flux exactly to float tolerance"
    );

    // The cancellation identity itself: the sampled flux must match the
    // analytic `E * A`, not merely be equal at the two radii. A zero or wrong
    // constant from the adapter cannot satisfy this.
    let analytic_flux = 4.0 * std::f64::consts::PI * scene.e0 * scene.radius * scene.radius;
    for (label, field, area, flux, error) in [
        ("r", field_first, area_first, flux_first, error_first),
        ("2r", field_second, area_second, flux_second, error_second),
    ] {
        // The extra 1e-6 covers the f32 `size` cast of the sampled surface;
        // the reported quadrature error alone is tighter than that rounding.
        let tolerance = error + 1e-6;
        assert!(
            close(field * area, analytic_flux, 1e-5),
            "{label}: E * A = {} must equal 4*pi*e0*R^2 = {analytic_flux}",
            field * area
        );
        assert!(
            close(flux, field * area, tolerance),
            "{label}: sampled flux {flux} must equal E * A = {} within the reported \
             +/− {error} quadrature error",
            field * area
        );
        assert!(
            close(flux, analytic_flux, tolerance),
            "{label}: sampled flux {flux} must equal 4*pi*e0*R^2 = {analytic_flux} within \
             the reported +/− {error} quadrature error"
        );
    }
}

#[test]
fn inside_the_shell_reports_zero_field_and_flux_with_an_explanation() {
    let scene = hollow(1.0);
    let reading = compute_reading(&scene, 0.5, FieldFamily::Electric, 0.0);
    assert!(
        matches!(reading.kind, GaussShellReadingKind::Interior { .. }),
        "r < R is the interior: {:?}",
        reading.kind
    );
    assert_eq!(reading.field(), Some(0.0), "the interior field is zero");
    let (flux, error) = reading.flux_estimate().expect("interior flux estimate");
    assert_eq!(flux, 0.0, "the interior flux is zero");
    assert_eq!(error, 0.0, "both passes agree on the zero flux");
    assert!(GAUSS_SHELL_INSIDE_NOTE.contains("zero"));
}

#[test]
fn the_ideal_interface_reports_an_excluded_sample_not_a_finite_reading() {
    let scene = hollow(1.0);
    let reading = compute_reading(&scene, scene.radius, FieldFamily::Electric, 0.0);
    assert!(
        matches!(reading.kind, GaussShellReadingKind::Excluded { .. }),
        "r = R is excluded: {:?}",
        reading.kind
    );
    assert!(reading.field().is_none(), "no finite field is reported");
    assert!(reading.flux().is_none(), "no finite flux is reported");
    let GaussShellReadingKind::Excluded { reason } = &reading.kind else {
        unreachable!();
    };
    assert!(reason.contains("excluded"));
    assert!(GAUSS_SHELL_INTERFACE_NOTE.contains("excluded"));
}

#[test]
fn activate_clamps_the_initial_probe_radius_to_the_control_range() {
    let mut large = GaussShellState::default();
    large.activate(&hollow(100.0));
    assert!(large.active);
    assert_eq!(
        large.probe_radius, 100.0,
        "a large R keeps the initial probe radius inside the UI control range 0.01..=100.0"
    );
    assert!((0.01..=100.0).contains(&large.probe_radius));

    let mut normal = GaussShellState::default();
    normal.activate(&hollow(1.0));
    assert_eq!(
        normal.probe_radius, 1.5,
        "the normal R=1 path still starts at 1.5*R"
    );
}

#[test]
fn pinning_keeps_the_reading_at_pin_time_and_clear_removes_it() {
    let scene = hollow(1.0);
    let mut state = GaussShellState::default();
    state.probe_radius = 1.5;
    let pinned = state
        .report_for(&scene, FieldFamily::Electric, 0.0)
        .at_radius
        .clone();
    state.pin(pinned.clone());
    assert_eq!(state.pinned(), std::slice::from_ref(&pinned));

    let doubled = state
        .report_for(&scene, FieldFamily::Electric, 0.0)
        .at_double
        .clone();
    state.pin(doubled.clone());
    assert_eq!(state.pinned().len(), 2);

    state.probe_radius = 2.5;
    let after = state.report_for(&scene, FieldFamily::Electric, 0.0).clone();
    assert_ne!(
        after.at_radius.radius, pinned.radius,
        "the live reading follows the radius control"
    );
    assert_eq!(
        state.pinned(),
        &[pinned, doubled],
        "pinned readings keep the radius and numbers that produced them"
    );

    state.clear_pinned();
    assert!(state.pinned().is_empty(), "clear removes both pinned cards");
}

#[test]
fn default_state_is_inactive_and_an_inactive_frame_has_no_side_effects() {
    let default_state = GaussShellState::default();
    assert!(!default_state.active);
    assert!(default_state.pinned().is_empty());
    assert_eq!(default_state.probe_radius, 0.0);

    let ctx = context();
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Maxwell);
    state.set_maxwell_scene(MaxwellScene::default());
    assert_eq!(state.maxwell.case, MaxwellCase::ParallelPlates);
    let scene = state.maxwell.clone();
    let surfaces = state.probe_surfaces().to_vec();
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
    assert!(
        !panel.gauss.active,
        "a plain frame never starts the activity"
    );
    assert!(panel.gauss.pinned().is_empty());
    assert_eq!(state.maxwell, scene, "the committed scene is untouched");
    assert_eq!(
        state.probe_surfaces(),
        surfaces.as_slice(),
        "no durable probe surface is added"
    );
    let nodes = nodes(&out);
    assert!(has(&nodes, "Investigate"), "the entry point is offered");
    assert!(
        has(&nodes, "Free explore"),
        "free exploration is offered beside it"
    );
    assert!(
        !has(&nodes, "Pin r reading"),
        "inactive state has no guided controls"
    );
}

#[test]
fn wide_and_compact_panels_render_the_prompt_and_guided_controls() {
    for (width, height, compact) in [(1200.0, 1800.0, false), (390.0, 1800.0, true)] {
        let ctx = context();
        let mut state = State::new(true);
        state.set_field_mode(FieldMode::Maxwell);
        assert!(state.set_maxwell_scene(hollow(1.0)));
        let mut panel = MaxwellPanelState::default();
        panel.sync_from(&state);
        panel.gauss.activate(&state.maxwell);

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
            GAUSS_SHELL_QUESTION,
            "Probe radius r",
            "Pin r reading",
            "Pin 2r reading",
            "Free explore",
        ] {
            assert!(has(&nodes, label), "{width} pt is missing {label}");
        }
        let texts = painted_texts(&out).join("\n");
        assert!(
            texts.contains("Surface area") && texts.contains("Flux"),
            "{width} pt must show field, area and flux together: {texts}"
        );
        for node in &nodes {
            assert!(
                node.rect.min.x >= -1.0 && node.rect.max.x <= width + 1.0,
                "{width} pt: {:?} leaves the window",
                node.label
            );
        }
    }
}

#[test]
fn panel_pin_buttons_pin_both_readings_and_show_the_pinned_cancellation() {
    let ctx = context();
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Maxwell);
    assert!(state.set_maxwell_scene(hollow(1.0)));
    let mut panel = MaxwellPanelState::default();
    panel.sync_from(&state);
    panel.gauss.activate(&state.maxwell);

    let out = panel_frame(
        &ctx,
        1200.0,
        1800.0,
        &mut state,
        &mut panel,
        false,
        Vec::new(),
    );
    let pin_r = find(&nodes(&out), "Pin r reading")
        .expect("the panel offers the r pin button")
        .rect
        .center();
    panel_frame(
        &ctx,
        1200.0,
        1800.0,
        &mut state,
        &mut panel,
        false,
        pointer(pin_r, true),
    );
    panel_frame(
        &ctx,
        1200.0,
        1800.0,
        &mut state,
        &mut panel,
        false,
        pointer(pin_r, false),
    );
    assert_eq!(
        panel.gauss.pinned().len(),
        1,
        "clicking Pin r reading snapshots the r reading"
    );

    let out = panel_frame(
        &ctx,
        1200.0,
        1800.0,
        &mut state,
        &mut panel,
        false,
        Vec::new(),
    );
    let pin_2r = find(&nodes(&out), "Pin 2r reading")
        .expect("the panel offers the 2r pin button")
        .rect
        .center();
    panel_frame(
        &ctx,
        1200.0,
        1800.0,
        &mut state,
        &mut panel,
        false,
        pointer(pin_2r, true),
    );
    let out = panel_frame(
        &ctx,
        1200.0,
        1800.0,
        &mut state,
        &mut panel,
        false,
        pointer(pin_2r, false),
    );
    assert_eq!(
        panel.gauss.pinned().len(),
        2,
        "clicking Pin 2r reading snapshots the 2r reading too"
    );

    let texts = painted_texts(&out);
    let cards = texts
        .iter()
        .filter(|text| text.starts_with("Pinned r = "))
        .count();
    assert_eq!(cards, 2, "two pinned cards are drawn: {texts:?}");
    let joined = texts.join("\n");
    assert!(
        joined.contains("Pinned cancellation in normalized units"),
        "the pinned cancellation algebra appears once both readings are pinned: {joined}"
    );
    assert!(
        joined.contains("E * A ="),
        "the pinned cards show the E * A cancellation: {joined}"
    );
}

#[test]
fn investigate_is_reachable_from_the_normal_maxwell_panel_entry() {
    let ctx = context();
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    app.state.set_field_mode(FieldMode::Maxwell);
    app.layout.maxwell_expanded = true;
    app.state.set_maxwell_scene(MaxwellScene::default());
    assert_eq!(app.state.maxwell.case, MaxwellCase::ParallelPlates);
    assert!(app.state.probe_surfaces().is_empty());
    let mut frame = eframe::Frame::_new_kittest();
    let mut render = |app: &mut VfApp, events: Vec<egui::Event>| {
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 1800.0),
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
    let investigate = find(&nodes(&out), "Investigate")
        .expect("Investigate entry point in the Maxwell panel")
        .rect
        .center();
    render(&mut app, pointer(investigate, true));
    render(&mut app, pointer(investigate, false));

    assert_eq!(
        app.state.maxwell.case,
        MaxwellCase::HollowSphere,
        "Investigate loads the shipped Hollow sphere case"
    );
    assert!(app.state.maxwell.radius > 0.0, "a sensible R is set");
    assert!(
        app.maxwell.gauss.active,
        "the guided investigation is activated in the Maxwell panel"
    );
    assert!(
        app.state.probe_surfaces().is_empty(),
        "the activity never adds a durable probe surface"
    );
    assert!(
        app.controls.probe_controls.open_section,
        "the Field probes measurement area is revealed"
    );
    assert!(app.layout.maxwell_expanded, "the Maxwell panel stays open");
    assert_ne!(
        app.sheet,
        SheetState::Controls,
        "compact navigation never swaps the guided Maxwell sheet for an unrelated one"
    );
}
