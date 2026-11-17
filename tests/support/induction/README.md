# Induction vector-potential fixtures

These CSV rows pin the unit-current softened azimuthal vector potential
`A_phi(r, z)` for the two Task 1 induction cases:

- `loop.csv`: radius `1.0`, line current.
- `solenoid.csv`: radius `0.5`, length `2.0`, `8` turns, continuous surface
  current with unit current per turn.

Each row records `case, r, z, softening, reference`, where `reference` is
`A_phi` for unit current. The Rust tests compare the production Gauss–Legendre
quadrature against these values with a `1e-9 * max(1, |reference|)` tolerance.

## Method independence

`generate.py` evaluates the closed softened integrals from the approved design
directly with `mpmath`:

```
A_phi = I R integral_0^{2pi} cos(phi) / D_loop dphi
A_phi = (N I / L) R integral_0^{2pi} cos(phi) *
        [asinh((z + L/2)/D_solenoid) - asinh((z - L/2)/D_solenoid)] dphi
D_loop     = sqrt(r^2 + R^2 - 2 r R cos(phi) + z^2 + a^2)
D_solenoid = sqrt(r^2 + R^2 - 2 r R cos(phi) + a^2)
```

The solenoid's axial integral is analytic, so its `D` deliberately excludes
`z`; only the loop keeps `z^2` inside the softened distance. This is the exact
form in the approved design.

It uses `mp.pi` and `mpmath.quad`, not the Rust quadrature, softening helper or
source evaluator. Each row is evaluated at 40 and 70 decimal digits and the
generator refuses to write a row whose two results differ by more than
`1e-12 + 1e-10 * |reference|`.

## Reproduce

```bash
python3 tests/support/induction/generate.py
```

`mpmath` is a generation-time tool only. There is no Python, mpmath or other new
runtime/Rust dependency.
