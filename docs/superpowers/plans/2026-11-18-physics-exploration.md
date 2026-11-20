# Physics Exploration Implementation Plan

> **DRAFT — NOT EXECUTABLE.** Written-design approval, plan approval and fresh
> owner authorization are required before implementation or dispatch.
> Future agentic workers use subagent-driven-development or executing-plans only
> after approval; roadmap design gates below are not numerical implementation tasks.

**Goal:** Connect editable teaching examples to scalar and Maxwell exploration.

**Architecture:** Extend merged Sources, Maxwell, Induction, Materials, Ode and
scalar animation rather than rebuilding them. Share validated parameter snapshots
between evaluation, diagnostics and capture; add bounded capabilities incrementally.

**Tech Stack:** Rust, eframe/egui, meval, serde/RON, existing native/wasm renderers.

**Spec:** [Draft customization/physics design](../specs/2026-11-18-customization-physics-design.md).

## Global Constraints

- Planning BASE: `4ed3610d1a642e7b8abaf68d7f73f5e614cb827f`.
- [#81](https://github.com/ttgg1/vector-field-explorer/issues/81) and child issue
  numbers below refer to GitHub's authoritative requirements/dependencies.
- This drafting assignment owns only this Markdown file, not prospective paths.
- Preserve native+web, GPL-compatible dependencies and one SimClock; CAS native-only.
- Keep rho (density), V (potential), E and B distinct; never add E and B.
- Normalized values are not SI. New serialization units/conventions need approval.
- Curves/height surfaces are not volumetric scalar fields; volumes/isosurfaces excluded.
- Static, quasistatic and full-wave analytic illustrations are separate models;
  a general arbitrary-source electromagnetic solver is excluded.
- Softening changes a model; requested epsilon is not certified error. No new
  algorithm, tolerance, cap or query envelope is approved by this draft.
- #18 performance evidence remains deferred, not passed and not a new blocker.
- Serialize shared viewmodel, persistence, parser, UI and suite-registration paths.

## Review Focus

- Parameter names/nonfinite imports: reject atomically without stale dependent data (#89).
- Inactive scenes/storage failure: preserve durable data and expose errors (#90).
- Poles/interfaces: missing scalar samples stay missing, not interpolated facts (#92).
- Law diagnostics: unsupported terms are not zero or proofs of validity (#94/#95).
- Animation/source inconsistency: shared time, continuity and model limits (#97/#98).

## Existing seams and stable issue contracts

CodeGraph inspected BASE in the indexed main checkout, not this unindexed worktree.
`ScalarExpr::parse(&str, ScalarContext) -> Result<Self, ScalarError>` and
`eval_at(&self, f64, f64, f64) -> Option<f64>` already support time.
`sample_surface_at(&ScalarExpr, &Domain, UVec2, f32) -> SurfaceMesh` and
`sample_curve_at(&ScalarExpr, &Domain, usize, f32) -> Vec<Option<Vec3>>`
already sample existing representations; extend rather than replace them.
`SourceRecord::from_source(&Source) -> Self`, `SourceScene`, `SourceSet`,
existing evaluators and `MaxwellPanelState::commit(&mut self, &mut State) -> bool`
provide validation/commit seams. BEM and existing Maxwell catalogue are merged.

The [visualization plan](2026-11-18-visualization-customization.md) supplies #83
effective styles for #93 and distinguishes #85 appearance records from #90 scenes.
#89 may integrate #86 accepted-edit transactions later without waiting to evaluate.
#97 depends on #89/#94/#95, not #96; #98 depends on #96/#97.
No catalogue-wide wait: #91 existing-backend entries follow #89/#90; advanced
entries wait only for their actual #92/#93/#95/#97 backend.

## Task 1 — #89 named parameters, bounded foundation

**Dependencies:** existing parsers; approved parameter/serialization contract.
**Prospective paths:** create `src/parameters.rs`, `tests/parameters_test.rs`;
modify `src/scalar.rs`, `src/field.rs`, `src/viewmodel.rs`, `src/persistence.rs`,
`src/lib.rs`, `tests/suite.rs`, `tests/scalar_test.rs`, `tests/field_test.rs`.
**Proposed interface:** `ParameterSet::validate(&self) -> Result<(), String>`;
approve binding/snapshot signatures before parser integration, preserving old APIs.

- [ ] Design valid names/reserved coordinates/time/functions, bounds/step rules,
  finite values, record versioning and coherent commit/capture snapshot boundaries.
- [ ] RED: `parameters_test::reserved_names_rejected` checks coordinate/time/parser
  collisions; `invalid_commit_is_atomic` checks nonfinite and invalid bounds retain
  old definitions/values and unchanged dependent scene generation.
- [ ] Run `cargo test parameters_test::`; inspect expected missing behavior.
- [ ] GREEN: implement validation and immutable records only; prove old settings
  load unchanged. Then separately approve exact shared parser binding signatures.
- [ ] RED/GREEN parser slice: parameterized scalar/vector formulas agree at fixed
  coordinates/time, edits invalidate all dependent caches once, detached snapshots
  retain captured values. Run `cargo test scalar_test::` and `cargo test field_test::`.
- [ ] Review each bounded slice; teaching-case/UI consumers require exact-path
  briefs before modifying files outside this listed foundation.

## Task 2 — #90 organized library and My presets

**Dependencies:** #89; separate camera/#85 appearance/complete-scene contracts.
**Prospective paths:** create `src/scene_library.rs`, `tests/scene_library_test.rs`;
modify `src/presets.rs`, `src/ui/presets.rs`, `src/persistence.rs`,
`src/lib.rs`, `tests/suite.rs`, `tests/ui_preset_test.rs`.
**Proposed interface:** `SceneLibrary::validate(&self) -> Result<(), String>`;
scene apply/import signatures and exact record scope need approval first.

- [ ] Design stable IDs, collapsible remembered teaching groups, search,
  2D/3D/animated filters, My presets, save-as/edit and atomic versioned import/export.
- [ ] RED: `scene_library_test::invalid_import_preserves_library` asserts unknown
  versions/invalid values preserve all entries; `scene_apply_preserves_inactive_data`
  pins explicitly retained inactive modes and unrelated preferences.
- [ ] RED: `ui_preset_test::search_and_filters_keep_stable_selection` checks
  grouping/filtering and remembered expansion; storage errors are actionable.
- [ ] Run named filters, inspect RED, implement minimal model/UI behavior, rerun
  GREEN; browser storage failures leave usable in-memory entries and visible errors.
- [ ] Review native/browser storage adapter ownership before extending that slice.

## Task 3 — #91 incremental teaching catalogue

**Dependencies:** #89/#90; each advanced entry only its enabling backend.
**Prospective paths:** modify `src/presets.rs`, `tests/presets_test.rs`,
`tests/source_presets_test.rs`, `tests/plot_examples_test.rs`.
**Interface:** consume approved #90 stable IDs/metadata; use existing backends.

- [ ] Inventory shipped entries first; reuse them. For each candidate record backend,
  assumptions, parameters, domain/camera, exploration prompt and actual playback/
  screenshot/animation/export eligibility, including unsupported reasons.
- [ ] Early source examples: quadrupole, unequal pair, off-axis ring, sphere
  interior/exterior, paired loops, Helmholtz-like pair, opposing wires and
  finite-solenoid end effects, only where existing backend validation permits.
- [ ] Early formula/scalar examples: pulsating radial, rotating vector, traveling
  wave; harmonic/anharmonic/double-well potentials, traveling/standing waves,
  Gaussian packet; saddle, anisotropic well, softened dipole, interference,
  moving Gaussian. Label mathematical animation separately from consistent EM.
- [ ] Maxwell examples: Gaussian sphere, wire circulation, shielding, images,
  conductor boundaries, changing flux and bounded plane wave. Ship supported
  entries early; probe-dependent entries wait #95, plane-wave terms wait #97;
  heatmap/analysis-dependent exercises wait #92/#93, not unrelated features.
- [ ] RED/GREEN per small batch: all records parse, select correct modes, expose
  meaningful parameters and satisfy independent representative sign/value/time
  tests; softened potentials explicitly retain model warnings.

## Task 4 — #92 scalar views and point/line probes

**Dependencies:** #89; existing scalar curves/surfaces/time support.
**Prospective design path:** create
`docs/superpowers/specs/2026-11-18-scalar-views-contract.md`.
**Interface deliverable:** heatmap/contour/probe sample records preserving invalidity.

- [ ] Specify synchronized time/coordinates/parameters, point probes and configurable
  line sections; define invalid/discontinuous cells and contour interpolation limits.
- [ ] Produce analytic fixtures for constant/linear/saddle functions, poles, domain
  edges and animated samples; absent samples must not become fabricated values.
- [ ] Inventory plot state/render/capture consumers; approve exact paths, APIs,
  work/storage bounds, focused tests and representation export eligibility.
- [ ] STOP for design approval; this task is not a marching-contours algorithm choice.

## Task 5 — #93 scalar comparisons and gradients

**Dependencies:** #89/#92 and visualization #83.
**Prospective design path:** create
`docs/superpowers/specs/2026-11-18-scalar-analysis-contract.md`.
**Interface deliverable:** independently styled comparison series and explicit
gradient provenance/error status, separate from E interpretation.

- [ ] Specify multiple curves, optional surface-gradient arrows, synchronized time,
  and V-only interpretation E = -grad(V); generic scalar gradients are not E.
- [ ] Compare analytic/native-CAS/numerical browser-compatible options; provide
  independent derivative references, boundary/pole tests and work/error evidence.
- [ ] Approve differentiation method, limitations, exact ownership and capture
  matrix before a RED/GREEN implementation subplan; no wasm CAS requirement.

## Task 6 — #94 equation panel

**Dependencies:** #89; existing Maxwell diagnostics.
**Prospective design path:** create
`docs/superpowers/specs/2026-11-18-equation-panel-contract.md`.
**Interface deliverable:** four-law integral/differential display and term status
schema: analytic identity, numerical estimate, unsupported/unavailable, invalid/singular.

- [ ] Map symbols, active laws, assumptions, normalization/constants and provenance
  to existing modes; distinguish source-to-field calculation from residual inspection.
- [ ] Pin fixtures for all four laws, unsupported time terms and singular queries;
  unavailable is never silently zero and a small residual is never proof.
- [ ] Approve panel/model paths and exact signatures; independent explanations
  need not wait for scalar analysis or custom distributions.

## Task 7 — #95 bounded integral probes

**Dependencies:** #94 and approved per-probe numerical design.
**Prospective design path:** create
`docs/superpowers/specs/2026-11-18-integral-probes-contract.md`.

- [ ] Design supported Gaussian surfaces/oriented loops, placement/size/sampling,
  same-model flux/charge and circulation/current/flux-change comparisons.
- [ ] Provide orientation-reversal, analytic sphere/wire references, source-crossing,
  interface and nonconvergence fixtures; distinguish uncertainty from softening.
- [ ] Compare methods and evidence; approve coordinates, tolerances, cancellation,
  work/storage bounds and diagnostic signatures before numerical implementation.
- [ ] STOP: exact owner-approved algorithms/constants and implementation paths
  belong to the resulting bounded subplan, not this roadmap.

## Task 8 — #96 custom static rho/J

**Dependencies:** #89/#94; approved bounded source design.
**Prospective design path:** create
`docs/superpowers/specs/2026-11-18-static-distributions-contract.md`.

- [ ] Reuse placed-source composition first; define bounded volume rho(x,y,z),
  explicit support and shared parameters. Keep rho distinct from V/E/B.
- [ ] Define supported steady-current geometries and continuity/boundary checks;
  arbitrary J is not automatically valid. Identify queries that must reject.
- [ ] Supply independent analytic/resolved references, singularity and cancellation
  cases; compare integration methods and normalization/softening/error semantics.
- [ ] STOP for approved numerical design covering requested epsilon, units,
  work/storage/query bounds and exact source/evaluator/UI/test ownership.

## Task 9 — #97 analytic animated Maxwell demonstrations

**Dependencies:** #89/#94/#95; explicitly independent of #96.
**Prospective design path:** create
`docs/superpowers/specs/2026-11-18-analytic-maxwell-animation-contract.md`.

- [ ] Reuse changing-flux Induction; design source-free analytic plane-wave E/B
  and equation terms with explicit normalized conventions and propagation direction.
- [ ] Separate static/quasistatic/full-wave analytic assumptions; pin phase/sign,
  equation-term identities, shared-clock playback and detached capture tests.
- [ ] Approve exact implementation paths, export eligibility and model warnings;
  no new general solver or duplicate animation clock.

## Task 10 — #98 supported dynamic distributions

**Dependencies:** #96/#97; not arbitrary rho/J evolution.
**Prospective design path:** create
`docs/superpowers/specs/2026-11-18-dynamic-distributions-contract.md`.

- [ ] Select explicit supported models; document continuity, initial/boundary
  assumptions and evidence. Reject inconsistent inputs without retaining stale fields.
- [ ] Require independent analytic references, continuity-failure, time/capture,
  convergence and bounded-work tests; approve exact model APIs and numerics.
- [ ] STOP until that bounded design and implementation subplan receive approval.

## Task 11 — #99 follow-on guided tour design only

**Dependencies:** delivered #80 and core siblings #89–#98.
#99 is the eleventh native child of #81: never depend on parent completion,
which would create a cycle; it does not block core sibling capabilities.
**Prospective path:** create `docs/superpowers/specs/2026-11-18-guided-tour-ux.md`.

- [ ] Draft chapters: navigation/recovery → styling/seeds → parameters/library →
  scalar views/analysis → Maxwell/probes → sources/animation/model limitations.
  Each chapter has a practical action, explanation and checkable progression.
- [ ] Design skip/exit/replay, narrow/platform-aware layouts and reused Help;
  specify dirty-scene consent and safe entry/exit/replay restore of scenes,
  unsaved work, presets, preferences and clock. No silent overwrite.
- [ ] Provide transition-table fixtures for interrupted chapters, cancel and restore
  failure; STOP for separate UX approval and later code plan. No tour code here.

## Verification and future execution protocol

- [ ] NOW: docs checker, local links, policy/coverage/signature review and
  `git diff --check` only. No implementation, build, install, commit, push or merge.
- [ ] FUTURE: refresh BASE/claims/exclusive paths; RED (observe correct failure),
  GREEN minimal change, focused tests, refactor, full gates, independent diff review.
- [ ] Per [Workflow](../../../references/workflow.md), use explicit Rustup PATH/RUSTC:
  `cargo build`, `scripts/test.sh`, `cargo clippy --all-targets -- -D warnings`,
  `python3 scripts/check-docs.py`, `git diff --check`, changed-file rustfmt;
  `scripts/smoke-run.sh 15` where renderer/window/input or ticket requires it.
- [ ] Preserve ten jobs, at most two Cargo processes, separate persistent target/
  TMPDIR pairs under `/home/paul/.cache/vector-field-explorer-targets/`.
  Web/browser/manual visual checks remain waived, not passed; preserve web behavior.
- [ ] Follow [Workflow](../../../references/workflow.md) role/model policy;
  standard and numerical work use their configured roles at lowest effort; fresh
  independent review; fixes in the configured fix role. Stop on quota/auth without
  retry or provider changes.
- [ ] Report exact BASE/HEAD, paths, commands/results and status. Future authorized
  commits carry `Generated-by: OpenCode`. Publication triggers no execution.
- [ ] Self-review: #89–#99 mapped; #97/#98 dependency signatures consistent,
  catalogue waits per backend, numerical decisions explicitly gated.
