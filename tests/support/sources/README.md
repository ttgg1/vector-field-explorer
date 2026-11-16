# Source fixture provenance

`fixtures.csv` is the committed, independently resolved reference matrix for the
Task 3 electric geometries (point charge, finite line charge, charged ring,
charged thin disc and uniform solid sphere) and the Task 4 magnetic geometries
(current loop, finite continuous solenoid and infinite straight wire). Runtime
tests read this file through `tests/support/sources/mod.rs`; they never invoke
Python, mpmath, SciPy, or production source code to build a reference.

## Encoding

- Every input column ending in `_bits` is the exact binary64 bit pattern as 16
  hexadecimal digits (`f64::from_bits`). This records the exact inputs rather
  than a rounded decimal spelling.
- `charge_bits` is the total source strength: signed electric charge or signed
  current. `dimension_bits` is the line length or the radius of a ring, disc,
  sphere or current loop; it is zero for a point and for a wire.
- `length_bits` is the solenoid length and is zero for every other kind.
  `turns` is the exact integer turn count and is zero for every other kind.
- `ref_x`, `ref_y`, `ref_z` are correctly rounded binary64 renderings of the
  high-precision reference components in the world frame (`axis = +Z`,
  `center = 0` for every committed row). Electromagnetic loop/solenoid rows
  store `(B_r, 0, B_z)`; the infinite-wire rows store `(0, B_phi, 0)`.
- `uncertainty` is the larger of the observed change between the two reference
  precision settings and `1e-300`.
- `method` names the independent reference reduction. `current-element+elliptic`
  records that the committed current-element result was cross-checked against
  the independent complete-elliptic loop formula. `loop-over-z` is the
  solenoid's independent axial integration.

## Generator and methods

`generate.py` is offline provenance, not a test or runtime dependency. It uses
`mpmath` 1.4.1 with true `mp.pi` at 40 and 70 decimal digits, converts the exact
binary64 inputs, and rejects any row whose change exceeds
`1e-12 + 1e-10 * |reference|`. All committed rows converged far below that
contract (the largest observed change is `1.5e-31` for the near-axis loop
radial term; the existing exact-cancellation ring row is `3.3e-32`). Loop rows
additionally cross-check the direct current-element integral against the
elliptic reduction at the higher precision.

| Kind | Independent reference method |
| --- | --- |
| point | Closed-form `Q s / (|s|^2 + a^2)^(3/2)`. |
| line | Design endpoint primitive with rationalized same-sign differences. |
| ring | Complete elliptic `K`/`E` for `S0 = <(1 - delta cos phi)^(-3/2)>` and `S1 = <cos phi (1 - delta cos phi)^(-3/2)>`; `Er = Q/A^(3/2) (r S0 - R S1)`, `Ez = Q/A^(3/2) z S0`. |
| disc | Polar ring integration: `Q/R^2 * integral_0^(R^2) ring(t) dt`, `t = rho^2`, using the elliptic ring formula. This is not the production chord decomposition. |
| sphere | Spherical-shell volume integration of the physical volume measure `3Q/(4 pi R^3)`, with the shell polar integral evaluated analytically. This is not the production divergence-theorem surface reduction. |
| loop | Direct current-element angular integration `I R integral e_phi x s / (|s|^2 + a^2)^(3/2) dphi`, cross-checked against the independent elliptic `S0`/`S1` reduction. Near-axis and `delta` rows commit the elliptic value. |
| solenoid | Loop-over-z integration: each axial slice is a loop of current `N I dz/L` using the elliptic loop field, with the integration split at the projected query location. This is not the production analytic axial reduction. |
| wire | Direct infinite-wire integral after the substitution `z' = sqrt(r^2 + a^2) tan(theta)`, evaluated numerically. |

The matrix spans on-axis (`r = 0`), near-axis (`r = 1e-10` electric ring and
magnetic loop, `r = 1e-8` solenoid), near-center (`r = 1e-8` sphere), off-axis
(`r = 0.4`), support/near-support, line ends, `z = 0` and `z = 0.7`, unit and
non-unit radii/lengths, softening `0.1`, `0.001` and selected `1e-6`, and signed
strengths. `ring_delta_*` and `loop_delta_*` rows pin
`delta = 1/8 * {0.99, 1.00, 1.01}` for the near-axis-series/direct angular
branch sweep. `sphere_near_surface_tiny` additionally pins a near-surface
small-softening case (`s = 1 + 1e-9`, `R = 1`, `a = 1e-6`) where the small
`(s - R)` term is much smaller than `a`; it must be evaluated with the
cancellation-safe `(s - R)^2 + a^2 + 2sR(1 - u)` form rather than `A - B u`.

The magnetic coverage includes ordinary `z = 0.2` loop rows (so the pinned
radial near-axis term is not the only evidence), a negative world-x near-axis
loop row (`loop_near_axis_neg_x`) that exercises the flipped radial direction
with the same reference and uncertainty contract, loop/solenoid on-winding and
solenoid end rows at ordinary softening, interior and exterior solenoid rows,
`N = 3` and signed current, non-unit radius/length, and far `a = 1e-6` rows.

## Retained limitations

- The `1e-6` rows use far queries so the resolution guard can resolve the
  softening scale inside the default cell budget. They are not on-support
  narrow-feature claims.
- The reference uncertainty is a convergence change between two decimal
  precisions, not a certified interval bound.
- `ring_near_axis` intentionally exposes radial cancellation at the `1e-27`
  level; its committed uncertainty follows the same contract.
