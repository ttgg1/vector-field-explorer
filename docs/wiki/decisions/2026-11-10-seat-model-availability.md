# Seat model availability — 2026-11-10

## Update (later on 2026-11-10) — reviewer/implementer split

This update supersedes the earlier single-seat rule and conditional preference
described below.

Superseding the "use beam-free for everything" guidance below, the user ruled
that **reviewers run on `opencode-go/longcat-2.5-free`** (effort matched to
the scope of the review: a scoped re-review is less effort than a whole-branch
review) and **implementers run on `opencode-go/beam-free`** at `#high`, a
higher variant (`#xhigh`, `#max`) for the bigger tasks. Reasoning: the free
seats are the only ones that dispatch here, and giving the review a different
model from the one that wrote the code is worth more than a stronger single
model.

## The claim (first ruling on 2026-11-10; superseded)

On 2026-11-10, in this workspace, `opencode-go/beam-free` is the **only**
subagent seat model that dispatches successfully. Every paid seat fails
immediately with `Policy budget exceeded`.

## Evidence

Confirmed failing at dispatch on 2026-11-10, each with an error rather than a
timeout:

| Seat model | Result |
| --- | --- |
| `opencode-go/claude-haiku-5-5` | `Policy budget exceeded` (3 dispatches) |
| `opencode-go/deepseek-v4.1-flash` | `Policy budget exceeded` (2 dispatches) |
| `opencode/claude-haiku-5-5` (built-in provider) | `Policy budget exceeded` |
| `opencode-go/beam-free` | runs (4+ dispatches) |

`opencode_console.list_models` reported the built-in provider
`conn_01M6XJG2D85V9V5M1MG7BTTX6S` with 89 models / 47 enabled, including
`claude-haiku-5-5` and `deepseek-v4.1-flash` at `enabled=true`,
`available=true`. Availability in the console therefore says nothing about
whether a seat can actually dispatch — the block is at dispatch time.

## Why this matters

`AGENTS.md` named `opencode-go/deepseek-v4.1-flash` for coding seats and
`opencode-go/claude-haiku-5-5` for review seats, with `opencode-go/beam-free`
only as a usage-limit fallback. Under the real budget state, that table makes
every dispatch a coin flip that costs a full minute to discover.

## The rule (as of 2026-11-10)

- Use `opencode-go/beam-free#high` for coding, review and re-review seats.
- Do not re-try a seat that failed with `Policy budget exceeded`; re-scope or
  split the task instead of escalating to another blocked model.
- If a paid seat becomes available again, `opencode-go/deepseek-v4.1-flash`
  (`#high`) is the preferred coding seat and `opencode-go/claude-haiku-5-5#high`
  the preferred review seat. At that time, `AGENTS.md` carried this as a conditional note.

## Update (latest ruling in `AGENTS.md`, 2026-11-10)

The current `AGENTS.md` Models section assigns controller and reviewers to
`openai/gpt-6.1-sol#medium`, and implementers to `openai/gpt-6-luna#low`.
Escalate effort one variant at the second fix round and at every later round.
This supersedes the free-seat assignments and conditional preference above;
see `AGENTS.md` (2026-11-10).

## Cost of a blocked seat

Each failed dispatch burns a seat slot and about a minute of wall clock with no
work done. When dispatching several seats at once, prefer one seat whose model
is already known to work, and probe a new model with a single seat rather than
three.

## Related

- [Dispatch, review and fix loop](../process/dispatch-and-review-loop.md)
- [Next frontier](2026-11-10-next-frontier.md)
