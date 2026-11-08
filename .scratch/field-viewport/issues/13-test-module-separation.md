# 13: Move inline unit tests out of `src/` into `tests/` (strict layout)

**What to build (refactor, no behavior change):** restore the repo's intended
layout — production code in `src/`, every test under `tests/`. After ticket 14
the split is 152 `#[cfg(test)]` tests across 18 source files versus 27 tests in
`tests/`. Move all inline test modules into integration-test files and add a
guard that keeps new ones out.

**Status:** ready-for-agent (refactor)

## Current inventory (2026-11-08, post-ticket-14)

| `src/` file | inline tests | target |
|---|---:|---|
| `src/viewmodel.rs` | 25 | extend `tests/viewmodel_test.rs` |
| `src/particles.rs` | 21 | `tests/particles_test.rs` |
| `src/streamlines.rs` | 19 | `tests/streamlines_test.rs` |
| `src/screenshot.rs` | 13 | `tests/screenshot_test.rs` |
| `src/persistence.rs` | 9 | `tests/persistence_test.rs` |
| `src/colormap.rs` | 9 | `tests/colormap_test.rs` |
| `src/camera.rs` | 8 | extend `tests/camera_test.rs` |
| `src/field.rs` | 7 | `tests/field_test.rs` |
| `src/ui/style_panel.rs` | 6 | `tests/ui_style_panel_test.rs` |
| `src/window.rs` | 5 | `tests/window_test.rs` |
| `src/ui/presets.rs` | 5 | extend `tests/ui_preset_test.rs` |
| `src/sampling.rs` | 5 | `tests/sampling_test.rs` |
| `src/domain.rs` | 5 | `tests/domain_test.rs` |
| `src/ui/layout.rs` | 4 | `tests/ui_layout_test.rs` |
| `src/ui/top_bar.rs` | 3 | `tests/ui_top_bar_test.rs` |
| `src/ui/sidebar.rs` | 3 | extend `tests/sidebar_controls_test.rs` |
| `src/presets.rs` | 3 | `tests/presets_test.rs` |
| `src/ui/help_overlay.rs` | 2 | `tests/ui_help_overlay_test.rs` |
| **total** | **152** | plus 27 existing in `tests/` |

Existing files use the `tests/<module>_test.rs` convention; extend a matching
file rather than creating a second one. The inventory must be re-counted at
implementation time if tickets 15–17 add tests first.

## Rust constraint (read before planning)

`tests/*.rs` compile as separate crates: they can reach only `pub` items.
Many inline tests exercise private helpers, private fields, and internal
constants, so strict separation has a real cost. Allowed moves, in order of
preference:

1. Move tests that already use only the public API — no source change.
2. Promote a helper to `#[doc(hidden)] pub` (or move helper + test into a
   deliberate `pub mod testing`) where the test genuinely needs it; say why in
   the commit/ledger.
3. Stop and escalate if a test cannot move without a bad API change. The
   controller rules (documented allowlist vs API change) in the ledger; do not
   keep a silent `#[cfg(test)]` exception in `src/`.

## Work

1. Classify each inline test as public-API or private-helper before moving.
2. Move in ascending risk, one commit per module (`refactor: move <module>
   tests to tests/`): field, colormap, domain, presets, sampling, window,
   camera, screenshot, persistence, ui modules, then viewmodel, streamlines,
   particles.
3. Convert imports: `use super::*` / `crate::` → `use
   vector_field_explorer::{...}`. Integration tests cannot use `crate::`.
4. Preserve every test name and assertion where possible. No test may be
   deleted or weakened; total count must not drop below the implementation-time
   baseline (152 + 27 = 179 as of 2026-11-08).
5. Add `tests/test_layout_test.rs`: walk `src/` and fail if any `.rs` file
   contains `#[cfg(test)]`. Prove it red by temporarily adding one, then
   remove it.
6. Update `AGENTS.md` conventions: tests live in `tests/`, named
   `tests/<module>_test.rs`; `src/` carries no test modules.
7. Gates: `cargo build --all-targets`, `cargo test --all-targets`, `cargo
   clippy --all-targets -- -D warnings`. Smoke run only if `src/renderer.rs`
   or a shader changes.

## Acceptance

- [ ] `grep -rn '#\[cfg(test)\]' src` returns nothing.
- [ ] All moved tests pass; total test count ≥ implementation-time baseline.
- [ ] No production behavior change; any new `pub` is minimal, justified, and
      `#[doc(hidden)]` when test-only.
- [ ] The layout guard fails when an inline test module is added back, and
      passes on the clean tree.
- [ ] `AGENTS.md` documents the rule.
- [ ] The SDD ledger records each module move and every API-surface ruling.

## Non-goals

Rewriting test logic, adding coverage, touching the GPU paint layer, or
changing behavior delivered by tickets 14–17.

## Scheduling

Land after ticket 15 (the last feature ticket that adds inline tests) and
before ticket 09's whole-branch review, so the review sees the final layout.
Do not run it concurrently with another implementer seat.

## Risk

API widening is the main cost: a mechanical move can expose internals and
couple tests to implementation details. Reviewers must check every promoted
item is test-only and documented; prefer moving public-API tests unchanged
over promoting helpers.
