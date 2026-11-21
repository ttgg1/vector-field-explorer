// Moved out of `src/ui/layout.rs`'s inline module (ticket-14 review I-4).
// The persisted layout record, reset/toggle behaviour and width clamping.
// Ticket 22 adds the rotated rail-label seam: angles, placement, chevron and
// the unchanged click/tooltip contract.

use crate::support::storage::{from_ron, ron_string};
use eframe::egui;

use crate::support;

use vector_field_explorer::ui::layout::{
    DEFAULT_APPEARANCE_WIDTH, DEFAULT_CONTROLS_WIDTH, DEFAULT_INDUCTION_WIDTH,
    DEFAULT_INTERACTION_WIDTH, DEFAULT_PRESETS_WIDTH, DEFAULT_SOURCES_WIDTH, LayoutMode,
    LayoutState, MAX_PANEL_WIDTH, MIN_PANEL_WIDTH, RAIL_CHEVRON_ZONE, RAIL_FRAME_MARGIN,
    RAIL_LABEL_SIZE, RAIL_WIDTH, RailLabel, RailSide, clamp_width, collapse_chevron, compact_plan,
    layout_mode, rail_chevron, rail_chevron_pos, rail_text_angle, rail_tooltip,
};
use vector_field_explorer::ui::panel_rails::{RailEntry, show_rail_strip};

// The rail must stay narrower than any expanded panel.
const _: () = assert!(RAIL_WIDTH < MIN_PANEL_WIDTH);
// Ticket 22: the rotated label must be readable and fit its rail. The real
// requirement is the content band, not the outer width: the frame takes
// RAIL_FRAME_MARGIN each side and the panel reserves ~1 pt for its separator.
// egui's proportional line height is ~1.15× the font size (measured: 16 pt at
// 14 pt type), so the band must clear that, not merely the 26 pt outer width
// the first pass codified.
const _: () = assert!(RAIL_LABEL_SIZE >= 13.0);
const _: () = assert!(RAIL_WIDTH - 2.0 * RAIL_FRAME_MARGIN as f32 >= RAIL_LABEL_SIZE * 1.15);

#[test]
fn first_launch_expands_both_panels_and_shows_help() {
    let layout = LayoutState::default();
    assert!(
        layout.controls_expanded,
        "the Controls panel starts expanded"
    );
    assert!(layout.presets_expanded, "the Presets panel starts expanded");
    assert!(
        layout.help_visible,
        "the help overlay is visible on first run"
    );
    assert_eq!(layout.controls_width, DEFAULT_CONTROLS_WIDTH);
    assert_eq!(layout.presets_width, DEFAULT_PRESETS_WIDTH);
    assert!(
        !layout.appearance_expanded,
        "the Appearance panel starts collapsed to its rail"
    );
    assert_eq!(layout.appearance_width, DEFAULT_APPEARANCE_WIDTH);
    assert!(
        !layout.interaction_expanded,
        "the Interaction panel starts collapsed to its rail"
    );
    assert_eq!(layout.interaction_width, DEFAULT_INTERACTION_WIDTH);
    assert!(
        !layout.sources_expanded,
        "the Sources panel starts collapsed to its rail"
    );
    assert_eq!(layout.sources_width, DEFAULT_SOURCES_WIDTH);
    assert!(
        !layout.induction_expanded,
        "the Induction panel starts collapsed to its rail"
    );
    assert_eq!(layout.induction_width, DEFAULT_INDUCTION_WIDTH);
    let rail = RAIL_WIDTH;
    assert!(
        (16.0..=40.0).contains(&rail),
        "the rail is a slim strip: {rail}"
    );
}

#[test]
fn toggles_flip_visibility_and_back() {
    let mut layout = LayoutState::default();
    layout.toggle_controls();
    assert!(
        !layout.controls_expanded,
        "one click collapses the Controls panel"
    );
    layout.toggle_controls();
    assert!(
        layout.controls_expanded,
        "a second click pins it open again"
    );
    layout.toggle_presets();
    assert!(!layout.presets_expanded);
    layout.toggle_presets();
    assert!(layout.presets_expanded);
    layout.toggle_help();
    assert!(!layout.help_visible, "dismissing help hides the overlay");
    layout.toggle_help();
    assert!(layout.help_visible);
}

#[test]
fn reset_restores_first_launch_layout() {
    let mut layout = LayoutState {
        controls_expanded: false,
        presets_expanded: false,
        controls_width: 400.0,
        presets_width: 350.0,
        help_visible: false,
        ..Default::default()
    };
    layout.reset();
    assert_eq!(
        layout,
        LayoutState::default(),
        "reset layout returns every knob"
    );
    assert!(
        layout.controls_expanded,
        "reset re-expands rather than hiding"
    );
}

#[test]
fn widths_clamp_and_corrupt_widths_fall_back() {
    assert_eq!(clamp_width(10.0, 250.0), MIN_PANEL_WIDTH);
    assert_eq!(clamp_width(10_000.0, 250.0), MAX_PANEL_WIDTH);
    assert_eq!(
        clamp_width(212.5, 250.0),
        212.5,
        "a sane width passes through"
    );
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(
            clamp_width(bad, 250.0),
            250.0,
            "{bad} falls back to the default"
        );
    }
    let corrupt = LayoutState {
        controls_width: f32::NAN,
        presets_width: 5000.0,
        appearance_width: f32::NEG_INFINITY,
        interaction_width: f32::NAN,
        sources_width: 10.0,
        induction_width: f32::NAN,
        maxwell_width: f32::NAN,
        ..Default::default()
    }
    .sanitized();
    assert_eq!(corrupt.controls_width, DEFAULT_CONTROLS_WIDTH);
    assert_eq!(corrupt.presets_width, MAX_PANEL_WIDTH);
    assert_eq!(
        corrupt.appearance_width, DEFAULT_APPEARANCE_WIDTH,
        "a non-finite Appearance width falls back to its default"
    );
    assert_eq!(
        corrupt.interaction_width, DEFAULT_INTERACTION_WIDTH,
        "a non-finite Interaction width falls back to its default"
    );
    assert_eq!(corrupt.sources_width, MIN_PANEL_WIDTH);
    assert_eq!(corrupt.induction_width, DEFAULT_INDUCTION_WIDTH);
    assert_eq!(
        corrupt.maxwell_width,
        vector_field_explorer::ui::layout::DEFAULT_MAXWELL_WIDTH
    );
    assert!(
        corrupt.controls_expanded && corrupt.presets_expanded,
        "visibility survives"
    );
}

// --- Ticket 53: compact mode is transient and never rewrites the record -----

#[test]
fn compact_mode_does_not_mutate_stored_widths() {
    // Computing the mode and the compact plan is read-only: the persisted
    // widths and expanded flags must survive a narrow frame untouched so a
    // return to wide restores the user's panels exactly.
    let layout = LayoutState {
        controls_expanded: false,
        presets_expanded: true,
        controls_width: 300.0,
        presets_width: 210.0,
        help_visible: false,
        ..Default::default()
    };
    let before = layout.clone();

    assert_eq!(layout_mode(360.0), LayoutMode::Compact);
    let plan = compact_plan(LayoutMode::Compact);
    assert!(
        !plan.controls_panel
            && !plan.presets_panel
            && plan.compact_top_bar
            && plan.tab_bar
            && plan.sheet
    );

    assert_eq!(
        layout, before,
        "compact mode reads the record, never writes it"
    );

    // The same record is what wide re-reads after a resize.
    assert_eq!(layout_mode(1200.0), LayoutMode::Wide);
    assert_eq!(layout.controls_width, 300.0);
    assert_eq!(layout.presets_width, 210.0);
    assert!(!layout.controls_expanded && layout.presets_expanded);
}

#[test]
fn layout_state_serializes_exactly_its_platform_persisted_fields() {
    let value = serde_json::to_value(LayoutState::default()).expect("layout serializes");
    let object = value.as_object().expect("layout is a map");
    let keys: std::collections::BTreeSet<&str> = object.keys().map(String::as_str).collect();
    let expected: std::collections::BTreeSet<&str> = [
        "controls_expanded",
        "presets_expanded",
        "appearance_expanded",
        "interaction_expanded",
        "controls_width",
        "presets_width",
        "appearance_width",
        "interaction_width",
        "sources_expanded",
        "sources_width",
        "induction_expanded",
        "induction_width",
        "ode_expanded",
        "ode_width",
        "materials_expanded",
        "materials_width",
        "maxwell_expanded",
        "maxwell_width",
        "help_visible",
        #[cfg(not(target_arch = "wasm32"))]
        "cas_expanded",
        #[cfg(not(target_arch = "wasm32"))]
        "cas_width",
    ]
    .into_iter()
    .collect();
    assert_eq!(keys, expected, "the persisted layout record gained a field");
}

#[test]
fn old_layout_defaults_only_new_fields_and_round_trips() {
    let old = "(controls_expanded:false,presets_expanded:true,controls_width:333.0,presets_width:211.0,help_visible:false)";
    let layout: LayoutState = from_ron(old);
    assert!(!layout.controls_expanded && layout.presets_expanded);
    assert_eq!(
        (layout.controls_width, layout.presets_width),
        (333.0, 211.0)
    );
    assert!(!layout.help_visible);
    assert!(!layout.appearance_expanded);
    assert_eq!(layout.appearance_width, DEFAULT_APPEARANCE_WIDTH);
    assert!(!layout.interaction_expanded);
    assert_eq!(layout.interaction_width, DEFAULT_INTERACTION_WIDTH);
    assert!(!layout.sources_expanded);
    assert_eq!(layout.sources_width, DEFAULT_SOURCES_WIDTH);
    assert!(!layout.induction_expanded);
    assert_eq!(layout.induction_width, DEFAULT_INDUCTION_WIDTH);
    assert!(!layout.materials_expanded);
    assert_eq!(
        layout.materials_width,
        vector_field_explorer::ui::layout::DEFAULT_MATERIALS_WIDTH
    );
    assert!(!layout.maxwell_expanded);
    assert_eq!(
        layout.maxwell_width,
        vector_field_explorer::ui::layout::DEFAULT_MAXWELL_WIDTH
    );
    #[cfg(not(target_arch = "wasm32"))]
    {
        assert!(!layout.cas_expanded);
        assert_eq!(layout.cas_width, 260.0);
    }
    assert_eq!(from_ron::<LayoutState>(&ron_string(&layout)), layout);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn cas_width_sanitizes_toggle_is_independent_and_reset_restores() {
    use vector_field_explorer::ui::layout::DEFAULT_CAS_WIDTH;
    for (width, expected) in [
        (f32::NAN, 260.0),
        (f32::INFINITY, 260.0),
        (f32::NEG_INFINITY, 260.0),
        (0.0, 180.0),
        (999.0, 460.0),
    ] {
        let mut layout = LayoutState {
            cas_width: width,
            ..Default::default()
        }
        .sanitized();
        assert_eq!(layout.cas_width, expected);
        let before = layout.clone();
        layout.toggle_cas();
        assert!(layout.cas_expanded);
        assert_eq!(
            layout,
            LayoutState {
                cas_expanded: true,
                ..before
            }
        );
        layout.toggle_cas();
        assert!(!layout.cas_expanded);
        assert_eq!(layout.cas_width, expected);
        layout.cas_expanded = true;
        layout.reset();
        assert_eq!(layout, LayoutState::default());
        assert_eq!(layout.cas_width, DEFAULT_CAS_WIDTH);
    }
    let layout = LayoutState {
        cas_expanded: true,
        cas_width: 333.0,
        ..Default::default()
    };
    assert_eq!(from_ron::<LayoutState>(&ron_string(&layout)), layout);
}

// --- Ticket 22: rotated rail label -----------------------------------------

/// The narrowest rail height the app can show: the 600 pt minimum window
/// height minus the menu and formula bars, rounded down. The test panel is
/// therefore shorter than the real minimum-height rail — a conservative bound.
const MIN_RAIL_HEIGHT: f32 = 500.0;

/// The parent Ui's rect the headless panel passes run in: minimum window width
/// and the conservative rail height above.
fn screen_rect() -> egui::Rect {
    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(960.0, MIN_RAIL_HEIGHT))
}

/// Lay out `label` at the rail's real type size and return the galley, so the
/// tests assert against the font metrics the rail actually paints instead of a
/// synthetic worst-case rectangle.
fn measure_galley(label: &str) -> std::sync::Arc<egui::Galley> {
    let ctx = support::context();
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 400.0));
    let mut galley = None;
    let mut output = ctx.run_pass(
        egui::RawInput {
            screen_rect: Some(rect),
            ..Default::default()
        },
        |ctx| {
            let ui = egui::Ui::new(
                ctx.clone(),
                egui::Id::unique("measure_galley"),
                egui::UiBuilder::new().max_rect(rect),
            );
            galley = Some(ui.painter().layout_no_wrap(
                label.to_owned(),
                egui::FontId::proportional(RAIL_LABEL_SIZE),
                egui::Color32::WHITE,
            ));
        },
    );
    output.textures_delta.clear();
    galley.expect("the pass lays the label out")
}

/// Run one headless pass that shows the rail through the app's own
/// [`rail_panel`] seam, optionally moving the pointer and pressing/releasing
/// the primary button over it. Returns the panel geometry and every text run
/// painted.
///
/// The panel configuration lives in `rail_panel`, which the app calls too, so
/// these tests measure the shipped outer size and frame instead of a synthetic
/// rectangle.
fn rail_pass(
    ctx: &egui::Context,
    side: RailSide,
    label: &str,
    pointer: Option<(egui::Pos2, Option<bool>)>,
    time: f64,
) -> (
    vector_field_explorer::ui::layout::RailPanel,
    Vec<egui::epaint::TextShape>,
    Vec<egui::epaint::RectShape>,
) {
    let screen = screen_rect();
    let mut input = egui::RawInput {
        screen_rect: Some(screen),
        time: Some(time),
        ..Default::default()
    };
    if let Some((pos, press)) = pointer {
        input.events.push(egui::Event::PointerMoved(pos));
        if let Some(pressed) = press {
            input.events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::default(),
            });
        }
    }
    let mut panel = None;
    let mut output = ctx.run_pass(input, |ctx| {
        let mut ui = egui::Ui::new(
            ctx.clone(),
            egui::Id::unique("rail_panel_test"),
            egui::UiBuilder::new().max_rect(screen),
        );
        panel = Some(vector_field_explorer::ui::layout::rail_panel(
            &mut ui,
            "rail_panel",
            side,
            label,
            "Ctrl+B",
        ));
    });
    // The pass paints text and therefore grows the font atlas; clear the delta
    // because the context is discarded or reused without a real backend.
    output.textures_delta.clear();
    let text = output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::Text(shape) => Some(shape.clone()),
            _ => None,
        })
        .collect();
    let rects = output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::Rect(shape) => Some(shape.clone()),
            _ => None,
        })
        .collect();
    (panel.expect("the pass shows the panel"), text, rects)
}

/// Run one headless pass that shows a single-entry strip through the app's
/// [`show_rail_strip`] seam. Returns the entry's clickable rect and the text
/// and rectangle shapes the pass painted, so its tile can be compared with the
/// right rail's entry directly.
fn strip_entry_pass(
    ctx: &egui::Context,
    side: RailSide,
    label: &'static str,
) -> (
    egui::Rect,
    Vec<egui::epaint::TextShape>,
    Vec<egui::epaint::RectShape>,
) {
    let screen = screen_rect();
    let mut expanded = false;
    let mut entry_rect = egui::Rect::NOTHING;
    let mut output = ctx.run_pass(
        egui::RawInput {
            screen_rect: Some(screen),
            time: Some(0.0),
            ..Default::default()
        },
        |ctx| {
            let mut ui = egui::Ui::new(
                ctx.clone(),
                egui::Id::unique("strip_test"),
                egui::UiBuilder::new().max_rect(screen),
            );
            let mut entries = [RailEntry {
                id: "entry",
                label,
                shortcut: Some("Ctrl+B"),
                expanded: &mut expanded,
            }];
            let strip = show_rail_strip(&mut ui, "strip", side, &mut entries);
            entry_rect = strip.entries[0].response.rect;
        },
    );
    output.textures_delta.clear();
    let text = output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::Text(shape) => Some(shape.clone()),
            _ => None,
        })
        .collect();
    let rects = output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::Rect(shape) => Some(shape.clone()),
            _ => None,
        })
        .collect();
    (entry_rect, text, rects)
}

fn label_font_size(shapes: &[egui::epaint::TextShape], label: &str) -> f32 {
    shapes
        .iter()
        .find(|shape| shape.galley.text() == label)
        .unwrap_or_else(|| panic!("no painted {label:?} label"))
        .galley
        .job
        .sections[0]
        .format
        .font_id
        .size
}

#[test]
fn right_rail_entry_matches_a_left_strip_entry() {
    // One left strip entry and one right rail panel at the same style and
    // label must paint the same tile: the right rail is an entry like any
    // other, not one full-height strip.
    let left_ctx = support::context();
    let right_ctx = support::context();
    for ctx in [&left_ctx, &right_ctx] {
        vector_field_explorer::ui::style_panel::apply(
            ctx,
            &vector_field_explorer::ui::style_panel::AppStyle {
                rounding: 6.0,
                ..Default::default()
            },
        );
    }

    let (left_rect, left_text, left_rects) =
        strip_entry_pass(&left_ctx, RailSide::Left, "Controls");
    let (panel, right_text, right_rects) =
        rail_pass(&right_ctx, RailSide::Right, "Controls", None, 0.0);

    let left_tile = left_rects
        .iter()
        .find(|shape| shape.rect == left_rect)
        .expect("the left strip paints its entry tile");
    let right_tile = right_rects
        .iter()
        .find(|shape| shape.rect == panel.content_rect)
        .unwrap_or_else(|| {
            panic!(
                "the right rail paints no tile at its content rect {:?}",
                panel.content_rect
            )
        });

    assert!(
        (left_tile.rect.height() - right_tile.rect.height()).abs() < 0.5,
        "right rail tile is {} tall, left strip tile is {} tall",
        right_tile.rect.height(),
        left_tile.rect.height()
    );
    assert_eq!(
        left_tile.fill, right_tile.fill,
        "the two rails must share the interact-selectable fill"
    );
    assert_eq!(
        left_tile.corner_radius, right_tile.corner_radius,
        "the two rails must share the configured corner radius"
    );
    assert_eq!(
        label_font_size(&left_text, "Controls"),
        label_font_size(&right_text, "Controls"),
        "the two rails must share the rotated label type size"
    );
    assert_eq!(
        label_font_size(&right_text, "Controls"),
        RAIL_LABEL_SIZE,
        "the right rail label keeps the rail type size"
    );
}

#[test]
fn right_rail_entry_mirrors_label_angle_and_chevron() {
    let ctx = support::context();
    for label in ["Presets", "Appearance", "Interaction"] {
        let (panel, text, _) = rail_pass(&ctx, RailSide::Right, label, None, 0.0);
        let placed = text
            .iter()
            .find(|shape| shape.galley.text() == label)
            .unwrap_or_else(|| panic!("the right rail paints {label}"));
        assert!(
            (placed.angle - rail_text_angle(RailSide::Right)).abs() < 1e-6,
            "{label} is rotated to {:?}, expected {:?}",
            placed.angle,
            rail_text_angle(RailSide::Right)
        );
        let chevron = text
            .iter()
            .find(|shape| shape.galley.text() == rail_chevron(RailSide::Right))
            .unwrap_or_else(|| {
                panic!("{label}: the collapsed right rail paints its outward chevron")
            });
        assert!(
            !text
                .iter()
                .any(|shape| shape.galley.text() == collapse_chevron(RailSide::Right)),
            "{label}: a collapsed rail must not paint the collapse chevron"
        );

        // The chevron is part of the painted entry tile, not stranded at the
        // bottom of a full-height panel. Containment alone would also hold for
        // the old full-strip rail (whose content rect was the whole band), so
        // also require the chevron ink to sit clear of the panel's bottom edge.
        let ink = chevron.visual_bounding_rect();
        assert!(
            panel.content_rect.contains_rect(ink),
            "{label}: chevron ink {ink:?} escapes the entry tile {:?}",
            panel.content_rect
        );
        assert!(
            ink.max.y < panel.outer_rect.bottom() - RAIL_CHEVRON_ZONE,
            "{label}: chevron ink ends at {} on the panel bottom edge, not inside the tile",
            ink.max.y
        );
    }
}

#[test]
fn right_rail_click_target_covers_the_tile() {
    let ctx = support::context();
    let galley = measure_galley("Controls");
    let expected_height = (galley.mesh_bounds.width() + RAIL_CHEVRON_ZONE + 8.0).max(40.0);
    let (panel, _, rects) = rail_pass(&ctx, RailSide::Right, "Controls", None, 0.0);
    let tile = rects
        .iter()
        .find(|shape| shape.rect == panel.content_rect)
        .unwrap_or_else(|| {
            panic!(
                "the right rail paints no tile at its click target {:?}",
                panel.content_rect
            )
        });
    assert!(
        (tile.rect.height() - expected_height).abs() < 0.5,
        "the right rail tile is {} tall, a rail entry is {expected_height}",
        tile.rect.height()
    );

    // The label's own click still pins the panel.
    let target = tile.rect.center();
    let (_, _, _) = rail_pass(&ctx, RailSide::Right, "Controls", Some((target, None)), 0.1);
    let (press, _, _) = rail_pass(
        &ctx,
        RailSide::Right,
        "Controls",
        Some((target, Some(true))),
        0.2,
    );
    let (release, _, _) = rail_pass(
        &ctx,
        RailSide::Right,
        "Controls",
        Some((target, Some(false))),
        0.3,
    );
    assert!(!press.clicked, "the press alone does not report a click");
    assert!(release.clicked, "the release over the tile pins the panel");

    // A point below the tile is outside the click target even though it is
    // inside the panel's full-height content band.
    let below = egui::pos2(target.x, tile.rect.bottom() + 60.0);
    let ctx = support::context();
    let (panel, _, _) = rail_pass(&ctx, RailSide::Right, "Controls", None, 0.0);
    assert!(
        below.y < panel.outer_rect.bottom(),
        "the below-tile probe must stay inside the panel"
    );
    let (_, _, _) = rail_pass(&ctx, RailSide::Right, "Controls", Some((below, None)), 0.1);
    let (press, _, _) = rail_pass(
        &ctx,
        RailSide::Right,
        "Controls",
        Some((below, Some(true))),
        0.2,
    );
    let (release, _, _) = rail_pass(
        &ctx,
        RailSide::Right,
        "Controls",
        Some((below, Some(false))),
        0.3,
    );
    assert!(!press.clicked, "pressing below the tile does not click it");
    assert!(
        !release.clicked,
        "the click target stops at the tile, not the whole panel height"
    );
}

#[test]
fn rail_panel_paints_the_configured_corner_radius() {
    for (rounding, expected) in [
        (6.0, egui::CornerRadius::same(6)),
        (0.0, egui::CornerRadius::ZERO),
    ] {
        for side in [RailSide::Left, RailSide::Right] {
            let ctx = support::context();
            vector_field_explorer::ui::style_panel::apply(
                &ctx,
                &vector_field_explorer::ui::style_panel::AppStyle {
                    rounding,
                    ..Default::default()
                },
            );
            let (panel, _, rects) = rail_pass(&ctx, side, "Controls", None, 0.0);
            let shape = rects
                .iter()
                .find(|shape| shape.rect == panel.content_rect)
                .unwrap_or_else(|| {
                    panic!(
                        "{side:?}: no rail background rect painted at {:?}",
                        panel.content_rect
                    )
                });
            assert_eq!(
                shape.corner_radius, expected,
                "{side:?}: rounding {rounding} must reach the rail panel"
            );
        }
    }
}

#[test]
fn rail_angles_are_vertical_with_opposite_signs() {
    let left = rail_text_angle(RailSide::Left);
    let right = rail_text_angle(RailSide::Right);

    // TextShape angles are clockwise on screen (epaint/emath convention), so a
    // negative angle turns the baseline upward (bottom-to-top reading) and a
    // positive angle turns it downward (top-to-bottom reading).
    assert!(left < 0.0, "left rail reads bottom-to-top: {left}");
    assert!(right > 0.0, "right rail reads top-to-bottom: {right}");

    // Each label is exactly 90 degrees from horizontal text; the pair is a
    // vertical mirror (180 degrees apart) because the reading directions are
    // opposite. This is the documented convention from ticket 22.
    assert!(
        (left.abs() - std::f32::consts::FRAC_PI_2).abs() < 1e-6,
        "{left}"
    );
    assert!(
        (right.abs() - std::f32::consts::FRAC_PI_2).abs() < 1e-6,
        "{right}"
    );
    assert!(
        ((right - left).abs() - std::f32::consts::PI).abs() < 1e-6,
        "left and right are mirror rotations: {left} vs {right}"
    );
}

#[test]
fn rail_panel_is_outer_width_and_the_label_band_is_narrower() {
    let ctx = support::context();
    let line_height = measure_galley("Controls").size().y;
    for side in [RailSide::Left, RailSide::Right] {
        let (panel, _, _) = rail_pass(&ctx, side, "Controls", None, 0.0);
        assert!(
            (panel.outer_rect.width() - RAIL_WIDTH).abs() < 0.5,
            "{side:?} outer panel is {} wide, expected RAIL_WIDTH {RAIL_WIDTH}",
            panel.outer_rect.width()
        );
        assert!(
            panel.content_rect.width() < RAIL_WIDTH,
            "{side:?} content band {} must be narrower than the outer width {RAIL_WIDTH}",
            panel.content_rect.width()
        );
        // The real requirement: the band clears the rotated label's line box,
        // not merely its tight ink. The old default frame left 15 pt, less than
        // the measured 16 pt line box — the bug this ticket-22 review found.
        assert!(
            panel.content_rect.width() >= line_height,
            "{side:?} band is {} wide, the 14 pt line box is {line_height} tall",
            panel.content_rect.width()
        );
        assert!(
            panel.outer_rect.contains_rect(panel.content_rect),
            "{side:?} content band escapes its panel: {:?} vs {:?}",
            panel.content_rect,
            panel.outer_rect
        );
    }
}

#[test]
fn rotated_labels_fit_inside_the_real_rail_band() {
    let ctx = support::context();
    for side in [RailSide::Left, RailSide::Right] {
        for label in ["Controls", "Presets", "Appearance"] {
            // Each rail entry is now sized to its own label, so use that
            // label's panel tile as the band. The old full-height rect made
            // one band fit every label; a single tile no longer pretends to.
            let (panel, _, _) = rail_pass(&ctx, side, label, None, 0.0);
            let band = panel.content_rect;
            let galley = measure_galley(label);
            let ink = galley.mesh_bounds;
            let placed = RailLabel::place(band, ink, side);
            let bounds = placed.visual_bounds(ink);

            assert!(
                band.contains_rect(bounds),
                "{label} ink on {side:?} escapes the real band: {bounds:?} not inside {band:?}"
            );
            assert!(
                bounds.width() <= band.width(),
                "{label} ink on {side:?} is {} wide, band is {}",
                bounds.width(),
                band.width()
            );
            assert!(
                bounds.height() <= band.height(),
                "{label} ink on {side:?} is {} tall, band is {}",
                bounds.height(),
                band.height()
            );
        }
    }
}

#[test]
fn chevron_stays_at_the_bottom_pointing_outward() {
    // Left rail expands rightwards, right rail expands leftwards.
    assert_eq!(rail_chevron(RailSide::Left), "›");
    assert_eq!(rail_chevron(RailSide::Right), "‹");
    // The collapse buttons point the other way, back toward the panel edge.
    assert_eq!(collapse_chevron(RailSide::Left), "‹");
    assert_eq!(collapse_chevron(RailSide::Right), "›");

    let ctx = support::context();
    for side in [RailSide::Left, RailSide::Right] {
        let (panel, text, _) = rail_pass(&ctx, side, "Controls", None, 0.0);
        let band = panel.content_rect;
        let pos = rail_chevron_pos(band);
        assert_eq!(
            pos.x,
            band.center().x,
            "the chevron is horizontally centred"
        );
        assert!(
            pos.y > band.center().y,
            "the chevron sits in the lower half"
        );
        assert!(
            band.contains(pos),
            "the chevron stays inside the band: {pos:?}"
        );
        assert!(
            pos.y > band.bottom() - RAIL_CHEVRON_ZONE,
            "the chevron sits in the reserved bottom zone: {pos:?}"
        );

        // The chevron's painted ink also stays inside the clickable band.
        let chevron = text
            .iter()
            .find(|shape| shape.galley.text() == rail_chevron(side))
            .expect("the rail paints the chevron");
        assert!(
            band.contains_rect(chevron.visual_bounding_rect()),
            "{side:?} chevron ink escapes the band: {:?}",
            chevron.visual_bounding_rect()
        );

        // The label band ends above the chevron so the two never overlap.
        let galley = measure_galley("Controls");
        let label =
            RailLabel::place(band, galley.mesh_bounds, side).visual_bounds(galley.mesh_bounds);
        assert!(
            label.max.y < pos.y - RAIL_LABEL_SIZE / 2.0,
            "label bottom {} overlaps the chevron at {}",
            label.max.y,
            pos.y
        );
    }
}

#[test]
fn rail_tooltip_matches_the_entry_state() {
    assert_eq!(
        rail_tooltip("Controls", Some("Ctrl+B"), false),
        "Show Controls (Ctrl+B)"
    );
    assert_eq!(
        rail_tooltip("Presets", Some("Ctrl+P"), false),
        "Show Presets (Ctrl+P)"
    );
    assert_eq!(
        rail_tooltip("Controls", Some("Ctrl+B"), true),
        "Hide Controls (Ctrl+B)"
    );
    assert_eq!(
        rail_tooltip("Sources", None, false),
        "Show Sources",
        "a shortcut-less rail omits the parenthesised shortcut"
    );
}

#[test]
fn rail_paints_one_rotated_label_inside_the_real_band() {
    let ctx = support::context();
    for side in [RailSide::Left, RailSide::Right] {
        for label in ["Controls", "Presets", "Appearance"] {
            let (panel, text, _) = rail_pass(&ctx, side, label, None, 0.0);
            let labels: Vec<_> = text
                .iter()
                .filter(|shape| shape.galley.text() == label)
                .collect();
            assert_eq!(labels.len(), 1, "one label run per rail, got {labels:?}");
            assert!(
                (labels[0].angle - rail_text_angle(side)).abs() < 1e-6,
                "label on {side:?} is rotated to {:?}",
                labels[0].angle
            );
            assert_ne!(labels[0].angle, 0.0, "the label must not be upright");

            // The painted 14 pt ink — not the line box — must lie inside the
            // real content band the panel produced.
            let ink = labels[0].visual_bounding_rect();
            assert!(
                panel.content_rect.contains_rect(ink),
                "{side:?} {label} ink escapes the real band: {ink:?} vs {:?}",
                panel.content_rect
            );

            // No per-character text runs survive: every other run is the
            // chevron.
            for shape in &text {
                let run = shape.galley.text();
                assert!(
                    run == label || run == rail_chevron(side),
                    "unexpected text run {run:?}"
                );
            }
        }
    }
}

#[test]
fn rail_click_target_covers_the_label_and_pins() {
    // Hover, press, release: egui's press detection uses the previous frame's
    // hover, and the click fires on release over the same widget.
    for side in [RailSide::Left, RailSide::Right] {
        let ctx = support::context();
        // One paint pass gives the label's painted ink, so the click can target
        // the glyph itself rather than the panel's geometric centre.
        let (panel, text, _) = rail_pass(&ctx, side, "Controls", None, 0.0);
        let ink = text
            .iter()
            .find(|shape| shape.galley.text() == "Controls")
            .expect("the rail paints the label")
            .visual_bounding_rect();
        assert!(
            panel.content_rect.contains_rect(ink),
            "the clickable band must cover the label ink: {ink:?} vs {:?}",
            panel.content_rect
        );
        let target = ink.center();

        // Move the pointer onto the glyph once, then leave it still: egui only
        // shows a tooltip after 0.5 s without pointer movement.
        let (_, _, _) = rail_pass(&ctx, side, "Controls", Some((target, None)), 0.1);
        let mut tooltip_seen = false;
        for time in [0.7, 1.0, 1.3] {
            let (_, shapes, _) = rail_pass(&ctx, side, "Controls", None, time);
            tooltip_seen |= shapes
                .iter()
                .any(|shape| shape.galley.text().contains("Show Controls (Ctrl+B)"));
        }
        assert!(
            tooltip_seen,
            "hovering the label on {side:?} shows the rail tooltip"
        );

        // Press and release over the same glyph: the click fires on release.
        let (press, _, _) = rail_pass(&ctx, side, "Controls", Some((target, Some(true))), 1.4);
        let (release, _, _) = rail_pass(&ctx, side, "Controls", Some((target, Some(false))), 1.5);
        assert!(!press.clicked, "the press alone does not report a click");
        assert!(
            release.clicked,
            "the release over the rail pins the panel on {side:?}"
        );
    }
}

/// Run one headless pass that paints the expanded panel's collapse button,
/// optionally moving the pointer and pressing/releasing the primary button over
/// it. Returns the button's clicked flag and every text run it painted.
fn collapse_pass(
    ctx: &egui::Context,
    side: RailSide,
    pointer: Option<(egui::Pos2, Option<bool>)>,
    time: f64,
) -> (bool, Vec<egui::epaint::TextShape>) {
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(240.0, 120.0));
    let mut input = egui::RawInput {
        screen_rect: Some(rect),
        time: Some(time),
        ..Default::default()
    };
    if let Some((pos, press)) = pointer {
        input.events.push(egui::Event::PointerMoved(pos));
        if let Some(pressed) = press {
            input.events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::default(),
            });
        }
    }
    let mut clicked = false;
    let mut output = ctx.run_pass(input, |ctx| {
        let mut ui = egui::Ui::new(
            ctx.clone(),
            egui::Id::unique(("collapse_button", matches!(side, RailSide::Left))),
            egui::UiBuilder::new().max_rect(rect),
        );
        clicked = vector_field_explorer::ui::layout::collapse_button(&mut ui, side, "Ctrl+B");
    });
    output.textures_delta.clear();
    let text = output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::Text(shape) => Some(shape.clone()),
            _ => None,
        })
        .collect();
    (clicked, text)
}

#[test]
fn collapse_button_paints_the_inward_chevron_and_reports_a_click() {
    for side in [RailSide::Left, RailSide::Right] {
        let ctx = support::context();
        // One paint pass gives the chevron's painted ink, so the click can
        // target the glyph itself.
        let (clicked, text) = collapse_pass(&ctx, side, None, 0.0);
        assert!(!clicked, "painting the button alone reports no click");
        let glyph = text
            .iter()
            .find(|shape| shape.galley.text() == collapse_chevron(side))
            .expect("the collapse button paints its inward chevron");
        let target = glyph.visual_bounding_rect().center();

        // Hover, press, release over the glyph: the click fires on release.
        let (_, _) = collapse_pass(&ctx, side, Some((target, None)), 0.1);
        let (press, _) = collapse_pass(&ctx, side, Some((target, Some(true))), 0.2);
        let (release, _) = collapse_pass(&ctx, side, Some((target, Some(false))), 0.3);
        assert!(!press, "the press alone does not collapse on {side:?}");
        assert!(
            release,
            "the release over the chevron collapses on {side:?}"
        );
    }
}
