# Plans and specifications

These are design/task records with individual approval states, **not a live queue or policy source**.
Before dispatching work, use [Workflow](../../references/workflow.md) and its
main-root ledger/handoff/queue pointers. Historical checkboxes, status labels
and gate lists do not override newer owner rulings or authorize redispatch.

| Record | Purpose/evidence |
| --- | --- |
| [Beamline mode design](specs/2026-11-21-beamline-design.md) | Proposed accelerator physics workspace, transport model and first focusing investigation. |
| [Customization and physics design](specs/2026-11-18-customization-physics-design.md) | Draft scope for #80/#81 and onboarding child #99; awaiting written approval, no product implementation authorized. |
| [Visualization customization plan](plans/2026-11-18-visualization-customization.md) | Draft task/dependency plan for #82–#88; planning only, not executable authorization. |
| [Physics exploration plan](plans/2026-11-18-physics-exploration.md) | Draft task/dependency plan for #89–#99; numerical/UX design gates remain open, planning only. |
| [Post-v0.1.0 plan](plans/2026-11-05-vector-field-explorer.md) | Historical task scopes; helper entrypoint for the old SDD workspace. [Roadmap history](../wiki/roadmap/post-v1-frontier.md). |
| [Native CAS plan](plans/2026-11-10-native-cas.md) | Adapter/panel task contracts; helper entrypoint for `.superpowers/sdd/2026-11-10-native-cas/`. |
| [Native CAS spec](specs/2026-11-10-native-cas-design.md) | Accepted symbolic grammar, semantics, compatibility and UI boundaries. |
| [CAS rails design](specs/2026-11-14-cas-rails-design.md) | GitHub #14 separate native CAS panel and extensible ordered rails; compatibility and Compact boundaries. |
| [CAS rails plan](plans/2026-11-14-cas-rails.md) | Two serialized TDD tasks; native gates and review before preserving merge. |
| [Reference visibility design](specs/2026-11-11-reference-visibility-design.md) | Ticket 77 approved reference-geometry and UI/persistence boundaries; not implementation authorization. |
| [Reference visibility plan](plans/2026-11-11-reference-visibility.md) | Ticket 77 task contracts, tests, reviews and current workflow verification gates. |
| [Native CLI plot export design](specs/2026-11-11-cli-plot-export-design.md) | Ticket 78 reviewed CLI/config, deterministic scene and headless output contract; design only, not implementation acceptance. |
| [Native CLI plot export plan](plans/2026-11-11-cli-plot-export.md) | Ticket 78 implementation tasks and verification handoffs; consult live workflow/ledger before dispatch. |
| [Accuracy-first source evaluator design](specs/2026-11-14-source-evaluator-design.md) | GitHub #24 all-eight pure model/evaluator; owner-approved 2026-11-16. Supersedes isolated 2026-11-10 source drafts. |
| [Accuracy-first source evaluator plan](plans/2026-11-14-source-evaluator.md) | Five bounded TDD/review units, provisional numerical defaults and separate E/B diagnostics; owner-approved 2026-11-16 implementation basis. |
| [Sources panel design](specs/2026-11-16-sources-panel-design.md) | GitHub #25 dedicated source CRUD panel, explicit Formula/Sources modes, fixed E/B Both colours and legend, accuracy opt-in and persistence; owner-approved 2026-11-17 and implemented. |
| [Sources panel plan](plans/2026-11-16-sources-panel.md) | GitHub #25 bounded TDD/review units; owner-approved 2026-11-17 implementation basis, completed and verified in Git history. |
| [Source visualization design](specs/2026-11-17-source-visualization-design.md) | GitHub #26 glyphs, projected selected-slice treatment, illustrative source presets, and shared export accuracy/model warnings; owner-approved 2026-11-17 and implemented. |
| [Animation export design](specs/2026-11-17-animation-export-design.md) | GitHub #71 APPROVED deterministic GIF export native+wasm, native MP4/WebM via optional external ffmpeg, GUI Formula/Induction/particles and CLI Formula/particles; implemented on the feature branch, not a merge claim. |
| [Animation export plan](plans/2026-11-17-animation-export.md) | Five bounded TDD/review units for GitHub #71: deterministic GIF/ffmpeg encoders, detached frame snapshots, GUI job/dialog, CLI animation and final gates; owner-approved implementation basis, completed on the feature branch with docs, benchmark evidence and final native gates. Not a merge claim. |
| [Source visualization plan](plans/2026-11-17-source-visualization.md) | GitHub #26 five bounded TDD/review units for glyphs, slice integration, presets, export provenance and final gates; owner-approved 2026-11-17, completed, reviewed and merged. |
| [Induction demo design](specs/2026-11-17-induction-demo-design.md) | GitHub #27 APPROVED dedicated Induction mode, implemented on the feature branch: editable loop/solenoid, softened vector-potential E, source-evaluator B, CAS/web waveform parity, shared clock, persistence and export provenance. |
| [Induction demo plan](plans/2026-11-17-induction-demo.md) | Five-task design-first delivery for #27; implemented on the feature branch with docs, benchmark evidence and final native gates. Not a merge claim. |
| [ODE workbench design](specs/2026-11-17-ode-workbench-design.md) | GitHub #28 APPROVED dedicated Ode mode: bounded fixed-step RK4 trajectories, seeds, shared clock, x/y phase and time-series charts, persistence/export and existing SceneGPU paint seam; implemented on the feature branch, not a merge claim. |
| [ODE workbench plan](plans/2026-11-17-ode-workbench.md) | Five bounded TDD/review units for GitHub #28: pure RK4 model, mode/state/clock, responsive panel/charts, persistence/export and docs/benchmark gates; owner-approved implementation basis, completed on the feature branch with docs, benchmark evidence and final native gates. Not a merge claim. |
| [Materials design](specs/2026-11-17-materials-design.md) | GitHub #29 APPROVED dedicated Materials mode over four bounded analytic dielectric/permeable sphere/boundary cases, total/applied/response separation, selected-plane slicing, explicit applicability and export provenance; implemented on the feature branch, not a merge claim. |
| [Materials plan](plans/2026-11-17-materials.md) | Five bounded TDD/review units for GitHub #29: pure analytic model, Materials mode/app, responsive panel, persistence/export and docs/benchmark gates; owner-approved implementation basis, completed on the feature branch with docs, benchmark evidence and final native gates. Not a merge claim. |
| [Maxwell teaching cases design](specs/2026-11-17-maxwell-design.md) | GitHub #30 APPROVED dedicated Maxwell mode over nine ideal analytic cases plus a bounded grounded 2D BEM conductor, image-plane/2D projection limits, display-only normalized units, persistence/export provenance and explicit applicability; implemented on the feature branch, not a merge claim. |
| [Maxwell teaching cases plan](plans/2026-11-17-maxwell.md) | Five bounded TDD/review units for GitHub #30: analytic catalogue, bounded 2D BEM, mode/state/BEM cache, responsive panel/help, and persistence/export/docs/benchmark gates; owner-approved implementation basis, completed on the feature branch with docs, benchmark evidence and final native gates. Not a merge claim. |
| [Public-catalog workflow design](specs/2026-11-15-public-catalog-workflow-design.md) | Superseded historical ruling: public rates only, no benchmark or accepted-work efficiency ratios. |
| [Public-catalog workflow plan](plans/2026-11-15-public-catalog-workflow.md) | Superseded historical report-surface change; see Markdown-first workflow records. |
| [Markdown-first agent workflow design](specs/2026-11-16-markdown-first-agent-workflow-design.md) | Owner-approved retirement of Python orchestration; native OpenCode Markdown roles and Python metrics only. |
| [Markdown-first agent workflow plan](plans/2026-11-16-markdown-first-agent-workflow.md) | Native OpenCode modes, metrics-only Python, and runtime retirement tasks. |
| [Object renderer design](specs/2026-11-20-object-renderer-design.md) | GitHub #106 accepted design for a depth-enabled instanced object pass, slice budgets and capture parity. |
| [Layer style contract](specs/2026-11-20-layer-style-contract.md) | GitHub #84 owner-accepted layer/family appearance overrides, precedence, persistence and appearance-panel boundaries; A1/A2/B slices need their own claims. |
| [Interaction profile contract](specs/2026-11-18-interaction-profile-contract.md) | GitHub #88 owner-accepted action bindings, conflict/reserved rules, camera feel, named profiles and the A1/A2/A3 boundaries. |
| [Edit history contract](specs/2026-11-18-edit-history-contract.md) | GitHub #86 accepted snapshot scope, bounded in-memory transactions, drag coalescing, redo invalidation and scoped resets; A1 model and A2 wiring need their own claims. |
| [Seed layout contract](specs/2026-11-18-seed-layout-contract.md) | GitHub #87 owner-accepted seed coordinates, bounded generators, Flow-section editing, document/history participation and named layouts; S1–S3 slices need their own claims. |
| [Scalar analysis contract](specs/2026-11-18-scalar-analysis-contract.md) | GitHub #93 proposed comparison series and potential-gradient records, differentiation proposal with error estimates, capture rules and A/B slices; owner approval required before implementation. |
| [Scalar views contract](specs/2026-11-18-scalar-views-contract.md) | GitHub #92 owner-accepted heatmap/contour/probe records, invalidity preservation, bounds and A/B/C slices; A/B/C slices need their own claims. |
| [Object renderer plan](plans/2026-11-20-object-renderer.md) | Six bounded TDD tasks for #106 slice 1: primitives, mapping, GPU pass, integration, slice/sign markers, persistence and performance evidence. Queued after the current #80/#81 and #18 work. |

CAS implementation evidence: [73 review record](../reviews/73-native-cas.md),
[75 review record](../reviews/75-native-cas-panel.md), and verifiable Git history.
Older source drafts in isolated worktrees are background, not implementation
authority; use the approved accuracy-first records above and reconcile current
workflow/ledger status before any dispatch.
