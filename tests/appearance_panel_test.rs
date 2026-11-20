//! Issue #84 slice A2b: the Appearance panel.
//!
//! The panel edits the six layer overrides and the two physics-family
//! overrides through `State::commit_layer_styles`, resolving each field
//! independently. Layout integration matches the Presets panel: a persisted
//! expansion flag and width, one compact sheet, and a reset that restores the
//! collapsed default.

use eframe::egui;

use crate::support;
use crate::support::storage::{from_ron, ron_string};

use vector_field_explorer::color_ramp::Rgb;
use vector_field_explorer::layer_style::{LayerId, LayerStyles, PhysicsFamily};
use vector_field_explorer::ui::appearance_panel::{
    AppearanceEdit, AppearancePanelState, apply_edit, reset_all, resolve_family,
};
use vector_field_explorer::ui::layout::{
    DEFAULT_APPEARANCE_WIDTH, LayoutState, MAX_PANEL_WIDTH, SheetState,
};
use vector_field_explorer::viewmodel::State;
use vector_field_explorer::VfApp;

fn layer_slot(layer: LayerId) -> usize {
    LayerId::ALL
        .iter()
        .position(|candidate| *candidate == layer)
        .expect("every layer is listed in LayerId::ALL")
}

#[test]
fn appearance_panel_toggles_and_remembers_its_width() {
    let mut layout = LayoutState::default();
    assert!(
        !layout.appearance_expanded,
        "the Appearance panel starts collapsed to its rail"
    );
    assert_eq!(layout.appearance_width, DEFAULT_APPEARANCE_WIDTH);

    layout.toggle_appearance();
    assert!(layout.appearance_expanded);
    layout.toggle_appearance();
    assert!(!layout.appearance_expanded);

    layout.appearance_expanded = true;
    layout.appearance_width = 212.5;
    let restored: LayoutState = from_ron(&ron_string(&layout));
    assert_eq!(restored, layout, "the layout fields round trip");

    let clamped = LayoutState {
        appearance_width: 5000.0,
        ..Default::default()
    }
    .sanitized();
    assert_eq!(clamped.appearance_width, MAX_PANEL_WIDTH);

    let corrupt = LayoutState {
        appearance_width: f32::NAN,
        ..Default::default()
    }
    .sanitized();
    assert_eq!(corrupt.appearance_width, DEFAULT_APPEARANCE_WIDTH);
}

#[test]
fn appearance_panel_is_a_single_compact_sheet() {
    let mut sheet = SheetState::default();
    sheet.open(SheetState::Appearance);
    assert_eq!(sheet, SheetState::Appearance);
    assert!(sheet.shows(SheetState::Appearance));

    // Opening Presets replaces the Appearance sheet instead of stacking.
    sheet.open(SheetState::Presets);
    assert_eq!(sheet, SheetState::Presets);
    assert!(!sheet.shows(SheetState::Appearance));

    sheet.open(SheetState::Appearance);
    assert_eq!(
        sheet,
        SheetState::Appearance,
        "opening Appearance closes Presets"
    );
    sheet.toggle(SheetState::Appearance);
    assert_eq!(sheet, SheetState::None, "the active tab closes its sheet");
}

#[test]
fn layer_opacity_changes_commit_and_resolve() {
    let mut state = State::new(false);
    apply_edit(
        &mut state,
        AppearanceEdit::LayerOpacity(LayerId::Arrows, Some(0.3)),
    )
    .expect("a bounded slider value commits");

    assert_eq!(
        state.layer_styles().layers[layer_slot(LayerId::Arrows)].opacity,
        Some(0.3)
    );
    assert_eq!(
        state.layer_styles().resolve(LayerId::Arrows, None).opacity,
        0.3
    );
    assert_eq!(
        state.layer_styles().resolve(LayerId::Flow, None).opacity,
        1.0,
        "another layer keeps the global default"
    );
}

#[test]
fn family_opacity_and_colour_apply_to_all_three_layers() {
    let mut state = State::new(false);
    apply_edit(
        &mut state,
        AppearanceEdit::FamilyOpacity(PhysicsFamily::Electric, Some(0.4)),
    )
    .expect("family opacity commits");
    apply_edit(
        &mut state,
        AppearanceEdit::FamilyColor(PhysicsFamily::Electric, Some(Rgb([16, 32, 48]))),
    )
    .expect("family colour commits");

    for layer in [LayerId::Sources, LayerId::Induction, LayerId::TeachingScene] {
        let resolved = state
            .layer_styles()
            .resolve(layer, Some(PhysicsFamily::Electric));
        assert_eq!(resolved.opacity, 0.4, "{layer:?} uses the family opacity");
        assert_eq!(
            resolved.color,
            Some(Rgb([16, 32, 48])),
            "{layer:?} uses the family colour"
        );
    }
}

#[test]
fn layer_value_beats_family_value_per_attribute() {
    let mut state = State::new(false);
    apply_edit(
        &mut state,
        AppearanceEdit::FamilyOpacity(PhysicsFamily::Magnetic, Some(0.4)),
    )
    .expect("family opacity commits");
    apply_edit(
        &mut state,
        AppearanceEdit::FamilyColor(PhysicsFamily::Magnetic, Some(Rgb([200, 10, 10]))),
    )
    .expect("family colour commits");
    apply_edit(
        &mut state,
        AppearanceEdit::LayerOpacity(LayerId::Sources, Some(0.9)),
    )
    .expect("layer opacity commits");
    apply_edit(
        &mut state,
        AppearanceEdit::LayerWidth(LayerId::Sources, Some(2.0)),
    )
    .expect("layer width commits");

    let resolved = state
        .layer_styles()
        .resolve(LayerId::Sources, Some(PhysicsFamily::Magnetic));
    assert_eq!(resolved.opacity, 0.9, "the layer opacity wins");
    assert_eq!(resolved.width, 2.0, "the layer width wins");
    assert_eq!(
        resolved.color,
        Some(Rgb([200, 10, 10])),
        "an absent layer colour falls back to the family colour"
    );
}

#[test]
fn family_row_ignores_layer_overrides_but_layer_precedence_is_unchanged() {
    let mut state = State::new(false);
    apply_edit(
        &mut state,
        AppearanceEdit::LayerOpacity(LayerId::Sources, Some(0.9)),
    )
    .expect("layer opacity commits");
    apply_edit(
        &mut state,
        AppearanceEdit::LayerWidth(LayerId::Sources, Some(2.0)),
    )
    .expect("layer width commits");

    // The family row's fallback and slider seed come from global + family only.
    let fallback = resolve_family(state.layer_styles(), PhysicsFamily::Electric);
    assert_eq!(
        (fallback.opacity, fallback.width),
        (1.0, 1.0),
        "Sources layer overrides must not leak into the family fallback"
    );

    let mut panel = AppearancePanelState::default();
    let output = render_panel(&mut panel, &mut state);
    let text = painted_texts(&output).join("\n");
    assert!(
        !text.contains("Default (0.90)"),
        "the Electric family row must not display the Sources layer opacity:\n{text}"
    );
    assert!(
        !text.contains("Default (2.00)"),
        "the Electric family row must not display the Sources layer width:\n{text}"
    );

    // Committing the family value applies where the layer leaves a field
    // default, while the merged resolver keeps the layer ahead for Sources.
    apply_edit(
        &mut state,
        AppearanceEdit::FamilyOpacity(PhysicsFamily::Electric, Some(0.4)),
    )
    .expect("family opacity commits");
    apply_edit(
        &mut state,
        AppearanceEdit::FamilyWidth(PhysicsFamily::Electric, Some(0.5)),
    )
    .expect("family width commits");
    let family = resolve_family(state.layer_styles(), PhysicsFamily::Electric);
    assert_eq!((family.opacity, family.width), (0.4, 0.5));

    let sources = state
        .layer_styles()
        .resolve(LayerId::Sources, Some(PhysicsFamily::Electric));
    assert_eq!(
        (sources.opacity, sources.width),
        (0.9, 2.0),
        "layer precedence over the family stays unchanged"
    );
    let induction = state
        .layer_styles()
        .resolve(LayerId::Induction, Some(PhysicsFamily::Electric));
    assert_eq!(
        (induction.opacity, induction.width),
        (0.4, 0.5),
        "the family value reaches the other family-aware layers"
    );
}

#[test]
fn panel_shows_inline_error_and_clears_it_on_a_valid_edit() {
    let mut state = State::new(false);
    let mut panel = AppearancePanelState::default();

    panel.apply(
        &mut state,
        AppearanceEdit::LayerOpacity(LayerId::Arrows, Some(f32::NAN)),
    );
    let output = render_panel(&mut panel, &mut state);
    let text = painted_texts(&output).join("\n");
    assert!(
        text.contains("opacity") && text.contains("finite"),
        "the rejected edit's message is painted inline:\n{text}"
    );

    panel.apply(
        &mut state,
        AppearanceEdit::LayerOpacity(LayerId::Arrows, Some(0.5)),
    );
    let output = render_panel(&mut panel, &mut state);
    let text = painted_texts(&output).join("\n");
    assert!(
        !text.contains("must be finite"),
        "a subsequent valid edit clears the inline error:\n{text}"
    );
    assert_eq!(
        state.layer_styles().resolve(LayerId::Arrows, None).opacity,
        0.5
    );
}

#[test]
fn clearing_a_field_restores_the_fallback() {
    let mut state = State::new(false);
    apply_edit(
        &mut state,
        AppearanceEdit::LayerWidth(LayerId::Reference, Some(0.25)),
    )
    .expect("an override commits");
    assert_eq!(
        state
            .layer_styles()
            .resolve(LayerId::Reference, None)
            .width,
        0.25
    );

    apply_edit(
        &mut state,
        AppearanceEdit::LayerWidth(LayerId::Reference, None),
    )
    .expect("clearing the field commits");
    let resolved = state.layer_styles().resolve(LayerId::Reference, None);
    assert_eq!(
        resolved.width, 1.0,
        "clearing an attribute restores the neutral fallback"
    );
}

#[test]
fn invalid_direct_write_is_rejected_with_a_message() {
    let mut state = State::new(false);
    let before = *state.layer_styles();
    let dirty_before = state.dirty;

    let error = apply_edit(
        &mut state,
        AppearanceEdit::LayerOpacity(LayerId::Arrows, Some(f32::NAN)),
    )
    .expect_err("a non-finite value must not commit");
    assert!(
        error.contains("opacity"),
        "the rejection names the field: {error}"
    );
    assert_eq!(
        *state.layer_styles(),
        before,
        "a rejected edit leaves the committed record untouched"
    );
    assert_eq!(
        state.dirty, dirty_before,
        "a rejected edit does not dirty the scene"
    );
}

#[test]
fn reset_all_restores_neutral_styles() {
    let mut state = State::new(false);
    apply_edit(
        &mut state,
        AppearanceEdit::LayerOpacity(LayerId::Flow, Some(0.2)),
    )
    .expect("override commits");
    apply_edit(
        &mut state,
        AppearanceEdit::FamilyOpacity(PhysicsFamily::Electric, Some(0.5)),
    )
    .expect("family override commits");
    assert!(!state.layer_styles().is_neutral());

    reset_all(&mut state).expect("reset commits");
    assert_eq!(*state.layer_styles(), LayerStyles::default());
    assert!(state.layer_styles().is_neutral());
}

fn context() -> egui::Context {
    let ctx = support::context();
    ctx.enable_accesskit();
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    ctx
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

fn accesskit_labels(output: &egui::FullOutput) -> Vec<String> {
    let Some(update) = output.platform_output.accesskit_update.as_ref() else {
        return Vec::new();
    };
    update
        .nodes
        .iter()
        .filter_map(|(_, node)| node.label().map(str::to_owned))
        .collect()
}

fn render_panel(
    panel: &mut AppearancePanelState,
    state: &mut State,
) -> egui::FullOutput {
    let ctx = context();
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(420.0, 3200.0),
            )),
            ..Default::default()
        },
        |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                panel.show(ui, state, false);
            });
        },
    );
    output.textures_delta.clear();
    output
}

#[test]
fn appearance_panel_renders_all_six_layers_and_two_families() {
    let mut state = State::new(false);
    let mut panel = AppearancePanelState::default();
    let output = render_panel(&mut panel, &mut state);

    let text = painted_texts(&output)
        .into_iter()
        .chain(accesskit_labels(&output))
        .collect::<Vec<_>>()
        .join("\n");
    for layer in LayerId::ALL {
        assert!(
            text.contains(layer.label()),
            "the panel renders the {} row:\n{text}",
            layer.label()
        );
    }
    for family in PhysicsFamily::ALL {
        assert!(
            text.contains(family.label()),
            "the panel renders the {} row:\n{text}",
            family.label()
        );
    }
    assert!(text.contains("Appearance"));
    assert!(
        text.contains("Reset all"),
        "the panel exposes the global reset"
    );
    assert!(
        text.contains("SVG"),
        "the header notes that SVG keeps the unstyled blueprint"
    );
}

#[test]
fn compact_appearance_tab_opens_and_switches_the_single_sheet() {
    let ctx = context();
    let mut app = VfApp::new();

    for _ in 0..3 {
        run_app_frame(&ctx, &mut app, Vec::new());
    }

    let presets = button_center(&run_app_frame(&ctx, &mut app, Vec::new()), "Presets")
        .expect("Presets tab");
    run_app_frame(&ctx, &mut app, pointer(presets, true));
    run_app_frame(&ctx, &mut app, pointer(presets, false));
    assert_eq!(app.sheet, SheetState::Presets);

    let appearance = button_center(&run_app_frame(&ctx, &mut app, Vec::new()), "Appearance")
        .expect("Appearance tab");
    run_app_frame(&ctx, &mut app, pointer(appearance, true));
    run_app_frame(&ctx, &mut app, pointer(appearance, false));
    assert_eq!(
        app.sheet,
        SheetState::Appearance,
        "the Appearance tab opens its compact sheet"
    );
    assert!(
        !app.sheet.shows(SheetState::Presets),
        "opening Appearance closes Presets"
    );

    let presets = button_center(&run_app_frame(&ctx, &mut app, Vec::new()), "Presets")
        .expect("Presets tab");
    run_app_frame(&ctx, &mut app, pointer(presets, true));
    run_app_frame(&ctx, &mut app, pointer(presets, false));
    assert_eq!(
        app.sheet,
        SheetState::Presets,
        "switching back closes the Appearance sheet"
    );
}

fn run_app_frame(
    ctx: &egui::Context,
    app: &mut VfApp,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let mut native = eframe::Frame::_new_kittest();
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(600.0, 800.0),
            )),
            events,
            ..Default::default()
        },
        |ui| eframe::App::ui(app, ui, &mut native),
    );
    output.textures_delta.clear();
    output
}

fn button_center(output: &egui::FullOutput, label: &str) -> Option<egui::Pos2> {
    let update = output.platform_output.accesskit_update.as_ref()?;
    update.nodes.iter().find_map(|(_, node)| {
        if node.role() != egui::Role::Button || node.label() != Some(label) {
            return None;
        }
        let bounds = node.bounds()?;
        Some(egui::pos2(
            ((bounds.x0 + bounds.x1) / 2.0) as f32,
            ((bounds.y0 + bounds.y1) / 2.0) as f32,
        ))
    })
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

#[test]
fn reset_layout_restores_the_appearance_panel_default() {
    let mut layout = LayoutState {
        appearance_expanded: true,
        appearance_width: 400.0,
        ..Default::default()
    };
    layout.reset();
    assert_eq!(layout, LayoutState::default());
    assert!(!layout.appearance_expanded);
    assert_eq!(layout.appearance_width, DEFAULT_APPEARANCE_WIDTH);
}
