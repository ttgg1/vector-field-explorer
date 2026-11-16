# Public-catalog-first OpenCode Go workflow

> **Superseded by owner decision on 2026-11-16.** Its compact metrics ruling remains historical evidence; the Python orchestration/runtime approach is retired. See the active [Markdown-first agent workflow design](2026-11-16-markdown-first-agent-workflow-design.md).

## Intent

Prioritize a low-friction, efficient OpenCode Go workflow using public OpenCode model catalog facts. No model-generation comparison or personal usage measurement is needed; the owner reports low AI usage and does not want an oversized telemetry/review loop.

## Approaches considered

1. **Full accepted-work evidence accounting:** reconcile every task, call, and message before publishing per-accepted-task token/USD ratios. This is auditable but has become disproportionate to the integration goal.
2. **Public catalog plus modest operational telemetry (chosen):** use current catalog-enabled Go models, variants, and published token rates; retain per-mode observed usage/cost totals and accepted/merged counts, but omit accepted-work efficiency ratios. Unknown usage and subscription allowance stay unknown. This supports actual routing without implying a measured productivity or personal-billing outcome.
3. **Live benchmark:** execute equivalent coding tasks in both modes. This may offer comparative evidence but requires model calls and is unnecessary per owner direction.

## Goals

- Keep OpenAI as default and OpenCode Go as explicit selectable mode, with fail-closed model/variant checks and no fallback.
- Prefer the approved Go routing for the ordinary integration loop: DeepSeek Flash for orchestration/simple work, DeepSeek V4 Pro for complex implementation, and GPT-6 Luna for independent reviews/recovery.
- Use public catalog pricing as rate metadata only. Current discovery (2026-11-15) lists DeepSeek V4.1 Flash 1017 at $0.15/$0.60 input/output per million tokens, cache read $0.003; DeepSeek V4 Pro at $0.66/$1.98, cache read $0.022; GPT-6 Luna at $0.10/$0.50 with a higher-context tier. Prices are catalog values, not evidence of subscription allowance consumption, user charges, or savings.
- Keep observed per-mode usage/cost totals where data exists and show accepted/merged counts separately. Do not expose cost/token per accepted or merged task in this version. Missing data remains unknown, not zero.

## Non-goals

- No generation-based benchmark, model-quality claim, or personal spending inference.
- No detailed per-task accepted-spend evidence reconciliation or extra task/call ledger solely to derive unit-efficiency ratios.
- No provider hopping, workflow security changes, or product/Rust work.

## Calls left to implementation

Use live catalog discovery for exact provider/model/variant validity and published rate metadata; do not hard-code rates as user charges. Retain the concise routing and override behavior already implemented. Remove/omit accepted-work ratio outputs rather than manufacture confidence from incomplete attribution. Keep straightforward offline tests for routing, totals, and null handling; no repeated broad test/review loop for the abandoned ratio reconciler. Existing committed Task 2 behavior and current in-progress Fix 2 changes must be reconciled narrowly to this approved scope, preserving an external patch/checkpoint before removing superseded WIP.
