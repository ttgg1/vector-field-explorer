# Native CLI Plot Export Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a native one-shot CLI that writes 2D SVG or 2D/3D PNG without a window/display while preserving no-argument GUI startup.

**Architecture:** Strict native CLI/config resolution creates a validated effective state and one CPU-testable output frame. One scene-preparation owner produces complete final `SceneGPU`; independent SVG and headless wgpu PNG backends consume that same frame and scene.

**Tech Stack:** Rust 2024, clap derive and pollster 0.4 (native-only), serde/serde_json, existing meval, glam, egui-wgpu/wgpu 30.0.1, image.

**Spec:** `docs/superpowers/specs/2026-11-11-cli-plot-export-design.md`

## Global Constraints

- CLI and any added dependency are native-only; wasm `start()` remains unchanged.
- No arguments keep ordinary GUI launch. CLI must not construct `VfApp`, use persistence, or open dialogs.
- Defaults → strict JSON config → explicitly supplied flags; config supplied values are validated before override; nested objects merge member-by-member.
- Output: one 2D SVG or 2D/3D PNG; no CPU PNG fallback, no 3D SVG, no new evaluator/CAS/physics/animation.
- Width/height each 1..=4096, product <=16,777,216. Start and duration are 0..=3600 s with endpoint <=3600; fixed step 1e-4..=0.25 s; no more than 100,000 steps.
- Seed is u64 and affects the existing particle RNG state only; zero is valid and default is `0x123456789ABCDEF0`, preserving the GUI stream. Time is simulated seconds, start at configured `start_time_seconds`, advect at start-of-step times, snapshot at endpoint.
- Preserve `effective_trail_len`: request max 32 points; 100,000 total stored-point cap means 25 points for 4000 particles.
- Shared output frame must drive SVG and PNG. Same-build/platform deterministic state/SVG; no cross-platform bit-identical GPU-byte promise.
- Reference controls use merged ticket 77's `State::show_grid` and `State::show_coordinate_system` bools (both default `true`), with the corresponding `set_show_grid` / `set_show_coordinate_system` setters. `show_grid` controls the 3D face grid and box together; JSON/flag names stay as specified.
- Tests live in `tests/`; `tests/suite.rs` ownership is released after the 76/77 merge, so register CLI modules there while coordinating any concurrent edits. No inline source tests, ignored tests, or broad test-only API.
- Required gates: `cargo build`, `scripts/test.sh`, `cargo clippy --all-targets -- -D warnings`, GUI smoke, wasm build/bootstrap check; renderer changes also `scripts/smoke-run.sh 15`; displayless PNG runtime evidence on release platforms is required implementation evidence.

## Review Focus

- **Time-dependent formulas:** assert start/intermediate RK4 times and final-time particle-free field/streamline snapshot (Task 3).
- **Config/flag contract:** assert parity for all controls, explicit false, partial nested overlay, strict nested unknown keys and `color_range:null` (Task 1).
- **Validation before/after merge:** assert unknown names rejected while singular/zero fields remain valid; invalid supplied config fails before override; density limits use final formula plus mode via `State::is_3d()`; plane/camera constraints are checked on final state (Task 1).
- **Bounded computation:** tiny steps and overflow/nonprogress reject before simulation; ordinary default nonintegral durations, exact/fractional times and valid 4000×32 requested trails preserve effective semantics (Task 3).
- **Visual/backend drift:** tests pin projected geometry, shared frame/color mapping, complete sorted 3D scene, and non-background PNG content (Tasks 2–4).

---

## File Structure

- `src/cli.rs`: native args, exact config types, overlay, validation, eventual dispatch/error reporting; declare as a native-only library module for the binary and integration tests. As each backend file arrives, declare it with `#[path = "cli_scene.rs"] mod cli_scene;` (and analogous paths for `cli_svg.rs` / `cli_gpu.rs`) and re-export the listed public API from `cli`.
- `src/cli_scene.rs`: `OutputFrame`, effective state construction, fixed-time simulation, complete renderer-ready scene and stable sorting. One owner for final scene.
- `src/cli_svg.rs`: pure CPU 2D SVG field geometry and serialization, without changing existing screenshot callers unexpectedly.
- `src/cli_gpu.rs`: direct wgpu 30.0.1 adapter/device, offscreen target, error scopes/readback; receives finalized scene/frame only.
- `src/main.rs`: native argument dispatch after all backends exist and before window options; wasm untouched.
- `src/lib.rs`: add `#[cfg(not(target_arch = "wasm32"))] pub mod cli;` as the real native CLI module interface, not a test-only API.
- `Cargo.toml`: add clap and pollster 0.4 only in native dependency table; confirm GPL-3.0-only compatibility.
- `src/particles.rs`: additive explicit initial seed seam, preserving GUI default.
- `src/reference.rs`: no edit planned; `reference_lines_visible` is `pub(crate)`. Use public `app::compose_lines` for CLI reference composition without widening visibility.
- `tests/cli_test.rs`, `tests/cli_scene_test.rs`, `tests/cli_svg_test.rs`: tests; `tests/cli_gpu_test.rs` or isolated subprocess/runtime script for displayless GPU.
- `tests/suite.rs`: register native-gated CLI test modules; ticket 76/77 ownership has been released by merge.
- `src/renderer.rs`: use existing `DrawState::new` and native `render_offscreen`; no renderer edit is planned.

## Interfaces

Keep helper functions private unless listed in this native CLI module interface. Exact seam contract:

```rust
pub struct CliConfig { /* strict JSON schema from spec */ }
pub struct CliFlags { /* parsed Option-valued flag counterparts for every config field */ }
pub struct CliOptions { pub output: PathBuf, pub width: u32, pub height: u32, /* all resolved schema settings */ }
pub struct OutputFrame {
    pub width: u32, pub height: u32, pub aspect: f32,
    pub view: glam::Mat4, pub projection: glam::Mat4, pub view_proj: glam::Mat4,
    pub eye: glam::Vec3, pub target: glam::Vec3,
    pub background_srgb: [u8; 4], pub background_linear: [f64; 4],
}
pub struct PreparedScene { pub scene: renderer::SceneGPU, pub frame: OutputFrame }
pub fn parse_config(json: &str) -> Result<CliConfig, CliError>;
pub fn parse_flags(args: impl IntoIterator<Item=OsString>) -> Result<CliFlags, CliError>;
pub fn run_from(args: impl IntoIterator<Item=OsString>) -> Result<(), CliError>;
pub fn resolve_config(config: Option<CliConfig>, flags: CliFlags) -> Result<CliOptions, CliError>;
pub fn validate_intrinsic_config(config: &CliConfig) -> Result<(), CliError>;
pub fn validate_effective(options: &CliOptions) -> Result<(), CliError>;
pub fn output_frame(options: &CliOptions) -> Result<OutputFrame, CliError>;
pub fn prepare_scene(options: &CliOptions, frame: OutputFrame) -> Result<PreparedScene, CliError>;
pub fn scene_to_cli_svg(prepared: &PreparedScene) -> Result<String, CliError>;
pub fn render_offscreen(prepared: &PreparedScene) -> Result<Vec<u8>, CliError>; // packed RGBA8
pub fn run_export(options: CliOptions) -> Result<(), CliError>;
```

`CliConfig` includes `output`, dimensions, mode/formula/domain/density; every
State control; complete particle appearance; all camera fields; two reference
controls; RGBA background; seed/start/duration/step. `CliFlags` has a one-to-one
explicit flag for every non-output config control (including each nested member),
as mapped in the spec. `color_range` uses `Auto | Fixed {min,max}` in effective
types; config missing means inherit, JSON null means `Auto`; no `auto_fit` alias.
`streamline_seed_mode` is the sole name for `State::seed_mode`.

## Tasks

### Task 1: Freeze and implement CLI/config contract

**Files:** `src/cli.rs`, `Cargo.toml`, `src/lib.rs`;
`tests/cli_test.rs`, merged `tests/suite.rs` registration.

**Produces:** exact config/flag types, parsing, overlay and validation APIs
above; no optional/unresolved schema fields. Mode is resolved before
constructing `State::new(mode)`. `run_from` and `run_export` are Task 6 APIs.

- [ ] **Step 1: Add parser/config contract tests.** Test clap help/version/no-args parse behavior (the binary's no-argument GUI routing is Task 6); JSON/flag output+dimensions; every spec table field has config and flag round-trip; explicit false wins; nested partial object leaves other values intact; nested unknown keys reject; `color_range` missing versus null/auto versus fixed range; no duplicate `seed_mode` alias; invalid config still fails when overridden by CLI; output extension/path errors; 2D config density 100 overridden to final mode 3D with final two-component formula stays valid (flat sampling), while the same density with a final three-component 3D formula fails; 2D rejects fly/perspective fields, 3D orbit rejects fly position, and 3D fly rejects orbit fields.
- [ ] **Step 2: Register only `cli_test` in the merged `tests/suite.rs` and run focused tests**; register later modules only when their files are created. Confirm expected failures and coordinate any concurrent registration edits.
- [ ] **Step 3: Add clap and pollster 0.4 native-only and implement strict nested serde schema/flags.** Confirm dependency licenses compatible with GPL-3.0-only. Enum types use only table spellings. All settings use Option presence; nested structs merge leaf-by-leaf. Deserialize `color_range` with a three-state `Missing | Null | Value([f32;2])` patch so explicit null is distinct from omission; null is accepted only here.
- [ ] **Step 4: Implement intrinsic pre-overlay validation.** Validate JSON type/unknown keys, tuple lengths, finite/range constraints, enums and formula parse/name binding for every provided value, even if later overridden.
- [ ] **Step 5: Implement effective validation and application order.** Resolve mode and final formula; construct `State::new(mode)`; parse/bind and apply formula before validating/applying density, then apply domain, plane, remaining controls, time and camera. Validate domain −100..=100 strict ordered bounds; density intrinsic 4..=200 then maximum 40 only when final `State::is_3d()` (otherwise 200); plane index against final grid; camera target/position finite, pitch/distance/FOV/zoom ranges and mode applicability from spec, and finite nonsingular matrices. Require start+duration <=3600. Reject unknown free formula names/functions while accepting valid zero/singular formulas. Do not rely on setter clamping.
- [ ] **Step 6: Re-run tests** for every field, override/null/validation ordering and parser help/version behavior; commit `feat: parse native plot export options` (no ticket marker).

### Task 2: Shared output frame and deterministic camera geometry

**Files:** `src/cli_scene.rs`, `src/cli.rs` (module declaration/re-exports), `tests/cli_scene_test.rs`, `tests/suite.rs` (register only this new test module).

**Produces:** `output_frame(&CliOptions) -> Result<OutputFrame, CliError>`.

- [ ] **Step 1: Add failing projection tests.** Use an asymmetric off-origin domain, a flat domain with z=40..42, portrait/landscape and 1-pixel edge dimensions. Assert finite matrices, orbit/fly pose and mode-specific errors, target at domain center or 2D mid-z, each 2D domain corner projects within framing margin, aspect is width/height. Test explicit camera component precedence and irrelevant 2D/3D fields; assert backend consumption of this frame in Tasks 4 and 5.
- [ ] **Step 2: Declare `cli_scene.rs` from `cli.rs` and implement default frame.** Re-export `OutputFrame`, `PreparedScene` and `output_frame`; define `PreparedScene` as the declared scene/frame carrier even though Task 3 populates it. Initialize `OrbitCamera::default()` (yaw .8, pitch .9, distance 3.2, FOV 55° radians, ortho zoom 1); recenter 3D on domain center and 2D on `(center.x,center.y,mid_z)`. 2D plane height `max(extent_x/aspect,extent_y)*1.35`; orbit 3D retains app default distance, not `ViewCommand::Fit`, and large domains can crop at max distance 20. Fly starts with `FlyCamera::from_orbit` from that orbit pose, with fixed 55° FOV, then explicit position/yaw/pitch overrides. Explicit mode-inapplicable camera options fail. Reject nonfinite/singular output transforms.
- [ ] **Step 3: Implement background transform** from panel-fill sRGB RGBA8 (gray 27 opaque default) to linear clear values; SVG stores same sRGB bytes and alpha. Preserve straight alpha. Test round-trip expected channel conversion.
- [ ] **Step 4: Re-run geometry tests** and commit `feat: define deterministic CLI output frame` (no ticket marker).

### Task 3: Seeded time-aware simulation and complete final scene

**Files:** `src/cli_scene.rs`, `src/particles.rs`, `tests/cli_scene_test.rs`; use the merged `src/app/mod.rs` composition seam without changing `src/reference.rs`.

**Produces:** `prepare_scene(options, frame) -> Result<PreparedScene, CliError>`.
This is the single owner of all final scene assembly. It calls `State::snapshot`
once; snapshot includes grid streamlines. Replace its line collection via
`app::compose_lines(scene.lines.as_ref(), &[], &state.domain, state.is_3d(),
state.show_grid, state.show_coordinate_system)` to prepend references exactly
once and preserve existing flow order; the CLI passes no click lines. The public
`#[doc(hidden)]` seam preserves flow-then-click order when clicks are supplied.
Do not retrace via a second `streamlines_snapshot` call or expose
`reference_lines_visible` (crate-private) to integration tests.

- [ ] **Step 1: Test scheduling through `prepare_scene` red.** Compare particle positions to an independently indexed f64 schedule and repeated existing RK4 `advect` calls for zero, exact multiple and fractional remainder. For `start=0, duration=0.05, fixed_step=1/60`, assert success with three reference intervals and matching particle positions at independently rounded f32 start boundaries and f32 interval differences; the last actual f32 `t+dt` differs from independently rounded `0.05` but succeeds. Assert final time-dependent scene samples the rounded requested endpoint. Reject tiny duration with f32 boundary stagnation or zero-progress RK4 midpoint, nonfinite/underflow, f64 boundary or count overflow, endpoint >3600, and >100,000 steps before spawn/advection. Confirm duration 3600/step 1e-4 rejects by step cap, not runtime promise; keep the schedule helper private.
- [ ] **Step 2: Add time-dependent behavior tests.** For field `(t, x+t, 0)` start 2 s, compare particle positions against repeated `advect` with `AdvectSettings.time` at each step start and RK4 intermediate stages. With particles off, assert arrows/streamline sample and final color range use `State.time=start+duration`; also test zero-duration snapshot uses start time.
- [ ] **Step 3: Add explicit particle RNG seed constructor** and test default `0x123456789ABCDEF0` reproduces the existing GUI stream, zero is accepted and a different seed changes positions. Keep `Particles::default()` behavior unchanged; seed is a u64 state for particle RNG only.
- [ ] **Step 4: Implement simulation loop** with preflight of at most 100,000 indexed f64 boundary intervals, f32-positive/finite dt, advancing boundaries and RK4 stage times. Pass each independently converted f64 start and interval difference as f32 `AdvectSettings.time`/`dt`; never enforce exact equality of actual f32 RK4 end with the independently rounded next boundary or accumulate f32 times. Spawn at start time, use full steps plus one shortened remainder, call existing `particles::advect` at each step start, and set final `State.time` to the f32-converted requested endpoint before all final scene sampling/instance/color builds, including with particles off or zero duration. Do not use wall-clock/transport speed.
- [ ] **Step 5: Add scene assembly tests.** Assert snapshot streamlines once, reference lines once; 2D no particles; particle-only 3D; 2D formula in 3D mode; combined arrow+dot tags/counts; stable far-to-near equal-depth order and trail order. Assert 4000 particles/requested trail 32 accepted and effective trail length 25.
- [ ] **Step 6: Implement complete renderer-ready scene.** Honor `particle_target`; use existing dots/trails and public `app::compose_lines` with both merged state visibility bools, supplying snapshot flow lines and empty clicks so reference lines appear once ahead of flow. No new reference API or domain-box toggle. 3D uses `combined_instances` and stable camera-depth sort; 2D sorts arrows and leaves combined unused.
- [ ] **Step 7: Re-run all pure tests** and commit `feat: prepare deterministic CLI plot scene` (no ticket marker).

### Task 4: CPU vector SVG matching supported 2D controls

**Files:** `src/cli_svg.rs`, `src/cli.rs` (module declaration/re-export), `tests/cli_svg_test.rs`, `tests/suite.rs` (register only this new test module). Do not modify the existing `screenshot::scene_to_svg` contract; the CLI serializer owns the required 2D-style mapping.

**Produces:** `scene_to_cli_svg(&PreparedScene) -> Result<String, CliError>`;
reject 3D before serialization and never initialize wgpu.

- [ ] **Step 1: Write mapping tests.** Require field-arrow primitives separately from backdrop; assert Flat emits a filled head, Streamline emits headless geometry; changing head length/width changes shape; opacity changes alpha; projected world width responds to view scale; alpha/color maps to the sRGB display values; particle/trail widths/fades map; clipping wholly/partially offscreen geometry works; hidden references vanish; portrait/landscape frame corners fit. Assert SVG has no `<image>`.
- [ ] **Step 2: Implement the separate pure CLI geometry serializer.** Match `arrow2d.wgsl`: normalize XY direction (fallback +Y when length <1e-5), shaft endpoint `(1-head_length)*length`, shaft half-width `shaft_radius*length`, head half-width `head_width*length`; Streamline is headless and full-length. Project every world vertex through shared `view_proj`; convert linear RGB to sRGB and preserve alpha; clip line/polygon geometry to output rectangle. Keep `screenshot::scene_to_svg` caller behavior unchanged.
- [ ] **Step 3: Test deterministic SVG and 3D rejection** through the pure serializer, verifying field primitives exist when reference geometry is empty; Task 6 checks that the wired SVG CLI works without an adapter.
- [ ] **Step 4: Re-run SVG tests** and commit `feat: export styled native vector plots` (no ticket marker).

### Task 5: Headless wgpu PNG backend

**Files:** `src/cli_gpu.rs`, `src/cli.rs` (module declaration/re-export), `tests/cli_gpu_test.rs` or isolated runtime script; register a new suite module in `tests/suite.rs` only when its test file exists.

**Produces:** `render_offscreen(&PreparedScene) -> Result<Vec<u8>, CliError>`;
packed RGBA8 frame with same frame/color/background semantics as SVG.

- [ ] **Step 1: Add pure limit/error tests** for requested dimensions, pixel bytes, texture dimensions, storage-buffer sizes, adapter limits, injected no-adapter/device failure; no test changes process `DISPLAY`/`WAYLAND_DISPLAY` in shared suite.
- [ ] **Step 2: Implement wgpu 30.0.1 request path** with `pollster::block_on(Instance::request_adapter(...))` then `pollster::block_on(adapter.request_device(...))`, no surface/window. Request only renderer-required features/adapter-bounded limits. Require `Rgba8UnormSrgb` render-attachment format capability; fail clearly rather than use another format. Validate output sizes before allocation.
- [ ] **Step 3: Reuse exact existing offscreen renderer/readback.** Call `DrawState::new(&device, TextureFormat::Rgba8UnormSrgb)` and `render_offscreen(device, queue, scene, view_proj, camera_pos, width, height, clear)`. Existing path maps, polls, strips row padding, unmaps and converts RGBA. Add validation error scopes around creation/submission plus uncaptured-error/device-loss reporting; preserve actionable no-adapter/device/OOM/validation/map/readback/write errors. Never fallback to CPU and do not modify `renderer.rs`.
- [ ] **Step 4: Run isolated displayless adapter subprocess/runtime check.** Decode output dimensions and assert pixels contain non-background field content. Exercise injected adapter absence even if no software adapter exists. Report real adapter unavailable honestly; do not ignore tests.
- [ ] **Step 5: Commit** `feat: render native CLI PNG offscreen` (no ticket marker).

### Task 6: User docs and complete verification

**Files:** `src/cli.rs` (`run_from`/`run_export`), `src/main.rs` (native dispatch), `README.md` or `docs/cli.md` (choose one discoverable location); task tests as needed.

- [ ] **Step 1: Write failing native dispatch tests.** Assert no arguments retain GUI route, help/version exit without a window, SVG/PNG choose the correct backend, and each error yields contextual stderr and a nonzero status; exercise CLI-vs-config dispatch and absence of persistence/dialog writes.
- [ ] **Step 2: Implement `run_from` and `run_export` in `src/cli.rs` and native dispatch in `src/main.rs`.** After logger setup and before `window_options`, choose export only when arguments request it; preserve no-args GUI and wasm `start()` unchanged. Route final scene to pure SVG or GPU PNG and save to the explicit output path.
- [ ] **Step 3: Document exact schema/flags and examples:** 2D SVG, 3D PNG, full flag/config parity, null/overlay precedence, modes/defaults, camera/time units, step limits, color/background semantics, GPU-only PNG, no dialogs/persistence, no-args GUI.
- [ ] **Step 4: Exercise native CLI manually.** Generate SVG and inspect actual field vector geometry; generate displayless PNG where supported and inspect non-background pixels/dimensions; test malformed input, 3D SVG, adapter absence, unwritable and non-UTF8 paths; verify no persistence changes and ordinary GUI no-args smoke.
- [ ] **Step 5: Run gates:** `cargo build`; `scripts/test.sh`; `cargo clippy --all-targets -- -D warnings`; `scripts/smoke-run.sh 15` for runtime/renderer path. Dedicated wasm/browser checks are owner-waived, not passed. Keep runtime checks distinct from compile/test evidence.
- [ ] **Step 6: Self-review** against ticket 78, spec, merged ticket-77 names, all review findings 1–8, and AGENTS. Check merged suite registration and ensure no ignored test, no CAS/physics, no future/transient controls.
- [ ] **Step 7: Commit** docs/fixes with `feat:`/`docs:`/`fix:` subject without ticket marker; provide gate/runtime evidence.

## Review-Finding Coverage

1. Existing t-aware fields and start-step/final snapshot semantics: Task 3.
2. Full explicit flag + config parity for every current control: Task 1.
3. Frozen canonical defaults/types/enums/null/nested rules; two merged reference
   choices wired through `State` and `app::compose_lines`: Task 1 and spec schema.
4. Intrinsic versus merged validation, variable binding, singular fields, mode,
   domain, plane and camera rules: Task 1.
5. Step bound/precision/progress and effective trail cap semantics: Task 3.
6. SVG 2D styles/head params/opacity/projected width/clipping: Task 4.
7. Shared frame/camera/aspect/mid-z/background color contract: Task 2 and Task 4/5.
8. Single owner of final camera-dependent combined/sorted scene and nonblank PNG:
   Task 3 and Task 5.
