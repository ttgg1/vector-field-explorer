# Depth-Enabled Object Renderer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give sources and induction objects recognizable shaded 3D bodies through a small reusable instanced mesh pass on the existing `wgpu` device, with a persisted global Solid/Wireframe switch.

**Architecture:** A pure `object_scene` module maps sources to instanced primitive transforms and static meshes; an `object_renderer` pass renders them offscreen into a reused colour+depth target and a textured quad composites that image before the existing arrow/line draws; the app, animation and CLI feed the same `MeshScene` so every capture matches the viewport.

**Tech Stack:** Rust 2024, `wgpu` 30, `egui`/`eframe` 0.37, `glam` 0.30, existing `PlotDrawState` pattern. No new dependency.

**Spec:** `docs/superpowers/specs/2026-11-20-object-renderer-design.md` (accepted 2026-11-20; ADR `docs/adr/0004-object-renderer.md`).

## Global Constraints

- Native and wasm must both build; no Vulkan-only or native-only dependency.
- GPL-3.0-only compatible dependencies; add none.
- Single simulation clock; object rendering is a pure function of the current scene.
- Default object style is Solid; the style is global, persisted and never dirties the scene on restore.
- Budgets: `MAX_OBJECTS = 64`, `MAX_DRAWN_TRIANGLES = 150_000`, static geometry 2,832 triangles.
- No picking, post-processing, shadows, CSG, mesh import, LOD or MSAA in this plan.
- All geometry must be finite for validated inputs; degenerate values never produce NaN transforms.
- Ordinary test runs are offline and must not require a GPU. Device tests are env-gated exactly like `tests/plot_gpu_test.rs`.
- Every task runs the full native gates before acceptance: `cargo build`, `scripts/test.sh`, `cargo clippy --all-targets -- -D warnings`, `python3 scripts/check-docs.py`, `git diff --check`, changed-file formatting.

## Review Focus

- **Degenerate dimensions:** zero/NaN radius, length or axis, and inverted domains, must yield finite bounded geometry or an honestly skipped object, never NaN or an inside-out transform.
- **Solenoid honesty:** the tube covers the scene length with at most 12 visible turns, matching the existing glyph cap; the source's numeric `turns` stays authoritative in panel/export text.
- **Zero objects or zero-area viewport:** no target allocation, no draw, no stale pixels, no error.
- **Capture parity:** PNG, screenshot, GIF/video frames and CLI animation must show the same objects as the live viewport because they share the pass.
- **Restore semantics:** missing or corrupt persisted style falls back to Solid without marking the scene dirty.
- **Painter ordering:** objects composite before arrows/lines/dots; no object draw may cover an arrow that should be in front.

---

### Task 1: Static primitive meshes and budget

**Files:**
- Create: `src/object_scene.rs`
- Create: `tests/object_scene_test.rs`
- Modify: `src/lib.rs` (one `pub mod object_scene;`)
- Modify: `tests/suite.rs` (one `mod object_scene_test;`)
- Modify: `docs/CODEBASE.md` (module map row and source-coverage note)

**Interfaces:**
- Produces: `MeshPrimitive { Sphere, Cylinder, Torus, Tube, Disc }`, `MeshPrimitive::ALL`, `triangles()`, `vertices()`, `index()`, `from_index(u32)`, `MeshVertex { position: [f32; 3], normal: [f32; 3], barycentric: [f32; 3] }`, `PrimitiveMesh { primitive, vertices: &'static [MeshVertex] }`, `primitive_mesh(MeshPrimitive) -> &'static PrimitiveMesh`, `STATIC_TRIANGLES: usize`, `STATIC_VERTICES: usize`, `VERTEX_BYTES: usize = 36`.

- [ ] **Step 1: Write the failing budget and mesh-shape tests**

```rust
#[test]
fn primitive_triangle_counts_match_the_budget() {
    assert_eq!(MeshPrimitive::Sphere.triangles(), 768);
    assert_eq!(MeshPrimitive::Cylinder.triangles(), 96);
    assert_eq!(MeshPrimitive::Torus.triangles(), 768);
    assert_eq!(MeshPrimitive::Tube.triangles(), 1152);
    assert_eq!(MeshPrimitive::Disc.triangles(), 48);
    assert_eq!(STATIC_TRIANGLES, 2_832);
    assert_eq!(STATIC_VERTICES, 8_496);
    assert_eq!(STATIC_VERTICES * VERTEX_BYTES, 305_856);
}

#[test]
fn every_primitive_mesh_has_finite_vertices_and_unit_normals() {
    for primitive in MeshPrimitive::ALL {
        let mesh = primitive_mesh(primitive);
        assert_eq!(mesh.vertices.len(), primitive.vertices());
        for v in mesh.vertices {
            assert!(v.position.iter().all(|x| x.is_finite()));
            assert!(v.barycentric.iter().all(|x| *x == 0.0 || *x == 1.0));
            let len = (v.normal[0] * v.normal[0] + v.normal[1] * v.normal[1] + v.normal[2] * v.normal[2]).sqrt();
            assert!((len - 1.0).abs() < 1e-4, "{primitive:?} normal {len}");
        }
    }
}

#[test]
fn sphere_vertices_lie_on_the_unit_sphere() {
    for v in primitive_mesh(MeshPrimitive::Sphere).vertices {
        let r = (v.position[0].powi(2) + v.position[1].powi(2) + v.position[2].powi(2)).sqrt();
        assert!((r - 1.0).abs() < 1e-4);
    }
}

#[test]
fn cylinder_spans_unit_height_and_has_caps() {
    let mesh = primitive_mesh(MeshPrimitive::Cylinder);
    assert!(mesh.vertices.iter().any(|v| (v.position[1] - 0.5).abs() < 1e-6));
    assert!(mesh.vertices.iter().any(|v| (v.position[1] + 0.5).abs() < 1e-6));
    assert!(mesh.vertices.iter().any(|v| (v.normal[1] - 1.0).abs() < 1e-6));
}

#[test]
fn disc_is_two_sided_and_flat() {
    let mesh = primitive_mesh(MeshPrimitive::Disc);
    assert!(mesh.vertices.iter().all(|v| v.position[1].abs() < 1e-6));
    assert!(mesh.vertices.iter().any(|v| v.normal[1] > 0.9));
    assert!(mesh.vertices.iter().any(|v| v.normal[1] < -0.9));
}

#[test]
fn tube_has_twelve_visible_turns() {
    let mesh = primitive_mesh(MeshPrimitive::Tube);
    let min = mesh.vertices.iter().map(|v| v.position[1]).fold(f32::INFINITY, f32::min);
    let max = mesh.vertices.iter().map(|v| v.position[1]).fold(f32::NEG_INFINITY, f32::max);
    assert!((max - min - 1.0).abs() < 1e-4, "tube spans unit height");
    assert_eq!(MeshPrimitive::Tube.triangles(), 12 * 8 * 6 * 2);
}
```

- [ ] **Step 2: Run to verify RED**

Run: `cargo test --test suite object_scene_test -- --nocapture`
Expected: FAIL to compile with `unresolved import vector_field_explorer::object_scene`.

- [ ] **Step 3: Implement the model**

Generate each mesh with nested loops (sphere 24×16, cylinder 24 sides + two caps, torus 32×12, disc 24 rim segments as a two-sided fan, tube 12 turns × 8 sides × 6 steps). Emit non-indexed triangles with per-vertex normals and barycentric `[1,0,0]`, `[0,1,0]`, `[0,0,1]` per triangle. Cache with `OnceLock<PrimitiveMesh>`, one per variant. Unit conventions: sphere/torus/cylinder/disc radius 1 or 0.5 as the tests demand, cylinder height 1 centred on Y, tube radius 1 and height 1 centred on Y, axis +Y before the instance transform. No `unsafe`.

- [ ] **Step 4: Run to verify GREEN**

Run: `cargo test --test suite object_scene_test`
Expected: 6 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/object_scene.rs tests/object_scene_test.rs src/lib.rs tests/suite.rs docs/CODEBASE.md
git commit -m "feat: add static object primitive meshes"
```

### Task 2: Scene mapping, slice clip and content fingerprint

**Files:**
- Modify: `src/object_scene.rs`
- Modify: `tests/object_scene_test.rs`
- Modify: `docs/CODEBASE.md`

**Interfaces:**
- Consumes: `MeshPrimitive`, `primitive_mesh`, `Domain`, `SourceScene`, `PlaneFrame`, `SourceKind`, `SourceDisplay`, `ELECTRIC_COLOR`, `MAGNETIC_COLOR`.
- Produces: `MeshObject { primitive: MeshPrimitive, transform: [[f32; 4]; 4], color: [f32; 3], opacity: f32, sign: i8 }`, `MeshScene { objects: Vec<MeshObject>, generation: u64 }`, `MeshScene::empty()`, `MeshScene::from_sources(&SourceScene, &Domain, Option<&PlaneFrame>)`, `MeshScene::objects()`, `MeshScene::drawn_triangles()`, `MeshScene::generation()`, `MeshScene::clip() -> Option<SliceClip>`, `SliceClip { normal: [f32; 3], offset: f32 }`, `MAX_OBJECTS`, `MAX_DRAWN_TRIANGLES`, `object_transform(center: Vec3, axis: Vec3, scale: Vec3) -> [[f32; 4]; 4]`.

Mapping (extent = largest `Domain::domain_size()` component, floor `1e-6`; `unit = 0.05 * extent`):

| Kind | Primitive | Scale | Sign |
| --- | --- | --- | --- |
| `PointCharge` | Sphere | radius `unit` | charge sign |
| `FiniteLineCharge` | Cylinder | radius `0.02 * extent`, length = `length` | charge sign |
| `ChargedRing` | Torus | major `radius`, minor `0.02 * extent` | charge sign |
| `ChargedDisc` | Disc | radius `radius`, thickness `0.02 * extent` | charge sign |
| `SolidSphere` | Sphere | radius `radius` | charge sign |
| `StraightWire` | Cylinder | radius `0.012 * extent`, length `2.2 * extent` | current sign |
| `CurrentLoop` | Torus | major `radius`, minor `0.02 * extent` | current sign |
| `Solenoid` | Tube | radius `radius`, length `length` | current sign |

> **Amendment (2026-11-21):** the `minor 0.02 * extent` entry above is the
> **vertical** minor radius only. A single per-instance linear transform cannot
> independently control the torus minor radius, so ring and current-loop bodies
> scale the torus (major `0.75`, minor `0.25`) by `radius / 0.75` radially and
> `0.02 * extent / 0.25` vertically: vertical minor `0.02 * extent`, radial minor
> `radius / 3`. Visual consequence: rings read proportionally thicker radially
> at large source radius. A dedicated thin-ring primitive with proportional wall
> thickness is deferred to a later renderer slice (follow-up candidate).

- [ ] **Step 1: Write the failing mapping tests**

Tests must assert, for one source of every kind: the primitive, the sign, the family colour, a finite transform with determinant > 0, and the mapped centre. Add: component axes are orthonormal for a non-Z axis; a zero axis maps like `Z`; a NaN radius still yields a finite transform; `Clip` equals the frame normal and plane offset; `display = Both` with `Electric`/`Magnetic` filters by family; disabled entries emit nothing; a `SourceScene` with 64 enabled sources yields 64 objects and `drawn_triangles() <= MAX_DRAWN_TRIANGLES`; `from_sources` twice on the same scene yields the same `generation`, while changing one radius changes it; `MeshScene::empty()` has zero objects, zero triangles and a generation that never collides with a non-empty scene.

- [ ] **Step 2: Run to verify RED**

Run: `cargo test --test suite object_scene_test`
Expected: FAIL to compile on the missing `MeshScene`/`MeshObject` items.

- [ ] **Step 3: Implement mapping and the FNV-1a fingerprint**

Use `PlaneFrame::normal` and `normal.dot(origin)` for `SliceClip`; build the transform as basis columns (right, up, normal) × scale, with `Vec3::Z` fallback and renormalization; compute `generation` by hashing primitive index, sign, opacity bits, colour bits and all 16 transform `f32` bit patterns. Colour is `[r/255, g/255, b/255]` from the family constant, opacity `1.0`. Extra entries beyond `MAX_OBJECTS` are dropped; that cannot happen through the validated `SourceScene`, so no truncation warning is needed.

- [ ] **Step 4: Run to verify GREEN**

Run: `cargo test --test suite object_scene_test`
Expected: all object-scene tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/object_scene.rs tests/object_scene_test.rs docs/CODEBASE.md
git commit -m "feat: map sources to object instances"
```

### Task 3: Offscreen object pass and shader

**Files:**
- Create: `src/object_renderer.rs`
- Create: `shaders/object.wgsl`
- Create: `tests/object_gpu_test.rs`
- Create: `scripts/test-object-gpu.sh`
- Modify: `src/lib.rs` (one `pub mod object_renderer;`)
- Modify: `tests/suite.rs` (one `mod object_gpu_test;`)
- Modify: `docs/CODEBASE.md`

**Interfaces:**
- Consumes: `MeshScene`, `MeshObject`, `MeshPrimitive`, `primitive_mesh`, `SliceClip`.
- Produces: `ObjectDrawState { pub pass: ObjectPass }`, `ObjectPass::new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self`, `ObjectPass::prepare(&mut self, device, queue, encoder, scene: &MeshScene, view: &ObjectView) -> Result<(), String>`, `ObjectPass::paint(&self, render_pass: &mut wgpu::RenderPass<'_>, screen: egui::Rect)`, `ObjectPass::texture_view() -> Option<&wgpu::TextureView>`, `ObjectView { pub view_proj: [[f32; 4]; 4], pub eye: [f32; 3], pub viewport: [u32; 2], pub wireframe: bool, pub clear: [f64; 4] }`, `OBJECT_PROBE_ENV: &str = "VFE_OBJECT_GPU_PROBE"`.

- [ ] **Step 1: Write the failing CPU contract test**

`tests/object_gpu_test.rs` runs a `cpu_contract_checks()` that always executes: zero objects → `prepare` returns `Ok` without allocating (`texture_view().is_none()`), zero-area viewport → `Ok` and no target, and `ObjectView` round-trips into the uniform array with finite values. A `native_object_device_worker` test prints `GPU not requested` unless `VFE_OBJECT_GPU_PROBE=1`; with the variable set it requires an adapter, renders one sphere over a known clear colour, and asserts at least one pixel differs from the clear colour and that no validation error was reported.

- [ ] **Step 2: Run to verify RED**

Run: `cargo test --test suite object_gpu_test`
Expected: FAIL to compile with the missing `object_renderer` module.

- [ ] **Step 3: Implement the pass and shader**

Follow `src/plot_renderer.rs`'s `PlotDrawState` structure: pipeline with `depth_stencil` `Depth32Float`, `LessEqual`, depth write, alpha blend, two-sided lighting, `SliceClip` discard when present, barycentric wireframe when `wireframe` is true, and a fullscreen textured composite with premultiplied alpha. Target and pipelines are created lazily; recreate the target only when `viewport` or format changes; upload instances only when `generation` changes. Shader uses one directional light, ambient floor and the slice plane in world space. No `unsafe`.

- [ ] **Step 4: Run to verify GREEN**

Run: `cargo test --test suite object_gpu_test` (expect CPU contracts pass, `GPU not requested`), then `scripts/test-object-gpu.sh` on the native host (expect the device probe to pass and report a non-background pixel).

- [ ] **Step 5: Commit**

```bash
git add src/object_renderer.rs shaders/object.wgsl tests/object_gpu_test.rs scripts/test-object-gpu.sh src/lib.rs tests/suite.rs docs/CODEBASE.md
git commit -m "feat: add offscreen object renderer"
```

### Task 4: Live viewport, capture and animation integration

**Files:**
- Modify: `src/renderer.rs` (`DrawState`, `DrawCallback::prepare`/`paint`, `submit_offscreen`)
- Modify: `src/app/mod.rs` (build `MeshScene` next to `compose_vector_lines`)
- Modify: `src/app/viewport.rs` (pass the scene into the callback)
- Modify: `src/animation_scene.rs`, `src/cli_scene.rs` (`AnimationFrame` carries `Arc<MeshScene>`)
- Test: `tests/renderer_test.rs`, `tests/animation_scene_test.rs`, `tests/cli_scene_test.rs`

**Interfaces:**
- Consumes: `MeshScene::from_sources`, `ObjectPass`, `ObjectDrawState`.
- Produces: `DrawState::set_object_scene(Arc<MeshScene>)`, `DrawState::object_scene() -> Option<&Arc<MeshScene>>`, `AnimationFrame::object_scene: Arc<MeshScene>`, `RenderCallbackData { ..., pub objects: Arc<MeshScene>, pub object_style: ObjectStyle }`.

- [ ] **Step 1: Write the failing integration tests**

Assert: `DrawState` starts with no object scene; setting one keeps its generation; `AnimationSnapshot::capture` copies the object scene into `AnimationFrame`; a CLI scene built from sources carries the same object count as `MeshScene::from_sources`; `submit_offscreen` renders objects before arrows by checking the recorded command order helper (add a pure `object_pass_precedes_arrows()` predicate if ordering is not observable otherwise).

- [ ] **Step 2: Run to verify RED**

Run: `cargo test --test suite renderer_test animation_scene_test cli_scene_test`
Expected: FAIL on the missing accessors/fields.

- [ ] **Step 3: Integrate**

In `DrawCallback::prepare`, call `ObjectPass::prepare` before recording scene draws; in `paint`, composite the object image first, then existing pipelines. Update `submit_offscreen` identically. Keep the current CPU sort untouched. Skip the pass when the scene has no objects.

- [ ] **Step 4: Run to verify GREEN and smoke**

Run: `cargo test --test suite renderer_test animation_scene_test cli_scene_test`; then `scripts/smoke-run.sh 15` (report the actual result; do not claim a visual pass).

- [ ] **Step 5: Commit**

```bash
git add src/renderer.rs src/app/mod.rs src/app/viewport.rs src/animation_scene.rs src/cli_scene.rs tests/renderer_test.rs tests/animation_scene_test.rs tests/cli_scene_test.rs
git commit -m "feat: composite objects in the viewport and captures"
```

### Task 5: Slice treatment, sign markers and glyph suppression

**Files:**
- Modify: `src/source_glyph.rs` (add `source_sign_marker_lines`, split body emission)
- Modify: `src/app/mod.rs` (choose line bodies vs sign markers by object style)
- Modify: `src/object_renderer.rs` (faint discarded-side wireframe draw)
- Test: `tests/source_glyph_test.rs`, `tests/object_gpu_test.rs`

**Interfaces:**
- Consumes: `SourceGlyph`, `ObjectStyle`.
- Produces: `pub fn source_sign_marker_lines(glyphs: &[SourceGlyph], domain: &Domain) -> Vec<LineInstanceGPU>` and `pub fn source_body_lines(glyphs: &[SourceGlyph], domain: &Domain) -> Vec<LineInstanceGPU>`; `source_glyph_lines` keeps its current signature as `body + markers` for SVG and wireframe callers.

- [ ] **Step 1: Write the failing glyph-split tests**

Assert: `source_body_lines` emits no sign marker; `source_sign_marker_lines` emits exactly two segments for positive and one for negative glyphs per source; `source_glyph_lines` equals body plus markers for every kind; in Solid and Wireframe object modes the composed viewport lines contain only markers; in line mode the composition equals the legacy function.

- [ ] **Step 2: Run to verify RED**

Run: `cargo test --test suite source_glyph_test`
Expected: FAIL to compile on the missing functions.

- [ ] **Step 3: Implement the split and slice cue**

Refactor `emit_sign_marker` to be callable independently; keep colours/widths/dash unchanged. In the shader, discard fragments on the far side of the slice plane and draw the discarded side once more as attenuated wireframe. No new geometry.

- [ ] **Step 4: Run to verify GREEN**

Run: `cargo test --test suite source_glyph_test object_gpu_test` and `scripts/test-object-gpu.sh`.

- [ ] **Step 5: Commit**

```bash
git add src/source_glyph.rs src/app/mod.rs src/object_renderer.rs tests/source_glyph_test.rs tests/object_gpu_test.rs
git commit -m "feat: slice object bodies and keep sign markers"
```

### Task 6: Persisted style, UI control, docs and performance evidence

**Files:**
- Modify: `src/persistence.rs` (`ObjectStyle` on `AppSettings`, sanitize, non-dirtying restore)
- Modify: `src/ui/sources_panel.rs` (one "Object style" control in the display section, help text)
- Modify: `src/app/mod.rs` (apply restored style without dirtying)
- Modify: `docs/CODEBASE.md`, `docs/perf/object-renderer.md` (new evidence note)
- Test: `tests/persistence_test.rs`, `tests/sources_panel_test.rs`

**Interfaces:**
- Produces: `pub enum ObjectStyle { Solid, Wireframe }` with `Default = Solid`, `pub struct ObjectStyleSettings { pub style: ObjectStyle }`, `AppSettings.object_style: ObjectStyle`, `State::object_style`, `State::set_object_style(style) -> bool` (dirties only on change), `State::restore_object_style(style)` (never dirties).

- [ ] **Step 1: Write the failing persistence/UI tests**

Assert: missing field loads Solid; `"Wireframe"` round-trips; corrupt text sanitizes to Solid and logs; restore does not dirty; `set_object_style` returns false for the same value and true for a change; the panel control writes the same enum; the help text names both modes and that SVG keeps line glyphs.

- [ ] **Step 2: Run to verify RED**

Run: `cargo test --test suite persistence_test sources_panel_test`
Expected: FAIL on the missing field/enum.

- [ ] **Step 3: Implement persistence and control**

Use `#[serde(default)]`, sanitize with the existing logging style, and assign directly during startup restore. Add the radio/selectable pair in the existing display section, not a new panel.

- [ ] **Step 4: Measure and record**

On the RX 9070 XT host, render the 64-object reference scene at 1280×800 and record prepare/paint CPU time plus frame-time delta in `docs/perf/object-renderer.md`, with the method, build flags and a clear statement that it is native evidence, not a CI assertion or a visual judgement.

- [ ] **Step 5: Run all gates and commit**

```bash
cargo build
scripts/test.sh
cargo clippy --all-targets -- -D warnings
python3 scripts/check-docs.py
git diff --check
scripts/smoke-run.sh 15
git add src/persistence.rs src/ui/sources_panel.rs src/app/mod.rs docs/CODEBASE.md docs/perf/object-renderer.md tests/persistence_test.rs tests/sources_panel_test.rs
git commit -m "feat: persist the global object style"
```

---

## Self-Review Notes

- Every design section maps to a task: model/mapping (1–2), GPU pass (3), integration/captures (4), slice/sign markers (5), persistence/UI/docs/perf (6).
- The `generation` field is a stable content fingerprint rather than a monotonic counter; it changes only with content, which preserves the specified upload behaviour without cross-frame state.
- Solenoid turns stay capped at 12 visible, matching `MAX_VISIBLE_TURNS` in the existing glyph model.
- No task adds a dependency, LOD, MSAA, picking or post-processing.
