//! Issue #86 slice A2: the app-level edit-history wiring.
//!
//! These tests drive the real `VfApp` through real egui frames (the same
//! harness pattern as `appearance_panel_test` and `plot_ui_test`) so the frame
//! driver, shortcut routing and scoped resets are exercised where the user
//! meets them. No GPU device is created.

use eframe::egui::{self, vec2};
use glam::{UVec3, Vec3};

use crate::support;
use crate::support::storage::MemoryStorage;

use vector_field_explorer::VfApp;
use vector_field_explorer::camera::OrbitCamera;
use vector_field_explorer::colormap::Colormap;
use vector_field_explorer::custom_density::{
    DensityFamily, DensitySpec, DensitySupport, SupportKind, TimeLaw,
};
use vector_field_explorer::domain::Domain;
use vector_field_explorer::edit_history::EditScope;
use vector_field_explorer::material::{MaterialCase, MaterialScene};
use vector_field_explorer::parameters::{Parameter, ParameterSet};
use vector_field_explorer::persistence::ViewRecord;
use vector_field_explorer::plane::{PlaneAxis, PlaneFrame, PlaneView};
use vector_field_explorer::scalar::ScalarContext;
use vector_field_explorer::scalar_view::MAX_PROBES;
use vector_field_explorer::scene_document::{SceneCamera, SceneDocument};
use vector_field_explorer::seed_layout::{SeedAnchor, SeedLayout, SeedPoint, clamp_seed_local};
use vector_field_explorer::source_scene::FieldMode;
use vector_field_explorer::ui::menu::MenuAction;
use vector_field_explorer::ui::presets::PresetPanel;
use vector_field_explorer::viewport_style::ViewportStyle;

/// Run one full production frame with the given events at a desktop size.
fn frame(ctx: &egui::Context, app: &mut VfApp, events: Vec<egui::Event>) -> egui::FullOutput {
    frame_height(ctx, app, events, 700.0)
}

/// Run one production frame at an explicit window height.
fn frame_height(
    ctx: &egui::Context,
    app: &mut VfApp,
    events: Vec<egui::Event>,
    height: f32,
) -> egui::FullOutput {
    let mut native = eframe::Frame::_new_kittest();
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(900.0, height),
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

/// A real press→release pair in a tall window, so lower sidebar sections stay
/// on screen and remain clickable.
fn click_tall(ctx: &egui::Context, app: &mut VfApp, pos: egui::Pos2) {
    let _ = frame_height(ctx, app, vec![moved(pos), button(pos, true)], 4000.0);
    let _ = frame_height(ctx, app, vec![button(pos, false)], 4000.0);
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
    let item =
        labelled_rect(&out, item).unwrap_or_else(|| panic!("the {group:?} menu paints {item:?}"));
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
    assert_eq!(
        app.state.density, before,
        "undo restores the pre-drag value"
    );
}

#[test]
fn a_generated_application_is_one_history_entry() {
    let ctx = support::context();
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    let _ = frame(&ctx, &mut app, Vec::new());
    // A blank viewport press opens the gesture without focusing a text editor
    // (the headless renderer has no wgpu device, so it seeds nothing).
    let start = egui::pos2(450.0, 350.0);

    // A generator application happens inside one primary gesture: the press
    // opens the transaction, the release commits the whole application once.
    let _ = frame(&ctx, &mut app, vec![moved(start), button(start, true)]);
    let center = app.state.scene_domain().center();
    app.state
        .apply_generated_seeds(&[center, center + Vec3::new(0.2, 0.0, 0.0)])
        .expect("the generated seeds apply");
    assert_eq!(app.history_len(), 0, "mid-gesture frames stay suspended");
    let _ = frame(&ctx, &mut app, vec![button(start, false)]);

    assert_eq!(app.history_len(), 1, "one application is one history entry");
    assert_eq!(app.state.seed_layout().len(), 2);
    assert!(app.undo_document(&ctx), "the application is undoable");
    assert!(
        app.state.seed_layout().is_empty(),
        "undo removes the whole application"
    );
}

#[test]
fn a_seed_drag_is_one_history_entry_and_clamps_to_the_plane() {
    let domain = Domain::new(Vec3::splat(-1.0), Vec3::splat(1.0));
    let plane_frame = PlaneFrame::new(
        &domain,
        PlaneView {
            enabled: true,
            axis: PlaneAxis::Z,
            index: 0,
        },
        UVec3::splat(8),
    );
    let clamped = clamp_seed_local(&plane_frame, Vec3::new(100.0, -100.0, 3.0));
    assert!(
        clamped.x.abs() <= plane_frame.half_u + 1e-5
            && clamped.y.abs() <= plane_frame.half_v + 1e-5,
        "the drag clamps into the active plane: {clamped:?}"
    );
    assert_eq!(clamped.z, 0.0, "the drag stays on the plane");

    let ctx = support::context();
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    let _ = frame(&ctx, &mut app, Vec::new());
    let start = egui::pos2(450.0, 350.0);

    let center = app.state.scene_domain().center();
    let dragged = SeedLayout {
        seeds: vec![SeedPoint {
            position: center,
            anchor: Some(SeedAnchor {
                axis: PlaneAxis::Z,
                index: 0,
                u: clamped.x,
                v: clamped.y,
            }),
        }],
    };
    let _ = frame(&ctx, &mut app, vec![moved(start), button(start, true)]);
    app.state
        .commit_seed_layout(dragged)
        .expect("the clamped drag position commits");
    assert_eq!(app.history_len(), 0, "the drag is still one open gesture");
    let _ = frame(&ctx, &mut app, vec![button(start, false)]);

    assert_eq!(app.history_len(), 1, "one completed drag is one entry");
    assert!(app.undo_document(&ctx), "the drag is undoable");
    assert!(app.state.seed_layout().is_empty(), "undo restores no seeds");
}

#[test]
fn hiding_the_seed_editor_disarms_place_mode_and_visible_arming_plants_one_seed() {
    let ctx = support::context();
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    app.controls.streamlines = true;
    let _ = frame(&ctx, &mut app, Vec::new());
    assert!(app.state.streamlines, "the Seed editor is visible");

    // Arming while the editor is visible survives a frame.
    app.controls.place_seed = true;
    let _ = frame(&ctx, &mut app, Vec::new());
    assert!(
        app.controls.place_seed,
        "a visible editor keeps the armed flag"
    );

    // A viewport click plants exactly one document seed and disarms.
    let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), vec2(900.0, 700.0));
    let aspect = rect.width() / rect.height();
    app.test_seed_click(rect.center(), rect, aspect, 0.0);
    assert_eq!(
        app.state.seed_layout().len(),
        1,
        "arming plants exactly one document seed"
    );
    assert!(!app.controls.place_seed, "placement disarms place mode");

    // Hide the editor (Streamlines off), then re-enable it. The armed flag must
    // not survive the hidden interval: re-enabling must not silently seed.
    app.controls.place_seed = true;
    app.controls.streamlines = false;
    let _ = frame(&ctx, &mut app, Vec::new());
    assert!(!app.state.streamlines, "the editor is hidden");
    assert!(
        !app.controls.place_seed,
        "hiding the editor disarms place mode"
    );

    app.controls.streamlines = true;
    let _ = frame(&ctx, &mut app, Vec::new());
    assert!(app.state.streamlines, "the editor is visible again");
    assert!(
        !app.controls.place_seed,
        "re-enabling the editor does not re-arm place mode"
    );

    // A later click must not plant a seed left armed while hidden.
    app.test_seed_click(rect.center() + vec2(40.0, 0.0), rect, aspect, 0.0);
    assert_eq!(
        app.state.seed_layout().len(),
        1,
        "no hidden seed is planted after re-enabling"
    );
}

#[test]
fn capture_activation_mid_seed_drag_stops_motion_and_keeps_one_entry() {
    let ctx = support::context();
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    // A 3D formula in the 3D view drives the ray-pick drag path.
    assert!(app.state.set_formula("(y, -x, z)".into()));
    assert!(app.state.set_mode(true));
    let original = app.state.scene_domain().center();
    app.state
        .commit_seed_layout(SeedLayout {
            seeds: vec![SeedPoint {
                position: original,
                anchor: None,
            }],
        })
        .expect("the starting seed commits");
    let _ = frame(&ctx, &mut app, Vec::new());
    let base_history = app.history_len();

    let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), vec2(900.0, 700.0));
    let aspect = rect.width() / rect.height();
    let press = rect.center();
    // A real primary press opens the gesture transaction. The headless viewport
    // returns before `seed_pointer_interaction`, so the drag itself is driven
    // through the same production move seam.
    let _ = frame(&ctx, &mut app, vec![moved(press), button(press, true)]);
    app.controls.seed_drag = Some(0);

    app.test_update_seed_drag(press + vec2(60.0, 0.0), rect, aspect);
    let moved_position = app.state.seed_layout().seeds[0].position;
    assert_ne!(moved_position, original, "the drag moves the seed");

    // Capture/fly activation mid-drag ends the gesture instead of writing.
    app.test_harness().set_capture_active(true);
    app.test_update_seed_drag(press + vec2(120.0, 0.0), rect, aspect);
    assert_eq!(
        app.state.seed_layout().seeds[0].position,
        moved_position,
        "a captured drag stops following the pointer"
    );
    assert!(
        app.controls.seed_drag.is_none(),
        "the captured drag is cleared rather than left open"
    );

    let _ = frame(
        &ctx,
        &mut app,
        vec![button(press + vec2(120.0, 0.0), false)],
    );
    assert_eq!(
        app.history_len(),
        base_history + 1,
        "the whole gesture is still exactly one history entry"
    );
    assert!(app.undo_document(&ctx), "the drag is undoable");
    assert_eq!(
        app.state.seed_layout().seeds[0].position,
        original,
        "undo restores the pre-drag seed position"
    );
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
        &app.orbit, 1.0,
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
fn same_mode_load_resyncs_the_materials_draft() {
    let ctx = support::context();
    let mut app = VfApp::new();
    idle(&ctx, &mut app);
    app.state.set_field_mode(FieldMode::Materials);
    let saved = MaterialScene {
        case: MaterialCase::PermeableSphere,
        radius: 0.8,
        ..MaterialScene::default()
    };
    app.state
        .set_material_scene(saved.clone())
        .expect("the saved material scene is valid");
    let document = SceneDocument::capture(
        "Saved materials",
        ViewRecord::capture(&app.state),
        app.state.parameters.clone(),
        SceneCamera::default(),
    );
    let dir = tempfile::tempdir().unwrap();
    let mut presets = PresetPanel::new_with_dir(dir.path().to_path_buf()).unwrap();
    presets.insert_scene(document).expect("the scene saves");
    app.presets = presets;

    // A same-mode state change with a stale user draft: the draft still names
    // the old scene and would overwrite the restored one on the next edit.
    app.state
        .set_material_scene(MaterialScene::default())
        .expect("a valid live material scene");
    app.material.sync_from(&app.state);
    app.material.draft.case = MaterialCase::DielectricBoundary;

    app.test_load_scene("Saved materials");
    assert_eq!(app.state.material, saved);
    assert_eq!(
        app.material.draft, app.state.material,
        "loading must rebind the panel draft to the restored model"
    );
}

#[test]
fn same_mode_undo_resyncs_the_materials_draft() {
    let ctx = support::context();
    let mut app = VfApp::new();
    idle(&ctx, &mut app);
    app.state.set_field_mode(FieldMode::Materials);
    idle(&ctx, &mut app);
    app.material.sync_from(&app.state);
    let restored = app.state.material.clone();

    // A direct durable edit is recorded by the next idle frame. The panel
    // draft stays on the old scene until a restore replaces the model.
    app.state
        .set_material_scene(MaterialScene {
            case: MaterialCase::PermeableSphere,
            ..MaterialScene::default()
        })
        .expect("a valid live material scene");
    app.material.draft.case = MaterialCase::DielectricBoundary;
    idle(&ctx, &mut app);

    assert!(app.undo_document(&ctx));
    idle(&ctx, &mut app);
    assert_eq!(app.state.material, restored);
    assert_eq!(
        app.material.draft, app.state.material,
        "undo must rebind the panel draft to the restored model"
    );
}

#[test]
fn undo_after_an_ode_step_preserves_the_solution_and_playback_clock() {
    let ctx = support::context();
    let mut app = VfApp::new();
    idle(&ctx, &mut app);
    app.state.set_field_mode(FieldMode::Ode);
    app.test_harness().advance_simulation();
    app.test_harness().ode_step();
    let clock = app.clock.time;
    let epoch = app.state.ode_epoch;
    let solution = app.state.ode_solution.clone().expect("solved");

    // An unrelated document edit records one history entry; undoing it must
    // restore the document without restarting ODE playback.
    app.state.set_formula("(y, -x)".into());
    idle(&ctx, &mut app);
    assert_eq!(app.history_len(), 1);
    assert!(app.undo_document(&ctx));
    app.test_harness().advance_simulation();

    assert_eq!(
        app.state.ode_epoch, epoch,
        "the solution epoch is unchanged"
    );
    assert!(
        std::sync::Arc::ptr_eq(
            &solution,
            app.state.ode_solution.as_ref().expect("preserved solution")
        ),
        "the unchanged solution is preserved, not re-solved"
    );
    assert_eq!(app.clock.time, clock, "stepped playback time survives undo");
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

    assert_eq!(
        app.history_len(),
        entries,
        "file effects leave history alone"
    );
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
    assert_eq!(
        app.state.formula, formula,
        "formula survives a camera reset"
    );
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
    assert_eq!(
        app.history_len(),
        entries,
        "a simulation reset is undo-free"
    );
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

#[test]
fn library_actions_do_not_record_history_but_applying_does() {
    let ctx = support::context();
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    let _ = frame(&ctx, &mut app, Vec::new());
    assert_eq!(app.history_len(), 0, "the first idle frame records nothing");

    let stored = seed_layout_profile("Mine", &[Vec3::ZERO]);
    // Saving a library entry is a settings-side action, not a document edit.
    app.seed_layouts
        .upsert(stored.clone(), &app.state.scene_domain())
        .expect("the layout is valid against the live domain");
    let _ = frame(&ctx, &mut app, Vec::new());
    assert_eq!(app.history_len(), 0, "saving a layout never records");
    assert!(app.state.seed_layout().is_empty(), "saving does not apply");

    // Deleting one is likewise history-neutral.
    assert!(app.seed_layouts.remove("Mine"));
    let _ = frame(&ctx, &mut app, Vec::new());
    assert_eq!(app.history_len(), 0, "deleting a layout never records");

    // Applying replaces the seeds through the existing commit seam. The next
    // idle frame records that document change exactly once.
    app.state
        .commit_seed_layout(stored.seeds.clone())
        .expect("the stored seeds validate");
    let _ = frame(&ctx, &mut app, Vec::new());
    assert_eq!(
        app.history_len(),
        1,
        "applying a layout is exactly one undo entry"
    );
    assert_eq!(app.state.seed_layout().len(), 1);
    let _ = frame(&ctx, &mut app, Vec::new());
    assert_eq!(app.history_len(), 1, "a second idle frame adds nothing");

    assert!(app.undo_document(&ctx), "the apply is undoable");
    assert!(
        app.state.seed_layout().is_empty(),
        "undo restores the pre-apply seeds"
    );
}

fn seed_layout_profile(
    name: &str,
    positions: &[Vec3],
) -> vector_field_explorer::seed_layout_library::SeedLayoutProfile {
    vector_field_explorer::seed_layout_library::SeedLayoutProfile {
        name: name.to_owned(),
        seeds: SeedLayout {
            seeds: positions
                .iter()
                .map(|position| SeedPoint {
                    position: *position,
                    anchor: None,
                })
                .collect(),
        },
    }
}

#[test]
fn probe_placement_adds_one_undo_entry_and_respects_the_cap() {
    let ctx = support::context();
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), vec2(900.0, 700.0));
    let aspect = rect.width() / rect.height();
    let center = rect.center();
    let base = app.history_len();

    app.controls.scalar_place_probe = true;
    // The Scalar "Place probe" checkbox arms one placement even while the
    // viewport strip's Keep placing default is active.
    app.controls.viewport_armed_one_shot = true;
    let _ = frame(&ctx, &mut app, vec![moved(center), button(center, true)]);
    app.test_seed_click(center, rect, aspect, 0.0);
    let _ = frame(&ctx, &mut app, vec![button(center, false)]);

    assert_eq!(app.state.probes.points.len(), 1, "one probe is placed");
    assert!(
        !app.controls.scalar_place_probe,
        "placement disarms the click"
    );
    assert_eq!(
        app.history_len(),
        base + 1,
        "placement is exactly one undo entry"
    );
    assert!(app.undo_document(&ctx));
    assert!(
        app.state.probes.points.is_empty(),
        "undo removes the placed probe"
    );

    // Fill the shared cap and prove the next placement is rejected with the
    // message slot instead of growing the document.
    for _ in 0..MAX_PROBES {
        app.state
            .add_point_probe(Vec3::new(0.1, 0.1, 0.0), None)
            .expect("room for the capped test probes");
    }
    app.controls.scalar_place_probe = true;
    app.controls.viewport_armed_one_shot = true;
    app.test_seed_click(center, rect, aspect, 0.0);
    assert_eq!(
        app.state.probes.points.len(),
        MAX_PROBES,
        "the cap is enforced"
    );
    assert!(
        app.controls.scalar_message.contains("at most 8"),
        "the cap rejection is reported: {:?}",
        app.controls.scalar_message
    );
    assert!(
        !app.controls.scalar_place_probe,
        "a cap rejection disarms the probe; retrying cannot succeed"
    );
}

#[test]
fn leaving_the_scalar_mode_while_armed_disarms_and_reports() {
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), vec2(900.0, 700.0));
    let aspect = rect.width() / rect.height();
    let center = rect.center();

    app.state.set_streamlines(true);
    app.controls.scalar_place_probe = true;
    app.controls.viewport_armed_one_shot = true;
    assert!(
        app.state.set_field_mode(FieldMode::Sources),
        "the fixture leaves the formula field"
    );

    app.test_seed_click(center, rect, aspect, 0.0);

    assert!(
        !app.controls.scalar_place_probe,
        "an armed probe click disarms when the probe path is gone"
    );
    assert!(
        !app.controls.viewport_armed_one_shot,
        "the cancelled one-shot marker does not leak"
    );
    assert!(
        !app.controls.scalar_message.is_empty(),
        "the user is told why the click no longer places a probe"
    );
    assert!(
        app.status
            .hint
            .as_deref()
            .is_some_and(|hint| hint.contains("Probes are unavailable")),
        "the status bar reports the same reason: {:?}",
        app.status.hint
    );
    assert!(
        app.state.probes.points.is_empty(),
        "the unavailable placement adds nothing"
    );
}

#[test]
fn probe_rows_show_values_and_remove_is_one_edit() {
    let ctx = support::context();
    ctx.enable_accesskit();
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    app.state
        .set_scalar_source(ScalarContext::Surface, "x + y".into());
    app.state
        .add_point_probe(Vec3::new(0.2, 0.2, 0.0), None)
        .expect("room for one probe");
    let _ = app.state.refresh_scalar_view(true);

    let mut out = frame_height(&ctx, &mut app, Vec::new(), 4000.0);
    for _ in 0..3 {
        out = frame_height(&ctx, &mut app, Vec::new(), 4000.0);
    }
    let header = labelled_rect(&out, "Scalar view").expect("the section header is exposed");
    click_tall(&ctx, &mut app, header.center());
    for _ in 0..20 {
        let _ = frame_height(&ctx, &mut app, Vec::new(), 4000.0);
    }
    let out = frame_height(&ctx, &mut app, Vec::new(), 4000.0);

    let texts: Vec<String> = out
        .platform_output
        .accesskit_update
        .as_ref()
        .map(|update| {
            update
                .nodes
                .iter()
                .filter_map(|(_, node)| {
                    node.label()
                        .map(str::to_owned)
                        .or_else(|| node.value().map(str::to_owned))
                })
                .collect()
        })
        .unwrap_or_default();
    assert!(
        texts
            .iter()
            .any(|text| text.starts_with("Probe 1:") && text.contains("0.4")),
        "the probe row shows its sampled value: {texts:?}"
    );

    let remove = labelled_rect(&out, "Remove").expect("the probe row exposes Remove");
    let base = app.history_len();
    click_tall(&ctx, &mut app, remove.center());
    assert!(app.state.probes.points.is_empty(), "Remove drops the probe");
    assert_eq!(
        app.history_len(),
        base + 1,
        "Remove is exactly one undo entry"
    );
    assert!(app.undo_document(&ctx));
    assert_eq!(
        app.state.probes.points.len(),
        1,
        "undo restores the removed probe"
    );
}

fn parameter_dependent_density() -> DensitySpec {
    DensitySpec {
        name: "scaled".to_string(),
        family: DensityFamily::VolumeCharge,
        support: DensitySupport {
            kind: SupportKind::Box,
            center: Vec3::ZERO,
            axis: Vec3::Z,
            size: Vec3::splat(2.0),
        },
        formula: "gain".to_string(),
        samples: 8,
        softening: 0.0,
        time_law: TimeLaw::Steady,
        charge_formula: None,
    }
}

#[test]
fn undo_keeps_a_density_that_binds_a_restored_parameter() {
    let ctx = support::context();
    let mut app = VfApp::new();
    idle(&ctx, &mut app);

    let parameters = ParameterSet {
        version: 1,
        entries: vec![Parameter {
            name: "gain".to_string(),
            value: 2.0,
            min: None,
            max: None,
            step: 1.0,
        }],
    };
    app.state
        .commit_parameters(parameters)
        .expect("the fixture parameter set is valid");
    let density = parameter_dependent_density();
    app.state
        .commit_densities(vec![density.clone()])
        .expect("the density binds the committed parameter");
    idle(&ctx, &mut app);
    assert_eq!(app.state.densities(), [density.clone()].as_slice());

    // Removing the parameter leaves the live density list alone but makes the
    // stored formula unbindable. This is the state the undo entry records.
    app.state
        .commit_parameters(ParameterSet::default())
        .expect("an empty parameter set is valid");
    idle(&ctx, &mut app);
    assert_eq!(
        app.state.densities(),
        [density.clone()].as_slice(),
        "parameter removal does not itself delete densities"
    );

    assert!(app.undo_document(&ctx), "the parameter removal is undoable");
    assert_eq!(
        app.state.parameters.value("gain"),
        Some(2.0),
        "undo restores the parameter set"
    );
    assert_eq!(
        app.state.densities(),
        [density].as_slice(),
        "undo restores parameters before validating the density, so it survives"
    );
}
