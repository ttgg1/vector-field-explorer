# Ticket 77 — reference visibility design (2026-11-11)

**Intent.** Let people clear viewport reference decoration without hiding field data. Preserve today's appearance on first launch and for existing saved settings. This is the owner-approved scope of [ticket 77](../../../.scratch/field-viewport/issues/77-reference-visibility-controls.md); [workflow](../../../references/workflow.md) governs execution and gates.

## Behavior and boundaries

- Add independent, checked-by-default **Show grid** and **Show coordinate system** controls in the sidebar's Display section (Wide and Compact), with unambiguous tooltips and Display reset restoring both to visible. Grid includes the 3D face grid **and domain wireframe box** (the flat mode has its plane grid); coordinate system means only the colored X/Y axes in 2D or X/Y/Z axes in 3D.
- Store two booleans on `viewmodel::State`, mirrored through `ui::sidebar::Controls` and its `apply` handoff. One pair applies across 2D/3D; edits mark the scene dirty without changing field sampling or suppressing flow/click lines. Persist both via `persistence::ViewRecord`'s existing capture/restore path, defaulting missing fields to `true`; preserve existing corrupt-settings fallback. No new storage envelope or version.
- No per-mode preferences, field-content toggles, ticks, numeric labels, gizmos, renderer overhaul, or independent screenshot composition.

## Composition choice

Prefer splitting `reference.rs` generation at the **reference geometry seam** into grid-plus-box and colored-axes contributions (or an equivalent explicit visibility selection). `app::compose_lines` includes each enabled contribution **before** flow and click lines; also honor restored visibility in the constructor's initial backdrop. This keeps the existing `SceneGPU.lines` pipeline intact: `frame_viewport` passes the same composed scene to paint and the existing PNG and 2D SVG capture paths. Splitting renderer line collections would duplicate ordering/state work across render and export; filtering already-composed lines by color would conflate identity with styling and risk removing similarly colored field lines. Preserve current line ordering when both flags are on.

## Evidence and handoff

Test-first additions in `tests/reference_lines_test.rs`, `tests/sidebar_controls_test.rs`, and `tests/persistence_test.rs` should cover all four visibility combinations in both dimensions, grid/box coupling, axes-only behavior, retained flow/click geometry, shared mode semantics, Display reset, defaults, storage round-trip, and old records with missing fields. The shared `SceneGPU` path supports viewport/PNG/SVG parity; GPU-free tests do **not** prove pixel output. Builder chooses helper signatures, setter names, and precise pure test seam without widening public API solely for tests; coordinate shared-file ownership after ticket 76.

For implementation, workflow requires `cargo build`, `scripts/test.sh`, `cargo clippy --all-targets -- -D warnings`, `python3 scripts/check-docs.py`, `git diff --check`, changed-file formatting, and `scripts/smoke-run.sh 15` for this rendering-composition change. Dedicated wasm/browser and manual visual/screenshot/painted-ink checks are **owner-waived, not passed** under the current workflow; retain platform behavior and automated coverage. This document alone is not implementation authorization.
