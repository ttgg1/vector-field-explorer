#!/usr/bin/env bash
# Release benchmark for source-scene sampling (issue 26 task 5).
#
# Runs the ignored `Source gallery` test at 40^3 and 200^2 and prints elapsed
# milliseconds plus arrow counts. Evidence only: there is no pass/fail
# threshold and no browser/manual claim.
#
# Usage: scripts/bench-source-presets.sh
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

export PATH=/home/paul/.cargo/bin:/usr/bin:/bin
export RUSTC=/usr/bin/rustc
export CARGO_TARGET_DIR=/home/paul/.cache/vector-field-explorer-targets/issue-26
export TMPDIR=/home/paul/.cache/vector-field-explorer-targets/issue-26/tmp
export KEEP_TEST_ARTIFACTS=1

mkdir -p "$TMPDIR"
cargo test --release --test suite source_preset_bench_test:: -- --ignored --nocapture
