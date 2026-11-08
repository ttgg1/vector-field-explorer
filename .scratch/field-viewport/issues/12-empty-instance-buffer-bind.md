# 12: Renderer panics when an instance collection is empty (zero-size buffer bind)

Status: ready-for-agent
Spec: `.scratch/field-viewport/spec.md` (background: wgpu arrow renderer)
Blocks: ticket 11's runtime verification, and any visual check (06, 08) — the app
panics on the first painted frame.

## What

After fixing ticket 11 (the arrow shader parse error), startup advances one step
further and panics in the first per-frame upload:

```text
thread 'main' panicked at .../wgpu-30.0.1/src/backend/wgpu_core.rs:1280:26:
wgpu error: Validation Error

Caused by:
  In Device::create_bind_group, label = 'vfe dot bind (frame)'
    Buffer with 'vfe dots (frame)' label binding size is zero
```

Dots are empty on a normal startup (no particle draw), which is the first
collection to hit this.

## Root cause

`DrawState::upload` (`src/renderer.rs`, lines ~336–394) recreates each instance
buffer with:

```rust
device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
    label: Some("vfe dots (frame)"),
    contents: bytemuck::cast_slice(scene.dots.as_slice()),
    usage: wgpu::BufferUsages::STORAGE,
})
```

When the slice is empty this produces a zero-sized buffer, and a bind group
cannot bind a zero-size storage buffer. The same latent bug exists for `arrows`
and `lines` — whichever collection is empty first triggers it. The one-instance
placeholder buffers created in `DrawState::new` are safe, but `upload` replaces
them every frame unconditionally.

## Fix

In `upload`, only recreate a collection's buffer + bind group when that slice is
non-empty (the `new()` placeholders stay valid; `paint()` already skips empty
draws). Alternatively, always allocate a one-instance sentinel buffer. Apply the
guard to all three collections, not just dots.

While here, note the parked minor from issue 01: per-frame buffer recreation is
wasteful; persistent buffers + `write_buffer` would remove this whole class of
error if picked up in the same change.

## Verification

- `scripts/smoke-run.sh 15` reaches the timeout with no panic (after ticket 11
  also lands).
- `cargo test --all-targets`, `cargo clippy --all-targets -- -D warnings`, and
  `cargo build --all-targets` stay green.
- Optional: exercise the empty-collection path by launching with particles/3D off
  (default startup already covers it) and toggling each layer on afterwards.
