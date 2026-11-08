# 09: README + cleanup + final whole-branch review

**What to build:** the repo becomes releasable from a newcomer's perspective: a
README that explains installing from the published release or building from
source, the controls and menus, and known limitations; a cleanup sweep of dead
code/branches/cruft; then one broad review of the whole branch (T1 → last
ticket), with every parked minor triaged to a fix-or-defer ruling.

**Blocked by:** 02–08, 11, 12, 13, 14, 15, 16, 17. This is the integration
point; nothing feature-shaped may land after it except review fixes.

**Status:** ready-for-agent

## README (repo root)

- [ ] What it is + one screenshot; link to the GitHub Pages site (ticket 16).
- [ ] Install: Linux/macOS download from `releases/latest`, archive contents,
      macOS Gatekeeper note.
- [ ] Build from source: Rust toolchain, system deps, `cargo run`,
      `scripts/smoke-run.sh`; test/clippy commands.
- [ ] Controls: formula syntax and field presets; 2D/3D mouse gestures;
      click-to-seed; panel rails and `Ctrl+B`/`Ctrl+P`; top menus and
      shortcuts (ticket 15); screenshot; help overlay; Settings.
- [ ] Persistence: what survives restart (view state, layout, style) and what
      does not (camera state — use camera presets). Field preset vs camera
      preset distinction from `GLOSSARY.md`.
- [ ] Limitations: static fields only (no `t`), no browser build, no
      import/export of data, NaN handling.
- [ ] License: GPL-3.0-only, link to `LICENSE`; no warranty note.

## Cleanup sweep

- [ ] `git status` clean except ignored/scratch paths; decide the fate of
      untracked `.scratch/vfe-tmp/` (delete or ignore explicitly).
- [ ] Delete stale local branches whose work is merged (e.g.
      `feature/camera-zoom-pan`, `feature/export-tests`, `feature/task5-sampling`,
      `feature/task6-rendering`, `feature/vector-field-explorer`,
      `feature/preset-persistence`); verify each is an ancestor of `main`
      before deleting. Prune remote-tracking branches.
- [ ] Remove dead code/dead comments; keep `cargo clippy --all-targets -- -D
      warnings` clean; no TODO/FIXME left without a ticket reference.
- [ ] Confirm `.gitignore` covers build artifacts, `.superpowers/`, and
      scratch dirs; confirm `LICENSE` + `Cargo.toml` license agree.

## Final whole-branch review

- [ ] `bash <sdd-scripts>/review-package docs/superpowers/plans/2026-11-05-vector-field-explorer.md caa7c09 HEAD`
      (T1 base `caa7c09`).
- [ ] Dispatch the reviewer at `opencode-go/claude-haiku-5-5#max` (background),
      with the brief, the diff package, `GLOSSARY.md`, and the ADRs.
- [ ] Triage every parked minor: issue-01 related (DrawState naming,
      `inv_view_proj`, sort duplicate, per-frame buffers), tickets 02–05
      (4+3+2+4), 06 (3), 07 (3), 08 (1 cosmetic), plus findings from 14, 15, 16,
      17. Each becomes a fix commit or a dated `Ruling:` in the ledger.
- [ ] Review fixes get a scoped re-review per the fix-loop reference.
- [ ] Verify docs consistency: README ↔ `GLOSSARY.md` ↔ `docs/adr/` ↔
      `docs/CODEBASE.md` (ticket 10 lands after this; only links/terms here).
- [ ] Verify every GitHub workflow on `main` is green: CI (build/test/clippy),
      smoke, Pages, release dry-run; then **merge the feature branch and
      `git push origin main`** (private repo
      `git@github.com:ttgg1/vector-field-explorer.git`).
- [ ] Ledger closes the plan: per-ticket commit ranges, final verdict, parked
      rulings, and the pushed `main` SHA.

**Dispatch notes:** brief = plan task 14 via `scripts/task-brief … 14` plus
this ticket; reviewer per current policy (`opencode-go/claude-haiku-5-5#max`).
Run seats in the background per AGENTS.md.
