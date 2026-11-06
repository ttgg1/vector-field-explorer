# AI Workflow & Subagent Usage

This repository uses the OpenCode superpowers framework with **subagents** to implement complex tasks in parallel or sequentially.

## Subagents Overview

| Task | Description | Subagent Used |
|------|-------------|----------------|
| Export PNG | Generate a dummy frame and write to disk. | `general` (export-tests) |
| Camera Zoom/Pan UI | Add zoom & pan controls in the UI. | `general` (camera-zoom-pan) |
| Preset Persistence | Save, load, delete camera presets. | `general` (preset-persistence) |
| i18n Skeleton | Load locale JSONs and provide a `t!()` macro. | `general` (i18n-skeleton) |
| GPU Instancing Helper | Provide an `Instancer` struct for future rendering. | `general` (render-optimization) |
| CI Workflow | GitHub Actions for lint, test, release. | `general` (ci-workflow) |

## How to Use Subagents

1. **Create a branch** for the task: `git checkout -b feature/<task>`. 
2. **Dispatch a subagent** via the OpenCode harness:
   ```bash
   # Example
   opencode subagent --agent general \
       --description "Implement export PNG" \
       --prompt "Add save_png and tests"
   ```
3. **Review** the branch once the subagent finishes.
4. **Merge** into `main` after passing tests.

## AI Workflow Integration

The repository is structured to allow future integration with LLM‑driven agents that can:
- Generate new feature modules based on specifications.
- Write tests automatically.
- Run static analysis and suggest refactors.

All generated code lives under `src/` or `tests/`, and commit messages follow the pattern `feat: <description>` for clear history.
