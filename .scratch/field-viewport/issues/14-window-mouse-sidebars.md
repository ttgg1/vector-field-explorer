# 14: Desktop UI — window, panel rails, mouse navigation, help overlay

**What to build (UX feature):** make the app comfortable on a normal desktop.
It currently opens at eframe's small default, both side panels are permanently
visible, 2D has no drag/zoom navigation, style controls sit above field
controls, and the sidebar gives no units or reset affordances. Agreed design
(2026-11-08): larger default window, collapsible edge rails for both panels, a
top menu bar with Settings/Help/screenshot, a dismissible help overlay, mouse
navigation in both views, clearer control grouping, and full view-state
persistence.

**Status:** ready-for-agent

## Agreed design

### Window
- Default inner size 1280x800, minimum 960x600, centred; resizable.
- Remember last size/position across restarts (eframe window persistence).
- Keep the title "Vector Field Explorer".

### Top bar
- A slim **top menu bar** above the formula row:
  - **View**: toggle Controls panel, toggle Presets panel, reset layout,
    reset camera.
  - **Settings**: opens the Settings popover.
  - **Help**: toggles the help overlay.
  - Screenshot action at the right end of the menu row (moved out of the
    sidebar).
- Formula row below it: field-preset selector, 2D/3D toggle, editable formula.
- The menu row is the future host for File/Edit; do not add dead menus now.

### Panels collapse to edge rails
- Controls (left) and Presets (right) each collapse to a narrow vertical rail
  with a short label (and chevron), not a top/bottom bar.
- Clicking the rail expands and **pins** the panel; a second click (or
  `Ctrl+B` / `Ctrl+P`) collapses it. No hover-peek.
- First launch: both expanded. Thereafter the collapsed/expanded state is
  remembered. No auto-collapse on small windows.
- Hidden panels release their width to the viewport; the rail remains as the
  restore affordance.

### Settings popover
- Non-modal popover from the top bar containing:
  - existing appearance controls (`AppStyle`: theme, accent, rounding,
    spacing, font size);
  - "Show help overlay" toggle;
  - "Reset layout" (panels expanded, default widths).
- Live preview stays visible while adjusting appearance.

### Help overlay
- Grey, dismissible text in the viewport's bottom-left corner: drag/wheel
  gestures per mode, click-to-seed, and a one-line formula syntax reminder.
- Small dismiss ×; visibility is a setting. Visible by default on first run,
  remembered after.

### Mouse navigation
- **3D:** left-drag orbits; right-drag (or middle-drag) pans the orbit target;
  wheel/pinch zooms; click without drag seeds.
- **2D:** left-drag pans; wheel/pinch zooms; click without drag seeds.
- Cursor feedback over the viewport: grab / grabbing for drag, move for pan.
- Extend the pure camera-input seam and unit-test direction/clamping; keep the
  click-seed path intact (ticket 06 behavior).

### Controls clarity
- Reorder the left sidebar to **Display → Features → Coloring → Domain**.
- Each section stays collapsible and gains a reset-to-default button.
- Tooltips with units/effect, e.g. "Density: samples per axis", "Normalize:
  scale arrows to unit length".
- Display and Features open by default; Coloring and Domain start collapsed.
- Remove Export from the sidebar (screenshot now lives in the top bar).

### Presets panel
- Explicit "Save current view" button; clear active-row highlight; delete with
  confirmation. Duplicate-name save still replaces (existing ruling).

### Persistence
- Persist view state: formula, 2D/3D mode, domain, and all control values.
- Persist layout (panel visibility, widths, help visibility) and style.
- Camera state resets to the default on launch; camera presets remain the way
  to restore a viewpoint.
- Absent/corrupt stored state falls back to defaults (never brick startup).

## Tests / gates

- Pure tests: window option sizes; 2D pan and zoom direction/clamping; 3D pan;
  layout/view-state persistence round-trip via a fake `eframe::Storage`
  (absent/corrupt → defaults); help-overlay default visibility.
- TDD: watched RED first for each pure helper.
- Gates: build / `cargo test --all-targets` / clippy `--all-targets -- -D
  warnings` / `scripts/smoke-run.sh 15`.
- Manual visual check by the user: window opens large; rails expand in one
  click; both panels hide/restore; drag orbits 3D and pans 2D; wheel zooms;
  help overlay readable and dismissible; controls legible with tooltips.
  The implementer states exactly what a headless environment could not verify.
- Commit: `feat: desktop UI — rails, navigation, help, persistence`.

## Scheduling

Land before ticket 13 (test/source separation) so the moved tests include the
new ones, and before ticket 09's final review. Order: 14 → 13 → 09 → 10.
