# 15: Conventional top-bar menus (File / Edit / View / Settings / Help)

**What to build:** ticket 14 introduced a menu row with View/Settings/Help
buttons. Replace it with a real menu bar using dropdown menus, so the app can
grow conventional commands without redesigning the bar. No dead entries: every
item performs an action.

**Blocked by:** 14 (top bar, layout, persistence, help overlay).

**Status:** ready-for-agent

## Menu contents (v1)

| Menu | Items |
|---|---|
| **File** | Export screenshot… (`Ctrl+Shift+S`), Quit (`Ctrl+Q`) |
| **Edit** | Copy formula (`Ctrl+Shift+C`), Reset all controls |
| **View** | Toggle Controls (`Ctrl+B`), Toggle Presets (`Ctrl+P`), Reset layout, Reset camera, Help overlay (`F1`) |
| **Settings** | Appearance… (opens the Settings popover from ticket 14) |
| **Help** | Keyboard & mouse (toggles the help overlay), About Vector Field Explorer |

- **About** shows app name, version from `CARGO_PKG_VERSION`, and
  "GPL-3.0-only" with a link to the repo.
- Keep the screenshot icon at the right end of the bar; it and the File item
  call the same action.
- Do not add File > Open/Save, undo/redo, or recent-files until those features
  exist — no greyed-out placeholders.

## Behaviour

- Click a title to open; while a menu is open, hovering another title switches
  menus; `Esc` closes; clicking elsewhere closes.
- Shortcuts are registered in one place and shown beside menu items. They fire
  only when no text field has focus (except `Ctrl+Q`, which quits from
  anywhere; view state is already persisted, so no save prompt).
- Menu items that need current state (Copy formula) read it at invocation time.
- "Reset camera" restores the default orbit framing; "Reset layout" restores
  expanded panels, default widths, and default help visibility.

## Seam / tests

- New pure `ui::menu` (or extend `ui::top_bar`): `MenuAction` enum and
  `apply_action(action, &mut UiState) -> SideEffect` (side effects: open
  Settings, take screenshot, quit, copy text). UI code only renders and
  dispatches.
- Unit-test: action → side-effect mapping, reset-layout/reset-camera defaults,
  copy-formula returns the current formula, shortcut table has no duplicates,
  every enum variant is reachable from a menu item.
- No UI snapshot tests.

## Acceptance

- [ ] All five menus open, close, and switch correctly; `Esc` works.
- [ ] Every item performs its action; About shows the real version and license.
- [ ] Shortcuts work only outside text fields; table is test-pinned.
- [ ] No dead/greyed placeholder items.
- [ ] Gates: build / `cargo test --all-targets` / clippy `-- -D warnings` /
      `scripts/smoke-run.sh 15`.
- [ ] Commit `feat: top-bar menus (File, Edit, View, Settings, Help)`.

## Non-goals

Undo/redo, file open/save, recent files, configurable keybindings.

## Scheduling

Immediately after ticket 14; before ticket 13's test move and ticket 09's final
review.
