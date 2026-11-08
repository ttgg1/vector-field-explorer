#!/usr/bin/env bash
# Release guard: the tag must be `v` + the Cargo.toml [package] version.
#
# Usage: scripts/check-version.sh v0.1.0
#
# Prints the version without the leading `v` on success, so the workflow can
# reuse it for asset names. Exits non-zero with a one-line reason on mismatch.
#
# Why: a tag that does not match the crate version would publish archives
# whose names and Info.plist disagree with the binary, and cargo would keep
# reporting the old version in the app.

set -euo pipefail

tag="${1:-}"
if [ -z "$tag" ]; then
  echo "usage: scripts/check-version.sh <tag>" >&2
  exit 2
fi

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

# First `version = "..."` inside [package]. Workspace-inherited versions are
# not used by this crate; an empty read is reported as an error below.
version="$(
  awk '
    /^\[/ { in_package = ($0 == "[package]"); next }
    in_package && /^[[:space:]]*version[[:space:]]*=/ {
      line = $0
      sub(/^[^=]*=[[:space:]]*"/, "", line)
      sub(/".*$/, "", line)
      print line
      exit
    }
  ' Cargo.toml
)"

if [ -z "$version" ]; then
  echo "check-version: could not read [package] version from Cargo.toml" >&2
  exit 1
fi

expected="${tag#v}"
if [ "$expected" = "$tag" ]; then
  echo "check-version: tag '$tag' must start with 'v' (expected v$version)" >&2
  exit 1
fi
if [ "$expected" != "$version" ]; then
  echo "check-version: tag '$tag' does not match Cargo.toml version '$version' (expected v$version)" >&2
  exit 1
fi

printf '%s\n' "$version"
