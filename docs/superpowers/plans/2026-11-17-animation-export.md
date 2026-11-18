# Animation Export Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add deterministic GIF and optional native ffmpeg MP4/WebM animation export for time-dependent Formula fields, Induction scenes and particle playback, with GUI and CLI paths that share the existing SimClock/SceneGPU seams.

**Architecture:** Pure animation/spec/encoder helpers stream frames through a `FrameSink`. A detached `AnimationSnapshot` advances a cloned `State` and particle cloud by an explicit frame step, builds the existing vector scene, and never touches live playback. GUI and CLI own GPU sessions but share the snapshot and encoder contracts.

**Tech Stack:** Rust 2024, eframe/egui, existing `SceneGPU`/`DrawState`, `gif` 0.14 (already in lock via `image`), optional external `ffmpeg`, clap/config, wasm compile path.

**Spec:** `docs/superpowers/specs/2026-11-17-animation-export-design.md`

## Global Constraints

- GIF is native + wasm. MP4/WebM are native-only and require an installed `ffmpeg`; never silently fall back to GIF.
- `AnimationSpec`: start/duration/fps/width/height/loop/format; fps `1..=30`; duration `0.1..=60.0`; max 1800 frames; max 8,388,608 pixels/frame; max 512 MiB output.
- Frame `i` time is `start + i/fps`; `ceil(duration*fps)` frames; no duplicate endpoint. GIF delay `round(100/fps).max(1)`.
- Video requires even dimensions; `--animation-loop` with video is invalid. GIF loop is the Netscape infinite repeat; single-play omits the repeat extension.
- No second clock, private timer, `advance_for` wall-clock dt, or GUI `MAX_STEP` for export. Particles use exact `1/fps` with `1/120` substeps and a final remainder.
- GUI animation covers time-dependent Formula, animated Induction and particles. CLI animation covers existing CLI Formula/particle scenes only.
- Every frame gets the request-time export provenance banner; GIF has no reliable metadata, video embeds sanitized metadata. Never claim a stripped-metadata warning is sufficient.
- No bundled video codec, audio, animated SVG, scalar/Sources/Materials/ODE animation, frame interpolation, or persistence of animation settings.
- Bounded, streaming encoders; one frame in flight; temp output removed on every failure/cancel; native/wasm compile.
- Tests in `tests/` and registered in `tests/suite.rs`; no inline `#[cfg(test)]` under `src/`.
- Browser/manual visual gates are owner-waived, not passed. Only known `nom 1.2.4` warning.

## Review Focus

1. **Determinism:** exporting the same captured state twice yields identical frame bytes; no wall-clock or live `State` mutation.
2. **Particle time:** exact `1/fps` advance with `1/120` substeps, not GUI `MAX_STEP`; a long first frame must not shorten/skip motion.
3. **Encoder failure:** ffmpeg missing/nonzero, GIF write failure, byte-cap, cancel, and partial temp files must never leave a final output or a stuck job.
4. **Provenance:** every frame carries the warning; an unavailable/invalid induction sample never becomes an unwarned valid animation.
5. **CLI compatibility:** still PNG/SVG and `--duration` particle preadvance remain unchanged; animation mode changes only when the format flag is present.
6. **Mode eligibility:** static Formula/Induction and non-animatable modes show exact disabled reasons and cannot start a job.

---

### Task 1: Pure animation spec and streaming encoders

**Files:**
- Create: `src/animation.rs`
- Modify: `src/lib.rs`, `Cargo.toml`, `docs/CODEBASE.md`
- Create: `tests/animation_test.rs`, `tests/animation_gif_test.rs`, `tests/animation_ffmpeg_test.rs`
- Modify: `tests/suite.rs`

**Interfaces:**
- Consumes: `ExportProvenance` from `src/export_provenance.rs`; `gif` crate.
- Produces: `AnimationFormat`, `AnimationSpec`, `AnimationError`, `FrameSink`, `frame_times`, `frame_count`, `gif_delay_centiseconds`, `gif_sink`, `video_sink`, hidden `video_sink_with_program`, `ffmpeg_available`, `ffmpeg_args`, `atomic_replace`, constants from the design.

- [ ] **Step 1: Write failing spec/encoder tests**

Create `tests/animation_test.rs` with concrete imports:

```rust
use vector_field_explorer::animation::{
    AnimationError, AnimationFormat, AnimationSpec, frame_count, frame_times,
    gif_delay_centiseconds,
};
```

Tests:

```rust
#[test]
fn frame_count_and_times_do_not_duplicate_the_endpoint() {
    let spec = AnimationSpec { start_time: 2.0, duration: 0.5, fps: 10, width: 320, height: 240, loop_gif: true, format: AnimationFormat::Gif };
    assert_eq!(frame_count(&spec).unwrap(), 5);
    let times = frame_times(&spec).unwrap();
    assert_eq!(times.len(), 5);
    for (actual, expected) in times.iter().zip([2.0, 2.1, 2.2, 2.3, 2.4]) {
        assert!((actual - expected).abs() < 1e-6);
    }
}

#[test]
fn animation_limits_reject_bad_specs() {
    let base = AnimationSpec { start_time: 0.0, duration: 1.0, fps: 12, width: 320, height: 240, loop_gif: true, format: AnimationFormat::Gif };
    for mutate in [
        |s: &mut AnimationSpec| s.fps = 0,
        |s: &mut AnimationSpec| s.fps = 31,
        |s: &mut AnimationSpec| s.duration = 0.0,
        |s: &mut AnimationSpec| s.duration = 61.0,
        |s: &mut AnimationSpec| s.start_time = f32::NAN,
        |s: &mut AnimationSpec| s.width = 0,
        |s: &mut AnimationSpec| { s.width = 4096; s.height = 4096; },
    ] {
        let mut spec = base.clone();
        mutate(&mut spec);
        assert!(matches!(spec.validate(), Err(AnimationError::InvalidSpec(_))));
    }
    let odd = AnimationSpec { format: AnimationFormat::Mp4, width: 321, ..base.clone() };
    assert!(matches!(odd.validate(), Err(AnimationError::InvalidSpec(_))));
    let mut too_long = base;
    too_long.duration = 60.0;
    too_long.fps = 30;
    assert_eq!(frame_count(&too_long).unwrap(), 1800);
}

#[test]
fn gif_delay_quantizes_predictably() {
    assert_eq!(gif_delay_centiseconds(1), 100);
    assert_eq!(gif_delay_centiseconds(12), 8);
    assert_eq!(gif_delay_centiseconds(30), 3);
}
```

Create `tests/animation_gif_test.rs`: encode 3 4×4 RGBA frames with distinct colors; decode with `gif::DecodeOptions::new().read_info`; assert 3 frames, dimensions, delay `gif_delay_centiseconds(10)`, loop extension present when `loop_gif=true` and absent when false; assert banner-ish color is present in every decoded frame; assert output file exists and no `.tmp` sibling remains.

Create `tests/animation_ffmpeg_test.rs`: `ffmpeg_args` exact mp4 args contains `libx264`, `yuv420p`, `+faststart`, `-vf`, `vflip`, metadata comment; webm contains `libvpx-vp9`; `#[cfg(unix)]` `video_sink_with_program` with a tiny Unix shell script that consumes stdin and writes a dummy output, asserting the script received `width*height*4*frame_count` bytes; nonzero exit surfaces stderr and leaves no final file; missing program returns `AnimationError::External`. A non-Unix build keeps the argv/absence tests and omits the shell-script process test, disclosed in the report.

- [ ] **Step 2: Run and confirm RED**

Run: `cargo test --test suite animation_test:: animation_gif_test:: animation_ffmpeg_test::`
Expected: compile failure for missing module/types.

- [ ] **Step 3: Implement the module**

Implement exact constants/API from design. `AnimationFormat`, `AnimationSpec`, and `AnimationError` derive `Debug, Clone, Copy, PartialEq` where meaningful; `AnimationSpec` implements `Default` (start 0, duration 4, fps 12, 1280×720, loop on, GIF). `FrameSink` receives tightly packed RGBA and validates length. `gif_sink` streams through a counting writer into a sibling `.tmp` file, uses per-frame local palettes and no dithering, writes `Repeat::Infinite` only for loop, and on `finish` checks bytes, closes, and calls `atomic_replace`. `video_sink` spawns ffmpeg with `ffmpeg_args`, writes frames to stdin, captures stderr, waits, checks temp size, and renames only on success. `ffmpeg_available` returns true for a `VFE_FFMPEG` executable or `ffmpeg -version` success; never executes during a pure spec test. `atomic_replace` removes an existing destination before rename on Windows-compatible paths.

Promote `gif = "0.14"` to a direct dependency. No other dependency.

- [ ] **Step 4: Run tests and commit**

Run:

```bash
cargo test --test suite animation_test::
cargo test --test suite animation_gif_test::
cargo test --test suite animation_ffmpeg_test::
```

Then full gates as in Task 1 of this plan. Commit:

```bash
git add src/animation.rs src/lib.rs Cargo.toml Cargo.lock tests/animation_test.rs tests/animation_gif_test.rs tests/animation_ffmpeg_test.rs tests/suite.rs docs/CODEBASE.md
git commit -m "feat: add bounded animation encoders" -m "Generated-by: OpenCode"
```

---

### Task 2: Detached animation snapshot and exact-dt frames

**Files:**
- Create: `src/animation_scene.rs`
- Modify: `src/viewmodel.rs`, `src/particles.rs`, `src/app/mod.rs`, `src/app/simulation.rs`, `src/app/viewport.rs`
- Create: `tests/animation_scene_test.rs`
- Modify: `tests/suite.rs`, `docs/CODEBASE.md`

**Interfaces:**
- Consumes: Task 1 `AnimationSpec`; `State`, `Particles`, `OrbitCamera`, `Option<FlyCamera>`, `SceneGPU`.
- Produces: `AnimationSnapshot { state, particles, orbit, fly, clear }`, `AnimationFrame { scene, view_proj, eye }`, `AnimationSnapshot::{from_parts,capture,frame_at}`, `State: Clone`, `particles::advance_exact`, `VfApp::animation_snapshot`.

- [ ] **Step 1: Write failing snapshot tests**

Create `tests/animation_scene_test.rs` with imports `AnimationSnapshot`, `AnimationFrame`, `State`, `Particles`, `particles`, `OrbitCamera`, `SimClock`. Tests:

```rust
#[test]
fn snapshot_captures_the_time_dependent_formula_scene() {
    let mut state = State::new(true);
    state.set_formula("sin(t), 0, 0");
    state.set_time(0.25);
    let snapshot = AnimationSnapshot::from_parts(
        state,
        Particles::default(),
        OrbitCamera::default(),
        None,
        [0.0;4],
    );
    let first = snapshot.clone().frame_at(0.25, 1.0 / 12.0, 1.0, true).expect("frame");
    let later = snapshot.clone().frame_at(1.25, 1.0 / 12.0, 1.0, false).expect("frame");
    assert_ne!(first.scene.arrows[0].dir, later.scene.arrows[0].dir);
}

#[test]
fn frame_step_does_not_use_the_gui_step_limit() {
    let mut state = State::new(true);
    state.set_formula("1, 0, 0");
    let mut particles = Particles::default();
    particles::spawn(&mut particles, &state.domain, 1, 3);
    let start = particles.pos[0];
    let snapshot = AnimationSnapshot::from_parts(
        state,
        particles,
        OrbitCamera::default(),
        None,
        [0.0;4],
    );
    let mut snapshot = snapshot;
    let _frame = snapshot.frame_at(0.5, 0.5, 1.0, false).expect("frame");
    assert_eq!(snapshot.particles.pos.len(), 1);
    // A 0.5 s non-first frame is subdivided, not clamped to 1/20 s.
    assert!(snapshot.particles.pos[0].distance(start) > 0.0);
}
```

The second test uses a nonzero formula and an independent helper `expected_substeps(dt)` to pin the 1/120 subdivision; assert the result equals a direct 60-substep advection rather than merely `> 0`.

- [ ] **Step 2: Confirm RED**

Run `cargo test --test suite animation_scene_test::`; expect missing `AnimationSnapshot` APIs.

- [ ] **Step 3: Implement the snapshot and scene assembly**

- Add `Clone` to `State`; confirm all fields clone and no GPU handles are stored. Add `Particles` clone is already available.
- Add `particles::advance_exact(particles, flow, domain, start_time, dt, settings)` that splits `dt` into `1/120` substeps plus remainder, calls the existing advection with exact substep time, and never clamps to `MAX_STEP`.
- `AnimationSnapshot::from_parts(state, particles, camera, clear)` stores them. `capture(app)` is implemented in `app/mod.rs` where private app fields are visible.
- `frame_at(time, dt, aspect, first)`:
  - Formula: `state.set_time(time)`, mark dirty, build the vector scene with the extracted live assembly; do not advance particles unless particles are enabled.
  - Induction: set time, refresh induction sampling/rebuild, compose arrows + one conductor glyph; no field sampling if static/invalid.
  - Particles: on non-first frames advance the detached cloud by exact `1/fps`; set time; build dots/trails/combined.
  - Set `view_proj` from the snapshot camera and the current scene domain/aspect; return `eye`.
- Extract the live vector assembly from `app/viewport.rs`/`app/simulation.rs` into a shared `compose_vector_scene(state, particles, ...)` function used by both `VfApp` and the snapshot. Preserve exact live 2D/3D ordering, references, glyphs, and empty-list behavior; existing viewport/plane/source tests are the guard.
- Snapshot construction must not mutate the live app.

- [ ] **Step 4: Run focused regressions and commit**

```bash
cargo test --test suite animation_scene_test::
cargo test --test suite particle_playback_test::
cargo test --test suite plane_particles_test::
cargo test --test suite source_mode_test::
cargo test --test suite induction_app_test::
```

Then commit:

```bash
git add src/animation_scene.rs src/viewmodel.rs src/particles.rs src/app/mod.rs src/app/simulation.rs src/app/viewport.rs tests/animation_scene_test.rs tests/suite.rs docs/CODEBASE.md
git commit -m "feat: add detached animation frame source" -m "Generated-by: OpenCode"
```

---

### Task 3: GUI animation job, dialog and menu

**Files:**
- Create: `src/animation_job.rs`, `src/ui/animation_dialog.rs`, `tests/animation_job_test.rs`, `tests/animation_dialog_test.rs`
- Modify: `src/ui/mod.rs`, `src/ui/menu.rs`, `src/ui/top_bar.rs`, `src/app/mod.rs`, `src/app/input.rs`, `src/app/panels.rs`, `src/screenshot.rs` (mutual exclusion only)
- Modify: `tests/suite.rs`, `docs/CODEBASE.md`

**Interfaces:**
- Consumes: Task 1 sinks/spec; Task 2 `AnimationSnapshot`.
- Produces: `AnimationEligibility`, `AnimationJob`, `AnimationJobState`, `AnimationDialogState`, exact disabled reasons, GUI export.

- [ ] **Step 1: Write failing job/dialog tests**

`tests/animation_job_test.rs` uses fake `AnimationRenderer` and `FrameSink`:

```rust
#[test]
fn job_renders_every_frame_and_finishes() {
    let spec = AnimationSpec { duration: 0.25, fps: 4, ..AnimationSpec::default() };
    let mut job = AnimationJob::fake(spec, 2); // fake renderer, fake sink
    assert_eq!(job.state(), AnimationJobState::Preparing);
    job.pump_for_test(usize::MAX).expect("job completes");
    assert_eq!(job.state(), AnimationJobState::Done);
    assert_eq!(job.frames_done(), 2);
}

#[test]
fn cancel_stops_without_finishing_the_sink() {
    let mut job = AnimationJob::fake(AnimationSpec::default(), 10);
    job.cancel();
    job.pump_for_test(1).expect("cancel drains");
    assert_eq!(job.state(), AnimationJobState::Cancelled);
}
```

Add failures for renderer error, sink error, and a cancellation during encoding. `tests/animation_dialog_test.rs` uses accesskit at 1200/390 and `VfApp` frames: menu item appears, dialog fields and frame count visible, Start disabled for static Induction, enabled for animated Induction, disabled for ODE/Sources/Materials with exact reasons.

- [ ] **Step 2: Confirm RED**

Run `cargo test --test suite animation_job_test:: animation_dialog_test::`; expect missing types/widgets.

- [ ] **Step 3: Implement the job and GUI**

- `AnimationEligibility { allowed: bool, reason: Option<&'static str>, animated: bool }` computed from mode/field/particles/induction sampling. Exact reasons from design.
- `AnimationJob` owns spec, snapshot, `Box<dyn FrameSink>`, renderer, next frame index, cancellation flag and state. `pump` renders at most one frame per call (native worker encode may continue in background), applies `apply_export_banner` with provenance, pushes to sink, and updates progress. `cancel` stops future frames and calls a sink drop that deletes temps.
- GUI dialog stores format/duration/fps/loop/size/error/progress. Start validates, captures snapshot and provenance, chooses GIF/ffmpeg sink, pauses live playback, starts job. On finish/failure restore playback and show status. Native uses `rfd` save dialog; wasm downloads GIF after encode.
- Menu adds `File > Export animation…`; disable via eligibility reason. Screenshot/PNG/SVG capture and animation job are mutually exclusive.
- wasm path uses one async offscreen readback at a time, max edge 2048 and validation error above it; no browser claim.
- Add `AnimationJobState` to app status; no second clock, no per-frame live `State` mutation.

- [ ] **Step 4: Run focused UI/job tests and commit**

```bash
cargo test --test suite animation_job_test::
cargo test --test suite animation_dialog_test::
cargo test --test suite screenshot_test::
cargo test --test suite svg_export_test::
```

Commit `feat: add animation export dialog and job`.

---

### Task 4: CLI animation and reusable headless session

**Files:**
- Modify: `src/cli.rs`, `src/cli_scene.rs`, `src/cli_gpu.rs`, `src/cli_svg.rs` (only if a config match needs an arm)
- Create: `tests/cli_animation_test.rs`
- Modify: `tests/suite.rs`, `docs/CODEBASE.md`

**Interfaces:**
- Consumes: Task 1 sinks/spec; Task 2 snapshot/particle advance.
- Produces: clap/config `animation_format`, `animation_fps`, `animation_loop`; `HeadlessRenderSession`; `prepare_animation_snapshot`; CLI GIF/MP4/WebM output.

- [ ] **Step 1: Write failing CLI tests**

`tests/cli_animation_test.rs`:
- `animation_flags_round_trip_config`.
- `animation_requires_matching_extension_and_even_video_dimensions`.
- `animation_loop_is_gif_only`.
- `still_export_ignores_animation_fields`.
- `animation_uses_one_headless_session` (test seam counts `HeadlessRenderSession::new` calls).
- `cli_animation_formula_and_particles_frame_count` builds configs and a fake renderer/encoder path.
- Invalid frame/fps/duration leaves no output and reports the exact error.

Use `CliOptions`/`CliConfig` construction, not a live process for unit cases. Keep one native smoke invoking a short GIF if feasible.

- [ ] **Step 2: Confirm RED**

Run `cargo test --test suite cli_animation_test::`; expect missing fields/session.

- [ ] **Step 3: Implement CLI animation**

- Add flags and JSON fields with defaults `animation_fps=12`, `animation_loop=false`; no `--animation` bool. Resolve effective duration default 4 s only when `animation_format` is set; validate extension/format, loop/video, limits, and fixed-step.
- Refactor `cli_gpu::render_offscreen` to `HeadlessRenderSession::new()` + `render_frame(scene, view_proj, camera, width, height, clear)`; keep the existing one-shot `render_offscreen` wrapper for still PNG.
- `prepare_animation_snapshot` in `cli_scene.rs` reuses the still formula/particle setup: set `state.time=start`, preadvance particles with existing `--fixed-step` to start, capture snapshot. No Induction/source flags.
- Frame loop uses `AnimationSpec` times, `AnimationSnapshot::frame_at`, `apply_export_banner`, and `FrameSink`; never call `render_offscreen` per frame.
- Write to sibling temp, check byte cap, atomic rename. Missing ffmpeg is `CliError::Config`/`External` with actionable text; GIF remains available.
- `run_export` branches only when `animation_format.is_some()`, preserving still behavior.

- [ ] **Step 4: Run CLI tests/gates and commit**

```bash
cargo test --test suite cli_animation_test::
cargo test --test suite cli_config_test::
cargo test --test suite cli_png_test::
cargo test --test suite cli_svg_test::
scripts/test-cli-dispatch.sh
```

Commit `feat: add CLI animation export`.

---

### Task 5: Docs, benchmark and final gates

**Files:**
- Modify: `README.md`, `GLOSSARY.md`, `docs/CODEBASE.md`, `docs/superpowers/README.md`, `docs/perf/animation-performance.md`
- Evidence: `.superpowers/sdd/2026-11-17-animation-export/evidence/task-5/`

**Interfaces:**
- Consumes all prior tasks.
- Produces user/reviewer documentation, measured evidence and gate report.

- [ ] **Step 1: Document animation export**

Cover GUI eligibility, GIF/ffmpeg formats, CLI flags and `--duration` semantics, deterministic frame schedule, particle substeps, limits, warnings/metadata limitations, wasm GIF only, missing ffmpeg, cancel/failure atomicity, no scalar/source/material/ODE animation, and browser/manual waiver. Add glossary entries for **Animation export**, **frame schedule**, **GIF delay**, and **ffmpeg-backed video**.

- [ ] **Step 2: Benchmark and evidence**

Run a native 120-frame 1280×720 GIF export from a time-dependent formula and from an animated induction case; record host, frame count, encode time, output size and decode check. Run a real MP4/WebM only if `ffmpeg` exists; otherwise report unavailable, not passed. Record `VFE_FFMPEG` result honestly.

- [ ] **Step 3: Run final gates**

```bash
cargo test --all-targets
cargo build
cargo clippy --all-targets -- -D warnings
rustfmt --edition 2024 --check --config skip_children=true <changed .rs files>
python3 scripts/check-docs.py
git diff --check
scripts/smoke-run.sh 15
cargo check --target wasm32-unknown-unknown
```

If the wasm target is unavailable, record unavailable; never claim pass.

- [ ] **Step 4: Commit docs/evidence**

```bash
git add README.md GLOSSARY.md docs/CODEBASE.md docs/superpowers/README.md docs/perf/animation-performance.md
git commit -m "docs: document animation export" -m "Generated-by: OpenCode"
```

---

## Self-Review

- **Spec coverage:** Five tasks cover spec/schedule/encoders, detached snapshot/particle exact-dt, GUI job/dialog, CLI animation/headless reuse, and docs/benchmark/gates. Every design requirement has an owning task and test.
- **Step scan:** Every step names exact files, APIs, tests, commands or commits. No TBD or assertion-free placeholder remains.
- **Type consistency:** `AnimationSpec`, `FrameSink`, `AnimationSnapshot`, `AnimationJob`, `AnimationFormat`, provenance fields and CLI names are identical across tasks.
- **Review Focus:** Each risk has a test: determinism, exact particle dt, encoder failure/atomicity, per-frame provenance, CLI compatibility, mode eligibility.
- **Proportion:** The plan locks APIs/values and test intent without transcribing every body. Ordinary implementation bodies remain the implementer’s work.
