# Markdown-first agent workflow

**Status: APPROVED FOR IMPLEMENTATION by owner on 2026-11-16.**

## Intent

Replace the Python OpenCode orchestration runtime with a smaller workflow based primarily on Markdown instructions and native OpenCode agents and commands. Retain Python only for metrics. The runtime was judged too costly to maintain and not worth spending hours repairing. Its review findings and useful safety concepts should inform this simpler workflow and may inform a future OpenCode plugin, but no plugin is in scope now.

## Goals

- Use `AGENTS.md`, `references/workflow.md`, and `references/fix-loop.md` as the compact policy route and authorities.
- Provide native OpenCode agent profiles and reusable commands for two explicit modes: `openai` and `opencode-go`.
- Start every newly dispatched task/role at the lowest supported effort; make escalation explicit, evidence-based, local, and non-sticky.
- Preserve GitHub Issues authority, worktree/file ownership, TDD, independent review, verification, quota stops, and owner-controlled integration.
- Keep workflow-specific Python limited to a small metrics utility that consumes OpenCode-exported session data; unrelated existing project scripts remain untouched. Do not use Python for task dispatch, prompt assembly, permission orchestration, issue state, verification, or merge proof.
- Preserve lessons/findings from the bounded runtime as historical evidence for a possible future plugin, not as a second workflow or a claim that Markdown enforces equivalent controls.

## Non-goals

- No plugin implementation, API integration, database, long-running service, or prompt-generation framework.
- No automatic provider, mode, or model switching; no automatic high-effort routing.
- No issue-tracker replacement or issue writes by scripts.
- No claims that Markdown policies provide the same enforcement as the retired runtime's per-session permission checks and Git validation.
- No product/Rust behavior changes, dependency changes, CI gate weakening, release operations, or shared-branch integration in this task.

## Approaches

1. **Markdown/native OpenCode workflow with metrics-only Python (chosen).** Native profiles and commands provide reusable role entry points; Markdown remains the policy source; Python only summarizes exported usage. This directly addresses maintenance overhead while retaining the important human and tool quality gates. The tradeoff is reduced machine enforcement versus the runtime.
2. **Keep the bounded Python runtime and repair it.** Rejected by the owner because it is not working reliably enough to justify hours of repair.
3. **Build an OpenCode plugin now.** Rejected as premature; retain the runtime's findings as future design inputs and revisit only if real use identifies a narrow deterministic automation gap.

## Model routing

### OpenAI

Use `openai/gpt-6.1-sol#low` for orchestration, complex implementation, and review. Use `openai/gpt-6-luna#none` for simple/standard implementation and review fixes because the current catalog exposes `none` as Luna's lowest effort; if `none` is unavailable in a later catalog, use the lowest supported variant. No role starts above its model's lowest supported effort.

### OpenCode Go

Use `opencode-go/deepseek-v4.1-flash#low` for orchestration, implementation, and review fixes. Default independent review to `opencode-go/glm-5.3-flash#low`. Provide `opencode-go/mimo-v2.6-flash` as an explicitly selectable reviewer alternative; its current catalog exposes no variants, so it uses the provider's unvaried default. The catalog confirms the model IDs and tool capability, not provider billing/allowance semantics or serving implementation details.

No automatic escalation. Begin at the lowest configured effort for every fresh role/task. If a concrete failure persists after checking requirements, context, and approach, the controller may explicitly select the next supported effort for that role only, record why, and reset later independent tasks to lowest. Expensive-model changes require explicit owner authorization.

## Workflow responsibilities and safety boundary

- `AGENTS.md` routes tasks to the authoritative policy and only task-relevant references.
- `references/workflow.md` owns issue/claim checks, dispatch, worktree and file ownership, skill selection, model policy, verification, review, quota stops, integration authority, and limitations.
- Native agent profiles identify role, model, tool permissions, and concise task instructions. Commands are entry points, not policy copies.
- GitHub Issues remains the source of requirements, claims, dependencies, status, and discussion. Before dispatch, inspect current issue state and claims; do not claim work already owned.
- Use isolated feature worktrees; record BASE, scope, dependencies and exact owned paths. Preserve TDD, applicable local/full gates, independent initial review (except existing trivial safe-prose exception), scoped fixes, and review-round ceiling.
- Preserve failed edits and evidence. On quota/security/auth stop, save state and stop; no fallback or retries without owner direction.
- Markdown and standard Git checks are instructions, not a sandbox. Native reviewer edit denial and read-only guidance reduce risk but do not make shell or model behavior tamper-proof. Explicitly document remaining operator-enforced checks. Never claim runtime-equivalent enforcement.
- Serialize integration and retain reviewed commit ancestry/evidence. No push/merge is performed as part of this migration.

## Metrics

Retain a small standard-library Python tool at `scripts/metrics.py` that consumes JSON exports produced by `opencode session export --sanitize`. It reports raw usage/cost by model and provider and known coverage without attempting orchestration, task acceptance attribution, or merge proof. Missing tokens, cost, or Go allowance remain unknown; a reported zero is not a claim of free or unlimited use. `opencode stats` remains the first-line built-in view. Avoid ratios that imply accepted/merged-work attribution unless verified task evidence supports them.

## Validation

- Validate agent/command discovery and config syntax using the installed OpenCode CLI, without model generation.
- Validate mode model identifiers and variants against the enabled catalog; OpenCode Go reviewer options are GLM 5.3 Flash default and MiMo V2.6 Flash optional.
- Unit-test metrics on representative exported-session fixtures, including unknown usage/cost, duplicate messages, and mode/model grouping.
- Check Markdown links/policy consistency, Python unit tests and compilation, and `git diff --check`.
- Keep normal project CI and native product gates unchanged. Require independent review of the actual migration diff and scoped repair as policy requires.

## Future plugin inputs

The retired runtime's review record documents useful failure cases: decomposed tasks blocked by validation semantics, protected paths bypassing trivial review, failed final output counted as accepted, and unknown zero cost suppressing valid estimates. The runtime also highlighted useful needs: explicit task identity, usage evidence, worktree ownership, fail-closed quota handling, and preserving merge proof. These are candidate plugin requirements only; each must be revalidated against native OpenCode behavior before implementation.
