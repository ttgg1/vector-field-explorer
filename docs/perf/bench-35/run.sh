#!/usr/bin/env bash
# Ticket 35 native app benchmark runner (reproduction artifact).
# Run from the repo root after applying instrumentation.patch and building.
# The binary and output paths are overridable; edit/override them on another host.
set -u
bin="${VFE_BENCH_BIN:-target/release/vector-field-explorer}"
out="${VFE_BENCH_OUT:-/tmp/vfe35}"
mkdir -p "$out"

run() {
  local label="$1"; shift
  echo "=== $label ==="
  env VFE_BENCH_SECONDS=25 VFE_BENCH_LABEL="$label" RUST_LOG=info "$@" \
    timeout --kill-after=5s 60s "$bin" 2>"$out/$label.log"
  echo "exit=$? lines=$(grep -c 'BENCH,' "$out/$label.log")"
}

run_gpu() {
  local label="$1"; local seconds="$2"; shift 2
  echo "=== $label (GPU) ==="
  env VFE_BENCH_SECONDS="$seconds" VFE_BENCH_GPU=1 VFE_BENCH_LABEL="$label" \
    RUST_LOG=info "$@" \
    timeout --kill-after=5s 60s "$bin" 2>"$out/$label.log"
  echo "exit=$? gpu_lines=$(grep -c 'BENCHGPU,' "$out/$label.log")"
}

# 1. Default scene: fresh app defaults (2D Pump, density 24, arrows, no particles).
run default_2d_pump \
  VFE_BENCH_MODE=2d VFE_BENCH_DENSITY=24 VFE_BENCH_ARROWS=1 VFE_BENCH_PARTICLES=0

# 1b. Default + streamlines, to price the line path.
run default_2d_pump_streamlines \
  VFE_BENCH_MODE=2d VFE_BENCH_DENSITY=24 VFE_BENCH_ARROWS=1 VFE_BENCH_PARTICLES=0 \
  VFE_BENCH_STREAMLINES=1

# 2. Densest shipped preset: 3D radial at the 3D default density 16 (4096 arrows).
run dense_3d_radial_16 \
  VFE_BENCH_MODE=3d VFE_BENCH_FORMULA='(x, y, z)' VFE_BENCH_DENSITY=16 \
  VFE_BENCH_ARROWS=1 VFE_BENCH_PARTICLES=0

# 2b. The most expensive shipped formula at the same grid.
run dense_3d_magnetic_dipole_16 \
  VFE_BENCH_MODE=3d \
  VFE_BENCH_FORMULA='(3*x*z/(x^2+y^2+z^2+0.05)^2.5, 3*y*z/(x^2+y^2+z^2+0.05)^2.5, (3*z^2-(x^2+y^2+z^2))/(x^2+y^2+z^2+0.05)^2.5)' \
  VFE_BENCH_DENSITY=16 VFE_BENCH_ARROWS=1 VFE_BENCH_PARTICLES=0

# 2c. Grid stress: the sidebar's 3D maximum (40^3 = 64,000 arrows).
run dense_3d_radial_40 \
  VFE_BENCH_MODE=3d VFE_BENCH_FORMULA='(x, y, z)' VFE_BENCH_DENSITY=40 \
  VFE_BENCH_ARROWS=1 VFE_BENCH_PARTICLES=0

# 3. Worst case: 4000 particles + 32 trail points + arrows + 3D.
run worst_3d_4000_trail32 \
  VFE_BENCH_MODE=3d VFE_BENCH_FORMULA='(x, y, z)' VFE_BENCH_DENSITY=16 \
  VFE_BENCH_ARROWS=1 VFE_BENCH_PARTICLES=1 VFE_BENCH_COUNT=4000 VFE_BENCH_TRAIL=32

# 3 A/B toggles around the worst case.
run worst_3d_4000_trail0 \
  VFE_BENCH_MODE=3d VFE_BENCH_FORMULA='(x, y, z)' VFE_BENCH_DENSITY=16 \
  VFE_BENCH_ARROWS=1 VFE_BENCH_PARTICLES=1 VFE_BENCH_COUNT=4000 VFE_BENCH_TRAIL=0

run worst_3d_4000_trail8 \
  VFE_BENCH_MODE=3d VFE_BENCH_FORMULA='(x, y, z)' VFE_BENCH_DENSITY=16 \
  VFE_BENCH_ARROWS=1 VFE_BENCH_PARTICLES=1 VFE_BENCH_COUNT=4000 VFE_BENCH_TRAIL=8

run worst_3d_4000_trail32_arrows_off \
  VFE_BENCH_MODE=3d VFE_BENCH_FORMULA='(x, y, z)' VFE_BENCH_DENSITY=16 \
  VFE_BENCH_ARROWS=0 VFE_BENCH_PARTICLES=1 VFE_BENCH_COUNT=4000 VFE_BENCH_TRAIL=32

run worst_3d_4000_trail32_particles_off \
  VFE_BENCH_MODE=3d VFE_BENCH_FORMULA='(x, y, z)' VFE_BENCH_DENSITY=16 \
  VFE_BENCH_ARROWS=1 VFE_BENCH_PARTICLES=0

run worst_3d_4000_trail32_both_off \
  VFE_BENCH_MODE=3d VFE_BENCH_FORMULA='(x, y, z)' VFE_BENCH_DENSITY=16 \
  VFE_BENCH_ARROWS=0 VFE_BENCH_PARTICLES=0

# 4. GPU microbench. The instrumentation gates this on VFE_BENCH_GPU and runs
# it once after frame 60; each process emits six BENCHGPU lines (clear/lines/
# trails/arrows/dots/scene). These are separate invocations from the app scenes.
run_gpu gpu_default_2d 25 \
  VFE_BENCH_MODE=2d VFE_BENCH_DENSITY=24 VFE_BENCH_ARROWS=1 VFE_BENCH_PARTICLES=0

run_gpu gpu_dense_radial_16 25 \
  VFE_BENCH_MODE=3d VFE_BENCH_FORMULA='(x, y, z)' VFE_BENCH_DENSITY=16 \
  VFE_BENCH_ARROWS=1 VFE_BENCH_PARTICLES=0

run_gpu gpu_dense_radial_40 25 \
  VFE_BENCH_MODE=3d VFE_BENCH_FORMULA='(x, y, z)' VFE_BENCH_DENSITY=40 \
  VFE_BENCH_ARROWS=1 VFE_BENCH_PARTICLES=0

run_gpu gpu_worst_4000_t32 25 \
  VFE_BENCH_MODE=3d VFE_BENCH_FORMULA='(x, y, z)' VFE_BENCH_DENSITY=16 \
  VFE_BENCH_ARROWS=1 VFE_BENCH_PARTICLES=1 VFE_BENCH_COUNT=4000 VFE_BENCH_TRAIL=32

# Optional early smoke (8 s): trails are sampled before the ring fills, so this
# is not a report source (the report uses gpu_worst_4000_t32).
run_gpu gpusmoke 8 \
  VFE_BENCH_MODE=3d VFE_BENCH_FORMULA='(x, y, z)' VFE_BENCH_DENSITY=16 \
  VFE_BENCH_ARROWS=1 VFE_BENCH_PARTICLES=1 VFE_BENCH_COUNT=4000 VFE_BENCH_TRAIL=32
