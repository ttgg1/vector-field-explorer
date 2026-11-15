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

Use applicable skills before acting; use `writing-for-agents` for agent docs.
Prune superseded instructions rather than appending another policy copy.
