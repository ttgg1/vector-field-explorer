# Induction evaluator cap benchmark

Research evidence for issue 27 (2026-11-17). This measures the unit-current
induction cache preparation, **not** a frame-time target and not a pass/fail
gate.

## Method

`tests/induction_bench_test.rs::induction_cap_benchmark` returns immediately
unless `VFE_INDUCTION_BENCH` is set. Through `scripts/bench-induction.sh`, it
prepares both induction cases with the canonical `[-1, 1]³` domain, softening
`0.1`, default accuracy, and the full unit-current vector-potential quadrature
(8-panel/32-point fine, 4-panel coarse):

- `20³` (8,000 points), the 3D cap;
- `80²` (6,400 points), the frozen 2D cap;
- `120²` (14,400 points), the former provisional 2D cap, retained as a
  comparison row only. It is not a supported cap and no longer gates the app.

Each row is one `prepare_induction` call. This is the same unit-current
cache-preparation seam that `State::refresh_induction` calls before its
per-frame `sample_induction` rescale. It does not measure app frames, GPU work,
panel layout, export encoding, or the per-frame rescale/reclassification, so no
interactive frame rate is claimed. The 33 ms particle-performance target is not
claimed for induction.

Reproduction:

```bash
scripts/bench-induction.sh
```

Standard issue-27 environment (`CARGO_TARGET_DIR`, `TMPDIR`,
`KEEP_TEST_ARTIFACTS=1`), test profile with `opt-level = 2`. Raw output:
`.superpowers/sdd/2026-11-17-induction-demo/evidence/task-5/benchmark.log`.

## Task 5 rerun

- Date: 2026-11-17.
- Host: `Schlepptop`, Intel Core i7-1260P (12 cores / 16 threads), Linux
  7.3.3-1-cachyos x86_64.
- Toolchain: `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, `cargo 1.99.1`.
- One foreground run in a shared development session; no CPU pinning,
  frequency locking, warm-up loop, or statistical repetition.

| Case | View | Points | Elapsed |
| --- | --- | ---: | ---: |
| Current loop | `20³` cap | 8,000 | 606.615 ms |
| Current loop | `80²` cap | 6,400 | 1108.330 ms |
| Current loop | `120²` comparison | 14,400 | 2630.636 ms |
| Solenoid | `20³` cap | 8,000 | 745.063 ms |
| Solenoid | `80²` cap | 6,400 | 659.508 ms |
| Solenoid | `120²` comparison | 14,400 | 1491.914 ms |

Both frozen caps stayed under 2 s per preparation on this run. The earlier
Task 1 run measured 956.636 ms / 1791.817 ms (loop 20³/80²) and 1181.690 ms /
1020.765 ms (solenoid 20³/80²), with `120²` at 4039.892 ms / 2262.408 ms.
These are one-shot preparation times on a shared host, not a claim of
interactive frame performance; run-to-run variation is material.

## Frozen caps

```rust
pub const MAX_INDUCTION_DENSITY_3D: u32 = 20;
pub const MAX_INDUCTION_DENSITY_2D: u32 = 80;
```

## Evidence scope

Native tests and the native benchmark cover this evidence. The WebAssembly
compile check is recorded separately when the target is installed. Browser,
WebAssembly runtime/visual, and manual visual gates were owner-waived for this
delivery; they are not claimed as passed.
