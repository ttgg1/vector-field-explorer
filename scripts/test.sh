#!/usr/bin/env bash
# Test entry point for developers and CI.
#
# Runs `cargo test --all-targets` with every forwarded argument, then prunes
# stale test executables via `scripts/clean-test-artifacts.sh` so repeated runs
# do not fill `target/debug/deps` (ticket 34). The test exit status is always
# preserved: cleanup never turns a passing run into a failure, and a failing
# run still cleans up and reports its own status.
#
# Usage: scripts/test.sh [cargo test arguments...]
#
#   --incremental   also remove `debug/incremental/` after the run
#   --help          show this help
#
# Environment:
#   KEEP_TEST_ARTIFACTS=1   skip cleanup entirely
#   CLEAN_INCREMENTAL=1     same as `--incremental`
#   TARGET_DIR=...          forwarded to the cleanup script

set -uo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

usage() {
  cat <<'EOF'
Usage: scripts/test.sh [cargo test arguments...]

Runs `cargo test --all-targets` with all forwarded arguments, then prunes stale
test executables from target/debug/deps (keeping the newest per crate).

  --incremental   also remove target/debug/incremental/ after the run
  --help          show this help

Environment:
  KEEP_TEST_ARTIFACTS=1   skip artifact cleanup
  CLEAN_INCREMENTAL=1     same as --incremental
  TARGET_DIR=...          target directory for cleanup (default <repo>/target)
EOF
}

incremental=0
cargo_args=()

while [ $# -gt 0 ]; do
  case "$1" in
    -h|--help) usage; exit 0 ;;
    --incremental) incremental=1 ;;
    *) cargo_args+=("$1") ;;
  esac
  shift
done

status=0
if [ ${#cargo_args[@]} -gt 0 ]; then
  cargo test --all-targets "${cargo_args[@]}" || status=$?
else
  cargo test --all-targets || status=$?
fi

if [ "${KEEP_TEST_ARTIFACTS:-0}" = "1" ]; then
  echo "test: KEEP_TEST_ARTIFACTS=1; leaving test artifacts in place"
else
  clean_args=()
  if [ "$incremental" -eq 1 ] || [ "${CLEAN_INCREMENTAL:-0}" = "1" ]; then
    clean_args+=(--incremental)
  fi
  # Cleanup failures are a warning only: the test run's status wins.
  if ! "$root/scripts/clean-test-artifacts.sh" ${clean_args[@]+"${clean_args[@]}"}; then
    echo "test: warning: artifact cleanup failed; test status unchanged" >&2
  fi
fi

exit "$status"
