# 11: Arrow shader fails to parse — missing `return` in `ring_point`

Status: ready-for-agent
Spec: `.scratch/field-viewport/spec.md` (background: wgpu arrow renderer)
Blocks: any runtime/visual verification (tickets 06, 08) until fixed — the app panics on startup.

## What

Launching the app panics during `DrawState::new` while creating the arrow shader
module:

```text
thread 'main' panicked at .../wgpu-30.0.1/src/backend/wgpu_core.rs:1084:30:
wgpu error: Validation Error

Caused by:
  In Device::create_shader_module, label = 'vfe arrow shader'
Shader 'vfe arrow shader' parsing error: expected `;`, found "}"
   ┌─ wgsl:54:1
   │
54 │ }
   │ ^ expected `;`
```

(The `lsfg-vk` config-key errors and the radv "not a conformant Vulkan
implementation" warning in the same log are environment noise, unrelated.)

## Root cause

WGSL has no implicit return: a function's final expression must be returned
explicitly with `return …;`. `ring_point` in `src/shaders.arrow.wgsl` ends with a
bare constructor call:

```wgsl
fn ring_point(theta: f32, y: f32, radius: f32) -> vec3<f32> {
    vec3<f32>(cos(theta) * radius, y, sin(theta) * radius)   // line 53 — no `return`
}
```

naga therefore rejects the whole module, and wgpu panics when the module is
created. This is runtime-only: `cargo build` / `cargo test` never create a wgpu
device, so CI cannot catch shader parse errors.

## Fix

Add the missing `return` in `src/shaders.arrow.wgsl` line 53:

```wgsl
return vec3<f32>(cos(theta) * radius, y, sin(theta) * radius);
```

Audit the other shaders for the same mistake while there. `src/shaders.line.wgsl`
and `src/shaders.dot.wgsl` already return explicitly, but `src/shaders.wgsl`
(the old combined shader, not referenced by any `include_str!`) has the same
missing `return` in `ring_point` at lines 64–65. Delete that unused file as
cleanup, or fix it in the same change if a later task intends to use it.

## Verification

No GPU unit tests (plan-ruled; device creation is unavailable in CI), so:

- `cargo build --all-targets` and `cargo test --all-targets` stay green.
- `cargo run` reaches the window with no wgpu validation panic; arrows render
  and are lit (this also re-confirms the ticket-01 `b * normal` fix).
- Optional hardening for a follow-up: a CPU-only naga parse test over each
  `include_str!`'d shader would have caught this without a GPU.

**See also ticket 12:** fixing this exposes an independent empty-instance-buffer
bind panic in `DrawState::upload`. Both must land before `scripts/smoke-run.sh`
can pass.
