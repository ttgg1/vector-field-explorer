# Vector Field Explorer — AI Workflow

OpenCode superpowers framework + SDD (subagent-driven development / executing-plans)
workflows. Every implementation run must leave a record: ledger + commits, not memory.

App: Rust eframe/egui GUI to explore vector fields (`f(x,y)` or `f(x,y,z)`)
typed as formulas — live arrows, streamlines, particles, camera presets,
colormaps. Vision spec: `docs/superpowers/specs/2026-11-05-vector-field-explorer-design.md`.

License: **GPL-3.0-only** (`LICENSE`; `Cargo.toml` carries
`license = "GPL-3.0-only"`). Private GitHub remote:
`git@github.com:ttgg1/vector-field-explorer.git`.

## Implementation Plans & Status (verified against git 2026-11-06)

| Plan | Scope | Status |
|------|-------|--------|
| `docs/superpowers/plans/2026-11-05-preset-persistence.md` | Camera presets: `export::Preset` serde, save/load/delete to `presets.json`, sidebar Preset panel, app data-dir config | **COMPLETE** (Tasks 1–3; commits `037825f`, `40f98f4`, `87c4e23`, `2dcb1a6` on `feature/preset-persistence`; final review pass done → fix commit `331d72e`, no re-review needed per skill). Ruled behaviors: duplicate-name save replaces, corrupt `presets.json` fails fast with io::Error; RGBA (not BGRA) PNG contract pinned by test |
| `docs/superpowers/plans/2026-11-05-vector-field-explorer.md` | The full app, 14 tasks T1–T14 | **T1–T6 complete** (T1 `caa7c09` shell; T2 `62b1c5c` field; T3 `fee0b18`+`e13b7b7` domain/field-presets; T4 `8fd8701` colormaps; T5 `a6790f8` sampling → `src/sampling.rs`; T6 `7e37254` OrbitCamera → `src/camera.rs`); **T7 complete** (`81d1b30` instanced wgpu renderer + fix round 1 `2020a31` arrow-shader world-space normal `b * normal`, scoped re-review clean); **T8A complete** (`522d011` viewmodel seam `src/viewmodel.rs` — pure State/dirty-flag/snapshot builder, 12 unit tests); **T8B + T9–T14 tracked as TICKETS 02–09** under `.scratch/field-viewport/issues/` (02 **COMPLETE** (c5cfd95..`c62507a`, review clean after fix round 1: zoom gated 3D-only; 4 minors parked for final review) → 03 **COMPLETE** (20701e0..`41fe108`, review clean after fix round 1: VfApp doc comment restored; 3 minors parked for final review) → 04 **COMPLETE** (00c4ee5..`13fa5dc`, review clean after fix round 1: both_directions composition, pinned `vortex_stays_bounded` verbatim, status-bar split counts; 2 minors parked for final review) → 05 **COMPLETE** (18307e6..`a42fa97`, review clean after fix round 1: `clear()` rng→`SPAWN_SEED` respawn contract + pinned covering test, cost note corrected; 4 minors parked for final review) → 11 **COMPLETE** (`fb4eca7`, review clean; shader parse fixed, dead `src/shaders.wgsl` deleted) → 12 **COMPLETE** (`d1b23c6`, review clean; smoke PASS 15s) → 06 **COMPLETE** (`ff5e708`, review clean after salvage; 3 minors parked) → 07 **COMPLETE** (`3db30e1`, review clean; 3 minors parked) → 08 **COMPLETE** (`9d0cb6d` + fix `43bd549`, review clean after fix round 1: physical-pixel capture; 1 cosmetic minor parked) → 14 **IMPLEMENTED** (`f4dbb1b`, task review pending; rails/navigation/help/persistence; 179 tests, smoke PASS) → frontier 15 top-bar menus; tickets 15 (menus), 16 (GitHub Pages site), 17 (Linux/macOS release pipeline) **filed**; 16 ← 17; 13 test/source separation ← 14,15; 09 final review ← 13,14,15,16,17; 10 codebase docs ← 09. ⚠ working tree (2026-11-08, post-ticket-14): `main` at docs `c27b793`; feature branch `feature/ux-window-mouse-sidebars` carries `f4dbb1b` awaiting review; only untracked `.scratch/vfe-tmp/`. Issue 01's "Related minors" + tickets 02–05 parked minors (4+3+2+4) all routed to final whole-branch review

**Session handoff 2026-11-08 (latest):** active SDD ledger = `.superpowers/sdd/2026-11-05-vector-field-explorer/progress.md` — read `git log` + the ledger's LAST dated section (later sections supersede the older `## HANDOFF` block). State: tickets 02–08, 11, 12 COMPLETE (`513b31b..43bd549`, reviews clean, smoke PASS); ticket 14 IMPLEMENTED (`f4dbb1b`, 179 tests, smoke PASS) and awaiting task review; docs/design commit `c27b793` on `main` (GLOSSARY, ADR 0001, rewritten ticket 14). Next order: **15 top-bar menus → 13 test/source separation → 17 release pipeline → 16 Pages site → 09 final whole-branch review → 10 codebase docs**; first real `v*` tag only after 09 merges and `main` is green. Ticket 13 inventory (post-14): 152 inline tests across 18 `src/` files vs 27 in `tests/`, baseline 179. `feature/preset-persistence` is already fully merged (no-op for 09; delete stale branch there). After every reviewed merge: `git push origin main` (private repo), verify GitHub CI/smoke workflows. Lost-report protocol + gate-probe-before-redispatch rule + clippy `--` separator lesson in ledger tail.

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
`src/render.rs` (2D pan/zoom `Camera`), `src/camera.rs` (3D `OrbitCamera` + gesture seam),
`src/screenshot.rs` (offscreen capture + save dialog), `src/window.rs` (window
options), `src/persistence.rs` (single APP_KEY envelope + migration),
`src/ui/presets.rs` (PresetPanel), `src/ui/style_panel.rs` (AppStyle),
`src/ui/top_bar.rs` (menu row), `src/ui/layout.rs` (panel rails/layout state),
`src/ui/sidebar.rs` (field controls), `src/ui/help_overlay.rs`, `src/ui/status_bar.rs`.

## Conventions (mandatory)

- **TDD**: no production code without a failing test first (superpowers:test-driven-development).
  Watch RED fail for the right reason; keep the whole `cargo test` suite green per task.
- Commits: `feat: <description>` / `fix: …` / `docs: …`, one commit set per task.
- Record progress in the plan's SDD ledger (`task-start` / `task-done` scripts
  under `~/.cache/opencode/npm/git-superpowers-*/…/skills/{executing-plans,subagent-driven-development}/scripts/`),
  never in memory or todos alone.
- Isolated work (trunk-based): one feature branch (`feature/<topic>`) or git worktree per
  feature; develop the whole feature on that branch, then merge it into `main` once the
  feature is reviewed (whole-branch review clean). `main` is the trunk: only reviewed,
  merged work lands there.
- Resolution ledgering (`Ruling: <decision> — <why>` in the ledger) is mandatory.
- CI (`.github/workflows/ci.yml`, Rust-based) gates on `cargo clippy --all-targets -- -D warnings`,
  `cargo test --all-targets`, `cargo build --all-targets` — keep code clippy-clean.
- **Runtime smoke test**: `cargo build`/`cargo test` never create a wgpu device, so
  broken WGSL and device-init errors surface only when the app launches. After
  touching `src/renderer.rs` or any `src/*.wgsl`, and before claiming any rendering
  task done, run `scripts/smoke-run.sh 15` — it launches the app under `timeout` and
  fails on a panic or wgpu validation error. CI runs the same script headlessly
  (Xvfb + software Vulkan) in the `smoke` job.
- **GitHub workflow**: private remote `git@github.com:ttgg1/vector-field-explorer.git`.
  After a reviewed feature branch is merged into `main`, `git push origin main` and
  confirm the GitHub CI/smoke workflows are green. Never force-push `main`; feature
  branches may be pushed for backup, but merging is the push gate.
- **License**: GPL-3.0-only. `LICENSE` at repo root; new files and dependencies must
  stay GPL-3.0-compatible.
- **Test layout**: `tests/` holds tests, `src/` holds production code. The existing
  inline `#[cfg(test)]` modules are debt tracked by ticket 13 — do not add new ones.
- **Background seats**: dispatch implementer/reviewer subagents with
  `background=true`; the controller continues ledger/docs work while waiting. Never
  run two implementer seats concurrently.

## Egui / Rust API notes (learned the hard way)

- **egui 0.37**: `App` trait method is `fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame)`
  — there is **no** `update(ctx, frame)`. Panels are the unified
  `egui::Panel::top/bottom/left/right(id)` with `.resizable()`, shown against the
  `Ui`; `egui::CentralPanel::default_margins().show(ui, …)`.
- tests in `tests/` are integration tests: import `vector_field_explorer::{…}` —
  bare `crate::` / `use super::*` will not resolve.
- Rust edition 2024: `std::env::set_var` / `remove_var` are `unsafe` — wrap them.
- wgpu usage must go through eframe's re-exports; `rfd = "0.15"` is a deliberate
  dependency for the screenshot save dialog.

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

**Model policy (user-selected 2026-11-08, supersedes all earlier policies):**
coding seats use OpenCode Go models; review seats use the cheap OpenCode Go
reviewer (`opencode-go/claude-haiku-5-5`, free `beam-free` fallback).
Always pass the model explicitly — subagents never silently inherit the session
model. No NVIDIA/Nemotron seats, no LM Studio seats, no `strata` seats.

- **Coding seats** (implementer, fix rounds, research) — `opencode-go/deepseek-v4.1-flash`
  by default. Raise the *variant* (`#high` → `#max`) when a seat needs more judgment,
  and bump the variant for a stuck implementer (BLOCKED/NEEDS_CONTEXT twice). For heavier
  work, escalate the model to `opencode-go/kimi-k3.1-pro` (`#high`/`#max`).
- **Review seats** (task reviewer, scoped re-review, final whole-branch review) —
  `opencode-go/claude-haiku-5-5#high` (cheap; user-selected 2026-11-08 to avoid
  OpenCode Go usage limits). Use `#max` for the final whole-branch review. Free
  fallback if unavailable: `opencode-go/beam-free`.

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
3. Dispatch implementer (`opencode-go/deepseek-v4.1-flash`, variant per policy) in
   the background (`background=true`) with:
   how the task fits, brief path,
   interfaces/decisions earlier tasks established, ambiguity resolutions,
   report-file path + report contract.
4. On DONE: `bash scripts/review-package PLAN_FILE BASE HEAD` → diff file.
5. Dispatch task reviewer (`opencode-go/claude-haiku-5-5#high`, `#max` for the final whole-branch review) in the background with brief, report, diff paths +
   binding global constraints; resolve findings via the fix loop (resume the
   same implementer session with `sessionID`); ledger completion line.
6. Merge the reviewed branch into `main`, then `git push origin main` and check
   GitHub CI/smoke are green.

## AI Workflow Integration

New feature modules follow the plans; write tests automatically alongside;
run `cargo test` (and later `cargo clippy -- -D warnings`) before claiming done.
All generated code lives under `src/` or `tests/`.

## Skills & Workflow map (future agents — start here)

Two skill families are installed:
- **superpowers** — `~/.cache/opencode/npm/git-superpowers-*/…/skills/`
  (brainstorming, writing-plans, executing-plans, subagent-driven-development,
  test-driven-development, systematic-debugging, using-git-worktrees,
  finishing-a-development-branch, requesting-code-review, receiving-code-review,
  verification-before-completion, writing-skills, dispatching-parallel-agents, …)
- **Matt Pocock skills** — `~/.agents/skills/` (code-review, codebase-design,
  domain-modeling, to-tickets, implement-spec, implement, tdd, diagnosing-bugs,
  grilling / grill-me / grill-with-docs, pr, research, prototype, wizard,
  writing-for-agents, find-skills, setup-pre-commit, …)

Skill rule: if a skill plausibly applies (even 1%), invoke it BEFORE acting.
Process skills first (they set the approach), implementation skills carry it out.

Workflow this repo uses, in order:
1. **New feature/design** → `brainstorming` (superpowers) before any creative work;
   stress-test the plan with `grilling` (mattpocock) if the user asks to "grill".
2. **Spec/plan** → `writing-plans` (superpowers) → plan under `docs/superpowers/plans/`;
   spec under `docs/superpowers/specs/`. Break a plan into tracker tickets with
   `to-tickets` (mattpocock) — tickets land in `.scratch/<feature-slug>/issues/`.
3. **Execution** → SDD loop: `subagent-driven-development` (superpowers) — controller
   dispatches one implementer subagent at a time, then a task reviewer; or
   `executing-plans` inline if the user chose that. Never both in parallel.
4. **Per task** → `test-driven-development` (TDD, RED first) gates every change;
   bugs/regressions → `systematic-debugging` (superpowers) or `diagnosing-bugs`
   (mattpocock) BEFORE proposing fixes.
5. **Reviews** → per-task review via SDD `task-reviewer-prompt.md`; branch/PR
   standards+spec review → `code-review` (mattpocock); request review at
   completion → `requesting-code-review`; consume feedback → `receiving-code-review`
   (verify, don't blind-implement). PR body → `pr` (mattpocock).
6. **Finish** → `verification-before-completion` before any "done" claim;
   integrate via `finishing-a-development-branch` (trunk-based: reviewed feature branch
   merged → `main`); isolated workspace via `using-git-worktrees` (one branch per feature).
7. **Docs/meta** → `writing-for-agents` when editing AGENTS.md/skills;
   `domain-modeling` for GLOSSARY.md + ADRs; `research` for primary-source facts;
   `find-skills` to discover more skills.

**Context management (mandatory):** this harness runs the `@tarquinen/opencode-dcp`
plugin. Keep a fresh context window by calling the `compress` tool itself (DCP
compression) as soon as a phase closes — do NOT rely on, wait for, or configure
around OpenCode's built-in auto-compaction. Summaries are authoritative records:
file paths, rulings, commit hashes.

## Agent skills

### Issue tracker
Issues live as local markdown under `.scratch/<feature-slug>/` — no git
remote, no gh/glab. See `docs/agents/issue-tracker.md`.

### Domain docs
Single-context: `GLOSSARY.md` at repo root + `docs/adr/`, created lazily by
`/domain-modeling`. See `docs/agents/domain.md`.
