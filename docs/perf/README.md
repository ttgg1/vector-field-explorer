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
