# Tickets 69/70/71 — review round 1 (2026-11-10)

## The claim

The post-hoc review of tickets 69, 70 and 71 (commits `6e3929b`, `b191a3c`,
`b0665b4`) found spec gaps in all three on 2026-11-10. The authoritative
finding text is in the ledger directory:
`.superpowers/sdd/2026-11-05-vector-field-explorer/task-69-findings.md`,
`task-70-findings.md`, `task-71-findings.md`. This page is the summary and the
status board; it never replaces the findings lists.

## Ticket 70 — fly camera under non-QWERTY layouts (`5cc8e49..b191a3c`)

Verdict: ❌ spec issues. I1 the ticket-mandated regression test in
`tests/fly_camera_test.rs` was missing, and that file still asserted the old
`menu::shortcut_for(MenuAction::ToggleFly)` contract. I2 `src/app/input.rs`
ignored `Event::Key.repeat`, so holding the physical C key toggled fly
repeatedly. Minors: the consumption check was hardcoded to logical `Key::C`
(`b191a3c:src/app/input.rs:58-64` matched the physical-or-logical C key;
`b191a3c:src/app/input.rs:66-69` consumed hardcoded `Key::C`, at `:68`). In
the current code, the event-derived predicate is `src/app/input.rs:49-55` and
the matched key is consumed at `:56-58`; this is the fix, not the historical
finding. The test at `tests/menu_test.rs:374` asserted the opposite of its
name; `src/fly_camera.rs` was in ticket scope but untouched.

Fix seat: `feature/fix-ticket-70` (worktree `vfe-fix-70`).

## Ticket 69 — SVG screenshot export (`b191a3c..6e3929b`)

Verdict: ❌ spec issues, including one Critical. C1 `src/screenshot.rs`
(~509-545) never serialised 2D particles: they live in `scene.dots`, but only
`scene.arrows`/`scene.combined` were iterated, so the `tag > 0.5` particle
branch was dead on the 2D path — the feature's target mode. I1 the
ticket-mandated element-count cap, user warning and size-risk documentation
never shipped. I2 the required test cases (element presence, hex colours, 3D
rejection, extension forcing) were missing from `tests/svg_export_test.rs`.
I3 the 3D entry was not visually disabled (only a toast on invoke) in
`src/ui/menu.rs` and `src/ui/top_bar.rs`. I4 `src/ui/top_bar.rs` was in scope
but untouched. Minors: `trim_end_matches(".png")` over-trims in
`default_svg_filename`; no `<?xml?>` declaration.

Positives recorded: `src/export.rs::save_svg` placeholder properly replaced,
platform gating correct, `ensure_svg_extension`/`default_svg_filename` tested.

Fix seats: `feature/fix-ticket-69a` (serializer: `src/screenshot.rs`,
`src/export.rs`, `tests/svg_export_test.rs`) and, after 70 and 71 land,
`feature/fix-ticket-69b` (UI: `src/ui/menu.rs`, `src/ui/top_bar.rs`,
`src/app/viewport.rs`).

## Ticket 71 — mobile compact UI polish (`6e3929b..b0665b4`)

Verdict: ❌ spec issues, seven Important. I1 the whole legibility half (≥44 pt
compact rows and touch targets in `src/ui/layout.rs`, `src/ui/presets.rs`,
`src/ui/sidebar.rs`) was unimplemented. I2 `tests/responsive_test.rs` was not
extended. I3 the sheet was a two-state `sheet_expanded: bool`
(`src/app/mod.rs:175`, `src/app/panels.rs:339+`) instead of the three
states the ruling specified. I4 swipe-down always closed
(`src/app/panels.rs:433+`): no 30% peek, no 10% viewport floor,
`SHEET_MAX_FRACTION` in `src/ui/layout.rs:72` still 0.55. I5 the GitHub button was plain text
(`src/ui/top_bar.rs:262-264`) where ruling Q9 requires an inline SVG texture
with text fallback. I6 Appearance stayed a hamburger-anchored popover
(`src/ui/top_bar.rs:279-285`) instead of moving into the sheet. I7
`tests/top_bar_test.rs:359-368` asserted neither logo presence nor gear
absence. Minors: the tab-bar ✕ did not reset sheet state, swipe-up lacked
`request_repaint`, and `tests/svg_export_test.rs` had an unrelated import
cleanup owned by another seat.

Fix seat: `feature/fix-ticket-71` (worktree `vfe-fix-71`).

## What the round actually taught

Every one of the three tickets implemented the *visible* half of its brief and
skipped a required half — the test file, the particle branch, the touch
target. The briefs had listed those halves explicitly. A seat that reads only
the headline of a brief will ship the headline; briefs need the acceptance
items repeated inside the seat's own prompt, and reviewers need the findings
lists kept next to the briefs.

## Related

- [Repairing the unreviewed tickets 69/70/71](../decisions/2026-11-10-repairing-unreviewed-tickets.md)
- [Dispatch, review and fix loop](../process/dispatch-and-review-loop.md)
