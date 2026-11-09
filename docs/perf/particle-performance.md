# Particle and rendering performance

Research record for ticket 35 (2026-11-09). Measurements were taken at base
`a7d295a` on the release build; the branch was then rebased onto `d68cfc5`
(ticket 21's test-suite layout). `src/` is identical between those commits, so
the release-binary measurements describe the same code.

**Provenance.** The full report is
`.superpowers/sdd/2026-11-05-vector-field-explorer/task-35-report.md` (local,
untracked SDD workspace). The tracked reproduction set is
`docs/perf/bench-35/`: the std-only CPU harness is
`docs/perf/bench-35/bench-35.rs`, and the instrumentation patch, runner,
analyzer, derived summary, and CPU microbenchmark log are tracked alongside it.
The app frame/upload probes and the GPU microbench were temporary and were
reverted from production. The tracked runner reproduces the app scenes and the
GPU runs; the GPU block sets `VFE_BENCH_GPU=1` (required for the microbench)
and runs after frame 60. App-level and GPU numbers can be re-run from the
tracked patch plus the runner on the same hardware, but they are **not**
reproducible from the shipped code. Exact values depend on the same
hardware/driver. The raw per-scene logs are **local/untracked** (about 10 MB in
the SDD workspace); only `summary.txt` is tracked. Nothing in the tables was
reconstructed from memory.

## Hardware and caveats

- AMD Ryzen 9 7950X (16C/32T, 30 GB), AMD Radeon RX 9070 XT (Navi 48,
  **discrete**), Mesa 26.2.6 Vulkan on Wayland, 179.92 Hz display.
- Native window pinned to 1280×800; paintable viewport 1196×682–705.
- The v0.1.0 floor is an **integrated** GPU at 1280×800. This machine exposes
  no iGPU, so the integrated floor is **unverified, not claimed**. All numbers
  come from this 7950X + discrete RX 9070 XT machine; absolute times and the CPU
  split are not established for other hardware.
- The 179.92 Hz refresh caps light scenes at ~5.55 ms (180 fps). A 5.55 ms
  result means “at the refresh cap with headroom”, not “GPU limit”. The
  64 000-arrow row's 184.4 fps is **not** explained by that cap (see below).
- Real-browser WebGPU timing is out of scope until ticket 16 deploys. The wasm
  release build is 12 211 828 bytes (11.6 MiB). The local/untracked test-web
  log records 17.14 s for `cargo check` and 27.38 s for Trunk release; the
  50.0 s shell-time total was not retained.

## Scenes

All 3D presets use `DEFAULT_DENSITY_3D = 16` (defined at
`src/viewmodel.rs:30`, applied at `:290`); “densest shipped preset” here means
that default 3D grid (4096 arrows), not a special preset. `src/presets.rs` only
lists the formula set.

| Scene | Settings |
| --- | --- |
| default | 2D `(x, y)` Pump, density 24 (576 arrows), particles off |
| 3D default grid (“densest shipped preset”) | 3D `(x, y, z)` radial, density 16 (4096 arrows), particles off |
| Magnetic dipole | 3D magnetic dipole, same default grid (4096 arrows), particles off |
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

`fps` is `1000 / p50_ms` from the unrounded interval; the printed ms is rounded
to two decimals, so 180.0 fps corresponds to ~5.556 ms (1000/5.55 = 180.2).
The 64 000-arrow row's 5.42 ms p50 (184.4 fps) is slightly **above** the nominal
179.92 Hz interval and is not explained by the refresh cap; treat it as a
scheduling/measurement artifact, not a capability claim. Its measured component
costs (3.20 ms CPU viewport + 0.28 ms upload + 0.45 ms GPU) are still far below
the frame budget.

Worst-case 37.07 ms splits as: advect 22.39, dot build/colour 5.68, trail
build 5.32 (live app probe, median ≈31 800 segments over the steady window,
range ≈30 400–33 600), trail clone+sort 1.89,
combined build+sort 0.31, buffer upload 0.18, GPU draw 0.12, residual
egui/submit ~1.15 ms after subtracting the GPU (~1.27 ms before it). The
synthetic CPU harness reports 5.86 ms for the same 4000×32 setting at 54 450
segments; the live GPU sample reached 30 855 segments because of fill/respawn
dynamics, and its cost is quoted below. The GPU microbenchmark (offscreen, no
vsync) costs 0.012 ms for the default 2D scene, 0.065 ms for 4096 arrows,
0.445 ms for 64 000 arrows, and 0.122 ms for the full worst-case scene at
1196×682.

## Bottleneck ranking

1. **CPU field evaluation inside advection — 22.4 ms (60%).** Four
   `FieldExpr::eval3` calls per particle; at 4000 that is 16 000 evaluations.
2. **CPU dot instance colour — 5.7 ms (15%).** One additional `eval3` per
   particle for Magnitude colour. Speed colour costs 0.09 ms, proving the whole
   cost is the evaluator.
3. **CPU trail instance build — 5.3 ms (14%).** Also one `eval3` per particle,
   plus segment emission (synthetic steady-state: 54 450 segments at 4000/32;
   live sample: 30 855 due to fill/respawn).
4. **Trail clone + depth sort — 1.9 ms (5%).** The largest non-evaluator cost.
5. **Buffer upload — 0.18 ms (0.5%).** 2–3 storage buffers + bind groups and
   ~1.9 MB copied per frame; not a floor risk.
6. **GPU draw — 0.12 ms (0.3%).** The GPU is idle waiting for the CPU.
7. **No per-frame stall or readback.** Screenshot readback is user-triggered.

The root cause is not RK4 arithmetic: `FieldExpr::eval_component` clones the
parsed `meval::Expr` and re-binds it on every component evaluation, and
`meval` allocates a fresh RPN stack `Vec` per call. Microbenchmark (release,
median of 21):

| Evaluator / basis | ns per `eval3` | Heap allocations per 4000 `eval3` calls |
| --- | ---: | --- |
| current `eval3`, 3D radial | 1416 | ~396 000 |
| bound-once `meval` closures | 31 | 12 000 (3 per call, one stack `Vec` per component; ~33× fewer than current) |
| hand-written closure (ideal) | 0.6 | 0 by construction |
| 4000-particle advect total (16 000 `eval3` calls) | — | 1 584 000 (83.6 MB) |

At 4000 particles the full CPU frame path performs ~2.36 M heap allocations
and allocates ~137 MB per frame. That total is from a separate full-frame run,
not the sum of the per-call rows above (the −381 difference is unexplained
run-to-run variance). `sample_field` costs 1.38 µs/sample: 5.6 ms at
16³, 88 ms at 40³, and 55 ms at 200² — a 55–88 ms edit stall at the maximum
density.

## Recommendation

**No architectural rewrite for v0.1.0.** On this **discrete** GPU the default
and 3D default-grid scenes hold the 180 Hz cap (5.55/5.56 ms p50) with ≤0.15 ms
CPU and ≤0.065 ms GPU. That is not a floor-compliance claim: the stated floor is
an integrated GPU, which this machine cannot expose, so integrated compliance is
unverified. The 4000-particle configurations are below 30 fps at p50 for trail
8 (28.8 fps) and trail 32 (27.0 fps); trail 0 is 36.0 fps p50 but 29.7 fps p99.
None is a shipped preset, and the 30 fps floor is defined on the densest preset.
These configurations are CPU-bound in evaluator re-binding.

The smallest change that fixes them is to **cache bound field evaluators in
`FieldExpr`** (bind each component once at parse time; internal only, no API
change, same `meval` semantics). The microbenchmark shows a 45.7× evaluator
speedup. Component arithmetic from the measured parts gives ~0.5 ms advect +
~0.12 ms dot + ~0.12 ms trail eval + ~0.2 ms trail build (segment emission) +
~1.9 ms trail clone/sort + 0.3 ms combined + 0.2 ms upload + ~1.2 ms residual
+ 0.1 ms GPU ≈ 4.6 ms. Allowing ~2 ms for unmeasured segment emission and
allocator/scheduler variance, the conservative bound is **≤7 ms (≥140 fps)**,
well above the 30 fps floor. On this display it would likely sit at the refresh
cap, but that is a projection, not a measurement. The earlier 8–11 ms
figure is withdrawn; it did not follow from the component sum. The 55–88 ms
rebuild stall drops to ~1–2 ms, and worst-case allocation churn falls by ~30×.

This is optional, not a v0.1.0 blocker: no shipped preset/default configuration
fails on this discrete GPU, and the integrated floor is unverified rather than
failed. If ticket 19's scope is tight, ship without it and document 4000
particles + 32 trails as the top of the interactive range.

**Post-v1:** bake a 2D/3D velocity grid on formula/domain change (cheap after
the evaluator cache, and independent of the frame loop), then add GPU compute
ping-pong advection. That is the real answer for time-dependent fields and very
large clouds; write an ADR first. Reuse instance buffers and optimize the trail
sort/LOD as small follow-ups, not floor work.

Do not implement GPU compute advection, a flow-field texture, or a dot-pipeline
split for v0.1.0.

## Follow-up ticket proposals (proposals, not filed)

1. **P1 — Cache bound field evaluators in `FieldExpr`.** Optional before 19;
   owner decision. Not a v0.1.0 blocker. Internal change, value-table TDD plus an
   allocation-count test in its own test binary. Evidence: 1416 → 31 ns per
   `eval3`.
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
