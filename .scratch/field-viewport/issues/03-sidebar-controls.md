# 03: Sidebar controls wired to the seam

**What to build:** From the user's perspective: the sidebar exposes every scene knob — sampling density (drag value; clamped 4..=200, and 4..=40 only when sampling actually runs three-dimensional), arrow-length slider (0.1..=4, normalize-lengths checkbox), coloring (colormap selectable-label row + auto/manual range with two drag values when auto is off), Features stubs (streamlines + particles checkboxes and a speed slider — inert until tickets 04/05 land), and the six domain extrema drag values with min<max enforced by the seam's swap rule. Every control marks the scene dirty so the snapshot rebuilds and the status-bar count reflects it.

**Blocked by:** 02 (bottom-bar/mode/status wiring must exist; sidebar rewrites build on the same status-bar dirty-flag plumbing).

**Status:** in-progress (implementer dispatched 2026-11-07, BASE c62507a)

- [ ] Density clamp keys to the dimension BEING SAMPLED (ledger ruling: 4..=40 / n³ only when sampling 3D; else 4..=200 / n²) — seam already carries a test for this; UI must expose it consistently
- [ ] Domain extrema drag values: reversed pairs accepted and swapped (seam enforces; no UI error path needed)
- [ ] Colormap row + manual-range pair feed the seam's color_range (None when auto-fit)
- [ ] Features checkboxes + speed slider exist as stubs (no streamline/particle behavior required here)
- [ ] Gates: build / clippy `-D warnings` clean / full `cargo test` green; commit `feat: sidebar controls wired to viewmodel`
- [ ] Same salvage-draft warning as ticket 02: uncommitted non-compiling UI churn on disk is a draft to salvage-or-restore, never to commit

**Dispatch notes:** brief = plan task 8 (second half — Steps 2–3 sidebar semantics). Spec `.scratch/field-viewport/spec.md` is binding on seams.
