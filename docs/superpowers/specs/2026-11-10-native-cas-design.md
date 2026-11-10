# Native CAS design — 2026-11-10

## Intent and accepted scope

Provide a native-only symbolic differentiation/derived-field workflow for
people exploring vector-field formulas. The owner selected `exmex` 0.21.0 with
`partial`, deferred CAS wasm support, and approved a panel with scalar/vector
input, operation selection, result preview/copy, and explicit Apply for vector
results. Failed operations leave the current field untouched. Results are not
automatically applied and history is not persisted.

The accepted operators are differentiation with respect to x, y, z, or t;
gradient; divergence; and curl. In 2D, gradient has x/y components, divergence
is scalar, and curl is the scalar `dFy/dx - dFx/dy`. In 3D, curl is a vector.
`meval` remains the only evaluator and the existing field evaluator is
unchanged. Unsupported symbolic functions produce a clear error; there is no
numeric-difference fallback.

## Approaches

- Use `exmex` as an additive transform adapter; selected by the owner. It
  requires a native-only dependency/module and a strict output compatibility
  boundary with `meval`.
- Implement differentiation over meval tokens/RPN; avoids a dependency but
  requires maintaining a symbolic parser/printer.
- Use `symcas`; not selected. Its smaller sampled wasm wrapper is irrelevant
  to the native-only decision, and its sampled meval interoperability remains
  unverified.

## Boundaries and constraints

- CAS dependency, module, and every caller are excluded from wasm via the
  existing native target-dependency and `cfg(not(target_arch = "wasm32"))`
  conventions. The app's web build and `meval` path remain intact. Do not make
  a wasm CAS build/size test a gate for this work.
- Preserve formula evaluation, parsing, sampling, and error behavior outside
  this additive transformation feature. Reparse every emitted result with
  meval before exposing it; any unsupported syntax/function or invalid result
  fails closed.
- The transformation adapter owns variable-name mapping explicitly; never
  assume exmex variable ordering equals x/y/z/t. An absent differentiation
  variable yields zero.
- Keep exmex AST, parser, and printer details private. No solve, integration,
  step-by-step algebra, automatic application, saved history, or persistence.
- The native panel is separate from ticket 71's active UI ownership. Its UI
  integration starts after ticket 71 merges and is file-disjoint from its
  changed files.

## Grammar boundary and unresolved acceptance

The meval 0.2.0 tokenizer source establishes numbers (decimal with optional
exponent), identifiers (ASCII letter/underscore followed by letters, digits,
or underscores), parentheses, commas, unary `+`/`-`, and binary `+ - * / % ^`.
Its default context defines constants `pi`, `e`; unary functions `sqrt`,
`exp`, `ln`, `abs`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `sinh`,
`cosh`, `tanh`, `asinh`, `acosh`, `atanh`, `floor`, `ceil`, `round`,
`signum`; binary `atan2`; and variadic `max`, `min`. This is the evaluator
catalog, not a promise that every entry has a symbolic derivative rule.

The first adapter's exact supported symbolic function subset and canonical
printer policy remain a builder-review gate: exmex probes only establish
sampled interoperability, not complete grammar coverage. The adapter ticket
must explicitly name the supported function subset, reject the rest with a
typed/clear error, cover precedence/associativity and reparse all output with
meval. Do not silently widen this boundary based on incidental parser support.

## UI sketch

```text
Symbolic tools (native)
Input:     [ scalar or vector formula                         ]
Operation: [ Differentiate / Gradient / Divergence / Curl     ]
Variable:  [ x / y / z / t ]      Dimension: [ 2D / 3D ]
Result:    (read-only formula preview)                 [Copy]
                                                     [Apply vector]
Error:     shown inline; the plotted field remains unchanged
```

Exact control layout, label wording, and safe disabled states are the panel
ticket's design call, provided the accepted interaction and fail-closed
behavior above remain unchanged.

## Ticket decomposition

1. **Adapter and tests (ticket 73):** native-only `exmex` adapter, vector
   operators, meval-compatible output and pure integration tests. Can run
   alongside source-engine work because its files are disjoint; it is not a
   prerequisite for Phase 1 sources. It must be complete before Phase 3
   induction consumes CAS.
2. **Native panel and integration (ticket 75):** depends on 73 and completion
   of active ticket 71; add the approved user path without touching ticket
   71-owned UI files until that work is merged. No persistence changes.

Both tickets are preparation for future implementation and remain blocked on
independent builder review of this spec/plan and confirmation of the adapter's
exact function/printer boundary. No implementation starts under this design
task.

## Sources

- Owner rulings and current status: `.superpowers/sdd/2026-11-05-vector-field-explorer/progress.md`,
  final dated section, 2026-11-10.
- Roadmap: `.scratch/field-viewport/issues/55-physics-roadmap.md`, CAS track.
- Engine research: `docs/research/2026-11-10-cas-selection.md` (sampling
  evidence only; selection is superseded by the owner ruling above).
- Evaluator grammar/functions: `meval` 0.2.0 `src/tokenizer.rs` and
  `src/expr.rs`, locally available Cargo registry source.
- Native/UI ownership: ticket 71 follow-up ruling in the SDD ledger.
