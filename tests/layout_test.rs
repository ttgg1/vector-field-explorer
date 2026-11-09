// Moved out of `src/ui/layout.rs`'s inline module (ticket-14 review I-4).
// The persisted layout record, reset/toggle behaviour and width clamping.
// Ticket 22 adds the rotated rail-label seam: angles, placement, chevron and
// the unchanged click/tooltip contract.

use eframe::egui;

mod support;

use vector_field_explorer::ui::layout::{
    DEFAULT_CONTROLS_WIDTH, DEFAULT_PRESETS_WIDTH, LayoutState, MAX_PANEL_WIDTH, MIN_PANEL_WIDTH,
    RAIL_CHEVRON_ZONE, RAIL_FRAME_MARGIN, RAIL_LABEL_SIZE, RAIL_WIDTH, RailLabel, RailSide,
    clamp_width, collapse_chevron, rail_chevron, rail_chevron_pos, rail_text_angle, rail_tooltip,
};

// The rail must stay narrower than any expanded panel.
const _: () = assert!(RAIL_WIDTH < MIN_PANEL_WIDTH);
// Ticket 22: the rotated label must be readable and fit its rail. The real
// requirement is the content band, not the outer width: the frame takes
// RAIL_FRAME_MARGIN each side and the panel reserves ~1 pt for its separator.
// egui's proportional line height is ~1.15× the font size (measured: 16 pt at
// 14 pt type), so the band must clear that, not merely the 26 pt outer width
// the first pass codified.
const _: () = assert!(RAIL_LABEL_SIZE >= 13.0);
const _: () = assert!(
    RAIL_WIDTH - 2.0 * RAIL_FRAME_MARGIN as f32 >= RAIL_LABEL_SIZE * 1.15
);

#[test]
fn first_launch_expands_both_panels_and_shows_help() {
    let layout = LayoutState::default();
    assert!(layout.controls_expanded, "the Controls panel starts expanded");
    assert!(layout.presets_expanded, "the Presets panel starts expanded");
    assert!(layout.help_visible, "the help overlay is visible on first run");
    assert_eq!(layout.controls_width, DEFAULT_CONTROLS_WIDTH);
    assert_eq!(layout.presets_width, DEFAULT_PRESETS_WIDTH);
    let rail = RAIL_WIDTH;
    assert!((16.0..=40.0).contains(&rail), "the rail is a slim strip: {rail}");
}

#[test]
fn toggles_flip_visibility_and_back() {
    let mut layout = LayoutState::default();
    layout.toggle_controls();
    assert!(!layout.controls_expanded, "one click collapses the Controls panel");
    layout.toggle_controls();
    assert!(layout.controls_expanded, "a second click pins it open again");
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
    };
    layout.reset();
    assert_eq!(layout, LayoutState::default(), "reset layout returns every knob");
    assert!(layout.controls_expanded, "reset re-expands rather than hiding");
}

#[test]
fn widths_clamp_and_corrupt_widths_fall_back() {
    assert_eq!(clamp_width(10.0, 250.0), MIN_PANEL_WIDTH);
    assert_eq!(clamp_width(10_000.0, 250.0), MAX_PANEL_WIDTH);
    assert_eq!(clamp_width(212.5, 250.0), 212.5, "a sane width passes through");
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(clamp_width(bad, 250.0), 250.0, "{bad} falls back to the default");
    }
    let corrupt = LayoutState {
        controls_width: f32::NAN,
        presets_width: 5000.0,
        ..Default::default()
    }
    .sanitized();
    assert_eq!(corrupt.controls_width, DEFAULT_CONTROLS_WIDTH);
    assert_eq!(corrupt.presets_width, MAX_PANEL_WIDTH);
    assert!(corrupt.controls_expanded && corrupt.presets_expanded, "visibility survives");
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
) -> (vector_field_explorer::ui::layout::RailPanel, Vec<egui::epaint::TextShape>) {
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
    (panel.expect("the pass shows the panel"), text)
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
    assert!((left.abs() - std::f32::consts::FRAC_PI_2).abs() < 1e-6, "{left}");
    assert!((right.abs() - std::f32::consts::FRAC_PI_2).abs() < 1e-6, "{right}");
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
        let (panel, _) = rail_pass(&ctx, side, "Controls", None, 0.0);
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
        let (panel, _) = rail_pass(&ctx, side, "Controls", None, 0.0);
        let band = panel.content_rect;
        for label in ["Controls", "Presets"] {
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
        let (panel, text) = rail_pass(&ctx, side, "Controls", None, 0.0);
        let band = panel.content_rect;
        let pos = rail_chevron_pos(band);
        assert_eq!(pos.x, band.center().x, "the chevron is horizontally centred");
        assert!(pos.y > band.center().y, "the chevron sits in the lower half");
        assert!(band.contains(pos), "the chevron stays inside the band: {pos:?}");
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
        let label = RailLabel::place(band, galley.mesh_bounds, side)
            .visual_bounds(galley.mesh_bounds);
        assert!(
            label.max.y < pos.y - RAIL_LABEL_SIZE / 2.0,
            "label bottom {} overlaps the chevron at {}",
            label.max.y,
            pos.y
        );
    }
}

#[test]
fn rail_tooltip_is_unchanged() {
    assert_eq!(rail_tooltip("Controls", "Ctrl+B"), "Show Controls (Ctrl+B)");
    assert_eq!(rail_tooltip("Presets", "Ctrl+P"), "Show Presets (Ctrl+P)");
}

#[test]
fn rail_paints_one_rotated_label_inside_the_real_band() {
    let ctx = support::context();
    for side in [RailSide::Left, RailSide::Right] {
        for label in ["Controls", "Presets"] {
            let (panel, text) = rail_pass(&ctx, side, label, None, 0.0);
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
        let (panel, text) = rail_pass(&ctx, side, "Controls", None, 0.0);
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
        let (_, _) = rail_pass(&ctx, side, "Controls", Some((target, None)), 0.1);
        let mut tooltip_seen = false;
        for time in [0.7, 1.0, 1.3] {
            let (_, shapes) = rail_pass(&ctx, side, "Controls", None, time);
            tooltip_seen |= shapes
                .iter()
                .any(|shape| shape.galley.text().contains("Show Controls (Ctrl+B)"));
        }
        assert!(
            tooltip_seen,
            "hovering the label on {side:?} shows the rail tooltip"
        );

        // Press and release over the same glyph: the click fires on release.
        let (press, _) = rail_pass(&ctx, side, "Controls", Some((target, Some(true))), 1.4);
        let (release, _) = rail_pass(&ctx, side, "Controls", Some((target, Some(false))), 1.5);
        assert!(!press.clicked, "the press alone does not report a click");
        assert!(release.clicked, "the release over the rail pins the panel on {side:?}");
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
        assert!(release, "the release over the chevron collapses on {side:?}");
    }
}
