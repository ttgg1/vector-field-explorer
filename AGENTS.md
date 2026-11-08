# Vector Field Explorer — AI Workflow

Rust eframe/egui app for exploring vector fields (`f(x,y)` or `f(x,y,z)`) typed
as formulas: arrows, streamlines, particles, colormaps, click-to-seed, camera
presets, screenshots. Ships as a native app and a WebAssembly browser build.

- Vision: `docs/superpowers/specs/2026-11-05-vector-field-explorer-design.md`
- Plans: `docs/superpowers/plans/2026-11-05-vector-field-explorer.md`,
  `docs/superpowers/plans/2026-11-05-preset-persistence.md`
- License: **GPL-3.0-only** (`LICENSE`; dependencies must stay compatible)
- Remote: `git@github.com:ttgg1/vector-field-explorer.git` (private until ticket
  16 starts; then public, because Pages and GPL source distribution require it)

**Authoritative progress:** `.superpowers/sdd/2026-11-05-vector-field-explorer/progress.md`
(last dated section) plus `git log`. Tickets live in `.scratch/field-viewport/issues/`.
Never trust an unverified status line over those two.

## v0.1.0 goals (user-selected 2026-11-08)

- **Native:** Linux x86_64 tarball; macOS x86_64/arm64 unsigned `.app` zips.
  No signing, notarization, auto-update, or telemetry.
- **Web:** real wasm build at `https://ttgg1.github.io/vector-field-explorer/`.
  Visualization parity: formula, field presets, 2D/3D, arrows/streamlines/
  particles, click-to-seed, colormaps, camera; persistence via browser storage;
  screenshot via browser download.
- **Firefox first, on WebGPU terms:** Firefox 141+ Windows, Firefox 145+ macOS
  Tahoe ARM, Firefox Nightly on Linux; Chromium/Edge where WebGPU is available.
  Unsupported browsers get a support page with enable instructions. No WebGL2
  fallback in v0.1.0.
- **One codebase:** `cfg(target_arch = "wasm32")` platform seam; eframe's
  `WebRunner` + `trunk`; native-only dependencies (`rfd`, filesystem paths)
  gated out.
- **Onboarding:** README quickstart, three "Start here" presets, in-app help.
  Manual five-minute check on native and web: launch → load preset → edit
  formula → toggle 3D → click to seed.
- **Performance floors:** native 60 fps default scene / 30 fps on the densest
  preset at 1280×800 integrated GPU; web 30 fps default; startup <2 s native,
  <5 s web.
- **Release:** SemVer; first tag `v0.1.0` only after ticket 19 is green;
  on-demand patch/minor releases from commits; no fixed cadence, no backports.
  Local ticket tracker through v0.1.0; GitHub Issues/PRs accepted; no CLA.
- **No localization in v0.1.0.** Delete the placeholder `src/i18n.rs` and
  `data/locales/` in ticket 09.
- **Post-v1 order:** Windows → time-dependent fields → div/curl overlays →
  shareable presets → CSV import.

## Status and frontier

Completed: plan tasks T1–T8A; tickets 02–08, 11, 12, 14, 15, 17, 20, 24, 25.
See the ledger's latest dated section before resuming.

Next order: **18 right-drag orbit + expanded controls → 22 sidebar rail
legibility → 23 3D cube + plane view → 13 test/source separation → 21 test-suite
performance → 16 wasm + Pages → 09 README, cleanup, final whole-branch review →
10 codebase documentation → 19 final whole-codebase review + organization
refactor.** Ticket 18 builds on 15's Help/menu surface; ticket 16 inherits 18's
bindings. Tickets 22 and 23 land after 18 (same panels/help) and before 13 so
their tests move once; ticket 21 follows 13 because relocating inline tests
would otherwise multiply integration-binary startup cost.

Ticket 09 triages all parked minors (issue-01, tickets 02–05, 06–08) and deletes
stale feature branches. Ticket 19 is the last development ticket: a complete
repo-wide review by `claude-haiku-5-5#max` plus a behavior-preserving refactor;
it is the final gate before the first tag.

## Repo map

Production code is under `src/`, tests under `tests/`.

- `field.rs` parse/eval via meval · `domain.rs` grid · `colormap.rs`
  Viridis/Turbo/Mono · `presets.rs` field presets (distinct from camera
  `export::Preset`) · `sampling.rs` `sample_field`/`ArrowData`/`ViewOptions`
- `viewmodel.rs` pure `State`, dirty flags, `SceneGPU` snapshot, camera-input
  seam · `camera.rs` `OrbitCamera` · `renderer.rs` instanced wgpu · `render.rs`
  2D pan/zoom camera
- `export.rs` PNG + camera presets · `screenshot.rs` offscreen capture/save
  dialog · `persistence.rs` single `APP_KEY` envelope + migration · `window.rs`
  window options
- `ui/` sidebar, presets, style panel, top bar, layout rails, help overlay,
  status bar
- `i18n.rs` placeholder, scheduled for deletion in ticket 09

## Conventions (mandatory)

- **TDD:** no production code without a failing test first; watch RED fail for
  the right reason, then keep the full suite green.
- **Tests live in `tests/`.** Existing inline `#[cfg(test)]` modules are ticket
  13 debt; add no new ones. Integration tests import
  `vector_field_explorer::{…}` — `crate::` and `use super::*` do not resolve.
- **Commits:** `feat:` / `fix:` / `docs:`, one commit set per task. Never ledger
  a task without a `git rev-parse`-verifiable hash.
- **Ledger first:** progress and every `Ruling: <decision> — <why>` go in the SDD
  ledger, not only in todos or chat.
- **Trunk-based:** one `feature/<topic>` branch or worktree per feature; merge
  only after review; `main` is reviewed work only.
- **Gates:** `cargo build`, `cargo test --all-targets`, `cargo
  clippy --all-targets -- -D warnings` (note the `--` separator).
- **Runtime smoke:** build/test create no wgpu device. After touching
  `src/renderer.rs`, shaders, windowing, or input, run `scripts/smoke-run.sh 15`;
  it fails on panics and wgpu validation errors. CI runs it headlessly.
- **Push after merge:** `git push origin main`, then confirm GitHub CI and smoke
  are green. Never force-push `main`.
- **Parallel seats:** default to several implementers at once when tickets are
  independent. Give each one its own worktree and `target/` directory, and only
  run tickets that touch disjoint files; never point two implementers at the
  same file. Reviewers run per diff in parallel; the controller serializes
  merges. Cargo is capped at 10 jobs (`.cargo/config.toml`) so concurrent builds
  leave CPU for every agent; do not raise it without a ruling.
- **Scope guard:** keep GPU paint behind `SceneGPU`; pure logic in testable
  seams; no test-only API widening without a ledger ruling.

## Egui / Rust notes

- egui 0.37: `App::ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame)`
  — there is no `update(ctx, frame)`. Panels are
  `egui::Panel::top/bottom/left/right(id)` shown against the `Ui`;
  `egui::CentralPanel::default_margins().show(ui, …)`.
- Rust edition 2024: `std::env::set_var` / `remove_var` are `unsafe`.
- wgpu goes through eframe's re-exports. `rfd = "0.15"` is deliberate (screenshot
  dialog), and is native-only.
- Persistence uses one RON envelope under `eframe::APP_KEY`; camera state resets
  on launch by design. Corrupt UI settings fall back/clamp; camera presets stay
  fail-fast.
- **Logging:** use the `log` facade macros at the call site. Native initializes
  `env_logger` in `main` (stderr, `RUST_LOG` filter, default `info`); wasm uses
  `console_log` to the browser console once ticket 16 lands. Per-frame output is
  `trace` only.

## SDD workflow

The session is controller: one implementer subagent per task, several at once
when tickets are independent and file-disjoint, then one task reviewer per diff.
The controller keeps the ledger, does not edit code during the loop, serializes
merges, and runs a fix round before moving on (read `references/fix-loop.md`). A
whole-branch review runs once at the end.

Scripts and prompt templates:
`~/.cache/opencode/npm/git-superpowers-*/…/skills/subagent-driven-development/`
(`implementer-prompt.md`, `task-reviewer-prompt.md`, `scripts/task-brief`,
`scripts/review-package`). Subagents read the task brief, never the whole plan.

**Models — always pass explicitly; never inherit silently.**

- Coding seats: `opencode-go/deepseek-v4.1-flash`, variant `#high` for complex
  work, `#max` when stuck (BLOCKED/NEEDS_CONTEXT twice). Heavier work may
  escalate to `opencode-go/kimi-k3.1-pro`.
- Review seats: `opencode-go/claude-haiku-5-5#high` (cheap, avoids usage
  limits); `#max` for the final whole-branch review; free fallback
  `opencode-go/beam-free`.
- No NVIDIA/Nemotron, LM Studio, or `strata` seats.

**Implementer contract:** work from the brief; TDD where required; run the full
suite before committing; `feat:/fix:/docs:` commits; self-review; never dispatch
sub-subagents; report DONE | DONE_WITH_CONCERNS | BLOCKED | NEEDS_CONTEXT with
commits, gate results, and a report at `…/task-N-report.md`.

**Dispatch flow:** record BASE → `task-brief` → background implementer →
`review-package` → background reviewer → fix loop → ledger completion → merge,
push, verify CI.

## Skills, docs, context

- Two skill families: superpowers (`~/.cache/opencode/npm/git-superpowers-*/…/skills/`)
  and Matt Pocock (`~/.agents/skills/`). Invoke a skill whenever it plausibly
  applies, process skills before implementation.
- Feature path: `brainstorming` → `grilling` when requested → `writing-plans` →
  `to-tickets` → SDD + TDD → `code-review`/`requesting-code-review` →
  `verification-before-completion` → `finishing-a-development-branch`.
- Bugs: `systematic-debugging`/`diagnosing-bugs` before proposing a fix.
- Domain: `GLOSSARY.md` at repo root and `docs/adr/`; update terms and ADRs
  through `domain-modeling`. Issue tracker conventions:
  `docs/agents/issue-tracker.md`; domain conventions: `docs/agents/domain.md`.
- **Context:** this harness runs `@tarquinen/opencode-dcp`. Call `compress` as
  soon as a phase closes; do not rely on built-in auto-compaction. Summaries are
  authoritative records: paths, rulings, commit hashes.
- Editing this file or any skill: use `writing-for-agents` and prune deprecated
  status detail rather than layering more onto it.
