# Vector Field Explorer — agent router

Rust eframe/egui vector-field explorer, native and WebAssembly. **GPL-3.0-only**;
keep dependencies compatible.

## Start here

1. Read [Workflow](references/workflow.md) before dispatch, implementation,
   review, or integration: it is the single tracked authority for current policy.
    For the Markdown-first OpenCode workflow and metrics, read the
    [agent workflow guide](docs/agent-workflow.md).
2. Reconcile outstanding requirements, claims, status, discussion and dependencies
   in [GitHub Issues](https://github.com/ttgg1/vector-field-explorer/issues)
   before dispatch. Workflow defines claim checks, attribution and scheduling.
   For recovery/provenance, read the active plan's ignored
   `.superpowers/sdd/<plan-name>/progress.md` and relevant reports. Historical
   migration and physics mapping pointers are in
   Workflow; preserved local tickets/queue and old handoffs are historical
   evidence, not current authority. Verify completion against Git and reports.
3. Read only the references needed for the task:
   - **Code/navigation:** [module map](docs/CODEBASE.md). When `.codegraph/`
     exists, every agent, including dispatched subagents, answers code questions
     and locates edits with `codegraph_explore` before grep or file reads.
   - **Rust, UI, rendering, persistence:** [engineering notes](references/rust.md).
   - **Tests:** [layout and test-profile rationale](docs/wiki/process/test-layout-and-gates.md).
   - **Review fixes:** [fix loop](references/fix-loop.md).
   - **Product/release scope:** [constraints](references/product-scope.md).
   - **Design:** [glossary](GLOSSARY.md), [ADRs](docs/adr/),
     [plan/spec index](docs/superpowers/README.md).
   - **History:** [wiki index](docs/wiki/index.md), [review records](docs/reviews/).

## Workflow route

- Follow [Workflow](references/workflow.md) for issue ownership, task selection,
  skill use, model policy, tests, review, quota stops, and integration. It is the
  single policy authority; the [agent workflow guide](docs/agent-workflow.md)
  explains native OpenCode entry points and metrics.
- Select `/develop-openai` or `/develop-opencode-go` explicitly; never silently
  change workflow mode or provider. New task roles start at configured lowest
  effort.
- Index every fresh worktree with `codegraph init` (each worktree has its own
  `.codegraph/`) before dispatching subagents, so they can use codegraph.
- Load only applicable skills and task-relevant references. Use
  `writing-for-agents` when changing agent-facing instructions.

Prune superseded instructions rather than appending another policy copy.
