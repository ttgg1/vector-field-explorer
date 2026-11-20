# Object renderer design — solid source and induction geometry

Design for [#106](https://github.com/ttgg1/vector-field-explorer/issues/106),
following the owner decision recorded there (approach C, generic reusable pass,
Solid/Wireframe switch, GPU-side rendering). See
[ADR 0004](../../adr/0004-object-renderer.md) and the
[engine research note](../../research/2026-11-20-3d-renderer-options.md).
Status: draft for owner approval; no implementation is authorized before that
approval and a bounded plan.

## Goal

Draw the eight source kinds and the induction loop/solenoid as shaded solid
objects, with one global Solid/Wireframe mode, without moving simulation work to
the CPU and without adopting a game engine.

## Non-goals

Picking, selection outlines, shadows, post-processing, CSG, mesh import,
volumetric rendering, per-object solid/wireframe, MSAA controls (an internal
seam only), and any change to the existing arrow/line/dot painter order.

## Architecture

### 1. Pure scene model — `src/object_scene.rs`

```rust
pub enum MeshPrimitive { Sphere, Cylinder, Torus, Tube, Disc }
pub struct MeshObject { primitive, transform: Mat4, color: [f32; 3], opacity: f32, sign: i8 }
pub struct MeshScene { objects: Vec<MeshObject>, generation: u64 }
```

- `MeshScene::from_sources(&SourceScene, slice: Option<&PlaneFrame>)` maps every
  enabled source to one object:
  point charge → sphere; finite line charge → cylinder; charged ring → torus;
  charged disc → thin cylinder; solid sphere → sphere; straight wire → cylinder
  spanning the domain; current loop → torus; solenoid → tube along the helix.
- Induction: one object from `InductionScene` (torus or tube) using the same
  mapping as its unit source. No object when the induction case is disabled.
- Transform is built from centre, axis and radius/length; axis degeneracy falls
  back to `Z`, every value is finite-checked, and degenerate radii/lengths use
  the existing source validation bounds.
- Colours reuse `ELECTRIC_COLOR`/`MAGNETIC_COLOR`; `sign` carries polarity for
  shading tint only. Polarity remains readable through the existing line sign
  markers (below), so colour is never the only cue.
- `MAX_OBJECTS = 64` matches `MAX_SOURCES`; `generation` increments only when
  the scene content changes. Pure, no wgpu, no egui.

### 2. Static primitive meshes

Generated once per process (`OnceLock`), non-indexed triangles carrying
position, normal and barycentric attributes:

| Primitive | Segments | Triangles |
| --- | --- | --- |
| Sphere | 24 × 16 | 768 |
| Cylinder | 24 sides + caps | 96 |
| Disc | 24 sides | 48 |
| Torus | 32 × 12 | 768 |
| Tube (solenoid) | 8 × 6 per turn, 12 turns | 1152 |

Worst case 64 tubes = 73,728 triangles, inside the 150k contract. Vertex data is
bounded at roughly 8 MB worst case; instance data is under 8 KB. These numbers
are asserted by tests, not estimated at runtime.

### 3. GPU pass — `src/object_renderer.rs` + `shaders/object.wgsl`

Follow the proven `PlotDrawState` pattern (`prepare` renders offscreen into the
egui encoder; `paint` composites):

- `ObjectPass` owns the pipeline (format = egui target format, `depth_stencil`
  `Depth32Float`, `LessEqual`, depth write on, alpha blending), one combined
  vertex buffer, one instance buffer per primitive family, a uniform buffer and
  one reused colour+depth target at viewport resolution.
- Uniforms: `view_proj`, camera position, one directional light, slice plane
  (`normal`, `offset`), `wireframe` flag.
- `prepare` is a no-op with zero objects or a zero-area viewport; it releases the
  target there and reports no error. It recreates the target only when the
  viewport size or format changes and uploads instances only when the scene
  generation changes.
- Draw order: one draw per primitive family with non-empty instances. Solid mode
  fills with two-sided lighting (backface normals flipped); wireframe mode
  renders the same geometry with a barycentric edge factor and no fill, no depth
  write changes.
- 2D slice: fragment discard beyond the slice plane. The cut body shows shaded
  backfaces (honest cut shell, no cap fill); a second low-alpha wireframe draw of
  the discarded side gives the faint off-slice cue. The selected plane, sign
  markers and existing off-slice dash semantics stay authoritative.
- `paint` composites a textured quad over the viewport rect with premultiplied
  alpha, before the arrow/line/dot draws.

### 4. Integration points

| File | Change |
| --- | --- |
| `src/renderer.rs` | `DrawState` owns `ObjectPass`; `DrawCallback::prepare` records the object pass, `paint` composites it first; `submit_offscreen` does the same so PNG captures match the live viewport |
| `src/app/mod.rs` | build `MeshScene` from sources + induction + slice alongside `compose_vector_lines` |
| `src/app/viewport.rs` | pass the mesh scene into the callback |
| `src/animation_scene.rs`, `src/cli_scene.rs` | `AnimationFrame` carries `Arc<MeshScene>`; headless animation renders objects |
| `src/source_glyph.rs` | add `source_sign_marker_lines`; Solid mode draws only sign markers, line/both modes unchanged |
| `src/persistence.rs` | `ObjectStyle { mode: Solid, Wireframe }`, serde default Solid, non-dirtying restore |
| `src/ui/sources_panel.rs` | one "Object style" control (Solid/Wireframe) in the display section; help text |
| `docs/CODEBASE.md` | new modules and render-path note |

SVG export keeps the existing line-glyph representation: shaded meshes cannot be
represented honestly as SVG, and the export note must say so. PNG, screenshot and
animation export include objects because they share `DrawState`.

## Behaviour decisions

- Default is Solid; the mode is global, persisted and restored without dirtying
  the scene.
- Wireframe mode suppresses body line glyphs to avoid double geometry but keeps
  sign markers.
- Solid mode suppresses body line glyphs and keeps sign markers; the off-slice
  faint wireframe replaces the dashed body outline.
- No objects means no pass, no target allocation and no measurable cost.
- Objects are opaque; transparency is a later seam. The induction object uses the
  same opacity 1.0 as sources.

## Verification

- Pure tests: mapping per source kind (primitive, transform, sign), induction
  mapping, slice clip inputs, generation stability, cap enforcement, triangle
  budget arithmetic, finite transforms for degenerate input, glyph split.
- Persistence: Solid/Wireframe round trip, missing field defaults to Solid,
  corrupt value sanitizes to Solid, restore does not dirty.
- GPU: one adapter-gated test renders a single sphere offscreen and asserts
  non-background pixels and a clean error scope (the existing plot GPU test is
  the precedent); no assertion on anti-aliasing or exact colours.
- Gates: build, full suite, clippy `-D warnings`, docs check, diff check, changed
  file formatting, `scripts/smoke-run.sh 15`, plus workflow CI (native, wasm,
  windows).
- Screenshot/manual visual verification stays owner-waived; automated pixel
  presence is not a visual judgement.

## Slices

1. Slice 1: pure model, primitives, pipeline, integration, persistence, mode
   control, PNG/animation parity, tests above. No LOD or MSAA.
2. Later: LOD/perf tuning, MSAA seam, picking/selection, per-layer appearance
   (#84), more primitives for the #91 catalogue, 3D direction-colour integration.

## Open questions for the owner

None blocking; the remaining choices (light direction, exact palette tint, edge
width) are builder calls within the design and will be listed in the plan.
