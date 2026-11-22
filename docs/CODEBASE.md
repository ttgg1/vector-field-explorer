# Vector Field Explorer — Codebase Guide

This is the whole-codebase map for the merged v0.1.0 tree. It is written for an
engineer (or agent) who needs to make a first safe change in any module without
re-reading history. Every API, constant, and test name below was checked
against the source in this tree; the authoritative inventory is `rg --files src`
and `cargo test --all-targets -- --list`.

Pointers, not duplicates:

- Domain vocabulary: [GLOSSARY.md](../GLOSSARY.md)
- Decisions and their rationale: [docs/adr/](adr/)
- User quickstart: [README.md](../README.md)
- Working conventions and gates: [AGENTS.md](../AGENTS.md)
- Performance harness, per-feature reports and dated baselines: [docs/perf/README.md](perf/README.md)
- Licence: [GPL-3.0-only](../LICENSE)

---

## 1. What the crate is

One Rust crate, `vector-field-explorer`, built two ways:

| Target | Entry point | UI/GPU runtime | Storage |
| --- | --- | --- | --- |
| Native desktop | `src/main.rs::main` | `eframe::run_native`, wgpu | eframe app storage (RON under `eframe::APP_KEY`) |
| Native CLI | `src/main.rs::main` → `cli::run_from` | CPU SVG / displayless wgpu PNG | explicit output file only; no GUI persistence |
| Browser | `src/main.rs::start` (`#[wasm_bindgen(start)]`) | `eframe::WebRunner`, wgpu/WebGPU | eframe's browser storage + `localStorage` for camera presets |

The crate is both `cdylib` and `rlib` ([Cargo.toml](../Cargo.toml)), so the wasm
bundle and the integration tests can share it. The only external rendering
dependency is eframe's re-exported wgpu; formula evaluation uses `meval`, while
`src/sources` is a separate pure CPU source-evaluator seam.

The field is formula-driven:

- `(u, v)` for a 2D field, `(u, v, w)` for 3D, each component an expression in
  `x`, `y`, `z`.
- Sampling, arrows, streamlines, particles, colormaps, plane inspection, two
  cameras (2D front / 3D orbit, plus an optional fly camera), camera presets,
  screenshots, deterministic GIF/ffmpeg animation export, and persistence are
  all in this tree.
- The app is a teaching-grade explorer, not a general numerical solver. The
  pure source-evaluator slice in `src/sources` evaluates all eight softened
  source geometries: point charges, finite charged lines, charged rings, charged
  thin discs, uniform solid spheres, infinite straight wires, current loops and
  finite continuous solenoids. The ODE workbench precomputes bounded fixed-step
  RK4 teaching trajectories. The Maxwell teaching catalogue lives in
  `src/maxwell.rs` and remains a bounded ideal-case model, not a general solver;
  Maxwell work beyond that is described in
  [ADR 0003](adr/0003-physics-foundations.md).

### Source control and licence

- `Cargo.toml` declares `license = "GPL-3.0-only"`; every dependency in the tree
  must remain GPL-3.0-compatible.
- Native-only dependencies (`rfd`, `dirs-next`, `env_logger`) are under
  `cfg(not(target_arch = "wasm32"))`; wasm-only dependencies (`web-sys`,
  `wasm-bindgen`, `console_log`, `web-time`, `getrandom/js`) are under the wasm
  cfg.
- `.cargo/config.toml` caps Cargo at 10 jobs and turns on the
  `web_sys_unstable_apis` cfg for wasm. Do not raise the job cap without a
  ledger ruling.

---

## 2. Architecture and data flow

### 2.1 The one screen state and the one scene seam

Native `cli` configuration exposes the pure `output_frame` seam implemented in
`src/cli_scene.rs`. Its `OutputFrame` owns output dimensions, camera transforms,
and straight-alpha sRGB/linear background values for both export backends;
`PreparedScene` pairs that frame with renderer-ready `SceneGPU` plus the mapped
`objects: Option<Arc<MeshScene>>` and committed `object_style`. The
`app::object_scene_for_state` mapping feeds the live viewport, the animation
snapshot (the CLI animation reaches it through `AnimationSnapshot::from_parts`)
and the CLI still scene, so a Sources/Induction state has one object count
everywhere. `prepare_scene_inner` stores `None` when the mapped scene has no
objects, so a Formula-mode PNG keeps the existing no-objects capture
byte-identical; a non-empty scene composites through
`render_offscreen_with_objects` with the committed Solid/Wireframe style. The
animation session forwards `AnimationFrame.object_scene`/`object_style` instead
of the live state. The SVG path ignores both fields and keeps its neutral
blueprint. Geometry, camera applicability and background conversion are covered
by `tests/cli_scene_test.rs`. The same module's `prepare_scene` preflights an indexed
f64 time schedule before seeded particle spawn/advection, then samples the final
f32 endpoint once through `State::snapshot`, composes references onto its flow
lines, and builds/sorts particle trails and the 3D combined arrow/dot stream.
Only CLI start/duration/fixed-step settings retain f64 input precision; existing
state and RK4 arguments stay f32. `Particles::with_seed` accepts an explicit RNG
state (including zero) without changing the GUI's default stream.

`viewmodel::State` is the single source of truth for what the viewport shows.
Every user edit goes through one of its setters, which owns parsing, clamping,
min/max swaps, and the dirty flag. The app keeps only widget mirrors and
per-frame simulation state beside it.

```
 FormulaBarState.text
        │  set_formula(String)
        ▼
 State::set_formula ──► FieldExpr::parse (meval)
        │                    │
        │                    └─ Err: parse_error set; previous FieldExpr kept
        ▼
  State { formula, field, domain, density, colour, styles, toggles,
          show_grid, show_coordinate_system, plane, particle knobs, dirty }
        │
        │ snapshot_if_dirty()          (only when dirty; clears the flag)
        ▼
 sample_field(field, domain, grid, view_options)
        │  → Vec<ArrowData> { position, unit dir, mag, colour }
        ├─ arrow_instances            → Vec<ArrowInstanceGPU>
        ├─ streamlines_snapshot       → Vec<LineInstanceGPU>
        └─ SceneGPU {
               arrows, lines,
               trails: [], dots: [], combined: [],   // filled by the app
               arrow_style, arrow_style_2d, arrow_params, mode_3d
           }
        │
        │ VfApp::rebuild_scene (app/simulation.rs)
         │  - compose_lines_with_frame(scene_domain, is_3d, plane_frame,
         │      reference lines + flow lines + click lines)
        │  - in Sources mode, append glyph lines (full glyphs, or only
        │    sign markers when mapped object bodies draw)
        │  - cache dots_range from the freshly sampled magnitudes
        │  - retrace click bursts on every dirty rebuild
        ▼
 VfApp::frame_viewport (app/viewport.rs)
        │
        ├─ 3D: combined_instances(arrows, dots) + sort_scene_back_to_front
        ├─ 2D: sorted_arrows_back_to_front(arrows); combined stays empty
        ├─ trails: sorted_lines_back_to_front(trails) (3D and 2D plane slices)
        │    (each depth transformed once, then a stable decorate–sort–undecorate)
        ▼
 SceneGPU + view_proj + eye_position
        │  egui_wgpu::Callback::new_paint_callback(rect, DrawCallback)
        ▼
 DrawCallback::prepare → DrawState::upload (Globals uniform + storage buffers)
 DrawCallback::paint   → DrawState::record  (DrawPass list)
        ▼
 wgpu render pipelines (vertex_index-generated geometry, no vertex buffers)
        ▼
 egui panels read State and mirrors, edit them through setters, and never
 touch GPU resources directly (only the paint callback does)
```

Why the seam matters:

- The viewmodel has no GPU types and no window. It can be unit-tested with plain
  values; `SceneGPU` is a plain Rust struct.
- `snapshot_if_dirty` is the only rebuild trigger for arrows and grid
  streamlines. Particle dots and trails are deliberately *not* in that snapshot:
  they change every frame and are rebuilt by the app from the live cloud.
- The app owns reference lines (2D grid and axes, 3D face grids/box/axes, or a
  plane slice's flat local grid and two axes) and clicks. `show_grid` selects
  the grid and 3D box; `show_coordinate_system` selects only the coloured axes.
  `compose_lines_with_frame` consumes the active `scene_domain`/`plane_frame`, so
  a 2D slice never gets world-cube references; the snapshot's `lines` holds
  grid-seeded streamlines only. In `FieldMode::Sources` the app also appends the
  active scene's styled source glyph lines after the reference/flow/click lines,
  so markers follow the same selected `plane_frame` as the samples and resolve
  the `Sources` layer with each glyph's family. When the object style draws
  bodies and `object_scene_for_state` maps at least one body, only the glyph's
  polarity sign markers join the line pass; the object pass owns the body. An
  empty object scene (the line-glyph fallback) keeps the legacy full glyph, and
  the app SVG export always composes the full neutral blueprint through
  `compose_vector_lines_blueprint`. In
  `FieldMode::Induction` it appends exactly the selected conductor's glyph,
  resolving the `Induction` layer and applying the same marker/body split, and
  no streamline/click geometry. `FieldMode::Materials` and
  `FieldMode::Maxwell` resolve one `(TeachingScene, family)` record for both the
  mode's arrows and its outline segments, so an Arrows-layer override no longer
  changes those teaching scenes; the SVG blueprint keeps the neutral default.
  Formula mode preserves the durable scene but appends none. In `FieldMode::Ode` the pure snapshot stays empty and
  the app owns precomputed trajectory lines plus axes, rebuilt once per solved
  scene;
  per-frame current-time markers are appended to a copy of `dots` (2D) or the
  sorted `combined` stream (3D). The resulting `SceneGPU.lines` is shared by
  both viewport modes, CLI export and screenshot capture.

### 2.2 Frame order

`src/app/frame.rs` implements `eframe::App::ui`:

1. `style_panel::apply` restyles every widget from the persisted `AppStyle`.
2. `layout::layout_mode(root_width)` decides Wide/Compact once per frame.
3. `frame_input` — screenshot completions, menu shortcuts, transport key, S/P
   seeds, clock advance, camera keys, help keys; captures `domain_before`.
4. `frame_chrome` — top bar, formula bar, fly sync, Controls/Presets panels or
   compact sheet, domain recentre, pointer-capture transition.
5. `frame_simulation` — dirty scene rebuild, particle retarget/advect/dot/trail
   rebuild, repaint request while animating.
6. `frame_status_bar` — FPS, parse message, snapshot counts, sim time, hints,
   compact tab bar/sheet.
7. `frame_viewport` — camera input, click/seeding, scene sort, screenshot
   capture, paint callback, fly crosshair.
8. `frame_overlays` — help overlay, About window, Keyboard & mouse window.
9. `drive_history` — one edit-history transaction pass over the frame's
   pointer state and any document change the frame committed.

The order is intentional: panels can change the state before the rebuild, the
rebuild feeds the status bar counts, and the viewport always paints the scene
built this frame. `frame_input` returning `domain_before` is what lets
`frame_chrome` recentre the camera after a domain edit.

### 2.3 Platform seam

The one seam is `cfg(target_arch = "wasm32")`, not a second app.

| Concern | Native | Browser |
| --- | --- | --- |
| Startup | `main` → CLI when arguments supplied, otherwise `eframe::run_native` | `start` → `ensure_webgpu` → `WebRunner::start` |
| Window options | `window.rs::window_options` | eframe `WebOptions` |
| Logging | `env_logger` to stderr, default `info` | `console_log` at `Info` |
| Time | `std::time` | `web-time` |
| Screenshot save | `rfd` dialog on a worker thread + `std::fs` | PNG encode + DOM anchor download |
| Camera presets | `presets.json` under XDG config | `localStorage` key |
| Raw pointer motion | winit `Event::MouseMoved` | `web/pointer_lock.js` via `vfePointerLock` |
| WebGPU availability | n/a | pre-start `navigator.gpu` + `requestAdapter` gate |

The wasm gate in `src/main.rs` runs *before* `WebRunner::new()`: if
`navigator.gpu` is missing or `requestAdapter()` yields nothing, the static
`#webgpu-unsupported` panel is revealed with the reason and the app returns
successfully without constructing eframe. There is deliberately no WebGL2 or
CPU fallback. `scripts/check-web-bootstrap.sh` pins both the DOM guidance and
the ordering of the Rust gate.

`index.html` is the Trunk shell; `web/style.css`, `web/pointer_lock.js` and
`web/webgpu.js` are copied/served by Trunk ([Trunk.toml](../Trunk.toml)). The
`vfe-canvas` id is the mount point.

### 2.4 Animation export path

`File > Export animation…` and the native CLI share one encoder contract but
different render owners.

1. `AnimationEligibility::from_state` accepts a time-dependent Formula/particle
   scene or an animated Induction scene. Sources, Materials, Ode, scalar plots
   and static scenes are rejected with the exact user-facing reason; the CLI
   never constructs Induction and covers Formula/particles only.
2. The GUI pauses playback and captures `AnimationSnapshot::capture`; the CLI
   builds `prepare_animation_snapshot`, preadvancing particles from 0 to
   `--start-time` with the still `--fixed-step`. The snapshot owns cloned `State`,
   `Particles`, camera and clear colour; the live app and shared clock are not
   stepped.
3. `AnimationSpec::validate` enforces fps `1..=30`, duration `0.1..=60 s`, at
   most 1800 frames, 8,388,608 pixels/frame and 512 MiB, plus even video
   dimensions. `frame_count = ceil(duration * fps)` and frame `i` is at
   `start + i / fps`, without a duplicated endpoint.
4. Each frame calls `AnimationSnapshot::frame_at(time, 1/fps, aspect, first)`,
   which rebuilds through the shared Formula/Induction scene assembly and
   advances particles only on later frames through `advance_exact` (1/120 s
   substeps plus remainder). No GUI `MAX_STEP` clamp or playback speed applies.
5. `apply_export_banner` paints the capture-time provenance on every frame. The
   GUI `AnimationJob::pump` renders at most one frame per app update and pushes
   it to a `FrameSink`; the CLI loops the same `AnimationSnapshot` through one
   `HeadlessRenderSession`. `gif_sink` streams a local-palette GIF; `video_sink`
   pipes raw RGBA to ffmpeg and embeds a sanitized metadata comment. GIF has no
   metadata contract.
6. `FrameSink::finish` checks the byte budget and atomically replaces the
   destination. Every error, cancel or dropped sink removes the sibling `.tmp`
   file, so no final output is claimed from a partial encode. Screenshot and
   animation capture are mutually exclusive.

The browser uses `memory_gif_sink` and a GIF download, with a 2048-pixel edge
validation limit; there is no wasm video path. GUI and CLI evidence, including a
real ffmpeg attempt, is in
[docs/perf/animation-performance.md](perf/animation-performance.md).

---

## 3. Module reference

Every file under `src/` is covered, including the WGSL shaders. Test names are the function names printed by
`cargo test --all-targets -- --list`; a module's test file is named in the
heading.

The tree evolved after the original ticket list. In particular, what the ticket
called the single `src/app` module is now `src/app/mod.rs` plus
`frame.rs`, `input.rs`,
`panels.rs`, `simulation.rs`, and `viewport.rs`; the later tickets also added
`arrow_geometry.rs`, `arrow_style.rs`, `fly_camera.rs`, `fonts.rs`, `plane.rs`,
`pointer_lock.rs`, `reference.rs`, `sim_clock.rs`, `render/instancer.rs`,
`ui/branding.rs`, `ui/menu.rs`, `ui/transport.rs`, `version.rs`,
`view_orientation.rs`, `viewport_style.rs`, `layer_style.rs`, and the four
shaders. Two files on disk are not compiled at all (`render/instancer.rs` and
`ui/arrows.rs`); they are documented where they appear so nobody adds live code
to them by mistake.

### 3.1 Crate shell and entry points

#### `src/lib.rs` — module tree and re-exports

**Purpose.** Declares all public modules and the small convenience re-export
surface used by integration tests and external callers.

**Public API.** `pub mod` for every module listed in the tree; re-exports
`app::VfApp`, `colormap::Colormap`, `domain::Domain`, `export::CameraPreset`,
  `field::FieldExpr`, `render::Camera`, and (native only) `export::load_presets` /
  `export::save_preset`.

**Invariants / notes.** There is no inline test module here;
`tests/test_layout_test.rs::src_carries_no_inline_test_modules` fails the suite
if `#[cfg(test)]` reappears under `src/`. Adding a module means adding it here
(and to `ui/mod.rs` / the `app` module list) or it is not compiled.

**Limitation.** Re-exports are intentionally narrow; most modules are reached by
path in tests. Do not widen this surface just to make a test compile.

#### `src/tour.rs` — guided onboarding tour model

**Purpose.** The pure, UI-agnostic half of the guided tour (#99 slice A): one
ordered table of stops and a session-only progress value. It carries no `egui`
types, no renderer, no persistence and no document mutation; the overlay and
app wiring are a separate slice.

**Public API.** `TourTarget::{Control(ControlId), Panel(PanelId),
Viewport(Region), Maximize}`, `AdvanceRule::{Next, Changed(&'static str)}`,
`TourStop { id, title, body, target, advance }`, the ordered `TOUR` table
(14 stops), the semantic `ControlId`/`PanelId`/`Region` enums with `ALL` and
stable keys, `resolve(target)` returning a `TourAnchor` descriptor, and
`TourProgress` with `advance`, `back`, `dismiss`, `restart`, `is_finished`,
`current`, `mark_opened` and `opened_panels`.

**Invariants / notes.** Every stop targets a capability that exists in this
build; a removed `ControlId`/`PanelId` variant stops the table compiling, and
`tests/tour_test.rs` checks that every target appears in the corresponding
`ALL` list. `AdvanceRule::Changed` stores a `ControlId::key`, so a wait cannot
name an unknown control; the Maxwell stop waits on `maxwell_case` because its
body asks the user to choose a case. `ControlId::Play` is chrome in the compact
transport strip rather than a panel child, so it maps to
`PanelId::FormulaBar` only because that strip is assembled with the top bar; UI
code should not treat `FormulaBar` as the transport's own panel. Progress is
bounded: `back` clamps at the first stop and is a no-op once finished,
`advance` clamps at the end, `dismiss` ends the tour for the session without
writing, and `restart` returns to the first stop and clears the tour-opened
panel list. The tour table is immutable; only `TourProgress` changes, and
nothing is persisted.

**Slice A boundary.** Slice A shipped the model only; slice B adds the overlay
and app wiring documented in `src/ui/tour_overlay.rs` below. The done-stop body
promises that the tour can be restarted, and the Help menu now carries the
"Start or restart tour" entry.

**Pinned by.** [tests/tour_test.rs](../tests/tour_test.rs):
`tour_table_has_unique_non_empty_ids_and_text`,
`every_stop_targets_an_existing_capability`,
`every_control_and_panel_resolves_through_the_anchor_descriptor`,
`changed_rules_name_a_real_control`,
`maxwell_stop_waits_for_a_case_change`,
`advance_and_back_preserve_index_bounds`,
`back_after_dismiss_keeps_the_tour_finished`,
`dismiss_finishes_and_restart_returns_to_the_start`,
`opened_panels_are_recorded_once_per_panel`,
`current_stop_follows_the_index_and_never_panics_after_finish`, and
`full_tour_run_opens_each_target_panel_exactly_once`.

#### `src/ui/tour_overlay.rs` — guided-tour card, anchoring and advance rule

**Purpose.** The UI half of #99: record the real panel/viewport rectangles the
app painted this frame, resolve a stop's semantic target against them, draw the
anchored card plus non-colour-only highlight, and evaluate `AdvanceRule`.

**Public API.** `advance_ready(rule, captured, current)` is the pure `Changed`
comparison; `control_panel(target)` names the panel that owns a control;
`TourFrameRects::{record_panel, record_control, record_viewport, anchor_rect}`
hold one frame of painted rectangles and `set_note`/`note` carry one actionable
card note; `place_card(target, screen, size)` places a card near but not over
the target inside the screen; `show(ctx, rects, progress, ready)` paints the
card and returns `TourOutcome { action, card_rect, target_rect }`.

**Invariants / notes.** `ControlId` anchors prefer a live control rectangle
when the app recorded one and otherwise fall back to the owning panel.
`ControlId::FieldProbeSurface` (the Field probes `Add sphere` button) requires
its own live rectangle: when the Field probes body is closed the flux stop has
no highlight rather than pointing at the whole Controls panel. The tour reveals
that body once per stop; a user close is respected. On entry it also switches a
Formula/Ode adapter to Sources through `State::set_field_mode`, so the stop
reaches a real probe field; when the scalar Plot view has no vector field the
card carries an actionable note instead of a dead section. An `AdvanceRule::Changed`
compares this frame's value with the snapshot captured
when that stop became current; an unavailable value never satisfies it. The
card has a title and body and uses a `UiKind::Modal` stack kind for an
accessible dialog role without blocking viewport pointer input. There is no
pulsing, timer or auto-advance. `VfApp::tour` is the only new state: it is
session-only, never in `ViewRecord`/`AppSettings`, and start/advance/back/
dismiss/finish never touch `dirty`, the document or the edit history. Collapsed
targets open through `LayoutState::toggle_*` in wide layout and the existing
compact sheet in compact layout; a second layout path does not exist.

**Pinned by.** [tests/tour_overlay_test.rs](../tests/tour_overlay_test.rs):
`starting_the_tour_records_no_document_or_history_change`,
`next_and_back_follow_the_stop_table`,
`escape_dismisses_and_the_menu_restarts_at_the_first_stop`,
`changed_rule_advances_only_after_the_control_changes`,
`opening_a_collapsed_panel_uses_the_normal_layout_toggle`,
`resolved_anchor_paints_a_highlight`,
`flux_stop_anchors_to_the_real_integral_surface_control_in_wide_and_compact_layouts`,
`an_absent_integral_control_does_not_fall_back_to_the_whole_panel`,
`unresolved_anchor_still_shows_the_card_without_a_highlight`,
`compact_layout_keeps_the_card_on_screen`,
`tour_keys_do_not_reach_viewport_handlers_but_text_editing_still_works`, and
`help_menu_entry_is_labelled_start_or_restart_truthfully`; the flux control
target is also pinned in `tests/tour_test.rs`.

#### `src/main.rs` — native and wasm bootstrap

**Purpose.** Native `main` and the browser `wasm_bindgen(start)` entry point,
including the pre-start WebGPU gate and the static fallback UI.

**Public API.** None as a library; the wasm `start()` is exported.

**Invariants.**
- Native initializes `env_logger`, logs the version from `version::VERSION`, and
  never runs on wasm.
- After logging setup, arguments dispatch to `cli::run_from` before window options
  or app construction; help/version succeed without a window or GPU. No arguments
  retain the ordinary GUI path and its startup log.
- `VFE_PERF_PRESENT=1` selects the bounded native present-path measurement before
  `run_native`: `PresentConfig::from_env` validates the scene/seconds, the app is
  built through `VfApp::new_present` with `None` storage, and
  `window_options_present` forces the 1280×800 measurement geometry. Invalid
  values exit 2 with a named error; every other value or an absent variable
  leaves the ordinary path untouched.
- Wasm initializes `console_log`, checks `navigator.gpu` and awaits
  `requestAdapter()` before `WebRunner::new()`, reveals `#webgpu-unsupported`
  with the reason on failure, hides the panel on success, and removes `#loading`
  only after the runner starts.
- Both GUI paths install `fonts::install` and construct
  `VfApp::new_with_storage(cc.storage)`.
- `show_startup_error` changes the loading overlay text rather than panicking.

**Pinned by.** `tests/version_test.rs::the_startup_log_reports_the_build_version`
(source-level check), `tests/web_assets_test.rs`, and
`scripts/check-web-bootstrap.sh` (not a Rust test; run by CI's wasm job).

**Limitations.** The browser matrix is manual; the gate only proves that
`navigator.gpu` and an adapter exist, not that every WebGPU feature the app uses
is available.

#### `src/animation.rs` — bounded animation spec and streaming encoders

**Purpose / API.** Pure animation contract with no app, GPU or clock
dependency. Exports `AnimationFormat`, `AnimationSpec`, `AnimationError`,
`FrameSink`, `frame_count`, `frame_times`, `gif_delay_centiseconds`, `gif_sink`,
`encode_gif_to_path`, `video_sink`, doc-hidden `video_sink_with_program`,
`ffmpeg_available`, `ffmpeg_args` and `atomic_replace`, plus the
`ANIMATION_*` limits. `AnimationSpec::default` is 0 s, 4 s, 12 fps, 1280×720,
looping GIF.

**Invariants.** `frame_count = ceil(duration * fps)` clamped to
`1..=1800`; frame `i` is at `start + i / fps` with no duplicated endpoint.
Validation rejects nonfinite times, fps outside `1..=30`, duration outside
`0.1..=60`, zero dimensions, more than `8_388_608` pixels/frame, frame-budget
overflow and odd MP4/WebM dimensions. `FrameSink` requires exactly
`width*height*4` tightly packed RGBA bytes for the spec dimensions. GIF uses
the direct `gif = "0.14"` encoder with per-frame local palettes, no dithering,
and `Repeat::Infinite` only when `loop_gif` is true; single-play output omits
the repeat extension. The counting writer aborts mid-stream at 512 MiB. Video
args pipe raw RGBA to ffmpeg (`libx264`/`yuv420p`/`+faststart` for MP4,
`libvpx-vp9` for WebM), capture stderr, and never fall back to GIF when ffmpeg
is missing. `VFE_FFMPEG` overrides PATH detection. Every encoder writes a
sibling `.tmp` file, checks the byte budget, and uses `atomic_replace`;
failures and dropped sinks delete the temporary file. ffmpeg is invoked with
`Command` arguments directly, never through a shell.

**Pinned by.** [tests/animation_test.rs](../tests/animation_test.rs),
[tests/animation_gif_test.rs](../tests/animation_gif_test.rs), and native
[tests/animation_ffmpeg_test.rs](../tests/animation_ffmpeg_test.rs). The
fake-ffmpeg process tests are Unix-only.

**Limitation.** GIF color quantization may band on large viridis/neodymium
frames; dithering is intentionally off. Video is native-only and requires a
user-installed ffmpeg. The sibling temporary filename has no container
extension, so `ffmpeg_args` selects the muxer explicitly with `-f mp4`/`-f
webm`; a real 2-frame 64×64 sink smoke passed for both formats. Full-app GUI
video and browser runtime remain unverified. See
[docs/perf/animation-performance.md](perf/animation-performance.md).

#### `src/animation_scene.rs` — detached animation frame source

**Purpose / API.** Owns the detached copy an animation export renders from:
`AnimationSnapshot { state, particles, orbit, fly, clear, object_scene }`,
`AnimationFrame { scene, object_scene, object_style, view_proj, eye }`,
`AnimationSnapshot::{from_parts, capture, frame_at}`, and
`VfApp::animation_snapshot`. `State: Clone` and `Particles: Clone` make the copy;
`State` holds no GPU handles. `object_scene: Arc<MeshScene>` is the mapped
source/induction bodies; `capture` shares the live app's cached `Arc` and every
frame clones it, so the frame exporter composites the same bodies as the
viewport without a per-frame rebuild. `object_style` is the snapshot's committed
[`ObjectStyle`], applied to the capture `DrawState` before each frame so an
exported GIF matches the viewport.

**Invariants.** `capture` clones the live app state, cloud, orbit/fly pose and
theme background without mutating playback. `frame_at(time, dt, aspect, first)`
validates finite inputs and a finite positive aspect, mirrors `time` into the
detached state, marks it dirty, rebuilds through the live `snapshot_if_dirty`
seam, and composes lines, glyphs and particles with the same shared app
functions as the live viewport.
Formula and Induction are supported; Sources, Materials and Ode are rejected as
non-animatable. `first` skips `advance_exact` and renders the captured particle
cloud as-is, including its trails; later frames advance by exactly `dt` through
`particles::advance_exact` (1/120 substeps plus a remainder), never the GUI
`MAX_STEP` clamp. Only Formula/Induction scenes with an enabled particle target
supply a cloud. Scene ordering (3D combined sort, 2D arrows-then-dots, trails
behind) is the shared live assembly and is independent of aspect; `view_proj`
uses the caller's aspect for the snapshot camera.

**Pinned by.** [tests/animation_scene_test.rs](../tests/animation_scene_test.rs):
time-dependent Formula sampling changes the frame, a 0.5 s particle frame moves
by 0.5 world units (not one GUI-clamped step), direct 60-substep advection
matches each exact substep time, the first frame renders the captured cloud's
dots, trails and combined stream, and an Induction snapshot's object scene is
the same `Arc` (and generation) in the frame.

**Limitations.** The snapshot does not capture transient click seeds or the live
viewport aspect; the caller passes the export aspect to `frame_at`. Induction
is GUI-only by design; CLI animation covers Formula and particles.

#### `src/animation_job.rs` — GUI animation job

**Purpose / API.** Owns one GUI animation export: `AnimationEligibility
{ allowed, reason, animated }` with the three exact disabled reasons,
`AnimationJobState`, the `AnimationRenderer` trait (fakeable in tests), and
`AnimationJob` (`new`, `fake*` test constructors, `pump`, `cancel`,
`take_next_frame`/`finish_rendered_frame` for the browser's async readback,
`state`, `frames_done/total`, `error`). The browser-only `memory_gif_sink`
encodes to memory and downloads the GIF on finish; `check_animation_byte_cap`
enforces the same 512 MiB output budget on its accumulated `Vec`.

**Invariants.** Eligibility is derived only from the live `State`: Formula needs
a time-dependent field or an enabled particle target; Induction needs
`is_animated()`; Sources, Materials, Ode and scalar plots take the shared
unsupported reason. `pump` renders at most one detached frame per app update and
never mutates `VfApp` or advances the shared clock; the explicit `1/fps` step and
captured snapshot are the only animation state. Every frame passes
`apply_export_banner` with the provenance captured at Start before the sink sees
it. A job that fails, is cancelled, or is dropped abandons its sink, so the
streaming encoder deletes its temporary file and never finishes the output. On
the browser, a terminal job and every new job clear the pending readback
receiver, so a stale callback can never complete against the next job.
`AnimationJob::new` validates the spec and computes the frame count once.

**Pinned by.** [tests/animation_job_test.rs](../tests/animation_job_test.rs):
one-frame-at-a-time progress, renderer and sink failures, cancellation before and
during encoding, banner application on every fake frame, temp-cleanup probes,
exact eligibility reasons, and a real `AnimationSnapshot` + `gif_sink` export
decoded back to its frame count.

#### `src/ui/animation_dialog.rs` — animation export dialog

**Purpose / API.** `AnimationDialogState` (format, duration, fps, GIF loop,
width/height, error, progress, status), `AnimationDialogAction`, the window
`show`, size/budget helpers and filename helpers. The dialog captures no app
state: the app applies Start/Cancel after the widget pass.

**Invariants.** The default size is the current viewport in physical pixels,
scaled down into `ANIMATION_MAX_PIXELS` while preserving aspect. There is no
start-time field; the spec starts at the captured live `State::time`. The exact
frame count and disabled reason are visible before Start. Explicit oversized
values are validation errors, not clamps. The browser build is GIF-only and
rejects an explicit edge above `ANIMATION_WASM_MAX_EDGE` (2048) instead of
silently shrinking it. Start is disabled while a screenshot capture is pending or
a job is running. A failed sink-open/snapshot attempt leaves Start available for
a retry; the next attempt clears the old error, while invalid-field validation
still keeps the button disabled.

**Pinned by.** [tests/animation_dialog_test.rs](../tests/animation_dialog_test.rs)
at 1200 and 390 px: menu/dialog rendering, frame-count text, exact disabled
reasons for static Formula/Induction and unsupported modes, menu disabled state,
screenshot mutual exclusion and the paused/restored transport state.

#### `src/cli.rs` — native plot export configuration and dispatch

**Purpose.** Parse one-shot export flags and strict JSON, overlay config with
explicit flags, and validate the resolved settings without opening a window.

**Public API.** Native-only `CliConfig`, `CliFlags`, `CliOptions`, `CliError`,
`parse_config`, `parse_flags`, `resolve_config`, `validate_intrinsic_config`,
`validate_effective`, `run_from` and `run_export`. See the [user CLI guide](cli.md)
for the complete JSON/flag mapping.

**Invariants.** Unknown JSON members are rejected, including nested members;
only `color_range` accepts null. Supplied values are intrinsically validated
before flags can override them. Final mode and formula determine density and
grid bounds; `CliOptions` carries resolved settings and explicit camera-member
markers for mode applicability. Clap and pollster are native-only dependencies.
`animation_format` is a lowercase `gif|mp4|webm` string at the clap/JSON edge;
`CliOptions` carries the resolved `AnimationFormat`. Animation mode requires the
output extension to match the format, defaults an omitted duration to 4 s, and
leaves the still-image duration default at 0. `animation_fps` defaults to 12 and
`animation_loop` to false; loop with MP4/WebM and odd video dimensions are
validation errors. Frame, pixel, byte and nonfinite-time limits come from
`AnimationSpec`; animation validation runs before any output file is created.
Still PNG/SVG bytes and `--duration` preadvance semantics are unchanged when
`animation_format` is absent.

**Pinned by.** `tests/cli_test.rs` and `tests/cli_animation_test.rs`, registered
in the native suite.

**Dispatch.** `run_from` handles clap help/version, reads an optional strict JSON
config, resolves flags and calls `run_export`. The latter validates and branches
only when `animation_format` is present: animation captures a detached
Formula/particle snapshot, opens one headless session and streams every frame
through `apply_export_banner` and a `FrameSink` (`gif_sink` or `video_sink`).
Otherwise it prepares one scene, selects CPU SVG or offscreen RGBA8 + PNG
encoding by output extension, and writes the explicit path with contextual
errors. SVG uses the neutral blueprint (`prepare_scene_with_styles` with the
default record) while PNG keeps the committed layer styles. Neither constructs
`VfApp`, uses storage, nor opens dialogs. No-argument
GUI and wasm startup are unchanged. `tests/cli_test.rs` exercises the actual
executable with child-only environments; `scripts/test-cli-dispatch.sh`
separately runs real displayless 2D/3D PNG decoding, non-background field checks
and persistence sentinels through the wired binary.

#### `src/cli_svg.rs` — native CPU vector export

**Purpose / API.** `cli::scene_to_cli_svg(&PreparedScene)` returns a 2D SVG or
rejects 3D before serialization, without creating a GPU adapter or device.

**Invariants.** Uses the prepared frame and instance order and never reads
`PreparedScene.objects`/`object_style`; those belong to the raster captures.
Flat arrow shaft/head
and headless Streamline geometry follow `shaders.arrow2d.wgsl`; line/trail widths
and particle radii remain world-space quantities. Every polygon is projected
through the shared view-projection and clipped to the WebGPU frustum before
pixel conversion. Instance linear RGB becomes display sRGB; straight alpha is
preserved, including the configured background. Existing screenshot SVG behavior
is unchanged. Particle discs use a 64-sided vector approximation, not GPU
lighting/antialiasing; normal prepared 2D CLI scenes have no eligible particles.

**Pinned by.** `tests/cli_svg_test.rs`: shader geometry, style/parameter/opacity
mapping, display color, world widths/radii, clipping, reference visibility,
portrait/landscape fit, deterministic bytes, no raster image, and 3D rejection.

#### `src/cli_gpu.rs` — native surface-free GPU export

**Purpose / API.** `cli::render_offscreen(&PreparedScene)` requests a headless
wgpu adapter/device and returns packed RGBA8. `HeadlessRenderSession::new`
initializes one adapter/device/`DrawState` and `render_frame` can paint many
scenes; `render_offscreen` remains the one-shot still-PNG wrapper. `render_frame`
takes the optional mapped objects and the committed `ObjectStyle`, sets the
style on the draw state and calls `render_offscreen_with_objects`; `None` keeps
the existing no-objects path. `AnimationRenderSession` forwards each frame's
`object_scene`/`object_style`. It does not create a surface, window, app or
persistence state. PNG encoding/writing belongs to CLI dispatch.

**Invariants.** Dimensions, packed/padded pixel bytes, storage sizes, vertex
counts and adapter/device limits are checked for every frame before resource
allocation. A streaming session requests the adapter's storage/buffer maxima so
later frames are not limited by the first frame, then checks each frame against
the device's limits; the one-shot still wrapper sizes to its frame, readback and
any carried object vertex buffers. Both request the vertex/sampler/heatmap
limits `DrawState` unconditionally uses, and an object-carrying still also sizes
for the static primitive vertex buffer. No optional device features are
requested. The target must
support rendering, blending and copying `Rgba8UnormSrgb`; no format or CPU/noop
fallback is allowed. The existing `DrawState` paints the prepared frame with its
linear clear color; readback is display-encoded sRGB. GPU validation/internal/OOM
scopes, uncaptured errors and device loss become actionable CLI errors. The
doc-hidden production `offscreen_error_to_cli` preserves render versus typed
readback classification. Two further doc-hidden production seams support
GPU-free failure checks: `validate_gpu_limits` delegates to the same private
resource-size validator used for rendering, and `acquire_device_with` invokes
the actual device request once and preserves its original cause in a contextual
`CliError::Device`.

**Pinned by.** `tests/cli_gpu_test.rs` covers invalid dimensions, reduced texture,
readback/storage/binding limits, injected device-request failure, exactly-once
acquisition success/failure, actual map callback failure, and the env-gated
`VFE_OBJECT_GPU_PROBE=1` CLI PNG object/style probe (non-clear pixels and a
Wireframe/Solid difference; ordinary runs print `GPU not requested`).
`tests/cli_animation_test.rs` pins that one animation export opens exactly one
session through the injected factory and streams the scheduled frame count.
`scripts/test-cli-gpu.sh` runs isolated displayless workers for
injected adapter absence, real 2D/3D dimensions/field content, PNG decoding, exact
clear/linear-alpha-blend channel bytes, and the `VFE_OBJECT_GPU_PROBE=1` CLI PNG
object/style probe, plus software Vulkan when installed.
The shared suite never mutates display environment; ordinary worker returns
without GPU execution are not runtime proof.

#### `src/app/mod.rs` — application root and shared state

**Purpose.** Defines `VfApp`, `StatusBarState`, the two app-level constructors,
and the helpers that belong to no single frame phase.

**Public API.**
- `VfApp` (fields are mostly `pub` for the integration tests and sibling
  modules): `state`, `bar`, `sidebar`, `status`, `fps_meter`, `camera`, `orbit`,
  `fly`, `fly_keys`, `presets`, `profiles` (`ProfileLibrary`), `style`,
  `layout`, `layout_mode`, `sheet`,
  `material` (`MaterialPanelState`), `maxwell` (`MaxwellPanelState`), `clock`,
  `screenshot`, `animation`
  (`AnimationDialogState`), `animation_job`, and private
  scene/click/particle caches, including the
  `ode_lines`/`last_ode_epoch`/`ode_autofit_pending` Ode cache, the
  `scene_frame` key and particle-availability flag reconciled by
  [`src/app/plane.rs`](#srcappplaners--scene-frame-transitions), and the private
  in-memory `history`/`last_committed` pair driven by
  [`src/app/history.rs`](#srcapphistoryrs--edit-history-wiring).
- `VfApp::new()` and `VfApp::new_with_storage(Option<&dyn eframe::Storage>)`.
- `VfApp::new_present(PresentConfig)` (native measurement entry; `None` storage)
  and the `#[doc(hidden)] present_mode_active()` test seam.
- `StatusBarState` (parse message, fps/frame ms, arrow/streamline/particle/trail
  counts, density, `is_3d`, sim time, flying, hint, animation progress).
- `presets_data_dir()` (native re-export of the XDG directory).
- `Default for VfApp`.

**Invariants.**
- `new_with_storage` restores `AppSettings` through the viewmodel setters before
  the first frame; `parameters`, `viewport_style` and `layer_styles` are then
  assigned directly, so restoring them does not dirty the scene. The persisted
  user profiles build `VfApp::profiles`; that assignment neither applies a
  profile nor dirties the scene. The active interaction record builds
  `VfApp::interaction`, and the persisted user interaction profiles build
  `VfApp::interaction_profiles`; neither restore applies a profile or dirties
  the scene. The persisted named seed layouts build `VfApp::seed_layouts`
  without applying a layout or dirtying the scene. A restored
  non-default domain is recentred on the correct
  camera (`recentre_on` in 3D, `recentre_on_2d` in 2D). It passes the restored
  reference flags into `compose_lines_with_frame` for the initial private scene;
  dirty rebuilds and click recompositions use that same frame-aware path.
- `MAX_CLICK_SEEDS = 6`: click seeds are oldest-first and the oldest is dropped
  when a seventh is added.
- `compose_lines_with_frame` prepends only the enabled reference groups (world
  or local slice) and then the cached flow and click segments; the status bar's
  streamline count is `flow_lines + click_lines`, not the full scene line count.
  `compose_lines_with_frame_style` takes the same arguments plus the resolved
  reference `ViewportStyle`; the legacy entry point delegates with the default
  style, and live/capture/CLI paths pass `state.reference_viewport_style()`.
- `scene` is a cache; `State` remains the source of truth.
- `scene_color_facts` derives the shared range, colour stats, clip counts and
  active mapping from the same drawn arrows; `clips` is computed with
  `ColorMapping::clip_counts` when a mapping is active and defaults otherwise.
- `camera` is the 2D preset mirror (`Camera { position, scale }`), not the live
  painting camera; `orbit` is live.

**Pinned by.** `tests/persistence_test.rs` (round-trip and app restore),
`tests/status_bar_test.rs` (the readout type), `tests/fly_camera_test.rs`
(app-level fly/preset behavior), `tests/responsive_test.rs` (layout/status).
There is no dedicated `app_test.rs`; the shell itself is exercised through the
seams it calls.

**Limitations.** Click-seed history is app-local and bounded; it survives a
rebuild but not a restart. Edit history is the separate bounded document stack
in [`src/app/history.rs`](#srcapphistoryrs--edit-history-wiring); it is
memory-only too.

#### `src/app/history.rs` — edit-history wiring

**Purpose.** Issue #86 slice A2: capture and apply the document snapshot, drive
the gesture/transaction frame pass, route undo/redo, and implement the four
scoped resets without touching persistence or the renderer.

**Public API.**
- `VfApp::undo_document(&egui::Context) -> bool` and
  `redo_document(&egui::Context) -> bool` (the one undo/redo seam).
- `VfApp::reset_scope(EditScope)` — the one scoped-reset implementation behind
  the Edit menu's colour/simulation/document actions and the View menu's
  `Reset camera` action. The pure menu seam only reports
  `SideEffect::ResetScope(scope)`; it does not duplicate any reset.
- App-private `capture_document(&State, &OrbitCamera) -> EditSnapshot` and
  `apply_document(&EditSnapshot, &mut State, &mut Controls, &mut OrbitCamera,
  &mut Option<FlyCamera>)`; the snapshot is `ViewRecord` + `ParameterSet` +
  `SceneCamera` + `AppearanceProfile`.
- Doc-hidden test seams: `history_len`, `history_redo_len`,
  `capture_document_for_test`, `apply_document_for_test`,
  `apply_menu_action_for_test`.

**Invariants.**
- One snapshot is captured from one `State`, so the two colour copies agree.
  Apply restores `view` first, then parameters through `commit_parameters`,
  then the camera with the saved-scene clamps (pitch ±1.55, distance, FOV and
  ortho-zoom constants; fly left), then the appearance profile from the same
  source. No file IO, preset write or profile-library mutation.
- `drive_history` runs once after the UI: a primary-button press opens one
  transaction, every frame while any button is down is suspended, and the
  primary release commits at most one entry. A secondary press/release is not a
  gesture boundary and can neither commit nor split the primary gesture.
  Outside a gesture any document change is recorded directly. Idle frames,
  simulation ticks and save/export paths record nothing. Applying a named seed
  layout commits the seeds once; saving, deleting or importing library entries
  touches settings-side data only and records nothing.
  `tests/edit_history_app_test.rs::library_actions_do_not_record_history_but_applying_does`
  pins that split.
- Every idle frame captures a `ViewRecord`/`ParameterSet`/`AppearanceProfile`
  and compares it with `last_committed`. That per-frame clone-and-compare cost
  is accepted for this slice; the intended cheap path is a dirty/fingerprint
  gate, not a heuristic.
- Undo/redo are inert while a text field has focus, cancel any open gesture and
  refresh `last_committed` so applying a snapshot is not re-recorded as an
  edit. Fly pose, time, particles and layout are never in a snapshot.
- `Color`, `Camera` and `All` reset one document scope and are recorded once by
  the driver; `Simulation` resets runtime state (clock time zero, particles and
  click bursts cleared) and is undo-free because the snapshot cannot see it.
  `State::reset_color_scope` bumps `plot.generation` once and marks dirty once;
  `State::reset_document` replaces the state with a fresh 2D session.

**Pinned by.** `tests/edit_history_app_test.rs`: one real DragValue gesture is
one entry and undo restores the value, idle/simulation frames record nothing,
settings save and export queueing leave history and cursor alone, a rejected
parameter commit records nothing, each scoped reset preserves the others, and
`apply(capture(document))` is a no-op for a changed document.

**Limitations.** Memory-only: history never survives a restart and is absent
from `AppSettings`. Scalar plot navigation and the four reset menus are wired;
open sheets, panel widths and preset-library contents are not document state.

#### `src/app/plane.rs` — scene-frame transitions

**Purpose.** Keep the cached vector scene and its click seeds attached to the
right frame, and clear/refit when that frame changes.

**Public API.** `SceneFrameKey { World3D, World2D, Plane { axis, index,
plane_grid, domain } }` with `of(&State)`; crate-visible
`VfApp::sync_scene_frame`; doc-hidden `VfApp::pending_burst` test seam.

**Invariants.**
- `advance_simulation` calls `sync_scene_frame` before `rebuild_scene` and
  `advance_particles`, so a transition builds and advects in the new frame in
  the same step.
- When either the previous or next key is a plane and the keys differ, the app
  clears click seeds, the last-seed undo record, `flow_lines`/`click_lines`, the
  streamline segment count, and the particle cloud/target, then marks the state
  dirty. An ordinary 3D or two-component 2D formula change keeps click seeds.
- A three-component-to-two-component formula change (`World3D` to `World3D`)
  still loses particle availability; that path clears the cloud and only a
  particle-burst undo record, leaving streamline seeds intact.
- `Fit` runs only on plane entry or an axis/index change, through the local
  `view_context` and `apply_view_command` (which leaves fly); density/domain
  edits inside the same plane keep the current camera.

**Pinned by.** `tests/plane_particles_test.rs`
(`plane_transition_clears_seeds_and_refills_particles`,
`plane_axis_change_clears_stale_click_seeds`,
`world_mode_keeps_existing_click_seeds`, `plane_enter_fits_local_camera`,
`losing_particle_availability_clears_the_cloud_and_burst_undo`).

#### `src/app/plot.rs` — scalar plot integration

**Purpose.** Reconciles plot-kind transitions, owns the transient plot camera
and remembered vector pose, installs the isolated `PlotDrawState` callback,
routes plot navigation, and guards scalar SVG / dispatches scalar PNG capture.

**Public API.** `VfApp::fit_plot`, the doc-hidden `VfApp::test_harness`
integration seam (including `material_arrow_count`, `material_line_count`,
`material_sampling_error`, `material_scene` and
`test_assemble_viewport_scene`), and `SvgExportRoute` for the final SVG
execution guard.

**Invariants.** Scalar mode pauses rather than deletes vector particle/trail/
time state; the live and captured plot share one `PlotScene` Arc and the source
viewport's frame aspect; Reset cancels tagged capture completions and releases
plot targets before the next vector frame. The scalar viewport pinches/drags
through `PlotCamera::apply_gesture` with its own plot scale semantics, so the
vector `CameraFeel` multipliers and invert flags do not apply there.

**Pinned by.** `tests/plot_app_test.rs`; `tests/plot_renderer_test.rs` covers
the renderer configuration this module installs.

#### `src/app/frame.rs` — eframe entry point and save

**Purpose.** Implements `eframe::App` for `VfApp`: the frame phase order and the
persistence write.

**Public API.** `impl eframe::App for VfApp { fn ui(...); fn save(...) }`.

**Invariants.**
- `style_panel::apply` runs before any widget.
- Layout mode is decided from the root width before panels consume width; a
  mode change requests one more frame, and Wide closes any stale compact sheet.
- Phase order is input → chrome → simulation → status → viewport → overlays.
- `save` writes exactly `AppSettings { style, layout, view:
  ViewRecord::capture(&state) }` plus the durable parameter, viewport-style,
  layer-style, object-style, display-only up-axis, user-profile, interaction and
  named-seed-layout records; the
  user profile libraries come from `VfApp::profiles.user()`,
  `VfApp::interaction_profiles.user()` and `VfApp::seed_layouts.user()`.
  Cameras and fly state are intentionally absent.

**Pinned by.** `tests/persistence_test.rs::settings_round_trip_through_eframe_storage`,
`tests/fly_camera_test.rs::the_fly_camera_is_not_persisted`,
`tests/responsive_test.rs` (layout mode behavior), and
`tests/persistence_test.rs::compact_mode_and_sheet_are_never_written_to_storage`.

**Limitation.** eframe calls `save` on its own schedule; the code does not force
a save on every edit.

#### `src/app/input.rs` — input dispatch and click side effects

**Purpose.** The frame's input phase and the app-side click/seeding logic.

**Public API (crate-visible).** `frame_input`, `apply_menu_action`,
`view_context`, `apply_view_command`, `update_fly_keys`, `sync_fly_capture`,
`apply_seed_shortcuts`, `seed_click`, `apply_click_action`, `click_world_seed`,
`undo_last_click_seed`, `seeds_editable`, `seed_pointer_interaction`,
`pick_seed`, `select_seed_at`, `begin_seed_drag`, `update_seed_drag`.
Doc-hidden test seams `test_seed_click` and `test_update_seed_drag` expose the
same production entry points because the headless viewport returns before
`seed_pointer_interaction`.

**Invariants.**
- Input order: screenshot drain → focus-loss fly exit → menu shortcuts → Space
  transport → S/P seeds → clock advance → camera keys → `?`/Esc. Menu and
  transport chords therefore win over camera keys.
- Menu shortcuts are consumed before widgets; `Ctrl+Q` is global, every other
  shortcut is inert while a text field has focus.
- Camera keys use `view_command::view_key_commands`; they are inert while
  typing.
- Fly keys are accumulated from `Event::Key` (physical QWERTY position first),
  cleared on focus loss, Esc, and any Ctrl/Cmd/Alt chord.
- Pointer capture is edge-triggered by `capture_transition`; native sends
  `CursorGrab`/`CursorVisible`, wasm calls the pointer-lock bridge. A text field
  or a lost web lock tears fly down.
- A left click runs exactly one of `ClickActionPlan`: seed one streamline, add
  one particle burst, or set a one-line hint. The pure plan owns the gates.
- `click_world_seed` maps the point through the active `ViewCamera` (orbit or
  fly) via `streamlines::click_seed`; 2D passes the padded plane size.
- A double-click calls `undo_click_seed` (0.3 s window) before resetting the
  view, so the first click's burst is taken back.
- A keyboard seed clears `last_click_seed`, so a later double-click cannot undo
  an unrelated mouse click.
- Issue #87 S2: when formula streamlines are on, a press that lands on an
  explicit seed (12 pt screen radius) selects it and begins a drag; the camera
  suppresses its own gesture for that press. Dragging maps the pointer onto the
  active `PlaneFrame` and clamps the local `u`/`v`, or uses the existing
  camera-plane pick on the scene plane; non-finite/out-of-domain moves are
  rejected and leave the seed where it was. The drag is one existing
  press/release history transaction. Fly or pointer capture activating mid-drag
  clears the drag and ends the gesture instead of writing under the camera. A
  click that hits a seed selects it instead of planting a runtime click seed;
  "Place seed" adds one document seed and disarms, and the armed flag is cleared
  whenever the Flow editor is not visible. Click bursts and explicit seeds
  compose.

**Pinned by.** `tests/menu_test.rs` (dispatch and focus rules),
`tests/shortcut_test.rs` (S/P and seed point), `tests/left_click_action_test.rs`
(plans, hints, rollback interaction), `tests/fly_camera_test.rs` (capture, keys),
`tests/view_command_test.rs` (undo and click outcomes).

**Limitations.** `frame_input` itself has no headless test; it relies on the pure
seams. Click seeds are capped at 6 and are not persisted.

#### `src/app/panels.rs` — chrome panels, compact sheet, overlays

**Purpose.** Draws the top bar, formula bar, transport strip, side panels/rails,
compact tab bar and bottom sheet, status-bar phase, and the overlays.

**Public API (crate-visible).** `frame_chrome`, `frame_status_bar`,
`frame_overlays`, plus private `show_*` helpers and `recentre_domain`.

**Invariants.**
- Wide layout shows both side panels (or rails) and no compact chrome; Compact
  uses `layout::compact_plan` to hide both panels, swap the top bar, and show
  one bottom sheet. The left rail strip orders Controls, native CAS and Sources.
- The right side is ONE rail strip ordering Presets, Appearance and Interaction
  above the expanded right bodies; the strip is drawn first so it stays the
  outermost right column, and it reserves exactly one `RAIL_WIDTH` in the
  right-chrome budget. Each entry toggles only its own `LayoutState` flag.
- The right Appearance body and compact Appearance tab draw
  `appearance_panel::show`; the right Interaction body and compact
  Interaction tab draw `interaction_panel::show` against
  `VfApp::interaction`/`VfApp::interaction_profiles`. An armed rebind capture
  is cancelled whenever the Interaction body is not drawn.
- The Sources rail/body and the compact Sources tab draw
  `sources_panel::show`; the formula bar's one-shot `open_sources` request
  expands the wide rail or opens the compact sheet, then clears.
- The ODE rail/body and compact ODE tab draw `ode_panel::show`; the formula
  bar's one-shot `open_ode` request expands the wide rail or opens the compact
  sheet. Step, Fit and Reset dispatch to `VfApp::ode_step`, the shared Fit
  command and `State::reset_ode` plus the default Reset command.
- The Materials rail/body and compact Materials tab draw
  `material_panel::show`; the formula bar's one-shot `open_materials` request
  expands the wide rail or opens the compact sheet. Fit uses the shared Fit
  command; Reset calls `State::reset_material`, re-syncs the panel draft and
  applies the default camera command. Entering Materials syncs the panel draft
  from the committed scene only on entry, restore or reset, never every frame.
- The Maxwell rail/body and compact Maxwell tab draw `maxwell_panel::show`; the
  formula bar's one-shot `open_maxwell` request expands the wide rail or opens
  the compact sheet. Fit uses the shared Fit command; Reset calls
  `State::reset_maxwell`, re-syncs the draft and applies the default camera
  command; `Switch to 2D` only calls
  `set_mode(false)` and re-samples the same case, recording the new scene frame
  first so the entry refit cannot move the camera. Entering Maxwell syncs the
  panel draft only on entry, restore or reset.
- ODE mode status reports `State::ode_error`; the Features controls are
  disabled with the ODE note and the Coloring section is not drawn. Materials
  status reports `State::material_error` or the current sampling warning; its
  Features controls are disabled with the arrows-only note and its Coloring
  section is hidden. Maxwell reports `State::maxwell_error` or the current
  sampling warning with the same Features/Coloring gates. CAS is disabled
  outside Formula, including Maxwell.
- The viewport paints `Materials - {case} - {Total|Applied|Response}` in
  Materials mode and shows `Response is identically zero for this view.` when
  the sampling is response-empty. Leaving Materials closes its compact sheet.
- The Controls panel is drawn before the dirty rebuild, so a control edit lands
  in this frame's snapshot and counts.
- `recentre_domain` follows a domain change: 3D to the new centre, 2D in x/y
  only; a flying camera translates by the centre delta. Orientation, distance,
  FOV and 2D zoom are untouched.
- The Presets panel's camera mirror is updated before "Save current view" reads
  it; `take_applied` moves one saved camera into the live orbit camera and
  leaves fly.
- Compact tab taps toggle exactly one sheet; the sheet is capped by
  `layout::sheet_height`; a swipe down past 24 pt or a tap on the handle closes
  it.
- Overlays paint last. The help overlay is suppressed in Compact below 520 pt of
  viewport width; the full Help window remains reachable from the menu. In
  Sources mode a small viewport line shows `sources_panel::status_text`.
- The Features section is hidden in Sources mode; the exact formula-field note
  is the one-line replacement, and the Sources panel repeats it.

**Pinned by.** `tests/layout_test.rs`, `tests/responsive_test.rs`,
`tests/persistence_test.rs` (restored domain framing),
`tests/ui_preset_test.rs` (camera handoff), `tests/sidebar_*` (Controls body).

**Limitations.** Compact mode is width-only (no height breakpoint); the help
overlay is desktop-sized. No test drives the whole panel assembly end to end.

#### `src/app/simulation.rs` — clock, particle step, scene rebuild

**Purpose.** The dirty rebuild and the per-frame particle simulation.

**Public API (crate-visible).** `advance_clock`, `frame_simulation`.

**Invariants.**
- `advance_simulation` branches to `advance_ode` before the scalar/vector paths:
  it lazily solves a pending Ode scene, rebuilds trajectory lines once when the
  epoch or dirty flag changes, auto-fits the first solution after mode entry,
  resets the clock time on a new epoch, and never samples the field, retraces
  clicks, spawns particles or appends source/induction glyphs.
- Materials branches before the formula/scalar path: it runs
  `sync_scene_frame` and the dirty rebuild, then returns. It never samples the
  formula, advects particles, retraces clicks, appends glyphs or refreshes the
  scalar plot; the pure snapshot already carries material arrows and outline.
- Otherwise `advance_simulation` runs `sync_scene_frame` first (see
  `src/app/plane.rs`), then the dirty rebuild, then particle advection.
- Ode's `advance_clock` clamps the shared clock to the solution duration and
  pauses once; `state.time` mirrors the clamped value. The doc-hidden harness
  adds `ode_step`, `ode_line_count`, `ode_marker_count` and `ode_epoch`.
- `rebuild_scene` calls `snapshot_if_dirty`; `None` leaves the last scene
  untouched. On `Some`, it copies style/mode, arrows, trails, dots, combined,
  and recomputes `dots_range`. Grid streamlines are unwrapped from the snapshot;
  click bursts are retraced against the new domain/formula and then composed
  with the backdrop by `app::compose_vector_lines`, which also appends the
  probe wireframes and then the mode-owned glyphs and reports only the glyph
  count. The Formula branch appends no glyphs but still paints stored probes.
  `app::compose_vector_lines_blueprint` is the sibling SVG entry point: same
  single composition implementation, but neutral layer records and the full
  legacy glyph expansion regardless of `ObjectStyle`. `source_glyph_line_count`
  and `viewport_line_count` expose the live result to the doc-hidden
  integration harness.
- `advance_particles` calls `retarget` only with the current target. A changed
  target trims or spawns only the difference; a click burst above the count
  survives until the count changes. Off or a two-component field produces
  `None`, clears the cloud, and empties trails; a 2D plane slice is a valid
  target and advects through the local projected flow.
- Advection uses `clock.step_dt()`; a paused clock is a zero step and leaves the
  cloud exactly where it was.
- Dots and trails use the same `dots_range` and the same `speed_range` so colour
  modes agree.
- `transport::request_particle_repaint` keeps egui painting only while a
  consumer exists and the clock is playing.

**Pinned by.** `tests/viewmodel_test.rs` (dirty/rebuild, trails-empty snapshot),
`tests/particles_test.rs` (retarget/advect/colour/trail),
`tests/particle_playback_test.rs` (clock/repaint), and
`tests/material_app_test.rs` (Materials arrows/outline-only branch, caps, plane
slice reuse, error retention and mode isolation).

**Limitations.** Grid and click streamlines are retraced on any rebuild; this is
deliberate but can be expensive at high density. Particle state is CPU-side and
not persisted.

#### `src/app/viewport.rs` — GPU viewport, clicks, screenshots

**Purpose.** The central panel: renderer initialization, camera input, click
handling, scene ordering, screenshot capture, and the fly crosshair.

**Public API (crate-visible).** `frame_viewport`; private
`draw_webgpu_fallback`, `reload_page`.

**Invariants.**
- `DrawState` is inserted into `callback_resources` exactly once
  (`renderer_ready`).
- Fly mode owns the camera directly (WASD/QE, drag/raw look, wheel speed); orbit
  mode feeds `viewmodel::apply_camera_input` through `drag_channel` and
  `two_finger_pan`.
- Camera feel applies to the vector viewports only: 3D orbit, 2D pan/zoom and
  fly look scale through `feel_orbit`/`feel_zoom`. The scalar plot viewport
  keeps its own `PlotCamera` scale control and deliberately ignores camera
  feel.
- A two-finger gesture suppresses the pointer-drag orbit channel; pinch zoom is
  folded into `top_bar::wheel_zoom` by egui.
- 3D builds the combined arrow+particle stream and sorts it far-to-near; 2D
  sorts only arrows and never reads `combined`; 2D plane-slice trails are sorted
  and painted behind them, while ordinary two-component 2D has no trails. The
  ordering lives in `app::assemble_vector_scene`, shared with the detached
  animation snapshot.
- Ode appends current-time marker dots to a copy of the 2D `dots` list or to
  the 3D combined stream before the sort. The cached scene is never mutated;
  markers use `PARTICLE_KIND` and the fixed ODE palette.
- The cached object scene is replaced when `MeshScene::cache_matches` fails,
  which compares the content `generation()` **and** the slice `clip()`. The
  fingerprint omits the clip, so generation alone would retain the 2D clip in
  3D (and the unclipped 3D scene in 2D) across a mode switch (#207).
- The screenshot runs before the paint callback, using the same `scene`,
  `view_proj` and eye position, so the capture is the displayed frame. A
  request is consumed once per capture.
- The crosshair paints only while flying with capture active.
- If eframe has no wgpu render state, a readable fallback is painted (with a
  Reload button on wasm); there is no alternate renderer.

**Pinned by.** The pure pieces have direct tests
(`tests/view_command_test.rs`, `tests/arrow_alpha_test.rs`,
`tests/line_alpha_test.rs`, `tests/screenshot_test.rs`); the GPU/window path is
covered by `scripts/smoke-run.sh` and the CI smoke job. There is no headless
test of `frame_viewport` itself.

**Limitations.** No depth buffer; ordering is CPU sort. No MSAA. The fallback is
informational only.

### 3.2 Field, domain, sampling, presets, colour, plane

#### `src/field.rs` — formula parsing and evaluation

**Purpose.** Parse `(u, v)` / `(u, v, w)` into `meval` expressions and evaluate
them at a point.

**Public API.** `FieldExpr { dimension, .. }`, `FieldExpr::parse`,
`FieldExpr::eval3`, `FieldExpr::eval2`, `FieldError::Parse`.

**Invariants.**
- Outer parentheses are optional list delimiters; commas split only at top level
  (nested parentheses are respected).
- Exactly 2 or 3 components are accepted; anything else is `Parse`.
- Evaluation binds `x`, `y`, `z`, and the shared simulated time `t`; unknown
  variables produce `None`.
- A 2D field evaluated at 3D `p` ignores `p.z` and returns `(u, v, 0)`.
- Any NaN/infinite component makes the whole evaluation `None`.

**Pinned by.** `tests/field_test.rs`: `parse_two_component`,
`parse_three_component`, `top_level_comma_only_must_not_split`, `err_on_invalid`,
`eval_nan_becomes_none`, `unknown_variable_not_evaluated`,
`three_component_evaluated_in_two_d`.

**Limitations.** No implicit multiplication (`2x` is invalid), and no symbolic
differentiation in the formula parser; differentiation is the separate native
CAS panel. A formula that references `t` is time-dependent and animates from
the shared clock.

#### `src/field_cache.rs` — bounded field-evaluation cache

**Purpose.** Avoid repeating an identical formula sampling pass when only
non-field state changed (issue #240).

**Public API.** `FieldSampleKey` (formula text, full committed `ParameterSet`,
`Domain`, density, 2D/3D mode, `Option<PlaneFrame>`, `FieldMode`, `ViewOptions`);
`FieldSampleCache::{get, insert, clear, is_empty, sample_count}`; `ArrowSample`
(the `sampling::ArrowData` alias); `SAMPLE_CAP = 64_000`.

**Invariants.**
- At most one entry; `get` is a hit only on exact key equality.
- An insert above `SAMPLE_CAP` is refused and drops the previous entry. The
  worst supported grid is 3D `40³ = 64_000` samples, so the retained bound is
  `64_000 × size_of::<ArrowSample>()` (about 2 MiB for the 32-byte sample).
- Colours are part of an `ArrowSample`, so `ViewOptions` is part of the key.
- Simulation time is absent from the key: time-dependent fields bypass the
  cache entirely, so no time-dependent hit can exist.
- Cloning the cache resets it; it is runtime-only and never serialised.

**Pinned by.** `tests/field_cache_test.rs`: hit path via the doc-hidden
`State::field_sample_passes_for_tests` counter, per-key-component changes,
direct and State-level colour-key misses, animated time misses, static time
advances keeping a hit, oversized refusal, cached/uncached byte equality, and
table-driven setter invalidation.

**Limitations.** One entry only; every miss replaces it. Time-dependent fields
bypass it entirely, so animation never trades correctness for a hit.

#### `src/field_bake.rs` — CPU-baked field lattice (P3 slice 1b)

**Purpose.** Bake a bounded, deterministic lattice of a time-independent
formula field and sample it instead of re-evaluating the formula per particle
when the opt-in is on. Slice 1b wires `SceneFlow::Baked`, the accuracy readout,
runtime lifecycle and persisted opt-in; the exact flow remains the default.

**Public API.** `BakeAxis::{Volume3d, Plane2d}`, `BakeResolution::new` (with
`axis`, `resolution`, `points`, `dims`), `BakeSpec` (formula, parameters,
domain, axis, resolution, field mode, optional plane frame; exact
`PartialEq`), `BakedField::{build, sample, spec, lattice, axis, resolution,
accuracy}`, `BakeAccuracy { probes, max_abs_error, max_rel_error,
divergence_residual, tolerance }`, `BakeSettings { enabled, resolution_3d,
resolution_2d }`, `BakeError` (`TimeDependent`, `UnsupportedMode`,
`Resolution`, `Memory`, `EmptyDomain`), the resolution bounds,
`MIN_/MAX_VOLUME_RESOLUTION`, `MIN_/MAX_PLANE_RESOLUTION`,
`DEFAULT_BAKE_RESOLUTION_3D = 48`, `DEFAULT_BAKE_RESOLUTION_2D = 128`,
`BAKE_TOLERANCE = 1e-3`, and `MAX_LATTICE_BYTES = 3_145_728`.

**Invariants.**
- A time-dependent field is refused, never frozen at `t = 0`; a non-`Formula`
  adapter is refused as `UnsupportedMode`.
- Resolutions are validated, never clamped by `build`: `16..=64` for a volume
  and `32..=256` for a plane. `BakeSettings::sanitized` clamps persisted
  requests into those bounds.
- The byte budget is checked before the resolution bound, so an over-cap
  request returns `Memory` without allocating. `64³ × 12` is exactly the cap.
- An empty or non-finite active extent returns `EmptyDomain`.
- `Volume3d` samples the full world box; `Plane2d` samples `(x, y)` on one z
  layer and ignores the query's z, matching `FieldExpr`'s two-component
  evaluation.
- Sampling is trilinear (bilinear for a plane). Outside the sampled domain,
  non-finite input, a non-finite contributing corner, or a non-finite
  interpolated result all return `None`; a bad corner never becomes NaN motion.
- The lattice is x-major then y then z, the same order as
  `Domain::grid_points`; the same spec and field reproduce identical bytes.
- `build` measures accuracy against the exact field on at most 512 cell-centre
  probes plus at most `8³` interior divergence nodes. Each metric is finite or
  `None`; zero is never fabricated. `exceeds_tolerance` reports a measured
  error above `BAKE_TOLERANCE` instead of hiding it.

**Pinned by.** `tests/field_bake_test.rs`: linear reproduction, bounded and
reported quadratic error, tolerance exceedance, outside-domain `None`,
time-dependence refusal, non-finite corner handling, lattice determinism,
memory/resolution/empty-domain rejections, setter invalidation,
`ViewRecord`/history absence, a coarse bake changing particles and streamlines
while disabling restores exact bytes, the current-frame plane rule after a
density re-clamp, and baked plane projection matching `PlaneFlow`.
`tests/bake_ui_test.rs` pins the accuracy caption and refusal status copy.

**Limitations.** Model only. The opt-in call-site switch, UI note and export
provenance belong to slice 1b; GPU compute advection is slice 2.

#### `src/ode.rs` — bounded ODE workbench model and fixed-step RK4

**Purpose.** Owns the pure ODE workbench seam: scene validation, typed-equation
or current-field RHS preparation, bounded fixed-step RK4, deterministic samples,
padded bounds, stop status and playback marker interpolation. It is CPU data
only; it does not sample the field grid, use a `PlaneFrame`, or touch a renderer
type.

**Public API.** `OdeSource`, `OdeSeed`, `OdeScene` (with `Default`,
`validate`), `OdeStop`, `OdeSample`, `OdeTrajectory`, `OdeBounds`,
`OdeSolution` (with `sample_at`, `markers_at`, `view_domain`), `OdeError`,
`OdePrepared`, `OdeChart`, `ODE_STEP`, `ODE_MAX_SAMPLES`, `ODE_MAX_DURATION`,
`ODE_DEFAULT_DURATION`, `ODE_MAX_SEEDS`, `ODE_MIN_DIMENSION`,
`ODE_MAX_DIMENSION`, `ODE_COLORS`, `ODE_LINE_WIDTH_FACTOR`,
`ODE_MARKER_RADIUS_FACTOR`, `prepare_ode`, `integrate_ode`, `solve_ode`, the
hidden `integrate_ode_with_step`, and the pure renderer-seam helpers
`ode_trajectory_lines` / `ode_axes` (`OdeLine` / `OdeAxisLine` tuples).

**Invariants.** Scenes accept 2-3 typed dimensions, at most eight finite seeds,
and a finite duration in `(0, 25]`. `Equations` parses the first `dimension`
entries into a three-component `FieldExpr` with `"0"` for unused slots; entries
outside `dimension` are ignored. `CurrentField` clones the durable
`State::field`, requires `state.field.dimension == scene.dimension` and rejects a
non-empty `state.parse_error`; the durable `State::domain` is snapshotted, never
a plane-local frame. Samples are stored at `n * ODE_STEP`, including `t = 0`;
each seed independently ends `Complete`, `StepLimit` at
`ODE_MAX_SAMPLES`, or `NonFinite` at the last finite sample. Bounds use only
finite points, pad each axis extent by 5% with a `1e-3` minimum, and a flat
`z` gets a symmetric view extent. Markers interpolate linearly between samples
and clamp to `[0, duration]`. `FieldExpr::eval3_at_f64` exposes the f64
evaluation boundary; the solver uses its f64 component seam so RK4 error is not
floored by f32 rounding, while formula sampling is unchanged.

**Pinned by.** [tests/ode_test.rs](../tests/ode_test.rs):
`harmonic_oscillator_matches_the_analytic_circle`,
`damped_oscillator_decays_like_the_reference`,
`rk4_halving_the_step_reduces_error_by_at_least_eight`,
`step_limit_is_explicit_and_bounded`,
`nonfinite_evolution_stops_without_fake_points`,
`current_field_and_typed_rhs_agree`,
`invalid_scene_and_nonfinite_seed_are_rejected`, and
`markers_clamp_and_interpolate_between_samples`. Task 2's app/mode/clock
contract is pinned by [tests/ode_app_test.rs](../tests/ode_app_test.rs).
Performance evidence and host numbers are in
[docs/perf/ode-performance.md](perf/ode-performance.md).

**Limitations.** Fixed-step RK4 only: no adaptive/stiff solver, symbolic
solve, 3D phase cube or parameter sweep. This is a precomputed teaching
trajectory solve, not a per-frame solver. The responsive panel and charts,
mode/state/shared-clock playback, persistence and PNG/SVG provenance are wired
through [src/ui/ode_panel.rs](../src/ui/ode_panel.rs),
[src/app/simulation.rs](../src/app/simulation.rs),
[src/viewmodel.rs](../src/viewmodel.rs) and
[src/export_provenance.rs](../src/export_provenance.rs). Particle RK4 is
untouched.

#### `src/source_scene.rs` — validated source scene, catalogue and sampling adapter

**Purpose.** Owns the editable source list and its scene-wide display settings
without duplicating source math or validation. `FieldMode` selects Formula,
Sources, Induction, Materials, Ode or Maxwell; `SourceDisplay` selects Electric,
Magnetic or Both;
`SourceEntry` pairs
an enabled flag with a `Source`; `SourceScene` holds the entries, display,
softening and accuracy opt-in. The same module turns a scene into drawable
arrows through the real evaluator.

**Public API.** `FieldMode`, `SourceDisplay`, `SourceEntry`, `SourceScene`,
`SourceSceneError`, `SourceKindTag` (with `ALL`, `label`, `family` and
`default_source`), `SourceRecord` (flat serde shape with `from_source` and
`to_source`), and `MIN_SOFTENING = 1e-6` / `MAX_SOFTENING = 1.0`.
`SourceScene` exposes `add`, `remove`, `set_enabled`, `replace`, `reset_entry`,
`reset`, `set_softening`, `set_display`, `set_allow_inaccurate`,
`enabled_sources`, `validate`, `to_records` and `from_records`. `SourceKindTag`
also exposes `of(&SourceKind)`. Sampling adds `SourceSampling`,
`SourceFamilyStatus`, `FamilyDisposition`, `classify_family_result`,
`sample_source_scene`, and the fixed `ELECTRIC_COLOR = [255, 176, 64]` /
`MAGNETIC_COLOR = [64, 196, 255]` used by `Both`. Readable-error helpers
`validate_source`, `enabled_row`, `error_message` and
`scene_error_message` keep raw enum debug out of the panel and map an
enabled-only validation index to the 1-based row the user sees.

**Invariants.** Defaults are Formula, Both, softening `0.1`, opt-in off and no
entries. The catalogue defaults are center zero and axis +Z, charge/current
`1.0`, line length/radius `1.0`, and a solenoid radius `0.5`, length `2.0` and
eight turns. `add` rejects the 65th entry with `Limit { max: 64 }`; `add` and
`replace` validate the candidate scene before committing. Softening must be
finite and within the closed `1e-6..=1.0` range. `validate` copies only enabled
sources through `SourceSet::new` with default `WorkLimits`, then validates
`EvalOptions`; `InputError` maps to typed scene errors including a stable
parameter name. `reset_entry` restores the kind defaults while keeping the
entry's center, axis and enabled state; `reset` restores the whole default
scene. Invalid indices make `remove`/`set_enabled`/`reset_entry` no-ops.
`to_records` flattens every entry (including disabled ones) into the stable
`SourceRecord` kind spellings; `from_records` rebuilds a default scene in list
order, revalidating each record through `SourceSet` and skipping unknown kinds,
non-finite numbers and impossible dimensions. The global reset empties the
scene, clears the sampling cache/notice and returns to Formula; plot-section
reset leaves sources alone.

**Sampling contract.** `sample_source_scene` validates the enabled scene through
`SourceSet::new`/`EvalOptions`, then builds one `Evaluator`. A 3D view samples
`Domain::grid_points`; a 2D view samples the existing `PlaneFrame` local grid,
evaluates in world and projects onto the plane, returning local `(u, v, 0)`
positions. `TargetMet` is drawable; `AccuracyNotMet` is drawable only with the
opt-in; `Unavailable` and non-finite values never are. `Both` tags E amber and B
cyan without summing or sharing one magnitude scale, ignoring any active colour
mapping; a single family colours through `ViewOptions::color_mapping` when one is
set, otherwise the normal colormap over its drawn magnitudes. An enabled family
with no sources is `empty: true` with zero arrows and no error, and invalid scenes
return their typed message with no arrows. Counters track sources, drawn arrows,
target-met, accuracy-not-met total/drawn and unavailable per family.

**Pinned by.** [tests/source_scene_test.rs](../tests/source_scene_test.rs):
`the_default_scene_is_empty_in_formula_mode`,
`all_eight_kinds_have_valid_defaults_and_a_family`,
`add_remove_enable_and_reset_change_only_the_scene`,
`source_limit_stops_the_sixty_fifth_entry`,
`invalid_geometry_is_rejected_with_a_parameter_name`,
`softening_is_bounded_and_never_nan`,
`disabled_entries_are_not_part_of_the_validated_scene`,
`source_arrows_follow_the_active_colour_mapping`,
`source_legacy_colours_are_byte_identical_without_a_mapping` and
`both_display_keeps_fixed_family_colours_with_a_mapping`; source-mode behaviour is
in [tests/source_mode_test.rs](../tests/source_mode_test.rs).

**Limitations.** Sampling is CPU and static. Persistence stores the scene; the
editable panel lives in `src/ui/sources_panel.rs`. Classic field presets return
to Formula without clearing the scene, and native CAS refuses Sources mode. The
user guide is [docs/sources-panel.md](../docs/sources-panel.md).

#### `src/induction.rs` — induction scene, vector-potential E and unit-current cache

**Purpose.** Pure Task 1 seam for the Faraday teaching demo. One axisymmetric
current loop or finite continuous solenoid carries
`I(t) = amplitude*cos(2*pi*frequency*t)`. `B` is the existing softened source
evaluator at unit current, scaled by `I(t)`; `E` is reconstructed from the
softened azimuthal vector potential as `-dI/dt * A_phi(unit current) * phi_hat`
and is never a finite time difference or an inverse-Faraday solve. The two
families remain separate.

**Public API.** `InductionCase`, `InductionScene` (`Debug`, `Clone`,
`PartialEq`, `Default`), `InductionKey`, `InductionPrepared`,
`InductionSampling`, `InductionFamilyResult`, `InductionEstimate`,
`InductionFamilyStatus`, `InductionPointSample`, `induction_key`,
`prepare_induction`, `sample_induction`, `evaluate_induction_point`, and the
frozen caps `MAX_INDUCTION_DENSITY_3D = 20` / `MAX_INDUCTION_DENSITY_2D = 80`.
`InductionScene` exposes `current`, `current_derivative`, `is_animated`,
`validate`, `unit_source` and `unit_vector_potential`.

**Invariants.** The waveform derivative is analytic; `is_animated()` is exactly
`amplitude != 0.0 && frequency > 0.0`. Softening is one positive finite shared
value. The loop potential uses `D_loop^2 = r^2 + R^2 - 2rR cos(phi) + z^2 + a^2`;
the solenoid's analytic axial integral uses
`D_solenoid^2 = r^2 + R^2 - 2rR cos(phi) + a^2` (no `z^2`). Both integrate the
symmetric `[0, pi]` half-range with eight panels of 32-point Gauss-Legendre
nodes and double it; a four-panel pass supplies
`max(|fine-coarse|, 32*EPSILON*|fine|)`. On-axis `E` is exactly zero. The
prepared cache is keyed by case/geometry/domain/grid/plane/softening/accuracy
and stores positions, unit-current E with uncertainty, and compact B
value/uncertainty/stop reason. `sample_induction` only rescales and
reclassifies: E by `dI/dt`, B by `I(t)`. Non-finite or over-target results are
`Unavailable`/`AccuracyNotMet`, never drawable confident arrows; accuracy-not-met
is hidden unless the opt-in is set. `Both` emits E amber `[255,176,64]` then B
cyan `[64,196,255]`, ignoring any active colour mapping; a single family colours
through `ViewOptions::color_mapping` when one is set, otherwise the normal
colormap magnitude range.
Native-only sampling may report the CAS waveform derivative string as
diagnostics; the induced field itself never depends on CAS.

**Pinned by.** [tests/induction_test.rs](../tests/induction_test.rs):
`waveform_and_static_predicate_are_exact`,
`loop_vector_potential_matches_independent_fixtures`,
`solenoid_vector_potential_matches_independent_fixtures_and_curl_matches_b`,
`faraday_contour_matches_flux_rate_for_both_cases`,
`static_current_has_exactly_zero_induced_field`,
`nonfinite_scene_is_rejected`, `validation_rejects_bad_geometry`,
`induction_arrows_follow_the_active_colour_mapping`,
`induction_legacy_colours_are_byte_identical_without_a_mapping`, and the
native-only `cas_derivative_matches_the_analytic_waveform`. Fixtures and their
independent mpmath generator live in `tests/support/induction/`.

**App integration (Task 2).** `State::induction` is the durable scene;
`induction_prepared` caches the unit-current geometry and `induction_sampling`
the current frame. `set_induction_scene` validates before replacing: an invalid
edit records `induction_error`, returns false and leaves the previous scene,
prepared `Arc` and arrows untouched; a valid edit clears the error.
`refresh_induction` compares the `InductionKey`, re-prepares only on a geometry/
domain/grid/plane/softening change, and otherwise rescales the same `Arc`.
Entering Induction preserves the current 2D/3D projection; the stored density is
never destroyed, only sampled against the 20³ (3D) or 80² (2D slice) cap. Induction is a clock
consumer only while `is_animated()`; the app starts no second clock, draws no
particles/streamlines/clicks, and pauses with the existing scalar-plot rule.
`snapshot` returns arrows and empty line/trail/dot geometry; `rebuild_scene`
composes the backdrop, one conductor glyph and those arrows. `ViewRecord`
persists the case token, center, axis, radius/length/turns, amplitude/frequency,
display and opt-in; restore revalidates the scene and reports failures through
`induction_error`. Exports capture `induction_export_provenance` at request
time, covering the model line, accuracy/unavailable counts and missing or
invalid sampling.

**Pinned by.** [tests/induction_app_test.rs](../tests/induction_app_test.rs):
caps and mode preservation, clock consumption and pause, time-only `Arc`
identity, geometry re-prepare, fixed E/B colours, local plane positions, the
invalid-edit retain-last-valid rule, arrows-only snapshots and glyph
composition, density restore on leaving, and mirrored clock time. The model
itself is pinned by `tests/induction_test.rs` as above.

**Limitations.** CPU-only, quasistatic and softened: no displacement current,
retardation or physical singular limit. The cap benchmark measures unit-current
cache preparation, not frame time; it is recorded in
[docs/perf/induction-performance.md](../docs/perf/induction-performance.md).

#### `src/material.rs` — bounded analytic material cases and outlines

**Purpose.** Pure seam for the Materials mode. Four canonical analytic cases are
supported: a dielectric sphere, a planar dielectric interface, a permeable
sphere and a planar permeable interface. Each case exposes `Total`, `Applied`
and `Response`; `response` is exactly `total - applied`. Electric and magnetic
families are selected by case and are never summed. The module is CPU only and
has no renderer, clock, source, persistence or export dependency; the app,
panel, persistence and export adapters are documented in their own sections.

**Public API.** `MaterialCase`, `MaterialView`, `MaterialFamily`,
`MaterialScene` (`Debug`, `Clone`, `PartialEq`, `Default`), `MaterialError`,
`MaterialLine`, `MaterialSampling`, `material_family`,
`material_assumptions`, `material_warning`, `validate_material`,
`material_field`, `material_applied`, `material_response`, `material_outline`,
and `sample_materials`. Constants are
`MATERIAL_DEFAULT_RADIUS`, `MATERIAL_DEFAULT_INTERFACE_OFFSET`,
`MATERIAL_EXACT_HIT_FACTOR`, `MATERIAL_COINCIDENT_SLICE_FACTOR`,
`MATERIAL_MIN_RELATIVE`,
`MATERIAL_MAX_RELATIVE`, `MATERIAL_ELECTRIC_COLOR` (amber `[255,176,64]`) and
`MATERIAL_MAGNETIC_COLOR` (cyan `[64,196,255]`); the exact warning strings are
exported as `MATERIAL_INTERFACE_WARNING`, `MATERIAL_RESPONSE_EMPTY_WARNING`,
`MATERIAL_EMPTY_DOMAIN_WARNING` and `MATERIAL_COINCIDENT_SLICE_WARNING`.

**Invariants.** Relative constants are finite in `1.0..=100.0`, the applied
field is finite and nonzero, sphere radius is finite and positive, and boundary
offset is finite. Sphere formulas use the standard dipole coefficient and
uniform interior value; planar formulas keep tangential E / normal B continuous
(and normal D / tangential H continuous where applicable). The no-contrast
limit returns the applied field and an identically zero response. A point
exactly on the interface returns `MaterialError::Unsupported`; sampling applies
the domain-scaled `1e-9 * max(domain extent, 1.0)` guard before evaluation,
counts `omitted_interface`, and keeps near-interface samples on both sides.
Vectors below `1e-6`, non-finite values and invalid scenes never draw arrows.
`response_empty` is set only for the `Response` view when every finite sampled
response is below `1e-6`. Warnings join in the fixed order interface omission,
empty response, empty/impossible domain, coincident slice. Sampling uses fixed
family colours with no colormap or softening.

**2D and outline.** With `Some(PlaneFrame)`, sampling visits the frame's local
grid, evaluates the same 3D field at `to_world(local)`, returns local positions
and projects the selected family vector with `project`. `material_outline`
draws the 3D sphere's three great circles and applied radial marker, or only the
slice cut circle; it draws the 3D boundary square and normal marker, or the
clipped slice-intersection line. Parallel noncoincident boundary slices draw
nothing; a coincident slice draws the local square and adds the discontinuity
warning. Every returned coordinate is finite.

**Pinned by.** [tests/material_test.rs](../tests/material_test.rs):
`dielectric_sphere_matches_inside_far_field_and_no_contrast`,
`dielectric_boundary_conditions_and_response_are_piecewise`,
`permeable_sphere_and_boundary_match_their_interface_conditions`,
`applied_plus_response_reconstructs_total_for_all_cases`,
`exact_interface_hits_are_unsupported`,
`validation_rejects_bad_constants_geometry_and_applied`,
`sampling_omits_exact_interface_but_keeps_near_interface`,
`no_contrast_response_is_empty_with_exact_note`,
`empty_domain_returns_warning_not_arrows`,
`warnings_join_in_the_deterministic_documented_order`,
`outline_uses_slice_intersection_only`,
`every_design_case_has_a_family_assumptions_and_fixed_colour`,
`two_dimensional_sampling_is_a_projected_three_dimensional_slice`, and
`sphere_and_boundary_fields_are_finite_off_the_interface`. Cap evidence is
[tests/material_bench_test.rs](../tests/material_bench_test.rs) via
`scripts/bench-materials.sh`; results are recorded in
[docs/perf/materials-performance.md](perf/materials-performance.md).

**App integration.** `FieldMode::Materials` and its State adapter live in
[src/viewmodel.rs](../src/viewmodel.rs); the arrows-only scene branch and
outline reach the viewport through `VfApp::advance_simulation`. The responsive
panel and viewport label are in
[src/ui/material_panel.rs](../src/ui/material_panel.rs) and
[src/app/panels.rs](../src/app/panels.rs); persistence and lazy restore are in
[src/persistence.rs](../src/persistence.rs); request-time export lines and
metadata are in
[src/export_provenance.rs](../src/export_provenance.rs). Materials is static:
`has_time_dependent_consumer` is false and no formula/scalar/persisted particle
or CAS path is sampled. The applied/response views use fixed family colours, and
the 2D outline is the slice intersection only.

**Limitations.** Exact catalog only: no general material solver, overlapping
inclusions, anisotropic/nonlinear/dispersive media, conductors, free surface
charge, hysteresis or eddy currents. 2D is a slice of the same 3D case. The
benchmark measures analytic sampling, not frame time or browser performance.
Browser/WebAssembly runtime and manual visual checks were owner-waived for this
delivery and are not claimed as passed.

#### `src/maxwell.rs` — bounded Maxwell teaching catalog

**Purpose.** Pure seam for the Maxwell teaching mode. Nine named ideal cases
cover parallel plates, coaxial electric and magnetic cylinders, a hollow charged
sphere, an infinite surface-current wire, an ideal solenoid, a conducting sphere
in a uniform applied field, an image plane with one point charge, and a 2D
grounded conductor cross-section. Electric and magnetic families are selected by
case and never summed. Amplitudes are normalized display values only: there is
no SI constant engine. The module is CPU only with no renderer, clock, source or
persistence dependency.

**Public API.** `MaxwellCase` (plus `MaxwellCase::ALL`), `MaxwellFamily`,
`MaxwellView::{Field}`, `ConductorOutline`, `MaxwellScene` (`Debug`, `Clone`,
`PartialEq`, `Default`, serde), `MaxwellError`, `MaxwellSampling`,
`MaxwellLine`, `BemConvergence`, `BemSolution`, `MaxwellStop`, `maxwell_family`,
`maxwell_assumptions`, `maxwell_warning`, `validate_maxwell`, `maxwell_field`,
`maxwell_field_with_bem`, `maxwell_outline`, `solve_bem`, `sample_maxwell`, and
`sample_maxwell_with_bem`. `BemSolution` exposes `field_at`, `check_net_charge`,
`convergence`, `matches_scene` and the doc-hidden net-charge test seam. Constants
are `MAXWELL_MAX_DENSITY_3D`, `MAXWELL_MAX_DENSITY_2D`,
`MAXWELL_MAX_BEM_ELEMENTS`, `MAXWELL_BEM_MIN_ELEMENTS`, `MAXWELL_BEM_TARGET`,
`MAXWELL_INTERFACE_TOLERANCE_FACTOR`, `MAXWELL_MAX_RELATIVE`,
`MAXWELL_MAX_AMPLITUDE`, `MAXWELL_DENSITY_CAP_WARNING`, and the exported exact
warning strings.

**Invariants.** Amplitudes are finite and within `±1e3`; geometry is finite and
strictly positive; `outer_radius > radius`; `elements` is even and within
`32..=256`; a BEM applied field is finite and nonzero. Analytic formulas use
`f64` internals and cast once to `f32`; no evaluation returns NaN/Inf. Points
exactly on a discontinuity return `MaxwellError::Unsupported`. The image plane
returns zero for `z <= 0` (the unsupported half-space is not an error) and
rejects only the real charge point; a point just above the plane keeps its
O(`z/h`) tangential component, so boundary tests use a relative tolerance.
`Conductor2D` is `Unavailable` in 3D with the exact "switch to 2D" message. Its
2D solve is a bounded constant-panel collocation BEM supplied by the caller;
`sample_maxwell` never solves, and missing/stale solutions report `Unavailable`
rather than fabricated arrows. `BemSolution::field_at` is exterior-only:
interior and boundary points are rejected with an explicit error, never
extrapolated. A 2D view without a supplied solution uses the exact
`Conductor2D needs a completed 2D boundary solve; no field is drawn yet.`
message.

**2D BEM conductor.** `solve_bem` accepts only `Conductor2D`; `scene.elements` is
the fine panel count (even, `32..=256`) and the coarse diagnostic run is exactly
`elements / 2`. Panels are constant-density with midpoint collocation on the
circle or square outline; the 2D Laplace Green's function `-ln(r)/(2π)` gives a
dense `N²` matrix and Gaussian elimination with partial pivoting (pivot threshold
`1e-12 * row scale`) uses `N³` work. Non-finite or singular systems return
`Unavailable`; solved panels are checked for the grounded isolated-neutral net
charge `|Q| <= 1e-6 * max(|E0| * extent, 1)`. Convergence compares the fields at
a fixed 8-point ring at `4 * max(outline_extent, 1e-6)` with one global
normalization `max_probe(max(|E_fine|, |E0|, 1e-9))`; missing the target is an
explicit `AccuracyNotMet` and draws only with the scene's opt-in. The 2D sampler
omits the conductor interior, boundary singularities, degenerate and extreme
arrows, and joins the BEM approximation, `accuracy not met` and projection
warnings deterministically.

**Sampling.** The 3D grid is sampled in world space; a 2D slice samples the
frame's local grid, evaluates the same world field and projects the selected
vector. Exact interfaces, point singularities, non-finite values and vectors
below `1e-6` are omitted and counted. A finite arrow above
`MAXWELL_MAX_RELATIVE * max(|E0|, |B0|, 1e-9)` is omitted and counted, never
clamped. Warnings are joined in a fixed order: invalid/unavailable, interface
omission, extreme omission, projection suppression, coincident ground slice.
Empty/impossible domains warn explicitly. A coincident `ImagePlane` `z=0` slice
draws no field and warns that the ground slice is empty. Sampling never
densifies the input grid.

**State and app integration.** `FieldMode::Maxwell` samples the durable
`State::maxwell: MaxwellScene` through `refresh_maxwell_sampling`; the app keeps
`maxwell_sampling: Option<MaxwellSampling>`, `maxwell_error: Option<String>` and
a private BEM cache. Issue #97 slice B adds an optional self-contained
`maxwell_wave: Option<WaveSpec>`: `set_maxwell_wave` validates through
`WaveSpec::validate`, no-ops on an equal spec and dirties once on a change;
`restore_maxwell_wave` assigns a validated stored spec without dirtying and
drops an invalid one with one warning. While a wave is enabled,
`refresh_maxwell_sampling` clears any earlier `maxwell_error` so a stale
case/BEM failure cannot be read against the wave arrows; it then samples the
grid (or the selected 2D plane) through `maxwell_wave::sample_wave_grid` and
stores two separate fixed-family sets in `maxwell_wave_sampling`: E in the
electric colour and B in the magnetic colour.
The sets are never summed and never pass through the #83 colour mapping; the
`SceneGPU` arrow list concatenates both so PNG, GUI screenshots and animation
captures include them. An enabled wave replaces the selected case's field arrows
while the selected case's outline geometry remains drawn, and its equation
statuses are evaluated at the current clock time. The SVG blueprint carries no
arrows, so the panel states that limit rather than changing SVG output.
`maxwell_wave()` reads the durable
spec and `set_maxwell_wave(None)` restores the normal case sampling exactly. `set_maxwell_scene` validates before replacing anything, so
an invalid edit keeps the last valid scene and sampling; a valid edit clears the
cache, sampling and error. `refresh_maxwell_sampling` is the only path that may
call `solve_bem`, at most once for a matching scene cache; plane, camera and
sampling refreshes reuse it, and a failed solve is not retried until a scene
edit/reset. The 3D grid is capped at `40^3` and the 2D plane at `200^2`, with the
visible `MAXWELL_DENSITY_CAP_WARNING` when a request is reduced. Two-dimensional
sampling uses the selected `PlaneFrame`; three-dimensional sampling uses the
durable domain. Maxwell is arrows plus the case outline only: no formula
streamlines, particles, trails, dots, click seeds or source/induction glyphs,
and entering the mode preserves the camera and every durable scene. A Maxwell
case is not a clock consumer, but an enabled plane wave is: `set_time` dirties
the scene only while a wave is present, so the one simulation clock animates or
freezes it and no second clock exists. The app branch in `src/app/simulation.rs` composes the outline at
`0.004 * scene extent` width before the formula path; status/sampling errors flow
to the existing status slot. Test seams are `maxwell_arrow_count`,
`maxwell_line_count`, `maxwell_scene`, `maxwell_sampling`, `maxwell_sampling_error`,
`maxwell_bem_solution` (read-only cache access for the learning panel),
`maxwell_bem_solves`, its `maxwell_bem_solve_count` alias, and
`test_assemble_viewport_scene`. `ViewRecord` persists the durable scene and the
optional wave only (no sampling). `restore_maxwell_fields` validates the scene,
restores the self-contained wave through the non-dirtying seam (dropping an
invalid spec with one warning), then `State::restore_maxwell_scene` clears
sampling/BEM/error and marks the first frame dirty without sampling or solving;
a malformed scene payload falls back to the default scene and reports only for a
stored Maxwell mode. Global Reset calls
`reset_maxwell` and returns to Formula. PNG/SVG requests derive
`maxwell_export_provenance` from the scene and cached sampling at request time.

**2D and outline.** `maxwell_outline` draws world geometry in 3D (plates, coaxial
circles, sphere great circles, wire axis and ticks, solenoid rings, image-plane
ground square and charge marker, applied arrow) and only slice intersections in
2D. Circle/square cross-sections and clipped plane intersections use finite
endpoints; coincident interface lines may be drawn even though field sampling
still warns and omits exactly-on-interface samples.

**Pinned by.** [tests/maxwell_test.rs](../tests/maxwell_test.rs):
`plates_match_the_ideal_uniform_field_and_interface_guard`,
`coaxial_electric_and_magnetic_are_azimuthal_and_zero_outside`,
`hollow_sphere_and_solenoid_have_expected_limits`,
`infinite_wire_is_a_surface_shell_with_zero_interior`,
`image_sphere_satisfies_the_conducting_boundary_and_far_field`,
`image_plane_is_zero_on_the_ground_and_has_no_tangential_field_above_it`,
`conductor_case_is_2d_only_and_unavailable_in_3d_sampling`,
`extreme_arrows_are_omitted_and_counted`,
`empty_domain_is_explicitly_invalid_sampling`,
`azimuthal_2d_slice_warns_about_the_suppressed_component`,
`coincident_ground_slice_warns_and_shows_no_field`,
`exact_interface_slice_warns_and_omits_every_sample`, and
`outline_and_validation_cover_supported_geometry`. The BEM path is pinned by
[tests/maxwell_bem_test.rs](../tests/maxwell_bem_test.rs):
`grounded_circle_solve_matches_the_dipole_far_field`,
`square_refines_globally_and_never_reports_identical_coarse_fine`,
`max_elements_still_uses_a_distinct_coarse_run`,
`net_charge_residual_is_explicit`,
`slice_2d_sampling_uses_the_solved_field_and_projection_warning`,
`slice_2d_sampling_without_a_solution_is_explicitly_unavailable`,
`conductor_3d_sampling_is_unavailable_with_the_design_message`, and
`stale_solution_and_non_conductor_scenes_are_explicit` (outline, elements and
applied-field mismatches plus interior rejection). The app/mode path is pinned by
[tests/maxwell_app_test.rs](../tests/maxwell_app_test.rs):
`maxwell_mode_isolated_from_formula_and_durable_scenes`,
`maxwell_sampling_is_arrow_and_outline_only`,
`maxwell_2d_uses_the_selected_plane`,
`conductor_2d_solves_once_and_reuses_the_cache`,
`conductor_2d_3d_is_unavailable_without_solving`,
`invalid_maxwell_edit_retains_the_last_valid_scene_and_cache`,
`maxwell_caps_apply_per_view`,
`maxwell_mode_does_not_consume_the_clock_or_spawn_particles`,
`maxwell_bem_failure_is_reported_and_not_retried_per_sample`, and
`maxwell_round_trip_preserves_projection_and_camera`.

**Limitations.** Closed-form ideal cases only: no 3D BEM/FEM, arbitrary CSG,
full-wave/FDTD, displacement current, radiation, retardation, nonlinear or
anisotropic media. The infinite wire is a thin surface-current shell and the
image cases assume infinite ideal boundaries. The 2D BEM conductor is a bounded
constant-panel infinite-cylinder approximation, not a 3D finite conductor. The
responsive panel/selector/help, persistence, export and the benchmark land in
later tasks.

#### `src/maxwell_learning.rs` — Maxwell equation teaching text

**Purpose.** Pure teaching model for the four Maxwell equations: Gauss's law
for electricity, Gauss's law for magnetism, Faraday's law of induction, and the
Ampere-Maxwell law. Each record carries both the integral and differential
statement in normalized teaching units, the symbol glossary and the limit that
applies. There is no solver, sampling, renderer or persistence in this module.

**Public API.** `MaxwellEquation` (`GaussElectric`, `GaussMagnetic`, `Faraday`,
`AmpereMaxwell`) with `ALL` and `title()`, `EquationForm::{Integral,
Differential}`, `EquationSpec { equation, form, title, integral, differential,
symbols, assumptions }`, and `spec(equation)`. Both text fields are always
populated; `form` is the canonical integral form returned by `spec`.

**Invariants.** Every integral and differential statement names normalized
teaching units and says the catalogue is not a solver. `symbols` names every
symbol used in the statement, including the closed surface `S` for both Gauss
laws. `assumptions` names electrostatic, magnetostatic, quasistatic or full
time-dependent limits explicitly, and the Gauss-electric and Ampere-Maxwell
assumptions state the unit convention `eps0 = 1` and `mu0 = 1` that the
residual diagnostics rely on. No text claims a general time-dependent solve.

**Pinned by.** [tests/maxwell_learning_test.rs](../tests/maxwell_learning_test.rs):
`all_four_equations_have_integral_and_differential_text`,
`specs_name_every_symbol_used_in_the_statement`,
`gauss_surfaces_are_named_in_the_symbol_tables`, and
`assumptions_state_the_unit_conventions_used_by_the_residuals`.

#### `src/maxwell_residual.rs` — Maxwell residual diagnostics

**Purpose.** One-sample-point residual check for the four equations, fed by
already-evaluated terms in normalized teaching units. It separates
sources-to-fields evaluation from user-specified-field diagnostics and never
claims a full time-dependent solve.

**Public API.** `TermStatus::{Analytic, Numerical, Unsupported, Singular}`,
`FieldTerms` (optional `div_e`, `div_b`, `curl_e`, `curl_b`, `rho`, `j`,
`de_dt`, `db_dt`), `Residual { equation, lhs, rhs, vector_residual, normalized,
status, explanation }`, and `residual(equation, &FieldTerms)`.

**Invariants.** Status precedence is fixed: a missing input is checked before a
non-finite one, and the left term before the right term, so a missing term
reports `Unsupported` even when another supplied term is non-finite. A missing
input is `Unsupported` naming the exact symbol and is never substituted with
zero. A non-finite input is `Singular` naming the offending symbol and no value
is reported. Gauss's law for magnetism always has `rhs = 0`. The Ampere-Maxwell
displacement term is included only when `de_dt` is supplied; otherwise the check
is `Unsupported("displacement current (de_dt)")`. The scalar scale is
`|residual| / max(1, |lhs|, |rhs|)`; `vector_residual` and its normalized value
divide every term by `max(1, |largest component|)` before adding or subtracting,
so both stay finite for every finite input even when the raw sum would overflow.
`TermStatus::Numerical` is reserved for slice B sampling estimates and is never
reported by this one-sample-point slice. An `Unsupported` term can never produce
a satisfied verdict, and every explanation states that only one sample point
was checked and the coupled time-dependent system was not solved.

**Pinned by.** [tests/maxwell_residual_test.rs](../tests/maxwell_residual_test.rs):
`gauss_magnetic_residual_uses_a_zero_rhs`,
`consistent_terms_give_zero_residual`,
`missing_terms_are_reported_as_unsupported_not_zero`,
`non_finite_terms_are_singular_and_never_nan`,
`ampere_maxwell_requires_the_displacement_term`,
`normalized_residual_is_finite_and_bounded`,
`huge_same_sign_finite_vectors_keep_both_fields_finite`,
`unsupported_takes_precedence_over_singular`, and
`residual_does_not_claim_a_full_time_dependent_solve`.

#### `src/maxwell_wave.rs` — bounded source-free plane-wave teaching model

**Purpose.** Bounded analytic full-wave illustration for #97 slices A and B: a
single source-free plane wave in normalized teaching units with `c = 1` by
construction. `E = A p̂ cos(k·r − ωt + φ)` and `B = (1/c) k̂ × E` are evaluated
together, and closed-form `FieldTerms` feed [`maxwell_residual`](../src/maxwell_residual.rs)
so both Gauss laws and Faraday's law report `Analytic` while Ampere–Maxwell
honestly reports `Unsupported`. This is not an SI measurement, not a
boundary-value solution, and not a solver for arbitrary sources or current
distributions. Slice B adds only the grid helper used by the viewmodel; the
durable spec, panel, persistence, renderer arrows and clock wiring live in
`viewmodel`, `persistence`, `ui/maxwell_panel` and the app scene path.

**Public API.** `TEACHING_SPEED` (`1.0`), `MIN_WAVELENGTH` (`1e-6`),
`MAX_AMPLITUDE` (`1e6`), `MIN_AXIS_LENGTH` (`1e-9`),
`WaveSpec { amplitude, wavelength, direction, polarization, phase }` with
`validate`, `omega` and `k`; `WaveSample { e, b, phase }`;
`sample_wave(spec, point, time) -> Option<WaveSample>`;
`sample_wave_grid(spec, domain, grid, plane, time) -> Vec<(Vec3, WaveSample)>`
(slice B: the viewport grid or the active 2D plane, with rejected samples
skipped); and `equation_terms(spec, point, time) -> Option<FieldTerms>`.

**Invariants.** `validate` rejects a non-finite or out-of-range
amplitude/wavelength/phase, a degenerate direction, and a polarization
(near-)parallel to propagation, without clamping; it renormalizes `direction`
and projects `polarization` perpendicular to it. `sample_wave` and
`equation_terms` re-check the same bounds and reject a non-finite point or
time, returning `None` rather than NaN. `B = (1/c) k̂ × E`, so `E`, `B` and
`k̂` are mutually orthogonal and `|E| = |B|` when `c = 1`. `equation_terms`
supplies `div_e = div_b = 0`, `rho = 0` (an exact analytic zero for the
source-free wave), `curl_e`, `curl_b`, `de_dt` and `db_dt`; `j` stays `None`
because no free-current distribution is modelled, so Ampere–Maxwell remains
`Unsupported("j")`.

**Boundaries.** `phase` is only finiteness-checked, with no range bound; a very
large phase loses accuracy through argument reduction and any bound would belong
to a panel that exposes it. `validate` writes normalized unit vectors back as
`f32` `Vec3`, so re-validating an already-validated spec re-projects from the
rounded values and may shift unit vectors by `f32` rounding (not bit-idempotent,
but within teaching tolerance). `k` returns `Vec3::ZERO` for an unusable stored
direction as a deliberate no-direction sentinel, not a substitute axis; callers
validate first, and reject-without-substitution remains `validate`'s rule.
`sample_wave_grid` adds no density cap and returns only finite samples: an
invalid spec, non-finite point/time or non-finite field is skipped, never drawn
as NaN.

**Pinned by.** Slice A:
[tests/maxwell_wave_test.rs](../tests/maxwell_wave_test.rs):
`wavelength_and_amplitude_validation_rejects_non_finite_and_out_of_range`,
`polarization_is_projected_perpendicular_to_propagation`,
`wave_is_transverse_and_b_is_orthogonal_to_e_and_k`,
`sampled_e_and_b_are_finite_and_equal_in_magnitude_when_c_is_one`,
`gauss_equations_close_analytically_for_the_plane_wave`,
`faraday_residual_stays_within_the_analytic_tolerance`,
`ampere_maxwell_reports_unsupported_without_sources`,
`non_finite_point_or_time_yields_no_sample`, and
`samples_are_deterministic_and_repeatable`. Slice B:
[tests/maxwell_wave_ui_test.rs](../tests/maxwell_wave_ui_test.rs):
`wave_state_round_trips_through_the_settings_envelope`,
`invalid_stored_wave_is_dropped_on_restore_without_dirtying`,
`set_wave_is_a_noop_for_an_equal_spec_and_dirties_once_on_change`,
`wave_sampling_emits_separate_electric_and_magnetic_arrow_sets`,
`wave_arrows_follow_the_clock_and_freeze_when_paused`,
`wave_arrows_skip_non_finite_samples`,
`wave_panel_shows_validation_messages_and_equation_statuses`,
`wave_capture_includes_the_arrows` (a `SceneGPU`-level assertion, which pins the
same geometry the GUI/PNG/animation paths consume), and
`disabling_the_wave_restores_the_case_sampling`.

#### `src/source_presets.rs` — built-in source-scene presets

**Purpose.** The seventeen illustrative source scenes offered above the saved
camera views in the Presets panel. Most are built from
[`SourceKindTag::default_source`](../src/source_scene.rs) with unit
charge/current; `Electric dipole`, `Unequal charges` and `Opposing wires`
override their strength to `+1`/`-1` through the shared `with_strength` helper,
so the catalogue still adds no new physics and no formula conversion.

**Public API.** `SourcePreset { name, summary, build }`; `source_presets()`;
`source_preset(name)`; `source_strength(&Source) -> f64`;
`load_source_preset(&SourcePreset, &mut State) -> bool`.

**Invariants.**
- The catalogue order is `Coulomb point`, `Electric dipole`, `Charged line`,
  `Uniform ring`, `Thin disc`, `Charged sphere`, `Infinite wire`,
  `Current loop`, `Finite solenoid`, then ticket 91's `Unequal charges`,
  `Off-axis ring`, `Sphere inside and out`, `Paired loops`, `Helmholtz pair`,
  `Opposing wires`, `Solenoid ends`, and `Source gallery` last; names are unique
  and the gallery contains one entry of each of the eight kinds.
- Strengths are finite and illustrative: unit magnitude everywhere, with the
  dipole's and opposing wires' entries at `+1`/`-1`; the unequal-charge
  quadrupole alternates `+1`/`-1` over four point charges. Every scene keeps the
  default `0.1` softening, `Both` display and `allow_inaccurate = false`.
- `load_source_preset` validates the built scene first, then uses the ordinary
  setters to select `PlotKind::VectorField`, `FieldMode::Sources`, a 3D view,
  the canonical `[-2, 2]^3` domain and the new scene. It returns false without
  touching the state when validation fails. Ticket 91 keeps that single
  canonical domain: the off-axis ring and the length-2 solenoid's ends both sit
  inside it, so no per-preset domain field was added.
- Formula presets remain formulas; loading a source preset touches no saved
  formula or camera-preset storage.

**Pinned by.** [tests/source_presets_test.rs](../tests/source_presets_test.rs):
`source_presets_have_unique_names_and_cover_all_eight_kinds`,
`loading_a_source_preset_uses_real_setters_and_a_visible_domain`,
`every_preset_strength_is_the_illustrative_unit_and_finite`,
`legacy_formula_presets_keep_their_mode_and_label`,
`teaching_source_presets_use_the_expected_existing_kinds_and_signs`,
`teaching_source_presets_validate_and_load_inside_the_canonical_domain`, and
`panel_source_preset_loads_the_scene_and_tracks_the_active_source`.

**Limitations.** Presets are presentation examples, not calibrated physical
sources. They do not persist, and the gallery's ring/disc/sphere overlap in the
canonical domain.

#### `src/source_glyph.rs` — pure source-marker glyphs and line geometry

**Purpose.** Turns an enabled, visible [`SourceScene`](../src/source_scene.rs)
entry into a `SourceGlyph` carrying kind, family, signed charge/current, centre,
axis, radius, length, solenoid turns and the marker sizing basis. It never
samples the field. `source_glyph_lines` expands glyphs into ordinary
`LineInstanceGPU` segments for the existing line pass: a three-axis cross,
finite-line ticks, 32-segment rings/discs/sphere great circles, wire chevrons,
right-hand current-loop chevrons and a capped solenoid helix, plus a two-stroke
positive or one-stroke negative sign marker.

**Public API.** `GlyphSign`, `SourceGlyph`, `source_glyphs(scene, domain,
plane)`, `source_glyph_lines(glyphs, domain)`,
`source_glyph_lines_with(glyphs, domain, styles)`,
`source_glyph_lines_with_layer(glyphs, domain, styles, layer)`, plus the #114
split `source_body_lines`/`source_sign_marker_lines` and their `_with_layer`
forms. The legacy `source_glyph_lines` delegates with neutral
[`LayerStyles`](../src/layer_style.rs); `source_glyph_lines_with` fixes the layer
to `Sources`, and the induction conductor calls the layer-aware entry point with
`Induction`. The split functions apply the identical colours, widths, dashes and
per-glyph layer/family resolution: the legacy expansion is each glyph's body
followed by its marker, so `source_glyph_lines` stays byte-identical for SVG and
line-glyph callers while the object-backed path can request only markers.

**Invariants.** Electric kinds appear only for `Electric`/`Both` and magnetic
kinds only for `Magnetic`/`Both`; disabled entries emit nothing. Family colours
are fixed E amber and B cyan, independent of the colormap, unless that family's
resolved colour override is set. A non-neutral layer/family record multiplies
the base alpha and width and may replace the fixed family RGB; mapped colours
are never touched. Alpha is the on-slice `0.9` or off-slice `0.45` times the
resolved opacity, clamped to `0.0..=1.0`; `reference::sanitized_opacity` repairs
the opacity *multiplier* before the multiply (non-finite becomes `1.0`), so a
corrupt value keeps the neutral base alpha instead of brightening to fully
opaque. Width is `0.005 ×` or `0.0035 ×` scene extent
multiplied by the resolved width multiplier, clamped to `0.05..=4.0` with a
non-finite multiplier read as `1.0`. Off-slice lines keep their `4/3 ×
domain_scale` on/off dashes. A neutral record is byte-identical to the legacy
output. With a `PlaneFrame` the centre and axis are converted to local plane
coordinates, `center.z` is zero, and an out-of-plane source is flagged
`off_slice` when its normal distance exceeds `16 × f32::EPSILON × max(extent,
1.0)`. Marker geometry is sized from `domain_scale = 0.05 × max(extent, 1e-6)`
so a degenerate domain still produces finite segments. Glyphs are contextual
overlays, not depth-tested surfaces.

**Known limitation.** Body suppression is scene-level, not per-glyph: once
`object_scene_for_state` maps any object, `compose_vector_lines_with` requests
only sign markers for every glyph. A glyph whose own source produced no
drawable object still loses its body lines, leaving a floating sign marker with
no body and no legacy full-glyph fallback. The triggers are a source that
`MeshScene::from_sources` skips (for example a non-finite centre in an otherwise
mappable mixed scene) and a mapped transform that `ObjectInstance::from_object`
rejects as degenerate. An all-unmappable scene is empty and still takes the
full-glyph fallback, so the limitation needs at least one mapped object.
Per-glyph truth requires carrying a glyph-to-object mapping/index through the
scene and is outside this slice.

**Pinned by.** [tests/source_glyph_test.rs](../tests/source_glyph_test.rs):
`glyphs_cover_every_kind_with_family_colour_and_sign`,
`negative_strengths_choose_the_minus_marker`,
`glyph_center_and_axis_follow_the_source`,
`family_selection_and_disabled_entries_filter_glyphs`,
`off_slice_glyphs_project_and_are_visually_distinct`,
`neutral_glyph_styles_are_byte_identical`,
`source_layer_opacity_multiplies_glyph_alpha`,
`source_layer_width_multiplies_glyph_width`,
`family_colour_overrides_replace_the_fixed_family_colour`,
`induction_layer_applies_to_the_conductor_glyph`,
`layer_beats_family_per_attribute`,
`out_of_range_or_non_finite_glyph_styles_are_defensively_repaired`,
`off_slice_alpha_still_multiplies`, `body_lines_contain_no_sign_marker`,
`sign_markers_emit_two_segments_for_positive_and_one_for_negative`,
`legacy_glyph_lines_equal_body_plus_markers_for_every_kind`,
`off_slice_split_preserves_width_and_dash_expansion`, and
`styled_body_and_marker_lines_apply_the_same_layer_override`.

#### `src/sources/` — validated source model and bounded quadrature

**Purpose.** A no-UI, no-GPU, native/wasm-compatible seam for softened source
fields. `src/sources/mod.rs` re-exports only the public contract;
`src/sources/model.rs` validates and freezes the input;
`src/sources/numeric.rs` carries power-of-two-scaled elementary arithmetic, the
source-local frame and the finite-line kernel/primitive;
`src/sources/quadrature.rs` owns bounded adaptive Gauss–Kronrod partitions;
`src/sources/electric.rs` owns the electric ring, disc, sphere and finite-line
reductions; `src/sources/magnetic.rs` owns the wire, current-loop and finite
continuous-solenoid reductions; `src/sources/evaluate.rs` prepares an evaluator
and builds family results.

**Public API.** `SourceKind` (all eight vectors), `Source`, `AccuracyRequest`,
`EvalOptions`, `WorkLimits`, `SourceSet::new`, `Evaluator::new`,
`Evaluator::evaluate`, `Sample`, `FamilyResult`, `Estimate`, `Diagnostics`,
`StopReason`, `InputError`, `PrepareError`, and the hard-ceiling constants.
Defaults are epsilon `1e-7`, E/B scales `1e-2`, softening `0.1`, 64 sources,
2,000,000 work units/family/point, 8,192 cells/family, depth 48 and 65,536
queries/job. Callers may lower these limits; requests above the documented
ceilings are rejected.

**Invariants.**
- Every center/axis/strength must be finite; used axes must be nonzero and
  dimensions positive; turns must be at least one. Negative and zero strengths
  remain valid. Point/sphere axes are unused, so a finite zero axis is valid.
- `SourceSet::new` polls cancellation before the source copy, rejects
  over-limit lists before copying, validates all data before a fallible
  reservation, and stores a private immutable copy. `Evaluator::new` validates
  options before its fallible copy and two bounded family scratch reservations.
- `evaluate` counts invalid queries, publishes no vector for cancelled/invalid/
  unrepresentable queries, sums E and B separately, and uses compensated
  component, uncertainty and absolute-contribution sums. Empty and zero-strength
  families are exact `ZERO` with zero arithmetic uncertainty and a positive
  absolute target. Every accepted call consumes one hard job-query slot,
  including invalid queries; the call after `max_queries` is
  `Unavailable(QueryLimit)` for both families.
- Each family keeps one complete per-source estimate and refines against the
  terminal family criterion `T = epsilon*S_f + epsilon*norm(V_f)`, never
  `epsilon*sum(|v_i|)`. An epsilon-dependent accuracy pass first targets a share
  of the coarse family target, then the source with the largest absolute
  uncertainty is re-evaluated at the remaining allowance until the criterion
  passes or every source reaches its work/storage/depth/roundoff floor. Failed
  refinements retain the previous complete contribution; no partial integral is
  published. `AccuracyNotMet` therefore carries complete finite coverage, while
  incomplete preparation, an unresolved feature, `NumericRange`, or
  cancellation is `Unavailable`.
- Electric and magnetic families own separate scratch and separate
  `work_per_family` reservations; neither can borrow from or starve the other.
  A difficult family failing does not prevent the other family from completing.
  The `Diagnostics` provenance repeats the kernel ID, family, softening,
  epsilon, family scale, family source count (including zero-strength sources),
  total counted work, cell high-water mark, regularization, roundoff-limitation
  and underflow flags. `source_count` is family-local, not the whole mixed set.
- Elementary point and wire fields are evaluated with max-scaled norms and
  mantissa/exponent arithmetic, so huge displacements do not fabricate
  `NumericRange`; genuine underflow is published with a flag and uncertainty.
  A wire query exactly on its axis returns exact `ZERO` because the azimuthal
  basis is undefined there.
- A finite line covers its whole segment with finite cells. Every unresolved
  cell is split at the projected support location or midpoint until its physical
  width is at most half the conservative softened distance to the segment; an
  exhausted depth/cell/work budget publishes `Unavailable`, never a partial
  vector. Off-axis evaluation uses GK15/31 and both half-cell GK31 estimates;
  the half-cell sum is the estimate and `8*max(|K31-G15|, |K31-halfsum|)` is the
  absolute uncertainty indicator. Refinement replaces the leaf with the largest
  absolute uncertainty (stable lower-bound tie break) only after child capacity
  and work are confirmed. Work counts every sample and split, polls cancellation
  at most every 32 counted units, and never recurses.
- A charged ring and a current loop share the near-axis binomial series in
  `delta = 2rR/(R^2+z^2+a^2+r^2)` for `delta <= 1/8`, with an explicit positive
  geometric tail added to the absolute uncertainty, and direct symmetric-angle
  integration above that. The series has no division by `r`; below `1e-22`
  radial cancellation the series, not the direct integral, is authoritative.
  The loop reconstructs its world vector from the same radial/axial basis; its
  `2 pi I R/A^(3/2)` normalization is magnetic, never the ring's electric `Q`
  normalization.
  A charged disc uses the exact transverse chord integral off axis and the
  rationalized axial difference exactly on axis. A uniform solid sphere uses the
  centre-stable divergence-theorem surface integral, returns exact zero at the
  centre, and keeps the linear near-centre field; it is not a stack of planar
  rings. All non-axis paths reuse the bounded partition, resolution guard,
  absolute-uncertainty and work accounting above.
- A finite continuous solenoid integrates its loop measure analytically in the
  axial direction, then one bounded angular integral. Endpoint differences are
  rationalized when `u = z+L/2` and `v = z-L/2` share a sign; opposite signs use
  the stable direct sum. The angular kernel forms
  `R - r cos phi = (R-r) + 2 r sin^2(phi/2)`. Exactly on axis it uses the full
  analytic finite-solenoid expression and exact radial zero. Its resolution
  guard compares the cylinder arc width against the softened radial and axial
  distances to the whole support strip. Interior, on-winding and on-end queries
  at ordinary softening return complete finite results; an unresolvable narrow
  feature returns `Unavailable` with no vector.
- The infinite wire keeps its exact `2I(axis x s_perp)/(|s_perp|^2+a^2)`
  azimuthal reconstruction and factor 2. A query exactly on the wire axis is
  exact zero because the azimuthal basis is undefined there. Zero-strength
  sources are skipped and exact.

**Pinned by.** [tests/sources_model_test.rs](../tests/sources_model_test.rs):
`validation_is_strict`, `prepare_is_cancellable_before_allocation`,
`empty_and_zero_are_exact`, `axis_direction_is_not_squared_norm_limited`.
[tests/sources_numeric_test.rs](../tests/sources_numeric_test.rs):
`scaled_elementary_fields`, `huge_near_axis_wire_is_refused_not_confidently_wrong`,
`line_against_closed_form`, `narrow_displaced_line`,
`budget_and_cancellation_are_bounded`.
[tests/sources_electric_test.rs](../tests/sources_electric_test.rs):
`analytic_axial_fields_and_scaling`, `off_axis_fixtures_meet_requested_target`,
`tiny_budget_distinguishes_complete_from_unavailable`,
`ring_near_axis_and_branch_sweep`, `ring_monopole_far_field`,
`sphere_near_surface_small_softening_is_stable`,
`disc_exact_axis_symmetry_is_not_underflow`.
[tests/sources_magnetic_test.rs](../tests/sources_magnetic_test.rs):
`analytic_loop_and_solenoid_axial_fields`,
`off_axis_fixtures_meet_requested_target`, `loop_near_axis_and_branch_sweep`,
`loop_magnetic_dipole_far_field`, `finite_current_scales_inverse_length`,
`tiny_budget_distinguishes_complete_from_unavailable`.
[tests/sources_evaluate_test.rs](../tests/sources_evaluate_test.rs):
`family_target_uses_cancelled_total`,
`both_family_orders_have_reserved_work`,
`all_eight_kinds_have_committed_references`,
`finite_inaccurate_is_complete_only`,
`convergence_tracks_requested_accuracy`,
`point_dipole_grid_matches_analytic_sums`,
`underflow_and_numeric_range_are_distinct`. Independent finite-line endpoint
references, the committed electric and magnetic fixture matrix and its offline
`generate.py`/provenance README live in
[tests/support/sources/](../tests/support/sources/).

**Limitations.** Uncertainty is an empirical absolute allowance accumulated from
quadrature indicators, series tails and roundoff terms, not a certified interval
bound. Cancellation-heavy families can honestly miss a tight target when the
per-source modeled rounding exceeds it. The `1e-6` fixture rows use far queries
so the resolution guard can resolve the softening scale within the default cell
budget; on-support narrow features are not claimed. Tight requests may return
complete finite `AccuracyNotMet` or `Unavailable` when a refinement cap is
reached. There is no UI, persistence, glyph, rendering, export or app
integration. Wasm/browser and manual visual checks remain waived, not passed.
See [sources accuracy notes](sources-accuracy.md) for tested cases and
timings.

#### `src/field_probe.rs` — bounded Gaussian surfaces and circulation loops

**Purpose.** A no-UI, no-GPU, native/wasm-compatible seam for closed-surface
flux and circulation estimates. The caller supplies the already selected field
through `ProbeField`; the report records the explicit `FieldFamily` so electric
and magnetic contributions are never added. All values are normalized teaching
units, not SI measurements.

**Public API.** `SurfaceKind`, `LoopKind`, `GaussianSurface`, `CirculationLoop`,
`ProbeField`, `IntegralStatus`, `FluxReport`, `CirculationReport`, `flux`,
`circulation`, `GaussianSurface::validate`, `GaussianSurface::area`,
`CirculationLoop::validate`, `MAX_SURFACES`, `MAX_LOOPS`, `MAX_PROBE_SAMPLES`,
`MIN_SEGMENTS`, and `MAX_SEGMENTS`. `size` holds full extents along the local
axes (sphere/circle diameter, box/rectangle width-height, cylinder diameter and
height); local `z` follows the explicit axis.

**Invariants.**
- Validation rejects non-finite centre/axis/size, non-positive extents, a
  degenerate axis and `segments` outside `8..=64`. The axis is normalized on
  use; a degenerate axis is never replaced by `+Z` in this model.
- The full-resolution patch budget is `segments²` for a sphere,
  `6 * segments²` for a box, `3 * segments²` for a cylinder (side plus two caps),
  `segments` for a circle and `4 * segments` for a rectangle. Any geometry above
  `MAX_PROBE_SAMPLES` is rejected before integration, with the limit named.
- Quadrature is midpoint patches for surfaces, weighted by the outward area
  normal, and midpoint segments for loops, weighted by the oriented tangent.
  Patch areas use exact geometry weights; work is deterministic and bounded.
  Surface normals are built in the surface's local basis: a cylinder side uses
  `u*cos(phi) + v*sin(phi)` with caps at `±w`, a box uses `±u/±v/±w`, a sphere
  is radial, and loop tangents use the local `u`/`v` basis. No normal assumes
  the axis is `+Z`.
- A second pass at half resolution (halved segments, floor four) must complete
  before `Estimate { value, error }` is reported; `error` is the absolute spread
  between the two passes, not a certified bound.
- A missing or unsupported sample makes the result `Unresolved`; any non-finite
  sample makes it `Singular`. The reported `samples` count is the full-resolution
  patch/segment count (within `MAX_PROBE_SAMPLES`); failed patches/segments are
  counted from the pass that failed and are never silently treated as zero
  contributions. A failed full pass skips the half-resolution pass.
- Electric and magnetic families remain separate inputs and separate reports.
  Supported loops are closed, so `one_sided` is currently always false.

**Pinned by.** [tests/field_probe_test.rs](../tests/field_probe_test.rs):
`sphere_flux_of_a_point_charge_follows_the_enclosed_charge_relation`,
`uniform_field_has_zero_flux_through_a_closed_surface`,
`tilted_cylinder_in_a_uniform_field_has_zero_flux`,
`tilted_cylinder_normals_follow_the_local_basis`,
`loop_around_a_straight_wire_recovers_the_ampere_relation`,
`missing_and_non_finite_samples_are_reported_not_zeroed`,
`validation_rejects_non_finite_and_degenerate_geometry`,
`validation_rejects_geometry_over_the_sample_budget`,
`refinement_does_not_increase_the_reported_error`, and
`reports_are_deterministic_for_repeated_calls`.

**Limitations.** Midpoint quadrature is a teaching estimate for the sampled
field, not a Maxwell solver or a certified enclosure. The error is a
resolution-spread indicator. Induction probe reports are **not**
accuracy-gated: `InductionScene::unit_vector_potential` returns the fine value
and discards its kernel uncertainty, so an Induction report's error is only the
half/full segment spread of the flux or circulation integration. A
roundoff-limited kernel can therefore be presented as an `Estimate` even though
the underlying potential carries a larger uncertainty. Wasm/browser and manual
visual checks remain waived, not passed.

**Slice B (document state, reports, panel and wireframes).** `State` owns the
bounded surface and loop lists; `add_probe_surface`/`update_probe_surface`/
`remove_probe_surface` (and the loop equivalents) validate first, treat an
identical edit as a no-op and dirty exactly once on an accepted real change,
while a rejected or out-of-range edit leaves both the list and the dirty flag
untouched. `set_probe_family` selects one explicit `FieldFamily`; E and B are
never combined. `ViewRecord.probe_surfaces`/`probe_loops` store serde-friendly
records (`ProbeSurfaceRecord`, `ProbeLoopRecord`) with `#[serde(default)]`, so
they join `EditSnapshot` undo/redo and scene documents; restore drops unknown
kinds, invalid geometry and excess entries, logs once and never dirties.
`ViewRecord.probe_family` stores the stable token `electric` or `magnetic`
with `#[serde(default)]`; restore assigns it after the mode is known, falls
back to `electric` (with one warning only when a stored non-empty token is
corrupt or unavailable in that mode) and never dirties beyond the enclosing
restore. Sources supply both families; Induction supplies only `electric`;
Materials and Maxwell supply the family of their active case; Formula and Ode
supply neither.

`State::probe_field` builds one read-only adapter per check: the Sources
`Evaluator` from the active source set (only target-met samples count),
Induction's induced electric field only, Materials by case family, and Maxwell
through `maxwell_field_with_bem` with any matching BEM solution. Formula, Ode
and the Plot view are refused with the mode named. `refresh_probe_reports(true)`
caches per probe on mode, case/formula, parameters, geometry, family, domain and
the 100 ms-quantised clock (`REPORT_REFRESH_SECONDS`); a collapsed section
performs no sampling, and unsupported or refused checks store status text rather
than a number. While the section is open in Induction mode a full refresh can
reach roughly `MAX_PROBE_SAMPLES` surfaces plus the half-resolution pass, with
each field sample paying the fixed Gauss-Legendre-32 azimuthal integration (and
its coarse companion): millions of kernel evaluations, so the 0.1 s
quantisation may degrade to near-per-frame refresh. That worst case is bounded
by the model's caps and only occurs while the user opts into the open section;
the cache is not redesigned for it. `probe_overlay_lines` emits only
`LineInstanceGPU` wireframes
(sphere latitude/longitude rings, box edges, cylinder rims, circle, rectangle)
using the existing electric/magnetic constants, alpha `0.6` and the Sources
layer/family style via the shared sanitisers; `app::compose_vector_lines`
appends them, so every capture path includes them with no `SceneGPU` change.
The collapsed "Field probes" sidebar section shows the family selector, add rows
per kind, editable geometry, the readout or status with sample count, and
Remove; it is disabled with the model's reason when the mode cannot supply a
field. Drafts are session-only and never persisted. A one-shot
`ProbeControls::open_section` request opens the body for a Maxwell integral
action or the guided tour's flux stop, and the `Add sphere` response rectangle
is recorded for that stop's live anchor; a user close is respected because the
tour reveals the body once per stop.

**Pinned by (slice B).** `tests/probe_state_test.rs`:
`adding_and_updating_probes_dirty_once_and_rejected_edits_do_not`,
`flux_through_a_sphere_uses_the_active_family_and_mode`,
`unsupported_modes_report_a_reason_instead_of_a_number`,
`probe_reports_are_cached_until_geometry_or_mode_changes`,
`probe_wireframes_are_included_in_composed_capture_lines`, and
`sidebar_probe_rows_add_remove_and_show_honest_status`; persistence and scene
documents in `probe_surfaces_and_loops_round_trip_through_the_envelope_and_scene_document`
and `restoring_invalid_probes_drops_them_without_dirtying`; the family record in
`legacy_envelope_without_probe_family_restores_electric`,
`magnetic_probe_family_round_trips_and_keeps_magnetic_reports_working` and
`corrupt_or_unavailable_probe_family_sanitises_to_electric_without_dirtying`.

#### `src/flow.rs` — the tracer's velocity-field abstraction

**Purpose.** Give the streamline tracer one `Flow::eval(point, t)` seam over
either the world field or a projected slice.

**Public API.** `Flow`, `PlaneFlow { field, frame }`, `BakedFlow { baked,
plane }`, `SceneFlow::{World, Plane, Baked}`; `FieldExpr` implements `Flow`.

**Invariants.**
- `FieldExpr`'s flow delegates to `eval3_at` unchanged.
- `PlaneFlow` lifts local `(u, v, 0)` points through `PlaneFrame::to_world`,
  evaluates the world field, then projects the velocity with
  `PlaneFrame::project`, so a trace never leaves the slice.
- `BakedFlow` samples a world-coordinate `BakedField`: with a plane frame it
  lifts, samples and projects exactly like `PlaneFlow`; without one it samples
  the world point. Its ignored simulated time is safe because a bake exists
  only for a non-time-dependent field. Both views report an exact zero as
  stationary, matching `SceneFlow::Baked`.
- `SceneFlow` selects world flow outside a plane slice and plane flow inside
  one. When the opt-in is on and a matching bake is present, it selects
  `SceneFlow::Baked`; its exact-zero rule is stationary, like a plane slice.
  The `BakedFlow` plane is always the CURRENT `plane_frame()`, never the frame
  stored in the bake spec, so a re-clamped slice index cannot sample the wrong
  plane.

**Pinned by.** `tests/scene_flow_test.rs` and `tests/streamlines_test.rs`
(`trace_at_accepts_projected_plane_flow`).

**Limitations.** Projection drops the out-of-plane velocity component, so a
purely normal field has zero slice flow.

#### `src/scalar.rs` — static scalar right-hand-side expressions

**Purpose / API.** `ScalarExpr::parse(source, ScalarContext::{Curve, Surface})`
validates scalar syntax and bindings with existing `meval`; `eval(x, y)` returns
an optional finite f64 height. `ScalarError` implements `Display` and `Error`,
preserving the source, underlying error and allowed variables.

**Invariants.** Curves allow `x`; surfaces allow `x, y`. Built-in constants and
functions remain available. Binding validation does not require a finite value
at an arbitrary point: `sqrt(x)` and even `sqrt(-1)` are valid syntax. Assignment
syntax, unknown symbols/functions and static-context `z`/`t` are rejected.
GPU-coordinate representability belongs to the sampler, not this evaluator.

**Pinned by.** [tests/scalar_test.rs](../tests/scalar_test.rs):
`scalar_rhs_context_and_errors`, `partly_undefined_is_valid_syntax`.

#### `src/scalar_view.rs` — bounded scalar grids, contours and probes

**Purpose / API (issue #92 slice A).** `ScalarSample`, `ScalarGrid`,
`ContourSegment`, `PointProbe`, `LineProbe` and `ProbeSet` model scalar views
on one `PlaneFrame`. `sample_grid`/`sample_line` evaluate through
`ScalarExpr::eval_at` with `ScalarContext::Surface`; committed parameters are
substituted into the formula text first. `contour_segments` performs explicit
marching squares. `ScalarViewCache` is a one-entry cache keyed on formula text,
parameters, frame, domain, grid side, explicit levels and the clock quantised to
`SCALAR_RESAMPLE_INTERVAL` (100 ms).

**Invariants.** Grid sides stay in `MIN_GRID_SIDE=8..=MAX_GRID_SIDE=160` and
line samples in `2..=MAX_LINE_SAMPLES=512`, validated before allocation. A cell
is painted only with four finite corners; missing samples stay `None` and are
never interpolated. Contour levels are finite, strictly increasing and at most
`MAX_CONTOUR_LEVELS=16`; cells with a missing corner emit no segment. `z`
formulas return `UNSUPPORTED_Z_REASON` ("scalar views support x/y formulas").
A hidden cache samples nothing; a changed key causes exactly one rebuild.
Probes keep world positions and optional `SeedAnchor`s as document state while
sampled values/points remain runtime-only and unpersisted.

**Pinned by.** [tests/scalar_view_test.rs](../tests/scalar_view_test.rs):
`constant_field_grid_is_uniform_and_finite` through
`z_formula_reports_the_scalar_unsupported_reason`; probe persistence in
`tests/persistence_test.rs` and `tests/scene_document_test.rs`.

**Slice B1 UI/render seam.** `State::scalar_grid_side`/`scalar_contour_levels`
(default 64/8) commit only through `set_scalar_grid_side` (16..=160) and
`set_scalar_contour_levels` (0..=16). `set_scalar_grid_side` validates and stores
without touching the dirty flag; `set_scalar_contour_levels` is the
geometry-affecting commit, so a real change dirties the scene once and bumps
`plot.generation`, while an equal count is a free no-op. `scalar_view_request`
generates explicit
evenly spaced levels from a bounded 32² preview on the quantised clock, so an
animated formula keeps the 100 ms throttle; the preview levels are memoised
under the scalar identity, so a visible but idle section does not re-sample
them. `add_point_probe`/`remove_point_probe` are the only probe edits (cap
`MAX_PROBES=8`, one dirty document edit each); the sidebar owns the collapsed
"Scalar view" section after Coloring. `refresh_scalar_view` samples only while
that body is open, re-evaluates point-probe values, and marks the scene dirty
only on a real rebuild. `scalar_overlay_lines` converts cached contour segments
to plane-world `LineInstanceGPU`s (reference edge width, theme text colour) and
each point probe to an on-slice cross; it composes only while the record still
matches the current formula/parameters/plane/domain/clock/grid identity, so a
collapsed section cannot paint stale contours or crosses. `snapshot_with_styles`
appends them after the flow lines, so PNG/SVG/animation captures include them
without new capture code. `app/input.rs` maps an armed "Place probe" click
through the active `PlaneFrame`, rejects picks outside the scene domain or past
the cap, and disarms with a message when the probe path disappears.

**B1 tests.** `contours_are_composed_as_scene_lines_for_a_linear_field`,
`contours_do_not_cross_invalid_samples`,
`contour_and_probe_lines_survive_scene_assembly_for_capture` and
`collapsed_scalar_view_does_not_composite_stale_geometry` in
`tests/scalar_view_test.rs`; the section/reason/bounds/cache/message tests in
`tests/sidebar_controls_test.rs`; the history, row and mode-switch tests in
`tests/edit_history_app_test.rs`.

**Slice B2 heatmap.** `HeatmapGPU::from_grid`/`heatmap_legend` build a bounded
`MAX_HEATMAP_SIDE=160` RGBA8 sRGB fill from cell-centred texels (contours and
probes sit on sample vertices, so the half-cell offset is intentional), always
colour by `ColorQuantity::Magnitude` even when arrow mapping uses
`Direction2d`, and fingerprint the grid+mapping for upload skipping. The upload
uses unpadded `queue.write_texture` rows because only buffer-copy layouts need
`COPY_BYTES_PER_ROW_ALIGNMENT`; the pipeline targets the surface's blendable
format with the sRGB texture, and a future non-blendable target must disable the
pass with a reason rather than degrade it. The heatmap and legend gate on the
cached grid (`record_matches_grid`), so a constant or zero-time field whose
contour levels collapse to an empty list still paints. Pinned by
[tests/scalar_heatmap_test.rs](../tests/scalar_heatmap_test.rs):
`heatmap_quad_covers_the_sampled_plane_extent`,
`heatmap_colours_follow_the_active_palette_and_manual_range`,
`heatmaps_colour_by_magnitude_even_when_arrows_use_direction`,
`animation_frames_keep_the_heatmap_across_the_clock`,
`clipped_values_use_the_endpoint_colour_and_report_clipping` and the upload
skip/ordering/pass tests.

#### `src/scalar_analysis.rs` — comparison series and potential gradients

**Purpose / API (issue #93 slice A).** `ComparisonSeries`, `ComparisonSet`,
`SeriesRole`, `SeriesStyle`, `GradientSpec`, `GradientSample`, `GradientCacheKey`
and `GradientCache` model display-only comparison curves and one finite-difference
gradient request. `ComparisonSet` caps at `MAX_COMPARISON_SERIES=4`, rejects
duplicate ids, and `sanitized()` repairs stored data (drops empty expressions and
duplicates, truncates labels to `MAX_SERIES_LABEL_BYTES=48` on a character
boundary, applies palette fallbacks, enforces the cap). Only
`SeriesRole::ElectricPotential` is labelled `E = -∇V`; a generic source stays
`∇f` and is never called an electric field.

**Differentiation.** `sample_gradient` validates the spec first (step finite and
in `MIN_STEP=1e-6..=MAX_STEP=1e-1`, never clamped; `max_error` finite and
non-negative), reports the #92 unsupported/parse reason for a `z` or otherwise
invalid source, then samples the frame at the #92 side `160² =
MAX_GRADIENT_SAMPLES=25_600` in row-major order. Central differences use
`h = step`; exact domain edges use the inward one-sided stencil and set
`one_sided`. Any non-finite stencil evaluation yields `gradient: None` with
`error_estimate = 0.0` and `usable = false`; holes are never interpolated.
`error_estimate` is the largest component disagreement between `h` and `h / 2`
(an agreement indicator, not a true-error bound) and a finite sample above
`max_error` is marked unusable. The sampled vector is the field the overlay must
draw in frame-world coordinates: `∂f/∂u · right + ∂f/∂v · up` for
`SeriesRole::Scalar`, negated to `E = -∇V` for
`SeriesRole::ElectricPotential`. The negation is applied after differentiation,
so `error_estimate`, `usable` and `one_sided` are sign-independent, and the role
is part of the cache identity because the vectors differ.

**Cache / persistence.** `GradientCache` is a one-entry cache keyed on role,
source text, step, budget, frame, domain, the committed `ParameterSet` and the
clock quantised to `SCALAR_RESAMPLE_INTERVAL` (100 ms); `record_matches`/`store`
invalidate on any edit. `ViewRecord.comparison_series` and
`ViewRecord.gradient` are serde-default document state: capture writes them,
startup restore assigns them directly (series sanitized, an invalid gradient
dropped, one warning) without per-field dirtying, and older records load as no
series and no gradient. `State::set_comparison_series`/`set_gradient` validate
and dirty once on a real change; they never touch arrows, colour mapping or
probes.

**Pinned by.** `tests/scalar_analysis_test.rs` (the fourteen contract tests),
series/gradient envelope and corrupt-record pins in `tests/persistence_test.rs`,
and the scene JSON round-trip/legacy pins in `tests/scene_document_test.rs`.

**Slice B UI/render seam.** The Scalar view's collapsed "Scalar analysis"
subsection owns session-only `Controls` drafts (`ScalarAnalysisControls`,
`SeriesDraft`, `GradientDraft`) that are committed once per frame through
`State::set_comparison_series`/`set_gradient`; a rejection keeps the draft and
shows the model's message, and the drafts are cleared by `Controls::new` (never
persisted). `sidebar::apply` commits those drafts only while
`State::scalar_view_applicable()`: a scalar plot kind (`Curve2D`/`Surface3D`) or
another inapplicable view leaves `State::gradient`, `State::comparison_series`
and the dirty flag untouched, so an inert feature cannot rewrite the persisted
gradient frame. The list is capped at four and `Add series` uses the next palette
fallback (`Series N`, formula `x`). `State::refresh_scalar_analysis(visible)`
samples only while that body is open and the scalar view is applicable, and only
when its identity changed (series, gradient request, committed parameters,
domain, plane frame, curve sample count, theme text colour, or the 100 ms
clock); hidden or unchanged frames sample nothing. Comparison curves reuse
`plot::sample_curve_at`/`curve_edges_at` in Curve context over the frame's local
domain, map through `PlaneFrame::to_world`, and honour each `SeriesStyle` colour
and a phase-continuous dash pattern. A series expression that fails
`ScalarExpr::parse` paints nothing but reports a reason naming the series through
`State::scalar_analysis_error` (the same slot as a gradient failure, which takes
precedence); the rejected text stays in the draft. Gradient arrows are three
`LineInstanceGPU` segments per usable sample (shaft + two head segments), fixed
length `0.8 ×` sample spacing, coloured with
`source_scene::ELECTRIC_COLOR` for `E = -∇V` and the scalar text colour for
`∇f`; unusable/hole samples are omitted and counted. Both collections are
appended by `State::scalar_overlay_lines`, after contours/probes, so
`State::snapshot`/PNG/SVG/animation captures carry them with no new capture
path; they never touch vector arrows, the colour mapping or probes. Counts are
exposed by `scalar_analysis_counts`, `scalar_analysis_error` and
`scalar_analysis_resamples`.

**Slice B tests.** `comparison_curves_render_in_their_own_styles`,
`unparseable_comparison_series_surfaces_an_error_and_paints_nothing`,
`unusable_gradient_samples_are_omitted_and_counted`,
`gradient_arrows_carry_the_electric_label_colour_only_for_that_role`,
`comparison_and_gradient_geometry_do_not_change_arrows_mapping_or_probes`,
`svg_export_keeps_curves_and_gradient_as_lines`,
`capture_and_animation_include_the_overlay` and
`hidden_section_does_not_sample_the_gradient` in `tests/scalar_analysis_test.rs`;
the panel list/add/remove/bounds/step and
`scalar_analysis_commits_only_while_the_scalar_view_is_applicable` tests in
`tests/sidebar_controls_test.rs`.

#### `src/plot.rs` — pure bounded scalar geometry and plot types

**Purpose / API.** `PlotKind::{VectorField, Curve2D, Surface3D}` is serializable
and defaults to `VectorField`; `PlotBounds` carries f64 min/max coordinates.
`sample_curve` delegates to `sample_curve_with_diagnostics`, returning optional
`(x, f(x), 0)` slots. `curve_edges` returns original adjacent segments and
`SampleDiagnostics` in `CurveEdges`.
`sample_surface(expr, domain, UVec2)` returns `SurfaceMesh` positions, normals,
u32 indices and diagnostics, compacted to vertices used by surviving triangles.

**Invariants.** Requests cap at 4097 slots before allocation; zero requests are
empty, one samples the midpoint, and larger requests include both X endpoints
using f64 interpolation. Reversed/non-finite X/Y domains produce absent slots
and `invalid_domain`; Z is irrelevant, and Y/Z never clip heights. Non-finite
values and f32-unrepresentable coordinates remain gaps with distinct counters.
Edges skip missing, zero-length or non-finite geometry. Midpoint probes recurse
only when f64 height spread exceeds eight times `max(domain Y span, 1)`;
root level 0 through level 6 permits at most 127 probes per original edge.
Unresolved edges are omitted, never replaced with subdivisions.

Surface axes cap at 96 before allocation (9,216 slots / 54,150 indices maximum).
Reversed/non-finite XYZ domains set `invalid_domain`; finite Z never clips
heights. Zero/one axes or zero XY extent yield empty geometry. A missing corner
or invalid/unrepresentable center rejects both triangles of a cell; at most one
center probe runs per complete candidate cell (9,025 maximum). CCW triangles
`(a,b,c), (b,d,c)` face +Z. Each cell requires strictly positive projected XY
area for both faces after GPU rounding, preventing cancellation of used vertex
normals. Cross products and area-weighted accumulation use f64 and surviving
faces only.
The graph is open: no walls, caps, fake origin vertices or hole filling.

**Pinned by.** [tests/plot_curve_test.rs](../tests/plot_curve_test.rs): slot counts,
inclusive/extreme endpoints, holes, representability, pole rejection, input/work
caps, exact probe depth and threshold equality, and f64 spread arithmetic.
[tests/plot_surface_test.rs](../tests/plot_surface_test.rs) pins exact counts,
winding/unit/sloped normals, corner/center holes, domain rejection, degeneracy,
allocation/probe caps and extreme finite normal arithmetic.

**Limitations.** The conservative display heuristic may omit steep continuous
edges and cannot prove continuity; surface center probes likewise cannot detect
every singularity. This foundation has no renderer or bounds computation; later
consumers must derive bounds from surviving geometry rather than isolated slots.

#### `src/plot_examples.rs` — static scalar example catalogue

**Purpose / API.** `PlotExample { kind, name, source }` and `EXAMPLES` supply the
eight #12 formulas: three curves and five surfaces. Names and right-hand-side
sources are stable catalogue data, separate from vector presets.

**Invariants / limitations.** All entries parse in their declared scalar context
and have finite default-domain evaluations. Charge examples use a dimensionless
0.1 regularizer; they are illustrations, not physical solvers. The catalogue
owns no selection cache, UI or persistence framework.

**Pinned by.** [tests/plot_examples_test.rs](../tests/plot_examples_test.rs):
`catalogue_is_exact_and_unique`, including kind serialization round trips.

#### `src/plot_state.rs` — transient scalar plot data

`ScalarDraft` holds source, error and optional accepted expression and sampled
cache. `SampledPlot` owns curve slots/edges or a surface mesh. `PlotSettings` and
`PlotState` hold plot controls, independent drafts, remembered vector projection
and generation counters. These types are transient and do not persist caches.

#### `src/plot_scene.rs` — immutable scalar scene construction

`build_plot_scene` converts supplied sampled geometry into `PlotScene`, without
evaluating expressions. `PlotGeometry` carries curve instances or mesh vertices
and indices; `ViewportScene` distinguishes vector and plot scenes. Bounds and
automatic signed-height ranges use only rendered segment/triangle participants;
independent grid and axes references follow the explicit scalar kind. Empty or
mismatched data has no plot bounds and a status. `tests/plot_state_test.rs`
covers scene colors, bounds, invalid data and reference visibility.

#### `src/plot_camera.rs` — plot-only framing and navigation

**Purpose / API.** `PlotCamera` keeps f64 `target`, `distance`, full-height
`ortho_extent`, orbit `yaw`/`pitch`, `fov_y` and retained `fitted_bounds` free of
the vector camera's distance/near/far limits. `fit` frames supplied `PlotBounds`
with the current orientation, `frame` recomputes a `PlotFrame { view_proj, eye,
aspect }`, and `apply_gesture`/`apply_command` reuse the shared pointer and
view-command seams. `PlotFitError` is displayable and every failure leaves the
camera untouched.

**Invariants.** Collapsed axes receive `max(largest_span * 1e-6, 1e-6)` padding
on each side in f64. Fit uses the tighter horizontal/vertical FOV for perspective
surfaces and aspect-aware extents for orthographic curves, leaving a 10% total
span margin. Near/far planes are derived from the padded bounds and eye, with a
positive near plane, then the f64 transform and eye are converted to f32 only
after finite, positive-w, framing-margin and WebGPU `[0,1]` depth checks.
Projection fidelity also rejects matrices that erase a finite extent through f32
rounding. The curve front view keeps the existing +Y-up, look-down -Z convention;
`ViewCommand::Fit` frames the domain while the separate `fit` seam frames sampled
plot extrema. No GPU device or vector-camera state is involved.

**Pinned by.** [tests/plot_camera_test.rs](../tests/plot_camera_test.rs):
tall/wide portrait-landscape framing, collapsed-axis padding, invalid-fit
transactionality, domain-versus-plot separation, ordinary gesture conventions,
extreme/close-eye clipping, and unchanged vector fit/F behavior.

#### `src/plot_renderer.rs` — depth-equipped plot renderer and composite

**Purpose / API.** `PlotDrawState` is installed in egui_wgpu callback resources
per device. It owns the scalar plot pipelines, per-renderer uniform/geometry
buffers and a single-sample color target in the eframe format plus a
`Depth32Float` attachment. `PlotTargetKey` identifies extent/format/composite
sample count within one renderer object; resources are never reused across
devices. `plot_target_extent` reproduces egui's rounded/clamped viewport pixels
before a proportional device-edge cap, `plot_draw_order` excludes
`PlotKind::VectorField` (no scalar fallback), `plot_pipeline_policy` feeds actual
pipeline creation, and `prepared_matches` gates the paint composite. The live
prepare records the plot pass into the supplied egui encoder and returns no
command buffers. `PlotPassResources` plus `record_plot_pass` are shared by live
preparation and capture. [`src/plot_renderer/capture.rs`](plot_renderer/capture.rs)
is the separate `PlotCapture` owner: independent pipelines, buffers, color/depth
targets and readback for native synchronous or wasm event-loop completion.
`src/shaders.plot.wgsl` is the lit indexed mesh shader (48-byte `PlotVertex`
stride, ambient + directional diffuse); `src/shaders.plot_composite.wgsl` samples
the full target without a vertical flip or manual gamma. The unchanged
`src/shaders.line.wgsl` is used only by the two new depth-enabled reference and
curve pipelines; vector pipelines and draw plans are untouched.

**Invariants.** Plot color is `RENDER_ATTACHMENT | TEXTURE_BINDING | COPY_SRC`;
depth is `Depth32Float` with `RENDER_ATTACHMENT`, cleared to 1.0. Mesh is
CCW/back-culled, depth `LessEqual` with writes; references test without writes;
the curve tests and writes so it overlays its same-plane reference. Composite has
no depth, replaces color and takes the incoming sample count. Unsupported format
usages, non-filterable color, unsupported composite samples, invalid capture
dimensions, stride/readback overflow and non-PNG formats return errors before
allocation. The live path never submits independently; capture uses its own
submission/readback ordering and retains resources until completion.

**Pinned by.** [tests/plot_renderer_test.rs](../tests/plot_renderer_test.rs):
extent equality with `PaintCallbackInfo::viewport_in_pixels`, proportional caps,
zero area, the empty vector draw plan, all four pipeline policies and sample
counts, prepared generation/extent identity, and format/capture preflight
rejection. These CPU tests create no wgpu device. The required native device gate
is [tests/plot_gpu_test.rs](../tests/plot_gpu_test.rs): ordinary runs cover CPU
contracts and print `GPU not requested`; `scripts/test-scalar-gpu.sh` sets
`VFE_SCALAR_GPU_PROBE=1` only on the child and drives the actual
`PlotCallback` prepare/paint flow into a no-depth incoming target. It asserts
near/far depth order, reference occlusion, invalid-cell holes, curve-overlay and
singularity-gap behavior, backface culling, sRGB/linear targets, fractional-DPI
viewport/scissor, `129×97 → 257×113` resize, stale-generation and released-target
non-draws, interleaved capture/live independence, PNG encode/decode, and return
to the unchanged vector renderer on the same device. Missing device or required
format configuration is an error, never a noop/fake fallback.

#### `src/cas.rs` — native symbolic transform adapter

**Purpose.** Differentiate scalar meval formulas and compose gradient,
divergence, and curl results without changing ordinary field evaluation.

**Public API.** Native-only `Variable::{X, Y, Z, T}`;
`differentiate`, `gradient`, `divergence`, and `curl`; displayable `CasError`.

**Invariants.** Meval defines input meaning. The adapter validates its complete
RPN against a bounded grammar, lowers the parsed tree with explicit grouping,
and maps exmex variables by name. Engine output is parsed as a separate bounded
language, rewritten as fully parenthesized meval syntax, and context-bound.
Only dimensions 2 and 3 are accepted; 2D curl returns one scalar component and
3D curl returns x/y/z components. The exmex dependency and module are native
only; `meval` remains the field evaluator.

**Pinned by.** `tests/cas_test.rs`: approved unary-function derivatives,
meval-semantics and precedence cases, rejected syntax, coordinate/time
derivatives, vector identities, arity checks, deterministic output, and meval
context binding.

**Limitations.** Only the explicitly approved grammar is supported; this is not
a general-purpose CAS, and it makes no global domain-preservation guarantee.

#### `src/cas_tex.rs` — bounded linear-to-TeX display printer

**Purpose.** Issue #171: turn a CAS linear display string into TeX for the
optional typeset preview without changing the canonical string, grammar, or
evaluation.

**Public API.** Native-only `linear_to_tex(&str) -> Option<String>` and
`MAX_INPUT_BYTES` behavior.

**Invariants.** Total and non-panicking. Accepts only numbers, `x/y/z/t`,
`pi`/`e`, `+ - * / ^`, parentheses, and the one-argument functions `sin cos tan
asin acos atan sinh cosh tanh sqrt exp ln log abs`. Whitespace is ignored.
`*` becomes `\cdot`; a top-level parenthesised numerator over a parenthesised
denominator becomes `\frac`; every other `/` stays linear. `=`, `|`, brackets,
braces, commas, unknown identifiers, bare function names, empty input and
inputs over 512 bytes return `None`, which makes the panel keep the linear
result. Copy always uses the linear formula.

**Pinned by.** `tests/cas_tex_test.rs`.

#### `src/cas_backend.rs` — CAS backend bridge, detection and process bounds

**Purpose.** One `CasBackend` seam over the optional external CAS binaries
(`qalc`, `sage`, `sympy`) and the built-in reduced evaluator in `src/cas.rs`.
It owns capability reporting, `PATH`-only detection, the argv/stdin subprocess
protocol, fixed timeouts, bounded output, cancellation, and the honest fallback.

**Public API.** `CasBackend` (`capabilities`, `evaluate`), `CasRequest`
(`operation`, bounded `expression`, `variable`, optional `point`),
`CasBackendOp`, `CasCapability`, `CasCapabilities`, `CasBackendKind`
(`qalc|sage|sympy|builtin`, serde lowercase) with `ALL`, `detect_order()` and
`label()`, `CasBackendError`, `BuiltinBackend::new(reason)`,
`select_backend(kind, path_env)`, `detect_candidates(path_env)`, and the
native-only `CasRunner` (`start`/`poll`/`cancel`) plus its `CancelToken`.
Constants: `MAX_EXPRESSION_BYTES = 512`, `MAX_VARIABLE_BYTES = 32`,
`MAX_RESULT_BYTES = 4096`, `MAX_STDOUT_BYTES = 64 KiB`,
`MAX_STDERR_BYTES = 1 KiB`, `MAX_LINES = 512`,
`REQUEST_TIMEOUT_SECONDS = 5`, `PROBE_TIMEOUT_SECONDS = 2`.

**Invariants.** Detection resolves `qalc → sage → sympy` through the supplied
`PATH` only (no shell, globbing, empty-entry/current-directory fallback, or
auto-install) and probes with a 2 s timeout and 1 KiB caps. A missing binary is
omitted. A qalc/sage binary whose version probe fails is still listed with the
failure reason, but it advertises **no** capabilities and is never selected;
sympy is omitted unless a working `python3` with the `sympy` module probes
successfully. Selection probes too: it returns the requested backend only when
both the executable and its version probe succeed, otherwise `BuiltinBackend`
with a reason naming the missing binary or the failed probe.
Native requests validate first (expression ≤512 bytes and one control-free
line, identifier-shaped variable, finite point, dimension/order bounds), then
spawn an explicit argv with the expression on stdin: `qalc -t -c 0` receives
the raw expression; `sage`/`sympy` run a fixed `-c` program that reads a
`CAS1`-headed stdin payload. The fixed programs honour the payload's `point`
(Evaluate substitutes the point and simplifies) and `bounds` (Integrate uses a
definite integral with those limits). The child runs in a fresh temporary
working directory with a cleared environment (`PATH`, `LANG=C`, and the parent
`HOME` when set), 5 s wall-clock limit with kill, 64 KiB stdout / 1 KiB stderr
/ 512-line hard caps with kill, bounded non-zero exit reporting, and
single-line result validation; the temporary directory is removed on every
path. Reader threads hand their bounded buffer back over a channel; the worker
waits only until the request deadline or a fixed post-exit grace, so a
surviving grandchild holding a pipe open yields an honest error instead of a
hang. An unexpected read error is reported, never treated as clean EOF and a
successful answer. `CasRunner` enforces one live request (`Busy`), joins
finished work, and cancellation kills the child through the shared cancel
token; `cancel` and `Drop` wait at most a fixed bound and detach a worker that
outlives it. The built-in fallback supports differentiation (first order),
gradient, divergence and curl by reusing `src/cas.rs`; `DifferentiateN` and
every other capability is `Unsupported`, matching its advertised list. Process
code and the runner are gated to non-wasm targets; because `src/cas.rs` is
itself native-only, wasm compiles the types, detection (empty) and a
capability-free fallback that returns an honest `Unavailable` reason rather
than faking a backend.

**Pinned by.** `tests/cas_backend_test.rs`: empty-PATH honesty, documented
detection order, trimmed single-line success, timeout kill, oversized-output
kill, bounded non-zero stderr and its `Display` exit code, empty/control-
character malformed output, cancel of an in-flight request, bounded
evaluate/cancel/`Drop` when a surviving grandchild holds stdout open, fallback
with a reason, pre-spawn request validation, probe-failed candidates with no
capabilities that are not selected, sympy payload `point`/`bounds` fields,
built-in capability, `DifferentiateN` error consistency and
derivative/gradient/divergence behavior, and one in-flight `Busy` enforcement.
Test-only seams pin unexpected-read-error handling and the fixed sympy/sage
programs' point/bounds handling, since those internals are not otherwise
reachable from integration tests. Fixtures are shell scripts written to per-test
temp directories; no real qalc/sage/sympy, network or model output is used.

**Limitations.** The external protocol is exercised only by fixtures in this
slice; actual binary behavior is not a test dependency, and selection is
session state (no persistence until #170). No UI wiring, grammar change,
typesetting, app actions, plot preview or dependency addition lives here.

#### `src/cas_intent.rs` — tolerant CAS input grammar and intent

**Purpose.** Bound, normalise and classify a CAS input line before any engine
call, so the panel can echo the normalised text and show the inferred operation
while the dropdown remains an override.

**Public API.** `classify(&str) -> Result<CasParse, CasIntentError>`,
`normalize(&str) -> String`, `CasIntent` (derivative, gradient, divergence,
curl, integral, solve, simplify, evaluate, plain) with `operation_label()`,
`CasParse { intent, normalized }`, and `CasIntentError` (`Empty`, `TooLong`,
`TooDeep`, `MissingExpression`, `BadVariable`, `BadArity`,
`UnbalancedDelimiters`, `Unknown`). `MAX_INPUT_BYTES = 512`,
`MAX_NESTING = 32`, `MAX_ASSIGNMENTS = 8`.

**Invariants.** Input is limited to 512 UTF-8 bytes and 32 bracket levels;
classification is total and never panics on user text. Normalisation trims and
collapses whitespace; rewrites `\frac`, `\sqrt`, `\cdot`, `\times`, `\partial`,
`\nabla`, `\left`, `\right`, single-token superscripts (bare `^{n}` only when
the exponent is one `[A-Za-z0-9_]+` token), `·`, `×`, `∂`, `−` and `∗`; and is
idempotent. The `·`→`*` rewrite is why `∇* F` classifies as divergence, exactly
like `∇· F`. `\frac`/`\sqrt` group rewriting is skipped beyond 32 brace levels
so the public `normalize` cannot recurse without bound. Exactly one
`[A-Za-z][A-Za-z0-9_]*` variable is accepted where a variable is part of the
form. Recognised operators with missing or malformed
arguments return an error rather than silently becoming plain text; only
unrecognised leading text is `Plain`. `at name=value` assignments cap at eight.
The module performs no evaluation, backend call, UI mutation or application
state access.

**Pinned by.** `tests/cas_intent_test.rs`: every intent and alias, unicode and
LaTeX-lite rewrites, normalisation idempotence, definite/indefinite integrals,
the assignment cap, every rejection path, and multibyte/adversarial input
without panic.

**Limitations.** The grammar is intentionally shallow: expressions are carried
as text and not validated as meval or CAS expressions, LaTeX support is the
listed subset, and nested calls are left to the backend after the delimiter
checks.

#### `src/cas_actions.rs` — typed CAS app-control actions

**Purpose.** Parse a fixed, bounded verb vocabulary on top of the #168 CAS
line into typed app-control actions, and preview exactly what each action will
do before it applies.

**Public API.** `CasAction` (`SetFormula`, `SetScalarSource`, `SetParameters`,
`SetDomain`, `SetDensity`, `SetDimension`, `SetColors`,
`ApplyAppearanceProfile`, `AddPointProbe`, `ApplySeedLayout`, `SetCamera`,
`SetTransport`), `CameraFraming` (`Fit`, `Reset`, `Isometric`), `CasActionError`
(`Empty`, `TooLong`, `UnknownVerb`, `BadValue`, `UnsupportedHere`),
`parse_action`, `preview`, `MAX_PARAMETER_ASSIGNMENTS = 8`,
`MIN_HALF_EXTENT = 0.1`, `MAX_HALF_EXTENT = 100`.

**Invariants.** Verbs are `formula`, `scalar`, `param`, `domain`, `density`,
`2d`/`3d`, `colors`/`colours`, `appearance`, `probe`, `seeds`, `camera`,
`play`/`pause`, matched case-insensitively while values keep their case. Input
is limited to the shared 512-byte CAS bound and at most eight parameter
assignments. Values are validated before an action exists: the formula through
`FieldExpr`, the surface source through `ScalarExpr`, the domain through finite
half-extents in `0.1..=100`, density through the model's sampled bounds, colour
names through the built-in enums, and camera framings through the fixed list.
An unrecognised leading word returns `UnknownVerb` so the panel can fall back
to the mathematical intent grammar. Parsing and preview are pure: no app state,
no mutation and no sampling. Actions that need live state (parameter names,
library records, the probe cap or the current dimension's density bound) are
validated again by the app before the first mutation. `SetFormula` and
`SetScalarSource` edit the Formula-mode field only: `apply_cas_action` refuses
both outside `FieldMode::Formula` with the same `CAS edits the formula field;
switch to Formula.` explanation as the #168 mathematical path, because the
scalar overlay (`scalar_view_applicable`) and the Surface plot are Formula-mode
views. The panel stays enabled so the preview remains visible; the refusal
surfaces on Apply and leaves the document, dirty flag and history untouched.

**Pinned by.** `tests/cas_actions_test.rs`: every verb and case variant, preview
text, every rejection path, the assignment cap, and app-level application with
atomic rejection, one dirty transition/undo entry, named library resolution,
the probe cap, camera framing, runtime-only transport and neutral no-ops.

**Limitations.** The grammar is textual and intentionally shallow; parameters
may only be updated, never defined, and no action creates a source, runs a
solver, writes a file or touches preset storage.

#### `src/domain.rs` — sampling box and grid iterator

**Purpose.** Axis-aligned sampling domain and the exact lattice used everywhere
(arrows, streamlines, plane selection, reference geometry).

**Public API.** `Domain { min, max }`, `Domain::new`, `domain_size`, `center`,
`grid_points(UVec3)`, and the `GridPoints` iterator.

**Invariants.**
- `min`/`max` are inclusive; `center` is the midpoint.
- `n == 1` puts that axis at its midpoint; `n > 1` includes both endpoints.
- A zero dimension yields an empty iterator.
- Iteration is x-fastest, then y, then z.
- `Domain::default()` is `[-1,1]³`.

**Pinned by.** `tests/domain_test.rs`: `default_is_unit_cube`,
`new_ranges_and_size`, `cube_grid_count_and_endpoints_included`,
`line_grid_samples_axis_endpoints_only`, `zero_dim_grid_is_empty`.

**Limitations.** No `contains` method; modules that need containment write the
same inclusive test (`particles.rs`, `streamlines.rs`).

#### `src/sampling.rs` — grid sampling to arrows

**Purpose.** Evaluate the field over the domain grid and produce coloured arrow
data.

**Public API.** `ViewOptions { colormap, color_range, color_mapping }`, its
`Default` (`color_mapping: None`); `ArrowData { position, dir, mag, color }`;
`magnitude_range`; `sample_field`, `sample_field_at`, `sample_plane_at`.

**Invariants.**
- Degenerate samples (non-finite field, magnitude < `1e-6`) are skipped.
- `dir` is always unit length; `mag` is the original magnitude.
- With an active `ViewOptions::color_mapping` (issue #83), `ColorStats` comes
  from the arrows actually kept and each arrow is coloured by
  `ColorMapping::sample`; the explicit/auto `color_range` is then unused.
- Without a mapping, the colour range is the explicit
  `ViewOptions::color_range`, otherwise the min/max of the samples actually
  kept; an empty input gives `(0.0, 0.0)`. This legacy path is byte-identical.
- Plane sampling projects the world field onto the frame first; both the
  direction and the component quantities use that projected local vector,
  which is what the slice actually draws.
- `magnitude_range` on an empty slice gives `(0.0, 0.0)`.

**Pinned by.** `tests/sampling_test.rs`: `constant_field_fills_grid`,
`all_zero_field_yields_no_arrows`, `sample_skips_nan_arrows`,
`color_follows_provided_range`, `magnitude_range_of_constant_field`,
`legacy_mapping_is_byte_identical_to_no_mapping`,
`legacy_manual_range_parity_includes_out_of_range_values`,
`component_quantity_uses_a_zero_centred_range`,
`plane_direction_uses_the_projected_local_vector`,
`manual_range_is_honoured_and_auto_uses_the_sample_set`,
`non_finite_or_collapsed_ranges_fall_back_to_mid_grey`.

**Limitations.** One full resample per dirty rebuild when the
`field_cache` key changes; a time-invariant field reuses the previous pass
when the key is unchanged. `sample_field` always uses the 3D evaluator, so a
2D grid is a single z slice.

#### `src/presets.rs` — built-in field presets

**Purpose.** The grouped formula catalogue shown in the Presets panel.

**Public API.** `PresetCategory { Classic, Electrostatics, Magnetostatics,
Electrodynamics }` with `ALL`, `title`, `description`; `Preset { name, formula,
dimension, category }`; `presets()`; `presets_in(category)`;
`preset_approximation_label(name)`.

**Invariants.**
- Every formula parses and declares the right dimension.
- Every preset stays finite on the default domain (singular-looking fields are
  softened with `+0.05`).
- Names are unique; categories are ordered by `PresetCategory::ALL` and
  declaration order within a category.
- `presets_in` is the single grouping seam used by the panel and tests.
- `preset_approximation_label` returns `Some("2D approximation")` for exactly
  `Dipole`, `Source and sink`, `Point charge`, `Like charges`, `Quadrupole` and
  `Straight wire`; it is a display label only and never converts a formula.
- Ticket 91's teaching entries add pulsating-radial, rotating, traveling-wave,
  standing-wave, quadrupole-saddle, saddle-potential, anisotropic-well,
  double-well, softened-dipole and moving-Gaussian formulas. Slice B adds the
  3D entries `Uniform field`, `Quadrupole field`, `Softened point charge`,
  `Helical solenoid field`, `ABC flow`, `3D shear flow`, `Rotating uniform
  field` and `3D plane wave`. The six entries that reference `t` (`Pulsating
  radial field`, `Traveling wave field`, `Standing wave field`, `Moving Gaussian
  packet`, `Rotating uniform field`, `3D plane wave`) are driven by the shared
  `SimClock` through the ordinary `State::sample` path; no second timer exists.

**Pinned by.** `tests/presets_test.rs`: `every_preset_parses_with_declared_dimension`,
`every_preset_stays_finite_on_the_default_domain`, `preset_names_are_unique`,
`grouping_preserves_declared_category_order`, `softening_covers_the_origin`,
`new_teaching_presets_have_representative_values`,
`new_animated_presets_move_with_the_shared_clock`,
`only_the_clock_driven_teaching_presets_reference_t`,
`loading_a_new_animated_preset_leaves_unrelated_state_untouched`,
`slice_b_3d_presets_have_the_declared_category_and_dimension`, and the
category/limit tests.

**Limitations.** Formulas are illustrative, not physical (Coulomb numerators are
not normalised). Legacy static entries remain spatial snapshots; the ticket-91
animated entries read `t` from the shared clock but make no SI or propagation
claim.

#### `src/preset_library.rs` — indexed preset catalogue

**Purpose.** A pure, searchable index over the existing formula presets
([`presets()`](../src/presets.rs)), the slice-C pure scalar rows, and source
presets ([`source_presets()`](../src/source_presets.rs)). It owns no physics and
no UI; later panel work consumes the stable IDs and metadata.

**Public API.** `PresetId` with `new(&str)`/`as_str`; `PresetIdError`;
`PresetGroup { Classic, Physics, Sources, Gallery }`;
`PresetValidity { MathematicalIllustration, PhysicallyValid,
PhysicallyValidRegularized }` with `label`; `PresetFilters { dimension, animated }`;
`ScalarPresetSpec { expr, kind, show_heatmap, contour_levels }`;
`PresetEntry { id, name, summary, group, filters, assumptions, parameters,
exploration_prompt, validity, scalar }`; `PresetLibrary::builtin`, `from_entries`,
`entries`, `len`, `is_empty`, `search`, `filtered`, `search_filtered`, `get`;
`PresetLibraryError`.

**Invariants.**
- IDs match `[a-z0-9][a-z0-9_.-]*`, ASCII, 1..=64 bytes; malformed candidates
  return a descriptive error and never panic.
- Formula IDs are `formula.<slug>` (lowercased name with each non-`[a-z0-9]`
  run replaced by `_`) unless ticket 91 metadata fixes an explicit ID;
  `Quadrupole (unequal charges)` uses
  `formula.quadrupole-unequal-charges` because the legacy `Quadrupole` already
  owns `formula.quadrupole`. The five scalar rows reuse that same
  `formula.<slug>` prefix, for example `formula.scalar_saddle` and
  `formula.softened_dipole_potential`. Source IDs are `source.<slug>`. IDs are
  unique across the whole catalogue and construction rejects duplicates.
- Catalogue order is `presets()` order, then the slice-C `SCALAR_PRESETS` rows,
  then `source_presets()` order. Formula groups map `Classic` to `Classic` and
  the three physics categories to `Physics`; scalar rows are `Physics`; source
  entries are `Sources` except the named last gallery, which is `Gallery`.
- Formula entries carry `Preset::dimension`; the six ticket-91 clock-driven
  entries set `animated`, the rest stay static. Source entries are
  dimension-agnostic (`None`) and static. The five scalar rows are surface
  scalars (`Some(2)`); the moving Gaussian and interfering waves set `animated`.
- Only the five `SCALAR_PRESETS` rows carry `scalar: Some(ScalarPresetSpec)`;
  every existing vector and source row keeps `scalar: None`, so their load path
  is unchanged. The spec always names the shipped `PlotKind::Surface3D` scalar
  workbench and the existing heatmap/contour controls; no new plot kind exists.
  Rows are: Scalar saddle, Scalar anisotropic well, Softened dipole potential,
  Scalar moving Gaussian packet, and Interfering waves. The moving Gaussian row
  is named with a `Scalar` prefix because the vector row already owns
  `Moving Gaussian packet`.
- `assumptions` reuses `preset_approximation_label` for legacy formulas and
  carries the ticket-91 teaching note otherwise. The new formula, scalar and
  source entries also carry a non-empty summary, exploration prompt and a
  `PresetValidity` label (`Mathematical illustration`, `Physically valid`, or
  `Physically valid (regularized)` for the softened dipole, softened point
  charge and softened dipole potential); legacy entries without labels carry
  `validity: None`. The Gauss/flux, Ampere, shielding and image-charge prompts
  attach to Uniform field, Infinite wire, Sphere inside and out, and Electric
  dipole respectively. `parameters` stays empty until a preset defines them.
- `search` is trimmed, case-insensitive substring over name, summary and ID,
  with a blank query returning all entries in catalogue order. `filtered` keeps
  sources under every dimension and only animated entries when `animated` is
  true. `search_filtered` is the manual intersection of both.

**Pinned by.** [tests/preset_library_test.rs](../tests/preset_library_test.rs):
`ids_are_unique_and_stable`, `accepts_valid_ids_and_rejects_malformed`,
`formula_entries_keep_catalogue_order_and_groups`,
`source_entries_use_the_source_group_and_gallery_last`,
`search_is_trimmed_case_insensitive_and_ordered`,
`dimension_filter_keeps_sources_and_matching_dimension`,
`animated_filter_returns_exactly_the_clock_driven_entries`,
`teaching_formula_entries_carry_stable_metadata`,
`slice_b_3d_formula_entries_carry_stable_metadata`,
`teaching_source_entries_carry_stable_metadata`,
`scalar_presets_are_indexed_with_surface_expressions`,
`every_scalar_preset_expression_parses_in_surface_context`,
`interfering_waves_is_animated_and_evaluates_finitely`,
`softened_dipole_potential_is_a_pair_potential_in_the_plane`,
`scalar_saddle_gradient_is_the_shipped_saddle_field`,
`exploration_prompts_are_attached_to_the_gauss_ampere_shielding_and_image_charge_rows`,
`builtin_scalar_entries_leave_the_vector_and_source_catalogue_unchanged`,
`combined_search_and_filter_matches_manual_intersection`,
`get_returns_the_entry_or_none`, and `duplicate_ids_are_rejected`; the scalar
load path is pinned by `tests/ui_preset_test.rs`:
`loading_a_scalar_preset_switches_to_its_kind_without_touching_the_vector_formula`,
`a_scalar_reload_that_only_changes_the_contour_count_dirties_once`
and `clicking_a_scalar_library_row_loads_the_scalar_view`.

**Limitations.** Legacy formula presets have no summary in their source
catalogue, so their `summary` is empty and their `validity` label is `None`.
No preset is parameterised yet. The animated flag is catalogue metadata; the
panel already runs the shared clock from `State`, but no library API drives it.
Scalar rows carry a static surface expression; they reuse the shipped
`Surface3D` workbench and cannot name a separate heatmap or contour plot kind.

#### `src/colormap.rs` — scalar-to-RGB ramps

**Purpose.** Map a magnitude to a colour for arrows, streamlines, and
particles.

**Public API.** `Colormap { Viridis, Turbo, Mono }`, `ALL`, `label`,
`colormap(magnitude, min, max) -> [u8; 3]`, `from_unit(t) -> [u8; 3]`.

**Invariants.**
- `max <= min` returns mid-grey `[128,128,128]`.
- Values clamp to the ramp ends; `Mono` is linear black→white.
- `colormap` normalizes and routes through `from_unit`, so a normalized
  position and the equivalent magnitude are byte-identical.
- `Viridis`/`Turbo` come from `colorous`; `ALL` covers every variant with a
  unique label.

**Pinned by.** `tests/colormap_test.rs`: `degenerate_range_is_mid_gray`,
`out_of_range_clamps`, `viridis_endpoints`, `mono_endpoints`,
`offset_range_normalizes`, `from_unit_matches_colormap_for_every_builtin`,
`from_unit_endpoints_and_clamping_are_exact`,
`all_covers_every_variant_with_unique_labels`.

**Limitations.** No custom/user ramps and no alpha; alpha is supplied by the
instance colour.

#### `src/color_ramp.rs` — validated custom ramps and value scaling

**Purpose.** Issue #83 slice A: a pure, panic-free model for user-editable
colour ramps and value scaling. It is not yet wired into the sidebar, legends,
sampling or persistence.

**Public API.** `Rgb([u8; 3])` with `from_hex`/`to_hex` and serde;
`ColorStop { position, color }`; `RampKind { Sequential, Diverging, Cyclic }`
with `ALL` and `label`; `ColorRamp { stops, reversed, kind }` with `MIN_STOPS`,
`MAX_STOPS`, `validate`, `sanitized`, `sample` and `builtin`;
`ScaleMode { Linear, SignedLog { half_width } }` with `validate` and
`normalize`; `ColorStyle { ramp, mode }` with `validate`, `sample` and
`legend_ticks`.

**Invariants.**
- Ramps hold 2..=16 finite, strictly increasing positions covering `0.0` and
  `1.0`; diverging ramps add a `0.5` stop and distinct endpoints, cyclic ramps
  match endpoints. `sanitized` replaces an invalid ramp with the default
  black-to-white sequential ramp instead of repairing it.
- Sampling clamps, treats NaN as `0.0`, reverses as `1.0 - t`, returns stop
  colours exactly and interpolates pending channels in `f64` with rounding.
- Signed log accepts only finite ranges with `min < 0.0 < max` and a finite
  `half_width` in `(0.0, 1.0e6]`; `ColorStyle::sample` falls back to mid grey
  `[128,128,128]` when normalization is impossible.
- Legend ticks are bounded at nine, sorted, deduplicated and include both
  endpoints when the range is valid.
- `builtin(Colormap)` seeds nine stops at `0.0, 0.125, ..., 1.0` for
  Viridis/Turbo; those are editable approximations matching exactly only at
  the endpoints. Mono is a linear byte ramp and is seeded with exactly two
  stops (`#000000` at `0.0`, `#ffffff` at `1.0`); that seed is structurally
  exact, but interior samples may differ from `Colormap::Mono` by one LSB at
  f32 half-step boundaries because the legacy reference rounds
  `(255.0 * t + 0.5)` in f32 while `ColorRamp::sample` interpolates and
  rounds in f64.

**Pinned by.** `tests/color_ramp_test.rs`: hex round-trip/malformed input,
stop count/order/endpoint/kind rules, exact and interpolated sampling,
reversal and clamping, `Mono`/Viridis/Turbo builtin identity, linear and
signed-log normalization, continuity/monotonicity, legend ticks, `sanitized`
and the extreme-input no-panic loop.

**Limitations.** No UI, legend widget, HEX/RGB text entry, import/export or
`State`/`AppSettings` fields in this slice; the existing `Colormap` remains the
live renderer path.

#### `src/color_mapping.rs` — pure colour mapping and legend descriptor

**Purpose.** Issue #83 slice B1: the pure model tying a palette, quantity and
scale mode to one effective range for sampling, legend ticks and clipping
counts. Slice B2a wires it into grid/plane sampling and streamlines; it still
does not touch particles, trails, the renderer, export or any UI widget.

**Public API.** `ColorQuantity { Magnitude, ComponentX, ComponentY, ComponentZ,
Direction2d }` with `ALL`, `label` and `available(is_3d)`;
`ColorPalette { Builtin(Colormap), Custom(ColorRamp) }` with `validate`, `kind`
and `sample_unit`; `ColorSample`, `ColorStats` (`from_samples`), `ColorMapping`
(`validate`, `is_legacy_equivalent`, `effective_range`, `sample`, `legend`,
`clip_counts`, `explanation`); `ClipSide`, `MappedColor`, `ColorLegend`,
`ClipCounts`.

**Invariants.**
- `is_legacy_equivalent(colormap)` is true only for that builtin palette in
  `Linear` mode colouring `Magnitude` with no manual range, letting callers
  keep the old `Colormap` path byte-identical.
- Direction colouring is rejected in 3D with a fixed message; every other
  quantity is available in both views.
- Magnitude auto-fits to the sample magnitudes; components auto-fit
  symmetrically around zero; direction uses `atan2(y, x) / TAU` wrapped to
  `[0, 1)` and ignores the range. A manual range always wins.
- A builtin palette in linear mode calls `Colormap::colormap` directly, so
  bytes stay identical including out-of-range values and Mono. Other paths
  normalize then sample the palette.
- A signed log whose range does not strictly span zero is disabled, not
  coerced: sampling uses linear normalization on the same range and
  `explanation` says why. Usable signed log maps zero to the ramp centre.
- Non-finite values/bounds or `min >= max` yield mid grey `[128,128,128]`;
  clipped samples report `Low`/`High`, direction never clips and its counts are
  zero.
- Legend ticks come from the effective range and mode. Direction is cyclic
  `0..360` with `[0, 90, 180, 270, 360]`; linear and usable signed log ticks
  match `ColorStyle::legend_ticks` exactly; invalid ranges yield empty ticks
  with finite bounds.

**Pinned by.** `tests/color_mapping_test.rs`: stats and empty stats, magnitude
auto range, builtin byte parity across Viridis/Turbo/Mono, custom override,
symmetric component ranges, direction angle/availability, signed-log fallback
and centre, clipping sides/counts, legend range/mode and `ColorStyle` tick
parity, non-finite mid grey, State mapping/setter/freeze behaviour, and
`is_legacy_equivalent_only_accepts_the_default_builtin_path`.

**Limitations.** No particle/trail colouring, renderer, legend widget or export
wiring in this slice; streamlines only use the mapping when the state passes
one, and the legacy streamlines path stays byte-identical.

#### `src/layer_style.rs` — bounded layer and family appearance overrides

**Purpose.** Issue #84 slice A1: the pure appearance model for per-layer and
per-family opacity, width and fixed-colour overrides. It stores and validates
the record and resolves effective values; it does not touch the UI, renderer,
scene documents or profiles, so a neutral record renders exactly today's scene.

**Public API.** `MIN_WIDTH_MULTIPLIER = 0.05`, `MAX_WIDTH_MULTIPLIER = 4.0`;
`LayerId { Arrows, Flow, Sources, Induction, TeachingScene, Reference }` with
`ALL` and `label`; `PhysicsFamily { Electric, Magnetic }` with `ALL` and
`label`; `LayerOverride { opacity, width, color }` with `validate(field)`,
`sanitized` and `is_neutral`; `LayerStyles { layers, families }` with
`validate`, `sanitized`, `is_neutral` and `resolve(layer, family)`;
`ResolvedLayerStyle { opacity, width, color }`.

**Invariants.**
- `layers` is in `LayerId::ALL` order and `families` in `PhysicsFamily::ALL`
  order; both records are serde-defaulted arrays.
- Validation rejects non-finite values and opacity outside `0.0..=1.0` or
  width outside `0.05..=4.0`, naming the first path such as
  `layers[2].opacity`; bounds are inclusive.
- `sanitized` turns a non-finite value into `None` and clamps a finite
  out-of-range value into its bound; colours need no repair.
- Resolution is per attribute: defaults `1.0`, `1.0`, no colour, then the
  optional family override, then the layer override. `None` ignores the family
  record.
- `Default` is neutral, so an absent or empty record leaves drawing unchanged.

**Pinned by.** `tests/layer_style_test.rs`:
`neutral_styles_resolve_to_drawable_defaults`,
`layer_override_sets_only_the_fields_it_names`,
`family_override_applies_to_family_aware_layers`,
`layer_beats_family_per_attribute`,
`validation_rejects_non_finite_and_out_of_range_values`,
`sanitized_repairs_a_stored_record`,
`layer_styles_round_trip_through_settings`,
`older_settings_load_neutral_styles`,
`commit_is_a_no_op_for_an_equal_set`,
`invalid_commit_keeps_the_previous_styles`,
`commit_marks_dirty_once_for_a_real_change`,
`restored_layer_styles_reach_the_state_without_dirtying`.

**Limitations.** A2a wires Arrows, Flow and Reference only; no Appearance
panel, Sources/Induction/Teaching wiring, or scene/profile bundling yet. `State`
holds the record, the settings envelope round-trips it, and startup assigns it
directly without dirtying the scene.

#### `src/appearance_profile.rs` — pure appearance profiles

**Purpose.** Issue #85 slice A1: a named record bundling the #82 viewport
style, the #84 layer/family overrides and the #83 colour mapping. It validates,
sanitizes and applies atomically to `State`, and round-trips through a strict
versioned JSON document. It does not touch the Appearance panel, settings or
on-disk profile storage, scene documents, the renderer or sampling.

**Public API.** `PROFILE_FORMAT_VERSION` (currently `1`),
`MAX_PROFILE_NAME_BYTES = 48`, `DEFAULT_PROFILE_NAME = "Dark laboratory"`;
`ProfileColor { colormap, ramp, mode, quantity }`;
`AppearanceProfile { name, viewport, layer_styles, color }` with `capture`,
`validate`, `sanitized` and `apply(&self, state)`;
`ProfileDocument { version, profiles }` with `to_json` and `from_json`;
`builtins()`.

**Contract.**
- Names are trimmed, non-empty, at most 48 UTF-8 bytes and free of control
  characters; duplicates are rejected case-sensitively within a document.
- Validation checks the name, `ViewportStyle::validate`,
  `LayerStyles::validate`, an optional ramp (`ColorRamp::validate`) and
  `ScaleMode::validate`. A ramp is allowed alongside any builtin colormap; when
  present it wins and only the informational `colormap` field is retained.
- `sanitized` is total: names are trimmed, cleaned and truncated on a character
  boundary (empty becomes the default), viewport/layer fields use their own
  sanitizers, an invalid ramp becomes `ColorRamp::default`, and an invalid
  signed-log mode becomes linear. The output validates.
- `capture(name, state)` reads exactly the fields `apply` writes
  (`viewport_style`, `layer_styles`, `colormap`, `color_ramp`, `color_mode`,
  `color_quantity`); the name is stored verbatim and the result validates for a
  valid name. Applying a captured profile twice equals applying it once.
- `apply` validates everything before the first mutation, then writes only
  `viewport_style`, `layer_styles`, `colormap`, `color_ramp`, `color_mode` and
  `color_quantity`. A failure restores those fields and leaves the dirty flag
  untouched; a real change requests one rebuild. Formula, parameters, sources,
  camera, layout and UI preferences are never touched.
- Documents use `deny_unknown_fields`; `from_json` probes the version first, so
  a future version is reported before the profile body is parsed. Semantic
  errors use `invalid appearance profiles: `; serde errors use
  `invalid appearance profile JSON: `. Imports are atomic and reject duplicate
  names. `to_json` emits compact JSON with pinned field order.
- `builtins` ships 24 looks in fixed display order: the original nine (Dark
  laboratory, White paper, High contrast, Tokyo Night, Catppuccin Mocha,
  Catppuccin Latte, Nord, Gruvbox Dark, Solarized Light), then six seaborn
  categorical palettes (Deep, Muted, Pastel, Bright, Dark, Colorblind), four
  seaborn sequential approximations (Rocket, Mako, Crest, Flare) and five
  popular editor schemes (Dracula, One Dark, Monokai, Solarized Dark, Gruvbox
  Light). Theme text colours are sRGB hex converted to the linear RGB stored by
  `ViewportStyle`; ramp stops stay sRGB bytes. A multi-stop ramp helper spaces
  2–16 stops evenly from `0.0` to `1.0` with kind `Sequential`. Seaborn looks
  and popular themes are documented teaching approximations, not byte-exact
  reproductions.

**Pinned by.** `tests/appearance_profile_test.rs`: builtin validity, unique
names and pinned order, representative categorical/sequential/popular ramp
hexes and positions, the default profile, JSON round trip and pinned field
order, version-first and unknown-field rejection, atomic duplicate-name import,
appearance-only apply, capture round trip of all six appearance fields and
double-apply idempotence, rollback on invalid input, total sanitization,
sRGB-to-linear theme conversion (Tokyo Night background and axis), and the ramp
overriding the colormap field.

**Limitations.** No panel UI, on-disk profile library wiring or startup
application, no scene-document integration and no renderer/sampling changes in
this slice. Capturing reads the committed records only.

#### `src/palette_io.rs` — portable palette document

**Purpose.** Issue #83 slice C1: a strict, versioned JSON form for a custom
palette so users can copy a ramp between scenes and machines. Pure model only;
nothing is wired into the UI, persistence or renderer.

**Public API.** `PALETTE_FORMAT_VERSION` (currently `1`);
`PaletteStop { position, color: String }`; `PaletteDocument { version, kind,
reversed, stops }` with `from_ramp`, `to_ramp`, `to_json` and `from_json`;
`export_ramp` and `import_ramp`.

**Contract.**
- `from_ramp` copies kind, reversal and every position, writing canonical
  lowercase `#rrggbb` via `Rgb::to_hex` and the current version.
- `to_ramp` rejects any other version first (naming the found and expected
  versions), parses colours with `Rgb::from_hex` (uppercase and an optional
  `#` are accepted; a failure names the stop index and offending text), then
  returns the exact `ColorRamp::validate` error. Stops are never sanitised,
  clamped or dropped.
- Documents and stops reject unknown fields (`deny_unknown_fields`);
  `from_json` maps every serde failure to `invalid palette JSON: {error}`.
- `to_json` is compact serde output with stable field order: `version`,
  `kind`, `reversed`, `stops`.
- `export_ramp`/`import_ramp` compose those steps and never panic.

**Pinned by.** `tests/palette_io_test.rs`: sequential/diverging/cyclic round
trips, lowercase hex and current version, uppercase/bare hex imports, unknown
document and stop fields, future-version rejection naming both versions, bad
hex naming the stop index, validation failures (too few stops, duplicate
positions), malformed JSON, builtin Viridis round trip, and pinned field order.

**Limitations.** No file picker, clipboard UI, persistence or `State` wiring in
this slice.

#### `src/plane.rs` — plane-by-plane selection

**Purpose.** Select one sampled slice of the 3D cube and dim the rest.

**Public API.** `PlaneAxis { X, Y, Z }` with `ALL`, `label`, `count`,
`middle_index`; `PlaneView { enabled, axis, index }` with `middle`, `clamped`,
`active`, `count`; `PlaneFrame` with `new`, `local_domain`, `to_world`,
`to_local`, `project`; constants `DIM_FACTOR`, `DIM_GREY`, `DIM_ALPHA`;
`plane_coordinate`, `on_plane`, `dim_color`.

**Invariants.**
- `index` is a grid index, not a world coordinate.
- A one-slice axis collapses to index 0 and puts every point on the plane.
- `on_plane` recovers the index by rounding the coordinate ratio, so tiny
  floating-point differences do not move a sample off the plane.
- `plane_coordinate` matches `Domain::grid_points` exactly.
- `dim_color` lerps 75 % toward 128 grey; `DIM_ALPHA` is 0.2.

**Pinned by.** `tests/plane_test.rs`: `on_plane_matches_by_grid_index_not_float_equality`,
`plane_coordinate_matches_the_sampling_grid`,
`a_one_slice_axis_puts_every_point_on_the_plane`,
`dim_color_lerps_three_quarters_toward_neutral_grey`, and the axis/clamp tests.

**Limitations.** Plane dimming applies to arrows only. In the 2D slice view,
arrows, streamlines, particles and references are all local to the selected
plane; the out-of-plane flow component is omitted, so a purely normal field has
no slice flow (particles stay put rather than respawning).

#### `src/profile_library.rs` — user appearance-profile library

**Purpose.** Issue #85 slice A2a: validated user appearance profiles kept
beside the nine shipped looks, with a documented cap and an atomic
import/export text format. Pure model only; no UI, no `State` application and
no file I/O.

**Public API.** `MAX_USER_PROFILES = 32`; `ProfileLibrary` (`Default`, `new`,
`user`, `len`, `is_empty`, `upsert`, `remove`, `all`, `import_text`,
`export_text`).

**Contract.**
- `new` validates every profile, rejects duplicate names case-sensitively and
  rejects more than 32 user profiles. Validation errors name the profile and
  use the `invalid profile` prefix; a duplicate uses
  `duplicate profile name ...`.
- `upsert` validates the candidate, then replaces a same-named entry in place
  (order preserved) or appends. Replacing never grows the library; appending at
  the cap is rejected with `at most 32 user profiles`. A rejected upsert
  changes nothing.
- `remove` deletes by exact name and reports whether one was removed.
- `all` returns `builtins()` order, then the user profiles in library order.
- `import_text` parses with `ProfileDocument` (strict, versioned), then merges
  through a candidate clone; any error — malformed JSON, duplicate or invalid
  profile, or an over-cap result — leaves the library unchanged. It returns the
  user-profile count after the merge.
- `export_text` emits the user profiles as a compact `ProfileDocument` with the
  current version. Every stored profile is already validated, so serialization
  cannot fail; the fallback logs a warning rather than panicking.

**Pinned by.** `tests/profile_library_test.rs`:
`library_rejects_duplicate_and_invalid_profiles`,
`library_upsert_replaces_in_place_and_preserves_order`,
`library_enforces_the_user_profile_cap`,
`library_import_is_atomic_on_invalid_or_duplicate_documents`,
`library_import_merges_new_names_and_reports_the_user_count`,
`library_export_round_trips_through_import`, and
`library_all_lists_builtins_before_user_profiles`.

**Limitations.** The library itself remains UI-free. The app stores it in
`VfApp::profiles` and persists it through `AppSettings`; the Appearance panel's
`Profiles` section owns capture, apply, delete and portable import/export.

#### `src/interaction.rs` — pure interaction profiles

**Purpose.** Issue #88 slice A1: a named interaction profile bundling the
one-chord-per-action binding table and the camera-feel multipliers. It is pure
model code: no UI, input routing, `State`, renderer, camera constants or
dependency changes. Slice A2a persists it through `AppSettings`.

**Public API.** `INTERACTION_FORMAT_VERSION` (currently `1`),
`MAX_PROFILE_NAME_BYTES = 48`, `MAX_USER_PROFILES = 16`, `MAX_BINDINGS = 19`,
`MIN_CAMERA_MULTIPLIER = 0.25`, `MAX_CAMERA_MULTIPLIER = 4.0`,
`DEFAULT_PROFILE_NAME = "Default"`; `ChordModifiers { command, shift, alt }`;
`KeyChord { key, modifiers }` with `label` and `is_browser_reserved`;
`InteractionAction` with `ALL` (22), `REMAPPABLE` (19), `label`, `is_reserved`
and `is_global`; `Bindings` with `default_bindings`, `chord`, `owner_of`,
`assign`, `replace`, `reset_action`, `reset_all` and `validate`; `BindError`
(`Reserved`, `BrowserReserved`, `Conflict(owner)`, `Unknown`); `CameraFeel` and
`InteractionProfile` with `validate` and `sanitized`; `ProfileDocument` with
`to_json`/`from_json`; `ProfileLibrary` (`new`, `user`, `len`, `is_empty`,
`upsert`, `remove`, `all`, `import_text`, `export_text`).

**Contract.**
- Defaults reproduce today's chords: controls `Mod+B`, presets `Mod+P`,
  appearance `Mod+Shift+A`, interaction `Mod+Shift+L`, help `F1`, reset `R`,
  fit `F`, zoom in `Plus` (wiring also accepts `=`, not a second chord), zoom
  out `Minus`, view arrows, play/pause `Space`, seed `S`, particles `P`,
  screenshot `Mod+Shift+S`, copy `Mod+Shift+C`, quit `Mod+Q`.
- Fly toggle and fly movement are deliberately absent from the chord model and
  stay on the ticket-70 physical-key path, so `MAX_BINDINGS` is 19. Reserved
  actions `Dismiss`, `Undo` and `Redo` are read-only. Only `Quit` is `global`
  (fires while text input has focus).
- `assign` refuses reserved actions/chords, browser chords, and a chord owned
  by another action (`Conflict(owner)`), leaving the table untouched.
  `replace` swaps the target chord with the owner's in one step; a free chord is
  assigned. `reset_action`/`reset_all` restore defaults.
- Browser-owned chords are `Mod+W`, `Mod+T`, `Mod+N`, `Mod+Shift+W`,
  `Mod+Shift+I`, `Mod+Shift+J`, `Mod+Shift+K`, `Mod+Shift+C` and `Mod+Q`. The
  shipped `Mod+Q`, `Mod+Shift+C` and `Mod+Shift+S` defaults are grandfathered:
  they stay bindable to Quit, Copy formula and Screenshot respectively, but are
  refused for any other action.
- `validate` proves all 19 actions are bound, no duplicate chord exists, and no
  reserved or browser-owned chord is used. `sanitized` is total: trim/truncate
  names to 48 bytes, clamp multipliers, reset non-finite values to `1.0`,
  restore defaults for missing or invalid bindings and resolve duplicates; its
  output always validates.
- Documents are `deny_unknown_fields`, probe the version before parsing
  profiles, reject duplicate names and emit compact pinned JSON. Bindings are a
  stable action-id-keyed map. Imports are atomic on a library clone.
- `ProfileLibrary` keeps the built-in `Default` apart from at most 16 user
  profiles, rejects duplicate names case-sensitively, refuses overwriting or
  removing the built-in, and exports only user profiles (an empty string when
  there are none).

**Pinned by.** `tests/interaction_test.rs`:
`defaults_match_the_documented_bindings`,
`assign_rejects_a_reserved_action_or_chord`,
`assign_reports_the_conflicting_owner_and_changes_nothing`,
`replace_swaps_chords_so_no_duplicate_exists`,
`browser_reserved_chords_are_refused_with_a_reason`,
`camera_feel_clamps_and_sanitizes`,
`sanitized_profiles_always_validate`,
`profile_document_rejects_newer_versions_and_unknown_fields`,
`library_rejects_duplicates_and_the_cap`, `library_import_is_atomic`,
`library_export_excludes_builtins_and_handles_empty`, and
`profile_and_document_json_round_trip_is_stable`.

**Limitations.** No panel UI or input routing in this slice. Slice A2a stores
the model in the settings envelope (`AppSettings::interaction` and
`AppSettings::interaction_profiles`); slice A2b owns the panel and slice A3 the
routing.

### 3.3 Viewmodel and persistence

#### `src/viewmodel.rs` — state, setters, snapshot, input decisions

**Purpose.** The one pure seam for visible state and the pure camera/click
decisions that do not need a window.

**Public API (major).**
- Density/grid/plane constants: `DEFAULT_DENSITY_2D = 24`,
  `DEFAULT_DENSITY_3D = 16`, `MAX_DENSITY_2D = 200`, `MAX_DENSITY_3D = 40`,
  `MATERIAL_MAX_DENSITY_3D = 40`, `MATERIAL_MAX_DENSITY_2D = 200`,
  `MIN_DENSITY = 4`; particle size/count and arrow/opacity/magnitude/domain
  bounds; `PLANE_MARGIN = 1.35`; `STREAMLINE_WIDTH = 0.012`,
  `STREAMLINE_MAX_LEN = 120`, `STREAMLINE_JITTER = 0.45`.
- Starting formulas: `FORMULA_2D = "(x, y)"`, `FORMULA_3D = "(x, y, z)"`.
- `ArrowLengthMode { Proportional, Uniform, Magnitude }` (`ALL`, `label`,
  `tooltip`, `from_normalize`).
- `StreamlineSeedMode { Auto, Click }` and
  `LeftClickAction { SeedStreamline, AddParticles }`.
- `State` with all visible fields and setters. Every setter returns `bool`
  (“did this actually change?”) and marks dirty only on a real change. Key
  setters: `set_formula`, `set_mode`, `set_field_mode`, `set_source_scene`,
  `set_density`, `set_domain`,
  `set_arrow_scale`, `set_arrow_opacity`, `set_plane_dim_opacity`,
  `set_arrow_length_mode`, `set_magnitude_scale`, `set_proportional_exponent`,
  `set_colormap`, `set_color_ramp`, `set_color_mode`, `set_color_quantity`,
  `set_auto_fit`, `set_color_range`, `freeze_color_range`, `set_arrows`,
  `set_show_grid`, `set_show_coordinate_system`,
  `set_arrow_style`, `set_arrow_style_2d`, `set_arrow_params`,
  `set_streamlines`, `set_seed_mode`, `set_left_click_action`, `set_particles`,
  `set_particle_size`, `set_particle_count`, `set_particle_appearance`,
  `set_plane`, `set_plane_enabled`, `set_plane_axis`, `set_plane_index`.
- Issue #83 colour fields: `color_ramp: Option<ColorRamp>` (`None` uses
  `colormap`), `color_mode: ScaleMode` and `color_quantity: ColorQuantity`;
  `color_mapping()` builds the effective `ColorMapping`. `set_colormap` always
  clears a custom ramp, and `freeze_color_range` commits the current effective
  range into `color_min`/`color_max` with auto-fit off.
- Issue #84 slice A1 appearance field: `layer_styles: LayerStyles` with
  `layer_styles()` and `commit_layer_styles` (validate, equal-set no-op, assign,
  exactly one dirty request; a rejected record leaves the previous styles and
  dirty untouched).
- Issue #115 object style: private `object_style: ObjectStyle` with
  `object_style()`, `set_object_style(style) -> bool` (equal value is a false
  no-op; a change assigns once and marks dirty once) and
  `restore_object_style(style)` (startup assignment that never dirties).
- Issue #87 seed field: `seed_layout: SeedLayout` with `seed_layout()`,
  `commit_seed_layout` (validate against the visible scene domain, equal-set
  no-op, assign, exactly one dirty request; a rejected layout leaves the
  previous seeds and dirty untouched), `reset_seed_layout` (clear, dirty only
  when non-empty), `seed_point_on_current_plane` (world position plus the active
  plane anchor), `apply_generated_seeds` (validate the whole application, reject
  per-application/layout over-cap requests in full, one dirty) and `place_seed`.
  `streamlines_snapshot_with_styles` samples a non-empty layout instead of the
  automatic grid or explicit count; an empty layout keeps the old grid path
  byte-identical, and `Click` mode still paints an explicit layout on top of its
  click bursts.
- Issue #84 slice A2a drawable seams: `effective_arrow_params()` scales only the
  arrow girth (`head_width`, `shaft_radius`) by the Arrows width and leaves the
  stored record untouched; `flow_layer_style()` resolves the Flow opacity/width
  pair shared by streamlines, dots and trails; `reference_viewport_style()`
  applies the Reference width and alpha to the committed `ViewportStyle` while
  background and RGB pass through. The `*_for(&LayerStyles)` variants serve the
  blueprint snapshot, and both width seams repair a directly written
  non-finite/out-of-range multiplier (non-finite → `1.0`, finite clamped to
  `0.05..=4.0`) instead of trusting the public field.
- Issue #84 slice A2a blueprint split: `snapshot_blueprint()` is
  `snapshot_with_styles(&LayerStyles::default())`; `snapshot()` passes the
  committed record and `snapshot_if_dirty` still drives the live/raster cache.
  SVG exports use the blueprint, so Arrows/Flow/Reference overrides never change
  vector bytes or geometry. `src/app/viewport.rs` mirrors this with
  `assemble_viewport_scene_with_styles` (SVG passes the neutral default) and
  `src/cli.rs::run_export` routes SVG through
  `prepare_scene_with_styles(..., &LayerStyles::default())` while PNG and
  animation keep `prepare_scene`'s committed styles.
- Source mode fields: `field_mode`, `sources`, `source_sampling` and
  `source_notice`; `active_dimension`, `refresh_source_sampling`,
  `source_sampling` and `reset_sources`.
- Induction mode fields: `induction`, `induction_prepared`,
  `induction_sampling` and `induction_error`; `set_induction_scene`,
  `refresh_induction` and `reset_induction`.
- Materials mode fields: `material` (durable `MaterialScene`),
  `material_sampling` (`Option<MaterialSampling>`) and `material_error`;
  `set_material_scene`, `refresh_material_sampling` and `reset_material`.
- Ode mode fields: `ode`, `ode_solution` (`Arc<OdeSolution>`), `ode_error`,
  `ode_epoch`, `ode_chart`, `ode_variables` and private `ode_dirty`;
  `set_ode_scene` (validate/solve atomically), `refresh_ode_if_dirty`,
  `reset_ode`, `set_ode_chart` and `set_ode_variables` (at least one variable
  stays on).
- Queries/builders: `is_3d`, `grid`, `spacing`, `plane_size`,
  `view_options`, `sample`, `color_range`, `particle_radius`, `particle_target`,
  `accepts_click_seed`, `plane_active`, `arrow_instances`,
  `streamlines_snapshot`, `click_streamlines_snapshot`, `snapshot`,
  `snapshot_blueprint`, `snapshot_if_dirty`.
- Pure helpers: `domain_between`, `clamp_domain_extent`, `ORBIT_SENSITIVITY`,
  `ViewGesture`, `apply_camera_input`, `DragButtons`, `DragChannel`,
  `drag_channel`, `TouchPan`, `two_finger_pan`, `ClickOutcome`, `click_outcome`,
  `CapturedClickOutcome`, `captured_click_outcome`, `DOUBLE_CLICK_WINDOW`,
  `LastClickSeed`, `ClickSeedUndo`, `undo_click_seed`.

**Invariants.**
- `active_dimension()` is 3 in Sources, Induction and Materials, the solved
  scene's dimension in Ode (the selected scene before the first lazy solve),
  and the formula's component count otherwise; `is_3d()` means `mode_3d &&
  active_dimension() == 3`. This gates the cube grid, density cap, plane
  frame/slice and streamline depth, so source/induction/materials mode uses the
  existing 2D plane path in 2D and the world cube in 3D. Ode never hardcodes 3.
- `set_field_mode` preserves the formula, source scene, induction scene and the
  current 2D/3D projection, clears the cached sampling/notice and marks dirty.
  Entering Induction never rewrites the stored density or the projection;
  sampling caps density at `MAX_INDUCTION_DENSITY_3D` in 3D and
  `MAX_INDUCTION_DENSITY_2D` on a 2D slice, so leaving restores the untouched
  formula density. A changed active dimension syncs the plane index.
  `set_source_scene` replaces the durable scene and clears the cache. Formula
  edits never invalidate source or induction state. Entering Sources or toggling
  its 3D view re-clamps a legal 200² 2D density to `MAX_DENSITY_3D`, and
  `grid`/`plane_grid` defensively cap source-mode cubic sampling, so the 40³ cap
  has no transition bypass. Global reset calls `reset_sources`,
  `reset_induction` and `reset_ode` before selecting Formula; each restores its
  default scene and clears its caches/error without touching the other adapter
  or the clock. `reset_ode` synchronously solves the default scene and bumps
  the epoch; merely leaving Ode preserves the last solution and epoch.
- Entering Ode ensures the vector plot kind and preserves the projection,
  formula/source/induction durable scenes and camera. It clears the shared loop
  period (in the app cache), clears source/induction caches/notices, marks the
  scene dirty and does not solve synchronously. A fresh or `CurrentField` scene
  queues a lazy solve; a preserved typed solution re-enters without a new epoch
  so playback time survives. `mark_dirty` only queues a `CurrentField` re-solve.
- In Ode, `is_plane_slice` is false and `plane_frame` is `None`; `particle_target`
  is `None`; `scene_domain` is the solved padded `view_domain` (durable domain
  before a solve); streamlines/click bursts are empty and clicks are refused.
  `set_time` mirrors only. `has_time_dependent_consumer` is true while a
  solution exists or a lazy solve is pending.
- `snapshot_if_dirty` refreshes the active adapter first: formula mode holds no
  source/induction cache, source mode stores a fresh `SourceSampling`, induction
  mode re-prepares only on a changed `InductionKey` and otherwise rescales the
  same prepared `Arc`. `snapshot` in Sources/Induction mode returns cached arrows
  only (empty if unsampled) and never samples the formula; Induction also emits
  no lines, trails, dots or combined stream. In Ode the pure snapshot is an
  empty vector scene: the app owns trajectory lines and per-frame markers.
- Induction is a clock consumer exactly while `induction.is_animated()`; its
  `set_time` only dirties an animated waveform, so pause/resume and the scalar
  plot pause hold the sampled field at the shared time.
- Materials is static: `has_time_dependent_consumer` is false, `set_time` is a
  no-op and `particle_target` is `None` (clicks and streamlines are refused).
  `set_material_scene` validates first; a rejection records `material_error` and
  retains the last valid scene and sampling, while a valid change clears the
  error/sampling and marks dirty. `refresh_material_sampling` samples the
  capped 40³ cube or the existing `PlaneFrame` slice (200²), stores warnings in
  `MaterialSampling::warning`, and never samples the formula. `snapshot` emits
  cached arrows plus `material_outline` lines only; outlines are not reference
  lines. `reset_material` restores the default scene and clears the cache.
- `set_formula` keeps the last good `FieldExpr` on a parse error and still marks
  dirty so the status bar updates; `set_mode`/density/domain/formula-dimension
  changes call `sync_plane` with the previous grid.
- Domain extents are swapped per axis by `domain_between`; `clamp_domain_extent`
  bounds them to `-100..=100` (NaN → 0 in the pure function; callers keep the
  previous value).
- `snapshot_if_dirty` clears `dirty` and returns `Some` exactly once per change.
- The independent reference toggles default true, dirty the scene when changed,
  and leave field arrows/flow/click segments untouched.
- `snapshot` is pure: no GPU resource; `trails`, `dots` and `combined` are empty
  because the app owns per-frame particles and sorting.
- Arrow length: Proportional is `arrow_scale * spacing` normalised by the max
  sampled magnitude (raised to `proportional_exponent`); Uniform is the same
  length for all; Magnitude is `mag * magnitude_scale` and is deliberately not
  clamped.
- Plane mode: on-plane arrows get `arrow_opacity`, off-plane arrows
  `arrow_opacity * plane_dim_opacity` and a dimmed RGB.
- Streamlines: Auto seeds a jittered grid (half density, clamped 4..=24) plus
  clicks; Click hides the grid but still accepts clicks; off hides both.
  Tracing is RK4 with step `0.5 * spacing`, budget 120, both directions.
  `view_options` passes the effective `ColorMapping` only when it is not
  legacy-equivalent; the mapping uses stats from the sampled arrows, while
  `None` keeps the old `Colormap` segment colours byte-identical. Click bursts
  reuse the view's sampled arrows for the same stats.
- Click bursts: 5 seeds, 8 % of the smallest positive extent, deterministic from
  the clicked bits; 2D pins z, 3D jitters depth.
- `particle_target()` is `Some` only when `particles && active_dimension() == 3`
  in Formula mode (never in Sources, Induction or Ode), independent of the view
  mode, so a
  two-component formula gets no cloud in 3D while a three-component field gets
  one in the 2D plane slice.
- `drag_channel`: middle always pans; secondary orbits in 3D unless Shift, pans
  in 2D; primary orbits in 3D and pans in 2D.
- `apply_camera_input`: 3D orbit/pan/zoom uses orbit distance and FOV; 2D pans
  world X/Y one-to-one at the visible height and multiplies `ortho_zoom`.
- `click_outcome`: double-click wins over click; a click only seeds if the
  gesture never became a drag. Captured clicks always seed the centre.
- `undo_click_seed`: only inside `DOUBLE_CLICK_WINDOW`, only removes an exactly
  matching last seed, and only truncates particles when the current count is at
  least the pre-click count.

**Pinned by.** `tests/viewmodel_test.rs` (76 tests) is the main contract:
`snapshot_rebuilds_only_when_dirty`, `garbage_formula_keeps_field_and_reports_error`,
`invalid_edit_reports_error_without_losing_the_field`,
`unchanged_setters_report_false_and_stay_clean`, `density_clamped_per_mode`,
`density_cap_follows_sampling_dimension`, `domain_extrema_swap_makes_valid_box`,
`every_mode_keeps_unit_direction_and_magnitude_colour`,
`exponent_one_is_exactly_the_old_linear_formula`,
`snapshot_leaves_the_combined_stream_empty_for_the_app_to_sort`,
`streamlines_use_the_fixed_point_budget`, `plane_mode_is_inert_in_two_d`,
`particle_target_is_none_while_off_or_in_2d`, and the plane/streamline/particle
groups. Camera input tests are in `tests/camera_input_test.rs`,
`tests/camera_pan_zoom_test.rs`, `tests/click_seed_zoom_test.rs`; click/undo
tests are in `tests/view_command_test.rs`.

**Limitations.** `State` is not persisted directly; persistence captures an
effective subset. Streamlines are retraced on any scene rebuild. The snapshot
does not include per-frame particles by design, so a caller that only reads
`snapshot()` sees an empty dot/trail/combined stream.

#### `src/viewmodel/plot.rs` — scalar state transitions and lazy scenes

**Contract.** `State` keeps independent curve and surface drafts, settings and
sample caches while preserving vector state. Plot setters reconcile transitions,
sanitize controls and invalidate relevant caches; `plot_scene` lazily samples
accepted expressions and delegates scene construction to `plot_scene`.
`tests/plot_state_test.rs` covers editing, transitions, settings and caching.

#### `src/parameters.rs` — shared named-parameter records

**Purpose.** Validated, durable named numbers (issue #89 slice A) that formula
parsing substitutes (slice B) and the sidebar Variables editor commits through
`State::commit_parameters` (slice C).

**Public API.** `MAX_PARAMETERS = 32`, `MAX_NAME_LEN = 32`,
`PARAMETER_VERSION = 1`, `RESERVED_NAMES`; `Parameter { name, value, min, max,
step }`; `ParameterSet { version, entries }` (`Default` is version 1 with no
entries; `validate`, `value`).

**Invariants.** At most 32 entries. Names are 1–32 ASCII bytes matching
`[A-Za-z_][A-Za-z0-9_]*`, case-sensitive and unique, and may not be `x`, `y`,
`z`, `t`, `pi`, `e`, or any `meval` 0.2.0 builtin. Values, bounds and step are
finite; `step > 0`; `min <= max` when both exist; `value` lies within the
existing bounds. Errors name the parameter and the failed rule.

**Pinned by.** `tests/parameters_test.rs` (reserved/duplicate/malformed names,
bounds, cap, atomic `State::commit_parameters`).

#### `src/custom_density.rs` — bounded custom density model and diagnostics

**Purpose.** Issue #96 slice A: the pure, bounded custom charge/steady-current
model and its diagnostics. No editing UI, persistence, scene document, renderer
or new field solver; later slices wire it in.

**Public API.** `MAX_DENSITY_SAMPLES = 110_592`, `MIN_SAMPLES = 8`,
`MAX_SAMPLES = 48`, `MAX_NAME_BYTES = 48`, `CONTINUITY_TOLERANCE = 0.05`,
`MAX_ANGULAR_FREQUENCY = 1.0e3`;
`SupportKind {Sphere, Box, Cylinder}`,
`DensitySupport {kind, center, axis, size}` with `validate`, `contains` and
`volume`; `DensityFamily {VolumeCharge, SurfaceCharge, SteadyCurrent}`;
`TimeLaw {Steady, Harmonic {omega, phase}}` (default `Steady`);
`DensitySpec {name, family, support, formula, samples, softening, time_law,
charge_formula}`
with `validate(&ParameterSet)`; `DiagnosticsStatus {Estimate {error}, Unresolved
{reason}, Violation {reason}}`; `ContinuityStatus {Estimate {value, error},
Unresolved {reason}, Violation {residual, tolerance}}`; `ChargeReport`;
`ContinuityReport`; `charge_report`, `continuity_report` and
`validate_time_law_combination`.

**Contract.**
- `size` holds full extents. A sphere uses `size.x` as its diameter, a box uses
  all three extents, and a cylinder uses `size.x` as its length along `axis` and
  `size.y` as its diameter. All supports are closed. `axis` must be
  non-degenerate only for a cylinder. Because `worst_point` is an f32 `Vec3`,
  validation also rejects a support whose centre plus any half-extent exceeds
  the finite f32 range.
- Names are trimmed, non-empty, at most 48 UTF-8 bytes and free of control
  characters; scene uniqueness is the caller's responsibility. Softening is
  finite in `0..=1`. The lattice side is `8..=48`, and `samples^3` may not exceed
  `MAX_DENSITY_SAMPLES` (48^3 is exactly the budget).
- Charge formulas are one scalar `rho(x,y,z,t)`; steady currents are three
  comma-separated components `Jx,Jy,Jz`. Both use `FieldExpr`'s shared grammar
  and `ParameterSet` substitution. Validation evaluates every accepted interior
  lattice point at `t = 0`; any non-finite point is rejected with its position,
  never clamped or replaced by zero.
- `charge_formula: Option<String>` is the independent scalar `rho(x,y,z,t)`
  paired with a time-dependent current, in the same grammar. Validation parses
  it when present (including `ParameterSet` substitution) and rejects a
  non-empty requirement violation, but never decides physical consistency;
  that is a diagnostic question. It must be absent for `VolumeCharge` and
  `SurfaceCharge`, whose own `formula` is already `rho`. A steady current
  ignores it and keeps the historical `div J = 0` check unchanged.
- `TimeLaw::Steady` is the historical behaviour and is exactly one. A harmonic
  law requires finite `0 < omega <= MAX_ANGULAR_FREQUENCY` and a finite phase;
  every report sampler multiplies the spatial density by
  `cos(omega * t + phase)`. The lattice, `MAX_DENSITY_SAMPLES` and on-demand
  refresh rules are unchanged, so a time law adds no samples and no per-frame
  work. A steady current beside a harmonic charge density is rejected
  explicitly by `validate_time_law_combination` (and therefore by
  `State::commit_densities`); the current must declare `Harmonic` itself rather
  than have its input coerced.
- Sampling is `samples^3` deterministic jittered cell centres over the support's
  axis-aligned bounding box, keyed by a splitmix64 salt over the frozen support
  geometry and lattice index (the streamline seed scheme). Jitter is at most
  0.45 cell. Points outside the support are excluded; density outside a support
  is identically zero.
- `charge_report` is only a volume-charge total: midpoint weights times the
  analytic lattice cell volume, interior-only min/max, and `sign_changes` over
  adjacent interior lattice pairs (a zero never counts as a change). `Estimate
  {error}` requires full and half passes; `error = |full - half|`. Any other
  family, or an unevaluable interior, is `Unresolved` with non-finite readouts.
  `SurfaceCharge` is a slice-A placeholder: it is validated as a scalar `rho`
  with the same interior rule and has no surface integration yet, so
  `charge_report` refuses it as `Unresolved`.
- `continuity_report` accepts a `SteadyCurrent` (and a harmonic charge family,
  which is `Unresolved`). It uses central differences at jittered interior
  lattice points. The stencil evaluates one cell outside the support interior,
  so a field singular just outside may make the report `Unresolved`. There are
  three distinct cases:
  - `TimeLaw::Steady` keeps the genuine historical check: `div J` must vanish.
    `|div J|` larger than `CONTINUITY_TOLERANCE` times the largest sampled
    `|J|` is `Violation {residual, tolerance}`; a bounded near-zero result is
    `Estimate {value, error}`.
  - A harmonic current requires `charge_formula`. The sampler evaluates that
    independent `rho` at `t` and `t + dt` with the bounded step
    `dt = min(1e-3 / omega, 1e-3)` and compares sampled `div J(t)` with the
    finite difference `-(rho(t + dt) - rho(t)) / dt`. This is a
    finite-difference check, not an analytic identity: its truncation error is
    `O(dt)` and stays far below `CONTINUITY_TOLERANCE`. The residual is
    normalized by the largest sampled `|div J|` or `|-(Delta rho)/dt|` term.
    A missing paired formula or a non-finite sample is `Unresolved`, never a
    silent pass.
  - A harmonic charge family's `formula` is `rho`, but this build does not
    model its induced current, so the report is explicitly `Unresolved` with a
    reason. It never derives a current from the same spec and never fabricates
    an `Estimate` or `Violation`.
  `worst_point` is the position of the largest residual (or `|div J|` when
  steady).
- Normalized teaching units (`eps0 = mu0 = 1`); no SI claim, and electric and
  magnetic results are never combined.

**Pinned by.** [tests/custom_density_test.rs](../tests/custom_density_test.rs):
name/geometry/budget validation (including the f32 report bound), non-finite-
inside-support rejection, the analytic uniform-sphere total, sign-change counting
(including the zero-exclusion rule), zero and divergent steady currents,
`Unresolved` instead of fabricated zeros, parameter re-evaluation, deterministic
bounded sampling, steady-document byte identity, harmonic envelope round-trip,
invalid-law rejection/drop-on-restore, clock-following charge samples, a
consistent finite-difference harmonic pair, a deliberately inconsistent pair
that must violate, a missing pair and a harmonic charge that must be
`Unresolved`, charge-formula round-trip/drop-on-restore, a discontinuous flow's
numeric violation, the steady-current/harmonic-charge rejection, and the
unchanged sample budget.

**Limitations.** Validation samples the declared lattice, not the continuum; a
singularity between accepted points is not detected. Slice B1 wires the list
into document state, persistence and the Sources panel; it still never feeds the
sampled E/B fields, renders a support wireframe or adds a new solver.

#### Custom density document state and editor (issues #96 slice B1 and #98 slice B)

**Public API.** `viewmodel::MAX_DENSITIES = 4`,
`DensityReportRow { spec_index, charge: Option<ChargeReport>, continuity:
Option<ContinuityReport> }`, `State::densities()`,
`State::commit_densities(Vec<DensitySpec>) -> Result<(), String>`,
`State::reset_densities()`, `State::refresh_density_reports()` and
`State::density_reports()`. `SourcesPanelState` keeps session-only
`DensityDraft`s and a `density_error`, and exposes `sync_densities`,
`add_density`, `remove_density` and `commit_density`. Each draft carries
`TimeLawKind { Steady, Harmonic }`, `omega`, `phase` and `charge_formula`
session mirrors beside the spatial fields.

**Contract.**
- `ViewRecord.densities: Vec<DensitySpecRecord>` is serde-defaulted and capped
  at `MAX_DENSITIES`; capture copies the committed list, and restore drops
  entries that fail `validate` against the already restored parameters, keeps
  the survivors in order, logs once and adds no dirtying beyond the enclosing
  restore. Each mirror record carries an optional `time_law` with plain
  tokens/numbers (`kind`, `omega`, `phase`) and an optional `charge_formula`
  string; both are serde-defaulted and omitted when `None`, so older documents
  stay byte-identical. An unknown law token or an out-of-range value drops only
  that law (the density keeps its spatial formula as `Steady`) with one warning.
  A structurally malformed `time_law` object — a missing `kind` or a wrong JSON
  type for `omega` — is not field-level recovery: it fails the whole document at
  deserialization, exactly like a wrong-typed `samples`, which is the existing
  envelope convention. An invalid stored `charge_formula` (or one on a charge
  family) drops only the pairing, keeping the spatial density and its law, with
  one warning. A stored steady-current/harmonic-charge conflict is demoted the
  same way, and restore never dirties. When a harmonic current loses its law
  this way, a still-valid `charge_formula` survives on what is now a steady
  current; the editor seeds the pairing into its mirror, shows that it is
  ignored while the law is steady, and keeps it through edits rather than
  destroying it. Scenes and the edit history inherit the field through the
  existing `ViewRecord`/`EditSnapshot` seams.
- `commit_densities` validates the whole list (cap, unique names, support,
  formula parsing/evaluation, sample budget, time law and the cross-spec
  steady-current/harmonic-charge rule) atomically; a stale list is untouched on
  rejection, an equal list is a no-op and a real change dirties once and clears
  stale reports. `reset_densities` clears without dirtying when already empty.
- Reports are computed only by `refresh_density_reports`, called by the panel's
  Compute action or after an accepted edit, never per frame. Volume charges get
  a charge report, steady currents a continuity report, and a surface charge
  honestly reports that it has no volume total. `Unresolved`/`Violation`
  reasons are shown verbatim and estimates show their reported error. A
  parameter change invalidates every stored report.
- The collapsed Sources section edits name, family, support kind and geometry,
  formula, samples (`MIN_SAMPLES..=MAX_SAMPLES`) and softening, plus the declared
  clock: a Steady/Harmonic selector, with `omega` (rad/s) and `phase` (rad)
  revealed only for Harmonic, and a `Charge formula` field on SteadyCurrent
  rows. A harmonic current without that pairing still commits (the model accepts
  it) and its `Unresolved` continuity reason names the missing term verbatim; an
  unparseable pairing is rejected by the model seam and shown on the row. A
  steady current may keep a non-empty pairing: the editor shows that it is
  ignored and preserves the value. Continuous numeric widgets defer through the
  existing pending-commit queue, so a drag validates and samples once on
  release. The omega/phase mirrors survive a committed Steady law, so disabling
  Harmonic does not discard the user's values; re-enabling restores them. Add is
  bounded by `MAX_DENSITIES` with a message, and a per-draft rejection never
  clears that panel-level notice. Copy keeps the normalized teaching units, the
  no-field-integration boundary and the surface-charge limit explicit; report
  lines never render a NaN placeholder, including a non-finite estimate
  extreme.

**Pinned by.** [tests/density_state_test.rs](../tests/density_state_test.rs)
(atomic validation, equal-list no-op, cap message, one history entry per edit),
[tests/persistence_test.rs](../tests/persistence_test.rs) (missing/default,
round trip, sanitised restore and cap) and
[tests/sources_panel_test.rs](../tests/sources_panel_test.rs) (on-demand reports,
verbatim unresolved/violation reasons, harmonic controls, deferred omega drags,
the steady-current ignored-pairing note and the omega/phase steady round trip).

#### `src/scene_document.rs` — versioned scene document

**Purpose.** Issue #90 slice B2a: a strict, versioned JSON form for a whole
scene (the durable view record, shared parameters and a camera pose). Pure
model only; no `State`, storage, UI, renderer or app wiring yet.

**Public API.** `SCENE_FORMAT_VERSION` (currently `1`),
`MAX_SCENE_NAME_BYTES` (`64`); `SceneCamera { position, scale, yaw, pitch,
distance, target, fov_y, ortho_zoom }` with `Default` (orbit defaults, 2D
position zero and scale `1.0`) and `validate`; `SceneDocument { version, name,
view, parameters, camera }` with `capture`, `validate`, `to_json` and
`from_json`; `export_scene` and `import_scene`.

**Contract.**
- `capture` stores the name unchanged at `SCENE_FORMAT_VERSION`; validation,
  not capture, rejects an empty or over-long name.
- `validate` checks the version first (naming the found and expected versions),
  then the trimmed-non-empty name (at most 64 UTF-8 bytes, no ASCII control
  characters), the camera and finally `ParameterSet::validate`. The view record
  is deliberately not re-validated: `ViewRecord::restore` owns those rules.
- Every camera component must be finite. `scale` is `0.05..=50`, `distance`
  `0.05..=1.0e4`, `fov_y` `1°..=179°` in radians and `ortho_zoom` within
  `OrbitCamera::MIN_ORTHO_ZOOM..=OrbitCamera::MAX_ORTHO_ZOOM`; errors name the
  field and the accepted range.
- Documents and the camera reject unknown fields (`deny_unknown_fields`);
  `from_json` maps serde failures to `invalid scene JSON: {error}` and
  validation failures to `invalid scene: {error}`. Imports never clamp, repair
  or default a field.
- `to_json` is compact serde output in declaration order `version`, `name`,
  `view`, `parameters`, `camera`, and returns an error rather than an empty
  string.
- `export_scene` validates before serializing; `import_scene` parses and
  validates.

**Pinned by.** `tests/scene_document_test.rs`: full JSON round trip including a
custom colour ramp and scalar probes, a legacy document without `probes`,
future-version rejection naming both versions, unknown
document and camera fields, the name rules, non-finite and out-of-range camera
fields, the exact `ParameterSet` error, malformed-JSON prefixes, export/import
composition, pinned field order, and validation never mutating the document.

**Limitations.** No UI, file/clipboard I/O, persistence-envelope, camera
capture or `State` wiring in this slice; later slices call
`ViewRecord::capture` at the call site.

#### `src/seed_layout.rs` — explicit streamline seed layout

**Purpose.** Issue #87: the pure document model and deterministic generators
for explicit streamline seeds. A seed is a world position plus an optional
planting-plane anchor; the layout is document state that is validated,
sanitised, persisted and captured by the edit history. Slice S1 added the
model; S2 adds the line/circle/region generators and the pure drag clamp. Named
layouts are S3.

**Public API.** `MAX_SEEDS` (`256`), `MAX_GENERATED_PER_APPLICATION` (`64`);
`SeedAnchor { axis, index, u, v }` (`axis` is a `PlaneAxis`);
`SeedPoint { position, anchor: Option<SeedAnchor> }`; `SeedLayout { seeds }`
with `Default` (empty), `is_empty`, `len`, `clear`, `validate`, `sanitized` and
`push`; `SeedError`; `line_seeds`, `circle_seeds`, `region_seeds` and
`clamp_seed_local`.

**Contract.**
- `validate` never repairs: it rejects more than `MAX_SEEDS` seeds, any
  non-finite position, any position outside the inclusive domain bounds (with a
  small absolute round-off epsilon) and any non-finite anchor `u`/`v`. Errors
  name the seed index and the failed rule.
- `sanitized` is total: it drops invalid seeds, then truncates to `MAX_SEEDS`,
  preserving insertion order. An empty layout is valid.
- `push` validates the point and enforces the cap, rejecting in full without
  partially updating the collection. Generators append; insertion order is
  preserved.
- The anchor stores the plane axis, slice index and local `u`/`v` so a reopened
  plane can restore them through `PlaneFrame::to_world`/`to_local`. It is a
  record, not a live binding: moving the plane never moves a stored seed.
- Serde keeps `glam` out of the schema: `position` is a plain `[f32; 3]` and
  `axis` is the lower-case `x`/`y`/`z` token. An older/absent record loads the
  empty layout.
- In `State`, `commit_seed_layout` validates against `scene_domain`, is a
  validated no-op when equal and otherwise dirties once; `reset_seed_layout`
  clears and dirties only when non-empty. Startup restore sanitises the stored
  layout against the same `scene_domain` authority and assigns it directly,
  warning once when seeds were dropped.
- Generators are pure, deterministic and bounded: `line_seeds` includes both
  endpoints (`count >= 2`), `circle_seeds` uses the `PlaneFrame` basis normal
  to the chosen axis starting at angle 0 (`count >= 3`, `radius > 0`), and
  `region_seeds` reuses `explicit_streamline_seeds`'s jitter/lattice/salt
  scheme over `[min, max]`. `SeedError` names non-finite input, degenerate
  geometry, zero/below-minimum/above-cap counts, points outside the scene
  domain and an application above the layout cap. Nothing is clipped or
  truncated, and an over-cap application is rejected in full.
- `clamp_seed_local` clamps a dragged `(u, v)` into the frame's half extents
  and drops the normal component; non-finite components collapse to the plane
  centre.

**Pinned by.** `tests/seed_layout_test.rs`: the empty layout, the cap and
one-over rejection, sanitising non-finite/out-of-domain seeds and truncation,
the rotated-plane anchor round trip, in-full push rejection and insertion
order, the generators' determinism/inclusivity and every rejection, in-full
over-cap applications, explicit-seed grid replacement and Click composition,
the anchor surviving a plane switch, the drag clamp, and the commit
no-op/dirty/reset rules. `tests/persistence_test.rs` round-trips the layout
through the envelope, defaults older records to empty, sanitises a genuinely
corrupt stored layout on restore (invalid seeds dropped, valid seeds kept in
order, the result re-validated against the scene domain) and pins the
non-dirtying startup assignment seam; `tests/scene_document_test.rs` keeps a
non-empty layout through a saved scene. `tests/edit_history_app_test.rs` pins
one entry per generated application and per completed drag, that a capture
activation mid-drag stops the seed and keeps one entry, and that hiding the Seed
editor disarms placement while a visible editor still plants exactly one seed;
`tests/streamlines_test.rs` pins the byte-identical empty-layout grid, and
`tests/sidebar_controls_test.rs` pins the Flow-section enable/visibility rules.

**Residual risk.** The headless viewport returns before
`seed_pointer_interaction`, so the drag/placement tests call the production
`update_seed_drag` and `seed_click` seams directly. The pointer-response gating
and camera suppression around those seams remain pinned only by review.

**Limitations.** The Flow editor, placement, picking and dragging are in S2;
named layout libraries and portable text are S3. Seed positions are validated
against the visible `scene_domain`, so a non-Z plane slice in 2D still uses the
S1 authority even though the stored position is documented as world.

#### `src/seed_layout_library.rs` — named seed-layout library

**Purpose.** Issue #87 slice S3a: validated, versioned user seed layouts kept
beside the built-in empty `Default`, with a documented cap, case-sensitive name
uniqueness and an atomic portable import/export format. Pure model only; no UI,
no `State` application and no history wiring.

**Public API.** `SEED_LAYOUT_FORMAT_VERSION = 1`,
`MAX_LAYOUT_NAME_BYTES = 48`, `MAX_USER_LAYOUTS = 16`,
`DEFAULT_LAYOUT_NAME = "Default"`; `SeedLayoutProfile { name, seeds }` with
`validate(&Domain)` and `sanitized(self, &Domain)`;
`SeedLayoutDocument { version, layouts }` with `validate(&Domain)`, `to_json`
and `from_json`; `SeedLayoutLibrary` (`Default`, `new`, `user`, `len`,
`is_empty`, `upsert`, `remove`, `all`, `import_text`, `export_text`).

**Contract.**
- A profile name must be non-empty, trimmed, at most 48 bytes and never
  `Default`; `validate` also delegates to `SeedLayout::validate(domain)`.
- `sanitized` is total: it trims the name, truncates on a UTF-8 character
  boundary, resets an unusable name (empty or the built-in) to the stable
  placeholder `"Layout"`, and runs `SeedLayout::sanitized(domain)`. Its output
  always validates.
- Documents are `deny_unknown_fields`, check the version first, then every
  profile and then case-sensitive name uniqueness. `to_json` emits compact
  pinned JSON (`version`, `layouts`; per layout `name`, `seeds`; per seed
  `position`, `anchor`; per anchor `axis`, `index`, `u`, `v`); `from_json`
  probes the version before the body. Structural failures use the
  `invalid seed layouts: ` prefix, serde failures `invalid seed layout JSON: `.
  Seed positions are domain-dependent, so `validate(&Domain)` (which
  `import_text` runs) owns that check.
- `SeedLayoutLibrary::new` validates every profile against the supplied domain,
  rejects duplicate names case-sensitively, refuses the built-in name and
  enforces `MAX_USER_LAYOUTS`.
- `upsert` validates, replaces a same-named entry in place (order preserved) or
  appends within the cap; a rejected upsert changes nothing. `remove` never
  removes the built-in. `all` creates the empty `Default` on demand first, then
  the user layouts; the built-in is never stored, exported or deleted.
- `import_text` parses, validates every seed against the domain and merges
  through a candidate clone, so any error leaves the library untouched; it
  returns the post-merge user count. `export_text` is user-only compact JSON, or
  an empty string when the library has no user layouts.

**Pinned by.** `tests/seed_layout_library_test.rs`:
`layouts_round_trip_through_a_versioned_document`,
`documents_reject_unknown_fields_future_versions_and_duplicate_names`,
`library_rejects_duplicates_the_builtin_name_and_the_cap`,
`library_import_merges_atomically_and_reports_the_user_count`,
`library_export_is_user_only_compact_and_empty_when_unset`,
`built_in_default_is_always_first_and_never_stored`,
`sanitized_repairs_names_and_drops_invalid_seeds_total` and
`saving_a_layout_replaces_in_place_and_preserves_order`. The envelope is pinned
by `tests/persistence_test.rs`:
`seed_layout_profiles_round_trip_through_the_envelope`,
`startup_restore_of_seed_layouts_is_non_applying_and_non_dirtying` and
`corrupt_seed_layout_profiles_are_sanitised_on_load`.

**Limitations.** No Flow-section panel UI, apply/save-current action, startup
apply or history entry in this slice; those are S3b.

#### `src/edit_history.rs` — bounded edit history and reset scopes

**Purpose.** Issue #86 slice A1: a pure, in-memory undo/redo model for the
editable document, with transaction semantics for continuous gestures and the
four reset scopes. No `State`/`VfApp`, UI, input, persistence, renderer or
simulation wiring in this slice.

**Public API.** `MAX_HISTORY` (`64`); `EditScope { Color, Camera, Simulation,
All }` with `ALL`, `label` and `records_history`; `EditSnapshot { view,
parameters, camera, appearance }` with `new`; `EditHistory<S>` (private stacks)
with `new`, `len`, `redo_len`, `is_empty`, `can_undo`, `can_redo`,
`is_recording`, `begin`, `commit`, `cancel`, `record`, `undo`, `redo` and
`clear`, plus a hand-written `Default` for every `S: Clone + PartialEq`.

**Contract.**
- An entry is a document-value snapshot: `ViewRecord`, `ParameterSet`,
  `SceneCamera` and `AppearanceProfile`. Runtime state is never stored; the
  appearance name is a sentinel, since a snapshot carries style data only.
- `begin` keeps the first pre-edit snapshot when called again during an open
  transaction. `commit` records at most one entry, ends the transaction and
  returns `true` only when `after != before`; a commit without `begin` records
  nothing and returns `false`.
- A recording drops the redo tail and evicts the oldest entry once the undo
  stack reaches `MAX_HISTORY`. `cancel` discards only the pending snapshot.
  `record(before, after)` is `begin` + `commit`.
- `undo(current)` pops the last past snapshot, pushes `current` onto redo and
  returns the popped value; `redo(current)` is the mirror image and preserves
  the bound. Both return `None` and change nothing when their stack is empty.
- `records_history` is `false` only for `Simulation`: runtime state is outside
  the document snapshot, so its reset is undo-free. `Color`, `Camera` and `All`
  are recorded.

**Pinned by.** `tests/edit_history_test.rs`: record/undo/redo transitions, the
redo tail, unchanged and begin-less commits, a repeated `begin`, `cancel`, the
70-record eviction and reverse walk, no-ops at both ends, redo staying within
the bound, the four scope labels/`records_history` values, and `EditSnapshot`
equality changing one field at a time.

**Limitations.** The history is in-memory only and does not survive restart;
snapshot capture/apply and all app wiring live in
[`src/app/history.rs`](#srcapphistoryrs--edit-history-wiring).

#### `src/scene_store.rs` — saved-scene store

**Purpose.** Issue #90 slice B2b: a validated, ordered collection of
`SceneDocument`s plus the versioned envelope and storage seams behind the
My-presets section of the Presets panel. This module picks no path and touches
no UI or app state; native callers pass the directory (the panel passes
`ui::presets::default_preset_dir()`).

**Public API.** `SCENE_STORE_VERSION` (currently `1`), `SCENES_FILE`
(`scenes.json`), `MAX_SCENES` (`64`), `SCENES_STORAGE_KEY`
(`vector-field-explorer.saved-scenes`, wasm only); `SceneStore` with `new`,
`from_documents`, `scenes`, `get`, `upsert`, `delete`, `to_json` and
`from_json`; `export_store` and `import_store`; native `load_dir`/`save_dir`;
wasm `load_wasm`/`save_wasm`; `storage_note`.

**Contract.**
- Order is insertion order. `from_documents`, `upsert` and `from_json` validate
  every document through `SceneDocument::validate` and reject exact,
  case-sensitive duplicate names and more than `MAX_SCENES`; errors name the
  scene or the cap. A rejected insert leaves the store unchanged.
- `upsert` replaces the same name in place and appends otherwise. `delete`
  removes by name and reports whether anything was removed.
- `to_json` is compact `{"version":1,"scenes":[...]}`, with the envelope's
  `version` then `scenes` and each document's declaration order pinned; a serde
  failure is an `Err`, never an empty string.
- `from_json`/`import_store` use `deny_unknown_fields` on the envelope, require
  the store version to equal `1` (naming the found and expected versions), then
  validate every document. Any failure rejects the whole payload atomically: no
  clamping, dropping or partial load.
- Native `load_dir`: a missing `scenes.json` is an empty store with no note; an
  unreadable, corrupt or invalid file is an empty store plus a note naming the
  unusable file, logged with `log::warn!`. `save_dir` creates the directory,
  serializes and writes the file.
- Wasm: a missing key is an empty store with no note; corrupt or unavailable
  storage is an empty store plus `storage_note()`, logged. `save_wasm` writes
  through and returns the error text on failure.
- Library code has no panic path: no `unwrap`, `expect` or indexing.

**Pinned by.** `tests/scene_store_test.rs`: JSON round trip and order,
duplicate-name and 64-scene cap rejection, future envelope and scene versions,
atomic whole-payload rejection with an untouched target, in-place replacement,
delete reporting, native missing/round-trip/corrupt-file behaviour, export and
import composition with a failure, pinned compact field order, and rejected
upserts leaving the store unchanged.

**Limitations.** The collection is replaced or edited only through the
My-presets section; there is no per-scene rename or reordering, and the app
supplies the directory and user-facing copy.

#### `src/persistence.rs` — one settings envelope

**Purpose.** Restore and save style, layout, parameter, appearance, interaction
profile, named seed layout and view state under one eframe storage key, with
migration and corruption tolerance.

**Public API.** `AppSettings { style, layout, view, parameters, viewport_style,
layer_styles, appearance_profiles, interaction, interaction_profiles,
seed_layouts, preset_ui, object_style, up_axis, bake }`,
`BakeSettings { enabled, resolution_3d, resolution_2d }`,
`AppSettings::sanitized`;
`ViewRecord` (all fields, `capture`, `restore`, `restore_material_fields`,
`restore_maxwell_fields`, `restore_maxwell_wave`, `Default`); `finite_or`;
`load`, `save`; private serde helpers.

**Schema (all under `eframe::APP_KEY`).**

| Record | Fields | Compatibility |
| --- | --- | --- |
| `AppStyle` | `theme_dark`, `accent: Color32`, `rounding`, `spacing`, `font_size` | required in the envelope so a bare ticket-07 style fails to decode and is migrated |
| `LayoutState` | `controls_expanded`, `presets_expanded`, `sources_expanded`, `induction_expanded`, `ode_expanded`, `materials_expanded`, `maxwell_expanded`, matching widths (including `maxwell_width`), `help_visible`, native `cas_expanded`, `cas_width` | `#[serde(default)]`; widths sanitized; Sources defaults collapsed/280, Maxwell 300 |
| `ViewRecord` | formula, mode, domain min/max, density, arrow scale/opacity/length mode/magnitude scale/proportional exponent, colormap, custom colour ramp, colour mode, colour quantity, auto-fit and range, `show_grid`, `show_coordinate_system`, arrows, 3D/2D arrow style, arrow params, streamlines, seed mode, explicit seed layout, left-click action, particles, particle size/count/appearance (8 flat fields), `field_mode`, `typeset_output`, `source_entries`, `source_display`, `source_softening`, `source_allow_inaccurate`, `induction_case`, `induction_center`, `induction_axis`, `induction_radius`, `induction_length`, `induction_turns`, `induction_amplitude`, `induction_frequency`, `induction_display`, `induction_allow_inaccurate`, `ode_source`, `ode_dimension`, `ode_equations`, `ode_seeds`, `ode_duration`, `ode_chart`, `ode_variables`, `material_case`, `material_background_epsilon_r`, `material_background_mu_r`, `material_material_epsilon_r`, `material_material_mu_r`, `material_radius`, `material_interface_axis`, `material_interface_offset`, `material_applied`, `material_view`, `maxwell_case`, `maxwell_e0`, `maxwell_b0`, `maxwell_separation`, `maxwell_radius`, `maxwell_outer_radius`, `maxwell_charge`, `maxwell_charge_height`, `maxwell_outline`, `maxwell_applied`, `maxwell_elements`, `maxwell_allow_inaccurate`, `maxwell_wave`, legacy `normalize`, legacy `speed` | `#[serde(default)]` + per-field defaults/aliases; both reference flags have explicit field-level true-on-missing serde defaults (a missing bool otherwise becomes false); source fields default to Formula/empty/Both/`0.1`/false and the opt-in only accepts an exact `true`; induction fields default to the valid current-loop scene (`current_loop`, origin, +Z axis, radius 1, length 0, turns 1, amplitude 1, frequency 0.25, Both, opt-in false) and the opt-in only accepts an exact `true`; ODE fields default to the valid typed harmonic scene (`equations`, dimension 2, `y`/`-x`/`0`, one seed at `[1,0,0]`, duration 20, `phase`, `[true,true,false]`); Material fields default to the valid dielectric sphere (`dielectric_sphere`, εr 1/4, μr 1/1, radius 1, axis `z`, offset 0, applied `[1,0,0]`, view `total`); Maxwell fields default to the valid parallel-plate scene (`parallel_plates`, E0/B0 1, separation/radius/charge/charge-height 1, outer radius 2, outline `circle:1`, applied `[1,0]`, 128 elements, opt-in false) with the opt-in accepting only an exact `true`; `maxwell_wave` is serde-default `None` and stores the self-contained spec as plain arrays (no `glam` in the schema), while an invalid stored spec drops on restore with one warning; `maxwell_outline` round-trips both cross-sections as `circle:<radius>` or `square:<half-extent>`; issue #83 colour fields default to `None`/linear/magnitude and an invalid ramp/mode is dropped on restore without losing the record; `normalize` and `speed` are read-only legacy; `typeset_output` defaults to true |
| `ParameterSet` | `version`, `entries` (`Parameter { name, value, min, max, step }`) | `#[serde(default)]`; an absent key loads version 1 with no entries; an invalid set sanitizes to the default |
| `LayerStyles` | `layers: [LayerOverride; 6]` in `LayerId::ALL` order, `families: [LayerOverride; 2]` in `PhysicsFamily::ALL` order; each override has optional `opacity`, `width`, `color` | `#[serde(default)]`; an absent key loads the neutral record; an invalid record is repaired by `sanitized` with a warning and never fails the envelope |
| `appearance_profiles: Vec<AppearanceProfile>` | user profile records (`name`, `viewport`, `layer_styles`, `color`) | `#[serde(default)]`; an absent key loads an empty library; each entry is sanitized, an entry whose sanitized name duplicates an earlier one is dropped with a warning, and entries beyond `MAX_USER_PROFILES = 32` are dropped with a warning |
| `interaction: InteractionProfile` | active bindings (`name`, one chord per remappable action) and camera feel (`orbit_sensitivity`, `zoom_speed`, three invert flags) | `#[serde(default)]`; an absent key loads `InteractionProfile::default()` (today's chords, neutral feel); an invalid record is repaired by `InteractionProfile::sanitized` with a warning and resets to the default if it still fails validation |
| `interaction_profiles: Vec<InteractionProfile>` | user interaction profiles only (the built-in `Default` is never stored) | `#[serde(default)]`; an absent key loads an empty library; each entry is sanitized, an entry named `DEFAULT_PROFILE_NAME` or whose sanitized name duplicates an earlier one is dropped with a warning, and entries beyond `MAX_USER_PROFILES = 16` are dropped with a warning |
| `seed_layouts: Vec<SeedLayoutProfile>` | user named seed layouts only (the built-in empty `Default` is never stored) | `#[serde(default)]`; an absent key loads an empty library; each entry is sanitized against the restored view domain, an entry whose trimmed name is empty or `DEFAULT_LAYOUT_NAME` or whose sanitized name duplicates an earlier one is dropped with a warning, and entries beyond `MAX_USER_LAYOUTS = 16` are dropped with a warning |
| `object_style: ObjectStyle` | `Solid` or `Wireframe` | `#[serde(default)]`; an absent key loads `Solid`; an unknown token is repaired to `Solid` with one warning by the enum decoder, and a non-string stored value is repaired to `Solid` by the field wrapper, leaving every other field intact |
| `up_axis: UpAxis` | display-only `Z` (default), `X` or `Y`, stored lowercase as `z`, `x`, `y` | `#[serde(default)]`; an absent key loads `Z`; an unknown token is repaired to `Z` with one warning by the enum decoder, and a non-string stored value is repaired to `Z` by the field wrapper. The default is genuinely Z-up (physics convention); the token `y` selects the old historical Y-up view. It is assigned at the startup seam without dirtying and never appears in `ViewRecord`, scenes, appearance profiles or edit history |
| `bake: BakeSettings` | `enabled` (default false), `resolution_3d` (default 48), `resolution_2d` (default 128) | `#[serde(default)]`; an absent key loads the disabled record; out-of-range resolutions clamp into `16..=64` / `32..=256` with one warning. Only the settings are stored; the lattice is runtime-only and never enters `ViewRecord`, scenes, profiles or history. An enabled startup build is bounded and non-dirtying; a refusal records the reason instead of failing startup |

**Migration and fallback rules.**
1. `load(None)` → defaults.
2. `eframe::get_value::<AppSettings>` succeeds → `sanitized()`.
3. Else `eframe::get_value::<AppStyle>` succeeds → ticket-07 bare-style
   migration: keep the style, default layout and view, log a warning.
4. Else, if the key holds any string, log “corrupt” and return defaults.
5. A partially valid envelope is decoded with serde defaults; `sanitized`
   clamps style/layout numbers; `ViewRecord::restore` sends every value through
   the viewmodel setters and then `sidebar::apply`, which clamps domain extents,
   swaps reversed corners, and keeps the previous extent for NaN.
6. A stored bad formula restores through `set_formula`, so the last good field
   stays and `parse_error` is reported; the app still starts.
7. `normalize: true/false` maps to Uniform/Proportional when present; new records
   never write it. Unknown `left_click_action` names fall back to
   `SeedStreamline`. Old `Pyramid`/`Line` spellings decode to Cone/Flat.
8. Unknown future keys are ignored by serde; absent fields take their documented
   defaults.
9. Source records restore after the formula/controls: each entry is revalidated
   through `SourceScene::from_records`, malformed entries are skipped with a
   one-line `source_notice`, softening outside `1e-6..=1.0` falls back to `0.1`,
   the accuracy opt-in is true only for an exact stored `true`, and Sources mode
   falls back to Formula when no valid entry remains. `formula` and the source
   scene never overwrite each other.
10. Induction restores after the mode: the case token, center, axis and
    parameters are revalidated through `InductionScene::validate`. Invalid
    fields (unknown case, non-finite values, negative radius/frequency,
    non-unit axis, or an invalid solenoid) restore the valid default scene,
    clear the prepared/sampling caches and set `induction_error`; Induction
    mode then falls back to Formula. An absent legacy induction payload is the
    valid default scene and sets no error. The shared `source_softening` field
    remains the one softening value.
11. ODE restores lazily through `State::restore_ode_fields`: the source token,
    dimension, equations, seeds, duration, chart and variables are validated by
    the normal `prepare_ode` path, no solver runs during restore, any stale
    solution is cleared and the first Ode frame/`refresh_ode_if_dirty` solves
    the scene. Malformed values (unknown source, non-finite seed, invalid
    dimension/duration, unparseable equation or a current-field mismatch) keep
    an explicitly stored Ode mode but restore the valid default scene, record
    `ode_error` and still leave the default scene pending. Legacy payloads keep
    Formula selected and set no ODE error. Global reset restores the default
    scene, chart `phase` and variables `[true,true,false]` and clears any shared
    Ode loop period.
12. Materials restores through `restore_material_fields`: case/view/axis tokens
    and every number are revalidated by `validate_material`; the durable scene
    is stored without sampling. A malformed payload restores the valid default
    dielectric sphere and sets `material_error` only when the stored mode is
    explicitly Materials, so an old or damaged record never promotes the mode.
    Global reset clears the Material scene, sampling cache and error before
    returning to Formula; the Materials section Reset restores only the default
    scene. `VfApp::new_with_storage` refreshes the panel draft through
    `MaterialPanelState::sync_from` after a restored Materials record.
13. Maxwell restores through `restore_maxwell_fields`: the case and flat
    `circle:<radius>`/`square:<half-extent>` outline tokens plus every number
    are revalidated by `validate_maxwell`; `State::restore_maxwell_scene` stores
    the durable scene, drops sampling and the private BEM cache, clears the
    error, marks the first frame dirty and never samples or solves. A malformed
    payload restores the valid default parallel-plate scene and sets
    `maxwell_error` only when the stored mode is explicitly Maxwell, so an old
    or damaged record never promotes the mode. Global reset clears the Maxwell
    scene, sampling, BEM cache and error before returning to Formula; the
    Maxwell section Reset restores only the default scene. `tests/maxwell_persistence_test.rs`
    pins this record.
14. An absent `layer_styles` record loads neutral. A stored invalid record is
    repaired by `LayerStyles::sanitized` (a non-finite value becomes unset, a
    finite out-of-range value clamps into its bound) with a warning, and the
    envelope still loads. Startup restore assigns the loaded record directly,
    without dirtying the scene.
15. An absent `appearance_profiles` list loads empty. Stored entries are
    sanitized with `AppearanceProfile::sanitized`; an entry whose sanitized
    name duplicates an earlier entry is dropped with a warning, and entries
    beyond `MAX_USER_PROFILES` are dropped with a warning. Restoring the
    envelope never applies a profile and never dirties the scene; the live
    library is wired in slice A2b.
16. An absent `interaction` record loads the documented default chords and
    neutral camera feel. A stored invalid record is repaired by
    `InteractionProfile::sanitized` (name trim/truncate, out-of-range
    multipliers clamp, non-finite values reset, missing/duplicate/reserved/
    browser-owned bindings return to defaults) with a warning; if it still
    fails validation it resets to the default. Startup assigns the loaded
    record to `VfApp::interaction` directly, without applying it to the
    document, camera, layout or panels and without dirtying the scene.
17. An absent `interaction_profiles` list loads empty. Stored entries are
    sanitized with `InteractionProfile::sanitized`; an entry named
    `DEFAULT_PROFILE_NAME` is the built-in and is dropped with a warning, an
    entry whose sanitized name duplicates an earlier entry is dropped, and
    entries beyond `interaction::MAX_USER_PROFILES = 16` are dropped. A
    library that still fails `ProfileLibrary::new` logs a warning and starts
    empty rather than failing startup. Save writes the live active record and
    `interaction_profiles.user()`, so a save cannot silently discard loaded
    profiles.
18. An absent `seed_layouts` list loads empty. Each stored profile is
    sanitized against the domain the stored view record restores to (the
    repaired 2D plane collapse or plane-slice local domain); an entry whose
    trimmed name is empty or `DEFAULT_LAYOUT_NAME` is dropped, the first of a
    case-sensitive duplicate survives, and entries beyond
    `MAX_USER_LAYOUTS = 16` are dropped. A library that still fails
    `SeedLayoutLibrary::new` logs a warning and starts empty rather than
    failing startup. Startup never applies a layout or dirties the scene, and
    save writes `seed_layouts.user()`.

**Tested by.** `tests/persistence_test.rs` (109 tests):
`settings_round_trip_through_eframe_storage`,
`a_ticket_07_style_only_record_still_loads_its_style`,
`corrupt_storage_falls_back_to_defaults`, `absent_storage_falls_back_to_defaults`,
`every_view_value_round_trips`,
`a_legacy_normalize_true_migrates_to_uniform_without_fail_fast`,
`a_legacy_normalize_false_migrates_to_proportional`,
`a_pre_ticket_64_record_decodes_old_line_to_flat_in_both_sets`,
`an_unknown_left_click_action_loads_as_seed_streamline`,
`compact_mode_and_sheet_are_never_written_to_storage`,
`a_bad_formula_in_the_record_keeps_the_last_good_field`,
`each_reference_preference_survives_storage_restore_and_constructor_independently`,
`a_partial_reference_record_defaults_only_the_missing_coordinate_flag`,
`source_scene_round_trips_through_the_view_record`,
`a_legacy_record_opens_in_formula_with_opt_in_off`,
`malformed_source_records_are_skipped_without_losing_other_settings`,
`a_record_without_valid_sources_forces_formula_mode`,
`source_records_keep_order_and_skip_invalid_numbers`,
`global_reset_clears_source_scene_and_sampling_cache`,
`induction_scene_round_trips_and_legacy_defaults_to_formula`,
`malformed_induction_fields_fall_back_and_report_the_error`,
`global_reset_clears_induction_cache_and_restores_defaults`, and the
restore/sanitize tests. The user-profile envelope is pinned by
`settings_drop_duplicate_or_excess_user_profiles_on_load`,
`restoring_the_envelope_does_not_apply_profiles_or_dirty_the_state`,
`new_with_storage_loads_the_user_profile_library_without_applying_it` and
`user_profile_library_round_trips_through_an_app_save`. The interaction
envelope is pinned by
`interaction_settings_round_trip_through_the_envelope`,
`older_envelopes_without_interaction_fields_load_the_defaults`,
`sanitizing_repairs_an_invalid_active_interaction_record`,
`sanitizing_drops_builtin_duplicate_and_excess_user_interaction_profiles`,
`new_with_storage_restores_interaction_settings_without_dirtying_or_applying`
and `user_interaction_profiles_round_trip_through_an_app_save`. The
named seed-layout envelope is pinned by
`seed_layout_profiles_round_trip_through_the_envelope`,
`startup_restore_of_seed_layouts_is_non_applying_and_non_dirtying` and
`corrupt_seed_layout_profiles_are_sanitised_on_load`. The
object-style field is pinned by
`object_style_defaults_to_solid_when_the_field_is_missing`,
`wireframe_object_style_round_trips_through_storage`,
`corrupt_object_style_falls_back_to_solid_without_losing_the_envelope`,
`non_string_object_style_is_repaired_without_losing_the_envelope` and
`object_style_setter_and_restore_own_the_dirty_flag`. The
layer-style record is pinned by
[tests/layer_style_test.rs](../tests/layer_style_test.rs): neutral resolution,
per-attribute precedence, validation and sanitising, envelope round-trip,
older-envelope defaults, and the State commit no-op/keep/one-dirty rules.

ODE persistence is pinned separately by
[tests/ode_persistence_test.rs](../tests/ode_persistence_test.rs):
`ode_scene_round_trips_all_fields`, `malformed_ode_payload_falls_back_lazily`,
`legacy_record_leaves_ode_unselected_and_error_free`,
`global_reset_clears_ode_fields`, and
`global_reset_returns_to_formula_and_clears_the_ode_loop_period`. ODE export
provenance is pinned by [tests/ode_export_test.rs](../tests/ode_export_test.rs).

**Limitations.** There is no explicit schema version number; compatibility is a
set of serde defaults and aliases that must be maintained forever. Camera state,
fly state, compact mode/sheet, and the simulation clock are deliberately not
stored. Camera presets are a separate record (see `export.rs`), not part of this
envelope.

### 3.4 Cameras, commands, and clock

#### `src/camera.rs` — orbit camera, projections, rays

**Purpose.** The live 3D orbit camera and the 2D front view, plus the
projection/ray math shared by painting, screenshots, and click seeding.

**Public API.** `OrbitCamera` fields (`yaw`, `pitch`, `distance`, `target`,
`fov_y`, `ortho_zoom`, `up_axis`) and methods; `MIN_DISTANCE`, `MAX_DISTANCE`,
`MIN_ORTHO_ZOOM`, `MAX_ORTHO_ZOOM`; `ViewCamera<'a>` (`Orbit`/`Fly`) with
`view`, `vp_matrix`, `eye_position`, `ndc_ray`, `is_fly`;
`apply_camera_record`.

**Invariants.**
- Spherical position has exactly `distance` from `target`; pitch clamps to
  ±1.55 to keep the up vector valid.
- Issue #208: `up_axis` is display-only. The default `Z` applies the physics
  Z-up rotation so world `+Z` is screen-up; `X` puts world `+X` up, and `Y` is
  the old historical Y-up orientation (identity basis). Each derives the orbit
  basis from its deterministic world `(up, forward)` pair. `ndc_ray` and click
  picking invert the same view, so they keep returning world-space rays. The 2D
  front view ignores `up_axis`.
- `zoom` clamps distance to 0.5..=20; `zoom_2d` clamps ortho zoom to
  0.05..=50 and recovers from a non-finite current value.
- The 2D view is a front view: eye at `target + Z*distance`, looking down -Z,
  Y up; `proj_ortho` uses `plane_size / ortho_zoom` and a far plane that always
  covers the target.
- `ndc_ray` is built analytically from the camera basis (not by inverting the VP
  matrix) so a centre click is exact at long distances.
- `recentre_on` moves the target to the domain centre and touches nothing else;
  `recentre_on_2d` moves only x/y.
- `apply_camera_record` leaves fly, ignores non-finite components, and clamps
  the 2D zoom.
- `ViewCamera` is the only camera abstraction consumers need; fly is always
  perspective.

**Pinned by.** `tests/camera_test.rs`, `tests/camera_pan_zoom_test.rs`,
`tests/camera_zoom_pan_test.rs`, `tests/click_seed_zoom_test.rs`,
`tests/view_orientation_test.rs`, `tests/fly_camera_test.rs::view_camera_*`:
`ndc_ray_at_center_looks_at_target`, `target_maps_to_vp_center`,
`front_view_looks_down_negative_z_with_y_up`,
`two_d_vp_uses_the_front_view_not_the_orbit_view`,
`zoom_2d_recovers_from_a_non_finite_ortho_zoom`,
`recentre_on_targets_the_domain_centre_without_changing_the_pose`,
`z_up_is_the_default_and_projects_world_z_to_screen_up`, etc.

**Limitations.** The default `Z` view intentionally changes the old pre-#208
Y-up screen orientation; the `Y` option restores it. There is no roll. The far
plane is 100 world units. The 2D camera is orthographic only.

#### `src/view_orientation.rs` — display-only vertical world axis

**Purpose.** Issue #208: `UpAxis { Z (default), X, Y }` with one deterministic
world-to-display rotation. It is display-only; coordinates, formulas, probes and
saved scenes remain in x/y/z. The labels name the world axis that actually points
up on screen: the physics-convention default is `Z up (default)`, and `Y up`
selects the pre-change historical orientation.

**Public API.** `UpAxis` (`ALL`, `label`, `basis`, `rotation`), lenient
`Deserialize` that accepts the lowercase tokens `z`, `x`, `y` and repairs an
unknown token to `Z` with one warning.

**Invariants.** `Z` is the default and a proper -90° turn about X mapping world
`+Z` to display `+Y` and world `+Y` to display forward `-Z`. `X` is a half turn
about `(1,1,0)` mapping world `+X` to display `+Y`. `Y` is the identity that
preserves the old historical view. All rotations are proper and right-handed;
`basis()` returns the inverse-rotated display `(up, forward)` world vectors.
`rotation().inverse()` reproduces world positions within float precision for
picking. `tests/view_orientation_test.rs` derives the world axis from each label
and asserts it projects to display up, pinning label/rendering agreement.

**Pinned by.** `tests/view_orientation_test.rs` (14 tests).

#### `src/fly_camera.rs` — six-DOF camera and capture policy

**Purpose.** Free-flight pose and all pure fly/capture decisions.

**Public API.** Constants (`DEFAULT_SPEED`, `MIN_SPEED`, `MAX_SPEED`,
`MAX_PITCH`, `LOOK_SENSITIVITY`, `BOOST_MULTIPLIER`, `MAX_DT`, `FOV_Y`,
`NEAR`, `FAR`, crosshair constants, wheel constants); `FlyCamera` (`from_orbit`,
`look_dir`, `right`, `up`, `step`, `adjust_speed`, `view`, `proj_perspective`,
`vp_matrix`, `ndc_ray`, `translate`); `FlyInput`; `FlyKeys` (`clear`,
`movement_held`, `apply_key`, `input`); `is_command_chord`, `toggle_fly`,
`sync_fly`, `capture_commands`, `capture_transition`,
`capture_exit_required`, `crosshair_visible`, `frame_look`, `crosshair_segments`,
`exit_on_focus_loss`, `sync_fly_mirror`.

**Invariants.**
- Fly is 3D-only. Entering snapshots the orbit eye/direction; exiting drops fly
  and returns the untouched orbit pose. Entering does not change orbit state.
- Movement keys take the physical QWERTY position first and fall back to the
  logical key; Shift boost is exactly 4×; `dt` clamps to 0..=0.1 s; speed clamps
  to 0.5..=20.
- A command chord clears movement; a text field or window focus loss exits.
- Capture is best-effort: attempt `Confined` then `Locked`, hide the cursor only
  while active, release on exit; a missing bridge leaves drag-look.
- `frame_look` uses raw motion only while captured and only when it is non-zero;
  it never sums raw and drag.
- Fly state, speed, and pose are not persisted.

**Pinned by.** `tests/fly_camera_test.rs` (31 tests):
`from_orbit_reproduces_the_orbit_eye_and_look_direction`,
`physical_key_positions_win_over_the_logical_key`,
`a_missing_physical_key_falls_back_to_the_logical_key`,
`shift_boosts_by_exactly_four`, `dt_is_clamped_and_junk_time_moves_nothing`,
`entering_and_exiting_restores_the_untouched_orbit`,
`capture_commands_confine_before_locking_and_hide_only_while_active`,
`the_sidebar_mirror_enters_fly_through_the_one_toggle_seam`,
`the_fly_camera_is_not_persisted`.

**Limitations.** No collision detection; the camera can fly through geometry
(the app is an explorer, not a game). Fly has no camera presets.

#### `src/pointer_lock.rs` — raw pointer motion bridge

**Purpose.** Supply the fly camera's raw look delta on platforms where eframe
does not provide one: winit's `Event::MouseMoved` natively, and the classic
`web/pointer_lock.js` bridge on wasm.

**Public API.** `request`, `release`, `take_delta`, `lock_lost`,
`motion_delta(ctx)`.

**Invariants.**
- Native `motion_delta` sums this frame's `Event::MouseMoved` deltas and divides
  by `pixels_per_point`, so the fly camera's 0.005 rad/point matches drag-look.
  Non-finite components become zero.
- Wasm drains the JS accumulator through
  `window.vfePointerLock.takeDelta()` exactly once per frame; `request` and
  `release` call the bridge and `release` also discards accumulated motion.
- `lock_lost` reports a lock that was acquired and later dropped (normally the
  browser Esc, which never reaches egui); an initial refusal is not a loss.
- Everything is best-effort: a missing bridge, refused lock, or missing API is
  silent, and fly remains usable with drag-look.
- Native `request`/`release` are no-ops; native `take_delta` returns zero.

**Pinned by.** `tests/pointer_lock_test.rs` (6):
`native_motion_delta_sums_raw_mouse_moves_in_points`,
`native_motion_delta_scales_hidpi_pixels_to_points`,
`native_motion_delta_ignores_non_finite_values`,
`native_take_delta_is_empty_and_request_release_are_noops`,
`the_bridge_exposes_request_release_and_a_drained_delta`,
`index_html_ships_the_pointer_lock_script_before_the_wasm_module`.

**Limitations.** Native raw motion is only available while the platform sends
`MouseMoved`; otherwise drag-look is the fallback. The bridge is a separate JS
file and must be copied by Trunk (`index.html` has the `data-trunk` copy-file
link).

#### `src/view_command.rs` — one-shot camera commands and key map

**Purpose.** One `ViewCommand` type for every non-drag camera action, plus the
pure keyboard, click-plan, and seed-shortcut rules.

**Public API.** `KEY_ORBIT_STEP = 0.06`, `KEY_PAN_FRACTION = 0.05`,
`KEY_PAN_FAST_MULTIPLIER = 3.0`, `KEY_ZOOM_FACTOR = 1.2`;
`ViewContext`; `ViewCommand { Orbit, Pan, Zoom, Reset, Fit }`;
`domain_center`, `domain_centre_delta`, `fit_distance`, `apply_view_command`,
`apply_view_command_with_fly`; `chord_modifiers`, `egui_modifiers`,
`bound_action`, `view_action_for_key`, `command_for_action`, `command_for_key`,
`command_for_key_with_bindings`, `view_key_commands`,
`view_key_commands_with_bindings`; `CLICK_HINT_*`, `ClickActionPlan`,
`click_action_plan`, `seed_shortcut`, `seed_shortcut_with_bindings`,
`shortcut_seed_point`.

**Invariants.**
- Orbit is inert in 2D; Pan uses world X/Y in 2D and the view basis in 3D;
  Zoom follows egui's “>1 = closer” convention.
- Reset returns default orientation/distance/zoom centred on the domain; Fit
  keeps orientation and fits the domain's bounding sphere in the tighter FOV
  (2D fits the padded plane and only zooms out for a narrow viewport).
- While flying, Reset/Fit leave fly first; orbit/pan/zoom commands are inert.
- View keys resolve through the active `Bindings`: arrows orbit in 3D (Shift
  pans) and pan in 2D (Shift 3× faster); `+`/`=`, `-`, `R`, `F` are the default
  chords. A remap moves the action, and the old chord no longer fires. Shift is
  tolerated for the four view directions, ZoomIn and ZoomOut, so `Shift+=`
  (plus) keeps `=` as `+`'s unshifted alias and `Shift+-` still zooms out.
- Arrow-key orbit applies `CameraFeel::orbit_sensitivity` and its yaw/pitch
  invert flags; the existing key step and camera clamps are unchanged.
- `click_action_plan`: seed requires Streamlines; particles require the feature
  and `mode_3d`; each failure returns its pinned hint.
- `seed_shortcut`: S is inert while flying (it is fly-backward); P is always
  available; repeats are ignored. `seed_shortcut_with_bindings` resolves the
  SeedStreamline/AddParticles chords from the active bindings.
- `shortcut_seed_point` uses the pointer only inside the viewport, otherwise the
  centre.

**Pinned by.** `tests/view_command_test.rs` (23 tests):
`fit_frames_square_and_rectangular_domains_deterministically`,
`arrow_keys_orbit_in_3d_and_pan_with_shift`,
`arrow_keys_pan_in_2d_and_shift_pans_faster`,
`left_drag_orbits_in_3d_and_pans_in_2d`,
`camera_keys_are_inert_while_a_text_field_has_focus`,
`command_modifiers_never_drive_the_camera`,
`reset_returns_the_default_framing_centred_on_the_domain`, and the undo/click
tests. The S/P mapping is also pinned by `tests/shortcut_test.rs`, and live
binding routing/camera feel by `tests/interaction_routing_test.rs`.

**Limitations.** Fit is sphere-based, so a very elongated box may be framed with
more margin than a tight box fit. Keyboard orbit is a fixed step, not
accelerating.

#### `src/sim_clock.rs` — the single simulation clock

**Purpose.** The only time source for animated features: particles,
time-dependent formula fields, active time-dependent scalar plots, and an
animated induction scene.

**Public API.** `MAX_STEP = 1/20`, `MIN_SPEED = 0.1`, `MAX_SPEED = 3.0`,
`DEFAULT_SPEED = 1.0`; `SimClock { time, playing, speed, .. }` with `advance`,
`advance_for`, `step_dt`, `set_playing`, `toggle_playing`, `set_speed`,
`clamp_time`, `seek`, `reset_time`, `reset`; `has_clock_consumer`,
`transport_visible`, `should_animate`.

**Invariants.**
- Non-finite/zero/negative frame times are ignored; a large frame clamps to
  `MAX_STEP`; the simulated step is `min(dt, MAX_STEP) * speed`.
- Paused frames produce a zero step and do not accumulate missed time.
- With no consumer, `advance_for` holds time and clears the step; re-enabling
  resumes from that time.
- Speed clamps to 0.1..=3.0; a non-finite value becomes 1×.
- `clamp_time(max)` shortens time to `max` and pauses once when it clamped;
  `seek(time)` clamps non-negative, clears the step and pauses. Ode uses both
  for duration-bounded playback and its Step action.
- The clock is transient: a fresh session and `Reset all controls` start at
  time 0, playing, 1×. Ode clears `loop_period` on mode entry.

**Pinned by.** `tests/particle_playback_test.rs` (17 tests):
`a_fresh_clock_is_playing_at_normal_speed`,
`pause_stops_time_and_does_not_accumulate`,
`the_clock_holds_without_a_consumer_and_resumes_from_the_held_time`,
`speed_scales_the_simulated_step`, `transport_is_visible_only_with_a_clock_consumer`,
`space_toggles_only_when_visible_and_never_while_typing`,
`reset_all_controls_restores_a_fresh_clock`, etc.

**Limitations.** One clock for everything (ADR 0002); features that need their
own time base require a new ADR. It is not persisted.

### 3.5 Motion, geometry, and decoration

#### `src/streamlines.rs` — RK4 tracing, seeds, clicks

**Purpose.** Trace field-tangent polylines and compute click seeds.

**Public API.** `StreamlineOpts { max_len, step_scale, both_directions }`,
`MAX_STREAMLINE_POINTS = 400`, `CLICK_BURST_SEEDS = 5`,
`CLICK_BURST_REACH = 0.08`; `trace`, `ray_seed`, `click_seed`, `burst_seeds`,
`seed_grid`, `explicit_streamline_seeds`.

**Invariants.**
- RK4 with adaptive `h = step_scale / |f|`; each step advances about
  `step_scale` of arc length; stop at the budget, zero/non-finite flow, or when
  the next point leaves the domain.
- `both_directions` returns one continuous path, farthest upstream → seed →
  farthest downstream; the seed appears once.
- `ray_seed` (3D) unprojects the click and lands on the focal plane through the
  orbit target (or the domain centre while flying), clamped into the box
  crossing; rays that miss fall back to a shell around the focus and may then
  trace nothing.
- `click_seed` uses `ray_seed` in 3D and the ortho ray's intersection with the
  domain's z midpoint in 2D; it is not clamped to the domain.
- `seed_grid` is a deterministic jittered lattice (jitter ≤ 0.45 cell) keyed by
  `salt`; `burst_seeds` is deterministic from the clicked point's bits and pins
  z for 2D.
- `explicit_streamline_seeds` grows the lattice until it holds the requested
  count, then truncates to exactly that count, independent of sampling density.
  A fully degenerate domain can only yield one seed even for `count > 1`; that
  intentional exception is unreachable through the UI's clamped extents.
- `trace` returns at least `[start]` for a valid seed, or empty for a bad seed.

**Pinned by.** `tests/streamlines_test.rs` (19 tests):
`constant_field_traces_straight`, `vortex_streamline_stays_round`,
`vortex_stays_bounded`, `zero_field_makes_no_advance`,
`ray_seed_seeds_the_aimed_point_and_the_focal_plane`,
`ray_seed_interior_clicks_run_the_line_through_the_box`,
`click_seed_2d_lands_on_the_midplane_under_the_click`,
`burst_seeds_are_deterministic_and_2d_pins_the_plane`,
`jittered_seeds_are_reproducible`.

**Limitations.** Fixed point budget, no adaptive error control, no event
detection; lines are static (not clock consumers). Seeds outside the domain draw
nothing.

#### `src/particles.rs` — advected cloud, trails, dot instances

**Purpose.** Live particle state, deterministic spawning/respawning, per-frame
advection, and the dot/trail instance builders.

**Public API.** `MAX_PARTICLES = 4000`, `PARTICLES_PER_DENSITY = 16`,
`SPAWN_INSET`, `DOT_SPACING_FRACTION`, `CLICK_BURST_PARTICLES = 64`,
`CLICK_SPAWN_REACH`, trail/stagnation constants; `ParticleColorMode`;
`ParticleAppearance` (+ `sanitized`); `Particles` (`with_positions`, `history`
test seams); `target_count`, `effective_particle_count`, `count`,
`effective_trail_len`, `clear`, `spawn`, `resize`, `retarget`, `spawn_at`;
`AdvectSettings`, `advect`, `advance_exact`, `speed_range`, `ParticlePalette`,
`particle_rgb`, `dot_instances`, `trail_instances`, `FlowLayerStyle`,
`dot_instances_styled_at`, `trail_instances_styled_at`, `DOT_QUAD_CORNERS`,
`dot_billboard_corners`.

**Invariants.**
- The Flow layer style multiplies after [`ParticleAppearance`]: dot radius and
  trail width scale by `width`, alpha is `appearance.opacity * opacity` clamped
  to `0.0..=1.0`, and particle RGB is untouched. The neutral `1.0`/`1.0`
  default matches the legacy `_at` entry points exactly.
- The cloud has a constant size between target changes. A particle that leaves
  the domain, hits zero/non-finite flow, or becomes non-finite respawns inside
  the inset box; the count never drifts and never exceeds `MAX_PARTICLES`
  (except a projected plane flow, whose exact zero velocity keeps the dot
  stationary in the slice instead of respawning).
- Spawn/respawn jitter is deterministic from an inline Park/Miller LCG, so a
  fresh session reproduces the same cloud. `clear` rewinds the stream.
- `resize` trims from the end or spawns only the difference; existing particles
  keep their positions. `retarget` applies only a changed target, preserving a
  click burst above the configured count.
- `advect` is one RK4 step per particle per call; a non-finite/zero step is a
  no-op; speed is `|q - p| / dt`; trails append and trim to
  `effective_trail_len`; stagnation is judged on speed, not per-step
  displacement; a respawn resets trail/stuck/speed.
- `advance_exact` splits an exported frame step into
  `ANIMATION_PARTICLE_SUBSTEP` (1/120 s) steps plus a final remainder and calls
  `advect` at each substep's exact time. It deliberately bypasses the GUI
  clock's `MAX_STEP` clamp; non-finite or non-positive steps are no-ops.
- Trails are capped at 32 points per particle and 100 000 points across the
  cloud; trail alpha fades toward the tail; width scales with grid spacing.
- Dot instances use `origin.w = PARTICLE_KIND`, `dir_len.w = radius`; RGB follows
  Magnitude / Speed / Fixed; alpha is the particle opacity.
- With an active `ParticlePalette::mapping`, Magnitude colours through the full
  mapping (quantity/palette/scale/manual-or-frozen range) and Speed uses only
  that palette with the observed `speed_range`; Fixed ignores the mapping.
  Without a mapping the legacy colormap calls are byte-identical. Dots and
  trails share one palette; trails sample the newest segment's midpoint.
- `dot_billboard_corners` mirrors the camera-facing quad used by both dot
  shaders.

**Pinned by.** `tests/particles_test.rs` (63 tests):
`spawn_is_reproducible`, `spawn_2d_mode_pins_the_plane`,
`outside_particle_respawns_inside`, `count_constant_under_vortex_advection`,
`resize_caps_at_the_particle_budget`,
`retarget_applies_a_changed_target_but_preserves_a_click_burst_above_it`,
`zero_step_leaves_particles_put`, `speed_is_displacement_over_dt`,
`stagnation_verdict_uses_speed_not_step_displacement`,
`trails_append_cap_and_keep_the_newest_point_last`,
`a_full_cloud_with_requested_32_stays_inside_the_segment_cap`,
`dots_colored_by_field_magnitude`, `fixed_dots_use_the_fixed_colour_and_speed_dots_use_speed_range`,
`particle_magnitude_uses_custom_ramp_and_manual_range`,
`particle_magnitude_follows_component_quantity`,
`particle_legacy_colours_are_byte_identical_without_a_mapping`,
`particle_speed_uses_custom_palette_with_its_own_range`,
`particle_fixed_colour_ignores_mapping`,
`particle_dots_and_trails_agree_for_the_same_palette`,
`non_finite_particle_sample_falls_back_to_mid_grey`.
`tests/dot_billboard_test.rs` pins the billboard geometry;
`tests/animation_scene_test.rs` pins
`scene_color_facts_collects_stats_from_drawn_arrows`.

**Limitations.** CPU simulation only; no GPU compute. Particles only exist in
the 3D view (`particle_target` keys on `mode_3d`). Trail history is capped, and
no particle state is persisted.

#### `src/arrow_style.rs` — family selection and geometry parameters

**Purpose.** Independent 3D and 2D arrow families plus their shared
head/shaft parameters and serde aliases.

**Public API.** `MIN/MAX_HEAD_LENGTH`, `MIN/MAX_HEAD_WIDTH`,
`MIN/MAX_SHAFT_RADIUS`, `MIN/MAX_SIDES`; `ArrowStyle { Cone, Cylinder, Flat,
Streamline }` and `ArrowStyle2D { Flat, Streamline }` with `ALL`, `label`,
`tooltip`, `id`, `uses_sides`, `uses_head`, `vertex_count`; `ArrowParams`
(`head_length`, `head_width`, `shaft_radius`, `sides`) and `sanitized`.

**Invariants.**
- The 3D and 2D sets are independent; changing one never changes the other.
- Shader ids are stable: 3D `Cone=0, Cylinder=1, Flat=2, Streamline=3`; 2D
  `Flat=0, Streamline=1`.
- Vertex counts: Cone `12 * sides - 6`, Cylinder `9 * sides - 6`, Flat 9,
  Line 6; 2D Flat 9, Line 6.
- `sides` applies only to Cone/Cylinder; head sliders only to Cone/Flat.
- Old persisted spellings `Pyramid` → Cone and `Line` → Flat are aliases; the
  headless family persists as `Streamline` and is labelled “Line” in the UI.
- `sanitized` clamps every finite value and falls back to the default for
  non-finite values.

**Pinned by.** `tests/arrow_style_test.rs` (30 tests):
`styles_have_stable_shader_ids`, `vertex_counts_match_the_family_budgets`,
`old_style_names_decode_to_the_new_families_and_new_names_are_stable`,
`the_two_style_fields_never_alias`, `three_d_draw_plan_ignores_the_2d_style`,
`every_2d_family_has_a_draw_plan`, `uses_sides_and_uses_head_match_the_families`,
`non_finite_params_fall_back_to_the_defaults_and_finite_ones_clamp`.

**Limitations.** Only these four 3D and two 2D silhouettes exist; no custom
meshes. The Line family is headless, so head controls do nothing for it.

#### `src/arrow_geometry.rs` — CPU mirror of shader geometry

**Purpose.** Reconstruct the shaders' generated triangles in Rust so triangle
counts, winding, caps, and non-degeneracy can be tested without a GPU.

**Public API.** Three `#[doc(hidden)] pub` test seams:
`cone_triangles(len, params)`, `cylinder_triangles(len, params)`,
`flat_arrow_triangles(len, params)`.

**Invariants.**
- Cone: `4 * sides - 2` triangles (`12 * sides - 6` vertices): tapered shaft,
  head, and a full underside plate fan.
- Cylinder: `3 * sides - 2` triangles (`9 * sides - 6` vertices): constant-radius
  shaft plus a far-end cap, no head or plate.
- Flat: three triangles (9 vertices) in the local XY plane.
- Winding matches the shader's declared vertex order and the cap faces outward.

**Pinned by.** `tests/arrow_geometry_test.rs` (9 tests):
`the_cone_layout_matches_its_budget_and_has_area`,
`the_cone_plate_fan_faces_down_and_covers_the_ring`,
`the_cylinder_layout_is_headless_and_matches_its_budget`,
`the_flat_2d_arrow_is_nine_non_degenerate_vertices`,
`every_cone_head_triangle_is_outward_and_reaches_the_apex`,
`shaft_quads_keep_the_shader_vertex_order`,
`renderer_budgets_match_the_new_families`,
`the_shader_declares_the_new_family_budgets`.

**Limitations.** The mirror deliberately does not apply the per-instance basis
rotation or projection; it checks local geometry only. These are test seams,
not a stable library API.

#### `src/reference.rs` — domain-matched grid, cube, axes

**Purpose.** App-side decoration: a 3D face grid and wireframe domain box or a
flat 2D grid, with coloured axes.

**Public API.** `REFERENCE_DIVISIONS = 10`; `EDGE_COLOR`, `GRID_COLOR`,
`AXIS_X_COLOR`, `AXIS_Y_COLOR`, `AXIS_Z_COLOR`; `GRID_WIDTH`, `EDGE_WIDTH`,
`AXIS_WIDTH`; `reference_lines(domain, sampling_3d)`; `reference_lines_with`
and `reference_plane_lines_with` take a validated
[`ViewportStyle`](../src/viewport_style.rs) (issue #82 slice A), while the
legacy entry points delegate to the default style and stay byte-identical.
`scaled_viewport_style` applies the Reference layer's width and opacity over a
`ResolvedLayerStyle` (issue #84 A2a), leaving background and RGB untouched.

**Invariants.**
- 3D draws the far-face grids first, then the 12 box edges, then the coloured
  axes through the centre; the order prevents thin grid lines disappearing
  under coincident edges.
- 2D draws one grid on the domain's mid-z plane (the same plane the single z
  slice samples) plus X/Y axes; no z axis.
- `show_grid` independently gates the 2D lattice or 3D face grids and wireframe
  box; `show_coordinate_system` gates only the coloured 2D/3D axes. Both flags
  default on, and neither filters field geometry.
- Every line stays inside the domain bounds.
- Reference geometry is never counted as field geometry and is not part of the
  viewmodel snapshot.
- Issue #208 keeps every axis line and colour in world coordinates. The display
  orientation is a camera property, so only the projected screen direction
  changes; labels/colours stay x/y/z and 2D is unaffected.

**Pinned by.** `tests/reference_lines_test.rs` (13 tests):
`three_d_reference_has_twelve_box_edges_subdivided_faces_and_three_axes`,
`three_d_reference_stays_inside_the_domain_and_draws_every_bound`,
`face_grid_is_drawn_before_the_box_edges_and_axes_last`,
`two_d_reference_is_one_flat_grid_on_the_mid_z_plane`,
`two_d_reference_has_coloured_x_and_y_axes_through_the_centre`,
`styled_axes_follow_world_plane`,
`default_style_is_byte_identical_to_legacy`.

**Limitations.** It is decoration only: no labels, ticks, or numeric axis
annotation, and no plane-mode dimming.

### 3.6 Rendering

#### `src/object_scene.rs` — primitive meshes and source mapping for object instances

**Purpose.** Pure, GPU-free geometry for the depth-enabled object renderer. Five
object bodies are generated once per process and reused by every instance:
`MeshPrimitive::{Sphere, Cylinder, Torus, Tube, Disc}`. `MeshScene::from_sources`
maps the enabled, displayed source entries to instances with a slice clip and a
content fingerprint. The module has no renderer, app, wgpu or persistence-module
dependency; the offscreen pass lives in `src/object_renderer.rs`, and the one
storage type here is `ObjectStyle`.

**Public API.** `MeshPrimitive` with `ALL`, `triangles()`, `vertices()`,
`index()` and `from_index(u32) -> Option<MeshPrimitive>`; `MeshVertex {
position: [f32; 3], normal: [f32; 3], barycentric: [f32; 3] }` (`repr(C)`);
`PrimitiveMesh { primitive, vertices: &'static [MeshVertex] }`;
`primitive_mesh(MeshPrimitive) -> &'static PrimitiveMesh`; the budget
constants `STATIC_TRIANGLES = 2_832`, `STATIC_VERTICES = 8_496`,
`VERTEX_BYTES = 36`, `MAX_OBJECTS = 64` and `MAX_DRAWN_TRIANGLES = 150_000`;
`MeshObject { primitive, transform: [[f32; 4]; 4], color: [f32; 3], opacity,
sign: i8 }`; `MeshScene` with `empty()`, `from_sources(&SourceScene, &Domain,
Option<&PlaneFrame>)`, `objects()`, `drawn_triangles()`, `generation()` and
`clip()`; `SliceClip { normal: [f32; 3], offset: f32 }`; `object_transform(
center: Vec3, axis: Vec3, scale: Vec3) -> [[f32; 4]; 4]`; and
`ObjectStyle { Solid, Wireframe }` with `Default = Solid` and
`is_wireframe()`. The style serializes as the stable unit-variant tokens
`Solid`/`Wireframe`; a missing field loads `Solid`, a custom enum decoder
repairs an unknown stored name to `Solid` with one warning, and an untagged
wrapper repairs a non-string value the same way, so neither corrupt form fails
the whole settings envelope.

**Invariants.** Every mesh is non-indexed, three vertices per triangle, with
exactly `[1,0,0]`, `[0,1,0]`, `[0,0,1]` barycentrics. Normals are outward unit
vectors; positions are finite. Triangle counts are pinned: Sphere 768 (24 × 16),
Cylinder 96 (24 sides plus two 24-triangle caps), Torus 768 (32 × 12), Tube 1,152
(12 turns × 8 sides × 6 steps × 2, helical body radius 1 and total height 1 on
`+Y`), Disc 48 (24 rim segments, two-sided fan). `STATIC_VERTICES × VERTEX_BYTES`
is 305,856 B of static geometry. Each variant is cached in its own
`OnceLock<PrimitiveMesh>`; there is no per-frame allocation and no `unsafe`.

**Mapping.** The domain extent is its largest `domain_size()` component with a
`1e-6` floor; `unit = 0.05 * extent`. Point charge → unit sphere; finite line
charge → cylinder (`0.02 * extent` radius, source length); charged ring and
current loop → torus with source major radius and a `0.02 * extent` **vertical**
minor radius, while the **radial** minor radius is `radius / 3`; charged disc →
disc (source radius; its `scale.y = 0.02 * extent` is a reserved value, not a
physical thickness, because the slice-1 disc is a zero-thickness two-sided fan
at local `Y = 0`); solid sphere → source-radius sphere; straight wire → cylinder
(`0.012 * extent` radius, `2.2 * extent` length); solenoid → tube (source radius
and length, 12 visible turns). Family colour comes from
`ELECTRIC_COLOR`/`MAGNETIC_COLOR`, opacity is `1.0`, and `sign` is `-1` only for
a finite negative charge/current (a NaN strength compares false and maps to
`+1`). `SourceDisplay` filters families; disabled entries emit nothing; entry
order is preserved, and either truncation drops the tail without reordering.

Torus geometry: the primitive has major `0.75` and minor `0.25`, and
`object_mapping` sets `scale = (radius / 0.75, 0.02 * extent / 0.25, radius /
0.75)`. So the major radius is exactly `radius`, the vertical minor radius is
exactly `0.02 * extent`, and the radial minor radius is
`(radius / 0.75) * 0.25 = radius / 3`. A single per-instance linear transform
cannot vary the tube's minor radius with direction: the minor cross-section
spans both the radial and vertical axes of the same mesh, and a diagonal scale
stretches it differently along each. The consequence is that a ring reads
proportionally thicker radially as its major radius grows, while its vertical
span stays fixed. A dedicated thin-ring primitive with proportional wall
thickness is deferred to a later renderer slice (follow-up candidate).

Transforms are column-major: the first three columns are the scaled right,
source-axis and forward basis vectors, and the last column is the centre. Local
`+Y` follows the source axis; a zero or non-finite axis falls back to `Z`, and
non-finite centres or degenerate scales are skipped or clamped so every emitted
transform is finite with a positive determinant. `clip()` is the frame normal
and `normal.dot(origin)`; the normal is taken as unit because
`PlaneFrame::new` builds it from unit in-plane axes, so consumers must not
assume that for hand-built or foreign frames. An empty mapped scene with a
plane has `generation == 0` but `clip: Some(_)`. The FNV-1a `generation` covers
each instance's primitive index, sign, opacity/colour bits and all 16 transform
bit patterns; `empty()` reserves generation `0`. It does **not** hash `clip()`:
the 2D and 3D mapping of the same sources share one generation even though only
2D carries a `SliceClip`, and another plane or index changes the clip without
changing the generation. The cache key is therefore `generation` **plus**
`clip`: `MeshScene::cache_matches` compares both, and the live viewport cache in
`src/app/viewport.rs` replaces its `Arc<MeshScene>` when either differs. A
2D → 3D switch drops the clip and a 3D → 2D switch adds it (issue #207).

**Pinned by.** [tests/object_scene_test.rs](../tests/object_scene_test.rs):
`primitive_triangle_counts_match_the_budget`,
`vertex_layout_is_thirty_six_bytes_and_aligns_to_four` (pins
`size_of::<MeshVertex>() == VERTEX_BYTES`),
`every_primitive_mesh_has_finite_vertices_and_unit_normals`,
`barycentric_corners_are_one_hot_per_triangle`,
`every_triangle_winds_ccw_from_outside_and_is_non_degenerate` (CCW-from-outside
winding and non-degenerate cross products), `generated_geometry_matches_static_budget`
(generated vertex/triangle totals equal the constants),
`sphere_vertices_lie_on_the_unit_sphere`,
`cylinder_spans_unit_height_and_has_caps`, `disc_is_two_sided_and_flat`,
`tube_has_twelve_visible_turns` (unit height and 12-turn azimuthal span),
`every_source_kind_maps_to_its_primitive_sign_colour_and_centre`,
`non_axis_aligned_basis_is_orthonormal_with_a_positive_determinant`,
`zero_or_non_finite_axis_falls_back_to_z_with_a_finite_transform`,
`transform_stays_finite_for_non_finite_center_and_scale`,
`unit_and_extent_scaling_follow_the_mapping_table`,
`nan_radius_still_yields_a_finite_transform`,
`non_finite_centres_are_skipped_without_panicking`,
`clip_matches_the_plane_frame_normal_and_offset`,
`both_display_filters_by_family`, `disabled_entries_emit_nothing`,
`object_order_is_the_scene_entry_order`, `sixty_four_solenoids_fit_both_budgets`,
`truncation_keeps_the_first_sixty_four_entries_and_drops_the_sixty_fifth`,
`triangle_budget_cannot_bind_before_the_object_budget` (documents that the
triangle-budget break is unreachable until a primitive exceeds 2,343 triangles
at 64 instances; the current largest is 1,152, so 64 × 1,152 = 73,728 <
150,000 and `MAX_OBJECTS` always binds first),
`nan_source_strength_maps_to_positive_sign`,
`generation_is_stable_and_changes_with_mapped_input`,
`cache_matches_compares_the_slice_clip_as_well_as_the_generation` (a 2D clip,
no clip and another plane all fail to match despite equal generations), and
`empty_scene_has_no_objects_or_triangles_and_a_distinct_generation`.

**Limitations.** Fixed tessellation, no LOD, no picking metadata and no
per-triangle colour. Solenoid turns are capped at 12 for honesty; the source's
numeric `turns` remains authoritative in panel and export text. Ring and
current-loop bodies have a fixed `0.02 * extent` vertical minor radius and a
`radius / 3` radial minor radius, so they read proportionally thicker radially
at large source radius; finer tube control needs a dedicated thin-ring
primitive (deferred). The disc's Y scale is reserved, not a thickness. The
triangle-budget branch of `from_sources` is unreachable with the current
primitive set, because `MAX_OBJECTS × largest primitive` is below
`MAX_DRAWN_TRIANGLES`.

#### `src/object_renderer.rs` — offscreen object pass and composite

**Purpose / API.** The depth-enabled GPU pass for the mapped bodies from
`src/object_scene.rs`, driven by `src/renderer.rs` before every vector draw in
both the live callback and `submit_offscreen`, so the viewport, screenshots and
animation frames share one object image. `ObjectDrawState { pass: ObjectPass }`
is the callback-resource owner shape. `ObjectPass::new(&Device, TextureFormat)`
builds the pipelines, the composite sampler and the uniform buffer; the
colour/depth target, instance buffer and primitive buffers stay unallocated until
a drawable frame arrives.
`prepare(&Device, &Queue, &mut CommandEncoder, &MeshScene, &ObjectView)`
allocates or reuses the target, uploads changed instances/uniforms and records
the geometry pass into the caller's encoder (it never submits).
`paint(&mut RenderPass, egui::Rect)` composites the prepared target as a
fullscreen textured quad over the pass's current viewport; it deliberately does
not call `set_viewport`, because egui_wgpu already sets the pass viewport to the
callback rect and restores the full surface before the next egui primitive.
`texture_view() -> Option<&TextureView>` exposes the colour target.
`OBJECT_PROBE_ENV = "VFE_OBJECT_GPU_PROBE"` gates the native device probe.
`ObjectView { view_proj: [[f32; 4]; 4], eye: [f32; 3], viewport: [u32; 2],
wireframe: bool, clear: [f64; 4] }` is the frame input; `target_extent()` maps a
zero axis to `None`. `ObjectGlobals` (128 bytes) is the uniform ABI
(`view_proj`, `eye`, `clip_plane`, `light_dir`, `params`), `ObjectInstance`
(128 bytes) is one per-instance vertex record (model matrix, inverse-transpose
normal matrix, linear colour), and `ObjectDrawPlan`/`object_draw_plan` are the
device-free prepare decision: `None` for an empty scene or a zero-area viewport,
otherwise the extent plus the sanitized draw list. `ObjectTargetKey` identifies
target extent/format within one pass object.

**Pipeline and shader.** `src/shaders.object.wgsl` holds both the geometry pair
(`vs_object`/`fs_object`), the attenuated discarded-side entry
(`fs_object_attenuated`) and the composite pair (`vs_composite`/`fs_composite`).
Geometry reads a 36-byte `MeshVertex` vertex stream plus the 128-byte instance
stream (bind group 0 is the `ObjectGlobals` uniform only; instances are vertex
attributes at locations 3–10), tests and writes `Depth32Float` with `LessEqual`,
never culls (two-sided lighting flips the normal on back faces), and blends
straight alpha. The fragment stage combines `0.25` ambient with one directional
diffuse term, discards the world-space half `dot(p, n) - offset < 0` when the
slice clip is active, and has a barycentric `fwidth` wireframe path selected by
`params.x`. When the slice clip is active the same instances are drawn once more
with the attenuated pipeline: `fs_object_attenuated` keeps only the discarded
half, applies the same two-sided lighting, multiplies alpha by `0.35` and never
writes depth, so a camera on the far side sees a shaded body instead of the dark
interior or an empty cut, while the kept half stays authoritative. In Wireframe
style the attenuated pass keeps the barycentric edges-only reading; in Solid it
fills the discarded side as a shaded body. It adds no vertices, buffers or
geometry. Composite binds the target texture and
sampler at bindings 2/3 (no binding collision with the uniform), draws an
oversized triangle and writes premultiplied alpha into egui's no-depth pass.

**Invariants.** Colour is `RENDER_ATTACHMENT | TEXTURE_BINDING`, depth is
`Depth32Float` with `RENDER_ATTACHMENT` and clears to `1.0`. An empty scene or a
zero-area viewport returns `Ok` without allocating a target, recording a pass or
uploading geometry. The target is recreated only when its extent or format
changes; instances are uploaded only when the scene generation or drawable count
changes, and the uniform only when the packed values differ. Every uniform and
instance value is finite: a non-finite view matrix becomes the identity,
non-finite eye components `0`, an unusable clip plane disables the discard, and
non-finite or singular object transforms/colours are skipped instead of uploaded
as NaN. Draw order is scene-entry order with the same `MeshPrimitive::vertices()`
count per draw, so identical scene + view produces identical draw calls and
target contents. No `unsafe`.

**Shared-pass contract (carried from the #112 review).** `paint` never changes
the render-pass viewport, so a callback cannot leave every later egui draw
squeezed into the object rect. egui_wgpu sets the viewport to the callback rect
immediately before `paint` and resets it to the full surface before the next
primitive; wgpu exposes no viewport getter, so this host sequencing cannot be
observed from a unit test. It is recorded here and in the module docs, and the
full-target composite remains pinned by the device probe.

**Alpha note.** The colour target accumulates bodies with straight-alpha
blending; the composite premultiplies once. That is exact while every body's
`opacity == 1.0` (the current mapping). A future transparent-object slice must
premultiply per object instead.

**Pinned by.** `tests/object_gpu_test.rs` always runs the device-free contracts
(empty scene and zero-area viewport plan nothing, the sphere plan is
deterministic, the view/wireframe/clip values round-trip into `ObjectGlobals`,
non-finite input is sanitized, and non-finite/singular instances are skipped),
pins composed Solid/Wireframe line sets to sign markers for object-backed glyphs
in both Sources and Induction modes (including the Induction layer override),
the legacy full glyph for the empty-object fallback, and the app SVG blueprint's
full bodies in both object styles, and prints `GPU not requested`. `scripts/test-object-gpu.sh` sets
`VFE_OBJECT_GPU_PROBE=1` on its child process only and drives the real pass on a
native adapter through `native_object_device_worker`: empty scene and zero-area
viewport return `Ok` with no target, a 64×64 sphere frame allocates and renders
(`2316/4096` non-clear pixels observed on the AMD RX 9070 XT / RADV probe), a
repeated frame is byte-identical, the capture-style stage proves a default
capture equals an explicit Solid capture while a Wireframe capture of the same
scene differs in pixels (#115), a sparse clipped disc proves the discarded slice
half reads as a shaded attenuated body from both `±Z` eyes (the full far-side
silhouette is filled and
differs from the opaque body, while the kept half stays byte-identical; a
wireframe-only pass leaves interior pixels empty), an unclipped sphere covers the
same area from all six axis eyes, and the stale-clip regression drives the
`MeshScene::cache_matches` rule through 2D → 3D (unclipped full body from both
`±Z` eyes) and 3D → 2D (the clipped scene), with equal generations (#207). No
validation/internal/out-of-memory error is reported. There is no noop backend or
silent skip.

#### `src/renderer.rs` — instanced wgpu renderer

**Purpose.** GPU resources, instance layouts, draw-plan decisions, painter
sorting, and the offscreen screenshot pass.

**Public API.**
- Instance types: `ArrowInstanceGPU { origin, dir_len, color }` (48 bytes,
  `repr(C)`, bytemuck Pod/Zeroable), `LineInstanceGPU { a, b, color }` (48
  bytes).
- `Globals` (224 bytes): `view_proj`, `inv_view_proj`, `camera_pos`,
  `light_dir`, `camera_right`, `camera_up`, `arrow_style: [u32;4]`,
  `arrow_params: [f32;4]`.
- Constants: `ARROW_VERTS_FLAT = 9`, `ARROW_VERTS_LINE = 6`,
  `ARROW2D_VERTS = 9`, `ARROW2D_VERTS_LINE = 6`, `MAX_ARROW_VERTS = 186`
  (`12 * MAX_SIDES - 6`), `ARROW_KIND = 1.0`, `PARTICLE_KIND = 0.0`,
  `DOT_VERTS_PER_INSTANCE = 6`; blend states.
- `ArrowDrawPlan { Flat2D { vertices }, Combined3D { budget } }`, `DrawPass {
  Lines, Trails, Scene3D, Arrow2D, Dots }`; `SceneGPU` and its methods
  (`is_empty`, `arrow_draw_plan`, `arrow_vertex_count`, `style_vertex_count`,
  `uses_flat_arrow`, `draw_passes`); `scene_vertex_count`,
  `combined_instances`, `camera_basis_from_view_proj`.
- Object compositing: `DrawStage { Objects, Vector(DrawPass) }`,
  `paint_stages(&SceneGPU, object_count) -> Vec<DrawStage>`,
  `ObjectSceneState` (`new`, `from_scene`, `set`, `clear`, `get`, `len`,
  `is_empty`, `has_objects`), and `object_viewport_pixels(egui::Rect, f32)`.
- `DrawState` (pipelines, buffers, bind groups; `new`, `set_object_scene`,
  `object_scene`, `set_object_style`, `object_style`, `render_offscreen`,
  `render_offscreen_with_objects`, `render_offscreen_async`,
  `render_offscreen_async_with_objects`),
  `DrawCallback { scene, objects, object_style, screen, object_viewport,
  object_clear, view_proj, camera_pos }`, and the sort helpers
  (`sort_scene_back_to_front`, `sort_lines_back_to_front`,
  `sorted_lines_back_to_front`, `sorted_arrows_back_to_front`,
  `sort_back_to_front`) plus the pure depth-key helpers
  (`scene_view_depths`, `line_view_depths`) they consume. `object_style` comes
  from the live state every frame;
  `prepare` maps it to `ObjectView::wireframe`.
- Native `OffscreenError::{Setup, Readback}` and `ReadbackError::{Map, Poll,
  CallbackLost, Range, Decode}` retain stage and typed causes. The doc-hidden
  production `readback_map_status` classifies the actual map callback. Native
  GUI capture stringifies only at `Screenshot::fail`; wasm async capture keeps
  its existing `String` result.

**Pipeline contract.**
- One bind-group layout: binding 0 = `Globals` uniform (vertex+fragment),
  binding 1 = read-only storage array of instances (vertex). No vertex buffers.
- Four pipelines, all `TriangleList`, `cull_mode: None`, no depth-stencil, one
  sample:
  | Pipeline | Shader entry points | Blend | Used for |
  | --- | --- | --- | --- |
  | `scene_pipeline` | `shaders.arrow.wgsl` `vs_scene`/`fs_scene` | alpha | 3D combined arrows + dots |
  | `arrow2d_pipeline` | `shaders.arrow2d.wgsl` `vs_arrow2d`/`fs_arrow2d` | alpha | 2D flat/line arrows |
  | `line_pipeline` | `shaders.line.wgsl` `vs_line`/`fs_line` | alpha | backdrop, streamlines, trails |
  | `dot_pipeline` | `shaders.dot.wgsl` `vs_dot`/`fs_dot` | alpha | 2D dots |
- Every pipeline writes to the surface's `target_format`; the offscreen texture
  uses the same format so the capture matches the screen.
- `upload` writes `Globals` every frame; it rebuilds only non-empty storage
  buffers. Empty collections keep the one-instance placeholder bindings, because
  binding a zero-size storage buffer is invalid.
- `record` follows `paint_stages`: `Objects` (only when the mapped scene has a
  body) → Lines → Trails → Scene3D (3D) or Arrow2D → Dots (2D). The object image
  is composited first; there is no depth attachment for the vector passes and
  ordering is still the CPU sort. An empty object scene adds no stage, allocates
  no target and leaves the vector order unchanged. `paint_stages` returns one
  small bounded `Vec` per paint (at most seven stages, no per-instance growth);
  that allocation is accepted rather than restructuring the pipeline around a
  custom iterator.
- `DrawCallback::prepare` uploads the vector scene, installs the object scene on
  `DrawState`, maps the live `object_style` to `ObjectView::wireframe`, then
  runs `ObjectPass::prepare` into the egui encoder; `paint`
  composites the object target before `record_draw`. `submit_offscreen` runs the
  same prepare and painting into the capture texture, drawing objects with the
  capture `DrawState`'s object style (set by every capture call site to the
  committed style; the default is Solid, the pre-style behaviour), with
  `render_offscreen_with_objects` (native and wasm) forwarding the frame's
  object scene. The live cache key is the scene generation plus the slice clip,
  so a 2D ↔ 3D switch replaces a scene whose objects are unchanged; the object
  renderer uploads instances only when the generation or drawable count changes,
  and an unchanged frame touches no object buffer.

**Invariants.**
- 3D uses one combined draw for arrows and particles, at the fixed 186-vertex
  budget. The shader derives `instance = vertex_index / 186`, reads
  `origin.w`, and exits unused invocations to one clip-space point before ring
  math.
- 2D uses the independent 2D family's 9/6 vertices and never reads `combined`.
- Empty scenes draw nothing; `SceneGPU::default()` is 3D with the cone family.
- Sort is stable far-to-near; a non-finite view depth becomes `NEG_INFINITY`
  (far group) instead of painting in front or panicking. Each depth (trail
  midpoint or arrow origin) is transformed once by `line_view_depths` /
  `scene_view_depths`, then a stable index permutation reorders the instances
  (decorate–sort–undecorate, #245); no view multiply runs inside a comparator.
  The observable order is unchanged, including `-0.0`/`0.0` ties.
- `arrow_style.x` is mode-dependent: the 3D family id while 3D paints, the 2D
  family id while 2D paints.
- `sides` is sanitized and re-clamped to 4..=16 before the uniform.

**Pinned by.** `tests/arrow_alpha_test.rs`:
`arrow_pipeline_blends_straight_alpha`,
`combined_instances_tags_arrows_and_particles_and_keeps_their_fields`,
`scene_sort_interleaves_arrows_and_particles_by_depth`,
`scene_sort_keeps_input_order_for_equal_depth`,
`non_finite_depth_sorts_behind_without_panicking`;
`tests/line_alpha_test.rs`: `line_pipeline_blends_straight_alpha`,
`dot_pipeline_blends_straight_alpha`,
`trails_draw_between_lines_and_the_combined_scene`,
`trails_sort_far_to_near_by_their_midpoint`, `two_d_never_draws_trails`;
`tests/renderer_sort_test.rs` (#245):
`lines_sort_byte_identically_to_the_legacy_comparator`,
`arrows_sort_byte_identically_to_the_legacy_comparator`,
`equal_depths_keep_chronological_order`,
`line_view_depths_evaluates_one_transform_per_element`,
`scene_view_depths_evaluates_one_transform_per_element`;
`tests/arrow_style_test.rs`: `three_d_draw_passes_are_lines_then_one_combined_scene`,
`two_d_draw_passes_keep_arrows_then_dots_and_ignore_combined`,
`scene_vertex_count_multiplies_instances_by_the_budget`,
`combined_scene_shader_guards_the_budget_and_early_out`;
`tests/dot_billboard_test.rs`: `globals_uniform_layout_carries_the_camera_basis_and_arrow_uniforms`,
`camera_basis_matches_the_orbit_camera`, `camera_basis_of_the_2d_front_view_is_screen_aligned`.

**Limitations.** No depth buffer, MSAA, or per-pixel lighting beyond
half-Lambert; all 3D geometry is one draw with a fixed vertex budget, so every
instance pays the widest-arrow invocation count. Storage buffers are recreated
on change rather than persistently mapped. The GPU path is not covered by
`cargo test`; it is covered by the smoke run.

#### `src/render.rs` — legacy 2D camera record

**Purpose.** A tiny 2D camera type (`position`, `scale`), retained as the
camera-preset payload and the 2D mirror; the live painting camera is
`camera::OrbitCamera`.

**Public API.** `Camera { position: [f32; 2], scale: f32 }` with
`world_to_ndc`, `zoom`, `pan`, `Default`.

**Invariants.**
- `scale` is positive; `zoom` floors it at 0.01.
- `world_to_ndc` is a simple centred scale, assuming a 2-unit-wide viewport
  after scaling.
- Presets store only this record, so camera presets are 2D views (target x/y +
  zoom); 3D orbit orientation/distance are not part of a preset.

**Pinned by.** `tests/camera_zoom_pan_test.rs`
(`tests::test_zoom_multiplicative`, `tests::test_pan_adds_position`),
`tests/ui_preset_test.rs` (preset handoff), `tests/preset_test.rs` (JSON shape).

**Limitations.** Not used by the viewport for painting; `world_to_ndc` is not
the pipeline's projection. Do not build new rendering on it.

#### `src/render/instancer.rs` — unused helper (not compiled)

**Purpose.** A generic vec4 instance-buffer helper left from an earlier
renderer.

**Status.** `src/render.rs` does **not** declare `mod instancer;`, so this file
is not part of the crate and is not compiled. The live renderer uses
`DrawState` and wgpu `DeviceExt` directly.

**Public API if it were compiled.** `Instancer { buffer, count }`, `new`,
`update`.

**Invariant/limitation.** `update` assumes four floats per instance and does not
resize the buffer; it cannot handle more data than allocated. Treat it as dead
code unless a future change adds the module declaration and tests.

#### `src/shaders.arrow.wgsl` — combined 3D arrow + dot shader

**Purpose.** Generate Cone/Cylinder/Flat/Line arrow geometry or a particle
billboard from `vertex_index`, light the arrows, and draw the round particle
disc.

**Contract.**
- Binds `Globals` at group 0/binding 0 and `array<ArrowInstance>` at binding 1.
- `MAX_ARROW_VERTS = 186`; `inst_idx = vertex_index / 186`; unused invocations
  exit at clip-space `(0,0,2,1)`.
- `origin.w > 0.5` selects an arrow; otherwise the six-vertex billboard uses
  `camera_right`/`camera_up` and `dir_len.w` radius.
- Style branch: `arrow_style.x` 0 Cone, 1 Cylinder, 2 Flat, 3 Line.
- Cone/Cylinder use `arrow_style.y` sides (min 4), shaft/head proportions from
  `arrow_params`; Cone tapers its shaft to 0.6 at the junction and has a full
  underside plate; Cylinder is headless with a far-end cap.
- Flat/Line are camera-facing ribbons built in world space with a
  camera-right fallback when the view is parallel to the arrow.
- Half-Lambert lighting from `light_dir`; Flat/Line deliberately unlit.
- Fragment: particle disc with one-pixel alpha rim and spherical normal; arrow
  fragments use `0.35 + 0.65 * light`.

**Pinned by.** `tests/arrow_style_test.rs` (`the_3d_shader_branches_on_all_four_styles`,
`the_3d_shader_derives_shaft_normals_from_the_taper`,
`the_3d_line_is_a_headless_camera_facing_ribbon`,
`combined_scene_shader_guards_the_budget_and_early_out`,
`every_shader_globals_struct_carries_the_arrow_uniforms`),
`tests/arrow_geometry_test.rs`, `tests/dot_billboard_test.rs`.

#### `src/shaders.arrow2d.wgsl` — flat 2D arrow shader

**Purpose.** Build a flat arrow in world XY, projected onto the view plane.

**Contract.**
- Same bind-group shape; `arrow_style.x` is the 2D id (Flat 0, Line 1).
- `inst_idx = vertex_index / count`, where count is 9 (Flat) or 6 (Line).
- The sampled direction is projected to XY; a zero XY direction falls back to
  +Y so a depth-only arrow still has a silhouette.
- Flat draws a shaft quad plus head triangle; Line draws one headless quad over
  the full length.
- Fragment returns the instance colour unchanged (flat, unlit) so the colormap
  is directly comparable.

**Pinned by.** `tests/arrow_style_test.rs`
(`the_2d_shader_branches_on_its_own_style_set`,
`regression_2d_draw_plan_reads_only_the_2d_style`),
`tests/arrow_geometry_test.rs` (`the_flat_2d_arrow_is_nine_non_degenerate_vertices`).

#### `src/shaders.line.wgsl` — line ribbon shader

**Purpose.** One camera-facing ribbon quad per line segment, unlit.

**Contract.**
- Six vertices per instance; `a.xyz` start, `b.xyz` end, `b.w` width.
- Expands by `half_w` along `cross(direction, to_camera)`; a zero-length segment
  collapses to a point rather than producing NaN.
- Fragment returns the colour unchanged.

**Pinned by.** `tests/line_alpha_test.rs::line_pipeline_blends_straight_alpha`,
`tests/reference_lines_test.rs`, `tests/viewmodel_test.rs` (streamline
instances).

#### `src/shaders.dot.wgsl` — round particle dot shader

**Purpose.** Camera-facing billboard with a round, lit disc (2D dot path).

**Contract.**
- Six vertices per instance; expands along `camera_right`/`camera_up` by
  `dir_len.w`.
- Fragment discards outside the unit circle, fades a one-pixel rim, computes a
  spherical normal from the quad UV, applies the same half-Lambert falloff, and
  multiplies alpha by coverage.
- `DOT_QUAD_CORNERS` in `particles.rs` mirrors the six corners.

**Pinned by.** `tests/line_alpha_test.rs::dot_pipeline_blends_straight_alpha`,
`tests/dot_billboard_test.rs` (quad corners, inscribed circle, camera basis,
radius), `tests/particles_test.rs` (dot colours/alpha).

### 3.7 Export and capture

#### `src/export_provenance.rs` — source, induction, ODE, Materials and Maxwell export warning contract

**Purpose.** One capture-time provenance object feeds the baked PNG banner,
PNG `tEXt` chunks, the visible SVG warning and the SVG `<metadata>` block. It is
derived from the validated `SourceScene` plus cached `SourceSampling`, from the
durable `InductionScene` plus shared softening and cached
`InductionSampling`, from the durable `OdeScene` plus the already-computed
`OdeSolution` and shared clock, from the durable `MaterialScene` plus cached
`MaterialSampling`, or from the durable `MaxwellScene` plus cached
`MaxwellSampling`; it never samples the field, re-integrates the ODE scene,
solves a material or Maxwell boundary-value problem, or creates vector data.

**Public API.** `ExportProvenance { warning_lines, metadata, source_mode,
induction_mode, ode_mode, material_mode, maxwell_mode }`,
`source_export_provenance(mode, scene, sampling)`,
`induction_export_provenance(mode, scene, softening, sampling)`,
`ode_export_provenance(mode, scene, solution, error, timeline)`,
`material_export_provenance(mode, scene, sampling)`,
`maxwell_export_provenance(mode, scene, sampling)`,
`provenance_lines`, `provenance_metadata`, and the exact ASCII copy constants
`MODEL_LINE`, `ACCURACY_LINE`, `UNAVAILABLE_LINE`, `INVALID_LINE`,
`NO_SAMPLING_LINE`, `INDUCTION_MODEL_LINE`, `INDUCTION_NO_SAMPLING_LINE`,
`INDUCTION_INVALID_LINE`, `ODE_MODEL_LINE`, `ODE_BOUNDED_LINE`,
`ODE_UNAVAILABLE_LINE`, `ODE_INVALID_LINE`, `ODE_TIMELINE_LINE`,
`MATERIAL_MODEL_LINE`, `MATERIAL_NO_SAMPLING_LINE`, `MATERIAL_INVALID_LINE`,
`MAXWELL_MODEL_LINE`, `MAXWELL_NO_SAMPLING_LINE`, `MAXWELL_DISPLAY_UNITS_LINE`,
`MAXWELL_INVALID_LINE`.

**Invariants.**
- Formula mode returns `None` from all helpers; Sources/Induction/Ode always
  return `Some` for their own mode (`source_mode` / `induction_mode` /
  `ode_mode` respectively). `material_export_provenance` returns an empty
  provenance with `material_mode = false` for every other mode; the app router
  only calls it in Materials mode, so non-Materials exports gain no material
  metadata and no banner.
- Materials lines are exactly the bounded-model line, background constants,
  material constants, `Applied/response: {Total|Applied|Response}`, the applied
  field with `E ... V/m` or `B ... T`, the case assumptions, then the current
  sampling warning. Missing sampling adds `Materials sampling unavailable`; an
  invalid scene adds `Materials scene invalid: {error}`. None is suppressible.
- Maxwell mode returns `Some` with `maxwell_mode = true`; every other mode
  returns `None`. The first line is exactly
  `Maxwell - ideal canonical teaching cases - no general solver`, followed by
  `Conductor2D - infinite-cylinder BEM approximation` for that case. A missing
  sampling pass adds `Maxwell sampling unavailable`; an invalid scene adds
  `Maxwell scene invalid: {error}` and `vfe.validation_error`; `accuracy not met`
  is visible whenever the BEM pass misses the target; the retained
  unavailable/projection/interface warning is appended without splitting its
  internal punctuation; the last line is always
  `Normalized display amplitudes; no SI constant engine.`.
- Maxwell metadata is `vfe.model = maxwell-bounded-teaching`,
  `vfe.maxwell_case`, `vfe.maxwell_family`, `vfe.maxwell_units = display-only`,
  `vfe.maxwell_assumptions`, `vfe.maxwell_accuracy`, the omitted counts, the
  three BEM coarse/fine/relative-difference keys only when convergence exists,
  and `vfe.validation_error` only when present. Requesting an export never
  samples the grid and never runs `solve_bem`.
- Materials metadata is exactly `vfe.model = materials-bounded-analytic`,
  `vfe.material_case`, `vfe.material_family`, `vfe.background_eps_r`,
  `vfe.background_mu_r`, `vfe.material_eps_r`, `vfe.material_mu_r`,
  `vfe.material_view`, `vfe.material_units`, `vfe.material_interface_axis`,
  `vfe.material_interface_offset`, `vfe.material_applied`, and
  `vfe.material_warning` only when a warning is present. No field is evaluated
  and no non-finite arrow is exported.
- The softened-model line is always first. Missing sampling adds
  `Source sampling unavailable` or `Induction sampling unavailable`; a
  validation error adds the matching invalid line; accuracy/unavailable lines
  appear only when their counts are nonzero.
- ODE's first line is always
  `ODE - fixed-step RK4 teaching model - h = 0.005` (built from `ODE_STEP`),
  followed by the bounded-trajectory line. An available solution adds
  `timeline: duration = {duration}, step = {step}, t = {time}`; a missing
  solution adds `ODE solution unavailable`. A retained `ode_error` adds
  `ODE scene invalid: {error}` and `vfe.validation_error`.
- ODE metadata is `vfe.model = ode-fixed-step-rk4` plus `vfe.ode_source`
  (`equations`/`current_field`), `vfe.ode_dimension`, `vfe.ode_step` and
  `vfe.ode_duration`; no CAS text, no non-finite sample data and no fabricated
  trajectory geometry are exported. An export never re-solves or samples ODE.
- Induction's model line is
  `Induction - quasistatic axisymmetric softened model - a = {softening}` and
  its metadata is exactly `vfe.model = quasistatic-axisymmetric-faraday`,
  `vfe.induction_case`, `vfe.waveform`, `vfe.waveform_derivative`,
  `vfe.amplitude`, `vfe.frequency`, `vfe.softening`, `vfe.display`, the three
  count keys and `vfe.validation_error` only when present. Native CAS
  derivative text is never exported.
- A cached induction pass built at a different softening is treated as
  unavailable, so counts never pair with a different model `a`.
- An opt-in changes the drawn count only; it never removes a warning
  or relaxes a cap. Non-finite sampled values never enter warnings/metadata.
- Source metadata keys are `vfe.source_model`, `vfe.softening`, `vfe.display`,
  `vfe.accuracy_not_met_total`, `vfe.accuracy_not_met_drawn`,
  `vfe.unavailable`, and `vfe.validation_error` when present.

**Pinned by.** `tests/source_export_test.rs`
(`formula_exports_have_no_source_provenance`,
`source_warning_lines_and_metadata_are_exact_and_separate`,
`opt_in_adds_visible_data_but_never_removes_the_warning`,
`missing_or_invalid_sampling_keeps_the_warning`) and
`tests/induction_export_test.rs` (exact induction lines/metadata, missing and
invalid sampling, opt-in/caps, decoded PNG banner/tEXt bytes and visible SVG
text/metadata), `tests/ode_export_test.rs` (request-time Ode provenance,
decoded PNG banner/tEXt, visible SVG text/metadata, unavailable-solution
warning with no fabricated geometry, and the 2D-only SVG rejection) and
`tests/material_export_test.rs` (exact Materials lines/metadata, decoded PNG
banner/tEXt, visible SVG text/metadata, 2D-only rule, non-Materials isolation,
unsuppressed response/interface warnings and finite geometry), and
`tests/maxwell_export_test.rs` (exact Maxwell/BEM lines and metadata, decoded
PNG banner/tEXt, visible SVG text/metadata, 2D-only rule, unavailable/invalid
warnings with no arrows, non-Maxwell isolation, and request-time routing that
never solves again).

#### `src/export.rs` — PNG encoding and camera-preset files

**Purpose.** PNG encode/write, SVG placeholder, and camera-preset JSON
persistence (native file or browser `localStorage`).

**Public API.** `encode_png`, `encode_png_with_text`, `sanitize_png_text`,
`save_png` (native), `save_svg` (placeholder);
`Preset { name, position, scale }`, `presets_to_json`, `presets_from_json`,
`PRESETS_STORAGE_KEY`, `load_wasm_presets`, `save_wasm_presets`,
`storage_note`, and native `save_preset`, `delete_preset`, `load_presets`.

**Invariants.**
- `encode_png` rejects a frame whose length is not exactly `width * height * 4`
  and writes RGBA8.
- `encode_png_with_text` writes the same RGBA8 pixels through the `png` encoder
  and adds each metadata pair as a `tEXt` chunk; keywords and values are
  sanitized to printable ASCII (newlines become spaces, other non-ASCII `?`).
- Native preset files are `presets.json` under the chosen directory; save
  replaces by name, delete rewrites even when the list becomes empty.
- The wasm list is JSON under `vector-field-explorer.camera-presets`, separate
  from `eframe::APP_KEY`; corrupt JSON falls back to an empty list, and an
  unavailable `localStorage` returns a one-line panel note.
- `presets_from_json` returns `None` for corrupt/foreign shapes; the native
  loader returns an error instead (no silent overwrite).
- `save_svg` always returns `Unsupported`.

**Pinned by.** `tests/export_test.rs` (`encode_png_returns_a_decodable_rgba_image_without_a_file`,
`encode_png_rejects_a_frame_with_the_wrong_byte_count`,
`test_save_png_preserves_rgba_byte_order`, `test_save_preset_creates_missing_dir`,
`test_delete_preset_creates_missing_dir`), `tests/preset_test.rs`
(`preset_json_round_trips_the_shared_shape`,
`corrupt_preset_json_falls_back_to_none`), and `tests/ui_preset_test.rs`
(`test_presets_survive_restart`, `test_delete_removes_from_list_and_disk`).

**Limitations.** SVG export is not implemented. Presets carry only the 2D
camera record. No atomic file replace; a write failure surfaces as an error to
the panel/log.

#### `src/screenshot.rs` — capture contract, save path, toast

**Purpose.** The pure screenshot contract plus the native dialog and browser
download paths.

**Public API.** `MAX_CAPTURE_EDGE` (4096 native / 2048 wasm), `TOAST_TTL`,
`SaveOutcome`, `capture_size`, `padded_row_bytes`, `strip_row_padding`,
`png_readback_check`, `to_rgba`, `date_stamp`, `default_filename`,
`ensure_png_extension`, `spawn_save` (native; now takes the request-time
`Option<ExportProvenance>`), `finish_download` (wasm; same provenance),
`scene_to_svg`, `scene_to_svg_with_provenance`, `apply_export_banner`,
`Screenshot` (`new`, `with_channel`, `request`, `request_with_provenance`,
`request_svg`, `request_svg_with_provenance`, `take_request`,
`take_request_with_provenance`, `take_svg_request`,
`take_svg_request_with_provenance`, `svg_request_pending`,
`png_request_pending`, `pending_png_provenance`, `pending_svg_provenance`,
`sender`, `tick`, `fail`, `toast_text`, `toast`).
`VfApp::request_png_export`/`request_svg_export` capture
`field_export_provenance()` at request time; the viewport consumes it through
the `take_*_with_provenance` variants. A Source-, Induction-, Ode-, Materials-
or Maxwell-mode request made without provenance derives the fallback instead of
exporting silently. Scalar plot captures always pass `None`.

**Invariants.**
- Capture size converts logical points to physical pixels (`pixels_per_point`)
  and scales both axes by one factor to stay under the cap; a nonsense scale
  falls back to 1:1.
- Readback rows are padded to 256 bytes; `strip_row_padding` returns exactly
  `width * height * 4` bytes.
- `png_readback_check` accepts only RGBA8/BGRA8 (Unorm or sRGB); `to_rgba`
  swizzles BGRA back to RGBA. Screenshots are never written as BGRA.
- Default filename is `vector-field-explorer-YYYY-MM-DD-<mode>.png`; the
  extension is forced to `.png`.
- Native: dialog + write run on a worker thread and report through a channel;
  `tick` drains completions and expires the toast. A second request is allowed
  while the first dialog is open.
- Wasm: the readback maps through the browser event loop, encodes in the
  completion, starts a download, and reports; a second click is ignored while a
  capture is in flight.
- `toast` requests repaints while visible so it expires on time.
- `apply_export_banner` bakes the provenance warning into the RGBA frame with a
  dependency-free 5×7 bitmap font over a dark strip; a wrong frame length is an
  error and is reported as `SaveOutcome::Failed`. With provenance the capturer
  encodes through `encode_png_with_text`; without it the historical
  `encode_png`/`save_png` bytes are unchanged.
- `scene_to_svg` delegates to `scene_to_svg_with_provenance(..., None)`, so the
  no-provenance document is unchanged. The provenance variant adds an escaped
  `<metadata>` block and visible `<text>`, and converts line/trail widths from
  world units to pixels from the projected endpoint span (`0.5..=3.0`, with the
  historical `max(0.5)` fallback for a zero world span).
- Pending PNG/SVG requests carry the provenance captured at request time;
  `take_*_with_provenance` returns `Some(provenance)` for a live request and
  `None` when idle.
- Animation export and screenshot capture are mutually exclusive:
  `Screenshot::is_busy` blocks Start while a PNG/SVG capture is pending or in
  flight, and the app refuses PNG/SVG requests while an animation job exists.

**Pinned by.** `tests/screenshot_test.rs` (18 tests):
`capture_size_keeps_the_aspect_and_the_4096_cap`,
`capture_size_uses_physical_pixels_on_hidpi`,
`rows_are_padded_to_the_256_byte_copy_alignment`,
`strip_row_padding_returns_exactly_the_image_bytes`,
`screenshots_are_rgba_never_bgra`,
`the_early_format_gate_matches_the_converter`,
`default_filename_stamps_the_day_and_mode`, `png_extension_is_forced`,
`completion_lands_as_a_toast_and_expires`,
`a_click_asks_for_exactly_one_capture`,
`native_allows_a_second_capture_while_one_is_in_flight`.
Banner/stroke/SVG provenance behavior is pinned by `tests/source_export_test.rs`
(`png_banner_and_text_chunks_are_in_the_decoded_file`,
`svg_warning_is_visible_and_metadata_is_present`,
`svg_stroke_width_converts_world_units_to_pixels_and_clamps`,
`no_provenance_svg_keeps_existing_stroke_width_semantics`,
`screenshot_request_keeps_the_provenance_captured_at_request_time`).

**Limitations.** The GPU readback itself is only exercised by the runtime smoke
test. The browser cap is lower (2048) because the PNG encode runs on the main
thread.

### 3.8 UI modules

#### `src/ui/mod.rs` — UI module list

Declares `appearance_panel`, `branding`, `cas_preview` (compiled on both
targets; its tests are ungated in `tests/suite.rs`), native-only `cas_panel`
and `math_view`, `formula_bar`, `gauss_shell`, `help_overlay`, `induction_panel`,
`interaction_panel`, `layout`, `material_panel`, `maxwell_learning_panel`,
`maxwell_panel`, `menu`,
`ode_panel`, `panel_rails`, `plot_panel`, `presets`, `sidebar`, `sources_panel`,
`status_bar`, `style_panel`, `top_bar`, `transport`, `viewport_tool`.
**Important:** `src/ui/arrows.rs` is present on disk but is *not* declared here,
so it is not compiled. Do not add UI code to it expecting it to run.

#### `src/ui/viewport_tool.rs` — the viewport-edge click tool strip

**Purpose.** Issue #228 makes the next viewport click self-explanatory. The
strip at the viewport's top edge derives its effective tool from live state
(the left-click action plus the Streamline and Particle feature gates) and the
session-only arming choices in `Controls`, never from a second independent
flag. Inspect, Place probe, Seed streamline and Add particles are offered where
supported; a tool whose prerequisite is off becomes an explicit enable-and-place
entry, and unsupported tools carry a specific reason. Placement is one-shot or
sticky, the strip's buttons are ordinary focusable egui buttons (Tab plus
Enter/Space select them), and Escape returns to Inspect and cancels an armed
placement. The strip only surfaces existing S/P bindings; it never assigns or
repurposes a chord. `tests/viewport_tool_test.rs` covers the derivation table,
one-shot/sticky placement, enable-and-place, Escape, keyboard selection and both
layout widths.

#### `src/ui/appearance_panel.rs` — the Appearance panel

**Purpose.** Issue #84 slice A2b: edit the six per-layer and two per-family
`LayerStyles` overrides. Each row exposes bounded opacity (`0.0..=1.0`) and
width (`0.05..=4.0`) sliders with an explicit `Default` escape that clears the
field, plus family colour and a global `Reset all`. Issue #85 slice A2b adds a
`Profiles` section above the layers: one dropdown lists the built-ins first and
then the user profiles, with one `Apply` and one `Delete` button. Choosing an
entry only drafts a selection; `Apply` applies it, `Delete` is enabled only for
a user selection and still needs a two-click confirm. The section also saves
the current appearance under a name and imports/exports the user library as
portable text. The header notes that styling affects the live/raster view only;
SVG exports keep the unstyled blueprint, and preset labels say the seaborn and
editor-theme looks are teaching approximations, not byte-exact reproductions.
Issue #208 adds one `Viewport` section with a single `Up axis` ComboBox (Z/X/Y)
and an honest note that it changes only how the view is drawn; coordinates and
saved scenes are unchanged. The choice is display-only, never dirties the
document and is never written into a `ViewRecord` or appearance profile.

**Public API.** `AppearanceEdit` (`LayerOpacity`, `LayerWidth`, `FamilyOpacity`,
`FamilyWidth`, `FamilyColor`); `apply_edit`, `reset_all`; `AppearancePanelState`
with `show(ui, state, profiles, compact)`, `apply_profile`, an inline `error` and
a weak-coloured success `message`. Session drafts (`profile_name`, `import_text`,
`export_text`, `pending_delete`, `selected_profile`) are never persisted.

**Invariants.**
- Every edit copies the committed record, changes exactly one field and calls
  `State::commit_layer_styles`; validation is atomic, so a rejected direct write
  surfaces an error and leaves both the record and the dirty flag untouched.
- Slider values stay inside the model bounds, so normal UI edits cannot produce
  an invalid record.
- `reset_all` commits `LayerStyles::default()`; resetting this panel never
  touches particle or viewport controls.
- Applying a profile mutates `State` only through `AppearanceProfile::apply`
  and only for the dropdown's selected entry; selecting an entry never applies
  it. A selection that disappears (delete or an import replacing the user
  library) falls back to no selection without panicking. `Delete` is enabled
  only when the selection is a user profile (index >= the built-in count).
  Saving captures the live appearance and upserts it, rejecting empty and
  built-in names before the library is touched. A successful save, apply and
  import each clear any armed delete. Imports use the library's
  atomic merge, so a malformed, duplicate, invalid or over-cap document leaves
  the library unchanged, and the success message reports the number of new
  profiles added (a pure replacement says no new profiles were imported, while
  an empty document says nothing was imported).
  Errors share the inline error label; successes use the weak message field.
- The cached export text is recomputed from the library by one helper after
  every user-library mutation and when `Export text` is clicked. There are no
  revision counters; an empty user library yields an empty buffer, so `Copy`
  stays disabled.

**Pinned by.** `tests/appearance_panel_test.rs` (35): panel toggle/width
round trip and clamp, single compact sheet, compact Appearance tab opening,
layer/family commit and resolution, layer-over-family precedence and the
family-only fallback, default escape, inline error display/clearing, invalid
write rejection, reset, the rendered six-layer/two-family body, profile apply
and its rejected-record path, dropdown listing order, apply-only-the-selection,
selection never applying on its own and surviving frames, delete disabled for
built-ins and two-click delete for users with the vanished-selection fallback,
save/replace/built-in/empty-name handling, the armed-delete-cleared-by-save
case, the user-profile cap, atomic import with its added-profile delta and
armed-delete clearing, replacement-only import, empty import document, empty
export/Copy, and the compact profile section.

#### `src/ui/interaction_panel.rs` — the Interaction panel

**Purpose.** Issue #88 slice A2b: expose the pure A1 interaction model in the
right-side Interaction panel. `Bindings` lists one row per remappable action
with its current chord, a rebind capture mode, conflict Replace/Cancel and a
per-action reset; reserved `Dismiss`/`Undo`/`Redo` rows are read-only with
their fixed chords. `Camera feel` exposes the two bounded multipliers and three
invert flags with a reset. `Profiles` applies the built-in `Default` and user
profiles, saves the live bindings/feel under a typed name, deletes user
profiles after a two-click confirm, restores the default bindings/feel and
imports/exports the portable document. Every binding/feel edit commits straight
to `VfApp::interaction`, and profile edits mutate
`VfApp::interaction_profiles`. Slice A3 routes the live bindings and camera feel
through input: menu/transport/tool/view chords and orbit, wheel and fly-look.

**Public API.** `InteractionPanel` with `show(ui, live, profiles, compact)`,
`begin_capture`, `cancel_capture`, `apply_capture`, `resolve_conflict`,
`cancel_conflict`, `reset_action`, `commit_camera_feel`, `reset_camera_feel`,
`apply_profile`, `save_current`, `delete_profile`, `import_profiles`,
`sync_export`; `PendingConflict`; session drafts (`capturing`, `profile_name`,
`import_text`, `export_text`, `message`, `pending_delete`, `conflict`).

**Invariants.**
- A captured non-Escape key becomes a `KeyChord` from the same egui `Key` type
  as A1; pure modifier keys are ignored and Escape cancels without an edit.
- Capture never survives the panel being hidden: `cancel_capture` drops the
  armed target and any pending conflict when the wide panel is collapsed to
  its rail or the compact sheet closes or switches away from Interaction, so a
  later key press after reopening cannot bind silently.
- The panel's own profile-name and import fields are identified by stable ids;
  while either owns keyboard focus, capture ignores key events so typing edits
  text instead of binding a chord.
- `Bindings::assign` owns conflict detection: an in-use chord changes nothing,
  names the owner and offers Replace (A1 `replace`, swapping both chords
  atomically) or Cancel. Reserved/browser-owned refusals leave one message.
- `Reset action` uses the A1 swap path with the documented default, so even a
  cross-action default collision cannot leave a duplicate.
- Camera-feel writes pass through `CameraFeel::sanitized`; out-of-range
  multipliers clamp and non-finite values return to the default.
- Applying a profile replaces bindings and feel only; the live record's name,
  scene, camera pose, simulation and layout are untouched.
- Saving rejects empty and built-in names, replaces a same-named user profile
  in place, and reports library validation/cap errors verbatim.
- Restore defaults installs `Bindings::default_bindings()` and
  `CameraFeel::default()` directly, so it also repairs a damaged live record.
- Import is atomic through `ProfileLibrary::import_text`; the message reports
  new profiles, an honest zero-delta replacement, or `Nothing to import` for an
  empty document. Export/copy stay disabled when the user library is empty.

**Pinned by.** `tests/interaction_panel_test.rs` (18): every remappable row and
default chord, capture/assign and Escape cancel, modifier capture, capture
cleared by collapsing the wide panel or switching the compact sheet away,
typing in a focused text field without binding, conflict Replace swap and
Cancel, reserved read-only rows, per-action reset, camera-feel
clamp/apply/reset, profile apply, save validation/replace-in-place, restore
defaults from a damaged record, import/export delta wording, the compact
Interaction tab and single-sheet switch, layout defaults/clamping and the
display-only menu shortcut.

#### `src/ui/cas_preview.rs` — bounded in-panel scalar result preview

**Purpose.** Issue #169: build a small CPU heatmap for a scalar CAS result and
cache its upload, without a graphics device, wgpu readback, or platform split.
The GPU plot and export paths stay untouched.

**Public API.** `PREVIEW_MAX_EDGE`, `PREVIEW_SPAN`, `PreviewPlane`
(`Xy`/`Xz`/`Yz`), `PreviewImage { width, height, rgba }`,
`scalar_heatmap(expr, plane, range, palette)`, `preview_key(expr, range,
palette)`, and `PreviewCache` with `sync(ctx, image, key)` and `texture()`.

**Invariants.** Both axes are capped at `PREVIEW_MAX_EDGE` (160) texels. The
expression is parsed through the existing `meval` scalar environment and binds
`x`, `y`, `z`, `t`; a non-finite sample is fully transparent and never a NaN
colour. Finite values map through `Colormap::from_unit` after clamping to the
caller-supplied range. `preview_key` hashes only expression, range, and palette.
`PreviewCache` uploads one `egui::TextureHandle` and ignores an unchanged key,
so the panel neither resamples nor allocates a texture per frame.

**Pinned by.** `tests/cas_preview_test.rs`:
`test_preview_heatmap_is_bounded_and_finite`,
`test_preview_matches_palette_endpoints`,
`test_preview_key_changes_with_expression_range_and_palette`,
`test_non_finite_samples_are_transparent`,
`test_cache_does_not_rebuild_for_an_unchanged_key`.

**Limitations.** Normalized teaching thumbnail only, not an SI-scaled plot; one
fixed XY plane and a fixed `[-1, 1]` palette window in the panel; no vector or
error preview, no export, persistence, or GPU resource.

#### `src/ui/cas_panel.rs` — native symbolic tools

Native-only independent panel for transient scalar/vector formula input,
operation preview, copy, and explicitly applying validated vector results. A
recognised app-control action verb from `cas_actions` takes precedence over the
mathematical intent: the panel shows `preview()` (or the precise parse
rejection) and an `Apply action` button, and returns the parsed action to
`VfApp::apply_cas_action`, which commits it once through the existing setters.
`VfApp::cas` owns scratch state; `app/panels.rs::show_left_panels` assembles its
rail/body independently of Controls, and Compact reuses the single sheet.
`tests/cas_rails_test.rs` exercises actual rail/menu/tab/Apply and resize paths.
Preview identity includes input, operation, variable, and the current app mode;
CAS scratch, action messages and rejections are not persisted. Issue #169 paints
a bounded `ui/cas_preview` heatmap under a scalar result, with the caption
`Normalized teaching preview — not SI`; vector results and structured errors
keep the text-only output. The image and texture are reused until the
expression, palette, or fixed range changes. Pinned by
`tests/cas_panel_test.rs::test_cas_panel_shows_a_preview_for_a_scalar_result`
and `test_cas_panel_hides_the_preview_for_a_vector_result`. Formula mode
keeps its historical authoritative 2D/3D app-mode dimension (not the field's
arity). Sources mode has no formula editor: `apply` refuses it before
dimension/vector checks, and `show` disables Apply with the exact explanation
`CAS edits the formula field; switch to Formula.` The action seam matches that
rule: a previewed `formula` or `scalar` action is shown in Sources mode but
`VfApp::apply_cas_action` refuses it on Apply with the same explanation and no
mutation. The preview dimension reads
`State::active_dimension()` in Sources mode without changing formula-mode
semantics. Issue #171 adds the `Typeset result` checkbox and a persisted
`ViewRecord.typeset_output` (serde default true). With it on, the preview is
painted through `ui/math_view`; if the printer returns `None`, `katex-native`
fails to lay the document out, or a display item cannot be painted exactly,
the panel falls back to the linear editor and the weak note
`Showing linear result`. Copy still copies the canonical linear formula, and
structured errors are shown verbatim, never typeset.

#### `src/ui/math_view.rs` — native typeset display-list adapter

**Purpose.** Issue #171: paint `katex-native` display lists with egui without a
browser or JavaScript engine.

**Public API.** `shared_context() -> &'static KatexContext`;
`install_math_fonts(ctx)` (idempotent startup registration);
`layout_math(ui, tex, context) -> Option<MathLayout>`;
`MathLayout::{is_empty, size, paint}`;
`paint_tex_or_linear(ui, tex, linear) -> bool`; `show_linear(ui, linear)`;
and the `#[doc(hidden)] math_layout_from_items` finiteness test seam.

**Invariants.** Total and non-panicking: parse/layout errors, a layout panic,
non-finite display-list coordinates, missing font fallback, and unsupported
display items all return `None`, so the caller keeps the linear result.
`KatexContext::default()` is built once in a `OnceLock`, never per frame. KaTeX
faces are registered as named egui families through `Context::add_font`, once
at native app startup (before the first frame) and idempotently per item. Only
text, rectangles, line segments, and convex paths wholly inside their clip are
emitted; radicals and other non-convex or clipped paths fall back to linear. A
layout wider or taller than the available space falls back to linear instead of
painting a clipped fragment. Dimensions use the body text size and the layout's
baseline/depth. `Layout.tag` and `equation_numbers` are ignored because the
settings never enable numbering and the printer never emits `\tag`.

**wasm evidence.** Both KaTeX crates live in
`[target.'cfg(not(target_arch = "wasm32"))'.dependencies]`; `cas_tex` and
`math_view` are native-only. On 2026-11-21,
`cargo tree --target wasm32-unknown-unknown -e normal -i katex-rs` and
`... -i katex-native` both printed `warning: nothing to print.` (exit 0), and
the full wasm tree contained zero `katex` lines (`grep -ci katex` exited 1).
`cargo check --target wasm32-unknown-unknown --lib` exited 0. Trunk is not
installed on this host, so no wasm artifact size is claimed.

**Pinned by.** `tests/math_view_test.rs` and the CAS panel typeset tests.

#### `src/ui/arrows.rs` — unused placeholder (not compiled)

Contains only `ArrowRenderer` with `new()`. No module declaration and no
references anywhere in `src/` or `tests/`. It is dead code; the real arrow
rendering lives in `renderer.rs` and `shaders.arrow*.wgsl`.

#### `src/ui/branding.rs` — cached project mark

**Purpose.** Decode the committed 256 px PNG once per egui context and expose it
as an image.

**Public API.** `#[doc(hidden)] pub fn logo_texture(ctx)` (test seam);
`pub(crate) fn logo_image(ctx, size)`.

**Invariants.**
- The texture is cached in egui context temp data under one id; repeated calls
  return the same handle.
- A decode failure logs and uses a 1×1 transparent image so the UI never breaks.
- The mark is the same committed asset used by packaging.

**Pinned by.** `tests/top_bar_test.rs::the_logo_texture_is_the_cached_256px_mark`,
`about_draws_the_mark_at_128_points`, `tests/branding_assets_test.rs`.

**Limitation.** Per-context cache; no SVG rendering.

#### `src/ui/plot_panel.rs` — scalar plot controls

**Purpose.** The Plot section shared by the wide Controls panel and the compact
sheet: explicit kind selector, example catalogue, curve samples/line width,
surface resolution (Advanced), shared colormap, height range, Fit plot and the
plot-section reset.

**Public API.** `PlotPanelAction::FitPlot`, `PLOT_KINDS`, `kind_label`,
`source_label`, `source_hint`, `source_help`, `active_example_name`, `show`.

**Invariants.**
- `show` edits `State` only through existing setters and returns only the Fit
  action; the app runs the Task 6 `fit_plot` seam with the viewport aspect.
- Kind choices are exactly Vector field / Curve `y=f(x)` / Surface `z=f(x,y)`;
  only Curve exposes samples and line width, only Surface exposes resolution.
- `Custom` is derived from the active draft text, never stored separately.
- Fit is disabled with a visible explanation when no finite geometry exists.
- The section reset restores plot settings/draft and preserves the vector
  formula; no new shortcut or global settings page is added.

**Pinned by.** `tests/plot_ui_test.rs`.

#### `src/ui/sources_panel.rs` — editable source scene

**Purpose.** The dedicated Sources rail/sheet: display selector (Electric /
Magnetic / Both with the E amber / B cyan legend), global object-style pair
(Solid / Wireframe) with its SVG-boundary help line, Add combo and button,
per-entry enable, kind, center/axis and kind parameters, Remove, Reset this
source, a collapsible Advanced area with softening, the model note, the exact
accuracy opt-in and the sampling status, plus the formula-field help line.

**Public API.** `SourceDraft`, `SourcesPanelState` (`add_kind`, `drafts`) with
`sync`, `mark_edited`, `set_radius`, `set_strength`, `set_length`, `set_turns`,
`set_center`, `set_axis`, `commit`, `set_softening`, `set_allow_inaccurate`;
`show(ui, state, panel, compact)`; `status_text(&SourceSampling)`.

**Invariants.**
- A draft keeps invalid text across frames; `sync` only rebuilds new/removed
  entries or non-dirty drafts.
- Invalid edits set `draft.error` and never call `set_source_scene`; valid edits
  replace only that entry. `Add` is disabled at `MAX_SOURCES = 64`.
- Softening uses the scene range `1e-6..=1.0`; the opt-in and display go through
  the committed scene setters. Any valid source edit clears `source_notice`.
- The object-style pair lives in the existing display section, not a new panel.
  It writes `State::set_object_style`; `OBJECT_STYLE_HELP` names both modes and
  states that SVG exports keep the line blueprint.
- `status_text` reports hidden versus shown accuracy and unavailable counts per
  family without inventing counters; a scene validation error is shown readably.
- The Sources help line explains that streamlines/particles remain formula
  features. `Advanced` starts open so the opt-in and status are discoverable.

**Pinned by.** `tests/sources_panel_test.rs`
(`sources_panel_adds_removes_and_resets_through_real_widgets`,
`invalid_draft_does_not_replace_the_committed_source`,
`source_notice_is_rendered_and_survives_until_dismissed`,
`scene_error_text_names_the_displayed_row_and_parameter`,
`add_is_disabled_at_the_source_limit`,
`object_style_control_writes_the_enum_and_names_both_modes`), and
`tests/button_sizing_test.rs::compact_sources_panel_buttons_keep_touch_targets`.

#### `src/ui/induction_panel.rs` — the dedicated induction editor

**Purpose.** The wide left rail/compact sheet body for the `FieldMode::Induction`
teaching demo.

**Public API.** `InductionPanelState { scene, error, advanced }`,
`InductionPanelState::default`, `commit(&mut State) -> bool`,
`show(ui, &mut panel, &mut State, compact) -> Option<InductionPanelAction>`,
`InductionPanelAction::{Fit, Reset}`, and `status_text`.

**Invariants.**
- `commit` calls `State::set_induction_scene`; an invalid draft never replaces
  the committed scene, and the error names the failing field.
- A clean draft mirrors `State::induction`; a draft that failed to commit keeps
  its error and text until it is corrected or the section is reset.
- Case, geometry, amplitude, frequency, display and the default-off accuracy
  opt-in all commit through the same draft; `Fit` and `Reset induction section`
  are one-shot actions the app applies through the view-command and default-
  scene seams.
- Advanced contains the shared source-scene softening editor (clearing both
  adapter caches), E/B status counters, the assumptions note, and the native CAS
  derivative provenance when available.
- Induction is arrows-only; the panel states that streamlines, particles and
  click seeding stay off.

**Pinned by.** `tests/induction_panel_test.rs`
(`induction_panel_wide_and_compact_expose_all_controls`,
`edits_validate_and_keep_the_last_valid_scene`,
`induction_rail_expands_a_real_app_frame`) and
`tests/layout_test.rs` for the persisted layout fields.

#### `src/ui/material_panel.rs` — the responsive Materials panel

**Purpose.** The wide left rail/compact sheet body for `FieldMode::Materials`,
plus the shared case/view names, viewport summary and help rows.

**Public API.** `MaterialPanelState { draft, error, advanced }`,
`MaterialPanelState::default`, `commit(&mut State) -> bool`, `sync_from(&State)`,
`show(ui, &mut panel, &mut State, compact) -> Option<MaterialPanelAction>`,
`MaterialPanelAction::{Fit, Reset}`, `case_label`, `view_label`,
`family_symbol`, `family_unit`, `summary`, `MATERIAL_HELP_ROWS`,
`material_help_rows`, `MATERIALS_ONLY_NOTE`.

**Invariants.**
- `commit` calls `State::set_material_scene`; an invalid draft keeps the last
  valid scene/sampling and records an actionable error naming the field.
  `sync_from` runs on mode entry, Reset and future persistence restore, never
  every frame, so a dirty draft survives.
- Case, geometry, both background and both material constants, the applied
  vector and the view commit through the draft on click or release; a frame
  never commits merely because a value exists.
- The applied family uses the exact design units: `V/m` for electric cases and
  `T` for magnetic cases. The panel never converts to generic display units.
- Advanced is closed by default and shows the case assumptions, the interface
  tolerance, the current sampling status/count, the current warning only, and
  `No general material solver`.
- A rejected draft error is shown until the next successful commit or Reset;
  warnings come only from the sampling that belongs to the committed scene, so
  a valid scene edit cannot present a prior scene's warning as current.
- `Reset` restores the default material scene and the default camera through
  the app's shared Reset seam; `Fit` uses the existing view-command seam.
- Materials is arrows-only, with the exact note
  `Arrows only: streamlines, particles and click seeding stay off in Materials.`

**Pinned by.** `tests/material_panel_test.rs` (wide/compact controls, commit
and rejection, view/Reset, exact units, disabled Features/CAS, help rows,
viewport label, current-only warning, rail/sheet requests) and
`tests/layout_test.rs` for `materials_expanded`/`materials_width`.

#### `src/ui/maxwell_panel.rs` — the responsive Maxwell panel

**Purpose.** The wide left rail/compact sheet body for `FieldMode::Maxwell`,
plus case names, the formula-bar summary and the help rows.

**Public API.** `MaxwellPanelState { draft, error, advanced, wave, wave_error,
wave_open, learning, gauss }`, `MaxwellPanelState::default`,
`commit(&mut State) -> bool`, `commit_wave(&mut State) -> bool`,
`sync_from(&State)`, `show(ui, &mut panel, &mut State, compact) ->
Option<MaxwellPanelAction>`, `MaxwellPanelAction::{Fit, Reset, SwitchTo2d,
Measurement, InvestigateGaussShell, FreeExploreGaussShell}`,
`case_label`, `summary`, `MAXWELL_HELP_ROWS`, `maxwell_help_rows`,
`MAXWELL_ONLY_NOTE`, `MAXWELL_CONDUCTOR_NOTE`, `MAXWELL_ACCURACY_OPT_IN`,
`WAVE_SCOPE_NOTE`, `WAVE_NORMALIZED_NOTE`, `WAVE_AMPERE_NOTE`, `WAVE_SVG_NOTE`.

**Invariants.**
- `commit` calls `State::set_maxwell_scene`; an invalid draft keeps the last
  valid scene/sampling and records an actionable error naming the field.
  `sync_from` runs on mode entry and Reset, never every frame.
- Case, geometry, display amplitudes, outline, elements and applied x/y commit
  through the draft on click or release; a frame never commits merely because a
  value exists.
- Only case-relevant geometry is shown: Conductor2D alone adds the outline
  (circle/square), elements and applied x/y. The exact note
  `Conductor2D is an infinite-cylinder cross-section, not a 3D finite object.`
  is always visible for that case; in 3D it is followed by `Switch to 2D`.
- `Switch to 2D` calls `State::set_mode(false)`, never changes the case or mode,
  and the app records the new scene frame before the simulation pass so the
  camera is preserved.
- Advanced is closed by default and shows the case assumptions, the current
  sampling status/warning only, omitted/large counts, BEM coarse/fine/relative
  difference/target, and the accuracy opt-in. The opt-in is enabled only for a
  finite `MaxwellStop::AccuracyNotMet`; `Unavailable` can never be shown.
- Warnings come only from the sampling that belongs to the committed scene, so
  a valid scene edit cannot present a prior scene's warning as current.
- A collapsed `Plane wave` section adds enable/disable, amplitude, wavelength,
  phase, propagation axis with sign, and a polarization axis. Edits commit
  through `State::set_maxwell_wave`; a rejection keeps the committed wave and
  shows slice A's validation message verbatim. When a wave is enabled it also
  states the normalized `c = 1` scope and the four analytic equation-term
  statuses (Gauss-E, Gauss-B and Faraday `Analytic`; Ampere-Maxwell
  `Unsupported (j)` for a source-free wave), plus the SVG no-arrow note.
- A collapsed `Maxwell equations` section after the case controls embeds
  [`maxwell_learning_panel`](../src/ui/maxwell_learning_panel.rs). It receives
  the committed scene and the read-only cached BEM solution; it never commits a
  scene edit and never samples the field on a plain frame. `sync_from` (mode
  entry, restore and Reset) clears the learning report and message while keeping
  the equation and probe selections. Its one-shot `MeasurementRequest` becomes
  `MaxwellPanelAction::Measurement`, which the app applies by selecting the
  requested family and opening the matching Field probes surface or loop (or
  running the pointwise check), never by mutating the Maxwell scene.
- Maxwell is arrows-only, with the exact note
  `Arrows only: streamlines, particles and click seeding stay off in Maxwell.`
  Coloring is hidden and CAS is disabled with
  `CAS edits the formula field; switch to Formula.`
- The case controls also embed the session-only
  [`gauss_shell`](../src/ui/gauss_shell.rs) investigation. `Investigate` and
  `Free explore` return their own one-shot actions; the app loads
  `MaxwellCase::HollowSphere`, keeps `FieldMode::Maxwell`, reveals the existing
  Field probes area and never fabricates a durable probe surface. Exact
  cancellation input changes (scene, probe radius, family, clock) invalidate the
  cached reading.

**Pinned by.** `tests/maxwell_panel_test.rs` (wide/compact reflow, case-specific
controls, rejection, current-only warning, accuracy opt-in, Switch to 2D
camera/mode preservation, disabled Features/CAS, help rows, rail/sheet
requests), `tests/maxwell_learning_panel_test.rs` (the embedded section's
equation/form, residual and limitation contracts), `tests/maxwell_wave_ui_test.rs`
(the plane-wave controls, verbatim validation, equation statuses and export
notes) and `tests/layout_test.rs` for `maxwell_expanded`/`maxwell_width`.

#### `src/ui/maxwell_learning_panel.rs` — Maxwell learning and residual section

**Purpose.** Slice B of issue #94: a session-only teaching and diagnostics
section for the four Maxwell equations. It shows both statement forms, the
symbol table and the active-limit assumptions from
[`maxwell_learning`](../src/maxwell_learning.rs), then optionally runs one
bounded residual check through [`maxwell_residual`](../src/maxwell_residual.rs).

**Public API.** `MaxwellLearningPanel { equation, form, probe, report, message,
point_highlight }` with `Default` (Gauss electric, differential, probe
`[0, 0, 0]`), `show(ui, &MaxwellScene, Option<&BemSolution>) ->
Option<MeasurementRequest>`, `check(&MaxwellScene, Option<&BemSolution>)`,
`clear_report()`, `measurement_request()`, `measurement_explanation(&MaxwellScene)`,
`MeasurementRequest { equation, kind, family }`, `MeasurementKind::{Surface,
Loop, Point}`, `ResidualReport { residual, terms, absolute, inputs }`,
`ReportInputs { scene, equation, probe }`, `TermExplanation { terms }`,
`TermNote { symbol, central_difference, status }`, and the constants
`MAXWELL_LEARNING_LIMITATION`, `MAXWELL_NORMALIZED_SCALE`,
`MAXWELL_POINTWISE_NOTE`, `MAXWELL_REPORT_STALE`,
`MAXWELL_PARTIAL_MEASUREMENT_NOTE` and `MAXWELL_HIGHLIGHTED_POINT`.

**Invariants.**
- `measurement_request` maps the selected form to the measurement that
  actually checks it: integral Gauss to an oriented surface (E or B family),
  integral Faraday/Ampere to an oriented loop (E or B family), and every
  differential form to the existing pointwise residual check. The app applies
  the request; the panel itself never mutates probes or the scene. An
  incompatible committed case paints `measurement_explanation`, names the
  active family and offers a compatible case and the alternative law; clicking
  the button still reports that explanation and mutates nothing.
- Integral forms show `MAXWELL_PARTIAL_MEASUREMENT_NOTE`, naming the sampled
  integral and the unmeasured right-hand side. A pointwise check never claims
  an integral estimate, and highlights the sampled point with
  `MAXWELL_HIGHLIGHTED_POINT`.
- Field work happens only on `Check residual` or the pointwise measurement
  action: one press does exactly twelve `maxwell_field_with_bem` evaluations,
  six offsets at `h` and six at `h/2` with `h = 1e-3 * case extent`. A plain
  frame never samples.
- Every report records the exact committed `MaxwellScene`, equation and probe it
  was computed for. Before painting, a plain frame compares those inputs with
  the committed scene and current selection; on any mismatch it clears the
  report and shows `MAXWELL_REPORT_STALE` without evaluating the field. The
  display form is not compared because the numeric check is always the
  pointwise differential form, so switching Integral/Differential keeps the
  report. `sync_from` clears the report and message on mode entry, restore and
  Reset and keeps the equation/probe selections.
- The report always names the committed case and probe it belongs to, so a
  rejected draft can never make the on-screen result look like the draft's.
- Divergence and curl come from the six-offset central differences; the error
  is the largest absolute difference between the `h` and `h/2` stencils and is
  reported per term as `TermStatus::Numerical`.
- The teaching seam exposes one family per scene, so an electric case supplies
  only `div_e`/`curl_e` and a magnetic case only `div_b`/`curl_b`. The other
  family is never inferred or relabelled.
- `rho`, `j`, `de_dt` and `db_dt` are never fabricated: every teaching case
  leaves them `None`, and the model reports `Unsupported` with the exact symbol.
  The always-visible limitation states that the teaching cases are static or
  quasistatic, so the check is partial and not a full Maxwell verification.
- A non-finite probe or offset evaluation is passed as non-finite, so the model
  reports `Singular` with the offending symbol; the panel never paints NaN or
  infinity. A `Conductor2D` scene without a matching cached BEM solution shows
  the existing pending/stale warning and performs no residual check.
- The report readout states that the numeric check is the pointwise differential
  form at the probe point, so selecting `Integral` changes only the displayed
  statement and is never mistaken for an integral check. The integral text stays
  visible and the check stays enabled.

**Pinned by.** `tests/maxwell_learning_panel_test.rs`:
`panel_lists_every_equation_and_both_forms`,
`selecting_an_equation_shows_its_symbols_and_assumptions`,
`gauss_magnetic_residual_is_reported_against_a_zero_rhs`,
`solenoid_interior_divergence_matches_the_analytic_zero`,
`missing_time_derivative_terms_are_reported_unsupported_not_zero`,
`non_finite_probe_point_is_reported_singular_and_never_nan`,
`conductor_case_without_a_solution_skips_the_check_with_the_existing_warning`,
`conductor_case_with_a_stale_solution_skips_the_check_with_the_stale_warning`,
`residual_check_runs_only_on_press`,
`report_clears_when_the_committed_case_changes`,
`report_clears_when_a_geometry_parameter_changes`,
`report_clears_after_a_scene_reset_or_restore`,
`report_names_the_committed_scene_and_probe`,
`a_rejected_draft_keeps_the_report_labelled_with_the_on_screen_scene`,
`plain_frames_do_not_re_evaluate_after_a_check`,
`panel_never_claims_a_full_time_dependent_solution`, and
[tests/maxwell_measurement_test.rs](../tests/maxwell_measurement_test.rs)
`every_equation_and_form_maps_to_the_matching_measurement`,
`integral_magnetic_gauss_is_a_b_flux_surface_and_differential_is_a_point`,
`applying_an_integral_law_adds_or_reuses_the_matching_family_probe`,
`incompatible_action_is_explained_and_mutates_nothing`, and
`differential_action_highlights_the_point_and_names_unavailable_terms`.

#### `src/ui/gauss_shell.rs` — guided Gauss-law investigation (#219)

**Purpose.** A self-contained, session-only teaching activity for the shipped
`MaxwellCase::HollowSphere`: one physics question, a concentric Gaussian sphere
at probe radius `r` distinct from the shell radius `R`, live `r` and `2r`
readings (radial field, area, sampled flux), pin cards and the normalized
cancellation algebra. No new physical case, quadrature rule or tolerance is
added.

**Public API.** `GaussShellState { active, probe_radius, pinned, ... }` with
`Default` inactive, `activate(&MaxwellScene)`, `deactivate`, `reset`,
`invalidate_report`, `report_for(&MaxwellScene, FieldFamily, f64)`,
`pin`, `pinned`, `clear_pinned`; `GaussShellReading { radius, kind }` with
`field`, `area`, `flux`, `flux_estimate`; `GaussShellReadingKind::{Exterior,
Interior, Excluded}`; `GaussShellReport { radius, at_radius, at_double }`;
`GaussShellAction::{Investigate, FreeExplore}`;
`compute_reading(scene, radius, family, time) -> GaussShellReading`; and
`show(ui, &mut state, &MaxwellScene, FieldFamily, f64) -> Option<GaussShellAction>`.

**Invariants.**
- The Gaussian surface is a sphere centred on the origin with `size.x = 2r`, so
  `GaussianSurface::area` reports `4*pi*r^2`. Flux goes through the shipped
  `field_probe::flux` full/half-resolution passes via a local `ProbeField`
  adapter; the investigation never touches `State::probe_surfaces`.
- `r < R` reports zero radial field and zero flux with the inside explanation;
  `|r - R|` inside `hollow_sphere_interface_tolerance(R)` reports `Excluded`
  with no finite field or flux; `r > R` reports the sampled field, area and
  flux together, plus the quarter-field/fourfold-area/equal-flux ratios.
- Pin snapshots are values, so later radius or scene edits never rewrite them.
  At most two are kept; `Clear pinned readings` removes both.
- Everything is normalized teaching units and labelled as such. The algebra
  line states that equality holds only within the reported quadrature
  uncertainty.

**Pinned by.** `tests/gauss_shell_test.rs`: the exterior ratios and flux
uncertainty, interior zero, excluded interface, pin snapshot/clear, inactive
default/no side effects, wide/compact rendering and the normal Maxwell-panel
`Investigate` entry.

#### `src/ui/ode_panel.rs` — the responsive ODE workbench

**Purpose.** The wide left rail/compact sheet body for `FieldMode::Ode`: source
selector (equations/current field), typed dimension and equation rows, bounded
initial conditions, scene duration, chart selector, time-series variables,
Step/Fit/Reset actions, CPU charts and a closed Advanced solver/status block.

**Public API.** `OdePanelState { draft, error, advanced }`,
`OdePanelState::default`, `commit(&mut State) -> bool`,
`sync_from(&State)`, `show(ui, &mut panel, &mut State, compact) ->
Option<OdePanelAction>`, `OdePanelAction::{Step, Fit, Reset}`,
`ode_help_rows`, `ODE_HELP_SECTION`.

**Invariants.**
- `commit` calls `State::set_ode_scene`; invalid text keeps both the committed
  scene and the draft, and `error` comes from `State::ode_error`. A clean draft
  is reloaded from the committed scene by `sync_from`, which the app calls on
  mode entry, section reset and persistence restore — never on an ordinary frame
  with a dirty draft.
- Text commits on focus loss/Enter, DragValues on release, and seed/button
  actions on click; no edit re-solves once per dragged frame.
- Seeds stay finite and bounded to `ODE_MAX_SEEDS`; duration stays inside
  `0.1..=ODE_MAX_DURATION`; at least one time-series variable stays selected and
  the all-off attempt paints `Select at least one variable.`.
- Phase portrait paints x/y only (a projection in 3D) from the shared solution
  samples with `ODE_COLORS`, seed and current-time markers. Time series paints
  the selected variables against `t` from the same samples. Empty/error
  solutions paint an explanation and no fabricated lines.
- Advanced states the fixed-step RK4 `h`, sample/step/stop counts and the current
  field dimension when that source is active.
- The panel returns actions only; the app owns clock seeking, Fit, scene reset
  and camera reset.

**Pinned by.** `tests/ode_panel_test.rs`
(`ode_panel_exposes_equations_seeds_and_actions`,
`ode_equation_edit_commits_valid_scene_only`,
`ode_initial_conditions_are_bounded_and_distinct`,
`time_series_variables_reject_all_false`,
`ode_compact_and_wide_panels_reflow`,
`ode_chart_switch_commits_state`,
`ode_mode_disables_features_and_cas`,
`open_ode_reaches_the_wide_rail_or_compact_sheet`).

#### `src/ui/formula_bar.rs` — formula text and mode toggles

**Purpose.** The top formula row.

**Public API.** `FormulaBarState { text, open_sources, open_induction, open_ode, open_materials }`,
`FormulaBarState::new`, `show(ui, state, bar, compact)`.

**Invariants.**
- In vector mode the Formula/Sources/Induction/ODE/Materials selector calls
  `State::set_field_mode`; Sources, Induction, ODE and Materials hide the text
  editor and show their own summary plus
  `Open Sources`/`Open induction`/`Open ODE`/`Open materials`, which raise the
  matching one-shot request for the app to consume.
- The Materials summary names the selected case and view and the applied field
  with its exact unit (`V/m` or `T`).
- Compact keeps the projection and adapter selectors on one horizontally
  scrollable row and puts the editor/summary beneath it, so a large font cannot
  squeeze the compact sheet out of existence.
- The bar's text mirror is refreshed from `state.active_source()` when they
  differ, so presets, restores and kind changes update the field without
  overwriting a preserved scalar draft.
- Every edit is handed to `state.set_active_source`; the bar never parses or
  clamps itself.
- Scalar kinds show the fixed `y =` or `z =` label and an `x`/`x and y` hint;
  vector mode keeps comma-separated component syntax.
- Clicking 3D/2D calls `state.set_mode` (which selects the saved Curve/Surface
  in plot mode).
- Compact sets a 44 pt interaction height, wraps rather than overflowing the
  panel, and uses `layout::formula_field_width` (≥ 40 % of the bar); wide lets
  the field fill the remainder.

**Pinned by.** `tests/sources_panel_test.rs::formula_bar_switches_mode_and_hides_the_editor`;
`tests/induction_panel_test.rs::formula_bar_selects_induction_and_hides_the_formula_editor`;
`tests/ode_panel_test.rs::formula_bar_selects_ode_and_hides_the_editor`;
the setter contract is in `tests/viewmodel_test.rs`
(`valid_formula_replaces_field_and_clears_error`,
`garbage_formula_keeps_field_and_reports_error`), the width rule in
`tests/responsive_test.rs::the_formula_field_keeps_at_least_forty_percent_of_the_bar`,
and the real hitboxes in
`tests/button_sizing_test.rs::real_formula_transport_and_help_buttons_fit_and_grow`.

**Limitations.** No syntax highlighting, autocomplete, or history.

#### `src/ui/help_overlay.rs` — one help table, two surfaces

**Purpose.** The compact viewport overlay and the full Help › Keyboard & mouse
window.

**Public API.** Font-size constants, `HelpHeadingStyle`,
`section_heading_style`, `OVERLAY_HEADING_COLOR`, `window_heading_color`,
`HelpRow`, `HelpSection`, `FLY_HELP_*`, `SEED_HELP_*`, `LEFT_CLICK_HELP_*`,
`HELP_SECTIONS`, `SCALAR_HELP_SECTION`, `SOURCES_HELP_SECTION`,
`INDUCTION_HELP_SECTION`, `INDUCTION_HELP_ROWS`, `ODE_HELP_SECTION`,
`ODE_HELP_ROWS`, `scalar_help_rows`, `sources_help_rows`,
`induction_help_rows`, `ode_help_rows`, `all_rows`, `sections_for`,
`rows_for`, `live_global_rows`, `LiveRow`, `FORMULA_HINT`, `show`, `help_window`.

**Invariants.**
- `HELP_SECTIONS` holds the static 3D/2D view rows; the Global group's rows are
  generated from the live `Bindings` by `live_global_rows`, so a remap is
  visible in both help surfaces without a second static table. The overlay
  shows the active view + Global, the window shows all three groups plus the
  Scalar plots, Sources and Induction sections. `HELP_SECTIONS` is a static, not
  a const, so both surfaces can be proven to share the allocation.
- The Sources section covers formula/source separation, all eight editors,
  E/B/Both without summation, scene-wide softening, the default-off accuracy
  opt-in, hidden non-finite/unavailable results, and that streamlines,
  particles and click seeding remain formula features.
- The Induction section covers the quasistatic softened model, vector-potential
  reconstruction (arbitrary B alone cannot determine E), the shared clock,
  static-field zero induced E, arrows-only scope and the shared softening.
- `?`/F1 toggle the overlay; Esc and the × dismiss it; the window has its own
  open flag.
- Headings and rows use pinned sizes/colours; the window heading is the theme's
  strong colour, the overlay heading is fixed light grey.
- The formula hint is not an input row.

**Pinned by.** `tests/help_overlay_test.rs` (16 tests):
`the_overlay_and_window_share_one_table`,
`the_overlay_shows_only_the_active_view_and_global`,
`every_binding_is_listed_exactly_once_in_its_group`,
`help_explains_source_mode_and_accuracy_opt_in`,
`the_help_window_renders_the_sources_section`,
`the_stale_right_drag_pans_copy_is_gone`,
`two_d_help_never_mentions_orbiting`,
`help_typography_ranks_the_window_title_above_section_headings_and_rows`.

**Limitation.** The compact overlay is desktop-sized; the app hides it in
Compact below 520 pt of viewport width.

#### `src/ui/layout.rs` — panels, rails, compact assembly, persisted layout

**Purpose.** Layout constants/records, the desktop rails, and the compact
tab-bar/sheet plan.

**Public API.** `RAIL_WIDTH = 32`, `RAIL_FRAME_MARGIN = 4`,
`RAIL_LABEL_SIZE = 14`, `RAIL_CHEVRON_ZONE = 24`, `rail_frame`;
`DEFAULT_CONTROLS_WIDTH = 260`, `DEFAULT_PRESETS_WIDTH = 240`,
`DEFAULT_APPEARANCE_WIDTH = 240`, `DEFAULT_INTERACTION_WIDTH = 240`,
`DEFAULT_SOURCES_WIDTH = 280`, `DEFAULT_INDUCTION_WIDTH = 280`,
`DEFAULT_ODE_WIDTH = 300`, `DEFAULT_MATERIALS_WIDTH = 300`,
`DEFAULT_MAXWELL_WIDTH = 300`, `MIN_PANEL_WIDTH = 180`,
`MAX_PANEL_WIDTH = 460`; `COMPACT_WIDTH = 700`,
`SHEET_MAX_FRACTION = 0.55`, `VIEWPORT_MIN_FRACTION = 0.45`,
`VIEWPORT_CHROME_ALLOWANCE = 20`, `TAB_BAR_HEIGHT = 48`, `TOUCH_TARGET = 40`,
`FORMULA_MIN_FRACTION = 0.40`; `LayoutMode`, `SheetState`, `layout_mode`,
`CompactPlan`, `compact_plan`, `sheet_height`, `formula_field_width`,
`RailSide`, `rail_text_angle`, `rail_chevron`, `collapse_chevron`,
`rail_label_band`, `rail_chevron_pos`, `rail_tooltip`, `RailLabel`,
`rail_entry_height`, `rail_entry_interaction`, `paint_rail_entry_state`,
`paint_rail_entry`,
`LayoutState`, `clamp_width`, `rail`, `RailPanel`, `rail_panel`,
`collapse_button`, `collapse_button_named`.

**Invariants.**
- Compact applies at width ≤ 700; non-finite width stays Wide.
- Compact is transient: `LayoutState` stores the desktop panel
  visibility/width fields (Controls, Presets, Appearance, Interaction, Sources,
  Induction, ODE, Materials and Maxwell) plus help, and native-only
  `cas_expanded`/`cas_width`; the sheet and mode are never persisted.
- `SheetState::Appearance`, `SheetState::Interaction`, `SheetState::Sources`,
  `SheetState::Induction`, `SheetState::Ode`, `SheetState::Materials` and
  `SheetState::Maxwell` share the one compact sheet.
- Appearance and Interaction default to collapsed at 240 pt and sanitize to
  180..=460.
- Sources and Induction default to collapsed at 280 pt, ODE, Materials and
  Maxwell at 300 pt, and all sanitize to 180..=460; each record is consulted
  only in its own field mode.
- `sheet_height` caps at 55 % of the window and keeps 45 % + 20 pt for the
  viewport; nonsense inputs yield 0.
- Rails: left label reads bottom-to-top, right top-to-bottom; chevrons point
  outward on a rail and inward on a collapse button; the rail is the only
  restore affordance; the real content band is narrower than the outer width.
- Both rails share `rail_entry_height` and `paint_rail_entry`: a collapsed
  right-side panel paints one rounded, label-sized entry top-aligned in its
  content band, matching one entry of the ordered left strip instead of a
  full-height strip. `RailPanel::content_rect` is that entry tile (the click
  target); its width is still the real content band.
- `LayoutState::sanitized` clamps widths; `reset` restores first-launch layout
  (Controls/Presets expanded, Sources collapsed, native CAS closed, help
  visible).
- `formula_field_width` never goes below 40 % of the bar and never returns a
  negative width.
- Compact buttons have a 40-point floor; the tab bar grows beyond its 48-point
  baseline when font-derived button content and frame margins require it.

**Pinned by.** `tests/layout_test.rs` (14):
`rail_paints_one_rotated_label_inside_the_real_band`,
`rotated_labels_fit_inside_the_real_rail_band`,
`rail_panel_is_outer_width_and_the_label_band_is_narrower`,
`layout_state_serializes_exactly_its_platform_persisted_fields`,
`widths_clamp_and_corrupt_widths_fall_back`;
`tests/responsive_test.rs` (11):
`layout_mode_is_compact_at_and_below_700_points`,
`compact_plan_hides_the_side_panels_and_shows_the_tab_bar`,
`sheet_height_caps_at_55_percent_and_keeps_the_viewport_floor`,
`only_one_sheet_is_open_at_a_time`,
`two_finger_pan_maps_only_3d_multi_touch_translation`.

**Limitations.** Width-only breakpoint; the compact sheet does not persist which
tab was open; rail content band is deliberately narrow.

#### `src/ui/panel_rails.rs` — ordered reusable panel toggles

`RailEntry`, `RailEntryResponse`, `RailStripResponse` and `show_rail_strip` provide
an ordered, vertically scrolling 32pt strip with stable entry IDs, independent
visibility and measured rotated-label bounds. Each entry sizes itself with
`layout::rail_entry_height` and paints through `layout::paint_rail_entry`, the
same helpers the collapsed right rail uses, so the two rails cannot drift.
`fit_panel_widths` fits requested widths by minimums and proportional excess
without mutating persisted widths. Production widget interaction, third-entry
extensibility and width allocation are pinned by `tests/panel_rails_test.rs`.

#### `src/ui/menu.rs` — the pure menu and shortcut tables

**Purpose.** Every menu item, shortcut, and the state/effect dispatch behind
them.

**Public API.** `FLY_SHORTCUT`, `UNDO_SHORTCUT`, `REDO_SHORTCUT`, `ABOUT_*`;
`MenuAction` (25 variants, `ToggleCas` native-only) and `ALL`; `SideEffect`;
`MenuItem`; `Menu`; `MENUS`; `Shortcut`; `SHORTCUTS`;
`interaction_action_for`, `menu_action_for`, `chord_shortcut_label`,
`shortcut_for`, `shortcut_label(action, bindings)`,
`item_shortcut_label(item, bindings)`, `shortcut_allowed`,
`shortcut_actions(ctx, bindings)`, `help_key_pressed`; `UiState`;
`apply_action`.

**Invariants.**
- Every `MenuAction` is reachable from a menu item or shortcut; no action has
  two menu homes. `MenuAction::ALL` is generated from the same macro as the
  enum, so the test cannot drift.
- Remappable menu commands (panels, help, screenshot, copy formula, quit)
  resolve their chord and label from the active `Bindings`; `SHORTCUTS` keeps
  only the reserved Undo/Redo pair and the display-only fly hint.
- Shortcut matching is exact: extra modifiers do not fire a plain shortcut.
- All shortcuts except Ctrl+Q are inert while a text field has focus.
- `MENUS`: File (screenshot, quit), Edit (undo, redo, copy formula, colour/
  simulation/document resets, reset all controls), View (toggle controls,
  toggle presets, reset layout, reset camera, appearance, toggle/open
  interaction), Help (keyboard & mouse, about). Fly is toggled by C and the
  sidebar Camera section, not a menu item.
- Defaults: Ctrl+B Controls, Ctrl+P Presets, Ctrl+Shift+A Appearance panel,
  Ctrl+Shift+L Interaction panel, Ctrl+Q quit, Ctrl+Shift+S screenshot,
  Ctrl+Shift+C copy formula, C fly, F1 help. The labels are derived from the
  bindings, so a remap is visible immediately.
- `apply_action` mutates only `UiState`; app-only work is returned as a
  `SideEffect` (open popover, show help/about, screenshot, quit, copy, reset
  ids, and `ResetScope`).
- `ResetAllControls` resets the Controls mirror, clears fly, reapplies, and
  resets the clock, but does not change the formula.
- `ResetLayout` resets the persisted layout and asks for fresh panel ids.
  `ResetCamera` reports `SideEffect::ResetScope(EditScope::Camera)`; the app's
  one reset seam leaves fly, restores the default orbit/2D pose and clears the
  preset highlight. The Edit menu's colour, simulation and document resets use
  the same `ResetScope` seam, never an inline duplicate.
- `?` is a separate secondary help key; F1 remains the one table shortcut.

**Pinned by.** `tests/menu_test.rs` (28):
`every_enum_variant_is_reachable_from_a_menu_item_or_shortcut`,
`every_menu_item_is_backed_by_a_variant`,
`no_action_has_more_than_one_menu_home`,
`shortcuts_stay_out_of_text_fields_except_quit`,
`extra_modifiers_do_not_fire_the_plain_shortcuts`,
`reset_all_controls_returns_the_sidebar_knobs_without_touching_the_formula`,
`the_help_menu_entry_opens_the_full_help_window`; live labels and routing by
`tests/interaction_routing_test.rs`.

**Limitations.** No user-rebindable fly keys; fly stays physical (ticket 70).

#### `src/ui/presets.rs` — field presets and saved camera views

**Purpose.** The right Presets panel: built-in field presets grouped by
category, a `Source presets` section, camera Reset/Fit buttons, saved view
management, and the My-presets whole-scene section (issue #90 slice B2c).

**Public API.** `default_preset_dir` (native); `CAMERA_BUTTONS`; `SceneAction`
(`Save`, `Load(String)`); `PresetPanel` with `new`, `new_with_dir` (native),
`dir`, `presets`, `active_name`, `active_field_name`, `active_source_name`,
`pending_delete`, `take_applied`, `clear_active`, `save_current`, `load`,
`request_delete`, `cancel_delete`, `delete`, `load_field`, `load_source_field`,
`show`, plus `scenes`, `scene_name`, `set_scene_name`, `take_scene_action`,
`insert_scene`, `delete_scene`, `scene_message`, `set_scene_message`,
`scene_text`, `set_scene_text`.

**Invariants.**
- Built-in field presets return to Formula first, then load formula and
  dimension through the viewmodel setters; the saved source scene is preserved
  and the highlight only appears when the clicked name matches the current
  formula and mode (duplicate formulas highlight one row).
- Legacy 2D formula rows append ` (2D approximation)` from
  `preset_approximation_label`; 3D rows keep their ` (3D)` suffix.
- Clicking a source preset calls `load_source_field`, which validates and loads
  the scene through `load_source_preset`, clears the formula highlight,
  highlights the source row and asks the panel's caller for `ViewCommand::Fit`.
- Saving with an existing name replaces it; the new row becomes active.
- Delete asks for confirmation; delete removes from list and disk (or
  localStorage) and clears the highlight/request.
- `take_applied` returns a loaded camera exactly once; the app applies it to
  the orbit camera and leaves fly.
- A missing preset name changes nothing and returns false.
- Native `new` logs a load failure and starts empty; wasm storage failure shows
  one note but keeps the in-memory list usable.
- My presets loads `SceneStore` from the same directory/key on construction; a
  corrupt file yields an empty store and the storage note.
- `Save current scene` is disabled while the trimmed name is empty; one click
  posts `SceneAction::Save` and the app captures `ViewRecord::capture`, the
  committed parameters and the live 2D/orbit camera. The panel rejects a
  duplicate name or invalid document and reports it through `scene_message`;
  every successful mutation persists immediately, and a failed write keeps the
  in-memory store unchanged.
- `Load` posts `SceneAction::Load(name)`; the app restores the view, then commits
  the parameters, then applies the camera clamped to the live orbit limits and
  drops fly. Style, layout, preset-expansion and viewport-style records are
  never touched.
- `Delete` takes the existing two-click confirmation (`Delete scene` confirms).
- `Export text` fills `scene_text`; `Apply text` parses the whole collection
  with `import_store` and replaces the store only on success, persisting it;
  invalid or future-version text keeps the old store and shows the parser error.
  `Copy` uses `ui.ctx().copy_text`.

**Pinned by.** `tests/ui_preset_test.rs` (36), including
`every_built_in_field_preset_loads_through_the_panel`,
`loading_a_field_preset_sets_its_formula_and_dimension`,
`classic_presets_return_to_formula_and_keep_the_source_scene`,
`loading_a_duplicate_formula_preset_highlights_only_the_clicked_name`,
`saving_highlights_the_row_and_a_duplicate_replaces_it`,
`deleting_waits_for_confirmation_then_clears_the_highlight`,
`a_load_hands_the_camera_to_the_live_view_exactly_once`,
`test_presets_survive_restart`, and the scene battery
`saving_a_scene_captures_view_parameters_and_camera`,
`duplicate_scene_names_are_rejected_with_a_message`,
`loading_a_scene_applies_view_and_parameters_without_touching_ui_preferences`,
`loading_a_scene_clamps_the_camera_to_live_limits`,
`deleting_a_scene_needs_a_confirmation_click`,
`portable_scene_text_round_trips_through_apply_text`,
`invalid_portable_text_leaves_the_saved_scenes_untouched`,
`scene_name_rules_are_enforced_by_the_panel`,
`saved_scenes_load_from_the_preset_directory`,
`save_failures_surface_without_panicking`;
`tests/config_test.rs::test_data_dir_honors_xdg_override`.

**Limitations.** Saved views are 2D camera records only; no 3D orbit presets.
No rename; name field is free text. Saved scenes have no per-row rename and the
camera clamps to the live orbit limits rather than reproducing out-of-range
hand-edited values.

#### `src/ui/sidebar.rs` — the Controls panel

**Purpose.** Every scene knob (Display, Camera, Features, Coloring, Domain)
plus the formula's collapsed Variables editor, with a local mirror and a single
`apply` handoff.

**Public API.** `Section` + `SECTIONS`, `domain_summary`, `DomainAxis`,
`domain_axes`, `PlaneControlMode`, `plane_control_mode`,
`magnitude_scale_enabled`, `arrow_scale_enabled`,
`proportional_exponent_enabled`, `ArrowStyleSet`, `arrow_style_set`,
`sides_control_enabled`, `head_control_enabled`, `seed_mode_enabled`,
`seed_layout_visible`, `streamline_count_enabled_with_layout`,
`seed_layout_count_label`, `SEED_LAYOUT_DENSITY_INACTIVE_NOTE`,
`SeedGeneratorControls`, `variables_visible`, `VariablesControls`,
`commit_variables` (test seam), `particle_size_enabled`,
`particle_fixed_color_enabled`,
`fixed_color_rgb`,
`fixed_color_opaque`, `trail_fade_enabled`, `trail_width_enabled`,
`stagnation_seconds_enabled`, `particle_count_label`, `BOTH_COLORING_NOTE`,
`bake_resolution_label`, `bake_resolution_bounds`, `bake_accuracy_caption`,
`bake_provenance_line`, `bake_status_text`,
`DIRECTION_3D_EXPLANATION`, `MIN_SIGNED_LOG_HALF_WIDTH`,
`MAX_SIGNED_LOG_HALF_WIDTH`, `Controls` (+ `new`, per-section resets,
`Default`), `show`, `show_with_plot_action`, `show_compact`,
`show_compact_with_plot_action`, `apply`.

**Invariants.**
- Section order is Display → Camera → Features → Coloring → Domain; the first
  three start open. Each section has a reset button and resets only itself.
- The widgets edit `Controls`; `apply` calls the viewmodel setters and then
  refreshes the mirror from the effective state, so clamped/swapped values are
  what the next frame shows.
- Display's Show grid / Show coordinate system checkboxes default on, apply
  independently in both modes, and return to on with the Display reset.
- Density range keys to the sampling dimension (`is_3d`): 4..=200 flat,
  4..=40 for n³.
- `plane_control_mode(field.dimension, mode_3d)` is the one selector-visibility
  rule: `Hidden` for a two-component field, `SliceOnly` (axis + position, no
  enable/opacity) for a 3D field in 2D, and `Full` in the 3D view. The plane
  index always clamps against `state.plane_grid()`, never the render `grid()`.
- Domain rows gate on `field.dimension == 3`, so a 3D field in 2D keeps its z
  row; density still labels `n²` because the slice renders one n×n layer.
- Disabled controls are disabled for a reason: arrow scale in Magnitude,
  magnitude scale outside Magnitude, contrast outside Proportional, head/sides
  for headless families, seed mode with streamlines off, particle controls with
  particles off, trail controls below length 2, stagnation time with looping
  off, z row for a two-component field, Direction quantity in 3D (with a
  visible explanation). The streamline count is disabled when an explicit seed
  layout exists and a note explains that the automatic count/grid is inactive
  until "Clear all".
- The Features section hosts a collapsed-by-default "Baked flow" area: an
  enable checkbox (building once), the current axis's resolution selector and
  Rebuild, the measured accuracy caption naming the resolution, trilinear
  interpolation, `max |Δv|`, max relative error, divergence residual and the
  normalized-teaching-units caveat, plus the verbatim refusal or a stale
  prompt. Outside Formula the area stays reachable with disabled controls and a
  short "only available for Formula fields" explanation, so a recorded refusal
  remains visible. While a bake is active the streamline block also shows the
  baked provenance line; changing any field-affecting input makes it stale
  rather than silently rebuilding.
- The Seed layout editor ends with a collapsed-by-default nested "Named
  layouts" area, always available while the editor is visible even with zero
  explicit seeds. It lists the built-in `Default (generated grid)` first, then
  user layouts with seed counts. Apply validates the stored layout against the
  current scene domain and commits it through `State::commit_seed_layout`,
  clearing selection/drag/placement and reporting `Applied layout "…"`; the
  existing history driver records that document change as one entry. Save
  current and the two-click Delete mutate only `VfApp::seed_layouts`, never the
  document, so they neither dirty the scene nor record history. Portable
  export/import is versioned compact JSON; import is atomic and reports the
  new-layout delta honestly (replacement-only imports say so). One
  `seed_layout_message` slot carries these results and is separate from the
  generator/picking `seed_error`.
- The Features section's collapsed "Seed layout" area is shown only for
  formula streamlines (`seed_layout_visible`). It reads the count, offers
  "Place seed", "Clear all", numeric line/circle/region generator rows with
  inline `SeedError` text, and editable per-seed local/world coordinates with a
  per-seed delete. Generator and placement results go through the atomic
  `State` helpers; the editor never mutates the committed layout directly. The
  armed "Place seed" flag is cleared on every frame the editor is hidden, so
  re-enabling it cannot silently plant a seed later.
- The collapsed-by-default Variables section exists only for the formula field
  in a vector view (`variables_visible`), so Sources, Induction, Materials,
  Maxwell, Ode and plots hide it. One row per committed parameter shows its
  name, `Value`, optional `Min`/`Max` (an `unbounded` checkbox blanks a bound;
  non-finite values become blank), positive `Step` and a `Delete` button. The
  Add row validates through `State::commit_parameters`; a rejected add rolls
  the draft row back and keeps the typed name, while a rejected edit keeps every
  row's draft and shows the model's message inline. `Reset variables` commits an
  empty set. Drafts live in `VariablesControls` on `Controls`, are never
  persisted or mutated into `State.parameters`, and are refreshed only when the
  committed set changed. An edit equal to the committed set is not committed,
  so it requests no rebuild.
- Coloring keeps the palette row first, then Quantity, Scale, freeze and the
  legend, then auto-fit and the manual range, then the collapsed "Custom
  palette" editor. Direction is disabled in 3D. Signed log uses a validated,
  clamped half-width and shows the mapping's fallback explanation when the range
  does not span zero. Freeze is a one-shot request applied after the colour
  setters and before the mirror.
- The Custom palette editor is session-only: `Controls` keeps the draft
  (`palette_working`), the portable JSON buffer (`palette_text`), the last
  message and a one-shot `palette_apply_requested`. Editing seeds from the
  active override or the built-in's `ColorRamp::builtin` approximation; stop
  positions stay in `0.0..=1.0`, removal bottoms out at `MIN_STOPS`, and adding
  inserts at the widest gap's midpoint up to `MAX_STOPS`. Hex text that does not
  parse is reported inline and never changes the stop. `Apply palette` is
  disabled while `ColorRamp::validate` fails; `apply` consumes the request once,
  after the palette source and range setters and before freeze. Import/export
  text is versioned palette-only JSON: a parse or version failure writes
  `palette_message` and leaves both the draft and `State` untouched.
- A custom override survives later frames: `apply` calls `set_colormap` only
  when the built-in selection moved (or no override is active), because that
  setter always clears the override. Choosing a built-in therefore still clears
  a custom palette, including a same-value click, which raises a one-shot revert
  request in the colormap row. `reset_coloring` also resets the built-in to
  Viridis, so it clears an active override on the next `apply` whenever the
  built-in actually changed; an override survives the reset only when the
  built-in was already Viridis and the selection does not move.
- The legend paints 64 palette slices and reports the effective range, ticks,
  `<min`/`>max` caps and the app-supplied clip counts. Its stats and clips are
  cached `SceneColorFacts` copied into `Controls` before each wide or compact
  sidebar call.
- In Sources + Both the Coloring section shows `BOTH_COLORING_NOTE` and none of
  the palette, quantity, scale, freeze, legend, range or custom-palette
  controls.
- Domain extents are clamped before `set_domain`; NaN keeps the previous value;
  reversed pairs swap in the seam and mirror back.
- Plane index commits on toggle/axis/click or slider release, not during a live
  drag, so dragging does not resample.
- The Camera section's fly checkbox is a mirror; the app syncs the real camera
  into it before drawing and reconciles a click through
  `fly_camera::sync_fly_mirror`.
- In scalar mode the Plot section is drawn first; vector-only sections are
  disabled with explanations and retain their values. Only shared plot controls
  (grid/axes, domain, palette) reach the seam, and Fit is returned to the app.

**Pinned by.** `tests/sidebar_controls_test.rs` (71), including
`untouched_frame_rebuilds_nothing_through_apply`,
`synced_controls_never_redirty_the_scene`,
`density_clamp_keys_to_sampled_dimension_through_apply`,
`reversed_drag_extrema_swap_through_the_seam`,
`nan_domain_edit_keeps_the_previous_extent_through_apply`,
`apply_never_commits_the_plane_mirror`,
`every_control_dirties_the_scene_through_apply`,
`named_layouts_list_builtin_default_first_and_user_layouts_after`,
`applying_a_layout_replaces_seeds_and_reports_the_name`,
`applying_an_invalid_layout_reports_the_reason_and_keeps_seeds`,
`saving_current_seeds_rejects_empty_builtin_and_oversized_names`,
`saving_twice_with_one_name_replaces_in_place`,
`deleting_a_user_layout_needs_two_clicks_and_clears_on_other_actions`,
`export_copy_and_import_report_deltas_and_reject_bad_documents_atomically`,
`variables_section_lists_parameters_and_hides_outside_formula_vector_mode`,
`editing_a_value_commits_and_reparses_the_formula`,
`adding_an_invalid_name_reports_the_model_message_and_changes_nothing`,
`duplicate_and_reserved_names_are_rejected_without_mutation`,
`deleting_rows_commits_the_smaller_set_and_deleting_all_commits_empty`,
`reset_variables_commits_an_empty_set`,
`committing_an_unchanged_set_is_a_rebuild_no_op`;
`tests/sidebar_sections_test.rs` (order/reset isolation),
`tests/sidebar_camera_ui_test.rs`, `tests/sidebar_colormap_ui_test.rs`
(`quantity_selector_applies_component_colouring`,
`direction_quantity_is_disabled_in_3d_and_enabled_in_2d`,
`signed_log_half_width_applies_and_reports_unavailable_ranges`,
`freeze_button_commits_the_effective_range_and_turns_off_auto_fit`,
`legend_shows_effective_range_ticks_and_clip_counts`,
`both_display_keeps_the_fixed_family_note_without_advanced_controls`,
`reset_coloring_restores_quantity_scale_and_auto_fit`,
`editing_a_builtin_seeds_the_nine_stop_approximation`,
`removing_stops_bottoms_out_at_two`,
`adding_stops_is_bounded_and_keeps_positions_increasing`,
`invalid_hex_does_not_change_the_stop`,
`invalid_ramp_disables_apply_and_shows_the_reason`,
`applying_a_palette_overrides_the_builtin_and_choosing_a_builtin_clears_it`,
`export_text_round_trips_through_apply_text`,
`malformed_or_future_text_is_rejected_atomically`,
`copy_text_is_available_only_with_a_buffer`,
`reset_coloring_clears_the_palette_draft_and_keeps_the_override_when_the_builtin_is_unchanged`,
`reset_coloring_clears_the_override_when_it_also_resets_the_builtin`).

**Limitations.** The panel is long; it relies on collapsing sections rather than
a second settings page. There is no search/filter. `Controls` is a hand-written
mirror, so adding a state knob means updating the struct, `new`, the resets,
`apply`, and the tests.

#### `src/ui/status_bar.rs` — bottom readout and hints

**Purpose.** Parse error/hint on the left; FPS, counts, density, and sim time on
the right; compact stacking.

**Public API.** `HINT_LIFETIME = 2.5`, `COMPACT_STATS_SIZE = 12`,
`StatsMask`, `StatsMask::FULL`, `visible_stats`, `FpsMeter` (`fps`, `frame_ms`,
`update`), `advance_hint`, `#[doc(hidden)] restart_hint`, `compact_count`,
`format_status`, `show`.

**Invariants.**
- Mask priority (drops first): sim time at < 960 pt, density < 700, trails < 520,
  particles < 360; FPS, frame ms, arrows and streamlines are always shown.
- The width fed to the mask is the window content width, not the panel's inner
  width, so the 960 pt full-readout case works.
- Parse error wins over a hint; hint wins over “ready”.
- FPS is a 90/10 EMA seeded by the first valid sample; invalid dt is ignored.
- Hints live 2.5 s of frame time; a changed text restarts the lifetime; the app
  must set a hint only on the triggering event, not every frame.
- Counts come from the painted snapshot; streamlines are the app's field
  segment count (excluding the backdrop).
- Compact stacks the timing line above a truncated composition line.

**Pinned by.** `tests/status_bar_test.rs` (19):
`visible_stats_is_full_at_960_and_drops_in_priority_order`,
`format_status_labels_density_for_the_sampling_dimension`,
`format_status_prints_zero_counts_with_plurals`,
`fps_meter_smooths_later_samples`,
`advance_hint_expires_after_the_frame_time_lifetime`,
`status_bar_renders_hint_until_the_parse_error_wins`,
`compact_readout_lines_fit_a_360_point_panel_with_production_faces`.

**Limitation.** Counts are snapshot-derived and therefore lag one rebuild, which
is intended.

#### `src/ui/style_panel.rs` — appearance record and egui styling

**Purpose.** Theme, accent, rounding, spacing, font size; apply them to egui;
persist the `AppStyle`.

**Public API.** `SPACING_Y_RATIO`, slider bounds, `AccentPreset`,
`ACCENT_PRESETS`, `selected_preset`, `AppStyle` (+ `sanitized`),
`selection_text_color`, `apply`, `StyleControls`, `show_controls`.

**Invariants.**
- Defaults: dark theme, brand cyan `#4cc9f0`, rounding 6, spacing 8, font 13.
- `sanitized` clamps finite values and replaces non-finite with defaults.
- `apply` is idempotent, pins the theme explicitly, and configures both stored
  themes so a theme switch cannot resurrect stale accent/spacing/rounding.
- Selection text colour is chosen by WCAG contrast (white or black) and clears
  4.5:1 on every accent.
- Spacing's vertical component is `spacing * 3/8`, reproducing egui's default
  `(8, 3)` at the default.
- The accent swatches are eight distinct, opaque Material colours; a custom
  accent selects none.
- Button padding derives from the persisted font size (8–24). Menu overrides,
  icon-only controls, and color wells retain that sizing; compact hit targets
  stay at least 40 points. Non-button editors are not comprehensively rescaled.

**Pinned by.** `tests/ui_style_panel_test.rs` (13):
`apply_is_idempotent`, `apply_sets_the_selection_stroke_in_both_themes`,
`selection_text_color_is_readable_on_every_accent`,
`accent_presets_are_eight_unique_named_fully_opaque_colours`,
`default_accent_is_the_brand_cyan`;
`tests/accent_picker_test.rs` (popover/picker dismissal interaction);
`tests/button_sizing_test.rs` (real button hitboxes, painted ink, padding,
font growth, compact floors, menus, swatches, presets, CAS, and tab bar).

**Limitations.** One accent at a time; no custom font loading; no per-panel
style overrides.

#### `src/ui/top_bar.rs` — menu bar, appearance popover, About

**Purpose.** Renders the wide menu bar or compact hamburger, the gear/popover,
the screenshot button, and the About window.

**Public API.** `APPEARANCE_BUTTON_LABEL`, `WHEEL_ZOOM_RATE = 0.002`,
`wheel_zoom_factor`, `wheel_zoom`, `dismisses_appearance_popover`,
`HAMBURGER_LABEL`, `show`, `show_compact`, `about_window`.

**Invariants.**
- The wide bar renders `menu::MENUS` and reports one `MenuAction`; the compact
  hamburger lists the same items in one flat dropdown.
- The gear toggles the same popover as `View > Appearance…`; a click outside
  dismisses it, but a click on the popover or its open nested colour picker does
  not.
- Wheel zoom factor is `exp(0.002 * scroll_points)` times egui's pinch
  `zoom_delta`; nonsense input is identity. Ctrl-scroll is routed by egui into
  `zoom_delta`, so the two never double-count.
- About shows the 128 px mark, build version (`version::VERSION`), tagline,
  copyright, GPL-3.0-only, and both URLs.
- The screenshot button requests the same action as File > Export screenshot.

**Pinned by.** `tests/top_bar_test.rs` (14):
`the_bar_carries_the_v1_menu_table`,
`compact_hamburger_lists_every_wide_menu_item`,
`the_gear_opens_the_appearance_popover`,
`clicking_the_popover_or_its_open_picker_keeps_the_popover`,
`scrolling_up_zooms_in_and_down_zooms_out`,
`about_renders_the_full_brand_identity`;
`tests/accent_picker_test.rs`.

**Limitations.** The compact dropdown has no submenus (deliberate for thumbs).

#### `src/ui/transport.rs` — simulation transport strip

**Purpose.** Play/pause, reset time, readout, speed slider; repaint and Space
handling.

**Public API.** `play_label`, `reset_label`, `time_label`, `show`,
`request_particle_repaint`, `toggle_on_space`.

**Invariants.**
- The strip owns no state; it edits the app's `SimClock`.
- It is shown only while `sim_clock::transport_visible` is true (particles on in
  3D today).
- Speed edits a local copy and go through `set_speed`, the single clamp.
- Space is consumed only when the transport is visible and no text field has
  focus; otherwise the key stays with the focused widget.
- Repaint is requested only while `should_animate` (playing + consumer).

**Pinned by.** `tests/particle_playback_test.rs`:
`transport_labels_reflect_state_and_stay_compact_ready`,
`the_transport_strip_shows_play_reset_time_and_speed_and_toggles`,
`space_toggles_only_when_visible_and_never_while_typing`,
`repaint_is_requested_only_while_animating`,
`transport_is_visible_only_with_a_clock_consumer`.

**Limitation.** No scrubber or frame stepping; speed is time multiplier only.

### 3.9 Version, window, fonts

#### `src/version.rs` — compile-time build version

**Purpose.** Report `VFE_BUILD_VERSION` when set, otherwise the crate version.

**Public API.** `CARGO_VERSION`, `VERSION`, `resolve_version`.

**Invariants.**
- Unset or empty override falls back to `CARGO_VERSION`; a non-empty override
  wins. `resolve_version` is `const`.
- Nightly strings are formatted by `scripts/nightly-version.sh` as
  `<base>-nightly.<YYYYMMDD>+<short-sha>`.

**Pinned by.** `tests/version_test.rs` (5):
`a_build_override_wins`, `an_empty_override_falls_back_to_the_crate_version`,
`an_unset_override_falls_back_to_the_crate_version`,
`version_without_an_override_is_the_crate_version`,
`the_startup_log_reports_the_build_version`;
`tests/nightly_version_test.rs` (format/date/sha rules).

**Limitation.** The version is compiled in; changing it needs a rebuild.

#### `src/window.rs` — native window options and icon

**Purpose.** Desktop-sized, centred, persistence-backed window and the committed
mark as the window icon.

**Public API.** `WINDOW_TITLE = "Vector Field Explorer"`,
`DEFAULT_INNER_SIZE = [1280, 800]`, `MIN_INNER_SIZE = [960, 600]`,
`window_options()` (native), `window_options_present()` (native measurement
geometry), `#[doc(hidden)]`/`pub(crate)` `icon_data`.

**Invariants.**
- The window starts centred, resizable, at 1280×800, cannot shrink below
  960×600, and persists its size/position.
- `window_options_present()` instead pins exactly 1280×800 (min = max, not
  resizable), disables window persistence, uses a distinct app id and points
  eframe's storage at `/dev/null`, so a measurement cannot restore or rewrite
  user geometry.
- The 256 px PNG is decoded at startup; a decode failure logs and falls back to
  eframe's default icon rather than blocking launch.
- Wasm never calls this module's native functions.

**Pinned by.** `tests/window_test.rs` (6):
`opens_at_the_desktop_default_inner_size`,
`refuses_to_shrink_below_the_minimum`,
`remembers_its_size_and_position`,
`starts_centred_and_resizable`, `installs_the_mark_on_the_window_options`.

**Limitations.** No fullscreen/maximised startup option, no custom decorations.

#### `src/fonts.rs` — vendored text faces

**Purpose.** Install the app's own fonts because eframe is built without
`default_fonts`.

**Public API.** `#[doc(hidden)] font_definitions()` (test seam); `install`.

**Invariants.**
- Uses `FontDefinitions::empty` (not `default`) so re-enabling eframe's bundled
  fonts cannot silently reintroduce the UFL face.
- Proportional: DejaVu Sans, then egui-icons for `special_emojis`.
- Monospace: DejaVu Sans Mono, DejaVu Sans for missing symbols, egui-icons.
- The three vendored files and their licences live under `assets/fonts/`.

**Pinned by.** `tests/fonts_test.rs` (6):
`epaint_bundles_no_builtin_faces`, `installs_exactly_the_three_vendored_faces`,
`families_are_in_the_ruled_order`, `every_shipped_ui_glyph_has_a_face_in_both_families`,
`vendored_bytes_are_real_truetype_fonts`, `the_shared_context_has_real_text_faces`.

**Limitation.** DejaVu's glyph coverage is broad but not complete (no CJK); the
app does not load system fallback faces.

---

## 4. Persistence in detail

### 4.1 Storage keys and shapes

| Data | Key | Encoding | Failure rule |
| --- | --- | --- | --- |
| Style + layout + view | `eframe::APP_KEY` | eframe's serde/RON app value | absent/corrupt → defaults; ticket-07 bare style migrated |
| Native camera presets | `<dir>/presets.json` | JSON `Vec<Preset>` | read/write error is returned; panel logs and starts empty |
| Browser camera presets | `vector-field-explorer.camera-presets` | JSON `Vec<Preset>` | corrupt → empty list + warning; unavailable storage → panel note |

The native preset directory is `$XDG_CONFIG_HOME/vector-field-explorer`, else
`~/.config/vector-field-explorer` (`ui::presets::default_preset_dir`).

### 4.2 Why camera state is not persisted

This is a deliberate product decision recorded in `persistence.rs` and
`app/frame.rs`: the live camera resets to the default orbit pose on launch, and
**camera presets are the supported way to restore a viewpoint**. Consequences:

- `ViewRecord` contains no yaw/pitch/distance/target/ortho zoom and no fly pose.
- `tests/fly_camera_test.rs::the_fly_camera_is_not_persisted` asserts the saved
  envelope contains no fly state and round-trips unchanged.
- Camera presets are only the 2D record (`render::Camera` position + scale); a
  saved view is a 2D target/zoom, not a 3D orbit orientation. Applying one
  leaves fly mode.

### 4.3 Corruption and migration rules (practical summary)

- **Absent storage** → `AppSettings::default()`.
- **Corrupt storage** → defaults, with a warning only when a string was present.
- **Ticket-07 bare `AppStyle`** → style preserved, layout/view defaulted.
- **Partial envelope** → serde defaults fill missing keys; `sanitized` clamps
  style/layout numbers.
- **Bad stored formula** → `set_formula` keeps the last good field, sets
  `parse_error`, and the app starts.
- **Reversed/out-of-range domain** → `sidebar::apply` clamps, swaps, and keeps
  the previous value for NaN.
- **Unknown enum spelling** → documented default (`LeftClickAction` custom
  deserializer; old arrow style aliases).
- **Camera presets** → native load errors propagate (no silent rewrite); the
  panel logs and shows an empty list; wasm corrupt JSON falls back to empty.

---

## 5. Input and gesture map

### 5.1 Viewport

| Gesture | 3D | 2D |
| --- | --- | --- |
| Primary drag | Orbit (`drag_channel` → Orbit) | Pan |
| Secondary drag | Orbit (Shift+secondary → Pan) | Pan |
| Middle drag | Pan | Pan |
| Wheel / pinch | Zoom distance (egui `zoom_delta` × wheel factor) | Multiply ortho zoom |
| Two-finger pan | Pan (suppresses pointer orbit) | Inactive |
| Arrow keys | Orbit; Shift pans | Pan; Shift 3× faster |
| `+` / `=` / `-` | Zoom in/out | Zoom in/out |
| `R` | Reset view | Reset view |
| `F` | Fit domain | Fit domain |
| Double-click | Reset view (undoes first click's seed) | Reset view |
| Click | Per `Left click` action | Per `Left click` action |
| S | Seed streamline at pointer (inert in fly) | Seed streamline at pointer |
| P | Particle burst at pointer | Hint: particles need 3D |
| C | Enter/leave fly | Inert |
| Space | Play/pause when transport visible | Inert |

The pure decisions are `viewmodel::drag_channel`, `apply_camera_input`,
`two_finger_pan`, `click_outcome`, `captured_click_outcome`,
`view_command::command_for_key`, `view_key_commands`, `click_action_plan`,
`seed_shortcut`, and `shortcut_seed_point`.

### 5.2 Fly mode

| Input | Action |
| --- | --- |
| `C` / sidebar Camera checkbox | Enter/leave fly (3D only) |
| W/A/S/D (physical positions) | Forward/left/back/right |
| Q/E | Down/up |
| Shift | 4× speed |
| Primary drag | Look (fallback when raw motion is unavailable) |
| Pointer-lock motion | Look on native and web when captured |
| Wheel | Flight speed (0.5..20, exponential) |
| Esc / focus loss / 2D switch / preset / Reset/Fit | Leave fly, restore orbit |
| Left click (captured) | Seed at crosshair centre; no double-click reset |

Capture is best-effort and edge-triggered (`fly_camera::capture_transition`);
native tries `Confined` then `Locked`; web uses `web/pointer_lock.js`. A lost
lock or a text field exits fly so a hidden cursor is never stranded.

### 5.3 Panels, rails, and shortcuts

- Click a collapsed rail to expand; click the header chevron to collapse.
  Ctrl+B / Ctrl+P toggle Controls / Presets.
- Menu: File / Edit / View / Help; the compact bar replaces it with a hamburger
  listing the same commands. The screenshot button is at the right.
- Ctrl+Shift+S screenshot, Ctrl+Shift+C copy formula, Ctrl+Q quit (global; works
  while typing), F1 help, `?` help, Esc closes popovers/help.
- Space toggles play/pause only when the transport is visible and no text field
  has focus.
- Clicking outside the Appearance popover dismisses it, except when the click is
  in the popover or its nested colour picker.

All of these are table-driven in `ui::menu` and `ui::help_overlay`; changing a
binding means changing the table (and its tests), not wiring a widget.

---

## 6. Rendering contract

### 6.1 Instance layouts

`ArrowInstanceGPU` and `LineInstanceGPU` are `#[repr(C)]`, `bytemuck::Pod`
structs of three `[f32; 4]` fields, 48 bytes each:

- Arrow/dot: `origin = [x, y, z, kind]` (`kind` 1.0 arrow, 0.0 particle);
  `dir_len = [dir.x, dir.y, dir.z, length/radius]`; `color = [r, g, b, a]`.
  Dots use `origin.w = 0.0`, `dir_len.w = radius`, and `dir_len.xyz` unused.
- Line: `a = [x, y, z, 1.0]`; `b = [x, y, z, width]`; `color = [r, g, b, a]`.

`Globals` is 224 bytes: two 4×4 matrices, four `vec4`s (`camera_pos`,
`light_dir`, `camera_right`, `camera_up`), and two `u32`/`f32` parameter
`vec4`s. The Rust struct and every WGSL `Globals` struct must match exactly;
`tests/dot_billboard_test.rs::globals_uniform_layout_carries_the_camera_basis_and_arrow_uniforms`
and `tests/arrow_style_test.rs::every_shader_globals_struct_carries_the_arrow_uniforms`
pin that.

### 6.2 Shaders and pipelines

| File | Entry points | Role |
| --- | --- | --- |
| `shaders.arrow.wgsl` | `vs_scene`, `fs_scene` | Combined 3D arrows + particle billboards; per-style generated geometry; half-Lambert arrows, lit round dots |
| `shaders.arrow2d.wgsl` | `vs_arrow2d`, `fs_arrow2d` | Flat 2D arrows in world XY; unlit |
| `shaders.line.wgsl` | `vs_line`, `fs_line` | Camera-facing line ribbons; unlit |
| `shaders.dot.wgsl` | `vs_dot`, `fs_dot` | Camera-facing round dot billboards (2D dot path); half-Lambert |
| `shaders.object.wgsl` | `vs_object`, `fs_object`, `fs_object_attenuated`, `vs_composite`, `fs_composite` | Offscreen object bodies: lit 36-byte mesh + 128-byte instance stream, slice discard, barycentric wireframe, attenuated shaded discarded slice half; fullscreen premultiplied-alpha composite |

The four vector files share one bind group: `Globals` uniform + a read-only
storage array of instances. They generate geometry from `vertex_index`; there is
no vertex buffer. All are `TriangleList` with `cull_mode: None`, no depth
attachment, one sample, alpha blending. `shaders.object.wgsl` is the separate
depth-enabled object pass described in §3.6; it is not part of the vector
draw-plan table and its composite runs in egui's no-depth pass.

### 6.3 Orthographic vs perspective

- **3D:** `OrbitCamera::proj_perspective` (FOV 55°, near 0.01, far 100) ×
  `view` (orbit look-at). Fly uses its own perspective with the same FOV/near/
  far.
- **2D:** `OrbitCamera::front_view` (eye at `target + Z*distance`, looking down
  -Z, Y up) × `proj_ortho`. `plane_size` is the padded larger domain extent;
  `ortho_zoom` divides it. This is the same `vp_matrix` the viewport,
  screenshot, and click seeding share.
- The camera's `eye_position(ortho)` and `camera_basis_from_view_proj` keep line
  ribbons and dot billboards camera-facing in either projection.

### 6.4 Offscreen screenshot path (RGBA)

1. `frame_viewport` builds `scene`, `view_proj`, `eye_position` for the displayed
   frame and calls `DrawState::render_offscreen` (native) or
   `render_offscreen_async` (wasm).
2. `submit_offscreen` calls `screenshot::png_readback_check(self.format)` first;
   only RGBA8/BGRA8 (Unorm or sRGB) are accepted.
3. It re-uploads the same `Globals`/instances, creates a texture in the surface
   format with `RENDER_ATTACHMENT | COPY_SRC`, clears to the viewport's panel
   fill, records the same `draw_passes`, and copies to a readback buffer with
   256-byte-padded rows.
4. Native maps the buffer, polls `Wait`, strips padding, and converts to RGBA.
   Wasm maps through the event loop, strips/converts, encodes the PNG, and
   starts a DOM download in the completion; the UI gets a toast.
5. The capture size is physical pixels, both axes scaled under
   `MAX_CAPTURE_EDGE` (4096 native, 2048 wasm) so framing/aspect are unchanged.

### 6.5 Why the smoke test exists

`cargo build` and `cargo test` never create a wgpu device. WGSL parse errors,
bad bindings, pipeline validation errors, and font-less blank windows only
appear when the app actually runs. `scripts/smoke-run.sh` builds, launches the
binary under `timeout`, and fails on:

- `panicked at`
- `wgpu error:`
- `Validation Error`
- `Shader .* parsing error`
- `No font provider has any font`

A timeout (124/137) or clean exit is a pass. CI runs it under `xvfb-run` with
software Vulkan (`WGPU_BACKEND=vulkan`, `mesa-vulkan-drivers`) and installs
`libxkbcommon-x11-0`, which winit needs before any wgpu work. Run it after
touching renderer/shader/windowing/input code: `scripts/smoke-run.sh 15`.

---

## 7. Build, test, CI, and release

### 7.1 Local commands

```sh
cargo build
cargo run --release
scripts/test.sh                  # cargo test --all-targets, then prunes stale binaries
scripts/bench-perf-baseline.sh   # release perf baseline; add --gpu for offscreen frames
scripts/bench-present-floor.sh   # bounded native present-path floor record
KEEP_TEST_ARTIFACTS=1 scripts/test.sh
scripts/test.sh --incremental
cargo clippy --all-targets -- -D warnings
scripts/smoke-run.sh 15
scripts/check-web-bootstrap.sh
scripts/check-site.sh            # site HTML/text gate (delegates to scripts/check_site.py)
scripts/test-web.sh              # wasm check + trunk build --release + dist asset checks
cargo test -- --list             # inventory, no build of the app
```

`scripts/test.sh` runs `cargo test --all-targets` with forwarded arguments,
probes the exact test executables, and prunes stale hashed binaries (ticket 34).
It preserves the test exit status; cleanup is skipped for `--release`,
`--profile`, `--target`, `--target-dir`, or a target mismatch. Cargo is capped at
10 jobs by `.cargo/config.toml`.

`scripts/test-web.sh` requires `rustup` (to install/select the wasm target) and
Trunk 0.22.1, runs `cargo check --target wasm32-unknown-unknown --lib --bin
vector-field-explorer`, `trunk build --release`, and requires
`dist/index.html`, a `dist/*.wasm`, a `dist/*.js`, the manifest, OG image, and
the committed icon set.

### 7.2 GitHub Actions

| Workflow | Trigger | What it does |
| --- | --- | --- |
| [ci.yml](../.github/workflows/ci.yml) | push/PR to main, nightly cron | clippy, `scripts/test.sh`, debug build; separate smoke job (`xvfb-run` + software Vulkan); separate wasm job (`check-web-bootstrap.sh`, `test-web.sh`) |
| [nightly.yml](../.github/workflows/nightly.yml) | daily 03:17 UTC / manual | verifies main, computes `X.Y.Z-nightly.YYYYMMDD+sha`, calls `package.yml`, moves the single `nightly` prerelease tag only after assets are attached |
| [package.yml](../.github/workflows/package.yml) | reusable | release builds for Linux x86_64 AppImage, macOS x86_64/arm64 `.app.zip`, Windows x86_64 `.zip`; pinned linuxdeploy/appimagetool/runtime; excludes host GPU libraries |
| [pages.yml](../.github/workflows/pages.yml) | push to main (paths) / manual | builds the wasm bundle and site (`trunk build --release`), runs `scripts/check-site.sh`, deploys to GitHub Pages |
| [release.yml](../.github/workflows/release.yml) | `v*` tag / manual | tag must match `Cargo.toml` and be reachable from main; verifies, packages, publishes assets; pre-release tags get `--prerelease` |

### 7.3 Release and versioning

- SemVer; `Cargo.toml` version is the source of truth
  (`scripts/crate-version.sh`).
- A release tag is `v` + that version; `scripts/check-version.sh` rejects a
  mismatch and a tag not reachable from `main`.
- Nightly: `scripts/nightly-version.sh <base> <YYYYMMDD> <sha>` →
  `<base>-nightly.<date>+<7-char-sha>`; the daily workflow sets
  `VFE_BUILD_VERSION`, so About and the startup log report the exact commit.
- Assets: one Linux AppImage, two macOS `.app.zip` files, and one Windows
  `.zip`. macOS and Windows builds are unsigned; the release notes explain the
  first-launch approval (macOS) and the SmartScreen path (Windows). No
  auto-update, signing, notarization, or telemetry.
- Licence: GPL-3.0-only (`Cargo.toml`, `LICENSE`). Dependencies must remain
  GPL-3.0-compatible; the packaging workflow ships `README.md`, `LICENSE`,
  `THIRD_PARTY_LICENSES.md`, and `THIRD_PARTY_NOTICES.txt` in the macOS and
  Windows archives. The Linux AppImage also
  carries the bundled system libraries' package copyright notices under
  `usr/share/doc/vector-field-explorer/system-libraries/`.

### 7.4 Test layout rule

Every test is an integration test under `tests/`; `src/` carries no
`#[cfg(test)]` modules. `tests/test_layout_test.rs::src_carries_no_inline_test_modules`
enforces this. Test-only access is provided by a small number of
`#[doc(hidden)] pub` seams (arrow geometry, particle history, font definitions,
help typography helpers, branding cache, hint restart), documented in their
modules. Do not widen the public API for tests without a ledger ruling.

Gated benchmark modules (`tests/*_bench_test.rs` and
`tests/perf_baseline_test.rs`) are registered in `tests/suite.rs` but return
immediately unless their environment flag is set. The maintained post-epic
harness is `tests/perf_baseline_test.rs` behind `VFE_PERF_BASELINE=1`, driven by
`scripts/bench-perf-baseline.sh`; see [docs/perf/README.md](perf/README.md) for
what it measures and what it deliberately does not. The native present-path
floor harness is `src/perf_present.rs` (pure config/aggregation) plus
`src/app/present.rs` (raw wall-clock inter-update driver with a safety
deadline), behind `VFE_PERF_PRESENT=1` and driven by
`scripts/bench-present-floor.sh`; a normal app never reads the variable and
`tests/perf_present_test.rs` pins the inert default.

---

## 8. First safe changes: recipes and hazards

| Goal | Start here | Watch out |
| --- | --- | --- |
| New formula syntax | `field.rs` | Keep top-level comma splitting; update `field_test.rs`; do not add a second evaluator (ADR 0003) |
| New field control | `viewmodel.rs` setter + `ui/sidebar.rs` `Controls`/`apply`/resets | Also update `persistence.rs` capture/restore and every round-trip test |
| New built-in field | `presets.rs` | Must parse, stay finite, be unique and categorised; update `presets_test.rs` |
| New arrow family | `arrow_style.rs`, `arrow_geometry.rs`, `renderer.rs` draw plan, `shaders.arrow.wgsl` | Keep Rust/WGSL vertex budgets and shader ids in sync; add non-degenerate geometry tests |
| New camera move | `view_command.rs` or `viewmodel.rs` gesture seam | Keep it pure; keyboard, menu, panel and double-click all route through the same command |
| New panel section | `ui/sidebar.rs` `Section` + reset | Follow Display→Camera→Features→Coloring→Domain and the nested `CollapsingHeader` convention; update section tests |
| New persistence field | `persistence.rs` + `ui/sidebar.rs` | Choose a serde default; keep old records loading; add a legacy test; never persist camera/fly/clock |
| New render pass | `renderer.rs` `DrawPass`/`draw_passes`/`record` | No depth attachment; keep the painter order deterministic; add a structural test |
| New screenshot behavior | `screenshot.rs` pure contract first | RGBA only; 256-byte rows; physical pixels; test without a GPU |
| New wasm behavior | `main.rs` gate + `web/*.js` | Update `scripts/check-web-bootstrap.sh`; keep native build unaffected |
| UI text or shortcut | `ui/menu.rs` / `ui/help_overlay.rs` | One table; tests pin reachability and exact matching |

Hazards worth knowing before editing:

- **No depth buffer.** Any new 3D geometry must join the painter sort or it will
  draw in submission order. `SceneGPU::combined` is sorted by the app; other
  collections are sorted by their own helpers.
- **One bind group, no vertex buffers.** Instance layout changes must update
  Rust, every WGSL `Globals`/instance struct, and the layout tests together.
- **Fixed 3D vertex budget (186).** Adding a wider family would require a new
  budget constant and shader constant; the combined draw cannot vary vertices
  per instance.
- **Two arrow style sets.** 2D never reads the 3D style and vice versa; keep the
  draw plan and persistence fields separate.
- **Dirty flag discipline.** A setter that forgets `mark_dirty` leaves stale
  geometry; a setter that dirties on every frame destroys the idle path.
- **Particle state is not in the snapshot.** Do not move per-frame dots/trails
  into `snapshot()`; the app's simulation block owns them.
- **Ephemeral state is deliberate.** Camera, fly, clock, compact mode/sheet,
  click seeds, and particle state are not persisted. Add one only with a
  ledger ruling and migration tests.
- **GPL compatibility.** New dependencies must be GPL-3.0-compatible and, where
  applicable, wasm-capable.

---

## 9. Module ↔ test inventory

`cargo test --all-targets -- --list` at this commit reports **716** raw `#[test]`
lines: 715 are executed on this platform (714 in the suite plus the isolated
`config_test` target), and one more is wasm-only. The high-value map:

| Area | Test file(s) | Count |
| --- | --- | --- |
| Viewmodel seam | `viewmodel_test.rs` | 66 |
| Particles | `particles_test.rs` | 52 |
| Sidebar controls | `sidebar_controls_test.rs` | 51 |
| Persistence | `persistence_test.rs` | 81 |
| Fly camera | `fly_camera_test.rs` | 31 |
| Arrow style | `arrow_style_test.rs` | 30 |
| Menu | `menu_test.rs` | 26 |
| View commands/shortcuts | `view_command_test.rs` | 22 |
| Status bar | `status_bar_test.rs` | 19 |
| Streamlines | `streamlines_test.rs` | 19 |
| Screenshot | `screenshot_test.rs` | 19 |
| Particle playback/clock | `particle_playback_test.rs` | 17 |
| Help overlay | `help_overlay_test.rs` | 14 |
| Layout | `layout_test.rs` | 14 |
| Top bar | `top_bar_test.rs` | 14 |
| Saved views | `ui_preset_test.rs` | 13 |
| Arrow opacity | `arrow_opacity_test.rs` | 13 |
| Dot billboard | `dot_billboard_test.rs` | 12 |
| Style panel | `ui_style_panel_test.rs` | 13 |
| Font-responsive buttons | `button_sizing_test.rs` | 10 |
| Responsive/compact | `responsive_test.rs` | 11 |
| Plane | `plane_test.rs` | 11 |
| Camera projections | `camera_pan_zoom_test.rs` | 11 |
| Presets catalogue | `presets_test.rs` | 10 |
| Arrow alpha/sort | `arrow_alpha_test.rs` | 9 |
| Arrow geometry | `arrow_geometry_test.rs` | 9 |
| Colormap | `colormap_test.rs` | 9 |
| Camera | `camera_test.rs` | 9 |
| Left-click action | `left_click_action_test.rs` | 8 |
| Shortcut keys | `shortcut_test.rs` | 8 |
| Web assets | `web_assets_test.rs` | 8 |
| Camera input | `camera_input_test.rs` | 7 |
| Export/PNG | `export_test.rs` | 7 |
| Field | `field_test.rs` | 7 |
| Line alpha/trails | `line_alpha_test.rs` | 7 |
| Reference lines | `reference_lines_test.rs` | 10 |
| Fonts | `fonts_test.rs` | 6 |
| Pointer lock | `pointer_lock_test.rs` | 6 |
| Window | `window_test.rs` | 6 |
| Domain | `domain_test.rs` | 5 |
| Sampling | `sampling_test.rs` | 5 |
| Version | `version_test.rs` | 5 |
| Accent picker | `accent_picker_test.rs` | 4 |
| Branding assets | `branding_assets_test.rs` | 4 |
| Camera presets JSON | `preset_test.rs` | 4 |
| Sidebar camera | `sidebar_camera_ui_test.rs` | 4 |
| Sidebar sections | `sidebar_sections_test.rs` | 4 |
| Cleanup script | `clean_test_artifacts_test.rs` | 18 |
| Nightly version script | `nightly_version_test.rs` | 11 |
| Config/XDG | `config_test.rs` | 1 |
| Test layout guard | `test_layout_test.rs` | 2 |

| Download link | `download_link_test.rs` | 1 |
| Notices bundle | `notices_test.rs` | 1 |
| Packaging notices | `packaging_notices_test.rs` | 7 |

The remaining files (`camera_zoom_pan_test.rs`, `click_seed_zoom_test.rs`,
`sidebar_colormap_ui_test.rs`) add the small legacy/2D-specific checks.

---

## 10. Known limitations, gathered

- **Fields may be time-dependent.** `meval` expressions bind `x`, `y`, `z`, and
  the shared time `t`; the Electrodynamics presets themselves are static
  spatial snapshots. Induction supplies the dedicated dynamic-field demo.
- **Four separate concept modes.** Formula, Sources, Induction, and Ode are
  exclusive modes with durable state; there is no formula-to-source or
  formula-to-ODE conversion.
- **No general solver.** The native CAS differentiates formulas and derives
  gradient, divergence, and curl. The Ode workbench is a bounded, precomputed
  fixed-step RK4 trajectory viewer, not an adaptive/stiff or symbolic solver; no
  PDE or Maxwell solver is implemented.
- **No depth/MSAA.** 3D ordering is CPU painter sort; no shadows, anti-aliasing
  beyond dot rims, or transparency sorting per triangle.
- **One 3D draw budget.** Every instance pays for a 16-side cone even when the
  scene is dots or a simpler family.
- **CPU particles and sorts.** Large clouds and dense scenes cost CPU; particles
  are capped at 4000 and trails at 100k points.
- **Persistence is compatibility-by-serde.** No explicit schema version; every
  new field needs a default/alias and a legacy test.
- **Camera presets are 2D records.** No 3D orbit preset; applying a preset
  leaves fly.
- **Compact layout is width-only.** No landscape-phone height breakpoint; the
  help overlay is hidden below 520 pt of viewport width.
- **Two orphan files.** `src/render/instancer.rs` and `src/ui/arrows.rs` are on
  disk but not declared in a module, so they are not compiled.
- **GPU behavior is not unit-tested.** `cargo test` never creates a device; the
  smoke run and CI smoke job are the only checks for shaders/pipelines/windowing.

---

## 11. Where to look next

- Terms and distinctions (view state vs camera state, field preset vs camera
  preset, arrow style vs 2D arrow style, seed mode): [GLOSSARY.md](../GLOSSARY.md).
- Why the layout uses rails and one persisted envelope: [ADR 0001](adr/0001-ui-layout-panel-rails.md).
- Why one clock owns all animation and what a new time base costs:
  [ADR 0002](adr/0002-single-simulation-clock.md).
- Why physics is analytic-first, why formula evaluation uses `meval`, and the
  post-v1 order: [ADR 0003](adr/0003-physics-foundations.md).
