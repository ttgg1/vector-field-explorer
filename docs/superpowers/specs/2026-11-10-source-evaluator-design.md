# Source model and evaluator — proposed design

**Status: PROPOSED — not approved.** This design narrows the first approved
Phase 1 layer to an egui-free `Source` model and pure superposition evaluator.
The split into model/evaluator → Sources panel+persistence → glyphs/2D
slicing/presets is approved; the concrete math/API choices below are proposals
for owner review, not settled rulings. No implementation may infer approval
from this document.

## Context and fixed constraints

Ticket 55 (`.scratch/field-viewport/issues/55-physics-roadmap.md`) defines a
teaching-grade source explorer, not a quantitative solver. Formula mode and
source mode remain exclusive; formula presets remain formulas. Sources
superpose, 2D later samples one z slice of the same 3D model through `PlaneView`,
SI-like units are display-only, and there is one global softening parameter.
These settled rules are not reconsidered here. This deliverable is only the
pure, GPU-independent evaluator; no UI, persistence, glyphs, slice wiring, or
preset migration.

The module should fit current seams: `glam::Vec3`, `Domain`, and
`sample_field_at` use f32 coordinates/results; `FieldExpr` returns `Option<Vec3>`
for non-finite evaluation; tests are integration modules in `tests/` registered
by `tests/suite.rs`; `SceneGPU` remains the only paint seam. `src/lib.rs` is the
public module registry. Existing formula presets use dimensionless teaching
fields and denominator `+0.05`; this new model must not silently reuse that
formula convention as source softening.

## Proposed interface

Put the module in `src/sources.rs` (singular source *model/evaluator* module;
the user-facing panel is plural `Sources`). Keep the first external interface
small and independent from UI/state:

```rust
pub enum FieldFamily { Electric, Magnetic }
pub struct Source { pub center: Vec3, pub axis: Vec3, pub kind: SourceKind }
pub enum SourceKind {
    PointCharge { charge: f32 },
    FiniteLineCharge { charge: f32, length: f32 },
    Ring { charge: f32, radius: f32 },
    Disc { charge: f32, radius: f32 },
    Sphere { charge: f32, radius: f32 },
    StraightWire { current: f32 },
    CurrentLoop { current: f32, radius: f32 },
    Solenoid { current: f32, radius: f32, length: f32, turns: u16 },
}
pub struct SourceSet { pub family: FieldFamily, pub sources: Vec<Source> }
pub struct Softening(pub f32);
pub enum SourceError {
    InvalidParameter(&'static str),
    MixedFamilies,
    NonFiniteResult,
    QuadratureFailure(QuadratureFailure),
}
pub enum QuadratureFailure { BudgetExhausted, DidNotConverge }
pub fn evaluate_sources(set: &SourceSet, point: Vec3, softening: Softening)
    -> Result<Vec3, SourceError>;
```

These failure variants express possible evaluator outcomes, not an approved
budget, convergence threshold, or supported accuracy envelope. Their exact
public representation remains proposed until builder evidence establishes the
needed distinctions.

The type spellings are proposals. `SourceSet` makes the important invariant
explicit: a displayed vector field is electric **or** magnetic, never a numeric
sum of unlike E and B quantities. Charge kinds belong to `Electric`, current
kinds to `Magnetic`; constructing/evaluating a mixed set fails clearly. Empty
sets evaluate to zero. Each source contributes independently and invisibility
is not in this model (visibility is UI/state concern). Do not add `t`, units
types, per-source softening, arbitrary geometry, or solver traits.

`axis` is a finite nonzero direction, normalized internally; local z is the
source axis. For finite-line charge, ring/disc/sphere, loop and solenoid it
defines orientation. Straight wire is infinite along this axis. Point charge
does not use it. Storing a direction rather than a quaternion keeps the
geometry interface small and makes invalid zero axes testable.

## Proposed geometry and parameter contract

All scalar parameters and center/axis coordinates must be finite. The evaluator
rejects invalid values rather than producing NaN or silently repairing user
input. `axis` must have squared length greater than `1e-12`; valid axes are
normalized in f64 working arithmetic. Magnitudes are bounded to absolute
charge/current `<= 1e3`, lengths/radii `1e-3..=1e2`, and solenoid turns
`1..=1000`. These broad UI-safe bounds are proposals, not physics limits;
they prevent overflow and degenerate geometry while allowing the default
`[-1,1]^3` domain. `Softening` is a finite positive world-length in
`1e-3..=1`; one value applies to every source and every singular kernel.
Whether controls clamp versus reject remains a UI concern; the pure evaluator
rejects out-of-contract values.

Centers and the query point are world coordinates in the same display-only
length units. `charge` and `current` are signed display-only strengths. Positive
charge points electric field outward; negative charge reverses it. Positive
current uses the right-hand rule around `axis`; negative current reverses it.
The common proportionality constants (Coulomb `1/(4π ε₀)` and Biot–Savart
`μ₀/(4π)`) are omitted from all geometries, as are dimensional conversions.
This means every electric contribution uses the Coulomb kernel without its
common prefactor and every magnetic contribution uses the Biot–Savart kernel
without its common prefactor; relative strengths within each family are thereby
defined. In particular, integrating the softened line kernel
`I dℓ × r / (|r|²+a²)^(3/2)` over an infinite straight wire gives
`2 I (axis × r_perp) / (|r_perp|²+a²)`. The factor 2 is part of the evaluator's
relative normalization, not an optional proportionality. `charge` on extended
electric sources is total charge uniformly distributed over the indicated
geometry. `turns` is a turn count; current is current per turn in the teaching
model.

Proposed support geometry:

| Kind | Distribution / extent |
|---|---|
| Point charge | point at `center` |
| Finite line charge | uniform segment centered at `center`, length along `axis` |
| Ring | uniform infinitesimally thin circular charge ring in the plane normal to `axis` |
| Disc | uniform thin charged circular disc in the plane normal to `axis` |
| Sphere | uniformly volume-charged solid sphere |
| Straight wire | infinite current along `axis` through `center` |
| Current loop | one thin circular current loop in the plane normal to `axis` |
| Solenoid | finite ideal thin current-sheet winding, radius and axial length |

The “Ring” is an **electrostatic** ring; “Current loop” is its magnetic
counterpart. Keep them distinct so the source's units and field family cannot
be confused. Do not model a current loop as a magnetic dipole approximation
except as an explicitly documented far-field check. A solenoid is finite and
includes end effects; it is not the existing formula preset's spatially
uniform/azimuthal approximation.

## Proposed math and numerical method

Use f64 intermediates for geometry, kernels, quadrature accumulation, and
superposition, then cast once to `Vec3`; return `NonFiniteResult` if either
intermediate/final result is non-finite or outside f32 range. This preserves
the app's f32 seam while preventing intermediate overflow/underflow from
silently leaking into arrows. No per-sample allocation.

For point charge, use softened Coulomb kernel
`q r / (|r|² + a²)^(3/2)`, `r = point - center`, `a = softening`. This is a
softened teaching kernel, not a physical finite-size charge. For infinite
straight wire, use the softened integrated Biot–Savart line kernel
`2 I (axis × r_perp) / (|r_perp|² + a²)`, where `r_perp` is displacement
perpendicular to axis. Its sign follows the right-hand rule; the common
`μ₀/(4π)` remains omitted as declared above.

For bounded distributions, a fixed finite quadrature order is not an accuracy
contract: a softened kernel can be arbitrarily narrow relative to source size
over the proposed parameter envelope. Instead, propose deterministic adaptive
quadrature with a tensor-product 8-point/16-point Gauss–Legendre pair on each
parameter cell, local subdivision, a per-query evaluation budget, and a
documented requested absolute/relative error target. For each cell, compare the
two vector estimates componentwise; sum the high-order estimates and estimated
errors over cells. Accept only when the global estimated error norm is at most
`abs_tol + rel_tol * norm(estimate)`. Otherwise bisect the cell with the largest
estimated error, choosing its longest side in normalized parameter coordinates
(stable axis-order tie break), and repeat. Integrate source parameter domains
(line; ring/loop angle; disc radius/angle; solenoid axial position/angle) with
the corresponding normalized measures. The sphere uses the symmetry-reduced
angular/radial integral described below and applies the same adaptive rule to
its remaining variables. If the evaluation budget is exhausted before the
criterion passes, return an explicit convergence/budget error. The estimate is
an error indicator, not a rigorous bound; both rules can under-resolve a
narrow feature and agree incorrectly. Passing the indicator is not a
mathematical accuracy guarantee. Deterministic
cell selection and accumulation order make results reproducible. This can be
substantially slower than non-adaptive quadrature for narrow kernels; performance,
absolute/relative tolerances, work budget, and any resulting validity
restrictions require measured evidence and owner approval. Their numeric values
are deliberately unspecified here. No numerical parameter range is claimed
accurate by this proposal. A bounded total query budget and source-count limit
are prerequisites for a worst-case work bound; neither is specified yet.
Detected exhaustion/nonconvergence can be reported rather than returning an
inaccurate finite answer, but the estimator cannot guarantee detection of every
unresolved result.

For a uniformly volume-charged solid sphere, use spherical coordinates about
its center, but exploit rotational symmetry rather than treating the sphere as
concentric planar rings. Let `s` be the query displacement, `R=|s|`, `ρ` the
source radius, and `u=cosθ` the angle cosine relative to `s`. With the softened
kernel, the angular integral of its radial component is
`2π ∫[-1,1] (R-ρu)/(R²+ρ²+a²-2Rρu)^(3/2) du`; the azimuthal components cancel.
Multiply by the volume measure `ρ² dρ`, integrate `ρ` from `0` to sphere
radius, and normalize by `3q/(4π radius³)`. The adaptive angular integral
(or its analytic primitive, if implemented and independently checked) plus
adaptive radial integration therefore preserves the full solid-angle measure.
At `R=0`, symmetry gives exactly zero. This is a two-variable/reduced
calculation, not the incorrect radial × azimuthal planar-ring rule.

For other geometries apply the same softened Coulomb kernel at each charge
element and softened Biot–Savart kernel at each current element. Normalize
weights so `charge` is total charge. The charged ring uses normalized angular
measure `dφ/(2π)`. For a current loop of radius `b`, the line element is
`dℓ = b eφ dφ`; equivalently, an angular average requires the compensating
factor `2πb` in its kernel weight. The disc uses area measure
`2ρ/Rdisc² dρ × dφ/(2π)`; finite line integrates uniformly over
`[-L/2,L/2]`. The solenoid uses the current-loop line element weighted by
`N dz/L`, for turns `N` uniformly distributed over axial length `L`.
Independent analytic checks must pin loop axial magnitude and solenoid
turn/length normalization.
The exact adaptive cubature decomposition and cost are implementation details
only insofar as they preserve these measures and satisfy the explicit
convergence-or-error contract.

The charged ring is not a dipole source at large distance: its net charge is
`q`, so its leading electric field is the monopole field `q r/|r|³` (up to the
declared common prefactor). Its dipole moment is zero, but its quadrupole and
higher even multipoles are generally nonzero for a centered uniform ring. The
magnetic current loop, by contrast, has a leading magnetic
dipole along its axis, with moment proportional to `I πR² axis`. Acceptance
tests must distinguish these asymptotics; do not require a charged ring to have
a magnetic-loop-like dipole field.

Builder evidence must establish the supported source count, total query budget
across the source list, treatment of cancellation and error accumulation, and
bounded cell-storage strategy (including reconciliation with “no per-sample
allocation”). These are UNSET builder-evidence prerequisites, not completed
implementation decisions. Per-source caps alone do not bound a query with an
unrestricted source count. This does not approve any proposed parameter/query
bounds.

This method shares one kernel and one softening semantics across geometries,
avoids special-function dependencies/branch singularities, and is portable to
wasm. Its cost is variable, potentially high per-query work. Measure worst-case
and representative source presets against ticket 35's budget. If the budget is
exceeded, report the convergence failures and cost; do not silently cap work
and return a field, weaken the tolerance, or claim the proposed bounds are
supported. The owner must then choose among narrower supported ratios/queries,
a changed accuracy contract, or another algorithm. No such choice is made
here.

## Alternatives considered

- **Closed forms for every geometry:** attractive for speed, but off-axis
  charged ring/disc/sphere and loop/finite solenoid require elliptic or delicate
  special-function cases. That is a disproportionate dependency/numerical
  surface for a teaching explorer. Keep the point-charge and infinite-wire
  closed forms; use bounded deterministic quadrature for the rest.
- **Non-adaptive quadrature:** deterministic and cheap, but node count alone does
  not establish accuracy when the softened kernel is narrower than node spacing.
  Adaptive convergence indicators with a proposed work budget make some
  cost/failure tradeoffs explicit, but can miss under-resolution; they do not
  guarantee detection or accuracy.
- **Analytic axial-only formulas:** fast but would make the evaluator invalid
  for the full 3D grid that the source model promises.
- **Mix E and B in one vector sum:** rejected because those fields have unlike
  meanings/units even when units are display-only. A homogeneous `SourceSet`
  preserves exclusive source family while retaining superposition.
- **Clamp bad model inputs:** rejected at the pure seam; it hides mistakes.
  Editors may clamp before creating a valid model, while evaluator errors stay
  explicit.

## Validation and test policy (proposed)

Integration tests in `tests/sources_test.rs`, registered in `tests/suite.rs`;
no inline unit-test module and no test-only public API. Use exact tests at the
public `evaluate_sources` seam. Keep all tests deterministic and analytic where
possible; no random sampling or GPU/device setup.

Required groups:

1. **Model/validation:** all eight kinds accept documented bounds; reject NaN,
   infinities, zero/near-zero axis where used, nonpositive softening, invalid
   radii/lengths, zero turns, and mixed electric/magnetic sets. Empty set is
   exactly zero. Zero-strength valid sources are exactly zero.
2. **Point charge:** on-axis and off-axis vectors match the declared softened
   kernel at `a=0.1` with relative error `<= 1e-6`; sign reversal is exact
   within `1e-6`; at the source center the result is finite zero.
3. **Dipole acceptance:** two opposite point charges at `(-0.5,0,0)` and
   `(0.5,0,0)` match the sum of their analytic softened kernels at every point
   of a fixed 5×5×5 grid on `[-1,1]^3`, with component absolute error
   `<= 2e-6` and relative vector error `<= 2e-5` (relative denominator floored
   at `1e-6`). Include grid source locations and require all outputs finite.
4. **Geometry and orientation:** each extended kind has a symmetry/axis case
   and sign/reversal case; ring and loop have axial symmetry and correct
   far-field behavior (charged ring monopole; current-loop dipole); disc and
   sphere have expected radial symmetry; finite line and solenoid are symmetric
   about center. Compare quadrature to an independent, separately refined
   reference at off-axis points; assert the requested error on cases where both
   approximations converge, and assert explicit error on cases exceeding the
   work budget. Tests must include narrow-kernel cases at proposed parameter
   extremes. A higher-order answer is not an oracle until its own convergence is
   checked. Tolerance and budget values remain proposals requiring evidence and
   owner approval.
5. **Robustness:** across evidence-backed cases in the proposed domain, all
   kinds either converge within the requested estimate or return the explicit
   budget error; additive superposition is order-independent within the
   documented tolerance. Test overflow-scale centers/strength combinations are
   rejected, not cast to infinity.

Tolerance levels intentionally distinguish exact point math from quadrature. If
the independent reference fails, first inspect the formula,
measure/weight normalization, and axis transforms; do not loosen tolerances
without evidence and an explicit design update. Full source-preset performance
acceptance is owned by a later end-to-end Phase 1 gate, not this model slice.

## Decisions still needing owner approval

Only decisions that materially alter the public model/math need owner input:

1. Approve or revise the proposed homogeneous electric/magnetic `SourceSet`
   (including whether both families need to coexist in one editable
   configuration; if yes, keep separate E/B outputs rather than summing them).
2. Approve or revise the precise distribution meanings for sphere (solid
   volume charge) and solenoid (finite continuous current sheet), plus the
   proposed adaptive convergence/error contract. Its tolerance, work budget,
   and supported parameter/query envelope are explicitly unsettled pending
   numerical and performance evidence; fixed node counts are not presumed
   accurate over the broad proposed limits.
   In particular, what near-field / on-support behavior must remain
   teaching-correct, and may configurations or query regions be rejected when
   that accuracy cannot be established within the budget?
3. Approve the numeric limits and the global softening range/default. A
   concrete default is intentionally deferred to the UI/preset ticket, which
   must choose it and document it; this evaluator contract requires callers
   to pass a validated value.

The API/type organization, f64 intermediates, error behavior, and test layout
are technical proposals for review, not additional user questions. Formula vs
source exclusivity, global softening, z-slice semantics, display-only units,
and teaching-grade scope are settled and are not open questions.
