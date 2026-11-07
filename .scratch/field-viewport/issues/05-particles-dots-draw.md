# 05: Particle system (dots draw path)

**What to build:** From the user's perspective: the sidebar's particles feature ON animates particles that advect along the field and respawn when they leave the domain — they are drawn through the dots instance pipeline, so the scene now visibly paints all three primitive kinds (arrows + lines + dots). Advection and respawn are pure state stepping, unit-tested; animation advances once per frame behind the seam.

**Blocked by:** 03 (feature checkbox + speed slider must exist).

**Status:** IMPLEMENTER DONE (salvage, sessions #1–#5 + finisher #6) — commit `267dca1` `feat: advected particle animation`, BASE 18307e6. Gates at commit: `cargo test --all-targets` 99/0, clippy `-D warnings` clean. Task review dispatched (opencode/flint-40b-free, ses_e47ac938cffePqGku83Q1jzX6z, package review-18307e6..267dca1.diff) — verdict UNREACHABLE from fresh sessions (not a child of this session); close out on final review / re-dispatch if needed.

- [ ] Pure advection step (position, velocity, lifetime/respawn) RED→GREEN watched: particles follow a constant field in straight lines; speed slider scales the step
- [ ] Dots pipeline genuinely draws the particles — this closes deferred review minor (d) "paint() dots-count needs real dots draw"; ledger that minor as RESOLVED with this ticket's commit
- [ ] Snapshot rebuild budget unchanged: particles are per-frame state, arrows/lines stay dirty-flag cached (note any cost concern in the report)
- [ ] Gates: build / clippy `-D warnings` / full `cargo test` green; commit `feat: particle system with dots draw path`

**Dispatch notes:** brief = plan task 10 via `scripts/task-brief … 10`. 04 and 05 can run in parallel once 03 is done — do not share files, keep commits independent.
