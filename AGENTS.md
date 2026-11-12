# Vector Field Explorer — agent router

Rust eframe/egui vector-field explorer, native and WebAssembly. **GPL-3.0-only**;
keep dependencies compatible.

## Start here

1. Read [Workflow](references/workflow.md) before dispatch, implementation,
   review, or integration: it is the single tracked authority for current policy.
2. Reconcile current work from the main checkout's local resources (ignored,
   not necessarily present in worktrees):
   - `.superpowers/sdd/2026-11-10-native-cas/progress.md`, latest dated rulings,
     and `session-handoff-2026-11-11.md` beside it;
   - `.scratch/field-viewport/QUEUE.md` for scheduling;
   - `.scratch/field-viewport/issues/NN-slug.md` for requirements;
   - verifiable Git history for completion. A queue/status line is not evidence.
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
