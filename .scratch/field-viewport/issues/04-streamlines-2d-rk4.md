# 04: Streamlines in the 2D view (RK4)

**What to build:** From the user's perspective: flipping the sidebar's streamlines feature ON draws flow-field streamlines over the 2D arrow field — seeds on a jittered grid, traced with RK4 integration of field advection, rendered as line strips through the lines instance pipeline; the speed slider changes the step budget so lines read at different lengths. The RK4 stepper and seed placement are pure functions with real unit tests; GPU paint stays behind the seam, verified by compile gates only (plan ruling).

**Blocked by:** 03 (the feature checkbox and speed slider must exist).

**Status:** ready-for-agent

- [ ] Pure RK4 stepper, RED→GREEN watched: constant field ⇒ straight streamline; zero-magnitude field ⇒ no advance (no NaN/inf path)
- [ ] Jittered seed grid with seeded/deterministic RNG — a test asserts reproducible seeds
- [ ] Snapshot builder emits line instances when the feature is on; instance count visible in status bar
- [ ] Lines paint path in the GPU seam lights up (no unit test — build + clippy + suite green is the contract; visual check deferred to the user)
- [ ] Gates: build / clippy `-D warnings` / full `cargo test` green; commit `feat: streamlines 2D RK4`
- [ ] Deferred minor (b) unused `inv_view_proj`: not this ticket's gate; final review triages it

**Dispatch notes:** brief = plan task 9 via `scripts/task-brief docs/superpowers/plans/2026-11-05-vector-field-explorer.md 9`.
