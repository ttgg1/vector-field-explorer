#!/usr/bin/env bash
# Bounded native present-path floor record (#18 task b).
#
# Runs the release binary in its measurement mode for the two documented floor
# scenes, at the forced 1280x800 logical inner size, and records only the
# machine-readable `PERF present` lines in
# docs/perf/baselines/<date>-present-<gpu>.txt (the suffix is derived from the
# reported GPU token). It never overwrites an existing same-date record. A
# window appears on the desktop for the bounded run duration; the app closes
# itself, and each invocation is additionally wrapped in `timeout` so a stuck
# build or GPU cannot hang the script.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
date_stamp="$(date +%Y-%m-%d)"
log_dir="${PERF_LOG_DIR:-/tmp/opencode}"
cargo_bin="${CARGO:-cargo}"
seconds="${VFE_PERF_SECONDS:-10}"

# The authoritative floor-record host and display adapter. Overridable so a
# future host can move the expectation without editing this script.
expected_host="${PERF_EXPECTED_HOST:-AMD_Ryzen_9_7950X_16-Core_Processor}"
expected_gpu="${PERF_EXPECTED_GPU:-AMD_Radeon_RX_9070_XT_(RADV_GFX1201)}"
scenes=(default-2d densest-3d)

# Turn the reported `gpu=` token into a stable, filesystem-safe suffix. Drop
# parenthetical backend noise and vendor/brand words that add no model
# identity, then keep lowercase alphanumerics:
#   AMD_Radeon_RX_9070_XT_(RADV_GFX1201) -> rx9070xt
#   not_probed                          -> notprobed
gpu_suffix() {
  local token="$1" cleaned word suffix=""
  cleaned="$(printf '%s' "$token" | tr '[:upper:]' '[:lower:]' | sed -E 's/\([^)]*\)//g')"
  for word in $(printf '%s\n' "$cleaned" | tr -cs 'a-z0-9' ' '); do
    case "$word" in
      amd|nvidia|intel|radeon|geforce) continue ;;
    esac
    suffix+="$word"
  done
  if [ -z "$suffix" ]; then
    suffix="unknown"
  fi
  printf '%s' "$suffix"
}

for arg in "$@"; do
  case "$arg" in
    -h|--help)
      cat <<'EOF'
Usage: scripts/bench-present-floor.sh

Builds the release binary, then runs the bounded present-path measurement for
`default-2d` and `densest-3d` at 1280x800 and writes only the two-line
`PERF present` records to docs/perf/baselines/<date>-present-<gpu>.txt, where
<gpu> is derived from the run's reported GPU token (e.g. `rx9070xt`).
The script refuses to overwrite an existing same-date file. Full stdout/stderr
logs stay in /tmp/opencode/present-floor-<date>/.

Environment: VFE_PERF_SECONDS (default 10, clamped 5..60 by the harness),
PERF_LOG_DIR overrides the local log directory and CARGO the cargo binary.
It is Linux-only, like the rest of scripts/.
EOF
      exit 0
      ;;
    *)
      echo "unknown argument: $arg (try --help)" >&2
      exit 2
      ;;
  esac
done

# Wall-clock timeout per scene: the harness deadline (warm-up + window + 5 s
# slack) plus generous compile/startup headroom. The script must not hang if
# the window never produces a frame.
limit=90
if [[ "$seconds" =~ ^[0-9]+([.][0-9]+)?$ ]]; then
  limit=$(( ${seconds%%.*} + 2 + 5 + 30 ))
fi

mkdir -p "$log_dir"
run_dir="$log_dir/present-floor-${date_stamp}"
mkdir -p "$run_dir"

cd "$root"
"$cargo_bin" build --release --bin vector-field-explorer

target_dir="${CARGO_TARGET_DIR:-$root/target}"
binary="$target_dir/release/vector-field-explorer"
if [ ! -x "$binary" ]; then
  echo "release binary not found at $binary; no baseline written" >&2
  exit 1
fi

tmp="$(mktemp "$run_dir/baseline.XXXXXX")"
trap 'rm -f "$tmp"' EXIT

for scene in "${scenes[@]}"; do
  scene_out="$run_dir/${scene}.txt"
  scene_log="$run_dir/${scene}.log"
  rm -f "$scene_out"
  echo "measuring $scene (${seconds}s window after 2s warm-up)" >&2
  status=0
  timeout --kill-after=5s "${limit}s" \
    env VFE_PERF_PRESENT=1 \
        VFE_PERF_SCENE="$scene" \
        VFE_PERF_SECONDS="$seconds" \
        VFE_PERF_OUT="$scene_out" \
        "$binary" >"$scene_log" 2>&1 || status=$?
  if [ "$status" -ne 0 ]; then
    echo "present run for $scene failed or timed out (exit $status); see $scene_log" >&2
    exit 1
  fi
  if [ ! -s "$scene_out" ]; then
    echo "no PERF data for $scene; see $scene_log" >&2
    exit 1
  fi
  if grep -q 'error=' "$scene_out"; then
    echo "present run for $scene reported an error; see $scene_out" >&2
    exit 1
  fi
  if ! grep -q 'p50_ms=' "$scene_out"; then
    echo "present run for $scene produced no measurement; see $scene_out" >&2
    exit 1
  fi
  grep -E '^PERF present ' "$scene_out" >>"$tmp"
  cat "$scene_out"
done

if [ ! -s "$tmp" ]; then
  echo "no PERF present lines were produced; no baseline written" >&2
  exit 1
fi

header="$(grep -E '^PERF present host=' "$tmp" | tail -n 1 || true)"
if [ -z "$header" ]; then
  echo "no PERF present host line was produced; no baseline written" >&2
  exit 1
fi
host_token="$(printf '%s\n' "$header" | sed -n 's/.* host=\([^ ]*\).*/\1/p')"
gpu_token="$(printf '%s\n' "$header" | sed -n 's/.* gpu=\([^ ]*\).*/\1/p')"
if [ "$host_token" != "$expected_host" ]; then
  echo "warning: reported host=$host_token does not match the expected host=$expected_host" >&2
fi
if [ "$gpu_token" != "$expected_gpu" ]; then
  echo "warning: reported gpu=$gpu_token does not match the expected gpu=$expected_gpu" >&2
fi

suffix="$(gpu_suffix "$gpu_token")"
out="$root/docs/perf/baselines/${date_stamp}-present-${suffix}.txt"
mkdir -p "$(dirname "$out")"
if [ -e "$out" ]; then
  echo "refusing to overwrite existing present baseline: $out" >&2
  exit 1
fi
mv "$tmp" "$out"
trap - EXIT
echo "present baseline written: $out"
echo "run logs: $run_dir"
