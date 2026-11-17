# Material sampling cap benchmark

Initial Task 1 evidence for issue 29 / sub-issue 66 (2026-11-17). Task 5
finalizes this document. It measures pure analytic material sampling, **not** a
frame-time target, pass/fail gate, browser result or fps claim.

## Method

`tests/material_bench_test.rs::material_sampling_cap_benchmark` returns
immediately unless `VFE_MATERIAL_BENCH` is set. Through
`scripts/bench-materials.sh` it samples every one of the four bounded cases at:

- `40³` (64,000 points), the planned 3D cap;
- `200²` (40,000 points), the planned 2D slice cap.

The domain is `[-2, 2]³`, sphere radius `1.0`, interface axis X, offset `0`,
`Total` view, and an applied field along X. The 2D row samples the local grid of
a `Z = 0` `PlaneFrame` and projects the family vector. Each row is one
`sample_materials` call including grid iteration, exact-interface guarding,
field evaluation, decomposition and arrow construction. No app frame, GPU work,
panel layout, export encoding or browser runtime is measured.

Reproduction:

```bash
scripts/bench-materials.sh
```

Standard issue-29 environment (`CARGO_TARGET_DIR`, `TMPDIR`,
`KEEP_TEST_ARTIFACTS=1`), test profile with `opt-level = 2`. Raw output:
`.superpowers/sdd/2026-11-17-materials/evidence/task-1/benchmark.log`.

## Task 1 run

- Date: 2026-11-17.
- Host: `Schlepptop`, Linux 7.3.3-1-cachyos x86_64.
- Toolchain: `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, `cargo 1.99.1`.
- One foreground run in a shared development session; no CPU pinning,
  frequency locking, warm-up loop or statistical repetition.

| Case | View | Points | Omitted | Elapsed |
| --- | --- | ---: | ---: | ---: |
| Dielectric sphere | `40³` | 64,000 | 0 | 8.112 ms |
| Dielectric sphere | `200²` | 40,000 | 0 | 4.020 ms |
| Dielectric boundary | `40³` | 64,000 | 0 | 4.098 ms |
| Dielectric boundary | `200²` | 40,000 | 0 | 2.484 ms |
| Permeable sphere | `40³` | 64,000 | 0 | 6.682 ms |
| Permeable sphere | `200²` | 40,000 | 0 | 4.362 ms |
| Permeable boundary | `40³` | 64,000 | 0 | 3.707 ms |
| Permeable boundary | `200²` | 40,000 | 0 | 2.518 ms |

Every row stayed far below the 2 s review threshold. Omitted counts are zero
here because the `40³`/`200²` lattices do not place a sample exactly on these
interfaces; the exact-hit guard and omission accounting are pinned separately by
`tests/material_test.rs::sampling_omits_exact_interface_but_keeps_near_interface`.
The cap test asserts `< 2 s` per row, so a regression fails the benchmark rather
than silently lowering a cap.

## Caps and scope

The planned app caps are `40³` in 3D and `200²` in the 2D slice; those
constants belong to the Task 2 state integration. This document records only
analytic sampling evidence on the recorded host. Browser, WebAssembly runtime,
GPU and manual visual gates were owner-waived for this delivery and are not
claimed as passed.
