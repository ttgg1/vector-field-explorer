# Seat model availability — 2026-11-10

## Update (latest on 2026-11-10) — effort follows task difficulty

This is the authoritative current policy. It supersedes the dated
round-escalation decision retained below as historical context.

The user ruled that the **implementer seat's effort variant is chosen by the
difficulty of the task**, not by the review round: `openai/gpt-6-luna#low`
for a narrow mechanical change (a fix round with a clear file list, a
documentation edit, a small well-specified slice), `#medium` for ordinary
feature work, `#high` for a wide or subtle task. The **review seat's
variant is chosen by the scope of the review** — a scoped re-review of a few
findings is lighter than a whole-task review.

A repeated finding no longer raises the effort automatically. What it changes
is the *seat*: if the same finding comes back twice, resume a different,
stronger implementer rather than repeating the same seat at the same effort.

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

## Superseded decision (2026-11-10) — fixed seats and round-based escalation

The user ruling recorded in `progress.md:4348-4357` assigned the controller
and reviewers to `openai/gpt-6.1-sol#medium`, and implementers to
`openai/gpt-6-luna#low`. At the second fix round, step the variant up one
level (low → medium → high), and step up again at every further round. A
round that returns the same finding signals escalation rather than repeating
the same seat at the same effort. The older free models
(`opencode-go/beam-free`, `opencode-go/longcat-2.5-free`) were fallbacks when
an `openai` seat was unavailable. This was the then-current policy; the later
user ruling below supersedes its round-based effort rule.

## Current seat policy — latest user ruling, 2026-11-10

The named user ruling in the final dated section of `progress.md` (2026-11-10;
policy recorded in commit `2d28e50`) supersedes the round-based effort rule
above. The controller uses `openai/gpt-6.1-sol#medium`;
implementers use `openai/gpt-6-luna` with effort selected by task difficulty
(`#low` for narrow/mechanical work, `#medium` for ordinary work, `#high` for
wide or subtle work); reviewers use `openai/gpt-6.1-sol` with effort selected
by review scope. A repeated finding twice changes the implementer seat to a
different, stronger seat; it does not automatically raise effort by round.
Older free models (`opencode-go/beam-free`, `opencode-go/longcat-2.5-free`)
are fallbacks when an OpenAI seat is unavailable. See `AGENTS.md` for the
active operational policy.

## Cost of a blocked seat

Each failed dispatch burns a seat slot and about a minute of wall clock with no
work done. When dispatching several seats at once, prefer one seat whose model
is already known to work, and probe a new model with a single seat rather than
three.

## Related

- [Dispatch, review and fix loop](../process/dispatch-and-review-loop.md)
- [Next frontier](2026-11-10-next-frontier.md)
