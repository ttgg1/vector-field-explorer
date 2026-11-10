# Wiki lives in-repo — 2026-11-10

## The claim

On 2026-11-10 the user chose `docs/wiki/` — tracked in the repository — as the
home for this project's tribal knowledge, over a local `.scratch/wiki/` or an
external `~/wiki/`.

## Why

The knowledge that needs to survive (seat rules, review-loop practice, roadmap
reasoning, review outcomes) is worth keeping with the project: it is the
difference between a fresh session knowing how the loop works and re-deriving
it from scratch. Tracking it in `docs/wiki/` also puts it next to
`docs/CODEBASE.md` and `docs/adr/`, which are already the project's
documented conventions, and makes it reviewable in a pull request.

The cost is that wiki pages can go stale against the code. That is why pages
are dated historical records rather than living documentation: a stale claim is
visible as an old date, not as a silent lie.

## The rules

- `docs/wiki/index.md` lists every page, grouped by category, with a one-line
  summary. New pages are added to the index in the same change.
- Each page covers one thing: a decision, a recurring problem, a subsystem, a
  user preference.
- Claims carry a date and cite their source (commit, ticket, ledger entry,
  report file). Sources are cited, never copied or edited.
- Pages are never rewritten to change history; a superseding claim is a new
  dated line, and the old line is marked superseded.

## Related

- [Index](../index.md)
- [Post-v1 frontier](../roadmap/post-v1-frontier.md)
