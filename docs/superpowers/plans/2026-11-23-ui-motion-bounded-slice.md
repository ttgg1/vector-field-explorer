# UI motion — bounded first slice Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add central motion tokens with reduced-motion support and apply restrained motion to a small set of UI surfaces without idle repaint or layout changes.

**Architecture:** A new pure `src/ui/motion.rs` holds duration/easing tokens, a clamped progress sampler, and reduced-motion resolution. A persisted `AppStyle.reduced_motion` flag plus the platform preference gate every transition. Global button/toggle feedback is applied through `style_panel::apply`; short open/slide transitions for the compact sheet and floating viewport panels call `ctx.request_repaint()` only while in flight.

**Tech Stack:** Rust, eframe/egui (immediate mode), serde settings envelope, native + wasm.

**Spec:** `docs/superpowers/specs/2026-11-23-ui-motion-bounded-slice-design.md`

## Global Constraints

- GPL-3.0-only; no incompatible dependencies.
- No physics, solver, scene, or analysis changes.
- No continuous repaint while idle; settle to zero frames requested.
- Motion must not delay input, hide content, or change layout geometry.
- Preserve focus, keyboard operation, accessibility labels, compact layout, dark/light appearance.
- Reduced-motion (setting or platform preference) makes all state changes immediate.
- Rust gates: `cargo build`, `scripts/test.sh`, `cargo clippy --all-targets -- -D warnings`, `python3 scripts/check-docs.py`, `git diff --check`, changed-file formatting, `scripts/smoke-run.sh 15`.
- Toolchain: `PATH=/home/paul/.cargo/bin:/usr/bin:/bin`, `RUSTC=/home/paul/.cargo/bin/rustc`, `CARGO_BUILD_JOBS=10`, dedicated `CARGO_TARGET_DIR` under `/home/paul/.cache/vector-field-explorer-targets/`, max 2 concurrent Cargo processes.

## Review Focus

- Platform prefers-reduced-motion with the setting off — motion must still be disabled; test reduced-motion resolution with the platform signal.
- Corrupt/legacy settings missing the new field — must default to motion-on; test serde default migration of `AppStyle`.
- No idle repaint after a transition completes — test that `request_repaint` stops being called once progress reaches 1.
- Animation must not change layout geometry or clip content — wide/compact visual smoke check and a test asserting the sheet/panel rect is identical at progress 0 and 1.
- Keyboard/accessibility during a transition — test that focus and accessible labels are unchanged while animating.

---

### Task 1: Motion tokens module (pure core)

**Files:**
- Create: `src/ui/motion.rs`
- Modify: `src/ui/mod.rs` (module declaration)
- Test: `tests/motion_test.rs` (new)

**Interfaces:**
- Consumes: nothing (pure module).
- Produces: `pub struct MotionTokens { pub open_slide_ms: f32, pub hover_press_ms: f32 }` with `Default`; `pub fn progress(elapsed_ms: f32, duration_ms: f32) -> f32` (clamped ease-out `0..=1`); `pub enum MotionMode { Full, Reduced }`; `pub fn effective_mode(setting_reduced: bool, platform_reduced: bool) -> MotionMode`.

- [ ] **Step 1: Write the failing tests**

`tests/motion_test.rs`: `progress(0.0, 100.0) == 0.0`; `progress(50.0, 100.0)` strictly between 0 and 1; `progress(100.0, 100.0) == 1.0`; `progress(500.0, 100.0) == 1.0`; `progress(10.0, 0.0) == 1.0` (zero duration is immediate, no NaN); `effective_mode(false, false) == Full`; `effective_mode(true, false)`, `effective_mode(false, true)`, `effective_mode(true, true) == Reduced`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --test motion_test`
Expected: compile error — module/types not found.

- [ ] **Step 3: Implement the pure module**

Define `MotionTokens`, `progress`, `MotionMode`, `effective_mode` per the signatures. `progress` clamps elapsed/duration into `0..=1` and applies an ease-out (e.g. `1 - (1 - t)^2`); guard non-finite and zero-duration inputs.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --test motion_test`
Expected: PASS.

- [ ] **Step 5: Wire the module into the crate and register the test**

Add `pub mod motion;` to `src/ui/mod.rs` and `mod motion_test;` to `tests/suite.rs`. Run `cargo test --test suite` — PASS.

- [ ] **Step 6: Commit**

`git commit -m "feat: add central motion tokens and reduced-motion helper"`

---

### Task 2: Persisted reduced-motion setting and toggle

**Files:**
- Modify: `src/ui/style_panel.rs` (`AppStyle`, `show_controls`)
- Modify: `tests/style_panel_test.rs` (or the existing appearance settings test file)

**Interfaces:**
- Consumes: `motion::MotionMode`, `motion::effective_mode` (Task 1) for the resolved mode used at paint time.
- Produces: `AppStyle.reduced_motion: bool` (default `false`), read by Task 3/4.

- [ ] **Step 1: Write failing tests**

Test that deserializing an `AppStyle` JSON missing `reduced_motion` yields `false`; that a record with `true` round-trips; that the settings controls expose a "Reduced motion" toggle bound to the field.

- [ ] **Step 2: Run tests to verify RED**

Run: `cargo test --test style_panel_test` (or the owning test target)
Expected: FAIL — field/toggle not present.

- [ ] **Step 3: Add the field and toggle**

Add `pub reduced_motion: bool` to `AppStyle` with `#[serde(default)]`; set `false` in `Default`. Add a checkbox in `show_controls` consistent with existing appearance controls. No `sanitized()` change needed for a bool.

- [ ] **Step 4: Run tests to verify GREEN**

Run: `cargo test --test style_panel_test`
Expected: PASS.

- [ ] **Step 5: Commit**

`git commit -m "feat: persist reduced-motion preference in appearance settings"`

---

### Task 3: Global hover/press feedback through style_panel::apply

**Files:**
- Modify: `src/ui/style_panel.rs` (`apply`)
- Test: `tests/style_panel_test.rs`

**Interfaces:**
- Consumes: `MotionTokens::default().hover_press_ms`, `effective_mode` (Tasks 1–2).
- Produces: no new public API; visuals changes only.

- [ ] **Step 1: Write the failing test**

Assert that `apply` sets egui widget visuals so hovered/active states are visually distinct from inactive (e.g. distinct `bg_fill` or stroke) using the shared token, and that with reduced motion the transition is immediate (no eased intermediate state).

- [ ] **Step 2: Run tests to verify RED**

Expected: FAIL — no distinct feedback.

- [ ] **Step 3: Implement visual feedback**

In `apply`, set `visuals.widgets.hovered`/`active` colours to a restrained accent-derived highlight consistent with the existing theme and rounding. Do not alter spacing or sizes. Reduced-motion path applies the final visuals directly.

- [ ] **Step 4: Run tests to verify GREEN**

Expected: PASS.

- [ ] **Step 5: Commit**

`git commit -m "feat: add restrained button and toggle feedback"`

---

### Task 4: Open/slide transitions for the compact sheet and floating panels

**Files:**
- Modify: `src/app/mod.rs` (transition state field)
- Modify: `src/app/panels.rs` (sheet open path)
- Modify: `src/ui/viewport_panel.rs` (slide helper for floating panels)
- Test: `tests/motion_test.rs` + a UI harness test in the appropriate existing test file

**Interfaces:**
- Consumes: `MotionTokens::default().open_slide_ms`, `progress`, `effective_mode` (Tasks 1–2).
- Produces: `MotionTransition` per-panel state with `fn tick(&mut self, dt_ms, mode) -> f32` returning progress and `fn is_active(&self) -> bool`.

- [ ] **Step 1: Write the failing tests**

Test `MotionTransition`: starts at 0, reaches 1 after `open_slide_ms`, is_active only while 0<p<1, and jumps to 1 immediately under `MotionMode::Reduced`. Test that a UI frame at progress 0 and progress 1 yields the same panel/sheet rect (no layout change) and that `request_repaint` is not called once progress reaches 1. Add a UI-harness test that, during an active transition, a keyboard Tab/Enter on the sheet/panel control produces the same focus and accessible label as at rest.

- [ ] **Step 2: Run tests to verify RED**

Expected: FAIL — type/behaviour not present.

- [ ] **Step 3: Implement transition state**

Add `MotionTransition` in `src/ui/motion.rs`. Add a transition field to `App` keyed by surface. In the sheet open path and floating-panel show path, start a transition and compute a slide offset from `progress`; call `ctx.request_repaint()` only while `is_active()`. Under reduced motion set progress to 1 immediately.

- [ ] **Step 4: Run tests to verify GREEN**

Expected: PASS.

- [ ] **Step 5: Commit**

`git commit -m "feat: add short sheet and floating-panel open transitions"`

---

### Task 5: Full gates, smoke, and documentation

**Files:**
- Modify: `docs/CODEBASE.md` (brief module note, only if needed for navigation)

**Interfaces:**
- Consumes: all prior tasks.

- [ ] **Step 1: Run the full native gate set**

Run the Global Constraints gate commands. All must pass; capture logs.

- [ ] **Step 2: Run the renderer/input smoke check**

Run: `scripts/smoke-run.sh 15` — must report PASS.

- [ ] **Step 3: Self-review the diff**

Check scope, no physics/analysis changes, no idle repaint, reduced-motion correctness, formatting, and that no review-record CI rule is tripped by the commit subjects.

- [ ] **Step 4: Commit any docs-only cleanup** with a subject containing no `(ticket NN)` token unless a matching review record is added.

---

## Execution handoff

Execution method already supplied by the owner's configured lane: HY4 orchestrator dispatches an implementer child, then a fresh independent reviewer on the full diff, with serialized integration under standing release authority. This plan is to be read alongside the spec by the implementer.
