#!/usr/bin/env bash
# Build the real browser target and require Trunk's HTML, wasm and JS output.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

if ! command -v rustup >/dev/null 2>&1; then
  echo "web: rustup is required to install the wasm target and select the matching toolchain" >&2
  exit 1
fi

# A system cargo ahead of rustup on PATH lacks the wasm32 std library, and
# Trunk spawns whatever `cargo` it inherits. Point PATH at the active rustup
# toolchain first so both use the toolchain that owns the target.
if ! toolchain_cargo="$(rustup which cargo 2>/dev/null)"; then
  echo "web: rustup has no active toolchain with cargo (run: rustup toolchain install stable)" >&2
  exit 1
fi
toolchain_bin="$(dirname "$toolchain_cargo")"
export PATH="$toolchain_bin:$PATH"

rustup target add wasm32-unknown-unknown

if ! command -v trunk >/dev/null 2>&1; then
  echo "web: trunk 0.22.1 is required (https://trunkrs.dev)" >&2
  exit 1
fi

cargo check --target wasm32-unknown-unknown --lib --bin vector-field-explorer
trunk build --release

test -f dist/index.html || { echo "web: missing dist/index.html" >&2; exit 1; }
shopt -s nullglob
wasm=(dist/*.wasm)
javascript=(dist/*.js)
((${#wasm[@]} > 0)) || { echo "web: missing dist/*.wasm" >&2; exit 1; }
((${#javascript[@]} > 0)) || { echo "web: missing dist/*.js" >&2; exit 1; }

# Ticket 68: the deployed page references these committed identity assets.
for asset in \
  dist/manifest.webmanifest \
  dist/og-image.png \
  dist/icons/favicon-16.png \
  dist/icons/favicon-32.png \
  dist/icons/apple-touch-icon.png \
  dist/icons/icon-192.png \
  dist/icons/icon-512.png; do
  test -f "$asset" || { echo "web: missing $asset" >&2; exit 1; }
done

echo "web: PASS - dist/index.html, ${wasm[0]}, ${javascript[0]}, manifest, OG image and icons"
