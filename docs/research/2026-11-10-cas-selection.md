# CAS engine selection spike — 2026-11-10

## Result

**Recommendation: proceed to a tightly scoped in-house differentiator over `meval`'s public tokenizer/RPN, after a dedicated implementation ticket and tests. Do not select/adopt an engine from this spike.** The first candidate, `exmex` 0.21.0, differentiated real expressions, but its printed expression dialect is not `meval`-compatible: the observed derivative uses `{y}` and `{x}` while the app grammar takes bare variable identifiers. Its parser/variable behavior and extra evaluator surface also create an adapter burden contrary to the accepted one-grammar boundary. `symcas` 0.1.1 offers more algebra than the product needs, parses/prints its own “plain” grammar, and could not be proved to roundtrip to `meval` grammar here. An in-house AST/RPN rewrite can preserve `meval` as the only evaluator and tightly constrain simplification/output, but that is an engineering recommendation, not a demonstrated implementation or cost measurement.

No product dependency, manifest, lockfile, or source was changed. This is a recommendation-only spike, not adoption or completion of the CAS track.

## Scope and constraints

Ticket 55 requires x/y/z/t derivatives, gradient/divergence/curl, minimal simplification, `meval`-grammar output, finite-difference and vector-identity checks, and wasm build/size evidence. It explicitly excludes solve/integrate/step-by-step algebra and keeps `meval` as evaluator. GPL-3.0 compatibility and wasm cleanliness are gates; no hard size budget exists. Source: `.scratch/field-viewport/issues/55-physics-roadmap.md` (accepted epic; CAS track, especially lines 113–140), ADR 0003, and the physics glossary. Symbolica remains rejected and was not reconsidered.

## Candidate evidence

### exmex

- Exact crate: [`exmex` 0.21.0](https://crates.io/crates/exmex/0.21.0); upstream [repository](https://github.com/bertiqwerty/exmex/), [docs](https://docs.rs/exmex/0.21.0/). Crate metadata says `MIT OR Apache-2.0`, Rust 1.80.1; source manifest enables `partial` independently and has a direct `regex = 1.12.3` requirement (Cargo resolved regex 1.13.1 here). This dual license is compatible with GPL-3.0-only. Primary sources: crates.io metadata and the downloaded [0.21.0 manifest](https://github.com/bertiqwerty/exmex/blob/v0.21.0/Cargo.toml).
- Actual host experiment: enabled `partial`, parsed `sin(x)*y + z^2 + t`, called `FlatEx::<f64>::partial(index)`, and compiled/ran successfully in release mode. Output for tested indices included `1.0`, `{y}*cos({x})`, `sin({x})`, `{z}*2.0`; this demonstrates differentiation, but also surfaces an output dialect mismatch. This tiny probe did not verify reliable x/y/z/t index mapping, equivalence against meval, composition, simplification policy, or identities. `Differentiate` is publicly re-exported from the prelude; the optional feature is explicitly required.
- The upstream README describes exmex as both evaluator and differentiator; the product would need to use only its transformation facilities and evaluate emitted strings through meval. The observed braces mean output cannot be fed to meval unchanged. A robust output adapter/grammar contract would be required.
- The accepted epic’s prior “roughly 1.5–1.7 MB regex” claim was not reproduced as binary-size evidence in this spike. A measured wasm size delta is **unavailable** (see wasm below), so do not treat that estimate as a result.

### symcas

- Exact crate: [`symcas` 0.1.1](https://crates.io/crates/symcas/0.1.1); upstream [repository](https://github.com/cislunarspace/cas), [docs](https://docs.rs/symcas/0.1.1/). Crates.io reports `MIT OR Apache-2.0`, Rust 1.85. Its README’s license section confirms either MIT or Apache-2.0. The facade is split across `symcas`, `cas-domain`, `cas-expr`, `cas-print`, and `cas-parse` crates.
- Upstream README documents `Context::diff` and deterministic `plain` formatting/roundtrip within its own parser. It also advertises exact rational arithmetic, polynomial GCD/factorization, assumptions, Taylor series, and simplification—substantially beyond the required minimal differentiation slice.
- It has its own symbolic expression grammar and printer. Compatibility with `meval` grammar, the accepted function/operator subset, x/y/z/t composition, and evaluator equivalence were **not tested**. Treating its `plain` roundtrip as proof of meval compatibility would be incorrect. We did not build or size it.
- It is young (0.1.1); the crate metadata’s Rust 1.85 floor satisfies the local toolchain but should be checked against project MSRV before any future consideration. No conclusion about wasm readiness was measured.

### in-house over meval public tokens/RPN

- The exact current evaluator crate is [`meval` 0.2.0](https://crates.io/crates/meval/0.2.0), [repository](https://github.com/rekka/meval). Its published source exposes `tokenizer::Token` and public `tokenize`; token kinds include numbers, variables, functions with arity, unary/binary operators, parentheses, and commas. This is concrete evidence for a parser/token-level seam without private internals.
- No in-house prototype was implemented in this spike. An implementation would need to parse/tokenize into an owned expression tree (or use public RPN conversion if confirmed in the exact API), recursively differentiate, then print only validated meval syntax. It must explicitly test precedence/associativity, unary minus, function arities, implicit multiplication, constants, and unsupported functions/operators. Public tokens alone do not demonstrate that this is “easy” or correct.
- This route avoids adding a production CAS dependency and makes grammar equivalence structurally controllable. It still entails a new parser/AST/printer and a long-term maintenance burden. Prefer a minimal rewrite rule set and fail closed for unsupported differentiation rules; do not silently fall back to numerical derivatives.

## Required composition and verification status

Gradient, divergence and curl can be built by composing scalar derivatives of vector components; no candidate was wired through these operators. No finite-difference comparison, vector identity test, meval parse/evaluation equivalence test, or complete x/y/z/t variable mapping test was completed. Thus these requirements remain unproven for every candidate. Suggested follow-up acceptance includes:

1. scalar partials for x/y/z/t, constants/absent variables, powers and supported unary functions;
2. `meval::Expr` parse + evaluation of every emitted derivative, with deterministic output and finite-difference comparison on nonsingular sample points;
3. vector gradient/divergence/curl composition and identities (e.g. curl(grad f)=0, div(curl F)=0) on smooth expressions;
4. explicit failures for unsupported grammar/operators and numerical singularities;
5. regression cases for meval precedence and custom context/bound functions as applicable.

## wasm and size experiment

- Toolchain reports Rust 1.99.1 and installed-target listing initially showed `wasm32-unknown-unknown`; however compilation failed at dependency `fnv` with `E0463: can't find crate for std` (“target may not be installed”). `rustup target add wasm32-unknown-unknown` reported the component up to date, but the subsequent wasm build failed identically. Therefore **no candidate wasm build passed**.
- Attempted matched tiny release harnesses: baseline with `meval = 0.2` and candidate with `exmex = 0.21.0` + `partial`, both in this worktree’s isolated `target/`, `--target wasm32-unknown-unknown --release`. Baseline command failed before producing a wasm artifact; candidate wasm command was not run after baseline failure. No raw or gzip size exists. This is a blocker/limitation, not a zero delta.
- The harness would in any case measure only a small library wrapper, not full-app release wasm. Future results should report baseline/candidate harness raw and compressed deltas and separately label any full app artifact delta; no hard threshold should be invented.

## Recommendation and risks

Run the in-house direction as the next design/implementation ticket because it best matches the “same grammar, same evaluator, minimal transforms” contract while avoiding an unmeasured dependency tax. This does **not** mean that implementation is already selected for production. If a controlled in-house prototype reveals disproportionate parser complexity, revisit symcas with a narrowly defined meval-compatible printer/equivalence test before exmex; exmex’s observed braces and extra evaluator/regex cost are direct friction points. Do not decide on size from estimates alone.

Key risks: the meval token API may require reproducing parsing/precedence behavior; emitted syntax can change semantics if associativity/parentheses are wrong; symbol differentiation rules and domain restrictions need explicit boundaries; dependency-free does not mean maintenance-free. The largest immediate evidence gap is the unavailable wasm target/runtime setup, followed by absent algebraic/identity tests. API risk is moderate for in-house and high for translating either external engine’s grammar. No candidate has passed the complete CAS acceptance.

## Reproduction record

Executed from isolated worktree `/home/paul/.local/share/opencode/worktree/9d1d25/cas-selection-spike`, with all experiments under `.scratch/cas-spike/` and Cargo target at that worktree’s `target/`:

- `cargo search exmex --limit 5`; `cargo search symcas --limit 5`
- `cargo info exmex`; `cargo info symcas`; `cargo info cas-parse`; `cargo info cas-diff` (expected lookup failed: no crate named `cas-diff`)
- `CARGO_TARGET_DIR=$PWD/target cargo run --manifest-path .scratch/cas-spike/exmex/Cargo.toml --release` (after correcting an initial private-module import; successful differentiation probe)
- `cargo build --manifest-path .scratch/cas-spike/baseline/Cargo.toml --target wasm32-unknown-unknown --release` (failed: wasm `std` unavailable)
- `rustup target add wasm32-unknown-unknown` (reported up to date); repeated baseline build (same failure)
- `cargo build --manifest-path .scratch/cas-spike/exmex/Cargo.toml --target wasm32-unknown-unknown --release --lib`, size listing and gzip commands were chained after the failed baseline and therefore **did not run**.

Cargo emitted a future-incompatibility warning for transitive `nom 1.2.4` in the successful exmex host probe. No project build/test gates were run because production files were intentionally untouched.

## Sources

- Ticket 55 accepted epic: `.scratch/field-viewport/issues/55-physics-roadmap.md` (local project source).
- ADR 0003: `docs/adr/0003-physics-foundations.md`; vocabulary: `GLOSSARY.md`.
- exmex 0.21.0 crate metadata/source: https://crates.io/crates/exmex/0.21.0 and https://github.com/bertiqwerty/exmex/tree/v0.21.0
- symcas 0.1.1 crate metadata/source: https://crates.io/crates/symcas/0.1.1 and https://github.com/cislunarspace/cas/tree/v0.1.1
- meval 0.2.0 metadata/source: https://crates.io/crates/meval/0.2.0 and https://github.com/rekka/meval/tree/v0.2.0
