#!/usr/bin/env bash
# One maintained post-epic performance baseline record (#18 task a).
#
# Runs the release-profile `perf_baseline_test` through the shared suite, keeps
# the full local log under /tmp/opencode, and writes only the machine-readable,
# non-skip `PERF` lines to the tracked baseline file. The filename suffix is
# derived from the reported GPU token, so the name and the recorded hardware
# cannot disagree. It never overwrites an existing same-date record.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
date_stamp="$(date +%Y-%m-%d)"
log_dir="${PERF_LOG_DIR:-/tmp/opencode}"
log="$log_dir/perf-baseline-${date_stamp}.log"
cargo_bin="${CARGO:-cargo}"

# The authoritative baseline host recorded in docs/perf/README.md. Overridable
# so a future host can move the expectation without editing this script.
expected_host="${PERF_EXPECTED_HOST:-AMD_Ryzen_9_7950X_16-Core_Processor}"
expected_gpu="${PERF_EXPECTED_GPU:-AMD_Radeon_RX_9070_XT_(RADV_GFX1201)}"

gpu=0
for arg in "$@"; do
  case "$arg" in
    --gpu) gpu=1 ;;
    -h|--help)
      cat <<'EOF'
Usage: scripts/bench-perf-baseline.sh [--gpu]

Runs the release-profile performance baseline and records only the non-skip
`PERF` lines in docs/perf/baselines/<date>-<gpu>.txt, where <gpu> is derived
from the run's reported GPU token (e.g. `rx9070xt`). The full log stays in
/tmp/opencode/perf-baseline-<date>.log.

  --gpu   also time offscreen frames when a real wgpu adapter is available

Environment: VFE_PERF_BASELINE is set by this script; VFE_PERF_GPU is set only
with --gpu. PERF_LOG_DIR overrides the local log directory and CARGO the cargo
binary. The script refuses to overwrite an existing baselines file for the same
date. It is Linux-only, like the rest of scripts/.
EOF
      exit 0
      ;;
    *)
      echo "unknown argument: $arg (try --help)" >&2
      exit 2
      ;;
  esac
done

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

mkdir -p "$log_dir"

export VFE_PERF_BASELINE=1
if [ "$gpu" -eq 1 ]; then
  export VFE_PERF_GPU=1
else
  unset VFE_PERF_GPU
fi

cd "$root"
"$cargo_bin" test --release --test suite perf_baseline_test:: -- --nocapture 2>&1 |
  tee "$log"

header="$(grep -E '^PERF host=' "$log" | tail -n 1 || true)"
if [ -z "$header" ]; then
  echo "no PERF host line was produced; no baseline written" >&2
  exit 1
fi

host_token="$(printf '%s\n' "$header" | sed -n 's/.* host=\([^ ]*\).*/\1/p')"
gpu_token="$(printf '%s\n' "$header" | sed -n 's/.* gpu=\([^ ]*\).*/\1/p')"
if [ -z "$host_token" ]; then
  host_token="unknown"
fi
if [ -z "$gpu_token" ]; then
  gpu_token="unknown"
fi

if [ "$host_token" != "$expected_host" ]; then
  echo "warning: reported host=$host_token does not match the expected baseline host=$expected_host" >&2
fi
if [ "$gpu_token" != "$expected_gpu" ]; then
  echo "warning: reported gpu=$gpu_token does not match the expected baseline gpu=$expected_gpu; naming the record for the reported GPU" >&2
fi

suffix="$(gpu_suffix "$gpu_token")"
out="$root/docs/perf/baselines/${date_stamp}-${suffix}.txt"

if [ -e "$out" ]; then
  echo "refusing to overwrite existing baseline: $out" >&2
  exit 1
fi

mkdir -p "$(dirname "$out")"

# Skip notices stay in the local log: they describe a missing measurement
# rather than a number, and must never be mistaken for tracked baseline data.
skip_pattern='^PERF (gpu=skipped|[^ ]+ [^ ]+ skipped )'
skip_count="$(grep -cE "$skip_pattern" "$log" || true)"
grep -E '^PERF ' "$log" | grep -vE "$skip_pattern" >"$out.tmp" || true
if [ ! -s "$out.tmp" ]; then
  rm -f "$out.tmp"
  echo "no PERF data lines were produced; no baseline written" >&2
  exit 1
fi
mv "$out.tmp" "$out"

if [ "$skip_count" -gt 0 ]; then
  echo "excluded $skip_count GPU skip record(s) from the baseline; see $log" >&2
fi
echo "baseline written: $out"
echo "full log: $log"
