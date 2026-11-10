# Native CAS scope and sequencing — 2026-11-10

The owner selected exmex 0.21.0 with `partial`, deferred CAS wasm support, and
approved a native CAS panel with scalar/vector input, operation selection,
result preview/copy, and explicit Apply for vector results only. Failed
operations preserve the plotted field; application is never automatic and
history is not persisted. Differentiation covers x/y/z/t. In 2D, gradient has
x/y components, divergence is scalar, and curl is `dFy/dx - dFx/dy`; 3D curl
is a vector. Unsupported symbolic functions fail clearly; the existing meval
evaluator remains unchanged.

The resulting just-in-time ticket proposal is split into adapter/math (73) and
native panel/integration (75). CAS math may run alongside Phase 1 source math,
but the adapter is not a Phase 1 prerequisite; it must land before Phase 3
induction. `src/lib.rs` and `tests/suite.rs` registrations overlap and require
exclusive ownership or serial edits. The panel follows ticket 71's merge to
avoid UI file contention. Ticket 73 remains blocked pending independent
builder rereview of the exact symbolic semantics; ticket 75 depends on 73 and
71. CAS wasm support and its size/runtime acceptance are deferred, not
silently accepted.

Source: owner rulings in the final dated section of
`.superpowers/sdd/2026-11-05-vector-field-explorer/progress.md`; research
baseline `703e77e9cc983aaa4124b41b5ec0603d7e745edf`; design
`docs/superpowers/specs/2026-11-10-native-cas-design.md` and plan
`docs/superpowers/plans/2026-11-10-native-cas.md`.
