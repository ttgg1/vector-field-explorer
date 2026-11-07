# 10: Detailed whole-codebase documentation

**What to build:** A detailed documentation set for the entire codebase (user-requested 2026-11-07, last task): `docs/CODEBASE.md` — architecture overview (data flow: formula string → `FieldExpr` parse → `viewmodel::State` dirty-flag snapshot → `SceneGPU {arrows, lines, dots}` → wgpu instanced paint → egui panels), then one section per module (`src/app.rs`, `src/field.rs`, `src/domain.rs`, `src/presets.rs`, `src/sampling.rs`, `src/viewmodel.rs`, `src/camera.rs`, `src/render.rs`, `src/renderer.rs`, `src/colormap.rs`, `src/export.rs`, `src/i18n.rs`, `src/ui/*`) with public API, key invariants, and how tests exercise it; plus build/test/CI commands and the egui-0.37/wgpu pitfalls. Every claim verified against actual code, not the plan.

**Blocked by:** 09 (README+cleanup+final whole-branch review) — document the final merged code, post-review-fixes.

**Status:** ready-for-agent

- [ ] `docs/CODEBASE.md` covers every file under `src/` (no module skipped, no invented APIs)
- [ ] Data-flow + seam diagram (prose/ascii) matches code: seams = pure modules + the ONE viewmodel seam
- [ ] Per-module sections: public API, invariants, tests that pin them
- [ ] Cross-check against `cargo test` inventory: each documented behavior names its test
- [ ] Gates: no code changes; `docs:` commit `docs: whole-codebase documentation (docs/CODEBASE.md)`; accuracy reviewed against code

**Dispatch notes:** dispatch AFTER 09's final review wave lands its fixes. Brief for the implementer = this ticket (no plan task maps to it — it is user-added scope beyond plan task 14). Reviewer may be a cheap free model: accuracy check only.
