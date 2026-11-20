# 0004: Depth-enabled object renderer for source and induction geometry

Sources and induction objects are currently line glyphs: `src/source_glyph.rs`
expands them into `LineInstanceGPU` segments, and the induction loop/solenoid is
not drawn at all. The owner asked for recognizable solid objects, later reusable
for other teaching geometry, with rendering kept on the GPU so CPU headroom stays
with simulation. The choice is hard to reverse because it introduces a second
render path, a depth target, a shader and an instance format.

## Decision

Build a small, project-owned, depth-enabled instanced mesh pass on the existing
`wgpu` 30 device and composite it into the egui paint callback at the glyph
layer, instead of adopting a third-party engine. Verified engine findings and
citations are in
[`docs/research/2026-11-20-3d-renderer-options.md`](../research/2026-11-20-3d-renderer-options.md);
`rend3` is archived, `screen-13` is Vulkan-only, `three-d` is OpenGL, and Bevy
0.20 matches `wgpu` 30 but pins `egui` 0.36 and owns its own app, assets, window
and render graph.

The pass:

- renders into one reused offscreen colour+depth pair at viewport resolution;
- produces the image in `CallbackTrait::prepare`, then composites a textured
  quad in `paint` before arrows/lines draw, so painter ordering is explicit;
- uses instanced primitive meshes (sphere, cylinder, torus, tube, disc) with a
  per-instance transform, colour and opacity;
- draws one forward shader with simple directional lighting and a barycentric
  edge factor for the global Solid/Wireframe switch;
- rebuilds instance buffers only when the object scene or view changes, and is
  skipped entirely when there are no objects.

## Boundaries

- No engine, ECS, asset server, second window/event loop, picking,
  post-processing, shadows, CSG, mesh import or general solver.
- No change to the existing arrow/line/dot pipelines or their CPU ordering.
- No change to physics semantics: solid mode still shows charge/current sign
  markers and off-slice treatment through the existing glyph model.
- Bounded primitives and triangles (≤64 objects, ≤150k drawn triangles, 2,832
  static triangles), at most 2 ms added GPU time on the 1280×800 RX 9070 XT
  reference scene, finite-input safety, native + wasm parity, and the standing
  screenshot/manual-visual waiver.
- Reusable shape: a plain `MeshScene` (instances + camera + lighting) that later
  arrow, line or catalogue geometry can feed, without making the first slice
  generic beyond that seam.

Status: proposed, pending owner approval of the design and bounded plan.
