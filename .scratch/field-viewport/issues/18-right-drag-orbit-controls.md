# 18: Right-drag orbit + expanded camera controls

**What to build:** make 3D right-drag orbit the camera (today it pans), and
expand the camera/input controls into a discoverable, documented set. Update
the in-app help so every binding is listed.

**Status:** ready-for-agent — after 14 merges (input seam) and coordinates with
15 (Help menu). User request 2026-11-08.

## Current behaviour (at `b6b669a`)

- 3D: left-drag orbits, **right/middle-drag pans**, wheel/pinch zooms.
- 2D: drag pans, wheel/pinch zooms.
- Click (press+release without drag) seeds streamlines.
- Help overlay lists drag, right/middle-drag pan, wheel, click, formula.

Right-drag-to-pan is the odd one out: most 3D tools use right-drag to orbit or
to open a context action, and users reach for it expecting rotation.

## Target bindings

### 3D

| Input | Action |
|---|---|
| Left-drag | Orbit |
| **Right-drag** | **Orbit (new)** |
| Middle-drag | Pan |
| **Shift + right-drag** | **Pan** (trackpad/no-middle-button fallback) |
| Wheel / pinch | Zoom |
| Double-click viewport | Reset camera to the default 3D view |
| Arrow keys | Orbit (Shift+arrows pan) |
| `+` / `-` | Zoom in/out |
| `R` | Reset camera |
| `F` | Fit the domain in view |

### 2D

| Input | Action |
|---|---|
| Left/right-drag | Pan |
| Wheel / pinch | Zoom |
| Double-click viewport | Reset to the default 2D front view |
| Arrow keys | Pan (Shift+arrows pan faster) |
| `+` / `-` | Zoom in/out |
| `R` | Reset camera |
| `F` | Fit the domain in view |

### Global (unchanged unless ticket 15 moves them)

- `Ctrl+B` / `Ctrl+P` — toggle left/right panel rails.
- `F1` or `?` — show/hide the help overlay (add; today only the × hides it).
- `Esc` — close Settings/Help popovers and dismiss the help overlay.

Bindings must not swallow formula-bar typing: skip camera key handling while a
text field has focus, except the existing global panel shortcuts (documented
ruling from ticket 14).

## Expanded controls surface

1. **Help overlay** becomes a compact table grouped by mode (3D/2D/global),
   listing every binding above. Keep it dismissible and persisted.
2. **Help menu entry** (ticket 15) opens the same table as a window rather than
   duplicating strings.
3. **Camera buttons**: add `Reset view` and `Fit domain` next to the camera
   preset controls (or in the viewport corner), so the actions are clickable,
   not keyboard-only.
4. **Cursor feedback**: orbit = grab/grabbing; pan = move; zoom unchanged.

A full key-rebinding UI, gamepad support, and touch-gesture overhaul are
non-goals.

## Seams and tests

Keep logic pure and outside the renderer:

- Map pointer buttons in `src/app.rs` to `ViewGesture`; right-drag must set
  `drag` (orbit), not `pan`.
- Add a small pure `ViewCommand` / `apply_view_command(camera, mode, command)`
  for reset, fit, orbit, pan, and zoom; keyboard and double-click call it.
- Unit tests, all in `tests/`:
  - right-drag orbits in 3D and pans in 2D;
  - middle-drag and Shift+right-drag pan in 3D;
  - click-to-seed still fires only when the gesture never became a drag;
  - keyboard orbit/pan/zoom respects the existing clamps;
  - reset/fit produce deterministic cameras for a square and a rectangular
    domain;
  - help text lists every binding exactly once (3D, 2D, global).
- Browser check: right-drag must not open the browser context menu; verify in
  the wasm build when ticket 16 lands.

## Acceptance

- [ ] 3D right-drag orbits; middle-drag and Shift+right-drag pan.
- [ ] 2D right-drag pans; no accidental orbit.
- [ ] Double-click, `R`, `F`, `+`/`-`, and arrow keys work in both modes.
- [ ] Camera keys are inert while the formula field has focus.
- [ ] Help overlay/menu lists every binding; no stale "right-drag pans" copy.
- [ ] Reset/Fit buttons are clickable and match the keyboard actions.
- [ ] `cargo build`, `cargo test --all-targets`, clippy `-- -D warnings`,
      `scripts/smoke-run.sh 15` all pass.
- [ ] Manual: right-drag orbit feels correct with a mouse; context menu never
      appears; cursor feedback matches the action.

## Scheduling

After ticket 14 merges (it owns the current gesture seam) and after ticket 15
fixes the Help/menu surface. Wasm must inherit the same bindings in ticket 16.
