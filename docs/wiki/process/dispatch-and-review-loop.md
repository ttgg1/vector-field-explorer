# Dispatch/review lessons — 2026-11-10

**Historical, not policy.** [Workflow](../../../references/workflow.md) is the
current authority; [fix loop](../../../references/fix-loop.md) is the procedure.
Earlier full policy copies here are retired (Git preserves their text).

## Standing integration authority

The owner first chose “push as new branch/create PR,” then “do merges on yourself
as well, after review.” The second ruling superseded the interpretation that
every merge needed another approval. Source: main-root
`.superpowers/sdd/2026-11-05-vector-field-explorer/archive/progress-through-2026-11-11.md`, 2026-11-10
owner rulings (original lines 4637–4654). Later native-CAS ledger rulings made
CI asynchronous; old instructions to wait for green remote runs are obsolete.

## Helper-path correction

The claim that `task-brief` / `review-package` were absent was false. The glob
missed the **version directory** beneath `git-superpowers-*`; the helpers were
installed. The actual missing input was
`docs/superpowers/plans/2026-11-05-vector-field-explorer.md`, already named by
the SDD `plan-path` marker. Creating that file restored workspace resolution.
A missing path is not evidence that a tool is absent; check the installation
and inputs before inventing a substitute.

## Why independent review matters

Tickets 69/70/71 went straight to main and all failed post-hoc review despite
passing gates. Shared-file ownership also required splitting 69 into serializer
and UI work. See [repair evidence](../decisions/2026-11-10-repairing-unreviewed-tickets.md)
and [round-1 findings](../reviews/tickets-69-71-round-1.md).
