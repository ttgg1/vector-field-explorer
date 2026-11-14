# 0003: Physics foundations — analytic sources, additive CAS, and phase order

Physics is a post-v1, teaching-grade program built from editable sources whose
fields superpose. 2D mode is one z slice of the same 3D source model, selected
through `PlaneView`; ticket 31's 2D formula presets stay formulas and are
labelled as approximations. The CAS is an additive transform layer over
`meval` (differentiation and gradient/divergence/curl only; no solve or
integrate in v1), and `meval` remains the only evaluator. Solvers are
analytic-first: canonical Maxwell geometries, images for 3D sphere/plane cases,
and a capped 2D boundary-element method for free conductor outlines as an
explicit infinite-cylinder approximation; 3D BEM/FEM, full-wave Maxwell, and
radiation are out. The work order is Windows packaging → sources + CAS →
induction → ODE workbench → matter → Maxwell, with ticket 35 performance and
tickets 39/38 time animation as gates. These choices are hard to reverse because
the source model, 2D semantics, and solver boundaries define the physics
vocabulary and UI; a general solver or a second evaluator would be a rewrite.
Status: accepted.

## Dated superseding decision — 2026-11-10 (CAS only)

The owner selected `exmex` 0.21.0 with its `partial` feature for the native CAS
track and deferred CAS wasm support and its size/runtime acceptance for future
investigation. This supersedes the CAS candidate ordering and wasm-clean/size
gates above for this implementation slice only. The rest of this ADR, including
the additive transform layer, `meval` as the only evaluator, phase order, and
solver boundaries, remains in force. The web app and its existing evaluator
remain unchanged; the CAS dependency, module, and call paths are native-only.

The native workflow is a transient CAS panel with scalar/vector input,
operation selection, result preview/copy, and explicit Apply for vector
results. Failures preserve the current field; no automatic apply or saved
history is added. In 2D, curl is the scalar `dFy/dx - dFx/dy`; in 3D it is a
vector. Unsupported symbolic functions fail clearly. See the dated
[native CAS decision](../wiki/decisions/2026-11-10-native-cas.md); exact
symbolic grammar/printer acceptance remains subject to independent builder
review before implementation.

## Considered options

- **Runtime CAS:** `exmex` first (MIT/Apache, wasm, but roughly 1.5–1.7 MB from
  its regex), then `symcas` (small, young), then an in-house differentiator on
  `meval`'s public tokens/RPN. `Symbolica` was rejected: source-available,
  non-OSI runtime terms, GPL-incompatible. Replacing `meval` with a CAS
  evaluator was rejected to avoid two field grammars.
- **Matter solver:** analytic per-region cases first; no full FEM. BEM is kept
  for 2D conductor outlines only, with an element cap.
- **Maxwell solver:** canonical catalog and images; 3D BEM/FEM rejected on wasm
  cost and accuracy grounds. Free 2D conductors use the capped BEM exception.
- **ODE solver:** `differential-equations 0.7` for workbench trajectories
  (Apache-2.0, wasm, adaptive/events/dense); the existing `rk4_step` stays for
  particles; `ode_solvers` rejected because it lacks stiff support and pins
  `nalgebra 0.34`; `russell_ode` only for a future stiff need.
- **Parallelism:** native-only `rayon` behind `cfg`, adopted after measurement;
  wasm sequential; no `SharedArrayBuffer` requirement.

## Consequences

- New dependency surface: `nalgebra`/`faer`/`differential-equations`/`exmex`,
  each gated on GPL-3.0 compatibility and a wasm build.
- CAS wasm size is measured, not budgeted; the fallback is in-house.
- 2D free conductors are labelled infinite-cylinder approximations; 3D free
  conductors remain catalog/image-only.
- Physics panels multiply UI surface; split by concept (Sources, Materials,
  Maxwell, Plot/ODE).
- The app is a teaching-grade explorer, not a quantitative solver.
