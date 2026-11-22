# Issue #23 reconciliation — parked debt inventory

**Prepared:** 2026-11-22
**Task base:** `83aacec`; merged current `main` before this record: `258ce43`
**Branch:** `docs/issue-23-reconciliation`
**Scope:** documentation reconciliation only. No source, behaviour, test, release,
tag, visibility or settings change is included.

## 1. Purpose and evidence limits

Issue #23 is the single retained home for the historical whole-codebase review
and the earlier scoped-review residuals. Inventory inclusion is **not**
verification that a defect still exists, not readiness, not priority and not
implementation authorization. This record compares the retained inventory with
current source, tests and merged PRs and records one disposition per origin. It
does not fix, refactor, delete or reopen anything.

Evidence limits that remain in force:

- Dedicated browser, manual visual and painted-ink checks are **waived, not
  passed**. Nothing here turns a waived check into a passed one.
- Windows GUI first-launch evidence is absent. Source-only Windows branches are
  not runtime evidence.
- The historical native CAS 16/16 mutation-killed claim remains
  implementer-reported; raw logs were not independently authenticated.
- Performance floors are measured on the RX 9070 XT only. Integrated-GPU and
  browser floors remain unmeasured and are not claimed. See
  `docs/perf/p1-p6-disposition.md`.
- Some original scoped-review artifacts are unavailable in the current tree.
  Where a detail could not be re-read, this record says so and uses `Deferred`
  or `Evidence limit` rather than inventing closure.
- The archived line numbers in issue #23 are provenance only; they are not
  current source locations.

`Confirmed` below means current source still shows the retained concern. It does
not mean the concern is a release blocker. `Closed` and `Superseded` require
current source, test or merged-PR evidence. `Evidence limit` marks an unrun or
owner-waived check, not a code defect.

## 2. Disposition summary

| Group | Rows | Closed | Superseded | Deferred | Confirmed | Evidence limit |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Whole-codebase review (C-1, I4–I13, M1–M34) | 41 | 1 | 1 | 9 | 30 | 0 |
| Scoped origins 01–71 follow-up | 39 | 6 | 2 | 21 | 3 | 7 |
| **Total** | **80** | **7** | **3** | **30** | **33** | **7** |

The nine separate-home pointers are not counted above; their current GitHub
state is recorded in §5. No row in this inventory is a newly confirmed release
blocker. The release blockers remain the separate #290 audit findings.

## 3. Whole-codebase review dispositions

The finding set is retained at
`.superpowers/sdd/2026-11-05-vector-field-explorer/final-codebase-review.md`.
Part E deferred I4, I5, I10, I11, I12, I13 and M1–M34 to post-v1
performance/cleanup. Later work has closed or changed parts of that set; the
rest stays parked.

| Origin | Disposition | Current evidence / remaining decision |
| --- | --- | --- |
| C-1 onboarding "Start here" target | Closed | `src/presets.rs:100` defines `START_HERE` with three entries; `src/ui/presets.rs:1179` renders the shortlist. The old report was correct at its base and is no longer current. |
| I4 evaluator binding | Confirmed | `src/field.rs:154-163` still clones the parsed `meval::Expr` and calls `bind4` on every component evaluation. The #240 sample cache (`src/field_cache.rs`) avoids repeated passes for static fields but does not bind once. Post-v1 performance decision; no fix here. |
| I5 duplicate sampling | Superseded | `src/viewmodel.rs:4805` builds the scene from one sampled list; `scene_color_facts` (`src/app/mod.rs:1391`) computes legend facts separately. For static fields #240 serves the second pass from cache. Time-dependent fields still sample more than once; deferred to performance work. |
| I10 app-shell click/shortcut state machine | Deferred | App-frame tests now exist for other surfaces (for example `tests/accent_picker_test.rs`, `tests/button_sizing_test.rs`), but the retained VfApp click/double-click/particle-rollback state machine still has no direct app-level harness. Original “no harness” claim is obsolete; the specific gap remains parked. |
| I11 particle position visibility | Deferred | `src/particles.rs:337` adds `truncate` across all parallel vectors, closing the truncate path; `pos` remains `pub` at `src/particles.rs:247`. Visibility decision deferred. |
| I12 streamline rebuild cost | Deferred | No streamline-specific density-24/200 measurement was added. `docs/perf/p1-p6-disposition.md` covers general snapshot cost, not this acceptance measurement. Post-v1 performance. |
| I13 render allocations/sorts | Deferred | P4 landed depth keys computed once (`src/renderer.rs:1972-2048`, PR #246); P2 remains partial per `docs/perf/p1-p6-disposition.md`. P3/browser work is post-v1. No integrated/web floor claim. |
| M1 duplicated helpers | Confirmed | Duplicate `inside`/`unit_triple` remain at `src/streamlines.rs:44,351` and `src/particles.rs:398,426`; two RK4 walks at `src/particles.rs:467` and `src/ode.rs:481`; duplicated `finite_or` in `src/persistence.rs:1968`, `src/ui/style_panel.rs:131`, `src/object_renderer.rs:174`. Cleanup only. |
| M2 oversized modules/argument bundles | Confirmed | `src/viewmodel.rs` (5,646 lines), `src/ui/sidebar.rs` (3,903), `src/renderer.rs` (2,078); `#[allow(clippy::too_many_arguments)]` remains in `src/field_probe.rs`, `src/viewmodel.rs`, `src/ui/top_bar.rs` and others. Post-v1 cleanup. |
| M3 layering inversions | Deferred | `src/ui/status_bar.rs:14` still imports `crate::app::StatusBarState`; `src/persistence.rs:61-63` still imports UI types. Not re-audited further; cleanup only. |
| M4 argument-bundle suppressions | Confirmed | The same `too_many_arguments` suppressions listed under M2 remain. No behaviour impact. |
| M5 constant-in-production parameters/clutter | Confirmed | `dimension` is still a runtime parameter through `spawn`/`resize`/`retarget` (`src/particles.rs:489,523,546,569`); the test-only branch remains. Cleanup. |
| M6 duplicated finite-or helpers | Confirmed | Duplicates remain at `src/fly_camera.rs:488`, `src/view_command.rs:206`, `src/persistence.rs:1968`, `src/ui/style_panel.rs:131`, `src/particles.rs:217`. Cleanup. |
| M7 duplicated camera constants/ray code | Confirmed | `src/camera.rs:53-54` and `src/fly_camera.rs:42-44` repeat NEAR/FAR; `ndc_ray` exists in both. Cleanup. |
| M8 hand-written left-click decoder | Confirmed | `src/persistence.rs:2193-2254` still matches variant names by hand with `SeedStreamline` fallback. Cleanup. |
| M9 dead or test-only public items | Confirmed | Current examples: `src/export.rs:82` (`save_svg`), `src/render.rs:19` (`world_to_ndc`), `src/ui/help_overlay.rs:481,501` (`all_rows`, `rows_for`), `src/screenshot.rs:1406` (`toast_text`), `src/camera.rs:297` (`is_fly`). No production caller or seam ruling added. |
| M10 orphan files | Confirmed | `src/render/instancer.rs` (33 lines) and `src/ui/arrows.rs` (5 lines) remain on disk; neither is declared by `src/lib.rs`/`src/ui/mod.rs`. Deletion is a cleanup decision, not authorized here. |
| M11 inconsistent `#[doc(hidden)]` | Confirmed | Many test seams are hidden, but public test-only items such as `rows_for` and `all_rows` remain exposed without the label. Cleanup. |
| M12 stale “not yet drawn” comments | Confirmed | `src/viewmodel.rs:2733` and `:2787` still say stream/particle tracing is “not yet drawn”. Documentation-only drift. |
| M13 comments that contradict code | Confirmed | `src/particles.rs:19,225` still call the RNG “Park/Miller”; `src/ui/style_panel.rs:85` still says “three existing `Preset` types”. Cleanup. |
| M14 stale plan/spec/app references | Confirmed | Examples remain at `src/renderer.rs:1` (“main plan Task 7”), `src/streamlines.rs:23` (“see plan task 9”), `src/ui/sidebar.rs:3081` (“app.rs”). Historical identifiers are not authority. |
| M15 history identifiers in code/tests | Confirmed | Hundreds of ticket/plan references remain (`rg -c 'ticket|issue #|plan task' src` gives 736 lines across source). Cleanup, not a correctness issue. |
| M16 stale `allow(dead_code)` | Confirmed | `tests/support/mod.rs:27` still carries the allow. |
| M17 stale layout guard floor | Confirmed | `tests/test_layout_test.rs:168` still asserts `test_attributes >= 705`; the current suite is far larger. A deletion smaller than the drift would pass. Cleanup. |
| M18 test naming/restatements | Deferred | Not re-audited line by line; the retained naming/constant-restatement nits remain parked. No release impact. |
| M19 WGSL mirror tests by substring | Confirmed | `tests/arrow_style_test.rs:418-462` and related checks still use `contains(...)` on shader text. Formatting-sensitive, not wrong today. |
| M20 non-atomic preset writes | Confirmed | `src/export.rs:194,206` still call `fs::write` directly for `presets.json`. Scene-store writes are atomic (`src/scene_store.rs`), camera presets are not. Cleanup. |
| M21 BGRA screenshot allocation | Confirmed | `src/screenshot.rs:163-171` still builds a second buffer with `collect()` for BGRA. No in-place return. |
| M22 Cargo metadata/public surface | Confirmed | `Cargo.toml` has no `rust-version` and no `publish = false`; modules remain broadly `pub`. The 0.2.0 version bump is separately owned by #290 finding 16. |
| M23 CI hardening | Confirmed | `.github/workflows/ci.yml` has no top-level `permissions:` block and uses floating `dtolnay/rust-toolchain@stable` / major-tag actions. |
| M24 copy and link mismatches | Deferred | Some copy moved after the report; the retained list was not re-audited end to end. No current release claim depends on it. |
| M25 stale AGENTS lines | Deferred | The specific retained lines no longer match current text and were not re-verified; treat as historical wording debt, not policy. |
| M26 stale ADR statements | Confirmed | `docs/adr/0001-ui-layout-panel-rails.md:9` still says Appearance moved to a Settings popover; `docs/adr/0003-physics-foundations.md:53` still describes `rayon` as adopted after measurement. |
| M27 notice generator/AND coverage | Confirmed | `packaging/THIRD_PARTY_NOTICES.txt:1148-1149` still lists `libfuzzer-sys` as `(MIT OR Apache-2.0) AND NCSA` with no NCSA text in the file. The package is not in the shipped graph, but the generator policy gap remains. |
| M28 duplicate notice source | Confirmed | `packaging/notice-sources/mit-objc2.txt` and `mit-rust-objc.txt` both remain and are both referenced by `manifest.json`. Cleanup. |
| M29/M29a duplicated bind groups; public SimClock fields | Confirmed | `src/renderer.rs:1358-1500` still repeats bind-group construction per collection; `src/sim_clock.rs:29-41` keeps `time`, `playing`, `speed`, `loop_period` public. Cleanup. |
| M30 macOS preset location | Confirmed | `src/ui/presets.rs:43-56` still uses `XDG_CONFIG_HOME`/`~/.config` on every OS. Non-blocking platform decision. |
| M31 repeated Control/State/record fields | Confirmed | `src/ui/sidebar.rs:965`, `src/viewmodel.rs:480`, `src/persistence.rs:697` still mirror large overlapping field sets. Cleanup. |
| M32 documented but not wired | Deferred | The CODEBASE test-seam statement was not re-verified against every seam. Documentation cleanup. |
| M33 complex cleanup script | Confirmed | `scripts/test.sh` remains 261 lines with JSON probe, canonicalisation and hardlink handling. Tests cover it (`tests/clean_test_artifacts_test.rs`); simplification is optional. |
| M34 UI panel owns file I/O | Confirmed | `src/ui/presets.rs:681-773` still saves/loads/deletes through `crate::export` from the panel. Cleanup. |

## 4. Scoped-review origin dispositions

The origin numbers below are the retained local review scopes in issue #23, not
GitHub issue numbers. Where a later ticket demonstrably replaced the concern,
the disposition says `Superseded`; otherwise the row stays parked.

| Origin | Disposition | Current evidence / remaining action |
| --- | --- | --- |
| 01 arrow lighting minors | Confirmed | `src/shaders.arrow.wgsl:22`, `arrow2d.wgsl:21` and `line.wgsl:12` still declare `inv_view_proj`; `src/renderer.rs:435` still fills it. Per-frame buffer recreation also remains (M29/I13). Cosmetic/cleanup. |
| 02 formula bar/state semantics | Deferred | `State::set_formula` (`src/viewmodel.rs:1657`) now documents parse-error preservation and calls `sync_plane` once. No current failure was reproduced for the remaining doc/sync/status-copy nits; the density-24 plane cost is unmeasured. |
| 03 sidebar controls | Superseded | The old “no arrows toggle” absence is superseded by the Show arrows control (`src/ui/sidebar.rs:2424`) and persistence default (`src/persistence.rs:769`). Residual heading/manual-range seeding nits remain parked. |
| 04 streamlines | Confirmed | `src/viewmodel.rs:2733` still documents `set_streamlines` as “not yet drawn”; the asymmetric both-direction budget remains intentional. |
| 05 particles/dots | Deferred | Count/resize behavior is now directly tested (`tests/particles_test.rs:491-615,708-779`). The constant-field y/z test gap and the historical truncated package artifact were not re-verified. |
| 06 3D click seeding | Deferred | Click planning, drag rollback and 2D zoom are covered (`tests/left_click_action_test.rs:81-130`, `tests/click_seed_zoom_test.rs`). No direct app press/release harness for the panned-target 2D case; first-click colour staleness was not reproduced. |
| 07/08 picker/formula restyle/screenshot comment | Deferred | The picker API is equivalent; first-frame restyle and the glued screenshot comment were not re-audited. Cosmetic. |
| 12 empty instance-buffer bind | Closed | `src/renderer.rs:1402-1415` skips empty collections and keeps the one-instance placeholders; the same guard covers arrows, lines, trails and dots. |
| 14 2D painter comparator | Deferred | `sorted_arrows_back_to_front` (`src/renderer.rs:2064`) still reproduces the orbit-based comparator; ticket 62 explicitly preserved it. Separate 2D-sort decision. |
| 20/24 logging/smoke | Deferred | `scripts/smoke-run.sh:23` still defaults to `target/debug/vector-field-explorer` with `SMOKE_BIN` override; the `try_init`/doc-placement and runner-comment nits remain parked. |
| 28 Help/menu | Deferred | Menu dedup and F1/Help behavior are now tested (`tests/menu_test.rs:667-735`), with heading colours in `tests/help_overlay_test.rs:391-431`. The stale glossary/light-theme/formatting nits were not all re-audited. |
| 29 round particles/WGSL parity | Deferred | Dot billboard and fragment tests exist (`tests/dot_billboard_test.rs`), but the retained “parity comment only / fragment GPU path unexercised” limit remains. No fabricated visual or perf claim. |
| 18 particle truncation/help seam | Deferred | `Particles::truncate` (`src/particles.rs:337`) now aligns all vectors and is covered by `truncate_after_burst_realigns_positions_and_trails`; `rows_for` remains a public test seam and the comment-placement nit remains. |
| 22 rail typography | Deferred | Cosmetic rustfmt/comment and click-target geometry nits were not re-adjudicated. No behaviour claim. |
| 23 plane release | Deferred | No new density-24 measurement; the CPU sort is still the depth equivalent by design (`src/renderer.rs:826-827`). The release-only highlight and one-frame stale-grid nits stay parked. |
| 34 test-artifact cleanup | Closed | Current tests cover missing target, layout mismatch fail-safe, symlinked target and failing-test status preservation (`tests/clean_test_artifacts_test.rs:267-518`). Stale RED quotes were not re-opened. |
| 42 nightly release ordering | Deferred | Static tests now pin upload-before-tag and draft-before-publish (`tests/nightly_version_test.rs:149-194`). No tag-push failure simulation and no live dispatch evidence, so this is not `Closed`. |
| 43 AppImage packaging | Deferred | `package.yml` still scans only `usr/lib` at `-maxdepth 1` (`:303-304`) and does not add the retained `libwayland-egl` exclusion. Pick up when packaging is next touched; shipped-app runtime evidence is separate (#22). |
| 56 agent-instruction copy | Deferred | The retained feature-path/scope/“today” wording nits are cosmetic and were not re-opened. Policy authority is `references/workflow.md`. |
| 44/64 arrow shaft geometry | Closed | The deferred shaft-order gap is now covered across side counts and radii (`tests/arrow_geometry_test.rs:140-196`), and 64 added the family/geometry coverage. The inverted shader top-name comment remains cosmetic. |
| 41 accent picker/colormap | Deferred | The accesskit nested-picker regression is covered (`tests/accent_picker_test.rs:170`); the magic-number collapser lookup, duplicated `MemoryStorage` and missing mutation run remain parked. |
| 52 wasm screenshot cap | Evidence limit | `src/screenshot.rs:36-42` still caps wasm at 2048². The wasm-only in-flight refusal test is not executed locally; revisit only with browser encode evidence. |
| 58/59 appearance picker | Deferred | Nested accent picker and Escape/outside-click are tested (`tests/accent_picker_test.rs`); synthetic pointer/accesskit, alpha near-miss, closure simplicity and copy/glossary nits remain parked. |
| 45 arrow styles | Deferred | Globals remains 224 bytes with Rust alignment 4 (`tests/dot_billboard_test.rs:252-253`); flat half-width and `ArrowDrawPlan` semantics are intentional. No behaviour change. |
| 61 2D arrow family | Superseded | Ticket 64 added the separate family and behavioural 2D plan test (`tests/arrow_style_test.rs:255-297`). Do not revive the one-variant compile-red concern. |
| 62 depth order/perf | Deferred | P4 depth-key sorting landed (`src/renderer.rs:1972-2048`, PR #246); P2 buffer reuse is partial and integrated/web floors remain unmeasured. Pixel identity is still not an old/new binary diff. |
| 26 transport nits | Deferred | The `contains("speed:")` weak assertion is fixed in `tests/persistence_test.rs:411`; the 960-wide status readout now has a real frame test (`tests/status_bar_test.rs:439`). Constant-against-itself budget assertions and the duplicate typing-branch test were not re-audited. |
| 65 window/capture manual checks | Evidence limit | X11 confined-edge/sidebar clicks, Wayland grant and browser grant/denial remain manual-only. Waived, not passed. |
| 66 split app module | Closed | `src/app/` now contains `mod`, `frame`, `input`, `panels`, `simulation`, `viewport`, `present`, `plane`, `plot`, `history`; the ~340-line `frame_viewport` extraction is done into focused modules. Do not claim any further extraction. |
| 30/50 particle count/streamline gating | Deferred | Count, resize, burst preservation and streamline modes are covered (`tests/particles_test.rs:466-615`, `tests/streamlines_test.rs`). The per-frame/click glue still has seam+smoke coverage only; no app harness was added. |
| 48 particle trails | Closed | The retained 0.999 comment path was replaced by an explicit out-of-domain step (`tests/particles_test.rs:724-729`) and the zero-field RK4 sentinel test remains (`:810-825`). Assertions are valid. |
| 51 status bar 960 fit | Closed | The 960-wide status bar now has a real egui frame/accesskit test (`tests/status_bar_test.rs:439-458`) in addition to the pinned mask (`:166`). No physical 960×600 screenshot claim is made. |
| 36 arrow opacity persistence | Deferred | Round-trip and clamping tests exist (`tests/arrow_opacity_test.rs`, `tests/persistence_test.rs:1048`), but the retained “5.0 half masked / legacy fixture shape / formatting drift” nits were not re-adjudicated. |
| 68 project icons | Confirmed | `tests/window_test.rs:68` still uses `icon.rgba.iter().any(|&alpha| alpha != 0)`, which checks every byte rather than the alpha channel. Tighten only when the icon test is next touched. |
| 53 phone layout | Evidence limit | Real-browser touch/safe-area, Presets-sheet switching and physical two-finger/pinch remain unverified. Waived, not passed. |
| 19a/19b notices | Evidence limit | Notice generation/shipping is in `scripts/generate-notices.sh` and `packaging/`, but the actual shipped graph for Windows was not revalidated here. Residual wording nits remain. |
| 35 performance research | Evidence limit | P1–P6 are dispositioned in `docs/perf/p1-p6-disposition.md`: P3/P6 deferred, integrated/browser floors unverified. P1 is optional, not a blocker. |
| CAS research | Evidence limit | The retained `cas-selection-rereview-1.md` is the closure record; the original evidence-review artifact is unavailable. curl(grad) proof is z-only, symcas non-fail-fast success is unverified, and there is no wasm runtime proof. Exmex adoption is owner-selected, not inferred. |
| 71 mobile follow-up | Evidence limit | S2 live phone/browser/resize/capture checks are explicitly waived, not passed. No universal-impossibility claim; CSS viewport and Rust root/canvas/DPR measurements differ. |

## 5. Current release map

| Home | Current state | Relationship to 0.2.0 |
| --- | --- | --- |
| #290 release-audit tracker | Open. Audit found 16 major and 4 minor findings; minimum blocking set 1–10, 14, 16 per its body. PR #291 closed the data/CAS half (1–6, 13–15); PR #293 closed the IO/lifecycle half (9–12, 17–20). | **Blocks 0.2.0.** Findings 7, 8 and 16 were not closed by #291/#293 and remain part of #290's minimum set pending tracker confirmation; the final re-review, #222 and the version bump are also still listed there. |
| #23 this reconciliation | This record. | A #290 checklist item. It is documentation evidence only; it does not close any code finding. |
| #222 experiment notebook | Open feature request. | Listed on #290's checklist; not implemented here. |
| #287 typeset formula panel | Open feature request. | Post-0.2.0 feature scope unless the owner folds it into the release. Not part of the #290 minimum blocking set. |
| #289 UI motion | Open feature request. | Post-0.2.0 feature scope unless the owner folds it into the release. Not part of the #290 minimum blocking set. |
| #19 Windows dry run | Closed, not planned. | No Windows GUI first-launch evidence exists. Do not describe the dry run as passed. |
| #18 performance floors | Closed; disposition at `docs/perf/p1-p6-disposition.md`. | Integrated/web floors unverified; P3/P6 post-v1. |
| #20 public-launch reconciliation | Closed. | Evidence retained, not a 0.2.0 blocker. |
| #21 Pages evidence | Closed. | Evidence retained, not a 0.2.0 blocker. |
| #22 AppImage runtime evidence | Closed. | Shipped-AppImage runtime evidence remains a separate home; no new claim. |
| #17 physics roadmap | Closed. | Future physics/model discussion only; distinct from this inventory. |
| #15 native CLI | Closed; active work completed separately. | Not a parked item. |
| #12 scalar plot | Closed. | Implemented separately; not missing historical work. |
| #16 blocked animation | Closed. | Implemented separately; not missing historical work. |

The 0.1.0 tag/release remains at `aa55470`; `Cargo.toml` still reports `0.1.0`.
The 0.2.0 version bump, changelog and post-fix re-review are #290 work and are
not performed here.

## 6. No-fix statement

This task changed documentation only. It did not:

- change source, tests, Cargo metadata or behaviour;
- implement any confirmed or deferred finding;
- delete `src/render/instancer.rs`, `src/ui/arrows.rs`, notice files or any other
  artifact;
- change a release, tag, repository visibility, Pages or other settings;
- reopen or close an old issue, or claim a waived/unrun check passed.

Further work requires a scoped decision in the owning issue. The historical
inventory above is evidence for that decision, not authorization to act.
