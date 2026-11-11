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
- Reference controls are ticket 77 semantics: coupled grid+domain-box visibility and independent world axes, both visible. Only exact Rust wiring waits for 77 merge.
- Tests live in `tests/`; coordinate suite registration with 76/77; no inline source tests, ignored tests, or broad test-only API.
- Required gates: `cargo build`, `scripts/test.sh`, `cargo clippy --all-targets -- -D warnings`, GUI smoke, wasm build/bootstrap check; renderer changes also `scripts/smoke-run.sh 15`; displayless PNG runtime evidence on release platforms is required implementation evidence.

## Review Focus

- **Time-dependent formulas:** assert start/intermediate RK4 times and final-time particle-free field/streamline snapshot (Task 3).
- **Config/flag contract:** assert parity for all controls, explicit false, partial nested overlay, strict nested unknown keys and `color_range:null` (Task 1).
- **Validation before/after merge:** assert unknown names rejected while singular/zero fields remain valid; invalid supplied config fails before override; mode/plane/camera constraints are checked on final state (Task 1).
- **Bounded computation:** tiny steps and overflow/nonprogress reject before simulation; exact/fractional times and valid 4000×32 requested trails preserve effective semantics (Task 3).
- **Visual/backend drift:** tests pin projected geometry, shared frame/color mapping, complete sorted 3D scene, and non-background PNG content (Tasks 2–4).

---

## File Structure

- `src/cli.rs`: native args, exact config types, overlay, validation, dispatch/error reporting; declare as a native-only library module for the binary and integration tests.
- `src/cli_scene.rs`: `OutputFrame`, effective state construction, fixed-time simulation, complete renderer-ready scene and stable sorting. One owner for final scene.
- `src/cli_svg.rs`: pure CPU 2D SVG field geometry and serialization, without changing existing screenshot callers unexpectedly.
- `src/cli_gpu.rs`: direct wgpu 30.0.1 adapter/device, offscreen target, error scopes/readback; receives finalized scene/frame only.
- `src/main.rs`: native argument dispatch before window options; wasm untouched.
- `src/lib.rs`: add `#[cfg(not(target_arch = "wasm32"))] pub mod cli;` as the real native CLI module interface, not a test-only API.
- `Cargo.toml`: add clap and pollster 0.4 only in native dependency table; confirm GPL-3.0-only compatibility.
- `src/particles.rs`: additive explicit initial seed seam, preserving GUI default.
- `src/reference.rs`: wire existing lines to ticket 77's merged two-control interface; do not add ticket 77 functionality.
- `tests/cli_test.rs`, `tests/cli_scene_test.rs`, `tests/cli_svg_test.rs`: tests; `tests/cli_gpu_test.rs` or isolated subprocess/runtime script for displayless GPU.
- `tests/suite.rs`: register test modules only after ticket 76/77 release exclusive ownership.
- `src/renderer.rs`: use existing `DrawState::new` and native `render_offscreen`; no renderer edit is planned.

## Interfaces

Keep helper functions private unless listed in this native CLI module interface. Exact seam contract:

```rust
pub struct CliConfig { /* strict JSON schema from spec */ }
pub struct CliFlags { /* parsed Option-valued flag counterparts for every config field */ }
pub struct CliOptions { pub output: PathBuf, pub width: u32, pub height: u32, /* all resolved schema settings */ }
pub struct OutputFrame {
    width: u32, height: u32, aspect: f32,
    view: glam::Mat4, projection: glam::Mat4, view_proj: glam::Mat4,
    eye: glam::Vec3, target: glam::Vec3,
    background_srgb: [u8; 4], background_linear: [f64; 4],
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

**Files:** `src/cli.rs`, `src/main.rs`, `Cargo.toml`, `src/lib.rs`;
`tests/cli_test.rs`; suite registration after ownership release.

**Produces:** exact config/flag types and APIs above; no optional/unresolved
schema fields. Mode is resolved before constructing `State::new(mode)`.

- [ ] **Step 1: Add parser/config contract tests.** Test help/version/no args; JSON/flag output+dimensions; every spec table field has config and flag round-trip; explicit false wins; nested partial object leaves other values intact; nested unknown keys reject; `color_range` missing versus null/auto versus fixed range; no duplicate `seed_mode` alias; invalid config still fails when overridden by CLI; output extension/path errors; valid 2D density 100 overridden to final 3D fails final-mode check; 2D rejects fly/perspective fields, 3D orbit rejects fly position, and 3D fly rejects orbit fields.
- [ ] **Step 2: Run focused tests** after suite registration ownership is released; confirm expected failures. Until then keep new tests unregistered in this worktree and coordinate registration serially.
- [ ] **Step 3: Add clap and pollster 0.4 native-only and implement strict nested serde schema/flags.** Confirm dependency licenses compatible with GPL-3.0-only. Enum types use only table spellings. All settings use Option presence; nested structs merge leaf-by-leaf. Deserialize `color_range` with a three-state `Missing | Null | Value([f32;2])` patch so explicit null is distinct from omission; null is accepted only here.
- [ ] **Step 4: Implement intrinsic pre-overlay validation.** Validate JSON type/unknown keys, tuple lengths, finite/range constraints, enums and formula parse/name binding for every provided value, even if later overridden.
- [ ] **Step 5: Implement effective validation and application order.** Resolve mode; construct `State::new`; apply domain, density, formula, then plane, remaining controls, time and camera. Validate domain −100..=100 strict ordered bounds; density intrinsic 4..=200 then final mode maximum; plane index against final grid; camera target/position finite, pitch/distance/FOV/zoom ranges and mode applicability from spec, and finite nonsingular matrices. Require start+duration <=3600. Reject unknown free formula names/functions while accepting valid zero/singular formulas.
- [ ] **Step 6: Dispatch from native main** after logger setup and before `window_options`; no args use existing GUI route. `--help`/`--version` exit before any window creation. Do not touch wasm `start()`.
- [ ] **Step 7: Re-run tests** for every field, override/null/validation ordering and no-args dispatch; commit `feat: parse native plot export options` (no ticket marker).

### Task 2: Shared output frame and deterministic camera geometry

**Files:** `src/cli_scene.rs`, `tests/cli_scene_test.rs`.

**Produces:** `output_frame(&CliOptions) -> Result<OutputFrame, CliError>`.

- [ ] **Step 1: Add failing projection tests.** Use an asymmetric off-origin domain, a flat domain with z=40..42, portrait/landscape and 1-pixel edge dimensions. Assert finite matrices, orbit/fly pose and mode-specific errors, target at domain center or 2D mid-z, each 2D domain corner projects within framing margin, aspect is width/height, and the same `OutputFrame` values are consumed by SVG/PNG. Test explicit camera component precedence and irrelevant 2D/3D fields.
- [ ] **Step 2: Implement default frame.** Initialize `OrbitCamera::default()` (yaw .8, pitch .9, distance 3.2, FOV 55° radians, ortho zoom 1); recenter 3D on domain center and 2D on `(center.x,center.y,mid_z)`. 2D plane height `max(extent_x/aspect,extent_y)*1.35`; orbit 3D retains app default distance, not `ViewCommand::Fit`, and large domains can crop at max distance 20. Fly starts with `FlyCamera::from_orbit` from that orbit pose, with fixed 55° FOV, then explicit position/yaw/pitch overrides. Explicit mode-inapplicable camera options fail. Reject nonfinite/singular output transforms.
- [ ] **Step 3: Implement background transform** from panel-fill sRGB RGBA8 (gray 27 opaque default) to linear clear values; SVG stores same sRGB bytes and alpha. Preserve straight alpha. Test round-trip expected channel conversion.
- [ ] **Step 4: Re-run geometry tests** and commit `feat: define deterministic CLI output frame` (no ticket marker).

### Task 3: Seeded time-aware simulation and complete final scene

**Files:** `src/cli_scene.rs`, `src/particles.rs`, ticket-77 `src/reference.rs` call site after merge; `tests/cli_scene_test.rs`.

**Produces:** `prepare_scene(options, frame) -> Result<PreparedScene, CliError>`.
This is the single owner of all final scene assembly. It calls `State::snapshot`
once; snapshot includes grid streamlines. It adds reference lines once, and does
not retrace via a second `streamlines_snapshot` call.

- [ ] **Step 1: Test scheduling helper red.** Pin f64 step count/elapsed sequence for zero, exact multiple, fractional remainder; f32 stage-time handoff; reject nonfinite/underflow, any f32 start/mid/end stage that does not advance, endpoint >3600, and >100,000 steps before advection. Confirm duration 3600/step 1e-4 rejects by step cap, not runtime promise.
- [ ] **Step 2: Add time-dependent behavior tests.** For field `(t, x+t, 0)` start 2 s, compare particle positions against repeated `advect` with `AdvectSettings.time` at each step start and RK4 intermediate stages. With particles off, assert arrows/streamline sample and final color range use `State.time=start+duration`; also test zero-duration snapshot uses start time.
- [ ] **Step 3: Add explicit particle RNG seed constructor** and test default `0x123456789ABCDEF0` reproduces the existing GUI stream, zero is accepted and a different seed changes positions. Keep `Particles::default()` behavior unchanged; seed is a u64 state for particle RNG only.
- [ ] **Step 4: Implement simulation loop** with at most 100,000 scheduled calls, initial spawn at start time, `dt` full step plus shortened remainder, existing `particles::advect` at step-start time, final `State::set_time(start+duration)` before all final scene sampling/instance/color builds. Do not use wall-clock/transport speed.
- [ ] **Step 5: Add scene assembly tests.** Assert snapshot streamlines once, reference lines once; 2D no particles; particle-only 3D; 2D formula in 3D mode; combined arrow+dot tags/counts; stable far-to-near equal-depth order and trail order. Assert 4000 particles/requested trail 32 accepted and effective trail length 25.
- [ ] **Step 6: Implement complete renderer-ready scene.** Honor `particle_target`; use existing dots/trails and reference composition. Ticket 77 handoff is only exact visibility type/method wiring; preserve two choices and defaults without adding domain-box toggle. 3D uses `combined_instances` and stable camera-depth sort; 2D sorts arrows and leaves combined unused.
- [ ] **Step 7: Re-run all pure tests** and commit `feat: prepare deterministic CLI plot scene` (no ticket marker).

### Task 4: CPU vector SVG matching supported 2D controls

**Files:** `src/cli_svg.rs`, `tests/cli_svg_test.rs`. Do not modify the existing `screenshot::scene_to_svg` contract; the CLI serializer owns the required 2D-style mapping.

**Produces:** `scene_to_cli_svg(&PreparedScene) -> Result<String, CliError>`;
reject 3D before serialization and never initialize wgpu.

- [ ] **Step 1: Write mapping tests.** Require field-arrow primitives separately from backdrop; assert Flat emits a filled head, Streamline emits headless geometry; changing head length/width changes shape; opacity changes alpha; projected world width responds to view scale; alpha/color maps to the sRGB display values; particle/trail widths/fades map; clipping wholly/partially offscreen geometry works; hidden references vanish; portrait/landscape frame corners fit. Assert SVG has no `<image>`.
- [ ] **Step 2: Implement the separate pure CLI geometry serializer.** Match `arrow2d.wgsl`: normalize XY direction (fallback +Y when length <1e-5), shaft endpoint `(1-head_length)*length`, shaft half-width `shaft_radius*length`, head half-width `head_width*length`; Streamline is headless and full-length. Project every world vertex through shared `view_proj`; convert linear RGB to sRGB and preserve alpha; clip line/polygon geometry to output rectangle. Keep `screenshot::scene_to_svg` caller behavior unchanged.
- [ ] **Step 3: Test deterministic SVG and 3D rejection** and assert adapter-request seam count stays zero; verify field primitive exists when reference geometry is empty.
- [ ] **Step 4: Re-run SVG tests** and commit `feat: export styled native vector plots` (no ticket marker).

### Task 5: Headless wgpu PNG backend

**Files:** `src/cli_gpu.rs`, `tests/cli_gpu_test.rs` or isolated runtime script.

**Produces:** `render_offscreen(&PreparedScene) -> Result<Vec<u8>, CliError>`;
packed RGBA8 frame with same frame/color/background semantics as SVG.

- [ ] **Step 1: Add pure limit/error tests** for requested dimensions, pixel bytes, texture dimensions, storage-buffer sizes, adapter limits, injected no-adapter/device failure; no test changes process `DISPLAY`/`WAYLAND_DISPLAY` in shared suite.
- [ ] **Step 2: Implement wgpu 30.0.1 request path** with `pollster::block_on(Instance::request_adapter(...))` then `pollster::block_on(adapter.request_device(...))`, no surface/window. Request only renderer-required features/adapter-bounded limits. Require `Rgba8UnormSrgb` render-attachment format capability; fail clearly rather than use another format. Validate output sizes before allocation.
- [ ] **Step 3: Reuse exact existing offscreen renderer/readback.** Call `DrawState::new(&device, TextureFormat::Rgba8UnormSrgb)` and `render_offscreen(device, queue, scene, view_proj, camera_pos, width, height, clear)`. Existing path maps, polls, strips row padding, unmaps and converts RGBA. Add validation error scopes around creation/submission plus uncaptured-error/device-loss reporting; preserve actionable no-adapter/device/OOM/validation/map/readback/write errors. Never fallback to CPU and do not modify `renderer.rs`.
- [ ] **Step 4: Run isolated displayless adapter subprocess/runtime check.** Decode output dimensions and assert pixels contain non-background field content. Exercise injected adapter absence even if no software adapter exists. Report real adapter unavailable honestly; do not ignore tests.
- [ ] **Step 5: Commit** `feat: render native CLI PNG offscreen` (no ticket marker).

### Task 6: User docs and complete verification

**Files:** `README.md` or `docs/cli.md` (choose one discoverable location); task tests as needed.

- [ ] **Step 1: Document exact schema/flags and examples:** 2D SVG, 3D PNG, full flag/config parity, null/overlay precedence, modes/defaults, camera/time units, step limits, color/background semantics, GPU-only PNG, no dialogs/persistence, no-args GUI.
- [ ] **Step 2: Exercise native CLI manually.** Generate SVG and inspect actual field vector geometry; generate displayless PNG where supported and inspect non-background pixels/dimensions; test malformed input, 3D SVG, adapter absence, unwritable and non-UTF8 paths; verify no persistence changes and ordinary GUI no-args smoke.
- [ ] **Step 3: Run gates:** `cargo build`; `scripts/test.sh`; `cargo clippy --all-targets -- -D warnings`; `scripts/smoke-run.sh 15` after renderer changes; wasm build and web bootstrap check. Keep runtime checks distinct from compile/test evidence.
- [ ] **Step 4: Self-review** against ticket 78, spec, merged ticket-77 names, all review findings 1–8, and AGENTS. Check test registration ownership and ensure no ignored test, no CAS/physics, no future/transient controls.
- [ ] **Step 5: Commit** docs/fixes with `docs:`/`fix:` subject without ticket marker; provide gate/runtime evidence.

## Review-Finding Coverage

1. Existing t-aware fields and start-step/final snapshot semantics: Task 3.
2. Full explicit flag + config parity for every current control: Task 1.
3. Frozen canonical defaults/types/enums/null/nested rules; two reference choices
   with only ticket-77 Rust wiring deferred: Task 1 and spec schema.
4. Intrinsic versus merged validation, variable binding, singular fields, mode,
   domain, plane and camera rules: Task 1.
5. Step bound/precision/progress and effective trail cap semantics: Task 3.
6. SVG 2D styles/head params/opacity/projected width/clipping: Task 4.
7. Shared frame/camera/aspect/mid-z/background color contract: Task 2 and Task 4/5.
8. Single owner of final camera-dependent combined/sorted scene and nonblank PNG:
   Task 3 and Task 5.
