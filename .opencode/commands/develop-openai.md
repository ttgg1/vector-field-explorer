---
description: Start a repository task in the OpenAI workflow mode
agent: orchestrator-openai
---

Handle this request in explicitly selected OpenAI mode: $ARGUMENTS

Follow `AGENTS.md` and `references/workflow.md`. Check issue ownership and dependencies, select the lightest applicable skill workflow, and establish BASE, isolated worktree, and exact file ownership before dispatch. Never silently change mode/provider. New task roles start at their configured lowest effort.
