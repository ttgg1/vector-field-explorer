# Finite-line reference provenance

`tests/support/sources/mod.rs` contains an independent implementation of the
approved design's analytic finite-line endpoint primitive. It is used only as a
test oracle; production finite-line evaluation uses direct segment integration
(and the analytic primitive only for exact on-axis queries).

## Primitive

With `c = r² + a²`, `u = z + L/2`, `v = z - L/2`:

```text
Er = Q r / (L c) * [u / sqrt(c + u²) - v / sqrt(c + v²)]
Ez = Q / L       * [1 / sqrt(c + v²) - 1 / sqrt(c + u²)]
```

For far queries whose endpoint parameters share a sign, both raw differences
cancel. The reference rationalizes:

```text
1/a - 1/b = 2 z L / (a b (a + b))
u/b - v/a = c (u² - v²) / (a b (u a + v b))
```

where `a = sqrt(c + v²)` and `b = sqrt(c + u²)`. The opposite-sign branch is a
direct stable sum.

## Committed values

`FAR_SAME_SIGN_REFERENCES` was generated on 2026-11-16 with `mpmath` 1.4.1 at 60
decimal digits, using the formulas above and true binary64 inputs. The stored
`er`/`ez` are the correctly rounded f64 results.

## Retained historical miss

An earlier unsplit fixed-rule experiment evaluated the narrow displaced line
(`L = 2`, `a = 1e-6`, `Q = 2a`, `r = a`) as `4.639761e-10` where the resolved
value is near `1`. That number is retained as provenance for why the resolution
guard and half-rule comparison exist. It is **not** the new solver's expected
value and must not be used as an assertion target.
