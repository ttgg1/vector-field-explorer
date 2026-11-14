# Native CLI Plot Export — Design

## Intent, scope, and approaches

Add a native one-shot CLI for scripts that evaluates the existing meval field
and writes one static 2D SVG or 2D/3D PNG without eframe, a window, or display.
No arguments retain the existing GUI launch. PNG is wgpu-only; SVG is actual
CPU-generated vector geometry. Use existing `State`, `FieldExpr`, particle RK4,
sampling, and render semantics. Do not add CAS, physics, another evaluator,
animation/video, CPU PNG, or 3D SVG.

Approaches considered: (1) selected: native CLI adapter around pure app state
and scene seams, with separate CPU SVG and wgpu PNG backends; (2) reuse `VfApp`
and screenshot workflow, rejected because they involve window/UI/persistence;
(3) build a new field renderer, rejected because it duplicates current numerical
and visual behavior. The selected approach preserves no-args GUI behavior and
avoids semantic divergence.

## Entry, errors, and precedence

Native `main` initializes logging, then dispatches export only when CLI arguments
request it; no arguments continue through the existing `window_options` and
`run_native` path. Keep wasm `start()` unchanged and all CLI/dependency code
under `cfg(not(target_arch = "wasm32"))`. Help/version exit 0 without creating
a window. Parse/config/validation/formula/render/adapter/readback/write errors
print contextual stderr and exit nonzero. CLI does not construct `VfApp`, use
`eframe::Storage`, invoke persistence, or open a dialog.

Use clap derive and pollster 0.4 as native-only dependencies; pollster blocks
the native one-shot worker on wgpu's adapter/device futures without a new
executor abstraction. Check dependency license compatibility with the repo's
GPL-3.0-only policy. Use strict JSON via existing serde_json. Reject
unknown keys at every object level. Resolve defaults → config → explicitly
supplied flags; every flag is represented as `Option<T>` so explicit false/zero
is distinguishable from omission. Parse and validate every supplied config
value before overlay: an invalid config value fails even if a flag would replace
it. Then validate the fully merged record for mode-dependent and cross-field
rules. Nested objects merge member-by-member; omitted members preserve lower
precedence values. JSON `null` is accepted only for `color_range` and means
automatic range; otherwise null is a type error. No persistence envelope or
migration is used.

## Frozen CLI/config schema

The JSON object and flag mapping below are the complete supported one-shot
schema. All enum spellings are lowercase snake_case. Tuple flags use comma
separated numbers. Paths are `PathBuf` (no lossy UTF-8 conversion). `output`,
`width`, and `height` may be supplied in JSON or as flags; path is required
after merge. Missing width/height default independently to 1280/800. Output
extension `.svg`/`.png` (case-insensitive) selects format; other/missing
extensions fail. Saved native camera presets are not read because that would
access GUI persistence; static orbit and fly poses are configurable. Fly
movement keys, speed, capture and viewport input are transient GUI navigation,
not one-shot render settings.

| JSON key | CLI flags | Type / accepted values | Default / source |
|---|---|---|---|
| `output` | `-o, --output PATH` | path | required |
| `width`, `height` | `--width PX`, `--height PX` | integer; each 1..=4096, product <=16,777,216 | 1280, 800 |
| `mode` | `--mode 2d\|3d` | enum | 2d (mode chosen before `State::new`) |
| `formula` | `--formula EXPR` | string parsed/bound by meval | `State::new(mode)`: `(x, y)` / `(x, y, z)` |
| `domain.min`, `domain.max` | `--domain-min X,Y,Z`, `--domain-max X,Y,Z` | finite triples, each coordinate in −100..=100, min < max per axis | `Domain::default()` `[-1,1]³` |
| `density` | `--density N` | integer intrinsic range 4..=200; final `State::is_3d()` sampling max is 40, otherwise 200 | `State::new(mode)`: 24 / 16 |
| `arrows`, `streamlines`, `particles` | `--arrows BOOL`, `--streamlines BOOL`, `--particles BOOL` | bool | true / false / false from `State::new` |
| `arrow_scale` | `--arrow-scale N` | finite 0.1..=4 | 1.0 |
| `arrow_opacity` | `--arrow-opacity N` | finite 0.05..=1 | 1.0 |
| `plane_dim_opacity` | `--plane-dim-opacity N` | finite 0..=1 | 0.2 (`DIM_ALPHA`) |
| `arrow_length_mode` | `--arrow-length-mode proportional\|uniform\|magnitude` | enum | proportional |
| `magnitude_scale` | `--magnitude-scale N` | finite 0.01..=10 | 1 |
| `proportional_exponent` | `--proportional-exponent N` | finite 0.25..=4 | 1 |
| `colormap` | `--colormap viridis\|turbo\|mono` | enum | viridis |
| `color_range` | `--color-range auto\|MIN,MAX` | null/`auto` or finite pair with min < max | automatic |
| `arrow_style_3d` | `--arrow-style-3d cone\|cylinder\|flat\|streamline` | enum | cone |
| `arrow_style_2d` | `--arrow-style-2d flat\|streamline` | enum | flat |
| `arrow_params.head_length` | `--head-length N` | finite 0.1..=0.5 | 0.3 |
| `arrow_params.head_width` | `--head-width N` | finite 0.02..=0.3 | 0.12 |
| `arrow_params.shaft_radius` | `--shaft-radius N` | finite 0.01..=0.12 | 0.055 |
| `arrow_params.sides` | `--arrow-sides N` | integer 4..=16 | 12 |
| `streamline_seed_mode` | `--streamline-seed-mode auto\|click` | enum | auto |
| `plane.enabled` | `--plane-enabled BOOL` | bool | false |
| `plane.axis` | `--plane-axis x\|y\|z` | enum | z |
| `plane.index` | `--plane-index N` | nonnegative grid index, checked against final effective grid | middle index for z after final mode/formula/density; other axes middle if selected |
| `particle_count` | `--particle-count N` | integer 0..=4000; 0 is app Auto | 0 (density × 16, capped 4000) |
| `particle_size` | `--particle-size N` | finite 0.25..=3 | 1 |
| `particle_appearance.opacity` | `--particle-opacity N` | finite 0.05..=1 | 1 |
| `particle_appearance.color_mode` | `--particle-color magnitude\|speed\|fixed` | enum | magnitude |
| `particle_appearance.fixed_color` | `--particle-color-rgb R,G,B` | three u8 RGB values; CLI constructs current opaque RGBA color (alpha is not a supported particle control) | `[0,150,136]` |
| `particle_appearance.trail_len` | `--trail-len N` | integer 0..=32 (requested points; use current effective total-point budget) | 0 |
| `particle_appearance.trail_fade` | `--trail-fade N` | finite 0..=1 | 0.65 |
| `particle_appearance.trail_width` | `--trail-width N` | finite 0.05..=2 | 1 |
| `particle_appearance.loop_on_stagnation` | `--loop-on-stagnation BOOL` | bool | true |
| `particle_appearance.stagnation_seconds` | `--stagnation-seconds N` | finite 0.1..=5 | 0.5 |
| `camera.target` | `--camera-target X,Y,Z` | finite triple | output frame target defined below |
| `camera.kind` | `--camera-kind orbit\|fly` | enum; fly requires 3D mode | orbit |
| `camera.position` | `--camera-position X,Y,Z` | finite triple; fly only | `FlyCamera::from_orbit(OrbitCamera::default())` position |
| `camera.yaw` | `--camera-yaw RAD` | finite; unrestricted periodic angle | orbit 0.8; fly derived by `FlyCamera::from_orbit(default orbit)` |
| `camera.pitch` | `--camera-pitch RAD` | finite −1.55..=1.55 | orbit 0.9; fly derived by `FlyCamera::from_orbit(default orbit)` |
| `camera.distance` | `--camera-distance N` | finite 0.5..=20; orbit only | 3.2 |
| `camera.fov_y_degrees` | `--camera-fov-deg DEG` | finite (0,180), converted to radians; orbit only | 55 degrees; fly uses fixed `FlyCamera::FOV_Y` 55° |
| `camera.ortho_zoom` | `--camera-ortho-zoom N` | finite 0.05..=50; 2D only | 1 |
| `references.grid_and_domain_box` | `--references-grid-and-box BOOL` | bool; one coupled control, mapped to `State::show_grid` | true (`State::new`) |
| `references.world_axes` | `--references-world-axes BOOL` | bool; mapped to `State::show_coordinate_system` | true (`State::new`) |
| `background` | `--background-rgba R,G,B,A` | RGBA8 straight-alpha; converted explicitly per backend | egui dark panel fill gray 27, opaque; not hard-coded RGB 18,20,26 |
| `seed` | `--seed N` | unsigned 64-bit LCG state; zero is valid | `SPAWN_SEED = 0x123456789ABCDEF0` (preserves current GUI stream) |
| `start_time_seconds` | `--start-time SECONDS` | finite 0..=3600; endpoint plus duration must remain <=3600 | 0 |
| `duration_seconds` | `--duration SECONDS` | finite 0..=3600; endpoint plus start must remain <=3600 | 0 |
| `fixed_step_seconds` | `--fixed-step SECONDS` | finite; minimum 1e-4 and maximum 0.25 | 1/60 |

No separate `auto_fit` or `seed_mode`: `color_range` is the sole range control;
`streamline_seed_mode` is `State::seed_mode`. Missing `color_range` preserves
the lower-precedence range; explicit null/`auto` selects auto-fit; fixed pair
selects fixed range. In conflict between mode and all other config/flags, mode
is resolved first and then all remaining values are checked against that final
mode. Config default is every value in this table; instantiate from
`State::new(mode)` / source defaults rather than duplicate defaults in code.
Deserialize `color_range` as a three-state patch (`Missing`, explicit JSON
`Null`, or `Value([min,max])`): `Missing` inherits, `Null` selects automatic,
and `Value` selects fixed. Do not model it as plain `Option<T>`, which collapses
missing and null.
`streamline_seed_mode=click` has no automatic seeds in a one-shot export because
there is no click history/input. `left_click_action`, click-seed history,
keyboard/mouse actions, fly speed/capture and transport playback/speed are
explicitly GUI interaction state and are not export settings.
The CLI's three-byte `fixed_color` input maps to the existing four-byte
`ParticleAppearance.fixed_color` as `[r,g,b,255]`; alpha remains controlled only
by `particle_appearance.opacity`.

Reference controls are exactly ticket 77's merged two choices: grid plus
domain box as one switch (`State::show_grid` / `State::set_show_grid`), and
world axes independently (`State::show_coordinate_system` /
`State::set_show_coordinate_system`); both bools default to `true` in
`State::new` and `Controls::new`. The JSON/flag names above remain the CLI
schema, not Rust field names. Do not add a third domain-box toggle.
`reference::reference_lines_visible` is `pub(crate)`; the CLI scene owner uses
the existing public `app::compose_lines` rather than widening reference APIs.

## Validation and effective state

Perform intrinsic validation on every supplied config member before overlay:
JSON types/shape, unknown keys, enum membership, finite/range checks, tuple
arity, and formula parse/name binding. Density's intrinsic integer envelope is
4..=200; after merging mode and formula, the maximum is 40 when the resulting
`State::is_3d()` is true and 200 otherwise. For example, a 2D config with
density 100 overridden to `mode=3d` remains valid if the final formula has two
components (flat sampling), but fails if the final formula has three components.
Merge, select mode, parse/bind the final formula, construct `State::new(mode)`,
apply that formula before validating/applying density (then domain, plane,
remaining state controls, time, and camera/output checks). Never rely on setters
to clamp, swap, or retain old values.

Formula validation must bind all referenced free names to `x,y,z,t` (function
names remain the evaluator's recognized meval functions) without demanding
that evaluation be finite everywhere. Valid zero fields and valid singular
fields remain accepted; sample points with non-finite values are skipped by
existing semantics. Unknown names/functions fail before sampling so they do
not silently produce a blank image. Mode/dimension semantics are the existing
`State::is_3d`: mode 3D with a 2-component formula is allowed and samples a
single mid-z plane; particle eligibility remains `particle_target()` behavior
(requires particles enabled and 3D mode, including the existing flat-field-in-3D
case). Domain endpoints obey −100..=100 and strict min<max per coordinate.
Plane index uses final `State::grid()` axis length and is 0..length−1; inactive
2D plane controls follow existing inert behavior but still validate their
intrinsic types/ranges. Camera applicability is exact: 2D requires orbit and
allows target/ortho_zoom only; 3D orbit allows target/yaw/pitch/distance/FOV;
3D fly allows position/yaw/pitch and uses fixed FOV. Reject explicitly
supplied mode-inapplicable members rather than silently ignoring them. Pitch,
distance, FOV and orthographic zoom obey the table and source limits. Saved
presets are excluded because reading them violates the no-persistence CLI
contract. Reject singular/nonfinite view/projection matrices before rendering.

## Simulation and time-dependent field contract

Time is simulated seconds; existing `FieldExpr` binds x/y/z/t and existing
sampling, streamlines, RK4 particles, and color/instance builders accept time.
There is no separate loop-time setting in current source. GUI playback/speed is
not applicable: CLI duration is already simulated time and advances without
wall-clock pacing.

Initial time is `start_time_seconds`; require `start_time_seconds +
duration_seconds <= 3600`. Build the initial state/particle cloud at that time.
For each requested interval step, call existing `particles::advect`
once with `AdvectSettings.time = step_start_time` and `dt = step_dt`; its RK4
evaluations are at t, t+dt/2, t+dt/2, and t+dt. Full steps use the fixed step;
one shortened remainder step reaches the endpoint. Set `State.time` to
`(start_time + duration) as f32` before final arrow sampling, streamline snapshot, dot,
trail, and color construction, even if particles are disabled. Zero duration
does no advection but still makes the final snapshot at start time. This is
start-of-step integration, not the GUI's post-clock frame convention. Use f64
for schedule arithmetic and a checked integer step count. Compute the requested
endpoint in f64, then schedule full boundaries by indexed
`start_time + i * fixed_step` (not repeated f32 addition), stopping at the
endpoint; the last interval is shortened if necessary. For each interval
`[boundary_i, boundary_(i+1)]`, compute its f64 difference and hand
`AdvectSettings.time = boundary_i as f32` and
`AdvectSettings.dt = (boundary_(i+1) - boundary_i) as f32` to the existing RK4
path. Its actual stage times use f32 operations on those two arguments:
`t`, `t + dt*0.5`, `t + dt*0.5`, `t + dt`. Preflight the whole schedule before
spawn/advection: require finite f64 arithmetic and strictly advancing f64
boundaries; finite positive f32 `dt` and finite f32 boundaries; and strictly
increasing finite f32 `t < t + dt*0.5 < t + dt` for each step. Reject f32
boundary stagnation (`boundary_(i+1) as f32 <= boundary_i as f32`), even if
the actual RK4 end advances. Check indexed multiplication/addition and step
count for overflow; reject more than 100,000 steps. Do **not** require the
actual f32 RK4 end to equal the independently rounded f64 next boundary:
ordinary `duration=0.05, fixed_step=1/60` has three valid steps but its
third f32 sum differs from `0.05 as f32`. Each next step takes its own
scheduled boundary as `time`; this is not f32 accumulation. Set final
`State.time` to `(start_time + duration) as f32` (also preflight finite), even
when the last RK4 end differs by rounding. Thus the requested f64 endpoint
selects the snapshot time, not a promise that f32 RK4 lands numerically exactly
on that boundary. The duration/step combination could
otherwise request up to 36,000,000 steps; reject any pair exceeding the 100,000
cap. This is an operational work bound, not an accuracy or runtime
guarantee. Seed is only the existing particle LCG's spawn/respawn state; zero is
a valid state and it does not affect deterministic streamline seeds or field
math. Use an additive seeded constructor; preserve `Particles::default()` GUI
stream exactly by defaulting the CLI seed to its current `SPAWN_SEED` value.

Use current `effective_trail_len`: requested trail points are capped at 32 and
the global 100,000 stored-point budget yields 25 points at 4000 particles. Do
not reject a valid requested 4000×32 configuration or rename its semantics in
CLI; builder may report effective count in verbose diagnostics only.

## Shared output frame and scene ownership

Create one pure `OutputFrame` before backend selection containing width, height,
aspect, view, projection, view-projection, eye position, target, and background
interpretation. Both SVG and PNG consume exactly this frame. Defaults target the
domain center in 3D and `(center.x, center.y, mid_z)` in 2D. Orbit uses the
specified `OrbitCamera` pose. Fly mode starts from `FlyCamera::from_orbit` of
that default orbit pose, then explicit position/yaw/pitch replace individual
values. Fly mode is perspective-only with fixed `FlyCamera::FOV_Y` and rejects
orbit-only members. 2D uses front
orthographic view; set plane height to `max(domain_extent_x/aspect,
domain_extent_y) * 1.35`, so the full XY box plus the same 1.35 pad fits any
aspect. 3D uses the actual `OrbitCamera::default()` pose recentred on domain
center (yaw .8, pitch .9, distance 3.2, FOV 55 degrees); it does not silently
replace the app camera with `ViewCommand::Fit`. Explicit camera components
override only matching values. Large domains may be cropped at the camera's
bounded 20-unit maximum distance; the CLI does not invent an unbounded fit mode.
2D ignores yaw/pitch/distance/FOV for projection and uses only target and ortho
zoom; 3D ignores ortho zoom. No invented `front`/`isometric` pseudo-presets.
Use existing camera clipping planes (near 0.01, far 100 for perspective;
2D orthographic far is `distance + 100`); geometry outside those planes is
clipped, not used to silently adjust camera limits.

The background is straight RGBA8. SVG paints the same alpha as the rectangle;
do not force opaque. Use `Rgba8UnormSrgb` for the offscreen target if the
existing pipeline is compatible; wgpu clear values are linear, so decode the
configured sRGB bytes to linear before clear. Readback from the sRGB target
returns display-encoded channels matching the configured background bytes.
Convert linear scene-instance RGB to sRGB for SVG and preserve alpha in both
formats. Confirm clear/blend/readback behavior with an exact-color test.

Single scene owner (Task 3) builds final renderer-ready `SceneGPU`, not a partial
snapshot: set final state time, call `State::snapshot()` once (its `lines`
already contain grid-seeded streamlines), then replace its lines with
`app::compose_lines(scene.lines.as_ref(), &[], &state.domain, state.is_3d(),
state.show_grid, state.show_coordinate_system)`. This `#[doc(hidden)] pub` seam
is reachable through the public `app` module from the native CLI and integration
tests; it prepends selected reference lines once, then retains the supplied
flow lines followed by click lines. The CLI has no click history, so pass an
empty click slice; do not retrace via `streamlines_snapshot()` or call the
crate-private `reference_lines_visible` from an integration test. `show_grid`
owns the 3D face grid **and** domain box together. Construct particle cloud only if
`particle_target()` is `Some`; build dots/trails with existing at-time builders.
For 3D, combine arrow+dot instances with `combined_instances`, stable-sort by
descending positive camera-space depth (far-to-near) using the shared output
view; preserve original instance order for equal-depth ties. Sort trails
far-to-near with stable ties as well.
For 2D, leave `combined` unused, sort arrows as GUI does and particles remain
ineligible by `particle_target`. This owner returns `(SceneGPU, OutputFrame)`;
both exporters receive it. CPU-test populated arrow/dot combined scenes and
stable equal-depth sorting to prevent blank PNG scenes.

## SVG and PNG backends

SVG is CPU-only and rejects 3D before serialization. Existing
`screenshot::scene_to_svg` is a base serializer, not the complete 2D contract;
add a separate pure CLI serializer so current screenshot callers remain
unchanged. Emit actual field arrows separately from reference/streamline lines.
For 2D arrow style Flat, match `shaders.arrow2d.wgsl`: normalize direction
`(dir.x,dir.y)`; if its length is <1e-5 use +Y; shaft ends at
`(1-head_length)*length`, shaft half-width is `shaft_radius*length`, and head
half-width is `head_width*length`. Streamline uses a headless full-length shaft.
Project each world-space vertex through the output frame; world widths therefore
scale with orthographic framing as they do in the shader. Color RGB and alpha
come from instance RGBA. Apply
`arrow_style_2d`, parameters, scale already encoded in instance geometry,
opacity, particle radius/opacity, trail alpha/width, and line clipping. Clip
segments to the output rectangle; omit geometry wholly outside and clip partial
segments rather than allowing unbounded coordinates. Honors colormap through
precomputed instance colors. SVG tests distinguish arrow shapes, headless style,
changed parameters, alpha, width scale, clipping, visible/hidden references,
and portrait/landscape fit. This is not pixel identity with GPU output.

PNG directly requests an adapter/device with `wgpu 30.0.1` APIs (Cargo.lock),
using `pollster::block_on(Instance::request_adapter(...))` and then
`pollster::block_on(Adapter::request_device(...))`, with no surface/window/display.
Use only renderer-required features and request limits bounded by adapter
capabilities. Reuse `DrawState::new(&device, TextureFormat::Rgba8UnormSrgb)` and
`DrawState::render_offscreen(&mut self, device, queue, scene, view_proj,
camera_pos, width, height, clear)`: retain the existing mapping, polling, row
padding removal, unmapping and RGBA conversion, but make its native result
`Result<Vec<u8>, renderer::OffscreenError>`. Define native
`OffscreenError::{Setup(String), Readback(ReadbackError)}` and native
`ReadbackError::{Map(wgpu::BufferAsyncError), Poll(wgpu::PollError),
CallbackLost(std::sync::mpsc::RecvError), Range(wgpu::MapRangeError),
Decode(String)}`. Preserve the underlying typed cause in each readback variant
and display the stage and cause. Route the real `map_async` callback result
through `#[doc(hidden)] pub fn renderer::readback_map_status(status:
Result<(), wgpu::BufferAsyncError>) -> Result<(), renderer::ReadbackError>`;
this native-only production helper is callable from integration tests, not a
second test-only path. Classify submission/setup failures as `Setup` and
poll/receive/map/range/conversion failures as `Readback`; convert the typed
offscreen error into contextual `CliError::Render` or `CliError::Readback` via
the native production `#[doc(hidden)] pub fn cli::offscreen_error_to_cli(error:
renderer::OffscreenError) -> CliError`, also used directly by CLI PNG rendering.
Keep the sole native GUI caller in `src/app/viewport.rs` working with
`screenshot.fail(error.to_string())`; leave wasm async readback and its String
result unchanged. Require the adapter to report
`Rgba8UnormSrgb` as render-attachment-capable; if unavailable, return an
actionable error rather than silently changing color semantics. Create target
only after checking width/height, pixel bytes, texture dimensions, storage
buffer sizes and adapter/device limits. Capture validation error-scope around
resource/pipeline creation and submission, use uncaptured
error and device-loss handlers, and return contextual errors for no adapter,
request-device, validation/OOM/device loss, map/readback, and file write. No
CPU fallback. Real displayless adapter availability on release platforms is
mandatory implementation/runtime evidence, not a design unknown; unavailable
adapters remain a supported error case. The injected no-adapter path must run.

## Test contract and acceptance

Expose the native CLI seam as `#[cfg(not(target_arch = "wasm32"))] pub mod
cli` so native `main` and integration tests can use the same implementation.
The module's exact API is:

```rust
pub struct CliConfig { /* strict JSON fields from the schema table */ }
pub struct CliFlags { /* Option-valued flag counterparts for every config field */ }
pub struct CliOptions { pub output: PathBuf, pub width: u32, pub height: u32, /* all resolved schema settings */ }
pub struct OutputFrame {
    pub width: u32, pub height: u32, pub aspect: f32,
    pub view: glam::Mat4, pub projection: glam::Mat4, pub view_proj: glam::Mat4,
    pub eye: glam::Vec3, pub target: glam::Vec3,
    pub background_srgb: [u8; 4], pub background_linear: [f64; 4],
}
pub struct PreparedScene { pub scene: SceneGPU, pub frame: OutputFrame }
pub enum CliError { Parse(String), Config(String), Validation(String), Formula(String), Adapter(String), Device(String), Render(String), Readback(String), Write(String) }
pub fn parse_config(json: &str) -> Result<CliConfig, CliError>;
pub fn parse_flags(args: impl IntoIterator<Item = OsString>) -> Result<CliFlags, CliError>;
pub fn resolve_config(config: Option<CliConfig>, flags: CliFlags) -> Result<CliOptions, CliError>;
pub fn validate_intrinsic_config(config: &CliConfig) -> Result<(), CliError>;
pub fn validate_effective(options: &CliOptions) -> Result<(), CliError>;
pub fn output_frame(options: &CliOptions) -> Result<OutputFrame, CliError>;
pub fn prepare_scene(options: &CliOptions, frame: OutputFrame) -> Result<PreparedScene, CliError>;
pub fn scene_to_cli_svg(prepared: &PreparedScene) -> Result<String, CliError>;
pub fn render_offscreen(prepared: &PreparedScene) -> Result<Vec<u8>, CliError>;
pub fn run_export(options: CliOptions) -> Result<(), CliError>;
pub fn run_from(args: impl IntoIterator<Item = OsString>) -> Result<(), CliError>;
#[doc(hidden)] pub fn offscreen_error_to_cli(error: renderer::OffscreenError) -> CliError;
```

Use module-private implementation helpers. These are functional native CLI
interfaces, not test-only widened renderer APIs; integration tests may call the
pure config/frame/scene/SVG seams and exercise PNG through runtime/process tests.
The native renderer readback-status helper and native CLI error conversion above
are functional production interfaces shared by the renderer, CLI and integration
tests, not separate test-only APIs.

Tests live in integration `tests/*_test.rs`; `tests/suite.rs` is merged and
available for CLI test-module registration (coordinate any new concurrent edits).
Assertions must cover:

1. Parser help/version/no args, output extension/path errors, config output and
   dimensions, every flag-to-config field equivalence, explicit false override,
   nested partial merge, nested unknown keys, explicit null range, config
   invalid-before-override, and absence of conflicting duplicate aliases.
2. Every field/default/enum/range table entry; mode selected before baseline;
   invalid intrinsic values, final `State::is_3d`/domain/density/plane index
   validation ordering; density 100 with final mode 3D and two-component
   formula succeeds, while the same density with a three-component 3D formula
   fails.
3. Formula binding: unknown free variable/function rejected; valid zero field
   and valid singular field accepted and sampled with existing skip behavior.
4. Time-dependent field at nonzero start: particle RK4 trajectory uses
   `AdvectSettings.time` start-of-step and its intermediate times; final
   particle-free arrows/streamlines/color snapshot uses endpoint `State.time`.
   Test zero duration, exact multiple, fractional remainder, and the default
   nonintegral `duration=0.05, fixed_step=1/60` (three accepted steps despite
   last actual f32 RK4 end differing from independently rounded endpoint);
   assert f64 indexed boundaries, f32 start/dt and stage times, final snapshot
   at the rounded requested endpoint, tiny/zero-progress rejection before
   work, step cap rejection before work, and seeded initial state. Compare
   particles to repeated existing `advect` calls.
5. `effective_trail_len(4000,32)==25`; no CLI rejection of the valid requested
   pair. Exact output frame tests for asymmetric off-origin boxes, nonzero
   mid-z 2D domain, portrait/landscape, 1-pixel edges, finite matrices, camera
   component precedence, margins, and shared SVG/PNG frame/background
   interpretation.
6. Pure final-scene tests: snapshot streamlines only once, references once,
   2D particles absent, particle-only 3D, flat field in 3D, combined arrow/dot
   tags and counts, stable back-to-front sorting including ties, sorted trails.
7. SVG output contains field primitives (not merely reference lines), no image
   raster, maps Flat versus Streamline style, geometry params, opacity and
   projected width correctly, clipping, hidden references, deterministic
   bytes, and rejects 3D without adapter creation.
8. PNG decodes to requested dimensions and has non-background field content;
   injected adapter absence/device failures and an injected map callback failure
   are actionable. In `tests/cli_gpu_test.rs` import `eframe::wgpu` and call the same
   `renderer::readback_map_status(Err(wgpu::BufferAsyncError))` used by the
   actual native renderer callback; wrap its real `ReadbackError` in
   `OffscreenError::Readback` and pass it through the same
   `cli::offscreen_error_to_cli` used by CLI PNG rendering. Assert the typed
   `ReadbackError::Map` cause, `CliError::Readback` type and map stage/cause
   text; constructing `CliError` directly or testing a private/string-only
   translator is not injected renderer-path coverage. Exercise a
   real displayless software adapter where available via subprocess/runtime
   script or isolated process; do not mutate DISPLAY/WAYLAND_DISPLAY in shared
   test process and do not use ignored tests.
9. No persistence writes, ordinary GUI no-args smoke unchanged, native-only
   cfg/dependency isolation confirmed by wasm build/bootstrap check.

Required implementation gates: `cargo build`, `scripts/test.sh`,
`cargo clippy --all-targets -- -D warnings`, ordinary GUI smoke, displayless
PNG runtime exercise on release platforms where adapter support is available,
wasm build/web bootstrap check, and `scripts/smoke-run.sh 15` for renderer
changes (including this native readback seam). Runtime adapter/platform
evidence remains a future implementation gate; it is not claimed by this
design-only task.

## Non-goals and merged reference wiring

No GUI transient click history, click-seed actions, movement keys, transport
playback/speed, CAS, physics, future controls, general framework, or numerical
performance promise. Ticket 77's two reference visibility choices are merged:
use the existing state bools/setters and `app::compose_lines` as specified
above. The CLI schema remains unchanged; do not expose another public API just
for integration tests. The native readback helper and offscreen error converter
above serve production paths; no second test-only helper or inline source tests.
