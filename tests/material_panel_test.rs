//! Task 3: the responsive Materials panel, the fifth adapter selector, the
//! viewport label and the help rows. Widget assertions use the same
//! headless-accesskit helpers as `induction_panel_test.rs`; the rail/reset
//! checks drive a real `VfApp` frame.

use eframe::egui;

use crate::support;

use vector_field_explorer::VfApp;
use vector_field_explorer::material::{
    MATERIAL_RESPONSE_EMPTY_WARNING, MaterialCase, MaterialScene, MaterialView,
};
use vector_field_explorer::source_scene::FieldMode;
use vector_field_explorer::ui::formula_bar::{self, FormulaBarState};
use vector_field_explorer::ui::layout::SheetState;
use vector_field_explorer::ui::material_panel::{self, MaterialPanelState};
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
    state: &mut State,
    panel: &mut MaterialPanelState,
    compact: bool,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    run_frame(ctx, width, 1400.0, events, |ui| {
        egui::CentralPanel::default().show(ui, |ui| {
            let _ = material_panel::show(ui, panel, state, compact);
        });
    })
}

fn click_widget(
    ctx: &egui::Context,
    width: f32,
    state: &mut State,
    panel: &mut MaterialPanelState,
    compact: bool,
    label: &str,
) {
    let out = panel_frame(ctx, width, state, panel, compact, Vec::new());
    let rect = find(&nodes(&out), label)
        .unwrap_or_else(|| panic!("missing {label}"))
        .rect;
    panel_frame(
        ctx,
        width,
        state,
        panel,
        compact,
        pointer(rect.center(), true),
    );
    panel_frame(
        ctx,
        width,
        state,
        panel,
        compact,
        pointer(rect.center(), false),
    );
}

#[test]
fn material_panel_lists_all_cases_and_view_choices() {
    for (width, compact) in [(1200.0, false), (390.0, true)] {
        let ctx = context();
        let mut state = State::new(true);
        state.set_field_mode(FieldMode::Materials);
        let mut panel = MaterialPanelState::default();
        panel.sync_from(&state);
        let out = panel_frame(&ctx, width, &mut state, &mut panel, compact, Vec::new());
        let nodes = nodes(&out);
        for label in [
            "Materials",
            "Dielectric sphere",
            "Dielectric boundary",
            "Permeable sphere",
            "Permeable boundary",
            "Total",
            "Applied",
            "Response",
            "Fit",
            "Reset Materials section",
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
fn material_geometry_and_constants_commit_on_release() {
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Materials);
    let mut panel = MaterialPanelState::default();
    panel.sync_from(&state);
    let before = state.material.clone();

    panel.draft.radius = 0.5;
    panel.draft.background_epsilon_r = 2.0;
    panel.draft.background_mu_r = 1.5;
    panel.draft.material_epsilon_r = 9.0;
    panel.draft.material_mu_r = 3.0;
    panel.draft.applied = [2.0, 1.0, 0.0];
    assert_eq!(
        state.material, before,
        "a dirty draft must not reach the committed scene"
    );

    assert!(panel.commit(&mut state), "a valid draft commits");
    assert_eq!(state.material.radius, 0.5);
    assert_eq!(state.material.background_epsilon_r, 2.0);
    assert_eq!(state.material.background_mu_r, 1.5);
    assert_eq!(state.material.material_epsilon_r, 9.0);
    assert_eq!(state.material.material_mu_r, 3.0);
    assert_eq!(state.material.applied, [2.0, 1.0, 0.0]);
    assert!(panel.error.is_none());

    let last_valid = state.material.clone();
    panel.draft.material_epsilon_r = 0.5;
    assert!(!panel.commit(&mut state), "out-of-range constants reject");
    assert_eq!(
        state.material, last_valid,
        "a rejected edit keeps the last valid scene"
    );
    assert!(
        panel
            .error
            .as_deref()
            .unwrap_or("")
            .contains("material epsilon_r"),
        "the panel keeps an actionable error: {:?}",
        panel.error
    );
}

#[test]
fn invalid_material_draft_is_actionable_and_keeps_last_valid() {
    let ctx = context();
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Materials);
    let mut panel = MaterialPanelState::default();
    panel.sync_from(&state);
    panel.draft.radius = f64::NAN;
    assert!(!panel.commit(&mut state));
    let out = panel_frame(&ctx, 1200.0, &mut state, &mut panel, false, Vec::new());
    let painted = nodes(&out);
    assert!(
        has(
            &painted,
            "sphere radius must be finite and greater than zero"
        ),
        "the rejected value's message is visible"
    );
    assert_eq!(
        state.material.radius,
        MaterialScene::default().radius,
        "the last valid scene stays committed"
    );

    // The chosen behavior: a rejected draft error remains actionable while the
    // user edits the draft, and only a valid commit or Reset clears it.
    panel.draft.radius = 1.5;
    let out = panel_frame(&ctx, 1200.0, &mut state, &mut panel, false, Vec::new());
    assert!(
        has(
            &nodes(&out),
            "sphere radius must be finite and greater than zero"
        ),
        "the rejected draft error persists across an uncommitted edit"
    );
    assert!(panel.commit(&mut state));
    let out = panel_frame(&ctx, 1200.0, &mut state, &mut panel, false, Vec::new());
    assert!(
        !has(
            &nodes(&out),
            "sphere radius must be finite and greater than zero"
        ),
        "a valid commit clears the rejected-draft error"
    );
}

#[test]
fn materials_view_selection_and_reset_use_state() {
    let ctx = context();
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Materials);
    let mut panel = MaterialPanelState::default();
    panel.sync_from(&state);

    click_widget(&ctx, 1200.0, &mut state, &mut panel, false, "Response");
    assert_eq!(state.material.view, MaterialView::Response);
    click_widget(&ctx, 1200.0, &mut state, &mut panel, false, "Applied");
    assert_eq!(state.material.view, MaterialView::Applied);

    let out = panel_frame(&ctx, 1200.0, &mut state, &mut panel, false, Vec::new());
    let reset = find(&nodes(&out), "Reset Materials section")
        .expect("Reset button")
        .rect;
    panel_frame(
        &ctx,
        1200.0,
        &mut state,
        &mut panel,
        false,
        pointer(reset.center(), true),
    );
    let out = panel_frame(
        &ctx,
        1200.0,
        &mut state,
        &mut panel,
        false,
        pointer(reset.center(), false),
    );
    let _ = out;
    assert_eq!(
        panel.draft,
        MaterialScene::default(),
        "Reset restores the panel draft"
    );
}

#[test]
fn materials_compact_and_wide_panels_reflow() {
    for (width, compact) in [(1200.0, false), (390.0, true)] {
        let ctx = context();
        let mut state = State::new(true);
        state.set_field_mode(FieldMode::Materials);
        let mut panel = MaterialPanelState::default();
        panel.sync_from(&state);
        panel.advanced = true;
        let out = panel_frame(&ctx, width, &mut state, &mut panel, compact, Vec::new());
        let nodes = nodes(&out);
        for label in [
            "Radius",
            "Background εr",
            "Background μr",
            "Material εr",
            "Material μr",
            "Applied E (V/m)",
            "Advanced",
            "No general material solver",
        ] {
            let node =
                find(&nodes, label).unwrap_or_else(|| panic!("{width} pt is missing {label}"));
            assert!(
                node.rect.min.x >= -1.0 && node.rect.max.x <= width + 1.0,
                "{width} pt: {label} at {:?} leaves the window",
                node.rect
            );
        }
    }
}

#[test]
fn materials_features_and_cas_are_disabled() {
    let ctx = context();

    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Materials);
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
        "feature controls are disabled in Materials"
    );
    assert!(
        has(
            &painted,
            "Arrows only: streamlines, particles and click seeding stay off in Materials.",
        ),
        "the exact Materials features note is shown"
    );
    assert!(
        !has(&painted, "Coloring"),
        "Coloring is hidden in Materials mode"
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
            "CAS keeps its exact disabled message in Materials"
        );
    }
}

#[test]
fn material_help_rows_state_the_approximations() {
    let text = material_panel::material_help_rows()
        .iter()
        .map(|row| format!("{} {}", row.input, row.action))
        .collect::<Vec<_>>()
        .join(" ");
    for needle in [
        "homogeneous",
        "linear",
        "quasistatic",
        "no general solver",
        "interface",
    ] {
        assert!(text.contains(needle), "help is missing {needle}");
    }
}

#[test]
fn formula_bar_selects_materials_and_hides_the_formula_editor() {
    let ctx = context();
    let mut state = State::new(true);
    let mut bar = FormulaBarState::default();
    let out = run_frame(&ctx, 1200.0, 200.0, Vec::new(), |ui| {
        formula_bar::show(ui, &mut state, &mut bar, false);
    });
    let selector = find(&nodes(&out), "Materials")
        .expect("Materials selector")
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
    assert_eq!(state.field_mode, FieldMode::Materials);
    let nodes = nodes(&out);
    assert!(
        !has(&nodes, "Field formula"),
        "the formula editor is hidden"
    );
    assert!(has(&nodes, "Materials scene summary"));
    assert!(
        nodes
            .iter()
            .any(|node| node.label.contains("Dielectric sphere")),
        "the selected case shows"
    );
    let open = find(&nodes, "Open materials")
        .expect("Open materials button")
        .rect;
    run_frame(&ctx, 1200.0, 200.0, pointer(open.center(), true), |ui| {
        formula_bar::show(ui, &mut state, &mut bar, false)
    });
    run_frame(&ctx, 1200.0, 200.0, pointer(open.center(), false), |ui| {
        formula_bar::show(ui, &mut state, &mut bar, false)
    });
    assert!(bar.open_materials, "the button requests the panel");
}

#[test]
fn open_materials_reaches_the_wide_rail_or_compact_sheet() {
    let ctx = context();
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    app.state.set_field_mode(FieldMode::Materials);
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
    app.bar.open_materials = true;
    render(&mut app, 1200.0);
    assert!(
        app.layout.materials_expanded,
        "the wide request expands the Materials rail"
    );

    app.layout.materials_expanded = false;
    app.bar.open_materials = true;
    render(&mut app, 390.0);
    assert_eq!(
        app.sheet,
        SheetState::Materials,
        "the compact request opens the Materials sheet"
    );
}

#[test]
fn materials_reset_restores_the_scene_and_camera() {
    let ctx = context();
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    app.state.set_field_mode(FieldMode::Materials);
    app.layout.materials_expanded = true;
    app.state
        .set_material_scene(MaterialScene {
            radius: 0.5,
            material_epsilon_r: 9.0,
            ..MaterialScene::default()
        })
        .unwrap();
    app.orbit.target = glam::Vec3::new(7.0, 8.0, 9.0);
    app.orbit.yaw = 2.0;
    app.orbit.ortho_zoom = 4.0;

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
    let reset = find(&nodes(&out), "Reset Materials section")
        .expect("Reset Materials section")
        .rect;
    render(&mut app, pointer(reset.center(), true));
    render(&mut app, pointer(reset.center(), false));

    assert_eq!(app.state.material, MaterialScene::default());
    assert_eq!(app.orbit.target, glam::Vec3::ZERO);
    assert_eq!(app.orbit.yaw, 0.8);
    assert_eq!(app.orbit.pitch, 0.9);
    assert_eq!(app.orbit.distance, 3.2);
    assert_eq!(app.orbit.ortho_zoom, 1.0);
}

#[test]
fn materials_viewport_label_names_case_and_view() {
    let ctx = context();
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    app.state.set_field_mode(FieldMode::Materials);
    app.state
        .set_material_scene(MaterialScene {
            material_epsilon_r: 1.0,
            view: MaterialView::Response,
            ..MaterialScene::default()
        })
        .unwrap();
    let mut frame = eframe::Frame::_new_kittest();
    let mut out = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 900.0),
            )),
            ..Default::default()
        },
        |ui| eframe::App::ui(&mut app, ui, &mut frame),
    );
    out.textures_delta.clear();
    let texts = painted_texts(&out);
    assert!(
        texts
            .iter()
            .any(|text| text == "Materials - Dielectric sphere - Response"),
        "the viewport names the case and view; painted {texts:?}"
    );
    assert!(
        texts
            .iter()
            .any(|text| text == MATERIAL_RESPONSE_EMPTY_WARNING),
        "the current Response-empty note is painted"
    );
}

#[test]
fn material_warning_reflects_only_the_current_scene() {
    let ctx = context();
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Materials);
    state.set_density(25);
    state.set_mode(false);
    state.set_plane_index(12);
    state
        .set_material_scene(MaterialScene {
            case: MaterialCase::DielectricBoundary,
            interface_offset: 0.0,
            ..MaterialScene::default()
        })
        .unwrap();
    state.refresh_material_sampling();
    let mut panel = MaterialPanelState::default();
    panel.sync_from(&state);
    panel.advanced = true;
    let out = panel_frame(&ctx, 1200.0, &mut state, &mut panel, false, Vec::new());
    let first = painted_texts(&out).join("\n");
    assert!(
        first.contains("Slice lies on the interface"),
        "the current coincident-slice warning is shown: {first}"
    );

    // The warning belonged to the boundary slice. A valid edit clears the
    // sampling, so until the next refresh the old warning must not be shown as
    // if it described the new scene.
    panel.draft.case = MaterialCase::DielectricSphere;
    assert!(panel.commit(&mut state));
    assert!(state.material_sampling.is_none());
    let out = panel_frame(&ctx, 1200.0, &mut state, &mut panel, false, Vec::new());
    let stale = painted_texts(&out).join("\n");
    assert!(
        !stale.contains("Slice lies on the interface"),
        "a prior scene's warning is never shown as current: {stale}"
    );

    state.refresh_material_sampling();
    let out = panel_frame(&ctx, 1200.0, &mut state, &mut panel, false, Vec::new());
    let current = painted_texts(&out).join("\n");
    assert!(
        !current.contains("Slice lies on the interface"),
        "the refreshed sphere scene drops the boundary warning: {current}"
    );
}

#[test]
fn materials_rail_expands_a_real_app_frame() {
    let ctx = context();
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    app.state.set_field_mode(FieldMode::Materials);
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
    let rail = rail_bounds(&out, "Materials").expect("Materials rail");
    render(&mut app, pointer(rail.center(), true));
    let out = render(&mut app, pointer(rail.center(), false));
    assert!(
        app.layout.materials_expanded,
        "the rail expands the Materials panel"
    );
    assert!(
        has(&nodes(&out), "Reset Materials section"),
        "the expanded rail paints the Materials body"
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
