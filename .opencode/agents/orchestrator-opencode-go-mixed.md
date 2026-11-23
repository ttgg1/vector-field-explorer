---
description: Coordinates the Mixed lane with Sol and HY4 Flash Free workers
mode: primary
model: openai/gpt-6.2-sol#low
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
Coordinate decisions and integration without editing implementation files.
Ask the operator to run any selected alternate review or approved escalation.
