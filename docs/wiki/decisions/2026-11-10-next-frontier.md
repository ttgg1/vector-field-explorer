# Windows + time-dependent fields — decision, 2026-11-10

**Historical scheduling choice, not the current frontier.** See the live
resources in [Workflow](../../../references/workflow.md) for disposition/order.
Source: main-root `.superpowers/sdd/2026-11-05-vector-field-explorer/archive/progress-through-2026-11-11.md`,
2026-11-10 owner frontier/ticketing rulings.

## Choice and alternatives

The owner chose Windows packaging + ticket 39 in parallel, rather than packaging
alone, Phase 0 prerequisites (37/38/35/2D slice), or Wave-A cleanup. This advanced
platform support and user-visible time behavior together. Ticket 39's bind-once
meval refactor was the riskier half: it changed formula evaluation and needed
sampling/particle/streamline regression evidence and its own review.

The initial “file-disjoint” assumption was wrong: both fronts touched
`docs/CODEBASE.md`, requiring coordinated ownership despite distinct primary
implementation areas and isolated worktrees.

## Ticketing decisions retained

- Time support Part A became 39, independent of scalar plot 37. Plot animation
  Part B retained ticket 38, dependent on 37 and 39.
- Windows artifact: unsigned `.zip` of the `.exe`, no installer; MSVC on
  `windows-latest`, not MinGW cross-build.
- Windows CI: build/test/clippy and uploaded artifact; no GUI launch claim on
  a displayless runner.
- Windows is a v0.2.0 feature, not a v0.1.0 patch/backport.

Later dispositions and the distinction between implementation and release
verification debt are in [roadmap provenance](../roadmap/post-v1-frontier.md).
