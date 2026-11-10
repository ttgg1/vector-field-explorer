# Review record — ticket 70: fly camera on non-alphabetic layouts

**Ticket:** 70
**Reviewed:** 2026-11-10
**Branch:** `feature/fix-ticket-70`
**Merge:** fast-forward into `main` (this repository keeps a linear history)

## Commits

| Commit | Subject |
| --- | --- |
| `b191a3c` | fix: fly camera no longer exits on strafe under non-QWERTY layouts (ticket 70) |
| `bf16273` | fix: address ticket 70 review findings (repeat, consumption, regression test) (ticket 70) |
| `39ce6d9` | fix: add pure fly_toggle_key seam and regression tests (ticket 70) |

## Rounds

| Round | Range | Verdict | Open at the end |
| --- | --- | --- | --- |
| 1 (whole task, post-hoc) | base..`b191a3c` | Needs fixes | I1 (the ticket-mandated regression test was missing), I2 (a held key repeated the toggle because `repeat` was ignored), plus minors (consumption hardcoded to the logical key, a test name that asserted the opposite of its name) |
| 2 (fix 1) | `b191a3c`..`bf16273` | Needs fixes | I1 still open: the new test asserted a hand-written predicate instead of the production decision, so it passed against the pre-fix code |
| 3 (fix 2) | `bf16273`..`39ce6d9` | Approved | none |

## Findings and how they closed

- **I1 — the regression test.** Round 3 moved the decision into one pure
  function, `fly_toggle_key` (`src/fly_camera.rs`, called from
  `src/app/input.rs`), and made the regression test drive production code so
  it fails against the pre-fix behaviour.
- **I2 — key repeat.** The toggle consumes the key event only when the event
  is not a repeat, so a held key does not toggle fly mode repeatedly.
- **Minors.** The consumption is keyed on the physical key with the logical
  key as the fallback; the misleading test name was corrected.

## Seam

One `#[doc(hidden)] pub` seam was added (`fly_toggle_key`), with the reason
recorded in the commit body and in the SDD ledger. The alternative — a seam
over `frame_input` — was rejected because it would have required a whole
`VfApp` and an `egui::Context`.
