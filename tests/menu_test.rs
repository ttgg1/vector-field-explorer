// Ticket 15: the pure menu seam. The UI only renders the menu table and
// dispatches `MenuAction`s; everything that can be decided without a window is
// pinned here.

use eframe::egui;
use glam::Vec3;
use vector_field_explorer::camera::OrbitCamera;
use vector_field_explorer::colormap::Colormap;
use vector_field_explorer::render::Camera;
use vector_field_explorer::ui::layout::LayoutState;
use vector_field_explorer::ui::menu::{
    self, ABOUT_LICENSE, ABOUT_URL, ABOUT_VERSION, MenuAction, SHORTCUTS, SideEffect, UiState,
    apply_action,
};
use vector_field_explorer::ui::sidebar::Controls;
use vector_field_explorer::viewmodel::{DEFAULT_DENSITY_2D, State};

/// The mutable state the menu actions touch, owned so each test can build one
/// and borrow it into [`UiState`].
struct Fixture {
    layout: LayoutState,
    orbit: OrbitCamera,
    camera: Camera,
    state: State,
    controls: Controls,
}

impl Fixture {
    fn new() -> Self {
        Self {
            layout: LayoutState::default(),
            orbit: OrbitCamera::default(),
            camera: Camera::default(),
            state: State::new(false),
            controls: Controls::new(),
        }
    }

    fn ui(&mut self) -> UiState<'_> {
        UiState {
            layout: &mut self.layout,
            orbit: &mut self.orbit,
            camera: &mut self.camera,
            state: &mut self.state,
            controls: &mut self.controls,
        }
    }
}

fn effect_of(action: MenuAction) -> SideEffect {
    let mut fixture = Fixture::new();
    apply_action(action, &mut fixture.ui())
}

/// Run one egui pass with one key press and report what the real shortcut loop
/// fires. `run_pass` may run more than one pass; only the first sees the key,
/// so its result is captured once.
fn fired_shortcuts(modifiers: egui::Modifiers, key: egui::Key) -> Vec<MenuAction> {
    let ctx = egui::Context::default();
    let mut input = egui::RawInput::default();
    input.events.push(egui::Event::ModifiersChanged(modifiers));
    input.events.push(egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    });
    let mut fired = None;
    let mut output = ctx.run_pass(input, |ctx| {
        if fired.is_none() {
            fired = Some(menu::shortcut_actions(ctx));
        }
    });
    // The pass is only here to feed input to the shortcut loop; no painting
    // happens, so the texture delta must be cleared before it is dropped.
    output.textures_delta.clear();
    fired.unwrap_or_default()
}

#[test]
fn the_bar_carries_the_v1_menu_table() {
    let table: Vec<(&str, Vec<&str>)> = menu::MENUS
        .iter()
        .map(|menu| {
            (
                menu.title,
                menu.items.iter().map(|item| item.label).collect(),
            )
        })
        .collect();
    assert_eq!(
        table,
        vec![
            ("File", vec!["Export screenshot…", "Quit"]),
            ("Edit", vec!["Copy formula", "Reset all controls"]),
            (
                "View",
                vec![
                    "Toggle Controls",
                    "Toggle Presets",
                    "Reset layout",
                    "Reset camera",
                ],
            ),
            ("Settings", vec!["Appearance…"]),
            (
                "Help",
                vec!["Keyboard & mouse", "About Vector Field Explorer"],
            ),
        ],
        "the v1 menu table is pinned; no placeholder entries",
    );
}

#[test]
fn every_action_maps_to_its_side_effect() {
    let expected = [
        (MenuAction::ExportScreenshot, SideEffect::Screenshot),
        (MenuAction::Quit, SideEffect::Quit),
        (
            MenuAction::CopyFormula,
            SideEffect::CopyText(State::new(false).formula),
        ),
        (MenuAction::ResetAllControls, SideEffect::None),
        (MenuAction::ToggleControls, SideEffect::None),
        (MenuAction::TogglePresets, SideEffect::None),
        (MenuAction::ResetLayout, SideEffect::ResetLayout),
        (MenuAction::ResetCamera, SideEffect::ResetCamera),
        (MenuAction::ToggleHelp, SideEffect::None),
        (MenuAction::OpenSettings, SideEffect::OpenSettings),
        (MenuAction::About, SideEffect::ShowAbout),
    ];
    for (action, expected) in expected {
        assert_eq!(effect_of(action), expected, "{action:?} must report {expected:?}");
    }
}

#[test]
fn reset_layout_restores_expanded_panels_widths_and_help() {
    let mut fixture = Fixture::new();
    fixture.layout = LayoutState {
        controls_expanded: false,
        presets_expanded: false,
        controls_width: 400.0,
        presets_width: 350.0,
        help_visible: false,
    };
    let effect = apply_action(MenuAction::ResetLayout, &mut fixture.ui());
    assert_eq!(effect, SideEffect::ResetLayout, "the app must bump the panel ids");
    assert_eq!(fixture.layout, LayoutState::default(), "every layout knob returns");
    assert!(fixture.layout.controls_expanded, "reset re-expands rather than hiding");
    assert!(fixture.layout.presets_expanded);
    assert!(fixture.layout.help_visible, "help visibility returns to its default");
}

#[test]
fn reset_camera_restores_the_default_framing() {
    let mut fixture = Fixture::new();
    fixture.orbit.yaw = 2.4;
    fixture.orbit.pitch = -0.2;
    fixture.orbit.distance = 9.0;
    fixture.orbit.target = Vec3::new(4.0, -3.0, 1.0);
    fixture.orbit.ortho_zoom = 7.0;
    fixture.camera.position = [5.0, 6.0];
    fixture.camera.scale = 3.0;

    let effect = apply_action(MenuAction::ResetCamera, &mut fixture.ui());
    assert_eq!(effect, SideEffect::ResetCamera, "the app clears the preset highlight");
    assert_eq!(fixture.orbit, OrbitCamera::default(), "orbit framing returns");
    let default_camera = Camera::default();
    assert_eq!(fixture.camera.position, default_camera.position, "the 2D camera record returns");
    assert_eq!(fixture.camera.scale, default_camera.scale);
}

#[test]
fn copy_formula_reads_the_formula_at_invocation_time() {
    let mut fixture = Fixture::new();
    assert!(fixture.state.set_formula("(-y, x)".to_string()));
    let effect = apply_action(MenuAction::CopyFormula, &mut fixture.ui());
    assert_eq!(effect, SideEffect::CopyText("(-y, x)".to_string()));

    assert!(fixture.state.set_formula("(y, -x)".to_string()));
    assert_eq!(
        apply_action(MenuAction::CopyFormula, &mut fixture.ui()),
        SideEffect::CopyText("(y, -x)".to_string()),
        "a later invocation copies the later formula",
    );
}

#[test]
fn copy_formula_copies_even_an_unparseable_edit() {
    let mut fixture = Fixture::new();
    let broken = ")( not a formula";
    assert!(fixture.state.set_formula(broken.to_string()));
    assert!(
        !fixture.state.parse_error.is_empty(),
        "the edit is unparseable: `formula` holds the typed text, `field` keeps the last good expression",
    );
    assert_eq!(
        apply_action(MenuAction::CopyFormula, &mut fixture.ui()),
        SideEffect::CopyText(broken.to_string()),
        "Copy formula copies the text the user typed, parseable or not",
    );
}

#[test]
fn reset_all_controls_returns_the_sidebar_knobs_without_touching_the_formula() {
    let mut fixture = Fixture::new();
    assert!(fixture.state.set_formula("(-y, x)".to_string()));
    assert!(fixture.state.set_density(99));
    assert!(fixture.state.set_colormap(Colormap::Turbo));
    assert!(fixture.state.set_streamlines(true));
    fixture.controls.density = 99;
    fixture.controls.arrow_scale = 3.0;
    fixture.controls.colormap = Colormap::Turbo;
    fixture.controls.streamlines = true;
    fixture.controls.domain_min = Vec3::splat(-3.0);

    assert_eq!(
        apply_action(MenuAction::ResetAllControls, &mut fixture.ui()),
        SideEffect::None,
    );
    assert_eq!(fixture.controls.density, DEFAULT_DENSITY_2D, "the mirror returns");
    assert_eq!(fixture.state.density, DEFAULT_DENSITY_2D, "and the state follows it");
    assert_eq!(fixture.controls.colormap, Colormap::Viridis);
    assert_eq!(fixture.state.colormap, Colormap::Viridis);
    assert!(!fixture.state.streamlines, "traces are off at the defaults");
    assert_eq!(fixture.state.domain.min, Vec3::splat(-1.0), "the unit cube returns");
    assert_eq!(fixture.state.formula, "(-y, x)", "the field itself is not a control");
}

#[test]
fn toggles_flip_visibility() {
    let mut fixture = Fixture::new();
    apply_action(MenuAction::ToggleControls, &mut fixture.ui());
    assert!(!fixture.layout.controls_expanded);
    apply_action(MenuAction::TogglePresets, &mut fixture.ui());
    assert!(!fixture.layout.presets_expanded);
    apply_action(MenuAction::ToggleHelp, &mut fixture.ui());
    assert!(!fixture.layout.help_visible);
}

#[test]
fn the_shortcut_table_has_no_duplicates() {
    for (i, first) in SHORTCUTS.iter().enumerate() {
        for second in &SHORTCUTS[i + 1..] {
            assert_ne!(
                (first.key, first.modifiers),
                (second.key, second.modifiers),
                "{:?} and {:?} share one key combination",
                first.action,
                second.action,
            );
            assert_ne!(
                first.action, second.action,
                "{:?} has more than one shortcut",
                first.action,
            );
            assert_ne!(first.label, second.label, "two actions show one label");
        }
    }
}

#[test]
fn every_enum_variant_is_reachable_from_a_menu_item() {
    let reachable: Vec<MenuAction> = menu::MENUS
        .iter()
        .flat_map(|menu| menu.items.iter().map(|item| item.action))
        .collect();
    for action in MenuAction::ALL {
        assert!(
            reachable.contains(action),
            "{action:?} has no menu item: it would be a dead variant",
        );
    }
}

#[test]
fn no_action_has_more_than_one_menu_home() {
    // Ticket 28: the View and Help menus both offered the help toggle. A
    // command may appear in several places in the UI (the top-bar Screenshot
    // button dispatches ExportScreenshot), but only one menu item may own it.
    // There is no intentional menu-item exception; a second home is a
    // duplicate command and must be removed.
    let homes: Vec<(&str, &str, MenuAction)> = menu::MENUS
        .iter()
        .flat_map(|menu| {
            menu.items
                .iter()
                .map(move |item| (menu.title, item.label, item.action))
        })
        .collect();
    for (i, (menu, label, action)) in homes.iter().enumerate() {
        for (other_menu, other_label, other_action) in &homes[i + 1..] {
            assert_ne!(
                action, other_action,
                "{action:?} appears twice: {menu} > {label} and {other_menu} > {other_label}",
            );
        }
    }
}

#[test]
fn every_menu_item_is_backed_by_a_variant() {
    for menu in menu::MENUS {
        assert!(!menu.items.is_empty(), "{} must not be an empty menu", menu.title);
        for item in menu.items {
            assert!(
                MenuAction::ALL.contains(&item.action),
                "{} item {:?} is missing from MenuAction::ALL",
                menu.title,
                item.label,
            );
        }
    }
}

#[test]
fn shortcuts_are_shown_from_the_one_table() {
    assert_eq!(menu::shortcut_label(MenuAction::ToggleControls), Some("Ctrl+B"));
    assert_eq!(menu::shortcut_label(MenuAction::TogglePresets), Some("Ctrl+P"));
    assert_eq!(menu::shortcut_label(MenuAction::ExportScreenshot), Some("Ctrl+Shift+S"));
    assert_eq!(menu::shortcut_label(MenuAction::Quit), Some("Ctrl+Q"));
    assert_eq!(menu::shortcut_label(MenuAction::CopyFormula), Some("Ctrl+Shift+C"));
    assert_eq!(menu::shortcut_label(MenuAction::ToggleHelp), Some("F1"));
    assert_eq!(
        menu::shortcut_label(MenuAction::ResetCamera),
        None,
        "items without a shortcut show no key hint",
    );
}

#[test]
fn shortcuts_stay_out_of_text_fields_except_quit() {
    let controls = menu::shortcut_for(MenuAction::ToggleControls).expect("Ctrl+B exists");
    assert!(menu::shortcut_allowed(controls, false), "Ctrl+B works outside a text field");
    assert!(!menu::shortcut_allowed(controls, true), "typing B must not toggle panels");

    let quit = menu::shortcut_for(MenuAction::Quit).expect("Ctrl+Q exists");
    assert!(menu::shortcut_allowed(quit, false));
    assert!(menu::shortcut_allowed(quit, true), "Ctrl+Q quits even while typing");
    assert!(quit.global, "quit is the one global shortcut");
}

#[test]
fn extra_modifiers_do_not_fire_the_plain_shortcuts() {
    // Ticket-15 re-review: the loop matches modifiers exactly, so an extra
    // Shift never turns Ctrl+B into a panel toggle or Shift+F1 into help.
    assert_eq!(
        fired_shortcuts(egui::Modifiers::COMMAND, egui::Key::B),
        vec![MenuAction::ToggleControls],
        "plain Ctrl+B still toggles Controls",
    );
    assert!(
        fired_shortcuts(
            egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
            egui::Key::B,
        )
        .is_empty(),
        "Ctrl+Shift+B must not toggle Controls",
    );
    assert_eq!(
        fired_shortcuts(egui::Modifiers::NONE, egui::Key::F1),
        vec![MenuAction::ToggleHelp],
        "plain F1 still toggles help",
    );
    assert!(
        fired_shortcuts(egui::Modifiers::SHIFT, egui::Key::F1).is_empty(),
        "Shift+F1 must not toggle help",
    );
    assert_eq!(
        fired_shortcuts(egui::Modifiers::COMMAND, egui::Key::Q),
        vec![MenuAction::Quit],
        "Ctrl+Q still quits",
    );
}

#[test]
fn about_reports_the_package_version_and_gpl_license() {
    assert_eq!(ABOUT_VERSION, env!("CARGO_PKG_VERSION"));
    assert_eq!(ABOUT_LICENSE, "GPL-3.0-only");
    assert_eq!(ABOUT_URL, "https://github.com/ttgg1/vector-field-explorer");
}

#[test]
fn the_help_menu_entry_toggles_the_help_overlay() {
    // Ticket 28: Help > Keyboard & mouse is the one canonical help command.
    // It toggles the viewport overlay, and shows the F1 hint from SHORTCUTS.
    let help = menu::MENUS
        .iter()
        .find(|menu| menu.title == "Help")
        .expect("the Help menu exists");
    let item = help
        .items
        .iter()
        .find(|item| item.label == "Keyboard & mouse")
        .expect("Help keeps the Keyboard & mouse entry");
    assert_eq!(item.action, MenuAction::ToggleHelp);
    assert_eq!(
        menu::shortcut_label(item.action),
        Some("F1"),
        "the surviving item shows the F1 label sourced from SHORTCUTS",
    );

    let mut fixture = Fixture::new();
    assert!(fixture.layout.help_visible, "help starts visible");
    assert_eq!(
        apply_action(item.action, &mut fixture.ui()),
        SideEffect::None,
    );
    assert!(!fixture.layout.help_visible, "the menu entry dismisses the overlay");
    apply_action(item.action, &mut fixture.ui());
    assert!(fixture.layout.help_visible, "and brings it back");
}

#[test]
fn f1_still_fires_the_single_help_toggle() {
    assert_eq!(
        fired_shortcuts(egui::Modifiers::NONE, egui::Key::F1),
        vec![MenuAction::ToggleHelp],
        "the surviving help command keeps its F1 shortcut",
    );
}

#[test]
fn the_removed_view_help_overlay_entry_is_gone() {
    let view = menu::MENUS
        .iter()
        .find(|menu| menu.title == "View")
        .expect("the View menu exists");
    assert!(
        !view.items.iter().any(|item| item.label == "Help overlay"),
        "View no longer carries the duplicate help entry",
    );
    assert!(
        !menu::MENUS
            .iter()
            .flat_map(|menu| menu.items.iter())
            .any(|item| item.label == "Help overlay"),
        "the removed `Help overlay` label appears in no menu",
    );
}

/// Run one egui pass with one `?` press and report what the real help-key
/// check sees.
fn question_mark_fires(modifiers: egui::Modifiers) -> bool {
    let ctx = egui::Context::default();
    let mut input = egui::RawInput::default();
    input.events.push(egui::Event::ModifiersChanged(modifiers));
    input.events.push(egui::Event::Key {
        key: egui::Key::Slash,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    });
    let mut fired = None;
    let mut output = ctx.run_pass(input, |ctx| {
        if fired.is_none() {
            fired = Some(menu::help_key_pressed(ctx));
        }
    });
    output.textures_delta.clear();
    fired.unwrap_or(false)
}

#[test]
fn question_mark_toggles_help_without_firing_other_shortcuts() {
    assert!(
        question_mark_fires(egui::Modifiers::SHIFT),
        "Shift+/ (?) toggles the help overlay",
    );
    assert!(
        !question_mark_fires(egui::Modifiers::NONE),
        "a bare slash is not the help key",
    );
    assert!(
        !question_mark_fires(egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT)),
        "Ctrl+Shift+/ must not toggle help",
    );
}
