# Sources panel — design

**Status:** PROPOSED — pending owner approval.

**Issue:** [#25](https://github.com/ttgg1/vector-field-explorer/issues/25), child of the
[#17 physics epic](https://github.com/ttgg1/vector-field-explorer/issues/17).

**Base:** `99aec5a4f0fa66d9e603bffef9dd544b633db628` (Issue #24 source evaluator merged).

## Intent

Make the eight validated source kinds usable from the app, not only from the CPU
evaluator. A learner should be able to build a mixed electric/current scene, see
the field it produces, save it, and come back to it, while the classic formula
field remains available and untouched.

This is the first consumer of the evaluator. It owns the persistent source scene,
the source-mode sampling adapter, the dedicated panel and the accuracy gate. It
does not own source mathematics, glyphs, presets, export warnings or a new
renderer path.

## Goals

- A dedicated **Sources** panel that lists, adds, edits, enables/disables,
  removes and resets all eight source kinds.
- Mixed electric and magnetic source lists in one saved scene.
- Explicit **Formula** / **Sources** field modes. Each keeps its own durable
  state; switching modes never merges a formula vector and a source vector.
- Display selection **E**, **B** or **Both**, with both drawn as two visually
  distinct families and never numerically added.
- Source-mode field sampling through the existing `SceneGPU` / arrow-instance
  boundary, so the viewport, camera, references and PNG path keep working.
- Persistence in the existing `APP_KEY` envelope, with backward defaults and
  malformed-entry recovery.
- One scene-wide softening control and an **Accuracy not met** display opt-in
  that is default-off and cannot be enabled by an older payload.
- Actionable validation for every editor; invalid input never replaces the last
  valid source or corrupts the saved scene.
- A small, testable provenance surface (`SourceSampling`) for #26 to consume.

## Non-goals

- New source mathematics, geometry kinds or changes to the evaluator's accuracy,
  work or storage contract.
- Source glyphs, cell painting, preset migration, export composition or warning
  metadata. #26 owns those.
- Drag placement, grouping, naming, units/dimensional analysis, a new CLI flag,
  a new config schema, or per-source softening.
- Streamlines, particles or click seeding on a source field. They remain formula
  features in this delivery; the UI explains that in Sources mode.
- A second clock or any source-mode animation. Sources are static; the shared
  clock keeps driving formula/scalar consumers only.
- Reimplementing #13 plane slicing. Source mode reuses the existing plane frame
  and selected-plane geometry.

## Approaches considered

### A. First-class source scene with a sibling sampling adapter (chosen)

Add `FieldMode`, `SourceScene` and a source sampling adapter to the viewmodel.
The existing formula `FieldExpr` stays exactly as it is. `State::snapshot` and
the dirty rebuild choose the active adapter. The Sources panel edits the scene,
and persistence stores it in `ViewRecord`.

- **Trade-off:** a second sampling path and state fields to maintain.
- **Why chosen:** it preserves formula behaviour, keeps the evaluator's
  provenance intact, makes mode isolation testable, and gives #26 one place to
  consume warnings. It adds no renderer pipeline.

### B. Compile the source scene into a synthetic `FieldExpr`

Represent source evaluation as a closure/field expression so every existing
consumer (arrows, streamlines, particles, scalar plots) sees one formula-like
field.

- **Trade-off:** the evaluator is stateful, bounded and accuracy-aware; a
  `FieldExpr` returns only a vector. Flattening it would discard status,
  provenance, per-family results and query accounting, and would tempt unsafe
  particle/streamline reuse.
- **Rejected:** hides the very uncertainty this issue must expose.

### C. A separate Sources application mode with its own viewport pipeline

Swap the whole app chrome and renderer for a sources workspace.

- **Trade-off:** duplicates camera, references, capture and layout; creates two
  truths for the same viewport; makes #26 integration harder.
- **Rejected:** more surface for no learner benefit.

## Data model

New module `src/source_scene.rs`, re-exported from `lib.rs`:

```rust
pub enum FieldMode { Formula, Sources }

pub enum SourceDisplay { Electric, Magnetic, Both }

pub struct SourceEntry {
    pub enabled: bool,
    pub source: Source,
}

pub struct SourceScene {
    pub entries: Vec<SourceEntry>,
    pub display: SourceDisplay,
    pub softening: f64,
    pub allow_inaccurate: bool,
}
```

- `FieldMode` defaults to `Formula`.
- `SourceDisplay` defaults to `Both`; an empty family simply contributes no
  arrows.
- `softening` defaults to the evaluator's `0.1` and is clamped by the panel to
  `1e-6..=1.0`. Those bounds are the values already exercised by evaluator
  fixtures; the UI does not invent a new numerical contract.
- `allow_inaccurate` defaults to `false`.
- The entry limit is the evaluator's `MAX_SOURCES` (64). The panel disables Add
  at the limit; `SourceSet` remains the final validator.
- `SourceScene::enabled_sources()` returns only enabled entries, in list order.
- `SourceScene::validate()` builds `SourceSet` and `EvalOptions` through the
  public evaluator seams and returns a typed error. The panel calls it before
  committing; the sampler also refuses to evaluate an invalid scene.

`SourceScene` also owns pure CRUD helpers so the UI and tests share one path:

```rust
pub fn add(&mut self, source: Source) -> Result<usize, SourceSceneError>;
pub fn remove(&mut self, index: usize) -> Option<SourceEntry>;
pub fn set_enabled(&mut self, index: usize, enabled: bool) -> bool;
pub fn reset_entry(&mut self, index: usize) -> bool;   // kind defaults, kept center/axis
pub fn reset(&mut self);                              // empty scene, defaults
```

Invalid indices are no-ops returning `false`/`None`. `add` returns
`SourceLimit { max }` at the evaluator cap.

`src/sources` gains no UI-facing state. The scene module converts `Source`
values into `SourceSet`/`EvalOptions`; source math and validation stay in their
current module.

## Viewmodel integration

`State` gains:

```rust
pub field_mode: FieldMode,
pub sources: SourceScene,
pub source_sampling: Option<SourceSampling>,
```

`State::active_dimension()` returns `3` in `FieldMode::Sources` and otherwise the
formula's `FieldExpr::dimension`. It replaces the direct
`self.field.dimension` checks that determine 3D sampling, plane geometry,
particle availability, domain-axis visibility and the 2D slice predicate. In
Sources mode the field is always three-dimensional, so all existing 2D-plane
behaviour applies unchanged to source sampling.

`State::set_field_mode(mode)`:

- no-op when unchanged;
- preserves the formula string, parsed field, formula error and source scene;
- clears `source_sampling` when entering Sources;
- drops transient app-owned click seeds/particles outside this setter (the app
  clears them on the same transition) but never mutates source entries;
- marks the state dirty and clears `source_sampling`; there is no separate
  `field_generation` counter. One dirty flag is enough because only the active
  adapter is sampled: formula edits in Sources mode update formula state without
  sampling it, and source edits clear the source cache. Switching back to
  Formula marks dirty and samples the formula then;
- never changes the 2D/3D projection.

The 2D/3D toggle stays user-controlled in Sources mode. Entering Sources from a
two-component 2D formula gives `active_dimension() == 3`, so the 2D view becomes
the selected plane slice of the source field. The existing #13 contract applies:
`plane.axis`/`plane.index` select the slice, the 2D slice ignores
`plane.enabled` (preserved for the 3D view), and the plane controls are visible
as the existing SliceOnly axis/position controls. `set_field_mode` changes
neither the axis, index nor enabled flag.

`State::set_formula` still parses and updates formula state in either mode, so a
preset or CAS path can prepare the formula before switching back. Source-mode
users do not see a formula editor.

`State::set_source_scene`/individual source setters replace the scene, clear
`source_sampling`, and mark dirty. Formula edits do not invalidate source
samples; source edits do not invalidate formula state.

`State::is_vector_mode()` keeps its current meaning (vector plot kind). Source
mode is a sub-mode of the vector field, not a fourth plot kind. `set_plot_kind`
therefore preserves `field_mode` when leaving and returning to VectorField.

`State::snapshot_if_dirty()` refreshes the active adapter before building the
`SceneGPU`:

- Formula mode: existing `sample()` / streamlines path, unchanged.
- Sources mode: `sample_source_scene()` builds arrows only; streamlines,
  particles and click lines are empty.

`State::snapshot()` remains available for formula tests. For sources it returns
the cached source arrows when present; a missing cache yields an empty scene
rather than silently sampling the formula field.

## Source sampling adapter

New in `src/source_scene.rs`:

```rust
pub struct SourceSampling {
    pub arrows: Vec<ArrowData>,
    pub electric: SourceFamilyStatus,
    pub magnetic: SourceFamilyStatus,
    pub softening: f64,
    pub allow_inaccurate: bool,
    pub validation_error: Option<String>,
}

pub struct SourceFamilyStatus {
    pub sources: usize,
    pub drawn: usize,
    pub target_met: usize,
    pub accuracy_not_met_total: usize,
    pub accuracy_not_met_drawn: usize,
    pub unavailable: usize,
    pub empty: bool,
}
```

`State::sample_source_scene(&self) -> SourceSampling`:

1. Validate the scene via `SourceSet::new` and `EvalOptions` (default accuracy
   `1e-7`, E/B scales `1e-2`, default limits, scene softening). On failure,
   return the typed message and no arrows.
2. Build one `Evaluator` for this snapshot.
3. Iterate the same grid as the active view:
   - 3D: `state.domain.grid_points(state.grid())` in world coordinates.
   - 2D source field: `state.plane_frame()` local grid; convert each local point
     to world with `frame.to_world`. The plane frame already exists because the
     active dimension is 3. The returned arrow positions are local `(u,v,0)` so
     camera, click masking and reference geometry stay in the existing scene
     space.
4. Call `Evaluator::evaluate(world_point, &|| false)` once per point. Use the
   independent family results.
5. For the selected display:
   - `Electric`: consider only `sample.electric`.
   - `Magnetic`: only `sample.magnetic`.
   - `Both`: consider both, tagging their arrows with the family colour below.
6. Convert an eligible `Estimate` into an `ArrowData`:
   - finite `value` with length at least `1e-6` (same degeneracy threshold as
     formula sampling);
   - in 2D project onto `frame.project(value)` and drop nearly normal vectors;
   - `TargetMet` is always eligible;
   - `AccuracyNotMet` is eligible only when `allow_inaccurate` is true, and is
     counted and labelled;
   - `Unavailable`, non-finite values and `QueryLimit` are never drawn.
7. Colour:
   - single-family display uses the existing colormap over the magnitudes of the
     drawn arrows, exactly like formula sampling;
   - `Both` uses fixed family colours so the two families are distinguishable
     without pretending a shared magnitude scale: electric amber `(255, 176, 64)`
     and magnetic cyan `(64, 196, 255)`. The panel/legend names them. No vectors
     are summed.
8. Return the arrows and per-family counters. Empty enabled families are
   `empty: true` with zero arrows, not an error.

The sample count at the largest supported density is `40³ = 64,000` in 3D and
`200² = 40,000` in 2D, both below the evaluator's `MAX_QUERIES` of 65,536. No
limit is raised or bypassed. The panel shows the current point count and a
one-line status when a family is omitted, unavailable or accuracy-limited.

Source arrows reuse the existing arrow presentation controls: arrow scale,
length mode (Uniform/Proportional/Magnitude), proportional exponent, magnitude
scale, arrow opacity and 2D plane dim opacity. They also reuse the normal
single-family auto/manual colour range. Auto-fit considers only arrows actually
drawn, so hidden `AccuracyNotMet` arrows never stretch the range. In `Both`
display the family colours replace the colormap and the colour-range controls
are hidden with a one-line reason; arrow geometry controls stay active.

Source sampling is static. `has_time_dependent_consumer` returns false in
Sources mode unless a formula consumer is active, which it is not; the shared
clock therefore holds, and there is no private timer.

`particle_target()` returns `None` in Sources mode. Streamline snapshots return
empty. Click actions are rejected with a one-line hint. The formula transient
state may be cleared by the app on the mode transition; durable formula and
source state are preserved.

## UI

### Field-mode switch

The formula bar (wide and compact) gains a two-option segmented control
**Formula | Sources** while the active plot kind is VectorField. It calls
`State::set_field_mode`.

- Formula mode keeps today's text editor and hints.
- Sources mode hides the editor and shows a compact summary: source count,
  display selection, softening and the accuracy state. An **Open Sources**
  button expands/focuses the Sources panel.
- Scalar plot kinds hide the switch; their formula editors are unchanged.
- Loading a classic preset switches to Formula before applying its formula. It
  does **not** clear or rewrite the saved source scene, formula drafts or
  source-side settings; only global reset clears sources. A preset load is
  therefore one click away from returning to the untouched source scene.
- Native CAS Apply is disabled in Sources mode with an explanation. Copy
  formula continues to copy the formula source, never a source scene.

In Sources mode the sidebar keeps Display, Camera, Coloring and Domain active
because they drive the same viewport. The Features section (streamlines,
particles, click/seed controls) is hidden, not shown-disabled, because those
features do not exist for a source field in this delivery; the Sources panel
carries one line saying so. The plane section uses the existing SliceOnly
controls in 2D.

### Dedicated Sources panel

A third left rail entry, **Sources**, with its own resizable panel and persisted
`layout.sources_expanded`/`sources_width`. In compact layout it becomes a fourth
bottom-sheet tab.

Panel body:

```
Sources                         [Reset scene]
[ Electric | Magnetic | Both ]   E amber / B cyan when Both

+ Add source [Point charge v] [Add]     n / 64

▼ Point charge 1        [x enabled] [Remove]
  Center  x [0.0] y [0.0] z [0.0]
  Charge  [1.0]
  (axis hidden for point/sphere)
  [Reset this source]

▼ Current loop 2        [x enabled] [Remove]
  Center ... Axis ... Radius ... Current ...
  [Reset this source]

Advanced
  Softening [0.1]  1e-6 .. 1.0
  Model note: softening is part of the teaching model; smaller values
  sharpen near-source peaks, larger values smooth them.
  [ ] Show finite results labelled "accuracy not met" (default off)
  Status: E 1000 target met; B 12 accuracy not met hidden; 0 unavailable
```

Every kind gets only its relevant parameters:

| Kind | Parameters |
| --- | --- |
| Point charge | charge |
| Finite line charge | charge, length, axis |
| Charged ring | charge, radius, axis |
| Charged disc | charge, radius, axis |
| Solid sphere | charge, radius |
| Straight wire | current, axis |
| Current loop | current, radius, axis |
| Solenoid | current, radius, length, turns, axis |

Editors use local drafts. On every change the panel validates the draft with the
same public model validation and commits only when valid; otherwise it shows an
inline error and keeps the last committed source. Non-finite typed values,
zero/negative dimensions, zero turns, zero axis and an empty/non-finite center
are all rejected with specific text. Remove drops the entry. Reset scene empties
the list and restores display/softening/opt-in defaults; Reset this source
restores kind defaults but keeps the entry's center and enabled state.

Add/remove/enable/reset and every valid edit mark only source sampling stale.
The formula field is not touched.

When `Both` is selected and both families have arrows, the viewport legend (a
small overlay) names E amber and B cyan. A source-status badge appears when
anything is hidden or accuracy-limited:

- default off: “N finite results hidden — accuracy not met”;
- opt-in: “N shown with accuracy not met”;
- unavailable/non-finite: “N unavailable and not drawn”.

No non-finite or unavailable value is drawn even with opt-in. The badge is the
provenance #26 will carry into exports; it is not export implementation here.

## Persistence and migration

`ViewRecord` gains:

```rust
#[serde(default)] field_mode: FieldMode,          // Formula
#[serde(default)] source_entries: Vec<SourceRecord>, // empty
#[serde(default)] source_display: SourceDisplay,  // Both
#[serde(default = "default_source_softening")] source_softening: f64, // 0.1
#[serde(default)] source_allow_inaccurate: bool,  // false
```

`SourceRecord` is a flat, serde-friendly shape (center/axis arrays, kind string,
numeric parameters, enabled). It converts to/from `Source`.

Restore policy:

- missing keys use the defaults above; every pre-#25 payload opens in Formula
  mode with no sources and opt-in off;
- each record is sanitized; invalid/non-finite/unknown-kind records are skipped
  and reported in the existing status-bar message slot as a one-line restore
  notice rather than failing the whole envelope;
- softening outside `1e-6..=1.0` falls back to `0.1`;
- `field_mode` is forced to Formula when no valid source entry remains;
- `allow_inaccurate` becomes true only when the stored value is exactly true;
  missing/corrupt values become false;
- the saved source scene is independent of the formula string; restoring one
  never overwrites the other.

Capture stores only valid committed entries. In-flight invalid drafts are not
persisted. `reset_plot_state` and the app's global reset clear the source scene
and sampling cache along with the vector/scalar state.

## Tests

Pure model (`tests/source_scene_test.rs`):

- defaults; add/remove/enable/reset; limit at 64; every kind's defaults validate;
- all eight editors produce valid sources and all eight kinds coexist;
- invalid center/axis/charge/current/radius/length/turns/softening are rejected
  and leave the previous scene intact;
- empty electric or magnetic family contributes zero arrows, not a failure;
- `Electric` draws only E, `Magnetic` only B, `Both` draws both with family
  colours and never a summed vector;
- `AccuracyNotMet` is hidden by default and finite-only when opted in;
  `Unavailable`/non-finite is never drawn even with opt-in;
- 3D and 2D-plane sampling positions are local/world as specified; plane
  projection drops normal-only vectors; entering Sources from a two-component
  2D formula slices at the preserved axis/index without altering `enabled`;
- arrow scale/length mode/opacity apply to source arrows; auto colour fit
  ignores hidden accuracy-not-met arrows; Both hides range controls;
- no source clock consumer, no particle target and empty streamline/click
  snapshots in Sources mode.

Viewmodel/app (`tests/source_mode_test.rs`):

- formula/source mode switches preserve both durable states and do not merge;
- formula edits do not invalidate source samples and vice versa;
- leaving and returning to VectorField preserves the field mode;
- transient click/particle state is cleared on the transition and never
  resurrects a formula seed inside Sources.

Persistence (`tests/persistence_test.rs`):

- round-trip all eight records, display, softening, opt-in and mode;
- legacy payload missing every new key opens in Formula with opt-in false;
- malformed entries are skipped without losing unrelated view settings;
- forcing `allow_inaccurate` corrupt/absent yields false;
- global reset clears sources and the sampling cache.

UI (`tests/sources_panel_test.rs`):

- wide 1200pt and compact 390pt render the panel/tab, add control, list and
  advanced control without clipping;
- actual egui widgets add, edit, enable, remove, reset one and reset scene;
- invalid draft shows actionable text and does not change the committed source;
- formula/source segmented control switches mode and the formula editor is
  absent in Sources;
- Both legend, hidden/opt-in/unavailable status text, and Add disabled at the cap;
- preset load returns to Formula and leaves the saved source scene untouched.

All tests live under `tests/`, registered in `tests/suite.rs`; no inline
`#[cfg(test)]` under `src/`.

## Builder-check resolutions

A fresh builder review asked six questions. They are designer calls, settled
here rather than deferred: arrow presentation controls reuse the existing
pipeline with family-colour override only in `Both`; plane axis/index carry over
and `enabled` is ignored/preserved in 2D; preset loads never clear sources; the
Features sidebar section is hidden while Display/Camera/Coloring/Domain remain
active; staleness uses the existing dirty flag plus the source cache (no new
generation counter or second dirty bit); and the restore notice uses the
existing status bar.

## Review focus

The inputs most likely to hurt a user are covered by the tests above:

1. A typed non-finite or zero geometry value silently corrupting the saved scene
   — rejected drafts and malformed restore.
2. An old payload silently enabling inaccurate rendering — default-off opt-in
   with exact-true restore only.
3. Both display accidentally adding E and B or sharing one magnitude scale —
   fixed family colours and no summation.
4. A 3D source field sampled in 2D with the wrong coordinate basis — local plane
   positions and world evaluation.
5. Switching modes losing a formula, a source list or the vector projection —
   explicit isolation tests.

Cannot verify in this delivery: browser/manual visual quality (waived, not
passed), #26 export warning placement, and glyph/preset design.

## Risks

- Source sampling at maximum density is heavier than formula sampling. It runs
  only on a dirty rebuild, uses the evaluator's existing per-point bounds, and
  does not advance on the clock. If it proves too slow in practice, #26 can add
  a cached preview; this design does not pre-optimise.
- The panel's per-source drafts duplicate some editor state. They are the only
  way to keep invalid text from becoming a committed source; persistence reads
  the committed scene, never the drafts.
- Adding a rail entry changes layout budget. Responsive tests own that risk.
