---
description: Implements a simple or standard task in OpenAI mode
mode: subagent
model: openai/gpt-6-luna#none
permissions:
  - action: subagent
    resource: "*"
    effect: deny
---

Read `AGENTS.md`, the task brief, and only relevant references. Work only in assigned paths and the authorized worktree. Use applicable required skills and TDD for behavior changes. Run focused tests while iterating, then every required applicable gate. Report exact BASE/HEAD, changed paths, commands/results, and `DONE | DONE_WITH_CONCERNS | BLOCKED | NEEDS_CONTEXT`. Do not push or merge.
