# Runnable OpenCode workflow

Use this runner for an **authorized, trusted request** in a clean linked Git
worktree. [Workflow](../references/workflow.md) remains the policy authority;
GitHub claims and the controller's integration authority are unchanged. This
tool accepts edits; it never commits, pushes or merges them. It does not switch
the model of the session reading this guide.

## Run a request

Prerequisites: Python 3.10+ on Linux, Git, and the authenticated OpenCode **V2**
CLI/service already configured by the owner. No SDK, pip install, global config
edit, custom agent profile or warm-up generation is required.

```sh
python3 scripts/agent_workflow.py --help
python3 scripts/agent_workflow.py doctor --offline
python3 scripts/agent_workflow.py doctor
```

`doctor` validates the complete [role/limit config](../.opencode/agent-workflow.json)
and discovers exact enabled OpenAI chat/tool models and effort variants using
`opencode api GET /api/model`. It creates no sessions and generates nothing.
`--offline` validates configuration only. An unavailable role fails closed.
That JSON file is the only default model/effort source; do not duplicate seats
in service configuration. Models and variants are recorded for each run.

Create a linked worktree on a feature branch, then put the trusted request
**outside** it (an untracked manifest would make it dirty). For example, from
the repository containing these scripts:

```sh
git worktree add -b feature/docs-workflow-example /tmp/opencode/workflow-example HEAD
cp .opencode/request.example.json /tmp/opencode/workflow-request.json
python3 - <<'PY'
import json, subprocess
from pathlib import Path
p = Path('/tmp/opencode/workflow-request.json')
r = json.loads(p.read_text())
r['expected_head'] = subprocess.check_output(
    ['git', '-C', '/tmp/opencode/workflow-example', 'rev-parse', 'HEAD'], text=True).strip()
p.write_text(json.dumps(r, indent=2) + '\n')
PY
python3 scripts/agent_workflow.py --root /tmp/opencode/workflow-example run /tmp/opencode/workflow-request.json
```

The [complete example manifest](../.opencode/request.example.json) has all
required fields; its zero HEAD is intentionally unusable until replaced. Use a
fresh unique `id`, exact full `expected_head` and exact `expected_branch`.
Requirements and acceptance criteria are nonempty text arrays; ownership is a
nonempty list of **exact files**, not directories/globs. `context` is an array
of `{file}` or `{file,start,end}` entries, with inclusive one-based lines (it
may be empty). `complexity` is `simple|complex`, `risk` is
`trivial|normal|high`. `validation` is a nonempty list of trusted argv arrays.
Optional `task_validation` maps planned task IDs to nonempty lists of trusted
argv arrays. The planner may associate these IDs with tasks, but cannot create or
alter commands. Without a task-local check, a decomposed task remains provisional
and cannot use the trivial-review skip; complete request validation always runs
after all tasks and is required for final acceptance.
Extra/duplicate keys, invalid types, traversal, symlink aliases/escapes and
empty acceptance/ownership/validation are rejected. CLI JSON input is capped
at 1 MiB. Context, actual diffs, model responses, validation output and API
transport/timeline pages have explicit bounds; exceeding one is an error,
never a silently shortened review. Non-UTF-8 textual evidence fails explicitly;
Git binary patches are retained and always reviewed.

To override limits, copy and edit the **whole** config outside the worktree;
the override replaces rather than merges it:

```sh
python3 - <<'PY'
import json
from pathlib import Path
c = json.loads(Path('.opencode/agent-workflow.json').read_text())
c['limits']['max_attempts'] = 2
Path('/tmp/opencode/workflow-config.json').write_text(json.dumps(c, indent=2) + '\n')
PY
python3 scripts/agent_workflow.py --config /tmp/opencode/workflow-config.json doctor --offline
```

Pass that same `--config` before `run` when using the override. The CLI exits
nonzero on rejected input or failed/blocked work; run failures return persisted
JSON evidence, while preflight errors go to stderr. Inspect `status`, not only
stdout existence. The runner assumes a trusted repository, Git configuration,
OpenCode service/plugins, manifest and config; it is not a hostile-repository
sandbox.

## Control flow and security boundary

- A simple request within `simple_max_files` bypasses planning. Otherwise the
  orchestrator gets the request and targeted context once, returning a bounded
  structured task list. Duplicate IDs, unknown dependencies, cycles, command
  injection fields or ownership expansion fail. Tasks run serially, including
  shared files. Validation commands come only from the trusted manifest.
- Complex requests start with escalation; others start with the implementer.
  Attempts are durably incremented before model work. Targeted retries reuse
  the implementer session, sending actionable validation/review feedback and
  diff/evidence pointers instead of repeating unchanged context. Configured
  escalation switches the model/effort for subsequent turns in that session.
- The controller runs configured task-local validation during that task's retry
  loop, recording exits. In a decomposed request, tasks without local checks
  remain provisional and receive independent review; the controller runs **every**
  global validation argv only after all tasks, as the mandatory final acceptance
  gate. A failed final gate preserves edits and prior task reviews but accepts no
  changes; recovery is explicit rather than guessing which task to reopen. For a
  single task, global validation also serves its retry loop. Commands record exits,
  output and time/output failures. Commands must leave files unchanged, write
  generated output outside the worktree and terminate within the bounds.
  Python bytecode writing is disabled in the CLI and validation environment.
  A passing command with no real diff is not acceptance. Ownership includes
  new/deleted/renamed/binary files; snapshots include ignored files as well.
- Actual path/count/binary/risk policy determines review. Only validated safe
  trivial diffs within config thresholds skip review, with a recorded reason.
  Otherwise a fresh independent reviewer receives original requirements,
  acceptance, validation results and the **complete incremental task diff**.
  Strict JSON findings must reference that diff's files and be actionable.
  Approvals with blocking findings, malformed output and `block` fail closed;
  `request_changes` feeds a bounded retry. Nonblocking minor findings remain
  in evidence. Reaching either cap with unresolved work is failure.
- Sessions use primary builtin `build` plus explicit **service-enforced**
  deny-default per-session rules. Only declared/context files can be read;
  implementers may edit only task-owned files. Planners/reviewers get no edit
  grants. Shell, subagent, Code Mode, external-directory and unlisted/plugin
  actions are denied. Glob/grep are not granted because their resource
  permissions cannot narrow search paths reliably; supply needed context
  explicitly. The returned session must confirm its agent/model/location and
  exact rules. Existing service hard-denies still apply; this tool never
  answers an approval request or grants a blanket allow.
- Validation argv is **arbitrary user-approved code with the host user's
  authority**, not restricted agent tooling. `shell=True` is never used, but
  `['sh','-c',...]`, Python, Git hooks/configuration or subprocesses may still
  execute arbitrary code. Process groups, time and output limits are resource
  guards, not a filesystem/network sandbox; a deliberate `setsid` escape is
  outside them. Approve argv only if that authority is intended.

Before/after each agent round/session change and validation, HEAD and branch must match
the trusted start identity. Agent edits survive every failure; there is no
reset, clean, commit or rollback. Symlinked files, submodules, special files,
preexisting ignored output and noncanonical filter/EOL worktrees are rejected
rather than weakening snapshot identity. Preserve reviewed commits using the
controller's integration process, not an agent shell.

## Durable evidence and explicit recovery

Evidence lives at the path printed by
`git rev-parse --absolute-git-dir`, beneath `agent-workflow/runs/ID.json`.
Atomic, fsynced saves retain config/catalog, tasks, attempts, validation results,
session/admission references, events, actual diffs/trees and message usage on
both success and failure. Git blobs/trees retain accepted content without
committing it. The worktree's exclusive `agent-workflow/lock` prevents concurrent
controllers. A persistent ID is never reused; a failed, blocked or interrupted
run prevents whole-run restart in that worktree, even with a different ID.

OpenCode prompt POST is **admission only**. The adapter waits, follows bounded
timeline pagination and requires the admitted user ID followed by a completed
round and terminal idle. Prior idle is insufficient. It collects all assistant
message usage, deduplicated by session/message ID, including failed rounds.
Authentication/security/rate/quota/budget failures stop without retry or
provider/model hopping. Only explicitly completed non-security model errors
may retry within the attempt cap. Admission/wait timeouts, missing timelines,
unknown session creation IDs and malformed/length-limited results stop; no
blind POST resubmission or automatic resume exists.

On interruption or failure:

1. Preserve the run JSON, Git objects, worktree edits and stale lock (if any).
   Inspect the stored session/admission IDs in the existing OpenCode service;
   an ambiguous call may still be running. Stop/reconcile it explicitly before
   starting replacement work. Do not assume killing the CLI stopped the service.
2. A human/controller may remove **only** the stale lock after verifying the
   recorded process and associated sessions are no longer active. Do not erase
   run files or manufacture a successful status. There is no resume command.
3. Retry the whole request in a **new clean linked worktree** with a new ID and
   an updated expected identity; carry forward reconciled evidence and useful
   edits explicitly. A usage/security stop also needs the owner's resolution.

JSON saves are local evidence, not a tamper-proof audit log. Abrupt termination
can leave a pending call with unknown usage; metrics keeps that uncertainty.
Finite controller bounds cannot disable the service/provider's own internal
retry policy, so configure that separately if stricter spend controls are needed.

## Acceptance, preserving merge proof and metrics

An accepted run leaves uncommitted edits. After independent integration
authorization, create **one single-parent commit directly on the recorded
starting HEAD**, with exactly the accepted tree (including modes/new files and
deletions). Include it verbatim in the target ref using preserving merge or
fast-forward. Then, from the original linked worktree, record it:

```sh
python3 scripts/agent_workflow.py record-merge RUN_ID --commit FULL_ACCEPTED_COMMIT --target main
python3 scripts/agent_workflow.py metrics
python3 scripts/agent_workflow.py metrics /absolute/path/to/run-one.json /absolute/path/to/run-two.json
```

The proof checks the direct baseline parent, exact whole-tree and stored patch
identity, ancestry in the named target ref, and records full base/commit/tree/
target hashes. A preexisting ancestor, different patch, cherry-pick, squash,
multi-commit reconstruction or extra unrelated commit contents is rejected,
even when superficially similar. Recording does not perform integration.
Retain the original worktree/Git-dir evidence until recording and archiving;
do not delete its Git objects as a cleanup shortcut.

`metrics` aggregates all role messages and failed attempts, reporting raw input,
output, reasoning, cache-read/write counts, coverage, actual positive reported
costs, attempts/reviews/rejections/escalations and per-task validation/status.
Only tasks from a run that passes its final global gate and has a nonempty final
diff count as accepted; historical provisional approvals remain in run evidence
but do not enter acceptance denominators after final failure. Accepted and
Git-proven merged work have separate denominators. The merged-line
proxy counts **nonblank added source lines in the final accepted patch**,
excluding docs/tests/generated/vendor/build/config paths and nonsource suffixes;
shared files are attributed once to their final editing task. It counts comments
and replacements as additions, not net growth, correctness, quality or value.

Raw cache counts are independent, not subtracted from input. Reasoning/output
semantics default to **unknown**; aggregate token totals and price estimates
remain unknown until explicitly configured. Matching nontiered catalog prices
or trusted configured per-million rates may support estimates, but prices are
never invented. A reported zero cost without known pricing/explicit coverage is
not assumed free. Incomplete/failed calls with unavailable usage remain unknown,
not zero; zero/unknown denominators have unknown ratios. Static first-turn
prefixes and reused implementer sessions permit provider caching, but only
reported cache hits are measured—there are no warm-up calls or cache promises.

## Offline verification

```sh
python3 -m unittest discover -s scripts/tests -v
python3 -m py_compile scripts/agent_workflow.py scripts/workflow/api.py scripts/workflow/runner.py
python3 scripts/check-docs.py
git diff --check
```

CI runs the same stdlib tests without model generation, including a fake CLI
service end-to-end run/merge-record/metrics flow and real temporary linked Git
worktrees. Rust jobs are unchanged; tooling-only work does not need Cargo gates.
