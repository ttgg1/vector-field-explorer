# 09: README + cleanup + final whole-branch review

**What to build:** From the user's perspective: the repo README explains building, running, and every visible control; dead code/copy-paste cruft from the task history is swept; then the whole branch (T1 → last ticket) gets one broad review pass whose minors are triaged to fix-or-defer rulings in the ledger.

**Blocked by:** 02, 03, 04, 05, 06, 07, 08 (all feature tickets must land first — this is the integration point).

**Status:** ready-for-agent

- [ ] README accurate against the built app (build/run instructions, controls, known limitations)
- [ ] Cleanup sweep keeps `cargo clippy --all-targets -- -D warnings` clean
- [ ] Final whole-branch review dispatched via `review-package <PLAN> <T1_BASE> HEAD`; findings triaged: minors from issue 01's "Related minors" (a DrawState naming, b inv_view_proj, c sort duplicate, e per-frame buffers — d was resolved by ticket 05) + anything observed along the tickets
- [ ] Review fix commits (if any) get a scoped re-review per fix-loop reference
- [ ] Ledger closes the plan: per-task commit ranges + final review verdict

**Dispatch notes:** brief = plan task 14 via `scripts/task-brief … 14`. Review seat on the mandated model (policy: strata/coder-iq1_m, bump variant if needed).
