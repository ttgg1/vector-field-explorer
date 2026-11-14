# Reference Visibility Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Independently hide the viewport grid/domain box and colored world axes, with shared 2D/3D preferences that survive restarts.

**Architecture:** Keep `SceneGPU.lines` as the single composition path for painting, PNG, and 2D SVG. Split reference generation into grid-plus-box and axes contributions, then compose selected contributions before unmodified field and click lines; state setters and the existing sidebar/persistence handoffs carry the two preferences.

**Tech Stack:** Rust 2024, eframe/egui 0.37, existing serde/RON settings and GPU-free `tests/suite.rs` integration tests; no new dependency.

**Spec:** `docs/superpowers/specs/2026-11-11-reference-visibility-design.md` (approved ticket: main checkout `.scratch/field-viewport/issues/77-reference-visibility-controls.md`).

## Global Constraints

- Controls are **Show grid** and **Show coordinate system**, independent and checked by default, in Display in both Wide and Compact; Display reset restores both visible.
- Grid includes the 3D face grid **and domain wireframe box**, or the 2D plane grid; coordinate system means colored X/Y (2D) or X/Y/Z (3D) axes only.
- One shared pair on `viewmodel::State` and `ui::sidebar::Controls` applies across 2D/3D; edits dirty the scene, never change sampling options or hide flow/click lines.
- Persist in `persistence::ViewRecord` via existing capture/restore, with missing fields `true` and existing corrupt-settings fallback unchanged; no new envelope/version.
- Preserve current ordering when both visible: face grid, box, axes, flow, clicks (2D: plane grid, axes, flow, clicks); the constructor's initial backdrop honors restored flags.
- No per-mode preferences, field-content toggles, ticks, numeric labels, gizmos, renderer overhaul, or independent screenshot composition. Preserve GPL-3.0-only compatibility and existing native/wasm behavior without adding dependencies.
- Work begins only after controller confirms ticket 76's merged ownership and authorizes implementation; do not infer dispatch from this document. Initial independent review per task; controller owns ledger, queue and integration.

## Review Focus

- **Restored hidden flags before first frame:** constructor backdrop must already be filtered; Task 4 tests restored state/mirror, and Task 3 audits the constructor call site against the production composition seam. Do not claim direct private-scene/pixel proof.
- **Click seed/undo after hiding references:** every recomposition path must retain both visibility choices and existing click/flow geometry; test in Task 3 with distinct line sentinels and call-site audit.
- **3D mode with a flat formula:** reference dimension follows `State::is_3d()`, not raw `mode_3d`; test in Task 3 using a flat field in 3D mode.
- **Flow colors matching axes:** composition must select by contribution, not filter already-composed lines by color; test in Task 3 using flow/click sentinels with axis/grid colors.
- **Old or corrupt storage:** omitted flags load visible while malformed settings retain the existing default fallback; test in Task 4 using the pre-existing serialized legacy fixture and corrupt-storage fixture.

---

## File map and task ownership

| File | Responsibility / owner task |
| --- | --- |
| `src/viewmodel.rs`, `tests/sidebar_controls_test.rs` | Two effective state flags, setter/dirty and shared-mode assertions (Task 1); Task 2 adds sidebar tests in the same owned test file **after** Task 1 review. |
| `src/ui/sidebar.rs`, `tests/sidebar_controls_test.rs` | Widget mirrors, Display checkboxes/tooltips/reset and pure `apply` handoff (Task 2). Existing `show_inner` serves Wide and Compact. |
| `src/reference.rs`, `src/app/mod.rs`, `src/app/simulation.rs`, `src/app/input.rs`, `tests/reference_lines_test.rs` | Geometry separation, initial backdrop and every rebuild/click/undo composition path (Task 3). Integration tests in `tests/reference_lines_test.rs` exercise the one narrow hidden production composition seam. No `src/renderer.rs`/`src/app/viewport.rs` or screenshot change. |
| `src/persistence.rs`, `tests/persistence_test.rs` | Default-true serde fields, capture/restore and old-record/storage tests (Task 4). |
| `docs/CODEBASE.md`, `README.md` | Update architecture/persistence map and user-facing controls description with the owning implementation task (Task 4). |

Test files already appear in `tests/suite.rs`; no registration edit. Serialize tasks in order for their shared `State`/`Controls` interfaces and test file; each task is its own reviewable TDD change/commit. Task 3 touches app composition only after Task 2 has landed. Docs/index additions for this planning work are not implementation commits. **2026-11-11 ruling:** the initial Task 3 plan's inline unit tests failed the enforced `tests/test_layout_test.rs` guard; the tested fix follows `references/rust.md`'s one `#[doc(hidden)] pub` production seam instead. The main-root ticket 77 ledger retains the original failure and both implementation commits. Do not restore inline tests.

### Task 1: Shared effective state

**Files:** Modify `src/viewmodel.rs`; test `tests/sidebar_controls_test.rs` (state-only assertions here; next task owns sidebar wiring).

**Interfaces:** Produce `State::{show_grid: bool, show_coordinate_system: bool}` and `State::set_show_grid(&mut self, bool) -> bool`, `State::set_show_coordinate_system(&mut self, bool) -> bool`; both start `true`, return whether changed and mark dirty only on change. Consume existing `State::{new, set_mode, set_formula, snapshot_if_dirty, view_options, sample}`. No per-mode fields or `sampling::ViewOptions` changes.

- [ ] **Step 1: Write failing tests.** In `tests/sidebar_controls_test.rs`, add `reference_visibility_defaults_and_setters_are_independent`: for `State::new(false)` and `State::new(true)`, both true; consume initial `snapshot_if_dirty`, assert same-value setters return false and leave `dirty == false`, toggle each false and assert only that flag changes, dirty becomes true, and `snapshot_if_dirty()` returns `Some` once. Add `reference_visibility_is_shared_across_mode_switches`: toggle both false, switch 2D→3D (`set_formula("(x, y, z)".into())`, `set_mode(true)`)→2D, assert neither flag resets; within a fixed mode/formula compare `view_options()` and sampled arrow content before/after visibility changes (use the existing `PartialEq` arrow data), so the toggles cannot alter field sampling.
- [ ] **Step 2: Verify RED.** Run `cargo test sidebar_controls_test::reference_visibility -- --nocapture`; expect missing fields/setters (before production edit).
- [ ] **Step 3: Implement the exact fields and setters** in `src/viewmodel.rs` using the `set_arrows` dirty/no-op pattern; no sampling or snapshot field gating.
- [ ] **Step 4: Verify GREEN.** Run the same filtered test; expect PASS.
- [ ] **Step 5: Commit** `feat: add shared reference visibility state` (ticket marker only with matching review record). Obtain initial independent review before Task 2.

### Task 2: Sidebar controls and Display reset

**Files:** Modify `src/ui/sidebar.rs`; test `tests/sidebar_controls_test.rs`.

**Interfaces:** Consume Task 1's `State` fields/setters. Produce `Controls::{show_grid: bool, show_coordinate_system: bool}` and preserve `Controls::{new, reset_display}` and `sidebar::apply(&mut State, &mut Controls)` signatures. Both checkboxes sit beside `Show arrows` inside `Section::Display`'s shared `show_inner`, labeled exactly **Show grid** and **Show coordinate system**; tooltip for grid explicitly names plane/face grid **and domain box**, coordinate tooltip names colored X/Y(/Z) axes and distinguishes field lines. No separate Compact rendering path.

- [ ] **Step 1: Write failing tests.** Extend `sidebar_defaults_mirror_the_seam_defaults` with both true/matching state; add `reference_visibility_applies_independently_without_redirtying`: after consuming the first snapshot, change each `Controls` flag separately and call `apply`, assert state/mirror pairs match, other flag remains unchanged, snapshot becomes `Some` once per edit, then untouched `apply` leaves `snapshot_if_dirty() == None`. Add `reference_visibility_survives_modes_and_display_reset`: hide both through `apply`, switch modes, `apply` again (sync the mode's density if needed), assert flags stay false; `reset_display` then `apply` restores both true without changing streamline toggle/seed mode. Add true/no-op assertions to `synced_controls_never_redirty_the_scene`.
- [ ] **Step 2: Verify RED.** Run `cargo test sidebar_controls_test::reference_visibility -- --nocapture`; expect missing Controls flags/reset behavior.
- [ ] **Step 3: Implement** the two mirror fields, defaults, Display reset, checkboxes/tooltips, setter calls and effective-state mirror refresh in `src/ui/sidebar.rs` only. Existing `section` header reset drives `reset_display`; avoid ticket-76 button/row sizing changes.
- [ ] **Step 4: Verify GREEN.** Run `cargo test sidebar_controls_test:: -- --nocapture`; expect PASS, including old Display tests.
- [ ] **Step 5: Commit** `feat: add reference visibility display controls`; obtain initial independent review before Task 3.

### Task 3: Selected reference geometry and single scene composition

**Files:** Modify `src/reference.rs`, `src/app/mod.rs`, `src/app/simulation.rs`, `src/app/input.rs`; test `tests/reference_lines_test.rs`.

**Interfaces:** Preserve `reference::reference_lines(&Domain, bool) -> Vec<LineInstanceGPU>` and its existing both-visible output/order. Introduce `pub(crate) reference::reference_lines_visible(domain: &Domain, sampling_3d: bool, show_grid: bool, show_coordinate_system: bool) -> Vec<LineInstanceGPU>`; it appends grid+box before axes and `reference_lines` delegates with `true, true`. Use exactly one `#[doc(hidden)] pub app::compose_lines(flow: &[LineInstanceGPU], clicks: &[LineInstanceGPU], domain: &Domain, sampling_3d: bool, show_grid: bool, show_coordinate_system: bool) -> Vec<LineInstanceGPU>` production-used seam, tested from the integration suite. The hidden visibility is the explicitly ruled test-layout exception; do not expose helper collections or widen `VfApp`/`SceneGPU`.

- [ ] **Step 1: Write failing tests.** In registered `tests/reference_lines_test.rs`, table-drive `app::compose_lines` for `(true,true)`, `(true,false)`, `(false,true)`, `(false,false)` and 2D/3D: 3D reference counts `6*(REFERENCE_DIVISIONS+1)+12+3`, grid-only excludes all 3 axes, axes-only has precisely 3 axis lines/no grid or box, neither has zero references; 2D counts `2*(REFERENCE_DIVISIONS+1)+2`, grid-only has no axes, axes-only has exactly X/Y, neither zero. Assert the original `reference_lines` equals all-visible prefix and order for a non-default domain and 2D grid stays at mid-z. Append distinct flow and click line sentinels **including colors identical to `AXIS_X_COLOR` and `GRID_COLOR`**; assert they survive every combination, remain ordered flow then click, and are not mistaken for reference lines. Include flat-formula/3D-mode `State::is_3d()` case (expect 2D reference counts) and a non-default-domain bounds check for selected contributions; preserve existing all-visible public-contract tests.
- [ ] **Step 2: Verify RED.** Run `cargo test reference_lines_test::reference_visibility -- --nocapture`; expect missing composition visibility signature. The first implementation's inline tests instead produced a test-layout failure in the full suite; both RED results are preserved in the ticket ledger.
- [ ] **Step 3: Implement** geometry split in `src/reference.rs` without color-based filtering; change `app::compose_lines` to take two flags, use it for constructor's initial `SceneGPU.lines` (empty flow/click), dirty rebuild in `simulation.rs`, and **both** click seed/undo recompositions in `input.rs`. Always derive dimension from `state.is_3d()`; append flow/click unchanged, retain current streamline count and `SceneGPU` pipeline.
- [ ] **Step 4: Verify GREEN.** Run `cargo test reference_lines_test:: -- --nocapture`, then `cargo test sidebar_controls_test:: -- --nocapture`, and the full `scripts/test.sh` test-layout guard; expect PASS. Audit all `compose_lines(` call sites (constructor, rebuild, click, undo) and `src/app/viewport.rs`: one composed scene feeds paint callback, PNG and 2D SVG; GPU-free tests assert lines, **not pixels or direct private-scene inspection**.
- [ ] **Step 5: Commit** `feat: compose selected reference lines` and the layout-fix commit with narrow-seam reason in its body; obtain initial independent review before Task 4.

### Task 4: Persist defaults, migration and document behavior

**Files:** Modify `src/persistence.rs`, `tests/persistence_test.rs`, `docs/CODEBASE.md`, `README.md`.

**Interfaces:** Consume Task 1 state, Task 2 controls and Task 3 composition. Extend `ViewRecord` with `pub show_grid: bool` and `pub show_coordinate_system: bool`; capture effective `State`, restore via `Controls` then existing `sidebar::apply`. Use explicit field-level `#[serde(default = "default_reference_visible")]` with `fn default_reference_visible() -> bool { true }` for **each** field: container-level `#[serde(default)]` alone does not guarantee missing booleans are true. Preserve `load`/`save` API and corrupt-envelope fallback.

- [ ] **Step 1: Write failing tests.** In `tests/persistence_test.rs`, set both false in `configured()` (or a dedicated state) and assert `ViewRecord::capture`, eframe storage `save/load`, `restore` into state and mirror, and `VfApp::new_with_storage` all retain false; also test each asymmetric pair independently to rule out swapped fields. Extend `default_record_matches_a_fresh_two_d_state` to assert both true; extend legacy `load_legacy(false)` fixture (which omits both keys) and a partial record containing only `show_grid: false` to assert missing coordinate flag true and restored state/mirror values correct. Extend `corrupt_storage_falls_back_to_defaults` to assert both true. Audit that the constructor passes restored flags to the same production composition path before the first frame; do not add a scene getter just for a test.
- [ ] **Step 2: Verify RED.** Run `cargo test persistence_test:: -- --nocapture`; expect missing record fields or false default/restore failures.
- [ ] **Step 3: Implement** explicit serde defaults/capture/restore in `src/persistence.rs`; leave envelope and corrupt fallback unchanged. Update `docs/CODEBASE.md` state→reference→SceneGPU flow, app constructor/rebuild and `ViewRecord` field/default descriptions; in `README.md` describe Display checkboxes, grid+box/axes distinction, shared modes and remembered preferences without claiming pixel-verified screenshots.
- [ ] **Step 4: Verify GREEN.** Run `cargo test persistence_test:: -- --nocapture`, `cargo test reference_lines_test:: -- --nocapture`, and `cargo test sidebar_controls_test:: -- --nocapture`; expect PASS.
- [ ] **Step 5: Commit** `feat: persist reference visibility preferences` (docs included); obtain initial independent review.

## Final verification and handoff (controller/implementer after reviewed tasks)

- [ ] Run exact Rust gates: `PATH=/home/paul/.cargo/bin:/usr/bin:/bin RUSTC=/home/paul/.cargo/bin/rustc /home/paul/.cargo/bin/cargo build`; `scripts/test.sh` (full `cargo test --all-targets`); `PATH=/home/paul/.cargo/bin:/usr/bin:/bin RUSTC=/home/paul/.cargo/bin/rustc /home/paul/.cargo/bin/cargo clippy --all-targets -- -D warnings`; `python3 scripts/check-docs.py`; `git diff --check`; changed-file formatting (`cargo fmt --check` only if it does not fail on unrelated drift; otherwise check only modified Rust files with `rustfmt --check` and record scope). Use explicit Rustup toolchain for scripts/Cargo, at most two concurrent Cargo processes with **separate persistent** `CARGO_TARGET_DIR` and `TMPDIR` under `/home/paul/.cache/vector-field-explorer-targets/`, 10-job cap; do not clean shared targets. For this rendering-composition change also run `scripts/smoke-run.sh 15` and report what it does and does not prove.
- [ ] Docs/link/policy consistency: `python3 scripts/check-docs.py`, `git diff --check`, check index/spec/plan links and reconcile `README.md`/`docs/CODEBASE.md` against workflow/ticket without superseding policy. All gates must be recorded against exact HEAD; report any failures, do not call waivers passes.
- [ ] Dedicated web/wasm compatibility and browser/manual visual, screenshot and separate painted-ink checks are **owner-waived, not passed** under `references/workflow.md` (overrides old ticket/preflight gate wording); retain same platform behavior and automated functional/layout tests. Native smoke is required, not a pixel-output assertion. No commit, push, integration, ledger or queue update by the docs-only planner; controller handles authorization, reviews and integration.
