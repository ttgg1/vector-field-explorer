# Public-Catalog-First Workflow Integration Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans or the bounded workflow runtime. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Keep selectable OpenCode Go routing straightforward and use public catalog data for model availability and published-rate estimates without claiming measured savings.

**Architecture:** Preserve existing fail-closed catalog validation, task routing and per-message usage records. Report usage/cost totals and accepted/merged counts by workflow mode; omit per-outcome token/USD and useful-lines/token ratios because this project does not need a call/task completeness framework or a live benchmark.

**Tech Stack:** Existing Python workflow runtime and OpenCode V2 model catalog; no new dependencies and no model generation.

**Spec:** `docs/superpowers/specs/2026-11-15-public-catalog-workflow-design.md`.

## Global Constraints

- `openai` remains default; `opencode-go` is explicit.
- Exact provider/model/variant/tool-chat validation remains fail-closed; no fallback.
- Public per-token prices are rate metadata, not account billing or subscription allowance.
- Missing usage/cost stays unknown; no live comparative-efficiency claim.
- Preserve runtime permissions, retry limits, independent review, validation and Git proof.
- Keep the change small; no evidence reconciliation framework or repeated test-fix loop.

## Review Focus

1. Catalog estimates must use the exact observed model and only unambiguous flat-rate entries; tiered rates remain unknown.
2. Failed and incomplete usage remains included in all-spend totals where observed, but must not be labeled accepted-work efficiency.
3. Workflow-mode accepted/merged counts must use existing verified run/task statuses, not inferred model quality.

---

### Task 1: Simplify mode report to observed totals and outcome counts

**Files:**
- Modify `scripts/workflow/metrics.py`.
- Modify `scripts/tests/test_workflow_metrics.py`.
- Modify `scripts/workflow/runner.py` only as needed to remove superseded per-record accepted-ratio attribution while retaining existing run/Git status evidence.
- Modify `scripts/tests/test_workflow_runtime.py` only for the matching report contract.
- Add this approved design/plan and link both from `docs/superpowers/README.md`.

**Interfaces:** Keep existing `message_record` and `summarize` entry points. Keep total/per-mode tokens, costs, cost source/coverage, accepted/merged counts, and basic retry/review counts. Remove `tokens_per_accepted`, `cost_per_accepted`, `tokens_per_merged`, `cost_per_merged`, and `useful_merged_lines_per_million_tokens`. Do not remove per-message usage, cost, mode, or verified run/task outcomes.

- [ ] Inspect and preserve current public catalog rate use: exact model lookup, flat-rate-only estimates, no assumed Go allowance.
- [ ] Add one focused test asserting mode totals and accepted/merged counts remain, unit-efficiency ratios are absent, and unknown usage/cost remains null.
- [ ] Run the focused metrics/runtime test groups once; fix only regressions directly caused by this compact output contract.
- [ ] Run Python compile, offline doctor, docs check, and `git diff --check`; no model-generation request.
- [ ] Commit the focused change with `Generated-by: OpenCode`; report current catalog source/date and checks.

## Execution Notes

- Supersedes Task 2 Fix 2's uncommitted exhaustive call/task evidence reconciler. Before removing it, preserve the exact patch outside the worktree as recovery evidence.
- Keep the existing accepted Task 2 and Fix 1 commits. The current runtime worktree is `feature/token-efficient-workflow`; verify exact HEAD before changes.
- A focused review may verify this report-surface change; do not restart broad accepted-work ratio review, since those ratios are intentionally removed.
- No actual OpenCode Go invocation is needed or authorized for this integration; public catalog discovery is sufficient.
