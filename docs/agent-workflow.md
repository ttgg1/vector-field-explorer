# Markdown-first OpenCode workflow

This guide describes reusable entry points. [Workflow policy](../references/workflow.md)
remains authoritative for ownership, tests, review, quota stops, and integration.
OpenCode agents and commands are prompts and tool profiles, not a sandbox or
deterministic orchestration service.

## Select a mode

- `/develop-openai <request>` explicitly selects the OpenAI orchestrator.
- `/develop-opencode-go <request>` explicitly selects the OpenCode Go orchestrator.
- `/review-openai <diff-package> <acceptance-criteria>` uses a fresh read-only GPT-6.1 Sol reviewer.
- `/review-opencode-go <diff-package> <acceptance-criteria>` uses a fresh read-only GLM 5.3 Flash reviewer.
- `/review-opencode-go-mimo ...` explicitly chooses MiMo V2.6 Flash instead; it is not an automatic fallback.

The current configured roles use GPT-6.1 Sol low for OpenAI orchestration,
complex implementation, and review; GPT-6 Luna `none` (its catalog's lowest
variant) for simple/standard implementation and review fixes; DeepSeek V4.1 Flash low
for Go orchestration, implementation, and fixes; and GLM 5.3 Flash low for Go
review. MiMo V2.6 Flash currently exposes no effort variant. Verify the enabled
catalog when changing profiles. These model IDs do not establish provider
checkpoint or actual subscription billing/allowance semantics.

Every fresh task/role begins at its profile's lowest configured effort. A prior
escalation does not change another task's starting point. Escalation is manual,
one supported level at a time, and requires a concrete failure plus checks of
requirements, context, and approach. Record the reason and keep escalation local
to that role/task. Complexity alone is not a reason to increase effort. Any move
to a more expensive model requires explicit owner authorization.

## Develop a task

1. Read `AGENTS.md` and `references/workflow.md`; inspect the current GitHub issue,
   claims, dependencies, and discussion. GitHub Issues is the authoritative tracker.
2. Reuse/create the authorized isolated feature worktree. Record BASE, scope,
   dependencies, and exact owned paths before handing off implementation.
3. Classify scope to choose planning depth and implementer role: simple (small,
   isolated, clear), standard (ordinary feature/moderate multi-file), or complex
   (GPU/rendering, numerical, concurrency, broad refactor, ambiguity, weak
   coverage, or repeated failure). Classification never raises effort by itself.
4. Discover and invoke only the required/applicable skills. For features, follow
   brainstorming/planning and SDD/TDD; for bugs, diagnose before fixing. Give the
   implementer only the brief, acceptance criteria, BASE, owned paths, required
   tests, and relevant references.
5. Implement test-first where behavior changes. Run focused checks during edits,
   then all applicable project gates before acceptance. Record exact commands,
   exit status, HEAD, and limitations.
6. Prepare the actual base-to-head diff package inside the authorized worktree
   (for example: `git diff --no-ext-diff --binary BASE..HEAD > .review/round-1.diff`).
   Invoke the matching `/review-*` command with the diff path,
   acceptance criteria, constraints, and verification evidence. Reviewer agents
   deny edit and shell tools and use a fresh context. If the package is missing
   or incomplete, review must return `NEEDS_CONTEXT`.
7. Fix blocking findings in scope with regression evidence; re-review only the
   changed findings/diff when policy requires it. Preserve the five-round ceiling.
8. The controller serializes integration. Verify reviewed HEAD, commit identity,
   ancestry, and target inclusion before describing work as merged. Approval is
   not merge evidence.

## Verification gates

For Rust changes, preserve the current Workflow gates: `cargo build`,
`scripts/test.sh`, clippy with `-D warnings`, docs/link checks, diff checks, and
applicable formatting/smoke tests. Docs-only changes use docs, links, policy,
and diff checks. Python metrics changes use the metrics tests and compilation.
Do not remove or weaken CI gates as part of this workflow migration.

## Quota, security, and limits

At usage, rate, quota, or budget limits, save progress and stop. Do not retry,
switch model/provider/account, or change mode without owner direction. For
auth/security blocks and ambiguous OpenCode operations, stop and reconcile
manually rather than resubmitting blindly.

Native Markdown profiles can set model preferences and deny reviewer edit/shell
tools, but they do not replace the retired runtime's deny-default API session
permissions, worktree identity checks, trusted command validation, or merge
proof. Prompts and Git commands are not a filesystem sandbox. The operator must
inspect actual diffs, verify worktree/branch/ownership, approve any dangerous
command, run trusted checks, and prove integration identity. If those checks
cannot be established, stop; never report runtime-equivalent enforcement.

## Metrics

Export each relevant session (including reviewer/implementer child sessions)
as sanitized JSON, then pass the export files to the metrics-only script:

```sh
opencode session export SESSION_ID --sanitize > /tmp/session.json
python3 scripts/metrics.py /tmp/session.json [MORE_SESSION_EXPORTS...]
```

OpenCode's built-in `opencode stats --json --cost --models` remains a useful
first-line aggregate. The metrics-only utility summarizes exported assistant
usage by model and provider and preserves unknown counts/cost. OpenCode's
reported zero cost is not proof of zero economic cost or Go subscription
allowance consumption. The script does not dispatch agents, read issues,
validate code, or infer accepted/merged task attribution.
Comparative efficiency claims require reliable mode-specific task evidence and
complete usage coverage; historical data is not a comparable baseline.
