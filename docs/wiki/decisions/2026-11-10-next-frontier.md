# Next frontier: Windows packaging + time-dependent fields — 2026-11-10

## The claim

On 2026-11-10 the user chose the next work wave after the current fix loop
merges: **Windows packaging** and **ticket 39 (time-dependent fields)** run as
two parallel fronts, rather than packaging alone or the Phase 0 prerequisites.

## The options considered

1. Windows packaging alone — the single blocking item for Phase 1 sources;
   narrow, mostly CI/toolchain work, low conflict risk with later physics work.
2. **Windows + ticket 39** — chosen. Packaging is independent of the `t`
   (time) / bind-once `meval` refactor, so two seats can work without touching
   the same files.
3. Phase 0 prerequisites first (ticket 37 plot surface, 38 animated plots, 35
   perf harness, the 2D slice seam) — better foundation, but delays
   user-visible capability.
4. Wave-A quality cleanup — no new capability and no roadmap movement.

## Why this pairing

The post-v1 order in `AGENTS.md` is Windows → Phase 1 (sources) → time-dependent
fields (39/38) → Phase 3 (induction) → Phase 5 (ODE) → Phase 2 (matter) →
Phase 4 (Maxwell). Packaging blocks the first physics phase; ticket 39 blocks
the time-dependent phase. Taking both now removes two serial dependencies
before they become critical, and neither touches the other's files.

Ticket 39's `meval` bind-once refactor is the risky half: it changes how
`field.rs` evaluates formulas, so every sampling, particle and streamline test
is a witness to it. It needs its own review attention, not a shared seat.

## Cost, recorded honestly

Two fronts means two seats and two worktrees, and the free seat model
(see [seat model availability](2026-11-10-seat-model-availability.md)) means
both will need careful briefs. Parallel seats must stay file-disjoint;
ticket 39's evaluator change touches `src/field.rs` and everything that calls
it, so the packaging seat must avoid the evaluator entirely.

## Related

- [Post-v1 frontier and critical path](../roadmap/post-v1-frontier.md)
- [Dispatch, review and fix loop](../process/dispatch-and-review-loop.md)
