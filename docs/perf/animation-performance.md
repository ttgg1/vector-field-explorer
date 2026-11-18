# Animation export performance and evidence

Research evidence for issue 71 (2026-11-18). This records native animation
renders, GIF encoding and the real external-ffmpeg attempt. It is not a
frame-time target and makes **no GPU fps claim**: the measured wall time covers
scene rebuild, offscreen GPU render, banner painting, GIF quantization and file
write for the whole export.

## Host and build

- Host: `Schlepptop` (Lenovo 16T90Q-G.AP79G), Intel Core i7-1260P
  (12 cores / 16 threads), 15 GiB RAM, Linux 7.3.3-1-cachyos x86_64.
- GPU: Intel Iris Xe Graphics (ADL GT2), Vulkan, Mesa 26.2.6-arch3.1.
- Toolchain: `rustc 1.99.1 (58cf8f9c3 2026-10-13)`,
  `cargo 1.99.1 (5f94df478 2026-08-27)`.
- Binary: release build at commit `6972f6e` plus this docs task, built with
  `cargo build --release` and `CARGO_BUILD_JOBS=1`.
- `ffmpeg version n9.0.2`, `/usr/bin/ffmpeg`, present on `PATH`.
- `VFE_FFMPEG` was **unset**, so the app used the `PATH` ffmpeg. Setting
  `VFE_FFMPEG=/nonexistent/ffmpeg` produced the actionable missing-executable
  error and no output; that is the only `VFE_FFMPEG` result recorded here.
- One foreground run per case in a shared development session; no CPU pinning,
  frequency locking, warm-up loop, or statistical repetition.

Environment:

```bash
PATH=/home/paul/.cargo/bin:/usr/bin:/bin
RUSTC=/usr/bin/rustc
CARGO_TARGET_DIR=/home/paul/.cache/vector-field-explorer-targets/issue-71
TMPDIR=/home/paul/.cache/vector-field-explorer-targets/issue-71/tmp
```

## GIF benchmark: time-dependent Formula

Exported 120 frames at 1280×720 (12 fps for 10 s) from
`(sin(t), cos(t), 0)` through the **built CLI binary**:

```bash
/home/paul/.cache/vector-field-explorer-targets/issue-71/release/vector-field-explorer \
  --output .superpowers/sdd/2026-11-17-animation-export/evidence/task-5/bench/formula-120.gif \
  --formula "sin(t), cos(t), 0" --width 1280 --height 720 \
  --animation-format gif --animation-fps 12 --duration 10 --start-time 0
```

| Metric | Value |
| --- | --- |
| Requested schedule | 120 frames, `ceil(10 × 12)` |
| Wall time (render + GIF encode) | 2.307 s |
| Output size | 2,606,278 bytes (2.49 MiB) |
| Decoded frames | 120 |
| Decoded dimensions | 1280×720 |
| GIF delay | 80 ms per frame (8 cs) |
| Loop extension | absent (single-play default) |
| First/last frame differ | yes, changed bbox `(362, 70, 909, 647)` |
| Deterministic rerun | identical SHA-256 (`26e3d9d9…c65846`) |

Command output, decode and hash logs are in
`evidence/task-5/bench/formula.log`, `formula-decode.log`,
`content-decode.log` and `determinism.log`. A second identical run
(`formula-120-rerun.gif`) produced the same 2,606,278 bytes and hash.

## GIF benchmark: animated Induction

The shipped CLI deliberately has no Induction flags, so this case used a
benchmark-only external harness (`/tmp/opencode/issue-71-induction-bench`)
built against the same worktree and the same release profile, lockfile-reused
dependencies, public `AnimationSnapshot`/`HeadlessRenderSession`/`gif_sink`
APIs. It is **not** a new production or test path. The scene was an animated
current loop (`radius = 1`, amplitude `1`, frequency `0.5 Hz`, E+B display),
2D density 80, 1280×720, 120 frames at 12 fps for 10 s.

| Metric | Value |
| --- | --- |
| Requested schedule | 120 frames, `ceil(10 × 12)` |
| Wall time (render + GIF encode) | 2.820 s |
| Output size | 3,587,720 bytes (3.42 MiB) |
| Decoded frames | 120 |
| Decoded dimensions | 1280×720 |
| GIF delay | 80 ms per frame (8 cs) |
| Loop extension | absent (single-play) |
| First/last frame differ | yes, changed bbox `(367, 87, 913, 633)` |

Raw output: `evidence/task-5/bench/induction.log`,
`induction-decode.log`, `content-decode.log`, harness build log and
`induction-harness-build.log`.

## Real MP4/WebM attempt: failed, not passed

`ffmpeg` was installed, so real video was attempted rather than recorded as
unavailable. Both formats failed before frame 1 **with the shipped temp-path
policy**:

```text
export: ffmpeg rejected frame 2: Broken pipe (os error 32):
[AVFormatContext ...] Unable to choose an output format for
'.superpowers/.../bench/.formula-120.mp4.tmp'; use a standard extension for
the filename or specify the format manually.
```

WebM failed the same way for `.formula-120.webm.tmp`. The app writes the
sibling temporary filename without the container extension and does not pass an
output `-f`, so ffmpeg cannot select the muxer. No final `.mp4`/`.webm` was
created, and no temp file remained. This is a real-encoder **failure**, not a
pass:

| Format | Command | Result |
| --- | --- | --- |
| MP4 | `--animation-format mp4` | failed before frame 1; no output |
| WebM | `--animation-format webm` | failed before frame 1; no output |

Logs: `evidence/task-5/bench/formula-mp4.log` and `formula-webm.log`. The
GIF path on the same binary and host passed.

The missing-ffmpeg path was separately exercised with
`VFE_FFMPEG=/nonexistent/ffmpeg`; it reported
`ffmpeg video export could not start ... Install ffmpeg or set VFE_FFMPEG to its executable.`
and created no output (`missing-ffmpeg.log`). This confirms the documented
"error, never a GIF fallback" behavior, but it is not a video-codec pass.

## Scope and exclusions

- These are integrated export wall times on one host, not GPU throughput or
  interactive fps. GIF quantization and file I/O are included.
- No browser/WebAssembly runtime, browser decode, or manual visual pass is
  claimed. The wasm compile gate is recorded separately in the task report.
- The Induction GIF is warning-free only because the harness passed the
  default (empty) provenance; production captures the induction warning
  provenance at export start, and every frame receives that banner.
- GIF metadata is not attempted; only the visible banner is the GIF warning
  contract. MP4/WebM metadata is embedded when video succeeds.
