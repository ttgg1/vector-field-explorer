# Induction evaluator cap benchmark

Research evidence for issue 27 task 1 (2026-11-17). This measures the
unit-current induction cache preparation, **not** a frame-time target and not a
pass/fail gate.

## Method

`tests/induction_bench_test.rs::induction_cap_benchmark` returns immediately
unless `VFE_INDUCTION_BENCH` is set. Through `scripts/bench-induction.sh`, it
prepares both induction cases with the canonical `[-1, 1]³` domain, softening
`0.1`, default accuracy, and the full unit-current vector-potential quadrature
(8-panel/32-point fine, 4-panel coarse):

- `20³` (8,000 points), the 3D cap;
- `120²` (14,400 points), the provisional 2D cap;
- `80²` (6,400 points), the frozen 2D cap.

Each row is one preparation. Reproduction:

```bash
scripts/bench-induction.sh
```

Standard issue-27 environment (`CARGO_TARGET_DIR`, `TMPDIR`,
`KEEP_TEST_ARTIFACTS=1`), test profile with `opt-level = 2`. Raw output:
`.superpowers/sdd/2026-11-17-induction-demo/evidence/task-1/benchmark.log`.

## Platform

- Intel Core i7-1260P (12 cores / 16 threads), Linux 7.3.3-1-cachyos x86_64.
- `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, `cargo test --test suite`.
- Single foreground run in a shared development session; no CPU pinning,
  frequency locking, warm-up loop or statistical repetition.

## Measured values (one run)

| Case | View | Points | Elapsed |
| --- | --- | ---: | ---: |
| Current loop | `20³` | 8,000 | 956.636 ms |
| Current loop | `120²` provisional | 14,400 | 4039.892 ms |
| Current loop | `80²` frozen cap | 6,400 | 1791.817 ms |
| Solenoid | `20³` | 8,000 | 1181.690 ms |
| Solenoid | `120²` provisional | 14,400 | 2262.408 ms |
| Solenoid | `80²` frozen cap | 6,400 | 1020.765 ms |

The 3D cap stays at `20`. The provisional 2D cap of `120` exceeded the 2 s
Task 1 budget (loop 4.04 s, solenoid 2.26 s), so it was lowered to `80`
(loop 1.79 s, solenoid 1.02 s) before Task 2. These are one-shot preparation
times on a shared host, not a claim of interactive frame performance; Task 5
reruns the benchmark through the integrated app/export path.

## Frozen caps

```rust
pub const MAX_INDUCTION_DENSITY_3D: u32 = 20;
pub const MAX_INDUCTION_DENSITY_2D: u32 = 80;
```
