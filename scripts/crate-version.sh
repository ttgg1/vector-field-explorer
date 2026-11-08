#!/usr/bin/env bash
# Print the crate version from Cargo.toml `[package] version` (ticket 42).
#
# One reader for the one source of truth: `scripts/check-version.sh` validates
# release tags against it, and `nightly.yml` uses it as the nightly base
# version. Workspace-inherited versions are not used by this crate; an empty
# read is an error.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

if [ ! -f Cargo.toml ]; then
  echo "crate-version: could not read Cargo.toml in $root" >&2
  exit 1
fi

# First `version = "..."` inside [package].
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
  echo "crate-version: could not read [package] version from Cargo.toml" >&2
  exit 1
fi

printf '%s\n' "$version"
