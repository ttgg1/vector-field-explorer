# CAS engine selection spike — 2026-11-10

## Result

**Conclusion: inconclusive shortlist; no candidate is selected.** Host checks now demonstrate useful differentiation for both exmex 0.21.0 and symcas 0.1.1, including meval parsing/evaluation of their outputs on tested formulas. A narrow lexical adapter removes exmex’s braces safely for the tested identifier set; brace output alone is not a disqualifier. However, neither candidate has complete grammar, numerical, and composition coverage, no in-house prototype was built, and measured wasm size differs substantially. These results are enough to retain both as options, not enough to select either. Do not create implementation work from this spike alone.

No product dependency, manifest, lockfile, or source was changed. This is a recommendation-only spike, not adoption or completion of the CAS track.

## Scope and constraints

Ticket 55 requires x/y/z/t derivatives, gradient/divergence/curl, minimal simplification, `meval`-grammar output, finite-difference and vector-identity checks, and wasm build/size evidence. It explicitly excludes solve/integrate/step-by-step algebra and keeps `meval` as evaluator. GPL-3.0 compatibility and wasm cleanliness are gates; no hard size budget exists. Source: `.scratch/field-viewport/issues/55-physics-roadmap.md` (accepted epic; CAS track, especially lines 113–140), ADR 0003, and the physics glossary. Symbolica remains rejected and was not reconsidered.

## Candidate evidence

### exmex

- Exact crate: [`exmex` 0.21.0](https://crates.io/crates/exmex/0.21.0); upstream [repository](https://github.com/bertiqwerty/exmex/), [docs](https://docs.rs/exmex/0.21.0/). Crate metadata says `MIT OR Apache-2.0`, Rust 1.80.1; source manifest enables `partial` independently and has a direct `regex = 1.12.3` requirement (Cargo resolved regex 1.13.1 here). This dual license is compatible with GPL-3.0-only. Primary sources: crates.io metadata and the downloaded [0.21.0 manifest](https://github.com/bertiqwerty/exmex/blob/v0.21.0/Cargo.toml).
- Host mapping probe: `x + 2*y + 3*z + 5*t` reports variables in order `["t", "x", "y", "z"]`, not the product x/y/z/t order. Looking up each name in the exposed `var_names()` then differentiating by the resulting index produced `dx=1.0`, `dy=2.0`, `dz=3.0`, `dt=5.0`; explicit lookup is reliable for this case, positional assumptions are not. `Differentiate` is publicly re-exported; feature `partial` is required.
- Tested an adapter that strips braces only around identifiers in `{x,y,z,t}`, rejects any other braced name, and then parses/evaluates the emitted expression through meval. For `sin(x)*y + z^2 + t`, normalized derivatives `y*cos(x)`, `sin(x)`, `2.0*z`, `1.0` were accepted by meval; finite-difference checks passed for representative `sin`, `exp`, polynomial, and time-variable expressions (details below). This establishes a viable minimal adapter for the sampled output—not a full printer/grammar guarantee. A proper implementation should operate on structured identifiers or rigorously tokenize output; blind global brace replacement is not approved.
- Equivalent tiny release cdylib harness measured a raw wasm delta of 1,543,870 bytes and gzip-9 delta of 437,161 bytes over meval-only baseline. The crate’s regex and parser code are included; this is wrapper-level evidence, not the full app release delta.

### symcas

- Exact crate: [`symcas` 0.1.1](https://crates.io/crates/symcas/0.1.1); upstream [repository](https://github.com/cislunarspace/cas), [docs](https://docs.rs/symcas/0.1.1/). Crates.io reports `MIT OR Apache-2.0`, Rust 1.85. Its README’s license section confirms either MIT or Apache-2.0. The facade is split across `symcas`, `cas-domain`, `cas-expr`, `cas-print`, and `cas-parse` crates.
- Upstream README documents `Context::diff` and deterministic `plain` formatting/roundtrip within its own parser. It also advertises exact rational arithmetic, polynomial GCD/factorization, assumptions, Taylor series, and simplification—substantially beyond the required minimal differentiation slice.
- Host interoperability probe parsed three comparable formulas with symcas, differentiated each with respect to x/y/z/t, formatted with `plain`, then passed every derivative string through meval at a sample point. All outputs parsed/evaluated, including `y*cos(x)`, `2*z`, and absent-variable `0`; this is actual evidence for the sampled grammar subset. No finite-difference or gradient/vector-identity test was run against symcas.
- An equivalent tiny release cdylib with symcas differentiating `sin(x)*x + x^2` and meval evaluating its printed derivative built for wasm. Raw size was 367,569 bytes (242,536-byte delta from baseline); gzip-9 was 124,622 bytes (71,637-byte delta). This proves this small configuration builds, not complete app wasm or runtime behavior. It is young (0.1.1), with reported Rust 1.85 minimum; check project MSRV before any future consideration.

### in-house over meval public tokens/RPN

- Exact API/source confirmed: meval 0.2.0 exposes `pub mod tokenizer`, `pub fn tokenize`, `pub mod shunting_yard`, and `pub fn to_rpn` in `src/shunting_yard.rs`; `Expr` dereferences to `[Token]`, exposing stored RPN. The token enum includes numbers, variables, functions/arity, unary/binary operators and delimiters. This establishes a public RPN seam; it does not prove a differentiator would be small/correct. No in-house prototype was implemented.
- This route avoids adding a production CAS dependency and makes grammar equivalence structurally controllable. It still entails a new parser/AST/printer and a long-term maintenance burden. Prefer a minimal rewrite rule set and fail closed for unsupported differentiation rules; do not silently fall back to numerical derivatives.

## Required composition and verification status

For exmex, explicit x/y/z/t mapping passed on a weighted linear expression. Meval accepted and evaluated brace-normalized derivatives for representative smooth expressions; central finite differences (step `1e-6`) agreed within `2e-6` for `sin(x)*y + z^2 + t` w.r.t. x, `exp(x*y)+cos(z)+t^2` w.r.t. y, and `x^3+y^2*z+sin(t)` w.r.t. z. A throwaway composition harness formed gradient and curl components by repeated scalar differentiation; `curl(grad f)` and `div(curl F)` evaluated to zero at two points for smooth polynomial inputs. These are limited sampled checks, not comprehensive acceptance. Symcas outputs passed meval parsing/evaluation for its comparison expressions, but were not subjected to finite differences or vector identities. Remaining acceptance includes:

1. broad x/y/z/t mapping and derivatives across absent variables, constants, powers, supported unary/binary functions and meval precedence;
2. randomized/deterministic derivative equivalence and finite differences for both engines;
3. broader gradient/divergence/curl coverage and identities across nontrivial smooth fields;
4. explicit unsupported grammar/operator behavior and custom context/bound-function policy;
5. deterministic printer round-tripping and adversarial identifier/output tests for any adapter.

## wasm build and matched harness sizes

The earlier wasm failure was an environment mismatch, not a crate failure: `/usr/bin/rustc` used `/usr` sysroot without wasm std while rustup stable had wasm std installed. The separate read-only rootcause report is `/tmp/opencode/vfe-cas-wasm-diagnosis-2026-11-10.md`. Cargo confirmation and both candidate builds succeeded using explicit rustup shims; no toolchain/global environment was changed.

Matched wrapper harnesses used `cdylib`, release profile, same target/compiler/settings, same exported formula `sin(x)*x + x^2`, `wasm32-unknown-unknown`, and isolated worktree `target/`. Baseline uses meval to evaluate that formula. exmex harness differentiates it and evaluates derivative with exmex; symcas differentiates and sends printed derivative through meval. Therefore these are comparable small integration wrappers, though call paths differ by design. Sizes (raw; gzip-9):

| Harness | wasm bytes | gzip-9 bytes | Delta from baseline raw / gzip |
|---|---:|---:|---:|
| meval-only baseline | 125,033 | 52,985 | — |
| exmex 0.21.0 + `partial` + meval | 1,668,903 | 490,146 | +1,543,870 / +437,161 |
| symcas 0.1.1 + meval | 367,569 | 124,622 | +242,536 / +71,637 |

No wasm runtime invocation occurred. These measurements are harness costs only, not app release artifact deltas; no hard budget is implied. We did not measure an in-house implementation. All builds used `/home/paul/.cargo/bin/cargo` with command-local `RUSTC=/home/paul/.cargo/bin/rustc`, `CARGO_TARGET_DIR=$PWD/target`, and `-j 4`.

## Recommendation and risks

No implementation direction is recommended from this evidence alone. Both external candidates passed sampled host interoperability, and both build on wasm; symcas is substantially smaller in the matched wrapper while exmex is larger. exmex’s brace syntax is addressable by a narrow, validated identifier adapter on sampled output, so it is not a stand-alone rejection reason. In-house remains a plausible alternative because meval exposes public RPN APIs, but its implementation effort and wasm cost are unknown. Selection requires a common broader host test corpus, unsupported-grammar policy, and explicit tradeoff on size versus implementation/API risk.

Key risks: the meval token API may require reproducing parsing/precedence behavior; emitted syntax can change semantics if associativity/parentheses are wrong; symbol differentiation rules and domain restrictions need explicit boundaries; dependency-free does not mean maintenance-free. The main evidence gaps are broader grammar/identity coverage and actual wasm runtime testing. API risk remains unmeasured for in-house; adapters are viable but not fully hardened for the external engines. No candidate has passed the complete CAS acceptance.

## Reproduction record

Executed from isolated worktree `/home/paul/.local/share/opencode/worktree/9d1d25/cas-selection-spike`, with all experiments under `.scratch/cas-spike/` and Cargo target at that worktree’s `target/`:

- `cargo search exmex --limit 5`; `cargo search symcas --limit 5`
- `cargo info exmex`; `cargo info symcas`; `cargo info cas-parse`; `cargo info cas-diff` (expected lookup failed: no crate named `cas-diff`)
- `CARGO_TARGET_DIR=$PWD/target cargo run --manifest-path .scratch/cas-spike/exmex/Cargo.toml --release -j 4` (mapping, meval-adapter finite differences and polynomial identities passed; first run exposed missing-variable handling, corrected by returning zero for absent variables)
- `CARGO_TARGET_DIR=$PWD/target cargo run --manifest-path .scratch/cas-spike/symcas/Cargo.toml --release -j 4` (symcas printed derivatives accepted and evaluated by meval for three formulas and x/y/z/t)
- `CARGO_TARGET_DIR=$PWD/target RUSTC=/home/paul/.cargo/bin/rustc /home/paul/.cargo/bin/cargo build --manifest-path .scratch/cas-spike/baseline/Cargo.toml --target wasm32-unknown-unknown --release -j 4`
- Same explicit cargo/rustc/target command with manifest `.scratch/cas-spike/exmex/Cargo.toml`, `--release --lib -j 4`; repeated for `.scratch/cas-spike/symcas/Cargo.toml`
- `stat -c '%n %s' target/wasm32-unknown-unknown/release/{baseline_size,exmex_spike,symcas_spike}.wasm`; `gzip -9 -c <artifact> | wc -c` (sizes above).
- Earlier system-cargo wasm attempts failed with E0463; rootcause diagnosis explains the sysroot mismatch. They are not evidence of candidate failure.

Cargo emitted a future-incompatibility warning for transitive `nom 1.2.4` in the successful exmex host probe. No product build/test gates or wasm runtime checks were run because production files were intentionally untouched. Each crate reported Cargo future-incompatibility warning for transitive nom 1.2.4.

## Sources

- Ticket 55 accepted epic: `.scratch/field-viewport/issues/55-physics-roadmap.md` (local project source).
- ADR 0003: `docs/adr/0003-physics-foundations.md`; vocabulary: `GLOSSARY.md`.
- exmex 0.21.0 crate metadata/source: https://crates.io/crates/exmex/0.21.0 and https://github.com/bertiqwerty/exmex/tree/v0.21.0
- symcas 0.1.1 crate metadata/source: https://crates.io/crates/symcas/0.1.1 and https://github.com/cislunarspace/cas/tree/v0.1.1
- meval 0.2.0 metadata/source: https://crates.io/crates/meval/0.2.0 and https://github.com/rekka/meval/tree/v0.2.0
