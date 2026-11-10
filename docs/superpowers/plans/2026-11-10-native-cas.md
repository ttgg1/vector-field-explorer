# Native CAS Implementation Plan

> **For agentic workers:** use `superpowers:subagent-driven-development` or `superpowers:executing-plans` to implement task-by-task. Do not start until the independent builder review resolves the gates in the spec and tickets.

**Goal:** Add native-only symbolic differentiation and derived vector-field operations, then expose them in a transient CAS panel.

**Architecture:** A native-only adapter encapsulates exmex and returns deterministic meval-compatible strings or errors. A later native panel calls the adapter, previews/copies results, and applies vector results only on explicit user action.

**Tech Stack:** Rust, eframe/egui, `exmex` 0.21.0 with `partial`, existing `meval` 0.2.0 evaluator.

**Spec:** `docs/superpowers/specs/2026-11-10-native-cas-design.md`

## Global Constraints

- CAS dependency, module, and call sites are native-only; defer CAS wasm support.
- `meval` remains the only evaluator and existing field evaluation is unchanged.
- Unsupported symbolic functions/syntax fail clearly; no numerical fallback.
- Results are transient; no saved history or persistence.
- 2D curl is `dFy/dx - dFx/dy`; 3D curl is a vector.
- Only an explicit Apply action may replace the plotted vector field.

## Review Focus

- Exmex variable order differs from x/y/z/t order; test each named variable and absent-variable zero.
- Printed output may not be valid meval syntax or preserve precedence; test complete meval parse/eval round trips across the approved grammar.
- Unsupported/custom functions must fail closed; test unknown and evaluator-known but symbolically unsupported functions.
- 2D and 3D vector arities differ; test malformed component counts and the exact 2D curl sign.
- Failed preview/apply must preserve the current field; test parse errors and operation errors without mutation.

---

### Task 73: Native CAS adapter and math tests

**Files:**
- Modify: `Cargo.toml` (native dependency section)
- Modify: `src/lib.rs` (native-gated module declaration)
- Create: `src/cas.rs`
- Create: `tests/cas_test.rs`
- Modify: `tests/suite.rs`

**Interfaces:**
- Produces `cas::Variable::{X,Y,Z,T}`, `cas::differentiate(&str, Variable) -> Result<String, CasError>`, `cas::gradient(&str, usize) -> Result<Vec<String>, CasError>`, `cas::divergence(&[&str], usize) -> Result<String, CasError>`, and `cas::curl(&[&str], usize) -> Result<Vec<String>, CasError>`.
- `usize` dimension is exactly 2 or 3; reject any other dimension and component arity mismatch. Gradient requires a scalar; divergence/curl require exactly `dimension` components. For 2D curl return a one-element vector containing `dFy/dx-dFx/dy`; for 3D return `[dFz/dy-dFy/dz, dFx/dz-dFz/dx, dFy/dx-dFx/dy]`.
- `CasError` is a public displayable error; keep exmex parser/AST and printer types private. Derive `Clone`/`Debug`/`PartialEq` only as useful for tests.
- Exact symbolic function subset and printer normalization must be settled by the independent builder review before implementation; catalog and evidence are in the spec. Functions outside that approved subset return an error.

- [ ] **Step 1: Add failing integration tests** for explicit x/y/z/t differentiation, absent-variable zero, gradient component order, divergence, 2D curl sign, 3D curl order, invalid dimensions/arities, unsupported function errors, and reparsing/evaluating every result through `meval::Expr`.
- [ ] **Step 2: Run `cargo test cas_test::`**; confirm the expected missing-module/API failures.
- [ ] **Step 3: Add native-only `exmex = { version = "0.21.0", features = ["partial"] }` and declare `cas` behind `#[cfg(not(target_arch = "wasm32"))]`; implement the five interfaces in `src/cas.rs`.** Resolve variable indices by name, validate emitted syntax with meval, and keep existing field evaluation untouched.
- [ ] **Step 4: Run `cargo test cas_test::`**, then `cargo build`, `scripts/test.sh`, and `cargo clippy --all-targets -- -D warnings`; all pass. A CAS wasm build/size gate is intentionally deferred.
- [ ] **Step 5: Commit** as `feat: add native symbolic transform adapter` (use ticket marker only with its review record process).

### Task 75: Native CAS panel and explicit apply flow

**Depends on:** Task 73 and ticket 71 merged. Do not edit ticket 71-owned files before its merge.

**Files:**
- Create: `src/ui/cas_panel.rs`
- Modify: `src/app/` integration module selected after ticket 71 merge (avoid `src/app/panels.rs` unless its ownership is free)
- Modify: owning UI module declaration and `tests/suite.rs` only as needed
- Create/modify: focused integration tests under `tests/`

**Interfaces:**
- Consume Task 73's `cas::{differentiate, gradient, divergence, curl}`.
- Keep input, selected operation, preview and error transient; no persistence/history fields.
- Apply accepts a vector result only, after a successful preview and explicit click; scalar results expose Copy but no Apply.

- [ ] **Step 1: Add failing tests** for operation selection/input preview, copy-ready result, successful explicit vector Apply, scalar Apply unavailable, failed derivation preserving field, and no apply on input/operation change.
- [ ] **Step 2: Run focused tests** and confirm failures identify missing UI integration/state behavior.
- [ ] **Step 3: Add the native panel and integrate it at an unowned app seam.** Show inline errors; preserve the plotted field unless the explicit Apply action receives a valid vector result. Keep all CAS imports/call sites native-gated.
- [ ] **Step 4: Run focused tests, `cargo build`, `scripts/test.sh`, `cargo clippy --all-targets -- -D warnings`, and `scripts/smoke-run.sh 15`; all pass.** Verify the wasm target has no CAS dependency/call path as part of existing web build verification, not as a CAS support gate.
- [ ] **Step 5: Commit** as `feat: add native CAS panel` with the ticket review process.
