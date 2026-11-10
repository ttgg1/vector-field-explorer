# Next frontier: Windows packaging + time-dependent fields — 2026-11-10

## Update (later on 2026-11-10) — the fronts are now ticketed

The original claim below is superseded by this update and the subsequent
frontier ruling recorded in `AGENTS.md` (2026-11-10).

The two fronts were published as tickets on 2026-11-10: **ticket 72**
(Windows packaging) and **ticket 39** (time-dependent fields, Part A). The
user ruled four points that shaped them:

- **Ticket 39 is split.** Part A (the `t` variable, clock stepping, per-frame
  rebuild) is ticket 39 and is **not** blocked by ticket 37; Part B (plot
  animation) reuses the existing ticket 38 and is blocked by 37 and 39.
  The numbers stay meaningful for anyone reading the roadmap ticket.
- **Windows artifact:** an unsigned `.zip` of the `.exe`, no installer.
- **Windows toolchain:** MSVC on a `windows-latest` runner, not a MinGW
  cross-build.
- **Windows CI gate:** build + test + clippy on Windows, plus an uploaded
  artifact. No GUI launch claim — a runner has no display.

Windows ships as **v0.2.0**: a new supported platform is a feature, not a
patch, and v0.1.0 is not touched.

## The claim

On 2026-11-10 the user chose the next work wave after the current fix loop
merges: **Windows packaging** and **ticket 39 (time-dependent fields)** run as
two parallel fronts, rather than packaging alone or the Phase 0 prerequisites.

## The options considered

1. Windows packaging alone — the single blocking item for Phase 1 sources;
   narrow, mostly CI/toolchain work, low conflict risk with later physics work.
2. **Windows + ticket 39** — chosen. Their primary implementation work is
   independent (packaging versus the `t` / bind-once `meval` refactor), but the
   original assumption that their file sets do not overlap was incorrect:
   both tickets may edit `docs/CODEBASE.md`. Parallel work therefore needs
   coordination on that shared documentation file, as recorded in the update
   below (tickets 72 and 39, 2026-11-10).
3. Phase 0 prerequisites first (ticket 37 plot surface, 38 animated plots, 35
   perf harness, the 2D slice seam) — better foundation, but delays
   user-visible capability.
4. Wave-A quality cleanup — no new capability and no roadmap movement.

## Why this pairing

The post-v1 order recorded in `AGENTS.md` on 2026-11-10 places tickets 72 and
39 in parallel, then Phase 1 → Phase 3 → Phase 5 → Phase 2 → Phase 4, followed
by overlays, shareable presets and CSV import. The tickets share
`docs/CODEBASE.md` (tickets 72 and 39), so parallel work requires coordination
on that file rather than assuming disjoint file sets.

Ticket 39's `meval` bind-once refactor is the risky half: it changes how
`field.rs` evaluates formulas, so every sampling, particle and streamline test
is a witness to it. It needs its own review attention, not a shared seat.

## Cost, recorded honestly

Two fronts mean two implementer worktrees; the 2026-11-10 model in `AGENTS.md`
assigns controller/reviewers and implementers to different seats. Ticket 39's
evaluator change touches `src/field.rs` and its callers; both tickets also
share `docs/CODEBASE.md`, so coordinate edits to that file.

## Related

- [Post-v1 frontier and critical path](../roadmap/post-v1-frontier.md)
- [Dispatch, review and fix loop](../process/dispatch-and-review-loop.md)
