//! Issue #229: select the charged shell (`R`) or the measuring surface (`r`)
//! from the viewport or the keyboard-accessible list, edit only the selected
//! radius, and undo that activity-local change.
//!
//! The physics stays on the existing `compute_reading` seam; these tests cover
//! the selection/edit/undo contract and the honest exclusion of the interface.

use eframe::egui;

use crate::support;

use vector_field_explorer::VfApp;
use vector_field_explorer::maxwell::{MaxwellCase, MaxwellScene, validate_maxwell};
use vector_field_explorer::source_scene::FieldMode;
use vector_field_explorer::sources::FieldFamily;
use vector_field_explorer::ui::gauss_shell::{
    GAUSS_SHELL_LOCAL_UNDO_NOTE, GAUSS_SHELL_MIN_RADIUS, GaussObject, GaussShellReadingKind,
    GaussShellState, shell_radius_range,
};
use vector_field_explorer::ui::maxwell_panel::{self, MaxwellPanelState};
use vector_field_explorer::viewmodel::State;

struct WidgetNode {
    role: egui::Role,
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
                role: node.role(),
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

/// The SpinButton (DragValue) painted just below `label`.
fn spin_button_after(nodes: &[WidgetNode], label: &str) -> Option<egui::Rect> {
    let label = find(nodes, label)?.rect;
    nodes
        .iter()
        .filter(|node| node.role == egui::Role::SpinButton)
        .filter(|node| node.rect.top() >= label.bottom() - 2.0)
        .filter(|node| node.rect.top() <= label.bottom() + 48.0)
        .min_by(|a, b| a.rect.top().total_cmp(&b.rect.top()))
        .map(|node| node.rect)
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

#[allow(clippy::too_many_arguments)]
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

fn active_state(scene: &MaxwellScene, probe_radius: f64) -> GaussShellState {
    let mut state = GaussShellState::default();
    state.activate(scene);
    state.probe_radius = probe_radius;
    state
}

#[test]
fn list_selection_is_shared_and_reports_the_selected_object() {
    let ctx = context();
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Maxwell);
    assert!(state.set_maxwell_scene(hollow(1.0)));
    let mut panel = MaxwellPanelState::default();
    panel.sync_from(&state);
    panel.gauss.activate(&state.maxwell);
    panel.gauss.probe_radius = 1.5;

    let out = panel_frame(
        &ctx,
        1200.0,
        1800.0,
        &mut state,
        &mut panel,
        false,
        Vec::new(),
    );
    let first_nodes = nodes(&out);
    assert!(has(&first_nodes, "Shell (R)"), "the list offers the shell");
    assert!(
        has(&first_nodes, "Measuring surface (r)"),
        "the list offers the measuring surface"
    );
    assert_eq!(panel.gauss.selected, GaussObject::MeasuringSurface);

    let shell = find(&first_nodes, "Shell (R)")
        .expect("shell row")
        .rect
        .center();
    panel_frame(
        &ctx,
        1200.0,
        1800.0,
        &mut state,
        &mut panel,
        false,
        pointer(shell, true),
    );
    let out = panel_frame(
        &ctx,
        1200.0,
        1800.0,
        &mut state,
        &mut panel,
        false,
        pointer(shell, false),
    );
    assert_eq!(panel.gauss.selected, GaussObject::Shell);
    let shell_nodes = nodes(&out);
    assert!(
        has(&shell_nodes, "Shell radius R"),
        "selecting the shell shows its exact numeric entry and drag handle"
    );
    let texts = painted_texts(&out).join("\n");
    assert!(
        texts.contains("Selected: Shell (R = 1.000"),
        "the selected shell's name and exact radius are visible: {texts}"
    );

    let surface = find(&nodes(&out), "Measuring surface (r)")
        .expect("surface row")
        .rect
        .center();
    panel_frame(
        &ctx,
        1200.0,
        1800.0,
        &mut state,
        &mut panel,
        false,
        pointer(surface, true),
    );
    let out = panel_frame(
        &ctx,
        1200.0,
        1800.0,
        &mut state,
        &mut panel,
        false,
        pointer(surface, false),
    );
    assert_eq!(panel.gauss.selected, GaussObject::MeasuringSurface);
    let texts = painted_texts(&out).join("\n");
    assert!(
        texts.contains("Selected: Measuring surface (r = 1.500"),
        "the selected surface's name and exact radius are visible: {texts}"
    );
}

#[test]
fn viewport_click_selects_the_shell_and_surface_but_empty_space_is_inert() {
    let mut app = VfApp::new();
    assert!(app.state.set_field_mode(FieldMode::Maxwell));
    assert!(app.state.set_maxwell_scene(hollow(1.0)));
    app.state.set_mode(false);
    app.maxwell.gauss.activate(&app.state.maxwell);
    app.maxwell.gauss.probe_radius = 1.5;

    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(240.0, 240.0));
    let aspect = rect.width() / rect.height();
    let screen = |app: &VfApp, x: f32, y: f32| {
        let plane = app.state.plane_size();
        let vp = app.orbit.vp_matrix(aspect, Some(plane));
        let clip = vp * glam::Vec4::new(x, y, 0.0, 1.0);
        let ndc = clip.truncate() / clip.w;
        egui::pos2(
            rect.left() + (ndc.x + 1.0) * 0.5 * rect.width(),
            rect.top() + (1.0 - ndc.y) * 0.5 * rect.height(),
        )
    };

    // Start on the surface, then click the shell outline (world radius 1.0).
    assert_eq!(app.maxwell.gauss.selected, GaussObject::MeasuringSurface);
    let shell_pos = screen(&app, 1.0, 0.0);
    app.test_seed_click(shell_pos, rect, aspect, 0.0);
    assert_eq!(
        app.maxwell.gauss.selected,
        GaussObject::Shell,
        "a click on the shell outline selects the shell"
    );

    // The measuring surface (r = 1.5) is a separate object.
    let probe_pos = screen(&app, 1.5, 0.0);
    app.test_seed_click(probe_pos, rect, aspect, 0.0);
    assert_eq!(
        app.maxwell.gauss.selected,
        GaussObject::MeasuringSurface,
        "a click on the measuring surface selects the surface"
    );

    // Empty space near the centre changes nothing.
    let empty_pos = screen(&app, 0.2, 0.0);
    app.test_seed_click(empty_pos, rect, aspect, 0.0);
    assert_eq!(
        app.maxwell.gauss.selected,
        GaussObject::MeasuringSurface,
        "an empty-space click does not change the selection"
    );
}

#[test]
fn overlap_selector_routes_the_click_to_the_chosen_object() {
    let mut app = VfApp::new();
    assert!(app.state.set_field_mode(FieldMode::Maxwell));
    assert!(app.state.set_maxwell_scene(hollow(1.0)));
    app.state.set_mode(false);
    app.maxwell.gauss.activate(&app.state.maxwell);
    // Coincident objects: the selector, not a guess, decides the target.
    app.maxwell.gauss.probe_radius = 1.0;
    app.maxwell.gauss.overlap_choice = GaussObject::Shell;
    assert!(app.maxwell.gauss.objects_overlap(&app.state.maxwell));

    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(240.0, 240.0));
    let aspect = 1.0;
    let plane = app.state.plane_size();
    let vp = app.orbit.vp_matrix(aspect, Some(plane));
    let clip = vp * glam::Vec4::new(1.0, 0.0, 0.0, 1.0);
    let ndc = clip.truncate() / clip.w;
    let pos = egui::pos2(
        rect.left() + (ndc.x + 1.0) * 0.5 * rect.width(),
        rect.top() + (1.0 - ndc.y) * 0.5 * rect.height(),
    );

    app.test_seed_click(pos, rect, aspect, 0.0);
    assert_eq!(app.maxwell.gauss.selected, GaussObject::Shell);

    app.maxwell.gauss.overlap_choice = GaussObject::MeasuringSurface;
    app.test_seed_click(pos, rect, aspect, 0.0);
    assert_eq!(app.maxwell.gauss.selected, GaussObject::MeasuringSurface);
}

#[test]
fn numeric_entry_changes_only_the_selected_radius() {
    let scene = hollow(1.0);
    let mut state = active_state(&scene, 1.5);

    // Measuring surface edit: only r moves.
    state.select(GaussObject::MeasuringSurface);
    assert!(state.set_probe_radius(2.25));
    assert_eq!(state.probe_radius, 2.25);
    assert_eq!(scene.radius, 1.0, "the shell R is untouched");
    assert_eq!(state.pending_shell_radius(), None);

    // Shell edit: only the shell request moves; r stays.
    state.select(GaussObject::Shell);
    assert!(state.request_shell_radius(1.4));
    assert_eq!(
        state.probe_radius, 2.25,
        "the measuring radius is untouched"
    );
    assert_eq!(state.take_pending_shell_radius(), Some(1.4));
    assert_eq!(state.pending_shell_radius(), None);

    // Invalid values are refused and never reset the other radius.
    assert!(!state.set_probe_radius(f64::NAN));
    assert!(!state.set_probe_radius(0.0));
    assert!(!state.set_probe_radius(101.0));
    assert_eq!(state.probe_radius, 2.25);
    assert!(!state.request_shell_radius(-1.0));
    assert_eq!(state.pending_shell_radius(), None);
}

#[test]
fn doubling_r_changes_only_the_measuring_geometry_and_its_readings() {
    let scene = hollow(1.0);
    let mut state = active_state(&scene, 1.5);
    state.select(GaussObject::MeasuringSurface);
    let before = state.report_for(&scene, FieldFamily::Electric, 0.0).clone();
    let shell_outline = scene.clone();

    assert!(state.double_probe_radius());
    assert_eq!(state.probe_radius, 3.0);
    assert_eq!(scene.radius, 1.0, "R stays fixed");
    assert_eq!(scene, shell_outline, "the scene is never mutated");

    let after = state.report_for(&scene, FieldFamily::Electric, 0.0).clone();
    assert_eq!(after.at_radius.radius, 3.0);
    let (before_field, after_field) = (
        before.at_radius.field().expect("exterior before"),
        after.at_radius.field().expect("exterior after"),
    );
    assert!(
        (before_field / after_field - 4.0).abs() < 1e-3,
        "the dependent radial reading follows r: {before_field} / {after_field}"
    );
}

#[test]
fn activity_undo_restores_r_and_names_what_it_undoes() {
    let scene = hollow(1.0);
    let mut state = active_state(&scene, 1.5);
    state.select(GaussObject::MeasuringSurface);

    assert!(state.double_probe_radius());
    assert_eq!(state.probe_radius, 3.0);
    assert_eq!(state.undo_label(), Some("Undo change measuring radius"));
    assert!(GAUSS_SHELL_LOCAL_UNDO_NOTE.contains("not the document history"));

    assert_eq!(state.undo_last(), Some(GaussObject::MeasuringSurface));
    assert_eq!(state.probe_radius, 1.5, "the undo restores r");
    assert_eq!(scene.radius, 1.0, "the shell R is untouched");
    assert_eq!(state.undo_label(), None);
}

#[test]
fn the_local_undo_stack_is_bounded_and_session_only() {
    let scene = hollow(1.0);
    let mut state = active_state(&scene, 1.5);
    for step in 0..40 {
        state.set_probe_radius(1.0 + step as f64 * 0.01);
    }
    let mut undos = 0;
    while state.undo_last().is_some() {
        undos += 1;
        assert!(undos <= 16, "the activity-local undo stack stays bounded");
    }
    assert!(undos > 0);
    // A fresh activation never carries stale selection or undo state.
    state.activate(&scene);
    assert_eq!(state.undo_label(), None);
    assert_eq!(state.selected, GaussObject::MeasuringSurface);
}

#[test]
fn selection_and_probe_edits_never_touch_durable_state() {
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Maxwell);
    assert!(state.set_maxwell_scene(hollow(1.0)));
    let surfaces = state.probe_surfaces().to_vec();
    let scene = state.maxwell.clone();

    let mut gauss = GaussShellState::default();
    gauss.activate(&state.maxwell);
    gauss.select(GaussObject::Shell);
    gauss.select(GaussObject::MeasuringSurface);
    assert!(gauss.set_probe_radius(2.0));
    assert!(gauss.double_probe_radius());
    assert!(gauss.undo_last().is_some());
    // A shell request waits for the app's ordinary scene-control handler.
    assert!(gauss.request_shell_radius(1.2));
    assert_eq!(gauss.take_pending_shell_radius(), Some(1.2));

    assert_eq!(
        state.probe_surfaces(),
        surfaces.as_slice(),
        "the activity never adds or mutates a durable probe surface"
    );
    assert_eq!(
        state.maxwell, scene,
        "selection and measuring-radius edits never mutate the committed scene"
    );
}

#[test]
fn interface_boundary_readings_stay_explicitly_excluded() {
    let scene = hollow(1.0);
    let mut state = active_state(&scene, 1.0);
    state.select(GaussObject::MeasuringSurface);
    let report = state.report_for(&scene, FieldFamily::Electric, 0.0).clone();
    assert!(
        matches!(
            report.at_radius.kind,
            GaussShellReadingKind::Excluded { .. }
        ),
        "r = R is still excluded, not a confident value: {:?}",
        report.at_radius.kind
    );
    assert!(report.at_radius.field().is_none());
    assert!(report.at_radius.flux().is_none());

    // Selection alone never turns the excluded interface into a finite value.
    state.select(GaussObject::Shell);
    let again = state.report_for(&scene, FieldFamily::Electric, 0.0).clone();
    assert!(matches!(
        again.at_radius.kind,
        GaussShellReadingKind::Excluded { .. }
    ));
}

#[test]
fn overlap_selector_is_offered_when_the_objects_coincide() {
    let ctx = context();
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Maxwell);
    assert!(state.set_maxwell_scene(hollow(1.0)));
    let mut panel = MaxwellPanelState::default();
    panel.sync_from(&state);
    panel.gauss.activate(&state.maxwell);
    panel.gauss.probe_radius = 1.0;

    let out = panel_frame(
        &ctx,
        1200.0,
        1800.0,
        &mut state,
        &mut panel,
        false,
        Vec::new(),
    );
    let nodes = nodes(&out);
    assert!(
        has(&nodes, "Target shell (R)"),
        "the overlap selector offers the shell"
    );
    assert!(
        has(&nodes, "Target measuring surface (r)"),
        "the overlap selector offers the measuring surface"
    );
    let texts = painted_texts(&out).join("\n");
    assert!(
        texts.contains("coincide"),
        "the selector explains why it is shown: {texts}"
    );
}

#[test]
fn shell_entry_advertises_the_scene_outer_radius_limit() {
    let ctx = context();
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Maxwell);
    assert!(state.set_maxwell_scene(hollow(1.0)));
    let mut panel = MaxwellPanelState::default();
    panel.sync_from(&state);
    panel.gauss.activate(&state.maxwell);
    panel.gauss.select(GaussObject::Shell);

    let out = panel_frame(
        &ctx,
        1200.0,
        1800.0,
        &mut state,
        &mut panel,
        false,
        Vec::new(),
    );
    let texts = painted_texts(&out).join("\n");
    assert!(
        texts.contains("outer radius"),
        "the shell control must name the scene's outer-radius limit: {texts}"
    );
}

#[test]
fn shell_entry_range_stops_below_the_scene_outer_radius() {
    let scene = hollow(1.0);
    let range = shell_radius_range(&scene).expect("the default outer radius leaves room");
    assert_eq!(*range.start(), GAUSS_SHELL_MIN_RADIUS);
    assert!(
        *range.end() < scene.outer_radius,
        "the advertised max must stay below the rejected outer-radius boundary"
    );
    let mut candidate = scene.clone();
    candidate.radius = *range.end();
    assert!(
        validate_maxwell(&candidate).is_ok(),
        "the advertised max is a valid committed scene"
    );

    // The app handler accepts and commits the advertised maximum.
    let mut app = VfApp::new();
    assert!(app.state.set_field_mode(FieldMode::Maxwell));
    assert!(app.state.set_maxwell_scene(scene));
    app.maxwell.gauss.activate(&app.state.maxwell);
    app.maxwell.gauss.select(GaussObject::Shell);
    assert!(app.maxwell.gauss.request_shell_radius(*range.end()));
    app.test_apply_gauss_shell_pending();
    assert_eq!(app.state.maxwell.radius.to_bits(), range.end().to_bits());
    assert_eq!(app.maxwell.gauss.shell_edit_error(), None);
}

#[test]
fn shell_radius_at_or_above_the_outer_radius_is_rejected_with_feedback() {
    let ctx = context();
    let mut app = VfApp::new();
    assert!(app.state.set_field_mode(FieldMode::Maxwell));
    assert!(app.state.set_maxwell_scene(hollow(1.0)));
    app.maxwell.gauss.activate(&app.state.maxwell);
    app.maxwell.gauss.select(GaussObject::Shell);
    let original = app.state.maxwell.clone();
    let rejected = original.outer_radius + 0.5;

    assert!(app.maxwell.gauss.request_shell_radius(rejected));
    app.test_apply_gauss_shell_pending();
    assert_eq!(
        app.state.maxwell, original,
        "a rejected shell edit leaves the committed scene untouched"
    );
    let error = app
        .maxwell
        .gauss
        .shell_edit_error()
        .expect("the rejection is surfaced to the learner")
        .to_owned();
    assert!(
        error.contains("outer radius must be greater than the inner radius"),
        "the scene's own rejection is shown verbatim: {error}"
    );

    let out = panel_frame(
        &ctx,
        1200.0,
        1800.0,
        &mut app.state,
        &mut app.maxwell,
        false,
        Vec::new(),
    );
    let texts = painted_texts(&out).join("\n");
    assert!(
        texts.contains("outer radius must be greater than the inner radius"),
        "the panel keeps the rejected edit visible: {texts}"
    );
}

#[test]
fn shell_drag_commits_through_the_app_handler_and_readings_follow_r() {
    let ctx = context();
    let mut app = VfApp::new();
    assert!(app.state.set_field_mode(FieldMode::Maxwell));
    assert!(app.state.set_maxwell_scene(hollow(1.0)));
    app.maxwell.gauss.activate(&app.state.maxwell);
    app.maxwell.gauss.probe_radius = 1.5;
    app.maxwell.gauss.select(GaussObject::Shell);

    let scene_before = app.state.maxwell.clone();
    let before = app
        .maxwell
        .gauss
        .report_for(&app.state.maxwell, FieldFamily::Electric, 0.0)
        .clone();
    assert_eq!(before.at_radius.radius, 1.5);

    let out = panel_frame(
        &ctx,
        1200.0,
        1800.0,
        &mut app.state,
        &mut app.maxwell,
        false,
        Vec::new(),
    );
    let start = spin_button_after(&nodes(&out), "Shell radius R")
        .expect("the selected shell offers a drag handle")
        .center();
    let mid = start + egui::vec2(20.0, 0.0);
    let end = start + egui::vec2(40.0, 0.0);

    for events in [
        pointer(start, true),
        vec![egui::Event::PointerMoved(mid)],
        vec![egui::Event::PointerMoved(end)],
        pointer(end, false),
    ] {
        panel_frame(
            &ctx,
            1200.0,
            1800.0,
            &mut app.state,
            &mut app.maxwell,
            false,
            events,
        );
    }
    assert!(
        app.maxwell.gauss.pending_shell_radius().is_some(),
        "the drag stages a shell edit for the app handler"
    );
    assert_eq!(
        app.state.maxwell, scene_before,
        "the panel never writes the committed scene itself"
    );

    app.test_apply_gauss_shell_pending();
    assert_ne!(
        app.state.maxwell.radius, scene_before.radius,
        "the ordinary app handler commits the dragged radius"
    );
    assert!(
        app.state.maxwell.radius < app.state.maxwell.outer_radius,
        "the committed shell radius stays valid"
    );
    assert_eq!(
        app.maxwell.gauss.probe_radius, 1.5,
        "editing R leaves the measuring radius r fixed"
    );
    let after = app
        .maxwell
        .gauss
        .report_for(&app.state.maxwell, FieldFamily::Electric, 0.0)
        .clone();
    assert_eq!(after.at_radius.radius, 1.5, "the reading stays at r");
    assert_ne!(
        after.at_radius.field(),
        before.at_radius.field(),
        "the dependent reading follows the new shell radius"
    );
}

#[test]
fn compact_layout_offers_the_list_and_exact_entry() {
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
            "Shell (R)",
            "Measuring surface (r)",
            "Probe radius r",
            "Double measuring radius",
        ] {
            assert!(has(&nodes, label), "{width} pt is missing {label}");
        }
        for node in &nodes {
            assert!(
                node.rect.min.x >= -1.0 && node.rect.max.x <= width + 1.0,
                "{width} pt: {:?} leaves the window",
                node.label
            );
        }
    }
}
