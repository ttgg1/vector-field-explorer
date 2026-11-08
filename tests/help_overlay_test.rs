// Ticket 18: the help table is one source of strings for both the viewport
// overlay and the Help menu's window. These tests pin every binding exactly
// once in its group and reject the stale "right-drag pans" copy.

use vector_field_explorer::ui::help_overlay::{HELP_SECTIONS, HelpRow, all_rows};

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
    ("Ctrl+B / Ctrl+P", "Toggle panels"),
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
fn the_overlay_and_window_share_one_table() {
    // Both surfaces render HELP_SECTIONS; the flattened rows are exactly the
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
