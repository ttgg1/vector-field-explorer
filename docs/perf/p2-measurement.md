# P2 measurement: per-frame buffer assembly and dirty uploads (2026-11-21)

P2 of [#18](https://github.com/ttgg1/vector-field-explorer/issues/18) said
"measure before more": object instances are already generation-gated (#112)
and the object pass reuses one colour+depth target, so the open question was
whether the remaining per-frame vector-scene assembly (arrow, line, dot and
trail buffers) is worth a reuse refactor. This record answers that question
from a current-main release run; it changes no code, floor or threshold.

Issue: [#247](https://github.com/ttgg1/vector-field-explorer/issues/247).
Base: `32df6c2` (P1 #240 and P4 #245 merged). No production `src/` change.

## Method

The maintained harness (`tests/perf_baseline_test.rs`) gained two bounded
`Extra` modes; the existing `snapshot` and `sample` rows are unchanged.

- `densest-3d` (`Extra::ArrowAssembly`): the shipped 3D floor scene, no
  particles.
- `worst-particles` (`Extra::ParticleAssembly`): 4000 particles, trail length
  32, 3D. The particle cloud is preadvanced one second at the shipped
  `0.01 s` fixed step (`benchmark_particle_cloud`, mirroring
  `cli_scene::particle_cloud`) so trails are at their steady length.

Every input is built outside the clock; each timed iteration repeats only the
named work. The metrics are:

| Row | What it times | When the live app pays it |
| --- | --- | --- |
| `sample` | `State::sample()` (existing row) | dirty rebuild / drag |
| `snapshot` | `State::snapshot()` (existing row) | dirty rebuild |
| `advect` | `particles::advect` one fixed step (per-call cost is dt-independent: one RK4 step per particle) | every vector frame |
| `dot_build` | `dot_instances_styled_at` over the real cloud | every vector frame with particles |
| `trail_build` | `trail_instances_styled_at` over the real trail history | every vector frame with particles |
| `trail_sort` | `sorted_lines_back_to_front` over freshly built trails | every assembled frame |
| `combined_assemble` | `combined_instances(arrows, dots)` + `sort_scene_back_to_front` | every 3D frame |
| `frame_assembly` | the app's `assemble_vector_scene` combined/arrow/trail branches (harness mirror) | every rendered frame |
| `compose_lines` | `compose_lines_with_frame` over the cached flow lines | dirty rebuild only |

`frame_assembly` mirrors `src/app/mod.rs::assemble_vector_scene`; a unit test
pins the 3D combined kind tags and ordering, the 2D empty-combined/sorted-arrow
branch, and that the cached inputs are never mutated. `compose_lines` is
reported but excluded from the steady-state frame because the app caches the
composed lines until the state is dirty.

`sample` is now a P1 cache hit for the time-invariant default formula
(repeated identical passes reuse the previous result, #240), so it is the
sampling cost of a rebuild that lands on an unchanged field, not a cost paid on
every playing frame. The live playing path does not re-run `sample()`; the
particle builds below do their own per-particle evaluation. A cold sampling
miss is a dirty-frame event and is not part of these frame numbers.

## Release numbers

Host line from the run:
`host=AMD_Ryzen_9_7950X_16-Core_Processor gpu=AMD_Radeon_RX_9070_XT_(RADV_GFX1201) backend=Vulkan build=release date=2026-11-21`.

`n = 30` timed iterations after 5 discarded warm-up iterations, per metric.
Percent shares use a 16.67 ms (16670 µs) frame budget and the measured
present-path p50 **5.553 ms** (`baselines/2026-11-21-present-rx9070xt.txt`).
The present-path record covers particle-free `default-2d` and `densest-3d` at
the 179.92 Hz vsync cap; the `worst-particles` ratios are a scale reference,
not a like-for-like present measurement of that stress scene.

### `densest-3d`

| Row | p50 µs | p95 µs | % of 16.67 ms | % of 5.553 ms |
| --- | ---: | ---: | ---: | ---: |
| `sample` (cache hit) | 1.640 | 1.655 | 0.010% | 0.030% |
| `snapshot` (dirty rebuild) | 15.060 | 15.680 | 0.090% | 0.271% |
| cached-sample geometry (`snapshot` − `sample`) | ≈13.42 | ≈14.03 | 0.081% | 0.242% |
| `combined_assemble` | 53.005 | 57.088 | 0.318% | 0.954% |
| `frame_assembly` | 52.864 | 53.265 | 0.317% | 0.952% |
| `compose_lines` (dirty rebuild) | 0.260 | 0.506 | 0.002% | 0.005% |

The whole per-frame vector-scene assembly for the shipped 3D floor scene is
**52.9 µs p50 — 0.32% of the frame budget, ~0.95% of the present-path p50**.
The only per-frame buffer rebuilt from unchanged content is the combined arrow
stream, and it is 0.32% of the budget.

### `worst-particles`

| Row | p50 µs | p95 µs | % of 16.67 ms | % of 5.553 ms |
| --- | ---: | ---: | ---: | ---: |
| `sample` (cache hit) | 1.670 | 1.690 | 0.010% | 0.030% |
| `snapshot` (dirty rebuild) | 15.095 | 15.191 | 0.091% | 0.272% |
| cached-sample geometry (`snapshot` − `sample`) | ≈13.43 | ≈13.50 | 0.081% | 0.242% |
| `advect` | 21269.671 | 21494.485 | 127.593% | 383.03% |
| `dot_build` | 5226.799 | 5235.254 | 31.355% | 94.13% |
| `trail_build` | 5357.334 | 5370.376 | 32.138% | 96.48% |
| `trail_sort` | 1364.220 | 1378.284 | 8.184% | 24.57% |
| `combined_assemble` | 110.639 | 115.124 | 0.664% | 1.99% |
| `frame_assembly` | 1518.514 | 1527.344 | 9.109% | 27.35% |
| `compose_lines` (dirty rebuild) | 0.310 | 1.010 | 0.002% | 0.006% |

Derived p50 sums (not a distribution; percentiles cannot be added):

- **Steady-state frame**: `advect + dot_build + trail_build + frame_assembly`
  = **33.372 ms** = **200.2%** of the 16.67 ms budget, **601%** of the 5.553 ms
  present p50. On this stress scene the CPU is the limit; the present-path
  vsync cap says nothing about it.
- **Vector-scene assembly** (`dot_build + trail_build + frame_assembly`, with
  `compose_lines` excluded because it is cached): **12.103 ms** = **72.6%** of
  the budget, **218%** of the present p50.
- Of that 12.103 ms, **10.584 ms (87.5%) is the two instance builds**
  (`dot_build` + `trail_build`); `frame_assembly` (combine + painter sorts +
  cached arrow share) is 1.519 ms, and the trail sort inside it is 1.364 ms.

## Decision: do not refactor the buffers

**No buffer reuse or persistent-upload follow-up is promoted.** The evidence:

1. **The dominant assembly cost is per-particle field evaluation, not buffer
   allocation.** `dot_instances_styled_at` and `trail_instances_styled_at` each
   call `particle_rgb` once per particle, which evaluates the field. Cost
   consistency with `advect` (≈5.3 µs/particle for four evaluations) suggests
   evaluator dominance, but the build rows also include the instance fill and
   are not separately isolated here. Either way, a persistent allocation,
   stable bind group or `Vec` reuse cannot skip that evaluation or the fill
   while the content changes.
2. **Every playing frame changes the content.** Advection moves the particles
   and appends/trims trail points, and the painter depth changes with the
   camera. There is no measured steady-state buffer that is rebuilt with
   unchanged content and carries a material cost. The one unchanged-content
   rebuild — the `densest-3d` combined arrow stream — is 53 µs (0.32% of the
   budget). The brief's promote condition (a dominant buffer rebuilt with
   unchanged content in the measured frame) therefore does not fire.
3. **The shipped 3D floor scene needs no help.** Its complete per-frame vector
   assembly is 53 µs p50 (0.32% of the budget). The present-path floor is
   already met at the vsync cap; this measurement does not change that claim.
4. **P4 already addressed the largest removable sort cost.** The trail sort is
   1.364 ms p50 (down from 3.23 ms before #245); the remaining sort is
   order-visible work on changed depths, not reusable geometry.
5. **This matches the disposition's condition.** P2 was to be promoted only if
   P1's measurements showed "a geometry-build cost that reuse would remove".
   The measured build cost is evaluator-bound and the buffers change each
   frame, so reuse would not remove it.

## Residual risks and unmeasured costs

- **Buffer upload and allocation churn are not separately observable here.**
  `DrawState::upload` is called inside the paint callback and the offscreen
  `gpu_frame` proxy also includes draw/readback, so no row isolates upload. A
  production hot-path counter was not added (out of bounds). The CPU-side
  build cost above is the material one; no upload number is claimed.
- **Cold sampling misses are not in these frame numbers.** P1 makes repeated
  identical sampling a cache hit (1.6 µs). A time-dependent or parameter-changed
  dirty frame still pays a full sampling pass; that is a dirty-frame cost, not
  steady-state frame assembly, and historical cold numbers are not reused here.
- **Paused/static frames rebuild unchanged particle buffers.** `advance_particles`
  calls `particle_scene_instances` when a particle target exists, before the
  zero-step clock short-circuits advection. While paused, the measured
  `dot_build + trail_build` (10.58 ms on `worst-particles`) is repeated for
  unchanged content, and a camera orbit still needs the sort. This is a
  dirty-gate/interaction change, not buffer reuse; it was not measured as a
  frame path, the stress scene has no floor, and it is recorded here rather
  than promoted.
- **`worst-particles` is a stress configuration**, not a shipped preset, and
  the 30 fps floor is defined on the densest preset. The absolute numbers
  describe this host and this release build only.

## Reproduction

```sh
PATH=/home/paul/.cargo/bin:/usr/bin:/bin \
RUSTC=/home/paul/.cargo/bin/rustc \
CARGO_TARGET_DIR=/home/paul/.cache/vector-field-explorer-targets/issue-247 \
TMPDIR=/home/paul/.cache/vector-field-explorer-targets/issue-247-tmp \
PERF_LOG_DIR=/home/paul/.cache/vector-field-explorer-targets/issue-247-tmp \
  scripts/bench-perf-baseline.sh --gpu
```

The script ran and produced the numbers above, then refused to overwrite the
tracked same-date baseline, as designed. The raw log is local and untracked:
`/home/paul/.cache/vector-field-explorer-targets/issue-247-tmp/perf-baseline-2026-11-21.log`.
The tracked `baselines/2026-11-21-rx9070xt.txt` was not modified.
