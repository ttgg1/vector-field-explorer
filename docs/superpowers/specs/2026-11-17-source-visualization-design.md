# Source visualization — glyphs, selected slice, presets and export warnings

**Status:** APPROVED by owner on 2026-11-17 for Issue [#26](https://github.com/ttgg1/vector-field-explorer/issues/26). The owner approved **Approach A**, the projected/off-slice 2D glyph policy, and this detailed design. Implementation follows the approved plan and independent review.

**Issue:** [#26](https://github.com/ttgg1/vector-field-explorer/issues/26) under physics epic [#17](https://github.com/ttgg1/vector-field-explorer/issues/17).

**Spec authority:** “Source visualization — glyphs, z slice, presets and export accuracy warnings”. This design does not add geometry kinds, a second solver, x/y slice reorientation, drag placement, grouping, a private timer/softening, or CLI scope. #13 owns selected-plane particle/streamline seeding; #24/#25 own evaluator/source scene; #15 is closed. This design consumes those seams.

## Intent and why

The source scene now evaluates and draws arrows, but the user cannot see *where a source is*, *which kind it is*, *which way it points*, or *whether its sign is positive or negative*. In 2D the viewport shows a selected slice of the same 3D field; sources outside that slice would otherwise vanish. Exporting a source view also loses the disclosed softened-model and accuracy context, so a saved image can look more authoritative than the data.

This change makes sources visible as explicit scene glyphs, keeps the selected 2D slice honest without moving sources onto it, adds illustrative source presets without turning classic formula presets into source physics, and carries a shared provenance/warning contract into the existing viewport and export paths.

## Goals

- Draw a pure, testable glyph for every enabled source: kind, location, orientation, family, and signed charge/current.
- Show all source glyphs in 2D by projecting them onto the selected `PlaneFrame`; mark off-slice sources with dashed, translucent treatment instead of hiding them.
- Recompute glyphs and source samples on the existing dirty/rebuild and plane-transition path; never keep stale glyphs after a plane, domain, density, display, or scene change.
- Add a separate source-preset catalogue covering all eight source kinds across coherent examples, loaded through the ordinary source-scene setters.
- Keep classic formula presets as formulas; label existing 2D physics formulas as “2D approximation”; never silently convert saved formulas or camera presets into source physics.
- Derive one shared provenance/warning object from `SourceSampling` plus `SourceScene` for the viewport, PNG and SVG.
- Bake a legible visible warning/source-context banner into source PNG exports and include the same context in PNG text metadata; include visible SVG text and `<metadata>`.
- Never export non-finite or unavailable data as valid vectors; the accuracy opt-in never suppresses the warning or bypasses caps.
- Add pure scene/sampling/glyph/preset/export tests, integration tests through the real app seams, and a native benchmark report for the densest source preset.

## Non-goals / anti-goals

- No new source kinds, kernels, field grammar, solver, render backend or per-source softening.
- No x/y slice reorientation, drag placement, arbitrary grouping, per-geometry issue split, or forms beyond the existing panel.
- No reimplementation of #13’s plane/seeding/particle behavior, or of #15’s CLI scope.
- No numeric sum of E and B; no combined “magnitude” across unlike quantities.
- No formula-to-source conversion, saved-formula rewrite, or invented physics claims in presets.
- No claim that a waived browser/manual visual gate passed.
- A glyph is a contextual source marker, not a depth-tested physical surface; it may show through field geometry and must be documented that way.

## Constraints and shared contract

- Teaching-grade native + wasm; pure CPU data/model; `SceneGPU` remains the paint boundary.
- Single scene-wide Plummer softening and existing evaluator accuracy request. E and B remain separate outputs.
- SI-like labels are display-only; no unit engine.
- GPL-3.0-compatible dependencies only; preserve wasm; native CAS stays native-only.
- Tests live under `tests/`; no inline `#[cfg(test)]` under `src/`.
- Screen-space text in exports must be part of the image/SVG document, not GUI chrome captured by accident.
- Native/functional gates apply. Browser/manual visual verification remains waived, not passed.

## Approaches considered

1. **Approach A — pure glyph model in the existing line pass (chosen).** `SourceGlyph` values derived from the source scene are turned into `LineInstanceGPU` segments and appended to the existing line pass. A shared provenance object feeds viewport text and export overlays. Source presets live in a separate catalogue. Lowest new architecture, pure tests, reuses `SceneGPU`/SVG/PNG outputs. Limitation: glyphs are not depth-tested and cannot use a new shader.
2. **Approach B — a new glyph render pass and shader.** Better depth tiers and dash styling, but duplicates pipeline/export work and adds GPU/browser risk for a source marker. Rejected for this ticket.
3. **Approach C — paint glyphs as egui overlays only.** Easy screen UI, but glyphs would not exist in `SceneGPU`/PNG/SVG and could not satisfy export or pure scene tests. Rejected.

## Design

### 1. Pure glyph model

New module `src/source_glyph.rs` owns source-marker semantics; it never samples the field and never changes source math.

```rust
pub enum GlyphSign { Positive, Negative }

pub struct SourceGlyph {
    pub family: FieldFamily,
    pub kind: SourceKindTag,
    pub center: Vec3,        // world in 3D; local plane coordinates in 2D
    pub axis: Vec3,          // projected in 2D, normalized where nonzero
    pub sign: GlyphSign,
    pub radius: f32,         // zero for kinds without a radius
    pub length: f32,         // zero for kinds without a length
    pub turns: u32,          // solenoid only, zero otherwise
    pub off_slice: bool,     // 2D only; false in 3D
    pub domain_scale: f32,   // marker sizing basis, finite and positive
}
```

`SourceKindTag` gains `glyph_sign` or an equivalent pure helper reading the signed `charge`/`current`; zero strengths are already rejected, so every enabled glyph has a definite sign. `axis` is normalized with a safe fallback to +Z only if input validation were bypassed; the validated path must not need the fallback.

`source_glyphs(scene: &SourceScene, domain: &Domain, plane: Option<&PlaneFrame>) -> Vec<SourceGlyph>`:

- Walk `scene.entries` in list order; skip disabled entries.
- Include electric-family kinds only when `display` is `Electric` or `Both`; magnetic kinds only for `Magnetic` or `Both`.
- In 3D, use world centre/axis and `off_slice = false`.
- In 2D, convert the centre and axis into the plane frame, set `center.z = 0`, and mark `off_slice` when the absolute signed normal distance exceeds `16 × f32::EPSILON × max(domain_extent, 1.0)`. This derives the tolerance from representable f32 precision rather than a hand-picked world distance.
- `domain_scale = 0.05 × max(scene-domain extent, 1e-6)`; all marker dimensions are multiples of it and remain finite even for a degenerate domain.

`source_glyph_lines(glyphs, domain, plane) -> Vec<LineInstanceGPU>` builds the actual line segments:

- Colours: electric family uses `ELECTRIC_COLOR` (amber), magnetic uses `MAGNETIC_COLOR` (cyan), converted to the renderer’s `[0,1]` line-colour convention. Family colour is independent of the colormap so a glyph still identifies the family in `Both`.
- On-slice/3D lines use alpha `0.9` and an absolute world width `0.005 × max(scene extent, 1e-6)`; off-slice lines use alpha `0.45` and `0.0035 × max(scene extent, 1e-6)`. The renderer line width is in world units, so widths never depend on viewport pixels. `domain_scale` sizes marker geometry only.
- Off-slice segments are emitted as alternating on/off runs of `4 × domain_scale` and `3 × domain_scale` in 3D scene units. Dashes are separate short segments, so the existing line shader needs no dash state.
- Every glyph carries a small sign marker at an offset from its centre: two perpendicular strokes for positive, one stroke for negative. The marker is part of the same line pass.
- Kind shapes:
  - Point charge: a three-axis cross in 3D; the in-plane cross in 2D.
  - Finite line charge: a segment of the source length along its axis plus perpendicular end ticks.
  - Charged ring: a 32-segment circle of the source radius in the plane normal to the axis.
  - Charged disc: the ring circle plus two diameter strokes.
  - Solid sphere: three mutually perpendicular 32-segment great circles.
  - Straight wire: a line along its axis of `2.2 × scene extent`, suggesting an infinite wire, plus two direction chevrons fixed by current sign.
  - Current loop: a 32-segment circle plus three tangent chevrons showing current direction via the right-hand rule; reversing the sign reverses the chevrons.
  - Solenoid: a helical line of the source length/radius and `min(turns, 12)` visible turns, plus two end circles and one direction chevron. Capping visible turns is presentation only; it never changes the source.
- Segment counts are bounded by the fixed templates above; a scene of 64 sources cannot create unbounded glyph work.

Integration: `src/app/simulation.rs::rebuild_scene` computes glyph lines from the current `SourceScene`, `scene_domain()` and `plane_frame()`, and appends them after the reference/flow/click lines. Source mode has no streamlines/particles/clicks, so these are the only extra lines. Formula mode emits none. Glyphs do not enter `streamline_count`. Rebuild happens only on the existing dirty flag; plane changes already mark that dirty, so no stale glyph list exists.

### 2. Selected 2D slice and #13 coordination

The glyph path uses the same `PlaneFrame` as source sampling; it never creates a second plane model, camera basis, seeding pass, or cache. In 2D a source at a different normal coordinate remains visible at its projected location with the off-slice style. On-slice sources use the solid style. The `SourceScene` display and enabled state filter glyphs exactly as they filter arrows.

Acceptance test `3d_to_selected_slice_samples_are_equivalent` samples the same world points through: (a) the 3D source sampling and (b) the 2D `PlaneFrame` path with the same plane coordinate and density, then compares projected direction/magnitude within a derived `f32` tolerance. This pins “same 3D source field, selected slice” without touching #13’s particle or streamline seeding.

Plane change tests assert glyph/sample outputs change in the same rebuild, and the existing #13 transition tests stay unchanged. Full 3D and ordinary formula behavior is untouched.

### 3. Source presets and migration

New `src/source_presets.rs` with:

```rust
pub struct SourcePresetEntry {
    pub enabled: bool,
    pub kind: SourceKindTag,
    pub center: [f64; 3],
    pub axis: [f64; 3],
    pub strength: f64,
    pub radius: f64,
    pub length: f64,
    pub turns: u32,
}
pub struct SourcePreset {
    pub name: &'static str,
    pub description: &'static str,
    pub display: SourceDisplay,
    pub softening: f64,
    pub entries: &'static [SourcePresetEntry],
}
pub fn source_presets() -> &'static [SourcePreset];
pub fn load_source_preset(name: &str, state: &mut State) -> bool;
```

Catalogue (unique names; all eight kinds; coherent teaching examples):

- `Coulomb point` — one positive point charge at the origin.
- `Electric dipole` — equal positive/negative point charges at ±0.5 on x.
- `Charged line` — positive finite line charge, length 2, along y.
- `Uniform ring` — positive charged ring, radius 1, axis z.
- `Thin disc` — positive charged disc, radius 1, axis z.
- `Charged sphere` — positive solid sphere, radius 1.
- `Infinite wire` — positive current along z.
- `Current loop` — positive current loop, radius 1, axis z.
- `Finite solenoid` — positive solenoid, radius 0.5, length 2, 8 turns, axis z.
- `Source gallery` — all eight kinds spaced symmetrically; the densest integrated preset and benchmark scene.

Every preset uses the normal `SourceScene` validation seam, softening `0.1`, and `allow_inaccurate = false`. Charge and current strengths are an illustrative value of `1.0` in every preset; the shapes and distances carry the teaching point, and the benchmark reports any preset whose evaluator diagnostics do not reach the target instead of tuning around it. Loading sets `PlotKind::VectorField`, `FieldMode::Sources`, the canonical domain `[-2, 2]³`, the preset scene, and a 3D view. The canonical domain is part of the preset load: the catalogue is authored at unit scale, so preserving an unrelated `[0,10]³` domain would make the preset invisible or oversized. The Presets panel requests `ViewCommand::Fit` after a successful source-preset load so the user sees the canonical framing immediately; the camera preset store is untouched. Loading a preset leaves the previous source scene in place if validation fails.

Existing `presets.rs` formula presets stay formulas. Add a pure `Preset::is_2d_approximation` flag or helper for the existing 2D electrostatics/magnetostatics formula rows (Dipole, Source and sink, Point charge, Like charges, Quadrupole, Straight wire); the Presets panel renders a `2D approximation` badge on those rows only. No formula text changes. Saved camera presets are camera-only and untouched. There is no saved field-preset store to migrate: current payloads already restore formula/source mode through `ViewRecord`; legacy payloads with no source fields restore Formula and an empty source scene. A test pins that legacy formula payload behavior rather than inventing a conversion.

The Presets panel gains a `Sources (illustrative)` section above or beside formula presets, using `load_source_preset`; the panel highlights a clicked source row separately from formula and camera rows. Source preset clicks never edit formula drafts.

### 4. Shared export provenance and warnings

New module `src/export_provenance.rs` owns the single text contract used by the viewport, PNG and SVG:

```rust
pub struct ExportProvenance {
    pub mode_3d: bool,
    pub display: SourceDisplay,
    pub softening: f64,
    pub accuracy_not_met_total: usize,
    pub accuracy_not_met_drawn: usize,
    pub unavailable: usize,
    pub validation_error: Option<String>,
    pub warning: Option<String>,
}
pub fn source_export_provenance(sampling: &SourceSampling) -> Option<ExportProvenance>;
pub fn provenance_lines(provenance: &ExportProvenance) -> Vec<String>;
pub fn provenance_metadata(provenance: &ExportProvenance) -> Vec<(String, String)>;
```

Only Sources mode produces provenance. `warning` is `Some` when accuracy-not-met results exist (drawn or hidden), unavailable results exist, or validation failed. The model line is always present for Sources exports: `Sources · softened teaching model · a = <softening>`; the accuracy line names drawn vs hidden counts and the display. Exact copy is shared constants, not re-typed in each path:

```rust
pub const MODEL_LINE: &str = "Sources - softened teaching model - a = {softening}";
pub const ACCURACY_LINE: &str =
    "{display} - accuracy not met: {hidden} hidden, {drawn} drawn";
pub const UNAVAILABLE_LINE: &str = "Unavailable: {count}";
pub const INVALID_LINE: &str = "Source scene invalid: {error}";
pub const NO_SAMPLING_LINE: &str = "Source sampling unavailable";
```

`source_export_provenance` accepts `Option<&SourceSampling>` plus the `SourceScene` and field mode. In Formula mode it returns `None`. In Sources mode with `None` sampling it returns a provenance whose `warning` is `Some(NO_SAMPLING_LINE)`, with zero counts and no arrows: export never silently omits the warning when a rebuild has not completed. A validation-error sampling still yields provenance with the error and a warning. `allow_inaccurate` never changes the presence of the warning.

Viewport: `src/app/panels.rs` paints the same compact provenance text in the viewport when Sources mode is active, below the existing status/`Both` legend. This is GUI text only; export does not rely on it.

PNG:

- Extend the screenshot request to carry an `Option<ExportProvenance>` captured at request time, so a later scene change cannot alter an in-flight export. The pending PNG variant owns both the capture generation and the captured provenance; the button handler captures it from `State`, and the viewport consumes the pending struct when the frame is ready.
- Before encoding, `screenshot::apply_export_banner(rgba, width, height, provenance)` draws a bottom-left translucent backing rectangle and the provenance lines with a small built-in 5×7 uppercase bitmap font (no new font dependency, no GUI context). Scale is at least 2 pixels and grows with image height; lines are clipped/shortened to width. This makes the warning part of decoded pixels.
- Extend `src/export.rs` with `encode_png_with_text(rgba, width, height, metadata)` using the already-locked `png 0.18` crate as a direct dependency. Write `vfe.source_model`, `vfe.softening`, `vfe.display`, `vfe.accuracy_not_met_total`, `vfe.accuracy_not_met_drawn`, `vfe.unavailable`, and `vfe.validation_error` tEXt entries. Sanitize every value to printable ASCII, replacing anything else with `?`. Existing no-provenance PNG encoding stays on the image path.
- Native and wasm save paths both accept the provenance; the browser path applies the banner and metadata before download.

SVG:

- Add `scene_to_svg_with_provenance(scene, view_proj, width, height, clear, provenance)`; keep `scene_to_svg` as the no-provenance wrapper.
- Emit an escaped `<metadata>` block with model/softening/display/accuracy/unavailable/validation fields, then a visible `<text>` provenance banner over a translucent `<rect>`. Both are part of the SVG document, not GUI chrome.
- The SVG request captures provenance at request time alongside the pending generation, exactly like PNG; the later SVG build receives the captured value, so an intervening scene edit cannot drop or replace the warning. The pending SVG variant owns the captured provenance.
- `scene_to_svg_with_provenance` also converts each serialized line/trail world-unit width to a screen-pixel `stroke-width` from the projected endpoints, clamped to `0.5..=3.0`; the renderer's world widths were never pixel widths. This makes source glyphs and existing reference lines legible in SVG without changing GPU rendering. Tests assert a finite positive width and the fallback for a degenerate projection.
- Existing scalar SVG guards and the 2D-only rule are unchanged. Source SVG export is a 2D vector path like any vector SVG; 3D still uses PNG.

Non-finite/unavailable guard: provenance never creates vector data. Test scenes with a rejected source/validation error produce no arrows and `warning` set; a scanned export contains no NaN/Inf coordinates. `allow_inaccurate = true` changes only which finite `AccuracyNotMet` arrows draw and never clears the warning or caps.

### 5. UI, help and docs

- Sources panel gains a `Glyphs` visibility note? No new toggle is required by the issue: glyph visibility follows `display` plus enabled state. The panel/status shows on-slice vs off-slice glyph counts for orientation.
- The viewport provenance line explains off-slice dashed glyphs and the softened model/accuracy status.
- Help/GLOSSARY/README/docs describe: glyph shapes/sign, projected off-slice treatment, selected z-slice behavior, source presets vs formula presets/2D approximation, export warning and metadata.
- No drag placement, grouping or extra canvas controls.

## Testing

All tests live in `tests/` and are registered in `tests/suite.rs`.

### Pure glyph/scene tests (`tests/source_glyph_test.rs`)

- `glyphs_cover_every_kind_with_family_colour_and_sign` — all eight kinds, positive/negative strengths, family selection, unique shape/segment signature.
- `glyph_positions_and_axes_follow_the_source` — translated/rotated sources; point/line/ring/wire orientation endpoints.
- `disabled_and_filtered_sources_do_not_emit_glyphs` — disabled entries and `Electric`/`Magnetic`/`Both` filtering.
- `off_slice_glyphs_project_to_the_slice_and_are_dashed` — source at z=2 on a z=0 plane: local centre z=0, `off_slice = true`, dashed/translucent segments; an on-slice source solid.
- `degenerate_domains_keep_glyphs_finite` — tiny/zero-extent axes produce finite segments and no NaN.
- `glyph_lines_do_not_enter_the_streamline_budget` — line count/`streamline_count` separation through the app scene assembly.

### Slice/sampling tests (`tests/source_slice_test.rs`)

- `3d_to_selected_slice_samples_are_equivalent` — same world points through 3D and `PlaneFrame` 2D paths.
- `plane_change_rebuilds_glyphs_and_samples_together` — no stale glyph/sample geometry after axis/index changes; existing #13 transition tests unchanged.
- `full_3d_and_formula_behavior_is_unchanged` — source-mode-only guards; formula path still samples and renders as before.

### Preset tests (`tests/source_presets_test.rs`)

- `source_preset_names_are_unique_and_cover_every_kind` — all eight kinds, unique names, each preset validates.
- `source_presets_load_mode_strength_softening_and_finite_or_diagnosed_results` — loads through `load_source_preset`, checks mode/display/softening, and samples each at a modest grid (finite or explicit unavailable, never panic/NaN).
- `classic_formula_presets_remain_formulas_and_2d_physics_is_labelled` — formula preset load leaves `FieldMode::Formula`; only the specified existing rows carry the `2D approximation` label.
- `legacy_payload_keeps_formula_and_empty_sources` — legacy `ViewRecord` restore never fabricates sources.
- `saved_camera_presets_are_untouched`.

### Export tests (`tests/source_export_test.rs`)

- `png_export_contains_decoded_warning_pixels_and_metadata` — encode a synthetic provenance, decode PNG, find banner pixels and tEXt keywords/values.
- `svg_export_contains_visible_text_and_metadata` — parse text/metadata; warning escaped and present.
- `warning_cannot_be_suppressed_by_accuracy_opt_in` — same provenance with opt-in on/off still has warning; counts differ truthfully.
- `nonfinite_or_unavailable_scenes_export_no_valid_vectors` — no NaN/Inf coordinates in scene/PNG/SVG; warning set.
- `export_provenance_is_captured_at_request_time` — in-flight export keeps the warning it started with.
- `formula_exports_have_no_source_banner_or_metadata`.

### Benchmark / integration

- `scripts/bench-source-presets.sh` runs an `#[ignore]` release-mode test over `Source gallery` at 40³ and the 2D cap, reporting sampling/evaluator time, diagnostics, platform/backend and hardware. It compares the one-frame source-sampling cost to the #18 performance context (30 fps / ~33 ms integrated-GPU densest frame) and records the gap honestly. It is a sampling benchmark, not a full app fps claim.
- Native smoke and the full functional gates run after implementation. Browser/manual visual gates remain waived, not passed.

## Acceptance mapping

| Issue acceptance | Design/test owner |
|---|---|
| Glyph positions/orientations/kind/sign, visibility, family, slice equivalence | `source_glyph_test`, `source_slice_test` |
| Plane changes/no stale/#13 contract | dirty rebuild + `source_slice_test`; #13 tests unchanged |
| Preset uniqueness/all eight/mode/strength/softening/finite-or-diagnosed/legacy labels | `source_presets_test` |
| Warning in supported image output, metadata, nonfinite, opt-in | `source_export_test`, `export_provenance` |
| Benchmark vs #18, native gates, waivers honest | `scripts/bench-source-presets.sh` + handoff report |
| Full 3D/ordinary formula intact | `source_slice_test`, existing regression suites |

## Risks and design calls

- **Contextual overlay, not depth-tested geometry.** Glyphs share the line pass, so they can show through field geometry. Accepted: they are source markers; documentation says so.
- **Banner legibility.** Built-in 5×7 bitmap text is deterministic and dependency-free; its scanline test proves pixels, not beauty. Manual visual verification is waived, not claimed.
- **PNG metadata dependency.** `png 0.18.1` is already in the lock graph; making it a direct dependency avoids a new network/license risk. If it cannot be added cleanly, the visible banner remains mandatory and metadata is reported as unmet rather than silently dropped.
- **Off-slice threshold.** Derived from `f32` precision and scene extent, not an invented absolute distance.
- **Preset benchmark scope.** A sampling/one-frame measurement can be below the full app frame cost; the report must not claim app fps.
- **2D approximation labels.** Only existing formula rows whose semantics are a 2D restriction are labelled; classic kinematic 2D flows stay unlabelled.

## Builder notes

Implementation should split into sequential units: (1) glyph model/line geometry and app composition; (2) selected-slice equivalence/plane regression coverage; (3) source preset catalogue, loading and formula-preset labels; (4) provenance object, PNG/SVG overlay/metadata and request-time capture; (5) docs/help, benchmark, full gates and independent review. Each unit must preserve #13 and #15 seams and avoid CLI changes.
