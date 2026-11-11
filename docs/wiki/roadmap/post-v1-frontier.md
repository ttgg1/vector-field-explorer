# Post-v1 frontier — historical provenance

**Not the live queue or design approval.** Use the main-root queue, latest
ledger and handoff named in [Workflow](../../../references/workflow.md).
Old “next”, “blocked” and active-seat claims have been retired from this page.

## 2026-11-10 decisions

- At `b0665b4`, after v0.1.0 (`aa55470`), the owner chose Windows packaging
  and time-dependent fields as parallel fronts. The original Windows → sources
  → time ordering was superseded. Packaging and formula binding were distinct
  areas but shared `docs/CODEBASE.md`, so parallel work needed file ownership.
  See [frontier decision](../decisions/2026-11-10-next-frontier.md).
- Ticket 39 split into Part A (time-dependent fields, independent of 37) and
  Part B (38 plot animation, dependent on 37 and 39). This later split also
  supersedes the older ticket-38 tombstone; do not infer deletion from old notes.
- Completion recovery at `f6111341ccf903b4cc889b9841754578ef8dc025` established
  72, 39 and 74 merged, with 72's release dry-run still outstanding. Source:
  `.superpowers/sdd/2026-11-05-vector-field-explorer/archive/progress-through-2026-11-11.md`, 2026-11-10.
- Native CAS planning at `703e77e9cc983aaa4124b41b5ec0603d7e745edf` made the
  adapter a Phase 3 prerequisite, not a Phase 1 prerequisite. CAS/source math
  could be parallel, but `src/lib.rs` / `tests/suite.rs` needed exclusive writers;
  the panel followed the mobile UI work. [CAS decision](../decisions/2026-11-10-native-cas.md).

## Dependency evidence, not execution permission

Ticket 55 and the old SDD `SESSION-HANDOFF.md` (2026-11-10, original lines
56–69) recorded Windows as the platform prerequisite for sources, time support
for induction, and plot/animation, performance research and the 2D slice seam
as foundation work. Phase-specific dependencies and accepted phase order remain
in the owning roadmap/ADRs; this summary is not permission to implement proposed
source numerical/model choices.

Wave A (A-Click/I-10, A-Particle/I-11, A-Render/I-13, A-Cleanup/M-9..M-28,
A-Docs/M-12..M-26) was quality work, not that critical path. Finishing/reviewing
half-landed quality work first avoided contaminating the next capability diff.
The later 2026-11-11 owner ruling prioritizes smaller independent coherent
scopes while retaining genuine dependencies; live order belongs only in the queue.
