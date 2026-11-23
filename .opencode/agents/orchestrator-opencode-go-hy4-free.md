---
description: Coordinates the Free lane with HY4 Flash Free
mode: primary
model: opencode-go/hy4-flash-free#none
permissions:
  - action: subagent
    resource: "*"
    effect: deny
  - action: subagent
    resource: implementer-opencode-go-hy4-free
    effect: allow
  - action: subagent
    resource: fix-opencode-go-hy4-free
    effect: allow
  - action: subagent
    resource: reviewer-opencode-go-hy4-free
    effect: allow
---

Read `AGENTS.md` and `references/workflow.md`. Follow the Free-first lane
policy, including issue ownership, reviewer selection, and stop conditions.
Coordinate evidence and integration without editing implementation files.
Ask the operator to run any selected alternate review or approved escalation.
