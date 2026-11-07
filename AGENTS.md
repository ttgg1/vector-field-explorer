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
| `docs/superpowers/plans/2026-11-05-vector-field-explorer.md` | The full app, 14 tasks T1–T14 | **T1–T6 complete** (T1 `caa7c09` shell; T2 `62b1c5c` field; T3 `fee0b18`+`e13b7b7` domain/field-presets; T4 `8fd8701` colormaps; T5 `a6790f8` sampling → `src/sampling.rs`; T6 `7e37254` OrbitCamera → `src/camera.rs`); **T7 complete** (`81d1b30` instanced wgpu renderer + fix round 1 `2020a31` arrow-shader world-space normal `b * normal`, scoped re-review clean); **T8A complete** (`522d011` viewmodel seam `src/viewmodel.rs` — pure State/dirty-flag/snapshot builder, 12 unit tests); **T8B + T9–T14 tracked as TICKETS 02–09** under `.scratch/field-viewport/issues/` (02 **COMPLETE** (c5cfd95..`c62507a`, review clean after fix round 1: zoom gated 3D-only; 4 minors parked for final review) → 03 **COMPLETE** (20701e0..`41fe108`, review clean after fix round 1: VfApp doc comment restored; 3 minors parked for final review) → frontier 04 ∥ 05 ∥ 07 (04 streamlines ∥ 05 particles ∥ 07 style-persistence all unblocked by 03; sequential-implementers rule kept); 06 3D+click-seed ← 04,05; 07 style persistence ← 03; 08 screenshot ← 06; 09 README+cleanup+final whole-branch review ← 02–08; 10 full-codebase documentation ← 09 (user-added 2026-11-07)). ⚠ working tree (2026-11-07, post-ticket-03): CLEAN at HEAD `41fe108` + docs bookkeeping — ticket-03 WIP salvaged from dead seat #1, gate-fixed, committed+reviewed through fix round 1. Issue 01's "Related minors" + tickets 02/03 parked minors (4+3) all routed to final whole-branch review

**Session handoff 2026-11-07 (refreshed):** active SDD ledger = `.superpowers/sdd/2026-11-05-vector-field-explorer/progress.md` — read `git log` + the ledger's LAST dated section (later sections supersede the older `## HANDOFF` block). As of the refresh: ticket 03 COMPLETE (`41fe108`+docs), next ticket = 04 streamlines-2d-rk4 (plan task 9; brief `.scratch/field-viewport/issues/04-streamlines-2d-rk4.md`), order 04→05→06→07→08→09(final review)→10. Lost-report protocol + gate-probe-before-redispatch rule in ledger tail.

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
- Isolated work (trunk-based): one feature branch (`feature/<topic>`) or git worktree per
  feature; develop the whole feature on that branch, then merge it into `main` once the
  feature is reviewed (whole-branch review clean). `main` is the trunk: only reviewed,
  merged work lands there.
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

**Model policy (user-selected 2026-11-06, supersedes all earlier policies): ONLY
`qwen3.8-flash-next` from provider `strata`, for ALL subagent seats.**
Every seat — implementer, task reviewer, scoped re-review, research, and the
final whole-branch review — uses `strata/coder-iq1_m` ("Qwen3.8-Flash-Next
(local)"; always pass the model explicitly — subagents never silently inherit
the session model). No LM Studio seats, no cloud fallbacks (qwen3.8-27b,
devstral2, nemotron are retired). If a seat needs more judgment, raise the
*variant* of the same model: `strata/coder-iq1_m#medium` / `#high`
(available variants: low, medium, high). Escalation for stuck implementers
(BLOCKED/NEEDS_CONTEXT twice) bumps the variant, not the model.

**Erratum 2026-11-07 (user):** implementer seats stay `strata/coder-iq1_m`
(local); REVIEW seats may use a FREE OpenCode model (e.g.
`opencode/flint-40b-free`, `opencode/glm-5.3-flash-free`) instead of the
strata seat — scale the free model to the diff's size/complexity. Still
always pass the model explicitly.

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
3. Dispatch implementer (`strata/coder-iq1_m`) with:
   how the task fits, brief path,
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
