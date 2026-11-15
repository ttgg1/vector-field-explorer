# Agent Workflow Modes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add explicit OpenAI and OpenCode Go routing, task-sensitive classification, and honest per-task token/cost measurement to the existing bounded workflow.

**Architecture:** Extend the current stdlib Python runner and its JSON configuration rather than introducing a second controller. Resolve a selected mode and deterministic S/M/L class to catalog-validated role seats before session creation; persist the full routing/usage evidence and aggregate by mode, preserving unknowns. Retain GitHub/controller authority, fail-closed provider behavior, staged validation, independent review, and verified merge evidence.

**Tech Stack:** Existing Python 3.10+ stdlib, OpenCode V2 CLI/API adapter, JSON run records, Git evidence, existing unittest/CI setup. No new service, SDK, or dependency.

**Spec:** [Approved design](../specs/2026-11-15-agent-workflow-modes-design.md).

## Global Constraints

- `workflow.mode` is `openai | opencode-go`; the default remains `openai`.
- Model IDs and effort variants are validated against the current enabled provider catalog; no silent provider/mode fallback.
- The Go default model ID is `opencode-go/deepseek-v4.1-flash`; catalog label is DeepSeek V4.1 Flash 1017, not a guarantee about provider-internal serving.
- OpenAI preferences: Sol orchestrator medium; Luna simple implementer low and standard medium; Sol complex implementer high and standard/complex reviewer medium/high.
- A per-task model/effort override includes an explicit reason and is catalog-validated.
- Task class and routing decision are persisted. Deterministic scope/risk/failure signals may raise complexity but never silently lower declared risk.
- Independent initial diff review remains required; reviewers use separate contexts. High-risk review uses configured escalation and may use a different authorized model family only if catalog capabilities support it.
- Preserve permission, worktree/Git identity, validation, retry/review caps, quota-stop, evidence persistence, and integration protections in current Workflow policy/runtime.
- Missing usage, pricing, cost, allowance, or token semantics remains unknown; never infer zero or unlimited subscription usage.
- Do not claim comparative efficiency improvements until reliable accepted/merged mode-specific measurements exist.
- Keep project dependencies/license/platform/CI unchanged; no product/Rust changes.

**Initial role routing (catalog-confirmed 2026-11-15, provisional until measurements):**

| Role | `openai` | `opencode-go` |
|---|---|---|
| Orchestrator | `openai/gpt-6.1-sol#medium` | `opencode-go/deepseek-v4.1-flash#low` |
| Simple implementer | `openai/gpt-6-luna#low` | `opencode-go/deepseek-v4.1-flash#low` |
| Standard implementer | `openai/gpt-6-luna#medium` | `opencode-go/deepseek-v4.1-flash#high` (Go catalog has no `medium` variant) |
| Complex implementer | `openai/gpt-6.1-sol#high` | `opencode-go/deepseek-v4-pro#high` |
| Standard reviewer | `openai/gpt-6.1-sol#medium` | `opencode-go/gpt-6-luna#medium` (separate context and model family from default DeepSeek implementation) |
| Complex reviewer | `openai/gpt-6.1-sol#high` | `opencode-go/gpt-6-luna#high` (separate context/model family) |
| Recovery | `openai/gpt-6.1-sol#high` | `opencode-go/gpt-6-luna#high` |

Go review diversity is selected as a cost-conscious initial hypothesis because GPT-6 Luna is catalog-enabled on Go and its listed base rates are lower than Gemini 4 Argon; no comparative quality or repair-rate claim is made. All IDs, variants, tool capability, and catalog status are checked at discovery time. Catalog pricing is not allowance-consumption evidence.

## Review Focus

- Disabled, missing, or wrong-provider model/effort: reject before any session is created (Task 1).
- Ambiguous or risky request and repeated failures: deterministic class must not downgrade risk or suppress review (Task 1).
- Override accepted without reason or stale catalog identity: reject and retain auditable resolution (Task 2).
- Partial/failed Go usage, cached tokens, unavailable cost or subscription allowance: totals must show honest coverage and mode separation (Task 2).
- Failed final validation/empty output or unverified merge: accepted/merged denominators remain zero while historical reviews and spend remain (Task 3; preserve existing F1/F3 regression coverage).

---

### Task 1: Mode configuration, catalog validation, and deterministic task class

**Files:**
- Modify: `.opencode/agent-workflow.json`
- Modify: `scripts/workflow/policy.py`
- Modify: `scripts/tests/test_workflow_policy.py`
- Modify: `scripts/tests/test_workflow_runtime.py` only to generate fixture catalog entries for every selected mode route/variant (retain the fixture's strict exact-match behavior).
- Modify: `.opencode/request.example.json` only if the mode/class/override contract requires an example update here; otherwise Task 3 owns it.

**Interfaces:**
- Consumes current config loader/model catalog validation and existing `complexity`/`risk` manifest fields.
- Produces validated per-mode role definitions and deterministic `classify_task` result containing class and reason; exact names/schema chosen once in implementation and documented in the task report.
- Config `modes.openai` and `modes.opencode-go` (or equally explicit schema) owns all mode-specific roles; no duplicate defaults elsewhere.

- [ ] Add tests for default `openai`, explicit `opencode-go`, exact Go 1017 catalog identity, provider/model/effort/capability rejection, and no fallback.
- [ ] Add deterministic S/M/L tests for scope/risk/ambiguity/coverage/repeated failure; prove declared high risk cannot be lowered.
- [ ] Run focused tests; observe expected RED.
- [ ] Implement the minimal config and policy seam; preserve backwards compatibility for existing OpenAI manifests/runs.
- [ ] Run focused and full Python tests, py_compile, offline doctor, docs/link and diff checks applicable to this slice.
- [ ] Commit `feat:` with `Generated-by: OpenCode`; prepare exact task diff for independent initial review.

### Task 2: Mode-aware runtime routing and complete usage/cost telemetry

**Files:**
- Modify: `scripts/workflow/runner.py`
- Modify: `scripts/workflow/policy.py` only for exact same-mode override validation helpers.
- Modify: `scripts/workflow/metrics.py` only where runtime evidence needs a compatible additive schema.
- Modify: `scripts/tests/test_workflow_runtime.py`
- Modify: `scripts/tests/test_workflow_metrics.py`
- Modify: `scripts/tests/test_workflow_policy.py` for override validation.

**Interfaces:**
- Consumes Task 1 validated mode roles and deterministic classifier.
- Resolves mode/class/role to exact provider/model/effort before session creation. Supports explicit per-task override only with a nonempty reason and exact catalog validation.
- Records selected mode, class/reason, resolved seat, overrides, catalog snapshot/identity, invocations, attempt/review counts, duration, actual token/cost coverage, and acceptance/merge identity supported by verified run/Git data.

- [ ] Test routing for simple/standard/complex implementer, orchestrator, normal/high-risk independent reviewer, override/reason, and unavailable-model fail-closed before session creation.
- [ ] Test per-agent/per-model usage, cache/reasoning counts without double counting, provider-reported vs estimated vs unknown cost, partial coverage, subscription allowance as distinct from USD price, and failed-agent spend.
- [ ] Test mode-separated aggregation and accepted/merged work per token/cost only with complete measurable denominators; ensure missing values stay null.
- [ ] Observe RED; implement additive routing/evidence changes without weakening permissions, retries, final validation, or Git proof.
- [ ] Run focused then full Python suite, compile, offline mocked end-to-end flow, docs/link and diff checks.
- [ ] Commit `feat:`/`fix:` with attribution; obtain independent initial review before Task 3.

### Task 3: Compact task bookkeeping, reports, docs, and integration verification

**Files:**
- Modify: `scripts/agent_workflow.py` and/or small helper module only for deterministic repeatable operations with a concrete user benefit.
- Modify: `scripts/tests/` workflow tests for bookkeeping and report output.
- Modify: `docs/agent-workflow.md`, `references/workflow.md`, `AGENTS.md` only as a concise conditional pointer, `.opencode/request.example.json`, `.github/workflows/ci.yml` only if existing CI does not already run all required offline tooling tests.

**Interfaces:**
- Consumes Tasks 1–2 stable config, routing and run evidence.
- Adds compact generation/validation of task metadata/BASE/HEAD/review package/finding/integration facts only where derivable without replacing controller judgment or GitHub authority.
- Metrics report clearly labels measured, estimated, and unknown values and reports both token and economic efficiency by mode.

- [ ] Test deterministic metadata/report generation from actual Git/run evidence; reject stale BASE/HEAD and unproven merges; no issue-tracker writes.
- [ ] Update docs and single authoritative workflow policy, replacing contradictions rather than appending policy copies. Document Go catalog/pricing and checkpoint uncertainty, stop behavior, and evaluation procedure.
- [ ] Add/update offline CI test invocation only if necessary; retain every existing Rust/native/wasm/platform protection.
- [ ] Run full Python unittest discovery, compile all changed Python, offline doctor/catalog validation where available, mocked end-to-end run/accept/merge/report, docs/link/policy checks and diff checks. Preserve existing regression tests for the four prior review findings.
- [ ] Obtain independent initial review of this task's actual diff; close blocking findings with scoped tests/re-review as workflow policy requires.
- [ ] Run final complete-workflow verification once on accepted combined HEAD; record remaining limitations and no efficiency-improvement claim until benchmark data supports one.

## Integration

Tasks run serially on a dedicated workflow feature branch because config, runner records, and policy/docs are shared. Preserve each accepted task commit and review evidence. Controller serializes integration; no worker commits/pushes/merges shared branches. Before final integration, verify reviewed HEAD/ancestry, applicable gates on exact changes, and preserve the already integrated prior workflow runtime/fixes. No main push/PR/merge occurs without explicit authorization under current owner policy.
