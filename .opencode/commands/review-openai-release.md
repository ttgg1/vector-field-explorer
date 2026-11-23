---
description: Independently review a release or high-risk diff with the OpenAI release reviewer
agent: reviewer-openai-release
---

Independently review the supplied diff package and requirements: $ARGUMENTS

Use this entry point for an explicit release gate or a high-risk change. The
arguments must identify the exact base-to-head diff, acceptance criteria,
relevant constraints, and verification evidence. Use the reviewer profile's
fix-brief format.
