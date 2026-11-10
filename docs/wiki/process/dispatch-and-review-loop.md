# Dispatch, review and fix loop — as practised from 2026-11-10

## The claim

From 2026-11-10, work in this project moves ticket → brief → implementer seat
→ review seat → fix round → re-review → serialized merge to `main`. The
controller (the session that owns the goal) keeps the ledger and the wiki and
does not write production code during the loop.

## The pieces

- **Ledger:** `.superpowers/sdd/2026-11-05-vector-field-explorer/progress.md`.
  Every `Ruling: <decision> — <why>` goes here, not only into chat or todos.
  Never ledger a task without a `git rev-parse`-verifiable hash.
- **Tickets:** `.scratch/field-viewport/issues/NN-slug.md`, one per file, with
  `Status:` and dependencies. `.scratch/` is local and untracked; never delete
  a ticket.
- **Briefs / reports / findings:** `task-NN-brief.md`, `task-NN-report.md`,
  `task-NN-findings.md` in the ledger directory. A brief is what the seat is
  allowed to do; a report is what the seat claims it did; a findings list is
  what the reviewer found. The implementer never reads the review; the
  reviewer never reads the whole plan.
- **Review packages:** `review-<base>..<head>.diff`, generated with the
  superpowers `scripts/review-package` helper.

## The loop

1. **Brief.** The controller writes the brief from the ticket, naming the exact
   files the seat may touch.
2. **Implement.** The seat works TDD-first, runs the gates, commits with
   `feat:` / `fix:` / `docs:`, appends its report, and returns
   `DONE | DONE_WITH_CONCERNS | BLOCKED | NEEDS_CONTEXT`. Seats never dispatch
   sub-subagents and never merge or push.
3. **Review.** A separate seat reviews the diff against the brief and the
   ledger rulings, and writes findings.
4. **Fix round.** A fresh seat addresses the findings; the controller appends
   `Task <N>: fix round <R>/5 (<X> addressed, <Y> open — …; commits <a>..<b>)`
   to the ledger. Rounds 4 and 5 are the last; beyond that the task is
   re-scoped or split.
5. **Re-review.** A scoped re-review checks the findings list, the brief and
   the report against the new diff — not the whole task again.
6. **Merge.** Serialized, one branch at a time, after CI is green. `main` is
   reviewed work only; never force-push `main`.

## Parallelism rules

- Seats may run concurrently only when their file sets are disjoint. Before
  dispatching, list the files per seat and check for overlap.
- Each seat gets its own worktree and its own `target/` directory, so cold
  builds do not serialize behind each other.
- Cargo is capped at 10 jobs (`.cargo/config.toml`); raising it needs a ledger
  ruling.
- When a ticket's findings span files owned by two seats, split the ticket
  (e.g. 69 into a serializer half and a UI half) and dispatch the second half
  only after the first lands.

## Failure modes seen so far

- **Review skipped because the work "looked done".** On 2026-11-10 three
  tickets went straight to `main` unreviewed and all three failed their
  post-hoc review. The loop exists to catch exactly that.
- **A seat model that cannot dispatch.** See
  [seat model availability](../decisions/2026-11-10-seat-model-availability.md).

## Related

- [Test layout and gates](test-layout-and-gates.md)
- [Repairing the unreviewed tickets 69/70/71](../decisions/2026-11-10-repairing-unreviewed-tickets.md)
- [Tickets 69/70/71 review round 1](../reviews/tickets-69-71-round-1.md)
