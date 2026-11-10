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
pub enum SourceError { InvalidParameter(&'static str), MixedFamilies, NonFiniteResult }
pub fn evaluate_sources(set: &SourceSet, point: Vec3, softening: Softening)
    -> Result<Vec3, SourceError>;
```

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
`μ₀/(4π)`) are omitted, as are dimensional conversions. `charge` on extended
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
straight wire, use softened Biot–Savart line kernel proportional to
`I (axis × r_perp) / (|r_perp|² + a²)`, where `r_perp` is displacement
perpendicular to axis. Its sign follows the right-hand rule.

For all bounded distributions use deterministic fixed Gauss–Legendre
quadrature, not adaptive integration: 32 nodes per line/azimuthal integral;
disc and solid sphere use 16 radial × 32 azimuthal nodes; solenoid uses 16
axial × 32 azimuthal nodes. Apply the same softened Coulomb kernel at each
charge element and the softened Biot–Savart kernel at each current element;
quadrature weights are multiplied by the normalized uniform distribution so
`charge` is total charge, not density. Ring/loop are azimuth-only. Disc uses
area weighting (`2ρ/R² dρ`); sphere uses volume weighting (`3ρ²/R³ dρ`) and
uniform solid-volume charge. Finite line integrates uniformly from `-L/2` to
`L/2`. Solenoid integrates a continuous surface current with total turn count
distributed uniformly along its length (the finite-sheet teaching model).

This method shares one kernel and one softening semantics across geometries,
avoids special-function dependencies/branch singularities, and is portable to
wasm. Its cost is a numerical approximation and potentially high source-count
cost; document it and measure the densest source preset against ticket 35's
budget before Phase 1 ships. If that budget fails, optimize without changing
the public field semantics, or return for an explicit design decision. Do not
silently lower quadrature order or parallelize native-only in this slice.

## Alternatives considered

- **Closed forms for every geometry:** attractive for speed, but off-axis
  charged ring/disc/sphere and loop/finite solenoid require elliptic or delicate
  special-function cases. That is a disproportionate dependency/numerical
  surface for a teaching explorer. Keep the point-charge and infinite-wire
  closed forms; use bounded deterministic quadrature for the rest.
- **Point-sample or low-order polygon approximations:** simpler but geometry
  dependent errors and poor near-field behavior. Fixed quadrature has a stated,
  testable order and remains deterministic.
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
   far-field dipole direction; disc and sphere have expected radial symmetry;
   finite line and solenoid are symmetric about center. Compare quadrature to
   independent higher-order reference (at least 128 nodes / 64×128) at fixed
   off-axis points; relative vector error `<= 2e-3` away from soft singular
   regions. These are evaluator acceptance tests, not tests of the reference
   quadrature implementation.
5. **Robustness:** across the default domain grid and documented parameter
   extremes, all eight source kinds return finite results; additive
   superposition is order-independent within `1e-6` relative tolerance. Test
   overflow-scale centers/strength combinations are rejected, not cast to
   infinity.

Tolerance levels intentionally distinguish exact point math from fixed-order
quadrature. If the independent reference fails, first inspect the formula,
measure/weight normalization, and axis transforms; do not loosen tolerances
without evidence and an explicit design update. Full source-preset performance
acceptance is owned by a later end-to-end Phase 1 gate, not this model slice.

## Decisions still needing owner approval

Only decisions that materially alter the public model/math need owner input:

1. Approve or revise the proposed homogeneous electric/magnetic `SourceSet`
   (including whether both families need to coexist in one editable
   configuration; if yes, keep separate E/B outputs rather than summing them).
2. Approve or revise the precise distribution meanings for sphere (solid
   volume charge) and solenoid (finite continuous current sheet), and the
   deterministic quadrature strategy/order as the v1 teaching approximation.
3. Approve the numeric limits and the global softening range/default. A
   concrete default is intentionally deferred to the UI/preset ticket, which
   must choose it and document it; this evaluator contract requires callers
   to pass a validated value.

The API/type organization, f64 intermediates, error behavior, and test layout
are technical proposals for review, not additional user questions. Formula vs
source exclusivity, global softening, z-slice semantics, display-only units,
and teaching-grade scope are settled and are not open questions.
