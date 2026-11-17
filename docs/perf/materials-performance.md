# Material sampling cap benchmark

Final evidence for issue 29 / sub-issue 70 (2026-11-17). It measures pure
analytic material sampling, **not** a frame-time target, pass/fail gate,
browser result or fps claim.

## Method

`tests/material_bench_test.rs::material_sampling_cap_benchmark` returns
immediately unless `VFE_MATERIAL_BENCH` is set. Through
`scripts/bench-materials.sh` it samples every one of the four bounded cases at:

- `40³` (64,000 points), the app's 3D cap;
- `200²` (40,000 points), the app's 2D slice cap.

The domain is `[-2, 2]³`, sphere radius `1.0`, interface axis X, offset `0`, and
`Total` view. Dielectric cases use `eps_bg = 1`, `eps_in = 4`; permeable cases
use `mu_bg = 1`, `mu_in = 4`; the applied field is `[1, 0, 0]` in display units.
The 2D row samples the local grid of a `Z = 0` `PlaneFrame` and projects the
family vector. Each row is one `sample_materials` call including grid
iteration, exact-interface guarding, field evaluation, decomposition and arrow
construction. No app frame, GPU work, panel layout, export encoding or browser
runtime is measured.

Reproduction:

```bash
scripts/bench-materials.sh
```

Standard issue-29 environment (`CARGO_TARGET_DIR`, `TMPDIR`,
`KEEP_TEST_ARTIFACTS=1`), test profile with `opt-level = 2`. Raw output and host
metadata: `.superpowers/sdd/2026-11-17-materials/evidence/task-5/benchmark.log`
and `host.txt`.

## Task 5 integrated run

- Date: 2026-11-17.
- Host: `Schlepptop`, Linux 7.3.3-1-cachyos x86_64. The benchmark's own
  `HOSTNAME` lookup fell back to `linux-x86_64`, recorded in the raw log.
- Toolchain: `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, `cargo 1.99.1`.
- One foreground run in a shared development session; no CPU pinning,
  frequency locking, warm-up loop or statistical repetition.

| Case | View | Points | Omitted | Response | Elapsed |
| --- | --- | ---: | ---: | --- | ---: |
| Dielectric sphere | `40³` | 64,000 | 0 | `response_empty=false` | 5.687 ms |
| Dielectric sphere | `200²` | 40,000 | 0 | `response_empty=false` | 2.337 ms |
| Dielectric boundary | `40³` | 64,000 | 0 | `response_empty=false` | 2.417 ms |
| Dielectric boundary | `200²` | 40,000 | 0 | `response_empty=false` | 1.501 ms |
| Permeable sphere | `40³` | 64,000 | 0 | `response_empty=false` | 3.363 ms |
| Permeable sphere | `200²` | 40,000 | 0 | `response_empty=false` | 2.034 ms |
| Permeable boundary | `40³` | 64,000 | 0 | `response_empty=false` | 2.127 ms |
| Permeable boundary | `200²` | 40,000 | 0 | `response_empty=false` | 1.428 ms |

All rows sample `Total`; all four scenes have a 4:1 contrast, so the response is
nonzero and `response_empty` stays false. The benchmark does not separately
sample the `Response` view; zero-response behavior is pinned by
`tests/material_test.rs::no_contrast_response_is_empty_with_exact_note`.

Every row stayed far below the 2 s review threshold. Omitted counts are zero
here because the `40³`/`200²` lattices do not place a sample exactly on these
interfaces; the exact-hit guard and omission accounting are pinned separately by
`tests/material_test.rs::sampling_omits_exact_interface_but_keeps_near_interface`.
The cap test asserts `< 2 s` per row, so a regression fails the benchmark rather
than silently lowering a cap.

This run replaces the initial Task 1 evidence as the delivery record; the Task 1
log remains under
`.superpowers/sdd/2026-11-17-materials/evidence/task-1/benchmark.log`.

## Caps and scope

The app caps are `MATERIAL_MAX_DENSITY_3D = 40` in 3D and
`MATERIAL_MAX_DENSITY_2D = 200` on the 2D slice. This document records only
analytic sampling evidence on the recorded host; it does not claim interactive
frame times. Browser, WebAssembly runtime, GPU and manual visual gates were
owner-waived for this delivery and are not claimed as passed. The native
WebAssembly compile check is recorded separately when the target is installed.
