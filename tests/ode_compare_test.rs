//! Issue #221: frozen field line at `t = 0` versus the time-dependent ODE
//! trajectory from the same seed.

use eframe::egui;
use glam::Vec3;

use crate::support;

use vector_field_explorer::VfApp;
use vector_field_explorer::domain::Domain;
use vector_field_explorer::field::FieldExpr;
use vector_field_explorer::ode::{OdeScene, OdeSeed, OdeSource, solve_ode};
use vector_field_explorer::ode_compare::{
    CAVEAT, FROZEN_COLOR, FROZEN_LABEL, REFERENCE_SEED, TRAJECTORY_LABEL, apply_steady_reference,
    apply_time_dependent_reference, reference_formula, trace_frozen_line,
};
use vector_field_explorer::source_scene::FieldMode;
use vector_field_explorer::ui::ode_panel::{self, OdePanelState};
use vector_field_explorer::viewmodel::State;

fn field(source: &str) -> FieldExpr {
    FieldExpr::parse(source).expect("valid field")
}

fn current_field_scene(duration: f64) -> OdeScene {
    OdeScene {
        source: OdeSource::CurrentField,
        dimension: 2,
        equations: ["0".into(), "0".into(), "0".into()],
        seeds: vec![OdeSeed {
            position: REFERENCE_SEED,
        }],
        duration,
    }
}

#[test]
fn frozen_line_is_y_zero_and_trajectory_follows_t_t_squared_over_two() {
    let field = field("(1, t)");
    let frozen = trace_frozen_line(&field, REFERENCE_SEED, 2, Domain::default(), 4096)
        .expect("v=(1,0) traces a frozen line");
    assert!(frozen.points.len() > 2, "the line has interior samples");
    assert_eq!(frozen.color, FROZEN_COLOR);
    for point in &frozen.points {
        assert!(
            point[1].abs() < 1e-9,
            "the frozen line at t=0 lies on y=0: {point:?}"
        );
    }

    let mut state = State::new(true);
    state.set_formula("(1, t)".into());
    let solution = solve_ode(&state, &current_field_scene(2.0)).expect("valid scene");
    for t in [0.5, 1.0, 1.5, 2.0] {
        let sample = solution.sample_at(0, t).expect("sample");
        assert!(
            (sample.position[0] - t).abs() < 1e-6,
            "x(t)=t at {t}: {:?}",
            sample.position
        );
        assert!(
            (sample.position[1] - t * t / 2.0).abs() < 1e-3,
            "y(t)=t^2/2 at {t}: {:?}",
            sample.position
        );
    }
}

#[test]
fn steady_uniform_field_freezes_and_moves_along_the_same_spatial_path() {
    let field = field("(1, 0)");
    let domain = Domain::new(Vec3::splat(-3.0), Vec3::splat(3.0));
    let frozen = trace_frozen_line(&field, REFERENCE_SEED, 2, domain, 4096)
        .expect("steady field traces a line");

    let mut state = State::new(true);
    state.set_formula("(1, 0)".into());
    let solution = solve_ode(&state, &current_field_scene(2.0)).expect("valid scene");
    let trajectory = &solution.trajectories[0].points;
    assert!(trajectory.len() > 2);

    // Both curves are the x axis, so each trajectory point lies on the frozen
    // line within the display sampling tolerance.
    for sample in trajectory {
        assert!(
            sample.position[1].abs() < 1e-9,
            "trajectory stays on y=0: {:?}",
            sample.position
        );
        assert!(
            sample.position[0] >= frozen.points[0][0]
                && sample.position[0] <= frozen.points[frozen.points.len() - 1][0],
            "trajectory x stays on the frozen segment: {:?}",
            sample.position
        );
    }
    for point in &frozen.points {
        assert!(point[1].abs() < 1e-9, "frozen line stays on y=0");
    }
    assert!(
        frozen.points.iter().any(|point| point[0] < 0.0),
        "the frozen line is traced in both directions"
    );
}

#[test]
fn playback_moves_only_the_trajectory_marker() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Ode);
    apply_time_dependent_reference(&mut app.state).expect("reference scene solves");
    app.ode.compare.enabled = true;
    app.ode.compare.seed = REFERENCE_SEED;
    app.test_harness().advance_simulation();

    let frozen = app
        .ode
        .compare
        .frozen
        .clone()
        .expect("comparison frozen line is cached");
    let line_count = app.test_harness().ode_line_count();
    let before = app
        .state
        .ode_solution
        .as_ref()
        .expect("solution")
        .markers_at(app.state.time as f64)[0];
    app.test_harness().advance_clock(0.5);
    let after = app
        .state
        .ode_solution
        .as_ref()
        .expect("solution")
        .markers_at(app.state.time as f64)[0];
    assert!(
        (after[1] - before[1]).abs() > 1e-3,
        "the trajectory marker advances with the shared clock"
    );
    app.test_harness().advance_simulation();
    assert_eq!(
        app.ode.compare.frozen.as_ref(),
        Some(&frozen),
        "the cached frozen line never moves with playback"
    );
    for point in &app.ode.compare.frozen.as_ref().expect("cached").points {
        assert!(point[1].abs() < 1e-9, "the frozen line stays at t=0");
    }
    assert_eq!(
        app.test_harness().ode_line_count(),
        line_count,
        "playback does not rebuild the frozen geometry"
    );
}

#[test]
fn time_dependent_reference_is_one_call_and_preserves_the_previous_formula() {
    let mut state = State::new(true);
    let user_formula = "(y, -x)".to_string();
    state.set_formula(user_formula.clone());

    let previous = apply_time_dependent_reference(&mut state).expect("reference applies");
    assert_eq!(
        previous, user_formula,
        "the previous formula is recoverable"
    );
    assert_eq!(state.formula, reference_formula());
    assert_eq!(state.formula, "(1, t)");
    assert_eq!(state.ode.source, OdeSource::CurrentField);
    assert_eq!(state.ode.dimension, 2);
    assert_eq!(state.ode.seeds.len(), 1);
    assert_eq!(state.ode.seeds[0].position, REFERENCE_SEED);
    assert!(state.ode.duration > 0.0);

    let solution = state.ode_solution.as_ref().expect("solved once");
    let end = solution.sample_at(0, state.ode.duration).expect("sample");
    assert!((end.position[0] - state.ode.duration).abs() < 1e-6);
    assert!(
        (end.position[1] - state.ode.duration * state.ode.duration / 2.0).abs() < 1e-3,
        "the reference trajectory is (t, t^2/2): {:?}",
        end.position
    );

    state.set_formula(previous.clone());
    assert_eq!(
        state.formula, user_formula,
        "the user formula can be restored"
    );
}

#[test]
fn steady_reference_sets_uniform_velocity_from_the_origin() {
    let mut state = State::new(true);
    let previous = apply_steady_reference(&mut state).expect("reference applies");
    assert!(!previous.is_empty());
    assert_eq!(state.formula, "(1, 0)");
    assert_eq!(state.ode.source, OdeSource::CurrentField);
    assert_eq!(
        state.ode.seeds,
        vec![OdeSeed {
            position: REFERENCE_SEED,
        }]
    );
    let solution = state.ode_solution.as_ref().expect("solved once");
    for sample in &solution.trajectories[0].points {
        assert!(
            sample.position[1].abs() < 1e-9,
            "the uniform field keeps the trajectory on y=0"
        );
    }
}

#[test]
fn armed_pick_replaces_the_seed_solves_once_and_clears_the_arm() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Ode);
    apply_time_dependent_reference(&mut app.state).expect("reference scene solves");
    app.ode.compare.enabled = true;

    let mut scene = app.state.ode.clone();
    scene.seeds = vec![
        OdeSeed {
            position: [0.0, 0.0, 0.0],
        },
        OdeSeed {
            position: [0.1, 0.1, 0.0],
        },
        OdeSeed {
            position: [0.2, 0.2, 0.0],
        },
    ];
    app.state.set_ode_scene(scene).expect("three seeds solve");
    let epoch = app.state.ode_epoch;
    app.ode.compare.arm_seed = true;

    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 300.0));
    app.test_seed_click(rect.center(), rect, rect.width() / rect.height(), 0.0);

    assert!(!app.ode.compare.arm_seed, "one click clears the arm");
    assert_eq!(
        app.state.ode.seeds.len(),
        1,
        "the seed is replaced, not appended"
    );
    let seed = app.state.ode.seeds[0].position;
    assert_eq!(app.ode.compare.seed, seed);
    assert_eq!(app.state.ode_epoch, epoch + 1, "one click solves once");
    assert_eq!(app.state.ode.source, OdeSource::CurrentField);
    let domain = app
        .state
        .ode_solution
        .as_ref()
        .expect("solution")
        .view_domain();
    assert!(seed[0] >= domain.min.x as f64 && seed[0] <= domain.max.x as f64);
    assert!(seed[1] >= domain.min.y as f64 && seed[1] <= domain.max.y as f64);

    // The next simulation frame rebuilds the comparison geometry from the new
    // seed, so the frozen line starts where the user picked.
    app.test_harness().advance_simulation();
    let frozen = app
        .ode
        .compare
        .frozen
        .as_ref()
        .expect("the picked seed rebuilds the frozen line");
    let junction = frozen
        .points
        .iter()
        .position(|point| *point == seed)
        .expect("the frozen line is traced from the picked seed");
    assert!(
        junction > 0 && junction + 1 < frozen.points.len(),
        "the picked seed is the junction of both trace directions: {junction}"
    );
}

#[test]
fn visibility_checkbox_rebuilds_frozen_segments_without_another_edit() {
    let ctx = panel_context();
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    app.layout.ode_expanded = true;
    app.state.set_field_mode(FieldMode::Ode);
    apply_time_dependent_reference(&mut app.state).expect("reference scene solves");

    let out = idle_frame(&ctx, &mut app);
    let baseline = app.test_harness().ode_line_count();
    assert!(!app.ode.compare.enabled, "the comparison starts off");
    assert!(app.ode.compare.frozen.is_none());
    let checkbox = widget_bounds(&out, "Show frozen line vs trajectory")
        .expect("the comparison checkbox is exposed to accessibility");
    click(&ctx, &mut app, checkbox.center());

    assert!(
        app.ode.compare.enabled,
        "the checkbox enables the comparison"
    );
    let frozen = app
        .ode
        .compare
        .frozen
        .clone()
        .expect("enabling caches the frozen line in the same frame");
    assert!(
        frozen.points.len() > 2,
        "the frozen line has interior samples"
    );
    assert!(
        app.test_harness().ode_line_count() > baseline,
        "enabling appends frozen segments without an unrelated edit"
    );

    // Uncheck the same control; its cached segments must disappear at once.
    let out = idle_frame(&ctx, &mut app);
    let checkbox = widget_bounds(&out, "Show frozen line vs trajectory")
        .expect("the comparison checkbox is still exposed");
    click(&ctx, &mut app, checkbox.center());

    assert!(
        !app.ode.compare.enabled,
        "the checkbox disables the comparison"
    );
    assert!(
        app.ode.compare.frozen.is_none(),
        "disabling drops the cached frozen line"
    );
    assert_eq!(
        app.test_harness().ode_line_count(),
        baseline,
        "disabling removes the frozen segments in the same frame"
    );
}

#[test]
fn unarmed_click_keeps_ordinary_ode_behavior() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Ode);
    apply_time_dependent_reference(&mut app.state).expect("reference scene solves");
    app.ode.compare.enabled = true;
    app.ode.compare.arm_seed = false;
    let scene = app.state.ode.clone();
    let epoch = app.state.ode_epoch;

    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 300.0));
    app.test_seed_click(rect.center(), rect, rect.width() / rect.height(), 0.0);

    assert_eq!(app.state.ode, scene, "an unarmed Ode click never seeds");
    assert_eq!(app.state.ode_epoch, epoch);
    assert!(!app.ode.compare.arm_seed);
}

#[test]
fn escape_cancels_the_armed_pick_without_editing() {
    let ctx = support::context();
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    app.state.set_field_mode(FieldMode::Ode);
    apply_time_dependent_reference(&mut app.state).expect("reference scene solves");
    app.ode.compare.enabled = true;

    frame(&ctx, &mut app, Vec::new());
    app.ode.compare.arm_seed = true;
    frame(&ctx, &mut app, vec![key(egui::Key::Escape)]);
    assert!(!app.ode.compare.arm_seed, "Escape cancels the arm");

    let scene = app.state.ode.clone();
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 300.0));
    app.test_seed_click(rect.center(), rect, rect.width() / rect.height(), 0.0);
    assert_eq!(app.state.ode, scene, "the cancelled click edits nothing");
}

#[test]
fn panel_legend_and_caveat_name_the_two_curves_and_the_velocity_field() {
    let ctx = panel_context();
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Ode);
    apply_time_dependent_reference(&mut state).expect("reference scene solves");
    let mut panel = OdePanelState::default();
    panel.compare.enabled = true;

    let out = panel_frame(&ctx, 1200.0, 1800.0, &mut state, &mut panel, false);
    let labels = nodes(&out);
    for label in [
        FROZEN_LABEL,
        TRAJECTORY_LABEL,
        CAVEAT,
        "Use v = (1, t) reference",
        "Use v = (1, 0) reference",
        "Pick seed in viewport",
    ] {
        assert!(
            labels.iter().any(|node| node.label == label),
            "wide panel is missing `{label}`: {:?}",
            labels.iter().map(|node| &node.label).collect::<Vec<_>>()
        );
    }
}

#[test]
fn comparison_panel_renders_wide_and_compact_without_panic() {
    for (width, compact) in [(1200.0, false), (390.0, true)] {
        let ctx = panel_context();
        let mut state = State::new(true);
        state.set_field_mode(FieldMode::Ode);
        apply_time_dependent_reference(&mut state).expect("reference scene solves");
        let mut panel = OdePanelState::default();
        panel.compare.enabled = true;
        panel.compare.arm_seed = true;
        let _ = panel_frame(&ctx, width, 2200.0, &mut state, &mut panel, compact);
    }
}

struct WidgetNode {
    label: String,
}

fn nodes(output: &egui::FullOutput) -> Vec<WidgetNode> {
    let Some(update) = output.platform_output.accesskit_update.as_ref() else {
        return Vec::new();
    };
    update
        .nodes
        .iter()
        .filter_map(|(_, node)| {
            let label = node
                .label()
                .or_else(|| node.value())
                .unwrap_or_default()
                .to_owned();
            (!label.is_empty()).then_some(WidgetNode { label })
        })
        .collect()
}

fn panel_context() -> egui::Context {
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
    panel: &mut OdePanelState,
    compact: bool,
) -> egui::FullOutput {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, height),
            )),
            ..Default::default()
        },
        |ui| {
            if compact {
                egui::CentralPanel::default().show(ui, |ui| {
                    let _ = ode_panel::show(ui, panel, state, true);
                });
            } else {
                egui::Panel::left("ode_compare_test_panel")
                    .exact_size(300.0)
                    .resizable(false)
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("ode_compare_test_body")
                            .show(ui, |ui| {
                                let _ = ode_panel::show(ui, panel, state, false);
                            });
                    });
            }
        },
    );
    output.textures_delta.clear();
    output
}

fn frame(ctx: &egui::Context, app: &mut VfApp, events: Vec<egui::Event>) -> egui::FullOutput {
    let mut native = eframe::Frame::_new_kittest();
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 1000.0),
            )),
            events,
            ..Default::default()
        },
        |ui| eframe::App::ui(app, ui, &mut native),
    );
    output.textures_delta.clear();
    output
}

/// Settle three input-free frames so the panel publishes its accessibility
/// nodes and the simulation rebuilds any pending geometry.
fn idle_frame(ctx: &egui::Context, app: &mut VfApp) -> egui::FullOutput {
    for _ in 0..3 {
        let _ = frame(ctx, app, Vec::new());
    }
    frame(ctx, app, Vec::new())
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

fn click(ctx: &egui::Context, app: &mut VfApp, pos: egui::Pos2) {
    let _ = frame(ctx, app, pointer(pos, true));
    let _ = frame(ctx, app, pointer(pos, false));
}

/// Screen-space bounds of the accessibility node carrying `label`.
fn widget_bounds(output: &egui::FullOutput, label: &str) -> Option<egui::Rect> {
    let update = output.platform_output.accesskit_update.as_ref()?;
    update.nodes.iter().find_map(|(_, node)| {
        if node.label() != Some(label) && node.value() != Some(label) {
            return None;
        }
        let bounds = node.bounds()?;
        Some(egui::Rect::from_min_max(
            egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
            egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
        ))
    })
}

fn key(key: egui::Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}
