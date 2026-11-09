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
#
# The "does not run in browsers" guard is best-effort literal matching, not
# natural-language detection: it catches the recorded phrasings but cannot
# prove the absence of every possible claim.
#
# HTML is normalized (`tr '\n' ' '`) and inspected tag by tag, not line by
# line, so attributes split across lines, single-quoted or unquoted values,
# uppercase names and protocol-relative URLs are all visible. Inline
# `<script>`/`<style>` bodies are scanned conservatively: a literal `https?:`,
# `@import`, `import(` or `//host.tld` fails even inside a comment, because
# this check is a release gate, not a JavaScript parser.
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
# (Best-effort literal matching; see the header.)
for file in "$index" "$download"; do
  for claim in \
    "does not run in a browser" \
    "does not run in the browser" \
    "doesn't run in a browser" \
    "won't run in a browser" \
    "does not work in a browser" \
    "not a browser app" \
    "no browser support" \
    "not supported in browsers" \
    "not available in the browser" \
    "native-only" \
    "native only"; do
    reject_text "$claim" "$file"
  done
done

# --- HTML tag and attribute parsing ---------------------------------------
# A value is external when it is absolute or protocol-relative, whatever its
# case. `//host` is external in HTML just like `https://host`.
is_external() {
  local value="${1,,}"
  case "$value" in
    https://* | http://* | //*) return 0 ;;
  esac
  return 1
}

# Print every value of `attr` from one normalized tag. Quote-agnostic and
# case-insensitive; an unquoted value ends only at whitespace or `>`, so URLs
# keep their slashes.
attr_values() {
  local tag="$1" attr="$2" value
  while IFS= read -r value; do
    value="${value#\"}"
    value="${value%\"}"
    value="${value#\'}"
    value="${value%\'}"
    printf '%s\n' "$value"
  done < <(printf '%s\n' "$tag" |
    grep -oiE "(^|[[:space:]])${attr}[[:space:]]*=[[:space:]]*(\"[^\"]*\"|'[^']*'|[^[:space:]>]+)" |
    sed -E 's/^[^=]*=[[:space:]]*//' || true)
}

# The lower-case element name of a normalized tag.
tag_name() {
  local name="${1#<}"
  name="${name%%[[:space:]/>]*}"
  printf '%s' "${name,,}"
}

# Print the URL candidates of a srcset value: comma-separated entries with an
# optional density/width descriptor (`x.png 1x`), which is not a URL.
srcset_urls() {
  local value="$1" part
  local -a parts
  IFS=',' read -r -a parts <<< "$value"
  for part in "${parts[@]}"; do
    part="${part#"${part%%[![:space:]]*}"}"
    part="${part%%[[:space:]]*}"
    if [ -n "$part" ]; then
      printf '%s\n' "$part"
    fi
  done
}

# Whether a `<link>` tag declares the canonical URL (the one external href we
# allow: it names this site, it does not fetch anything).
tag_has_canonical_rel() {
  attr_values "$1" rel |
    tr '[:upper:]' '[:lower:]' |
    grep -qE '(^|[[:space:]])canonical([[:space:]]|$)' || return 1
}

# Reject external references inside inline <script>/<style> bodies. The block
# extractor stops at the next `<`, so a body containing markup is not scanned;
# that is the documented conservative limit, together with the comment
# false-positive note in the header.
scan_inline_blocks() {
  local file="$1" normalized="$2" block body
  while IFS= read -r block; do
    [ -n "$block" ] || continue
    body="$(printf '%s\n' "$block" | sed -E 's#^<[^>]*>##; s#</[^>]*>$##')"
    if printf '%s\n' "$body" |
      grep -Eqi 'https?:|@import|import[[:space:]]*\(|//[a-z0-9.-]+\.[a-z]{2,}'; then
      fail "external reference in inline <script>/<style> in ${file#"$root"/}"
    fi
  done < <(grep -oiE '<(script|style)[^>]*>[^<]*</(script|style)>' "$normalized" || true)
}

# --- No external runtime dependencies -------------------------------------
# Positive checks, not a CDN-host deny-list: every fetched asset must stay in
# this artifact. Only `<a href>` may point off-site (release, repository,
# issues), and only `<link rel="canonical">` may declare an external URL.
normalized="$(mktemp)"
trap 'rm -f "$normalized"' EXIT
checked_links=0

for file in "$index" "$download"; do
  tr '\n' ' ' < "$file" > "$normalized"

  # A base tag rewrites every relative URL on the page; there is no legitimate
  # use in this two-page artifact.
  if grep -qiE '<base([[:space:]][^>]*)?>' "$normalized"; then
    fail "<base> tag in ${file#"$root"/}: it rewrites every relative URL"
  fi

  scan_inline_blocks "$file" "$normalized"

  while IFS= read -r tag; do
    [ -n "$tag" ] || continue
    name="$(tag_name "$tag")"

    # Fetched resources never leave the artifact, including srcset candidates.
    for attr in src data srcset poster; do
      while IFS= read -r value; do
        [ -n "$value" ] || continue
        if [[ "$attr" == srcset ]]; then
          for candidate in $(srcset_urls "$value"); do
            if is_external "$candidate"; then
              fail "external $attr on <$name> in ${file#"$root"/}: $candidate"
            fi
          done
        elif is_external "$value"; then
          fail "external $attr on <$name> in ${file#"$root"/}: $value"
        fi
      done < <(attr_values "$tag" "$attr")
    done

    # Anchors may link off-site; canonical links may name the site. Any other
    # external href (stylesheet, icon, object, ...) must stay local.
    while IFS= read -r value; do
      [ -n "$value" ] || continue
      if is_external "$value"; then
        if [[ "$name" == a ]]; then
          continue
        fi
        if [[ "$name" == link ]] && tag_has_canonical_rel "$tag"; then
          continue
        fi
        fail "external href on <$name> in ${file#"$root"/}: $value"
      fi
    done < <(attr_values "$tag" href)

    # Local-file resolution for every attribute a browser may fetch or follow.
    for attr in href src data poster srcset; do
      while IFS= read -r value; do
        [ -n "$value" ] || continue
        candidates=("$value")
        if [[ "$attr" == srcset ]]; then
          candidates=()
          while IFS= read -r candidate; do
            candidates+=("$candidate")
          done < <(srcset_urls "$value")
        fi
        for ref in "${candidates[@]}"; do
          [ -n "$ref" ] || continue
          low="${ref,,}"
          case "$low" in
            http://* | https://* | //* | mailto:* | data:* | tel:* | javascript:* | \#*)
              continue
              ;;
          esac
          target="${ref%%#*}"
          target="${target%%\?*}"
          # Trunk rewrites the project-site base path to an absolute URL. The
          # base itself (with or without a trailing slash, or `/./`) is
          # index.html; anything else must name a file in the artifact.
          case "$target" in
            /vector-field-explorer | /vector-field-explorer/ | /vector-field-explorer/./)
              target=""
              ;;
            /vector-field-explorer/*)
              target="${target#/vector-field-explorer/}"
              target="${target#./}"
              ;;
            /*)
              fail "absolute local reference '$ref' in ${file#"$root"/}"
              ;;
            *)
              target="${target#./}"
              ;;
          esac
          if [[ "/$target/" == *"/../"* || "$target" == ".." ]]; then
            fail "path traversal in local reference '$ref' in ${file#"$root"/}"
          fi
          if [[ -z "$target" || "$target" == "." ]]; then
            require_file "$index"
            checked_links=$((checked_links + 1))
            continue
          fi
          require_file "$dist/$target"
          checked_links=$((checked_links + 1))
        done
      done < <(attr_values "$tag" "$attr")
    done
  done < <(grep -oiE '<[a-z][^>]*>' "$normalized")
done

# CSS must not fetch anything at runtime or import another sheet.
shopt -s nullglob
for sheet in "$dist"/*.css; do
  if grep -Eqi 'url\(\s*["'"'"']?(https?:)?//|@import' "$sheet"; then
    fail "external CSS dependency in ${sheet#"$root"/}"
  fi
done

echo "check-site: PASS - ${#wasm[@]} wasm, ${#javascript[@]} js, index.html, style.css, download.html"
echo "check-site: PASS - latest release, GPL-3.0-only, web-app link, screenshots and ${checked_links} local references"
