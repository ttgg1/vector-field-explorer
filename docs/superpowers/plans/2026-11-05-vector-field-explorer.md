# Plan — Vector Field Explorer, post-v0.1.0 effort (2026-11-05 →)

**Status:** active. This is the plan file for the SDD workspace
`.superpowers/sdd/2026-11-05-vector-field-explorer/` (the `plan-path` marker
in that directory points here). The superpowers helper scripts resolve the
workspace from this file:

```
SDD=~/.cache/opencode/npm/git-superpowers-*/<version>/node_modules/superpowers/skills/subagent-driven-development/scripts
bash $SDD/task-brief <this-file> <task-id> [outfile]
bash $SDD/review-package [-C <worktree>] <this-file> <base> <head> [outfile]
```

## Where the effort stands (2026-11-10)

`v0.1.0` is released. The three tickets implemented just after it (69 SVG
export, 70 fly camera, 71 mobile UI) were pushed to `main` without review
and are being repaired now through the fix loop in `references/fix-loop.md`.
The repair rounds are tracked in the ledger
`.superpowers/sdd/2026-11-05-vector-field-explorer/progress.md`.

After the repairs merge, this plan carries the next wave: the SVG export UI
half (69b), Windows packaging (72), and time-dependent fields (39). Ticket 38
(plot animation) waits for ticket 37 (the scalar plot).

Rules every seat in this plan inherits:

- **TDD.** No production code without a failing test first; watch it fail for
  the right reason.
- **Tests live in `tests/`**, one file per module, registered in
  `tests/suite.rs`. No `#[cfg(test)]` modules in `src/`.
- **Gates before committing:** `cargo build`, `scripts/test.sh`,
  `cargo clippy --all-targets -- -D warnings`. After renderer, shader,
  windowing or input changes: `scripts/smoke-run.sh 15`.
- **Commits:** `feat:` / `fix:` / `docs:`, one commit set per task, and the
  ticket number in the message.
- **One worktree and one branch per seat**; file-disjoint seats only.
- **A seat never dispatches a subagent.**
- Reviewers run on `opencode-go/longcat-2.5-free`, implementers on
  `opencode-go/beam-free` (higher variants for bigger tasks).

## Task 69b: SVG export — the UI half

**What to build:** the review of ticket 69 split the work. The serializer half
(69a) is merged. This task closes the two UI findings: the SVG export entry
must be visibly disabled in 3D, and the top bar's export affordance must be
wired.

Files: `src/ui/menu.rs`, `src/ui/top_bar.rs`, `src/app/viewport.rs`,
`tests/` (the export-menu test file).

- The menu and top-bar SVG entries are disabled (greyed out, not merely
  inert) while the view is in 3D, and enabled in 2D. Invoking a disabled
  entry still produces the existing explanatory toast, so the reason is
  stated when a user tries it.
- The top bar's screenshot/export control opens the SVG path in 2D.
- Tests: the menu entry's enabled state follows the 2D/3D mode; the top-bar
  control invokes the export path.

Gates: `cargo build`, `scripts/test.sh`, clippy `-D warnings`,
`scripts/smoke-run.sh 15`.

Done when: the findings are closed, the re-review is clean, and the branch is
merged into `main` with the ledger updated.

## Task 72: Windows packaging

**What to build:** a Windows x86_64 release artifact, built with MSVC on a
`windows-latest` runner, shipped as an unsigned `.zip` of the `.exe` plus the
licence and notice files.

Full spec: `.scratch/field-viewport/issues/72-windows-packaging.md`
(also `.scratch/field-viewport/specs/windows-packaging-spec.md`).

Files: `.github/workflows/package.yml`, `.github/workflows/release.yml`,
`.github/workflows/nightly.yml`, `.github/workflows/ci.yml`, `scripts/`,
`packaging/windows/`, `README.md`, `docs/CODEBASE.md`.

- A `windows-latest` / `x86_64-pc-windows-msvc` matrix entry builds the
  release binary and zips it with `README.md`, `LICENSE`,
  `THIRD_PARTY_LICENSES.md` and `packaging/THIRD_PARTY_NOTICES.txt` at the
  archive root. The packaging step validates the zip's contents.
- The asset-count assertions in `release.yml` and `nightly.yml` become four.
- The generated release notes gain a Windows section: unsigned by design, the
  SmartScreen "Windows protected your PC" path, no installer.
- CI gains a `windows` job: build, test and clippy on `windows-latest`, with
  rust-cache. That is the gate — no GUI launch is claimed, because a runner
  has no display.
- The application icon is generated into `packaging/windows/` and embedded in
  the `.exe` with `winres` through a Windows-gated `build.rs`. If the MSVC
  resource compiler is not on the runner's PATH, add `ilammy/msvc-dev-cmd` to
  the workflow. If the icon still cannot be embedded, stop and report — no
  silent un-iconed executable.
- README, the `docs/CODEBASE.md` platform table and the download page list
  Windows. The v0.1.0 target line in `AGENTS.md` does not change; Windows
  ships as `v0.2.0`.

Out of scope: signing, installers, auto-update, telemetry, ARM64 Windows, a
Windows GUI smoke test in CI, checksums, any `src/` change.

Gates: workflow YAML parses; `bash -n` on changed scripts; the Linux gates
(`cargo build`, `scripts/test.sh`, clippy) stay green; `scripts/check-docs.py`
passes.

Done when: a pre-release tag or `workflow_dispatch` dry run is green and
recorded, and no real tag is cut without it.

## Task 39: Time-dependent fields (Part A)

**What to build:** a `t` variable in vector-field formulas, so a field can
evolve over time, using the existing `SimClock` and transport from ticket 26.

Full spec: `.scratch/field-viewport/issues/39-time-component.md`
(also `.scratch/field-viewport/specs/time-dependent-fields-spec.md`).

Files: `src/field.rs`, `src/sim_clock.rs`, `src/viewmodel.rs`,
`src/sampling.rs`, `src/streamlines.rs`, `src/particles.rs`,
`src/app/simulation.rs`, `src/app/input.rs`, `src/ui/transport.rs`, `tests/`.

- `FieldExpr::eval3_at(p, t)` and `FieldExpr::is_time_dependent()`. Time
  dependence is detected at parse time by tokenizing the components and
  looking for `Token::Var("t")`. `eval3` delegates at `t = 0.0`. No
  `eval2_at`: every sampling path already evaluates through `eval3`.
- `SimClock` gains `loop_period: Option<f32>`; `advance` wraps into
  `[0, period)` with a floor-modulo. `reset_time` clears `t` only; the full
  reset clears the loop too.
- Time-aware seams, each delegating at `t = 0.0`: `sampling::sample_field_at`,
  `streamlines::trace_at`, `particles::AdvectSettings.time`,
  `dot_instances_at`, `trail_instances_at`.
- RK4 substeps evaluate the field at `t`, `t + dt/2`, `t + dt`.
- `State` gains a transient `time: f32` and `set_time(t)`, which marks the
  state dirty only when the field is time-dependent and `t` changed. The
  clock's `t` is mirrored into the state after each advance.
- `has_clock_consumer`, `transport_visible` and `should_animate` also become
  true for a time-dependent field, so the transport appears without particles
  and a paused or static view requests no repaints.
- The transport gains a loop toggle and a period control, shown only for a
  time-dependent field. No new shortcut, no second playback row.
- Nothing new is persisted.

Out of scope: plot animation (ticket 38), ticket 37 itself, a second clock,
persisting the clock, temporal coherence of streamlines, GPU advection.

Gates: `cargo build`, `scripts/test.sh`, clippy `-D warnings`,
`scripts/smoke-run.sh 15`.

Done when: `t` works in formulas, existing formulas behave exactly as before,
the scene rebuilds while playing and idles when paused or static, and the
re-review is clean.

## Task 38: Plot animation (Part B)

**Blocked by:** ticket 37 (the scalar plot) and ticket 39 (the `t` seam).

**What to build:** re-sample the scalar plot at the current `t` each frame
while the transport plays; a paused plot keeps its last mesh; a static plot
never rebuilds and never requests a repaint; an invalid edit keeps the last
valid mesh and shows the parse error; switching 2D/3D re-samples at the
current `t`.

Full spec: `.scratch/field-viewport/issues/38-animate-functions.md`.

Files: `src/plot.rs`, `src/sim_clock.rs`, `src/viewmodel.rs`,
`src/app/simulation.rs`, `src/ui/transport.rs`, `tests/plot_test.rs`.

This task does not start until both blockers are merged.

## Task 74: Review-record enforcement

**What to build:** make "main is reviewed work only" a mechanical check instead
of a habit. Three tickets (69, 70, 71) were pushed to `main` without review
before this effort's repair rounds; the rule exists in `AGENTS.md` but
nothing enforces it.

Files: `scripts/check-review-records.py` (new), `.github/workflows/ci.yml`,
`docs/reviews/` (new directory, one file per reviewed ticket).

- `docs/reviews/` holds one record per reviewed ticket — the ticket, the
  branch, the commit range per round, the reviewer's verdict per round, the
  findings and how each was closed. The controller writes the record as part
  of the merge step, from the reviewer's report.
- `scripts/check-review-records.py` scans the commit history for subjects
  containing `(ticket NN)` and requires a record under `docs/reviews/` that
  names each such commit hash. A commit with no record fails the check.
  The check must be deterministic, must not modify files, and must work from
  any directory. It needs a baseline for commits that predate the rule.
- The check runs in CI (its own job or step, independent of the Rust jobs).
- The rule it enforces is documented in `AGENTS.md` and the wiki.

Gates: the script passes on `main` after the record for every ticket-claiming
commit exists; a deliberately missing record fails it; the existing CI jobs
stay green.

Done when: an unreviewed ticket-claiming commit cannot reach `main` without a
CI failure.

## Task 37: Scalar function plot

**Status:** ready-for-agent, but not in this wave — it is the prerequisite
for ticket 38. Full spec: `.scratch/field-viewport/issues/37-scalar-function-plot.md`.

## Related

- The ledger and the per-task briefs, reports and findings:
  `.superpowers/sdd/2026-11-05-vector-field-explorer/`.
- The fix loop: `references/fix-loop.md`.
- The project wiki: `docs/wiki/index.md`.
