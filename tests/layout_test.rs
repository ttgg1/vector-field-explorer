// Moved out of `src/ui/layout.rs`'s inline module (ticket-14 review I-4).
// The persisted layout record, reset/toggle behaviour and width clamping.

use vector_field_explorer::ui::layout::{
    DEFAULT_CONTROLS_WIDTH, DEFAULT_PRESETS_WIDTH, LayoutState, MAX_PANEL_WIDTH, MIN_PANEL_WIDTH,
    RAIL_WIDTH, clamp_width,
};

// The rail must stay narrower than any expanded panel.
const _: () = assert!(RAIL_WIDTH < MIN_PANEL_WIDTH);

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
