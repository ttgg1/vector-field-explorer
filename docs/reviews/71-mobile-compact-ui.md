# Review record — ticket 71: mobile compact UI polish

**Ticket:** 71
**Reviewed:** 2026-11-10
**Branch:** `feature/fix-ticket-71`
**Merge:** fast-forward into `main` (this repository keeps a linear history)

## Commits

| Commit | Subject |
| --- | --- |
| `b0665b4` | feat: mobile compact UI polish (logo, GitHub link, swipe-expand sheet) (ticket 71) |
| `139c6e6` | fix: mobile compact UI polish (ticket 71) |
| `1c3858f` | fix: mobile compact UI polish (ticket 71) |
| `8d7af89` | fix: mobile compact UI polish (ticket 71) |
| `6c57f11` | fix: close compact UI review findings (ticket 71) |
| `fb33797` | fix: strengthen compact UI evidence (ticket 71) |

## Rounds

| Round | Range | Verdict | Open at the end |
| --- | --- | --- | --- |
| 1 (whole task, post-hoc) | base..`b0665b4` | Needs fixes | I1 (the 44 pt legibility work was not implemented), I2 (no tests), I3 (the sheet was a two-state boolean, not the specified three states), I4 (swipe-down always closed; no 30% peek, no 10% viewport floor), I5 (the GitHub button was plain text), I6 (Appearance stayed a hamburger popover), I7 (no logo or gear assertions), plus minors |
| 2 | `b0665b4`..`139c6e6` | Needs fixes | I1 (the row floor was a no-op), I2 (no tests), I5 (the icon texture was a white disc) and a new Important (the compact hamburger listed a dead `Appearance` entry) |
| 3 | `139c6e6`..`1c3858f` | Needs fixes | I1, I2, I5 open; the dead menu entry closed; M5-M8 open |
| 4 | `1c3858f`..`8d7af89`,`6c57f11` | Needs fixes | I5 closed only in implementation — the tokenizer consumed `[-.0-9eE+]` greedily and skipped the `A` arc, so the mark rasterised to a distorted blob; I1, I2 and the minors were closed in implementation but unevidenced |
| 5 | `6c57f11`..`fb33797` | Approved | none |

## Findings and how they closed

- **I1 — the 44 pt floor.** The compact sidebar rows, preset rows and sheet
  controls honour a compact-only floor; the wide layout is unchanged. The
  test drives the real compact panel widgets, not the constant.
- **I2 — tests.** `tests/responsive_test.rs` covers the three sheet states
  and their transitions, the Collapsed and Expanded height clamps, the 10%
  viewport floor, the swipe up and down transitions, and the compact row
  floor.
- **I3 / I4 — the sheet.** `SheetSize` replaces the two-state boolean;
  Collapsed 30%, Default 55%, Expanded 90%, with a 10% viewport floor and
  transitions that request a repaint.
- **I5 — the GitHub mark.** The SVG path data is parsed with a correct SVG
  1.1 number reader and drawn from that geometry; the mark rasterises with
  the expected bounding box, and the test rejects a filled circle.
- **I6 — Appearance.** In compact mode the Appearance controls live in the
  Controls sheet; the hamburger popover is gone.
- **I7 — top-bar assertions.** The logo is asserted present and the gear
  asserted absent.
- **Minors.** The dead hamburger response field was removed, the stale doc
  comments and unused parameters were cleaned up, the no-op rename was
  dropped, and the hamburger test was renamed to state that it excludes the
  sheet's Appearance entry.

## Evidence note

Rounds 4 and 5 separated "the code is right" from "the proof exists". Round
4 closed the implementation concerns; round 5 produced the failing-before /
passing-after output for each remaining finding, which is what the
controller's bar of 2026-11-10 requires. A reviewer may accept a
before/after text diff where no failing test is meaningful, but must say so.

## Not verified here

No wasm build and no manual phone check were run.
