#!/usr/bin/env bash
# Task 5 Maxwell analytic/BEM cap benchmark (release-opt test profile).
set -euo pipefail

export VFE_MAXWELL_BENCH=1
exec cargo test --test suite maxwell_bench_test:: -- --nocapture
