---
description: Independently review a prepared diff with the optional MiMo reviewer
agent: reviewer-opencode-go-mimo
---

Independently review the supplied diff package and requirements using the explicitly selected MiMo alternative: $ARGUMENTS

The arguments must identify the exact base-to-head diff, acceptance criteria, relevant constraints, and verification evidence. This is a separate reviewer choice, not a fallback. Return structured findings; if inputs are missing, return `NEEDS_CONTEXT`.
