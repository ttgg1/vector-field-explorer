# Closing a review finding

How a finding from a task review is turned into a closed finding. Written
2026-11-10, after the repair rounds for tickets 69, 70 and 71; the sources
are the review and re-review reports under
`.superpowers/sdd/2026-11-05-vector-field-explorer/` (`task-69a-rereview*.md`,
`task-70-rereview*.md`) and the SDD ledger.

## Evidence, not assertion

A finding is closed when something demonstrates the change, not when the
implementer says it is done.

- On 2026-11-10, ticket 70's round-1 fix shipped a regression test that
  asserted a hand-written predicate instead of the production decision, so
  the test passed against the pre-fix code. The round-2 fix moved the
  decision into one pure function and made the test drive it
  (`fly_toggle_key`, `src/fly_camera.rs`, called from `src/app/input.rs`).
  The bar after that: the test must fail against the pre-fix code.
- A re-review that only reads the diff cannot catch a test that cannot fail.
  The re-review prompt asks for the failing-before evidence.

## One seam per genuine need

Public API widening for tests is a last resort, and it is budgeted.

- On 2026-11-10 the controller ruled that ticket 69a may carry exactly two
  `#[doc(hidden)] pub` seams — `svg_export_enabled(mode_3d)` and a pure
  message-composition function — each with the reason in the commit body and
  in the ledger. No further widening.
- If a seam would need a whole application context to be meaningful (ticket
  70's `frame_input` would have needed a `VfApp` and an `egui::Context`),
  the seam is the wrong shape: push the decision down into a pure function
  that the caller passes values into.

## Exercise the path the user sees

A finding about user-visible behaviour is not closed by calling the seam.

- On 2026-11-10, ticket 69a's element-cap warning reached `Screenshot::notify`
  and the seam was called on the web path, but the toast was painted by the
  top bar before the viewport and the download confirmation overwrote the
  warning in the same frame, so a web user never saw it. The fix composes
  one message carrying both facts.
- Warnings must not wear the failure styling (ticket 69a minor): a size
  warning styled as a failure reads as an error.
- A guard that says "not available" must look unavailable, not just refuse
  when invoked (ticket 69's remaining UI finding).

## Report the honest state

A re-review that returns "Approved" with minors open still merges; the
minor then goes into the next round rather than being quietly dropped. The
ledger line for a round states how many findings were addressed and how many
remain open.
