# 3D object renderer options (2026-11-20)

Research for [#106](https://github.com/ttgg1/vector-field-explorer/issues/106):
rendering sources and induction objects as shaded solid geometry. This note is
recommendation-only; it authorizes no dependency or code change.

## Question

Can a Rust game/render engine replace a hand-written object renderer, given the
existing eframe/egui-wgpu application, native + WebAssembly delivery, GPL-3.0-only
compatibility, and a hard requirement that rendering stay on the GPU with small
CPU cost?

## Existing stack (verified in this repository)

| Component | Version |
| --- | --- |
| `eframe` / `egui-wgpu` | 0.37.0 |
| `wgpu` | 30.0.1 |
| `winit` | 0.30.13 |
| `glam` | 0.30 |
| Rust edition | 2024 |
| Local toolchain | rustc 1.99.1 |

The live viewport is an egui paint callback: `DrawCallback` uploads `SceneGPU`
and draws arrow, line and dot pipelines with `depth_stencil: None` into egui's
own render pass. The plot renderer already owns an offscreen colour+depth pair
for capture, which proves a depth-enabled pass is feasible inside this app.

## Candidates

Metadata below comes from the crates.io API (`/api/v1/crates/<name>` and
`/<name>/<version>`), fetched 2026-11-20.

| Candidate | Latest | Updated | License | Verdict |
| --- | --- | --- | --- | --- |
| `rend3` | 0.3.0 | 2022-02-12 | MIT OR Apache-2.0 OR Zlib | Rejected: repository `BVE-Reborn/rend3` is archived (last push 2024-07-08) and no release in over four years. |
| `screen-13` | 0.13.0 | 2025-12-14 | MIT OR Apache-2.0 | Rejected: Vulkan-only engine; no WebGPU/wasm target, so it breaks the native + wasm product scope. |
| `three-d` | 0.19.0 | 2026-04-17 | MIT | Rejected: OpenGL/WebGL renderer (`glow`), integrated through `egui_glow`. The app renders through wgpu, so this needs a second graphics context and a different backend. |
| `fyrox` | 1.0.1 | 2026-03-28 | MIT | Rejected: full engine with its own renderer, asset pipeline, window and event loop; same embedding problem as Bevy with a smaller ecosystem. |
| `bevy` | 0.20.1 | 2026-10-28 | MIT OR Apache-2.0 | Rejected for now: closest fit, but not a drop-in (see below). |

### Why Bevy is close but still not a drop-in

Verified facts, all from the crates.io API:

- `bevy_render` 0.20.1 depends on `wgpu ^30`, the same major version already in
  this tree, so a Bevy renderer would not duplicate wgpu.
- `bevy` 0.20.1 requires Rust 1.97.1; this host has 1.99.1.
- `bevy_egui` 0.43.1 depends on `egui ^0.36`, while this app uses `egui 0.37`.
  Using it would downgrade the UI stack or leave two egui versions.
- Bevy's renderer is driven by `bevy_app`, `bevy_ecs`, `bevy_asset`,
  `bevy_window`/`bevy_winit` and its render graph. Rendering into an existing
  egui callback texture is possible through a headless/extract setup, but that
  is a large integration surface for three object families, and it brings
  another scheduler, asset server and window stack into both native and wasm
  builds.

## Recommendation

Write a small, reusable object-renderer module on the existing `wgpu` 30 device,
owned by this project, instead of adopting an engine:

1. Instanced primitive meshes (sphere, cylinder, torus, tube/helix, disc) with a
   per-instance transform, colour and material record.
2. Offscreen colour + depth target at viewport resolution, reused across frames;
   composite the result into the egui pass at the glyph layer.
3. One forward shader with simple directional lighting and a barycentric
   edge factor for the solid/wireframe switch; no shadows, post-processing,
   picking, CSG or mesh import in the first slice.
4. CPU work limited to rebuilding instance buffers on scene/view change; GPU does
   transform, lighting and rasterization.
5. One draw per primitive family, bounded primitives and triangles, optional MSAA
   (off by default).

Rationale: zero new dependencies and no second event loop, device or UI stack;
wasm support already proven; keeps GPL-3.0-only compatibility trivial; and the
module remains reusable for future arrow/line geometry and the #91 catalogue.
The cost is writing roughly one shader plus one pipeline module, which is far
smaller than embedding a game engine's render graph.

## Things worth borrowing from game engines

- Instanced draws and material batching instead of per-object bind groups.
- A depth target and consistent depth bias for coplanar glyphs.
- A single shader with an edge factor for wireframe, avoiding line-topology
  duplication.
- Explicit budget/LOD decisions and a cheap fallback (existing line glyphs) for
  dense scenes.

## Open design questions (for the ADR and design doc)

- Compositing order against translucent arrows and lines that currently rely on
  CPU back-to-front sorting.
- 2D-slice cross-section clipping rules and the faint off-slice wireframe.
- Exact instance and triangle caps, and the wasm equivalent of the native
  performance budget.
