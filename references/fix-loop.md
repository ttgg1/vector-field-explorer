# Scoped fix loop

Read before fixing review findings. [Workflow](workflow.md) owns seat choice,
gates, the simple-task re-review exception, and integration authority.

1. **Brief:** name every open finding, file:line, severity and closure bar in
   `task-N-findings.md` / the round brief. Keep minor fixes cheap and in scope.
2. **Fix:** one implementer in the task's isolated worktree, reading the brief
   and findings, not the whole plan. For behavior fixes, observe a regression
   fail against the pre-fix production path and pass with the change. A copied
   predicate that always passes is not evidence. Documentation-only corrections
   use concrete source/link/policy checks instead of invented Rust tests.
3. **Verify/commit:** run applicable Workflow gates; report exact BASE/HEAD,
   changes, RED/GREEN evidence and remaining limitations. One commit set per
   round; new test-only API needs the engineering-reference seam ruling.
4. **Package:** run `scripts/review-package [-C <worktree>] <plan> <round-base>
   <head> [outfile]` from the installed SDD helpers. Round BASE is the previous
   reviewed HEAD, and HEAD must descend from it. Output to the main-root SDD
   workspace explicitly when working from another worktree.
5. **Close:** unless Workflow's simple-task exception applies, an independent
   review seat checks each finding (addressed/open/partial), earlier closures,
   and new issues in the round diff. Do not restart the whole-task review.
6. **Ledger:** record
   `Task N: fix round R/5 (X addressed — …; Y open — …; commits a..b)`.
   Record explicit waiver/adjudication rather than silently dropping a finding.
   Integrate only after required closure and gates, then ledger the verified
   merge hash. After five rounds with open findings, stop and adjudicate with
   the controller before continuing.

Historical evidence for the closure bar: [closing review findings](../docs/wiki/process/closing-review-findings.md).
