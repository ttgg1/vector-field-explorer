# Markdown-First Agent Workflow Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Python task orchestration with native OpenCode Markdown agents/commands, retaining Python only for metrics.

**Architecture:** Keep workflow policy in the existing Markdown authority; add explicit per-mode role agents and small command entry points. Delete Python runtime code and tests, retain a metrics-only CLI for OpenCode-exported usage data, and document the operator-enforced safety limits.

**Tech Stack:** OpenCode V2 Markdown agents/commands, Python 3.10+ standard library for metrics only, existing docs and unittest tooling.

**Spec:** [Approved design](../specs/2026-11-16-markdown-first-agent-workflow-design.md).

## Global Constraints

- GitHub Issues owns all outstanding requirements, claims, status, discussion, and dependencies.
- Each request uses an isolated `feature/<topic>` worktree with explicit BASE and exclusive file ownership.
- Feature work preserves brainstorming/planning as applicable, SDD + TDD, independent initial review, applicable deterministic gates, scoped fix loop, and existing five-round ceiling.
- Usage/rate/quota/budget limits stop work; no provider/model hopping absent owner authorization.
- OpenAI is the default workflow mode; OpenCode Go is explicitly selected and never automatic.
- Every new role/task begins at its assigned model's lowest supported effort; escalation is explicit and reasoned; independent tasks reset.
- OpenAI initial preferences: GPT-6.1 Sol low for orchestrator/complex implementer/reviewer; GPT-6 Luna lowest supported effort for simple/standard implementation and fixes.
- OpenCode Go defaults: DeepSeek V4.1 Flash low for orchestrator/implementation/fixes; GLM 5.3 Flash low for independent review. MiMo V2.6 Flash is an optional reviewer without a catalog-exposed variant.
- Workflow-specific Python remains only for metrics; unrelated existing project scripts stay untouched. No task dispatch, prompt assembly, issue tracking, verification orchestration, or merge proof in Python.
- Native configuration/Markdown are not a sandbox; document safeguards not provided by native OpenCode and do not claim parity with the removed runtime.
- No product/Rust functionality, dependencies, test gates, licensing, release, push, or merge changes.

## Review Focus

- Wrong/missing model or mode must never trigger provider fallback; users select an explicit role/mode.
- New tasks must use lowest configured effort even after a previous escalation; policy requires an explicit fresh role selection and recorded reason for escalation.
- Reviewer must be independent and read-only at tool level where supported; document that shell instructions are not a security boundary.
- OpenCode exports may omit token/cost fields; metrics reports unknown rather than zero or inferred allowance consumption.
- Removing runtime does not silently remove required CI gates, issue authority, quota stops, or preserving integration evidence.

---

### Task 1: Replace the workflow policy route and configure native roles

**Files:**
- Modify: `AGENTS.md`
- Modify: `references/workflow.md`
- Modify: `references/fix-loop.md` only where runtime-specific references need replacement
- Modify: `docs/agent-workflow.md`
- Create: `.opencode/agents/orchestrator-openai.md`
- Create: `.opencode/agents/implementer-openai.md`
- Create: `.opencode/agents/implementer-openai-complex.md`
- Create: `.opencode/agents/reviewer-openai.md`
- Create: `.opencode/agents/fix-openai.md`
- Create: `.opencode/agents/orchestrator-opencode-go.md`
- Create: `.opencode/agents/implementer-opencode-go.md`
- Create: `.opencode/agents/reviewer-opencode-go.md`
- Create: `.opencode/agents/reviewer-opencode-go-mimo.md`
- Create: `.opencode/agents/fix-opencode-go.md`
- Create: `.opencode/opencode.jsonc`
- Create: `.opencode/commands/develop-openai.md`
- Create: `.opencode/commands/develop-opencode-go.md`
- Create: `.opencode/commands/review-openai.md`
- Create: `.opencode/commands/review-opencode-go.md`
- Create: `.opencode/commands/review-opencode-go-mimo.md`
- Delete: `.opencode/agent-workflow.json`
- Delete: `.opencode/request.example.json`

**Interfaces:** Agents use native V2 frontmatter fields `description`, `mode`, `model`, and `permissions`; commands use `description`, `agent`, and prompt body. Commands are separate per mode/reviewer so mode selection is explicit.

- [x] Confirm current model IDs/variants with the installed catalog; record that MiMo has no explicit variants.
- [x] Write concise role agents and entry commands; reviewer gets edit denied, role behavior is explicit, and policy remains in `references/workflow.md`.
- [x] Replace runtime guide with a concise human workflow, safety boundary, and metrics invocation.
- [x] Remove stale runtime pointers from `AGENTS.md`, workflow, and fix-loop docs while preserving all extant quality/owner requirements.
- [x] Validate with `opencode debug agents` and `opencode debug config` (no model generation), `python3 scripts/check-docs.py`, and `git diff --check`.
- [x] Commit `feat:` with `Generated-by: OpenCode`.

### Task 2: Retain metrics only and retire runtime implementation

**Files:**
- Create: `scripts/metrics.py` as a standalone metrics-only CLI for OpenCode session-export JSON
- Create: `scripts/tests/test_metrics_cli.py`
- Create: `scripts/tests/fixtures/opencode-session-export.json`
- Delete: `scripts/agent_workflow.py`
- Delete: `scripts/workflow/api.py`
- Delete: `scripts/workflow/policy.py`
- Delete: `scripts/workflow/runner.py`
- Delete: `scripts/workflow/__init__.py` if empty/unused after removal
- Delete: `scripts/tests/test_workflow_api.py`
- Delete: `scripts/tests/test_workflow_policy.py`
- Delete: `scripts/tests/test_workflow_runtime.py`
- Delete: `scripts/tests/test_workflow_metrics.py`

**Interfaces:** `python3 scripts/metrics.py EXPORT.json [...]` reads files written by `opencode session export --sanitize` and writes a JSON summary to stdout. It reports totals/grouping and known coverage only; it does not infer task acceptance/merge results.

- [x] Add metrics CLI tests for actual supported export shape, totals by model, duplicate message handling, and unknown cost/usage.
- [x] Run targeted metrics tests and observe expected failure before implementation.
- [x] Implement the smallest adapter/CLI while retaining honest unknown accounting; keep no runtime imports or dependencies.
- [x] Run the metrics tests and Python compilation.
- [x] Remove runtime-only implementation and tests after confirming no remaining repository references.
- [x] Run `python3 -m unittest discover -s scripts/tests -v`, changed-file `py_compile`, `python3 scripts/check-docs.py`, and `git diff --check`.
- [x] Commit `fix:` with `Generated-by: OpenCode`.

### Task 3: Reconcile historical design, CI, and future plugin inputs

**Files:**
- Modify: `docs/superpowers/specs/2026-11-15-agent-workflow-modes-design.md` — mark superseded and link to chosen design
- Modify: `docs/superpowers/plans/2026-11-15-agent-workflow-modes.md` — mark superseded and link to this plan
- Modify: `.github/workflows/ci.yml` only if Python workflow-test step needs narrowing to metrics-only (keep documentation tests)
- Modify: `.superpowers/sdd/2026-11-16-markdown-first-agent-workflow/progress.md` (ignored local ledger)

**Interfaces:** Old documents remain historical; new design/plan are the active policy. CI continues running docs and Python metrics tests, not deleted runtime tests.

- [x] Mark old proposal/plan as superseded without deleting historical findings or review records.
- [x] Add the owner-approved rationale and future-plugin candidate lessons to the new spec, without promising a plugin.
- [x] Validate CI's Python test command covers the metrics tests and remains offline.
- [x] Run docs checks, Python tests, OpenCode agent discovery, and final `git diff --check`.
- [x] Prepare actual diff and verification evidence for the required independent review; no push/merge in this plan.
- [x] Commit `docs:` with `Generated-by: OpenCode`.

Independent reviewer result: a fresh `reviewer-openai` pass returned no actionable findings after the first-round blockers and suggestion were corrected. See the local SDD ledger for the review trail and final verification evidence.

## Integration

Keep changes on the dedicated workflow feature branch. Reconcile main before any later integration. This plan does not authorize push, PR, merge, or release. Preserve the review package, verification output, and runtime review findings as history.
