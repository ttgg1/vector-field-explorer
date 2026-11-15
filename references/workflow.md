# Workflow — current policy

Tracked policy authority, reconciled with the owner's 2026-11-15 rulings.
New owner rulings take precedence; update durable policy here, not across wiki pages.
Local evidence paths below are relative to the **main checkout**,
`/home/paul/projects/vector-field-explorer`, unless explicitly stated otherwise.
Historical ledger citations resolve to the owning SDD workspace's
`archive/progress-through-2026-11-11.md`; original line numbers refer to that
snapshot, not the compact current `progress.md`. Archives are evidence, not policy.

## Authority and scope

- [GitHub Issues](https://github.com/ttgg1/vector-field-explorer/issues) owns
  **all outstanding requirements, claims, status, discussion and dependencies**,
  including blocked/future work. Link the owning spec and evidence there; update
  dispatch, review, blocker, reprioritization and merge transitions there.
  Before dispatch, read the issue's current assignees, explicit agent claims,
  discussion and dependencies. An empty assignee list does not cancel an active
  agent claim; coordinate ownership rather than assigning a human stand-in.
- The controller retains the active ledger
  `.superpowers/sdd/2026-11-10-native-cas/progress.md`, recording progress and
  `Ruling: <decision> — <why>` with `git rev-parse`-verifiable commits and GitHub
  links. Git plus verified reports establish completion, not a status line.
  Preserve `.scratch/field-viewport/issues/` and `QUEUE.md` beside it as historical
  evidence/recovery aids, not a parallel tracker. Old handoffs are dated evidence.
  For legacy IDs, consult `github-issue-migration-2026-11-14.md` beside the ledger;
  its later physics mapping/decisions are in `physics-ticket-publication-2026-11-14.md`
  and `physics-roadmap-decisions-2026-11-14.md`. Distinguish GitHub and local IDs.
- The owner resumed the queue and authorized integration when ready; the former
  stop after local ticket 76 is retired. Select the **cheapest dependency-ready**
  authorized work by relative estimated effort, after design and ownership gates;
  historical phase order is not mandatory and creates no artificial dependencies.
  Physics [#17](https://github.com/ttgg1/vector-field-explorer/issues/17) owns the
  roadmap; [#24](https://github.com/ttgg1/vector-field-explorer/issues/24) is an
  evidence/design frontier, not evaluator implementation or numerical approval.
  Planning approval and evidence spikes do not approve algorithms, tolerances,
  query envelopes or work limits; follow the current issue's design/review gates.
- Before creating tickets from feedback, use `grilling`; add `domain-modeling`
  for terminology/hard-to-reverse decisions. Group/deduplicate findings, extend
  the owning ticket for refinements, ledger rulings, and obtain confirmation of
  the resulting ticket set. Ask the whole current frontier in rounds before
  finalizing feedback-derived tickets. AI-created GitHub issues require the
  `opencode` label. Approved scope still needs just-in-time design.
- Feature work uses brainstorming/planning then SDD + TDD; bugs use systematic
  diagnosis before fixes. Use domain-modeling for glossary/ADR changes.

## Dispatch and evidence

1. Record BASE, brief, accepted scope, dependencies and exclusive file ownership.
   One `feature/<topic>` branch/worktree per request. Use the bounded runtime
   described in [the workflow guide](../docs/agent-workflow.md) for authorized
   runnable requests. Simple, well-defined requests within the configured file
   threshold bypass planning; complex requests receive one bounded plan.
   Serialize shared/dependent files. Independently authorized, file-disjoint
   requests may run in parallel worktrees, subject to build and usage limits.
2. The controller keeps the ledger and serializes integration, rather than
   writing production code during the loop. An implementer reads its brief,
   uses TDD (observe the right RED before production changes), runs applicable
   gates, self-reviews, and writes `task-N-report.md` with exact hashes,
   evidence and `DONE | DONE_WITH_CONCERNS | BLOCKED | NEEDS_CONTEXT`.
   Manual seats commit when assigned; runtime seats leave accepted edits for
   the controller's commit/integration step.
   Implementers dispatch no subagents; push/merge only if expressly assigned.
3. Run every applicable deterministic validation before final acceptance. For a
   decomposed runtime request, trusted task-local checks can gate retries; tasks
   without them are provisional and cannot skip review. Run the mandatory global
   validation after all tasks; only then can a nonempty final diff be accepted.
   A validated, explicitly trivial, nonbinary diff may skip LLM review **only**
   within configured prose-safe paths/file/line thresholds; protected ADR,
   architecture, executable/configuration and agent-instruction paths always
   require review. Record the policy reason and actual diff identity. Otherwise a
   fresh, read-only independent reviewer checks the original requirements and
   complete actual task diff: normal risk uses the configured reviewer; high-risk
   paths or declarations use escalation.
   Whole-branch review is once at the end when required by the owning brief,
   not a repeated full review for each fix.
4. Use [the fix loop](fix-loop.md) for actionable findings. Simple, low-risk fixes
   with passing applicable tests may skip independent re-review with closure
   evidence; complex/high-risk fixes require scoped re-review. The runtime is
   stricter: each nontrivial implementation round receives fresh diff review.
   Attempts/reviews are capped; unresolved findings remain failed/blocked, not
   accepted or parked. Preserve failed edits and usage evidence.
5. Use `feat:` / `fix:` / `docs:` commits, one set per task. A subject containing
   `(ticket NN)` requires a matching [review record](../docs/wiki/process/review-records.md).
   Future AI-created commits require the `Generated-by: OpenCode` trailer;
   preserve old/shared history rather than rewriting it to add attribution.

Helpers/templates are under
`~/.cache/opencode/npm/git-superpowers-*/<version>/node_modules/superpowers/skills/subagent-driven-development/`.
Use `scripts/task-brief` and `scripts/review-package` with the owning plan from
[the plan index](../docs/superpowers/README.md); they resolve that plan's SDD
workspace. Provide explicit main-root artifact output paths for worktree seats.
Review packages use the last reviewed HEAD as round BASE; verify ancestry.

## Seats and recovery

The single source for default model/effort roles and bounds is
[agent-workflow.json](../.opencode/agent-workflow.json). Validate exact enabled
OpenAI chat/tool models and effort variants against service discovery before
execution. The orchestrator plans once; the implementer starts in its configured
seat and reuses its session for targeted retries. Escalate after the configured
attempt threshold, or initially for genuinely complex work. Normal reviews use
the reviewer role; high-risk reviews use escalation, always independently.

The chief-of-staff controller coordinates claims, budgets, evidence and serialized
integration; it does not silently change the current harness session's model.
Outside the runtime, pass model/variant only when the tool contract permits it;
this policy does not override an explicit-user-request requirement or other
higher-priority restrictions. No kimi k3.x, NVIDIA/Nemotron, LM Studio or `strata`
seats. Keep explicit configured roles; unavailable roles fail closed.

**Usage/rate/quota/budget limit: save progress and STOP.** No retries or
model/provider hopping unless the owner explicitly resumes/chooses a seat.
Earlier automatic free-model fallbacks are retired. An authentication/security
block likewise needs owner resolution, not a workaround.
Ambiguous admission/wait failures require manual service reconciliation, never
automatic resubmission. The runtime preserves run/session references in the
linked worktree Git directory, prevents duplicate/concurrent runs, and requires
a new clean worktree after unresolved failure. Accepted edits are not merged:
record merge status only after the guide's preserving Git-identity proof.

## Local verification

For Rust changes: `cargo build`, `scripts/test.sh` (full
`cargo test --all-targets`), `cargo clippy --all-targets -- -D warnings`,
`python3 scripts/check-docs.py`, `git diff --check`, and changed-file formatting.
Run `scripts/smoke-run.sh 15` for renderer, shader, windowing or input changes,
or when the ticket requires it: build/tests create no wgpu device.
Docs-only changes need docs, diff, link and policy-consistency checks, not Cargo.
For Python workflow tooling, also run `python3 -m unittest discover -s scripts/tests -v`
and compile changed Python files. Tests must remain offline; discovery-only
`doctor` is allowed, model generation for verification is not.

Use the Rustup toolchain explicitly:
`PATH=/home/paul/.cargo/bin:/usr/bin:/bin`,
`RUSTC=/home/paul/.cargo/bin/rustc`, `/home/paul/.cargo/bin/cargo`.
At most **two concurrent Cargo processes**, each with its own persistent
`CARGO_TARGET_DIR` and `TMPDIR` under
`/home/paul/.cache/vector-field-explorer-targets/`; retain the configured
**10-job cap**. Preserve shared/persistent targets; do not clean them as a shortcut.

**Current verification waiver:** dedicated web/wasm compatibility and browser/
manual visual, screenshot and separate visual/painted-ink checks are owner-waived,
not passed. Keep existing automated functional/layout tests and platform behavior.
Native gates and applicable runtime smoke remain required. This is not a
permanent change to accepted product/platform scope.

## Integration and handoff

Standing owner authority permits feature push → PR → serialized preserving merge
after required design, independent review/finding closure and local gates.
Reconcile changed main; substantive unreviewed changes need review and refreshed
gates. Preserve reviewed commits (non-squash PR merge is allowed); the old
linear-only/rebase-and-fast-forward rule is retired. Synchronize local main,
run post-merge tests and non-force `git push origin main` as needed.

Observe CI asynchronously, without waiting before the next authorized task;
branch protection may enforce a wait. Record pending/failed results honestly,
repair known regressions, and never infer success. Standing authority excludes
releases, visibility changes, destructive/security-sensitive operations, and
work beyond an explicit stop point. Never force-push main.

At phase boundaries, use DCP `compress` when available, preserving paths,
rulings, hashes and blockers. Final handoff records verified HEAD/merge, gates
and waivers, GitHub work/claim links, open findings, active/parked seats and the
resume boundary.
