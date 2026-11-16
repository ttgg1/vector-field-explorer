# 2D views as selected slices of a 3D field — design

**Status: PROPOSED — awaiting owner approval of this document.**

Date: 2026-11-16. Owning requirement:
[GitHub #13](https://github.com/ttgg1/vector-field-explorer/issues/13).
Inspected baseline: main `819e025` (after issue #12 merged).

Approval provenance: the owner chose **all three plane axes** and **projected
flow**, selected **Approach A** (“looks good”), and approved the behavior calls
summarized below. Those choices are binding on this design. This document is the
full written design required before planning; it is not approval to implement.

## Intent and success

A 2D view of a three-component field should feel like looking at a selected
slice of that same field, not at a second, unrelated two-component field. The
slice keeps the field's actual values at the selected plane position and uses
the field's actual in-plane velocity. Everything the user sees and seeds in 2D
lives in that slice.

Success means:

- Switching a 3D field to 2D shows a selected plane of the same field, using
  the same formula, domain and selected axis/position.
- The 2D camera, reference grid, arrows, streamlines and particles all use the
  slice's own in-plane coordinates.
- Particle spawning, respawning, click bursts, automatic streamlines and
  click-seeded streamlines stay inside the selected plane.
- Changing the plane or mode cannot leave hidden out-of-plane seeds behind.
- Full 3D visualization and seeding remain unchanged, including the normal
  component that 2D deliberately omits.
- Ordinary two-component fields keep their existing 2D behavior.
- Streamline seeding gains an explicit user count, with `0 = Auto` preserving
  the density-derived default.

### Goals

- Introduce a single, explicit plane-frame abstraction shared by sampling,
  camera/fit/click mapping, reference geometry, streamlines and particles.
- Keep the 2D renderer and shaders in a local `(u, v, 0)` plane; no new 3D
  camera mode.
- Make projected flow one consistent rule: every 2D velocity, magnitude, color
  and integration step uses the in-plane projection of the same 3D field.
- Make plane and mode transitions deterministic: clear frame-bound seeds, then
  rebuild/retrace from the new frame.
- Add an explicit, persisted streamline seed count while preserving the
  existing Auto behavior.

### Non-goals and anti-goals

- No arbitrary cutting-plane orientation: the selected plane is perpendicular
  to X, Y or Z only.
- No normal-component visualization, in-plane normal transport, or attempt to
  reconstruct 3D motion from 2D. The normal component is omitted in 2D and
  remains visible in 3D.
- No change to 3D seeding, 3D advection, 3D camera behavior, or the 3D plane
  dimming policy.
- No change to ordinary two-component field semantics, including the decision
  to keep particles unavailable there.
- No new CLI flag for streamline count. HUD/status segment counts are not
  redefined as seed counts.
- No new preset framework, animation work (#16), contour/volume work, or a
  general field-expression rewrite.
- Do not silently interpolate the field across the plane normal; sample exactly
  on the selected plane.
- Do not clamp or rescale projected vectors to make them look “2D”; projected
  magnitude is the honest in-plane magnitude.

## Approaches considered

### A. Plane-local scene with a projected-flow adapter — chosen

Build a `PlaneFrame` for the selected slice. Sample the 3D field at the frame's
world positions, project velocity onto `right`/`up`, and emit local
`(u, v, 0)` geometry. Point the existing 2D orthographic camera at the local
scene. Trace and advect through a plane-local flow adapter that maps local
points back to world for evaluation and projects the result.

**Why chosen:** it preserves the existing 2D camera, painter order, shaders and
much of the test harness; it puts every plane conversion in one place; and it
makes “stay in the plane” structural rather than a post-hoc filter.

### B. World-space geometry with a rotated 2D camera

Keep all geometry in world coordinates and rotate the 2D camera to look along
the selected plane's normal.

**Rejected:** it requires changing the 2D camera's basis, panning, click
unprojection, fit logic and several shader/test assumptions in every plane
orientation, while still needing a flow adapter to keep seeds in the plane. It
spreads the same conversion across more code.

### C. Extract a separate 2D field and leave the camera alone

Build a new two-component field expression for the slice, then feed the
existing 2D pipeline.

**Rejected:** it creates a second field identity and duplicates evaluation,
time dependence, coloring and click semantics. It also makes the normal
projection implicit and harder to keep consistent between arrows, streamlines
and particles.

## Plane frame

The frame is derived from the current `PlaneView`, `Domain`, and sampling
density. The origin is the plane center. `u`/`v` use world units, so the local
scene keeps true distances and magnitudes.

### Plane grid versus render sampling grid

The plane index must not be derived from the 2D render sampling shape. Add a
distinct **plane grid** concept:

- For a three-component field, the plane grid is `(density, density, density)`
  in both view modes. The 2D render still samples an `n × n` slice of that
  volume; the 2D-mode sampling grid is not the plane grid.
- For a two-component field, the plane grid is the existing `(n, n, 1)` and the
  2D view stays XY.

`PlaneView::clamped`, `sync_plane`, `plane_coordinate`, the plane selector's
index range, and the frame builder all use the plane grid. The 3D render grid
remains `n³`; the 2D render grid remains `n²` (one sample layer on the selected
plane). This keeps the selected world position stable across mode changes and
prevents a one-slice count from collapsing the index to the axis midpoint.

Because a three-component field has a real Z domain in 2D mode, the domain
editor exposes all three axes whenever `field.dimension == 3`, not only when
`is_3d()`. Two-component fields continue to hide the Z axis. `is_3d()` remains
the rendering/sampling-mode predicate.

For a three-component field, with domain center `c` and plane coordinate `p`:

| Axis | Origin | `right` (u) | `up` (v) | `normal` | Local extents |
| --- | --- | --- | --- | --- | --- |
| Z | `(c.x, c.y, p.z)` | `+X` | `+Y` | `+Z` | x extent, y extent |
| X | `(p.x, c.y, c.z)` | `-Z` | `+Y` | `+X` | z extent, y extent |
| Y | `(c.x, p.y, c.z)` | `-X` | `+Z` | `+Y` | x extent, z extent |

`right × up = normal` in every row. The negative `right` for X/Y slices is what
keeps the local right-handed frame looking along `+normal`; it is a screen-basis
choice, not a negation of field values.

Conversions:

- `local = (dot(world - origin, right), dot(world - origin, up), 0)`
- `world = origin + u * right + v * up`

Local extents are computed from the projected domain corners, not assumed to be
the domain's x/y extents. The frame carries finite half-widths and is rebuilt
when the axis, position, domain, or field dimension changes.

For an ordinary two-component field, the 2D view keeps its existing XY plane
and world X/Y camera semantics. Plane controls are hidden and the plane is
ignored, preserving current behavior.

## Sampling and projected flow

The slice sampler:

1. Samples an `n × n` grid across the frame's local bounds (the same density
   resolution used by the 2D view).
2. Maps each local point to world with the frame.
3. Evaluates the existing field at `t`; a missing/non-finite value remains a
   missing arrow, not a fabricated zero.
4. Projects the world velocity onto `right`/`up`, producing a local
   `(u_component, v_component, 0)` vector.
5. Emits local-position arrow data. Existing magnitude-range, color and arrow
   instance code then works in local coordinates.

Projected magnitude is `sqrt(u² + v²)`. It drives arrow length/color,
streamline color and particle speed coloring. The world normal component is
never folded into the in-plane magnitude and is never used for 2D integration.

This one sampler is the source of truth for 2D arrows. Streamlines and particles
use the same projection through a small flow adapter rather than reimplementing
the dot products.

## Camera, view commands, click mapping and references

- The 2D camera remains the existing fixed front view. Its target is a local
  `(u, v)` point; pan continues to move target x/y, now interpreted as local u/v.
- `plane_size` becomes the selected frame's local in-plane size, not world
  x/y size.
- Fit uses the local frame extents; Reset returns the 2D target to the local
  origin. Fit/Reset/recentre code must not use world X/Y extents in 2D.
- Entering 2D or changing the selected plane frames the new slice with the
  existing 2D Fit calculation so a previously panned/zoomed view cannot hide
  it. Entering 3D continues to preserve the 3D camera pose as today.
- A 2D click unprojects through the same orthographic camera into local
  `(u, v, 0)`, then maps to world for field evaluation and click-seed history.
  The click plane is the selected frame, not the world z midpoint.
- 2D reference grid and axes are built in local coordinates. Axis colors follow
  the world axis each local direction represents (for example an X slice's
  horizontal right is `-Z`, colored as the Z axis, and its vertical up is `Y`).
- 3D references, camera and clicks remain world-space and unchanged.

## Streamlines

- Automatic/grid streamlines seed across the active scene: the local plane for
  a 3D field in 2D, the full world volume in 3D, and the existing XY domain for
  a two-component field in 2D.
- Tracing in 2D evaluates the projected flow at local points and stays in the
  local plane by construction. The traced points are local; colors use projected
  magnitude.
- Click-seeded streamlines use the local click point and plane-local burst
  offsets. They too trace the projected flow and are cleared when the plane or
  mode changes. Click seeds are stored in the frame they were created in; no
  seed survives a frame change.
- 3D tracing continues to evaluate the full world field and uses the existing
  camera-ray/burst behavior.
- Auto streamlines are recomputed from the new frame after a plane/mode change;
  click-seeded lines do not survive.

### Streamline count

Add a user-facing `streamline_count: u32` state value:

- `0 = Auto` preserves today's density-derived seed count. In 3D that is the
  existing per-axis lattice cubed; in 2D it is the existing in-plane lattice
  squared.
- `1..=4096` requests exactly that many automatic seeds. The lattice shape
  depends only on `N` and the active dimensionality, never on density:
  - 3D volume: `n = ceil(N^(1/3))`, build an `n × n × n` jittered lattice and
    take the first `N` points.
  - 2D plane (and the existing two-component 2D view): `n = ceil(sqrt(N))`,
    build an `n × n` jittered lattice and take the first `N` points.
  The result is stable across rebuilds and camera changes. A density change
  rebuilds the same explicit seed positions; only the Auto path changes its
  count and lattice with density. Jitter remains deterministic for the frame
  and seed ordinal.
- Click-seeded streamlines remain the separate 5-seed burst and are not
  counted against this number.
- Status/HUD “streamline count” continues to describe emitted segments/points,
  not the seed setting.

The control is a `DragValue` in the Feature section's streamline controls,
directly after the Seed mode row, formatted as `Auto` at 0, clamped to
`0..=4096`, included in the `Controls` mirror and persistence record with a
serde default of 0. It is enabled only when streamlines are on and Seed is
Auto; Click-only mode disables it with an explanation. It is not added to the
CLI in this issue. Reset returns it to Auto.

## Particles

2D particles are enabled for three-component fields in the 2D view. Ordinary
two-component fields keep particles disabled, preserving current behavior.
The particle target and clock-consumer predicate must both recognize the new
case.

- Each 2D particle cloud lives in the current frame's local coordinates. The
  cloud is cleared whenever the selected plane or mode changes, so a cloud
  never spans coordinate frames.
- Spawning uses the local plane bounds; no z jitter and no world z midpoint.
  Particle count, Auto behavior, traits and click-burst counts keep their
  existing semantics.
- Advection evaluates the projected flow at local points. A zero projected
  velocity triggers the existing stagnation/respawn behavior rather than
  drifting along the omitted normal.
- Dot and trail colors use projected magnitude. Rendering uses local positions.
- Click bursts spawn in the plane around the click point. Undo restores the
  particle/click history for both 2D and 3D when the relevant feature is on.
- `SimClock` treats 2D particles as a clock consumer while they are active, so
  animation and repaint continue in 2D.

## Transitions

A scene-frame identity consists of mode (`2D`/`3D`), field dimension, plane
axis, plane index, domain extents and density. Domain, density and the plane
coordinate matter because they change the plane's world coordinate or local
geometry even when the axis is unchanged. Click seeds are stored in scene
coordinates together with the frame identity that created them; the undo record
carries the same identity. Geometry generation (frame identity mismatch) is what
controls clearing, not merely a mode/axis change.

When the identity changes:

1. Clear the particle cloud and its trails/burst history.
2. Clear click-seeded streamlines and the undo-seed record. Keeping a seed in a
   changed frame could produce an invisible or out-of-domain line; clearing is
   the deterministic choice. Formula edits that keep the same dimension and
   frame geometry may keep click seeds, as today.
3. Rebuild the scene from the new frame and retrace automatic streamlines.
4. If particles are on, let the normal spawn path refill the cloud in the new
   frame; do not reuse old positions.
5. For 2D geometry changes, reset/fit the local camera as described above.

Density and domain changes keep their existing plane clamping rules but change
the frame identity, so they also clear seeds and rebuild. A plane index that no
longer exists is clamped/recentred before the frame is built.

Full 3D transitions back from 2D preserve the 3D camera and 3D seeds are rebuilt
through the ordinary path.

## UI

- The plane selector is available in both 2D and 3D for three-component fields.
  It offers axis X/Y/Z and the plane position/index. In 2D, the plane is always
  the displayed slice; the 3D “dim outside plane” checkbox is hidden because it
  has no meaning there. In 2D the selector ignores `PlaneView.enabled` and does
  not modify it, so returning to 3D restores the user's previous dim toggle
  unchanged. In 3D, the existing enable/dim behavior remains.
- The selector's index range uses the conceptual `n³` sampling grid even though
  the 2D slice samples `n²`, so the selected world position stays consistent
  when switching modes.
- The 2D formula hint continues to describe a 3D expression where relevant; no
  new scalar/vector syntax is introduced.
- A `Streamline count` DragValue (Auto/1–4096) sits with the streamline Seed row
  in the Feature section, matching the particle-count pattern. It is enabled
  only for Auto seeding and explains why it is disabled in Click-only mode.
- Help/docs gain a concise explanation: 2D shows a selected slice, uses projected
  in-plane flow, omits the normal component, and does not imply hidden 3D
  motion.

## Persistence

- Add `streamline_count` to the persisted view record with default 0; older
  records load as Auto.
- Restore/sanitize it through the ordinary Controls path and clamp it to
  `0..=4096`.
- Plane selection remains session state, as today; this design does not add a
  new persisted plane field.
- Global reset restores streamline count to Auto and returns plane state to its
  default.
- No CLI flag and no persisted particle-frame data.

## Testing strategy

Unit/state tests:

- Frame derivation for X/Y/Z: origin, basis, `right × up = normal`, local
  bounds, and local↔world round trips.
- Slice sampling: exact plane coordinate, projected direction, projected
  magnitude, normal component omission, invalid samples remain gaps.
- Camera/fit/pan/click in 2D use local extents and map the click to the selected
  plane; X/Y slices are not treated as XY.
- Streamlines in 2D stay within the local plane, including click bursts;
  projected magnitude drives color; 3D traces and colors are unchanged.
- 2D particles spawn/advect/color in the plane, stay in frame, respawn on zero
  projected flow, and participate in the clock; 2D-field particles stay
  disabled.
- Mode/plane/domain/density transitions clear stale seeds and rebuild; a
  formula edit that keeps the frame geometry does not clear click seeds.
- Streamline count: Auto unchanged, explicit exact count for 1 and 4096,
  lattice shape depends only on N, deterministic rebuild, density-independent
  explicit positions, clamping, reset, persistence round trip.

UI/integration tests:

- Plane axis/index controls are reachable in 2D and 3D for 3D fields and hidden
  for two-component fields.
- Streamline count DragValue formats Auto, clamps, and persists through the
  Controls path.
- Switching mode shows the selected slice, and switching back restores 3D.
- Existing z-slice-only tests are updated to the new all-axis contract where
  they directly contradict it; tests that merely pin two-component behavior
  remain.

Visual/manual acceptance should confirm that the three axes read correctly,
projected arrows and seeds stay on the plane, and a plane switch leaves no
ghost particles/streamlines.

## Risks and mitigations

- **Basis/sign mistakes** could mirror axes or invert flow. Pin the frame table
  and round-trip tests; use world-axis colors so a mirrored basis is visible.
- **Mixed coordinate frames** could leave invisible particles. Clear all
  frame-bound seeds on every frame change and keep one adapter for conversion.
- **Camera assumptions** are scattered across view commands and tests. Route
  all 2D extents/target/click work through the frame and update the contradicted
  tests explicitly.
- **Streamline count distribution** is necessarily approximate for arbitrary
  `N`; document the deterministic lattice/truncation rule and test exact counts
  and reproducibility rather than promising perfect uniformity.
- **Projected stagnation** can make seeds look short-lived or stationary. This
  is the honest in-plane flow; the existing zero-flow respawn rule prevents
  hidden normal drift.

## Owner decisions recorded

- All three plane axes are supported.
- The 2D view uses projected in-plane flow, omitting the normal component.
- Approach A (plane-local scene plus projected-flow adapter).
- Plane/mode changes clear/reseed particles and retrace streamlines.
- Streamline count: `0 = Auto`, explicit up to 4096, persisted like particle
  count, no CLI flag.
