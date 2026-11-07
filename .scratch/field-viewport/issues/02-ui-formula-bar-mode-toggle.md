# 02: Live formula bar, mode toggle, status bar

**What to build:** From the user's perspective: typing a formula into the bottom bar re-samples the arrow field live (arrow count reacts to valid edits); an invalid formula keeps the previous field working and shows a red parse error in the status bar; selecting a field-preset in the bar's ComboBox overwrites the formula (and switches mode when the preset is 3D); a 2D/3D selectable-label pair toggles between the orthographic 2D view and the perspective orbit 3D view; in 3D, left-drag orbits the camera and scrolling zooms (clamped) — camera input goes through the viewmodel seam's pure camera actions. The status bar shows the snapshot's instance count.

**Blocked by:** None (can start immediately) — the viewmodel seam landed with Task 8A (ledger: commit `522d011`).

**Status:** ready-for-agent

- [ ] Live edit: a valid formula edit marks the scene dirty and rebuilds the snapshot; status-bar count updates accordingly
- [ ] Invalid formula: old field preserved, parse error rendered RED in the status bar (seam behavior already unit-tested there; UI wiring needs build/clippy green, plus real tests for any NEW pure helper)
- [ ] Preset ComboBox selection overwrites the formula with the preset expression
- [ ] Mode toggle switches projection (ortho ↔ perspective) and enables the orbit/zoom gestures in 3D only
- [ ] Right-drag ortho pan: OPTIONAL — reviewer must not block on its absence
- [ ] Gates: cargo build green; `cargo clippy --all-targets -- -D warnings` clean; FULL `cargo test` suite green before committing; commit `feat: live formula editing, mode toggle, status bar readouts`
- [ ] ⚠ The working tree carries a NON-COMPILING salvage draft from an aborted dispatch (~226 uncommitted lines in the UI modules + seam helpers). The implementer decides: salvage or restore-then-rebuild — broken state must never be committed

**Dispatch notes:** brief = plan task 8 via `scripts/task-brief docs/superpowers/plans/2026-11-05-vector-field-explorer.md 8` (first half only — sidebar knobs belong to ticket 03). Keep dispatch scope small (flash-model stall history); commit before session end.
