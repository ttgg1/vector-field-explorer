# 14: Window size, mouse controls, hideable sidebars, clearer controls

**What to build (UX feature):** the app currently opens at eframe's default
window size, the 2D view has no mouse navigation (drag only orbits in 3D), the
sidebar controls read as an undifferentiated list, and the left/right panels
cannot be hidden. Make the app usable on a normal desktop: a larger default
window, predictable mouse navigation in both views, toggleable panels, and
clearer grouping/affordances in the controls.

**Status:** ready-for-agent

## Current behaviour (2026-11-08, `main` at ticket 08)

- `src/main.rs` uses `eframe::NativeOptions::default()` — no `inner_size` or
  `min_inner_size`, so the window opens at eframe's small default (800x600) and
  can be shrunk until panels collapse.
- `src/viewmodel.rs:475` `apply_camera_input(camera, mode_3d, dx, dy, zoom)`
  calls `orbit`/`zoom` **only when `mode_3d` is true**. The default state is 2D,
  so left-drag does nothing and the wheel does nothing; there is no pan.
- `src/app.rs:206` and `:289` show `Panel::left("sidebar")` and
  `Panel::right("presets")` unconditionally; `resizable(true)` only changes
  width. There is no hide/show control or shortcut.
- Sidebar sections exist but lack a consistent heading/tooltip/reset pattern,
  and panels give no hint about drag/zoom gestures.

## Desired behaviour

### Window
- Default inner size 1280x800, minimum 960x600, centred; still resizable.
- Build options in a small pure helper (`window_options()`), not inline in
  `main`, so sizes are testable. Keep the existing app title.

### Mouse navigation (both views)
- **3D:** left-drag orbits; wheel/pinch zooms; right-drag (or middle-drag) pans
  the orbit target.
- **2D:** left-drag pans the plane; wheel/pinch zooms; a click without drag
  still seeds streamlines/particles (do not regress ticket 06).
- Cursor feedback over the viewport: grab / grabbing (and a move cursor for
  pan), so the gesture is discoverable.
- One-line hint in the status bar (or a `?` tooltip): "Drag to rotate/pan ·
  Wheel to zoom · Click to seed".
- Keep the camera math in the pure seam: extend `apply_camera_input` (or a
  sibling pure function) to return the new camera for 2D pan/zoom and 3D
  orbit/pan/zoom; unit-test the direction and clamping.

### Hideable sidebars
- Toggle buttons in the always-visible top bar: "Controls" (left) and
  "Presets" (right), plus shortcuts `Ctrl+B` / `Ctrl+P`.
- Hidden panels release their space to the viewport; the toggles stay visible
  in the top bar so a hidden panel can always be restored.
- Persist visibility across restarts through the same eframe app-state storage
  used by `AppStyle` (extend the stored settings; absent/corrupt → both shown).

### Clearer controls
- Group the left sidebar into collapsible sections with headings (Field,
  Sampling, Rendering, Streamlines, Particles, Export), each control labelled
  and given a hover tooltip with units/effect.
- Add a per-section or per-control "reset to default" affordance where it is
  cheap (at minimum for density, arrow scale, spacing, speed).
- Presets panel: make Save / Load / Delete row affordances unambiguous (clear
  primary action, delete confirmation or undo), and show a name field
  placeholder rather than a bare text box.
- Keep all existing behaviour: setters, clamping, dirty flag, and the 2D/3D
  mode gate semantics unchanged unless the ticket explicitly changes them.

## Tests / gates

- Pure tests: window option sizes; 2D drag pans in the expected direction and
  clamps; 3D drag orbits / right-drag pans; wheel zoom both modes; sidebar
  visibility round-trips through a fake `eframe::Storage` and absent/corrupt
  storage shows both panels.
- TDD: each pure helper gets a watched RED first.
- Gates: build / `cargo test --all-targets` / clippy `-- -D warnings` /
  `scripts/smoke-run.sh 15` (touches `src/app.rs` and likely `src/renderer.rs`
  wiring).
- Manual visual check by the user: window opens large; drag rotates in 3D and
  pans in 2D; wheel zooms; both panels hide/restore; controls read clearly.
  The implementer states exactly what a headless environment could not verify.
- Commit: `feat: larger window, mouse navigation, hideable sidebars`.

## Scheduling

Land **before ticket 13** (test/source separation), so ticket 13 moves the new
tests too, and before ticket 09's final whole-branch review. Order:
08 → 14 → 13 → 09 → 10.
