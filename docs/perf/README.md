# Performance evidence

This directory collects native performance evidence: per-feature reports and
the maintained post-epic baseline harness. None of it is a CI gate, and none of
it claims a product floor, frame-rate target or pass/fail threshold.

## Current baseline harness

`tests/perf_baseline_test.rs` (registered in `tests/suite.rs`) builds the
post-epic scene set from shipped public APIs and measures, per scene:

- `snapshot`: `State::snapshot()`, the CPU geometry build (arrow instances,
  streamlines, scalar overlays and, for the scalar scene, the heatmap). This
  call also runs the field sampling pass internally: `snapshot_with_styles`
  calls `sample()` at `src/viewmodel.rs:4408`. Its numbers therefore sit
  close to the `sample` row, and the `sample` column is the isolated sampling
  cost, not an addition to `snapshot`.
- Extra per-scene work where the interesting cost is outside the two above:
  `scalar_view` for the 160x160 heatmap, `continuity_report` for the harmonic
  custom density, `source_sample` and `probe_report` for the Sources Gaussian
  surface, and `maxwell_sample` for the plane wave.

`probe_report` clocks only `State::update_probe_surface` (validation and dirty
mark) plus `refresh_probe_reports(true)` (the uncached flux pass). The harness
pre-builds every replacement sphere outside the clock; the changing geometry
still defeats the report cache on each iteration.

Each measurement discards warm-up iterations, then reports p50/p95/p99 in
microseconds plus the sample count `n`. A one-shot cold-start proxy
(`State::new(false)` plus its first snapshot) is also reported, in milliseconds.
It is a proxy for first geometry build, not application startup.

The scene set is `default-2d`, `densest-3d`, `dipole-3d`, `grid-stress`,
`worst-particles` (4000 particles, 32 trail points), `scalar-heatmap`
(160x160), `custom-density` (harmonic current with its continuity report),
`field-probe` (Gaussian sphere in Sources mode) and `plane-wave` (Maxwell plane
wave enabled).

With `VFE_PERF_GPU=1` and a real headless adapter, the same scenes are rendered
once per scene through `HeadlessRenderSession` built from `PreparedScene`, and
offscreen frames are timed as a proxy. The host line names the adapter and
backend. If no adapter initialises, the run prints an explicit skip line and
keeps the CPU results; no GPU number is fabricated.

A normal `cargo test` returns immediately: the module only runs when
`VFE_PERF_BASELINE=1`.

### Reproduce

```sh
scripts/bench-perf-baseline.sh          # CPU baseline
scripts/bench-perf-baseline.sh --gpu    # CPU + offscreen-frame proxy
```

The script runs the release-profile suite, keeps the full local log in
`/tmp/opencode/perf-baseline-<date>.log`, and writes only the machine-readable
non-skip `PERF` lines to `docs/perf/baselines/<date>-<gpu>.txt`. The `<gpu>`
suffix is derived from the `gpu=` token in the run's `PERF host=...` line
(sanitised to lowercase model tokens), so the filename cannot disagree with the
recorded GPU: `rx9070xt` for the authoritative host, `notprobed` for a CPU-only
run. GPU skip notices remain in the local log and are never copied into the
tracked baseline. The script warns when the reported host or GPU differs from
the expected baseline host, and it refuses to overwrite an existing same-date
file.

`PERF_LOG_DIR` overrides the local log directory and `CARGO` the cargo binary.
Like the rest of `scripts/`, the baseline script is Linux-only.

### What this harness does not measure

- The egui present path: layout, widget tessellation, painter submission and
  the window compositor.
- Present-mode vsync or any real display frame cadence.
- The browser/wasm build or WebGPU path.
- The integrated GPU (the authoritative baseline host is the discrete AMD
  Radeon RX 9070 XT, RADV GFX1201).
- Application startup cost: the cold-start proxy omits window, GPU, font and
  UI initialisation.

It claims no floor. A recorded baseline describes one host, one build profile
and one code revision; compare like with like.

## Present-path floor record (#18 task b)

The accepted floors in
[particle-performance.md](particle-performance.md) describe **presented** rates:
60 fps for the default 2D scene and 30 fps for the densest shipped 3D scene at
1280×800 on the integrated-GPU reference. The offscreen harness above cannot
prove them because it never exercises the egui painter, swapchain or
compositor. The bounded native present-path mode does:

```sh
scripts/bench-present-floor.sh
# equivalent single scene:
VFE_PERF_PRESENT=1 VFE_PERF_SCENE=densest-3d VFE_PERF_SECONDS=10 \
  cargo run --release --bin vector-field-explorer
```

`VFE_PERF_PRESENT=1` is the only enabling value; absent or any other value
leaves the app byte-for-byte unchanged. `VFE_PERF_SCENE` defaults to
`default-2d` and accepts `densest-3d`; `VFE_PERF_SECONDS` defaults to 10 and is
clamped to 5..=60; `VFE_PERF_OUT` optionally receives just the two `PERF present`
lines. The window is pinned to 1280×800 logical (min = max), the first 2 s are
discarded, then **raw wall-clock intervals between successive `ui()` update
calls** are aggregated to p50/p95/p99 ms and `fps_p50`. Each interval includes
the previous frame's present and swapchain work; GPU submit cost inside the
current frame is only visible through the next interval. The app closes itself,
settings and history are in-memory, and while presets and saved scenes may be
read read-only by their panels, they are never overwritten. A wall-clock
deadline (warm-up + window + 5 s slack) prints a bounded `error=deadline_exceeded`
line and closes even if frames stop arriving. The record line names the actual
adapter, backend and pixel size. Dropped frames are not observable through
eframe, so none are claimed.

**Recorded 2026-11-21 at base `b4c9a39`, release build, AMD Ryzen 9 7950X +
AMD Radeon RX 9070 XT (RADV GFX1201), Vulkan on Wayland, 1280×800, 10 s window
after 2 s warm-up** ([baseline](baselines/2026-11-21-present-rx9070xt.txt)).
This is a **re-measurement** with the raw inter-update wall-clock sampler; the
earlier `stable_dt`-smoothed record was replaced, not kept alongside it.

| Scene | Documented floor | p50 ms | p95 ms | p99 ms | fps_p50 | Verdict |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| `default-2d` | 60 fps | 5.553 | 5.737 | 5.891 | 180.1 | **PASS** |
| `densest-3d` | 30 fps | 5.570 | 5.766 | 5.933 | 179.5 | **PASS** |

Explicit verdict wording:

- **PASS** means the measured p50 on this host, build and 1280×800 size reached
  the documented floor. Both floor scenes passed; no floor was lowered.
- **FAIL** would mean the measured p50 fell below the floor on a correctly
  configured run at 1280×800; none did here.
- **not verified** remains the integrated-GPU reference itself: this host has no
  iGPU, so the record proves the discrete RX 9070 XT clears the floors, not that
  the documented integrated target does. Other scenes and hardware are also not
  verified by this record.
- The display refreshes at 179.92 Hz, so ~5.55 ms is the vsync cap with
  headroom, not a GPU-limit claim; the p50 cannot distinguish “fast enough” from
  “faster than this display can show”. Raw inter-update p95/p99 now include
  scheduling outliers that the earlier smoothed sampler damped.

Run logs stay under `/tmp/opencode/present-floor-<date>/`; the script refuses to
overwrite an existing same-date baseline. It is a measurement record, not a CI
threshold.

## P1–P6 disposition

[p1-p6-disposition.md](p1-p6-disposition.md) records the task-(c) decisions for
the historical performance recommendations: P1 (bounded evaluation cache) and P4
(trail ordering) are promoted, P2 is measure-before-more, P3 is deferred post-v1
pending an ADR, P5 is not authorized, and P6 is unverifiable on this host. It
changes no floor and promotes no code by itself.

## Per-feature reports

- [animation-performance.md](animation-performance.md): animation export,
  GIF encoding and ffmpeg sink.
- [induction-performance.md](induction-performance.md): induction evaluator
  cap benchmark.
- [materials-performance.md](materials-performance.md): material sampling cap
  benchmark.
- [maxwell-performance.md](maxwell-performance.md): Maxwell teaching-case
  sampling and bounded Conductor2D BEM.
- [ode-performance.md](ode-performance.md): precomputed ODE solve.
- [object-renderer.md](object-renderer.md): native object pass.
- [particle-performance.md](particle-performance.md): ticket 35 CPU harness;
  reproduction set in [bench-35/](bench-35/).
- [source-preset-benchmark.md](source-preset-benchmark.md): densest source
  preset sampling.
- [baselines/](baselines/): dated `PERF` records from the maintained harness.
