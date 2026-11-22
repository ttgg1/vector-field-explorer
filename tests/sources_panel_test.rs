//! Task 4: the responsive Sources rail/panel and the formula-bar mode switch.

use eframe::egui;
use eframe::egui::{Pos2, Rect, vec2};
use vector_field_explorer::color_ramp::Rgb;
use vector_field_explorer::custom_density::{
    ContinuityReport, ContinuityStatus, DensityFamily, DensitySpec, DensitySupport,
    DiagnosticsStatus, SupportKind, TimeLaw,
};
use vector_field_explorer::layer_style::PhysicsFamily;
use vector_field_explorer::object_scene::ObjectStyle;
use vector_field_explorer::source_scene::{
    FamilyColors, FieldMode, SourceDisplay, SourceEntry, SourceFamilyStatus, SourceKindTag,
    SourceSampling,
};
use vector_field_explorer::sources::SourceKind;
use vector_field_explorer::ui::appearance_panel::{AppearanceEdit, apply_edit};
use vector_field_explorer::ui::formula_bar::{self, FormulaBarState};
use vector_field_explorer::ui::layout::{LayoutState, SheetState};
use vector_field_explorer::ui::object_style_control::OBJECT_STYLE_HELP;
use vector_field_explorer::ui::sources_panel::{self, SourcesPanelState};
use vector_field_explorer::viewmodel::{DensityReportRow, MAX_DENSITIES, State};

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    ctx
}

struct WidgetNode {
    role: egui::Role,
    label: String,
    rect: Rect,
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
            (!label.is_empty()).then(|| WidgetNode {
                role: node.role(),
                label,
                rect: Rect::from_min_max(
                    Pos2::new(bounds.x0 as f32, bounds.y0 as f32),
                    Pos2::new(bounds.x1 as f32, bounds.y1 as f32),
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

fn coloured_texts(output: &egui::FullOutput) -> Vec<(String, egui::Color32)> {
    output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) => {
                let color = text
                    .galley
                    .job
                    .sections
                    .iter()
                    .find_map(|section| {
                        (section.format.color != egui::Color32::PLACEHOLDER)
                            .then_some(section.format.color)
                    })
                    .unwrap_or(text.fallback_color);
                Some((text.galley.text().to_owned(), color))
            }
            _ => None,
        })
        .collect()
}

fn rgb(color: [u8; 3]) -> egui::Color32 {
    egui::Color32::from_rgb(color[0], color[1], color[2])
}

fn show_sources_frame(
    ctx: &egui::Context,
    width: f32,
    state: &mut State,
    panel: &mut SourcesPanelState,
    compact: bool,
) -> egui::FullOutput {
    show_sources_frame_events(ctx, width, state, panel, compact, Vec::new())
}

fn show_sources_frame_events(
    ctx: &egui::Context,
    width: f32,
    state: &mut State,
    panel: &mut SourcesPanelState,
    compact: bool,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 900.0))),
            events,
            ..Default::default()
        },
        |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                sources_panel::show(ui, state, panel, compact);
            });
        },
    );
    output.textures_delta.clear();
    output
}

fn pointer(pos: Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

/// The SpinButton (DragValue) on the same row as `label`, to its right.
fn drag_value_for(nodes: &[WidgetNode], label: &str) -> Option<Rect> {
    // The panel can paint the same label elsewhere (Advanced also shows
    // "Softening"); the density editor is the lower occurrence.
    drag_values_for(nodes, label).last().copied()
}

/// Every SpinButton (DragValue) to the right of `label`, ordered top to bottom.
/// A label can repeat across sections (Advanced's "Softening") and rows.
fn drag_values_for(nodes: &[WidgetNode], label: &str) -> Vec<Rect> {
    let mut rects: Vec<Rect> = nodes
        .iter()
        .filter(|node| node.label == label)
        .filter_map(|label_node| spin_button_right_of(nodes, label_node.rect))
        .collect();
    rects.sort_by(|a, b| a.top().total_cmp(&b.top()));
    rects.dedup();
    rects
}

fn spin_button_right_of(nodes: &[WidgetNode], label: Rect) -> Option<Rect> {
    nodes
        .iter()
        .filter(|node| node.role == egui::Role::SpinButton)
        .filter(|node| (node.rect.center().y - label.center().y).abs() < 8.0)
        .filter(|node| node.rect.left() >= label.right() - 2.0)
        .min_by(|a, b| a.rect.left().total_cmp(&b.rect.left()))
        .map(|node| node.rect)
}

/// Every TextInput to the right of `label`, ordered top to bottom. Rows repeat
/// the "Formula" label, so callers pick an occurrence.
fn text_inputs_for(nodes: &[WidgetNode], label: &str) -> Vec<Rect> {
    let mut rects: Vec<Rect> = nodes
        .iter()
        .filter(|node| node.label == label)
        .filter_map(|label_node| {
            nodes
                .iter()
                .filter(|node| node.role == egui::Role::TextInput)
                .filter(|node| (node.rect.center().y - label_node.rect.center().y).abs() < 8.0)
                .filter(|node| node.rect.left() >= label_node.rect.right() - 2.0)
                .min_by(|a, b| a.rect.left().total_cmp(&b.rect.left()))
                .map(|node| node.rect)
        })
        .collect();
    rects.sort_by(|a, b| a.top().total_cmp(&b.top()));
    rects.dedup();
    rects
}

/// The ComboBox to the right of `label`. Several density selectors share a row,
/// so match by proximity rather than by the selected text alone.
fn combo_for(nodes: &[WidgetNode], label: &str) -> Option<Rect> {
    let label_node = find(nodes, label)?;
    nodes
        .iter()
        .filter(|node| node.role == egui::Role::ComboBox)
        .filter(|node| (node.rect.center().y - label_node.rect.center().y).abs() < 8.0)
        .filter(|node| node.rect.left() >= label_node.rect.right() - 2.0)
        .min_by(|a, b| a.rect.left().total_cmp(&b.rect.left()))
        .map(|node| node.rect)
}

/// Open the combo next to `label` and click the option with `option` text.
fn select_combo_option(
    ctx: &egui::Context,
    width: f32,
    state: &mut State,
    panel: &mut SourcesPanelState,
    compact: bool,
    label: &str,
    option: &str,
) {
    let painted = nodes(&paint_panel(ctx, width, state, panel, compact, 1));
    let combo = combo_for(&painted, label).unwrap_or_else(|| panic!("{label} has no combo box"));
    click_sources(ctx, width, combo.center(), state, panel, compact);
    let mut painted = nodes(&paint_panel(ctx, width, state, panel, compact, 1));
    // A click cycle can just toggle a popup shut depending on which frame egui
    // opens it in, so retry the open before giving up.
    let mut tries = 0;
    while find(&painted, option).is_none() && tries < 2 {
        click_sources(ctx, width, combo.center(), state, panel, compact);
        painted = nodes(&paint_panel(ctx, width, state, panel, compact, 1));
        tries += 1;
    }
    let target = find(&painted, option)
        .unwrap_or_else(|| {
            panic!(
                "{option} is not offered by {label}: {:?}",
                painted
                    .iter()
                    .map(|node| node.label.as_str())
                    .collect::<Vec<_>>()
            )
        })
        .rect
        .center();
    click_sources(ctx, width, target, state, panel, compact);
}

fn click_sources(
    ctx: &egui::Context,
    width: f32,
    pos: Pos2,
    state: &mut State,
    panel: &mut SourcesPanelState,
    compact: bool,
) {
    for pressed in [true, false] {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 900.0))),
                events: vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    sources_panel::show(ui, state, panel, compact);
                });
            },
        );
        output.textures_delta.clear();
    }
}

fn show_formula_frame(
    ctx: &egui::Context,
    state: &mut State,
    bar: &mut FormulaBarState,
) -> egui::FullOutput {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 240.0))),
            ..Default::default()
        },
        |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                formula_bar::show(ui, state, bar, false);
            });
        },
    );
    output.textures_delta.clear();
    output
}

fn click_formula(ctx: &egui::Context, pos: Pos2, state: &mut State, bar: &mut FormulaBarState) {
    for pressed in [true, false] {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 240.0))),
                events: vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    formula_bar::show(ui, state, bar, false);
                });
            },
        );
        output.textures_delta.clear();
    }
}

/// Run the panel for `frames` frames at `width` and return the last output.
fn paint_panel(
    ctx: &egui::Context,
    width: f32,
    state: &mut State,
    panel: &mut SourcesPanelState,
    compact: bool,
    frames: usize,
) -> egui::FullOutput {
    let mut last = None;
    for _ in 0..frames {
        last = Some(show_sources_frame(ctx, width, state, panel, compact));
    }
    last.unwrap()
}

#[test]
fn sources_panel_adds_removes_and_resets_through_real_widgets() {
    for (width, compact) in [(1200.0, false), (390.0, true)] {
        let ctx = context();
        let mut state = State::new(true);
        state.set_field_mode(FieldMode::Sources);
        let mut panel = SourcesPanelState::default();
        let mut out = None;
        for _ in 0..3 {
            out = Some(show_sources_frame(
                &ctx, width, &mut state, &mut panel, compact,
            ));
        }
        let painted = nodes(out.as_ref().unwrap());
        assert!(has(&painted, "Sources"));
        assert!(has(&painted, "Add"));
        assert!(has(&painted, "Advanced"));
        assert!(has(
            &painted,
            "Show finite results labelled \"accuracy not met\""
        ));
        assert!(has(
            &painted,
            "Streamlines and particles use the formula field; switch to Formula to use them."
        ));
        let add = find(&painted, "Add").unwrap().rect.center();
        click_sources(&ctx, width, add, &mut state, &mut panel, compact);
        assert_eq!(state.sources.entries.len(), 1);
        for _ in 0..3 {
            out = Some(show_sources_frame(
                &ctx, width, &mut state, &mut panel, compact,
            ));
        }
        let painted = nodes(out.as_ref().unwrap());
        let remove = find(&painted, "Remove source").unwrap().rect.center();
        click_sources(&ctx, width, remove, &mut state, &mut panel, compact);
        assert!(state.sources.entries.is_empty());
    }
}

#[test]
fn formula_bar_switches_mode_and_hides_the_editor() {
    let ctx = context();
    let mut state = State::new(false);
    let mut bar = FormulaBarState::default();
    let out = show_formula_frame(&ctx, &mut state, &mut bar);
    let painted = nodes(&out);
    assert!(has(&painted, "Formula") && has(&painted, "Sources"));
    let sources = find(&painted, "Sources").unwrap().rect.center();
    click_formula(&ctx, sources, &mut state, &mut bar);
    assert_eq!(state.field_mode, FieldMode::Sources);
    let out = show_formula_frame(&ctx, &mut state, &mut bar);
    let painted = nodes(&out);
    assert!(
        !painted
            .iter()
            .any(|node| node.role == egui::Role::TextInput)
    );
    assert!(has(&painted, "Open Sources"));
}

#[test]
fn invalid_draft_does_not_replace_the_committed_source() {
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Sources);
    state
        .sources
        .add(SourceKindTag::ChargedRing.default_source())
        .unwrap();
    let mut panel = SourcesPanelState::default();
    panel.sync(&state.sources);
    let original = state.sources.entries[0].source;
    let error = panel.set_radius(0, 0.0);
    assert!(error.is_some());
    assert_eq!(state.sources.entries[0].source, original);
    assert!(panel.drafts[0].error.is_some());
}

#[test]
fn advanced_controls_update_softening_and_opt_in_only() {
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Sources);
    state
        .sources
        .add(SourceKindTag::PointCharge.default_source())
        .unwrap();
    let mut panel = SourcesPanelState::default();
    panel.sync(&state.sources);
    assert!(panel.set_softening(&mut state, 0.02).is_ok());
    assert_eq!(state.sources.softening, 0.02);
    panel.set_allow_inaccurate(&mut state, true);
    assert!(state.sources.allow_inaccurate);
    assert!(panel.set_softening(&mut state, 1e-9).is_err());
    assert_eq!(
        state.sources.softening, 0.02,
        "a rejected softening is inert"
    );
}

#[test]
fn source_status_text_names_hidden_and_shown_accuracy() {
    let hidden_sampling = SourceSampling {
        electric: SourceFamilyStatus {
            sources: 1,
            accuracy_not_met_total: 3,
            ..SourceFamilyStatus::default()
        },
        ..SourceSampling::default()
    };
    let hidden = sources_panel::status_text(&hidden_sampling).expect("hidden accuracy is reported");
    assert!(hidden.contains('3'));
    assert!(hidden.contains("hidden"));

    let shown_sampling = SourceSampling {
        electric: SourceFamilyStatus {
            sources: 1,
            accuracy_not_met_total: 3,
            accuracy_not_met_drawn: 3,
            ..SourceFamilyStatus::default()
        },
        allow_inaccurate: true,
        ..SourceSampling::default()
    };
    let shown = sources_panel::status_text(&shown_sampling).expect("shown accuracy is reported");
    assert!(shown.contains("accuracy not met"));

    let unavailable_sampling = SourceSampling {
        magnetic: SourceFamilyStatus {
            sources: 1,
            unavailable: 2,
            ..SourceFamilyStatus::default()
        },
        ..SourceSampling::default()
    };
    let unavailable =
        sources_panel::status_text(&unavailable_sampling).expect("unavailable is reported");
    assert!(unavailable.contains("unavailable"));
}

#[test]
fn layout_and_sheets_expose_sources() {
    let mut layout = LayoutState::default();
    assert!(!layout.sources_expanded);
    layout.toggle_sources();
    assert!(layout.sources_expanded);
    layout.sources_width = 10.0;
    layout = layout.sanitized();
    assert!(layout.sources_width >= 180.0);
    let mut sheet = SheetState::default();
    sheet.open(SheetState::Sources);
    assert!(sheet.shows(SheetState::Sources));
}

#[test]
fn source_notice_is_rendered_and_survives_until_dismissed() {
    let ctx = context();
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Sources);
    state.source_notice = Some("Some saved sources were invalid and were skipped".into());
    let mut panel = SourcesPanelState::default();
    let out = paint_panel(&ctx, 1200.0, &mut state, &mut panel, false, 3);
    let painted = nodes(&out);
    assert!(has(
        &painted,
        "Some saved sources were invalid and were skipped"
    ));
    assert_eq!(
        state.source_notice.as_deref(),
        Some("Some saved sources were invalid and were skipped")
    );
    let dismiss = find(&painted, "Dismiss").unwrap().rect.center();
    click_sources(&ctx, 1200.0, dismiss, &mut state, &mut panel, false);
    assert!(state.source_notice.is_none());

    // Any real scene edit also clears the restored notice.
    state.source_notice = Some("Some saved sources were invalid and were skipped".into());
    let out = paint_panel(&ctx, 1200.0, &mut state, &mut panel, false, 2);
    let add = find(&nodes(&out), "Add").unwrap().rect.center();
    click_sources(&ctx, 1200.0, add, &mut state, &mut panel, false);
    assert_eq!(state.sources.entries.len(), 1);
    assert!(
        state.source_notice.is_none(),
        "an edit clears the restore notice"
    );
}

#[test]
fn scene_error_text_names_the_displayed_row_and_parameter() {
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Sources);
    // Row 1 is disabled, so the enabled-only validation index 0 belongs to
    // the second row the user actually sees.
    let mut invalid = SourceKindTag::ChargedRing.default_source();
    let SourceKind::ChargedRing { radius, .. } = &mut invalid.kind else {
        unreachable!()
    };
    *radius = 0.0;
    state.sources.entries = vec![
        SourceEntry {
            enabled: false,
            source: SourceKindTag::PointCharge.default_source(),
        },
        SourceEntry {
            enabled: true,
            source: invalid,
        },
    ];
    state.refresh_source_sampling();
    let message = state
        .source_sampling()
        .and_then(|sampling| sampling.validation_error.clone())
        .expect("invalid scene reports a message");
    assert!(
        message.contains("Source 2"),
        "row-mapped message: {message}"
    );
    assert!(
        !message.contains("Source 1"),
        "never the enabled-only index: {message}"
    );
    assert!(message.contains("radius"), "names the parameter: {message}");
    assert!(
        !message.contains("InvalidSource") && !message.contains("SourceParameter"),
        "no raw enum debug: {message}"
    );
}

#[test]
fn valid_draft_edits_replace_only_the_edited_entry() {
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Sources);
    state
        .sources
        .add(SourceKindTag::ChargedRing.default_source())
        .unwrap();
    state
        .sources
        .add(SourceKindTag::PointCharge.default_source())
        .unwrap();
    let mut panel = SourcesPanelState::default();
    panel.sync(&state.sources);
    let untouched = state.sources.entries[1].source;
    assert!(panel.set_radius(0, 2.5).is_none());
    assert!(panel.commit(0, &mut state).is_none());
    let SourceKind::ChargedRing { radius, .. } = state.sources.entries[0].source.kind else {
        unreachable!()
    };
    assert_eq!(radius, 2.5);
    assert_eq!(state.sources.entries[1].source, untouched);
    assert!(panel.drafts[0].error.is_none());
    assert!(!panel.drafts[0].dirty);
}

#[test]
fn removal_drops_invalid_draft_text_before_the_survivor_edit() {
    let ctx = context();
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Sources);
    state
        .sources
        .add(SourceKindTag::ChargedRing.default_source())
        .unwrap();
    state
        .sources
        .add(SourceKindTag::PointCharge.default_source())
        .unwrap();
    let survivor = state.sources.entries[1].source;

    let mut panel = SourcesPanelState::default();
    panel.sync(&state.sources);
    assert!(panel.set_radius(0, 0.0).is_some(), "entry 0 is dirty");
    assert!(panel.drafts[0].dirty && panel.drafts[0].error.is_some());

    // Remove entry 0 through the real topmost Remove button.
    let out = paint_panel(&ctx, 1200.0, &mut state, &mut panel, false, 2);
    let remove = nodes(&out)
        .into_iter()
        .filter(|node| node.label == "Remove source")
        .min_by(|a, b| a.rect.top().total_cmp(&b.rect.top()))
        .unwrap()
        .rect
        .center();
    click_sources(&ctx, 1200.0, remove, &mut state, &mut panel, false);
    assert_eq!(state.sources.entries.len(), 1);
    assert_eq!(state.sources.entries[0].source, survivor);

    // The surviving draft must be B, so a valid edit cannot inherit A's error
    // or its kind/parameters.
    let edit_error = panel.set_center(0, survivor.center.to_array());
    assert!(
        edit_error.is_none(),
        "a valid survivor edit must not inherit the removed draft's error: {edit_error:?}"
    );
    assert!(panel.commit(0, &mut state).is_none());
    assert_eq!(
        state.sources.entries[0].source, survivor,
        "the removed draft must never be committed onto the survivor"
    );
}

#[test]
fn dirty_draft_after_an_earlier_removal_never_rebinds() {
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Sources);
    state
        .sources
        .add(SourceKindTag::PointCharge.default_source())
        .unwrap();
    state
        .sources
        .add(SourceKindTag::ChargedRing.default_source())
        .unwrap();
    // Dirty entry 1 with a valid-but-uncommitted value, then simulate the
    // earlier entry disappearing and sync as `remove` does.
    let mut panel = SourcesPanelState::default();
    panel.sync(&state.sources);
    assert!(panel.set_radius(1, 2.0).is_none());
    assert!(panel.drafts[1].dirty);

    let survivor = state.sources.entries[1].source;
    let mut scene = state.sources.clone();
    scene.remove(0);
    state.set_source_scene(scene);
    panel.sync(&state.sources);

    assert!(
        panel.set_center(0, survivor.center.to_array()).is_none(),
        "the survivor's fresh draft accepts a valid edit"
    );
    assert!(panel.commit(0, &mut state).is_none());
    assert_eq!(
        state.sources.entries[0].source, survivor,
        "a dirty draft after the removed entry must not rebind to the survivor"
    );
}

#[test]
fn add_is_disabled_at_the_source_limit() {
    let ctx = context();
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Sources);
    for _ in 0..64 {
        state
            .sources
            .add(SourceKindTag::PointCharge.default_source())
            .unwrap();
    }
    let mut panel = SourcesPanelState::default();
    let out = paint_panel(&ctx, 1200.0, &mut state, &mut panel, false, 2);
    let painted = nodes(&out);
    assert!(
        find(&painted, "Add").unwrap().disabled,
        "Add is disabled at 64 sources"
    );
}

#[test]
fn both_source_display_paints_the_viewport_family_legend() {
    // Drive the real app frame so the legend comes from `frame_overlays`, not
    // from a replica. The Sources panel is collapsed by default, so the panel's
    // own E/B swatch row cannot satisfy these assertions.
    let ctx = context();
    let mut app = vector_field_explorer::VfApp::new();
    assert!(!app.layout.sources_expanded, "the panel starts collapsed");
    app.state.set_field_mode(FieldMode::Sources);
    let mut scene = app.state.sources.clone();
    scene.set_display(SourceDisplay::Both);
    app.state.set_source_scene(scene);

    let mut eframe_frame = eframe::Frame::_new_kittest();
    let mut out = None;
    for _ in 0..2 {
        let mut frame = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 800.0))),
                ..Default::default()
            },
            |ui| eframe::App::ui(&mut app, ui, &mut eframe_frame),
        );
        frame.textures_delta.clear();
        out = Some(frame);
    }
    let out = out.unwrap();
    let texts = coloured_texts(&out);
    let defaults = FamilyColors::default();
    assert!(
        texts
            .iter()
            .any(|(text, color)| text == "E" && *color == rgb(defaults.electric)),
        "the viewport legend paints the electric family in its shipped colour"
    );
    assert!(
        texts
            .iter()
            .any(|(text, color)| text == "B" && *color == rgb(defaults.magnetic)),
        "the viewport legend paints the magnetic family in its shipped colour"
    );
    assert!(
        !texts
            .iter()
            .any(|(text, _)| text.contains("amber") || text.contains("cyan")),
        "the legend never names a fixable colour"
    );

    // A live Appearance override must repaint the real overlay while the other
    // family keeps its shipped fallback.
    let electric = Rgb([12, 34, 56]);
    apply_edit(
        &mut app.state,
        AppearanceEdit::FamilyColor(PhysicsFamily::Electric, Some(electric)),
    )
    .unwrap();
    let mut repainted = None;
    for _ in 0..2 {
        let mut frame = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 800.0))),
                ..Default::default()
            },
            |ui| eframe::App::ui(&mut app, ui, &mut eframe_frame),
        );
        frame.textures_delta.clear();
        repainted = Some(frame);
    }
    let repainted = coloured_texts(&repainted.unwrap());
    assert!(
        repainted
            .iter()
            .any(|(text, color)| text == "E" && *color == rgb(electric.0)),
        "the viewport legend follows the live electric override"
    );
    assert!(
        repainted
            .iter()
            .any(|(text, color)| text == "B" && *color == rgb(defaults.magnetic)),
        "the magnetic legend keeps its shipped fallback while electric is overridden"
    );
}

#[test]
fn both_source_panel_swatches_follow_appearance_overrides_in_wide_and_compact() {
    let ctx = context();
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Sources);
    let mut scene = state.sources.clone();
    scene.set_display(SourceDisplay::Both);
    state.set_source_scene(scene);
    let mut panel = SourcesPanelState::default();

    let mut out = None;
    for _ in 0..3 {
        out = Some(show_sources_frame(
            &ctx, 1200.0, &mut state, &mut panel, false,
        ));
    }
    let texts = coloured_texts(out.as_ref().unwrap());
    let defaults = FamilyColors::default();
    assert!(
        texts
            .iter()
            .any(|(text, color)| text == "E" && *color == rgb(defaults.electric)),
        "the panel's Both row paints the shipped electric swatch"
    );
    assert!(
        texts
            .iter()
            .any(|(text, color)| text == "B" && *color == rgb(defaults.magnetic)),
        "the panel's Both row paints the shipped magnetic swatch"
    );

    let electric = Rgb([12, 34, 56]);
    let magnetic = Rgb([200, 150, 100]);
    apply_edit(
        &mut state,
        AppearanceEdit::FamilyColor(PhysicsFamily::Electric, Some(electric)),
    )
    .unwrap();
    apply_edit(
        &mut state,
        AppearanceEdit::FamilyColor(PhysicsFamily::Magnetic, Some(magnetic)),
    )
    .unwrap();
    let out = show_sources_frame(&ctx, 1200.0, &mut state, &mut panel, false);
    let texts = coloured_texts(&out);
    assert!(
        texts
            .iter()
            .any(|(text, color)| text == "E" && *color == rgb(electric.0)),
        "the electric swatch follows the live Appearance override"
    );
    assert!(
        texts
            .iter()
            .any(|(text, color)| text == "B" && *color == rgb(magnetic.0)),
        "the magnetic swatch follows the live Appearance override"
    );
    assert!(
        !texts
            .iter()
            .any(|(text, _)| text.contains("amber") || text.contains("cyan")),
        "an overridden colour is never labelled amber or cyan"
    );

    // The compact sheet keeps the same live Both row usable.
    let mut compact_panel = SourcesPanelState::default();
    let mut out = None;
    for _ in 0..3 {
        out = Some(show_sources_frame(
            &ctx,
            360.0,
            &mut state,
            &mut compact_panel,
            true,
        ));
    }
    let texts = coloured_texts(out.as_ref().unwrap());
    assert!(
        texts
            .iter()
            .any(|(text, color)| text == "E" && *color == rgb(electric.0)),
        "the compact Sources row keeps the electric swatch"
    );
    assert!(
        texts
            .iter()
            .any(|(text, color)| text == "B" && *color == rgb(magnetic.0)),
        "the compact Sources row keeps the magnetic swatch"
    );
}

#[test]
fn object_style_control_writes_the_enum_and_names_both_modes() {
    let ctx = context();
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Sources);
    let mut panel = SourcesPanelState::default();

    let mut out = None;
    for _ in 0..3 {
        out = Some(show_sources_frame(
            &ctx, 1200.0, &mut state, &mut panel, false,
        ));
    }
    let painted = nodes(out.as_ref().unwrap());
    assert!(has(&painted, "Object style"));
    assert!(
        has(&painted, OBJECT_STYLE_HELP),
        "the shared help text names both modes and the SVG line blueprint"
    );
    assert_eq!(state.object_style(), ObjectStyle::Solid);

    let wireframe = find(&painted, "Wireframe").unwrap().rect.center();
    click_sources(&ctx, 1200.0, wireframe, &mut state, &mut panel, false);
    assert_eq!(state.object_style(), ObjectStyle::Wireframe);

    for _ in 0..3 {
        out = Some(show_sources_frame(
            &ctx, 1200.0, &mut state, &mut panel, false,
        ));
    }
    let painted = nodes(out.as_ref().unwrap());
    let solid = find(&painted, "Solid").unwrap().rect.center();
    click_sources(&ctx, 1200.0, solid, &mut state, &mut panel, false);
    assert_eq!(state.object_style(), ObjectStyle::Solid);
}

#[test]
fn display_selector_changes_only_the_scene_display() {
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Sources);
    let mut panel = SourcesPanelState::default();
    panel.sync(&state.sources);
    assert_eq!(state.sources.display, SourceDisplay::Both);
    // The panel uses the same setter path; exercising it keeps the committed
    // scene and display isolated from the draft list.
    let mut scene = state.sources.clone();
    scene.display = SourceDisplay::Electric;
    state.set_source_scene(scene);
    assert_eq!(state.sources.display, SourceDisplay::Electric);
    assert!(state.sources.entries.is_empty());
}

fn density_support() -> DensitySupport {
    DensitySupport {
        kind: SupportKind::Box,
        center: glam::Vec3::ZERO,
        axis: glam::Vec3::Z,
        size: glam::Vec3::splat(2.0),
    }
}

fn density_spec(name: &str, family: DensityFamily, formula: &str) -> DensitySpec {
    DensitySpec {
        name: name.to_string(),
        family,
        support: density_support(),
        formula: formula.to_string(),
        samples: 8,
        softening: 0.0,
        time_law: TimeLaw::Steady,
        charge_formula: None,
    }
}

fn open_custom_densities(
    ctx: &egui::Context,
    width: f32,
    state: &mut State,
    panel: &mut SourcesPanelState,
    compact: bool,
) {
    let painted = nodes(&paint_panel(ctx, width, state, panel, compact, 3));
    let header = find(&painted, "Custom densities")
        .expect("the Sources panel paints the custom densities section")
        .rect
        .center();
    click_sources(ctx, width, header, state, panel, compact);
    let _ = paint_panel(ctx, width, state, panel, compact, 2);
}

#[test]
fn reports_are_computed_on_demand_not_per_frame() {
    for (width, compact) in [(1200.0, false), (390.0, true)] {
        let ctx = context();
        let mut state = State::new(true);
        state.set_field_mode(FieldMode::Sources);
        state.time = 1.0;
        state
            .commit_densities(vec![density_spec(
                "transient",
                DensityFamily::VolumeCharge,
                "1/(t-1)",
            )])
            .expect("the fixture density is valid at t = 0");
        let mut panel = SourcesPanelState::default();
        assert!(
            state.density_reports().is_empty(),
            "a committed density does not sample a report by itself"
        );
        let generation = state.density_report_generation();
        let _ = paint_panel(&ctx, width, &mut state, &mut panel, compact, 4);
        assert_eq!(
            state.density_report_generation(),
            generation,
            "painting frames never refreshes the reports"
        );
        assert!(state.density_reports().is_empty());

        open_custom_densities(&ctx, width, &mut state, &mut panel, compact);
        let painted = nodes(&paint_panel(
            &ctx, width, &mut state, &mut panel, compact, 1,
        ));
        let compute = find(&painted, "Compute")
            .expect("the open section offers an explicit Compute action")
            .rect
            .center();
        click_sources(&ctx, width, compute, &mut state, &mut panel, compact);
        assert_eq!(
            state.density_report_generation(),
            generation + 1,
            "the Compute action refreshes exactly once"
        );
        assert_eq!(state.density_reports().len(), 1);
    }
}

#[test]
fn unresolved_and_violation_reasons_are_shown_verbatim() {
    let ctx = context();
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Sources);
    state.time = 1.0;
    state
        .commit_densities(vec![
            density_spec("transient", DensityFamily::VolumeCharge, "1/(t-1)"),
            density_spec("divergent", DensityFamily::SteadyCurrent, "x,0,0"),
        ])
        .expect("both fixtures are valid at t = 0");
    state.refresh_density_reports();
    let mut expected = Vec::new();
    for row in state.density_reports() {
        if let Some(report) = &row.charge {
            expected.push(match &report.status {
                DiagnosticsStatus::Estimate { error } => format!("Estimate: {error}"),
                DiagnosticsStatus::Unresolved { reason } => format!("Unresolved: {reason}"),
                DiagnosticsStatus::Violation { reason } => format!("Violation: {reason}"),
            });
        }
        if let Some(report) = &row.continuity {
            expected.push(match &report.status {
                ContinuityStatus::Estimate { value, error } => {
                    format!("Estimate: div J in [{:.6e}, {:.6e}], residual = {value:.6e}, error = {error:.6e}", report.div_j_min, report.div_j_max)
                }
                ContinuityStatus::Unresolved { reason } => format!("Unresolved: {reason}"),
                ContinuityStatus::Violation {
                    residual,
                    tolerance,
                } => format!("Violation: residual = {residual:.6e}, tolerance = {tolerance:.6e}"),
            });
        }
    }
    assert!(
        expected.iter().any(|line| line.starts_with("Unresolved: ")),
        "the fixture produces an unresolved report: {expected:?}"
    );
    assert!(
        expected.iter().any(|line| line.starts_with("Violation: ")),
        "the fixture produces a violation report: {expected:?}"
    );

    let mut panel = SourcesPanelState::default();
    open_custom_densities(&ctx, 1200.0, &mut state, &mut panel, false);
    let painted = nodes(&paint_panel(&ctx, 1200.0, &mut state, &mut panel, false, 1));
    for line in expected {
        assert!(
            painted.iter().any(|node| node.label.contains(&line)),
            "the panel shows {line:?} verbatim; painted labels: {:?}",
            painted
                .iter()
                .map(|node| node.label.as_str())
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn a_multi_frame_density_drag_commits_and_refreshes_once() {
    let ctx = context();
    // Snap collapsing headers open so the row's interact rect does not grow
    // while the drag is being synthesised.
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Sources);
    state
        .commit_densities(vec![density_spec("drag", DensityFamily::VolumeCharge, "1")])
        .expect("the fixture density is valid");
    let mut panel = SourcesPanelState::default();
    open_custom_densities(&ctx, 1200.0, &mut state, &mut panel, false);

    let painted = nodes(&paint_panel(&ctx, 1200.0, &mut state, &mut panel, false, 1));
    let rect = drag_value_for(&painted, "Softening")
        .expect("the density row paints a Softening drag value");
    let start = rect.center();
    let end = start + vec2(40.0, 0.0);
    let base = state.density_report_generation();

    // Press, two move frames, release: a real continuous drag.
    let _ = show_sources_frame_events(
        &ctx,
        1200.0,
        &mut state,
        &mut panel,
        false,
        vec![egui::Event::PointerMoved(start), pointer(start, true)],
    );
    let _ = show_sources_frame_events(
        &ctx,
        1200.0,
        &mut state,
        &mut panel,
        false,
        vec![egui::Event::PointerMoved(start + vec2(20.0, 0.0))],
    );
    let _ = show_sources_frame_events(
        &ctx,
        1200.0,
        &mut state,
        &mut panel,
        false,
        vec![egui::Event::PointerMoved(end)],
    );
    assert_eq!(
        state.density_report_generation(),
        base,
        "mid-drag frames must not validate, commit or refresh the reports"
    );
    let _ = show_sources_frame_events(
        &ctx,
        1200.0,
        &mut state,
        &mut panel,
        false,
        vec![pointer(end, false)],
    );
    assert_eq!(
        state.density_report_generation(),
        base + 1,
        "the whole drag refreshes the reports exactly once"
    );
    assert_ne!(
        state.densities()[0].softening,
        0.0,
        "the drag still reached the committed density"
    );
}

#[test]
fn a_drag_release_and_a_text_edit_in_one_frame_commit_both_rows() {
    let ctx = context();
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Sources);
    state
        .commit_densities(vec![
            density_spec("dragged", DensityFamily::VolumeCharge, "1"),
            density_spec("typed", DensityFamily::VolumeCharge, "1"),
        ])
        .expect("the fixture densities are valid");
    let mut panel = SourcesPanelState::default();
    open_custom_densities(&ctx, 1200.0, &mut state, &mut panel, false);

    // Focus the second row's formula field. The text event sent on the drag
    // release frame below lands in this focused editor.
    let painted = nodes(&paint_panel(&ctx, 1200.0, &mut state, &mut panel, false, 1));
    let formula = text_inputs_for(&painted, "Formula")
        .into_iter()
        .next_back()
        .expect("the second density row paints a formula text field");
    click_sources(
        &ctx,
        1200.0,
        formula.center(),
        &mut state,
        &mut panel,
        false,
    );

    // The panel also paints Advanced's "Softening"; the density rows follow it,
    // so occurrence 1 is the first density row.
    let painted = nodes(&paint_panel(&ctx, 1200.0, &mut state, &mut panel, false, 1));
    let softening = drag_values_for(&painted, "Softening");
    assert_eq!(
        softening.len(),
        3,
        "Advanced plus two density rows paint a Softening drag value: {softening:?}"
    );
    let start = softening[1].center();
    let end = start + vec2(40.0, 0.0);

    let _ = show_sources_frame_events(
        &ctx,
        1200.0,
        &mut state,
        &mut panel,
        false,
        vec![egui::Event::PointerMoved(start), pointer(start, true)],
    );
    let _ = show_sources_frame_events(
        &ctx,
        1200.0,
        &mut state,
        &mut panel,
        false,
        vec![egui::Event::PointerMoved(start + vec2(20.0, 0.0))],
    );
    let _ = show_sources_frame_events(
        &ctx,
        1200.0,
        &mut state,
        &mut panel,
        false,
        vec![egui::Event::PointerMoved(end)],
    );
    // One frame ends the drag gesture and edits the other row's formula.
    let _ = show_sources_frame_events(
        &ctx,
        1200.0,
        &mut state,
        &mut panel,
        false,
        vec![pointer(end, false), egui::Event::Text("2".to_owned())],
    );
    let _ = paint_panel(&ctx, 1200.0, &mut state, &mut panel, false, 1);

    let softening = state.densities()[0].softening;
    let formula = state.densities()[1].formula.clone();
    assert!(
        softening != 0.0 && formula != "1",
        "the drag release and the text edit both commit in one frame: \
         first row softening = {softening}, second row formula = {formula:?}"
    );
}

#[test]
fn removing_one_row_preserves_another_rows_pending_drag() {
    let ctx = context();
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Sources);
    state
        .commit_densities(vec![
            density_spec("dragged", DensityFamily::VolumeCharge, "1"),
            density_spec("removed", DensityFamily::VolumeCharge, "1"),
        ])
        .expect("the fixture densities are valid");
    let mut panel = SourcesPanelState::default();
    open_custom_densities(&ctx, 1200.0, &mut state, &mut panel, false);

    let painted = nodes(&paint_panel(&ctx, 1200.0, &mut state, &mut panel, false, 1));
    let start = drag_values_for(&painted, "Softening")[1].center();
    let _ = show_sources_frame_events(
        &ctx,
        1200.0,
        &mut state,
        &mut panel,
        false,
        vec![egui::Event::PointerMoved(start), pointer(start, true)],
    );
    let _ = show_sources_frame_events(
        &ctx,
        1200.0,
        &mut state,
        &mut panel,
        false,
        vec![egui::Event::PointerMoved(start + vec2(40.0, 0.0))],
    );

    // Remove the *other* row while the first row's drag is still in flight.
    assert!(
        panel.remove_density(1, &mut state).is_none(),
        "the second row is removed"
    );

    assert_ne!(
        state.densities()[0].softening,
        0.0,
        "removing another row must not cancel the pending drag"
    );
}

#[test]
fn a_rejected_draft_edit_keeps_the_panel_cap_message() {
    let mut state = State::new(true);
    let at_cap: Vec<DensitySpec> = (0..MAX_DENSITIES)
        .map(|index| {
            density_spec(
                &format!("density {index}"),
                DensityFamily::VolumeCharge,
                "1",
            )
        })
        .collect();
    state
        .commit_densities(at_cap)
        .expect("the cap itself is accepted");
    let mut panel = SourcesPanelState::default();
    panel.sync_densities(&state);
    assert!(
        panel.add_density(&mut state).is_some(),
        "adding at the cap is rejected"
    );
    let cap_message = panel
        .density_error
        .clone()
        .expect("the cap rejection sets the panel message");

    panel.density_drafts[0].formula = "(".to_string();
    assert!(
        panel.commit_density(0, &mut state).is_some(),
        "the invalid draft is rejected"
    );
    assert!(
        panel.density_drafts[0].error.is_some(),
        "the rejection is shown on the draft itself"
    );
    assert_eq!(
        panel.density_error.as_deref(),
        Some(cap_message.as_str()),
        "a rejected per-draft edit must not clear another action's panel message"
    );

    // A successful per-draft edit does not change the cap condition either.
    panel.density_drafts[0].formula = "1".to_string();
    assert!(
        panel.commit_density(0, &mut state).is_none(),
        "the repaired draft commits"
    );
    assert_eq!(
        panel.density_error.as_deref(),
        Some(cap_message.as_str()),
        "the cap message stays until the cap condition clears"
    );
}

fn harmonic(spec: &mut DensitySpec, omega: f64, phase: f64) {
    spec.time_law = TimeLaw::Harmonic { omega, phase };
}

#[test]
fn harmonic_selector_reveals_omega_and_phase_and_commits_them() {
    let ctx = context();
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Sources);
    state
        .commit_densities(vec![density_spec(
            "ac wire",
            DensityFamily::SteadyCurrent,
            "0,0,1",
        )])
        .expect("the steady fixture is valid");
    let mut panel = SourcesPanelState::default();
    open_custom_densities(&ctx, 1200.0, &mut state, &mut panel, false);

    let painted = nodes(&paint_panel(&ctx, 1200.0, &mut state, &mut panel, false, 1));
    assert!(
        !has(&painted, "Omega (rad/s)") && !has(&painted, "Phase (rad)"),
        "a steady law hides the harmonic parameters"
    );
    assert!(
        combo_for(&painted, "Time law").is_some(),
        "every density row paints a time-law selector"
    );

    select_combo_option(
        &ctx, 1200.0, &mut state, &mut panel, false, "Time law", "Harmonic",
    );
    assert_eq!(
        state.densities()[0].time_law,
        TimeLaw::Harmonic {
            omega: 1.0,
            phase: 0.0
        },
        "selecting Harmonic commits the seeded law"
    );
    let painted = nodes(&paint_panel(&ctx, 1200.0, &mut state, &mut panel, false, 1));
    assert!(
        has(&painted, "Omega (rad/s)") && has(&painted, "Phase (rad)"),
        "the harmonic law reveals omega and phase"
    );

    panel.density_drafts[0].omega = 2.0;
    panel.density_drafts[0].phase = 0.5;
    assert!(
        panel.commit_density(0, &mut state).is_none(),
        "the omega/phase mirrors commit"
    );
    assert_eq!(
        state.densities()[0].time_law,
        TimeLaw::Harmonic {
            omega: 2.0,
            phase: 0.5
        }
    );
}

#[test]
fn harmonic_current_without_a_charge_formula_shows_the_model_rejection() {
    let ctx = context();
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Sources);
    let mut spec = density_spec("ac wire", DensityFamily::SteadyCurrent, "0,0,1");
    harmonic(&mut spec, 1.0, 0.0);
    state
        .commit_densities(vec![spec])
        .expect("a harmonic current without a pairing is accepted by the model");
    state.refresh_density_reports();
    let missing_reason = state.density_reports()[0]
        .continuity
        .as_ref()
        .and_then(|report| match &report.status {
            ContinuityStatus::Unresolved { reason } => Some(reason.clone()),
            _ => None,
        })
        .expect("a pairing-free harmonic current reports the missing term");
    assert_eq!(
        missing_reason,
        "a harmonic current needs its paired charge density (charge_formula) for a \
         continuity check"
    );

    let mut panel = SourcesPanelState::default();
    open_custom_densities(&ctx, 1200.0, &mut state, &mut panel, false);
    let painted = nodes(&paint_panel(&ctx, 1200.0, &mut state, &mut panel, false, 1));
    assert!(
        painted.iter().any(|node| node
            .label
            .contains(&format!("Unresolved: {missing_reason}"))),
        "the missing-term rejection is shown verbatim: {:?}",
        painted
            .iter()
            .map(|node| node.label.as_str())
            .collect::<Vec<_>>()
    );

    // An unparseable pairing is rejected by the same model seam, and its text
    // is shown on the row rather than folded into a number.
    panel.density_drafts[0].charge_formula = "(".to_string();
    let rejection = panel
        .commit_density(0, &mut state)
        .expect("an unparseable charge formula is rejected");
    assert!(
        rejection.starts_with("paired charge formula:"),
        "the model rejection is surfaced verbatim: {rejection}"
    );
    let painted = nodes(&paint_panel(&ctx, 1200.0, &mut state, &mut panel, false, 1));
    assert!(
        painted
            .iter()
            .any(|node| node.label.contains("paired charge formula:")),
        "the unparseable rejection is painted on the draft"
    );
}

#[test]
fn steady_current_charge_formula_is_kept_with_an_ignored_note() {
    let ctx = context();
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Sources);
    let mut spec = density_spec("steady wire", DensityFamily::SteadyCurrent, "0,0,1");
    spec.charge_formula = Some("1".to_string());
    state
        .commit_densities(vec![spec])
        .expect("a steady current may keep an ignored pairing");
    let mut panel = SourcesPanelState::default();
    open_custom_densities(&ctx, 1200.0, &mut state, &mut panel, false);

    let painted = nodes(&paint_panel(&ctx, 1200.0, &mut state, &mut panel, false, 1));
    assert!(
        has(
            &painted,
            "Charge formula is ignored while the time law is steady."
        ),
        "the ignored-pairing note is shown"
    );
    assert_eq!(
        panel.density_drafts[0].charge_formula, "1",
        "the panel keeps the value instead of dropping it"
    );
    assert_eq!(
        panel.density_drafts[0].spec().charge_formula.as_deref(),
        Some("1"),
        "the mirror round-trips through the spec"
    );

    panel.density_drafts[0].formula = "0,0,2".to_string();
    assert!(panel.commit_density(0, &mut state).is_none());
    assert_eq!(
        state.densities()[0].charge_formula.as_deref(),
        Some("1"),
        "editing another field keeps the ignored pairing"
    );
}

#[test]
fn harmonic_charge_rows_hide_the_charge_formula_field() {
    let ctx = context();
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Sources);
    let mut spec = density_spec("space charge", DensityFamily::VolumeCharge, "1");
    harmonic(&mut spec, 1.0, 0.0);
    state
        .commit_densities(vec![spec])
        .expect("a harmonic charge density is valid");
    let mut panel = SourcesPanelState::default();
    open_custom_densities(&ctx, 1200.0, &mut state, &mut panel, false);

    let painted = nodes(&paint_panel(&ctx, 1200.0, &mut state, &mut panel, false, 1));
    assert!(
        has(&painted, "Omega (rad/s)"),
        "the charge row still edits its harmonic law"
    );
    assert!(
        !has(&painted, "Charge formula"),
        "only a SteadyCurrent row paints the pairing field"
    );
}

#[test]
fn dragging_omega_defers_the_commit_until_release() {
    let ctx = context();
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Sources);
    let mut spec = density_spec("ac wire", DensityFamily::SteadyCurrent, "0,0,1");
    harmonic(&mut spec, 1.0, 0.0);
    spec.charge_formula = Some("0".to_string());
    state
        .commit_densities(vec![spec])
        .expect("the harmonic fixture is valid");
    let mut panel = SourcesPanelState::default();
    open_custom_densities(&ctx, 1200.0, &mut state, &mut panel, false);

    let painted = nodes(&paint_panel(&ctx, 1200.0, &mut state, &mut panel, false, 1));
    let rect = drag_value_for(&painted, "Omega (rad/s)")
        .expect("the harmonic law paints an omega drag value");
    let start = rect.center();
    let end = start + vec2(40.0, 0.0);
    let base = state.density_report_generation();

    let _ = show_sources_frame_events(
        &ctx,
        1200.0,
        &mut state,
        &mut panel,
        false,
        vec![egui::Event::PointerMoved(start), pointer(start, true)],
    );
    let _ = show_sources_frame_events(
        &ctx,
        1200.0,
        &mut state,
        &mut panel,
        false,
        vec![egui::Event::PointerMoved(start + vec2(20.0, 0.0))],
    );
    let _ = show_sources_frame_events(
        &ctx,
        1200.0,
        &mut state,
        &mut panel,
        false,
        vec![egui::Event::PointerMoved(end)],
    );
    assert_eq!(
        state.densities()[0].time_law,
        TimeLaw::Harmonic {
            omega: 1.0,
            phase: 0.0
        },
        "mid-drag frames leave the committed omega untouched"
    );
    assert_eq!(
        state.density_report_generation(),
        base,
        "mid-drag frames never sample or refresh a report"
    );

    let _ = show_sources_frame_events(
        &ctx,
        1200.0,
        &mut state,
        &mut panel,
        false,
        vec![pointer(end, false)],
    );
    let TimeLaw::Harmonic { omega, phase } = state.densities()[0].time_law else {
        panic!("the released drag still commits a harmonic law");
    };
    assert_ne!(omega, 1.0, "the released drag reaches the committed omega");
    assert_eq!(phase, 0.0);
    assert_eq!(
        state.density_report_generation(),
        base + 1,
        "one drag release refreshes the reports exactly once"
    );
}

#[test]
fn continuity_violation_and_unresolved_reasons_are_shown_verbatim() {
    let ctx = context();
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Sources);
    state.time = 1.0;
    state
        .commit_densities(vec![
            density_spec("transient", DensityFamily::VolumeCharge, "1/(t-1)"),
            density_spec("divergent", DensityFamily::SteadyCurrent, "x,0,0"),
            density_spec("conserved", DensityFamily::SteadyCurrent, "0,0,1"),
        ])
        .expect("the fixtures are valid at t = 0");
    state.refresh_density_reports();
    let mut expected = Vec::new();
    for row in state.density_reports() {
        if let Some(report) = &row.charge {
            expected.push(match &report.status {
                DiagnosticsStatus::Estimate { error } => format!("Estimate: {error}"),
                DiagnosticsStatus::Unresolved { reason } => format!("Unresolved: {reason}"),
                DiagnosticsStatus::Violation { reason } => format!("Violation: {reason}"),
            });
        }
        if let Some(report) = &row.continuity {
            expected.push(match &report.status {
                ContinuityStatus::Estimate { value, error } => {
                    format!("Estimate: div J in [{:.6e}, {:.6e}], residual = {value:.6e}, error = {error:.6e}", report.div_j_min, report.div_j_max)
                }
                ContinuityStatus::Unresolved { reason } => format!("Unresolved: {reason}"),
                ContinuityStatus::Violation {
                    residual,
                    tolerance,
                } => format!("Violation: residual = {residual:.6e}, tolerance = {tolerance:.6e}"),
            });
        }
    }
    for prefix in ["Estimate: ", "Unresolved: ", "Violation: "] {
        assert!(
            expected.iter().any(|line| line.starts_with(prefix)),
            "the fixtures produce a {prefix:?} readout: {expected:?}"
        );
    }

    let mut panel = SourcesPanelState::default();
    open_custom_densities(&ctx, 1200.0, &mut state, &mut panel, false);
    let painted = nodes(&paint_panel(&ctx, 1200.0, &mut state, &mut panel, false, 1));
    for line in expected {
        assert!(
            painted.iter().any(|node| node.label.contains(&line)),
            "the panel shows {line:?} verbatim"
        );
    }
    assert!(
        painted.iter().all(|node| !node.label.contains("NaN")),
        "a report readout never renders a NaN placeholder"
    );
    assert!(
        painted
            .iter()
            .any(|node| node.label.contains("normalized teaching units")),
        "the continuity readout names its normalized units"
    );
}

#[test]
fn disabling_harmonic_restores_a_steady_law_and_keeps_the_formula() {
    let ctx = context();
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Sources);
    let mut spec = density_spec("ac wire", DensityFamily::SteadyCurrent, "0,0,1");
    harmonic(&mut spec, 2.0, 0.5);
    spec.charge_formula = Some("1".to_string());
    state
        .commit_densities(vec![spec])
        .expect("the harmonic fixture is valid");
    let mut panel = SourcesPanelState::default();
    open_custom_densities(&ctx, 1200.0, &mut state, &mut panel, false);

    select_combo_option(
        &ctx, 1200.0, &mut state, &mut panel, false, "Time law", "Steady",
    );
    assert_eq!(state.densities()[0].time_law, TimeLaw::Steady);
    assert_eq!(
        state.densities()[0].charge_formula.as_deref(),
        Some("1"),
        "disabling harmonic keeps the charge formula"
    );
    assert_eq!(panel.density_drafts[0].charge_formula, "1");

    let painted = nodes(&paint_panel(&ctx, 1200.0, &mut state, &mut panel, false, 1));
    assert!(
        has(
            &painted,
            "Charge formula is ignored while the time law is steady."
        ),
        "the steady row explains why the retained formula is inert"
    );

    panel.density_drafts[0].formula = "0,0,3".to_string();
    assert!(panel.commit_density(0, &mut state).is_none());
    assert_eq!(
        state.densities()[0].charge_formula.as_deref(),
        Some("1"),
        "later edits keep the retained formula"
    );
}

#[test]
fn density_controls_do_not_trigger_field_sampling() {
    let ctx = context();
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Sources);
    let mut spec = density_spec("ac wire", DensityFamily::SteadyCurrent, "0,0,1");
    harmonic(&mut spec, 1.0, 0.0);
    spec.charge_formula = Some("0".to_string());
    state
        .commit_densities(vec![spec])
        .expect("the fixture is valid");
    let before_densities = state.densities().to_vec();
    let base = state.density_report_generation();

    let mut panel = SourcesPanelState::default();
    open_custom_densities(&ctx, 1200.0, &mut state, &mut panel, false);
    let painted = nodes(&paint_panel(&ctx, 1200.0, &mut state, &mut panel, false, 5));
    assert_eq!(
        state.density_report_generation(),
        base,
        "painting the controls never samples the reports"
    );
    assert_eq!(state.densities(), before_densities.as_slice());

    // An in-flight omega drag is a control gesture, not a sample.
    let rect = drag_value_for(&painted, "Omega (rad/s)")
        .expect("the harmonic law paints an omega drag value");
    let start = rect.center();
    let _ = show_sources_frame_events(
        &ctx,
        1200.0,
        &mut state,
        &mut panel,
        false,
        vec![egui::Event::PointerMoved(start), pointer(start, true)],
    );
    let _ = show_sources_frame_events(
        &ctx,
        1200.0,
        &mut state,
        &mut panel,
        false,
        vec![egui::Event::PointerMoved(start + vec2(40.0, 0.0))],
    );
    assert_eq!(
        state.density_report_generation(),
        base,
        "a drag in flight never samples"
    );
    assert_eq!(state.densities(), before_densities.as_slice());
    assert!(state.density_reports().is_empty());
}

#[test]
fn harmonic_mirrors_survive_a_steady_round_trip() {
    let ctx = context();
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Sources);
    let mut spec = density_spec("ac wire", DensityFamily::SteadyCurrent, "0,0,1");
    harmonic(&mut spec, 2.5, 0.75);
    spec.charge_formula = Some("0".to_string());
    state
        .commit_densities(vec![spec])
        .expect("the harmonic fixture is valid");
    let mut panel = SourcesPanelState::default();
    open_custom_densities(&ctx, 1200.0, &mut state, &mut panel, false);
    assert_eq!(panel.density_drafts[0].omega, 2.5);
    assert_eq!(panel.density_drafts[0].phase, 0.75);

    select_combo_option(
        &ctx, 1200.0, &mut state, &mut panel, false, "Time law", "Steady",
    );
    assert_eq!(state.densities()[0].time_law, TimeLaw::Steady);
    assert_eq!(
        panel.density_drafts[0].omega, 2.5,
        "a committed steady law keeps the draft's omega mirror"
    );
    assert_eq!(
        panel.density_drafts[0].phase, 0.75,
        "a committed steady law keeps the draft's phase mirror"
    );

    select_combo_option(
        &ctx, 1200.0, &mut state, &mut panel, false, "Time law", "Harmonic",
    );
    assert_eq!(
        state.densities()[0].time_law,
        TimeLaw::Harmonic {
            omega: 2.5,
            phase: 0.75
        },
        "re-enabling harmonic restores the user's values"
    );
}

#[test]
fn continuity_estimate_never_prints_non_finite_extremes() {
    let row = DensityReportRow {
        spec_index: 0,
        charge: None,
        continuity: Some(ContinuityReport {
            div_j_min: f64::NAN,
            div_j_max: 0.0,
            worst_point: None,
            status: ContinuityStatus::Estimate {
                value: 1.0,
                error: 0.0,
            },
        }),
    };
    let lines = sources_panel::density_report_lines(&row);
    assert!(
        lines.iter().all(|line| !line.contains("NaN")),
        "every formatted continuity field must be finite: {lines:?}"
    );
    assert!(
        lines[0].starts_with("Unresolved: "),
        "a non-finite estimate is reported honestly: {lines:?}"
    );
}

#[test]
fn a_text_edit_and_an_in_flight_omega_drag_commit_both_in_one_frame() {
    let ctx = context();
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Sources);
    let mut spec = density_spec("ac wire", DensityFamily::SteadyCurrent, "0,0,1");
    harmonic(&mut spec, 1.0, 0.0);
    spec.charge_formula = Some("0".to_string());
    state
        .commit_densities(vec![spec])
        .expect("the harmonic fixture is valid");
    let mut panel = SourcesPanelState::default();
    open_custom_densities(&ctx, 1200.0, &mut state, &mut panel, false);

    let painted = nodes(&paint_panel(&ctx, 1200.0, &mut state, &mut panel, false, 1));
    let formula = text_inputs_for(&painted, "Formula")
        .into_iter()
        .next_back()
        .expect("the density row paints a formula text field");
    click_sources(
        &ctx,
        1200.0,
        formula.center(),
        &mut state,
        &mut panel,
        false,
    );

    let painted = nodes(&paint_panel(&ctx, 1200.0, &mut state, &mut panel, false, 1));
    let start = drag_value_for(&painted, "Omega (rad/s)")
        .expect("the harmonic law paints an omega drag value")
        .center();
    let end = start + vec2(40.0, 0.0);

    let _ = show_sources_frame_events(
        &ctx,
        1200.0,
        &mut state,
        &mut panel,
        false,
        vec![egui::Event::PointerMoved(start), pointer(start, true)],
    );
    let _ = show_sources_frame_events(
        &ctx,
        1200.0,
        &mut state,
        &mut panel,
        false,
        vec![egui::Event::PointerMoved(start + vec2(20.0, 0.0))],
    );
    let _ = show_sources_frame_events(
        &ctx,
        1200.0,
        &mut state,
        &mut panel,
        false,
        vec![egui::Event::PointerMoved(end)],
    );
    assert_eq!(
        state.densities()[0].time_law,
        TimeLaw::Harmonic {
            omega: 1.0,
            phase: 0.0
        }
    );

    let _ = show_sources_frame_events(
        &ctx,
        1200.0,
        &mut state,
        &mut panel,
        false,
        vec![pointer(end, false), egui::Event::Text("2".to_owned())],
    );
    let _ = paint_panel(&ctx, 1200.0, &mut state, &mut panel, false, 1);

    let TimeLaw::Harmonic { omega, .. } = state.densities()[0].time_law else {
        panic!("the row stays harmonic");
    };
    assert_ne!(
        state.densities()[0].formula,
        "0,0,1",
        "the text edit reaches the committed spec"
    );
    assert_ne!(omega, 1.0, "the dragged omega reaches the committed spec");
}
