# Issue — Arrow lighting uses local-space normal against world-space light

Status: ready-for-agent
Spec: .scratch/field-viewport/spec.md (background: wgpu arrow renderer)

## What

In the arrow shader the per-face normal is built in the arrow's local space
(shaft radial, cone slope, base plate) but is dotted directly against the
fixed world-space light direction from the shader globals. Every arrow whose
direction is not close to +Y receives wrong shading (over/under-lit faces
that don't correspond to the actual scene light).

## Fix

Transform the normal to world space before the half-lambert dot: because the
instance basis is orthonormal, `b * normal` is sufficient — no
inverse-transpose needed. One line in the vertex shader where the half-lambert
factor is computed.

## Verification

No unit tests for GPU code (plan-ruled). Verify by build + clippy + full
suite, and — at first visual launch — eyeball that arrows shading is
consistent with a fixed light from above (arrows pointing away from +Y should
no longer look flat/wrong).

## Related minors from the same review (fix opportunistically or route to next task)

- Shader globals declare an inverse view-projection that pipelines compute
  but never read — remove it from both shaders and the Rust Globals, or start
  using it (3D click-seeding in a later task could genuinely use it).
- The renderer helper that sorts instances far-to-near is unused; the app
  sorts earlier, before conversion — dedupe or delete.
- Paint-draw accounting includes the (currently empty) dots array in a
  combined count but never issues a dots draw call; particles task must add
  the real dots draw.
- Per-frame instance buffer recreation works but is wasteful; persistent
  buffers + write_buffer are the cheap improvement.
- Struct holding the pipelines is named per implementation choice (DrawState)
  where the task brief said Renderer — either rename or record a naming
  ruling.
