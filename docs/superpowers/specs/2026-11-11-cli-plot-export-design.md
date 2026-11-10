# Native CLI Plot Export — Design

## Intent

Add a native, one-shot command-line route for scripts to evaluate the existing
meval field and save a static 2D SVG or 2D/3D PNG without starting eframe, a
window, or a display. No arguments must retain the ordinary GUI launch. PNG is
wgpu-only; SVG is genuine vector output generated on the CPU.

## Goals and non-goals

- Preserve `State`/`FieldExpr` and the existing particle advection and scene
  builders as the source of field/simulation semantics.
- Expose the controls implemented at implementation time through JSON config;
  offer concise flags for common choices and explicit flags for all options in
  the CLI schema. Precedence is app defaults → config values → supplied flags.
- Write exactly one requested image; reject invalid input and 3D SVG with a
  contextual error and nonzero exit. Never show dialogs or write GUI settings.
- Exclude CAS, new physics/evaluators, animation, CPU PNG, 3D SVG, and browser
  CLI behavior.

## Approaches

1. **Native CLI adapter around pure state/scene seams (selected).** Small CLI
   config/merge/validation layer dispatches into existing state and scene
   semantics; CPU SVG and isolated wgpu PNG paths remain separate. This best
   preserves GUI behavior and makes the no-GPU SVG guarantee testable.
2. **Reuse `VfApp` and screenshot workflow.** Rejected: it initializes app/UI
   persistence and screenshot capture is dialog/window-oriented.
3. **Build a separate field renderer/exporter.** Rejected: duplicates sampling,
   particle, and drawing behavior, and risks semantic divergence.

## Entry and error behavior

Keep wasm `start()` unchanged. Native `main` initializes logging as today, then
dispatches only when CLI arguments request a command; no arguments go through
the existing window path. Implement a native-only `cli` module and, if needed,
a narrowly scoped native offscreen adapter. `--help` and `--version` exit
successfully without a window. Parse/config/validation/evaluation/render/write
errors print one actionable message to stderr and return nonzero. CLI dispatch
does not construct `VfApp`, access `eframe::Storage`, initialize `window.rs`,
open `rfd`, or call persistence load/save.

Use clap derive for arguments and serde/serde_json for JSON config; any added
dependency is native-target-only and GPL-compatible. Config is strict JSON
(unknown keys rejected), with omitted fields left unspecified for precedence.
The exact config type is `CliConfig` with optional fields, serialized as a
single JSON object (no persistence envelope/version migration):

```json
{
  "mode": "2d", "formula": "(-y, x)",
  "domain": {"min": [-1, -1, -1], "max": [1, 1, 1]},
  "density": 24, "arrows": true, "streamlines": false,
  "seed_mode": "auto", "particles": false, "particle_count": 0,
  "particle_size": 1.0,
  "particle_appearance": {
    "opacity": 1.0, "color_mode": "magnitude",
    "fixed_color": [0, 150, 136, 255], "trail_len": 0,
    "trail_fade": 0.65, "trail_width": 1.0,
    "loop_on_stagnation": true, "stagnation_seconds": 0.5
  },
  "arrow_scale": 1.0, "arrow_opacity": 1.0,
  "plane_dim_opacity": 0.2, "arrow_length_mode": "proportional",
  "magnitude_scale": 1.0, "proportional_exponent": 1.0,
  "colormap": "viridis", "auto_fit": true, "color_range": null,
  "arrow_style": "cone", "arrow_style_2d": "flat",
  "arrow_params": {"head_length": 0.25, "head_width": 0.16, "shaft_radius": 0.025, "sides": 8},
  "plane": {"enabled": false, "axis": "z", "index": null},
  "streamline_seed_mode": "auto",
  "camera": {"target": [0, 0, 0], "yaw": 0.7, "pitch": 0.35,
             "distance": 6.0, "fov_y": 0.7853982, "ortho_zoom": 1.0},
  "references": {"grid": true, "domain_box": true, "world_axes": true},
  "background": [18, 20, 26, 255], "seed": 1,
  "duration_seconds": 0.0, "fixed_step_seconds": 0.016666667
}
```

Fields in this example communicate the schema shape; **builder must verify
exact names/defaults/bounds against merged source and ticket 77 before freezing
it**. In particular `arrow_params` keys and reference visibility API must match
the merged implementation. JSON optional members map only to supported fields;
unsupported/absent capabilities must not be claimed. Schema and CLI validation
are strict, fail-fast (unlike GUI setters that sanitize/clamp). `color_range`
is either null (auto-fit) or `[min,max]` finite with `min < max`.

### Arguments

`vector-field-explorer [OPTIONS]`:

| Flag | Meaning |
| --- | --- |
| `--config PATH` | Read the strict JSON object above. |
| `-o, --output PATH` | Required for export; output extension selects format (`.svg` or `.png`), case-insensitive. |
| `--mode 2d\|3d`, `--formula EXPR` | Selected mode and meval formula. |
| `--domain-min X,Y,Z`, `--domain-max X,Y,Z`, `--density N` | Sampling box and samples per active axis. |
| `--width PX`, `--height PX` | Output dimensions; both omitted means 1280×800. |
| `--arrows BOOL`, `--streamlines BOOL`, `--particles BOOL` | Feature toggles. |
| `--seed-mode auto\|click`, `--particle-count N`, `--particle-size N` | Implemented streamline/particle controls. |
| `--colormap viridis\|turbo\|mono`, `--arrow-scale N`, `--arrow-opacity N`, `--arrow-length proportional\|uniform\|magnitude` | Common visual controls. |
| `--camera-preset front\|isometric`, `--camera-target X,Y,Z`, `--camera-yaw N`, `--camera-pitch N`, `--camera-distance N`, `--camera-fov N`, `--camera-zoom N` | Deterministic camera/framing controls. |
| `--seed N`, `--duration SECONDS`, `--fixed-step SECONDS` | Particle state reproducibility contract. |
| `--help`, `--version` | Standard successful informational exits. |

Remaining currently implemented style, color-range, plane, particle appearance,
reference-visibility, and background controls are config-only in this iteration;
they are still first-class schema fields, not silently omitted. Enum spellings
are lowercase snake_case except the mode values shown. Reconcile reference
visibility flags/config keys with ticket 77 after it merges; do not implement
ticket 77 or freeze guessed API names independently.

### Defaults and validation

Default mode is 2D. Construct defaults from `State::new(mode)` so its formula,
density, plane, and all effective controls remain mode-sensitive and sourced
from the app. A format/output path is required unless informational help or
version was requested. If mode is omitted but a config has a mode, that mode
selects the `State::new` baseline before config overlay. Formula defaults are
`(x, y)` in 2D and `(x, y, z)` in 3D; density is 24 and 16 respectively;
domain is `[-1,1]³`; arrows on, streamlines/particles off; remaining view
defaults are those in `State::new` and the associated defaults (`ParticleAppearance`,
`ArrowParams`, camera and style types). Static formula evaluator is meval only;
there is no `t` variable.

Reject nonfinite floats, malformed/reversed/degenerate domain bounds, density
outside `MIN_DENSITY..=MAX_DENSITY_2D` (2D) or
`MIN_DENSITY..=MAX_DENSITY_3D` (effective 3D sampling), invalid enum values,
invalid formula, invalid control-specific ranges, output dimensions outside
1..=4096 per edge, and a pixel product greater than 16,777,216. Do not rely on
viewmodel clamping to make invalid CLI data succeed. Resource bounds also honor
existing `MAX_PARTICLES=4000`, `MAX_TRAIL_LEN=32`, total trail segment budget
100,000, and streamline point/length budgets; reject values above limits rather
than silently clamp. Mode/field dimension interactions follow `State::is_3d()`;
3D mode with a 2-component field is permitted as the existing flat-projection
case, but particle availability follows existing `particle_target()` semantics.
All paths are filesystem paths, not lossy UTF-8 conversions.

## Simulation contract

Expose `seed: u64` (default 1) as the spawn/respawn jitter seed only; current
randomness is the particle Park/Miller stream. Retain the GUI's current default
stream unchanged by making seeded construction additive, not changing
`Particles::default()` or GUI behavior. `duration_seconds` is finite simulated
seconds, default 0; it must be nonnegative and bounded to 3600 seconds. The
fixed step defaults to 1/60 s, is finite and in `(0, 0.25]`. Run full steps
until the next full step would exceed duration, then one shortened final step;
zero duration performs no advection. At each step, call existing `particles::advect`
once with that `dt`, current field/domain, and selected appearance settings;
do not use wall-clock time, GUI `SimClock::MAX_STEP` clamping, or a new
integrator. The field is static; `time` is the elapsed duration passed to the
existing dot/trail/color builders where needed, not a formula variable.
Simulation output is deterministic for equivalent config, build and platform.
Do not promise cross-platform bit-identical GPU PNG bytes; pure field/particle
state and SVG should compare exactly within same build/platform.

## Scene and output semantics

- Build arrows/streamlines using `State::snapshot()` and its existing sampling
  and streamline seams. Compose reference geometry using the merged
  `reference_lines` controls and ticket 77's independent grid/domain-box and
  world-axis visibility API; references default to their app-visible values.
- Build particle positions by seeded existing spawn/retarget/advect operations,
  then use existing dot/trail instance constructors. Keep renderer ordering and
  scene layout semantics aligned with the GUI where applicable.
- 2D camera is a deterministic front orthographic view centered on domain XY;
  3D defaults to a deterministic isometric orbit target at domain center. Camera
  values in config/flags override that mode default. No viewport input or
  preset filesystem lookup.
- Background defaults to the app's viewport clear color; expose RGBA8 explicitly
  in config (and `--background R,G,B,A` if promoted as a common flag). SVG uses
  an explicit opaque background rectangle for the selected background.
- SVG serializes CPU-built `SceneGPU` through `screenshot::scene_to_svg` with a
  deterministic output transform/fit and actual vector `<line>`/`<circle>`/
  polygon primitives; no embedded raster and no adapter initialization. Reject
  3D SVG before export.
- PNG creates a wgpu `Instance`, requests adapter/device directly without
  surface/window/display, initializes existing `DrawState`, renders to an
  offscreen texture and uses existing RGBA readback/`export::encode_png` or
  `save_png`. No adapter/device/render/readback failure becomes an actionable
  error; no CPU fallback. Verify exact wgpu API, limits, backend selection, and
  readback lifecycle against the resolved dependency before implementation.

## Testable boundaries

Suggested APIs (adjust only to match actual wgpu/merged ticket 77 interfaces):

```rust
pub struct CliConfig { /* optional schema members */ }
pub struct CliOptions { pub output: PathBuf, pub width: u32, pub height: u32, /* merged effective values */ }
pub fn resolve_config(default_mode: Mode, config: Option<CliConfig>, flags: ExplicitFlags) -> Result<CliOptions, CliError>;
pub fn validate_options(options: &CliOptions) -> Result<(), CliError>;
pub fn build_scene(options: &CliOptions) -> Result<SceneGPU, CliError>;
pub fn render_svg(scene: &SceneGPU, options: &CliOptions) -> Result<String, CliError>;
pub fn run_export(options: &CliOptions) -> Result<(), CliError>;
```

Keep types private unless needed across module boundaries; tests use public crate
APIs and do not justify broad API widening. Inject/narrowly abstract adapter
creation only if necessary to assert SVG does not request one and adapter errors
are testable. Tests stay in `tests/*_test.rs`, registered by the suite owner
after tickets 76/77 finish; coordinate rather than edit their live registration.

Required assertions:

1. Parser/help/version; no CLI args choose GUI dispatch; CLI dispatch chooses
   export without calling window/app constructors; wasm source remains gated.
2. JSON config parse, absent-field defaults, explicit flag precedence, omitted
   flags preserving config, unknown key/malformed JSON rejection, and bad config
   rejection even if a flag would override that field.
3. Invalid formula/enums/nonfinite values/domain ordering/edge and pixel caps,
   zero/oversized density, particle/trail bounds, missing output, and 3D SVG
   each produce nonzero, actionable errors with no dialog.
4. Mode-specific default formula/density from `State::new`, and 3D mode plus
   2-component formula semantics match `State::is_3d`/`particle_target`.
5. Fixed seed+duration produces equal final particle positions and exact SVG;
   another seed changes initial cloud; 0 s causes no advection; exact and
   fractional step boundaries reach precisely the requested total duration;
   compare with repeated existing `advect` calls.
6. SVG has vector primitives, no `<image>`/data raster, requested `viewBox` and
   background semantics; explicit adapter-call counter remains zero. 3D SVG
   rejects before serializer.
7. PNG decode reports exact requested dimensions. Conditionally exercise
   displayless software-adapter PNG when available; unavailable adapter returns
   a clear error. Do not mark GPU integration test ignored; use environment
   detection/skip reporting accepted by the test harness or a dedicated runtime
   smoke script.
8. CLI invocation writes no GUI persistence; ordinary GUI no-argument smoke
   still launches. Run native `cargo build`, `scripts/test.sh`, clippy, regular
   GUI smoke, displayless PNG runtime check where supported, and wasm build/gate
   to verify cfg/dependency isolation. Renderer changes additionally require
   `scripts/smoke-run.sh 15`.

## Implementation-time verification / open technical risk

These are preflight checks, not presumed guarantees: inspect merged ticket 77
for exact reference types/defaults and field names; inspect resolved wgpu API
version for headless adapter/device request and poll/readback APIs; prove each
release target's backend can create a displayless adapter; verify render target
format/features and limits at chosen dimensions; establish how to capture
validation/device-loss errors without swallowing them; verify SVG projection
does not emit invalid/off-canvas coordinates for asymmetric domains; verify
schema fields and range constants against current source. If a platform cannot
provide a usable adapter, report that limitation clearly—never add a fallback.

No CAS integration, physics work, generalized config framework, numerical
performance claims, animation, or speculative controls are authorized.
