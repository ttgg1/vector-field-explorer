# Object renderer native performance (issue #115)

Native evidence for the depth-enabled source/induction object pass. This is a
debug-profile measurement on one host, not a CI assertion, not a visual pass,
and not a browser/wasm timing. Browser and manual visual verification remain
owner-waived.

## Method

- Host: AMD Radeon RX 9070 XT (RADV GFX1201), Vulkan, Mesa 26.3.1-arch3.5.
- Build profile: the ordinary debug profile (`cargo run --example`, no
  `--release`), `wgpu` 30 through eframe/egui_wgpu.
- Viewport: 1280×800 RGBA8 render target, perspective camera, clear colour
  `(0.05, 0.06, 0.08, 1.0)`.
- Scene: the bounded 64-object reference scene, cycling all eight source kinds
  in an 8×8 grid inside a `[-4, 4]³` domain. Mapping produced 64 instances and
  35,712 drawn triangles (the #113 budget cap is 150,000).
- Harness: a temporary `examples/object_perf.rs` (removed before hand-off)
  built the scene through `MeshScene::from_sources`, then repeated the exact
  `ObjectPass::prepare` / `ObjectPass::paint` work the live callback does. It
  timed CPU encode time with `Instant`, and GPU time with
  `QueryType::Timestamp` markers around the geometry pass and the composite
  paint pass (`TIMESTAMP_QUERY` + `TIMESTAMP_QUERY_INSIDE_ENCODERS`, 10 ns/tick
  on this host).
- Sampling: 120 CPU frames after warm-up; 60 timestamped GPU frames. The first
  CPU frame includes target allocation and the first instance upload, so the
  steady-state row excludes it. The empty-object baseline used
  `MeshScene::empty()`.
- Both styles were measured by toggling `ObjectView::wireframe`; scene mapping
  and instance count are identical.

## CPU callback encode time

| Scene / style | Prepare p50 | Prepare p95 | Paint p50 | Paint p95 |
| --- | --- | --- | --- | --- |
| Empty object scene | 0.43 µs | 0.45 µs | 0 (no prepared target) | 0 |
| 64 objects, Solid | 49.6 µs | 54.2 µs | 2.29 µs | 2.38 µs |
| 64 objects, Wireframe | 50.4 µs | 55.2 µs | 2.37 µs | 2.45 µs |

One-time costs are outside the steady-state rows: the first Solid prepare took
3.30 ms (target creation plus the initial instance upload), and the first
wireframe prepare took 3.34 ms. Subsequent frames upload nothing because the
scene fingerprint and uniform are unchanged.

## GPU time

Measured between timestamp markers on the same frames:

| Work | Solid p50 | Solid p95 | Wireframe p50 | Wireframe p95 |
| --- | --- | --- | --- | --- |
| Object geometry pass | 0.0448 ms | 0.0456 ms | 0.0614 ms | 0.0626 ms |
| Composite paint pass | 0.0100 ms | 0.0100 ms | 0.0100 ms | 0.0102 ms |
| Total object GPU | 0.0548 ms | 0.0556 ms | 0.0714 ms | 0.0728 ms |

The empty-object baseline records no geometry pass and no composite draw, so
its GPU work is zero and the added GPU time is the whole object total above.
The largest wireframe total (0.073 ms) is well inside the ADR's 2 ms bound on
this host and scene.

## Interpretation and limits

- These are callback encode/GPU-pass timings, not whole-window frame rate.
  Vsync, egui layout, vector geometry and the rest of the app frame are not
  included; a whole-window comparison would need a fixed 1280×800 window and
  frame-time instrumentation and was not measured here.
- Numbers are native RADV only. They say nothing about wasm/WebGPU, integrated
  GPUs, other drivers, or visual correctness.
- The temporary harness was deleted, so this file records evidence rather than
  a repeatable benchmark command. Reproducing it means rebuilding the same
  prepare/paint timing scaffold; it is not part of `scripts/test.sh` or any CI
  gate.
