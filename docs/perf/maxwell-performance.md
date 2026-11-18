# Maxwell teaching-case cap benchmark

Final evidence for issue 30 / sub-issue 76 (2026-11-18). It measures pure
analytic Maxwell sampling and the bounded Conductor2D BEM solve/sampling. It is
**not** a frame-time target, pass/fail product gate, browser result or fps
claim.

## Method

`tests/maxwell_bench_test.rs::maxwell_cap_benchmark` returns immediately unless
`VFE_MAXWELL_BENCH` is set. Through `scripts/bench-maxwell.sh` it measures:

- `CoaxialElectric` analytic sampling at `40³` (64,000 points), the app's 3D
  cap;
- `CoaxialElectric` analytic sampling at `200²` (40,000 points) on a `Z` slice,
  the app's 2D cap;
- `Conductor2D` with `elements = 256` (fine) and the internal coarse run at 128:
  one `solve_bem` call plus one `200²` `sample_maxwell_with_bem` call using the
  cached solution.

The domain is `[-2, 2]³`; coaxial radii are the scene defaults (`radius = 1`,
`outer_radius = 2`); the conductor is a circle of radius 1 with `E0 = (1, 0)`.
The 2D row samples the local grid of a `Z` `PlaneFrame`. Each analytic row is
one pure `sample_maxwell` call. The BEM rows separate the dense solve
(including its coarse diagnostic run) from the cached sampling pass. No app
frame, GPU work, panel layout, export encoding, wasm runtime or browser runtime
is measured.

Reproduction:

```bash
scripts/bench-maxwell.sh
```

Standard issue-30 environment (`CARGO_TARGET_DIR`, `TMPDIR`), test profile with
`opt-level = 2`. Raw output and host metadata live in
`.superpowers/sdd/2026-11-17-maxwell/evidence/task-5/benchmark.log` and
`host.txt`.

## Run

- Date: 2026-11-18.
- Host: `Schlepptop`, Linux x86_64. The test's `HOSTNAME` lookup fell back to
  `linux-x86_64`, recorded in the raw log.
- Toolchain: `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, `cargo 1.99.1`.
- One foreground run in a shared development session; no CPU pinning,
  frequency locking, warm-up loop or statistical repetition.

| Row | Points/work | Elapsed | Diagnostics |
| --- | ---: | ---: | --- |
| Analytic `40³` | 64,000 samples | 3.701 ms | 35,360 arrows, 28,640 omitted |
| Analytic `200²` | 40,000 samples | 1.749 ms | 23,276 arrows, 16,724 omitted |
| Conductor2D solve, fine 256 / coarse 128 | 16,777,216 work units | 4.187 ms | relative difference `6.67e-5` (target `1e-3`), net charge `2.10e-13` |
| Conductor2D `200²` sampling with cached solve | 40,000 samples | 13.998 ms | 32,212 arrows, 7,788 omitted, target met |

Both analytic rows omit the exact-interface and outside-region samples; these
are counted, not clamped. The BEM solve work is the fine `N³` dense elimination
(`256³`); its coarse run is half the element count and is never compared as an
identical run. The sampling row reuses the solved panel charges and does not
solve again. Every measured row is far below the 2 s plan threshold, so no cap
was changed. The test asserts `< 2 s` per row, so a regression fails the
benchmark rather than silently reducing a cap.

The host reported is the environment's `HOSTNAME` fallback; this document
records the actual host separately. Numbers are single-run observations, not
statistical guarantees.

## Caps and scope

The app caps are `MAXWELL_MAX_DENSITY_3D = 40` and
`MAXWELL_MAX_DENSITY_2D = 200`; the BEM fine count is capped at
`MAXWELL_MAX_BEM_ELEMENTS = 256` with the coarse run at `elements / 2`. This
document records only pure sampling/solve evidence on the recorded host; it
does not claim interactive frame times. Browser, WebAssembly runtime, GPU and
manual visual gates remain owner-waived, not passed. The native WebAssembly
compile check is recorded separately when the target is installed.
