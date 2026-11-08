#!/usr/bin/env bash
# Test entry point for developers and CI.
#
# Runs `cargo test --all-targets` with every forwarded argument, then prunes
# stale test executables via `scripts/clean-test-artifacts.sh` so repeated runs
# do not fill `target/debug/deps` (ticket 34). The test exit status is always
# preserved: cleanup never turns a passing run into a failure, and a failing
# run still cleans up and reports its own status.
#
# The prune uses the exact set of binaries this run produced: a
# `cargo test --all-targets --no-run --message-format=json` probe reports every
# test executable (including the lib and bin unit-test binaries of one crate,
# which share a crate prefix and would otherwise fight under the default
# keep-newest rule). The real test run then hits cargo's cache. The set is
# expanded with cargo's hardlink of the built binary into `deps`, which has the
# same `<crate>-<hash>` shape as a test binary. If the probe cannot produce a
# set, cleanup falls back to the standalone default rule.
#
# Cleanup is skipped with a warning when cargo's output is not under the
# directory this script would prune: a forwarded `--release`, `--profile`,
# `--target` or `--target-dir`, a `build.target-dir` in `.cargo/config.toml`
# that TARGET_DIR/CARGO_TARGET_DIR do not mirror, or a probe whose executables
# live elsewhere. Pruning the wrong tree could delete every hashed executable
# there. Probe paths and the cleanup directory are compared in canonical form,
# so a symlinked target directory or macOS `$TMPDIR` (`/var` vs `/private/var`)
# does not disable cleanup.
#
# Usage: scripts/test.sh [cargo test arguments...]
#
#   --incremental   also remove `debug/incremental/` after the run
#   --help          show this help
#
# Environment:
#   KEEP_TEST_ARTIFACTS=1   skip cleanup entirely (and the probe)
#   CLEAN_INCREMENTAL=1     same as `--incremental`
#   TARGET_DIR=...          target directory for cleanup (default
#                           $CARGO_TARGET_DIR, else <repo>/target)
#   CARGO=...               cargo binary to run (test seam; default `cargo`)

set -uo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cargo_bin="${CARGO:-cargo}"

usage() {
  cat <<'EOF'
Usage: scripts/test.sh [cargo test arguments...]

Runs `cargo test --all-targets` with all forwarded arguments, then prunes stale
test executables from target/debug/deps, keeping exactly the binaries this run
produced (cargo's JSON artifact list). Falls back to "newest per crate" if the
artifact probe fails.

  --incremental   also remove target/debug/incremental/ after the run
  --help          show this help

Environment:
  KEEP_TEST_ARTIFACTS=1   skip artifact cleanup
  CLEAN_INCREMENTAL=1     same as --incremental
  TARGET_DIR=...          target directory for cleanup (default
                          $CARGO_TARGET_DIR, else <repo>/target)
  CARGO=...               cargo binary to run (default `cargo`)
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

# One cargo invocation, with the forwarded arguments when there are any.
run_cargo_test() {
  if [ ${#cargo_args[@]} -gt 0 ]; then
    "$cargo_bin" test --all-targets "${cargo_args[@]}"
  else
    "$cargo_bin" test --all-targets
  fi
}

# Print the path of a file listing this run's test executables, or nothing when
# the probe fails (the real run will then report the build error itself).
collect_keep_set() {
  local json keep
  json="$(mktemp)" || return 0
  if "$cargo_bin" test --all-targets --no-run --message-format=json \
    ${cargo_args[@]+"${cargo_args[@]}"} >"$json" 2>/dev/null; then
    keep="$(mktemp)" || { rm -f "$json"; return 0; }
    grep -o '"executable":"[^"]*"' "$json" |
      sed 's/^"executable":"//; s/"$//' >"$keep" || true
    if [ ! -s "$keep" ]; then
      rm -f "$keep"
      keep=""
    fi
  else
    keep=""
  fi
  rm -f "$json"
  printf '%s' "$keep"
}

# Cargo also hardlinks the built binary (`target/debug/<name>`) into
# `target/debug/deps/<crate>-<hash>`, which has the same shape as a test
# executable. Expand the probe's list with those hardlinks so cleanup does not
# delete the app binary's deps copy and force a relink on every run.
expand_keep_set() {
  local raw="$1" out="$2" exe dir
  : >"$out"
  while IFS= read -r exe; do
    [ -n "$exe" ] || continue
    printf '%s\n' "$exe" >>"$out"
    dir="$(dirname "$exe")"
    if [ "$(basename "$dir")" != "deps" ] && [ -d "$dir/deps" ]; then
      find "$dir/deps" -maxdepth 1 -samefile "$exe" -print 2>/dev/null >>"$out" || true
    fi
  done <"$raw"
}

# The `debug` directory cleanup will prune, resolved like the cleanup script
# does: TARGET_DIR, else CARGO_TARGET_DIR, else <repo>/target.
cleanup_debug_dir() {
  local target="${TARGET_DIR:-${CARGO_TARGET_DIR:-$root/target}}"
  if [ -d "$target" ]; then
    target="$(cd "$target" && pwd -P)"
  fi
  printf '%s/debug' "$target"
}

# Resolve one path to its physical form without GNU-only `realpath`: canonicalize
# the parent directory and re-attach the basename. Returns non-zero when the
# parent directory no longer exists (a vanished probe entry).
canonicalize_path() {
  local path="$1" dir
  dir="$(dirname "$path")"
  if [ ! -d "$dir" ]; then
    return 1
  fi
  printf '%s/%s\n' "$(cd "$dir" && pwd -P)" "$(basename "$path")"
}

# Cargo can write somewhere else than the directory we would prune: `--release`
# (`target/release/deps`), `--profile <name>` (`target/<profile>/deps`),
# `--target <triple>` (`target/<triple>/debug/deps`), `--target-dir`, or a
# `build.target-dir` in `.cargo/config.toml` that `TARGET_DIR`/`CARGO_TARGET_DIR`
# do not mirror. Pruning the wrong directory could delete every hashed
# executable there, so print the first offender and let the caller skip cleanup.
#
# Both sides are compared in canonical form: cargo reports the paths it used, and
# on macOS `$TMPDIR` (`/var/folders/…`) or a symlinked target directory would
# otherwise never match the resolved cleanup directory and silently disable
# cleanup.
keep_set_in_target() {
  local raw="$1" expected exe resolved
  expected="$(cleanup_debug_dir)/"
  while IFS= read -r exe; do
    [ -n "$exe" ] || continue
    # A probe entry whose directory vanished cannot be under the target; it is
    # already gone, so skip it rather than failing the check.
    resolved="$(canonicalize_path "$exe")" || continue
    case "$resolved" in
      "$expected"*) : ;;
      *)
        printf '%s' "$exe"
        return 1
        ;;
    esac
  done <"$raw"
  return 0
}

# Flags that move cargo's output out of `<target>/debug`; checked even when the
# probe fails, because then there is no artifact list to compare against.
forwarded_layout_flag() {
  local arg
  for arg in ${cargo_args[@]+"${cargo_args[@]}"}; do
    [ "$arg" = "--" ] && break
    case "$arg" in
      --release|--profile|--profile=*|--target|--target=*|--target-dir|--target-dir=*)
        printf '%s' "$arg"
        return 0
        ;;
    esac
  done
  return 1
}

status=0
raw_keep=""
if [ "${KEEP_TEST_ARTIFACTS:-0}" != "1" ]; then
  raw_keep="$(collect_keep_set)"
fi

run_cargo_test || status=$?

if [ "${KEEP_TEST_ARTIFACTS:-0}" = "1" ]; then
  echo "test: KEEP_TEST_ARTIFACTS=1; leaving test artifacts in place"
else
  # Never prune a directory cargo did not write to: a forwarded layout flag, a
  # `build.target-dir` in `.cargo/config.toml`, or a TARGET_DIR /
  # CARGO_TARGET_DIR that does not mirror cargo's output would otherwise let
  # cleanup delete every hashed executable in the wrong tree.
  skip_cleanup=""
  layout_flag="$(forwarded_layout_flag)" || true
  if [ -n "$layout_flag" ]; then
    skip_cleanup="'$layout_flag' moves cargo's output out of $(cleanup_debug_dir)"
  elif [ -n "$raw_keep" ]; then
    mismatch="$(keep_set_in_target "$raw_keep")" || true
    if [ -n "$mismatch" ]; then
      skip_cleanup="cargo wrote test binaries under '$(dirname "$mismatch")', but cleanup targets '$(cleanup_debug_dir)'"
    fi
  fi

  if [ -n "$skip_cleanup" ]; then
    echo "test: warning: $skip_cleanup; skipping artifact cleanup" >&2
    rm -f "$raw_keep"
  else
    keep_file=""
    if [ -n "$raw_keep" ]; then
      # Expand after the run so the bin artifact exists even on a cold target.
      keep_file="$(mktemp)" || keep_file=""
      if [ -n "$keep_file" ]; then
        expand_keep_set "$raw_keep" "$keep_file"
        if [ ! -s "$keep_file" ]; then
          rm -f "$keep_file"
          keep_file=""
        fi
      fi
      rm -f "$raw_keep"
    fi

    clean_args=()
    if [ "$incremental" -eq 1 ] || [ "${CLEAN_INCREMENTAL:-0}" = "1" ]; then
      clean_args+=(--incremental)
    fi
    if [ -n "$keep_file" ]; then
      clean_args+=(--keep-file "$keep_file")
      echo "test: cleaning test artifacts, keeping the binaries from this run"
    fi
    # Cleanup failures are a warning only: the test run's status wins.
    if [ ${#clean_args[@]} -eq 0 ]; then
      if ! "$root/scripts/clean-test-artifacts.sh"; then
        echo "test: warning: artifact cleanup failed; test status unchanged" >&2
      fi
    else
      if ! "$root/scripts/clean-test-artifacts.sh" "${clean_args[@]}"; then
        echo "test: warning: artifact cleanup failed; test status unchanged" >&2
      fi
    fi
    if [ -n "$keep_file" ]; then
      rm -f "$keep_file"
    fi
  fi
fi

exit "$status"
