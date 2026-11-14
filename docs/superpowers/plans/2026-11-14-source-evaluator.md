# All-eight Source Evaluator Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Follow the controller's assigned method; implementers spawn no subagents.

**PROPOSED IMPLEMENTATION DESIGN — pending owner review, not implementation authorization.**

**Goal:** Deliver #24's pure all-eight softened source evaluator with adjustable requested tolerance, independent E/B availability and bounded cancellable computation.

**Architecture:** Validated immutable f64 source data feeds elementary kernels and stable one-dimensional reductions. Geometry-aware adaptive integration and absolute uncertainty accounting refine complete family estimates; separate equal E/B reservations prevent evaluation-order starvation. No app/render/UI/persistence changes.

**Tech Stack:** Rust 2024, existing glam DVec3, standard library; no new dependency. Tests use the existing suite, committed fixtures and analytic references, no numerical Python runtime requirement.

**Spec:** [2026-11-14 source evaluator design](../specs/2026-11-14-source-evaluator-design.md). Read both; that document defines exact formulas, types, uncertainty semantics and provisional defaults.

## Global Constraints

- Mixed sources, separate E/B; one global softening, no private timer. Formula/source modes remain exclusive downstream.
- Epsilon is requested numerical tolerance, not a certified bound and not physical softening. Accuracy-first, optimize later; do not reopen an exhaustive research prerequisite.
- TargetMet means estimated target met; AccuracyNotMet requires complete resolved finite coverage; unavailable has no vector. No skipped-source or partial-integral fallback.
- All eight geometries remain one capability. No Sources UI/persistence/glyph/render/export implementation or Cargo/dependency changes.
- Preserve native/wasm-compatible pure CPU code, GPL-3.0-only compatibility. Tests live in tests/ and register in tests/suite.rs.
- Spec's provisional defaults: epsilon 1e-7, separate E/B scales 1e-2, a=0.1; source64, work2,000,000/family/point, cells8,192/family, depth48, queries65,536/job. Caller may lower hard ceilings; numerical defaults are not owner-approved constants.
- Each task follows RED → minimal implementation → GREEN → self-review → commit with `Generated-by: OpenCode`. Independent initial review before dependent delivery; the controller owns scheduling/integration.

## Review Focus

1. Finite extremes can lose representable answers through overflowing intermediates: preserve point components at 1e103 and 1e154; explicit refusal for unrepresentable transformations (Tasks 1–2).
2. Near-axis/near-center fields must remain stable across branch boundaries, not be clamped to zero (Tasks 3–4).
3. Jointly missed narrow features must not get TargetMet from paired-rule agreement (Task 2 and each geometry's near-support checks).
4. Strong cancellation demands a family-total target, not accumulated local relative tolerances (Task 5).
5. A difficult B must not starve a cheap E; cancellation, preallocation and limits cannot leak partial estimates (Tasks 1, 2, 5).

## File responsibilities and sequencing

Create `src/sources/{mod,model,numeric,quadrature,electric,magnetic,evaluate}.rs`.
The facade reexports only spec public types; model validates, numeric handles
scaled arithmetic/rounding, quadrature owns reusable bounded partitions, electric
and magnetic own reductions, evaluate owns family refinement/status/provenance.
Modify `src/lib.rs` only for module registration. Create
`tests/{sources_model,sources_numeric,sources_electric,sources_magnetic,sources_evaluate}_test.rs`
and register each in `tests/suite.rs` as introduced. Shared reference helpers and
fixtures live in `tests/support/sources/` (register from `tests/support/mod.rs`).
Update `docs/CODEBASE.md` alongside introduced production files, and link fixture
provenance there. Source changes/tests below are **future work**, not files added
by this docs-only proposal. Tasks are sequential internal review units, not new
issues and not independently complete versions of #24.

### Task 1: Validated model, preparation and finite arithmetic boundary

**Files:** Create facade, model, numeric and evaluate modules; modify lib/suite;
create model/numeric test modules; update CODEBASE. Later geometry modules are
introduced by their owning tasks, not placeholder implementations.

**Interfaces:** Produce spec `SourceKind`, `Source`, `SourceSet::new`,
`AccuracyRequest`, `EvalOptions`, `WorkLimits`, result/diagnostic/error types,
`Evaluator::new` and `Evaluator::evaluate`. Consumes existing DVec3 only.
Private numeric helpers return a value plus absolute arithmetic allowance or
NumericRange. Initially dispatch elementary point/wire and empty families only;
nonzero not-yet-implemented kinds explicitly unavailable (remove before Task 5).

- [ ] Write RED `validation_is_strict`: assert all eight valid variants accepted in one set; negative and zero strengths accepted; NaN/Inf in every scalar/vector field rejected; a/radius/length/epsilon/scales <=0 rejected; zero used axis and turns0 rejected. For point/sphere finite zero axis is valid. Assert max_sources64 accepts 64, rejects65 before copy; zero/above-ceiling work limits reject.
- [ ] Write RED `prepare_is_cancellable_before_allocation` and `empty_and_zero_are_exact`: cancellation before prep yields PrepareError::Cancelled; empty/valid zero-strength lists give exactly E=B=ZERO with finite uncertainty. A cancelled point evaluation returns no new vectors. Exercise huge and subnormal axis directions through sign/right-hand-rule checks, not a squared-norm cutoff.
- [ ] Run `cargo test --test suite sources_model_test::` and observe intended RED (missing seam or wrong assertions), then implement validation, fallible checked preparation and public provenance. Preallocate fixed bounded family scratch; check cancellation before reservations/copying.
- [ ] Write RED `scaled_elementary_fields`: point `(1,0,0),Q=1,a=0.1` equals `(1/1.01^(3/2),0,0)` within 2e-15; at center exactly zero. Wire `(1,0,0),axis=Z,I=1,a=0.1` equals `(0,2/1.01,0)` within 2e-15. Point `(1e103,1e103,0)` components match `3.5355339059327376e-207` within 2e-14 relative; at 1e154 match `3.5355339059327374e-309` within 16 smallest subnormal steps. Huge genuine underflow flags, never confuses the 1e308 case with representable-loss failure. Unrepresentable point-minus-center yields NumericRange.
- [ ] Run numeric filter to intended RED; implement power-of-two/mantissa-exponent arithmetic, stable norm/cross/frame operations and uncertainty, then rerun both filters to GREEN. Assert sign reversal, translated point and non-axis-aligned wire against directly computed vectors. Self-review and commit `feat: add validated source model and scaled elementary fields`.

### Task 2: Bounded geometry-aware integration and uncertainty accounting

**Files:** Create quadrature module; extend numeric/evaluate, numeric tests and
support reference helpers; register support module and document CODEBASE.

**Interfaces:** Private `integrate(domain, geometry_guard, kernel, scratch,
budget, cancelled)` produces a complete partition estimate, sum of absolute
uncertainties and refinable cells, or a reason with no publishable partition.
`Budget` counts every sample/series/analytic work unit; scratch is created in
Task 1. Produce largest-absolute-uncertainty refinement with deterministic ties.
Exercise integration through finite-line evaluation rather than exposing a test
solver API. Finite line uses direct segment integration in production; the
independent stable endpoint primitive is the test oracle. An analytic fast path
can wait for later optimization rather than bypass this integrator's regressions.

- [ ] Write RED `line_against_closed_form`: L2,Q1,a0.1 at radial1,z0 has Er `1/(1.01*sqrt(2.01))`, Ez0 within 2e-13. Axial off-center/end queries use the spec primitive independently in test arithmetic; sign/length/scale normalization match. Compare far same-sign endpoint cases with committed high-precision endpoint references; require integrated-path work diagnostics > elementary cost.
- [ ] Write RED `narrow_displaced_line`: L2, a=1e-6, Q=2a, radial=a at z=0,.123,.431271 has analytic transverse answer from endpoints; error <=1e-9 + 1e-7*norm(reference), TargetMet only with resolution guard passed. Lower cell/work limits so feature cannot be resolved: assert Unavailable(UnresolvedFeature or WorkLimit), not finite inaccurate partial coverage. Retain the unsplit-GL miss (`4.639761e-10` versus ~1) in fixture provenance, not as the new solver's expected value.
- [ ] Run numeric filter to RED; implement geometry splits, GK15/31 plus mandatory half comparison, factor8 indicator, rounding terms and global absolute partition sums. No local relative acceptance. Retain a full resolved coarse partition before refinements; check child capacity/work before replacing a parent. No recursive allocations.
- [ ] Write RED budget/cancel assertions through line evaluation: work_used<=configured limit including parent/half probes, cells_high_water<=limit, cancel polling gap<=32 counted units, repeated-midpoint/depth refusal, failed scratch preparation before evaluation. Require no publishable estimate when initialization is incomplete. Use low public limits and deterministic cancellation closure counters, no allocator instrumentation dependency.
- [ ] Run numeric/model filters to GREEN, inspect line's independent analytic-reference agreement and integration diagnostics, self-review uncertainty/coverage accounting and commit `feat: add bounded feature-aware source quadrature`.

### Task 3: Stable extended electric geometries and independent fixtures

**Files:** Create electric module; extend numeric/evaluate; create electric tests;
add `tests/support/sources/{fixtures.csv,README.md,generate.py}` and support loader;
register suite, update CODEBASE. Generator is optional offline provenance, not a
test/runtime dependency or new benchmark spike.

**Interfaces:** Private per-source electric estimate/partition with Task 2's
uncertainty/refinement representation. Implement ChargedRing, ChargedDisc and
SolidSphere with the spec reductions and finite line from Task 2. Produce
independently resolved axial/off-axis/support fixtures for every electric kind.

- [ ] Write RED analytic assertions at a0.1,Q1,R1: ring axis z0.7 is `0.7/(1.5)^(3/2)`; disc axis uses rationalized difference in spec; sphere center zero and E/r near center agrees with `1/(1.01)^(3/2)` to 1e-12 relative at r1e-8. Check total Q scaling, radius scaling with all lengths/a scaled, negative Q, and rotated/translated vectors (orthogonal rotation preserves vector norm within 2e-12 relative).
- [ ] Commit a small fixture matrix spanning r0,1e-10,1e-8,0.4,1 and displaced near-support; z0,0.7 and ends; a0.1,0.001 plus selected1e-6, nonunit sizes and signed strengths. Point/line use analytic/direct elements; ring uses independently evaluated elliptic formulas at high precision; disc uses polar ring integration, sphere volume/slab integration. Resolve references with two precision/refinement settings, record exact input bits, true pi, changes and uncertainties <=1e-12 + 1e-10*norm(reference). Keep generators independent of production code. If an oracle fails, fix/omit that fixture with a recorded reason, never label it resolved or remove required geometry coverage.
- [ ] Add RED off-axis fixture assertions: error<=1e-9 + 1e-7*norm(reference) at defaults, and TargetMet; force tiny budget to distinguish complete AccuracyNotMet from incomplete Unavailable. Pin ring regression r1e-10,z0.7,a0.1: Er near `-2.2156608357924563e-27`, Ez `.38103173776627215`; absolute radial error<=1e-22. Sweep δ on both sides of1/8 at factors .99,1,1.01 with independent references; vector errors fit target, no branch discontinuity exceeding reference change plus two targets.
- [ ] Run electric filter to intended RED; implement stable series/tail and angular ring path, chord disc, rationalized surface sphere. Include finite near/interior/on-support results at ordinary softening, sphere volume normalization (not planar rings), near-center slopes, and ring monopole far-field magnitude/direction at distance100/200 with decreasing ideal asymptotic discrepancy.
- [ ] Run electric/model/numeric filters to GREEN; inspect estimated U separately from external reference error (no certified-bound assertion), record work/high-water/native timings as observability. Self-review and commit `feat: evaluate all softened electric source geometries`.

### Task 4: Magnetic loop and finite continuous solenoid

**Files:** Create magnetic module; extend evaluate/numeric and fixture provenance;
create magnetic tests and suite registration; update CODEBASE.

**Interfaces:** Magnetic estimates/refinable partitions use exactly Task 2's
absolute uncertainty and budget contract. Produce CurrentLoop and Solenoid;
preserve the wire's azimuthal world reconstruction from Task 1. Reuse ring
series moments, not electric normalization.

- [ ] Write RED analytic assertions at R1,I1,a0.1: loop axial z0.7 has Bz `2π/(1.5)^(3/2)`; solenoid N1,L2,z0 has Bz `2π/(1.01*sqrt(2.01))`. N3 doubles/triples once only; compare changed L0.02/2/20 to the full analytic axial expression. Negative I/axis reversal reverses B; translated tilted axes match independent vectors; wire keeps factor2.
- [ ] Extend committed independent fixtures: direct current elements for loop (plus elliptic high-precision off-axis references), loop-over-z integration for solenoid, direct infinite-wire integral. Cover off-axis, near-axis, winding crossings/end effects, a1e-6 displaced feature, nonunit R/L, N and signed I. Two reference convergence settings and uncertainty criterion from Task 3 apply. Loop r1e-10,z0.7,a0.1 must have Br `2.3940930163021465e-10`, Bz `3.420132880431638`, radial error<=1e-20. Include ordinary z0.2 to avoid relying on the ring's nearly vanishing leading radial term.
- [ ] Run magnetic filter to RED, implement stable near-axis/direct loop and axially integrated solenoid with rationalized endpoint differences and geometry guard. Test δ switch overlap as Task 3; interior/on-wire/on-end queries at a0.1 return complete finite target-met results, not blanket exclusions.
- [ ] Assert loop magnetic dipole far-field direction/normalization using moment `πIR² axis` at axial/equatorial distances100/200, decreasing asymptotic discrepancy; distinguish charged-ring monopole. Assert finite-current B scales I/s (fixed N) when all lengths/a scale by s, unlike E's Q/s². Force unresolved-feature/work and roundoff-limited outcomes without returning partial integrals.
- [ ] Run magnetic/electric/numeric filters to GREEN; record actual work/storage/timing, self-review sign/measure/turn-count and commit `feat: evaluate current loops and finite continuous solenoids`.

### Task 5: Family-global accuracy, independent availability and delivery gate

**Files:** Complete evaluate/facade/model diagnostics, create evaluate tests;
extend shared fixtures/provenance and suite/CODEBASE. No application consumers.

**Interfaces:** Finish spec public Sample/FamilyResult/Estimate/Diagnostics
contract. Consumes all eight private kernels and refinable partitions; produces
separate family results using `T=epsilon*S + epsilon*norm(V)` and compensated
sums/global U. Remove all temporary unsupported-kind branches. Downstream
receives enough immutable provenance for stale generation/warnings, not UI code.

- [ ] Write RED `family_target_uses_cancelled_total`: construct nearly cancelling signed sources with known independent residual; assert target equals epsilon*S + epsilon*norm(value), not epsilon*sum(abs contributions). Reverse all source permutations of a three-source large/small/opposite scene; compare to independent high-precision physical totals, not only fsum of rounded source outputs. Zero residual still has positive absolute target; independently change e_scale/b_scale and check only that family's target changes. Assert finite U and no source omitted.
- [ ] Write RED `both_family_orders_have_reserved_work`: mixed cheap E/difficult solenoid B with deliberately small equal caps; permute source order and compare statuses/work within each family's reservation. E must remain usable when B fails, and conversely cheap B when difficult E fails. Sum work<=2*work_per_family, no borrowing; all-eight mixed totals match independent world-space fixtures. Query max boundary accepts exactly configured calls then Unavailable(QueryLimit) for both; invalid queries count too.
- [ ] Write RED `finite_inaccurate_is_complete_only`: epsilon1e-30 with sufficient initialization but limited refinement gives AccuracyNotMet with finite complete all-source estimate/U, missed target and reason. Incomplete preparation/unresolved feature/NaN/Inf/cancelled family never has an Estimate. Cancelling after one family completes preserves that family, never partially publishes the other. Diagnostic kernel/a/epsilon/scales and work provenance match options. API has no draw-inaccurate flag that raises limits.
- [ ] Run evaluate filter to RED; implement global family refinement and uncertainty sum, separate budgets, hard query lifetime, final finite checks/provenance and exact empty-family zero. Validate underflow and NumericRange classifications separately from target failure; leave f32 conversion, warning drawing, export and generation-safe publication to #25/#26.
- [ ] Add convergence tests over selected disc/solenoid/cancellation fixtures at epsilon1e-4,1e-7,1e-10 with scales fixed. Against better-resolved references assert default target, tight-case measured error<=loose-case error plus reference/f64 noise floor, and tight-case error<=1e-12 + 1e-10*norm(reference) when TargetMet. Require at least one genuinely improving case; diagnose failures rather than assert universal monotonicity. Report work/time and limited statuses honestly; no throughput/admission threshold. Add softened point/dipole fixed5×5×5 grid including centers; each matches analytic sums within 2e-14 absolute + 2e-13 relative. Require all-eight reference coverage before declaring complete.
- [ ] Run all source filters to GREEN, then full current workflow gates: `cargo build`, `scripts/test.sh`, `cargo clippy --all-targets -- -D warnings`, `python3 scripts/check-docs.py`, changed-file rustfmt and `git diff --check`. Use explicit rustup PATH/RUSTC/Cargo and own persistent target/TMPDIR under `/home/paul/.cache/vector-field-explorer-targets/`, ten jobs, max two concurrent Cargo processes. This pure CPU seam needs no GPU smoke unless scope changes. Dedicated wasm/browser/manual visual checks remain waived, **not passed**.
- [ ] Self-review all eight distribution measures, scale/series branches, global uncertainty versus independently measured error, symmetric caps and preallocation. Update user-facing numerical notes with tested cases/known limitations and timings, not an unlimited envelope guarantee. Commit `feat: complete family-local source accuracy and diagnostics`; report hashes/RED-GREEN/gates/waivers and request the controller's independent numerical/quality review. Do not push/merge or authorize downstream implementation from this plan alone.

## Handoff

Owner review of this design/plan and independent numerical/design review are the
next boundary; no further broad experiment is prerequisite. Changes to provisional
numerics during implementation require rationale/tests/review, not invented owner
approval. Return only actual product-scope/semantics trade-offs to the owner.
Finish/merge #14 and honor the owner's break before physics implementation;
this concurrent docs-only exception does not change that scheduling boundary.
