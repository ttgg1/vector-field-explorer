#!/usr/bin/env bash
# Nightly build version (ticket 42).
#
# Usage: scripts/nightly-version.sh <base-version> <YYYYMMDD> <sha>
#
# Prints `<base>-nightly.<YYYYMMDD>+<short-sha>`, the version stamped into a
# nightly build through `VFE_BUILD_VERSION` and into every nightly asset name,
# for example:
#
#   0.1.0-nightly.20261109+1a2b3c4
#
# Pure by design: the caller supplies the date and the commit, so the format is
# testable without a clock or a git checkout. The short sha is the first seven
# hex characters, lowercased. A base that already carries a prerelease
# (`0.1.0-rc.1`) keeps it in front of the nightly marker.

set -euo pipefail

usage() {
  echo "usage: scripts/nightly-version.sh <base-version> <YYYYMMDD> <sha>" >&2
  exit 2
}

[ "$#" -eq 3 ] || usage

base="$1"
date="$2"
sha="$3"

fail() {
  echo "nightly-version: $*" >&2
  exit 1
}

if [ -z "$base" ]; then
  fail "base version is required"
fi
if ! [[ "$base" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]]; then
  fail "base version '$base' is not semver (expected e.g. 0.1.0 or 0.1.0-rc.1)"
fi

if ! [[ "$date" =~ ^[0-9]{8}$ ]]; then
  fail "date '$date' must be YYYYMMDD"
fi
month=$((10#${date:4:2}))
day=$((10#${date:6:2}))
if [ "$month" -lt 1 ] || [ "$month" -gt 12 ] || [ "$day" -lt 1 ] || [ "$day" -gt 31 ]; then
  fail "date '$date' is not a calendar date"
fi

if ! [[ "$sha" =~ ^[0-9a-fA-F]{7,40}$ ]]; then
  fail "sha '$sha' must be 7-40 hex characters"
fi

short="$(printf '%s' "$sha" | cut -c1-7 | tr '[:upper:]' '[:lower:]')"
printf '%s-nightly.%s+%s\n' "$base" "$date" "$short"
