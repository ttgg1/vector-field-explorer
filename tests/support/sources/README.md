# Electric source fixture provenance

`fixtures.csv` is the committed, independently resolved reference matrix for the
Task 3 electric geometries: point charge, finite line charge, charged ring,
charged thin disc and uniform solid sphere. Runtime tests read this file through
`tests/support/sources/mod.rs`; they never invoke Python, mpmath, SciPy, or
production source code to build a reference.

## Encoding

- Every input column ending in `_bits` is the exact binary64 bit pattern as 16
  hexadecimal digits (`f64::from_bits`). This records the exact inputs rather
  than a rounded decimal spelling.
- `charge_bits` is the total source strength. `dimension_bits` is the line
  length or the radius of a ring/disc/sphere and is zero for a point.
- `ref_x`, `ref_y`, `ref_z` are correctly rounded binary64 renderings of the
  high-precision reference components in the world frame (`axis = +Z`,
  `center = 0` for every committed row).
- `uncertainty` is the larger of the observed change between the two reference
  precision settings and `1e-300`.
- `method` names the independent reference reduction.

## Generator and methods

`generate.py` is offline provenance, not a test or runtime dependency. It uses
`mpmath` 1.4.1 with true `mp.pi` at 40 and 70 decimal digits, converts the exact
binary64 inputs, and rejects any row whose change exceeds
`1e-12 + 1e-10 * |reference|`. All committed rows converged far below that
contract (the largest observed change is `3.3e-32` for the exact-cancellation
ring row).

| Kind | Independent reference method |
| --- | --- |
| point | Closed-form `Q s / (|s|^2 + a^2)^(3/2)`. |
| line | Design endpoint primitive with rationalized same-sign differences. |
| ring | Complete elliptic `K`/`E` for `S0 = <(1 - delta cos phi)^(-3/2)>` and `S1 = <cos phi (1 - delta cos phi)^(-3/2)>`; `Er = Q/A^(3/2) (r S0 - R S1)`, `Ez = Q/A^(3/2) z S0`. |
| disc | Polar ring integration: `Q/R^2 * integral_0^(R^2) ring(t) dt`, `t = rho^2`, using the elliptic ring formula. This is not the production chord decomposition. |
| sphere | Spherical-shell volume integration of the physical volume measure `3Q/(4 pi R^3)`, with the shell polar integral evaluated analytically. This is not the production divergence-theorem surface reduction. |

The matrix spans on-axis (`r = 0`), near-axis (`r = 1e-10`), near-center
(`r = 1e-8`), off-axis (`r = 0.4`), support/near-support, line ends, `z = 0`
and `z = 0.7`, unit and non-unit radii/lengths, softening `0.1`, `0.001` and
selected `1e-6`, and signed strengths. `ring_delta_*` rows pin
`delta = 1/8 * {0.99, 1.00, 1.01}` for the near-axis-series/direct angular
branch sweep.

## Retained limitations

- The `1e-6` rows use far queries so the resolution guard can resolve the
  softening scale inside the default cell budget. They are not on-support
  narrow-feature claims.
- The reference uncertainty is a convergence change between two decimal
  precisions, not a certified interval bound.
- `ring_near_axis` intentionally exposes radial cancellation at the `1e-27`
  level; its committed uncertainty follows the same contract.
