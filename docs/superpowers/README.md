# Plans and specifications

These are design/task records with individual approval states, **not a live queue or policy source**.
Before dispatching work, use [Workflow](../../references/workflow.md) and its
main-root ledger/handoff/queue pointers. Historical checkboxes, status labels
and gate lists do not override newer owner rulings or authorize redispatch.

| Record | Purpose/evidence |
| --- | --- |
| [Post-v0.1.0 plan](plans/2026-11-05-vector-field-explorer.md) | Historical task scopes; helper entrypoint for the old SDD workspace. [Roadmap history](../wiki/roadmap/post-v1-frontier.md). |
| [Native CAS plan](plans/2026-11-10-native-cas.md) | Adapter/panel task contracts; helper entrypoint for `.superpowers/sdd/2026-11-10-native-cas/`. |
| [Native CAS spec](specs/2026-11-10-native-cas-design.md) | Accepted symbolic grammar, semantics, compatibility and UI boundaries. |
| [CAS rails design](specs/2026-11-14-cas-rails-design.md) | GitHub #14 separate native CAS panel and extensible ordered rails; compatibility and Compact boundaries. |
| [CAS rails plan](plans/2026-11-14-cas-rails.md) | Two serialized TDD tasks; native gates and review before preserving merge. |
| [Reference visibility design](specs/2026-11-11-reference-visibility-design.md) | Ticket 77 approved reference-geometry and UI/persistence boundaries; not implementation authorization. |
| [Reference visibility plan](plans/2026-11-11-reference-visibility.md) | Ticket 77 task contracts, tests, reviews and current workflow verification gates. |
| [Native CLI plot export design](specs/2026-11-11-cli-plot-export-design.md) | Ticket 78 reviewed CLI/config, deterministic scene and headless output contract; design only, not implementation acceptance. |
| [Native CLI plot export plan](plans/2026-11-11-cli-plot-export.md) | Ticket 78 implementation tasks and verification handoffs; consult live workflow/ledger before dispatch. |
| [Accuracy-first source evaluator design](specs/2026-11-14-source-evaluator-design.md) | GitHub #24 all-eight pure model/evaluator; owner-approved 2026-11-16. Supersedes isolated 2026-11-10 source drafts. |
| [Accuracy-first source evaluator plan](plans/2026-11-14-source-evaluator.md) | Five bounded TDD/review units, provisional numerical defaults and separate E/B diagnostics; owner-approved 2026-11-16 implementation basis. |
| [Sources panel design](specs/2026-11-16-sources-panel-design.md) | GitHub #25 dedicated source CRUD panel, explicit Formula/Sources modes, fixed E/B Both colours and legend, accuracy opt-in and persistence; owner-approved 2026-11-17 and implemented. |
| [Sources panel plan](plans/2026-11-16-sources-panel.md) | GitHub #25 bounded TDD/review units; owner-approved 2026-11-17 implementation basis, completed and verified in Git history. |
| [Public-catalog workflow design](specs/2026-11-15-public-catalog-workflow-design.md) | Superseded historical ruling: public rates only, no benchmark or accepted-work efficiency ratios. |
| [Public-catalog workflow plan](plans/2026-11-15-public-catalog-workflow.md) | Superseded historical report-surface change; see Markdown-first workflow records. |
| [Markdown-first agent workflow design](specs/2026-11-16-markdown-first-agent-workflow-design.md) | Owner-approved retirement of Python orchestration; native OpenCode Markdown roles and Python metrics only. |
| [Markdown-first agent workflow plan](plans/2026-11-16-markdown-first-agent-workflow.md) | Native OpenCode modes, metrics-only Python, and runtime retirement tasks. |

CAS implementation evidence: [73 review record](../reviews/73-native-cas.md),
[75 review record](../reviews/75-native-cas-panel.md), and verifiable Git history.
Older source drafts in isolated worktrees are background, not implementation
authority; use the approved accuracy-first records above and reconcile current
workflow/ledger status before any dispatch.
