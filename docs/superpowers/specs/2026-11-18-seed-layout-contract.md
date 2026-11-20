# Seed layout contract (issue #87)

Accepted 2026-11-20. Owner rulings: seeds are document state; world
positions plus their planting plane; line/circle/region generators; editing in
the existing Flow section; named layouts with a 16-layout cap and portable
JSON; one undo entry per accepted edit; explicit seeds replace the automatic
grid.

This contract bounds slices S1 (model and persistence), S2 (generators and
Flow-section editing) and S3 (named layouts and portable text). It does not
authorize new streamline physics, new field evaluations or renderer changes.

## Coordinate record

A seed is planted on a plane and keeps both forms:

```rust
pub struct SeedAnchor {
    pub axis: PlaneAxis,
    pub index: u32,
    pub u: f32,
    pub v: f32,
}

pub struct SeedPoint {
    pub position: Vec3,        // world, authoritative for sampling
    pub anchor: Option<SeedAnchor>,
}
```

- `position` is what streamlines sample; it is always finite and inside the
  scene domain (the domain check uses the same finite/bounds rules as
  sampling).
- `anchor` records the selected plane at planting time so a plane can be
  reopened and the local coordinates restored. It is `None` for seeds placed
  without a plane (3D free placement).
- `PlaneFrame::to_world`/`to_local` convert between forms. Moving the selected
  plane never moves stored seeds: the anchor is a record, not a live binding.
- The editing UI shows both: local `u`, `v` (or `x`, `y`, `z` when no plane is
  selected) with a read-only world readout.

## Collection and limits

```rust
pub const MAX_SEEDS: usize = 256;
pub const MAX_GENERATED_PER_APPLICATION: usize = 64;

pub struct SeedLayout {
    pub seeds: Vec<SeedPoint>,
}
```

- `SeedLayout::validate` rejects non-finite positions, out-of-domain
  positions, more than `MAX_SEEDS` entries, and non-finite anchor components;
  it never repairs.
- `SeedLayout::sanitized` drops invalid seeds, truncates to `MAX_SEEDS` and is
  total (an empty layout is valid).
- An edit that would exceed `MAX_SEEDS` is rejected in full with a message; the
  collection is never partially updated. Sanitising truncation applies only to
  stored or imported records.
- The collection preserves insertion order; generators append.

## Generators

Three pure, deterministic generators, each bounded by
`MAX_GENERATED_PER_APPLICATION`:

```rust
pub fn line_seeds(start: Vec3, end: Vec3, count: u32) -> Result<Vec<Vec3>, SeedError>;
pub fn circle_seeds(center: Vec3, radius: f32, count: u32) -> Result<Vec<Vec3>, SeedError>;
pub fn region_seeds(min: Vec3, max: Vec3, count: u32) -> Result<Vec<Vec3>, SeedError>;
```

- `line`: `count` points evenly spaced including both endpoints; `count >= 2`,
  endpoints distinct.
- `circle`: in the active plane's basis (or world XY with no plane); `count`
  points at even angles starting at angle 0; `count >= 3`, `radius > 0`.
- `region`: deterministic jittered lattice inside `[min, max]`, capped at
  `count`, reusing the lattice and salt/index scheme of
  `explicit_streamline_seeds` (the jitter stays clamped into the requested box,
  exactly as the automatic grid clamps into the domain). Seeds of a sub-region
  are therefore reproducible but do not coincide with automatic-grid seeds.
- Invalid input returns `SeedError` with a message; nothing is clamped
  silently. Generated points are checked against the scene domain and rejected
  rather than clipped.
- Generators are pure functions in a new module; the app converts generated
  points into `SeedPoint`s with the current plane anchor.

## Editing (slice S2)

The Flow section gains a collapsed-by-default "Seed layout" area, visible
only when Streamlines are on and the field is a Formula field (Sources,
Induction, Materials, Maxwell and Ode keep their own seeding):

- Count readout, "Place seed" toggle, "Generate…" controls for the three
  generators, "Clear all".
- Clicking the viewport while "Place seed" is armed adds one seed at the
  picked point (same picking path as click seeding today) and disarms.
- Dragging a seed moves it in the selected plane; the drag is one history
  transaction, committed on release, and a rejected position leaves the seed
  where it was with a message. A drag re-anchors the seed to the plane active
  during the drag (a seed dragged with no plane keeps `anchor = None`).
- Clicking a seed selects it; a numeric editor edits the selected seed's local
  coordinates and the selection can be deleted.
- With a non-empty layout the automatic jittered grid seeds are hidden and the
  density/count control is disabled with a note; "Clear all" restores the
  generated grid. `StreamlineSeedMode::Click` keeps its click-only behaviour
  and composes with an explicit layout by painting both.
- Existing click seeds (the runtime `clicks` list added by viewport clicks) stay
  runtime state and are not converted into the layout: they remain the
  ad-hoc exploration aid, while `SeedLayout` is the saved document seeds.

## History and persistence

- `SeedLayout` is document state: it joins the edit-history snapshot, so
  generator application, placement, drag, deletion, clear and layout apply are
  recorded; each accepted gesture is exactly one entry. Rejected edits record
  nothing.
- Mechanically, `drive_history` already opens one transaction per primary press
  and commits on release, and an unchanged snapshot is a no-op. A rejected drag
  therefore leaves the document unchanged and records nothing; each slice's
  tests pin that reliance rather than adding a second recording path.
- It persists in the settings envelope as part of the view record, and saved
  scenes capture it (scenes stay appearance-neutral but keep document
  geometry). Missing on older envelopes → empty layout.

## Named layouts (slice S3)

```rust
pub struct SeedLayoutProfile {
    pub name: String,          // trimmed, 1..=48 bytes, unique case-sensitively
    pub seeds: SeedLayout,
}

pub struct SeedLayoutLibrary { /* built-in "Default" + <= 16 user layouts */ }
```

- Built-in "Default" is the empty layout (the generated grid); it is never
  stored, exported or deleted.
- User layouts live in the settings envelope beside the appearance and
  interaction profile libraries, capped at `MAX_USER_LAYOUTS = 16`, validated
  in full, duplicate names rejected case-sensitively, imports atomic.
- Portable documents use `SEED_LAYOUT_FORMAT_VERSION = 1`,
  `deny_unknown_fields`, probe the version before parsing, and export user
  layouts only as compact JSON text. Applying a layout replaces the current
  seeds in one history entry; saving or deleting a library entry is not
  recorded in history.
- Applying a layout never changes the document formula, camera, colour
  mapping or simulation.

## Work limits and safety

- Total seeds and per-application caps bound regeneration work; a full layout
  of 256 seeds must stay within the existing streamline cost budget, remembering
  that each seed is a full `trace_at` integration of up to
  `MAX_STREAMLINE_POINTS` steps, not one segment loop.
- All stored and imported positions are finite and inside the scene domain
  before use; a corrupt stored layout is sanitized with a warning and the app
  keeps running.
- No per-frame allocation, generation or layout parsing: generators and
  parsing run only on explicit user action.

## Consumers, ownership and commands

Task 6 also owns this inventory. Each consumer keeps its existing file owner;
the slice adds the seed path without moving other behaviour.

| Consumer | Where | Owner and check |
| --- | --- | --- |
| Viewport picking | `src/app/input.rs` (click-seed path, `ray_seed`), `src/plane.rs` | S2 implementer; `cargo test --test suite plane_test_input_seed` |
| Seed regeneration | `src/viewmodel.rs` snapshot/`push_line`, `click_streamlines_snapshot`, `src/streamlines.rs` `trace_at` | S1/S2 implementer; `cargo test --test suite streamlines seed_layout` |
| History capture/apply | `src/app/history.rs` `capture_document`/`apply_document`, `src/edit_history.rs` | S1 implementer; `cargo test --test suite edit_history_app` |
| Envelope persistence | `src/persistence.rs` `ViewRecord`/`AppSettings`, `src/app/mod.rs` startup | S1 implementer; `cargo test --test suite persistence seed_layout` |
| Saved scenes | `src/scene_document.rs` `SceneDocument::capture` | S1 implementer; `cargo test --test suite scene_document` |
| Flow-section UI | `src/ui/sidebar.rs` Flow section | S2 implementer; `cargo test --test suite sidebar_colormap_ui seed_ui` |
| Raster, SVG and animation capture | `src/app/viewport.rs`, `src/cli_scene.rs`, `src/animation_scene.rs` consume the snapshot only | S1 verifier; `cargo test --test suite cli_svg animation_scene` |

A consumer that turns out to need a new file gets that ownership written into
its slice brief before dispatch; the shared files above stay serialized across
slices.

## Slices

- **S1** — pure model: `SeedPoint`/`SeedAnchor`/`SeedLayout`, validation,
  sanitising, envelope persistence in the view record, EditSnapshot
  participation. No generators, UI or layouts.
- **S2** — generators, Flow-section editing, picking/dragging and history
  integration.
- **S3** — named layout library, portable text, import/export and apply.

## Pinned tests

- Round trip through a rotated plane (`to_world`/`to_local`) preserves the
  planted position within float tolerance; moving the plane leaves stored
  positions unchanged.
- Generators are deterministic for the same input and honour their caps;
  non-finite, zero-size and out-of-domain input is rejected with a message.
- A layout of `MAX_SEEDS` seeds validates; `MAX_SEEDS + 1` is rejected and
  sanitising truncates; an edit that would exceed the cap is rejected whole.
- A drag re-anchors to the plane active during the drag, and a rotated-plane
  round trip still reproduces the stored local coordinates.
- One drag is one undo entry; a rejected drag records nothing and keeps the
  seed; `Clear all` is one entry and restores the grid.
- Envelope round trip, older-envelope default, corrupt-layout sanitising and
  non-dirtying startup restore.
- Layout library: cap, duplicate rejection, built-in protection, atomic import,
  version rejection, compact JSON round trip, apply replaces seeds in one
  entry without touching other state.
