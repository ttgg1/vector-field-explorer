# Source preset sampling benchmark

Research evidence for issue 26 task 5 (2026-11-17). This is measured sampling
throughput for the densest built-in source preset, **not** a frame-time target
and not a pass/fail gate.

## Method

`tests/source_preset_bench_test.rs::source_gallery_sampling_benchmark` is
`#[ignore]`d. It loads **Source gallery** through the ordinary preset setters
(the canonical `[-2, 2]³` domain, 3D view, unit strengths, softening `0.1`,
accuracy opt-in off) and calls `sample_source_scene` once at each grid:

- `40³` (64 000 sample positions), output `128 000` arrows (both families);
- `200²` (40 000 sample positions), output `80 000` arrows (both families).

It measures wall time around one sampling call, asserts every returned arrow is
finite, asserts both family diagnostics are nonempty, and asserts the gallery
validates. Reproduction:

```bash
scripts/bench-source-presets.sh
```

Release profile, standard issue-26 environment (`CARGO_TARGET_DIR`,
`TMPDIR`, `KEEP_TEST_ARTIFACTS=1`). Raw output:
`.superpowers/sdd/2026-11-17-source-visualization/evidence/task-5/benchmark.log`.

## Platform

- Intel Core i7-1260P (12 cores / 16 threads), Linux 7.3.3-1-cachyos x86_64.
- `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, release profile, `--test suite`.
- Single foreground run in a shared development session; no CPU pinning,
  frequency locking, warm-up loop, or statistical repetition.

## Measured values (one run)

| Scene / grid | Samples | Arrows | Elapsed |
| --- | ---: | ---: | ---: |
| Source gallery, `40³` | 64 000 | 128 000 | 6698.754 ms |
| Source gallery, `200²` | 40 000 | 80 000 | 8311.437 ms |

A first run before final formatting measured 6622.986 ms and 8220.413 ms, and
an independent controller rerun after review measured 6252.975 ms and
8510.180 ms; the spread is ordinary shared-machine noise, not a regression
signal. Raw controller output:
`.superpowers/sdd/2026-11-17-source-visualization/evidence/task-5/controller-benchmark.log`.

These are slow: both sample passes are seconds, not tens of milliseconds. The
33 ms “densest frame” context from the particle benchmark is **not met** by
this source sampling pass, and these numbers do not establish any interactive
frame budget. They are a baseline for the current evaluator and grid sizes; a
future performance task would need warm-up, repetition, and profiling rather
than treating this one-shot run as a threshold. No browser or manual visual
evidence is claimed.
