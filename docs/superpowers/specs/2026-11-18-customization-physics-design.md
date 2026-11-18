# Customization and physics-student exploration

## Status and authority

Owner confirmed the conversational scope on 2026-11-18. This written design is
**DRAFT — awaiting owner approval**, not implementation authorization.
The owner subsequently requested written plans explicitly, with no implementation.
Draft subsystem plans accompany this design; none is executable until design and
plan approval plus renewed implementation authorization. Numerical work retains
just-in-time bounded design gates. GitHub owns live requirements, claims,
dependencies and discussion.

- [Visualization and interaction epic #80](https://github.com/ttgg1/vector-field-explorer/issues/80).
- [Physics-student exploration epic #81](https://github.com/ttgg1/vector-field-explorer/issues/81).
- [Follow-on guided onboarding #99](https://github.com/ttgg1/vector-field-explorer/issues/99)
  is the final child of #81, after its core feature children.
- BASE: `4ed3610d1a642e7b8abaf68d7f73f5e614cb827f`, including Maxwell #30/PR #79.

## Intent

Give physics students control over what they investigate and how it is displayed.
Make parameter changes connect visibly to fields, plots and applicable Maxwell
equations. Keep the initial interface approachable while exposing advanced
controls, safe experimentation and reusable examples.

Success means users can customize a view without changing its physics, save
their own examples, compare meaningful quantities, and understand both what a
model demonstrates and what it does not establish.

## Approaches

The accepted conversational direction is incremental extension of existing
concept modes, pure model seams and shared parameter/preset infrastructure.
This reuses existing source evaluation, scalar animation, Induction, Materials,
Ode and Maxwell cases rather than commissioning them again.

Separate disconnected mini-apps would duplicate controls and state, hindering
comparison. A general arbitrary-source electromagnetic solver would introduce
substantial initial/boundary-condition and numerical scope beyond the agreed
bounded teaching models; it is not the chosen approach.

## Constraints and anti-goals

Preserve native and wasm scope, GPL-3.0-only compatibility, existing saved
settings, the single simulation clock, current rendering seams and keyboard
focus/capture safeguards. CAS remains native-only; supported physics needs an
analytic/numerical browser path, not a new wasm CAS requirement.

Never numerically add E and B, present normalized amplitudes as SI quantities,
turn numerical residuals into proof of exact physical validity, hide failed
convergence, or let customization remove required model/export warnings.
Distinguish numerical uncertainty, source softening/model distortion and missing
physics. Singularity/interface cases must be explicit, not quietly interpolated.

No general 3D BEM/FEM/FDTD, arbitrary-source full-wave evolution, volumetric
scalar isosurfaces, dimensional-analysis engine, release operation or performance
floor claim is authorized here. #18 performance evidence remains deferred, not
passed, and does not impose an invented dependency on the new capabilities.

## Visualization and interaction: #80

1. **#82 Viewport styling:** independent background, grid, box and per-axis
   colors, applicable opacity/width controls, exact entry, previews and resets.
   Include dark-laboratory, white-paper and high-contrast looks.
2. **#83 Color mapping:** editable gradient stops, reversal, portable colors;
   magnitude/component/direction mappings with sequential/diverging/cyclic
   legends as appropriate. Extend existing auto/manual ranges with freeze and
   suitable logarithmic scaling; signed/zero inputs require explicit semantics.
3. **#84 Layer styling:** shared inheritance or independent styles for arrows,
   streamlines, particles and trails; configurable electric/magnetic family
   colors and Ode trajectories. Effective legends and exports must agree.
4. **#85 Appearance profiles:** named portable styling records, separate from
   complete scene and camera records. Applying one cannot change physical
   parameters, formulas, source geometry, camera or simulation state.
5. **#86 Safe editing:** bounded undo/redo for accepted edits, coalesced drags,
   scoped color/camera/simulation resets, explicit preset/mode transition
   behavior. Simulation frames and file/export side effects are not undo events.
6. **#87 Seed layouts:** exact placement, selection/move/delete and configurable
   line/circle/region seeding; reusable records, plane-aware coordinates and
   bounded work. Integrate with undo rather than inventing a second history.
7. **#88 Interaction profiles:** remappable shortcuts and camera sensitivities,
   zoom/inversion controls, conflict reporting and reset. Preserve escape,
   pointer-capture recovery, typing focus and platform-specific restrictions.

## Shared parameter and preset foundations: #81

**#89 Named parameters** carry validated names, finite values, bounds and steps.
Coordinates, time and parser/function identifiers are reserved. Vector/scalar
formulas and supported teaching cases consume the same definitions. A committed
edit updates dependent scene/plot/diagnostic data coherently; exports capture
immutable definitions/values rather than reading a changing live scene.

**#90 Preset library** uses stable identities, collapsible teaching groups with
remembered expansion, search, 2D/3D/animated filters and My presets. Camera
presets, appearance profiles and complete scenes remain distinct. User scene
records support save-as, editing and versioned import/export with atomic
validation and actionable storage errors. Applying a scene explicitly changes
its documented scope, not unrelated UI preferences. Preserve durable inactive
mode data unless a documented, confirmed action intentionally replaces it.

Each teaching preset includes its backend, assumptions, editable parameters,
suggested domain/camera and an exploration prompt. Animated means actual
supported playback, not necessarily export eligibility. Unsupported combinations
must be explained; do not expose inert controls or unsupported presets.

**#91 Catalogue additions**, reusing existing entries where applicable:

| Group | Examples |
| --- | --- |
| 3D electrostatics | Quadrupole, unequal charge pair, off-axis ring, sphere interior/exterior |
| 3D magnetostatics | Paired loops, Helmholtz-like pair, opposing wires, finite-solenoid end effects |
| Animated vectors | Pulsating radial, rotating vector and traveling-wave fields |
| Maxwell | Gaussian sphere, wire circulation, shielding, images, conductor boundaries, changing flux, bounded plane wave |
| Scalar curves | Harmonic/anharmonic potentials, double well, traveling/standing waves, Gaussian wave packet |
| Scalar surfaces | Saddle, anisotropic harmonic well, softened dipole potential, interference, moving Gaussian |

Label mathematical vector animations separately from physically consistent EM
examples. A softened dipole potential must not be presented as an unsoftened
point-charge singular solution. Advanced catalogue entries follow their actual
enabling backend (#92/#93/#95/#97) without blocking existing-backend entries.

## Scalar workbench

**#92** adds heatmaps/contours, point probes and configurable line cross-sections
to existing curve/height-surface functionality. Time, parameters and coordinates
stay synchronized. Discontinuity/invalid-sample handling and contour
interpolation limits remain visible; probes cannot report fabricated values.

**#93** adds multiple independently styled comparison curves and optional
gradient arrows for supported scalar surfaces. Electric-potential interpretation
explicitly uses E = -grad(V); generic scalar gradients are not automatically E.
Reuse existing time support and define actual screenshot/animation/export
eligibility for each new representation instead of assuming vector export paths
already handle scalar plots. Analytic and numerical differentiation must be
identified with their limitations; no browser CAS dependency is assumed.

## Maxwell learning and custom sources

**#94 Equation panel** presents all four Maxwell equations in integral and
differential forms, symbol explanations, active laws and assumptions.
Distinguish sources-to-fields calculation from user-specified-field residual
inspection. Mark each result as analytic identity, numerical estimate,
unsupported/unavailable term, or invalid/singular query. Display conventions,
normalization and applicable constants explicitly; connecting present normalized
models is not an implicit conversion to SI.

**#95 Integrals and probes** provide bounded supported Gaussian surfaces and
oriented circulation loops, editable placement/size/sampling and comparisons of
flux/enclosed charge or circulation/applicable current and flux-change terms.
Surface normals and loop orientation must be explicit. Handle source crossings,
interfaces, uncertainty and nonconvergence honestly. Integral and differential
forms must refer to the same underlying model, not unrelated formula displays.

**#96 Custom distributions** first reuses placed-source composition, then adds
bounded formula-defined volume charge density rho(x,y,z), explicit support and
shared parameters. Steady-current inputs require supported geometry, continuity
and boundary checks; an arbitrary J is not automatically a valid static source.
Parameter changes update supported fields and Maxwell diagnostics.

Before numerical implementation, approve a bounded per-feature design covering
coordinates/support, normalization, integration/differentiation methods,
softening where applicable, requested epsilon, work/storage/cancellation,
singularity handling and independent reference evidence. Epsilon is a requested
tolerance, not a certified error guarantee. Reuse source safety and accuracy
reporting; do not restart a broad performance-research prerequisite.

**#97 Bounded animated demonstrations** reuse Induction for changing-flux
examples and add an explicit analytic source-free plane-wave model. Keep
quasistatic and full-wave analytic assumptions separate. Synchronize E/B,
equation terms, playback and detached capture through the shared clock.
These analytic demos do not depend on arbitrary custom distribution support.

**#98 Time-dependent distributions** follows #96 and #97 for explicitly
supported models only. Check charge continuity and disclose initial/boundary
assumptions. Reject inconsistent inputs; no generic inference of electromagnetic
evolution from arbitrary rho/J is promised.

## Follow-on onboarding: #99

After #80 and core #81 feature children #89–#98, provide a beginner-friendly
replayable guided tour with
manageable chapters covering the full supported feature set. Include practical
actions and physics/model explanations, skip/exit/replay, and platform/layout
awareness. Reuse Help content. Never silently overwrite scenes, unsaved work,
presets or preferences; detailed entry/exit restoration and progression behavior
need their own UX approval. This issue does not block other feature children.
It is a native child of #81, not a dependency on completion of its own parent.

## Dependency-aware rollout and implementation planning boundary

| Slice | Capabilities | Genuine prerequisites |
| --- | --- | --- |
| Independent foundations | #82, #83, #86, #88, #89 | Existing merged infrastructure and each task's approved design |
| Styling integration | #84, #85, #87 | #84 after #83; #85 after #82/#83/#84; #87 after #86 |
| Organized examples | #90, initial #91 | #90 after #89; #91 after #89/#90, advanced entries after their backends |
| Scalar exploration | #92, #93 | #92 after #89; #93 after #89/#92/#83 |
| Equation exploration | #94, #95 | #94 after #89; #95 after #94 |
| Custom sources | #96 | #89/#94 plus approved bounded numerical design |
| Animated physics | #97, #98 | #97 after #89/#94/#95; #98 after #96/#97 |
| Guided tour | #99 | Delivered #80 and #89–#98 feature set and approved tour UX |

This is not mandatory phase order. Choose the cheapest genuinely ready,
authorized task; independent equation explanations need not wait for scalar
analysis, interaction profiles or arbitrary sources. Serialize shared files.
More advanced #91 entries can follow their prerequisites without delaying its
existing-backend examples.

Draft plans are split by subsystem:
[visualization and interaction](../plans/2026-11-18-visualization-customization.md)
and [physics exploration](../plans/2026-11-18-physics-exploration.md).
After written design/plan approval and renewed implementation authorization,
refresh implementation task briefs with
exact BASE, exclusive owned paths, tests, dependencies, acceptance criteria and
references against current Git. Do not pretend exact future file ownership is
known before inspecting each feature's seams. Simple/standard work uses the
configured Luna implementer; complex numerical/rendering work uses Sol, each at
its lowest configured effort. Fresh independent review checks actual diffs;
OpenAI review fixes route to Luna's configured fix role.

## Verification and completion

Each preset must parse/validate, select the intended mode, expose meaningful
parameters and have representative sign/value/behavior tests. Numerical tools
need independent analytic/resolved references, invalid/unsupported cases,
orientation, convergence and finite work/storage tests. UI tests cover
responsive controls, grouping/filtering, focus and safe state transitions.
Persistence/import tests cover old records, version errors and atomic failure;
capture tests use immutable snapshots and preserve styling/model warnings.

Per references/workflow.md: Rust changes require build, scripts/test.sh, clippy
with warnings denied, docs/link checks, diff checks, changed-file formatting and
applicable 15-second runtime smoke. Preserve the ten-job Cargo cap and at most
two Cargo processes with separate persistent targets/temp directories.
Docs-only changes use docs/link/policy/diff checks, not Cargo. Dedicated
web/browser/manual/visual checks remain waived, not passed; this does not waive
web behavior or existing automated functional/layout tests.

No product implementation, merge, release or numerical correctness claim is
established by this design or by publication of the issue tree. Publishing the
reviewed draft documents on their feature branch does not authorize product work.
