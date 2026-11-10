# Post-v1 frontier and critical path — as of 2026-11-10

## Update (later on 2026-11-10) — ticketed and split

The two fronts became tickets on 2026-11-10: **72** (Windows packaging) and
**39** (time-dependent fields). Ticket 39 was split — Part A (fields) is
ticket 39 and is independent of the scalar plot; Part B (plot animation) is
ticket 38, blocked by 37 and 39. Windows ships as **v0.2.0**. The order in
`AGENTS.md` now reads: Windows packaging and time-dependent fields in
parallel → Phase 1 (sources) → Phase 3 → Phase 5 → Phase 2 → Phase 4 →
overlays → shareable presets → CSV import.

## The claim

As of 2026-11-10, `v0.1.0` (`aa55470`) was released and `main` stood at
`b0665b4`. The post-v1 order in `AGENTS.md` is Windows → Phase 1 (sources) →
time-dependent fields (39/38) → Phase 3 (induction) → Phase 5 (ODE) →
Phase 2 (matter) → Phase 4 (Maxwell) → div/curl overlays → shareable presets
→ CSV import. The user chose, on 2026-11-10, to take **Windows packaging** and
**ticket 39** as the next wave.

## Dependencies read out of the roadmap ticket

From `.scratch/field-viewport/issues/55-physics-roadmap.md`:

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

They are independent: packaging touches CI and packaging scripts, ticket 39
touches the evaluator (`src/field.rs`) and its callers. Running them as two
parallel seats removes two serial dependencies before either becomes
critical. The risk is ticket 39, because changing how formulas are bound
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
