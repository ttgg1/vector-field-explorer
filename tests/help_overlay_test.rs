// Ticket 18: the help table is one source of strings for the viewport overlay.
// Ticket 28 removed the duplicate Help-menu window, so the overlay is the one
// surface. These tests pin every binding exactly once in its group and reject
// the stale "right-drag pans" copy.

use vector_field_explorer::ui::help_overlay::{
    HELP_SECTIONS, HelpRow, all_rows, rows_for, sections_for,
};

const VIEW_3D_BINDINGS: &[(&str, &str)] = &[
    ("Left-drag", "Orbit"),
    ("Right-drag", "Orbit"),
    ("Middle-drag", "Pan"),
    ("Shift + right-drag", "Pan"),
    ("Wheel / pinch", "Zoom"),
    ("Double-click", "Reset view"),
    ("Arrow keys", "Orbit"),
    ("Shift + arrows", "Pan"),
    ("+ / -", "Zoom in / out"),
    ("R", "Reset view"),
    ("F", "Fit domain"),
    ("Click", "Seed streamlines"),
];

const VIEW_2D_BINDINGS: &[(&str, &str)] = &[
    ("Left / right-drag", "Pan"),
    ("Middle-drag", "Pan"),
    ("Wheel / pinch", "Zoom"),
    ("Double-click", "Reset view"),
    ("Arrow keys", "Pan"),
    ("Shift + arrows", "Pan faster"),
    ("+ / -", "Zoom in / out"),
    ("R", "Reset view"),
    ("F", "Fit domain"),
    ("Click", "Seed streamlines"),
];

const GLOBAL_BINDINGS: &[(&str, &str)] = &[
    ("F1 / ?", "Toggle help"),
    ("Ctrl+B / Ctrl+P", "Toggle panels (not while typing)"),
    ("Ctrl+Shift+S", "Export screenshot"),
    ("Ctrl+Shift+C", "Copy formula"),
    ("Ctrl+Q", "Quit"),
    ("Esc", "Close dialogs / dismiss help"),
];

fn section(title: &str) -> &'static [HelpRow] {
    HELP_SECTIONS
        .iter()
        .find(|section| section.title == title)
        .unwrap_or_else(|| panic!("help group `{title}` is missing"))
        .rows
}

fn assert_lists_once(rows: &[HelpRow], expected: &[(&str, &str)], what: &str) {
    assert_eq!(
        rows.len(),
        expected.len(),
        "{what}: expected exactly {} rows, found {}",
        expected.len(),
        rows.len(),
    );
    for wanted in expected {
        let matches: Vec<&HelpRow> = rows.iter().filter(|row| row.input == wanted.0).collect();
        assert_eq!(
            matches.len(),
            1,
            "{what}: `{}` must be listed exactly once, found {}",
            wanted.0,
            matches.len(),
        );
        assert_eq!(
            matches[0].action, wanted.1,
            "{what}: `{}` must do `{}`",
            wanted.0, wanted.1,
        );
    }
    for (i, first) in rows.iter().enumerate() {
        for second in &rows[i + 1..] {
            assert_ne!(first.input, second.input, "{what}: `{}` is listed twice", first.input);
        }
    }
}

#[test]
fn every_binding_is_listed_exactly_once_in_its_group() {
    assert_lists_once(section("3D view"), VIEW_3D_BINDINGS, "3D view");
    assert_lists_once(section("2D view"), VIEW_2D_BINDINGS, "2D view");
    assert_lists_once(section("Global"), GLOBAL_BINDINGS, "Global");
}

#[test]
fn the_overlay_rows_come_from_one_table() {
    // The overlay renders HELP_SECTIONS; the flattened rows are exactly the
    // section rows, in 3D/2D/global order.
    let titles: Vec<&str> = HELP_SECTIONS.iter().map(|section| section.title).collect();
    assert_eq!(titles, vec!["3D view", "2D view", "Global"]);
    assert_eq!(
        all_rows().len(),
        HELP_SECTIONS.iter().map(|section| section.rows.len()).sum::<usize>(),
        "the flattened rows are exactly the section rows",
    );
    let mut flattened: Vec<&str> = Vec::new();
    for section in HELP_SECTIONS {
        flattened.extend(section.rows.iter().map(|row| row.input));
    }
    let from_helper: Vec<&str> = all_rows().iter().map(|row| row.input).collect();
    assert_eq!(flattened, from_helper, "all_rows is the same strings, not a copy");
}

#[test]
fn the_overlay_shows_only_the_active_view_and_global() {
    for (mode_3d, active, inactive) in [(true, "3D view", "2D view"), (false, "2D view", "3D view")] {
        let sections = sections_for(mode_3d);
        assert_eq!(sections.len(), 2, "the overlay stays compact: one view group + Global");
        assert_eq!(sections[0].title, active, "the active view group comes first");
        assert_eq!(sections[1].title, "Global");
        assert!(
            !sections.iter().any(|section| section.title == inactive),
            "the overlay hides the inactive view's bindings",
        );

        let rows = rows_for(mode_3d);
        assert!(
            rows.iter().any(|row| row.input == "Wheel / pinch"),
            "the active view's bindings are present",
        );
        if mode_3d {
            assert!(
                rows.iter()
                    .any(|row| row.input == "Right-drag" && row.action == "Orbit"),
                "the 3D right-drag orbit row is in the 3D overlay",
            );
            assert!(
                !rows.iter().any(|row| row.input == "Left / right-drag"),
                "the 2D-only drag row stays out of the 3D overlay",
            );
        } else {
            assert!(
                rows.iter()
                    .any(|row| row.input == "Left / right-drag" && row.action == "Pan"),
                "the 2D drag row is in the 2D overlay",
            );
            assert!(
                !rows.iter().any(|row| row.input == "Right-drag"),
                "the 3D-only right-drag row stays out of the 2D overlay",
            );
        }
    }
}

#[test]
fn the_overlay_sections_come_from_the_same_constants() {
    // The overlay filters HELP_SECTIONS and hands back the very same
    // constants, so the strings cannot drift.
    assert!(std::ptr::eq(sections_for(true)[0], &HELP_SECTIONS[0]));
    assert!(std::ptr::eq(sections_for(true)[1], &HELP_SECTIONS[2]));
    assert!(std::ptr::eq(sections_for(false)[0], &HELP_SECTIONS[1]));
    assert!(std::ptr::eq(sections_for(false)[1], &HELP_SECTIONS[2]));
    assert_eq!(HELP_SECTIONS.len(), 3, "the table keeps all three groups");
}

#[test]
fn both_modes_document_the_panel_shortcuts_and_their_text_field_rule() {
    // Ticket 15 moved every shortcut into one table that stays inert while a
    // text field has focus (Ctrl+Q is the only exception), so the help must
    // keep describing that rule rather than dropping it.
    for mode_3d in [false, true] {
        let rows = rows_for(mode_3d);
        let text = rows
            .iter()
            .map(|row| format!("{}: {}", row.input, row.action))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("Ctrl+B"), "the Controls shortcut is documented: {text}");
        assert!(text.contains("Ctrl+P"), "the Presets shortcut is documented: {text}");
        assert!(
            text.contains("not while typing"),
            "the help says shortcuts stay out of text fields: {text}",
        );
        assert!(text.contains("Ctrl+Q"), "the quit shortcut is documented: {text}");
        assert!(text.contains("F1"), "the help-overlay shortcut is documented: {text}");

        let panel = rows
            .iter()
            .find(|row| row.input == "Ctrl+B / Ctrl+P")
            .expect("the panel shortcut row exists");
        assert!(
            panel.action.contains("not while typing"),
            "the panel row itself carries the text-field rule: {}",
            panel.action,
        );
    }
}

#[test]
fn the_stale_right_drag_pans_copy_is_gone() {
    let stale = [
        ("Right/middle-drag", "pan"),
        ("Drag", "orbit"),
        ("Drag", "pan"),
    ];
    for section in HELP_SECTIONS {
        for row in section.rows {
            for (input, action) in stale {
                assert!(
                    !(row.input.eq_ignore_ascii_case(input)
                        && row.action.eq_ignore_ascii_case(action)),
                    "stale help copy `{input}: {action}` is still listed",
                );
            }
            assert!(
                !row.input.to_lowercase().contains("right/middle"),
                "stale combined right/middle copy: {}",
                row.input,
            );
        }
    }
}

#[test]
fn two_d_help_never_mentions_orbiting() {
    for row in section("2D view") {
        assert!(
            !row.action.to_lowercase().contains("orbit"),
            "2D help must not advertise orbiting: {}: {}",
            row.input,
            row.action,
        );
    }
    assert!(
        section("3D view")
            .iter()
            .any(|row| row.action.to_lowercase().contains("orbit")),
        "the 3D table still documents orbiting",
    );
}
