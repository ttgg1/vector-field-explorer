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

use vector_field_explorer::VfApp;
use vector_field_explorer::appearance_profile::{
    AppearanceProfile, PROFILE_FORMAT_VERSION, ProfileDocument, builtins,
};
use vector_field_explorer::color_ramp::Rgb;
use vector_field_explorer::colormap::Colormap;
use vector_field_explorer::layer_style::{LayerId, LayerStyles, PhysicsFamily};
use vector_field_explorer::profile_library::ProfileLibrary;
use vector_field_explorer::ui::appearance_panel::{
    AppearanceEdit, AppearancePanelState, apply_edit, reset_all, resolve_family,
};
use vector_field_explorer::ui::layout::{
    DEFAULT_APPEARANCE_WIDTH, LayoutState, MAX_PANEL_WIDTH, SheetState,
};
use vector_field_explorer::viewmodel::State;

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
fn a_failed_edit_after_a_successful_save_replaces_the_status_message() {
    let mut h = ProfileHarness::new();
    h.panel.profile_name = "Saved".to_owned();
    let saved = h.click_label("Save current appearance");
    assert!(
        painted(&saved).contains("Saved profile Saved"),
        "the save reports success:\n{}",
        painted(&saved)
    );

    h.panel.apply(
        &mut h.state,
        AppearanceEdit::LayerOpacity(LayerId::Arrows, Some(f32::NAN)),
    );
    let out = h.idle();
    let text = painted(&out);
    assert!(
        text.contains("finite"),
        "the failed edit paints its error:\n{text}"
    );
    assert!(
        !text.contains("Saved profile Saved"),
        "the stale success message does not survive a later failed edit:\n{text}"
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
        state.layer_styles().resolve(LayerId::Reference, None).width,
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

fn render_panel(panel: &mut AppearancePanelState, state: &mut State) -> egui::FullOutput {
    let mut profiles = ProfileLibrary::default();
    render_panel_with(panel, state, &mut profiles, 420.0, false)
}

fn render_panel_with(
    panel: &mut AppearancePanelState,
    state: &mut State,
    profiles: &mut ProfileLibrary,
    width: f32,
    compact: bool,
) -> egui::FullOutput {
    let ctx = context();
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, 3200.0),
            )),
            ..Default::default()
        },
        |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                panel.show(ui, state, profiles, compact);
            });
        },
    );
    output.textures_delta.clear();
    output
}

/// Drives the real panel widgets for the profile section: buttons are found
/// through accesskit and clicked with real pointer events, while the drafts are
/// set the same way a user's text edit would leave them.
struct ProfileHarness {
    ctx: egui::Context,
    panel: AppearancePanelState,
    state: State,
    profiles: ProfileLibrary,
    width: f32,
    compact: bool,
    time: f64,
}

impl ProfileHarness {
    fn new() -> Self {
        Self {
            ctx: context(),
            panel: AppearancePanelState::default(),
            state: State::new(false),
            profiles: ProfileLibrary::default(),
            width: 420.0,
            compact: false,
            time: 0.0,
        }
    }

    fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(self.width, 3200.0),
            )),
            time: Some(self.time),
            predicted_dt: 1.0 / 60.0,
            events,
            ..Default::default()
        };
        self.time += 1.0 / 60.0;
        let mut output = {
            let panel = &mut self.panel;
            let state = &mut self.state;
            let profiles = &mut self.profiles;
            let compact = self.compact;
            self.ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    panel.show(ui, state, profiles, compact);
                });
            })
        };
        output.textures_delta.clear();
        output
    }

    fn idle(&mut self) -> egui::FullOutput {
        self.frame(Vec::new())
    }

    fn click(&mut self, pos: egui::Pos2) {
        self.frame(pointer(pos, true));
        self.frame(pointer(pos, false));
    }

    fn click_label(&mut self, label: &str) -> egui::FullOutput {
        let out = self.idle();
        let pos =
            button_center(&out, label).unwrap_or_else(|| panic!("the {label} button is painted"));
        self.click(pos);
        self.idle()
    }

    /// Open the profile dropdown and click `name` in its list. The selection is
    /// only a draft; the caller decides whether to click Apply.
    fn select_profile(&mut self, name: &str) -> egui::FullOutput {
        self.open_profile_dropdown();
        // The popup scrolls: AccessKit reports every option's bounds, including
        // ones clipped out of the visible menu. Scroll until the option's paint
        // is inside its clip rect, then click it.
        for _ in 0..40 {
            let out = self.idle();
            let rect = label_rect(&out, name)
                .unwrap_or_else(|| panic!("{name} is listed in the profile dropdown"));
            let clip = text_clip(&out, name, rect);
            if let Some(clip) = clip
                && clip.contains_rect(rect)
            {
                self.click(rect.center());
                return self.idle();
            }
            let pos = clip
                .map(|clip| rect.intersect(clip).center())
                .or_else(|| dropdown_scroll_pos(&out))
                .expect("the open dropdown has a scrollable viewport");
            self.frame(vec![
                egui::Event::PointerMoved(pos),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -300.0),
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Wheel,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
        }
        panic!("{name} never scrolled into the profile dropdown viewport");
    }

    fn open_profile_dropdown(&mut self) -> egui::FullOutput {
        let out = self.idle();
        let pos = combo_center(&out).expect("the profile dropdown is painted");
        self.click(pos);
        self.idle()
    }
}

fn node_rect(node: &egui::accesskit::Node) -> Option<egui::Rect> {
    let bounds = node.bounds()?;
    Some(egui::Rect::from_min_max(
        egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
        egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
    ))
}

fn label_rect(output: &egui::FullOutput, label: &str) -> Option<egui::Rect> {
    let update = output.platform_output.accesskit_update.as_ref()?;
    update.nodes.iter().find_map(|(_, node)| {
        (node.label() == Some(label))
            .then(|| node_rect(node))
            .flatten()
    })
}

fn text_clip(output: &egui::FullOutput, label: &str, rect: egui::Rect) -> Option<egui::Rect> {
    output.shapes.iter().find_map(|shape| match &shape.shape {
        egui::Shape::Text(text)
            if text.galley.text() == label && rect.intersects(text.visual_bounding_rect()) =>
        {
            Some(shape.clip_rect)
        }
        _ => None,
    })
}

fn dropdown_scroll_pos(output: &egui::FullOutput) -> Option<egui::Pos2> {
    for profile in builtins() {
        let Some(rect) = label_rect(output, &profile.name) else {
            continue;
        };
        if let Some(clip) = text_clip(output, &profile.name, rect) {
            return Some(rect.intersect(clip).center());
        }
    }
    combo_center(output).map(|center| center + egui::vec2(0.0, 30.0))
}

fn combo_center(output: &egui::FullOutput) -> Option<egui::Pos2> {
    let update = output.platform_output.accesskit_update.as_ref()?;
    update.nodes.iter().find_map(|(_, node)| {
        (node.role() == egui::Role::ComboBox)
            .then(|| node_rect(node))
            .flatten()
            .map(|rect| rect.center())
    })
}

fn button_disabled(output: &egui::FullOutput, label: &str) -> Option<bool> {
    let update = output.platform_output.accesskit_update.as_ref()?;
    update.nodes.iter().find_map(|(_, node)| {
        (node.role() == egui::Role::Button && node.label() == Some(label))
            .then(|| node.is_disabled())
    })
}

fn painted(output: &egui::FullOutput) -> String {
    painted_texts(output)
        .into_iter()
        .chain(accesskit_labels(output))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn applying_a_user_profile_updates_the_appearance_state() {
    let mut h = ProfileHarness::new();
    apply_edit(
        &mut h.state,
        AppearanceEdit::LayerOpacity(LayerId::Arrows, Some(0.35)),
    )
    .expect("the layer override commits");
    h.state.colormap = Colormap::Turbo;
    let saved = AppearanceProfile::capture("Mine", &h.state);
    h.profiles = ProfileLibrary::new(vec![saved.clone()]).expect("the captured profile is valid");

    // Move every appearance field away from the captured profile.
    apply_edit(
        &mut h.state,
        AppearanceEdit::LayerOpacity(LayerId::Arrows, None),
    )
    .expect("clearing the layer override commits");
    h.state.colormap = Colormap::Mono;

    h.select_profile("Mine");
    let out = h.click_label("Apply");

    assert_eq!(h.state.viewport_style, saved.viewport);
    assert_eq!(*h.state.layer_styles(), saved.layer_styles);
    assert_eq!(h.state.colormap, saved.color.colormap);
    assert_eq!(h.state.color_ramp, saved.color.ramp);
    assert_eq!(h.state.color_mode, saved.color.mode);
    assert_eq!(h.state.color_quantity, saved.color.quantity);
    assert!(
        painted(&out).contains("Applied Mine"),
        "applying reports the profile name:\n{}",
        painted(&out)
    );
}

#[test]
fn profile_dropdown_lists_builtins_then_user_profiles() {
    let mut h = ProfileHarness::new();
    h.profiles = ProfileLibrary::new(vec![
        AppearanceProfile::capture("User one", &h.state),
        AppearanceProfile::capture("User two", &h.state),
    ])
    .expect("the profiles are valid");

    let out = h.open_profile_dropdown();
    let expected: Vec<String> = builtins()
        .iter()
        .map(|profile| profile.name.clone())
        .chain(["User one".to_owned(), "User two".to_owned()])
        .collect();

    let mut listed: Vec<(f32, String)> = expected
        .iter()
        .filter_map(|name| label_rect(&out, name).map(|rect| (rect.center().y, name.clone())))
        .collect();
    assert_eq!(
        listed.len(),
        expected.len(),
        "every built-in and user profile is listed"
    );
    listed.sort_by(|a, b| a.0.total_cmp(&b.0));
    let order: Vec<String> = listed.into_iter().map(|(_, name)| name).collect();
    assert_eq!(
        order, expected,
        "the dropdown lists the built-ins first, then user profiles"
    );
}

#[test]
fn apply_button_applies_only_the_selected_profile() {
    let mut h = ProfileHarness::new();
    let mut selected_source = State::new(false);
    apply_edit(
        &mut selected_source,
        AppearanceEdit::LayerOpacity(LayerId::Flow, Some(0.2)),
    )
    .expect("the layer override commits");
    selected_source.colormap = Colormap::Turbo;
    let selected = AppearanceProfile::capture("Selected", &selected_source);
    let mut other_source = State::new(false);
    other_source.colormap = Colormap::Mono;
    let other = AppearanceProfile::capture("Other", &other_source);
    h.profiles =
        ProfileLibrary::new(vec![selected.clone(), other]).expect("the profiles are valid");

    h.select_profile("Selected");
    let out = h.click_label("Apply");

    assert_eq!(h.state.colormap, selected.color.colormap);
    assert_eq!(h.state.color_ramp, selected.color.ramp);
    assert_eq!(*h.state.layer_styles(), selected.layer_styles);
    assert!(
        painted(&out).contains("Applied Selected"),
        "the applied profile is reported:\n{}",
        painted(&out)
    );
}

#[test]
fn delete_is_unavailable_for_builtins_and_deletes_a_user_profile_after_two_clicks() {
    let mut h = ProfileHarness::new();
    h.profiles = ProfileLibrary::new(vec![AppearanceProfile::capture("Mine", &h.state)])
        .expect("the profile is valid");
    h.select_profile(&builtins()[0].name);

    let out = h.idle();
    assert_eq!(
        button_disabled(&out, "Delete"),
        Some(true),
        "a built-in selection cannot be deleted"
    );
    h.click_label("Delete");
    assert!(
        h.panel.pending_delete.is_none(),
        "clicking the disabled Delete button arms nothing"
    );
    assert_eq!(h.profiles.len(), 1);

    h.select_profile("Mine");
    let out = h.idle();
    assert_eq!(
        button_disabled(&out, "Delete"),
        Some(false),
        "a user selection enables Delete"
    );
    let out = h.click_label("Delete");
    assert_eq!(
        h.panel.pending_delete.as_deref(),
        Some("Mine"),
        "the first click only arms the delete"
    );
    assert_eq!(h.profiles.len(), 1, "the profile survives the first click");
    assert!(
        painted(&out).contains("again") || painted(&out).contains("confirm"),
        "the panel asks for the second click:\n{}",
        painted(&out)
    );

    h.click_label("Confirm delete");
    assert!(h.panel.pending_delete.is_none());
    assert!(
        h.profiles.is_empty(),
        "the second click removes the profile"
    );
    assert!(
        h.panel.selected_profile.is_none(),
        "a deleted selection falls back to no selection"
    );
}

#[test]
fn selection_never_applies_on_its_own_and_survives_frames() {
    let mut h = ProfileHarness::new();
    let mut source = State::new(false);
    source.colormap = Colormap::Turbo;
    let profile = AppearanceProfile::capture("Selected", &source);
    h.profiles = ProfileLibrary::new(vec![profile]).expect("the profile is valid");
    let before = AppearanceProfile::capture("Before", &h.state);

    h.select_profile("Selected");
    for _ in 0..5 {
        h.idle();
    }

    let after = AppearanceProfile::capture("After", &h.state);
    assert_eq!(after.viewport, before.viewport);
    assert_eq!(after.layer_styles, before.layer_styles);
    assert_eq!(
        after.color, before.color,
        "selecting never applies a profile"
    );
    assert_eq!(
        h.panel.selected_profile.as_deref(),
        Some("Selected"),
        "the session-only selection survives frames"
    );
    let out = h.idle();
    assert!(
        painted(&out).contains("Selected"),
        "the dropdown shows the selected name:\n{}",
        painted(&out)
    );
}

#[test]
fn applying_a_profile_reports_a_rejected_record_without_mutating_state() {
    let mut state = State::new(false);
    let before = AppearanceProfile::capture("Probe", &state);
    let mut invalid = AppearanceProfile::capture("Invalid", &state);
    invalid.name = "   ".to_owned();

    let mut panel = AppearancePanelState::default();
    panel.apply_profile(&mut state, &invalid);

    let after = AppearanceProfile::capture("Probe", &state);
    assert_eq!(
        after, before,
        "a rejected profile never mutates the committed appearance"
    );
    assert!(panel.error.is_some(), "the rejection is recorded inline");
    assert!(panel.message.is_none());

    let output = render_panel(&mut panel, &mut state);
    let text = painted(&output);
    assert!(
        text.contains("name"),
        "the rejected record's message is painted:\n{text}"
    );
}

#[test]
fn saving_the_current_appearance_adds_a_user_profile() {
    let mut h = ProfileHarness::new();
    apply_edit(
        &mut h.state,
        AppearanceEdit::LayerOpacity(LayerId::Flow, Some(0.45)),
    )
    .expect("the layer override commits");
    h.panel.profile_name = "  My look  ".to_owned();

    let out = h.click_label("Save current appearance");

    assert_eq!(h.profiles.len(), 1);
    assert_eq!(h.profiles.user()[0].name, "My look", "the draft is trimmed");
    assert_eq!(
        h.profiles.user()[0].layer_styles,
        *h.state.layer_styles(),
        "the saved profile captures the current appearance"
    );
    assert!(
        h.profiles.export_text().contains("My look"),
        "the saved profile appears in the export text"
    );
    assert!(
        painted(&out).contains("Saved"),
        "the save is reported:\n{}",
        painted(&out)
    );
}

#[test]
fn saving_with_a_builtin_name_is_rejected() {
    let mut h = ProfileHarness::new();
    h.panel.profile_name = builtins()[0].name.clone();

    let out = h.click_label("Save current appearance");

    assert!(
        h.profiles.is_empty(),
        "a built-in name never becomes a user profile"
    );
    let text = painted(&out);
    assert!(
        text.contains("built in") || text.contains("built-in"),
        "the rejection explains the reserved name:\n{text}"
    );
}

#[test]
fn saving_with_an_empty_name_is_rejected() {
    let mut h = ProfileHarness::new();
    h.panel.profile_name = "   ".to_owned();

    let out = h.click_label("Save current appearance");

    assert!(h.profiles.is_empty(), "an empty name never saves");
    assert!(
        painted(&out).contains("name"),
        "the rejection is shown:\n{}",
        painted(&out)
    );
}

#[test]
fn saving_twice_with_the_same_name_replaces_in_place() {
    let mut h = ProfileHarness::new();
    h.panel.profile_name = "First".to_owned();
    h.click_label("Save current appearance");
    h.panel.profile_name = "Second".to_owned();
    h.click_label("Save current appearance");

    apply_edit(
        &mut h.state,
        AppearanceEdit::LayerOpacity(LayerId::Arrows, Some(0.7)),
    )
    .expect("the layer override commits");
    h.panel.profile_name = "First".to_owned();
    let out = h.click_label("Save current appearance");

    let names: Vec<&str> = h
        .profiles
        .user()
        .iter()
        .map(|profile| profile.name.as_str())
        .collect();
    assert_eq!(names, ["First", "Second"], "order and length are preserved");
    assert_eq!(
        h.profiles.user()[0].layer_styles.layers[layer_slot(LayerId::Arrows)].opacity,
        Some(0.7),
        "the replacement holds the newly captured appearance"
    );
    assert!(
        painted(&out).contains("Replaced"),
        "the replacement is reported:\n{}",
        painted(&out)
    );

    let document =
        ProfileDocument::from_json(&h.profiles.export_text()).expect("the export parses");
    assert_eq!(document.profiles.len(), 2);
    assert_eq!(
        document
            .profiles
            .iter()
            .filter(|profile| profile.name == "First")
            .count(),
        1,
        "the export has one entry for the replaced name"
    );
}

#[test]
fn saving_at_the_user_profile_cap_reports_the_cap_and_keeps_the_library() {
    let mut h = ProfileHarness::new();
    let seeded: Vec<AppearanceProfile> = (0..32)
        .map(|index| AppearanceProfile::capture(format!("Profile {index}"), &h.state))
        .collect();
    h.profiles = ProfileLibrary::new(seeded).expect("32 profiles fit the cap");
    h.panel.profile_name = "One more".to_owned();

    let out = h.click_label("Save current appearance");

    assert_eq!(h.profiles.len(), 32, "the over-cap save adds nothing");
    let text = painted(&out);
    assert!(
        text.contains("at most 32 user profiles"),
        "the library's cap wording is painted verbatim:\n{text}"
    );
    assert!(
        !text.contains("Saved profile One more"),
        "an over-cap save is not reported as a success:\n{text}"
    );
}

#[test]
fn deleting_a_user_profile_needs_two_clicks() {
    let mut h = ProfileHarness::new();
    h.profiles = ProfileLibrary::new(vec![AppearanceProfile::capture("Mine", &h.state)])
        .expect("the profile is valid");
    h.select_profile("Mine");

    let out = h.click_label("Delete");
    assert_eq!(
        h.panel.pending_delete.as_deref(),
        Some("Mine"),
        "the first click only arms the delete"
    );
    assert_eq!(h.profiles.len(), 1, "the profile survives the first click");
    assert!(
        painted(&out).contains("again") || painted(&out).contains("confirm"),
        "the panel asks for the second click:\n{}",
        painted(&out)
    );

    h.click_label("Confirm delete");
    assert!(h.panel.pending_delete.is_none());
    assert!(
        h.profiles.is_empty(),
        "the second click removes the profile"
    );
}

#[test]
fn saving_over_an_armed_delete_clears_the_pending_delete() {
    let mut h = ProfileHarness::new();
    h.profiles = ProfileLibrary::new(vec![AppearanceProfile::capture("X", &h.state)])
        .expect("the profile is valid");
    h.select_profile("X");
    h.click_label("Delete");
    assert_eq!(
        h.panel.pending_delete.as_deref(),
        Some("X"),
        "the first delete click arms the delete"
    );

    h.panel.profile_name = "X".to_owned();
    let out = h.click_label("Save current appearance");

    assert!(
        h.panel.pending_delete.is_none(),
        "a successful save clears the armed delete"
    );
    assert_eq!(h.profiles.len(), 1, "the save replaces rather than adds");
    assert!(
        painted(&out).contains("Replaced profile X"),
        "the replacement is reported:\n{}",
        painted(&out)
    );
}

#[test]
fn import_replaces_the_user_library_atomically() {
    let mut h = ProfileHarness::new();
    let gamma = AppearanceProfile::capture("Gamma", &h.state);
    let delta = AppearanceProfile::capture("Delta", &h.state);
    h.profiles = ProfileLibrary::new(vec![
        AppearanceProfile::capture("Alpha", &h.state),
        gamma.clone(),
        delta.clone(),
    ])
    .expect("the profiles are valid");
    let mut updated = AppearanceProfile::capture("Alpha", &h.state);
    updated.color.colormap = Colormap::Turbo;
    let added = AppearanceProfile::capture("Beta", &h.state);
    let document = ProfileDocument {
        version: PROFILE_FORMAT_VERSION,
        profiles: vec![updated.clone(), added.clone()],
    };
    h.panel.import_text = document.to_json().expect("the document serializes");

    let out = h.click_label("Apply text");

    assert_eq!(
        h.profiles.user(),
        [updated, gamma, delta, added],
        "the import merges in document order"
    );
    let text = painted(&out);
    assert!(
        text.contains("Imported 1 new user profile"),
        "the delta over the starting library is reported, not the post-merge total:\n{text}"
    );
    assert!(
        !text.contains("Imported 4"),
        "the post-merge total is not painted as the imported count:\n{text}"
    );
}

#[test]
fn import_clears_the_armed_delete() {
    let mut h = ProfileHarness::new();
    h.profiles = ProfileLibrary::new(vec![AppearanceProfile::capture("Mine", &h.state)])
        .expect("the profile is valid");
    h.select_profile("Mine");
    h.click_label("Delete");
    assert_eq!(
        h.panel.pending_delete.as_deref(),
        Some("Mine"),
        "the first delete click arms the delete"
    );

    let document = ProfileDocument {
        version: PROFILE_FORMAT_VERSION,
        profiles: vec![AppearanceProfile::capture("Imported", &h.state)],
    };
    h.panel.import_text = document.to_json().expect("the document serializes");
    h.click_label("Apply text");

    assert!(
        h.panel.pending_delete.is_none(),
        "a successful import clears the armed delete"
    );
    assert_eq!(h.profiles.len(), 2, "the import merged its profile");
}

#[test]
fn import_that_only_replaces_existing_names_reports_no_new_profiles() {
    let mut h = ProfileHarness::new();
    h.profiles = ProfileLibrary::new(vec![
        AppearanceProfile::capture("Alpha", &h.state),
        AppearanceProfile::capture("Beta", &h.state),
    ])
    .expect("the profiles are valid");
    let mut updated = AppearanceProfile::capture("Alpha", &h.state);
    updated.color.colormap = Colormap::Turbo;
    let document = ProfileDocument {
        version: PROFILE_FORMAT_VERSION,
        profiles: vec![updated.clone()],
    };
    h.panel.import_text = document.to_json().expect("the document serializes");

    let out = h.click_label("Apply text");

    assert_eq!(
        h.profiles.user()[0],
        updated,
        "the existing name is replaced"
    );
    assert_eq!(h.profiles.len(), 2, "the replacement grows nothing");
    let text = painted(&out);
    assert!(
        text.contains("no new profiles"),
        "a zero delta says nothing new was imported:\n{text}"
    );
    assert!(
        !text.contains("Imported 1") && !text.contains("Imported 2"),
        "a replacement is not painted as an import:\n{text}"
    );
}

#[test]
fn empty_import_document_reports_nothing_to_import_without_replacement_wording() {
    let empty = ProfileDocument {
        version: PROFILE_FORMAT_VERSION,
        profiles: Vec::new(),
    }
    .to_json()
    .expect("the document serializes");

    let mut fresh = ProfileHarness::new();
    fresh.panel.import_text = empty.clone();
    let out = fresh.click_label("Apply text");
    let text = painted(&out);
    assert!(
        text.contains("Nothing to import"),
        "an empty document says there is nothing to import:\n{text}"
    );
    assert!(
        !text.contains("replaced") && !text.contains("no new profiles"),
        "an empty document is not painted as a replacement:\n{text}"
    );
    assert!(fresh.profiles.is_empty(), "the empty import adds nothing");

    let mut populated = ProfileHarness::new();
    let mine = AppearanceProfile::capture("Mine", &fresh.state);
    populated.profiles = ProfileLibrary::new(vec![mine.clone()]).expect("the profile is valid");
    let before = populated.profiles.user().to_vec();
    populated.panel.import_text = empty;
    let out = populated.click_label("Apply text");
    let text = painted(&out);
    assert!(
        text.contains("Nothing to import"),
        "an empty document against a non-empty library says there is nothing to import:\n{text}"
    );
    assert!(
        !text.contains("replaced") && !text.contains("no new profiles"),
        "the empty import is not painted as a replacement:\n{text}"
    );
    assert_eq!(
        populated.profiles.user(),
        before.as_slice(),
        "the non-empty library is unchanged"
    );
}

#[test]
fn importing_malformed_text_reports_an_error_and_keeps_the_library() {
    let mut h = ProfileHarness::new();
    let original = AppearanceProfile::capture("Mine", &h.state);
    h.profiles = ProfileLibrary::new(vec![original.clone()]).expect("the profile is valid");
    h.panel.import_text = "not json".to_owned();

    let out = h.click_label("Apply text");

    assert_eq!(
        h.profiles.user(),
        [original],
        "a malformed import leaves the library untouched"
    );
    let text = painted(&out).to_lowercase();
    assert!(
        text.contains("json"),
        "the malformed document is named:\n{text}"
    );
}

#[test]
fn export_text_is_empty_without_user_profiles_and_copy_is_disabled() {
    let mut h = ProfileHarness::new();

    let before = h.idle();
    assert_eq!(
        button_disabled(&before, "Copy"),
        Some(true),
        "an empty buffer cannot be copied"
    );

    let out = h.click_label("Export text");
    assert_eq!(
        h.panel.export_text, "",
        "no user profile means no export text"
    );
    assert_eq!(
        button_disabled(&out, "Copy"),
        Some(true),
        "Copy stays disabled for the empty export"
    );
}

#[test]
fn compact_width_keeps_the_profile_section_usable() {
    let mut h = ProfileHarness::new();
    h.width = 320.0;
    h.compact = true;
    h.profiles = ProfileLibrary::new(vec![AppearanceProfile::capture("Mine", &h.state)])
        .expect("the profile is valid");

    let out = h.idle();
    let text = painted(&out);
    for label in [
        "Profiles",
        "Apply",
        "Delete",
        "Save current appearance",
        "Export text",
        "Copy",
        "Apply text",
    ] {
        assert!(
            text.contains(label),
            "the compact panel renders {label:?}:\n{text}"
        );
    }
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

    let presets =
        button_center(&run_app_frame(&ctx, &mut app, Vec::new()), "Presets").expect("Presets tab");
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

    let presets =
        button_center(&run_app_frame(&ctx, &mut app, Vec::new()), "Presets").expect("Presets tab");
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
