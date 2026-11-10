# Review record — ticket 39: time-dependent fields (Part A)

**Ticket:** 39
**Reviewed:** 2026-11-10
**Branch:** `feature/ticket-39`
**Merge:** fast-forward into `main` (this repository keeps a linear history)

## Commits

| Commit | Subject |
| --- | --- |
| `517c3c7` | feat: time-dependent fields (Part A) (ticket 39) |
| `da457aa` | fix: add the ticket 39 test coverage (ticket 39) |
| `8b4b8c7` | fix: tighten the ticket 39 tests (ticket 39) |
| `7c2ff33` | fix: assert paused views request no repaint (ticket 39) |

## Rounds

| Round | Range | Verdict | Open at the end |
| --- | --- | --- | --- |
| 1 (whole task) | base..`517c3c7` | Needs fixes | Critical: the ticket's test plan was not implemented at all (zero new test functions) and the implementer report claimed coverage that did not exist |
| 2 (fix 1) | `517c3c7`..`da457aa` | Needs fixes | Important: `rk4_substeps_see_intermediate_times` was vacuous (passed with a frozen `t`); two minors (a streamline test that passed on rounding noise, a "paused" test that never paused) |
| 3 (fix 2) | `da457aa`..`8b4b8c7` | Needs fixes | the paused-clock repaint assertion and the quoted failing-before / passing-after evidence were still missing |
| 4 (fix 3) | `8b4b8c7`..`7c2ff33` | Approved | none |

## Findings and how they closed

- **Critical — no tests.** Round 2 added 18 tests covering the rulings:
  `eval3_at` differs from `eval3` for a `t` formula and both components see
  `t` in 2D and 3D; `is_time_dependent` is true only for a free `t`; the
  loop wraps with a floor-modulo including a negative `t`; a non-finite or
  non-positive period is rejected; a paused clock does not advance or wrap;
  `reset_time` keeps the loop period while the full reset clears it; the
  transport is visible for a time-dependent field; `sample_field_at` and
  `trace_at` vary with `t`; advection at a non-zero time differs; a
  time-dependent field rebuilds while playing and does not rebuild when
  static or when `t` is unchanged.
- **Important — vacuous RK4 test.** Round 3 made
  `rk4_substeps_see_intermediate_times` fail under a frozen-`t` mutation.
- **Minor — noise-sensitive streamline test.** Round 3 rejected
  rounding-only differences.
- **Minor — the "paused" half never paused.** Round 4 added the paused-clock
  repaint assertion and quoted the failing-before / passing-after output.

## Not verified here

The pre-feature base does not compile (the `eval3_at` family is missing), so
falsifiability was shown by behaviour mutations rather than by checking out
the base. The wasm target was checked with the rustup toolchain.

## Notes

- Two `#[doc(hidden)] pub` seams were added for the ticket-39 work
  (`eval3_at`, `sample_field_at`, `trace_at` and the `*_at` particle helpers
  are production API, not test-only); no test-only widening was needed.
- `cargo fmt` differences are not a gate in this repository (controller
  ruling, 2026-11-10).
