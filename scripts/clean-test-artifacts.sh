#!/usr/bin/env bash
# Prune stale test executables from `target/debug/deps`.
#
# Why: every `cargo test --all-targets` run links a new hashed test binary
# (`<crate>-<16 hex>`) and cargo never deletes the old ones. Left alone the
# directory grows by hundreds of megabytes per run (ticket 34 measured 29 GB
# and 231 test binaries in `target/debug/deps`).
#
# Default: keep the newest executable per crate prefix, delete older ones.
# `--keep-file FILE`: keep exactly the executables listed in FILE (one path per
#   line, as `cargo test --message-format=json` reports them); delete the rest.
#   `scripts/test.sh` uses this so the lib/bin unit-test binaries of one crate
#   both survive. An empty FILE keeps nothing.
# `--all`: delete every test executable (artifact reset without `cargo clean`);
#   wins over `--keep-file`.
# `--incremental`: also remove `debug/incremental/` (cargo regenerates it).
# `--strip-debug`: run `strip --strip-debug` on the kept binaries (GNU strip
#   only; warns and skips elsewhere). Opt-in because it drops backtrace line
#   info; the dev profile now strips DWARF at link time (ticket 40).
# `--dry-run`: print what would be removed and delete nothing.
#
# Only depth-1 regular files with the executable bit whose name is
# `<crate>-<16 lowercase hex>` are considered. `.rlib`, `.rmeta`, `.d`, shared
# libraries and directories are never touched. This script never calls
# `cargo clean` and never deletes outside the resolved `debug/deps` and
# `debug/incremental` directories.
#
# Usage: scripts/clean-test-artifacts.sh [--all] [--keep-file FILE]
#                                       [--incremental] [--strip-debug]
#                                       [--dry-run]
#
# Target directory precedence: `TARGET_DIR`, else `CARGO_TARGET_DIR`, else
# `<repo>/target`. The env seam exists so the integration test can point the
# script at a temp tree; it is not a second configuration system.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
target="${TARGET_DIR:-${CARGO_TARGET_DIR:-$root/target}}"

mode="keep-newest"
keep_file=""
dry_run=0
incremental=0
strip_debug=0

usage() {
  cat <<'EOF'
Usage: scripts/clean-test-artifacts.sh [--all] [--keep-file FILE]
                                      [--incremental] [--strip-debug]
                                      [--dry-run]

  (default)       keep the newest hashed test executable per crate, remove older
  --keep-file F   keep exactly the executables listed in F (one path per line)
  --all           remove every hashed test executable (wins over --keep-file)
  --incremental   also remove debug/incremental/ (cargo regenerates it)
  --strip-debug   strip debug info from kept binaries (GNU strip only)
  --dry-run       print what would be removed, delete nothing
  --help          show this help

Target directory: $TARGET_DIR, else $CARGO_TARGET_DIR, else <repo>/target.
EOF
}

while [ $# -gt 0 ]; do
  case "$1" in
    --all) mode="all" ;;
    --keep-file)
      if [ $# -lt 2 ] || [ -z "$2" ]; then
        echo "clean-test-artifacts: --keep-file needs a file path" >&2
        exit 2
      fi
      keep_file="$2"
      shift
      ;;
    --incremental) incremental=1 ;;
    --strip-debug) strip_debug=1 ;;
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

if [ -n "$keep_file" ] && [ ! -r "$keep_file" ]; then
  echo "clean-test-artifacts: cannot read keep file '$keep_file'" >&2
  exit 2
fi

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

# Compare keep-set entries by basename: cargo reports absolute paths, and the
# target directory can be reached through a relative path or a symlink. Hashes
# make the basename unique within one deps directory.
keep_names=""
if [ -n "$keep_file" ]; then
  keep_names="$(sed -n 's|.*/||p' "$keep_file" | tr -d '\r')"
fi

in_keep_set() {
  [ -n "$keep_names" ] && printf '%s\n' "$keep_names" | grep -Fxq -- "$1"
}

considered=0
kept=0
removed=0
reclaimed=0
kept_paths=""

keep_it() {
  kept=$((kept + 1))
  kept_paths="$kept_paths$1"$'\n'
}

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

# Physical path so the printed summary and du agree with what was scanned.
target="$(cd "$target" && pwd -P)"
deps="$target/debug/deps"
incremental_dir="$target/debug/incremental"

if [ -d "$deps" ]; then
  # Emit "<prefix>\t<mtime>\t<path>" for candidate test executables, newest
  # first within each prefix (ties broken by path name for determinism).
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
    if [ "$mode" = "all" ]; then
      remove_file "$path"
    elif [ -n "$keep_file" ]; then
      if in_keep_set "$(basename "$path")"; then
        keep_it "$path"
      else
        remove_file "$path"
      fi
    elif [ "$prefix" = "$previous" ]; then
      remove_file "$path"
    else
      keep_it "$path"
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

stripped=0
if [ "$strip_debug" -eq 1 ] && [ -n "$kept_paths" ]; then
  strip_version=""
  if command -v strip >/dev/null 2>&1; then
    strip_version="$(strip --version 2>/dev/null || true)"
  fi
  case "$strip_version" in
    *GNU*)
      while IFS= read -r path; do
        [ -n "$path" ] || continue
        if [ "$dry_run" -eq 1 ]; then
          echo "would strip debug info: $path"
        else
          strip --strip-debug -- "$path"
          stripped=$((stripped + 1))
        fi
      done <<< "$kept_paths"
      ;;
    *)
      echo "clean-test-artifacts: warning: --strip-debug requires GNU strip; leaving binaries unstripped" >&2
      ;;
  esac
fi

verb="removed"
verb_reclaim="reclaimed"
if [ "$dry_run" -eq 1 ]; then
  verb="would remove"
  verb_reclaim="would reclaim"
fi

echo "clean-test-artifacts: considered $considered test binaries, kept $kept," \
  "$verb $removed, $verb_reclaim $(human "$reclaimed")"

if [ "$stripped" -gt 0 ]; then
  echo "clean-test-artifacts: stripped debug info from $stripped kept binaries"
fi

if [ -d "$deps" ] && command -v du >/dev/null 2>&1; then
  echo "clean-test-artifacts: $deps is now $(du -sh "$deps" | cut -f1)"
fi
