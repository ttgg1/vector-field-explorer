// Moved out of `src/ui/help_overlay.rs`'s inline module (ticket-14 review I-4).

use vector_field_explorer::ui::help_overlay::help_lines;

#[test]
fn two_d_help_lists_pan_zoom_click_and_syntax() {
    let lines = help_lines(false);
    let text = lines.join("\n");
    assert!(text.contains("Drag: pan"), "2D drag pans: {text}");
    assert!(text.contains("Wheel"), "the wheel zooms: {text}");
    assert!(text.contains("Click"), "click seeding is documented: {text}");
    assert!(text.contains("Formula:"), "the syntax reminder is there: {text}");
    assert!(!text.contains("orbit"), "2D never mentions orbiting: {text}");
}

#[test]
fn three_d_help_adds_orbit_and_the_right_drag_pan() {
    let lines = help_lines(true);
    let text = lines.join("\n");
    assert!(text.contains("Drag: orbit"), "3D drag orbits: {text}");
    assert!(text.contains("Right/middle-drag: pan"), "3D right-drag pans: {text}");
    assert!(text.contains("Wheel"), "the wheel zooms: {text}");
    assert!(text.contains("Formula:"), "the syntax reminder is there: {text}");
    assert_eq!(lines.len(), help_lines(false).len() + 1, "3D adds exactly the orbit line");
}

#[test]
fn both_modes_document_the_panel_shortcuts_and_their_text_field_rule() {
    // Ticket 15 moved every shortcut into one table that stays inert while a
    // text field has focus (Ctrl+Q is the only exception), so the help must
    // describe the rule rather than the ticket-14 "even while typing" claim.
    for mode_3d in [false, true] {
        let text = help_lines(mode_3d).join("\n");
        assert!(text.contains("Ctrl+B"), "the Controls shortcut is documented: {text}");
        assert!(text.contains("Ctrl+P"), "the Presets shortcut is documented: {text}");
        assert!(
            text.contains("not while typing"),
            "the help says shortcuts stay out of text fields: {text}",
        );
        assert!(text.contains("Ctrl+Q"), "the quit shortcut is documented: {text}");
        assert!(text.contains("F1"), "the help-overlay shortcut is documented: {text}");
    }
}
