# Repairing the unreviewed tickets 69/70/71 — 2026-11-10

## The claim

On 2026-11-10, tickets 69, 70 and 71 were implemented and pushed straight to
`main` without review, while their ticket files already said "merged". The
post-hoc review found real gaps in all three. They are treated as **not done**
until a fix round and a scoped re-review pass.

## Evidence

`main` at `b0665b4` on 2026-11-10 contained, in order:

- `b191a3c` — ticket 70, fly camera under non-QWERTY layouts
- `6e3929b` — ticket 69, SVG screenshot export for the 2D view
- `b0665b4` — ticket 71, mobile compact UI polish

Only three commits since the `v0.1.0` tag (`aa55470`) mention "ticket NN", and
all three are these. Nothing else in the history claims a ticket, so a
review-record rule is cheap to introduce with a three-commit baseline.

Post-hoc reviews (one seat per diff, using the superpowers task-reviewer
instructions copied to
`.superpowers/sdd/2026-11-05-vector-field-explorer/task-reviewer-instructions.md`)
returned ❌ spec issues for all three. Findings lists:
`task-69-findings.md`, `task-70-findings.md`, `task-71-findings.md` in the same
directory.

## Why post-hoc review rather than a rewrite

The work itself is sound in shape and passes the gates; what is missing is
conformance to the ticket text and the ledger rulings — a missing regression
test, a dead code path, an unimplemented half of a feature. That is a
fix-round workload, not a re-implementation workload.

## The rule

- A ticket is done when its brief is satisfied, its tests exist and fail
  without the fix, its gates pass, and a reviewer has signed off on the diff.
- The ledger records the fix loop: `Task <N>: fix round <R>/5 (<X> addressed,
  <Y> open — …; commits <a>..<b>)`.
- Branches merge into `main` only after the re-review is clean, one at a time.

## Related

- [Tickets 69/70/71 review round 1](../reviews/tickets-69-71-round-1.md)
- [Dispatch, review and fix loop](../process/dispatch-and-review-loop.md)
