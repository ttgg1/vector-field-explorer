# Vector Field Explorer — Agent Guide

Rust eframe/egui app for exploring vector fields (`f(x,y)` or `f(x,y,z)`) typed
as formulas: arrows, streamlines, particles, colormaps, click-to-seed, camera
presets, screenshots. Ships native and as a WebAssembly browser build.

- Architecture and module map: `docs/CODEBASE.md`
- Decisions: `docs/adr/`; domain vocabulary: `GLOSSARY.md`
- License: **GPL-3.0-only** (`LICENSE`); dependencies must stay compatible
- Remote: `git@github.com:ttgg1/vector-field-explorer.git`

## Where the truth lives

- **Progress and rulings:** `.superpowers/sdd/2026-11-05-vector-field-explorer/progress.md`
  (last dated section) plus `git log`. Never trust an unverified status line over
  those two.
- **Tickets:** `.scratch/field-viewport/issues/NN-slug.md`, one ticket per file,
  with `Status:` and dependencies. `.scratch/` is local and untracked; do not
  delete tickets.
- **Current frontier and next order:** the ledger, not this file. This guide
  deliberately does not duplicate ticket status.
- **Tribal knowledge:** `docs/wiki/` (tracked, `docs/wiki/index.md` lists every
  page). Decisions with their reasoning, the working process, and the state of
  long-running efforts. Pages are dated historical records that cite their
  source — read the wiki before re-deriving process from scratch, and add a
  page when you learn something durable.

## v0.1.0 target

- **Native:** Linux x86_64 single-file AppImage; macOS x86_64/arm64 unsigned
  `.app` zips. No signing, notarization, auto-update, or telemetry.
- **Web:** real wasm build at `https://ttgg1.github.io/vector-field-explorer/`.
  Visualization parity: formula, field presets, 2D/3D, arrows/streamlines/
  particles, click-to-seed, colormaps, camera; browser persistence and
  screenshot download.
- **Browsers:** Firefox first on WebGPU terms (Firefox 141+ Windows, 145+ macOS
  Tahoe ARM, Nightly Linux; Chromium/Edge where WebGPU is available).
  Unsupported browsers get enable instructions. No WebGL2 fallback.
- **One codebase:** `cfg(target_arch = "wasm32")` platform seam; eframe
  `WebRunner` + `trunk`; native-only dependencies (`rfd`, filesystem paths)
  gated out.
- **Onboarding:** README quickstart, three "Start here" presets, in-app help.
  Manual five-minute check on native and web: launch → load preset → edit
  formula → toggle 3D → click to seed.
- **Performance floors:** native 60 fps default / 30 fps on the densest preset
  at 1280×800 integrated GPU; web 30 fps default; startup <2 s native, <5 s web.
- **Release:** SemVer; first tag `v0.1.0` only after ticket 19 is green;
  on-demand patch/minor releases; no fixed cadence, no backports. Windows
  lands as `v0.2.0` — a new supported platform is a feature, not a patch, and
  is not backported to v0.1.0. Local ticket tracker through v0.1.0; GitHub
  Issues/PRs accepted; no CLA.
- **No localization in v0.1.0.** There is no locale module; keep user-facing
  strings in the `ui/` modules.
- **Post-v1 order:** Windows packaging (ticket 72) and time-dependent fields
  (ticket 39, Part A) run as two parallel fronts → 55 Phase 1 (sources) →
  55 Phase 3 (induction) → 55 Phase 5 (ODE) → 55 Phase 2 (matter) →
  55 Phase 4 (Maxwell) → div/curl overlays → shareable presets → CSV import.
  Ticket 38 (plot animation, Part B) follows tickets 37 and 39.

## Repo map

Production code is under `src/`, tests under `tests/`.

- `field.rs` parse/eval via meval · `domain.rs` grid · `colormap.rs`
  Viridis/Turbo/Mono · `presets.rs` field presets (distinct from camera
  `export::CameraPreset`) · `sampling.rs` `sample_field`/`ArrowData`/`ViewOptions`
- `viewmodel.rs` pure `State`, dirty flags, `SceneGPU` snapshot, camera-input
  seam · `view_command.rs` pure camera commands · `camera.rs` `OrbitCamera` ·
  `renderer.rs` instanced wgpu · `render.rs` 2D pan/zoom · `particles.rs`
  particle state/advection
- `export.rs` PNG + camera presets · `screenshot.rs` offscreen capture/save
  dialog · `persistence.rs` single `APP_KEY` envelope + migration · `window.rs`
  window options
- `ui/` sidebar, presets, formula bar, style panel, menu, top bar, layout rails,
  help overlay, status bar

## Conventions (mandatory)

- **TDD:** no production code without a failing test first; watch RED fail for
  the right reason, then keep the full suite green.
- **Tests live in `tests/`.** Every test lives in `tests/<module>_test.rs`;
  `tests/suite.rs` includes each one as a module (`autotests = false` plus one
  main `[[test]]`), so `cargo test --all-targets` runs the suite in one process.
  A test that mutates process-global environment must be its own `[[test]]`
  target so it runs alone in its process — `tests/config_test.rs` (which sets
  `XDG_CONFIG_HOME`) is the example, and the layout guard accepts either
  registration. Filter by module or test name (`cargo test particles_test::`);
  run an isolated target with `cargo test --test config_test`. Only
  `config_test` has its own `--test` target; the other files are suite modules.
  Shared helpers live in `tests/support/` and are declared once in
  `tests/suite.rs`. `src/` carries no `#[cfg(test)]` modules, and
  `tests/test_layout_test.rs` fails the suite if one comes back, if a test file
  is unregistered, or if a test is `#[ignore]`d. Integration tests import
  `vector_field_explorer::{…}` — `crate::` and `use super::*` do not resolve,
  and they see only `pub` items. A helper that a test genuinely needs is
  promoted to a single `#[doc(hidden)] pub` seam with the reason in the commit
  body and the SDD ledger; never widen the API further for tests.
- **Test profile:** `Cargo.toml` sets `[profile.test] opt-level = 2` on top of
  the dev profile's `debug = 1` / `strip = "debuginfo"` (`.cargo/config.toml`).
  The math-heavy particle, streamline, and sampling tests dominate run time at
  opt-level 0; ticket 21 cut the warm suite from ~3.98 s to ~0.72 s median.
  Accepted margin: the cold `cargo test --all-targets --no-run` build is
  65.0 s versus 26.5 s at opt-level 0 (2.45×, +38.5 s), paid once per
  dependency/profile change; the incremental edit loop improved from 5.6 s to
  ~1.3 s. Keep the gates (and `scripts/test.sh`) on `cargo test --all-targets`;
  there is no separate runner requirement.
- **Commits:** `feat:` / `fix:` / `docs:`, one commit set per task. Never ledger
  a task without a `git rev-parse`-verifiable hash.
- **Ledger first:** progress and every `Ruling: <decision> — <why>` go in the SDD
  ledger, not only in todos or chat.
- **Ticket creation from feedback:** before filing tickets from a user issue
  list, testing notes, or a review, run `grilling` (with `domain-modeling` when
  terminology or a hard-to-reverse decision is involved). Group findings
  thematically, deduplicate against open tickets, and extend the owning ticket
  when a finding refines existing scope; create a new ticket only for new
  capability. Ask the whole current frontier in rounds, record every
  `Ruling:` in the SDD ledger, and do not create one ticket per bullet. The
  resulting ticket set is confirmed with the user before implementation.
- **Trunk-based:** one `feature/<topic>` branch or worktree per feature; merge
  only after review; `main` is reviewed work only.
- **Gates:** `cargo build`, `scripts/test.sh` (runs `cargo test --all-targets`
  and prunes stale test binaries; `KEEP_TEST_ARTIFACTS=1` skips cleanup), `cargo
  clippy --all-targets -- -D warnings` (note the `--` separator).
- **Runtime smoke:** build/test create no wgpu device. After touching
  `src/renderer.rs`, shaders, windowing, or input, run `scripts/smoke-run.sh 15`;
  it fails on panics and wgpu validation errors. CI runs it headlessly.
- **Push after merge:** `git push origin main`, then confirm GitHub CI and smoke
  are green. Never force-push `main`.
- **Parallel seats:** default to several implementers when tickets are
  independent. Give each its own worktree and `target/`, and run only
  file-disjoint tickets; never point two implementers at the same file.
  Reviewers run per diff in parallel; the controller serializes merges. Cargo is
  capped at 10 jobs (`.cargo/config.toml`); do not raise it without a ruling.
- **Scope guard:** keep GPU paint behind `SceneGPU`; pure logic in testable
  seams; no test-only API widening without a ledger ruling.
- **Customization lives with its feature.** Every user-facing feature gets an
  organized home in its owning panel: primary controls in the section body,
  advanced parameters in nested `CollapsingHeader` subsections, each with a
  tooltip and a section reset. Never add a flat settings dump or a global
  "Advanced" page. When a panel outgrows a comfortable size, split it by
  concept into another panel or column (the layout already has rails and
  persists widths); record the split and its reason in the ledger. Prefer
  multiple panels or columns for different parameter families when that keeps
  clutter low; future physics panels (Sources, Materials, Maxwell, Plot)
  follow this rule.

## Egui / Rust notes

- egui 0.37: `App::ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame)`
  — there is no `update(ctx, frame)`. Panels are
  `egui::Panel::top/bottom/left/right(id)` shown against the `Ui`;
  `egui::CentralPanel::default_margins().show(ui, …)`.
- Game-style movement keys use `Event::Key.physical_key` (QWERTY positions),
  falling back to the logical `key` where the backend does not populate it
  (eframe web today). `InputState::keys_down` is logical only; accumulate held
  movement keys from events and clear on focus loss.
- Rust edition 2024: `std::env::set_var` / `remove_var` are `unsafe`.
- wgpu goes through eframe's re-exports. `rfd = "0.15"` is deliberate
  (screenshot dialog) and native-only.
- Persistence uses one RON envelope under `eframe::APP_KEY`; camera state resets
  on launch by design. Corrupt UI settings fall back/clamp; camera presets stay
  fail-fast.
- **Logging:** use the `log` facade macros at the call site. Native initializes
  `env_logger` in `main` (stderr, `RUST_LOG` filter, default `info`); wasm uses
  `console_log` once ticket 16 lands. Per-frame output is `trace` only.

## SDD workflow

The session is controller: one implementer subagent per task, several at once
when tickets are independent and file-disjoint, then one task reviewer per diff.
The controller keeps the ledger, does not edit code during the loop, serializes
merges, and runs a fix round before moving on (read `references/fix-loop.md`). A
whole-branch review runs once at the end.

Scripts and prompt templates:
`~/.cache/opencode/npm/git-superpowers-*/<version>/node_modules/superpowers/skills/subagent-driven-development/`
(`implementer-prompt.md`, `task-reviewer-prompt.md`, `re-review-prompt.md`,
`scripts/task-brief`, `scripts/review-package`, `scripts/sdd-repo`,
`scripts/sdd-workspace`). Note the `<version>` segment — the package is
nested one level deeper than a plain glob suggests. The scripts take the
plan file as their first argument and resolve the SDD workspace from it;
this effort's plan is
`docs/superpowers/plans/2026-11-05-vector-field-explorer.md`. Subagents read
the task brief, never the whole plan.

**Models — always pass explicitly; never inherit silently.**

- **Chief of staff (the controller):** `openai/gpt-6.1-sol#medium`.
- **Implementer seats:** `openai/gpt-6-luna#low`.
- **Review seats:** `openai/gpt-6.1-sol#medium`.
- **Escalate the effort when a seat needs it.** At the second fix round step
  the variant up one level (low → medium → high), and step up again at every
  further round. A round that returns the same finding is a signal to
  escalate, not to repeat the same seat at the same effort.
- **Fallback seats:** the older free models (`opencode-go/beam-free`,
  `opencode-go/longcat-2.5-free`) when an `openai` seat is unavailable. Every
  `opencode-go` paid model fails with "Policy budget exceeded" (verified
  2026-11-10). Do not re-try a seat that failed that way — re-scope the task
  or split it instead of escalating to another blocked model. No kimi k3.x
  seats, no NVIDIA/Nemotron, LM Studio, or `strata` seats.

**Implementer contract:** work from the brief; TDD where required; run the full
suite before committing; `feat:/fix:/docs:` commits; self-review; never dispatch
sub-subagents; report DONE | DONE_WITH_CONCERNS | BLOCKED | NEEDS_CONTEXT with
commits, gate results, and a report at `…/task-N-report.md`.

**Dispatch flow:** record BASE → `task-brief` → background implementer →
`review-package` → background reviewer → fix loop → ledger completion → merge,
push, verify CI.

## Skills and context

- Two skill families: superpowers
  (`~/.cache/opencode/npm/git-superpowers-*/<version>/node_modules/superpowers/skills/`)
  and Matt Pocock
  (`~/.agents/skills/`). Invoke a skill whenever it plausibly applies; process
  skills before implementation.
- Feature path: `brainstorming` → `grilling` (mandatory before ticket
  creation) → `writing-plans` → `to-tickets` → SDD + TDD →
  `code-review`/`requesting-code-review` → `verification-before-completion` →
  `finishing-a-development-branch`.
- Bugs: `systematic-debugging`/`diagnosing-bugs` before proposing a fix.
- Domain: update `GLOSSARY.md` and `docs/adr/` through `domain-modeling`.
- **Context:** this harness runs `@tarquinen/opencode-dcp`. Call `compress` as
  soon as a phase closes; do not rely on built-in auto-compaction. Summaries are
  authoritative records: paths, rulings, commit hashes.
- Editing this file or any skill: use `writing-for-agents` and prune deprecated
  status detail rather than layering more onto it.
