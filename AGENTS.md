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
| `docs/superpowers/plans/2026-11-05-vector-field-explorer.md` | The full app, 14 tasks T1–T14 | **T1–T5 complete** (T1 `caa7c09` shell; T2 `62b1c5c` field; T3 `fee0b18`+`e13b7b7` domain/field-presets; T4 `8fd8701` colormaps; T5 `a6790f8` sampling → `src/sampling.rs`). **T6 in progress** (`src/camera.rs` OrbitCamera RED stage written; tests+impl to follow). **T7–T14 remain** (GPU renderer+shaders.wgsl, UI wiring incl. rebuilding the `Controls` struct — check `task-1-brief.md` for the original shape, streamlines, particles, 3D seeding, style panel, screenshot, README/cleanup) |

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

## Subagent Usage

Dispatch parallelizable, self-contained tasks to `general` subagents with fresh
context (review gates, big module implementations, final whole-branch reviews).
Sequential/shared-state work runs inline in the session with the ledger as memory.

```bash
# OpenCode harness equivalent
opencode --agent general   # give each dispatch: plan path, task brief, test command, conventions above
```

1. **Create a branch** for the task: `git checkout -b feature/<task>`.
2. Dispatch the subagent with the task brief path + plan path + review focus.
3. Review the branch once the subagent finishes (tests + plan alignment).
4. Merge into the target branch after the suite passes and the ledger is updated.

## AI Workflow Integration

New feature modules follow the plans; write tests automatically alongside;
run `cargo test` (and later `cargo clippy -- -D warnings`) before claiming done.
All generated code lives under `src/` or `tests/`.
