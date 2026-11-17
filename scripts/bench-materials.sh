#!/usr/bin/env bash
# Task 1 material sampling cap benchmark (release-opt test profile).
set -euo pipefail

export VFE_MATERIAL_BENCH=1
exec cargo test --test suite material_bench_test:: -- --nocapture
