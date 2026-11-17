#!/usr/bin/env bash
# Task 1 ODE solve benchmark (release-opt test profile).
set -euo pipefail

export VFE_ODE_BENCH=1
exec cargo test --test suite ode_bench_test:: -- --nocapture
