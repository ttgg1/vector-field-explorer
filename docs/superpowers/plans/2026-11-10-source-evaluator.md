# Source Evaluator Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a pure, validated model and deterministic superposition evaluator for the eight Phase 1 source kinds.

**Architecture:** Add one egui-free `src/sources.rs` module exposing the proposed source model and evaluator. Keep calculations in f64 with a final f32/glam conversion; bounded geometries use deterministic adaptive quadrature with an explicit convergence-or-error budget, and all source terms use the single caller-supplied softening length. The exact tolerances, budget, and supported numerical envelope are unapproved proposals pending evidence and owner decision.

**Tech Stack:** Rust 2024, `glam::Vec3`, existing `serde` availability only if the approved model needs it; integration tests in the existing single `tests/suite.rs` harness. No new dependency is proposed.

**Spec:** `docs/superpowers/specs/2026-11-10-source-evaluator-design.md` (PROPOSED; execution awaits explicit design approval and a ticket/brief).

## Global Constraints

- Teaching-grade, not a quantitative solver; no dimensional analysis.
- Formula mode and source mode are exclusive; formula presets remain formulas.
- Sources superpose only within one field family; do not numerically add E and B.
- Use one global softening parameter; no per-source softening.
- The model/evaluator is pure and GPU-independent; GPU paint stays behind `SceneGPU`.
- SI-like labels are display-only; no unit conversion in the evaluator.
- 2D remains a later z slice of this 3D source model via `PlaneView`; this plan does not wire slicing.
- Tests live in `tests/`, with `tests/suite.rs` registration; do not add `#[cfg(test)]` under `src/` or widen APIs only for tests.
- Keep dependencies GPL-3.0-compatible and wasm-clean; no dependency is needed by this proposal.

## Review Focus

- Mixed-family source lists and invalid source parameters must fail explicitly, never silently omit a source; test each family boundary and malformed numeric input.
- Singular query points and extreme-but-valid inputs must produce finite output or an explicit error; test point centers, default-domain grids, and f32 conversion overflow.
- Orientation/sign mistakes can look plausible; test rotated geometry, negative charge/current, and right-hand-rule direction.
- Quadrature can be normalized incorrectly while retaining symmetry; test measures and total-strength normalization against independently converged references. Adaptive indicators can also miss narrow under-resolved features; passing one is not a guarantee of accuracy.
- The charged ring has a far-field electric monopole, not dipole; the current loop has a magnetic dipole. Test those distinct asymptotics.
- The solid sphere uses full solid-angle measure (or a derived symmetry reduction), not radial × azimuthal planar rings. Verify the measure and normalization independently.
- Summation order can affect cancellation; test opposite-charge cancellation and order independence within the documented tolerance.

---

## File structure

- Create `src/sources.rs`: source types, validation, kernels, bounded integration, and superposition.
- Modify `src/lib.rs`: register `pub mod sources` (and re-export only if owner-approved convention requires it).
- Create `tests/sources_test.rs`: public-seam integration tests for model, evaluator, geometry and robustness.
- Modify `tests/suite.rs`: register `mod sources_test`.

This is one reviewer-sized deliverable: all eight kinds share the same model,
validation, softening, family invariant and evaluator seam. Splitting their math
would duplicate kernels/conventions and leave partial source behavior that no
caller can use. The test file can be grouped by kind to keep the diff legible.

## Interfaces

- **Consumes:** `glam::Vec3`; no app state, UI, GPU, persistence, or `FieldExpr` dependency.
- **Produces (proposed; reconcile only through approved spec):** `FieldFamily`, `Source`, `SourceKind`, `SourceSet`, `Softening`, `SourceError` including an explicit quadrature nonconvergence/work-budget failure, and
  `evaluate_sources(&SourceSet, Vec3, Softening) -> Result<Vec3, SourceError>`.
- Exact variant fields, bounds, kernels, and tolerance policy are in the linked spec. The adaptive error estimate is an indicator, not a rigorous bound; its tolerance, budget, and accepted validity envelope must be justified with numerical/performance evidence and owner approval. If the owner revises them, update this plan before implementation.
- Source-count limit, total query budget across sources, cancellation/error-accumulation policy, and bounded cell-storage strategy are UNSET builder-evidence prerequisites. They are not completed implementation decisions; per-source work limits alone do not bound an unrestricted source list. No proposed bounds are approved by this plan.

### Task 1: Pure source model, validation, and evaluator

**Files:**
- Create: `src/sources.rs`
- Modify: `src/lib.rs`
- Create: `tests/sources_test.rs`
- Modify: `tests/suite.rs`

**Interfaces:**
- Consumes: `glam::Vec3` and standard Rust math only.
- Produces: the `sources` module interface above; the model validates finite bounded values, family coherence, normalized usable axes, and positive global softening before calculating.

- [ ] **Step 1: Write failing integration tests for the source model and point-charge seam.** Cover all eight variant constructors at valid limits, family classification/mixed-family rejection, invalid numeric values and axes, empty/zero-strength sets, and point-charge softened on/off-axis values and sign reversal.
- [ ] **Step 2: Run `cargo test --test suite sources_test::` and verify RED for missing source model/evaluator symbols or expected validation behavior.** Confirm failures are caused by the absent behavior, not a test-registration or compile typo.
- [ ] **Step 3: Implement public types, parameter validation, point-charge and infinite-wire kernels, common finite-result checking, and evaluator dispatch in `src/sources.rs`.** Use the proposed f64 working representation and convert to `Vec3` once.
- [ ] **Step 4: Add failing geometry tests for finite line, charged ring, disc, solid sphere, current loop and finite solenoid.** Pin orientation, sign, symmetry, and total-strength normalization against independently converged references; test ring monopole versus loop dipole asymptotics, sphere solid-angle measure, convergence success, and explicit budget failure.
- [ ] **Step 5: Implement the proposed bounded-integration approach only after approval and evidence.** The present proposal compares tensor GL8/GL16 vector estimates per cell, aggregates component errors, and refines deterministically. These indicators may miss features both rules under-resolve; passing the estimate does not guarantee accuracy. The public error contract must represent detected nonconvergence/budget exhaustion, without implying a chosen budget, convergence threshold, or supported envelope is approved. Source-count limits, total work accounting, cancellation/error accumulation, and bounded cell storage require builder evidence before this step is implementation-ready. Use the sphere's derived rotational-symmetry reduction or full 3D measure, never a planar-ring substitute.
- [ ] **Step 6: Add acceptance and robustness tests.** Test the softened opposite-point-charge dipole at the fixed 5×5×5 `[-1,1]^3` grid (`a=0.1`), convergence-or-explicit-error over evidence-backed cases (not blanket finiteness over unverified extremes), summation-order tolerance, and f32 overflow rejection.
- [ ] **Step 7: Run `cargo test --test suite sources_test::`, then `scripts/test.sh`, `cargo build`, and `cargo clippy --all-targets -- -D warnings`.** All must pass; this pure module does not require runtime smoke.
- [ ] **Step 8: Self-review the diff against the approved spec, especially units/signs, family validation, quadrature weights, non-finite behavior, and no hidden UI/persistence scope. Commit as `feat: add pure source field evaluator` (without ticket marker until the controller supplies an approved ticket).**

## Handoff constraint

This plan is pending owner approval of the proposed math/model, followed by
ticket confirmation and a concrete task brief. The implementation agent must
not treat this document's proposed choices as accepted requirements until that
approval is recorded.
