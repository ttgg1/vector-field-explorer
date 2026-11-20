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
use crate::edit_history::EditScope;
use crate::fly_camera::{FlyCamera, toggle_fly};
use crate::interaction::{Bindings, InteractionAction, KeyChord};
use crate::render::Camera;
use crate::sim_clock::SimClock;
use crate::ui::layout::LayoutState;
use crate::ui::sidebar::{self, Controls};
use crate::view_command;
use crate::viewmodel::State;

/// Display-only fly-centre label (ticket 70). Fly toggle and movement stay on
/// the physical-key path and are not part of the binding model.
pub const FLY_SHORTCUT: &str = "C";
/// Reserved edit-history labels. Undo and redo are never remappable.
pub const UNDO_SHORTCUT: &str = "Ctrl+Z";
pub const REDO_SHORTCUT: &str = "Ctrl+Shift+Z";

/// About dialog facts, all from the package metadata except the display name.
/// The version is the compile-time seam, so a nightly build reports its
/// `<version>-nightly.<date>+<sha>` string (ticket 42).
pub const ABOUT_NAME: &str = "Vector Field Explorer";
pub const ABOUT_VERSION: &str = crate::version::VERSION;
pub const ABOUT_LICENSE: &str = "GPL-3.0-only";
pub const ABOUT_URL: &str = "https://github.com/ttgg1/vector-field-explorer";
/// The one-line pitch shown in About and on the browser page.
pub const ABOUT_TAGLINE: &str = "A formula-driven vector-field explorer for 2D and 3D fields.";
/// Legal holder while the project has no separate legal entity (ticket 19 may
/// replace it once the owner confirms a name).
pub const ABOUT_COPYRIGHT: &str = "Copyright © 2026 The Vector Field Explorer authors.";
/// The deployed browser build, linked from About.
pub const ABOUT_SITE_URL: &str = "https://ttgg1.github.io/vector-field-explorer/";
/// The static native-download page beside the browser build (ticket 16a),
/// linked from Help. The URL 404s until the first Pages deploy (gated on the
/// `v0.1.0` release); the menu item only opens it. A deliberate public UI-link
/// constant mirroring [`ABOUT_SITE_URL`], not test-only API widening.
pub const DOWNLOAD_URL: &str = "https://ttgg1.github.io/vector-field-explorer/download.html";

/// Every command a menu item (or a shortcut) can dispatch. Adding a variant
/// requires adding it to a menu: `tests/menu_test.rs` pins reachability.
///
/// The macro defines the enum and `MenuAction::ALL` from one token list, so
/// the reachability test's variant list can never drift from the enum without
/// the compiler noticing.
macro_rules! menu_actions {
    ($( $(#[$attr:meta])* $variant:ident),+ $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum MenuAction {
            $( $(#[$attr])* $variant),+
        }

        impl MenuAction {
            /// Every variant, in menu order. Used by the reachability test so
            /// a new variant cannot be forgotten in the table.
            pub const ALL: &'static [MenuAction] = &[$( $(#[$attr])* MenuAction::$variant),+];
        }
    };
}

menu_actions! {
    ExportScreenshot,
    ExportScreenshotSvg,
    ExportAnimation,
    Quit,
    CopyFormula,
    Undo,
    Redo,
    ResetColorScope,
    ResetSimulation,
    ResetDocument,
    ResetAllControls,
    ToggleControls,
    #[cfg(not(target_arch = "wasm32"))]
    ToggleCas,
    TogglePresets,
    ToggleAppearancePanel,
    ToggleInteractionPanel,
    OpenInteraction,
    ToggleFly,
    ResetLayout,
    ResetCamera,
    ToggleHelp,
    ShowHelpTable,
    OpenAppearance,
    OpenDownloadPage,
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
    /// Show the animation export dialog.
    Animation,
    /// Ask the screenshot capture for one frame.
    Screenshot,
    /// Ask for an SVG (vector) screenshot of the current 2D view.
    ScreenshotSvg,
    /// Close the window.
    Quit,
    /// Put `text` on the clipboard.
    CopyText(String),
    /// Open `url` in the user's browser.
    OpenUrl(String),
    /// Give the side panels fresh ids so the default widths apply again.
    ResetLayout,
    /// Apply the previous edit-history snapshot.
    Undo,
    /// Re-apply the next edit-history snapshot.
    Redo,
    /// Run the one recorded scoped-reset seam in the app.
    ResetScope(EditScope),
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
            MenuItem {
                label: "Export screenshot (SVG)…",
                action: MenuAction::ExportScreenshotSvg,
            },
            MenuItem {
                label: "Export animation…",
                action: MenuAction::ExportAnimation,
            },
            MenuItem {
                label: "Quit",
                action: MenuAction::Quit,
            },
        ],
    },
    Menu {
        title: "Edit",
        items: &[
            MenuItem {
                label: "Undo",
                action: MenuAction::Undo,
            },
            MenuItem {
                label: "Redo",
                action: MenuAction::Redo,
            },
            MenuItem {
                label: "Copy formula",
                action: MenuAction::CopyFormula,
            },
            MenuItem {
                label: "Reset colour scope",
                action: MenuAction::ResetColorScope,
            },
            MenuItem {
                label: "Reset simulation",
                action: MenuAction::ResetSimulation,
            },
            MenuItem {
                label: "Reset document",
                action: MenuAction::ResetDocument,
            },
            MenuItem {
                label: "Reset all controls",
                action: MenuAction::ResetAllControls,
            },
        ],
    },
    Menu {
        title: "View",
        items: &[
            MenuItem {
                label: "Toggle Controls",
                action: MenuAction::ToggleControls,
            },
            #[cfg(not(target_arch = "wasm32"))]
            MenuItem {
                label: "Toggle CAS",
                action: MenuAction::ToggleCas,
            },
            MenuItem {
                label: "Toggle Presets",
                action: MenuAction::TogglePresets,
            },
            // Ticket 65: the sidebar Camera section is the one visible fly
            // control; C keeps dispatching `ToggleFly` through this table.
            MenuItem {
                label: "Reset layout",
                action: MenuAction::ResetLayout,
            },
            MenuItem {
                label: "Reset camera",
                action: MenuAction::ResetCamera,
            },
            // The user reported looking for color controls under View (ticket
            // 41). Ticket 59 removed the duplicate Settings entry, so this is
            // the one menu command that opens the Appearance popover.
            MenuItem {
                label: "Appearance…",
                action: MenuAction::OpenAppearance,
            },
            MenuItem {
                label: "Toggle Interaction",
                action: MenuAction::ToggleInteractionPanel,
            },
            MenuItem {
                label: "Interaction…",
                action: MenuAction::OpenInteraction,
            },
        ],
    },
    Menu {
        title: "Help",
        items: &[
            MenuItem {
                label: "Keyboard & mouse",
                action: MenuAction::ShowHelpTable,
            },
            MenuItem {
                label: "Download desktop app",
                action: MenuAction::OpenDownloadPage,
            },
            MenuItem {
                label: "About Vector Field Explorer",
                action: MenuAction::About,
            },
        ],
    },
];

/// One keyboard shortcut that is not part of the interaction binding model:
/// the reserved undo/redo chords and the display-only fly toggle. Remappable
/// actions resolve through [`Bindings`] instead.
#[derive(Debug, Clone, Copy)]
pub struct Shortcut {
    pub action: MenuAction,
    pub label: &'static str,
    pub key: egui::Key,
    pub modifiers: egui::Modifiers,
    pub global: bool,
}

/// The static shortcut table: reserved chords plus the physical fly hint. The
/// remappable actions live in `Bindings`, so their labels and routing have one
/// source of truth each.
pub const SHORTCUTS: &[Shortcut] = &[
    Shortcut {
        action: MenuAction::Undo,
        label: UNDO_SHORTCUT,
        key: egui::Key::Z,
        modifiers: egui::Modifiers::COMMAND,
        global: false,
    },
    Shortcut {
        action: MenuAction::Redo,
        label: REDO_SHORTCUT,
        key: egui::Key::Z,
        modifiers: egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
        global: false,
    },
    Shortcut {
        action: MenuAction::ToggleFly,
        label: FLY_SHORTCUT,
        key: egui::Key::C,
        modifiers: egui::Modifiers::NONE,
        global: false,
    },
];

/// The interaction-model action backing `action`, for the menu commands that
/// are remappable. View actions are handled by their separate keyboard/view
/// seam and are deliberately absent.
pub fn interaction_action_for(action: MenuAction) -> Option<InteractionAction> {
    Some(match action {
        MenuAction::ToggleControls => InteractionAction::ToggleControls,
        MenuAction::TogglePresets => InteractionAction::TogglePresets,
        MenuAction::ToggleAppearancePanel => InteractionAction::ToggleAppearance,
        MenuAction::ToggleInteractionPanel => InteractionAction::ToggleInteraction,
        MenuAction::ToggleHelp => InteractionAction::ToggleHelp,
        MenuAction::ExportScreenshot => InteractionAction::Screenshot,
        MenuAction::CopyFormula => InteractionAction::CopyFormula,
        MenuAction::Quit => InteractionAction::Quit,
        _ => return None,
    })
}

/// The menu command an interaction action dispatches, if it has a menu entry.
pub fn menu_action_for(action: InteractionAction) -> Option<MenuAction> {
    Some(match action {
        InteractionAction::ToggleControls => MenuAction::ToggleControls,
        InteractionAction::TogglePresets => MenuAction::TogglePresets,
        InteractionAction::ToggleAppearance => MenuAction::ToggleAppearancePanel,
        InteractionAction::ToggleInteraction => MenuAction::ToggleInteractionPanel,
        InteractionAction::ToggleHelp => MenuAction::ToggleHelp,
        InteractionAction::Screenshot => MenuAction::ExportScreenshot,
        InteractionAction::CopyFormula => MenuAction::CopyFormula,
        InteractionAction::Quit => MenuAction::Quit,
        _ => return None,
    })
}

/// A chord label in the menu's platform spelling: `Mod` becomes `Ctrl` on
/// Linux/Windows and `Cmd` on macOS, matching the labels the menus have always
/// shown. The Interaction panel uses [`KeyChord::label`]'s `Mod` spelling.
pub fn chord_shortcut_label(chord: KeyChord) -> String {
    let mod_name = if cfg!(target_os = "macos") {
        "Cmd"
    } else {
        "Ctrl"
    };
    let mut label = chord.label();
    if let Some(rest) = label.strip_prefix("Mod+") {
        label = format!("{mod_name}+{rest}");
    }
    label
}

/// The shortcut bound to a static (non-remappable) action, if any.
pub fn shortcut_for(action: MenuAction) -> Option<&'static Shortcut> {
    SHORTCUTS.iter().find(|shortcut| shortcut.action == action)
}

/// The label shown beside `action`'s menu item, read from the active bindings
/// for remappable commands and from the static table for reserved ones.
pub fn shortcut_label(action: MenuAction, bindings: &Bindings) -> Option<String> {
    if let Some(interaction) = interaction_action_for(action) {
        return bindings.chord(interaction).map(chord_shortcut_label);
    }
    if action == MenuAction::ShowHelpTable {
        return shortcut_label(MenuAction::ToggleHelp, bindings);
    }
    shortcut_for(action).map(|shortcut| shortcut.label.to_owned())
}

/// The hint shown beside `item`. Help > Keyboard & mouse opens the full table
/// while F1 toggles the compact overlay, so it shows the help chord too.
pub fn item_shortcut_label(item: &MenuItem, bindings: &Bindings) -> Option<String> {
    match item.action {
        MenuAction::ShowHelpTable => shortcut_label(MenuAction::ToggleHelp, bindings),
        action => shortcut_label(action, bindings),
    }
}

/// Whether `shortcut` may fire this frame. Every shortcut stays inert while a
/// text field has focus except the global ones (Ctrl+Q).
pub fn shortcut_allowed(shortcut: &Shortcut, text_edit_focused: bool) -> bool {
    shortcut.global || !text_edit_focused
}

/// Consume this frame's pressed shortcuts and report the actions to dispatch,
/// in binding-table order followed by the static reserved table. Called before
/// any widget so a focused text field never receives a key the menu owns.
/// Matching is exact: an extra modifier never fires a plain chord.
pub fn shortcut_actions(ctx: &egui::Context, bindings: &Bindings) -> Vec<MenuAction> {
    let text_edit_focused = ctx.text_edit_focused();
    ctx.input_mut(|input| {
        let modifiers = input.modifiers;
        let mut actions = Vec::new();
        for action in InteractionAction::REMAPPABLE {
            let Some(menu_action) = menu_action_for(action) else {
                continue;
            };
            if !action.is_global() && text_edit_focused {
                continue;
            }
            let Some(chord) = bindings.chord(action) else {
                continue;
            };
            if modifiers.matches_exact(view_command::egui_modifiers(chord))
                && input.consume_key(view_command::egui_modifiers(chord), chord.key)
            {
                actions.push(menu_action);
            }
        }
        for shortcut in SHORTCUTS {
            // Ticket 70: ToggleFly is handled on the physical key.
            if shortcut.action == MenuAction::ToggleFly
                || !shortcut_allowed(shortcut, text_edit_focused)
            {
                continue;
            }
            if modifiers.matches_exact(shortcut.modifiers)
                && input.consume_key(shortcut.modifiers, shortcut.key)
            {
                actions.push(shortcut.action);
            }
        }
        actions
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
    /// The active fly camera, if any (ticket 46). Shared so the menu can leave
    /// fly mode before an orbit command and toggle it from View > Fly camera.
    pub fly: &'a mut Option<FlyCamera>,
    pub camera: &'a mut Camera,
    pub state: &'a mut State,
    pub controls: &'a mut Controls,
    /// The one simulation clock (ticket 26): `Reset all controls` returns it to
    /// a fresh session's time 0, playing, 1x.
    pub clock: &'a mut SimClock,
}

/// Reset the orbit/2D camera pose to its documented defaults and leave fly
/// mode. The framing rule behind the app's recorded `EditScope::Camera` reset;
/// the pure menu seam only reports that scope through `SideEffect`.
pub fn reset_camera_pose(
    orbit: &mut OrbitCamera,
    camera: &mut Camera,
    fly: &mut Option<FlyCamera>,
) {
    // Reset is an orbit framing command: leave fly first so the reset applies
    // to the view the user sees, and the old fly pose is gone.
    *fly = None;
    *orbit = OrbitCamera::default();
    *camera = Camera::default();
}

/// Apply `action` to the UI state and report what the app must still do.
pub fn apply_action(action: MenuAction, ui: &mut UiState<'_>) -> SideEffect {
    match action {
        MenuAction::ExportScreenshot => SideEffect::Screenshot,
        MenuAction::ExportScreenshotSvg => SideEffect::ScreenshotSvg,
        MenuAction::ExportAnimation => SideEffect::Animation,
        MenuAction::Quit => SideEffect::Quit,
        MenuAction::CopyFormula => SideEffect::CopyText(ui.state.active_source().to_string()),
        MenuAction::Undo => SideEffect::Undo,
        MenuAction::Redo => SideEffect::Redo,
        MenuAction::ResetColorScope => SideEffect::ResetScope(EditScope::Color),
        MenuAction::ResetSimulation => SideEffect::ResetScope(EditScope::Simulation),
        MenuAction::ResetDocument => SideEffect::ResetScope(EditScope::All),
        MenuAction::ResetAllControls => {
            // The Controls panel's knobs, not the field itself: the formula is
            // the document, the controls are how it is displayed.
            *ui.controls = Controls::new();
            // Ticket 65: the Camera section's fly toggle is also a control.
            // Clear the real camera too, otherwise the app's pre-sidebar
            // mirror sync would silently restore it from the still-active fly
            // mode and the reset would look undone.
            *ui.fly = None;
            sidebar::apply(ui.state, ui.controls);
            // The transport is session state, not part of the Controls panel,
            // but "Reset all controls" means a fresh session: time 0, playing,
            // 1x. The particle cloud itself is left alone.
            *ui.clock = SimClock::default();
            ui.state.set_time(0.0);
            SideEffect::None
        }
        #[cfg(not(target_arch = "wasm32"))]
        MenuAction::ToggleCas => {
            ui.layout.toggle_cas();
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
        MenuAction::ToggleAppearancePanel => {
            ui.layout.toggle_appearance();
            SideEffect::None
        }
        MenuAction::ToggleInteractionPanel => {
            ui.layout.toggle_interaction();
            SideEffect::None
        }
        MenuAction::OpenInteraction => {
            // Open is idempotent; the compact app routes it to its sheet.
            ui.layout.interaction_expanded = true;
            SideEffect::None
        }
        MenuAction::ToggleFly => {
            // Free flight is 3D-only; `toggle_fly` ignores the key in 2D.
            toggle_fly(ui.fly, ui.orbit, ui.state.is_3d(), ui.controls.sensitivity);
            SideEffect::None
        }
        MenuAction::ResetLayout => {
            ui.layout.reset();
            SideEffect::ResetLayout
        }
        MenuAction::ResetCamera => SideEffect::ResetScope(EditScope::Camera),
        MenuAction::ToggleHelp => {
            ui.layout.toggle_help();
            SideEffect::None
        }
        MenuAction::ShowHelpTable => SideEffect::ShowHelp,
        MenuAction::OpenAppearance => SideEffect::OpenAppearance,
        MenuAction::OpenDownloadPage => SideEffect::OpenUrl(DOWNLOAD_URL.to_string()),
        MenuAction::About => SideEffect::ShowAbout,
    }
}
