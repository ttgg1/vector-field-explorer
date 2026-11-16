# All-eight source evaluator — accuracy-first design

**APPROVED IMPLEMENTATION DESIGN — owner approved 2026-11-16.** Owns
[GitHub #24](https://github.com/ttgg1/vector-field-explorer/issues/24), under
[#17](https://github.com/ttgg1/vector-field-explorer/issues/17).
[Implementation plan](../plans/2026-11-14-source-evaluator.md).

## Authority and intent

The [latest owner direction](https://github.com/ttgg1/vector-field-explorer/issues/24#issuecomment-6885389213)
supersedes exhaustive accuracy/performance/input-envelope research as a coding
prerequisite: implement iteratively, prioritize practical accuracy for
physics/simulation-literate users (including students), accept expensive work,
optimize later. Accuracy can improve later too. Retain correctness, honest
uncertainty/failure reporting, finite-result safety and independent review.
All numerical choices below are **engineering proposals**, not owner-selected
constants. Approval of this document still precedes product implementation.

Evidence consulted: main-checkout ignored reports
`.superpowers/sdd/2026-11-10-native-cas/physics-roadmap-decisions-2026-11-14.md`,
`task24-evidence-spike-2026-11-14.md` and `task24-evidence-review-2026-11-14.md`.
Their numerical facts remain useful; their proposed further-research-first
sequence is superseded. The 2026-11-10 source drafts in the isolated
`physics-sources-plan` worktree are unapproved background, superseded by this
proposal, especially their homogeneous-family rejection and unspecified limits.

## Capability boundary

One pure CPU model/evaluator, native and wasm compatible, no new dependencies.
Use existing `glam::DVec3` and f64 throughout; no mandatory f32 conversion here.
Existing `FieldExpr` is formula-only and returns f32 vectors; leave it, app state,
sampling and `SceneGPU` unchanged. Formula/source exclusivity is a downstream
contract, not a new mode switch in #24. No Sources UI, persistence, glyphs,
rendering, presets or export implementation. #25 owns the actual epsilon slider;
#26 consumes availability, warning and provenance data.

Mixed electric/current sources coexist; sum E and B **separately**. One global
softening length applies to every source. Sources here are static; introduce no
timer or `t` argument. Future time dependence consumes the existing SimClock.
Display-only SI-like labels imply no dimensional-analysis/unit-conversion engine.
All eight kinds must exist and have independent reference coverage before #24
is complete; internal tasks do not become per-geometry GitHub tickets.

## Model and public seam

Create `src/sources/` with `mod.rs` (public facade), `model.rs` (validation),
`numeric.rs` (scaled arithmetic), `quadrature.rs` (bounded integration),
`electric.rs`, `magnetic.rs` (geometry reductions), `evaluate.rs` (family totals).
Register only `pub mod sources` in `src/lib.rs`. Proposed public contract:

```rust
pub enum FieldFamily { Electric, Magnetic }
pub struct Source { pub center: DVec3, pub axis: DVec3, pub kind: SourceKind }
pub enum SourceKind {
    PointCharge { charge: f64 },
    FiniteLineCharge { charge: f64, length: f64 },
    ChargedRing { charge: f64, radius: f64 },
    ChargedDisc { charge: f64, radius: f64 },
    SolidSphere { charge: f64, radius: f64 },
    StraightWire { current: f64 },
    CurrentLoop { current: f64, radius: f64 },
    Solenoid { current: f64, radius: f64, length: f64, turns: u32 },
}
pub struct AccuracyRequest { pub epsilon: f64, pub e_scale: f64, pub b_scale: f64 }
pub struct EvalOptions { pub softening: f64, pub accuracy: AccuracyRequest,
                         pub limits: WorkLimits }
pub struct SourceSet { /* validated immutable sources, private storage */ }
pub struct Evaluator { /* prepared sources and reusable bounded scratch */ }
pub struct Sample { pub electric: FamilyResult, pub magnetic: FamilyResult }
pub enum FamilyResult {
    TargetMet(Estimate),           // estimated target met, never certified
    AccuracyNotMet(Estimate, StopReason),
    Unavailable(StopReason),
}
```

`SourceSet::new(sources: &[Source], limits: &WorkLimits, cancelled: &dyn Fn() -> bool)
-> Result<Self, InputError>`;
`Evaluator::new(set: &SourceSet, options: EvalOptions, cancelled: &dyn Fn() -> bool)
-> Result<Self, PrepareError>`;
`Evaluator::evaluate(&mut self, point: DVec3, cancelled: &dyn Fn() -> bool) -> Sample`.
Preparation owns a validated copy and preallocated scratch; no GPU/UI/serde/thread
dependency. `evaluate` uses no new allocation. Cancellation closure supports
native workers or cooperative browser callers without choosing a scheduler.
Do not expose solver internals solely for tests.

`Estimate` exposes finite `value: DVec3`, `uncertainty: f64`, `target: f64`,
`absolute_contributions: f64`, and `diagnostics: Diagnostics`. Diagnostics contain
kernel/version ID `plummer-v1`, softening, requested epsilon/family scale,
source count, work units, resident cell high-water mark, and flags for model
regularization, roundoff limitation and underflow. No confidence percentage or
certified-bound language. Unavailable has no vector. `StopReason` distinguishes
`InvalidQuery`, `Cancelled`, `WorkLimit`, `StorageLimit`, `QueryLimit`,
`UnresolvedFeature`, `RoundoffLimited`, and `NumericRange`. Preparation/input
errors separately identify bad source index/parameter, invalid options, source
limit, allocation failure and cancellation (InputError::Cancelled before source
copy; PrepareError::Cancelled before evaluator allocation). Private validated fields prevent
post-validation edits.

Validate all finite centers/axes/strengths; positive finite softening, dimensions
and tolerances/scales; nonzero axis where used; turns >= 1. Negative Q/I and zero
strength are valid; point/sphere ignore axis direction but require finite data.
Normalize axes by max-component scaling, not a squared-norm threshold. A line is
centered on its axis; circles/discs are normal to it; wire extends infinitely.
Positive current follows the right-hand rule; reverse current or oriented axis
to reverse B. Sphere orientation is immaterial. Validate even zero-strength
sources before skipping their calculation. No arbitrary strength/length ratio
ban: finite inputs can still yield explicit NumericRange/limit outcomes.

## Physical model and normalization

Propose the Plummer teaching model, `F_a(s)=s/(|s|²+a²)^(3/2)`, for its smooth
on-support behavior and tractable reductions. This is not wire radius, disc
thickness, a conducting sphere or material physics. Omit the common Coulomb
and Biot–Savart prefactors consistently within each family.

| Kind | Distribution and evaluation path |
| --- | --- |
| Point | `Q F_a`; center exactly zero. |
| Finite charged line | Uniform `Q/L` on `[-L/2,L/2]`; direct one-dimensional segment integration. |
| Charged ring | `Q dφ/(2π)` at radius R; direct angular kernel plus stable near-axis series. |
| Charged thin disc | Uniform `Q/(πR²)`; exact transverse chord integration then one x integral. |
| Uniform solid sphere | `3Q/(4πR³)` volume measure; divergence-theorem surface reduction below. |
| Infinite straight wire | Exact `2I(axis × s_perp)/(|s_perp|²+a²)`; axis zero, disclose infinite extent. |
| Current loop | `I R e_φ dφ × F_a`; direct angular kernel plus near-axis series. |
| Finite continuous solenoid | Loop measure times `N dz/L`; integrate z analytically then angle. Includes end effects, not discrete turns. |

Softening distortion and numerical uncertainty are different. Emit the kernel
description/regularization flag for every sample; do not invent a universal
ideal-physics error percentage. As explanatory anchors, a point at `r=a` has
only `2^(-3/2)` of ideal magnitude; a wire there has half. Smaller epsilon
refines evaluation of the **same softened model**, not ideal physics.

## Concrete numerical approach

Choose analytic elementary paths plus one-dimensional, geometry-aware adaptive
Gauss–Kronrod 15/31 integration. Compared with the spike's elliptic hybrid this
avoids delicate special functions/division by radial coordinate; compared with
nested direct integration it removes a dimension for disc, sphere and solenoid.
Retain independent changed-coordinate oracles in tests, not a second production
solver. Fixed tensor rules alone miss narrow features; full closed-form elliptic
evaluation adds conditioning work without being necessary for initial delivery.
These are engineering trade-offs, not an already approved algorithm.

Use source-local cylindrical coordinates `(r,z)`, reconstruct world vectors with
radial/axial basis; wire uses the **azimuthal** basis. At r=0 use exact symmetry
and analytic axial formulas. Do not snap merely small r to zero.

- **Scaling:** use max-scaled hypot, half-angle distances
  `(r-R)² + 4rR sin²(φ/2) + z² + a²` in normalized coordinates and stable
  reciprocal/root differences. Scale by a power of two near the largest relevant
  local length. Carry strength and inverse-length factors as mantissa/exponent
  pairs until final multiplication (E scales Q/length²; finite-current fields
  and wire scale I/length). Avoid premature `N*I` overflow. A nonrepresentable
  displacement, lost positive normalized a, or nonfinite reconstruction gives
  NumericRange, never a fabricated zero. Genuine rounded underflow is permitted
  with an uncertainty contribution and flag; representable subnormals survive.
- **Finite line:** integrate `(Q/L)*(r,z-t)/(r²+(z-t)²+a²)^(3/2)` over the segment,
  with closest projected t and endpoint resolution guards. Its independent
  analytic reference, with `c=r²+a²`, `u=z+L/2`, `v=z-L/2`, is
  `Er=Qr/(Lc)[u/sqrt(c+u²)-v/sqrt(c+v²)]` and
  `Ez=Q/L[1/sqrt(c+v²)-1/sqrt(c+u²)]`. Rationalize differences when endpoints
  have the same sign; use direct sums when opposite signs. Propagate arithmetic
  sensitivity; a finite final value alone does not establish successful math.
- **Ring/loop near axis:** set `A=R²+z²+a²+r²`, `δ=2rR/A`. For `δ<=1/8`, expand
  `(1-δ cosφ)^(-3/2)` using binomial coefficients; angular moments are zero for
  odd powers and `C(2k,k)/4^k` for even powers. Let S0 be its angular average,
  S1 its cosine-weighted average. Ring components are
  `Q/A^(3/2) * (r*S0-R*S1, z*S0)`; loop components are
  `2πIR/A^(3/2) * (z*S1, R*S0-r*S1)`.
  This has no division by r. Bound the remaining positive series using the
  next coefficient and a geometric upper ratio (<= `2δ` after the first term),
  transform that tail into absolute component uncertainty. Stop at the requested
  family absolute allowance divided by `4*max(1, family_source_count)` or 64
  terms initially. Retain the series state/tail as a refinable contribution;
  the global family test can demand more terms, up to that cap. At
  larger δ integrate the direct charge/current elements over symmetric angles.
  *Implementation note (Task 5):* the direct angular domain is `[-π, π]`, which
  is exactly equivalent to `[0, 2π]` for these even, periodic kernels.
- **Disc:** put `x=R sinθ`, `H=R cosθ`, `C=(r-x)²+z²+a²`.
  The exact y-integrated vector is
  `Q/(πR²) * (r-x,z) * 2H/[C sqrt(C+H²)]`, times `dx=R cosθ dθ`,
  `θ∈[-π/2,π/2]`. Pair ±θ contributions with compensated sums; exact axis uses
  `Ez=2Qz/R²[1/sqrt(z²+a²)-1/sqrt(R²+z²+a²)]`, rationalized.
- **Sphere center-stable reduction:** let query radius be s, source radius R,
  `A=s²+R²+a²`, `B=2sR`, `d₋=sqrt(A-Bu)`, `d₊=sqrt(A+Bu)`. The radial field is
  `3Q/(2R) * ∫[0,1] 2B u² / (d₋ d₊ (d₋ + d₊)) du`.
  This follows from `ρ∮n/|s-Rn|_a dS`, not planar rings. The integrand is the
  half-range identity `u(1/d₋ - 1/d₊) = 2B u²/(d₋ d₊ (d₋ + d₊))`.
  Form `d₋²=(s-R)²+a²+2sR(1-u)` to avoid cancellation. At s=0 return exact zero;
  near center this retains the linear field rather than subtracting primitives.
  Use `u=cosθ` if needed for the surface-resolution guard.
- **Solenoid:** for angle φ set `C=(r-R)²+4rR sin²(φ/2)+a²`,
  `u=z+L/2`, `v=z-L/2`. Integrate
  `NI R/L * (cosφ[1/sqrt(C+v²)-1/sqrt(C+u²)],
  (R-r cosφ)/C * [u/sqrt(C+u²)-v/sqrt(C+v²)])` over `[0,2π]`.
  Rationalize the differences and form `R-r cosφ=(R-r)+2r sin²(φ/2)`.
  On axis use the same analytic finite-solenoid expression. No nested numerical
  inner error is hidden in an outer estimator.

### Resolution and global uncertainty

Every integral first covers its entire domain with finite cells. Seed projected
nearest-support locations, endpoints and geometric-distance splits, including
displaced features. A cell is resolved only when its physical parameter width
is <= half its minimum softened distance to the relevant support patch. Use
`sqrt(a²+d_min²)` with a conservative lower bound on distance: segment interval
for the finite line (width Δt), ring/surface arc
patches for ring/loop/sphere, cylinder strip for solenoid; for disc use
`sqrt(a²+z²+dist(r,[x_lo,x_hi])²)` and width `R*Δθ`. Arc width is R*Δangle.
This is a deliberately conservative **resolution guard**, not an error proof.
Subdivide failing cells before classifying success; exhausted unresolved cells
produce Unavailable(UnresolvedFeature), even if two rules agree.

For each resolved leaf compare GK31 to G15 **and** to GK31 on its two halves;
use the half-cell estimate and `8*max(norm(K31-G15), norm(K31-half_sum))` as
quadrature indicator. Keep weighted absolute primitive magnitudes, series tails,
and a `64*f64::EPSILON` times those magnitudes roundoff allowance; include
frame/coordinate sensitivity and primitive subtraction conditioning explicitly.
If these cannot be represented/accounted for, refuse. These empirical allowances
are not interval arithmetic/certified bounds; independent regression tests can
still discover missed uncertainty. Review suspicious TargetMet classifications
as bugs, not merely tune the indicator to pass.

Accumulate all **absolute** uncertainty contributions (quadrature, series,
primitive/transform rounding and Neumaier summation rounding) into U per family;
do not sum local relative allowances. Keep complete source estimates and a
compensated family sum V. Refine the leaf with largest absolute uncertainty
(stable source-index/domain tie break) until the family criterion passes or a
limit/roundoff floor stops progress. Analytic contributions also carry modeled
roundoff. Changing traversal order must not change reserved E/B resources.

For family f the exact requested criterion is
`T_f = epsilon*S_f + epsilon*norm(V_f)` and `U_f <= T_f`, using overflow-safe
norm/comparison. Because `T_f` depends on the current family estimate `V_f`,
re-evaluate the criterion with the terminal post-refinement `V_f` before
classifying the final status. `S_E` and `S_B` are **separate positive field-magnitude scales**
in their respective display-only units, caller supplied, independent of source
strength and evaluation order. Thus absolute tolerance is `epsilon*S_f` and
relative tolerance is epsilon. Near cancellation use the resulting family norm,
not sum-of-source norms; the absolute term remains nonzero at V=0. Track sum of
absolute contributions for conditioning diagnostics, never as a replacement
relative denominator. Reject nonfinite/unrepresentable tolerance arithmetic.

## Provisional defaults, limits and publication

All values are centralized named defaults, replaceable by explicit options and
changeable after tests/review; none is an accepted universal input envelope.

| Proposal | Initial value | Rationale |
| --- | --- | --- |
| epsilon | `1e-7` | Accuracy-first initial request with room above f64 noise; API accepts every finite positive value, including tighter requests that may fail. |
| E/B absolute scales | `1e-2` each, separately stored | Gives `1e-9 + 1e-7*norm(V)` initially, familiar from evidence, now explicitly family/global and unit-specific. |
| global softening | `0.1` world-length | Useful unit-size illustrative starting point, not size-independent physics; API accepts any positive finite a. |
| source cap | 64 total | Finite preparation/family aggregation; no per-kind quotas. |
| work | 2,000,000 units per family per point | Generous safety ceiling, not latency promise; one kernel sample, series term or analytic contribution costs >=1; all parent/child probes count. |
| resident cells | 8,192 per family | Fixed scratch bound, including source initialization and pending splits. |
| subdivision depth | 48 | Bounds degenerate subdivision; midpoint must make representable progress. |
| points per evaluator job | 65,536 | Count evaluate calls, including invalid queries; a new job is explicit, not silent renewal. |

`WorkLimits` exposes `max_sources: usize`, `work_per_family: u64`,
`cells_per_family: usize`, `max_depth: u8`, `max_queries: u64` with these defaults;
reject zero, checked-size overflow and requests above defaults (these are hard
ceilings in v1; callers can lower them). Precheck lengths and checked byte counts,
poll cancellation **before** cloning sources/reserving scratch, use fallible
allocation. Two separate equal family reservations, no shared depletion or
borrowing; a query costs at most 4,000,000 units plus bounded source validation.
E-first and B-first have identical family limits. Empty families are exact zero
without quadrature. Count all setup/split work by bounded loops and poll at each
source/cell and at most every 32 work units. No recursion or unbounded split Vec.
Record actual scratch bytes/high-water marks and native elapsed timings during
implementation; measurements are observability, not a new benchmark admission
gate. No initial parallelism, caching or optimization campaign.

Publish TargetMet only after all sources in that family are completely covered,
resolved and finite and the global criterion passes. AccuracyNotMet requires
the same complete resolved coverage and finite uncertainty but missed target:
e.g. refinement cap reached after retaining a complete coarse partition. If a
source is skipped, a feature unresolved, arithmetic fails, or cancellation is
observed, publish no vector for that family. Keep any already completed other
family. A failure in one family never prevents attempting the other, except
global cancellation/query rejection. No partial integral masquerades as a field.

Downstream: inaccurate drawing is explicit default-off, finite complete
AccuracyNotMet only; caps never change. Label “accuracy not met” in viewport and
export image/available metadata. While recomputing, retain previous plot visibly
“outdated—recomputing”, cancellation enabled, normal stale export blocked. #25/#26
own scheduling, generation identity, stale UI and export; #24 provides immutable
options/provenance/status, not UI. Downstream f32 conversion must check range and
add conversion uncertainty/reclassify rather than silently inherit TargetMet.

## Verification and approval boundary

Tests are deterministic pure CPU integration tests, under `tests/` registered in
the suite. Every geometry gets softened analytic anchors **and** off-axis
independent references. For the new disc/solenoid reductions, the old scratch
chord/axial references are now the same decomposition and **not independent**:
use polar ring integration/direct source elements instead. Commit a small
resolved fixture set with exact binary64 inputs, reference method, precision,
successive convergence, values, uncertainty and generator provenance. Runtime
tests read committed fixtures only; no SciPy/mpmath/install requirement. An
optional offline generator may use already available high precision, using true
`mp.pi`, but creates no new research phase. Preserve the known narrow-line,
near-axis, near-center and `1e103`/`1e154` arithmetic regressions.

Check tighter/looser epsilon on selected independently resolved cases: tighter
targets give smaller measured errors until the reference/roundoff floor and
usually require more work; do not promise universal monotonic measured errors
or TargetMet at arbitrary epsilon. Cancellation-heavy mixed totals test global
absolute/relative semantics and source permutations. Include scales, signs,
frames, on-support/interior, resource/preallocation and reversed E/B order.

Use current [Workflow](../../../references/workflow.md) gates and initial
independent numerical/design review. Dedicated web/wasm and manual visual checks
remain **waived, not passed**; pure CPU tests are not browser/GPU proof. No new
owner-only numerical question is identified: review this proposed design and
its provisional engineering choices, then authorize implementation separately.
Scheduling stays finish/merge #14 then owner-requested break; this docs-only
planning exception does not dispatch concurrent physics implementation.
