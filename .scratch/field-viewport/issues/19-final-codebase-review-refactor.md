# 19: Final whole-codebase review + organization refactor

**What:** the last development ticket before `v0.1.0`. A complete, read-everything
review of the repository by `opencode-go/claude-haiku-5-5#max`, followed by a
behavior-preserving refactor that leaves the codebase organized, documented, and
consistent with its own conventions. Review with Claude Haiku; implement fixes
with the normal coding seat under the usual fix loop.

**Status:** ready-for-agent (runs after ticket 10; final gate before the tag)

## Why this exists

Tickets 02–18 add features incrementally. Ticket 09 reviews the feature branch's
behavior and triages parked minors. Neither asks: *is the whole repository still
coherent?* This ticket does — module boundaries, duplicated helpers, dead code,
naming against `GLOSSARY.md`, test placement, docs that no longer describe the
code, and CI/release hygiene. It is deliberately last so it sees the finished
state.

## Phase 1 — complete review

Dispatch the reviewer (`claude-haiku-5-5#max`) over the whole repository, not a
diff: every `src/` module, `tests/`, `scripts/`, `.github/workflows/`, `docs/`,
`README`, `AGENTS.md`, `GLOSSARY.md`, and ADRs. Output
`.superpowers/sdd/2026-11-05-vector-field-explorer/final-codebase-review.md`
with an explicit coverage list (every file accounted for) and findings as
Critical / Important / Minor, each with `file:line`, the problem, why it matters,
and a proposed refactor.

Review axes:

- **Structure:** one module, one job; pure logic vs UI/GPU seams; oversized
  modules (report line counts and split only where a real seam exists);
  duplicated helpers; circular or leaky dependencies; `SceneGPU` boundary intact.
- **Naming and domain language:** names match `GLOSSARY.md`; the three distinct
  `Preset` types are unambiguous; no stale terms from superseded designs.
- **Dead and risky code:** unused items, `#[allow(dead_code)]`, unreachable
  branches, panics/unwraps on user input, unbounded allocations, silent fallbacks
  that hide errors; resource caps (particles, click seeds, capture sizes) intact.
- **API surface:** `pub` items that exist only for tests or history; minimal
  visibility; no accidental public commitments.
- **Tests:** all under `tests/` (ticket 13 enforcement holds), meaningful
  assertions rather than restatements, gaps in critical seams, duplicated test
  helpers, naming that describes behavior.
- **Docs:** README/CODEBASE/AGENTS/spec/ADR accuracy against the code; comments
  that contradict behavior; stale status text; no undocumented rulings.
- **Build and CI:** native and wasm `cfg` hygiene, clippy suppressions, workflow
  duplication, release/Pages scripts that no longer match the repo, licence
  compatibility.
- **Performance:** only clear hot-path waste (per-frame allocations, repeated
  work in render loops); no speculative micro-optimization.

## Phase 2 — refactor

Behavior-preserving only. Work highest-value first, one concern per commit:
`refactor: <concern>`. For each finding, either fix it, or record
`Ruling: <finding> — deferred/rejected because <why>` in the ledger. Keep tests
green at every commit; write the failing test first when a seam's contract
changes.

Rules:

- No new features, no UI redesign, no dependency major upgrades (a security or
  build blocker becomes its own ruling and commit).
- No public-API break without a ledger ruling; prefer tightening visibility over
  widening it.
- Keep GPL-3.0-compatible dependencies.
- Preserve all existing behavior, including camera reset-on-launch, corrupt-style
  fallback, preset fail-fast, and click-burst semantics.
- Update `docs/CODEBASE.md`, README, AGENTS status, and any ADR/glossary entries
  touched by the refactor in the same ticket.

## Phase 3 — re-review and close

Scoped re-review by the same reviewer over the refactor range. Zero Critical or
Important findings remaining; Minors either fixed or ruled. Ledger the final
review, coverage, and every ruling. Then merge, push `main`, and confirm CI plus
smoke are green.

## Acceptance

- [ ] Review report covers every repository file; no module skipped.
- [ ] Every Critical/Important finding fixed, or explicitly ruled in the ledger.
- [ ] All refactor commits are behavior-preserving; no feature changes mixed in.
- [ ] `tests/` holds all tests; `src/` has no `#[cfg(test)]` and no unexplained
      `allow(dead_code)`.
- [ ] Build, `cargo test --all-targets`, clippy `-- -D warnings`, and
      `scripts/smoke-run.sh 15` pass; CI and smoke are green after push.
- [ ] Test count does not fall below the ticket-10 baseline.
- [ ] README, `docs/CODEBASE.md`, AGENTS status, glossary, and ADRs match the code.
- [ ] Final scoped re-review is APPROVED.

## Non-goals

New features, wasm/Pages work (ticket 16), release automation (ticket 17),
performance tuning beyond obvious waste, or rewriting working code for taste
alone.

## Dispatch

- Reviewer: `opencode-go/claude-haiku-5-5#max` (complete review and scoped
  re-review), background.
- Implementer: `opencode-go/deepseek-v4.1-flash#high` (`#max` if stuck),
  background, one at a time, brief = this ticket plus the review report.
- Controller keeps the ledger; no code edits during the loop.

## Scheduling

After ticket 10, as the final development ticket. The first `v0.1.0` tag is cut
only after this ticket's acceptance is green — it supersedes ticket 09 as the
last gate for tagging.
