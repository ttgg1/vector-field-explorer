# Review record — ticket 74: review-record enforcement

**Ticket:** 74
**Reviewed:** 2026-11-10
**Branch:** `feature/ticket-74`
**Merge:** fast-forward into `main` (this repository keeps a linear history)

## Commits

| Commit | Subject |
| --- | --- |
| `9d68ba0` | feat: enforce review records for ticket commits (ticket 74) |
| `526e916` | fix: resolve abbreviated hashes in review records (ticket 74) |
| `1fe51da` | fix: use a reachable baseline commit (ticket 74) |

## Rounds

| Round | Range | Verdict | Open at the end |
| --- | --- | --- | --- |
| 1 (whole task) | base..`9d68ba0` | Needs fixes | Important: the checker matched only full SHAs, so the abbreviated hashes the records actually use never counted; Minor: a hash in running prose counted as a match; observation: only the exact lowercase single-spaced `(ticket NN)` marker was recognised |
| 2 (fix 1) | `9d68ba0`..`526e916` | Needs fixes | Important: the baseline was a stale pre-rebase twin of the fix commit — unreachable in a fresh clone (the checker exited 2) and it emptied the checked range, so the rule verified nothing and exempted the implementation commit |
| 3 (fix 2) | `526e916`..`1fe51da` | Approved | none |

## Findings and how they closed

- **I1 — abbreviated hashes.** A backticked abbreviated hash is resolved
  through `git rev-parse`. A missing record, a removed hash and a hash that
  names the wrong commit each fail and name the offending commit.
- **M1 — the shape of a match.** Only backticked spans count, so a hash
  quoted in running prose no longer matches a commit.
- **The subject marker.** The pattern is case-insensitive and tolerant of
  extra whitespace, so `(Ticket 69)` and `(ticket  69)` are caught too.
- **I2 — the baseline.** The baseline is the full SHA of `6b52ba8`, the last
  commit before the rule: it is on `main`, so it is reachable in a fresh
  clone, and it is an ancestor of the branch. A baseline that cannot be read
  still exits 2 with a clear message rather than passing silently.

## Evidence

The reviewer ran the cases itself, on a scratch copy of the repository, and
did not take the implementer's report as proof: a fresh clone runs the
checker instead of failing to read history; the branch reports exactly the
three ticket-74 commits and exits non-zero; a record naming all three exits
0 with "review records OK"; a broken baseline exits 2; a missing record, a
removed hash and a wrong hash each exit non-zero; and the script is
read-only, deterministic and behaves the same when invoked from another
directory.

## How the rule starts

The baseline grandfathers every commit up to and including `6b52ba8`, so
the rule applies to the ticket-74 commits themselves. The controller writes
this record in a separate `docs:` commit whose subject does not claim a
ticket, which is what lets the branch's own commits satisfy the rule.
