# Spec — Interactive field viewport & viewer features (fields-explorer)

Status: ready-for-agent

## Problem Statement

A user can already type a formula and see a static grid of arrows rendered in
the center panel, and camera presets persist between sessions. But they cannot
actually *explore* a field: the camera is frozen (no orbit, pan or zoom), the
display is not adjustable (density, arrow size, colors are hardcoded), the
bottom-right panel cannot stream streamlines or animate particles, there is no
way to seed anything by clicking, no light/dark/visual preferences, no way to
save a picture of what they see, and no README explaining any of it. The
viewport is a picture, not an instrument.

## Solution

Finish the explorer so the viewport behaves like an instrument:

- The formula bar, sidebar and status bar become live: edits re-sample the
  field immediately; an invalid expression keeps the last good view and says
  what went wrong; sidebar controls adjust arrow density, arrow scale,
  normalization, colormap, color range and the sampled domain extent.
- The camera becomes interactive: orbit by dragging, zoom by scrolling, pan,
  and a toggle between a 2D orthographic look (flat, like a plot) and a 3D
  perspective orbit camera. Everything above the GPU paints from one shared
  scene description.
- Streamlines trace paths through the field (RK4) with a click-to-seed; the
  same click logic seeds animated particles that advect with the flow and
  respawn, capped so the UI stays smooth. Clicking works in 2D and, via a
  camera-ray into the scene, in 3D.
- A style panel offers theme (dark/light), accent color, rounding, spacing
  and font size, applied immediately and remembered across restarts.
- A snapshot action renders the current viewport to a PNG through the system
  file dialog, with a sensible default filename.
- A README documents install, usage, the formula language and controls.

## User Stories

1. As a user, I want my formula edits to refresh the arrows immediately, so that I can mutate a field and see its shape change in real time.
2. As a user, I want an invalid formula to keep the last good view on screen, so that half-typed expressions never blank my viewport.
3. As a user, I want a status line that tells me the field failed to parse and what is wrong, so that I can fix my formula without guessing.
4. As a user, I want a density control, so that busy fields stay readable and sparse fields still cover the domain.
5. As a user, I want an arrow-scale control, so that I can trade clutter for visibility of direction.
6. As a user, I want a normalize toggle, so that I can choose between speaking arrows (magnitude proportional to length) and uniform arrows colored by magnitude.
7. As a user, I want to pick a colormap (Viridis, Turbo, Mono), so that I can match my display to my taste or colorblindness.
8. As a user, I want a color-range control with an automatic fit option, so that contrast follows the data or my fixed mapping of choice.
9. As a user, I want to adjust the sampled domain extent, so that I can zoom the analysis to the region I care about.
10. As a user, I want the 2D/3D mode toggle, so that planar fields look flat and 3-component fields get their third dimension.
11. As a user, I want to orbit the scene by dragging, so that I can look at the flow from any angle.
12. As a user, I want to zoom with the scroll wheel (clamped), so that I can get close without losing the scene entirely.
13. As a user, I want to pan the view, so that I can bring an off-center structure to the middle.
14. As a user, I want a reset-view control, so that I can recover from a disorienting camera state.
15. As a user, I want a light grid and axis lines behind the arrows, so that I can orient myself in the domain.
16. As a user, I want to click in the viewport to drop a streamline seed, so that I can trace a path through the exact region I'm curious about.
17. As a user, I want a streamline-length cap, so that runaway vortices don't produce infinite curves.
18. As a user, I want streamlines to follow the field in both directions from the seed, so that downstream and upstream structure both show up.
19. As a user, I want streamline tracing to stay bounded near singularities, so that a pole neither hangs the app nor floods it with geometry.
20. As a user, I want particles to stream along the flow when I enable them, so that I can perceive direction at a glance.
21. As a user, I want particles (like arrows) sampled so NaN/zero regions contribute nothing, so that 1/x poles degrade gracefully instead of crashing.
22. As a user, I want a particle cap, so that a pathological field cannot tank frame rate.
23. As a user, I want clicking in 3D mode to seed via a ray from the camera, so that I can pick points that lie within the domain even when the view is rotated.
24. As a user, I want a style panel with dark/light theme and an accent color, so that the app fits my working environment.
25. As a user, I want my style choices (rounding, spacing, font size) applied instantly and remembered after restart, so that I don't re-tune every session.
26. As a user, I want a Snapshot button, so that I can save the exact framing I am looking at.
27. As a user, I want the snapshot to offer a default filename, so that repeated saves don't force me to name files by hand.
28. As a user, I want the field-preset sidebar (formulas per preset) untouched by these changes, so that my existing workflow keeps working.
29. As a user, I want my camera presets (save/load/delete) to keep working with the new interactive camera, so that tracked views remain meaningful.
30. As a user, I want the app to keep launching and rendering correctly on first run with no presets directory, so that a fresh install degrades gracefully.
31. As a user, I want a README with install instructions, formula syntax and control summary, so that I can get productive without reading source.
32. As a user, I want the window to stay responsive with a maximum 3D grid, so that pushing density to the cap doesn't freeze the UI.

## Implementation Decisions

- **Seam structure (confirmed with the human partner):** everything testable
  flows through the existing pure-module seams — formula parse/evaluate, the
  domain grid, colormaps, field sampling, and the camera's matrices/rays —
  plus future pure modules for streamline tracing, particle stepping, styles
  and screenshot naming. Exactly ONE new seam is allowed: a viewmodel /
  scene-builder seam of pure functions that turn application state plus input
  events into the scene description and camera actions, so the UI layer stays
  thin and the wiring logic is testable without a window. The GPU paint layer
  is bounded behind the scene description and stays build-plus-visual only
  (already ruled in the plan ledger).
- **Camera:** one orbit camera drives both modes; in 2D mode it renders
  orthographically onto the z=0 plane, in 3D mode perspective. The camera's
  view-projection matrix and NDC-ray unprojection are the only camera surface
  the scene builder consumes.
- **Streamlines:** fixed-step RK4 with configurable length cap (400 points
  max), step scale, and optional both-directions tracing; seeds come from the
  preset seed ring or from viewport clicks converted through the camera ray.
- **Particles:** spawn/advect/respawn with RK4; a capped particle set; the
  random source is a small inline LCG so particle logic and its tests need no
  RNG dependency. Particles recycle (respawn) rather than accumulate.
- **3D seeding:** a click produces a camera NDC ray; the seed is the ray's
  parameterized intersection with the z=0 plane (3D mode: additionally with
  the domain box surface) — no depth buffer involvement.
- **Styling:** a style struct (theme, accent, rounding, spacing, font size)
  applied to the UI context each frame; persisted through the persistent
  app-state storage the framework already provides, keyed consistently with
  the existing app key.
- **Snapshot:** builds the same scene offscreen as the live viewport, encodes
  a PNG (the existing save-PNG path is reused; RGBA byte order is already
  pinned by test), and hands the path to the system save dialog through the
  file-dialog crate, re-added to dependencies deliberately for that single
  purpose; the default filename rule is its own pure function (tested).
- **Caps & guards:** 2D grid defaults to 24 and never exceeds 200×200; 3D
  grid defaults to 16³ and never exceeds 40³; particle set capped (5000);
  NaN and near-zero vectors are skipped during sampling; all-zero fields
  produce an empty scene rather than division by anywhere.
- **Persistence interplay:** camera presets and app styles are independent
  records; loading a camera preset restores exactly the camera state it
  saved (already tested), and style load never resets camera presets.

## Testing Decisions

- Tests verify external behavior through the module seams only — a good test
  asserts what a caller observes (parsed results, sampled arrows, traced
  points, particle positions, applied style values, generated file names),
  never internal representation.
- The viewmodel/scene-builder seam gets the integration-style coverage:
  state + a sim-input produce the expected scene/camera-action, following
  the existing sampling and camera test style as prior art.
- Streamline tests mirror the plan's vortex-case: bounded length around a
  rotational field, correct endpoints for a linear field, both-directions
  symmetry. Particle tests cover spawn caps, respawn on leaving the domain,
  and determinism of the LCG sequence.
- Style persistence is tested by round-tripping the stored state, not by
  launching windows. Screenshot naming is a pure-function test.
- GPU paint (shaders, pipelines, callbacks) has no unit tests per plan
  ruling: verified by build, clippy, and a manual visual pass by the human
  partner.

## Out of Scope

- Importing data files (fields come from formulas only).
- Any time dimension or unsteady fields.
- Rendering beyond arrows, line segments and dots (no volume rendering,
  no stream-ribbons, no vector-field slicing UI).
- Multi-language expansion beyond the existing translation skeleton.
- Export formats beyond PNG (SVG stays unimplemented, as already ruled).
- Restructuring completed modules (field, domain, colormap, sampling,
  presets, export) beyond what a seam requires.

## Further Notes

- The binding design authority is the project's design spec in
  docs/superpowers/specs/, and the task-level plan in docs/superpowers/plans/
  carries the exact interfaces and caps; this spec covers the remaining
  viewport tasks (T8–T14) on top of the completed foundation (shell, formula
  engine, domain grid, field-preset table, colormaps, sampling, orbit camera,
  instanced GPU renderer, camera presets).
- Rulings already recorded in the plan ledger remain binding: no unit tests
  for GPU code; shader split into arrow/line sources; CPU far-to-near sort
  instead of a depth attachment; visual verification deferred to the human
  partner (headless environment).
- The issue tracker is local markdown under .scratch/<feature-slug>/; break
  implementation into NN-slug.md issue files referencing this spec.
