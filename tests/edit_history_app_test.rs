//! Issue #86 slice A2: the app-level edit-history wiring.
//!
//! These tests drive the real `VfApp` through real egui frames (the same
//! harness pattern as `appearance_panel_test` and `plot_ui_test`) so the frame
//! driver, shortcut routing and scoped resets are exercised where the user
//! meets them. No GPU device is created.

use eframe::egui::{self, vec2};

use crate::support;
use crate::support::storage::MemoryStorage;

use vector_field_explorer::VfApp;
use vector_field_explorer::camera::OrbitCamera;
use vector_field_explorer::colormap::Colormap;
use vector_field_explorer::edit_history::EditScope;
use vector_field_explorer::parameters::ParameterSet;
use vector_field_explorer::ui::menu::MenuAction;
use vector_field_explorer::viewport_style::ViewportStyle;

/// Run one full production frame with the given events at a desktop size.
fn frame(ctx: &egui::Context, app: &mut VfApp, events: Vec<egui::Event>) -> egui::FullOutput {
    let mut native = eframe::Frame::_new_kittest();
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(900.0, 700.0),
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
    let _ = frame(ctx, app, Vec::new());
}

/// Let egui publish the current accesskit tree and settle popup state.
fn settle(ctx: &egui::Context, app: &mut VfApp) -> egui::FullOutput {
    for _ in 0..3 {
        let _ = frame(ctx, app, Vec::new());
    }
    frame(ctx, app, Vec::new())
}

/// The accesskit node carrying `label`, in screen coordinates. A button node
/// wins over a plain text/label node with the same text.
fn labelled_rect(output: &egui::FullOutput, label: &str) -> Option<egui::Rect> {
    let update = output.platform_output.accesskit_update.as_ref()?;
    update
        .nodes
        .iter()
        .filter_map(|(_, node)| {
            // Menu items append their shortcut hint to the accessible label
            // ("Undo Ctrl+Z"), so accept an exact label or label + hint.
            let matches = |text: &str| {
                text == label
                    || text
                        .strip_prefix(label)
                        .is_some_and(|rest| rest.starts_with(' '))
            };
            if !node.label().is_some_and(matches) && !node.value().is_some_and(matches) {
                return None;
            }
            let bounds = node.bounds()?;
            Some((
                node.role() == egui::Role::Button,
                egui::Rect::from_min_max(
                    egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                    egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                ),
            ))
        })
        .min_by(|a, b| b.0.cmp(&a.0).then(a.1.left().total_cmp(&b.1.left())))
        .map(|(_, rect)| rect)
}

fn moved(pos: egui::Pos2) -> egui::Event {
    egui::Event::PointerMoved(pos)
}

fn button(pos: egui::Pos2, pressed: bool) -> egui::Event {
    pointer(pos, egui::PointerButton::Primary, pressed)
}

fn pointer(pos: egui::Pos2, button: egui::PointerButton, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

/// A real press→release pair at `pos`, driven through two production frames.
fn click(ctx: &egui::Context, app: &mut VfApp, pos: egui::Pos2) {
    let _ = frame(ctx, app, vec![moved(pos), button(pos, true)]);
    let _ = frame(ctx, app, vec![button(pos, false)]);
}

/// Open `group` from the real menu bar and click `item`, both through genuine
/// pointer press→release frames so menu actions are exercised where the user
/// meets them.
fn menu_action(ctx: &egui::Context, app: &mut VfApp, group: &str, item: &str) {
    let out = settle(ctx, app);
    let group = labelled_rect(&out, group)
        .unwrap_or_else(|| panic!("the menu bar paints the {group:?} button"));
    click(ctx, app, group.center());
    let out = settle(ctx, app);
    let item = labelled_rect(&out, item)
        .unwrap_or_else(|| panic!("the {group:?} menu paints {item:?}"));
    click(ctx, app, item.center());
}

fn spin_button_center(output: &egui::FullOutput, label_prefix: &str) -> Option<egui::Pos2> {
    let update = output.platform_output.accesskit_update.as_ref()?;
    update.nodes.iter().find_map(|(_, node)| {
        if node.role() != egui::Role::SpinButton {
            return None;
        }
        let label = node.label()?;
        if !label.starts_with(label_prefix) {
            return None;
        }
        let bounds = node.bounds()?;
        Some(egui::pos2(
            ((bounds.x0 + bounds.x1) / 2.0) as f32,
            ((bounds.y0 + bounds.y1) / 2.0) as f32,
        ))
    })
}

#[test]
fn drag_is_one_event() {
    let ctx = support::context();
    ctx.enable_accesskit();
    let mut app = VfApp::new();
    // The first-launch help overlay can intercept pointer input; this test
    // exercises the Controls widget itself.
    app.layout.help_visible = false;
    let out = frame(&ctx, &mut app, Vec::new());
    let before = app.state.density;
    assert_eq!(app.history_len(), 0, "an idle frame records nothing");

    // A real drag across the Controls Density DragValue: one press, two moves
    // and one release. The value changes during the gesture; history must still
    // see exactly one edit.
    let start = spin_button_center(&out, "Samples per axis")
        .expect("the Controls panel paints the Density DragValue");
    let end = start + vec2(50.0, 0.0);
    let _ = frame(&ctx, &mut app, vec![moved(start), button(start, true)]);
    assert_eq!(app.history_len(), 0, "the press opens a transaction only");
    let _ = frame(&ctx, &mut app, vec![moved(start + vec2(25.0, 0.0))]);
    let _ = frame(&ctx, &mut app, vec![moved(end)]);
    assert_eq!(app.history_len(), 0, "mid-gesture frames stay suspended");
    let _ = frame(&ctx, &mut app, vec![button(end, false)]);

    assert_ne!(app.state.density, before, "the drag changed the density");
    assert_eq!(app.history_len(), 1, "one gesture is exactly one entry");
    assert!(app.undo_document(&ctx), "the gesture is undoable");
    assert_eq!(app.state.density, before, "undo restores the pre-drag value");
}

#[test]
fn secondary_button_does_not_split_a_primary_gesture() {
    let ctx = support::context();
    ctx.enable_accesskit();
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    let out = frame(&ctx, &mut app, Vec::new());
    let before = app.state.formula.clone();

    // Start a real primary press on the Density control. The widget is only
    // the press target; the document commits below model a panel editing the
    // same field across frames between press and release.
    let start = spin_button_center(&out, "Samples per axis")
        .expect("the Controls panel paints the Density DragValue");
    let _ = frame(&ctx, &mut app, vec![moved(start), button(start, true)]);
    assert!(app.state.set_formula("(y, -x)".into()));
    assert_eq!(app.history_len(), 0, "the gesture is still open");

    // A secondary-button press/release inside the primary gesture is not a
    // gesture boundary: it must neither commit the open transaction nor start
    // a second one.
    let _ = frame(
        &ctx,
        &mut app,
        vec![pointer(start, egui::PointerButton::Secondary, true)],
    );
    assert_eq!(
        app.history_len(),
        0,
        "a secondary press must not commit the primary gesture"
    );
    let _ = frame(
        &ctx,
        &mut app,
        vec![pointer(start, egui::PointerButton::Secondary, false)],
    );
    assert_eq!(
        app.history_len(),
        0,
        "a secondary release must not split the primary gesture"
    );
    assert_eq!(app.history_redo_len(), 0);

    // Finish the primary gesture: the whole gesture is exactly one entry, and
    // one undo restores the pre-press value.
    assert!(app.state.set_formula("(x, -y)".into()));
    let _ = frame(&ctx, &mut app, vec![moved(start + vec2(50.0, 0.0))]);
    let _ = frame(&ctx, &mut app, vec![button(start + vec2(50.0, 0.0), false)]);
    assert_eq!(
        app.history_len(),
        1,
        "the primary gesture remains exactly one entry"
    );
    assert!(app.undo_document(&ctx));
    assert_eq!(app.state.formula, before);
}

#[test]
fn menu_undo_during_an_open_gesture_consumes_one_entry_and_keeps_redo() {
    let ctx = support::context();
    ctx.enable_accesskit();
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    settle(&ctx, &mut app);

    let original = app.state.formula.clone();
    app.state.set_formula("(y, -x)".into());
    settle(&ctx, &mut app);
    assert_eq!(app.history_len(), 1, "the edit is recorded");

    // A real Edit > Undo click: the menu button's press opens a transaction and
    // the release dispatches the action. Undo must consume exactly the one
    // entry, never record the applied snapshot as a new edit, and preserve the
    // redo tail.
    menu_action(&ctx, &mut app, "Edit", "Undo");
    assert_eq!(app.history_len(), 0, "undo consumes exactly one entry");
    assert_eq!(app.history_redo_len(), 1, "the redo tail is preserved");
    assert_eq!(app.state.formula, original);

    // Redo through the same real menu path is the inverse and records nothing.
    menu_action(&ctx, &mut app, "Edit", "Redo");
    assert_eq!(app.history_len(), 1, "redo restores the entry");
    assert_eq!(app.history_redo_len(), 0);
    assert_eq!(app.state.formula, "(y, -x)");
}

#[test]
fn menu_colour_reset_is_one_recorded_entry_and_preserves_unrelated_state() {
    let ctx = support::context();
    ctx.enable_accesskit();
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    settle(&ctx, &mut app);

    // Change the formula, colour and camera together before the baseline.
    app.state.set_formula("(y, -x)".into());
    app.controls.colormap = Colormap::Turbo;
    app.orbit.yaw = 1.1;
    app.orbit.pitch = 0.4;
    app.orbit.target = glam::Vec3::new(1.0, 2.0, 3.0);
    settle(&ctx, &mut app);
    assert_eq!(app.history_len(), 1, "the combined change is one entry");
    let formula = app.state.formula.clone();
    let orbit = app.orbit;

    menu_action(&ctx, &mut app, "Edit", "Reset colour scope");
    settle(&ctx, &mut app);
    assert_eq!(app.state.colormap, Colormap::Viridis);
    assert_eq!(app.state.color_quantity, Default::default());
    assert_eq!(app.state.formula, formula, "the formula is untouched");
    assert_eq!(app.orbit, orbit, "the camera is untouched");
    assert_eq!(
        app.history_len(),
        2,
        "the menu reset is exactly one recorded entry"
    );

    // Undo restores the pre-reset colour without disturbing the document.
    assert!(app.undo_document(&ctx));
    assert_eq!(app.state.colormap, Colormap::Turbo);
    assert_eq!(app.state.formula, formula);
    assert_eq!(app.orbit, orbit);
}

#[test]
fn menu_camera_reset_is_reachable_recorded_and_reversible() {
    let ctx = support::context();
    ctx.enable_accesskit();
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    settle(&ctx, &mut app);

    app.state.set_formula("(y, -x)".into());
    app.orbit.yaw = 1.1;
    app.orbit.pitch = 0.4;
    app.orbit.target = glam::Vec3::new(1.0, 2.0, 3.0);
    app.orbit.ortho_zoom = 2.0;
    app.camera.position = [5.0, 6.0];
    app.camera.scale = 3.0;
    app.fly = Some(vector_field_explorer::fly_camera::FlyCamera::from_orbit(
        &app.orbit,
        1.0,
    ));
    settle(&ctx, &mut app);
    assert_eq!(app.history_len(), 1, "the combined change is one entry");
    let formula = app.state.formula.clone();
    let changed_orbit = app.orbit;

    // The View menu's existing "Reset camera" is the reachable Camera scope.
    menu_action(&ctx, &mut app, "View", "Reset camera");
    settle(&ctx, &mut app);
    assert_eq!(app.orbit, OrbitCamera::default());
    assert_eq!(app.camera.position, [0.0, 0.0]);
    assert!(app.fly.is_none(), "camera reset leaves fly mode");
    assert_eq!(app.state.formula, formula, "the document is untouched");
    assert_eq!(
        app.history_len(),
        2,
        "the camera reset is exactly one recorded entry"
    );

    // The scope is undoable as one entry; the rest of the document stays sane.
    assert!(app.undo_document(&ctx));
    assert_eq!(app.orbit, changed_orbit);
    assert_eq!(app.state.formula, formula);
    assert_eq!(app.history_len(), 1);
    assert_eq!(app.history_redo_len(), 1);
}

#[test]
fn frames_do_not_record() {
    let ctx = support::context();
    let mut app = VfApp::new();
    for _ in 0..6 {
        idle(&ctx, &mut app);
    }
    assert_eq!(app.history_len(), 0, "idle frames are not edits");

    // Simulation ticks move runtime state (time, particles) but never the
    // document snapshot.
    for _ in 0..10 {
        app.test_harness().advance_clock(0.016);
        app.test_harness().advance_simulation();
    }
    for _ in 0..4 {
        idle(&ctx, &mut app);
    }
    assert_eq!(app.history_len(), 0, "simulation ticks are not edits");
    assert_eq!(app.history_redo_len(), 0, "and they open no redo tail");
}

#[test]
fn file_effects_do_not_replay() {
    let ctx = support::context();
    let mut app = VfApp::new();
    idle(&ctx, &mut app);

    app.state.set_formula("(y, -x)".into());
    idle(&ctx, &mut app);
    assert_eq!(app.history_len(), 1, "the edit is recorded");

    let entries = app.history_len();
    let redo = app.history_redo_len();

    // Saving the settings envelope writes storage and captures the view, but
    // must not add a history entry or move the cursor.
    let mut storage = MemoryStorage::default();
    eframe::App::save(&mut app, &mut storage);
    assert!(!storage.values.is_empty(), "the envelope was written");

    // Exporting only queues the request; it is not a document edit.
    app.apply_menu_action_for_test(MenuAction::ExportScreenshot, &ctx);
    app.apply_menu_action_for_test(MenuAction::ExportScreenshotSvg, &ctx);
    idle(&ctx, &mut app);

    assert_eq!(app.history_len(), entries, "file effects leave history alone");
    assert_eq!(app.history_redo_len(), redo, "and leave the cursor alone");
}

#[test]
fn rejected_edit_keeps_history() {
    let ctx = support::context();
    let mut app = VfApp::new();
    idle(&ctx, &mut app);

    app.state.set_formula("(y, -x)".into());
    idle(&ctx, &mut app);
    assert_eq!(app.history_len(), 1);

    // A rejected commit leaves the document, dirty flag and history untouched.
    let rejected = ParameterSet {
        version: 999,
        ..Default::default()
    };
    assert!(
        app.state.commit_parameters(rejected).is_err(),
        "the invalid set is rejected"
    );
    idle(&ctx, &mut app);
    assert_eq!(app.history_len(), 1, "a rejected edit records nothing");
    assert_eq!(app.history_redo_len(), 0);

    assert!(app.undo_document(&ctx));
    assert_eq!(
        app.state.formula,
        vector_field_explorer::viewmodel::FORMULA_2D,
        "undo restores the entry before the accepted edit, not the rejected one"
    );
}

#[test]
fn reset_scope_preserves_other_state() {
    let ctx = support::context();
    let mut app = VfApp::new();
    idle(&ctx, &mut app);

    // Change formula, colour and camera together; one frame records one entry.
    app.state.set_formula("(y, -x)".into());
    app.controls.colormap = Colormap::Turbo;
    app.orbit.yaw = 1.1;
    app.orbit.pitch = 0.4;
    app.orbit.target = glam::Vec3::new(1.0, 2.0, 3.0);
    idle(&ctx, &mut app);
    let changed = app.history_len();
    assert_eq!(changed, 1, "the three changes are one recorded edit");
    assert_eq!(app.state.colormap, Colormap::Turbo);

    // Color: only the mapping returns; the document and camera stay.
    let formula = app.state.formula.clone();
    let orbit = app.orbit;
    app.reset_scope(EditScope::Color);
    idle(&ctx, &mut app);
    assert_eq!(app.state.colormap, Colormap::Viridis);
    assert_eq!(app.state.color_quantity, Default::default());
    assert_eq!(app.state.formula, formula, "formula is untouched");
    assert_eq!(app.orbit, orbit, "camera is untouched");
    assert_eq!(app.history_len(), changed + 1, "one recorded reset entry");

    // Camera: only the pose returns.
    app.reset_scope(EditScope::Camera);
    idle(&ctx, &mut app);
    assert_eq!(app.orbit, OrbitCamera::default());
    assert_eq!(app.state.formula, formula, "formula survives a camera reset");
    assert_eq!(
        app.state.colormap,
        Colormap::Viridis,
        "colour survives a camera reset"
    );
    assert_eq!(app.history_len(), changed + 2, "one recorded reset entry");

    // Simulation: runtime state only, undo-free and document-neutral.
    let entries = app.history_len();
    app.reset_scope(EditScope::Simulation);
    idle(&ctx, &mut app);
    assert_eq!(app.clock.time, 0.0, "simulation time returns to zero");
    assert_eq!(app.history_len(), entries, "a simulation reset is undo-free");
    assert_eq!(app.state.formula, formula);
    assert_eq!(app.state.colormap, Colormap::Viridis);
    assert_eq!(app.orbit, OrbitCamera::default());
}

#[test]
fn undo_leaves_the_fly_pose_alone() {
    let ctx = support::context();
    let mut app = VfApp::new();
    idle(&ctx, &mut app);

    app.state.set_mode(true);
    app.state.set_formula("(x, y, z+t)".into());
    idle(&ctx, &mut app);
    assert_eq!(app.history_len(), 1);

    let fly = vector_field_explorer::fly_camera::FlyCamera::from_orbit(&app.orbit, 1.0);
    app.fly = Some(fly);
    assert!(app.undo_document(&ctx));
    assert_eq!(app.fly, Some(fly), "the fly pose is runtime state");
}

#[test]
fn undo_restores_the_viewport_appearance() {
    let ctx = support::context();
    let mut app = VfApp::new();
    idle(&ctx, &mut app);
    let before = app.state.viewport_style;

    // Viewport style is appearance-only: it is not a `ViewRecord` field, so
    // this pins that the appearance half of the snapshot is applied on undo.
    app.state
        .commit_viewport_style(ViewportStyle::white_paper())
        .expect("the built-in white-paper style validates");
    idle(&ctx, &mut app);
    assert_ne!(app.state.viewport_style, before);
    assert_eq!(app.history_len(), 1, "the style change is one entry");

    assert!(app.undo_document(&ctx));
    assert_eq!(app.state.viewport_style, before);
}

#[test]
fn capture_then_apply_is_a_no_op_for_a_changed_document() {
    let ctx = support::context();
    let mut app = VfApp::new();
    idle(&ctx, &mut app);

    // Mutate the document through production setters/mirrors.
    app.state.set_formula("(y, -x)".into());
    app.controls.colormap = Colormap::Turbo;
    app.orbit.yaw = 0.7;
    app.orbit.target = glam::Vec3::new(-1.0, 0.5, 2.0);
    idle(&ctx, &mut app);

    let snapshot = app.capture_document_for_test();
    app.apply_document_for_test(&snapshot);
    assert_eq!(
        app.capture_document_for_test(),
        snapshot,
        "apply(capture(state)) reproduces the same document"
    );
    assert_eq!(app.state.formula, "(y, -x)");
    assert_eq!(app.state.colormap, Colormap::Turbo);
    assert_eq!(app.orbit.yaw, 0.7);
}
