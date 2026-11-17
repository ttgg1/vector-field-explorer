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
  screenshots, and persistence are all in this tree.
- The app is a teaching-grade explorer, not a general numerical solver. The
  pure source-evaluator slice in `src/sources` evaluates all eight softened
  source geometries: point charges, finite charged lines, charged rings, charged
  thin discs, uniform solid spheres, infinite straight wires, current loops and
  finite continuous solenoids. The ODE workbench precomputes bounded fixed-step
  RK4 teaching trajectories. Maxwell work beyond that is described in
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
`PreparedScene` pairs that frame with renderer-ready `SceneGPU`. Geometry,
camera applicability and background conversion are covered by
`tests/cli_scene_test.rs`. The same module's `prepare_scene` preflights an indexed
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
        │  - in Sources mode, append source_glyph_lines after those lines
        │  - cache dots_range from the freshly sampled magnitudes
        │  - retrace click bursts on every dirty rebuild
        ▼
 VfApp::frame_viewport (app/viewport.rs)
        │
        ├─ 3D: combined_instances(arrows, dots) + sort_scene_back_to_front
        ├─ 2D: sorted_arrows_back_to_front(arrows); combined stays empty
        ├─ trails: sorted_lines_back_to_front(trails) (3D and 2D plane slices)
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
  active scene's `source_glyph_lines` after the reference/flow/click lines, so
  markers follow the same selected `plane_frame` as the samples. In
  `FieldMode::Induction` it appends exactly the selected conductor's glyph and
  no streamline/click geometry; Formula mode preserves the durable scene but
  appends none. In `FieldMode::Ode` the pure snapshot stays empty and the app
  owns precomputed trajectory lines plus axes, rebuilt once per solved scene;
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
`ui/branding.rs`, `ui/menu.rs`, `ui/transport.rs`, `version.rs`, and the four
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
user-installed ffmpeg.

#### `src/animation_scene.rs` — detached animation frame source

**Purpose / API.** Owns the detached copy an animation export renders from:
`AnimationSnapshot { state, particles, orbit, fly, clear }`,
`AnimationFrame { scene, view_proj, eye }`,
`AnimationSnapshot::{from_parts, capture, frame_at}`, and
`VfApp::animation_snapshot`. `State: Clone` and `Particles: Clone` make the copy;
`State` holds no GPU handles.

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
matches each exact substep time, and the first frame renders the captured cloud's
dots, trails and combined stream.

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
encodes to memory and downloads the GIF on finish.

**Invariants.** Eligibility is derived only from the live `State`: Formula needs
a time-dependent field or an enabled particle target; Induction needs
`is_animated()`; Sources, Materials, Ode and scalar plots take the shared
unsupported reason. `pump` renders at most one detached frame per app update and
never mutates `VfApp` or advances the shared clock; the explicit `1/fps` step and
captured snapshot are the only animation state. Every frame passes
`apply_export_banner` with the provenance captured at Start before the sink sees
it. A job that fails, is cancelled, or is dropped abandons its sink, so the
streaming encoder deletes its temporary file and never finishes the output.
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
a job is running.

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

**Pinned by.** `tests/cli_test.rs`, registered in the native suite.

**Dispatch.** `run_from` handles clap help/version, reads an optional strict JSON
config, resolves flags and calls `run_export`. The latter validates and prepares
one scene, selects CPU SVG or offscreen RGBA8 + PNG encoding by output extension,
and writes the explicit path with contextual errors. Neither constructs `VfApp`,
uses storage, nor opens dialogs. No-argument GUI and wasm startup are unchanged.
`tests/cli_test.rs` exercises the actual executable with child-only environments;
`scripts/test-cli-dispatch.sh` separately runs real displayless 2D/3D PNG decoding,
non-background field checks and persistence sentinels through the wired binary.

#### `src/cli_svg.rs` — native CPU vector export

**Purpose / API.** `cli::scene_to_cli_svg(&PreparedScene)` returns a 2D SVG or
rejects 3D before serialization, without creating a GPU adapter or device.

**Invariants.** Uses the prepared frame and instance order. Flat arrow shaft/head
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
wgpu adapter/device and returns packed RGBA8. It does not create a surface,
window, app or persistence state. PNG encoding/writing belongs to CLI dispatch.

**Invariants.** Dimensions, packed/padded pixel bytes, storage sizes, vertex
counts and adapter/device limits are checked before resource allocation. No
optional device features are requested. The target must support rendering,
blending and copying `Rgba8UnormSrgb`; no format or CPU/noop fallback is allowed.
The existing `DrawState` paints the prepared frame with its linear clear color;
readback is display-encoded sRGB. GPU validation/internal/OOM scopes, uncaptured
errors and device loss become actionable CLI errors. The doc-hidden production
`offscreen_error_to_cli` preserves render versus typed readback classification.
Two further doc-hidden production seams support GPU-free failure checks:
`validate_gpu_limits` delegates to the same private resource-size validator used
for rendering, and `acquire_device_with` invokes the actual device request once
and preserves its original cause in a contextual `CliError::Device`.

**Pinned by.** `tests/cli_gpu_test.rs` covers invalid dimensions, reduced texture,
readback/storage/binding limits, injected device-request failure, exactly-once
acquisition success/failure, and actual map callback failure.
`scripts/test-cli-gpu.sh` runs isolated displayless workers for
injected adapter absence, real 2D/3D dimensions/field content, PNG decoding, exact
clear/linear-alpha-blend channel bytes, and software Vulkan when installed.
The shared suite never mutates display environment; ordinary worker returns
without GPU execution are not runtime proof.

#### `src/app/mod.rs` — application root and shared state

**Purpose.** Defines `VfApp`, `StatusBarState`, the two app-level constructors,
and the helpers that belong to no single frame phase.

**Public API.**
- `VfApp` (fields are mostly `pub` for the integration tests and sibling
  modules): `state`, `bar`, `sidebar`, `status`, `fps_meter`, `camera`, `orbit`,
  `fly`, `fly_keys`, `presets`, `style`, `layout`, `layout_mode`, `sheet`,
  `material` (`MaterialPanelState`), `clock`, `screenshot`, `animation`
  (`AnimationDialogState`), `animation_job`, and private
  scene/click/particle caches, including the
  `ode_lines`/`last_ode_epoch`/`ode_autofit_pending` Ode cache, the
  `scene_frame` key and particle-availability flag reconciled by
  [`src/app/plane.rs`](#srcappplaners--scene-frame-transitions).
- `VfApp::new()` and `VfApp::new_with_storage(Option<&dyn eframe::Storage>)`.
- `StatusBarState` (parse message, fps/frame ms, arrow/streamline/particle/trail
  counts, density, `is_3d`, sim time, flying, hint, animation progress).
- `presets_data_dir()` (native re-export of the XDG directory).
- `Default for VfApp`.

**Invariants.**
- `new_with_storage` restores `AppSettings` through the viewmodel setters before
  the first frame; a restored non-default domain is recentred on the correct
  camera (`recentre_on` in 3D, `recentre_on_2d` in 2D). It passes the restored
  reference flags into `compose_lines_with_frame` for the initial private scene;
  dirty rebuilds and click recompositions use that same frame-aware path.
- `MAX_CLICK_SEEDS = 6`: click seeds are oldest-first and the oldest is dropped
  when a seventh is added.
- `compose_lines_with_frame` prepends only the enabled reference groups (world
  or local slice) and then the cached flow and click segments; the status bar's
  streamline count is `flow_lines + click_lines`, not the full scene line count.
- `scene` is a cache; `State` remains the source of truth.
- `camera` is the 2D preset mirror (`Camera { position, scale }`), not the live
  painting camera; `orbit` is live.

**Pinned by.** `tests/persistence_test.rs` (round-trip and app restore),
`tests/status_bar_test.rs` (the readout type), `tests/fly_camera_test.rs`
(app-level fly/preset behavior), `tests/responsive_test.rs` (layout/status).
There is no dedicated `app_test.rs`; the shell itself is exercised through the
seams it calls.

**Limitations.** Click-seed history is app-local and bounded; it survives a
rebuild but not a restart. The app does not expose a general command/undo stack.

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
plot targets before the next vector frame.

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
  ViewRecord::capture(&state) }`. Cameras and fly state are intentionally absent.

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
`undo_last_click_seed`.

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
- ODE mode status reports `State::ode_error`; the Features controls are
  disabled with the ODE note and the Coloring section is not drawn. Materials
  status reports `State::material_error` or the current sampling warning; its
  Features controls are disabled with the arrows-only note and its Coloring
  section is hidden.
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
  with the backdrop by `app::compose_vector_lines`, which also appends
  mode-owned glyphs and reports their count. The Formula branch appends none.
  `source_glyph_line_count` and `viewport_line_count` expose the result to the
  doc-hidden integration harness.
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
Sources, Induction, Materials or Ode; `SourceDisplay` selects Electric,
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
cyan without summing or sharing one magnitude scale; a single family uses the
normal colormap over its drawn magnitudes. An enabled family with no sources is
`empty: true` with zero arrows and no error, and invalid scenes return their
typed message with no arrows. Counters track sources, drawn arrows, target-met,
accuracy-not-met total/drawn and unavailable per family.

**Pinned by.** [tests/source_scene_test.rs](../tests/source_scene_test.rs):
`the_default_scene_is_empty_in_formula_mode`,
`all_eight_kinds_have_valid_defaults_and_a_family`,
`add_remove_enable_and_reset_change_only_the_scene`,
`source_limit_stops_the_sixty_fifth_entry`,
`invalid_geometry_is_rejected_with_a_parameter_name`,
`softening_is_bounded_and_never_nan`, and
`disabled_entries_are_not_part_of_the_validated_scene`; source-mode behaviour is
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
cyan `[64,196,255]`; a single family uses the normal colormap magnitude range.
Native-only sampling may report the CAS waveform derivative string as
diagnostics; the induced field itself never depends on CAS.

**Pinned by.** [tests/induction_test.rs](../tests/induction_test.rs):
`waveform_and_static_predicate_are_exact`,
`loop_vector_potential_matches_independent_fixtures`,
`solenoid_vector_potential_matches_independent_fixtures_and_curl_matches_b`,
`faraday_contour_matches_flux_rate_for_both_cases`,
`static_current_has_exactly_zero_induced_field`,
`nonfinite_scene_is_rejected`, `validation_rejects_bad_geometry`, and the
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

#### `src/source_presets.rs` — built-in source-scene presets

**Purpose.** The ten illustrative source scenes offered above the saved camera
views in the Presets panel. Each is built from
[`SourceKindTag::default_source`](../src/source_scene.rs) with unit
charge/current, so the catalogue adds no new physics and no formula conversion.

**Public API.** `SourcePreset { name, summary, build }`; `source_presets()`;
`source_preset(name)`; `source_strength(&Source) -> f64`;
`load_source_preset(&SourcePreset, &mut State) -> bool`.

**Invariants.**
- The catalogue order is `Coulomb point`, `Electric dipole`, `Charged line`,
  `Uniform ring`, `Thin disc`, `Charged sphere`, `Infinite wire`,
  `Current loop`, `Finite solenoid`, `Source gallery`; names are unique and the
  gallery contains one entry of each of the eight kinds.
- Strengths are finite and illustrative: unit magnitude everywhere, with the
  dipole's charges at `+1`/`-1`. Every scene keeps the default `0.1` softening,
  `Both` display and `allow_inaccurate = false`.
- `load_source_preset` validates the built scene first, then uses the ordinary
  setters to select `PlotKind::VectorField`, `FieldMode::Sources`, a 3D view,
  the canonical `[-2, 2]^3` domain and the new scene. It returns false without
  touching the state when validation fails.
- Formula presets remain formulas; loading a source preset touches no saved
  formula or camera-preset storage.

**Pinned by.** [tests/source_presets_test.rs](../tests/source_presets_test.rs):
`source_presets_have_unique_names_and_cover_all_eight_kinds`,
`loading_a_source_preset_uses_real_setters_and_a_visible_domain`,
`every_preset_strength_is_the_illustrative_unit_and_finite`,
`legacy_formula_presets_keep_their_mode_and_label`, and
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
plane)` and `source_glyph_lines(glyphs, domain)`.

**Invariants.** Electric kinds appear only for `Electric`/`Both` and magnetic
kinds only for `Magnetic`/`Both`; disabled entries emit nothing. Family colours
are fixed E amber and B cyan, independent of the colormap. On-slice/3D lines use
alpha `0.9` and `0.005 ×` scene extent; off-slice lines use alpha `0.45`,
`0.0035 ×` extent and `4/3 × domain_scale` on/off dashes. With a `PlaneFrame`
the centre and axis are converted to local plane coordinates, `center.z` is
zero, and an out-of-plane source is flagged `off_slice` when its normal
distance exceeds `16 × f32::EPSILON × max(extent, 1.0)`. Marker geometry is
sized from `domain_scale = 0.05 × max(extent, 1e-6)` so a degenerate domain
still produces finite segments. Glyphs are contextual overlays, not
depth-tested surfaces.

**Pinned by.** [tests/source_glyph_test.rs](../tests/source_glyph_test.rs):
`glyphs_cover_every_kind_with_family_colour_and_sign`,
`negative_strengths_choose_the_minus_marker`,
`glyph_center_and_axis_follow_the_source`,
`family_selection_and_disabled_entries_filter_glyphs`, and
`off_slice_glyphs_project_and_are_visually_distinct`.

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

#### `src/flow.rs` — the tracer's velocity-field abstraction

**Purpose.** Give the streamline tracer one `Flow::eval(point, t)` seam over
either the world field or a projected slice.

**Public API.** `Flow`, `PlaneFlow { field, frame }`, `SceneFlow::{World,
Plane}`; `FieldExpr` implements `Flow`.

**Invariants.**
- `FieldExpr`'s flow delegates to `eval3_at` unchanged.
- `PlaneFlow` lifts local `(u, v, 0)` points through `PlaneFrame::to_world`,
  evaluates the world field, then projects the velocity with
  `PlaneFrame::project`, so a trace never leaves the slice.
- `SceneFlow` selects world flow outside a plane slice and plane flow inside
  one.

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

**Public API.** `ViewOptions { colormap, color_range }`, its `Default`;
`ArrowData { position, dir, mag, color }`; `magnitude_range`;
`sample_field`.

**Invariants.**
- Degenerate samples (non-finite field, magnitude < `1e-6`) are skipped.
- `dir` is always unit length; `mag` is the original magnitude.
- The colour range is the explicit `ViewOptions::color_range`, otherwise the
  min/max of the samples actually kept; an empty input gives `(0.0, 0.0)`.
- `magnitude_range` on an empty slice gives `(0.0, 0.0)`.

**Pinned by.** `tests/sampling_test.rs`: `constant_field_fills_grid`,
`all_zero_field_yields_no_arrows`, `sample_skips_nan_arrows`,
`color_follows_provided_range`, `magnitude_range_of_constant_field`.

**Limitations.** One full resample per dirty rebuild; no incremental sampling.
`sample_field` always uses the 3D evaluator, so a 2D grid is a single z slice.

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

**Pinned by.** `tests/presets_test.rs`: `every_preset_parses_with_declared_dimension`,
`every_preset_stays_finite_on_the_default_domain`, `preset_names_are_unique`,
`grouping_preserves_declared_category_order`, `softening_covers_the_origin`,
and the category/limit tests.

**Limitations.** Formulas are illustrative, not physical (Coulomb numerators are
not normalised). The preset formulas themselves are static spatial snapshots;
formulas may still reference `t` when the user edits them.

#### `src/colormap.rs` — scalar-to-RGB ramps

**Purpose.** Map a magnitude to a colour for arrows, streamlines, and
particles.

**Public API.** `Colormap { Viridis, Turbo, Mono }`, `ALL`, `label`,
`colormap(magnitude, min, max) -> [u8; 3]`.

**Invariants.**
- `max <= min` returns mid-grey `[128,128,128]`.
- Values clamp to the ramp ends; `Mono` is linear black→white.
- `Viridis`/`Turbo` come from `colorous`; `ALL` covers every variant with a
  unique label.

**Pinned by.** `tests/colormap_test.rs`: `degenerate_range_is_mid_gray`,
`out_of_range_clamps`, `viridis_endpoints`, `mono_endpoints`,
`offset_range_normalizes`, `all_covers_every_variant_with_unique_labels`.

**Limitations.** No custom/user ramps and no alpha; alpha is supplied by the
instance colour.

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
  `set_colormap`, `set_auto_fit`, `set_color_range`, `set_arrows`,
  `set_show_grid`, `set_show_coordinate_system`,
  `set_arrow_style`, `set_arrow_style_2d`, `set_arrow_params`,
  `set_streamlines`, `set_seed_mode`, `set_left_click_action`, `set_particles`,
  `set_particle_size`, `set_particle_count`, `set_particle_appearance`,
  `set_plane`, `set_plane_enabled`, `set_plane_axis`, `set_plane_index`.
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
  `snapshot_if_dirty`.
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

#### `src/persistence.rs` — one settings envelope

**Purpose.** Restore and save style, layout, and view state under one eframe
storage key, with migration and corruption tolerance.

**Public API.** `AppSettings { style, layout, view }`, `AppSettings::sanitized`;
`ViewRecord` (all fields, `capture`, `restore`, `restore_material_fields`,
`Default`); `finite_or`; `load`, `save`; private serde helpers.

**Schema (all under `eframe::APP_KEY`).**

| Record | Fields | Compatibility |
| --- | --- | --- |
| `AppStyle` | `theme_dark`, `accent: Color32`, `rounding`, `spacing`, `font_size` | required in the envelope so a bare ticket-07 style fails to decode and is migrated |
| `LayoutState` | `controls_expanded`, `presets_expanded`, `sources_expanded`, `controls_width`, `presets_width`, `sources_width`, `help_visible`, native `cas_expanded`, `cas_width` | `#[serde(default)]`; widths sanitized; Sources defaults collapsed/280 |
| `ViewRecord` | formula, mode, domain min/max, density, arrow scale/opacity/length mode/magnitude scale/proportional exponent, colormap, auto-fit and range, `show_grid`, `show_coordinate_system`, arrows, 3D/2D arrow style, arrow params, streamlines, seed mode, left-click action, particles, particle size/count/appearance (8 flat fields), `field_mode`, `source_entries`, `source_display`, `source_softening`, `source_allow_inaccurate`, `induction_case`, `induction_center`, `induction_axis`, `induction_radius`, `induction_length`, `induction_turns`, `induction_amplitude`, `induction_frequency`, `induction_display`, `induction_allow_inaccurate`, `ode_source`, `ode_dimension`, `ode_equations`, `ode_seeds`, `ode_duration`, `ode_chart`, `ode_variables`, `material_case`, `material_background_epsilon_r`, `material_background_mu_r`, `material_material_epsilon_r`, `material_material_mu_r`, `material_radius`, `material_interface_axis`, `material_interface_offset`, `material_applied`, `material_view`, legacy `normalize`, legacy `speed` | `#[serde(default)]` + per-field defaults/aliases; both reference flags have explicit field-level true-on-missing serde defaults (a missing bool otherwise becomes false); source fields default to Formula/empty/Both/`0.1`/false and the opt-in only accepts an exact `true`; induction fields default to the valid current-loop scene (`current_loop`, origin, +Z axis, radius 1, length 0, turns 1, amplitude 1, frequency 0.25, Both, opt-in false) and the opt-in only accepts an exact `true`; ODE fields default to the valid typed harmonic scene (`equations`, dimension 2, `y`/`-x`/`0`, one seed at `[1,0,0]`, duration 20, `phase`, `[true,true,false]`); Material fields default to the valid dielectric sphere (`dielectric_sphere`, εr 1/4, μr 1/1, radius 1, axis `z`, offset 0, applied `[1,0,0]`, view `total`); `normalize` and `speed` are read-only legacy |

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

**Tested by.** `tests/persistence_test.rs` (54 tests):
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
restore/sanitize tests.

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
`fov_y`, `ortho_zoom`) and methods; `MIN_DISTANCE`, `MAX_DISTANCE`,
`MIN_ORTHO_ZOOM`, `MAX_ORTHO_ZOOM`; `ViewCamera<'a>` (`Orbit`/`Fly`) with
`view`, `vp_matrix`, `eye_position`, `ndc_ray`, `is_fly`;
`apply_camera_record`.

**Invariants.**
- Spherical position has exactly `distance` from `target`; pitch clamps to
  ±1.55 to keep the up vector valid.
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
`tests/fly_camera_test.rs::view_camera_*`:
`ndc_ray_at_center_looks_at_target`, `target_maps_to_vp_center`,
`front_view_looks_down_negative_z_with_y_up`,
`two_d_vp_uses_the_front_view_not_the_orbit_view`,
`zoom_2d_recovers_from_a_non_finite_ortho_zoom`,
`recentre_on_targets_the_domain_centre_without_changing_the_pose`, etc.

**Limitations.** World up is fixed to +Y; there is no roll. The far plane is
100 world units. The 2D camera is orthographic only.

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
`KEY_PAN_FAST_MULTIPLIER = 3.0`, `KEY_ZOOM_FACTOR = 1.2`, `CAMERA_KEYS`;
`ViewContext`; `ViewCommand { Orbit, Pan, Zoom, Reset, Fit }`;
`domain_center`, `domain_centre_delta`, `fit_distance`, `apply_view_command`,
`apply_view_command_with_fly`, `command_for_key`, `view_key_commands`;
`CLICK_HINT_*`, `ClickActionPlan`, `click_action_plan`, `seed_shortcut`,
`shortcut_seed_point`.

**Invariants.**
- Orbit is inert in 2D; Pan uses world X/Y in 2D and the view basis in 3D;
  Zoom follows egui's “>1 = closer” convention.
- Reset returns default orientation/distance/zoom centred on the domain; Fit
  keeps orientation and fits the domain's bounding sphere in the tighter FOV
  (2D fits the padded plane and only zooms out for a narrow viewport).
- While flying, Reset/Fit leave fly first; orbit/pan/zoom commands are inert.
- Camera keys are ignored with Ctrl/Cmd/Alt; arrows orbit in 3D (Shift pans) and
  pan in 2D (Shift 3× faster); `+`/`=`, `-`, `R`, `F` map as documented.
- `click_action_plan`: seed requires Streamlines; particles require the feature
  and `mode_3d`; each failure returns its pinned hint.
- `seed_shortcut`: S is inert while flying (it is fly-backward); P is always
  available; repeats and non-plain modifiers are ignored.
- `shortcut_seed_point` uses the pointer only inside the viewport, otherwise the
  centre.

**Pinned by.** `tests/view_command_test.rs` (22 tests):
`fit_frames_square_and_rectangular_domains_deterministically`,
`arrow_keys_orbit_in_3d_and_pan_with_shift`,
`arrow_keys_pan_in_2d_and_shift_pans_faster`,
`left_drag_orbits_in_3d_and_pans_in_2d`,
`camera_keys_are_inert_while_a_text_field_has_focus`,
`command_modifiers_never_drive_the_camera`,
`reset_returns_the_default_framing_centred_on_the_domain`, and the undo/click
tests. The S/P mapping is also pinned by `tests/shortcut_test.rs`.

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
`AdvectSettings`, `advect`, `advance_exact`, `speed_range`, `dot_instances`,
`trail_instances`, `DOT_QUAD_CORNERS`, `dot_billboard_corners`.

**Invariants.**
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
- `dot_billboard_corners` mirrors the camera-facing quad used by both dot
  shaders.

**Pinned by.** `tests/particles_test.rs` (52 tests):
`spawn_is_reproducible`, `spawn_2d_mode_pins_the_plane`,
`outside_particle_respawns_inside`, `count_constant_under_vortex_advection`,
`resize_caps_at_the_particle_budget`,
`retarget_applies_a_changed_target_but_preserves_a_click_burst_above_it`,
`zero_step_leaves_particles_put`, `speed_is_displacement_over_dt`,
`stagnation_verdict_uses_speed_not_step_displacement`,
`trails_append_cap_and_keep_the_newest_point_last`,
`a_full_cloud_with_requested_32_stays_inside_the_segment_cap`,
`dots_colored_by_field_magnitude`, `fixed_dots_use_the_fixed_colour_and_speed_dots_use_speed_range`.
`tests/dot_billboard_test.rs` pins the billboard geometry.

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
`AXIS_WIDTH`; `reference_lines(domain, sampling_3d)`.

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

**Pinned by.** `tests/reference_lines_test.rs` (10 tests):
`three_d_reference_has_twelve_box_edges_subdivided_faces_and_three_axes`,
`three_d_reference_stays_inside_the_domain_and_draws_every_bound`,
`face_grid_is_drawn_before_the_box_edges_and_axes_last`,
`two_d_reference_is_one_flat_grid_on_the_mid_z_plane`,
`two_d_reference_has_coloured_x_and_y_axes_through_the_centre`.

**Limitations.** It is decoration only: no labels, ticks, or numeric axis
annotation, and no plane-mode dimming.

### 3.6 Rendering

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
- `DrawState` (pipelines, buffers, bind groups; `new`, `render_offscreen`,
  `render_offscreen_async`), `DrawCallback`, and the sort helpers
  (`sort_scene_back_to_front`, `sort_lines_back_to_front`,
  `sorted_lines_back_to_front`, `sorted_arrows_back_to_front`,
  `sort_back_to_front`).
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
- `record` follows `SceneGPU::draw_passes`: Lines → Trails → Scene3D (3D) or
  Arrow2D → Dots (2D). There is no depth attachment; ordering is the CPU sort.

**Invariants.**
- 3D uses one combined draw for arrows and particles, at the fixed 186-vertex
  budget. The shader derives `instance = vertex_index / 186`, reads
  `origin.w`, and exits unused invocations to one clip-space point before ring
  math.
- 2D uses the independent 2D family's 9/6 vertices and never reads `combined`.
- Empty scenes draw nothing; `SceneGPU::default()` is 3D with the cone family.
- Sort is stable far-to-near; a non-finite view depth becomes `NEG_INFINITY`
  (far group) instead of painting in front or panicking.
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

#### `src/export_provenance.rs` — source, induction, ODE and Materials export warning contract

**Purpose.** One capture-time provenance object feeds the baked PNG banner,
PNG `tEXt` chunks, the visible SVG warning and the SVG `<metadata>` block. It is
derived from the validated `SourceScene` plus cached `SourceSampling`, from the
durable `InductionScene` plus shared softening and cached
`InductionSampling`, from the durable `OdeScene` plus the already-computed
`OdeSolution` and shared clock, or from the durable `MaterialScene` plus cached
`MaterialSampling`; it never samples the field, re-integrates the ODE scene,
solves a material boundary-value problem or creates vector data.

**Public API.** `ExportProvenance { warning_lines, metadata, source_mode,
induction_mode, ode_mode, material_mode }`,
`source_export_provenance(mode, scene, sampling)`,
`induction_export_provenance(mode, scene, softening, sampling)`,
`ode_export_provenance(mode, scene, solution, error, timeline)`,
`material_export_provenance(mode, scene, sampling)`,
`provenance_lines`, `provenance_metadata`, and the exact ASCII copy constants
`MODEL_LINE`, `ACCURACY_LINE`, `UNAVAILABLE_LINE`, `INVALID_LINE`,
`NO_SAMPLING_LINE`, `INDUCTION_MODEL_LINE`, `INDUCTION_NO_SAMPLING_LINE`,
`INDUCTION_INVALID_LINE`, `ODE_MODEL_LINE`, `ODE_BOUNDED_LINE`,
`ODE_UNAVAILABLE_LINE`, `ODE_INVALID_LINE`, `ODE_TIMELINE_LINE`,
`MATERIAL_MODEL_LINE`, `MATERIAL_NO_SAMPLING_LINE`, `MATERIAL_INVALID_LINE`.

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
unsuppressed response/interface warnings and finite geometry).

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
the `take_*_with_provenance` variants. A Source-, Induction- or Ode-mode
request made without provenance derives the fallback instead of exporting
silently. Scalar plot captures always pass `None`.

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

Declares `branding`, native-only `cas_panel`, `formula_bar`, `help_overlay`,
`induction_panel`, `layout`, `menu`, `ode_panel`, `panel_rails`, `plot_panel`,
`presets`, `sidebar`, `sources_panel`, `status_bar`, `style_panel`, `top_bar`,
`transport`.
**Important:** `src/ui/arrows.rs` is present on disk but is *not* declared here,
so it is not compiled. Do not add UI code to it expecting it to run.

#### `src/ui/cas_panel.rs` — native symbolic tools

Native-only independent panel for transient scalar/vector formula input,
operation preview, copy, and explicitly applying validated vector results.
`VfApp::cas` owns scratch state; `app/panels.rs::show_left_panels` assembles its
rail/body independently of Controls, and Compact reuses the single sheet.
`tests/cas_rails_test.rs` exercises actual rail/menu/tab/Apply and resize paths.
Preview identity includes input, operation, variable, and the current app mode;
CAS scratch is not persisted. Formula mode keeps its historical authoritative
2D/3D app-mode dimension (not the field's arity). Sources mode has no formula
editor: `apply` refuses it before dimension/vector checks, and `show` disables
Apply with the exact explanation `CAS edits the formula field; switch to
Formula.` The preview dimension reads `State::active_dimension()` in Sources
mode without changing formula-mode semantics.

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
Magnetic / Both with the E amber / B cyan legend), Add combo and button,
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
- `status_text` reports hidden versus shown accuracy and unavailable counts per
  family without inventing counters; a scene validation error is shown readably.
- The Sources help line explains that streamlines/particles remain formula
  features. `Advanced` starts open so the opt-in and status are discoverable.

**Pinned by.** `tests/sources_panel_test.rs`
(`sources_panel_adds_removes_and_resets_through_real_widgets`,
`invalid_draft_does_not_replace_the_committed_source`,
`source_notice_is_rendered_and_survives_until_dismissed`,
`scene_error_text_names_the_displayed_row_and_parameter`,
`add_is_disabled_at_the_source_limit`), and
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
`rows_for`, `FORMULA_HINT`, `show`, `help_window`.

**Invariants.**
- `HELP_SECTIONS` is the single source of strings; the overlay shows the active
  view + Global, the window shows all three groups plus the Scalar plots,
  Sources and Induction sections. A static, not a const, so both surfaces can be
  proven to share the allocation.
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
`DEFAULT_SOURCES_WIDTH = 280`, `DEFAULT_INDUCTION_WIDTH = 280`,
`DEFAULT_ODE_WIDTH = 300`, `MIN_PANEL_WIDTH = 180`,
`MAX_PANEL_WIDTH = 460`; `COMPACT_WIDTH = 700`,
`SHEET_MAX_FRACTION = 0.55`, `VIEWPORT_MIN_FRACTION = 0.45`,
`VIEWPORT_CHROME_ALLOWANCE = 20`, `TAB_BAR_HEIGHT = 48`, `TOUCH_TARGET = 40`,
`FORMULA_MIN_FRACTION = 0.40`; `LayoutMode`, `SheetState`, `layout_mode`,
`CompactPlan`, `compact_plan`, `sheet_height`, `formula_field_width`,
`RailSide`, `rail_text_angle`, `rail_chevron`, `collapse_chevron`,
`rail_label_band`, `rail_chevron_pos`, `rail_tooltip`, `RailLabel`,
`LayoutState`, `clamp_width`, `rail`, `RailPanel`, `rail_panel`,
`collapse_button`, `collapse_button_named`.

**Invariants.**
- Compact applies at width ≤ 700; non-finite width stays Wide.
- Compact is transient: `LayoutState` stores eleven desktop fields (Controls,
  Presets, Sources, Induction and ODE visibility/width plus help) and
  native-only `cas_expanded`/`cas_width`; the sheet and mode are never persisted.
- `SheetState::Sources`, `SheetState::Induction` and `SheetState::Ode` share the
  one compact sheet.
- Sources and Induction default to collapsed at 280 pt, ODE at 300 pt, and all
  sanitize to 180..=460; each record is consulted only in its own field mode.
- `sheet_height` caps at 55 % of the window and keeps 45 % + 20 pt for the
  viewport; nonsense inputs yield 0.
- Rails: left label reads bottom-to-top, right top-to-bottom; chevrons point
  outward on a rail and inward on a collapse button; the rail is the only
  restore affordance; the real content band is narrower than the outer width.
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
visibility and measured rotated-label bounds. `fit_panel_widths` fits requested
widths by minimums and proportional excess without mutating persisted widths.
Production widget interaction, third-entry extensibility and width allocation
are pinned by `tests/panel_rails_test.rs`.

#### `src/ui/menu.rs` — the pure menu and shortcut tables

**Purpose.** Every menu item, shortcut, and the state/effect dispatch behind
them.

**Public API.** Shortcut label constants, `ABOUT_*`; `MenuAction` (13 variants)
and `ALL`; `SideEffect`; `MenuItem`; `Menu`; `MENUS`; `Shortcut`; `SHORTCUTS`;
`shortcut_for`, `shortcut_label`, `item_shortcut_label`, `shortcut_allowed`,
`shortcut_actions`, `help_key_pressed`; `UiState`; `apply_action`.

**Invariants.**
- Every `MenuAction` is reachable from a menu item or shortcut; no action has
  two menu homes. `MenuAction::ALL` is generated from the same macro as the
  enum, so the test cannot drift.
- Shortcut matching is exact: extra modifiers do not fire a plain shortcut.
- All shortcuts except Ctrl+Q are inert while a text field has focus.
- `MENUS`: File (screenshot, quit), Edit (copy formula, reset all controls),
  View (toggle controls, toggle presets, reset layout, reset camera,
  appearance), Help (keyboard & mouse, about). Fly is toggled by C and the
  sidebar Camera section, not a menu item.
- `apply_action` mutates only `UiState`; app-only work is returned as a
  `SideEffect` (open popover, show help/about, screenshot, quit, copy, reset
  ids/camera).
- `ResetAllControls` resets the Controls mirror, clears fly, reapplies, and
  resets the clock, but does not change the formula.
- `ResetLayout` resets the persisted layout and asks for fresh panel ids;
  `ResetCamera` leaves fly first and applies `ViewCommand::Reset`.
- `?` is a separate secondary help key; F1 remains the one table shortcut.

**Pinned by.** `tests/menu_test.rs` (26):
`every_enum_variant_is_reachable_from_a_menu_item_or_shortcut`,
`every_menu_item_is_backed_by_a_variant`,
`no_action_has_more_than_one_menu_home`,
`shortcuts_stay_out_of_text_fields_except_quit`,
`extra_modifiers_do_not_fire_the_plain_shortcuts`,
`reset_all_controls_returns_the_sidebar_knobs_without_touching_the_formula`,
`the_help_menu_entry_opens_the_full_help_window`.

**Limitations.** No user-rebindable shortcuts; no undo for control resets.

#### `src/ui/presets.rs` — field presets and saved camera views

**Purpose.** The right Presets panel: built-in field presets grouped by
category, a `Source presets` section, camera Reset/Fit buttons, and saved view
management.

**Public API.** `default_preset_dir` (native); `CAMERA_BUTTONS`; `PresetPanel`
with `new`, `new_with_dir` (native), `dir`, `presets`, `active_name`,
`active_field_name`, `active_source_name`, `pending_delete`, `take_applied`,
`clear_active`, `save_current`, `load`, `request_delete`, `cancel_delete`,
`delete`, `load_field`, `load_source_field`, `show`.

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

**Pinned by.** `tests/ui_preset_test.rs` (17):
`every_built_in_field_preset_loads_through_the_panel`,
`loading_a_field_preset_sets_its_formula_and_dimension`,
`classic_presets_return_to_formula_and_keep_the_source_scene`,
`loading_a_duplicate_formula_preset_highlights_only_the_clicked_name`,
`saving_highlights_the_row_and_a_duplicate_replaces_it`,
`deleting_waits_for_confirmation_then_clears_the_highlight`,
`a_load_hands_the_camera_to_the_live_view_exactly_once`,
`test_presets_survive_restart`;
`tests/config_test.rs::test_data_dir_honors_xdg_override`.

**Limitations.** Saved views are 2D camera records only; no 3D orbit presets.
No rename; name field is free text.

#### `src/ui/sidebar.rs` — the Controls panel

**Purpose.** Every scene knob (Display, Camera, Features, Coloring, Domain),
with a local mirror and a single `apply` handoff.

**Public API.** `Section` + `SECTIONS`, `domain_summary`, `DomainAxis`,
`domain_axes`, `PlaneControlMode`, `plane_control_mode`,
`magnitude_scale_enabled`, `arrow_scale_enabled`,
`proportional_exponent_enabled`, `ArrowStyleSet`, `arrow_style_set`,
`sides_control_enabled`, `head_control_enabled`, `seed_mode_enabled`,
`particle_size_enabled`, `particle_fixed_color_enabled`, `fixed_color_rgb`,
`fixed_color_opaque`, `trail_fade_enabled`, `trail_width_enabled`,
`stagnation_seconds_enabled`, `particle_count_label`, `Controls` (+ `new`,
per-section resets, `Default`), `show`, `show_with_plot_action`,
`show_compact`, `show_compact_with_plot_action`, `apply`.

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
  off, z row for a two-component field.
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

**Pinned by.** `tests/sidebar_controls_test.rs` (51), including
`untouched_frame_rebuilds_nothing_through_apply`,
`synced_controls_never_redirty_the_scene`,
`density_clamp_keys_to_sampled_dimension_through_apply`,
`reversed_drag_extrema_swap_through_the_seam`,
`nan_domain_edit_keeps_the_previous_extent_through_apply`,
`apply_never_commits_the_plane_mirror`,
`every_control_dirties_the_scene_through_apply`;
`tests/sidebar_sections_test.rs` (order/reset isolation),
`tests/sidebar_camera_ui_test.rs`, `tests/sidebar_colormap_ui_test.rs`.

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
`window_options()` (native), `#[doc(hidden)]`/`pub(crate)` `icon_data`.

**Invariants.**
- The window starts centred, resizable, at 1280×800, cannot shrink below
  960×600, and persists its size/position.
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

All four use the same bind group: `Globals` uniform + a read-only storage array
of instances. They generate geometry from `vertex_index`; there is no vertex
buffer. All are `TriangleList` with `cull_mode: None`, no depth attachment, one
sample, alpha blending.

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
| Persistence | `persistence_test.rs` | 39 |
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
