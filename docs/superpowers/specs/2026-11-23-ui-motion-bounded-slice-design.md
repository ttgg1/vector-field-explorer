# UI motion — bounded first slice (issue #289)

Date: 2026-11-23
Status: approved by owner

## Intent

Add restrained, functional motion to the egui interface so it feels modern without
making the physics workspace feel animated or distracting. This is the bounded
first slice chosen in brainstorming: central motion tokens plus application to a
small, testable set of surfaces. It is not the full #289 rollout.

## Goals

- One shared vocabulary for durations/easing and a deterministic sampler.
- Honour a user setting and the platform reduced-motion preference; when reduced,
  state changes are immediate.
- No continuous repaint while idle: repaint only while a transition is in flight.
- Preserve focus, keyboard operation, accessibility labels, compact layout, and
  light/dark appearance.

## Non-goals / anti-goals

- No physics, solver, scene, or analysis changes.
- No new typesetting/parser work or changes to formula behaviour.
- No animation of the vector view itself or of data readouts.
- No decorative or continuous animation; nothing that delays input or hides content.

## Constraints

- Immediate-mode egui: animation requires a small per-frame state that calls
  `ctx.request_repaint()` only while a transition is active, then settles.
- Styling is centralized in `src/ui/style_panel.rs::apply` (egui `Visuals`,
  accent, rounding, fonts).
- `AppStyle` (`src/ui/style_panel.rs`) is persisted in the settings envelope via
  `src/persistence.rs` with `#[serde(default)]`, and sanitized by `AppStyle::sanitized()`.
- Panels/rails are shown in `src/app/panels.rs` and `src/ui/panel_rails.rs`; the
  compact bottom sheet uses `SheetState`. Floating viewport panels use
  `src/ui/viewport_panel.rs` (movable/clamped/resettable from #295).

## Approaches considered

1. **Central tokens + a few surfaces (chosen).** One small motion module wired
   into `style_panel::apply`; hover/press feedback globally, short open/slide
   transitions for the compact sheet and the floating viewport panels. Small,
   testable blast radius near a release.
2. Tokens only, no UI wiring. Smallest and safest, but users see no motion until a
   follow-up; rejected as not delivering the issue's user-visible intent now.
3. Broad rollout across all controls, rails, sheets and transitions. Largest
   blast radius and review surface near a release; deferred to a follow-up.

## Design

### `src/ui/motion.rs` (new)
- `MotionTokens`: durations for `open_slide` (~140 ms) and `hover_press` (~80 ms),
  one ease-out curve. Values are bounded constants; no ad-hoc timings elsewhere.
- `progress(elapsed, duration) -> f32`: clamped `0..=1` sampler (ease-out). Pure
  and deterministic.
- `ReducedMotion`: resolves from the app setting plus the egui/platform
  prefer-reduced-motion signal; exposes `fn active(...)`. When active, every
  transition jumps to its end state on the first frame.

### Setting
- Add `reduced_motion: bool` (default `false`) to `AppStyle`; `#[serde(default)]`
  keeps old records loadable. `AppStyle::sanitized()` needs no clamping for a bool.
- Settings toggle in `style_panel::show_controls`, consistent with existing
  appearance controls. Platform preference is an additional automatic signal.

### Surfaces
1. Global button/toggle hover and press feedback via the egui `Visuals` set in
   `style_panel::apply`, using the shared hover/press token. Must not change
   layout geometry.
2. Short open/slide transition for the compact bottom sheet (`SheetState` open)
   and the viewport floating panels (legend, tool strip, typeset panel), driven
   by the same tokens. `ctx.request_repaint()` is called only while a transition
   is in flight; once complete, no further frames are requested.

### Tests
- RED first: token clamping/sampling, reduced-motion immediate behaviour, and a
  deterministic no-idle-repaint assertion.
- GREEN after implementation; full native gates plus `scripts/smoke-run.sh 15`.
- Wide/compact layout and keyboard/accessibility checks preserved.

## Left to the implementer / planner

- Exact egui API for the slide offset without changing layout geometry.
- Where per-frame transition state lives (App field keyed by panel id).
- How to read the platform reduced-motion signal available in this egui version.
