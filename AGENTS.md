# Vector Field Explorer — agent router

Rust eframe/egui vector-field explorer, native and WebAssembly. **GPL-3.0-only**;
keep dependencies compatible.

## Start here

1. Read [Workflow](references/workflow.md) before dispatch, implementation,
   review, or integration: it is the single tracked authority for current policy.
   For runnable OpenCode requests or usage/merge evidence, read the
   [runtime guide](docs/agent-workflow.md).
2. Reconcile outstanding requirements, claims, status, discussion and dependencies
   in [GitHub Issues](https://github.com/ttgg1/vector-field-explorer/issues)
   before dispatch. Workflow defines claim checks, attribution and scheduling.
   For recovery/provenance, read the main checkout's ignored
   `.superpowers/sdd/2026-11-10-native-cas/progress.md` (latest dated rulings)
   and relevant reports. The migration and physics mapping pointers are in
   Workflow; preserved local tickets/queue and old handoffs are historical
   evidence, not current authority. Verify completion against Git and reports.
3. Read only the references needed for the task:
   - **Code/navigation:** [module map](docs/CODEBASE.md).
   - **Rust, UI, rendering, persistence:** [engineering notes](references/rust.md).
   - **Tests:** [layout and test-profile rationale](docs/wiki/process/test-layout-and-gates.md).
   - **Review fixes:** [fix loop](references/fix-loop.md).
   - **Product/release scope:** [constraints](references/product-scope.md).
   - **Design:** [glossary](GLOSSARY.md), [ADRs](docs/adr/),
     [plan/spec index](docs/superpowers/README.md).
   - **History:** [wiki index](docs/wiki/index.md), [review records](docs/reviews/).

## Token-efficient coding policy

- Use OpenAI models only. Read [agent-workflow.json](.opencode/agent-workflow.json)
  for configurable role defaults and limits; validate model IDs and reasoning
  effort against the enabled provider catalog. Never silently switch provider.
- Keep simple tasks direct: skip decomposition when the request is clear and
  bounded. Give implementers only relevant requirements, acceptance criteria,
  owned paths and context. Reuse sessions and send only targeted retry feedback.
- Review the actual diff against the request. Skip model review only for a
  validated, low-risk trivial change inside the configured safe paths and size
  limits. Use the configured reviewer for normal changes and high-effort
  escalation for high-risk changes; never downgrade protected paths.
- Bound attempts and reviews using configuration. Stop on unresolved failures;
  retain validation evidence and account for usage per agent, including cached
  tokens and cost only when known. Report accepted work separately from Git-verified
  merged work; never infer a merge from approval.
- For runnable requests, follow the [workflow guide](docs/agent-workflow.md).
  [Workflow](references/workflow.md) remains the authority for claims, review and
  integration policy; this section is the concise agent entry point.

Use applicable skills before acting; use `writing-for-agents` for agent docs.
Prune superseded instructions rather than appending another policy copy.
