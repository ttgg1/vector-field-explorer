# Typeset CAS output spike — 2026-11-21

## Result

**Recommendation: adopt `katex-native` 0.1.1 with `katex-rs` 0.3.0 for a
follow-up implementation, behind the linear/typeset toggle and with linear
output retained as the fallback. Do not use `katex` 0.4.6 for this UI.**

The display-list path is viable: `katex-native` lays out real TeX into glyph
runs, rectangles, paths and lines in a coordinate system we can paint; it
builds for `wasm32-unknown-unknown`; its licences are compatible with
GPL-3.0-only; and a throwaway egui adapter converted and tessellated the list
without a browser. The costs and caveats are material and listed below: the
crate is very young (0.1.1, released 2026-10-30), the layout adds about
2.23 MB raw / 755 KB gzip to a minimal wasm harness, filled paths need
triangulation, and CAS linear strings need a separate linear-to-TeX display
printer before anything can be typeset.

This slice adds no production dependency, panel, settings schema, Cargo
manifest or lockfile change. It is evidence for an owner decision, not
adoption.

## Scope and method

Issue [#171](https://github.com/ttgg1/vector-field-explorer/issues/171) asks
for an optional typeset view of CAS results while copy continues to yield the
canonical linear/field-compatible string. The spike claim records BASE
`caca398` and authorises only a throwaway harness plus this tracked note.

Candidates:

- [`katex-rs` 0.3.0](https://crates.io/crates/katex-rs/0.3.0), a pure-Rust
  port of KaTeX that emits HTML and MathML.
- [`katex-native` 0.1.1](https://crates.io/crates/katex-native/0.1.1), native
  layout on top of `katex-rs` that returns a renderer-neutral display list.
- [`katex` 0.4.6](https://crates.io/crates/katex/0.4.6), Rust bindings that run
  the JavaScript KaTeX and return HTML.

The harness lived at `/tmp/opencode/katex-spike/` with throwaway Cargo
packages for layout/raster output, the `katex` bindings, wasm size, and the
egui adapter. Nothing under `/tmp/opencode/katex-spike/` is tracked.

The corpus was the ticket's representative set: fraction, root, definite
integral, mixed partial derivative, divergence, curl, 2x2 matrix and a long
rational/root/sum expression. The TeX forms rendered were:

```text
\frac{x^2+1}{x-3}
\sqrt{x^2+y^2}
\int_0^1 x^2\,dx
\frac{\partial^2 f}{\partial x\,\partial y}
\operatorname{div}\mathbf{F}
\operatorname{curl}\mathbf{F}
\begin{pmatrix} a & b \\ c & d \end{pmatrix}
\frac{x^{10}+3x^{9}-2x^{8}+7x^{7}-5x^{6}+11x^{5}-13x^{4}+17x^{3}-19x^{2}+23x-29}{x^{5}-3x^{4}+5x^{3}-7x^{2}+11x-13} + \sqrt{x^2+y^2} + \sum_{i=1}^{n} \alpha_i \beta_i
```

The raw CAS forms `(x^2+1)/(x-3)`, `sqrt(x^2+y^2)` and `2*x+1` were also fed
directly to `katex-native` to measure what happens without a display printer.

The eight expressions above are the measured corpus and are unchanged. They
do not include a CAS-emitted function call, so the follow-up acceptance corpus
(not measured evidence in this spike) must add at least one function-bearing
expression, for example `sin(x)*exp(-t)` displayed as `\sin(x)\,e^{-t}`. That
input uses only CAS-supported identifiers; `\alpha` cannot be CAS-emitted
because `src/cas.rs` accepts only variables `x`, `y`, `z`, `t` and the
constants `pi` and `e`. The token classes still untested are therefore
**function names** emitted by the CAS — `sin`, `cos`, `tan`, `asin`, `acos`,
`atan`, `sinh`, `cosh`, `tanh`, `asinh`, `acosh`, `atanh`, `sqrt`, `exp` and
`ln` — together with their argument grouping (`name(...)` to `\name{...}`) and
the implicit multiplication that a display printer would need between such a
call and a following factor.

## Candidate evidence

### `katex-rs` 0.3.0

- Primary metadata: crates.io reports `0.3.0` published 2026-09-05, licence
  MIT, repository <https://github.com/katex-rs/katex-rs>. The repository
  `Cargo.toml` sets `edition = "2024"` and licence MIT; the crate manifest
  declares no `rust-version`. The repository README says it tracks KaTeX
  0.18.5 at upstream commit `49904aa2`. GitHub showed 57 stars, 3 open issues
  and a last commit on 2026-10-10 (a dependency bump).
- API shape is a library plus settings builder:
  `KatexContext::default()`, `Settings::builder().display_mode(true).build()`,
  `render_to_string(&ctx, tex, &settings) -> Result<String, ParseError>`.
  `render_to_html_tree` is also public and is what `katex-native` consumes.
  Parsing errors are returned as `ParseError`, not panics. Source:
  <https://docs.rs/katex-rs/0.3.0/katex/>.
- wasm: default features are empty; the optional `wasm` feature adds
  `wasm-bindgen`/`web-sys` DOM helpers. The parsing and HTML-building core is
  pure Rust. A `cargo check --target wasm32-unknown-unknown` of the combined
  `katex-rs` + `katex-native` harness passed.
- Licence compliance note: the published `katex-rs-0.3.0.crate` tarball does
  not contain a `LICENSE` file even though the repository has one. If adopted,
  the project's third-party notice must carry the MIT text explicitly; relying
  on the crate archive is not sufficient. Evidence (fetched 2026-11-21):
  downloaded the archive from
  <https://crates.io/api/v1/crates/katex-rs/0.3.0/download> (sha256
  `2258545b8307484c0eb4dbe70d476a2bbe1febfcfc9defd5e92a580e8afe20c7`) and ran
  `tar -tzf katex-rs-0.3.0.crate`. The archive holds `Cargo.toml`,
  `Cargo.toml.orig`, `Cargo.lock`, `README.md`, `build.rs`, `benches/`,
  `data/` (`font_metrics_data.json`, `sigmas_and_xis.json`, `symbols.json`),
  `src/` and `tests/`. It contains **no** `LICENSE`, **no** `OFL.txt`, **no**
  font files and no `.github/workflows/`. The upstream repository does carry
  an MIT `LICENSE` at the crate's recorded commit `f9d9389`
  (<https://raw.githubusercontent.com/katex-rs/katex-rs/f9d93892c39424ba24cef0be18105a1629b66357/LICENSE>),
  so the text is obtainable, but not from the published archive.

### `katex-native` 0.1.1

- Primary metadata: crates.io reports `0.1.1` published 2026-10-30, licence
  `MIT AND OFL-1.1`, `rust-version = "1.88"`, `edition = "2024"`, repository
  <https://github.com/garlab/katex-native>. It depends on `katex-rs ^0.3.0`.
  GitHub showed 2 stars, no open issues, created 2026-10-30, last commit on
  2026-11-09. This is a very new, low-adoption 0.1.x crate.
- The crate applies KaTeX's CSS rules with KaTeX's font metrics and returns a
  display list. Source:
  <https://docs.rs/katex-native/0.1.1/katex_native/> and
  <https://github.com/garlab/katex-native/blob/main/src/lib.rs>. The public
  shape is small:

  ```rust
  let ctx = katex::KatexContext::default();
  let layout = katex_native::layout(&ctx, tex, display)?;
  // Layout { width, height, depth, items, tag, equation_numbers }
  // Item::Text { x, y, text, font, size, color }
  // Item::Rect { x, y, width, height, color }
  // Item::Path { commands, clip, color }
  // Item::Line { from, to, thickness, color }
  ```

  All dimensions are ems of the formula's font size, origin at the left end of
  the baseline, y down. `Font` names a KaTeX family plus bold/italic flags;
  `FONTS` embeds 20 KaTeX TTF faces (about 0.5 MB). `Error` is
  `Parse(ParseError)` or `Unsupported(String)`. `svg::to_svg` and the optional
  `raster` feature (resvg) are available but were not needed for the layout
  path.
- wasm: no target-specific dependencies. `cargo check --target
  wasm32-unknown-unknown --no-default-features` passed for the layout harness.
  The embedded fonts and metric tables are Rust data, so runtime needs no
  browser, JavaScript engine or DOM.
- Licence: MIT for code; the bundled fonts are OFL-1.1 with Reserved Font
  Names `KaTeX_*` (see `fonts/OFL.txt` in the published crate). The OFL FAQ
  explicitly allows bundling OFL fonts with software under another licence and
  says only the font portions remain OFL; the program itself does not become
  OFL ([OFL FAQ 1.2/1.3](https://openfontlicense.org/ofl-faq/)). Redistribution
  requires the copyright notice and licence text. Evidence (fetched
  2026-11-21): downloaded the archive from
  <https://crates.io/api/v1/crates/katex-native/0.1.1/download> (sha256
  `1ad88791e89f0098796b6b3e78dce2133e0dc1866911b9b6b24c768472893c55`) and ran
  `tar -tzf katex-native-0.1.1.crate`. The archive contains `LICENSE`,
  `fonts/OFL.txt` and 20 `fonts/KaTeX_*.ttf` faces (AMS, Caligraphic, Fraktur,
  Main, Math, SansSerif, Script, Size1–Size4 and Typewriter, in the regular,
  bold, italic or bold-italic variants KaTeX ships). It also ships
  `.github/workflows/ci.yml` and `release.yml`, so the licence files and the
  CI definition are both in the published archive.

### `katex` 0.4.6

- Primary metadata: crates.io reports `0.4.6` published 2023-02-05, licence
  `MIT OR Apache-2.0`, repository <https://github.com/xu-cheng/katex-rs>.
  GitHub showed 126 stars, 9 open issues, created 2020-02-01 and a last commit
  on 2023-05-27. `KATEX_VERSION` in the built harness is `0.16.4`; the KaTeX
  JavaScript payload is vendored.
- It is not a renderer in Rust: it embeds the JavaScript KaTeX and executes
  it in QuickJS or Duktape natively, or calls `js_sys::eval` in the browser
  (`wasm-js` feature). `render` returns an HTML string. Source:
  <https://docs.rs/katex/0.4.6/katex/> and
  <https://github.com/xu-cheng/katex-rs/blob/master/src/js_engine/wasm_js.rs>.
- Native QuickJS and wasm `wasm-js` both compiled in the harness. The wasm
  path still needs a JavaScript runtime at runtime (`js_sys::eval` and a
  global `katexRenderToString`); it is a browser binding, not a
  browser-free display-list renderer.
- HTML output sizes for the corpus (default `quick-js` backend) were:

  | Formula | HTML |
  |---|---:|
  | fraction | 1,966 B |
  | root | 2,685 B |
  | integral | 1,905 B |
  | partial | 2,317 B |
  | `div F` | 624 B |
  | `curl F` | 595 B |
  | matrix | 2,376 B |
  | long | 14,277 B |

  Since egui cannot paint HTML without a webview and the crate is stale (last
  release 2023), it is not a candidate for this panel.

## Comparison

| | `katex-rs` 0.3.0 | `katex-native` 0.1.1 | `katex` 0.4.6 |
|---|---|---|---|
| Released | 2026-09-05 | 2026-10-30 | 2023-02-05 |
| Licence | MIT | MIT AND OFL-1.1 | MIT OR Apache-2.0 |
| GPL-3.0-only compatible | yes | yes; fonts stay OFL | yes |
| Implementation | pure Rust | pure Rust layout over `katex-rs` | JavaScript KaTeX via QuickJS/Duktape/browser |
| Output | HTML + MathML string | display list (text, rect, path, line) | HTML string |
| Browser-free painting | no (HTML) | yes (display list/SVG; no JS) | no (HTML, JS runtime) |
| Native | yes | yes | yes, via embedded JS engine |
| wasm | yes, pure core | yes, verified `cargo check` | compiles, needs JS eval at runtime |
| MSRV | not declared; edition 2024 | declared and CI-tested 1.88 (`ci.yml` at `f8d5e32`) | not declared; edition 2021 |
| Maintenance signal | active, 57 stars | new, 2 stars, 0.1.1 | stale since 2023 |
| Verdict | keep as `katex-native` dependency | **adopt for follow-up** | reject for this UI |

## Harness evidence

### Fidelity and output size

`katex-native` release build, 32 px/em, display mode. Times are one run; the
long-formula timing was repeated 1,000 times. Display-list estimate sums the
`Item` storage plus text and path vector capacities; it excludes allocator
overhead.

| Formula | Result | Size (em) | Items | Display list | SVG | PNG | Layout |
|---|---|---:|---:|---:|---:|---:|---:|
| `\frac{x^2+1}{x-3}` | Ok | 2.93 × 1.49 (+0.77 depth) | 8 | 585 B | 1,319 B | 1,540 B | 0.023 ms |
| `\sqrt{x^2+y^2}` | Ok | 4.12 × 1.01 (+0.23) | 6 (1 path, 29 cmds) | 2,245 B | 3,479 B | 1,492 B | 0.046 ms |
| `\int_0^1 x^2\,dx` | Ok | 3.80 × 1.56 (+0.91) | 7 | 521 B | 1,252 B | 1,929 B | 0.046 ms |
| `\frac{\partial^2 f}{\partial x\,\partial y}` | Ok | 2.68 × 1.49 (+0.88) | 8 | 589 B | 1,360 B | 2,240 B | 0.043 ms |
| `\operatorname{div}\mathbf{F}` | Ok | 2.27 × 0.69 | 2 | 324 B | 415 B | 759 B | 0.025 ms |
| `\operatorname{curl}\mathbf{F}` | Ok | 2.56 × 0.69 | 2 | 325 B | 424 B | 764 B | 0.020 ms |
| 2x2 `pmatrix` | Ok | 3.52 × 1.45 (+0.95) | 6 | 454 B | 1,058 B | 2,135 B | 0.053 ms |
| long expression | Ok | 40.10 × 1.65 (+1.28) | 76 | 9,639 B | 14,983 B | 14,886 B | 0.219 ms |
| `\unknowncommand{x}` | **Err** | — | — | — | — | — | no panic |

`KatexContext::default()` took 0.17 ms. The long expression took p50 0.177 ms,
p95 0.188 ms and max 0.342 ms over 1,000 release-build layouts. This is small
relative to a frame and not a performance concern for a preview panel.

Fidelity was checked against the rasterised PNGs and the SVG output. The
fraction, root, integral with limits, mixed partial, `div F`, `curl F`, matrix
and long expression all rendered with correct structure, italics, nested
exponents, stretchy radical, integral limits, Greek and summation limits. The
matrix parentheses and the radical are drawn from KaTeX size fonts/paths, not
reconstructed from the linear text.

Passing the raw CAS strings produced the expected degradation, not typeset
math:

| Raw input | What `katex-native` drew | Width |
|---|---|---:|
| `(x^2+1)/(x-3)` | inline `(x² + 1)/(x − 3)`, not a stacked fraction | 7.04 em |
| `sqrt(x^2+y^2)` | italic letters `s q r t(x² + y²)`, not a radical | 5.69 em |
| `2*x+1` | `2 ∗ x + 1` with an asterisk | 3.74 em |

This is the key integration finding: `katex-native` typesets TeX, while
`src/cas.rs` emits meval-compatible linear text such as `2*x+1` and
`sqrt(x)`. A linear-to-TeX display printer (or a structured display AST) is
required before the toggle can show real fractions, roots and operators.

### Unsupported input and fallback behaviour

`\unknowncommand{x}` returned
`Err(Error::Parse(ParseError ... "Undefined control sequence"))` from
`katex-native` and the same error from `katex-rs::render_to_string`; the
harness observed no panic. A malformed or unsupported construct therefore has
a natural fail-closed path: keep the linear result, show a short
"typeset unavailable for this expression" note, and never leave a blank
result. The exact error text can be long and contains Unicode combining marks,
so it should be logged or summarised rather than shown verbatim.

### egui paintability

A throwaway egui 0.37 adapter converted every display-list item and forced
egui to tessellate the result:

| Formula | Display items | egui shapes | Tessellated primitives | Vertices |
|---|---:|---:|---:|---:|
| fraction | 8 | 8 | 1 | 36 |
| root | 6 | 7 | 2 | 474 |
| matrix | 6 | 6 | 1 | 24 |
| `div F` | 2 | 2 | 1 | 16 |

The mapping is direct:

- `Item::Text` → `egui::epaint::TextShape`, with a galley laid out in the named
  KaTeX face. `Glyph::font_ascent` is public in egui 0.37, so the galley's
  top-left can be shifted to put its baseline at the item's `y`; no guessed
  baseline offset is needed.
- `Item::Rect` → `Shape::rect_filled`.
- `Item::Line` → `Shape::LineSegment` with the given thickness.
- `Item::Path` → flattened cubic Béziers and `PathShape` per contour, clipped
  with `Painter::with_clip_rect`.
- `Color` → `Color32`; `None` uses the theme's text colour.

One caveat remains. `egui::epaint::PathShape` documents filled paths as
convex-only, while KaTeX radicals and other stretchy shapes can be concave or
multi-contour. The harness flattened and tessellated the root path without a
panic, but it did not visually validate the filled result. A production
adapter should triangulate non-convex fills (for example with a small
`lyon_tessellation` step) or rasterise paths; this is an implementation task,
not missing data. Text, rectangles and lines need no such workaround.

### wasm and size

A minimal cdylib harness that calls `KatexContext::default()` and
`katex_native::layout` built for `wasm32-unknown-unknown`:

| Harness | Raw wasm | gzip -9 |
|---|---:|---:|
| empty cdylib baseline | 386 B | 351 B |
| `katex-rs` 0.3.0 + `katex-native` 0.1.1 (layout only) | 2,231,247 B | 755,180 B |

The delta is about 2.23 MB raw / 755 KB gzip for this tiny wrapper; this is
dependency footprint, not the full app's release delta. The embedded 20 KaTeX
faces and the `katex-rs` metric/symbol tables are the bulk. Enabling the
optional `raster` (resvg) feature adds resvg on top and was deliberately not
part of the proposed path. No size budget was applied in this spike, but the
owner should see this number before adoption.

## Integration with the CAS panel and text measurement

The current panel is `src/ui/cas_panel.rs`: `CasPanelState::refresh` produces
`Preview { formula, kind, request }`; the result is a read-only multiline
`TextEdit` and `copy_text()` returns `preview.formula`. That canonical string
is the contract for Copy and Apply, and should not be replaced by typeset
output.

Recommended integration shape:

1. **Keep the linear string as the data model.** Add a persisted setting
   (`linear | typeset`, default linear) and render typeset output in a custom
   egui widget next to the existing linear editor. `copy_text()` still returns
   `preview.formula`; a separate "Copy linear" action can be added if the
   typeset canvas is not selectable.
2. **Measure from the layout, not from egui.** After `katex_native::layout`,
   multiply `Layout.width`, `Layout.height` and `Layout.depth` by the chosen
   point size. Allocate exactly
   `(width × size, (height + depth) × size)` and paint with the baseline at
   `height × size` from the top. Do not use `ui.fonts(...).layout_no_wrap` to
   derive the block width: egui's shaping is for painting individual runs, and
   the display list already carries the authoritative horizontal positions.
3. **Register each KaTeX face as a named egui family.** `FontData` can take
   each `FontFile::data`; use one `FontFamily::Name` key per
   family/bold/italic combination, because egui's `FontId` has no weight or
   style selector. The app's existing `src/fonts.rs` builds
   `FontDefinitions::empty`; add the KaTeX faces there or in a CAS-specific
   installer. Keep DejaVu as the UI face. The 20 faces are OFL and add about
   0.5 MB.
4. **Add a display printer from CAS linear text to TeX.** Parse the existing
   meval-compatible output (or reuse a structured CAS result if one becomes
   available) and emit TeX: `/` to `\frac`, `sqrt(...)` to `\sqrt{...}`,
   `^` to superscripts, `*` to juxtaposition, `pi`/`e` to Greek/constants,
   and operation wrappers such as `\operatorname{div}\mathbf{F}` for the
   normalised echo. This is a pure display concern and must not change the
   canonical string, grammar or evaluation.
5. **Fallback honestly.** If the display printer rejects a construct, or
   `layout` returns `Parse`/`Unsupported`, or the path tessellator fails,
   render the existing linear view and a one-line explanation. Do not show a
   partial formula or a typeset view that silently omits a term. Copy remains
   available in every case.
6. **Accessibility.** `katex-native` lays out glyphs but does not expose
   MathML. Keep a selectable/accessible linear representation (or generate
   MathML separately with `katex-rs`) so screen readers are not limited to a
   painted canvas.

## Risks and what this spike did not verify

- **Crate maturity.** `katex-native` is 0.1.1, released 22 days before this
  note, with two GitHub stars and a single visible maintainer. Its API is small
  and it pins `katex-rs` 0.3.0, but a 0.2 release can break the types. Pin the
  exact version and treat the adapter as an isolated module.
- **MSRV.** `katex-native` declares and CI-tests Rust 1.88: `Cargo.toml` has
  `rust-version = "1.88"`, and the `msrv` job in
  [`.github/workflows/ci.yml` at `f8d5e32`](https://github.com/garlab/katex-native/blob/f8d5e32b112d51d8d461f19f62c1b5189a0f06a6/.github/workflows/ci.yml)
  pins `toolchain: '1.88'` and runs
  `cargo check --all-features --all-targets --locked`. The same workflow file
  ships in the published archive at `.github/workflows/ci.yml`. This host has
  1.99.1 and no 1.88 toolchain installed, so the MSRV was not independently
  compiled here. `katex-rs` declares no `rust-version`; edition 2024 is its
  practical floor.
- **wasm runtime.** Only compilation was checked. No wasm runtime or browser
  was executed, and the repository's visual/browser verification is currently
  owner-waived.
- **egui path fill.** The adapter compiled and tessellated, but the filled
  non-convex path was not visually checked. `PathShape`'s convex-only
  limitation is documented; implementation needs triangulation or rasterisation.
- **Display printer.** No CAS-linear-to-TeX printer was written or tested. The
  raw-input evidence shows the typeset view is not useful without one.
- **Size.** The measured 2.23 MB / 755 KB is a minimal harness delta, not the
  app's actual wasm artifact delta. No budget was set.
- **Licences.** MIT and Apache-2.0 are GPL-3.0-compatible; OFL fonts remain
  OFL and may be bundled, but the follow-up must add the MIT and OFL notices
  to `THIRD_PARTY_LICENSES.md`. The `katex-rs` crate archive's missing
  `LICENSE` file must be handled explicitly.
- **Visual and UX details.** Dark-theme colours, DPI changes, clipping,
  selection/copy, focus, panel width, wrapping and screen-reader behaviour
  were not tested. Long formulas (40 em) will need clipping or horizontal
  scrolling in the narrow panel.
- **Robustness.** Only one unsupported expression was tested; no broad
  malformed-TeX corpus or fuzz pass was run. `katex-native` has a large
  upstream KaTeX test corpus, but that is not a substitute for our own
  acceptance corpus.

## Reproduction record

Host: Linux, rustc 1.99.1 (2026-10-13), cargo 1.99.1, rustup stable;
`wasm32-unknown-unknown` installed. All Cargo targets were isolated under
`/home/paul/.cache/vector-field-explorer-targets/`. The crates.io and GitHub
observations, including the crate archive downloads and the workflow/`LICENSE`
fetches cited above, are dated 2026-11-21.

- Metadata: `curl -s https://crates.io/api/v1/crates/<crate>` for
  `katex-rs`, `katex-native` and `katex`; GitHub REST repository and commit
  endpoints; raw `Cargo.toml`, README, `src/lib.rs`, `src/fonts.rs`,
  `src/svg.rs`, `src/path.rs`, `src/raster.rs` and JS-engine sources.
- Layout/raster:
  `cargo run --release` in `/tmp/opencode/katex-spike/harness-native`
  (dependencies `katex-rs = "0.3.0"`, `katex-native = "0.1.1"` with
  `raster`, `png = "0.18"`). Output in
  `/tmp/opencode/katex-spike/out/`; SVG and PNG inspected.
- Bindings: `cargo run --release` in
  `/tmp/opencode/katex-spike/harness-bindings` (`katex = "0.4.6"`, default
  `quick-js`).
- wasm layout check:
  `cargo check --release --target wasm32-unknown-unknown --no-default-features`
  in `harness-native`.
- wasm bindings check:
  `cargo check --release --target wasm32-unknown-unknown --no-default-features
  --features wasm-js` in `harness-bindings`.
- Size: `cargo build --release --lib --target wasm32-unknown-unknown` for
  `/tmp/opencode/katex-spike/size-baseline` and `size-katex`; `stat -c %s` and
  `gzip -9 -c | wc -c`.
- egui adapter: `cargo run --release` in
  `/tmp/opencode/katex-spike/harness-egui` (`egui = "0.37"`), using
  `Context::run_ui` and `Context::tessellate`.
- Repository gates: `python3 scripts/check-docs.py`; `git diff --check`.

## Sources

- Issue #171 and claim:
  <https://github.com/ttgg1/vector-field-explorer/issues/171>,
  <https://github.com/ttgg1/vector-field-explorer/issues/171#issuecomment-7064790745>.
- `katex-rs` crate metadata: <https://crates.io/crates/katex-rs> and
  <https://crates.io/api/v1/crates/katex-rs>; API
  <https://docs.rs/katex-rs/0.3.0/katex/>; repository
  <https://github.com/katex-rs/katex-rs>.
- `katex-native` crate metadata: <https://crates.io/crates/katex-native> and
  <https://crates.io/api/v1/crates/katex-native>; API
  <https://docs.rs/katex-native/0.1.1/katex_native/>; repository
  <https://github.com/garlab/katex-native>.
- `katex` crate metadata: <https://crates.io/crates/katex> and
  <https://crates.io/api/v1/crates/katex>; API
  <https://docs.rs/katex/0.4.6/katex/>; repository
  <https://github.com/xu-cheng/katex-rs>.
- OFL official text and FAQ:
  <https://openfontlicense.org/open-font-license-official-text/>,
  <https://openfontlicense.org/ofl-faq/>.
- Local UI context: `src/ui/cas_panel.rs`, `src/cas.rs`, `src/fonts.rs`.
