# Post-v1 frontier and critical path — as of 2026-11-10

## Native CAS ticketing (2026-11-10; current ruling)

At base `703e77e9cc983aaa4124b41b5ec0603d7e745edf`, the owner approved
exmex-based native CAS and deferred CAS wasm support. Tickets 73 (adapter/math)
and 75 (native panel) are proposed just-in-time slices from ticket 55, not new
global roadmap phases. The adapter is file-disjoint from Phase 1 source work
and may proceed alongside it after ownership checks; it is not a Phase 1 gate.
The adapter must be complete before Phase 3 induction. The panel follows
ticket 71's active UI follow-up to avoid file contention. Both tickets are
blocked on independent builder review of the exact symbolic grammar/printer
boundary. The ticket files are
`.scratch/field-viewport/issues/73-native-cas-adapter.md` and
`.scratch/field-viewport/issues/75-native-cas-panel.md`; see also [the
decision record](../decisions/2026-11-10-native-cas.md).

This does not reorder Windows (72) or time-dependent fields (39), both merged;
ticket 72's release dry-run remains a separate open verification item. It also
does not alter the accepted Phase 1 → Phase 3 → Phase 5 → Phase 2 → Phase 4
sequence or tickets 37/38 dependencies. Ticket 71's follow-up remains active.

## Historical update (2026-11-10) — ticketed and split

This dated update records the original ticketing and scheduling decision; the
completion recovery note below is the latest status.

The two fronts became tickets on 2026-11-10: **72** (Windows packaging) and
**39** (time-dependent fields). Ticket 39 was split — Part A (fields) is
ticket 39 and is independent of the scalar plot; Part B (plot animation) is
ticket 38, blocked by 37 and 39. Windows ships as **v0.2.0**. The order in
`AGENTS.md` now reads: Windows packaging and time-dependent fields in
parallel → Phase 1 (sources) → Phase 3 → Phase 5 → Phase 2 → Phase 4 →
overlays → shareable presets → CSV import.

## Completion recovery (2026-11-10)

Tickets 72, 39 (Part A), and 74 are merged and verified on `main` at
`f6111341ccf903b4cc889b9841754578ef8dc025`. Ticket 72 still has its
workflow-dispatch release dry-run open; do not treat all acceptance criteria
as complete. Do not redispatch these tasks. For verified dispositions and the
current frontier, consult the final dated sections of
`.superpowers/sdd/2026-11-05-vector-field-explorer/progress.md`. The next
physics execution is not approved/spec-ready; investigation is ongoing.

## The claim

As of 2026-11-10, `v0.1.0` (`aa55470`) was released and `main` stood at
`b0665b4`. The post-v1 order in `AGENTS.md` is Windows → Phase 1 (sources) →
time-dependent fields (39/38) → Phase 3 (induction) → Phase 5 (ODE) →
Phase 2 (matter) → Phase 4 (Maxwell) → div/curl overlays → shareable presets
→ CSV import. The user chose, on 2026-11-10, to take **Windows packaging** and
**ticket 39** as the next wave.

## Dependencies read out of the roadmap ticket

From `.superpowers/sdd/2026-11-05-vector-field-explorer/SESSION-HANDOFF.md`
(2026-11-10, lines 56–69):

- **Windows packaging** blocks Phase 1 (sources). It is the only platform
  still missing; Linux and macOS ship.
- **Ticket 39** (time variable `t` plus a bind-once `meval` refactor) blocks
  Phase 3 (induction) and the time-dependent phase as a whole.
- **Phase 0 prerequisites** named in ticket 55: ticket 37 (plot surface),
  ticket 38 (animate plots), ticket 35 (perf harness), and a "2D slice seam"
  (`PlaneView` z axis feeding 2D sampling).
- **Wave A** items (A-Click/I-10, A-Particle/I-11, A-Render/I-13,
  A-Cleanup/M-9..M-28, A-Docs/M-12..M-26) are quality work, not critical
  path.

## Why packaging and ticket 39 together

They have distinct primary implementation areas, but are not file-disjoint:
both tickets may touch `docs/CODEBASE.md` (tickets 72 and 39). Parallel work
requires coordination on that shared documentation file. The risk is ticket 39,
because changing how formulas are bound
affects every sampling, particle and streamline test — it needs its own
review attention.

## Where the quality items rank

Wave A is filler relative to the critical path, but it is not worthless: the
legibility and touch-target work in ticket 71 was exactly Wave-A material and
it was still shipped to `main` (unreviewed) before the review caught its
gaps. Quality work that is already half-finished should be finished and
reviewed before new capability starts, because leaving it half-landed makes
the next seat's diff harder to read.

## Related

- [Next frontier decision](../decisions/2026-11-10-next-frontier.md)
- [Dispatch, review and fix loop](../process/dispatch-and-review-loop.md)
