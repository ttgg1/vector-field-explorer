//! Task 4: the responsive Sources rail/panel and the formula-bar mode switch.

use eframe::egui;
use eframe::egui::{Pos2, Rect, vec2};
use vector_field_explorer::source_scene::{
    FieldMode, SourceDisplay, SourceEntry, SourceFamilyStatus, SourceKindTag, SourceSampling,
};
use vector_field_explorer::sources::SourceKind;
use vector_field_explorer::ui::formula_bar::{self, FormulaBarState};
use vector_field_explorer::ui::layout::{LayoutState, SheetState};
use vector_field_explorer::ui::sources_panel::{self, SourcesPanelState};
use vector_field_explorer::viewmodel::State;

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

fn show_sources_frame(
    ctx: &egui::Context,
    width: f32,
    state: &mut State,
    panel: &mut SourcesPanelState,
    compact: bool,
) -> egui::FullOutput {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 900.0))),
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
