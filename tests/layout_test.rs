// Moved out of `src/ui/layout.rs`'s inline module (ticket-14 review I-4).
// The persisted layout record, reset/toggle behaviour and width clamping.
// Ticket 22 adds the rotated rail-label seam: angles, placement, chevron and
// the unchanged click/tooltip contract.

use eframe::egui;
use vector_field_explorer::ui::layout::{
    DEFAULT_CONTROLS_WIDTH, DEFAULT_PRESETS_WIDTH, LayoutState, MAX_PANEL_WIDTH, MIN_PANEL_WIDTH,
    RAIL_CHEVRON_ZONE, RAIL_LABEL_SIZE, RAIL_WIDTH, RailLabel, RailSide, clamp_width, rail_chevron,
    rail_chevron_pos, rail_text_angle, rail_tooltip,
};

// The rail must stay narrower than any expanded panel.
const _: () = assert!(RAIL_WIDTH < MIN_PANEL_WIDTH);
// Ticket 22: the rotated label must be readable and fit its rail.
const _: () = assert!(RAIL_LABEL_SIZE >= 13.0);
const _: () = assert!(RAIL_WIDTH >= 26.0);

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

/// The narrowest rail height the app can show: 600 pt minimum window height
/// minus the menu, formula bar and panel margins, rounded down.
const MIN_RAIL_HEIGHT: f32 = 500.0;

/// A generous upper bound on a 14 pt label galley: default fonts render
/// "Controls" well under 90 pt wide and 20 pt tall. Using an upper bound keeps
/// the placement test independent of the installed font while still proving
/// the rotated box fits.
fn worst_case_galley(label: &str) -> egui::Vec2 {
    assert!(label.chars().count() <= 8, "labels stay short enough for the rail");
    egui::vec2(90.0, 20.0)
}

fn rail_rect() -> egui::Rect {
    egui::Rect::from_min_size(
        egui::pos2(0.0, 0.0),
        egui::vec2(RAIL_WIDTH, MIN_RAIL_HEIGHT),
    )
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
fn rotated_labels_fit_inside_the_rail_at_minimum_height() {
    let rail = rail_rect();
    for side in [RailSide::Left, RailSide::Right] {
        for label in ["Controls", "Presets"] {
            let galley = worst_case_galley(label);
            let placed = RailLabel::place(rail, galley, side);
            let bounds = placed.visual_bounds(galley);

            assert!(
                bounds.width() <= RAIL_WIDTH,
                "{label} on {side:?} is {} wide, rail is {RAIL_WIDTH}",
                bounds.width()
            );
            assert!(
                bounds.height() <= rail.height(),
                "{label} on {side:?} is {} tall, rail is {}",
                bounds.height(),
                rail.height()
            );
            assert!(
                rail.contains_rect(bounds),
                "{label} on {side:?} escapes the rail: {bounds:?} not inside {rail:?}"
            );
        }
    }
}

#[test]
fn chevron_stays_at_the_bottom_pointing_outward() {
    // Left rail expands rightwards, right rail expands leftwards.
    assert_eq!(rail_chevron(RailSide::Left), "›");
    assert_eq!(rail_chevron(RailSide::Right), "‹");

    let rail = rail_rect();
    let pos = rail_chevron_pos(rail);
    assert_eq!(pos.x, rail.center().x, "the chevron is horizontally centred");
    assert!(pos.y > rail.center().y, "the chevron sits in the lower half");
    assert!(rail.contains(pos), "the chevron stays inside the rail: {pos:?}");
    assert!(
        pos.y > rail.bottom() - RAIL_CHEVRON_ZONE,
        "the chevron sits in the reserved bottom zone: {pos:?}"
    );

    // The label band ends above the chevron so the two never overlap.
    for side in [RailSide::Left, RailSide::Right] {
        let galley = worst_case_galley("Controls");
        let label = RailLabel::place(rail, galley, side).visual_bounds(galley);
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

/// Run one headless egui pass that paints a rail, optionally pressing or
/// releasing the primary button over it. Returns the rail's clicked flag and
/// every text run it painted.
fn run_rail(
    side: RailSide,
    label: &str,
    pointer: Option<(egui::Pos2, bool)>,
) -> (bool, Vec<egui::epaint::TextShape>) {
    let ctx = egui::Context::default();
    let rect = rail_rect();
    let mut input = egui::RawInput {
        screen_rect: Some(rect),
        ..Default::default()
    };
    if let Some((pos, pressed)) = pointer {
        input.events.push(egui::Event::PointerMoved(pos));
        input.events.push(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        });
    }
    let mut clicked = false;
    let mut output = ctx.run_pass(input, |ctx| {
        let mut ui = egui::Ui::new(
            ctx.clone(),
            egui::Id::unique(("rail_test", label)),
            egui::UiBuilder::new().max_rect(rect),
        );
        clicked = vector_field_explorer::ui::layout::rail(&mut ui, side, label, "Ctrl+B");
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
fn rail_paints_one_rotated_label_not_stacked_characters() {
    for side in [RailSide::Left, RailSide::Right] {
        let (_, text) = run_rail(side, "Controls", None);
        let labels: Vec<_> = text
            .iter()
            .filter(|shape| shape.galley.text() == "Controls")
            .collect();
        assert_eq!(labels.len(), 1, "one label run per rail, got {labels:?}");
        assert!(
            (labels[0].angle - rail_text_angle(side)).abs() < 1e-6,
            "label on {side:?} is rotated to {:?}",
            labels[0].angle
        );
        assert_ne!(labels[0].angle, 0.0, "the label must not be upright");
        // The real 14 pt galley must fit the rail at the minimum height too.
        let bounds = labels[0].visual_bounding_rect();
        assert!(
            rail_rect().contains_rect(bounds),
            "{side:?} label escapes the rail at 14 pt: {bounds:?}"
        );
        assert!(
            bounds.width() <= RAIL_WIDTH,
            "{side:?} label is {} wide, rail is {RAIL_WIDTH}",
            bounds.width()
        );
        // No per-character text runs survive: every other run is the chevron.
        for shape in &text {
            let run = shape.galley.text();
            assert!(
                run == "Controls" || run == rail_chevron(side),
                "unexpected text run {run:?}"
            );
        }
    }
}

#[test]
fn rail_click_still_expands_and_pins() {
    // Hover, press, release: egui's press detection uses the previous frame's
    // hover, and the click fires on release over the same widget.
    let rect = rail_rect();
    let ctx = egui::Context::default();
    let mut clicks = Vec::new();
    for step in [None, Some(true), Some(false)] {
        let mut input = egui::RawInput {
            screen_rect: Some(rect),
            ..Default::default()
        };
        input.events.push(egui::Event::PointerMoved(rect.center()));
        if let Some(pressed) = step {
            input.events.push(egui::Event::PointerButton {
                pos: rect.center(),
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::default(),
            });
        }
        let mut clicked = false;
        let mut output = ctx.run_pass(input, |ctx| {
            let mut ui = egui::Ui::new(
                ctx.clone(),
                egui::Id::unique("rail_click_test"),
                egui::UiBuilder::new().max_rect(rect),
            );
            clicked = vector_field_explorer::ui::layout::rail(
                &mut ui,
                RailSide::Left,
                "Controls",
                "Ctrl+B",
            );
        });
        // The pass paints text and therefore grows the font atlas; clear the
        // delta because the context is discarded.
        output.textures_delta.clear();
        clicks.push(clicked);
    }
    assert_eq!(
        clicks,
        [false, false, true],
        "only the release over the rail reports a click"
    );
}
