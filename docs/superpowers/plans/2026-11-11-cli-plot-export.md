# Native CLI Plot Export Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a native one-shot CLI that writes a static 2D SVG or 2D/3D PNG without opening a window or requiring a display, while retaining GUI no-argument startup.

**Architecture:** Native-only CLI dispatch resolves strict JSON config and explicit flags into validated app state, builds a plain `SceneGPU` with existing math, then selects CPU SVG or headless wgpu PNG. Keep the wasm entry and ordinary GUI path untouched; do not use `VfApp` or persistence.

**Tech Stack:** Rust 2024, clap derive (native-only), serde/serde_json, existing meval/egui-wgpu/wgpu/image/glam seams.

**Spec:** `docs/superpowers/specs/2026-11-11-cli-plot-export-design.md`

## Global Constraints

- Native-only CLI/dependencies under `cfg(not(target_arch = "wasm32"))`; wasm entry behavior remains unchanged.
- Existing meval, `State`, `particles::advect`, instance builders, SVG serializer, and renderer remain the semantic source of truth.
- No-args launch remains GUI; CLI never constructs `VfApp`, accesses GUI storage, opens dialogs, or writes persistence.
- Output is one snapshot: 2D SVG or 2D/3D PNG; no CPU PNG fallback and no 3D SVG.
- Precedence is app defaults → JSON config → explicitly supplied flags. Reject malformed or invalid input; do not silently clamp.
- Dimensions are each 1..=4096 and at most 16,777,216 pixels. Duration is 0..=3600 simulated seconds; fixed step is finite and `(0, 0.25]` seconds, default 1/60 s.
- Particle seed defaults to 1 and controls only the particle spawn/respawn stream. Full fixed steps plus one shortened final step reach duration exactly; zero duration performs no advection.
- Determinism is same-build/platform only; do not promise cross-platform bit-identical GPU PNG bytes.
- Coordinate test registration with tickets 76/77; tests live under `tests/`, no `#[cfg(test)]` in `src/`, no test-only broad API widening.
- No CAS, new physics, animation/video, speculative performance claims, or general-purpose config framework.

## Review Focus

- **Mode/default coupling:** config mode is known before `State::new`; add assertions that mode-specific formula/density defaults match `State::new(false/true)`.
- **Override semantics:** omitted flags preserve config; malformed config fails even when a CLI flag would replace the same field; test both.
- **Platform initialization:** SVG must not create an adapter and PNG must not create a window/display; test with an adapter-call seam and displayless runtime exercise.
- **Simulation boundaries:** exact and fractional fixed-step endpoints and zero duration must match repeated existing `advect` calls.
- **Resource-invalid input:** dimensions, product, density, particles, trail, finite values, and reversed/degenerate domains fail before allocation/sampling; pin each boundary.

---

## File Structure

- `src/main.rs`: native dispatch before window construction; preserve existing no-args path and leave wasm `start` unchanged.
- `src/cli.rs`: native-only CLI args/config/schema/merge/validation/error/orchestration contracts.
- `src/cli_scene.rs`: pure effective-state → scene and deterministic particle stepping, only if scene composition does not fit cleanly in `cli.rs`.
- `src/cli_gpu.rs`: native headless wgpu adapter/device/offscreen readback boundary; isolated from SVG.
- `src/lib.rs`: declare native-only CLI modules only where library access requires them; keep API narrow.
- `Cargo.toml`: add clap only to native-target dependencies, with GPL-compatible version pinned by normal dependency policy.
- `tests/cli_test.rs`: parser/config/validation/dispatch/simulation/SVG unit-integration contracts; add to suite registration only after ticket 76/77 ownership is released.
- `tests/cli_gpu_test.rs` or runtime script: PNG decode and conditional displayless software-adapter exercise; choose based on whether the shared suite can robustly report adapter absence without ignored tests.
- `src/renderer.rs` / `src/screenshot.rs` / `src/export.rs`: modify only the minimum narrow seam needed for offscreen readback/transform/background semantics; renderer change means smoke gate applies.

## Interfaces

The exact effective control record must mirror the approved spec's `CliOptions` and its `CliConfig` optional-field overlay. Suggested seams:

```rust
pub struct CliConfig { /* optional serialized current controls */ }
pub struct ExplicitFlags { /* Option<T> for each explicitly supplied flag */ }
pub struct CliOptions { /* fully resolved, validated options + output */ }
pub fn resolve_config(default_mode: Mode, config: Option<CliConfig>, flags: ExplicitFlags) -> Result<CliOptions, CliError>;
pub fn validate_options(options: &CliOptions) -> Result<(), CliError>;
pub fn build_scene(options: &CliOptions) -> Result<SceneGPU, CliError>;
pub fn render_svg(scene: &SceneGPU, options: &CliOptions) -> Result<String, CliError>;
pub fn run_export(options: &CliOptions) -> Result<(), CliError>;
```

Use private visibility except boundaries genuinely shared across files. Ticket 77's merged reference-visibility API is a required interface handoff: reconcile its exact source names/defaults before freezing schema; this plan does not independently implement that feature.

## Tasks

### Task 1: Native CLI argument and config resolution

**Files:**
- Create: `src/cli.rs`
- Modify: `src/main.rs`
- Modify: `src/lib.rs` if required for module compilation
- Modify: `Cargo.toml`
- Test: `tests/cli_test.rs`
- Test registration: `tests/suite.rs` only after 76/77 release ownership

**Interfaces:**
- Produces `CliConfig`, `ExplicitFlags`, `CliOptions`, `CliError`, `resolve_config`, `validate_options`, and `parse_args` for later tasks.
- Native `main` invokes CLI only when command arguments are present; no args retain GUI launch.

- [ ] **Step 1: Write failing config/parser tests.** Assert help/version successful parse; output selection; config parse; 2D/3D mode defaults sourced from `State::new`; explicit flags override a config field while omitted flags preserve it; unknown keys, malformed JSON, bad config values, invalid enums, missing output, unknown flag, and malformed values return errors.
- [ ] **Step 2: Run the targeted new test** with the suite's focused filter after registration is allowed. Confirm failures are expected missing CLI API/test code; do not alter `tests/suite.rs` while 76 owns it.
- [ ] **Step 3: Add native-only clap dependency and implement parser/merge.** All flag fields are `Option<T>` so omission does not erase config. Read JSON strictly; parse config before applying explicit flags. Choose selected mode from explicit `--mode`, else config `mode`, else 2D; instantiate baseline via `State::new(selected_mode)`.
- [ ] **Step 4: Implement fail-fast validation.** Enforce spec bounds, finite values, valid formula, domain validity and actual supported enum/control ranges. Keep unsupported/nonmerged ticket-77 controls out until source interface reconciliation.
- [ ] **Step 5: Wire dispatch.** Parse args after existing native logger setup, before `window_options`/`run_native`. No args follow the current GUI path. Keep all CLI code/dependencies native-only and do not edit wasm `start`.
- [ ] **Step 6: Re-run parser/config tests** and confirm the explicit/config/default matrix and no-args dispatch contract pass.
- [ ] **Step 7: Commit** `feat: parse native plot export options` (no ticket marker).

### Task 2: Pure deterministic scene and SVG export

**Files:**
- Create: `src/cli_scene.rs` if separate seam is warranted
- Modify: `src/cli.rs`
- Modify: `src/particles.rs` only for additive caller-seeded particle construction
- Modify: `src/reference.rs` only to consume ticket 77's already-merged visibility API (not to add ticket 77 behavior)
- Modify: `src/screenshot.rs` only for a pure SVG output transform/background parameter if existing interface cannot express contract
- Test: `tests/cli_test.rs`

**Interfaces:**
- Consumes validated `CliOptions` from Task 1.
- Produces `build_scene(&CliOptions) -> Result<SceneGPU, CliError>` and `render_svg(&SceneGPU, &CliOptions) -> Result<String, CliError>`.
- Seeded particle seam is additive and preserves `Particles::default()` GUI stream.

- [ ] **Step 1: Add failing deterministic simulation tests.** Pin seed=1 initial state, alternate seed changes positions, duration zero makes no advect calls/state change, exact and fractional step durations reach the requested elapsed time, and generated particle positions equal repeated existing `advect` semantics.
- [ ] **Step 2: Add failing SVG assertions.** For 2D output assert actual vector primitives, no `<image>`/data raster, deterministic bytes, expected bounds/background, and explicit 3D-SVG rejection before serialization. Add an adapter creation counter/test seam and assert SVG leaves it at zero.
- [ ] **Step 3: Add additive seeded particle initialization.** Implement a caller-seed entry point while proving existing `Particles::default()` reproducibility fixtures remain unchanged. Do not add a second advection implementation.
- [ ] **Step 4: Build the scene from existing seams.** Apply resolved setters/options to `State`; use snapshot and streamline snapshot; compose references using merged ticket 77 interface; advect through `particles::advect` with complete fixed steps and a final shortened step; generate dots/trails through existing constructors and preserve render ordering.
- [ ] **Step 5: Implement CPU SVG path.** Fit a deterministic front projection to the XY domain/output aspect; serialize existing scene vector instances; use explicit RGBA8 background semantics; reject 3D before renderer/serializer. Resolve ticket 77 reference fields/defaults from its merged source before schema is frozen.
- [ ] **Step 6: Run pure CLI/SVG/simulation tests** and confirm default field state and repeated advection are the same as existing code paths.
- [ ] **Step 7: Commit** `feat: export deterministic vector field SVG` (no ticket marker).

### Task 3: Headless wgpu PNG output

**Files:**
- Create: `src/cli_gpu.rs`
- Modify: `src/cli.rs`
- Modify: `src/renderer.rs` and/or `src/screenshot.rs` only for the narrow offscreen entry seam
- Modify: `src/export.rs` only if existing PNG encoding needs a narrow reuse point
- Test: `tests/cli_gpu_test.rs` or runtime script, chosen according to adapter absence reporting

**Interfaces:**
- Consumes `SceneGPU`, validated dimensions and camera/background from Tasks 1–2.
- Produces packed RGBA8 frame bytes or an actionable `CliError`; `run_export` saves via existing PNG encoding.
- SVG remains independent of this module and does not call adapter creation.

- [ ] **Step 1: Inspect resolved eframe/wgpu API and write failing boundary tests** for `width=1,height=1`, upper dimension/product boundaries, adapter absence, and PNG dimensions. Test no-display path without invoking eframe/window APIs.
- [ ] **Step 2: Implement direct headless adapter/device request.** Select no surface; request only supported features/limits required by current renderer. Return distinct contextual errors for no adapter, device request, render validation, readback, and file write. Do not add a CPU fallback.
- [ ] **Step 3: Reuse `DrawState` offscreen rendering** with deterministic `view_proj`, target format and readback. Honor bounded output dimensions before texture/buffer allocation; encode/save with existing RGBA `export` path.
- [ ] **Step 4: Run GPU test when a software/headless adapter exists.** If none exists, assert the explicit adapter error through a narrow injectable request seam and report the integration check unavailable (not ignored). Decode PNG and assert exact requested dimensions when rendered.
- [ ] **Step 5: Commit** `feat: render native CLI PNG offscreen` (no ticket marker).

### Task 4: CLI examples, compatibility, and full verification

**Files:**
- Modify: `README.md` or add `docs/cli.md` (choose one discoverable user-facing home; avoid duplicating full schema)
- Modify: tests from Tasks 1–3 only if findings require

**Interfaces:**
- Public user docs match frozen flags/schema/defaults and examples in the design spec.
- Complete implementation verified against the acceptance gates below.

- [ ] **Step 1: Add docs assertions/manual review** for 2D SVG and 3D PNG examples, config precedence, fixed seed/duration/step units, resource bounds, no-args GUI behavior, GPU requirement/no CPU fallback, and 3D SVG error.
- [ ] **Step 2: Exercise CLI outputs.** Run a real SVG export and inspect vector markup; run displayless PNG with software adapter when available and decode dimensions; verify failure path on unavailable adapter; verify no persistence file changes and ordinary GUI startup.
- [ ] **Step 3: Run required gates:** `cargo build`; `scripts/test.sh`; `cargo clippy --all-targets -- -D warnings`; `scripts/smoke-run.sh 15` because renderer changes; wasm build and web bootstrap check for cfg/dependency isolation. Do not claim gates without captured successful results.
- [ ] **Step 4: Self-review diff** against ticket 78, spec, merged ticket 77 interface, and `AGENTS.md`; verify no suite registration conflict, no CAS/new evaluator, no test ignores, and no GUI regression.
- [ ] **Step 5: Commit docs/fixes** with `docs:`/`fix:` subjects without ticket marker; report runtime checks that are unavailable separately from pass results.

## Scope Coverage

Task 1 covers flags/config/defaults/validation/no-args/web gating. Task 2 covers
all supported controls, scene semantics, explicit seeded simulation, SVG and
3D-SVG rejection, ticket 77 handoff. Task 3 covers displayless wgpu PNG and
unavailable-adapter failure. Task 4 covers user instructions, GUI compatibility,
all project gates, wasm isolation and runtime evidence. No claim of completion
is valid until required gates and applicable runtime checks have evidence.
