# Seat availability history — 2026-11-10

**Historical, not a model-selection table.** [Workflow](../../../references/workflow.md)
owns current seats and usage-limit STOP behavior. All fallback/round-escalation
instructions formerly repeated here are retired; Git preserves the original.

## Observed dispatch failures

| Model | 2026-11-10 evidence |
| --- | --- |
| `opencode-go/claude-haiku-5-5` | `Policy budget exceeded`, 3 dispatches |
| `opencode-go/deepseek-v4.1-flash` | same error, 2 dispatches |
| `opencode/claude-haiku-5-5` | same error |
| `opencode-go/beam-free` | ran, 4+ dispatches |

The console's built-in connection `conn_01M6XJG2D85V9V5M1MG7BTTX6S` reported
89 models / 47 enabled, including enabled/available Haiku and DeepSeek. Catalog
availability did not establish dispatch entitlement. Failed dispatches consumed
a seat slot and roughly a minute without doing work.

## Superseded policy sequence

Source: dated owner rulings in main-root
`.superpowers/sdd/2026-11-05-vector-field-explorer/archive/progress-through-2026-11-11.md`:

1. With paid dispatch blocked, Beam became the single working seat.
2. LongCat review / Beam implementation restored independent model diversity.
3. Sol controller/review / Luna implementation replaced that split; fixed
   low effort initially rose by fix-round number (original lines 4348–4357).
4. Commit `2d28e50` recorded the replacement: effort by task difficulty/review
   scope; the same finding twice changes to a fresh stronger seat, not automatic
   effort-by-round escalation.

Subsequent native-CAS ledger rulings require an owner-directed resume after a
usage limit. Old automatic free-seat fallbacks and blocked-model probing are
obsolete, not recovery instructions.
