# Sources Panel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A dedicated Sources panel that builds, edits, samples, saves and restores mixed electric/current scenes without changing the classic formula field.

**Architecture:** Add a first-class `SourceScene` to the viewmodel and a sibling source-sampling adapter that emits the existing `ArrowData`/`SceneGPU` instances. Keep `FieldExpr` formulas untouched; select one adapter by an explicit `FieldMode`. Add a Sources rail/compact sheet and persist the scene through the existing `ViewRecord` envelope.

**Tech Stack:** Rust 2024, eframe/egui, glam, serde, existing `src/sources` CPU evaluator. Native + wasm; no new dependency.

**Spec:** `docs/superpowers/specs/2026-11-16-sources-panel-design.md`

## Global Constraints

- GPL-3.0-only; no new dependency; preserve wasm/native builds.
- No new source math, geometry kinds or evaluator accuracy/work/storage changes.
- Source mode is a vector-field sub-mode, not a new plot kind.
- Formula and source state are exclusive and independently preserved; never add a
  formula vector to a source vector.
- E and B are separate families. `Both` draws two visually distinct families and
  does not sum them or share one magnitude scale.
- Use the evaluator's `MAX_SOURCES = 64` and `MAX_QUERIES = 65_536`; do not raise
  or bypass them. Source sampling maximum is `40³ = 64,000` or `200² = 40,000`.
- Softening defaults to `0.1`; the panel accepts only `1e-6..=1.0`.
- Accuracy opt-in defaults to false, restores true only from an exact `true`,
  and can never make non-finite or `Unavailable` data drawable.
- All cells are source coordinates; source-mode 2D sampling uses the existing
  `PlaneFrame` local positions and world evaluation.
- No per-source softening, private clock, CLI flag, config-schema change, glyph,
  preset migration, export warning implementation or unit engine in this plan.
- Tests live under `tests/`, registered in `tests/suite.rs`; no inline
  `#[cfg(test)]` under `src/`.
- Run the standard gates after each task: focused tests, full suite, build,
  clippy `-D warnings`, changed-file rustfmt with `--config skip_children=true`,
  `check-docs.py`, `git diff --check`; native smoke after UI integration.

## Review Focus

1. **Non-finite or impossible geometry typed into an editor** must show an
   actionable error and leave the previous source and saved scene intact
   (Task 1 model tests, Task 4 panel tests).
2. **An old payload must never silently enable inaccurate drawing**; missing or
   corrupt opt-in restores false, and malformed entries are skipped without
   losing unrelated settings (Task 3 persistence tests).
3. **`Both` accidentally mixing E and B** or presenting unlike magnitudes on one
   colour scale: fixed family colours, separate counters, no summation
   (Task 2 tests).
4. **A 3D source field sampled in 2D on the wrong basis** — local plane
   positions, world evaluation, normal projection (Task 2 tests).
5. **Mode switching losing a formula, a source scene or the projection choice** —
   explicit preservation and transient-clearing tests (Task 2, Task 5).

---

### Task 1: Source scene model, catalogue and validation

**Files:**
- Create: `src/source_scene.rs`
- Modify: `src/lib.rs`, `src/sources/model.rs`, `src/sources/mod.rs`
- Test: `tests/source_scene_test.rs`, `tests/suite.rs`
- Docs: `docs/CODEBASE.md`

**Interfaces:**
- Consumes: `Source`, `SourceKind`, `SourceSet`, `WorkLimits`, `InputError`,
  `FieldFamily` and `MAX_SOURCES` from `crate::sources`.
- Produces: `FieldMode`, `SourceDisplay`, `SourceEntry`, `SourceScene`,
  `SourceSceneError`, `SourceKindTag`, `MIN_SOFTENING`, `MAX_SOFTENING` and
  `SourceKind::family() -> FieldFamily`.

- [ ] **Step 1: Write the failing model tests**

```rust
use glam::DVec3;
use vector_field_explorer::source_scene::{
    FieldMode, SourceDisplay, SourceEntry, SourceScene, SourceSceneError, SourceKindTag,
    MAX_SOFTENING, MIN_SOFTENING,
};
use vector_field_explorer::sources::SourceKind;

#[test]
fn the_default_scene_is_empty_in_formula_mode() {
    let scene = SourceScene::default();
    assert_eq!(FieldMode::default(), FieldMode::Formula);
    assert_eq!(scene.display, SourceDisplay::Both);
    assert_eq!(scene.softening, 0.1);
    assert!(!scene.allow_inaccurate);
    assert!(scene.entries.is_empty());
    assert!(scene.validate().is_ok());
}

#[test]
fn all_eight_kinds_have_valid_defaults_and_a_family() {
    assert_eq!(SourceKindTag::ALL.len(), 8);
    for tag in SourceKindTag::ALL {
        let source = tag.default_source();
        let mut scene = SourceScene::default();
        scene.add(source.clone()).unwrap();
        scene.validate().unwrap();
        assert_eq!(scene.entries[0].source, source);
        assert_eq!(source.kind.family(), tag.family());
        assert!(tag.label().len() > 3);
    }
}

#[test]
fn add_remove_enable_and_reset_change_only_the_scene() {
    let mut scene = SourceScene::default();
    let mut source = SourceKindTag::ChargedRing.default_source();
    source.center = DVec3::new(2.0, 3.0, 4.0);
    let index = scene.add(source).unwrap();
    assert_eq!(index, 0);

    assert!(scene.set_enabled(0, false));
    assert!(!scene.entries[0].enabled);
    assert!(scene.set_enabled(0, true));

    assert!(scene.reset_entry(0));
    assert_eq!(scene.entries[0].source.center, DVec3::new(2.0, 3.0, 4.0));
    assert_eq!(
        scene.entries[0].source.kind,
        SourceKindTag::ChargedRing.default_source().kind
    );

    let removed = scene.remove(0).unwrap();
    assert_eq!(removed.source.center, DVec3::new(2.0, 3.0, 4.0));
    assert!(scene.remove(0).is_none());
}

#[test]
fn source_limit_stops_the_sixty_fifth_entry() {
    let mut scene = SourceScene::default();
    for _ in 0..64 {
        scene
            .add(SourceKindTag::PointCharge.default_source())
            .unwrap();
    }
    assert_eq!(
        scene.add(SourceKindTag::PointCharge.default_source()),
        Err(SourceSceneError::Limit { max: 64 })
    );
    assert_eq!(scene.entries.len(), 64);
}

#[test]
fn invalid_geometry_is_rejected_with_a_parameter_name() {
    let mut scene = SourceScene::default();
    let mut source = SourceKindTag::ChargedRing.default_source();
    let SourceKind::ChargedRing { radius, .. } = &mut source.kind else {
        unreachable!()
    };
    *radius = 0.0;
    assert_eq!(
        scene.add(source),
        Err(SourceSceneError::InvalidSource {
            index: 0,
            parameter: "radius",
        })
    );

    let mut nonfinite = SourceKindTag::PointCharge.default_source();
    if let SourceKind::PointCharge { charge } = &mut nonfinite.kind {
        *charge = f64::NAN;
    }
    assert!(matches!(
        scene.add(nonfinite),
        Err(SourceSceneError::InvalidSource { .. })
    ));
    assert!(scene.entries.is_empty());
}

#[test]
fn softening_is_bounded_and_never_nan() {
    let mut scene = SourceScene::default();
    assert!(scene.set_softening(MIN_SOFTENING).is_ok());
    assert!(scene.set_softening(MAX_SOFTENING).is_ok());
    assert_eq!(
        scene.set_softening(1e-9),
        Err(SourceSceneError::SofteningOutOfRange)
    );
    assert_eq!(
        scene.set_softening(f64::NAN),
        Err(SourceSceneError::SofteningOutOfRange)
    );
    scene.reset();
    assert_eq!(scene.softening, 0.1);
    assert_eq!(scene.display, SourceDisplay::Both);
    assert!(!scene.allow_inaccurate);
}

#[test]
fn disabled_entries_are_not_part_of_the_validated_scene() {
    let mut scene = SourceScene::default();
    scene
        .add(SourceKindTag::PointCharge.default_source())
        .unwrap();
    scene.set_enabled(0, false);
    assert_eq!(scene.enabled_sources().count(), 0);
    assert!(scene.validate().is_ok());
    scene.set_display(SourceDisplay::Electric);
    assert_eq!(scene.display, SourceDisplay::Electric);
    assert!(!scene.entries[0].enabled);
}
```

- [ ] **Step 2: Run the test filter to verify RED**

Run: `cargo test --test suite source_scene_test::`
Expected: compile error, missing `source_scene` module and `SourceKind::family`.

- [ ] **Step 3: Implement the module**

In `src/sources/model.rs`, add `pub fn family(&self) -> FieldFamily` on `SourceKind`
returning Electric for the five charge kinds and Magnetic for wire/loop/solenoid,
and re-export `FieldFamily` from `src/sources/mod.rs`.

In `src/source_scene.rs` implement:

```rust
pub const MIN_SOFTENING: f64 = 1e-6;
pub const MAX_SOFTENING: f64 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum FieldMode { #[default] Formula, Sources }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum SourceDisplay { Electric, Magnetic, #[default] Both }

#[derive(Debug, Clone, PartialEq)]
pub struct SourceEntry { pub enabled: bool, pub source: Source }

#[derive(Debug, Clone, PartialEq)]
pub struct SourceScene {
    pub entries: Vec<SourceEntry>,
    pub display: SourceDisplay,
    pub softening: f64,
    pub allow_inaccurate: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SourceSceneError {
    Limit { max: usize },
    InvalidSource { index: usize, parameter: &'static str },
    SofteningOutOfRange,
    InvalidLimits,
    Cancelled,
    AllocationFailed,
}

impl Default for SourceScene {
    fn default() -> Self { Self { entries: Vec::new(), display: SourceDisplay::Both, softening: 0.1, allow_inaccurate: false } }
}

impl SourceScene {
    pub fn add(&mut self, source: Source) -> Result<usize, SourceSceneError>;
    pub fn remove(&mut self, index: usize) -> Option<SourceEntry>;
    pub fn set_enabled(&mut self, index: usize, enabled: bool) -> bool;
    pub fn replace(&mut self, index: usize, source: Source) -> Result<(), SourceSceneError>;
    pub fn reset_entry(&mut self, index: usize) -> bool;
    pub fn reset(&mut self);
    pub fn set_softening(&mut self, softening: f64) -> Result<(), SourceSceneError>;
    pub fn set_display(&mut self, display: SourceDisplay);
    pub fn set_allow_inaccurate(&mut self, allowed: bool);
    pub fn enabled_sources(&self) -> impl Iterator<Item = &Source>;
    pub fn validate(&self) -> Result<(), SourceSceneError>;
}
```

`validate` builds `SourceSet::new` from the enabled sources with
`WorkLimits::default()` and a non-cancelled closure, maps `InputError` to the
typed variants, and checks `softening.is_finite()` and
`MIN_SOFTENING..=MAX_SOFTENING`. `add`/`replace` validate the candidate scene
before committing and return the mapped error. `reset_entry` keeps the entry's
center and enabled flag and restores that kind's default parameters; `reset`
restores `SourceScene::default()`.

`SourceKindTag` is a fieldless enum with `ALL` in design order:

```rust
pub enum SourceKindTag { PointCharge, FiniteLineCharge, ChargedRing, ChargedDisc, SolidSphere, StraightWire, CurrentLoop, Solenoid }
pub fn label(self) -> &'static str;
pub fn family(self) -> FieldFamily;
pub fn default_source(self) -> Source;
```

Defaults: center `DVec3::ZERO`, axis `DVec3::Z`; charge/current `1.0`;
line length `1.0`; radius `1.0`; solenoid radius `0.5`, length `2.0`, turns `8`;
all other absent parameters as the kind requires.

Export the module in `src/lib.rs`; add the scoped module row in
`docs/CODEBASE.md`.

- [ ] **Step 4: Run the model test filter to verify GREEN**

Run: `cargo test --test suite source_scene_test::`
Expected: 7 passed.

- [ ] **Step 5: Commit**

```bash
git add src/source_scene.rs src/lib.rs src/sources/model.rs src/sources/mod.rs tests/source_scene_test.rs tests/suite.rs docs/CODEBASE.md
git commit -m "feat: add the source scene model" -m "Generated-by: OpenCode"
```

---

### Task 2: Field modes, source sampling and source-mode gating

**Files:**
- Modify: `src/source_scene.rs`, `src/viewmodel.rs`, `src/app/simulation.rs`,
  `src/app/input.rs`, `src/app/plot.rs`
- Test: `tests/source_mode_test.rs`, `tests/suite.rs`
- Docs: `docs/CODEBASE.md`

**Interfaces:**
- Consumes: Task 1's `FieldMode`, `SourceScene`, `SourceDisplay`, `SourceEntry`;
  `crate::sampling::{ArrowData, ViewOptions, magnitude_range}`;
  `crate::plane::PlaneFrame`; `crate::sources::{Evaluator, SourceSet, EvalOptions, FamilyResult, Estimate, Sample, StopReason}`.
- Produces: `SourceSampling`, `SourceFamilyStatus`, `FamilyDisposition`,
  `classify_family_result`, `sample_source_scene`,
  `State::{field_mode, sources, source_sampling, active_dimension, set_field_mode, set_source_scene, refresh_source_sampling, source_sampling}`.

- [ ] **Step 1: Write the failing mode/sampling tests**

```rust
use glam::DVec3;
use vector_field_explorer::source_scene::{
    classify_family_result, FamilyDisposition, FieldMode, SourceDisplay, SourceScene,
    SourceKindTag,
};
use vector_field_explorer::sources::{
    Diagnostics, Estimate, FamilyResult, FieldFamily, StopReason,
};
use vector_field_explorer::viewmodel::State;

fn one_point() -> SourceScene {
    let mut scene = SourceScene::default();
    scene.add(SourceKindTag::PointCharge.default_source()).unwrap();
    scene
}

fn estimate(value: DVec3, uncertainty: f64, target: f64) -> Estimate {
    Estimate {
        value,
        uncertainty,
        target,
        absolute_contributions: uncertainty,
        diagnostics: Diagnostics {
            kernel_id: "test",
            family: FieldFamily::Electric,
            softening: 0.1,
            epsilon: 1e-7,
            family_scale: 1.0,
            source_count: 1,
            work_units: 0,
            cells_high_water: 0,
            model_regularized: true,
            roundoff_limited: false,
            underflow: false,
        },
    }
}

#[test]
fn mode_switches_preserve_formula_and_sources() {
    let mut state = State::new(false);
    state.set_formula("(y, -x, z)".into());
    let formula = state.formula.clone();
    state.set_source_scene(one_point());
    assert_eq!(state.sources.entries.len(), 1);
    assert!(state.set_field_mode(FieldMode::Sources));
    assert_eq!(state.field_mode, FieldMode::Sources);
    assert_eq!(state.formula, formula);
    assert_eq!(state.sources.entries.len(), 1);
    let three_d = state.mode_3d;
    assert!(!state.set_field_mode(FieldMode::Formula));
    assert_eq!(state.mode_3d, three_d);
    assert_eq!(state.formula, formula);
    assert_eq!(state.sources.entries.len(), 1);
}

#[test]
fn source_mode_is_three_dimensional_and_2d_uses_the_plane_frame() {
    let mut state = State::new(false);
    state.set_source_scene(one_point());
    state.set_field_mode(FieldMode::Sources);
    state.set_mode(false);
    assert_eq!(state.active_dimension(), 3);
    assert!(state.is_plane_slice());
    state.snapshot_if_dirty();
    let sampling = state.source_sampling().expect("source mode sampled");
    assert!(!sampling.arrows.is_empty());
    assert!(sampling.arrows.iter().all(|a| a.position.z.abs() < 1e-6));
}

#[test]
fn accuracy_not_met_is_hidden_by_default_and_opt_in_draws_finite_only() {
    let estimate = estimate(DVec3::X, 1.0, 0.1);
    let not_met = FamilyResult::AccuracyNotMet(estimate.clone(), StopReason::WorkLimit);
    assert_eq!(
        classify_family_result(&not_met, false),
        FamilyDisposition::HiddenAccuracy
    );
    assert_eq!(
        classify_family_result(&not_met, true),
        FamilyDisposition::Draw
    );
    assert_eq!(
        classify_family_result(&FamilyResult::Unavailable(StopReason::NumericRange), true),
        FamilyDisposition::Unavailable
    );

    let mut state = State::new(false);
    state.set_source_scene(one_point());
    state.set_field_mode(FieldMode::Sources);
    state.sources.allow_inaccurate = false;
    state.mark_dirty();
    state.snapshot_if_dirty();
    let default_sampling = state.source_sampling().unwrap();
    assert_eq!(default_sampling.electric.accuracy_not_met_drawn, 0);
}

#[test]
fn both_display_uses_two_family_colours_without_summing() {
    let mut scene = SourceScene::default();
    scene.add(SourceKindTag::PointCharge.default_source()).unwrap();
    scene.add(SourceKindTag::CurrentLoop.default_source()).unwrap();
    scene.set_display(SourceDisplay::Both);
    let mut state = State::new(true);
    state.set_source_scene(scene);
    state.set_field_mode(FieldMode::Sources);
    state.snapshot_if_dirty();
    let sampling = state.source_sampling().unwrap();
    let electric: Vec<_> = sampling.arrows.iter().filter(|a| a.color == [255, 176, 64]).collect();
    let magnetic: Vec<_> = sampling.arrows.iter().filter(|a| a.color == [64, 196, 255]).collect();
    assert!(!electric.is_empty(), "E family is drawn");
    assert!(!magnetic.is_empty(), "B family is drawn");
}

#[test]
fn source_mode_has_no_formula_transients_and_does_not_consume_the_clock() {
    let mut state = State::new(true);
    state.set_formula("(-y, x, 0)".into());
    state.set_streamlines(true);
    state.set_particles(true);
    state.set_source_scene(one_point());
    state.set_field_mode(FieldMode::Sources);
    state.snapshot_if_dirty();
    assert!(state.streamlines_snapshot().is_empty());
    assert!(state.click_streamlines_snapshot().is_empty());
    assert!(state.particle_target().is_none());
    assert!(!state.has_time_dependent_consumer());
}

#[test]
fn an_empty_family_is_zero_not_an_error() {
    let mut scene = SourceScene::default();
    scene.add(SourceKindTag::PointCharge.default_source()).unwrap();
    let mut state = State::new(true);
    state.set_source_scene(scene);
    state.set_field_mode(FieldMode::Sources);
    state.sources.set_display(SourceDisplay::Magnetic);
    state.snapshot_if_dirty();
    let sampling = state.source_sampling().unwrap();
    assert!(sampling.magnetic.empty);
    assert_eq!(sampling.magnetic.drawn, 0);
    assert!(sampling.validation_error.is_none());
}

#[test]
fn formula_edits_do_not_replace_source_samples() {
    let mut state = State::new(false);
    state.set_source_scene(one_point());
    state.set_field_mode(FieldMode::Sources);
    state.snapshot_if_dirty();
    let before = state.source_sampling().unwrap().arrows.clone();
    state.set_formula("(1, 2, 3)".into());
    state.snapshot_if_dirty();
    let after = state.source_sampling().unwrap().arrows.clone();
    assert_eq!(before, after);
}
```

- [ ] **Step 2: Run the test filter to verify RED**

Run: `cargo test --test suite source_mode_test::`
Expected: compile error on the missing `SourceSampling`, `classify_family_result`
and `State` mode methods.

- [ ] **Step 3: Implement source sampling**

Add to `src/source_scene.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SourceFamilyStatus {
    pub sources: usize,
    pub drawn: usize,
    pub target_met: usize,
    pub accuracy_not_met_total: usize,
    pub accuracy_not_met_drawn: usize,
    pub unavailable: usize,
    pub empty: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SourceSampling {
    pub arrows: Vec<ArrowData>,
    pub electric: SourceFamilyStatus,
    pub magnetic: SourceFamilyStatus,
    pub softening: f64,
    pub allow_inaccurate: bool,
    pub validation_error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FamilyDisposition { Draw, HiddenAccuracy, Unavailable }

pub fn classify_family_result(
    result: &FamilyResult,
    allow_inaccurate: bool,
) -> FamilyDisposition {
    match result {
        FamilyResult::TargetMet(estimate) if drawable(estimate) => FamilyDisposition::Draw,
        FamilyResult::AccuracyNotMet(estimate, _)
            if allow_inaccurate && drawable(estimate) => FamilyDisposition::Draw,
        FamilyResult::AccuracyNotMet(_, _) => FamilyDisposition::HiddenAccuracy,
        FamilyResult::Unavailable(_) => FamilyDisposition::Unavailable,
        FamilyResult::TargetMet(_) => FamilyDisposition::Unavailable,
    }
}

pub fn sample_source_scene(
    scene: &SourceScene,
    domain: &Domain,
    grid: UVec3,
    plane: Option<&PlaneFrame>,
    view: &ViewOptions,
) -> SourceSampling;
```

`drawable` is finite and `value.length() >= 1e-6`. `sample_source_scene`
validates the enabled sources through `SourceSet::new`/`WorkLimits::default()`,
builds one `Evaluator` with `EvalOptions { softening: scene.softening, ..Default::default() }`,
returns `validation_error` on failure, and otherwise:
- chooses `plane.local_domain().grid_points(UVec3::new(grid.x, grid.y, 1))` and
  `plane.to_world(point)` when `plane` is `Some`; otherwise
  `domain.grid_points(grid)`;
- evaluates both families once per point;
- counts every family outcome;
- for `Both`, pushes electric arrows with `[255, 176, 64]` and magnetic arrows
  with `[64, 196, 255]`;
- for one family, collects its drawable magnitudes, computes
  `magnitude_range`, and assigns the active colormap colour;
- projects 2D directions with `plane.project(world_value)` and positions with
  `plane.to_local(world_point)`, skipping nearly normal vectors;
- leaves `empty` true only when the family has no enabled sources.

Use `crate::sampling::colormap` (or the existing colour helper used by
`sample_field_at`) for the single-family colours.

- [ ] **Step 4: Integrate the viewmodel and app**

Add `field_mode`, `sources`, `source_sampling` and `source_notice` to `State`;
initialize them in `State::new`. Add:

```rust
pub fn active_dimension(&self) -> usize;
pub fn set_field_mode(&mut self, mode: FieldMode) -> bool;
pub fn set_source_scene(&mut self, scene: SourceScene) -> bool;
pub fn refresh_source_sampling(&mut self);
pub fn source_sampling(&self) -> Option<&SourceSampling>;
```

`active_dimension` returns 3 for Sources and `self.field.dimension` otherwise.
Replace the `field.dimension == 3` decisions in `is_3d`, `plane_grid`,
`is_plane_slice`, `plane_frame`, `particle_target`, domain-axis visibility and
the 2D plane path with `active_dimension()`. Keep `sample()` formula-only.

`set_field_mode` returns false on no-op; otherwise sets the mode, clears
`source_sampling`, sets `source_notice = None`, marks dirty, and returns true.
`set_source_scene` replaces the scene, clears `source_sampling`, marks dirty.
`refresh_source_sampling` builds the active adapter:
- formula mode clears the source cache;
- sources mode uses `self.grid()`, `self.plane_frame()` for 2D, and
  `sample_source_scene(...)`; stores `Some(sampling)`.
`snapshot_if_dirty` calls `refresh_source_sampling()` before `snapshot()`.
`snapshot()` in Sources mode returns the cached arrows and no flow lines; a
missing cache returns an empty scene. In Sources mode `streamlines_snapshot`,
`click_streamlines_snapshot` return empty, `particle_target` returns None, and
`has_time_dependent_consumer` returns false.

In `src/app/input.rs` early-return from `apply_click_action` when
`state.field_mode == FieldMode::Sources`. In `src/app/simulation.rs` add
`sync_field_mode` with a `last_field_mode` on `VfApp`: on a transition clear
`particles`, `particle_target`, `click_seeds`, `last_click_seed`, `flow_lines`,
`click_lines` and the click/streamline counts, then mark the state dirty. Call it
at the start of `advance_simulation`, before rebuilding the scene.
`src/app/plot.rs::reset_plot_state` must not clear sources; add
`State::reset_sources` and call it only from the global-reset path in Task 3/5.

- [ ] **Step 5: Run the focused tests to verify GREEN**

Run:
`cargo test --test suite source_mode_test::`
`cargo test --test suite source_scene_test::`
`cargo test --test suite particle_playback_test::`
`cargo test --test suite plot_app_test::`
Expected: all pass; the existing formula/scalar tests are unchanged.

- [ ] **Step 6: Commit**

```bash
git add src/source_scene.rs src/viewmodel.rs src/app/simulation.rs src/app/input.rs src/app/plot.rs tests/source_mode_test.rs tests/suite.rs docs/CODEBASE.md
git commit -m "feat: sample source fields in a dedicated mode" -m "Generated-by: OpenCode"
```

---

### Task 3: Persistence, migration and reset

**Files:**
- Modify: `src/persistence.rs`, `src/source_scene.rs`, `src/app/input.rs`
- Test: `tests/persistence_test.rs`
- Docs: `docs/CODEBASE.md`

**Interfaces:**
- Consumes: Task 1 scene/model; Task 2 `State::{field_mode, sources, set_source_scene, set_field_mode}`.
- Produces: public `SourceRecord`; `ViewRecord` fields `field_mode`,
  `source_entries`, `source_display`, `source_softening`, `source_allow_inaccurate`;
  `SourceScene::from_records`/`to_records`.

- [ ] **Step 1: Write the failing persistence tests**

```rust
use glam::DVec3;
use vector_field_explorer::persistence::ViewRecord;
use vector_field_explorer::source_scene::{
    FieldMode, SourceDisplay, SourceKindTag, SourceRecord, SourceScene,
};
use vector_field_explorer::ui::sidebar::Controls;
use vector_field_explorer::viewmodel::State;

fn scene() -> SourceScene {
    let mut scene = SourceScene::default();
    for tag in SourceKindTag::ALL {
        let mut source = tag.default_source();
        source.center = DVec3::new(1.0, -2.0, 0.5);
        scene.add(source).unwrap();
    }
    scene.set_display(SourceDisplay::Electric);
    scene.set_softening(0.02).unwrap();
    scene.set_allow_inaccurate(true);
    scene
}

#[test]
fn source_scene_round_trips_through_the_view_record() {
    let mut state = State::new(true);
    state.set_source_scene(scene());
    state.set_field_mode(FieldMode::Sources);
    let record = ViewRecord::capture(&state);
    assert_eq!(record.field_mode, FieldMode::Sources);
    assert_eq!(record.source_entries.len(), 8);
    assert_eq!(record.source_display, SourceDisplay::Electric);
    assert_eq!(record.source_softening, 0.02);
    assert!(record.source_allow_inaccurate);

    let mut restored = State::new(true);
    let mut controls = Controls::new(&restored);
    record.restore(&mut restored, &mut controls);
    assert_eq!(restored.field_mode, FieldMode::Sources);
    assert_eq!(restored.sources.entries.len(), 8);
    assert_eq!(restored.sources.display, SourceDisplay::Electric);
    assert_eq!(restored.sources.softening, 0.02);
    assert!(restored.sources.allow_inaccurate);
    assert!(restored.formula == state.formula);
}

#[test]
fn a_legacy_record_opens_in_formula_with_opt_in_off() {
    let record: ViewRecord = serde_json::from_str("{}").unwrap();
    assert_eq!(record.field_mode, FieldMode::Formula);
    assert!(record.source_entries.is_empty());
    assert_eq!(record.source_display, SourceDisplay::Both);
    assert_eq!(record.source_softening, 0.1);
    assert!(!record.source_allow_inaccurate);
}

#[test]
fn malformed_source_records_are_skipped_without_losing_other_settings() {
    let json = r#"{
        "formula": "(1, 2, 3)",
        "field_mode": "Sources",
        "source_entries": [
            {"kind":"ChargedRing","center":[0,0,0],"axis":[0,0,1],"strength":1,"radius":1},
            {"kind":"NotAKind","center":[0,0,0],"axis":[0,0,1],"strength":1},
            {"kind":"SolidSphere","center":[0,0,0],"axis":[0,0,1],"strength":1,"radius":0}
        ],
        "source_display": "Both",
        "source_softening": 0.1,
        "source_allow_inaccurate": "not-a-bool"
    }"#;
    let record: ViewRecord = serde_json::from_str(json).unwrap();
    let mut state = State::new(true);
    let mut controls = Controls::new(&state);
    record.restore(&mut state, &mut controls);
    assert_eq!(state.formula, "(1, 2, 3)");
    assert_eq!(state.sources.entries.len(), 1);
    assert!(!state.sources.allow_inaccurate);
    assert!(state.source_notice.is_some());
}

#[test]
fn a_record_without_valid_sources_forces_formula_mode() {
    let record = ViewRecord {
        field_mode: FieldMode::Sources,
        source_entries: vec![SourceRecord {
            kind: "NotAKind".into(),
            ..SourceRecord::default()
        }],
        ..ViewRecord::default()
    };
    let mut state = State::new(false);
    let mut controls = Controls::new(&state);
    record.restore(&mut state, &mut controls);
    assert_eq!(state.field_mode, FieldMode::Formula);
    assert!(state.sources.entries.is_empty());
    assert!(state.source_notice.is_some());
}

#[test]
fn source_records_keep_order_and_skip_invalid_numbers() {
    let mut scene = SourceScene::default();
    for tag in SourceKindTag::ALL {
        let mut source = tag.default_source();
        source.center = DVec3::new(1.0, 2.0, 3.0);
        scene.add(source).unwrap();
    }
    let records = scene.to_records();
    assert_eq!(records.len(), 8);
    let restored = SourceScene::from_records(&records);
    assert_eq!(restored.entries.len(), 8);
    assert_eq!(restored.entries[0].source.kind, scene.entries[0].source.kind);
    assert_eq!(restored.entries[7].source.kind, scene.entries[7].source.kind);
}
```

- [ ] **Step 2: Run the test filter to verify RED**

Run: `cargo test --test suite persistence_test::source`
Expected: compile error on the new fields/record.

- [ ] **Step 3: Implement the record and restore policy**

In `src/source_scene.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct SourceRecord {
    pub enabled: bool,
    pub kind: String,
    pub center: [f64; 3],
    pub axis: [f64; 3],
    pub strength: f64,
    pub radius: f64,
    pub length: f64,
    pub turns: u32,
}

pub fn record(&self) -> SourceRecord;          // Source -> flat record
pub fn source(&self) -> Result<Source, String>; // rejects unknown/invalid
pub fn to_records(&self) -> Vec<SourceRecord>;
pub fn from_records(records: &[SourceRecord]) -> Self; // skips invalid, sets no notice
```

`source()` maps `kind` strings exactly to the eight kinds, uses only the
parameters that kind needs, and calls the same validation path as `SourceSet`.
`from_records` keeps valid enabled/disabled entries and ignores malformed ones.

In `src/persistence.rs`, add to `ViewRecord`:

```rust
#[serde(default)] pub field_mode: FieldMode,
#[serde(default)] pub source_entries: Vec<SourceRecord>,
#[serde(default)] pub source_display: SourceDisplay,
#[serde(default = "default_source_softening")] pub source_softening: f64,
#[serde(default)] pub source_allow_inaccurate: bool,
```

`source_allow_inaccurate` uses
`#[serde(default, deserialize_with = "deserialize_bool_or_false")]`, where a
private untagged helper accepts `bool` and treats every other value as `false`
without failing the envelope. `default_source_softening` returns `0.1`. `capture` writes all fields from
`state`; `restore` sanitizes:
- `source_entries` -> `SourceScene::from_records`; if the input length differs,
  set `state.source_notice = Some("Some saved sources were invalid and were skipped".into())`;
- softening outside `1e-6..=1.0` or non-finite -> `0.1`;
- `allow_inaccurate` only if exactly true; otherwise false;
- `field_mode = Sources` only if the resulting scene has at least one valid
  entry, else `Formula`;
- `state.source_sampling = None`; mark dirty.
Restore sources after formula/controls so a legacy formula is preserved. Add
`State::reset_sources()` (clears entries, cache, notice, restores scene defaults)
and call it from the global-reset path in `src/app/input.rs` alongside the
existing plot reset. Do not call it from plot-section reset.

- [ ] **Step 4: Run the persistence filter to verify GREEN**

Run: `cargo test --test suite persistence_test::`
Expected: all persistence tests pass, including the new six.

- [ ] **Step 5: Commit**

```bash
git add src/persistence.rs src/source_scene.rs src/app/input.rs tests/persistence_test.rs docs/CODEBASE.md
git commit -m "feat: persist source scenes with safe migration" -m "Generated-by: OpenCode"
```

---

### Task 4: Sources rail, panel and formula-bar mode switch

**Files:**
- Create: `src/ui/sources_panel.rs`
- Modify: `src/ui/mod.rs`, `src/ui/formula_bar.rs`, `src/ui/sidebar.rs`,
  `src/ui/layout.rs`, `src/app/panels.rs`, `src/app/mod.rs`
- Test: `tests/sources_panel_test.rs`, `tests/layout_test.rs`,
  `tests/responsive_test.rs`, `tests/button_sizing_test.rs`, `tests/suite.rs`
- Docs: `docs/CODEBASE.md`

**Interfaces:**
- Consumes: Task 1/2 state and scene; existing `theme`, `style_panel`, layout
  and accesskit test helpers.
- Produces: `SourcesPanelState`, `SourceDraft`, `sources_panel::show`,
  `sources_panel::status_text`, `FormulaBarState.open_sources`,
  `LayoutState.{sources_expanded, sources_width}`, `SheetState::Sources`.

- [ ] **Step 1: Write the failing UI tests**

```rust
use egui::{Pos2, Rect, vec2};
use vector_field_explorer::source_scene::{
    FieldMode, SourceDisplay, SourceFamilyStatus, SourceSampling, SourceScene, SourceKindTag,
};
use vector_field_explorer::ui::formula_bar::{self, FormulaBarState};
use vector_field_explorer::ui::layout::{LayoutState, SheetState};
use vector_field_explorer::ui::sources_panel::{self, SourcesPanelState};
use vector_field_explorer::viewmodel::State;

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    ctx
}

struct WidgetNode {
    role: egui::Role,
    label: String,
    rect: Rect,
    disabled: bool,
}

fn nodes(output: &egui::FullOutput) -> Vec<WidgetNode> {
    let Some(update) = output.platform_output.accesskit_update.as_ref() else {
        return Vec::new();
    };
    update
        .nodes
        .iter()
        .filter_map(|(_, node)| {
            let bounds = node.bounds()?;
            let label = node
                .label()
                .or_else(|| node.value())
                .unwrap_or_default()
                .to_owned();
            (!label.is_empty()).then(|| WidgetNode {
                role: node.role(),
                label,
                rect: Rect::from_min_max(
                    Pos2::new(bounds.x0 as f32, bounds.y0 as f32),
                    Pos2::new(bounds.x1 as f32, bounds.y1 as f32),
                ),
                disabled: node.is_disabled(),
            })
        })
        .collect()
}

fn find<'a>(nodes: &'a [WidgetNode], label: &str) -> Option<&'a WidgetNode> {
    nodes.iter().find(|node| node.label == label)
}

fn has(nodes: &[WidgetNode], label: &str) -> bool {
    find(nodes, label).is_some()
}

fn click_at(ctx: &egui::Context, width: f32, pos: Pos2, draw: &mut impl FnMut(&mut egui::Ui)) {
    for pressed in [true, false] {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 900.0))),
                events: vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ui| draw(ui),
        );
        output.textures_delta.clear();
    }
}

#[test]
fn sources_panel_adds_removes_and_resets_through_real_widgets() {
    for (width, compact) in [(1200.0, false), (390.0, true)] {
        let ctx = context();
        let mut state = State::new(true);
        state.set_field_mode(FieldMode::Sources);
        let mut panel = SourcesPanelState::default();
        let mut last = None;
        let mut draw = |ui: &mut egui::Ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                sources_panel::show(ui, &mut state, &mut panel, compact);
            });
        };
        for _ in 0..3 {
            last = Some(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 900.0))),
                    ..Default::default()
                },
                |ui| draw(ui),
            ));
        }
        let out = last.unwrap();
        let painted = nodes(&out);
        assert!(has(&painted, "Sources"));
        assert!(has(&painted, "Add"));
        assert!(has(&painted, "Advanced"));
        assert!(has(&painted, "Show finite results labelled \"accuracy not met\""));
        assert!(has(
            &painted,
            "Streamlines and particles use the formula field; switch to Formula to use them."
        ));
        let add = find(&painted, "Add").unwrap().rect.center();
        click_at(&ctx, width, add, &mut draw);
        assert_eq!(state.sources.entries.len(), 1);
        for _ in 0..3 {
            last = Some(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 900.0))),
                    ..Default::default()
                },
                |ui| draw(ui),
            ));
        }
        let painted = nodes(&last.unwrap());
        let remove = find(&painted, "Remove source").unwrap().rect.center();
        click_at(&ctx, width, remove, &mut draw);
        assert!(state.sources.entries.is_empty());
    }
}

#[test]
fn formula_bar_switches_mode_and_hides_the_editor() {
    let ctx = context();
    let mut state = State::new(false);
    let mut bar = FormulaBarState::default();
    let mut draw = |ui: &mut egui::Ui| {
        egui::CentralPanel::default().show(ui, |ui| {
            formula_bar::show(ui, &mut state, &mut bar, false);
        });
    };
    let out = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 240.0))),
            ..Default::default()
        },
        |ui| draw(ui),
    );
    let painted = nodes(&out);
    assert!(has(&painted, "Formula") && has(&painted, "Sources"));
    let sources = find(&painted, "Sources").unwrap().rect.center();
    click_at(&ctx, 1200.0, sources, &mut draw);
    assert_eq!(state.field_mode, FieldMode::Sources);
    let out = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 240.0))),
            ..Default::default()
        },
        |ui| draw(ui),
    );
    let painted = nodes(&out);
    assert!(!painted.iter().any(|node| node.role == egui::Role::TextInput));
    assert!(has(&painted, "Open Sources"));
}

#[test]
fn invalid_draft_does_not_replace_the_committed_source() {
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Sources);
    state
        .sources
        .add(SourceKindTag::ChargedRing.default_source())
        .unwrap();
    let mut panel = SourcesPanelState::default();
    panel.sync(&state.sources);
    let original = state.sources.entries[0].source.clone();
    let error = panel.set_radius(0, 0.0);
    assert!(error.is_some());
    assert_eq!(state.sources.entries[0].source, original);
    assert!(panel.drafts[0].error.is_some());
}

#[test]
fn advanced_controls_update_softening_and_opt_in_only() {
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Sources);
    state
        .sources
        .add(SourceKindTag::PointCharge.default_source())
        .unwrap();
    let mut panel = SourcesPanelState::default();
    panel.sync(&state.sources);
    assert!(panel.set_softening(&mut state, 0.02).is_ok());
    assert_eq!(state.sources.softening, 0.02);
    panel.set_allow_inaccurate(&mut state, true);
    assert!(state.sources.allow_inaccurate);
    assert!(panel.set_softening(&mut state, 1e-9).is_err());
}

#[test]
fn source_status_text_names_hidden_and_shown_accuracy() {
    let mut sampling = SourceSampling::default();
    sampling.electric = SourceFamilyStatus {
        sources: 1,
        accuracy_not_met_total: 3,
        ..SourceFamilyStatus::default()
    };
    let hidden = sources_panel::status_text(&sampling).expect("hidden accuracy is reported");
    assert!(hidden.contains('3'));
    assert!(hidden.contains("hidden"));

    sampling.allow_inaccurate = true;
    sampling.electric.accuracy_not_met_drawn = 3;
    let shown = sources_panel::status_text(&sampling).expect("shown accuracy is reported");
    assert!(shown.contains("accuracy not met"));

    sampling.magnetic = SourceFamilyStatus {
        sources: 1,
        unavailable: 2,
        ..SourceFamilyStatus::default()
    };
    let unavailable = sources_panel::status_text(&sampling).expect("unavailable is reported");
    assert!(unavailable.contains("unavailable"));
}

#[test]
fn layout_and_sheets_expose_sources() {
    let mut layout = LayoutState::default();
    assert!(!layout.sources_expanded);
    layout.toggle_sources();
    assert!(layout.sources_expanded);
    layout.sources_width = 10.0;
    layout.sanitize();
    assert!(layout.sources_width >= 180.0);
    let mut sheet = SheetState::default();
    sheet.open(SheetState::Sources);
    assert!(sheet.shows(SheetState::Sources));
}
```

- [ ] **Step 2: Run the UI tests to verify RED**

Run: `cargo test --test suite sources_panel_test::`
Expected: compile error, missing module and fields.

- [ ] **Step 3: Implement the panel and layout**

`src/ui/sources_panel.rs`:

```rust
#[derive(Debug, Clone)]
pub struct SourceDraft {
    pub committed: crate::sources::Source,
    pub strength: f64,
    pub radius: f64,
    pub length: f64,
    pub turns: u32,
    pub error: Option<String>,
    pub dirty: bool,
}

#[derive(Debug, Default)]
pub struct SourcesPanelState {
    pub add_kind: SourceKindTag,
    pub drafts: Vec<SourceDraft>,
}

impl SourcesPanelState {
    pub fn sync(&mut self, scene: &SourceScene);
    pub fn set_radius(&mut self, index: usize, value: f64) -> Option<String>;
    pub fn set_softening(&mut self, state: &mut State, value: f64) -> Result<(), String>;
    pub fn set_allow_inaccurate(&mut self, state: &mut State, allowed: bool);
    pub fn commit(&mut self, index: usize, state: &mut State) -> Option<String>;
}

pub fn show(ui: &mut egui::Ui, state: &mut State, panel: &mut SourcesPanelState, compact: bool);
pub fn status_text(sampling: &SourceSampling) -> Option<String>;
```

`show` renders the display selector (`Electric`, `Magnetic`, `Both` plus the
E amber/B cyan legend when Both), the add combo/`Add` button, each entry's
enable checkbox, kind label, `Remove source`, center/axis and kind parameters,
`Reset this source`, then a collapsed `Advanced` area containing `Softening`,
the model note, the exact checkbox `Show finite results labelled "accuracy not met"`,
and the status text. `Add` is disabled at 64. `sync` rebuilds drafts only for
new/removed entries or when a draft is not dirty, so invalid text survives.
Every valid edit calls `state.set_source_scene` with the replaced entry; invalid
edits set `draft.error` and leave the committed scene untouched.

Add `sources_expanded`/`sources_width` to `LayoutState` (default false/280,
sanitize 180..=460) and `SheetState::Sources`. In `src/app/mod.rs` own a
`SourcesPanelState`. In `src/app/panels.rs` add the **Sources** rail and compact
tab, draw `sources_panel::show` inside the existing scroll area, and add a small
viewport overlay from `status_text`. In `src/ui/formula_bar.rs` add
`FormulaBarState.open_sources`, the `Formula`/`Sources` selector in vector mode,
and the Sources summary + `Open Sources` button; do not show the editor in
Sources. When the flag is set, the app expands the rail or opens the compact
sheet and clears the flag. Add a `Sources` help row in the panel; hide the
Features sections in `src/ui/sidebar.rs` when `state.field_mode == Sources` and
show the note “Streamlines and particles use the formula field; switch to
Formula to use them.”

- [ ] **Step 4: Run the UI regressions**

Run:
`cargo test --test suite sources_panel_test::`
`cargo test --test suite layout_test::`
`cargo test --test suite responsive_test::`
`cargo test --test suite button_sizing_test::`
`cargo test --test suite sidebar_controls_test::`
Expected: all pass at wide and compact sizes.

- [ ] **Step 5: Commit**

```bash
git add src/ui/sources_panel.rs src/ui/mod.rs src/ui/formula_bar.rs src/ui/sidebar.rs src/ui/layout.rs src/app/panels.rs src/app/mod.rs tests/sources_panel_test.rs tests/layout_test.rs tests/responsive_test.rs tests/button_sizing_test.rs tests/suite.rs docs/CODEBASE.md
git commit -m "feat: add the responsive sources panel" -m "Generated-by: OpenCode"
```

---

### Task 5: Presets, CAS, help, docs and full gates

**Files:**
- Modify: `src/ui/presets.rs`, `src/ui/cas_panel.rs`,
  `src/ui/help_overlay.rs`, `README.md`, `GLOSSARY.md`, `docs/CODEBASE.md`,
  `docs/superpowers/README.md` if needed
- Test: `tests/ui_preset_test.rs`, `tests/cas_panel_test.rs`,
  `tests/help_overlay_test.rs`, `tests/sources_panel_test.rs`
- Docs: help/README/GLOSSARY.

**Interfaces:**
- Consumes: Tasks 1–4.
- Produces: preset mode return, CAS refusal in Sources, help rows, docs.

- [ ] **Step 1: Write the failing integration tests**

```rust
use vector_field_explorer::source_scene::{FieldMode, SourceKindTag};
use vector_field_explorer::ui::presets::PresetPanel;
use vector_field_explorer::ui::cas_panel::CasPanelState;
use vector_field_explorer::ui::help_overlay::sources_help_rows;
use vector_field_explorer::viewmodel::State;

#[test]
fn classic_presets_return_to_formula_and_keep_the_source_scene() {
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Sources);
    state
        .sources
        .add(SourceKindTag::PointCharge.default_source())
        .unwrap();
    PresetPanel::load_field("Gravity well", &mut state);
    assert_eq!(state.field_mode, FieldMode::Formula);
    assert_eq!(state.sources.entries.len(), 1);
    assert!(state.formula.len() > 3);
}

#[test]
fn cas_apply_refuses_source_mode_without_touching_the_formula() {
    let mut state = State::new(false);
    state.set_formula("(y, -x, 0)".into());
    let formula = state.formula.clone();
    let mut panel = CasPanelState::default();
    panel.input = "x".into();
    panel.refresh(3);
    assert!(panel.can_apply(3), "the vector formula edit is applicable");
    state.set_field_mode(FieldMode::Sources);
    assert!(!panel.apply(&mut state, 3));
    assert_eq!(state.formula, formula);
}

#[test]
fn help_explains_source_mode_and_accuracy_opt_in() {
    let text = sources_help_rows()
        .iter()
        .map(|row| format!("{} {}", row.input, row.action))
        .collect::<Vec<_>>()
        .join(" ");
    assert!(text.contains("Sources"));
    assert!(text.contains("accuracy not met"));
    assert!(text.contains("formula"));
}
```

- [ ] **Step 2: Run the filters to verify RED**

Run:
`cargo test --test suite ui_preset_test::source`
`cargo test --test suite cas_panel_test::source`
`cargo test --test suite help_overlay_test::source`
Expected: compile error/missing `sources_help_rows` and mode checks.

- [ ] **Step 3: Implement**

- `PresetPanel::load_field` calls `state.set_field_mode(FieldMode::Formula)`
  before its existing plot/mode/formula setters; source entries are untouched.
- `CasPanelState::apply` returns false when `state.field_mode == Sources`;
  `show` disables Apply with the text “CAS edits the formula field; switch to
  Formula.”
- Add `pub const SOURCES_HELP_ROWS: &[HelpRow]`, `SOURCES_HELP_SECTION` and
  `pub fn sources_help_rows() -> &'static [HelpRow]` to `help_overlay.rs`,
  following the existing scalar-help pattern, covering:
  Formula/Sources separation, all-eight editors, E/B/Both and no summation,
  scene-wide softening/model distortion, default-off accuracy opt-in, hidden
  non-finite/unavailable results, and the fact that streamlines/particles/click
  seeding remain formula features. Render the section in the help window.
- README feature bullet and GLOSSARY entries for **source scene**, **field
  mode**, **E/B display** and **accuracy opt-in**. Add `docs/sources-panel.md`
  if the existing docs pattern expects a page; keep it short and link it from
  README.
- `docs/CODEBASE.md` rows for the new module/panel/record.

- [ ] **Step 4: Run the focused and full gates**

Run:
`cargo test --test suite ui_preset_test::`
`cargo test --test suite cas_panel_test::`
`cargo test --test suite help_overlay_test::`
`cargo test --test suite sources_panel_test::`
`cargo test --test suite source_mode_test::`
`cargo test --test suite source_scene_test::`
`cargo test --test suite persistence_test::`
`cargo test --test suite`
`cargo build`
`cargo clippy --all-targets -- -D warnings`
`rustfmt --edition 2024 --check --config skip_children=true <changed .rs files>`
`python3 scripts/check-docs.py`
`git diff --check`
`SMOKE_BIN="$CARGO_TARGET_DIR/debug/vector-field-explorer" scripts/smoke-run.sh 15`
Expected: all pass; native smoke prints `smoke: PASS`.

- [ ] **Step 5: Commit**

```bash
git add src/ui/presets.rs src/ui/cas_panel.rs src/ui/help_overlay.rs README.md GLOSSARY.md docs/CODEBASE.md docs/sources-panel.md tests/ui_preset_test.rs tests/cas_panel_test.rs tests/help_overlay_test.rs tests/sources_panel_test.rs tests/suite.rs
git commit -m "feat: finish source-mode integration and docs" -m "Generated-by: OpenCode"
```

## Self-Review

**Spec coverage:** every design section maps to a task: scene/model/CRUD and
all-eight defaults (Task 1); field mode, active dimension, source sampling,
E/B/Both, accuracy gating, 2D plane basis, transient/clock gating (Task 2);
persistence, migration, reset (Task 3); Sources rail/tab, formula-bar switch,
advanced controls, status/legend, layout (Task 4); presets, CAS, help, docs,
full gates (Task 5). Glyph/preset/export work is explicitly deferred to #26.

**Step scan:** every test step contains executable assertions with concrete
values; every implementation step names the file/signature and only the bodies
the tests do not determine. No placeholder/TBD remains. UI tests use the existing
accesskit/`ctx.run_ui` harness and real buttons.

**Type consistency:** `SourceScene`, `SourceRecord`, `SourceSampling`,
`SourceFamilyStatus`, `FieldMode`, `SourceDisplay`, `SourceKindTag`,
`SourcesPanelState`, `LayoutState.sources_*`, `SheetState::Sources` and
`FormulaBarState.open_sources` are introduced once and consumed with the same
names in later tasks.

**Review Focus:** each listed failure mode has an owning test: invalid draft
(Task 1/4), old payload opt-in (Task 3), E/B separation (Task 2), 2D basis
(Task 2), mode-state preservation (Task 2/5).

**Proportion:** the plan is longer than the design in test code but does not
transcribe production bodies; implementation steps give signatures and the
decisions the tests do not fix.
