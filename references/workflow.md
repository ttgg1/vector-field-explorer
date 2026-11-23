# Workflow — current policy

Tracked policy authority, reconciled with the owner's 2026-11-16 rulings.
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
- The controller retains the active plan ledger
  `.superpowers/sdd/<plan-name>/progress.md`, recording progress and
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
- Before creating tickets from feedback, use the installed `grilling`; add
  `domain-modeling` for terminology/hard-to-reverse decisions. Group/deduplicate findings, extend
  the owning ticket for refinements, ledger rulings, and obtain confirmation of
  the resulting ticket set. Ask the whole current frontier in rounds before
  finalizing feedback-derived tickets. AI-created GitHub issues require the
  `opencode` label. Approved scope still needs just-in-time design.
- Feature work uses brainstorming/planning then SDD + TDD; bugs use systematic
  diagnosis before fixes. Use `domain-modeling` for glossary/ADR changes and
  `writing-for-agents` for agent-facing docs. Use `handoff` only when transferring
  an active task to another session; do not load all skills by default.

## Dispatch and evidence

1. Record BASE, brief, accepted scope, dependencies, and exclusive file ownership.
   Use one `feature/<topic>` branch/worktree per request and the explicit native
   OpenCode entry points in [the workflow guide](../docs/agent-workflow.md).
   Clear, bounded tasks need no separate plan artifact; applicable skills such
   as brainstorming and TDD still apply. Serialize shared files; parallelize
   only independent, file-disjoint, dependency-ready tasks when worthwhile.
2. The controller keeps the ledger and serializes integration. The implementer
   reads its brief, uses TDD (observe the right RED before production changes),
   runs applicable gates, self-reviews, and reports exact BASE/HEAD, evidence,
   and `DONE | DONE_WITH_CONCERNS | BLOCKED | NEEDS_CONTEXT`. It does not push
   or merge unless expressly assigned.
3. Run every applicable deterministic validation before acceptance. A validated,
   explicitly trivial, nonbinary docs-only diff of at most 30 changed lines and
   at most two Markdown files may skip LLM review **only** when every file is
   root-level Markdown or under `docs/`, and formatting, link, policy, and diff
   checks pass. ADR, architecture, executable/configuration, and agent-instruction
   paths always require review. Record the reason and diff identity. Otherwise,
   a fresh read-only independent reviewer checks the original requirements and
   complete actual diff. High-risk work still requires independent review; effort
   increases only through the documented task-local escalation process, except the
   explicit owner-authorized release-review role defined in
   [Native roles and recovery](#native-roles-and-recovery).
4. Use [the fix loop](fix-loop.md) for actionable findings. Simple, low-risk fixes
   with passing applicable tests may skip independent re-review with closure
   evidence; complex/high-risk fixes require scoped re-review. Limit a task to
   three implementation attempts and three review rounds; the finding fix loop
   stops after five rounds. Unresolved work remains failed/blocked, not accepted
   or parked. Preserve failed edits and usage evidence.
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

## Native roles and recovery

Select roles through the native Markdown agents and commands under `.opencode/`.
Implementers do not delegate; only orchestrators dispatch approved implementer,
reviewer, and fix roles.
Validate configured model identifiers and variants against the enabled catalog
when adding or changing profiles. `openai` is the default mode; use OpenCode Go
only when explicitly selected. Complexity guides planning depth and role, but
never automatically raises reasoning effort. Every new task/role starts at its
profile's lowest supported effort. Escalate one supported level at a time only
after checking requirements, context, and approach; record the failure and
reason. Escalation is local to that task/role. A new independent task resets to
lowest effort. More expensive model changes require explicit owner authorization.

OpenAI: GPT-6.2 Sol low for orchestration, simple/standard implementation,
routine review, and scoped fixes. GPT-6.2 Astra uses its lowest supported
variant (`low`) for complex implementation and release-blocking/high-risk fixes,
because the 6.2 family exposes no `none`. The explicit `reviewer-openai-release`
role uses GPT-6.2 Sol high for the owner-authorized release/high-risk review
gate; every other review starts at low. OpenCode Go: DeepSeek V4.1 Flash low for
orchestration, implementation, and fixes; GLM 5.3 Flash low for independent
review. MiMo V2.6 Flash is an explicit alternative reviewer; the current catalog
 exposes no variant for it.

### Free-first lanes (opt-in)

Select an entry point and record the lane before dispatch. Existing OpenAI and
OpenCode Go modes are unchanged.

| Lane | Controller | Implementer, fixer, routine reviewer |
| --- | --- | --- |
| `/develop-opencode-go-free` | HY4 Flash Free `none` | HY4 Flash Free `none` |
| `/develop-opencode-go-mixed` | GPT-6.2 Sol `low` | HY4 Flash Free `none` |

Use Free when the acceptance criteria, owned paths, and checks are clear. If
design or task decomposition is the only hard part, use a bounded Sol planning
pass and hand the approved brief to Free. Use Mixed when substantive design or
sequencing decisions are expected between rounds. In Mixed, workers run routine
checks; bring back decisions, blockers, and integration evidence, not progress
recaps.

Choose the reviewer before dispatching a review, after inspecting the actual
diff and deterministic gate results. Use a fresh, read-only HY4 reviewer for
bounded, well-tested diffs. Ask the owner to select `/review-openai` (Sol low)
instead for material cross-module contract or persistence changes, or when
tests leave a material behavior question unresolved. For the required
release/high-risk gate, use `/review-openai-release` (Sol high) instead. Do not
run a routine review before a selected Sol review. Beam Free `low` is an
explicit optional second opinion, not a fallback or substitute for a required
gate. Check data eligibility before sending a review package; in particular,
[OpenCode Go lists 30-day retention for Beam Free](https://opencode.ai/docs/console/go/#privacy).

Both lane controllers can dispatch only the three HY4 roles. The operator
invokes any alternate reviewer or approved escalation separately and returns
its report to the lane controller. Escalate only the blocked role: record the
failure, exact HEAD, expected deliverable, and owner authorization; then resume
the chosen lane. Existing effort rules, stop conditions, verification, and
[finding closure](fix-loop.md) still apply.

### Catalog and stop conditions

Catalog discovery confirms model IDs/capabilities, not internal checkpoints,
actual billing, or subscription allowance consumption.

Do not use Kimi K3.x, NVIDIA/Nemotron, LM Studio, or Strata seats. If a configured
model is unavailable or fails, stop and request owner direction; do not fall back.

The chief-of-staff controller coordinates claims, budgets, evidence and serialized
integration; it does not silently change the current harness session's model.
Use the selected native role's model preference only when the OpenCode/tool
contract permits it; this policy does not override explicit user requirements
or higher-priority restrictions. Never silently change provider or account.
Unavailable roles require owner resolution rather than fallback.

**Usage/rate/quota/budget limit: save progress and STOP.** No retries or
model/provider hopping unless the owner explicitly resumes/chooses a seat. An
authentication/security block likewise needs owner resolution, not a workaround.
Preserve session IDs, relevant logs, worktree, and edits for manual reconciliation;
do not resubmit an ambiguous request blindly. A review approval is not a merge:
verify commit identity, reviewed HEAD, ancestry, and inclusion in the intended
target before reporting merged status.

## Local verification

For Rust changes: `cargo build`, `scripts/test.sh` (full
`cargo test --all-targets`), `cargo clippy --all-targets -- -D warnings`,
`python3 scripts/check-docs.py`, `git diff --check`, and changed-file formatting.
Run `scripts/smoke-run.sh 15` for renderer, shader, windowing or input changes,
or when the ticket requires it: build/tests create no wgpu device.
Docs-only changes need docs, diff, link and policy-consistency checks, not Cargo.
For metrics tooling, run `python3 -m unittest discover -s scripts/tests -v`
and compile changed Python files. Tests remain offline and consume fixtures;
never generate model output as verification.

Use the Rustup toolchain explicitly:
`PATH=/home/paul/.cargo/bin:/usr/bin:/bin`,
`RUSTC=/home/paul/.cargo/bin/rustc`, `/home/paul/.cargo/bin/cargo`.
At most **two concurrent Cargo processes**, each with its own persistent
`CARGO_TARGET_DIR` and `TMPDIR` under
`/home/paul/.cache/vector-field-explorer-targets/`; retain the configured
**10-job cap**. Preserve shared/persistent targets; do not clean them as a shortcut.

**Disk and workspace hygiene.** Before dispatch, check free space
(`df -h /home`) and keep per-task Cargo targets bounded; a full filesystem
aborts implementer gates and is not a task failure. When a slice merges,
delete that slice's own `CARGO_TARGET_DIR`, which is the sanctioned exception
to the preservation rule above; keep the shared post-merge-gate target
`/home/paul/.cache/vector-field-explorer-targets/main-check`. Remove the
merged worktree and branch and prune deleted remote branches at each merge
boundary rather than in a later cleanup pass. Copy out any unmerged evidence
before removing a worktree.

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

At each phase boundary (a task closes and the next is not yet dispatched), run
a DCP compression pass (the model `compress` tool, or `/dcp-compress` from the
TUI) and direct its focus to preserving paths, rulings, hashes, blockers,
reviewed HEADs and open findings; re-read the plan ledger before dispatching the
next task. DCP is configured globally as `@tarquinen/opencode-dcp`; if neither
entry point is available, install it with
`opencode plugin add @tarquinen/opencode-dcp@latest` and restart OpenCode
rather than silently skipping compression. Compression supplements, never
replaces, the durable plan ledger and evidence files, which remain recovery
authority. Final handoff records verified HEAD/merge, gates and waivers, GitHub
work/claim links, open findings, active/parked seats and the resume boundary.
