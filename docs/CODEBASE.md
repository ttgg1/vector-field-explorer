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
| Browser | `src/main.rs::start` (`#[wasm_bindgen(start)]`) | `eframe::WebRunner`, wgpu/WebGPU | eframe's browser storage + `localStorage` for camera presets |

The crate is both `cdylib` and `rlib` ([Cargo.toml](../Cargo.toml)), so the wasm
bundle and the integration tests can share it. The only external rendering
dependency is eframe's re-exported wgpu; the only evaluator is `meval`.

The field is formula-driven:

- `(u, v)` for a 2D field, `(u, v, w)` for 3D, each component an expression in
  `x`, `y`, `z`.
- Sampling, arrows, streamlines, particles, colormaps, plane inspection, two
  cameras (2D front / 3D orbit, plus an optional fly camera), camera presets,
  screenshots, and persistence are all in this tree.
- The app is a teaching-grade explorer, not a solver. Post-v1 physics work
  (sources, Maxwell, ODE) is described in [ADR 0003](adr/0003-physics-foundations.md)
  and is not implemented here.

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
         │  - compose_lines(reference_lines_visible(domain, is_3d,
         │      show_grid, show_coordinate_system) + flow lines + click lines)
        │  - cache dots_range from the freshly sampled magnitudes
        │  - retrace click bursts on every dirty rebuild
        ▼
 VfApp::frame_viewport (app/viewport.rs)
        │
        ├─ 3D: combined_instances(arrows, dots) + sort_scene_back_to_front
        ├─ 2D: sorted_arrows_back_to_front(arrows); combined stays empty
        ├─ trails: sorted_lines_back_to_front(trails) (3D only)
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
- The app owns reference lines (2D grid/axes or 3D face grid/box and axes)
  and clicks. The two independent `State` flags filter reference geometry
  before `compose_lines` appends flow and click lines; the snapshot's `lines`
  holds grid-seeded streamlines only. The resulting `SceneGPU.lines` is shared
  by both viewport modes and screenshot capture.

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
| Startup | `main` → `eframe::run_native` | `start` → `ensure_webgpu` → `WebRunner::start` |
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

Every file under `src/` is covered: 47 Rust files plus four WGSL shaders (51
files total). Test names are the function names printed by
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
- Wasm initializes `console_log`, checks `navigator.gpu` and awaits
  `requestAdapter()` before `WebRunner::new()`, reveals `#webgpu-unsupported`
  with the reason on failure, hides the panel on success, and removes `#loading`
  only after the runner starts.
- Both paths install `fonts::install` and construct
  `VfApp::new_with_storage(cc.storage)`.
- `show_startup_error` changes the loading overlay text rather than panicking.

**Pinned by.** `tests/version_test.rs::the_startup_log_reports_the_build_version`
(source-level check), `tests/web_assets_test.rs`, and
`scripts/check-web-bootstrap.sh` (not a Rust test; run by CI's wasm job).

**Limitations.** The browser matrix is manual; the gate only proves that
`navigator.gpu` and an adapter exist, not that every WebGPU feature the app uses
is available.

#### `src/app/mod.rs` — application root and shared state

**Purpose.** Defines `VfApp`, `StatusBarState`, the two app-level constructors,
and the helpers that belong to no single frame phase.

**Public API.**
- `VfApp` (fields are mostly `pub` for the integration tests and sibling
  modules): `state`, `bar`, `sidebar`, `status`, `fps_meter`, `camera`, `orbit`,
  `fly`, `fly_keys`, `presets`, `style`, `layout`, `layout_mode`, `sheet`,
  `clock`, `screenshot`, and private scene/click/particle caches.
- `VfApp::new()` and `VfApp::new_with_storage(Option<&dyn eframe::Storage>)`.
- `StatusBarState` (parse message, fps/frame ms, arrow/streamline/particle/trail
  counts, density, `is_3d`, sim time, flying, hint).
- `presets_data_dir()` (native re-export of the XDG directory).
- `Default for VfApp`.

**Invariants.**
- `new_with_storage` restores `AppSettings` through the viewmodel setters before
  the first frame; a restored non-default domain is recentred on the correct
  camera (`recentre_on` in 3D, `recentre_on_2d` in 2D). It passes the restored
  reference flags into `compose_lines` for the initial private scene; dirty
  rebuilds and click recompositions use that same composition path.
- `MAX_CLICK_SEEDS = 6`: click seeds are oldest-first and the oldest is dropped
  when a seventh is added.
- `compose_lines` prepends only the enabled reference groups and then the
  cached flow and click segments; the status bar's streamline count is
  `flow_lines + click_lines`, not the full scene line count.
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
  one bottom sheet.
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
  viewport width; the full Help window remains reachable from the menu.

**Pinned by.** `tests/layout_test.rs`, `tests/responsive_test.rs`,
`tests/persistence_test.rs` (restored domain framing),
`tests/ui_preset_test.rs` (camera handoff), `tests/sidebar_*` (Controls body).

**Limitations.** Compact mode is width-only (no height breakpoint); the help
overlay is desktop-sized. No test drives the whole panel assembly end to end.

#### `src/app/simulation.rs` — clock, particle step, scene rebuild

**Purpose.** The dirty rebuild and the per-frame particle simulation.

**Public API (crate-visible).** `advance_clock`, `frame_simulation`.

**Invariants.**
- `rebuild_scene` calls `snapshot_if_dirty`; `None` leaves the last scene
  untouched. On `Some`, it copies style/mode, arrows, trails, dots, combined,
  and recomputes `dots_range`. Grid streamlines are unwrapped from the snapshot;
  click bursts are retraced against the new domain/formula and then composed
  with the backdrop.
- `advance_particles` calls `retarget` only with the current target. A changed
  target trims or spawns only the difference; a click burst above the count
  survives until the count changes. Off/2D produces `None`, clears the cloud,
  and empties trails.
- Advection uses `clock.step_dt()`; a paused clock is a zero step and leaves the
  cloud exactly where it was.
- Dots and trails use the same `dots_range` and the same `speed_range` so colour
  modes agree.
- `transport::request_particle_repaint` keeps egui painting only while a
  consumer exists and the clock is playing.

**Pinned by.** `tests/viewmodel_test.rs` (dirty/rebuild, trails-empty snapshot),
`tests/particles_test.rs` (retarget/advect/colour/trail), and
`tests/particle_playback_test.rs` (clock/repaint).

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
  sorts only arrows and never reads `combined`; trails are 3D only.
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
- Evaluation binds `x`, `y`, `z`; unknown variables produce `None`.
- A 2D field evaluated at 3D `p` ignores `p.z` and returns `(u, v, 0)`.
- Any NaN/infinite component makes the whole evaluation `None`.

**Pinned by.** `tests/field_test.rs`: `parse_two_component`,
`parse_three_component`, `top_level_comma_only_must_not_split`, `err_on_invalid`,
`eval_nan_becomes_none`, `unknown_variable_not_evaluated`,
`three_component_evaluated_in_two_d`.

**Limitations.** No time variable (v0.1.0 is static), no implicit
multiplication (`2x` is invalid), and no symbolic differentiation.

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
dimension, category }`; `presets()`; `presets_in(category)`.

**Invariants.**
- Every formula parses and declares the right dimension.
- Every preset stays finite on the default domain (singular-looking fields are
  softened with `+0.05`).
- Names are unique; categories are ordered by `PresetCategory::ALL` and
  declaration order within a category.
- `presets_in` is the single grouping seam used by the panel and tests.

**Pinned by.** `tests/presets_test.rs`: `every_preset_parses_with_declared_dimension`,
`every_preset_stays_finite_on_the_default_domain`, `preset_names_are_unique`,
`grouping_preserves_declared_category_order`, `softening_covers_the_origin`,
and the category/limit tests.

**Limitations.** Formulas are illustrative, not physical (Coulomb numerators are
not normalised); there is no time dependence, and ElectroDynamics presets are
static spatial snapshots.

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
`active`, `count`; constants `DIM_FACTOR`, `DIM_GREY`, `DIM_ALPHA`;
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

**Limitations.** Plane dimming applies to arrows only. Streamlines, particles,
and reference lines are not sliced or dimmed.

### 3.3 Viewmodel and persistence

#### `src/viewmodel.rs` — state, setters, snapshot, input decisions

**Purpose.** The one pure seam for visible state and the pure camera/click
decisions that do not need a window.

**Public API (major).**
- Density/grid/plane constants: `DEFAULT_DENSITY_2D = 24`,
  `DEFAULT_DENSITY_3D = 16`, `MAX_DENSITY_2D = 200`, `MAX_DENSITY_3D = 40`,
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
  setters: `set_formula`, `set_mode`, `set_density`, `set_domain`,
  `set_arrow_scale`, `set_arrow_opacity`, `set_plane_dim_opacity`,
  `set_arrow_length_mode`, `set_magnitude_scale`, `set_proportional_exponent`,
  `set_colormap`, `set_auto_fit`, `set_color_range`, `set_arrows`,
  `set_show_grid`, `set_show_coordinate_system`,
  `set_arrow_style`, `set_arrow_style_2d`, `set_arrow_params`,
  `set_streamlines`, `set_seed_mode`, `set_left_click_action`, `set_particles`,
  `set_particle_size`, `set_particle_count`, `set_particle_appearance`,
  `set_plane`, `set_plane_enabled`, `set_plane_axis`, `set_plane_index`.
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
- `is_3d()` means `mode_3d && field.dimension == 3`; this gates the cube grid,
  density cap, plane mode, and streamline depth.
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
- `particle_target()` is `Some` only when `particles && mode_3d` (note: raw
  projection, not `is_3d`), so a flat formula in 3D mode still gets a cloud.
- `drag_channel`: middle always pans; secondary orbits in 3D unless Shift, pans
  in 2D; primary orbits in 3D and pans in 2D.
- `apply_camera_input`: 3D orbit/pan/zoom uses orbit distance and FOV; 2D pans
  world X/Y one-to-one at the visible height and multiplies `ortho_zoom`.
- `click_outcome`: double-click wins over click; a click only seeds if the
  gesture never became a drag. Captured clicks always seed the centre.
- `undo_click_seed`: only inside `DOUBLE_CLICK_WINDOW`, only removes an exactly
  matching last seed, and only truncates particles when the current count is at
  least the pre-click count.

**Pinned by.** `tests/viewmodel_test.rs` (66 tests) is the main contract:
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

#### `src/persistence.rs` — one settings envelope

**Purpose.** Restore and save style, layout, and view state under one eframe
storage key, with migration and corruption tolerance.

**Public API.** `AppSettings { style, layout, view }`, `AppSettings::sanitized`;
`ViewRecord` (all fields, `capture`, `restore`, `Default`); `finite_or`;
`load`, `save`; private serde helpers.

**Schema (all under `eframe::APP_KEY`).**

| Record | Fields | Compatibility |
| --- | --- | --- |
| `AppStyle` | `theme_dark`, `accent: Color32`, `rounding`, `spacing`, `font_size` | required in the envelope so a bare ticket-07 style fails to decode and is migrated |
| `LayoutState` | `controls_expanded`, `presets_expanded`, `controls_width`, `presets_width`, `help_visible` | `#[serde(default)]`; widths sanitized |
| `ViewRecord` | formula, mode, domain min/max, density, arrow scale/opacity/length mode/magnitude scale/proportional exponent, colormap, auto-fit and range, `show_grid`, `show_coordinate_system`, arrows, 3D/2D arrow style, arrow params, streamlines, seed mode, left-click action, particles, particle size/count/appearance (8 flat fields), legacy `normalize`, legacy `speed` | `#[serde(default)]` + per-field defaults/aliases; both reference flags have explicit field-level true-on-missing serde defaults (a missing bool otherwise becomes false); `normalize` and `speed` are read-only legacy |

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

**Tested by.** `tests/persistence_test.rs` (39 tests):
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
`a_partial_reference_record_defaults_only_the_missing_coordinate_flag`, and
the restore/sanitize tests.

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

**Purpose.** The only time source for animated features (particles today; future
time-dependent fields and plots).

**Public API.** `MAX_STEP = 1/20`, `MIN_SPEED = 0.1`, `MAX_SPEED = 3.0`,
`DEFAULT_SPEED = 1.0`; `SimClock { time, playing, speed, .. }` with `advance`,
`advance_for`, `step_dt`, `set_playing`, `toggle_playing`, `set_speed`,
`reset_time`, `reset`; `has_clock_consumer`, `transport_visible`,
`should_animate`.

**Invariants.**
- Non-finite/zero/negative frame times are ignored; a large frame clamps to
  `MAX_STEP`; the simulated step is `min(dt, MAX_STEP) * speed`.
- Paused frames produce a zero step and do not accumulate missed time.
- With no consumer, `advance_for` holds time and clears the step; re-enabling
  resumes from that time.
- Speed clamps to 0.1..=3.0; a non-finite value becomes 1×.
- The clock is transient: a fresh session and `Reset all controls` start at
  time 0, playing, 1×.

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
`seed_grid`.

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
`AdvectSettings`, `advect`, `speed_range`, `dot_instances`, `trail_instances`,
`DOT_QUAD_CORNERS`, `dot_billboard_corners`.

**Invariants.**
- The cloud has a constant size between target changes. A particle that leaves
  the domain, hits zero/non-finite flow, or becomes non-finite respawns inside
  the inset box; the count never drifts and never exceeds `MAX_PARTICLES`.
- Spawn/respawn jitter is deterministic from an inline Park/Miller LCG, so a
  fresh session reproduces the same cloud. `clear` rewinds the stream.
- `resize` trims from the end or spawns only the difference; existing particles
  keep their positions. `retarget` applies only a changed target, preserving a
  click burst above the configured count.
- `advect` is one RK4 step per particle per call; a non-finite/zero step is a
  no-op; speed is `|q - p| / dt`; trails append and trim to
  `effective_trail_len`; stagnation is judged on speed, not per-step
  displacement; a respawn resets trail/stuck/speed.
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
- `show_grid` independently gates the 2D lattice/3D face grids;
  `show_coordinate_system` gates 2D axes/3D box edges and axes. Both flags
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

#### `src/export.rs` — PNG encoding and camera-preset files

**Purpose.** PNG encode/write, SVG placeholder, and camera-preset JSON
persistence (native file or browser `localStorage`).

**Public API.** `encode_png`, `save_png` (native), `save_svg` (placeholder);
`Preset { name, position, scale }`, `presets_to_json`, `presets_from_json`,
`PRESETS_STORAGE_KEY`, `load_wasm_presets`, `save_wasm_presets`,
`storage_note`, and native `save_preset`, `delete_preset`, `load_presets`.

**Invariants.**
- `encode_png` rejects a frame whose length is not exactly `width * height * 4`
  and writes RGBA8.
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
`ensure_png_extension`, `spawn_save` (native), `finish_download` (wasm),
`Screenshot` (`new`, `with_channel`, `request`, `take_request`, `sender`,
`tick`, `fail`, `toast_text`, `toast`).

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

**Limitations.** The GPU readback itself is only exercised by the runtime smoke
test. The browser cap is lower (2048) because the PNG encode runs on the main
thread.

### 3.8 UI modules

#### `src/ui/mod.rs` — UI module list

Declares `branding`, native-only `cas_panel`, `formula_bar`, `help_overlay`,
`layout`, `menu`, `presets`, `sidebar`, `status_bar`, `style_panel`, `top_bar`,
`transport`.
**Important:** `src/ui/arrows.rs` is present on disk but is *not* declared here,
so it is not compiled. Do not add UI code to it expecting it to run.

#### `src/ui/cas_panel.rs` — native symbolic tools

Native-only Controls section for transient scalar/vector formula input,
operation preview, copy, and explicitly applying validated vector results.
Preview identity includes input, operation, variable, and the current app mode;
CAS scratch is not persisted.

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

#### `src/ui/formula_bar.rs` — formula text and 2D/3D toggle

**Purpose.** The top formula row.

**Public API.** `FormulaBarState { text }`, `FormulaBarState::new`, `show(ui,
state, bar, compact)`.

**Invariants.**
- The bar's text mirror is refreshed from `state.formula` when they differ, so
  a preset or restore updates the field.
- Every edit is handed to `state.set_formula`; the bar never parses or clamps
  itself.
- Clicking 3D/2D calls `state.set_mode`.
- Compact sets a 44 pt interaction height and uses
  `layout::formula_field_width` (≥ 40 % of the bar); wide lets the field fill
  the remainder.

**Pinned by.** There is no dedicated `formula_bar_test.rs`; the setter contract
is in `tests/viewmodel_test.rs` (`valid_formula_replaces_field_and_clears_error`,
`garbage_formula_keeps_field_and_reports_error`) and the width rule is in
`tests/responsive_test.rs::the_formula_field_keeps_at_least_forty_percent_of_the_bar`.

**Limitations.** No syntax highlighting, autocomplete, or history.

#### `src/ui/help_overlay.rs` — one help table, two surfaces

**Purpose.** The compact viewport overlay and the full Help › Keyboard & mouse
window.

**Public API.** Font-size constants, `HelpHeadingStyle`,
`section_heading_style`, `OVERLAY_HEADING_COLOR`, `window_heading_color`,
`HelpRow`, `HelpSection`, `FLY_HELP_*`, `SEED_HELP_*`, `LEFT_CLICK_HELP_*`,
`HELP_SECTIONS`, `all_rows`, `sections_for`, `rows_for`, `FORMULA_HINT`, `show`,
`help_window`.

**Invariants.**
- `HELP_SECTIONS` is the single source of strings; the overlay shows the active
  view + Global, the window shows all three groups. A static, not a const, so
  both surfaces can be proven to share the allocation.
- `?`/F1 toggle the overlay; Esc and the × dismiss it; the window has its own
  open flag.
- Headings and rows use pinned sizes/colours; the window heading is the theme's
  strong colour, the overlay heading is fixed light grey.
- The formula hint is not an input row.

**Pinned by.** `tests/help_overlay_test.rs` (14 tests):
`the_overlay_and_window_share_one_table`,
`the_overlay_shows_only_the_active_view_and_global`,
`every_binding_is_listed_exactly_once_in_its_group`,
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
`MIN_PANEL_WIDTH = 180`, `MAX_PANEL_WIDTH = 460`; `COMPACT_WIDTH = 700`,
`SHEET_MAX_FRACTION = 0.55`, `VIEWPORT_MIN_FRACTION = 0.45`,
`VIEWPORT_CHROME_ALLOWANCE = 20`, `TAB_BAR_HEIGHT = 48`, `TOUCH_TARGET = 40`,
`FORMULA_MIN_FRACTION = 0.40`; `LayoutMode`, `SheetState`, `layout_mode`,
`CompactPlan`, `compact_plan`, `sheet_height`, `formula_field_width`,
`RailSide`, `rail_text_angle`, `rail_chevron`, `collapse_chevron`,
`rail_label_band`, `rail_chevron_pos`, `rail_tooltip`, `RailLabel`,
`LayoutState`, `clamp_width`, `rail`, `RailPanel`, `rail_panel`,
`collapse_button`.

**Invariants.**
- Compact applies at width ≤ 700; non-finite width stays Wide.
- Compact is transient: `LayoutState` stores only the five desktop fields; the
  sheet and mode are never persisted.
- `sheet_height` caps at 55 % of the window and keeps 45 % + 20 pt for the
  viewport; nonsense inputs yield 0.
- Rails: left label reads bottom-to-top, right top-to-bottom; chevrons point
  outward on a rail and inward on a collapse button; the rail is the only
  restore affordance; the real content band is narrower than the outer width.
- `LayoutState::sanitized` clamps widths; `reset` restores first-launch layout
  (both panels expanded, help visible).
- `formula_field_width` never goes below 40 % of the bar and never returns a
  negative width.
- Compact buttons have a 40-point floor; the tab bar grows beyond its 48-point
  baseline when font-derived button content and frame margins require it.

**Pinned by.** `tests/layout_test.rs` (14):
`rail_paints_one_rotated_label_inside_the_real_band`,
`rotated_labels_fit_inside_the_real_rail_band`,
`rail_panel_is_outer_width_and_the_label_band_is_narrower`,
`layout_state_serializes_exactly_its_five_persisted_fields`,
`widths_clamp_and_corrupt_widths_fall_back`;
`tests/responsive_test.rs` (11):
`layout_mode_is_compact_at_and_below_700_points`,
`compact_plan_hides_the_side_panels_and_shows_the_tab_bar`,
`sheet_height_caps_at_55_percent_and_keeps_the_viewport_floor`,
`only_one_sheet_is_open_at_a_time`,
`two_finger_pan_maps_only_3d_multi_touch_translation`.

**Limitations.** Width-only breakpoint; the compact sheet does not persist which
tab was open; rail content band is deliberately narrow.

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
category, camera Reset/Fit buttons, and saved view management.

**Public API.** `default_preset_dir` (native); `CAMERA_BUTTONS`; `PresetPanel`
with `new`, `new_with_dir` (native), `dir`, `presets`, `active_name`,
`active_field_name`, `pending_delete`, `take_applied`, `clear_active`,
`save_current`, `load`, `request_delete`, `cancel_delete`, `delete`,
`load_field`, `show`.

**Invariants.**
- Built-in field presets load formula and dimension through the viewmodel
  setters; the highlight only appears when the clicked name matches the current
  formula and mode (duplicate formulas highlight one row).
- Saving with an existing name replaces it; the new row becomes active.
- Delete asks for confirmation; delete removes from list and disk (or
  localStorage) and clears the highlight/request.
- `take_applied` returns a loaded camera exactly once; the app applies it to
  the orbit camera and leaves fly.
- A missing preset name changes nothing and returns false.
- Native `new` logs a load failure and starts empty; wasm storage failure shows
  one note but keeps the in-memory list usable.

**Pinned by.** `tests/ui_preset_test.rs` (13):
`every_built_in_field_preset_loads_through_the_panel`,
`loading_a_field_preset_sets_its_formula_and_dimension`,
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
`domain_axes`, `magnitude_scale_enabled`, `arrow_scale_enabled`,
`proportional_exponent_enabled`, `ArrowStyleSet`, `arrow_style_set`,
`sides_control_enabled`, `head_control_enabled`, `seed_mode_enabled`,
`particle_size_enabled`, `particle_fixed_color_enabled`, `fixed_color_rgb`,
`fixed_color_opaque`, `trail_fade_enabled`, `trail_width_enabled`,
`stagnation_seconds_enabled`, `particle_count_label`, `Controls` (+ `new`,
per-section resets, `Default`), `show`, `apply`.

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
- Disabled controls are disabled for a reason: arrow scale in Magnitude,
  magnitude scale outside Magnitude, contrast outside Proportional, head/sides
  for headless families, seed mode with streamlines off, particle controls with
  particles off, trail controls below length 2, stagnation time with looping
  off, z row in 2D.
- Domain extents are clamped before `set_domain`; NaN keeps the previous value;
  reversed pairs swap in the seam and mirror back.
- Plane index commits on toggle/axis/click or slider release, not during a live
  drag, so dragging does not resample.
- The Camera section's fly checkbox is a mirror; the app syncs the real camera
  into it before drawing and reconciles a click through
  `fly_camera::sync_fly_mirror`.

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

- **Static fields only.** `meval` expressions have no time variable; Electrodynamics
  presets are spatial snapshots. Time animation is post-v1 (ADR 0003).
- **Formula-only field definition.** Sources and superposition exist only as a
  glossary/ADR plan, not code.
- **No solver.** No differentiation, divergence, curl, gradient, ODE, or Maxwell
  solver is implemented.
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
- Why physics is analytic-first, why `meval` is the only evaluator, and the
  post-v1 order: [ADR 0003](adr/0003-physics-foundations.md).
