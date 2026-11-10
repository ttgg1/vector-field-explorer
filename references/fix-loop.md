# The fix loop

How a task that failed review gets fixed, re-reviewed and merged. Read this
before starting a fix round; the short version is in `AGENTS.md` under
"SDD workflow".

## The loop, step by step

1. **Findings.** A review seat writes findings to a findings file, one file
   per task: `.superpowers/sdd/<session>/task-<n>-findings.md`. Each finding
   is Critical / Important / Minor, with the file:line it refers to and the
   fix it wants. Minor findings are "fix if cheap, do not expand scope".
2. **Brief.** The controller writes a fix brief that names each open finding
   with its file:line target and the bar for closing it. A finding is closed
   only when a test fails without the change and passes with it — a test that
   asserts a hand-written predicate instead of the production decision is not
   a closing test.
3. **Implementer seat.** One implementer, one worktree, one branch
   (`feature/fix-ticket-<n>`), its own `target/`. The implementer works from
   the brief and the findings file, never from the whole plan. It must not
   dispatch subagents.
4. **Gates.** Before committing: `cargo build`, `scripts/test.sh` (which runs
   `cargo test --all-targets` and prunes stale test binaries),
   `cargo clippy --all-targets -- -D warnings`. After touching the renderer,
   shaders, windowing or input: `scripts/smoke-run.sh 15`.
5. **Commit.** `fix: <what changed> (ticket <n>)`, one commit set per round.
6. **Review package.** Generate the diff by hand:
   `git diff <round-base>..<head> > .superpowers/sdd/<session>/review-<base>..<head>.diff`
   where `<round-base>` is the head the previous round was reviewed at. (The
   superpowers `review-package` helper is not installed in this environment;
   do not wait for it.)
7. **Scoped re-review.** A reviewer seat — a *different* model from the
   implementer — reads the findings file, the round's diff and the
   implementer's claims, and rules on each open finding: closed / open /
   partial, with evidence. It also checks that the findings closed in earlier
   rounds still hold, and that the round introduced no new issues. The
   re-review is scoped: it does not re-review the whole task from scratch.
8. **Ledger.** Append one line per round:
   `Task <n>: fix round <r>/5 (<x> addressed, <y> open — <what>; commits <a>..<b>)`
   and, when approved, a completion line with the merge commit.
9. **Merge.** Rebase the branch onto `main`, then `git merge --ff-only`. This
   repo keeps a linear history with no merge commits. Merge one branch at a
   time — the controller serializes merges even when seats ran in parallel.
10. **Push and check.** `git push origin main`, then confirm the GitHub CI
    run (build + smoke) and the Pages run are green before starting the next
    task.

## Rules that keep the loop honest

- **Rounds 1–3 resume the original implementer** with the same brief and the
  updated findings file; round 4 says "a prior implementer attempted this
  task 3 times — find a different approach"; round 5 says "this task has
  failed 4 times — stop and escalate to the controller". Always state which
  round it is in the brief.
- **File-disjointness.** Only run two implementer seats in parallel when their
  file sets do not overlap. A seat that needs another seat's file waits for
  it to land; rebase onto `main` first so it sees the current version.
- **One seam per need.** When a test needs a new seam, promote exactly one
  `#[doc(hidden)] pub` item, put the reason in the commit body and the
  ledger, and never widen the API further for tests.
- **Do not merge unreviewed work.** `main` is reviewed work only. If a
  round's re-review is not clean, fix and re-review; do not push the branch
  and hope.
- **Record the ruling.** Every decision made during a fix round goes in the
  ledger as a `Ruling: <decision> — <why>` line, not only in a chat message.
- **Post-mortem after five rounds.** If a task burns all five rounds, stop,
  write up what kept failing, and change the brief or the approach before
  trying again.

## Seat models

The free seats are the only ones that dispatch in this workspace; paid seats
fail with "Policy budget exceeded". Implementers run on
`opencode-go/beam-free` (`#high`, `#xhigh`/`#max` for the bigger tasks);
reviewers run on `opencode-go/longcat-2.5-free`, with the effort matched to
the scope of the review.
