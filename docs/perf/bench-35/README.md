# Ticket 35 benchmark artifacts

Tracked reproduction set for the ticket-35 research. These are the inputs and
the temporary instrumentation used for the numbers in
`../../../.superpowers/sdd/2026-11-05-vector-field-explorer/task-35-report.md`
(local, untracked SDD workspace) and `../particle-performance.md`.

**What is tracked here:** the CPU harness, the exact app/GPU instrumentation
patch, the runner and analyzer, the derived summary table, and the CPU
microbenchmark output. **What is not tracked:** the large raw per-scene logs
(about 10 MB) and the original gate/build transcripts were kept only in the
local SDD workspace (`bench-35-artifacts/raw/`); they are not in git. `summary.txt`
was generated from those local logs and is committed so the report's table can
be checked without them.

## What is here

| File | Contents |
| --- | --- |
| `bench-35.rs` | Std-only CPU benchmark harness (copy to `examples/bench35.rs`). |
| `instrumentation.patch` | The exact temporary app/GPU instrumentation applied to base `a7d295a` and reverted before the original commit. It includes the per-frame `BENCH` probe, `BENCHUPLOAD` buffer probe, `BENCHGPU` offscreen microbench (`DrawState::bench_variants`), env-driven scene switch, and fixed 1280×800 window. |
| `summary.txt` | Analyzer output used for the report's p50/p95/p99 table. Generated from the local raw logs. Its `counts=` value is the first steady-window frame, not a median. |
| `analyze.py` | Parser that produced `summary.txt`. It discards the first 2 s of wall time from the `BENCH` CSV and computes p50/p95/p99 plus phase medians. Defaults to `/tmp/vfe35`. |
| `run.sh` | Scene matrix runner. Runs the 11 app scenes, then five GPU-microbench invocations with `VFE_BENCH_GPU=1` (including the 8 s `gpusmoke`). Writes one log per scene to `/tmp/vfe35` by default. |
| `cpu-microbench.log` | Output of `bench-35.rs`. |

## Reproduction

Measurements were taken at base `a7d295a` with the release binary and the
temporary patch. The branch was later rebased onto `d68cfc5`; `Cargo.toml`,
`AGENTS.md` and `tests/` changed between those commits, but **no `src/` file
did**, so the release-binary setup is the same. The patch applies cleanly to
both commits as saved.

```sh
# From the repo root on this feature branch, copy the tracked artifacts out
# first: checking out the measured base removes docs/perf/bench-35/.
cp -r docs/perf/bench-35 /tmp/bench-35

# Check out the measured code base (a7d295a; d68cfc5 has identical src/).
git checkout a7d295a
git apply /tmp/bench-35/instrumentation.patch
mkdir -p examples
cp /tmp/bench-35/bench-35.rs examples/bench35.rs
cargo build --release

# Regenerate the CPU microbenchmark log (same filtered invocation used for the
# tracked cpu-microbench.log).
cargo run --release --example bench35 2>&1 \
  | grep -v '^warning\|^note\|^   Compiling\|^    Finished\|^     Running' \
  > /tmp/bench35-cpu.log

# The runner's binary path defaults to target/release/vector-field-explorer and
# is machine-specific; edit run.sh or override VFE_BENCH_BIN on another host.
# It writes logs to /tmp/vfe35 (override with VFE_BENCH_OUT). The default flow
# runs the 11 app scenes and then the GPU microbench, which sets
# VFE_BENCH_GPU=1 internally and produces gpu_*.log.
bash /tmp/bench-35/run.sh

# analyze.py must read the runner's output directory, not a saved copy.
python3 /tmp/bench-35/analyze.py /tmp/vfe35
```

The runner invokes `timeout` and the release binary. App scenes run for 25 s and
write `BENCH`/`BENCHUPLOAD`/`BENCHMETA` lines; the GPU block runs after frame 60
with `VFE_BENCH_GPU=1` and writes six `BENCHGPU` lines per run into
`gpu_default_2d.log`, `gpu_dense_radial_16.log`, `gpu_dense_radial_40.log`,
`gpu_worst_4000_t32.log`, and the 8 s `gpusmoke.log`. `analyze.py` reads those
logs and prints the percentiles and phase medians. The committed `summary.txt`
was produced the same way; the raw logs themselves are local/untracked.

`counts=` in `summary.txt` is the **first steady-window frame**, not a median.
For `worst_3d_4000_trail32` the steady-window trail count (n=619) has median
≈31 800 segments and range ≈30 400–33 600; 33 255 is that first steady frame.

## Local-only raw logs

These files were retained in the local SDD workspace but are **not** in git:

| Scene | Local log |
| --- | --- |
| default 2D Pump | `bench-35-artifacts/raw/default_2d_pump.log` |
| default + streamlines | `bench-35-artifacts/raw/default_2d_pump_streamlines.log` |
| 3D radial, density 16 (4096 arrows) | `bench-35-artifacts/raw/dense_3d_radial_16.log` |
| Magnetic dipole, density 16 | `bench-35-artifacts/raw/dense_3d_magnetic_dipole_16.log` |
| 3D radial, density 40 (64 000 arrows) | `bench-35-artifacts/raw/dense_3d_radial_40.log` |
| worst, trail 0 | `bench-35-artifacts/raw/worst_3d_4000_trail0.log` |
| worst, trail 8 | `bench-35-artifacts/raw/worst_3d_4000_trail8.log` |
| worst, trail 32 | `bench-35-artifacts/raw/worst_3d_4000_trail32.log` |
| worst trail 32, arrows off | `bench-35-artifacts/raw/worst_3d_4000_trail32_arrows_off.log` |
| worst trail 32, particles off | `bench-35-artifacts/raw/worst_3d_4000_trail32_particles_off.log` |
| worst trail 32, both off | `bench-35-artifacts/raw/worst_3d_4000_trail32_both_off.log` |
| GPU microbench | `bench-35-artifacts/raw/gpu_*.log` |

`gpusmoke.log` is an early 8 s smoke that sampled trails before the ring filled
(4000 segments); it is not the source of the report's GPU numbers. The GPU
numbers come from `gpu_worst_4000_t32.log` (30 855 segments at the sample
point). The local gate/build transcripts (`test.log`, `test-web.log`) are also
not tracked.
