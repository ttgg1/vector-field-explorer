//! Issue #218: the promoted `Uniform field` exploration prompt and its
//! session-only closed-box flux activity.
//!
//! The activity reads the formula already loaded in the scene; it never adds a
//! probe surface, rewrites the formula except through the explicit
//! `F=(Fx,Fy,0)` extension action, or changes the Formula-mode probe refusal.

use eframe::egui;

use crate::support;

use vector_field_explorer::field::FieldExpr;
use vector_field_explorer::preset_library::{
    ExplorationActivity, PresetLibrary, exploration_activity,
};
use vector_field_explorer::render::Camera;
use vector_field_explorer::ui::presets::PresetPanel;
use vector_field_explorer::ui::uniform_flux::{
    self, UNIFORM_FLUX_FIELD_LABEL, UNIFORM_FLUX_TWO_COMPONENT_NOTE, UniformFluxReading,
    UniformFluxState,
};
use vector_field_explorer::viewmodel::State;

fn render_context() -> egui::Context {
    let ctx = support::context();
    ctx.enable_accesskit();
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    ctx
}

fn run_activity(
    ctx: &egui::Context,
    state: &mut State,
    activity: &mut UniformFluxState,
    width: f32,
    height: f32,
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
                egui::ScrollArea::vertical().show(ui, |ui| {
                    uniform_flux::show(ui, state, activity);
                });
            });
        },
    );
    output.textures_delta.clear();
    output
}

fn run_panel(
    ctx: &egui::Context,
    panel: &mut PresetPanel,
    state: &mut State,
    camera: &mut Camera,
    width: f32,
    height: f32,
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
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let _ = panel.show(ui, camera, state);
                });
            });
        },
    );
    output.textures_delta.clear();
    output
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

fn has_text_containing(output: &egui::FullOutput, text: &str) -> bool {
    painted_texts(output)
        .iter()
        .any(|painted| painted.contains(text))
}

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

fn click_panel_label(
    ctx: &egui::Context,
    panel: &mut PresetPanel,
    state: &mut State,
    camera: &mut Camera,
    label: &str,
    width: f32,
    height: f32,
) {
    let out = run_panel(ctx, panel, state, camera, width, height, Vec::new());
    let rect = nodes(&out)
        .into_iter()
        .find(|node| node.label == label)
        .unwrap_or_else(|| panic!("missing clickable {label}"))
        .rect;
    run_panel(
        ctx,
        panel,
        state,
        camera,
        width,
        height,
        pointer(rect.center(), true),
    );
    run_panel(
        ctx,
        panel,
        state,
        camera,
        width,
        height,
        pointer(rect.center(), false),
    );
}

#[test]
fn only_the_uniform_field_prompt_is_promoted_and_it_has_a_destination() {
    let library = PresetLibrary::builtin().expect("builtin library");
    let promoted: Vec<&str> = library
        .entries()
        .iter()
        .filter(|entry| exploration_activity(entry.name).is_some())
        .map(|entry| entry.name)
        .collect();
    assert_eq!(
        promoted,
        vec!["Uniform field"],
        "the Uniform field prompt is the only promoted activity in this slice"
    );
    assert_eq!(
        exploration_activity("Uniform field"),
        Some(ExplorationActivity::UniformFlux)
    );
    assert_eq!(exploration_activity("Vortex"), None);
    assert_eq!(exploration_activity("Coulomb point"), None);
    for entry in library.entries() {
        if exploration_activity(entry.name).is_some() {
            assert!(
                !entry.exploration_prompt.is_empty(),
                "a promoted prompt must exist"
            );
            assert!(
                !entry.assumptions.is_empty(),
                "a promoted prompt keeps its assumptions beside it"
            );
        }
    }
    let uniform = library
        .entries()
        .iter()
        .find(|entry| entry.name == "Uniform field")
        .expect("Uniform field entry");
    assert!(uniform.exploration_prompt.contains("flux"));
}

#[test]
fn three_component_uniform_field_has_six_signed_faces_that_cancel() {
    let field = FieldExpr::parse("(1, 0, 0)").expect("formula");
    let UniformFluxReading::ThreeComponent(flux) = uniform_flux::analyze(&field) else {
        panic!("a constant three-component field must produce a box table");
    };

    assert_eq!(flux.faces.len(), 6, "a closed box has six faces");
    for face in flux.faces {
        let magnitude =
            (face.normal[0].powi(2) + face.normal[1].powi(2) + face.normal[2].powi(2)).sqrt();
        assert_eq!(magnitude, 1.0, "every normal is a unit outward normal");
        assert_eq!(face.area, 4.0, "every face of the unit box has area 4");
    }
    let face = |normal: [f64; 3]| {
        flux.faces
            .iter()
            .find(|face| face.normal == normal)
            .expect("axis face")
            .flux
    };
    assert_eq!(face([1.0, 0.0, 0.0]), 4.0, "outgoing +x flux");
    assert_eq!(face([-1.0, 0.0, 0.0]), -4.0, "incoming -x flux");
    for normal in [
        [0.0, 1.0, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, -1.0],
    ] {
        assert_eq!(face(normal), 0.0, "no component crosses this face");
    }

    assert_eq!(flux.incoming(), -4.0);
    assert_eq!(flux.outgoing(), 4.0);
    assert!(
        flux.total.abs() <= flux.tolerance,
        "incoming and outgoing contributions cancel within the stated tolerance"
    );
    assert!(flux.cancels());
    assert!(flux.faces.iter().any(|face| face.flux > 0.0));
    assert!(flux.faces.iter().any(|face| face.flux < 0.0));
}

#[test]
fn the_generic_field_is_labelled_f_not_e_or_b() {
    assert_eq!(UNIFORM_FLUX_FIELD_LABEL, "F");
    let ctx = render_context();
    let mut state = State::new(false);
    state.set_formula("(1, 0, 0)".to_owned());
    let mut activity = UniformFluxState::default();
    activity.open();
    let output = run_activity(&ctx, &mut state, &mut activity, 900.0, 1400.0, Vec::new());

    assert!(has_text_containing(&output, "Field F"));
    assert!(has_text_containing(&output, "Flux through a closed box"));
    assert!(
        !has_text_containing(&output, "E ="),
        "a generic formula field is never labelled E"
    );
    assert!(
        !has_text_containing(&output, "B ="),
        "a generic formula field is never labelled B"
    );
}

#[test]
fn two_component_constant_field_previews_the_explicit_extension_and_restores_it() {
    let field = FieldExpr::parse("(2, -1)").expect("formula");
    let UniformFluxReading::TwoComponent(extension) = uniform_flux::analyze(&field) else {
        panic!("a genuinely constant two-component field offers the 3D extension");
    };
    assert_eq!(extension.field, [2.0, -1.0]);
    assert_eq!(extension.extension_formula, "(2, -1, 0)");
    assert!(
        UNIFORM_FLUX_TWO_COMPONENT_NOTE.contains("F=(Fx,Fy,0)"),
        "the preview names the extension before switching"
    );

    let mut state = State::new(false);
    state.set_formula("(2, -1)".to_owned());
    state.set_mode(false);
    let mut activity = UniformFluxState::default();
    activity.open();
    activity.preview_extension();
    assert!(activity.is_previewing());

    // Cancelling the preview and closing the activity never rewrite the
    // formula.
    activity.cancel_preview();
    assert_eq!(state.formula, "(2, -1)");
    activity.close();
    assert!(!activity.is_open());
    assert_eq!(state.formula, "(2, -1)");

    // Only the explicit switch changes the loaded formula, and the original
    // stays available to restore.
    activity.open();
    assert!(activity.apply_extension(&mut state, &extension.extension_formula));
    assert_eq!(state.formula, "(2, -1, 0)");
    assert!(state.mode_3d);
    assert_eq!(
        activity.restore_point().map(|point| point.formula.as_str()),
        Some("(2, -1)")
    );

    assert!(activity.restore_original(&mut state));
    assert_eq!(state.formula, "(2, -1)");
    assert!(!state.mode_3d);
    assert!(!activity.restore_available());
}

#[test]
fn non_constant_two_and_three_component_fields_get_no_extension_offer() {
    for formula in ["(x, y)", "(x, 0)", "(sin(x), 0)", "(1 + t, 0)", "(x, y, 0)"] {
        let field = FieldExpr::parse(formula).expect("formula");
        assert_eq!(
            uniform_flux::analyze(&field),
            UniformFluxReading::NotUniform,
            "`{formula}` is not a genuinely constant field"
        );
    }
}

#[test]
fn opening_the_activity_keeps_formula_mode_probes_refused_and_adds_nothing_durable() {
    let mut state = State::new(false);
    state.set_formula("(1, 0, 0)".to_owned());
    let mut activity = UniformFluxState::default();
    activity.open();
    let ctx = render_context();
    let _ = run_activity(&ctx, &mut state, &mut activity, 900.0, 1400.0, Vec::new());

    assert!(
        state.probe_mode_unsupported_reason().is_some(),
        "Formula-mode ordinary probes stay refused"
    );
    assert!(
        state.probe_surfaces().is_empty(),
        "the activity never fabricates a durable probe surface"
    );
    assert_eq!(state.formula, "(1, 0, 0)");
}

#[test]
fn the_presets_panel_promotes_the_uniform_prompt_into_a_working_activity() {
    let ctx = render_context();
    let dir = tempfile::tempdir().unwrap();
    let mut panel = PresetPanel::new_with_dir(dir.path().to_path_buf()).unwrap();
    let mut state = State::new(false);
    let mut camera = Camera::default();
    panel.set_query("Uniform field");
    assert!(panel.load_field("Uniform field", &mut state));

    let shown = run_panel(
        &ctx,
        &mut panel,
        &mut state,
        &mut camera,
        900.0,
        1600.0,
        Vec::new(),
    );
    assert!(has_text_containing(
        &shown,
        "Compare the flux entering and leaving a closed surface"
    ));
    assert!(has_text_containing(&shown, "Assumptions:"));
    assert!(has_text_containing(&shown, "Open flux investigation"));

    click_panel_label(
        &ctx,
        &mut panel,
        &mut state,
        &mut camera,
        "Open flux investigation",
        900.0,
        1600.0,
    );
    let open = run_panel(
        &ctx,
        &mut panel,
        &mut state,
        &mut camera,
        900.0,
        1600.0,
        Vec::new(),
    );
    assert!(has_text_containing(&open, "Flux through a closed box"));
    assert!(has_text_containing(&open, "+x"));
    assert!(has_text_containing(&open, "Total flux"));

    click_panel_label(
        &ctx,
        &mut panel,
        &mut state,
        &mut camera,
        "Close investigation",
        900.0,
        1600.0,
    );
    let closed = run_panel(
        &ctx,
        &mut panel,
        &mut state,
        &mut camera,
        900.0,
        1600.0,
        Vec::new(),
    );
    assert!(!has_text_containing(&closed, "Flux through a closed box"));
    assert!(has_text_containing(&closed, "Open flux investigation"));
    assert_eq!(state.formula, "(1, 0, 0)");
}

#[test]
fn the_panel_previews_switches_and_restores_the_two_component_extension() {
    let ctx = render_context();
    let dir = tempfile::tempdir().unwrap();
    let mut panel = PresetPanel::new_with_dir(dir.path().to_path_buf()).unwrap();
    let mut state = State::new(false);
    let mut camera = Camera::default();
    panel.set_query("formula.uniform-field");
    assert!(panel.load_field("Uniform field", &mut state));
    // The user edits the loaded formula to a genuinely constant 2D field and
    // views it in 2D.
    state.set_formula("(2, -1)".to_owned());
    state.set_mode(false);

    let shown = run_panel(
        &ctx,
        &mut panel,
        &mut state,
        &mut camera,
        900.0,
        1600.0,
        Vec::new(),
    );
    assert!(has_text_containing(&shown, "Open flux investigation"));
    click_panel_label(
        &ctx,
        &mut panel,
        &mut state,
        &mut camera,
        "Open flux investigation",
        900.0,
        1600.0,
    );

    let two_d = run_panel(
        &ctx,
        &mut panel,
        &mut state,
        &mut camera,
        900.0,
        1600.0,
        Vec::new(),
    );
    assert!(has_text_containing(&two_d, "F=(Fx,Fy,0)"));
    assert!(has_text_containing(&two_d, "(2, -1, 0)"));
    assert!(has_text_containing(&two_d, "Preview 3D extension"));
    assert_eq!(state.formula, "(2, -1)");

    click_panel_label(
        &ctx,
        &mut panel,
        &mut state,
        &mut camera,
        "Preview 3D extension",
        900.0,
        1600.0,
    );
    let preview = run_panel(
        &ctx,
        &mut panel,
        &mut state,
        &mut camera,
        900.0,
        1600.0,
        Vec::new(),
    );
    assert!(has_text_containing(&preview, "Switch to 3D extension"));
    assert_eq!(state.formula, "(2, -1)");

    click_panel_label(
        &ctx,
        &mut panel,
        &mut state,
        &mut camera,
        "Switch to 3D extension",
        900.0,
        1600.0,
    );
    let switched = run_panel(
        &ctx,
        &mut panel,
        &mut state,
        &mut camera,
        900.0,
        1600.0,
        Vec::new(),
    );
    assert_eq!(state.formula, "(2, -1, 0)");
    assert!(state.mode_3d);
    assert!(has_text_containing(&switched, "Total flux"));
    assert!(has_text_containing(&switched, "Restore original formula"));

    click_panel_label(
        &ctx,
        &mut panel,
        &mut state,
        &mut camera,
        "Restore original formula",
        900.0,
        1600.0,
    );
    let restored = run_panel(
        &ctx,
        &mut panel,
        &mut state,
        &mut camera,
        900.0,
        1600.0,
        Vec::new(),
    );
    assert_eq!(state.formula, "(2, -1)");
    assert!(!state.mode_3d);
    assert!(has_text_containing(&restored, "Preview 3D extension"));
}

#[test]
fn other_catalogue_prompts_stay_informational_without_a_dead_action() {
    let ctx = render_context();
    let dir = tempfile::tempdir().unwrap();
    let mut panel = PresetPanel::new_with_dir(dir.path().to_path_buf()).unwrap();
    let mut state = State::new(false);
    let mut camera = Camera::default();
    panel.set_query("Rotating field");
    assert!(panel.load_field("Rotating field", &mut state));

    let shown = run_panel(
        &ctx,
        &mut panel,
        &mut state,
        &mut camera,
        900.0,
        1600.0,
        Vec::new(),
    );
    assert!(
        has_text_containing(&shown, "Compare the circulation"),
        "an unpromoted prompt stays informational text beside the loaded scene"
    );
    assert!(
        !has_text_containing(&shown, "Open flux investigation"),
        "only a prompt with a working destination gets a clickable action"
    );
}

#[test]
fn a_narrow_compact_panel_keeps_the_prompt_and_activity_reachable() {
    let ctx = render_context();
    let dir = tempfile::tempdir().unwrap();
    let mut panel = PresetPanel::new_with_dir(dir.path().to_path_buf()).unwrap();
    let mut state = State::new(false);
    let mut camera = Camera::default();
    panel.set_query("formula.uniform-field");
    assert!(panel.load_field("Uniform field", &mut state));

    let shown = run_panel(
        &ctx,
        &mut panel,
        &mut state,
        &mut camera,
        320.0,
        2000.0,
        Vec::new(),
    );
    assert!(has_text_containing(
        &shown,
        "Compare the flux entering and leaving a closed surface"
    ));

    click_panel_label(
        &ctx,
        &mut panel,
        &mut state,
        &mut camera,
        "Open flux investigation",
        320.0,
        2000.0,
    );
    let open = run_panel(
        &ctx,
        &mut panel,
        &mut state,
        &mut camera,
        320.0,
        2000.0,
        Vec::new(),
    );
    assert!(has_text_containing(&open, "Flux through a closed box"));
    assert!(has_text_containing(&open, "Total flux"));
}
