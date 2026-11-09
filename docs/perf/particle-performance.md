# Particle and rendering performance

Research record for ticket 35 (2026-11-09), measured at base `a7d295a` on the
release build. The full report and the std-only CPU benchmark harness live in
the SDD workspace: `.superpowers/sdd/2026-11-05-vector-field-explorer/task-35-report.md`
and `bench-35.rs`. This page is the durable summary; commands and raw logs are
in the report.

## Hardware and caveats

- AMD Ryzen 9 7950X (16C/32T, 30 GB), AMD Radeon RX 9070 XT (Navi 48,
  **discrete**), Mesa 26.2.6 Vulkan on Wayland, 179.92 Hz display.
- Native window pinned to 1280×800; paintable viewport 1196×682–705.
- The v0.1.0 floor is an **integrated** GPU at 1280×800. This machine exposes
  no iGPU, so the integrated floor is **unverified, not claimed**. Absolute
  times and the CPU split transfer; the GPU split is a lower bound.
- The 179.92 Hz refresh caps light scenes at ~5.55 ms (180 fps). A 5.55 ms
  result means “at the refresh cap with headroom”, not “GPU limit”.
- Real-browser WebGPU timing is out of scope until ticket 16 deploys. The wasm
  release build is 12 211 828 bytes (11.6 MiB); `scripts/test-web.sh` passed in
  50.0 s wall (17.1 s check + 27.4 s Trunk release).

## Scenes

| Scene | Settings |
| --- | --- |
| default | 2D `(x, y)` Pump, density 24 (576 arrows), particles off |
| densest shipped preset | 3D `(x, y, z)` radial, density 16 (4096 arrows), particles off |
| worst case | 3D radial, 4096 arrows, 4000 particles, 32 trail points, Magnitude colour |
| grid stress | 3D radial, density 40 (64 000 arrows), particles off |

## Headline numbers (25 s runs, 2 s warm-up discarded; p50/p95/p99)

| Scene | p50 ms | p95 ms | p99 ms | p50 fps |
| --- | ---: | ---: | ---: | ---: |
| default 2D Pump | 5.55 | 5.74 | 5.87 | 180.0 |
| densest shipped preset (4096 arrows) | 5.56 | 5.68 | 5.90 | 179.9 |
| Magnetic dipole, density 16 | 5.56 | 5.64 | 5.73 | 179.9 |
| grid stress (64 000 arrows) | 5.42 | 6.40 | 6.52 | 184.4 |
| worst, trail 0 | 27.80 | 32.56 | 33.63 | 36.0 |
| worst, trail 8 | 34.71 | 38.27 | 39.84 | 28.8 |
| worst, trail 32 | 37.07 | 39.29 | 40.87 | 27.0 |
| worst, trail 32, arrows off | 37.53 | 40.06 | 42.71 | 26.6 |
| worst, particles off | 5.56 | 5.75 | 5.90 | 180.0 |

Worst-case 37.07 ms splits as: advect 22.39, dot build/colour 5.68, trail
build 5.32, trail clone+sort 1.89, combined build+sort 0.31, buffer upload
0.18, GPU draw 0.12, residual egui/submit ~1.3 ms. The GPU microbenchmark
(offscreen, no vsync) costs 0.012 ms for the default 2D scene, 0.065 ms for
4096 arrows, 0.445 ms for 64 000 arrows, and 0.122 ms for the full worst-case
scene at 1196×682.

## Bottleneck ranking

1. **CPU field evaluation inside advection — 22.4 ms (60%).** Four
   `FieldExpr::eval3` calls per particle; at 4000 that is 16 000 evaluations.
2. **CPU dot instance colour — 5.7 ms (15%).** One additional `eval3` per
   particle for Magnitude colour. Speed colour costs 0.09 ms, proving the whole
   cost is the evaluator.
3. **CPU trail instance build — 5.3 ms (14%).** Also one `eval3` per particle,
   plus segment emission (54 450 segments at 4000/32).
4. **Trail clone + depth sort — 1.9 ms (5%).** The largest non-evaluator cost.
5. **Buffer upload — 0.18 ms (0.5%).** 2–3 storage buffers + bind groups and
   ~1.9 MB copied per frame; not a floor risk.
6. **GPU draw — 0.12 ms (0.3%).** The GPU is idle waiting for the CPU.
7. **No per-frame stall or readback.** Screenshot readback is user-triggered.

The root cause is not RK4 arithmetic: `FieldExpr::eval_component` clones the
parsed `meval::Expr` and re-binds it on every component evaluation, and
`meval` allocates a fresh RPN stack `Vec` per call. Microbenchmark (release,
median of 21):

| Evaluator | ns per `eval3` | Allocs per 4000 calls |
| --- | ---: | ---: |
| current `eval3`, 3D radial | 1416 | ~1 584 000 |
| bound-once `meval` closures | 31 | 12 000 |
| hand-written closure (ideal) | 0.6 | 0 |

At 4000 particles the full CPU frame path performs ~2.36 M heap allocations
and allocates ~137 MB per frame. `sample_field` costs 1.38 µs/sample: 5.6 ms at
16³, 88 ms at 40³, and 55 ms at 200² — a 55–88 ms edit stall at the maximum
density.

## Recommendation

**No architectural rewrite for v0.1.0.** The default and densest shipped
presets already meet the floors on this hardware at the refresh cap, with
≤0.15 ms CPU and ≤0.065 ms GPU. The only measured below-floor setting is the
optional 4000-particle + 32-trail configuration (27 fps p50, 24.5 fps p99),
and it is CPU-bound in evaluator re-binding.

The smallest change that fixes it is to **cache bound field evaluators in
`FieldExpr`** (bind each component once at parse time; internal only, no API
change, same `meval` semantics). The microbenchmark shows a 45.7× evaluator
speedup; projected worst frame ~8–11 ms (90–125 fps), and the 55–88 ms rebuild
stall drops to ~1–2 ms. It also cuts worst-case allocation churn by ~30×.

This is a recommendation, not a v0.1.0 blocker: no shipped preset/default
configuration fails its floor here, and the integrated floor is unverified
rather than failed. If ticket 19's scope is tight, ship without it and document
4000 particles + 32 trails as the top of the interactive range.

**Post-v1:** bake a 2D/3D velocity grid on formula/domain change (cheap after
the evaluator cache, and independent of the frame loop), then add GPU compute
ping-pong advection. That is the real answer for time-dependent fields and very
large clouds; write an ADR first. Reuse instance buffers and optimize the trail
sort/LOD as small follow-ups, not floor work.

Do not implement GPU compute advection, a flow-field texture, or a dot-pipeline
split for v0.1.0.

## Follow-up ticket proposals

1. **P1 — Cache bound field evaluators in `FieldExpr`.** Recommended before 19;
   internal change, value-table TDD plus an allocation-count test in its own
   test binary. Evidence: 1416 → 31 ns per `eval3`.
2. **P2 — Reuse per-frame instance buffers / dirty upload.** Persistent
   buffers, stable bind groups, optional `Vec` reuse; 0.18 ms and ~1.9 MB/frame
   at the worst case. No visual change.
3. **P3 — Flow-field bake (phase 1), then GPU advection (phase 2, post-v1).**
   Trilinear lookup replaces `meval` in the frame loop; arrows/streamlines stay
   on exact eval; needs an ADR and a browser/compute validation plan.
4. **P4 — Trail build/sort reduction.** Reuse, depth keys/radix sort, optional
   age LOD/segment cap; ~7.2 ms at 4000/32 today, sort becomes dominant after
   P1.
5. **P5 — Browser performance validation harness.** After ticket 16, collect
   p50/p95/p99 for the three scenes in Firefox/Chromium WebGPU; the web 30 fps
   floor is currently unverified.
6. **P6 — Integrated-GPU floor validation.** Run the same scenes on an iGPU at
   1280×800; this machine cannot expose one. Ticket 19 must not claim
   integrated compliance without it.
