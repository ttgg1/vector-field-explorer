#!/usr/bin/env bash
# Task 1 induction evaluator cap benchmark (release-opt test profile).
set -euo pipefail

export VFE_INDUCTION_BENCH=1
exec cargo test --test suite induction_bench_test:: -- --nocapture
