# 08: Screenshot export

**What to build:** From the user's perspective: a screenshot action opens a save dialog and writes a PNG of exactly what the viewport shows right now — arrows, streamlines, particles, in whichever mode is active. This intentionally re-adds the save-dialog dependency (it was pruned from deps; re-adding it is the decision, not drift).

**Blocked by:** 06 (all three paint passes must exist in both views — this is the paint-completeness gate; style-panel persistence does NOT gate screenshots).

**Status:** COMPLETE (2026-11-08; `9d0cb6d` + fix `43bd549`, review clean after fix round 1; 1 cosmetic minor parked for ticket 09/13)

- [x] Framebuffer readback + PNG encode path pinned by real tests (RGBA byte order contract — repo precedent pins RGBA-not-BGRA for the preset PNGs; ledger the same ruling for screenshots)
- [x] Save-dialog flow is manual (headless env): implement per brief; user does the visual check; gates are compile + pure-path tests
- [x] Dependency re-add recorded in the report as intentional
- [x] Gates: build / clippy `-D warnings` / full `cargo test` green; commit `feat: screenshot export via save dialog`

**Dispatch notes:** brief = plan task 13 via `scripts/task-brief … 13`.
