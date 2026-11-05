# Vector Field Explorer — Design

Date: 2026-11-05

## Intent

A desktop Rust app (fresh project, no existing codebase) where the user types a
vector-field formula at runtime and sees it visualized live, in 2D and 3D, with
a modern, customizable UI. It is a math playground first: type, see, iterate.

## Goals

- Formula input parsed live on every keystroke. 2D: `f(x,y) -> (u,v)`; 3D:
  `f(x,y,z) -> (u,v,w)`. Entered as one tuple like `(sin(y), x*cos(y))` or, for
  3D, `(x,-y,z)`. Variables `x y z`; standard math functions; no time variable
  `t` in v1 (static fields only).
- Invalid input → friendly inline parse error; the last valid field keeps
  rendering.
- Presets dropdown (vortex, dipole, shear flow, source/sink, curl / no-curl
  examples) that loads editable formulas; doubles as syntax examples.
- Viewport rendered by a real GPU scene (wgpu). 2D mode = arrow grid with an
  orthographic camera; 3D mode = orbit camera (drag rotate, scroll zoom),
  lit, depth-tested arrows built as instanced shaft+head geometry, plus
  grid, axes, and a bounding box.
- Sidebar sections (collapsing): Display (sample density, arrow length
  scaling, normalize toggle), Coloring (viridis / turbo / monochrome,
  auto-fit or manual magnitude range), Features (streamlines with seeds,
  animated particles, speed), Domain (adjustable extent sliders/inputs,
  default `[-1, 1]` per axis).
- Streamlines **and** animated particles exist in **both** 2D and 3D.
  Seeds: a preset seed set derived from the domain plus click-to-seed.
  In 3D a click seeds via the camera ray. RK4 integration.
- Sidebar and panel/viewport split are **resizable** (drag handles).
- Bottom status bar: parse status, instance count, FPS.
- Style section: dark-first theme, accent color picker, corner rounding /
  spacing / font-size sliders, settings persisted to disk.
- Screenshot button exports the viewport as PNG via a native save dialog
  (`rfd`).

## Non-goals (v1)

- No data/file import (CSV, NumPy, etc.).
- No volume rendering, no fondant-level polish of shading beyond lit arrows.
- No `t` (unsteady fields), no browser build, no SIMD optimization.

## Approaches considered and rejected

- **Pure-egui software-projected 3D** (project arrows onto 2D painter, sort by
  depth): rejected by the user in favor of a real GPU scene.
- **three-d engine integration**: rejected in favor of raw wgpu via eframe's
  built-in `egui_wgpu` callback API — one dependency less, egui stays the UI
  layer, and we control all shaders.
- **egui painter for 2D arrows**: rejected; routing 2D through the same
  instanced GPU renderer (ortho projection) avoids a second arrow code path
  and makes the screenshot export a straight offscreen re-render of the view.

## Decisions made (by the user)

- GPU scene via wgpu (not software projection).
- Streamlines/particles in 3D too (ray-based seeding).
- Adjustable domain extent, default `[-1, 1]`.
- Screenshot via native save dialog.
- Resizable sidebar.

## Calls delegated to the builder (already settled)

- Tuple strings are split at top-level commas; each component is compiled with
  `meval` 0.2 independently; each picks up whichever of x/y/z it references.
  The component count (2 vs 3) infers the mode; a 3D field shown in 2D view
  renders `(u, v)` and ignores `w` (noted in the UI).
- NaN/∞ (division by zero at vortex centers, etc.): magnitude clamped,
  degenerate arrows skipped — never a crash or blank viewport.
- Colormaps via the `colorous` crate (exact viridis and turbo), plus a
  monochrome ramp; magnitude range auto-fits the current field by default.
- Numbers: 2D density default 24×24 (cap 200×200); 3D default 16³ (cap 40³);
  particle count cap 5000; RK4 step scaled to the current max magnitude.
- Streamline seeding: preset seed set = a ring / grid derived from the domain,
  plus click seeds; max points per polyline bounded; segment count capped.
- Persistence: eframe's `persistence` feature (serde-derived app state saved
  to the OS-appropriate config dir) — covers theme, sliders, last formula.
- Screenshot mechanism: offscreen re-render of the scene (arrows, streamline
  overlays are UI-only and may be excluded) into a texture, read back, encode
  PNG (`image` crate), save via `rfd`. Exported PNG = the scene, not the whole
  window chrome.
- Math crate: `glam`; GUI: `eframe`/`egui` 0.37 with the `persistence`
  feature; `rfd` for dialogs; `image` for PNG encoding.

## What's left to the builder

- Exact shader code, instance layouts, pipeline construction details.
- The unproject math for 3D click-to-seed (camera ray through the click
  point; seed at a chosen distance / first crossing of the domain box).
- Minor visual styling (background, grid color) consistent with the modern
  dark-first look.
- Widget wiring details inside the sections.
