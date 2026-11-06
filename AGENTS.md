# Vector Field Explorer — AI Workflow

OpenCode superpowers framework + SDD (subagent-driven development / executing-plans)
workflows. Every implementation run must leave a record: ledger + commits, not memory.

App: Rust eframe/egui GUI to explore vector fields (`f(x,y)` or `f(x,y,z)`)
typed as formulas — live arrows, streamlines, particles, camera presets,
colormaps. Vision spec: `docs/superpowers/specs/2026-11-05-vector-field-explorer-design.md`.

## Implementation Plans & Status (verified against git 2026-11-06)

| Plan | Scope | Status |
|------|-------|--------|
| `docs/superpowers/plans/2026-11-05-preset-persistence.md` | Camera presets: `export::Preset` serde, save/load/delete to `presets.json`, sidebar Preset panel, app data-dir config | **COMPLETE** (Tasks 1–3; commits `037825f`, `40f98f4`, `87c4e23`, `2dcb1a6` on `feature/preset-persistence`; final review pass done → fix commit `331d72e`, no re-review needed per skill). Ruled behaviors: duplicate-name save replaces, corrupt `presets.json` fails fast with io::Error; RGBA (not BGRA) PNG contract pinned by test |
| `docs/superpowers/plans/2026-11-05-vector-field-explorer.md` | The full app, 14 tasks T1–T14 | **T1–T6 complete** (T1 `caa7c09` shell; T2 `62b1c5c` field; T3 `fee0b18`+`e13b7b7` domain/field-presets; T4 `8fd8701` colormaps; T5 `a6790f8` sampling → `src/sampling.rs`; T6 `7e37254` OrbitCamera → `src/camera.rs`); **T7 implemented** (`81d1b30` instanced wgpu renderer, `shaders.arrow.wgsl`/`shaders.line.wgsl`/`renderer.rs`) and reviewed — verdict "Needs fixes": **IMPORTANT pending fix = arrow shader lights local-space normal against world-space light** (see `.scratch/field-viewport/issues/01-arrow-lighting.md` + ledger), minors list there too. **T8–T14 remain** (viewmodel seam + UI wiring incl. rebuilding the `Controls` — check `task-1-brief.md`, streamlines, particles, 3D seeding, style panel, screenshot, README/cleanup)

**Spec for the remaining work (published, ready-for-agent):** `.scratch/field-viewport/spec.md` — seams confirmed with the user: existing pure-module seams + exactly ONE new seam (viewmodel/scene-builder of pure functions state→SceneGPU, input→camera actions); GPU paint stays behind SceneGPU, no unit tests. Issue tracker: local markdown under `.scratch/field-viewport/issues/`. |

Ledgers (git-ignored SDD workspaces, authoritative progress record):
- `.superpowers/sdd/2026-11-05-preset-persistence/progress.md`
- `.superpowers/sdd/2026-11-05-vector-field-explorer/progress.md`
  ⚠ correction recorded 2026-11-06: its old "Task 5/6 complete" lines were
  phantom (commits `a1b2c3d4e5f6g7h8i9j0` / `a916cc0` exist in no ref). Never
  ledger a task without a `git rev-parse`-verifiable commit hash.

Module map (branch state may lag; trust `git log` + ledger for what's merged):
`src/field.rs` (parse/eval via meval), `src/domain.rs` (grid), `src/colormap.rs`
(Viridis/Turbo/Mono via colorous), `src/presets.rs` (field-preset table — note:
a *different* `Preset` type than `export::Preset`, which is the camera preset),
`src/sampling.rs` (`sample_field`, `ArrowData`, `ViewOptions`, `magnitude_range`),
`src/export.rs` (PNG/presets), `src/i18n.rs` (locale JSONs in `data/locales/`),
`src/render.rs` (2D pan/zoom `Camera`), `src/camera.rs` (3D `OrbitCamera`),
`src/ui/presets.rs` (PresetPanel).

## Conventions (mandatory)

- **TDD**: no production code without a failing test first (superpowers:test-driven-development).
  Watch RED fail for the right reason; keep the whole `cargo test` suite green per task.
- Commits: `feat: <description>` / `fix: …` / `docs: …`, one commit set per task.
- Record progress in the plan's SDD ledger (`task-start` / `task-done` scripts
  under `~/.cache/opencode/npm/git-superpowers-*/…/skills/{executing-plans,subagent-driven-development}/scripts/`),
  never in memory or todos alone.
- Isolated work: feature branch (`feature/<topic>`) or git worktree; never on `main`.
- Resolution ledgering (`Ruling: <decision> — <why>` in the ledger) is mandatory.
- CI (`.github/workflows/ci.yml`, Rust-based) gates on `cargo clippy --all-targets -- -D warnings`,
  `cargo test --all-targets`, `cargo build --all-targets` — keep code clippy-clean.

## Egui / Rust API notes (learned the hard way)

- **egui 0.37**: `App` trait method is `fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame)`
  — there is **no** `update(ctx, frame)`. Panels are the unified
  `egui::Panel::top/bottom/left/right(id)` with `.resizable()`, shown against the
  `Ui`; `egui::CentralPanel::default_margins().show(ui, …)`.
- tests in `tests/` are integration tests: import `vector_field_explorer::{…}` —
  bare `crate::` / `use super::*` will not resolve.
- Rust edition 2024: `std::env::set_var` / `remove_var` are `unsafe` — wrap them.
- wgpu usage must go through eframe's re-exports; `rfd` was removed from deps
  (re-add intentionally when screenshot dialogs land).

## Subagent Usage (SDD mode — active from 2026-11-06)

Implementation runs in **subagent-driven-development (SDD)** mode: the session
acts as controller and dispatches a fresh **implementer** subagent per task,
then a **task reviewer** subagent against the task's diff; the controller
keeps the ledger, never edits code during the loop, and fixes/moves on only
after resolving review findings (read `references/fix-loop.md` before every
fix round). A broad whole-branch review runs once at the end.

Skill location (scripts + prompt templates):
`~/.cache/opencode/npm/git-superpowers-*/…/skills/subagent-driven-development/`
— `implementer-prompt.md` and `task-reviewer-prompt.md` are the dispatch
templates; `scripts/task-brief`, `scripts/review-package` generate the brief
and diff-package artifacts; never make a subagent read the whole plan.

**Model policy (user-selected 2026-11-06):**
- **Implementer subagents: `qwen3.8-27b`** — subagent model id
  `lmstudio/qwen/qwen3.8-27b` (LM Studio local, "slow"; always pass the model
  explicitly — subagents never silently inherit the session model).
- Reviewer subagents and the final whole-branch review: most capable
  available model (e.g. `opencode/nemotron-3.5-super-vl-free`).
- If an implementer status is BLOCKED/NEEDS_CONTEXT twice, escalate that task
  to a more capable model instead of looping.

**Implementer contract (in every dispatch):** work from the task brief file
only (never the whole plan); TDD when the brief requires tests (RED→GREEN
watched); run the full `cargo test` suite before committing; commits
`feat:/fix:/docs:`; self-review before reporting; never dispatch sub-subagents
(review is the controller's job); report status
DONE | DONE_WITH_CONCERNS | BLOCKED | NEEDS_CONTEXT + commits + one-line test
summary, full report to `…/task-N-report.md`.

Dispatch flow per task:
1. Record BASE (`git rev-parse HEAD`).
2. `bash scripts/task-brief PLAN_FILE N` → brief path.
3. Dispatch implementer (qwen3.8-27b) with: how the task fits, brief path,
   interfaces/decisions earlier tasks established, ambiguity resolutions,
   report-file path + report contract.
4. On DONE: `bash scripts/review-package PLAN_FILE BASE HEAD` → diff file.
5. Dispatch task reviewer (capable model) with brief, report, diff paths +
   binding global constraints; resolve findings via the fix loop (resume the
   same implementer session with `sessionID`); ledger completion line.

## AI Workflow Integration

New feature modules follow the plans; write tests automatically alongside;
run `cargo test` (and later `cargo clippy -- -D warnings`) before claiming done.
All generated code lives under `src/` or `tests/`.

## Agent skills

### Issue tracker
Issues live as local markdown under `.scratch/<feature-slug>/` — no git
remote, no gh/glab. See `docs/agents/issue-tracker.md`.

### Domain docs
Single-context: `GLOSSARY.md` at repo root + `docs/adr/`, created lazily by
`/domain-modeling`. See `docs/agents/domain.md`.
