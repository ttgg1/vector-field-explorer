# Review record — ticket 69: SVG screenshot export (2D vector)

**Ticket:** 69
**Reviewed:** 2026-11-10
**Branches:** `feature/fix-ticket-69a` (serializer half), `feature/ticket-69b`
(UI half)
**Merge:** fast-forward into `main` (this repository keeps a linear history)

## Commits

| Commit | Subject |
| --- | --- |
| `6e3929b` | feat: add SVG screenshot export for the 2D view (ticket 69) |
| `2bb4c5b` | fix: serializer side of SVG export (ticket 69) |
| `03a116b` | fix: surface SVG element cap warning via toast (ticket 69) |
| `b92fd5e` | fix: make SVG element-cap warning visible on web path (ticket 69) |
| `bdd2a4b` | fix: remove wasm-unused over_cap and dead toast arm (ticket 69) |
| `7f85459` | fix: wire SVG screenshot controls (ticket 69) |
| `e1d15f8` | fix: fit SVG export into compact menu (ticket 69) |

## Rounds

| Round | Range | Verdict | Open at the end |
| --- | --- | --- | --- |
| 1 (whole task, post-hoc) | base..`6e3929b` | Needs fixes | C1 (2D particles were never serialised), I1 (element cap, user warning and size-risk note missing), I2 (required test cases missing), I3 (the 3D entry was not visibly disabled), I4 (the top bar was never wired), plus minors |
| 2 (69a fix 1) | `6e3929b`..`2bb4c5b` | Needs fixes | I1 partial (the cap warning only reached `log::warn!`, never the user), plus minors (warning styled as a failure, the viewport duplicated the seam, seam provenance missing, misattached doc comments, unused aliases, a truncatable no-truncation test) |
| 3 (69a fix 2) | `2bb4c5b`..`03a116b`,`b92fd5e` | Approved | two non-blocking minors (a wasm-unused variable, a dead toast arm) |
| 4 (69a follow-up) | `b92fd5e`..`bdd2a4b` | Approved (scoped) | none |
| 5 (69b fix 1) | `bdd2a4b`..`7f85459` | Needs fixes | I4 (the compact SVG control overlapped the GitHub and hamburger controls at 320, 360 and 390 point widths) and a minor (the screenshot toast rendered twice) |
| 6 (69b fix 2) | `7f85459`..`e1d15f8` | Approved | none |

## Findings and how they closed

- **C1 — 2D particles were missing from the SVG.** The serializer now emits
  the 2D particles as `<circle>` elements from `scene.dots`, with the colormap
  colour.
- **I1 — the element-cap warning never reached the user.** The warning is now
  surfaced through `Screenshot::notify`; on the web build one composed
  message carries both the download confirmation and the size warning, so the
  confirmation can no longer overwrite it. The native build warns only when
  the scene is over the cap, and the warning uses the warning style (the 3D
  rejection keeps the failure style).
- **I2 — missing test cases.** `tests/svg_export_test.rs` covers element
  presence, hex colours, the 3D rejection and the forced extension; the
  no-truncation test counts the arrowhead markers so a truncating serializer
  fails it.
- **I3 — the 3D entry is visibly disabled.** The menu and top-bar entries are
  greyed out in 3D through the shared `svg_export_enabled(mode_3d)` seam, and
  invoking a disabled entry still produces the explanatory toast.
- **I4 — the top bar was wired.** The top bar has an SVG export control; in
  the compact layout it is reachable through the hamburger menu and the
  compact controls no longer overlap or clip at 320, 360 and 390 point
  widths (regression test on the laid-out rectangles).

## Seams

Exactly two `#[doc(hidden)] pub` seams were allowed for this ticket (the
controller ruling of 2026-11-10): `svg_export_enabled(mode_3d)` and the pure
message-composition function. No further public API was added.

## Not verified here

The Windows build and the MSVC resource compiler are not exercised on Linux;
a `workflow_dispatch` dry run is required before any real tag.
