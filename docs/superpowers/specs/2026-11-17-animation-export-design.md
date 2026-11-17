# Animation Export Design

**Status:** APPROVED by owner on 2026-11-17
**Date:** 2026-11-17
**Issue:** [#71](https://github.com/ttgg1/vector-field-explorer/issues/71)
**Dependencies:** #16 (shared clock scalar animation), #27 (induction), #28 (ODE/clock seams), #15 (CLI export)

## Intent

Add a deterministic animation export for the scenes this app already knows how to animate:

- time-dependent Formula fields,
- Induction scenes,
- particle playback.

The output is an animated GIF everywhere, plus MP4/WebM on native when a user-installed `ffmpeg` is available. Native CLI export gains animation options. The same shared `SimClock` and scene sampling/rendering seams are used; the exporter steps time explicitly and never introduces a second timer.

## Goals

- Produce deterministic frame sequences independent of wall-clock frame rate.
- Reuse the current camera, mode, scene, particle state and provenance at export start.
- Support native GUI, native CLI, and wasm GIF.
- Bound frame count, pixels and output size; fail with actionable messages.
- Bake model/accuracy warnings visibly into every exported frame; put metadata where the format supports it.
- Preserve current still PNG/SVG behavior and all existing mode behavior.

## Non-goals

- No second playback clock or private timer.
- No new renderer/backend; use the existing `SceneGPU`/`DrawState` pipelines.
- No CLI Induction/source/material/ODE configuration. CLI animation covers the scenes the CLI already configures: Formula fields and particles.
- No bundled video codec, no linking to ffmpeg, no audio, no timeline editing, no frame interpolation, no variable frame rate, no transparency in video, no animated SVG.
- No animation of scalar plots, Materials, Sources, or ODE in this issue.
- No persistence of animation settings beyond the current dialog defaults.
- No browser/manual visual pass claimed as done.

## Approaches

### A. GIF only

Use the `image`/`gif` stack already in the lock graph. Smallest and safest, works on wasm, but does not deliver video.

### B. Bundled pure-Rust video encoder

Add `rav1e`/WebM muxing or a similar encoder. Self-contained video, but a heavy native dependency, long builds, slow exports, large binary and no wasm path.

### C. GIF + optional external ffmpeg (chosen)

GIF is built in and works everywhere. Native MP4/WebM export pipes raw frames to an installed `ffmpeg`. If it is absent, the UI/CLI explains that GIF is available and video requires `ffmpeg`. No linking, no bundled codec, no wasm video. This satisfies the deliverable without turning the app into a media encoder.

Chosen by the owner on 2026-11-17. CLI coverage is the existing CLI Formula/particle scene family; Induction remains GUI animation.

## Format and scheduling contract

```rust
pub enum AnimationFormat { Gif, Mp4, Webm }

pub struct AnimationSpec {
    pub start_time: f32,
    pub duration: f32,
    pub fps: u32,
    pub width: u32,
    pub height: u32,
    pub loop_gif: bool,
    pub format: AnimationFormat,
}
```

`AnimationFormat`, `AnimationSpec`, and `AnimationError` derive `Debug, Clone, Copy, PartialEq` where meaningful. `AnimationSpec::default()` is start 0, duration 4 s, fps 12, 1280×720, GIF loop on; the GUI still uses the current viewport size and captured start time.

Constants:

```rust
pub const ANIMATION_MIN_FPS: u32 = 1;
pub const ANIMATION_MAX_FPS: u32 = 30;
pub const ANIMATION_MAX_FRAMES: usize = 1800;
pub const ANIMATION_MAX_DURATION: f32 = 60.0;
pub const ANIMATION_MIN_DURATION: f32 = 0.1;
pub const ANIMATION_MAX_PIXELS: u64 = 8_388_608; // 4K pixel budget
pub const ANIMATION_MAX_BYTES: u64 = 512 * 1024 * 1024;
pub const ANIMATION_WASM_MAX_EDGE: u32 = 2048;
pub const ANIMATION_PARTICLE_SUBSTEP: f32 = 1.0 / 120.0;
```

- `frame_count = ceil(duration * fps)` clamped to `1..=ANIMATION_MAX_FRAMES`.
- Frame `i` time is `start_time + i / fps`; the endpoint is not duplicated. A 2 s/10 fps export has frames at 0.0..1.9, 20 frames.
- `fps` is integer. GIF delay is `round(100 / fps).max(1)` centiseconds; duration may quantize slightly, documented.
- GIF uses a per-frame local palette (the `gif` crate quantizes each RGBA frame), so later viridis/neodymium frames are not forced through frame 0's palette. Dithering remains off.
- GIF looping uses `Repeat::Infinite` only when `loop_gif` is true. Single-play GIFs omit the repeat extension entirely; do not encode `Finite(0)`, whose browser semantics are ambiguous.
- The wasm edge limit is a validation rule, not a silent clamp: GUI defaults are clamped to the current budget, but an explicit larger size returns `InvalidSpec`.
- Video requires even width and height; odd dimensions are rejected before frame 1.
- Particle advances use a new exact-dt API, not the GUI `MAX_STEP` clamp: subdivide `1/fps` into steps of at most `ANIMATION_PARTICLE_SUBSTEP`, with a final remainder. Export is always 1× speed; the captured `SimClock.speed` is not applied.
- `AnimationSpec::validate` rejects nonfinite start/duration, out-of-range values, zero dimensions, pixel budget overflow, frame budget overflow and odd video sizes.
- GIF loop maps to the Netscape repeat extension: infinite when `loop_gif`, otherwise a single play. Video ignores `loop_gif` with a visible note.

## Architecture

### Pure animation module: `src/animation.rs`

```rust
pub enum AnimationError {
    InvalidSpec(String),
    Encoder(String),
    Io(String),
    External(String),
    Cancelled,
}

pub trait FrameSink {
    fn push_frame(&mut self, rgba: &[u8], width: u32, height: u32) -> Result<(), AnimationError>;
    fn bytes_written(&self) -> u64;
    fn finish(self: Box<Self>) -> Result<(), AnimationError>;
}

pub fn frame_times(spec: &AnimationSpec) -> Result<Vec<f32>, AnimationError>;
pub fn frame_count(spec: &AnimationSpec) -> Result<usize, AnimationError>;
pub fn gif_delay_centiseconds(fps: u32) -> u16;
pub fn ffmpeg_args(spec: &AnimationSpec, output: &Path, provenance: &ExportProvenance) -> Vec<String>;
pub fn ffmpeg_available() -> bool;
pub fn encode_gif_to_path(frames: impl Iterator<Item = Vec<u8>>, spec: &AnimationSpec, path: &Path) -> Result<(), AnimationError>;
```

`frame_times` allocates at most 1800 floats. Encoders stream; no all-frames-in-memory buffer.

GIF uses the `gif` crate directly (already a transitive dependency from `image`; promote to a direct dependency at the existing lock version, `gif = "0.14"`). This gives exact control over the Netscape repeat extension, palette quantization and per-frame delay. No new transitive crate version. GIF comment extensions are explicitly dropped: `gif 0.14` has no public comment API and the visible banner is the contract.

Video spawns `ffmpeg` with:

```
-y -loglevel error -f rawvideo -pixel_format rgba -video_size WxH
-framerate FPS -i pipe:0 -vf vflip
[mp4: -c:v libx264 -pix_fmt yuv420p -movflags +faststart]
[webm: -c:v libvpx-vp9 -pix_fmt yuv420p]
[-metadata comment=<provenance>] output
```

Frames are written to stdin. stderr is retained and surfaced on failure. `ffmpeg` is detected once; missing is a clear actionable error, not a silent GIF fallback. The completed file size is checked against `ANIMATION_MAX_BYTES`; an oversize file is deleted with an error. GIF uses a counting sink and aborts mid-stream as soon as the byte cap is exceeded, deleting the temporary file. Output goes to a sibling temporary path. On Unix a successful export renames atomically; on Windows, where rename-over-existing fails, it removes the destination then renames. Every failure/cancel path deletes the temporary file. A shared `atomic_replace` helper owns that policy. `ffmpeg` is found on `PATH`, with `VFE_FFMPEG` as an explicit executable override for both runtime and the real-encoder test; missing ffmpeg reports how to install it or set the override. Metadata is sanitized to one line (newlines to `; `, controls removed, 1024-char cap) before `-metadata comment=...`.

### Frame production seam

A new `AnimationSource`/`AnimationFrame` seam in `src/animation_scene.rs` (name finalized during planning) owns a detached snapshot:

```rust
pub struct AnimationSnapshot {
    pub state: State,              // clone; durable scenes/caches cloned
    pub particles: Particles,
    pub orbit: OrbitCamera,
    pub fly: Option<FlyCamera>,
    pub clear: [f64; 4],
}

pub struct AnimationFrame {
    pub scene: SceneGPU,
    pub view_proj: Mat4,
    pub eye: Vec3,
}

impl AnimationSnapshot {
    pub fn capture(app: &VfApp) -> Result<Self, AnimationError>;
    pub fn frame_at(&mut self, time: f32, first: bool) -> Result<AnimationFrame, AnimationError>;
}
```

- Capture copies the current state, particle cloud and camera; live playback is paused during GUI export and restored afterward.
- Formula: set `state.time`, resample the field scene at that time. Time-dependent formula sampling already exists.
- Induction: set `state.time`, refresh induction sampling and recompose arrows/glyph/conductor.
- Particles: set `state.time` and advance the detached cloud by exactly `1/fps` between frames; the first frame uses the captured cloud. No `advance_for`/wall-clock dt.
- 2D/3D camera and reference/glyph composition reuse the current vector scene assembly. A small refactor extracts the assembly currently in `app/simulation.rs` into a function usable by both the live app and the detached snapshot; behavior must remain identical for the live path.
- The first animation frame is the state at `start_time` for Formula/Induction. For particles, `start_time` must equal the captured `state.time` in the GUI; CLI may preadvance to `start_time` deterministically before the first frame.
- `State` must derive `Clone` (or an equivalent `animation_snapshot` clone method) so the exporter never mutates playback state. Caches are cloned; no GPU handles are stored in `State`.

### Render/encode state machine

Native and wasm share a job state machine, `AnimationJob`:

```
Idle -> Preparing -> RenderingFrame(i) -> Encoding -> Writing -> Done
                              ^                |
                              +----------------+
```

- `RenderingFrame` builds one `AnimationFrame`, calls the installed `DrawState`:
  - native: `render_offscreen` synchronously, before the live viewport paint;
  - wasm: `render_offscreen_async`, one readback in flight at a time.
- At most one offscreen animation render and one live paint happen per app frame. Animation start is blocked while a screenshot readback is pending, and screenshot requests are blocked while an animation job runs.
- The frame is passed through `apply_export_banner` with request-time provenance, then to the `FrameSink`.
- `Encoding` runs on a worker thread on native; wasm encodes GIF on the main thread with a 2048 max edge and yields between frames.
- The job owns progress (`frames_done/frames_total`), a cancellation token and the completion toast.
- Live app state is never stepped by the job. The snapshot is the only mutable animation state.
- Capture twice from the same app state must produce identical frames; the snapshot is not required to be re-runnable after it is consumed. Field sampling and particle substeps are deterministic single-threaded work, so frame bytes are identical on the same host and build.

Refactor `cli_gpu::render_offscreen` into a reusable `HeadlessRenderSession` that initializes one adapter/device/`DrawState` and renders many frames; the existing single-frame wrapper keeps its signature. Headless limits are checked per frame against that frame's scene counts; the first over-limit frame fails with an actionable error. Do not pre-sample all frames just to size one maximum.

## CLI

New native-only clap/config fields:

```
--animation-format <gif|mp4|webm>
--animation-fps <1..30>          default 12
--animation-loop                  GIF only
```

`--duration` and `--start-time` define the animation span when `--animation-format` is present:

- start = existing `--start-time` (default 0),
- frame times = start + i/fps,
- duration = existing `--duration` (default changes to 4 s in animation mode if omitted; still-image default stays 0),
- particles are deterministically preadvanced from 0 to `start_time` using `--fixed-step`; in animation mode `--duration` is the frame span and does **not** add particle preadvance time,
- output extension must equal the animation format: `.gif`, `.mp4`, `.webm`.

Still PNG/SVG semantics do not move: `--duration` there is still the particle preadvance span, with the snapshot at `start + duration`. The animation-mode reinterpretation is explicit and tested.

No `--animation` boolean is needed: the format flag selects animation. Without it, still PNG/SVG behavior is unchanged. JSON config mirrors the three fields. `--animation-loop` with video is a validation error (not ignored), because the format cannot express it.

`run_export` branches before still rendering:

```
if animation_format.is_some() {
    prepare_animation_options ...
    prepare_animation -> AnimationSnapshot (Formula/particles)
    HeadlessRenderSession -> loop frames -> banner -> FrameSink
    finish + size check + atomic rename
}
```

CLI animation supports only the modes the CLI already configures. Attempting to add induction-specific flags is out of scope.

## GUI

- File menu adds **Export animation…** with a format selector, duration, fps, loop (GIF), width/height, and an explicit **Start** button.
- The action is disabled with a reason when the current scene is not animatable:
  - Formula without `t` or particles: `Animation needs a time-dependent formula or moving particles.`
  - Induction not animated: `Set a nonzero amplitude and frequency to animate this induction case.`
  - Sources/Materials/ODE/scalar plot: `Animation export supports time-dependent Formula fields, Induction and particles.`
- Default duration 4 s, fps 12, current viewport pixel size clamped to the animation pixel budget, GIF format. There is no GUI start-time field: export begins at the captured `State::time` for every animatable mode, including particles.
- The dialog shows the exact rendered frame count and estimated pass before starting.
- Native save dialog selects the path; wasm starts a GIF download. Cancel stops the job and restores playback.
- Progress is shown in the existing toast/status area; the app requests repaint only while a frame is in flight.
- While exporting, disable scene mutation and playback controls, block screenshot requests, and show a cancel action. The live scene can stay visible but playback is paused.

## Provenance and warnings

At export start capture `ExportProvenance` from the current mode exactly like still PNG/SVG:

- Formula/particles: no extra model warning unless the current formula/source is already annotated.
- Induction: existing `Induction - quasistatic axisymmetric softened model - a = …` and accuracy/unavailable lines.
- GIF metadata is not attempted. MP4/WebM metadata is the only metadata contract; warning text is always painted.
- If sampling is unavailable: the existing unavailable/invalid warning, never an empty frame labeled valid.

Every frame gets `apply_export_banner` with the same provenance; warning text is visible even when metadata is stripped. The first frame does not hide it.

Metadata:

- GIF: no metadata; visible banner is the contract.
- MP4/WebM: `-metadata comment=<warning + model/softening/accuracy>` through ffmpeg; also `title=Vector Field Explorer animation`.
- CLI Formula animation may have no warning; no fake model metadata.

## Tests

### Pure/spec
- `frame_count_and_times_do_not_duplicate_the_endpoint`
- `animation_limits_reject_bad_specs` (fps, duration, frames, pixels, odd video, nonfinite)
- `gif_delay_quantizes_predictably`
- `ffmpeg_args_select_the_right_codec_and_metadata`
- `ffmpeg_absence_is_an_actionable_error`

### GIF
- Encode a 3-frame synthetic animation; decode with the `gif` decoder; assert 3 frames, dimensions, delays, loop count.
- Assert frames differ in a pixel region and provenance banner pixels exist in every frame.
- Assert output is bounded and atomic (temporary path removed/renamed).
- wasm compilation only; no browser decode claim.

### Video
- Fake `ffmpeg` executable test: capture argv and stdin byte count, return success; assert exact codec/args and frame count.
- Fake failure test: nonzero exit propagates stderr and leaves no final output.
- Real ffmpeg encoding is a gated native integration test when `VFE_FFMPEG` is set; it is not claimed if absent.
- Oversize-output path deletes the final file and reports the limit.

### Frame source and CLI
- Deterministic Formula: `t`-dependent field at frame 1 and frame 5 differs, and capturing the same app state twice produces identical frame data.
- Particle animation: positions at frame N equal a direct fixed-step advance; no wall-clock dependence.
- Induction snapshot: animated case changes; static case is rejected with the exact reason.
- CLI config/flag round-trip, output extension mismatch, `--animation-loop` with video, invalid start/duration.
- CLI animation uses one headless session (test seam counts `HeadlessRenderSession::new` calls == 1).
- Invalid/limit paths don't create a partial final file.

### GUI/job
- Accesskit real-widget test: animation menu/dialog, frame-count text, disabled reasons for Formula-without-t, Induction-static, ODE, Materials.
- State machine test with a fake renderer/encoder: prepares, renders N frames, encodes, reports done, cancellation stops, failure restores playback.
- Provenance captured at start and applied to every synthetic frame.
- Existing screenshot/export tests remain green.

### Evidence
- Native full suite, build, clippy, docs, diff, smoke, wasm check.
- GIF from a real time-dependent formula and an induction case, decoded frame count and warning pixels.
- Video with real ffmpeg only if available on the host; otherwise report unavailable, not passed.
- Benchmark: frame build + GIF encode time for 120 frames at 1280×720, host/spec recorded. No fps claim for GUI/GPU.

## Builder-check resolutions

- Particle advancement gains an exact-dt path with bounded 1/120 substeps; export never uses the GUI `MAX_STEP` clamp and ignores `speed`.
- CLI keeps still `--duration` semantics; animation mode uses `--start-time` as the particle preadvance target and `--duration` as the frame span. The distinction is documented and tested.
- GIF uses per-frame local palettes; looping is an infinite repeat extension, single-play omits it.
- Windows is supported: `atomic_replace` removes an existing destination before rename; all failures delete temporaries.
- `VFE_FFMPEG` is both the runtime override and the test gate. Missing ffmpeg is an actionable error.
- The byte cap is enforced mid-stream for GIF and after completion for video; every error/cancel deletes the temporary file.
- Headless size limits are checked per frame, with a clear first-over-limit error.
- Determinism means two captures from the same app state produce identical frame bytes; no snapshot rewind API is required.
- GUI runs at most one offscreen animation render and one live paint per app frame; screenshot and animation capture are mutually exclusive.
- wasm size limits are validation errors, not silent clamps.
- ffmpeg metadata is sanitized to one line and capped; GIF metadata is dropped.
- GUI particle export starts at the captured time; there is no start field.

## Risks

- **ffmpeg availability**: video is optional and must fail clearly; GIF remains the guaranteed path. Tests use a fake ffmpeg so CI does not depend on it.
- **GIF color quantization**: large neodymium/viridis frames may band. Use the encoder's standard quantization; do not add dithering in this issue. Document the limitation.
- **Main-thread stalls**: GUI export renders one frame per app frame on native and writes to a worker; wasm GIF is bounded to 2048² and yields between frames.
- **State cloning**: `State` may hold large caches; clone once per export, not per frame. Particle cloud clone is bounded by `MAX_PARTICLES`.
- **Two renderers**: GUI uses the installed `DrawState`; CLI uses one headless session. The snapshot/scene assembly is shared; the GPU wrapper differs only by device acquisition.
- **Semantics of `--duration`**: only changes when `--animation-format` is present; documented and tested so still exports do not move.
- **Loop metadata**: GIF repeat is explicit; video ignores loop and rejects `--animation-loop`.

## Acceptance mapping

- Deterministic GIF/video for Formula, Induction and particles: frame source, job, CLI and GUI tests.
- Shared clock/no private timer: snapshots step explicit `1/fps`; live clock is paused, never used for dt.
- Bounded: spec limits, streaming encoders, byte cap, one frame in flight.
- Warnings/provenance: per-frame banner + format metadata; unavailable/invalid never silently valid.
- Native + wasm: native full path; wasm GIF path compile/runtime by design, browser manual waived.
- CLI options: format/fps/loop, config parity, validation and atomic output.
