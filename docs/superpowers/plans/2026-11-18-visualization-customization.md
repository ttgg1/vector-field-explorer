# Visualization Customization Implementation Plan

> **DRAFT — NOT EXECUTABLE.** Written-design approval, plan approval and fresh
> owner authorization are required before any implementation or dispatch.
> For future agentic workers: use subagent-driven-development or executing-plans
> only after those gates; the checkboxes below are proposals, not authorization.

**Goal:** Let students customize presentation and interaction without altering physics.

**Architecture:** Extend existing pure style, colormap and viewmodel seams.
Keep appearance records distinct from scenes and cameras; route accepted edits
through one bounded history. Preserve existing renderer and capture boundaries.

**Tech Stack:** Rust, eframe/egui, wgpu, serde/RON; native and WebAssembly.

**Spec:** [Draft customization/physics design](../specs/2026-11-18-customization-physics-design.md).

## Global Constraints

- Planning BASE: `4ed3610d1a642e7b8abaf68d7f73f5e614cb827f`.
- Epic [#80](https://github.com/ttgg1/vector-field-explorer/issues/80) owns live
  requirements; issue numbers below use that repository, not historical local IDs.
- This draft owns only this Markdown file; prospective Rust paths are not claims.
- Preserve native/wasm, GPL-3.0-only compatibility, old settings and one SimClock.
- CAS stays native-only; normalized values are not SI; E and B are never summed.
- Retain model/export warnings regardless of opacity, profiles or custom palettes.
- #18 performance evidence is deferred, not passed and not a new global blocker.
- Serialize shared files, especially `src/viewmodel.rs`, `src/persistence.rs`,
  `src/ui/sidebar.rs` and suite registration; rebase ownership against current Git.
- Exact serialization colors/units, migration policy and new range semantics need
  bounded design approval; this draft does not approve constants or algorithms.

## Review Focus

- Corrupt/nonfinite styling: safe rejection/defaults without poisoning rendering (#82).
- Signed/zero/log data: meaningful legend or explicit unsupported result (#83).
- Profile application: physics, camera, clock and inactive modes unchanged (#85).
- Rapid edits/transition boundaries: one drag event, no frame/file undo (#86).
- Typing/focus/capture loss: recovery remains available after remapping (#88).

## Confirmed seams and cross-plan contracts

CodeGraph inspected the main checkout at BASE; this worktree has no index.
Existing `reference_lines(&Domain, bool) -> Vec<LineInstanceGPU>` and
`reference_plane_lines(&PlaneFrame, bool, bool) -> Vec<LineInstanceGPU>` produce
reference geometry; `Colormap::colormap(&self, f32, f32, f32) -> [u8; 3]`
is the compatibility color seam. `AppStyle::sanitized(self) -> Self` styles UI,
not a license to store viewport physics in appearance.
`sidebar::apply(&mut State, &mut Controls)` already hands changes to setters.

The [physics plan](2026-11-18-physics-exploration.md) consumes #83 for #93;
#90 must keep #85 appearance profiles, camera presets and scenes separate.
#89 parameter commits may later use #86 transactions without depending on history
for basic parameter evaluation. #99 is a child of #81 after #80 and core siblings
#89–#98; it does not depend on its own parent or block core capabilities.
These issue-level contracts are stable; proposed Rust APIs below require review.

## Task 1 — #82 viewport style foundation

**Dependencies:** existing reference geometry; approved bounded style/record design.
**Prospective exclusive paths:** create `src/viewport_style.rs`,
`tests/viewport_style_test.rs`; modify `src/reference.rs`,
`src/ui/style_panel.rs`, `src/viewmodel.rs`, `src/persistence.rs`,
`src/lib.rs`, `tests/suite.rs`, `tests/reference_lines_test.rs`.
Renderer/background integration paths must be inspected and added to a fresh
exclusive brief before that separate integration slice; no implicit ownership.
**Proposed interface:** `ViewportStyle::validated(&self) -> Result<(), String>`
and `ViewportStyle::default()` in the new module; compatibility reference
functions remain available. Styled reference API signatures require approval.

- [ ] Design: specify background/grid/box/per-axis colors, applicable opacity and
  widths, exact-entry format, previews, scoped reset, and named dark-laboratory,
  white-paper and high-contrast looks; approve portable color conventions.
- [ ] RED: add `viewport_style_test::rejects_nonfinite_width` and
  `reference_lines_test::styled_axes_follow_world_plane`; assert invalid edits
  retain the previous style and rotated slices retain world-axis identity.
- [ ] Run `cargo test viewport_style_test::` and the named reference test;
  observe the missing behavior rather than unrelated compilation failure.
- [ ] GREEN: implement the approved pure style seam and settings migration;
  prove old records retain existing defaults and hidden decorations stay hidden.
- [ ] Run focused tests, then close this slice through the verification protocol.
- [ ] STOP before rendering integration until its exact paths, capture parity
  tests and smoke requirement have been independently reviewed and authorized.

## Task 2 — #83 gradients and quantity mappings

**Dependencies:** existing colormap/ranges; independent of #82 history.
**Prospective paths:** modify `src/colormap.rs`, `src/ui/sidebar.rs`,
`src/viewmodel.rs`, `src/persistence.rs`, `tests/colormap_test.rs`,
`tests/sidebar_colormap_ui_test.rs`.
**Interface:** preserve the existing `Colormap::colormap` signature; approve
new mapping/legend descriptor signatures before adding custom-gradient callers.

- [ ] Design editable stops/reversal and portable colors; specify duplicate stops,
  invalid stops, component/direction coordinate conventions, sequential/diverging/
  cyclic legends, auto/manual/frozen ranges and signed/zero/log behavior.
- [ ] RED: add `colormap_test::reversal_swaps_endpoints`,
  `colormap_test::signed_log_is_explicit` and
  `sidebar_colormap_ui_test::freeze_keeps_committed_range`; assert the approved
  domain policy and exact equality between legend and sampled effective mapping.
- [ ] Run `cargo test colormap_test::` and
  `cargo test sidebar_colormap_ui_test::`; observe expected RED.
- [ ] GREEN: implement approved mapping/range logic without changing field values;
  reject nonfinite ranges and preserve old built-in colormaps.
- [ ] Verify pure/UI tests and approve renderer/export integration brief with
  exact paths and parity tests before extending beyond this owned slice.

## Task 3 — #84 layer styles and family legends

**Dependencies:** #83 effective mapping contract.
**Prospective design ownership:** create
`docs/superpowers/specs/2026-11-18-layer-style-contract.md`.
**Interface deliverable:** exact inheritance/override descriptor shared by arrows,
streamlines, particles/trails, E/B family styles and Ode trajectories.

- [ ] Inventory actual paint/export consumers and propose exact implementation
  ownership; approve conflict rules between family colors and quantity mappings.
- [ ] Specify effective-style resolution, warning visibility and legend semantics;
  attached tests must cover inherit→override→inherit and independent E/B styles.
- [ ] Require render/export fixtures for every supported layer and capture path;
  approve a finite feature matrix rather than assuming all paths share styling.
- [ ] STOP: produce a reviewed implementation subplan with RED tests and exact
  consumer signatures; this roadmap task authorizes no renderer changes.

## Task 4 — #85 portable appearance profiles

**Dependencies:** #82/#83/#84; #90 consumes the separate record contract.
**Prospective paths:** create `src/appearance_profile.rs`,
`tests/appearance_profile_test.rs`; modify `src/persistence.rs`,
`src/ui/style_panel.rs`, `src/viewmodel.rs`, `src/lib.rs`, `tests/suite.rs`.
**Proposed interface:** `AppearanceProfile::apply(&self, state: &mut State)
-> Result<(), String>`; approve record fields/versioning before implementation.

- [ ] Design named records, import/export, atomic validation, error presentation
  and compatible migration; exclude scene formulas, parameters, source geometry,
  camera and simulation state by construction.
- [ ] RED: `appearance_profile_test::apply_changes_only_appearance` snapshots
  excluded state, applies a visibly different profile and asserts equality of
  all excluded fields, including durable inactive-mode data.
- [ ] RED: `appearance_profile_test::invalid_import_is_atomic` checks unknown
  versions/nonfinite values preserve the existing record and live appearance.
- [ ] Run `cargo test appearance_profile_test::`; observe RED, implement minimally,
  then rerun GREEN and review storage errors on native and browser seams.
- [ ] Close through verification; coordinate #90 without merging record types.

## Task 5 — #86 accepted-edit history and scoped resets

**Dependencies:** existing state setters; independent foundation before #87.
**Prospective design ownership:** create
`docs/superpowers/specs/2026-11-18-edit-history-contract.md`.
**Interface deliverable:** approved transaction begin/commit/cancel/undo/redo
signatures and snapshot scope, not a clone of arbitrary live renderer state.

- [ ] Define bounded storage/eviction, drag coalescing, rejected-edit behavior,
  redo invalidation, preset/mode transitions and color/camera/simulation resets.
- [ ] Specify tests `drag_is_one_event`, `frames_do_not_record`,
  `file_effects_do_not_replay`, `rejected_edit_keeps_history`, and
  `reset_scope_preserves_other_state`, with explicit before/after snapshots.
- [ ] Identify immutable model snapshots versus runtime caches and shared clock;
  decide what a mode/preset transition retains with owner approval.
- [ ] STOP until bounded history design and exact implementation paths/tests are
  approved; each later implementation slice follows RED→GREEN→review.

## Task 6 — #87 exact and reusable seed layouts

**Dependencies:** #86 transaction contract; existing seed/plane infrastructure.
**Prospective design ownership:** create
`docs/superpowers/specs/2026-11-18-seed-layout-contract.md`.
**Interface deliverable:** approved plane/world coordinate record and bounded
line/circle/region generator APIs; reuse the single history.

- [ ] Specify exact placement, selection/move/delete, reusable record import,
  out-of-domain policy, plane changes, generation work and storage limits.
- [ ] Pin tests for rotated-plane round trips, deterministic generation,
  invalid/nonfinite input, count/work bounds and one undo per accepted drag.
- [ ] Inventory pointer picking, streamline regeneration and capture consumers;
  propose exact source/test ownership and focused commands in a reviewed subplan.
- [ ] STOP before implementation; do not invent a second history or an
  unbounded region seeder to fill unresolved interaction decisions.

## Task 7 — #88 interaction profiles

**Dependencies:** current focus/camera/capture safeguards; independent of #87.
**Prospective design ownership:** create
`docs/superpowers/specs/2026-11-18-interaction-profile-contract.md`.
**Interface deliverable:** action-based binding/profile schema, not raw egui events.

- [ ] Approve shortcut conflicts, reserved recovery actions, sensitivity/zoom/
  inversion units, defaults/reset, physical/logical keys and platform exclusions.
- [ ] Specify tests for typing suppression, focus loss, pointer-capture recovery,
  conflict reporting, corrupted profiles and reset in narrow layouts.
- [ ] Inspect exact input consumers before claiming files; preserve Escape and
  recovery even when importing adversarial/remapped bindings.
- [ ] STOP for approved bounded implementation brief and fresh authorization.

## Verification, review and handoff protocol

- [ ] NOW, docs only: `python3 scripts/check-docs.py`, local link existence,
  policy/coverage self-review and `git diff --check`; no Cargo or installation.
- [ ] FUTURE behavior slices: record fresh BASE/claims/paths; write failing tests,
  run the named filter and inspect correct RED, implement minimally, rerun GREEN,
  refactor, run full applicable gates and obtain fresh independent actual-diff review.
- [ ] Use explicit Rustup PATH/RUSTC per [Workflow](../../../references/workflow.md):
  `cargo build`, `scripts/test.sh`, `cargo clippy --all-targets -- -D warnings`,
  `python3 scripts/check-docs.py`, `git diff --check`, changed-file rustfmt.
- [ ] Run `scripts/smoke-run.sh 15` for renderer/window/input changes. Retain
  ten jobs, at most two Cargo processes, distinct persistent target/TMPDIR pairs
  under `/home/paul/.cache/vector-field-explorer-targets/`; never clean caches.
- [ ] Web/browser/manual visual checks are owner-waived, not passed. Preserve
  automated functional/layout tests and web behavior despite the waiver.
- [ ] Future standard work: Luna lowest; numerical/rendering: Sol lowest;
  fresh independent reviewer, fixes Luna lowest. Complexity does not raise effort.
- [ ] Report exact BASE/HEAD, paths, commands/results, waivers and status.
  Future authorized commits need `Generated-by: OpenCode`; no automatic execution,
  commit, push or merge follows publication of this draft.
- [ ] Self-review: #82–#88 covered; #83/#85/#86 cross-plan contracts consistent;
  proposed APIs distinguished from confirmed signatures; unresolved designs stop.
