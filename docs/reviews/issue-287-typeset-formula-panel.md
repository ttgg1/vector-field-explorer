# Review record — issue #287: floating typeset formula panel

**Ticket:** 287

**Reviewed:** 2026-11-23
**Branch:** `feature/issue-287-typeset-formula-panel`
**Product base:** `1952c033eac4c899a576f3f112710c88ed168e05` (current `origin/main`)
**Reviewed commits:**
`0853e1557b6ef655c76642539df72325ede8e752` and
`4f4c4bc66f201d2e54a24e01b85bb1f0e0994641`

## Review disposition

The initial fresh independent review of `0853e155` returned **BLOCKED** with
two findings: (F1) the panel sourced the editable bar draft (`bar.text`) and
presented an empty/stale formula as active in non-Formula vector modes, and
(F2) parameter/time substitution used `format!("{value:.4}")`, collapsing small
values to `0` and disagreeing with the field evaluator's full-precision
substitution. A scoped fix at `4f4c4bc` closed both with regression tests
(`non_formula_vector_mode_never_shows_empty_or_stale_formula_as_active` and
`tiny_substituted_value_matches_field_evaluator_precision`).

The fresh re-review of the full `origin/main..HEAD` diff at `4f4c4bc` returned
**APPROVED** with no blocking findings: both prior findings closed with
file:line evidence, all six issue #287 acceptance criteria and the movable-panel
layout requirement met, and no new defect, scope creep, or state/ABI break.
Non-blocking observations (default overlap with the colour legend, a cosmetic
parenthesis difference, time-display readability, a weak secondary assertion)
were noted and are not release blockers.

## Evidence and limits

The implementer and fix reports record TDD RED/GREEN logs, full native gates
(`scripts/test.sh` 2696 passed / 0 failed / 1 ignored, `cargo build`,
`cargo clippy --all-targets -- -D warnings`, `python3 scripts/check-docs.py`,
`git diff --check`, changed-file rustfmt, `scripts/smoke-run.sh 15`, and a wasm
build). The controller independently verified commit identity, ancestry, diff
SHA-256 `1000f2df9c538a458a82c09d9289e3ebacdd8567fd1ce16966d0b0576242f189`,
and gate evidence. Dedicated browser/manual visual checks remain owner-waived,
not passed.
