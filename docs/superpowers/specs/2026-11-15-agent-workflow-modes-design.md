# Agent workflow modes and measurable routing

**Status: SUPERSEDED by owner decision on 2026-11-16.** Historical record retained; the Python-runtime implementation approach is no longer active. See [Markdown-first design](2026-11-16-markdown-first-agent-workflow-design.md).

Date: 2026-11-15. Scope: extend the already-reviewed bounded OpenCode workflow on `feature/token-efficient-workflow`; no scalar/product changes.

## Intent and success

Route authorized repository tasks through selectable OpenAI or OpenCode Go model seats while preserving accepted-work quality and making actual usage/cost coverage visible. The initial priority is to make the OpenCode Go path usable with supported identifiers, not to claim comparative efficiency before it has been exercised.

Success means: `openai` remains the default and preserves current behavior; explicit `opencode-go` mode uses only catalog-validated Go models; deterministic S/M/L task classification affects role/effort routing; overrides and reasons are durable run evidence; metrics distinguish provider-reported, estimated, and unknown usage/cost; current security, review, quota-stop, Git evidence, and integration boundaries remain intact.

## Existing architecture and evidence

The branch contains a stdlib Python CLI/runtime (`scripts/agent_workflow.py`, `scripts/workflow/{api,runner,policy,metrics}.py`), JSON config, offline tests, docs and CI. The OpenCode V2 adapter owns bounded session permissions, run persistence, validation, independent diff review, and Git-backed merge proof. Existing accounting retains usage from unique assistant messages and leaves unsupported token/cost semantics unknown. The combined review found F1–F4; all were fixed and independently scoped-re-reviewed before local integration. Preserve those fixes: provisional task validation vs final validation, protected review paths, failed-run acceptance reconciliation, and unknown-zero cost handling.

Current catalog discovery confirms `opencode-go/deepseek-v4.1-flash`, display name **DeepSeek V4.1 Flash 1017**, active, with low/high/max variants and listed rates (USD per million: input 0.15, output 0.60, cache read 0.003, cache write 0). This confirms the catalog's model label/ID, not the provider's internal serving checkpoint. Do not hard-code a checkpoint claim beyond catalog metadata. OpenAI models are active but expose no catalog pricing. No comparative efficiency baseline or live-generation workflow benchmark is established.

## Goals

- Add explicit `workflow.mode: openai | opencode-go`; default `openai`.
- Centralize per-mode role mappings and model validation; no automatic provider fallback or mode change.
- Start Go mode with the exact supported ID `opencode-go/deepseek-v4.1-flash` for orchestrator, simple/standard implementer, and standard reviewer, in separate agent contexts. Difficult roles may use a stronger cost-effective Go catalog model only when present and explicitly configured; do not invent model IDs.
- Preserve the requested OpenAI initial role preferences (Sol orchestrator medium; Luna simple low and standard medium; Sol complex high and reviewer medium/high), adapting to current task complexity without automatic provider switching.
- Deterministically classify S/M/L from scope, risk, ambiguity, test coverage, and known failures/review findings. Record the class and why in each task/run. Per-task model/effort override requires a recorded reason and exact catalog validation.
- Capture all participating assistant-message usage, including failed rounds, by task/role/model; record invocations, attempts, reviews, findings, duration, and final accepted/merged outcome where evidence supports it.
- Report token and economic efficiency only with complete denominators and explicit coverage. Separate measured/provider-reported, estimated-from-catalog/config, unknown, and subscription allowance consumption. Never label Go subscription use as free or unlimited.
- Reuse the current runtime, permissions, records, and review pipeline; add only small deterministic helpers for task metadata, BASE/HEAD and evidence/report bookkeeping where they eliminate repeatable controller work.

## Non-goals and guardrails

- No change to product code, Rust dependencies, Cargo profiles, CI protections, or GitHub issue authority.
- No automatic model/provider hopping on unavailable models, errors, quotas, or budget stops; save evidence and stop.
- No model escalation solely to compensate for unclear requirements; clarification remains a controller decision.
- No fake efficiency claims, assumed reasoning-token semantics, fabricated prices, or provider checkpoint guarantees unsupported by catalog evidence.
- No replacement orchestration service/database, automatic push/PR/merge, or new issue tracker.
- Initial benchmark is instrumentation plus documented evaluation procedure; comparative claims wait for reliable accepted/merged task data in each mode.

## Approaches considered

1. **Incrementally extend the existing runtime (chosen).** Add mode-aware config/catalog validation, task classification/routing, durable override/usage metadata, reports, and bounded bookkeeping helpers to existing modules. Tradeoff: touches security-sensitive runner/policy code, so keep changes cohesive and covered by offline tests and independent review. Chosen because existing ownership, evidence, permissions, and review mechanisms should remain the foundation.
2. **Build mode/routing and metrics as a separate front-end, leave runtime single-mode.** Lower immediate runner risk, but duplicates task state and cannot prove that Go mode used the requested seats or account for every participating agent. Rejected because it creates competing authorities and incomplete measurement.
3. **Replace the runtime with a larger orchestration framework.** Could enable broader scheduling, but adds dependencies/services and reopens already-reviewed security/integration contracts. Rejected as disproportionate and contrary to the repository's simple stdlib runner.

## Proposed shape

Keep `.opencode/agent-workflow.json` the single source for mode defaults, role maps, classifier thresholds, validation limits, and optional price/allowance metadata. Store the selected mode, classifier result, resolved role/model/effort, override reason, and immutable model catalog facts in each run record. Resolve routing before agent creation and fail closed if model/provider/variant is not active or lacks required tool capability. Ensure every reviewer is a fresh session/context, distinct from implementation; for high-risk changes, use a different model family where an authorized active Go model supports the required tool/reasoning behavior. Do not assert that model diversity is automatically better; record the choice.

S/M/L is deterministic where facts suffice: S for small isolated prose/mechanical/known-cause work; M for ordinary multi-file features; L for cross-module, GPU/rendering, numerical, concurrency, ambiguous, low-coverage, or repeated-failure/review-risk work. Scope/risk/failure signals can only raise class, not silently lower declared risk. A controller may correct classification with an auditable reason. L routes to configured difficult seats; repeated failures follow existing bounded escalation/owner stop policy.

Extend persisted run evidence and `metrics` output to report per task/agent/model: raw input/output/cache/reasoning counts when exposed, invocation and retry/review counts, cost source, duration, accepted/merged status, and regressions/reverts when supplied by verified records. Aggregate by mode, never infer unknown metrics as zero. Report accepted-work/token and merged-work/token plus corresponding cost ratios only where measured usage/cost is complete; clearly identify source, coverage, and estimate basis. Do not use LOC as the only productivity measure.

Small bookkeeping automation may generate/validate compact task metadata and review-package identity from a trusted manifest and Git evidence, but GitHub stays authoritative for issue claims/status/dependencies and the controller retains judgment, review dispatch, integration, and merge authority. No script creates or updates issue state absent explicit owner authorization.

## Verification and evaluation

Keep existing staged validation and all review/repair limits. Add offline tests for both mode schemas, exact model/provider/variant rejection, no fallback, classifier monotonicity, override reason persistence, independent reviewer sessions, unknown and measured cost accounting, mode-separated reports, failed/incomplete run denominator handling, and bookkeeping identity/evidence checks. Mock only the API boundary; no model generation in tests. Run Python unit tests, compile, docs/link/policy checks, CLI offline doctor, and mocked end-to-end run/merge/metrics; retain Rust CI jobs unchanged. Require an independent initial diff review and applicable scoped fix re-review.

Evaluation starts with a baseline report explicitly marked **not yet comparable** where historical message/token data is absent. Collect representative S/M/L accepted and merged tasks in each explicitly selected mode. Compare total measured tokens/cost, first-pass acceptance, repair/review rounds, time-to-merge, regression/revert rate, and usage coverage. Change model assignments only when this evidence shows improved accepted merged work per token/cost without unacceptable quality regression. Respect subscription allowance accounting; never equate catalog price with the user's actual billed or quota consumption.

## Left to implementation

The implementer may choose compact schema names and exact deterministic thresholds consistent with current limits, provided both modes, auditability, fail-closed validation, independent review, and unknown accounting remain testable. If the runtime cannot obtain provider-reported Go allowance consumption, record that limitation and expose unknown rather than synthesizing it.
