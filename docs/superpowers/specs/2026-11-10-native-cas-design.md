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
  this additive transformation feature. CAS validates its own bounded input
  and output; it does not change the application's ordinary evaluator.
- The transformation adapter owns variable-name mapping explicitly; never
  assume exmex variable ordering equals x/y/z/t. An absent differentiation
  variable yields zero.
- Keep exmex AST, parser, and printer details private. No solve, integration,
  step-by-step algebra, automatic application, saved history, or persistence.
- The native panel is separate from ticket 71's active UI ownership. Its UI
  integration starts after ticket 71 merges and is file-disjoint from its
  changed files.

## Bounded compatibility contract

The adapter is a strict semantic bridge, not a text converter. Meval 0.2.0
defines the input expression's meaning; exmex 0.21.0 performs differentiation;
the adapter reconstructs and validates results under meval semantics.

### Accepted scalar input and rejected forms

Parse with meval and validate its RPN before any constant-folding or
absent-variable shortcut. Accept only finite numeric literals; free variables
exactly `x`, `y`, `z`, `t`; constants `pi`, `e`; unary `+`/`-`; binary `+`,
`-`, `*`, `/`, `^`; parentheses; and exactly these one-argument functions:
`sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `sinh`, `cosh`, `tanh`,
`asinh`, `acosh`, `atanh`, `sqrt`, `exp`, `ln`.

Reject `%`; `abs`, `floor`, `ceil`, `round`, `signum`, `atan2`, `max`, `min`;
custom/unknown functions; wrong function arities; free identifiers other than
the four variables and two constants; non-finite numeric literals; and
exmex-only syntax/aliases or braced user identifiers. Reject these even inside
a constant subexpression and even when differentiating with respect to an
absent variable. Such rejection is local to CAS; normal meval field evaluation
is unchanged.

### Input lowering and constants

Validate the complete meval token/RPN stream, then reconstruct a fully
parenthesized expression preserving meval's parsed tree before giving it to
exmex. Translate `pi` and `e` constants to deterministic finite f64 literals
before parsing with exmex; never pass lowercase `pi` as an exmex variable.
Resolve the requested variable by name. Only after complete validation may a
missing differentiation variable return zero. Use strict differentiation;
do not enable relaxed missing-operator behavior.

This boundary is necessary because raw parser precedence differs: e.g. meval's
`-x^2` means `-(x^2)`, while exmex raw parsing can mean `(-x)^2`; powers also
differ in associativity, and `*`/`/` and `+`/`-` priorities differ. Input
lowering must cover `-x^2` versus `(-x)^2`, `x^y^z` versus `(x^y)^z`,
`x/y*z`, `x*y/z`, `x-y+z`, nested powers/signs/functions, and negative numeric
bases.

### Output lowering and context validation

Do not globally strip braces or accept a meval parse as proof of semantic
equivalence. Parse exmex's output with a bounded lexer/parser accepting only
finite numeric literals, exact engine identifiers `{x}`, `{y}`, `{z}`, `{t}`,
the accepted arithmetic/sign operators, approved unary calls, and parentheses.
Interpret the resulting tree with exmex's actual operator semantics, then
emit fully parenthesized meval syntax. Strip braces only from exact validated
identifier tokens. Reject unexpected tokens, names, functions, non-finite
constants, or malformed structure. Use stable round-trip float formatting;
no algebraic-normal-form promise is made.

Reparse each emitted component with meval and bind it against the existing
default meval context plus x/y/z/t (or perform equivalent exhaustive
token/context checks). Reparse alone is insufficient because `Expr::from_str`
does not reject unknown names. Reject literal NaN/infinity introduced by
constant folding. Do not reject valid formulas merely because evaluation at
the origin is outside their real domain; this contract does not prove global
domain validity or guarantee domain-preserving cancellation.

### Acceptance evidence

Use finite differences against the original meval expression's meaning, not
against exmex's unadapted parse. Cover at least one analytic/finite-difference
case per approved function, x/y/z/t, constants and absent variables; all
precedence regressions above; positive-domain variable powers and negative-
domain integer powers; every rejected function/operator, unknown name and
malformed brace case; and deterministic repeated output. Test the complete
vector identities: all three components of `curl(grad f)` and `div(curl F)` on
nontrivial smooth fields, plus operation/component ordering. Native-gate the
new test module and imports so web builds stay compilable.

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

Exact control layout and labels are the panel ticket's design call. The panel
must tag scalar versus vector results explicitly; vector length alone does not
determine result kind (2D curl is one scalar component).

Tie each successful preview to the complete input, operation, variable, and
dimension request. Editing any item or switching app dimension invalidates or
recomputes the preview; stale results cannot be applied as the new request,
and failed recomputation disables Apply. Treat the current app dimension as
authoritative and recheck result arity at Apply. Validate the
whole vector formula before mutating app state; only then use the existing
state setter so formula synchronization and dirty flags remain correct. A
setter that mutates on parse failure is not the CAS validation boundary.
Scratch input, selection, preview, and errors remain transient. A vector
explicitly applied as the ordinary field may use existing field persistence;
do not add CAS history or persistence.

## Ticket decomposition

1. **Adapter and tests (ticket 73):** native-only `exmex` adapter, vector
   operators, compatibility bridges and math tests. Math implementation is
   independent of Phase 1 source-engine implementation, but `src/lib.rs` and
   `tests/suite.rs` registration overlap: serialize/exclusively assign those
   edits or land them one at a time. It is not a prerequisite for Phase 1
   sources. It must be complete before Phase 3 induction consumes CAS.
2. **Native panel and integration (ticket 75):** depends on 73 and completion
   of active ticket 71; add the approved user path without touching ticket
   71-owned UI files until that work is merged. No new CAS persistence.

Ticket 73 remains blocked pending independent rereview closure of the
compatibility contract recorded here and in the ticket. Ticket 75 remains
blocked on 73 and ticket 71. No implementation starts under this design task.

## Sources

- Owner rulings and current status: `.superpowers/sdd/2026-11-05-vector-field-explorer/progress.md`,
  final dated section, 2026-11-10.
- Roadmap: `.scratch/field-viewport/issues/55-physics-roadmap.md`, CAS track.
- Engine research: `docs/research/2026-11-10-cas-selection.md` (sampling
  evidence only; selection is superseded by the owner ruling above).
- Evaluator grammar/functions: `meval` 0.2.0 `src/tokenizer.rs` and
  `src/expr.rs`, locally available Cargo registry source.
- Derivative function table and parser/printer limits: `exmex` 0.21.0
  `src/expression/partial.rs`, `src/operators.rs`, and
  `src/expression/flat.rs`, locally available Cargo registry source.
- Native/UI ownership: ticket 71 follow-up ruling in the SDD ledger.
