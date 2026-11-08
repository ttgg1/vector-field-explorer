#!/usr/bin/env bash
# Runtime smoke test: launch the app under a timeout and fail on startup
# panics or wgpu validation errors.
#
# Why: `cargo build` and `cargo test` never create a wgpu device, so broken
# WGSL (parse errors, bad bindings) only surfaces when the app actually runs.
#
# Usage: scripts/smoke-run.sh [seconds]   (default 15)
#
# Pass = the app stayed up until the timeout (or exited cleanly) with no
# panic / wgpu validation error in its output.

set -euo pipefail

secs="${1:-15}"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

# Build first so `secs` measures app startup, not compilation.
cargo build --quiet

bin="${SMOKE_BIN:-target/debug/vector-field-explorer}"
log="$(mktemp)"
trap 'rm -f "$log"' EXIT

set +e
timeout --kill-after=5s "${secs}s" "$bin" 2>&1 | tee "$log"
status=${PIPESTATUS[0]}
set -e

# A panic can happen on a non-main thread while the process keeps running, so
# grep the log as well as trusting the exit status.
if grep -Eq 'panicked at|wgpu error:|Validation Error|Shader .* parsing error' "$log"; then
  echo "smoke: FAIL - panic or wgpu validation error detected" >&2
  exit 1
fi

# timeout(1) exits 124 when it had to kill the still-running app: success.
if [ "$status" -eq 124 ] || [ "$status" -eq 137 ]; then
  echo "smoke: PASS - app stayed up for ${secs}s"
  exit 0
fi

if [ "$status" -eq 0 ]; then
  echo "smoke: PASS - app exited cleanly"
  exit 0
fi

echo "smoke: FAIL - app exited with status ${status}" >&2
exit "$status"
