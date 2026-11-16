# Source evaluator accuracy notes

`src/sources` is a pure CPU, native/Wasm-compatible evaluator for eight
softened source kinds. This page states what the public accuracy contract means,
which cases are tested, and the known limits. It is not an unlimited input
envelope guarantee.

The public seam is documented in [CODEBASE.md](CODEBASE.md#srcsources--validated-source-model-and-bounded-quadrature).
The independent fixture matrix and its provenance are in
[tests/support/sources/README.md](../tests/support/sources/README.md).

## Model

Every kind uses the Plummer kernel
`F_a(s) = s / (|s|^2 + a^2)^(3/2)` with one global softening length `a`. The
common Coulomb/Biot–Savart prefactors are omitted consistently inside each
family, so values are in family-local display units. Electric and magnetic
results are independent; a mixed set sums E and B separately.

| Family | Kinds |
| --- | --- |
| Electric | point charge, finite line charge, charged ring, charged disc, solid sphere |
| Magnetic | infinite straight wire, current loop, finite continuous solenoid |

Negative and zero strengths are valid. Zero-strength and empty families are
exact zero with zero arithmetic uncertainty. `a` is a teaching regularizer, not
a wire radius, disc thickness or material model; reducing epsilon refines the
same softened model rather than recovering ideal singular physics.

## Accuracy semantics

For one family with field scale `S` (independent of source strength) and the
compensated family sum `V`, the exact requested criterion is

```text
T = epsilon * S + epsilon * norm(V)
U <= T
```

`U` is the accumulated absolute uncertainty (quadrature indicator, series tail,
primitive/transform roundoff and compensated summation rounding). The relative
term uses the resulting family vector, not the sum of source magnitudes, so
near cancellation keeps the nonzero absolute term and does not inherit a
meaningless sum-of-|v_i| denominator.

- `TargetMet(Estimate)` means the estimated target was met after complete,
  resolved, finite coverage of all active sources in that family. It is never a
  certified bound.
- `AccuracyNotMet(Estimate, StopReason)` still carries a complete finite
  estimate and `U > T`; the reason names the work/storage/depth/roundoff floor
  that stopped further refinement.
- `Unavailable(StopReason)` carries no vector. Incomplete preparation, an
  unresolved feature, `NumericRange`, cancellation or query rejection never
  publishes a partial integral.
- Genuine representable underflow is a finite value with a diagnostic flag, not
  `NumericRange`. A nonrepresentable transformation is `NumericRange`.

Defaults: epsilon `1e-7`, E/B scales `1e-2` each, `a = 0.1`, 64 sources,
2,000,000 work units per family per point, 8,192 resident cells per family,
depth 48 and 65,536 evaluated points per evaluator job. Callers may lower but not
raise these v1 ceilings. Every call, including an invalid query, consumes one
hard query slot; the next call returns `Unavailable(QueryLimit)` for both
families.

## Tested cases

Runtime tests are deterministic and read committed fixtures only; no Python,
SciPy or new dependency is required.

- Committed independent references cover all eight kinds: softened analytic
  anchors, off-axis elliptic ring/loop values, polar-ring disc references,
  spherical-shell sphere references, endpoint-primitive line values,
  loop-over-axial-slice solenoid references and a direct infinite-wire integral.
  `tests/sources_evaluate_test.rs` asserts that every kind has at least one row
  and that each row meets the fixture tolerance.
- A `5 x 5 x 5` grid (including point centers and loop centers) sums independent
  closed-form point fields and an independent complete-elliptic loop reduction;
  every grid point is required to agree within
  `2e-14 + 2e-13 * norm(reference)`.
- Three source permutations of a large/small/opposite electric scene and a
  zero-residual pair check the family target formula, compensated summation and
  order stability. A cancellation-heavy family may honestly be
  `AccuracyNotMet` when its per-source modeled rounding exceeds a very tight
  target.
- Independent electric and magnetic budgets are checked by making the difficult
  family exceed a deliberately small cap in both source orders, while the cheap
  family remains `TargetMet`. Total work never exceeds the two family
  reservations.
- Requested epsilon `1e-4`, `1e-7`, `1e-10` is compared on ordinary disc and
  solenoid fixtures plus the `delta = 1/8 * 0.99` ring/loop branch cases. The
  ordinary disc/solenoid cases are already at the reference/f64 noise floor;
  the branch cases genuinely improve from roughly `1e-6`/`1e-5` error to
  `1e-13`, so no universal monotonicity is asserted.
- Extreme tight requests (`1e-30`) return either complete finite
  `AccuracyNotMet` or `Unavailable`; a one-cell scratch, a depth-limited narrow
  feature, NaN/Inf queries and cancellation never publish an estimate.

## Observed work and timing

Measurements below are from one native debug-profile test binary on the
integration worktree machine (epoch 2026-11-16). They are observability, not
latency thresholds or an admission envelope. Work counts every sample, series
term and split; `cells` is the per-family high-water mark.

| Case | epsilon | Work | Cells | `U` | Observed wall time |
| --- | --- | ---: | ---: | ---: | ---: |
| point, default | `1e-7` | 1 | 0 | `7.1e-14` | ~9 µs |
| `ring_delta_099` | `1e-4` | 4 | 0 | `2.1e-6` | ~2 µs |
| `ring_delta_099` | `1e-10` | 8 | 0 | `1.2e-13` | ~2 µs |
| `loop_delta_099` | `1e-4` | 4 | 0 | `1.6e-5` | ~13 µs |
| `loop_delta_099` | `1e-10` | 8 | 0 | `9.4e-13` | ~2 µs |
| `disc_offaxis_z0p7` | `1e-4` | 939 | 10 | `3.2e-14` | ~97 µs |
| `disc_offaxis_z0p7` | `1e-10` | 939 | 10 | `3.2e-14` | ~45 µs |
| `solenoid_end_effect` | `1e-4` | 12,031 | 128 | `1.3e-13` | ~1.0 ms |
| `solenoid_end_effect` | `1e-10` | 12,031 | 128 | `1.3e-13` | ~0.6 ms |

The evaluator's family refinement is source-granular in this delivery: the
complete integration for the source with the largest absolute allowance is
re-run at a tighter target, rather than retaining one resident cell heap across
all sources. This keeps the configured `cells_per_family` bound and the
no-allocation `evaluate` contract; all repeated work is counted, and the
terminal criterion is still the single family formula above. A leaf-global
resident partition remains an optimization opportunity, not a numerical
contract change.

## Known limitations

- Uncertainty is an empirical absolute allowance, not interval arithmetic or a
  certified error bound. Independent tests can still discover a missed
  uncertainty; a suspicious `TargetMet` is a bug, not a tuning opportunity.
- The `1e-6` fixture rows use far queries so the conservative resolution guard
  fits the default cell budget. On-support narrow features are not claimed; a
  depth/cell/work cap can return `Unavailable` instead.
- A cancellation-heavy family whose individual source rounding exceeds
  `epsilon*S + epsilon*norm(V)` cannot meet a tight target; it reports finite
  `AccuracyNotMet` honestly.
- Tight requests may hit the hard work/storage/depth ceilings. There is no
  draw-inaccurate flag that raises a cap, and no throughput or input-envelope
  guarantee.
- Native and Wasm builds share this pure CPU path. Dedicated browser/Wasm and
  manual visual checks remain waived for this seam; native unit/integration
  results are not browser proof.
