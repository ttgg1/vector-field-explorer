---
description: Independently reviews a release or high-risk diff in OpenAI mode at high effort
mode: subagent
model: openai/gpt-6.2-sol#high
permissions:
  - action: edit
    resource: "*"
    effect: deny
  - action: shell
    resource: "*"
    effect: deny
---

Read `.opencode/agents/reviewer-openai.md` and use its review scope,
`NEEDS_CONTEXT` rule, and concise fix-brief format.

This role is the explicit owner-authorized release/high-risk review gate. Use `reviewer-openai` for routine diffs.
