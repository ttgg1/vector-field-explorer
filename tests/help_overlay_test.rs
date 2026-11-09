// Ticket 18: the help table is one source of strings for both the viewport
// overlay and the Help menu's full window (ticket 28 keeps that window as the
// only menu entry). These tests pin every binding exactly once in its group
// and reject the stale "right-drag pans" copy.

use vector_field_explorer::ui::help_overlay::{
    FLY_HELP_ACTION, HELP_ROW_SIZE, HELP_SECTIONS, HELP_SECTION_TITLE_SIZE,
    HELP_WINDOW_TITLE_SIZE, LEFT_CLICK_HELP_ACTION, LEFT_CLICK_HELP_INPUT,
    LEFT_CLICK_HELP_TOOLTIP, OVERLAY_HEADING_COLOR, SEED_HELP_ACTION, SEED_HELP_INPUT,
    SEED_HELP_TOOLTIP, HelpRow, all_rows, rows_for, section_heading_style, sections_for,
    window_heading_color,
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
    ("Click", "Per Left click setting"),
    ("Seed:", "auto | click"),
    ("Left click:", "seed streamline | add particles"),
    ("S", "Seed streamline at pointer"),
    ("P", "Particle burst at pointer"),
    (
        "C",
        "fly · WASD move · Q/E down/up · Shift boost · mouse look · Esc exit · Camera toggles it",
    ),
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
    ("Click", "Per Left click setting"),
    ("Seed:", "auto | click"),
    ("Left click:", "seed streamline | add particles"),
    ("S", "Seed streamline at pointer"),
    ("P", "Particle burst at pointer"),
];

const GLOBAL_BINDINGS: &[(&str, &str)] = &[
    ("Space", "Play/pause simulation; particles follow the clock"),
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
fn the_seed_mode_line_documents_both_modes_and_the_off_state() {
    assert_eq!(
        format!("{SEED_HELP_INPUT} {SEED_HELP_ACTION}"),
        "Seed: auto | click",
        "the overlay shows the short seed-mode line",
    );
    assert!(
        SEED_HELP_TOOLTIP.contains("Off") && SEED_HELP_TOOLTIP.contains("click seeding"),
        "the tooltip carries the off-state explanation: {SEED_HELP_TOOLTIP}",
    );
    for mode_3d in [false, true] {
        assert!(
            rows_for(mode_3d)
                .iter()
                .any(|row| row.input == SEED_HELP_INPUT && row.action == SEED_HELP_ACTION),
            "both view overlays carry the seed-mode line",
        );
    }
}

#[test]
fn the_left_click_selector_is_documented_in_both_views() {
    assert_eq!(
        format!("{LEFT_CLICK_HELP_INPUT} {LEFT_CLICK_HELP_ACTION}"),
        "Left click: seed streamline | add particles",
        "the overlay names both choices",
    );
    assert!(
        LEFT_CLICK_HELP_TOOLTIP.contains("Features") && LEFT_CLICK_HELP_TOOLTIP.contains("off"),
        "the tooltip says where to choose and why an off feature is a no-op: \
         {LEFT_CLICK_HELP_TOOLTIP}",
    );
    for mode_3d in [false, true] {
        assert!(
            rows_for(mode_3d).iter().any(|row| row.input == LEFT_CLICK_HELP_INPUT
                && row.action == LEFT_CLICK_HELP_ACTION),
            "both view overlays carry the left-click selector line",
        );
        for key in ["S", "P"] {
            assert!(
                rows_for(mode_3d).iter().any(|row| row.input == key),
                "both view overlays document {key}",
            );
        }
    }
}

#[test]
fn help_actions_fit_the_overlay_width_budget() {
    // Ticket-65 review LOW-2: the fly row had grown to 95 characters without a
    // visual width check. The compact overlay wraps neither column, so keep
    // every action inside the 88-character budget (the shortened fly row is
    // 87). A future row that needs more should split rather than widen.
    const MAX_ACTION_CHARS: usize = 88;
    for row in all_rows() {
        let length = row.action.chars().count();
        assert!(
            length <= MAX_ACTION_CHARS,
            "`{}: {}` is {length} characters, over the {MAX_ACTION_CHARS}-character overlay budget",
            row.input,
            row.action,
        );
    }
    assert_eq!(
        FLY_HELP_ACTION.chars().count(),
        87,
        "the fly row is pinned at the shortened length",
    );
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
fn the_overlay_and_window_share_the_same_section_constants() {
    // The overlay filters HELP_SECTIONS; the window renders it whole. Both
    // hand back the very same constants, so the strings cannot drift.
    assert!(std::ptr::eq(sections_for(true)[0], &HELP_SECTIONS[0]));
    assert!(std::ptr::eq(sections_for(true)[1], &HELP_SECTIONS[2]));
    assert!(std::ptr::eq(sections_for(false)[0], &HELP_SECTIONS[1]));
    assert!(std::ptr::eq(sections_for(false)[1], &HELP_SECTIONS[2]));
    assert_eq!(HELP_SECTIONS.len(), 3, "the window keeps all three groups");
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
fn help_typography_ranks_the_window_title_above_section_headings_and_rows() {
    // Ticket 28: the window/overlay title is the largest line, then a section
    // heading, then a binding row. A regression that flattens or inverts the
    // hierarchy fails here. The sizes arrive as parameters so clippy does not
    // fold the comparison into an `assertions_on_constants` warning.
    fn assert_size_ladder(window: f32, section: f32, row: f32) {
        assert!(
            window > section,
            "the help window title ({window} pt) must outrank a section heading ({section} pt)",
        );
        assert!(
            section > row,
            "a section heading ({section} pt) must outrank a binding row ({row} pt)",
        );
    }

    assert_size_ladder(
        HELP_WINDOW_TITLE_SIZE,
        HELP_SECTION_TITLE_SIZE,
        HELP_ROW_SIZE,
    );
}

#[test]
fn the_section_heading_helper_uses_the_section_title_size() {
    // The shared table in `src/ui/help_overlay.rs` renders headings through
    // this helper. This pins the size only; emphasis is a colour, covered by
    // `the_heading_emphasis_colour_differs_from_normal_text_in_both_themes`.
    let heading = section_heading_style();
    assert_eq!(
        heading.size, HELP_SECTION_TITLE_SIZE,
        "the section heading style must use HELP_SECTION_TITLE_SIZE",
    );
}

#[test]
fn the_overlay_heading_colour_is_light_and_outranks_the_row_colours() {
    // The compact overlay sits on a fixed `from_black_alpha(140)` fill in both
    // themes, so its headings must use the fixed light colour rather than the
    // theme's strong text colour (which is black in the light theme). The
    // colour must also be brighter than the 185/155/150 row/footer greys.
    for (theme, visuals) in [
        ("dark", eframe::egui::Visuals::dark()),
        ("light", eframe::egui::Visuals::light()),
    ] {
        let overlay = OVERLAY_HEADING_COLOR;
        assert_eq!(
            overlay.r(),
            overlay.g(),
            "{theme}: the overlay heading colour is a neutral grey",
        );
        assert_eq!(
            overlay.g(),
            overlay.b(),
            "{theme}: the overlay heading colour is a neutral grey",
        );
        let gray = overlay.r();
        assert!(
            gray >= 200,
            "{theme}: the overlay heading ({gray}) must stay light on the dark fill",
        );
        for row_gray in [185u8, 155, 150] {
            assert!(
                gray > row_gray,
                "{theme}: the overlay heading ({gray}) must outrank the row/footer grey ({row_gray})",
            );
        }
        assert_ne!(
            overlay,
            window_heading_color(&visuals),
            "{theme}: the overlay keeps its own light colour instead of the theme strong colour",
        );
    }
}

#[test]
fn the_help_window_heading_colour_is_the_theme_strong_colour() {
    // The Help window sits on the theme background, so its 16 pt title and
    // 15 pt headings use the theme's strong text colour: light in dark mode,
    // dark in light mode — and never the normal/row text colour.
    for (theme, visuals) in [
        ("dark", eframe::egui::Visuals::dark()),
        ("light", eframe::egui::Visuals::light()),
    ] {
        let emphasis = window_heading_color(&visuals);
        assert_eq!(
            emphasis,
            visuals.strong_text_color(),
            "{theme}: the Help window heading must be the strong text colour",
        );
        assert_ne!(
            emphasis,
            visuals.text_color(),
            "{theme}: the Help window heading must differ from normal/row text",
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
