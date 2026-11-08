# 10: Detailed whole-codebase documentation

**What to build:** a complete documentation set for the final merged codebase
(user-requested 2026-11-07, last task): `docs/CODEBASE.md` — architecture
overview (data flow: formula string → `FieldExpr` parse → `viewmodel::State`
dirty-flag snapshot → `SceneGPU {arrows, lines, dots}` → wgpu instanced paint →
egui panels), then one section per module with public API, key invariants, and
the tests that pin them; plus build/test/CI/release commands, persistence
schema/migration, and the egui-0.37/wgpu pitfalls. Every claim verified against
actual code, not the plan.

**Blocked by:** 09 (README + cleanup + final review) — document the final
merged, reviewed code.

**Status:** ready-for-agent

## Coverage

Every file under `src/`, including the modules added after the original plan:

- `src/app.rs`, `src/main.rs`, `src/lib.rs`, `src/window.rs`
- `src/field.rs`, `src/domain.rs`, `src/presets.rs`, `src/sampling.rs`
- `src/viewmodel.rs`, `src/persistence.rs`
- `src/camera.rs`, `src/render.rs`, `src/renderer.rs`
- `src/colormap.rs`, `src/export.rs`, `src/screenshot.rs`, `src/i18n.rs`
- `src/particles.rs`, `src/streamlines.rs`
- `src/ui/*` (`top_bar`, `formula_bar`, `sidebar`, `layout`, `style_panel`,
  `presets`, `help_overlay`, `status_bar`, `arrows`)
- shaders `src/*.wgsl` and the render pipeline/layout contract

## Content requirements

- [ ] Data-flow + seam diagram (prose/ascii): pure modules plus the one
      viewmodel/scene-builder seam; where GPU paint sits.
- [ ] Per-module sections: purpose, public API, invariants, tests that pin
      them, and known limitations.
- [ ] Persistence: the single `eframe::APP_KEY` envelope (`AppStyle`,
      `LayoutState`, `ViewRecord`), ticket-07 bare-style migration, corruption
      fallback rules, and why camera state is not persisted.
- [ ] Input: gesture map (2D pan/zoom, 3D orbit/pan/zoom, click-to-seed),
      panel rails/shortcuts, menu actions (ticket 15), and the pure seams.
- [ ] Rendering: instance layouts, shaders, ortho vs perspective, offscreen
      screenshot path (RGBA), smoke-test rationale.
- [ ] Build/test/CI: cargo commands, `scripts/smoke-run.sh`, GitHub Actions
      (CI, smoke, Pages, release), licensing (GPL-3.0-only).
- [ ] Pointers to `GLOSSARY.md` and `docs/adr/`; no duplication of their
      content.

## Acceptance

- [ ] `docs/CODEBASE.md` covers every file under `src/` (no module skipped, no
      invented APIs).
- [ ] Cross-check against `cargo test` inventory: each documented behavior
      names its test.
- [ ] Links to README/GLOSSARY/ADRs/release workflows resolve.
- [ ] Gates: no code changes; `docs:` commit
      `docs: whole-codebase documentation (docs/CODEBASE.md)`; accuracy
      reviewed against code.
- [ ] Reading it cold is enough to make a first safe change in any module.

**Dispatch notes:** dispatch AFTER 09's final review fixes land. Brief for the
implementer = this ticket (no plan task maps to it). Reviewer: accuracy check
only (`opencode-go/claude-haiku-5-5#high`), run in the background.
