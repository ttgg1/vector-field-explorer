# Native CAS Implementation Plan

> **Historical implementation plan.** Preserve the task contracts below, but
> do not treat unchecked steps as outstanding work. Use [Workflow](../../../references/workflow.md)
> for execution policy/current-state pointers and the [plan index](../README.md)
> for evidence entrypoints; this file still resolves the native-CAS SDD workspace.

**Goal:** Add native-only symbolic differentiation and derived vector-field operations, then expose them in a transient CAS panel.

**Architecture:** A native-only adapter encapsulates exmex and returns deterministic meval-compatible strings or errors. A later native panel calls the adapter, previews/copies results, and applies vector results only on explicit user action.

**Tech Stack:** Rust, eframe/egui, `exmex` 0.21.0 with `partial`, existing `meval` 0.2.0 evaluator.

**Spec:** `docs/superpowers/specs/2026-11-10-native-cas-design.md`

## Global Constraints

- CAS dependency, module, and call sites are native-only; defer CAS wasm support.
- `meval` remains the only evaluator and existing field evaluation is unchanged.
- Unsupported symbolic functions/syntax fail clearly; no numerical fallback.
- CAS scratch input/selection/preview/errors are transient; no saved history.
  Explicitly applied formulas become ordinary fields and may use existing
  field persistence.
- 2D curl is `dFy/dx - dFx/dy`; 3D curl is a vector.
- Only an explicit Apply action may replace the plotted vector field.

## Review Focus

- Exmex variable order differs from x/y/z/t order; test each named variable and absent-variable zero.
- Raw meval/exmex precedence differs; test the input/output bridges against
  meval meaning, including `-x^2`, `x^y^z`, mixed arithmetic and nested signs.
- Unsupported/custom functions must fail closed; test unknown and evaluator-known but symbolically unsupported functions.
- 2D and 3D vector arities differ; test malformed component counts and the exact 2D curl sign.
- Failed preview/apply must preserve the current field; test parse errors and operation errors without mutation.

---

### Task 73: Native CAS adapter and math tests

**Files:**
- Modify: `Cargo.toml` (native dependency section)
- Modify: `src/lib.rs` (native-gated module declaration; serialize with Phase 1 ownership)
- Create: `src/cas.rs`
- Create: `tests/cas_test.rs`
- Modify: `tests/suite.rs` (registration; serialize with Phase 1 ownership)

**Interfaces:**
- Produces `cas::Variable::{X,Y,Z,T}`, `cas::differentiate(&str, Variable) -> Result<String, CasError>`, `cas::gradient(&str, usize) -> Result<Vec<String>, CasError>`, `cas::divergence(&[&str], usize) -> Result<String, CasError>`, and `cas::curl(&[&str], usize) -> Result<Vec<String>, CasError>`.
- `usize` dimension is exactly 2 or 3; reject any other dimension and component arity mismatch. Gradient requires a scalar; divergence/curl require exactly `dimension` components. For 2D curl return a one-element vector containing `dFy/dx-dFx/dy`; for 3D return `[dFz/dy-dFy/dz, dFx/dz-dFz/dx, dFy/dx-dFx/dy]`.
- `CasError` is a public displayable error; keep exmex parser/AST and printer types private. Derive `Clone`/`Debug`/`PartialEq` only as useful for tests.
- Input is meval 0.2.0 RPN authority: finite numeric literals, variables exactly x/y/z/t, constants pi/e, unary +/-, binary + - * / ^, parentheses, and one-argument `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `sinh`, `cosh`, `tanh`, `asinh`, `acosh`, `atanh`, `sqrt`, `exp`, `ln` only. Reject `%`; `abs`, `floor`, `ceil`, `round`, `signum`, `atan2`, `max`, `min`; custom/unknown functions, wrong arity, other identifiers/constants, non-finite literals, and exmex-only syntax/aliases/braced user identifiers. Reject unsupported syntax before constant folding or absent-variable zero handling.
- Reconstruct fully parenthesized input from the validated meval tree and lower `pi`/`e` to deterministic finite f64 literals before exmex parsing. Resolve variables by name; use strict differentiation.
- Parse exmex output with a bounded lexer/parser for finite numeric literals, exact `{x}`, `{y}`, `{z}`, `{t}` identifiers, the approved operators/functions and parentheses. Interpret using exmex output semantics, then emit fully parenthesized meval syntax. Strip braces only for validated identifier tokens. Reparse and context-bind output against meval built-ins plus x/y/z/t; reject unexpected syntax and literal nonfinite results, but do not treat evaluation at the origin as a global domain proof.

- [ ] **Step 1: Add failing integration tests** for every supported unary function and finite differences against original meval semantics; x/y/z/t, constants, absent variables; parser precedence cases `-x^2`/`(-x)^2`, `x^y^z`/`(x^y)^z`, `x/y*z`, `x*y/z`, `x-y+z`, nested signs/functions and negative bases; output nested subtraction/division; complete `curl(grad f)` and `div(curl F)` identities; every rejected operation/name/function, including inside constants and absent-variable requests; deterministic output; malformed dimensions/arities; and meval parse/context binding for results.
- [ ] **Step 2: Run `cargo test cas_test::`**; confirm the expected missing-module/API failures.
- [ ] **Step 3: Add native-only `exmex = { version = "0.21.0", features = ["partial"] }` and declare `cas` behind `#[cfg(not(target_arch = "wasm32"))]`; implement the five interfaces in `src/cas.rs`.** Validate the complete meval RPN before shortcuts, lower both semantic bridges as specified, resolve variables by name, and keep existing field evaluation untouched. Integrate `src/lib.rs` and `tests/suite.rs` serially with Phase 1 if both seats need them; math work remains parallel and there is no Phase 1 dependency.
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
- Keep CAS scratch input, selected operation, preview and error transient; no history/persistence fields. Explicitly applied formula becomes the ordinary field and follows existing persistence.
- Tag result kind as scalar or vector explicitly; 2D curl is scalar even though the adapter returns one component. Apply accepts a vector result only, after a successful preview and explicit click; scalar results expose Copy but no Apply.
- Key a successful preview to the complete input/operation/variable/dimension request. Any edit or app dimension switch invalidates/recomputes it; failed recomputation disables Apply. Recheck current dimension and vector arity at Apply.
- Validate the complete vector formula before changing app state; after validation use the existing state setter so formula-bar synchronization and dirty flags remain correct. Do not use a setter that mutates on parse failure as the validation step.

- [ ] **Step 1: Add failing tests** for operation selection/input preview, copy-ready result, scalar/vector result tagging, successful explicit vector Apply, scalar Apply unavailable, failed derivation and parse preserving formula/field/rendered samples, and stale preview invalidation on input/operation/variable/dimension changes. Assert Apply rechecks current dimension/arity and is disabled after failed recomputation.
- [ ] **Step 2: Run focused tests** and confirm failures identify missing UI integration/state behavior.
- [ ] **Step 3: Add the native panel and integrate it at an unowned app seam.** Show inline errors; use an explicit scalar/vector result tag; stale-check the full request; validate the whole field before using the existing setter. Preserve the plotted field unless explicit Apply receives a valid vector result. Keep all CAS imports/call sites native-gated.
- [ ] **Step 4: Run focused tests, `cargo build`, `scripts/test.sh`, `cargo clippy --all-targets -- -D warnings`, and `scripts/smoke-run.sh 15`; all pass.** Verify the wasm target has no CAS dependency/call path as part of existing web build verification, not as a CAS support gate.
- [ ] **Step 5: Commit** as `feat: add native CAS panel` with the ticket review process.
