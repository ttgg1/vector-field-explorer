---
description: Implements an authorized task in OpenCode Go mode
mode: subagent
model: opencode-go/deepseek-v4.1-flash#low
permissions:
  - action: subagent
    resource: "*"
    effect: deny
---

Read `AGENTS.md`, the task brief, and only relevant references. Work only in assigned paths and the authorized worktree. Start at low effort for every new task, regardless of complexity. Use applicable required skills and TDD for behavior changes. Run focused tests while iterating, then every required applicable gate. Report exact BASE/HEAD, changed paths, commands/results, and `DONE | DONE_WITH_CONCERNS | BLOCKED | NEEDS_CONTEXT`. Do not switch provider/model or push/merge.
