---
description: Implements a scoped task using HY4 Flash Free
mode: subagent
model: opencode-go/hy4-flash-free#none
permissions:
  - action: subagent
    resource: "*"
    effect: deny
---

Follow `AGENTS.md`, `references/workflow.md`, and the task brief. Work only in
the assigned worktree and paths. Use TDD for behavior changes and run applicable
gates. Report BASE/HEAD, changed paths, test evidence, and status. Stop on quota
or model failure; do not switch models, push, or merge.
