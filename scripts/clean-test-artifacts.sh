#!/usr/bin/env bash
# Prune stale test executables from `target/debug/deps`.
#
# Why: every `cargo test --all-targets` run links a new hashed test binary
# (`<crate>-<16 hex>`) and cargo never deletes the old ones. Left alone the
# directory grows by hundreds of megabytes per run (ticket 34 measured 29 GB
# and 231 test binaries in `target/debug/deps`).
#
# Default: keep the newest executable per crate prefix, delete older ones.
# `--all`: delete every test executable (artifact reset without `cargo clean`).
# `--incremental`: also remove `debug/incremental/` (cargo regenerates it).
# `--dry-run`: print what would be removed and delete nothing.
#
# Only depth-1 regular files with the executable bit whose name is
# `<crate>-<16 lowercase hex>` are considered. `.rlib`, `.rmeta`, `.d`, shared
# libraries and directories are never touched. This script never calls
# `cargo clean` and never deletes outside the resolved `debug/deps` and
# `debug/incremental` directories.
#
# Usage: scripts/clean-test-artifacts.sh [--all] [--incremental] [--dry-run]
#
# TARGET_DIR overrides the target directory (default `<repo>/target`); it
# exists so the integration test can point the script at a temp tree.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
target="${TARGET_DIR:-$root/target}"
deps="$target/debug/deps"
incremental_dir="$target/debug/incremental"

mode="keep-newest"
dry_run=0
incremental=0

usage() {
  cat <<'EOF'
Usage: scripts/clean-test-artifacts.sh [--all] [--incremental] [--dry-run]

  (default)     keep the newest hashed test executable per crate, remove older
  --all         remove every hashed test executable
  --incremental also remove debug/incremental/ (cargo regenerates it)
  --dry-run     print what would be removed, delete nothing
  --help        show this help

Environment: TARGET_DIR overrides the target directory (default <repo>/target).
EOF
}

while [ $# -gt 0 ]; do
  case "$1" in
    --all) mode="all" ;;
    --incremental) incremental=1 ;;
    --dry-run) dry_run=1 ;;
    -h|--help) usage; exit 0 ;;
    *)
      echo "clean-test-artifacts: unknown argument '$1'" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

# `stat` differs between GNU (Linux) and BSD (macOS); pick once.
if stat -c %Y . >/dev/null 2>&1; then
  mtime() { stat -c %Y -- "$1"; }
  size_of() { stat -c %s -- "$1"; }
else
  mtime() { stat -f %m -- "$1"; }
  size_of() { stat -f %z -- "$1"; }
fi

human() {
  awk -v bytes="$1" 'BEGIN {
    split("B KiB MiB GiB TiB", unit, " ");
    i = 1;
    while (bytes >= 1024 && i < 5) { bytes /= 1024; i++ }
    if (i == 1) printf "%d %s", bytes, unit[i];
    else printf "%.1f %s", bytes, unit[i];
  }'
}

considered=0
kept=0
removed=0
reclaimed=0

remove_file() {
  local path="$1" size
  # A concurrent cargo run (or a second cleanup) can remove the file between
  # listing and deletion; that is not an error, it is already gone.
  size="$(size_of "$path" 2>/dev/null || true)"
  if [ -z "$size" ]; then
    return 0
  fi
  if [ "$dry_run" -eq 1 ]; then
    echo "would remove: $path"
  else
    rm -f -- "$path"
  fi
  removed=$((removed + 1))
  reclaimed=$((reclaimed + size))
}

if [ ! -d "$target" ]; then
  echo "clean-test-artifacts: $target does not exist; nothing to do"
  exit 0
fi

if [ -d "$deps" ]; then
  # Emit "<prefix>\t<mtime>\t<path>" for candidate test executables, newest
  # first within each prefix (ties broken by path name for determinism), then
  # keep only the first entry per prefix.
  candidates="$(
    find "$deps" -mindepth 1 -maxdepth 1 -type f -print |
      while IFS= read -r file; do
        [ -x "$file" ] || continue
        base="$(basename "$file")"
        if [[ "$base" =~ ^([A-Za-z0-9_]+)-([0-9a-f]{16})$ ]]; then
          printf '%s\t%s\t%s\n' "${BASH_REMATCH[1]}" "$(mtime "$file")" "$file"
        fi
      done |
      LC_ALL=C sort -t $'\t' -k1,1 -k2,2nr -k3,3
  )"

  previous=""
  while IFS=$'\t' read -r prefix _mtime path; do
    [ -n "$prefix" ] || continue
    considered=$((considered + 1))
    if [ "$mode" = "all" ] || [ "$prefix" = "$previous" ]; then
      remove_file "$path"
    else
      kept=$((kept + 1))
      previous="$prefix"
    fi
  done <<< "$candidates"
else
  echo "clean-test-artifacts: $deps does not exist; no test binaries to prune"
fi

if [ "$incremental" -eq 1 ] && [ -d "$incremental_dir" ]; then
  incremental_bytes="$(du -sk "$incremental_dir" | awk '{ print $1 * 1024 }')"
  if [ "$dry_run" -eq 1 ]; then
    echo "would remove: $incremental_dir"
  else
    rm -rf -- "$incremental_dir"
  fi
  reclaimed=$((reclaimed + incremental_bytes))
fi

verb="removed"
verb_reclaim="reclaimed"
if [ "$dry_run" -eq 1 ]; then
  verb="would remove"
  verb_reclaim="would reclaim"
fi

echo "clean-test-artifacts: considered $considered test binaries, kept $kept," \
  "$verb $removed, $verb_reclaim $(human "$reclaimed")"

if [ -d "$deps" ] && command -v du >/dev/null 2>&1; then
  echo "clean-test-artifacts: $deps is now $(du -sh "$deps" | cut -f1)"
fi
