#!/usr/bin/env bash
# Guard the WebGPU-unavailable gate (ticket 54). The help panel must stay in
# index.html, the browser guidance and links must remain, and the wasm entry
# must check navigator.gpu / requestAdapter() before it constructs eframe's
# WebRunner, so the gate cannot be silently removed. This is a source-level
# check; the browser matrix itself is manual.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
index="$root/index.html"
entry="$root/src/main.rs"
helper="$root/web/webgpu.js"
style="$root/web/style.css"

fail() {
  echo "web-bootstrap: $*" >&2
  exit 1
}

require_file() {
  [[ -f "$1" ]] || fail "missing ${1#"$root"/}"
}

require_text() {
  local needle="$1" file="$2" why="$3"
  grep -qF -- "$needle" "$file" || fail "$why (missing '$needle' in ${file#"$root"/})"
}

for file in "$index" "$entry" "$helper" "$style"; do
  require_file "$file"
done

# Panel and its accessibility contract.
require_text 'id="webgpu-unsupported"' "$index" "the unsupported panel id is missing"
require_text 'role="alert"' "$index" "the unsupported panel must be an alert"
require_text 'id="webgpu-retry"' "$index" "the Retry button is missing"
require_text '>Retry<' "$index" "the Retry label is missing"
require_text 'id="webgpu-reason-code"' "$index" "the failure-reason code line is missing"
require_text '<noscript>' "$index" "the no-JavaScript fallback is missing"

# Browser guidance: the no-fallback sentence, every browser, and enable steps.
require_text 'no WebGL2 fallback' "$index" "the no-WebGL2 sentence is missing"
for browser in Firefox Chrome Edge Linux; do
  require_text "$browser" "$index" "the $browser guidance is missing"
done
require_text 'about:config' "$index" "the Firefox about:config step is missing"
require_text 'dom.webgpu.enabled' "$index" "the Firefox WebGPU pref is missing"
require_text 'chrome://flags/#enable-unsafe-webgpu' "$index" "the Chrome flag is missing"
require_text 'edge://flags/#enable-unsafe-webgpu' "$index" "the Edge flag is missing"
require_text '--enable-features=Vulkan' "$index" "the Linux Chromium Vulkan flag is missing"

# Links open in a new tab, and the native download is present.
require_text 'https://developer.mozilla.org/en-US/docs/Web/API/WebGPU_API' "$index" "the MDN link is missing"
require_text 'https://caniuse.com/webgpu' "$index" "the caniuse link is missing"
require_text 'href="download.html"' "$index" "the native download link is missing"
grep -qF 'target="_blank"' "$index" || fail "help links must open in a new tab"

# The helper is copied by Trunk and loaded; it must parse.
require_text 'href="web/webgpu.js"' "$index" "the Trunk copy-file for webgpu.js is missing"
require_text 'src="webgpu.js"' "$index" "the webgpu.js script tag is missing"
if command -v node >/dev/null 2>&1; then
  node --check "$helper" || fail "web/webgpu.js does not parse"
fi

# The stylesheet must hide the canvas and loading overlay while the panel shows.
require_text '#webgpu-unsupported' "$style" "the panel style is missing"
require_text 'body.webgpu-unsupported #vfe-canvas' "$style" "the canvas is not hidden with the panel"
require_text 'body.webgpu-unsupported #loading' "$style" "the loading overlay is not hidden with the panel"

# Retry depends on web/webgpu.js: the helper installs the click handler and
# updates the browser link. The Rust gate reveals the panel without it, but a
# blocked/missing helper leaves Retry inert; the copy-file and script tag above
# are therefore part of the contract. Do not add a second click path here.
#
# Rust gate ordering: match code-only tokens, never doc comments. The removed-
# call mutation test (see the ticket-54 report) is why the call itself is
# required, not just the presence of the helper body.
line_of_first() {
  awk -v needle="$1" 'index($0, needle) { print NR; exit }' "$2"
}

# Strip line comments so a comment that merely mentions a token cannot satisfy
# the gate. src/main.rs has no `//` inside string literals today.
code_source="$(mktemp)"
trap 'rm -f "$code_source"' EXIT
sed 's://.*$::' "$entry" >"$code_source"

start_line="$(line_of_first 'async fn start_app(' "$code_source")"
call_line="$(line_of_first 'ensure_webgpu(&window).await' "$code_source")"
gpu_line="$(line_of_first 'navigator().gpu()' "$code_source")"
adapter_line="$(line_of_first 'request_adapter_with_options(' "$code_source")"
runner_line="$(line_of_first 'WebRunner::new()' "$code_source")"

[[ -n "$start_line" ]] || fail "src/main.rs has no start_app"
[[ -n "$call_line" ]] || fail "start_app never calls ensure_webgpu(&window).await"
[[ -n "$gpu_line" ]] || fail "ensure_webgpu never calls navigator().gpu()"
[[ -n "$adapter_line" ]] || fail "ensure_webgpu never calls request_adapter_with_options()"
[[ -n "$runner_line" ]] || fail "src/main.rs never constructs WebRunner::new()"
(( start_line < call_line )) || fail "ensure_webgpu must be called inside start_app (start line $start_line, call line $call_line)"
(( call_line < runner_line )) || fail "the ensure_webgpu call must precede WebRunner::new() (line $call_line vs $runner_line)"
(( gpu_line < adapter_line )) || fail "navigator().gpu() must be read before request_adapter_with_options() (line $gpu_line vs $adapter_line)"
(( adapter_line < runner_line )) || fail "request_adapter_with_options() must precede WebRunner::new() (line $adapter_line vs $runner_line)"

echo "web-bootstrap: PASS - panel, browser guidance, links, and the pre-start adapter gate are present"
