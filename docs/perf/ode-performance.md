# ODE solver performance

Research evidence for issue 28, Task 1 (2026-11-17). This measures the pure
precomputed `solve_ode` path, **not** a frame-time target and not a pass/fail
gate. Task 5 reruns the integrated benchmark and finalizes this record.

## Method

`tests/ode_bench_test.rs` returns immediately unless `VFE_ODE_BENCH` is set.
Through `scripts/bench-ode.sh` it solves two scenes with the production path
(`ODE_STEP = 0.005`, `ODE_MAX_SAMPLES = 4096`):

- **default:** eight seeds at `x = 1.0 + 0.1*i`, `OdeScene::default()`
  equations (`dx/dt = y`, `dy/dt = -x`), duration `20`;
- **cap:** the same eight seeds and equations at duration `25`, which exhausts
  the 4096 stored-sample cap and ends every trajectory with `StepLimit`.

Each row is one `solve_ode` call, including scene validation, RHS preparation,
RK4 integration and bounds. There is no warm-up, CPU pinning, frequency lock or
statistical repetition. The byte figure is a lower-bound estimate:
`OdeSample` (32 bytes) per stored sample plus one `OdeTrajectory` (40 bytes) per
seed; it excludes allocator slack.

Reproduction:

```bash
scripts/bench-ode.sh
```

Standard issue-28 environment (`CARGO_TARGET_DIR`, `TMPDIR`,
`KEEP_TEST_ARTIFACTS=1`), test profile with `opt-level = 2`. Raw output:
`.superpowers/sdd/2026-11-17-ode-workbench/evidence/task-1/benchmark.log`.

## Task 1 run

- Date: 2026-11-17.
- Host: `Schlepptop`, Intel Core i7-1260P (12 cores / 16 threads), Linux
  7.3.3-1-cachyos x86_64.
- Toolchain: `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, `cargo 1.99.1`.
- One foreground run in a shared development session.

| Case | Seeds | Stored samples | Elapsed | Est. bytes | Stop |
| --- | ---: | ---: | ---: | ---: | --- |
| Default (2D harmonic, duration 20) | 8 | 32,008 | 333.312 ms | — | Complete |
| Cap (2D harmonic, duration 25) | 8 | 32,768 | 338.693 ms | 1,048,896 | StepLimit |

Both measured solves are far below the 2 s intervention threshold, so the
design caps (`ODE_MAX_SAMPLES = 4096`, `ODE_MAX_DURATION = 25`) remain
unchanged. These are one-shot precomputed solve times on a shared host, not a
claim of interactive frame performance; run-to-run variation is material.
