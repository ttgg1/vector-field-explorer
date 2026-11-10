# Vector Field Explorer — wiki

Tribal knowledge for this project: decisions with their reasoning, the working
process, and the state of long-running efforts. Every page is a historical
record — claims carry a date and cite the commit, ledger entry, ticket or
report they came from. Nothing here describes "how things are now" without
that evidence.

Conventions: one page per thing; pages link to each other; sources (tickets,
commits, reports, review packages) are cited, never copied.

## Process

- [Dispatch, review and fix loop](process/dispatch-and-review-loop.md) — how
  a ticket becomes a reviewed commit on `main`, and what each seat may touch.
- [Test layout and gates](process/test-layout-and-gates.md) — where tests live,
  which commands gate a change, and the parts of the build that are cold.
- [Closing a review finding](process/closing-review-findings.md) — what counts
  as evidence, the seam budget, and why the user-visible path has to be
  exercised.

## Decisions

- [Seat model availability (2026-11-10)](decisions/2026-11-10-seat-model-availability.md)
  — the latest 2026-11-10 ruling in `AGENTS.md` assigns controller/reviewers
  and implementers to different seats, with effort escalated by round.
- [Repairing the unreviewed tickets 69/70/71 (2026-11-10)](decisions/2026-11-10-repairing-unreviewed-tickets.md)
  — post-hoc reviews found real gaps, so the three commits go through a fix
  loop before they count as done.
- [Wiki lives in-repo (2026-11-10)](decisions/2026-11-10-wiki-in-repo.md) —
  `docs/wiki/` is tracked; the user chose this over a local or external wiki.
- [Next frontier: Windows + time-dependent fields (2026-11-10)](decisions/2026-11-10-next-frontier.md)
  — two parallel fronts once the current fix loop merges.

## Roadmap

- [Post-v1 frontier and critical path](roadmap/post-v1-frontier.md) — which
  ticket unblocks which, and what the physics phases depend on.

## Reviews

- [Tickets 69/70/71 review round 1 (2026-11-10)](reviews/tickets-69-71-round-1.md)
  — the round-1 findings snapshot; later dispositions are in the SDD ledger.
