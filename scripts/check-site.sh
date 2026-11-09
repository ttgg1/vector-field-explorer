#!/usr/bin/env bash
# Check the GitHub Pages artifact (ticket 16a): the Trunk build plus the static
# native-download page. Runs locally after `trunk build` and in
# `.github/workflows/pages.yml` before the artifact is uploaded.
#
# Usage: scripts/check-site.sh
#        DIST_DIR=/tmp/site scripts/check-site.sh   # check a copy
#
# DIST_DIR lets a negative check run against a throwaway copy instead of the
# working tree (remove a file or a required string, then expect a non-zero
# exit). The real artifact is always `dist/` in the repository root.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dist="${DIST_DIR:-$root/dist}"

fail() {
  echo "check-site: $*" >&2
  exit 1
}

require_file() {
  [[ -f "$1" ]] || fail "missing ${1#"$root"/}"
}

require_text() {
  grep -qF -- "$2" "$1" || fail "missing '$2' in ${1#"$root"/}"
}

require_pattern() {
  grep -qE -- "$2" "$1" || fail "missing pattern '$2' in ${1#"$root"/}"
}

reject_text() {
  local needle="$1" file="$2"
  if grep -qiF -- "$needle" "$file"; then
    fail "forbidden claim '$needle' in ${file#"$root"/}"
  fi
}

index="$dist/index.html"
download="$dist/download.html"
stylesheet="$dist/style.css"

# --- The Trunk build -------------------------------------------------------
require_file "$index"
require_file "$stylesheet"

shopt -s nullglob
wasm=("$dist"/*.wasm)
((${#wasm[@]} > 0)) || fail "dist has no Trunk .wasm output"

# webgpu.js and pointer_lock.js are Trunk copy-files, so they exist even when
# the Rust/JS build failed. The artifact needs a Trunk-generated script too.
javascript=()
copy_scripts=(pointer_lock.js webgpu.js)
for candidate in "$dist"/*.js; do
  name="$(basename "$candidate")"
  if [[ " ${copy_scripts[*]} " == *" $name "* ]]; then
    continue
  fi
  javascript+=("$candidate")
done
((${#javascript[@]} > 0)) || fail "dist has no Trunk-generated .js output"
css=("$dist"/style-*.css)
((${#css[@]} > 0)) || fail "dist has no Trunk-generated CSS output"

# Assets index.html and download.html reference, plus the committed identity
# set ticket 68 requires (see scripts/test-web.sh).
for asset in \
  manifest.webmanifest \
  og-image.png \
  pointer_lock.js \
  webgpu.js \
  icons/favicon-16.png \
  icons/favicon-32.png \
  icons/apple-touch-icon.png \
  icons/icon-192.png \
  icons/icon-512.png \
  screenshots/vector-field-explorer.png; do
  require_file "$dist/$asset"
done

# --- The native-download page ---------------------------------------------
require_file "$download"

# The latest-release link, the licence, and the way back to the browser app.
latest="https://github.com/ttgg1/vector-field-explorer/releases/latest"
require_text "$download" "$latest"
require_text "$download" "GPL-3.0-only"
require_text "$download" "Open the web app"
require_pattern "$download" 'href="\./"[^>]*>Open the web app'
require_text "$download" "WebGPU"

# One real screenshot of the app in the page body; the OG image is the hero.
require_pattern "$download" 'src="screenshots/vector-field-explorer\.png"'
require_pattern "$download" 'src="og-image\.png"'

# The site must never claim the app is native-only or that no browser works.
for file in "$index" "$download"; do
  for claim in \
    "does not run in a browser" \
    "does not run in the browser" \
    "no browser support" \
    "not supported in browsers" \
    "not available in the browser" \
    "native-only" \
    "native only"; do
    reject_text "$claim" "$file"
  done
done

# --- No external runtime dependencies -------------------------------------
for file in "$index" "$download"; do
  if grep -Eqi 'cdn\.|unpkg\.com|jsdelivr|fonts\.googleapis|fonts\.gstatic' "$file"; then
    fail "external CDN/font reference in ${file#"$root"/}"
  fi
done

# --- Local links and files ------------------------------------------------
# Every relative href/src in either page must resolve inside the artifact.
# `./` and `.` point at the browser app's index.html. Absolute and protocol
# links are the release page, the repository and issue tracker.
checked_links=0
for file in "$index" "$download"; do
  while IFS= read -r ref; do
    [ -n "$ref" ] || continue
    case "$ref" in
      http://* | https://* | mailto:* | data:* | \#*) continue ;;
    esac
    target="${ref%%#*}"
    target="${target%%\?*}"
    [ -n "$target" ] || continue
    if [[ "$target" == "./" || "$target" == "." ]]; then
      require_file "$index"
      checked_links=$((checked_links + 1))
      continue
    fi
    # Trunk rewrites the project-site base path to an absolute URL; it must
    # still name a file inside this artifact.
    if [[ "$target" == /vector-field-explorer/* ]]; then
      target="${target#/vector-field-explorer/}"
    elif [[ "$target" == /* ]]; then
      fail "absolute local reference '$ref' in ${file#"$root"/}"
    else
      target="${target#./}"
    fi
    require_file "$dist/$target"
    checked_links=$((checked_links + 1))
  done < <(grep -oE '(href|src)="[^"]+"' "$file" |
    sed -E 's/^(href|src)="//; s/"$//')
done

echo "check-site: PASS - ${#wasm[@]} wasm, ${#javascript[@]} js, index.html, style.css, download.html"
echo "check-site: PASS - latest release, GPL-3.0-only, web-app link, screenshots and ${checked_links} local references"
