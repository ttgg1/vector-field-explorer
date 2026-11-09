#!/usr/bin/env bash
# Render the one canonical mark into every committed PNG size (ticket 68).
#
# Outputs:
#   packaging/icons/vector-field-explorer-{16,32,48,64,128,256,512}.png
#   packaging/linux/vector-field-explorer-256.png   (stable linuxdeploy path)
#   web/icons/favicon-16.png, favicon-32.png, apple-touch-icon.png,
#   web/icons/icon-192.png, icon-512.png
#
# Requirements: rsvg-convert (preferred) or inkscape. Generated files are
# committed; CI never runs this script. The exact commands are recorded in
# packaging/ARTWORK.md.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

svg="packaging/linux/vector-field-explorer.svg"
if [[ ! -f "$svg" ]]; then
  echo "gen-icons: missing canonical SVG: $svg" >&2
  exit 1
fi

if command -v rsvg-convert >/dev/null 2>&1; then
  renderer="rsvg-convert"
elif command -v inkscape >/dev/null 2>&1; then
  renderer="inkscape"
else
  echo "gen-icons: install rsvg-convert or inkscape to render $svg" >&2
  exit 1
fi

mkdir -p packaging/icons web/icons

render() {
  local size="$1"
  local out="$2"
  local background="${3:-}"
  mkdir -p "$(dirname "$out")"
  if [[ "$renderer" == "rsvg-convert" ]]; then
    if [[ -n "$background" ]]; then
      rsvg-convert --width "$size" --height "$size" \
        --background-color "$background" --output "$out" "$svg"
    else
      rsvg-convert --width "$size" --height "$size" --output "$out" "$svg"
    fi
  else
    if [[ -n "$background" ]]; then
      inkscape "$svg" --export-type=png --export-filename="$out" \
        --export-width="$size" --export-height="$size" \
        --export-background="$background" --export-background-opacity=255 >/dev/null
    else
      inkscape "$svg" --export-type=png --export-filename="$out" \
        --export-width="$size" --export-height="$size" >/dev/null
    fi
  fi
  echo "gen-icons: $renderer -> $out (${size}x${size})${background:+ on $background}"
}

for size in 16 32 48 64 128 256 512; do
  render "$size" "packaging/icons/vector-field-explorer-${size}.png"
done

# linuxdeploy keeps its stable 256 path; keep it byte-identical to the set.
cp packaging/icons/vector-field-explorer-256.png \
  packaging/linux/vector-field-explorer-256.png
echo "gen-icons: copied the 256 px mark to packaging/linux/"

# Browser sizes: 16/32/512 are the packaged renders; 180 and 192 are web-only.
cp packaging/icons/vector-field-explorer-16.png web/icons/favicon-16.png
cp packaging/icons/vector-field-explorer-32.png web/icons/favicon-32.png
cp packaging/icons/vector-field-explorer-512.png web/icons/icon-512.png
# iOS masks the touch icon on the home screen, so a transparent corner would
# become an unpredictable colour: render it on the brand page chrome.
render 180 web/icons/apple-touch-icon.png "#101a26"
render 192 web/icons/icon-192.png

echo "gen-icons: done ($renderer)"
