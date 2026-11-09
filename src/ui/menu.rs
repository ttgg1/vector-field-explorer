//! The pure menu seam (ticket 15): the menu table, the shortcut table, and
//! `apply_action`, which turns a picked [`MenuAction`] into the mutations the
//! UI state owns plus the [`SideEffect`] only the app can run (open a popover,
//! request a capture, quit, copy text).
//!
//! The top bar renders [`MENUS`] and looks shortcut labels up in [`SHORTCUTS`];
//! it never wires behaviour itself, so the menu contents and the keyboard are
//! one table each and stay in step with [`MenuAction::ALL`] under the tests in
//! `tests/menu_test.rs`.

use eframe::egui;

use crate::camera::OrbitCamera;
use crate::render::Camera;
use crate::ui::layout::LayoutState;
use crate::ui::sidebar::{self, Controls};
use crate::viewmodel::State;

/// Shortcut labels shown beside menu items and in tooltips. They live next to
/// [`SHORTCUTS`] so a label can never drift from the key it describes.
pub const CONTROLS_SHORTCUT: &str = "Ctrl+B";
pub const PRESETS_SHORTCUT: &str = "Ctrl+P";
pub const SCREENSHOT_SHORTCUT: &str = "Ctrl+Shift+S";
pub const COPY_FORMULA_SHORTCUT: &str = "Ctrl+Shift+C";
pub const QUIT_SHORTCUT: &str = "Ctrl+Q";
pub const HELP_SHORTCUT: &str = "F1";

/// About dialog facts, all from the package metadata except the display name.
/// The version is the compile-time seam, so a nightly build reports its
/// `<version>-nightly.<date>+<sha>` string (ticket 42).
pub const ABOUT_NAME: &str = "Vector Field Explorer";
pub const ABOUT_VERSION: &str = crate::version::VERSION;
pub const ABOUT_LICENSE: &str = "GPL-3.0-only";
pub const ABOUT_URL: &str = "https://github.com/ttgg1/vector-field-explorer";

/// Every command a menu item (or a shortcut) can dispatch. Adding a variant
/// requires adding it to a menu: `tests/menu_test.rs` pins reachability.
///
/// The macro defines the enum and `MenuAction::ALL` from one token list, so
/// the reachability test's variant list can never drift from the enum without
/// the compiler noticing.
macro_rules! menu_actions {
    ($($variant:ident),+ $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum MenuAction {
            $($variant),+
        }

        impl MenuAction {
            /// Every variant, in menu order. Used by the reachability test so
            /// a new variant cannot be forgotten in the table.
            pub const ALL: &'static [MenuAction] = &[$(MenuAction::$variant),+];
        }
    };
}

menu_actions! {
    ExportScreenshot,
    Quit,
    CopyFormula,
    ResetAllControls,
    ToggleControls,
    TogglePresets,
    ResetLayout,
    ResetCamera,
    ToggleHelp,
    ShowHelpTable,
    OpenAppearance,
    About,
}

/// What the app must do after the pure state mutation: the effects the menu
/// seam cannot perform without a window or a GPU.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SideEffect {
    /// The action only mutated [`UiState`].
    None,
    /// Show the appearance popover anchored to the top-bar gear.
    OpenAppearance,
    /// Show the About dialog.
    ShowAbout,
    /// Show the full keyboard & mouse window (all three help groups).
    ShowHelp,
    /// Ask the screenshot capture for one frame.
    Screenshot,
    /// Close the window.
    Quit,
    /// Put `text` on the clipboard.
    CopyText(String),
    /// Give the side panels fresh ids so the default widths apply again.
    ResetLayout,
    /// Clear the highlighted camera preset after a camera reset.
    ResetCamera,
}

/// One entry in a dropdown. The shortcut hint (if any) is looked up in
/// [`SHORTCUTS`] by action, so it is defined exactly once.
#[derive(Debug, Clone, Copy)]
pub struct MenuItem {
    pub label: &'static str,
    pub action: MenuAction,
}

/// One top-level menu.
#[derive(Debug, Clone, Copy)]
pub struct Menu {
    pub title: &'static str,
    pub items: &'static [MenuItem],
}

/// The v1 menu bar. No dead or greyed-out entries: every item dispatches an
/// action that does something today.
pub const MENUS: &[Menu] = &[
    Menu {
        title: "File",
        items: &[
            MenuItem {
                label: "Export screenshot…",
                action: MenuAction::ExportScreenshot,
            },
            MenuItem { label: "Quit", action: MenuAction::Quit },
        ],
    },
    Menu {
        title: "Edit",
        items: &[
            MenuItem { label: "Copy formula", action: MenuAction::CopyFormula },
            MenuItem {
                label: "Reset all controls",
                action: MenuAction::ResetAllControls,
            },
        ],
    },
    Menu {
        title: "View",
        items: &[
            MenuItem { label: "Toggle Controls", action: MenuAction::ToggleControls },
            MenuItem { label: "Toggle Presets", action: MenuAction::TogglePresets },
            MenuItem { label: "Reset layout", action: MenuAction::ResetLayout },
            MenuItem { label: "Reset camera", action: MenuAction::ResetCamera },
            // The user reported looking for color controls under View (ticket
            // 41). Ticket 59 removed the duplicate Settings entry, so this is
            // the one menu command that opens the Appearance popover.
            MenuItem { label: "Appearance…", action: MenuAction::OpenAppearance },
        ],
    },
    Menu {
        title: "Help",
        items: &[
            MenuItem { label: "Keyboard & mouse", action: MenuAction::ShowHelpTable },
            MenuItem { label: "About Vector Field Explorer", action: MenuAction::About },
        ],
    },
];

/// One keyboard shortcut. `global` shortcuts fire even while a text field has
/// focus; only Quit is global, because view state is already persisted and
/// there is no save prompt to intercept.
#[derive(Debug, Clone, Copy)]
pub struct Shortcut {
    pub action: MenuAction,
    pub label: &'static str,
    pub key: egui::Key,
    pub modifiers: egui::Modifiers,
    pub global: bool,
}

/// The one shortcut table. Menu rendering, tooltips and the app's input loop
/// all read it, so a displayed key is always a handled key.
pub const SHORTCUTS: &[Shortcut] = &[
    Shortcut {
        action: MenuAction::ExportScreenshot,
        label: SCREENSHOT_SHORTCUT,
        key: egui::Key::S,
        modifiers: egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
        global: false,
    },
    Shortcut {
        action: MenuAction::Quit,
        label: QUIT_SHORTCUT,
        key: egui::Key::Q,
        modifiers: egui::Modifiers::COMMAND,
        global: true,
    },
    Shortcut {
        action: MenuAction::CopyFormula,
        label: COPY_FORMULA_SHORTCUT,
        key: egui::Key::C,
        modifiers: egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
        global: false,
    },
    Shortcut {
        action: MenuAction::ToggleControls,
        label: CONTROLS_SHORTCUT,
        key: egui::Key::B,
        modifiers: egui::Modifiers::COMMAND,
        global: false,
    },
    Shortcut {
        action: MenuAction::TogglePresets,
        label: PRESETS_SHORTCUT,
        key: egui::Key::P,
        modifiers: egui::Modifiers::COMMAND,
        global: false,
    },
    Shortcut {
        action: MenuAction::ToggleHelp,
        label: HELP_SHORTCUT,
        key: egui::Key::F1,
        modifiers: egui::Modifiers::NONE,
        global: false,
    },
];

/// The shortcut bound to `action`, if it has one.
pub fn shortcut_for(action: MenuAction) -> Option<&'static Shortcut> {
    SHORTCUTS.iter().find(|shortcut| shortcut.action == action)
}

/// The label shown beside `action`'s menu item.
pub fn shortcut_label(action: MenuAction) -> Option<&'static str> {
    shortcut_for(action).map(|shortcut| shortcut.label)
}

/// The hint shown beside `item`. Normally the item's own shortcut; Help >
/// Keyboard & mouse is the exception: it opens the full table window while F1
/// toggles the compact overlay, so the item shows the help shortcut's label
/// (F1) and the two help surfaces advertise the same key.
pub fn item_shortcut_label(item: &MenuItem) -> Option<&'static str> {
    match item.action {
        MenuAction::ShowHelpTable => shortcut_label(MenuAction::ToggleHelp),
        action => shortcut_label(action),
    }
}

/// Whether `shortcut` may fire this frame. Every shortcut stays inert while a
/// text field has focus except the global ones (Ctrl+Q).
pub fn shortcut_allowed(shortcut: &Shortcut, text_edit_focused: bool) -> bool {
    shortcut.global || !text_edit_focused
}

/// Consume this frame's pressed shortcuts and report the actions to dispatch,
/// in table order. Called before any widget so a focused text field never
/// receives a key the menu table owns (Ctrl+Q is consumed from anywhere).
/// Matching is exact: `Ctrl+Shift+B` is not `Ctrl+B`, and `Shift+F1` is not
/// `F1`, so an extra modifier never fires a plain shortcut.
pub fn shortcut_actions(ctx: &egui::Context) -> Vec<MenuAction> {
    let text_edit_focused = ctx.text_edit_focused();
    ctx.input_mut(|input| {
        let modifiers = input.modifiers;
        SHORTCUTS
            .iter()
            .filter(|shortcut| shortcut_allowed(shortcut, text_edit_focused))
            .filter(|shortcut| modifiers.matches_exact(shortcut.modifiers))
            .filter(|shortcut| input.consume_key(shortcut.modifiers, shortcut.key))
            .map(|shortcut| shortcut.action)
            .collect()
    })
}

/// Whether this frame pressed `?` (Shift+/ on a US layout), the secondary help
/// shortcut. It is not in [`SHORTCUTS`] because that table keeps exactly one
/// key per action (F1); the help overlay documents both. Inert while a text
/// field has focus, like every camera/view key.
pub fn help_key_pressed(ctx: &egui::Context) -> bool {
    if ctx.text_edit_focused() {
        return false;
    }
    ctx.input(|input| {
        let modifiers = input.modifiers;
        modifiers.shift
            && !modifiers.ctrl
            && !modifiers.command
            && !modifiers.alt
            && !modifiers.mac_cmd
            && input.key_pressed(egui::Key::Slash)
    })
}

/// The state a menu action mutates: everything the pure seam can touch. The
/// app builds one per dispatch from its own fields, so there is no second copy
/// of the layout, camera, viewmodel or controls records.
pub struct UiState<'a> {
    pub layout: &'a mut LayoutState,
    pub orbit: &'a mut OrbitCamera,
    pub camera: &'a mut Camera,
    pub state: &'a mut State,
    pub controls: &'a mut Controls,
}

/// Apply `action` to the UI state and report what the app must still do.
pub fn apply_action(action: MenuAction, ui: &mut UiState<'_>) -> SideEffect {
    match action {
        MenuAction::ExportScreenshot => SideEffect::Screenshot,
        MenuAction::Quit => SideEffect::Quit,
        MenuAction::CopyFormula => SideEffect::CopyText(ui.state.formula.clone()),
        MenuAction::ResetAllControls => {
            // The Controls panel's knobs, not the field itself: the formula is
            // the document, the controls are how it is displayed.
            *ui.controls = Controls::new();
            sidebar::apply(ui.state, ui.controls);
            SideEffect::None
        }
        MenuAction::ToggleControls => {
            ui.layout.toggle_controls();
            SideEffect::None
        }
        MenuAction::TogglePresets => {
            ui.layout.toggle_presets();
            SideEffect::None
        }
        MenuAction::ResetLayout => {
            ui.layout.reset();
            SideEffect::ResetLayout
        }
        MenuAction::ResetCamera => {
            *ui.orbit = crate::view_command::apply_view_command(
                *ui.orbit,
                crate::view_command::ViewContext::new(ui.state.mode_3d, ui.state.domain, 1.0),
                crate::view_command::ViewCommand::Reset,
            );
            *ui.camera = Camera::default();
            SideEffect::ResetCamera
        }
        MenuAction::ToggleHelp => {
            ui.layout.toggle_help();
            SideEffect::None
        }
        MenuAction::ShowHelpTable => SideEffect::ShowHelp,
        MenuAction::OpenAppearance => SideEffect::OpenAppearance,
        MenuAction::About => SideEffect::ShowAbout,
    }
}
