# Baked flow field and optional GPU advection — design

Status: proposed 2026-11-21. Implements the direction in
[ADR 0005](../../adr/0005-flow-bake-gpu-advection.md). No implementation is
authorized until this design and its first slice are approved.

## Problem

`particles::advect` evaluates the flow at four RK4 stages per particle per
substep. At 4,000 particles and a 60 Hz step that is ~1M field evaluations per
second, independent of the P1 grid cache (each particle is at a unique point).
The particle scene is therefore the one interactive path still dominated by
formula evaluation after P1 and P4.

## Slice 1 — CPU baked lattice (deterministic)

New `src/field_bake.rs`:

- `BakeSpec { axis: BakeAxis, resolution: u32, domain: Domain, formula,
  parameters, field_mode, plane_frame }`
- `BakedField` with `sample(point) -> Option<Vec3>` using trilinear
  interpolation and `None` outside the domain or on non-finite corners.
- `BakeAxis::{Volume3d, Plane2d}`; resolutions `16..=64` (3D) and `32..=256`
  (2D), default `48³` / `128²`; memory asserted against a cap.
- Built only for fields where `is_time_dependent()` is false; a time-dependent
  field returns `None` from the bake constructor and callers keep direct
  evaluation. No silent freezing of `t`.
- Rebuild triggers are exactly the P1 key components that affect the field
  (formula, parameters, domain, mode, plane frame). Stored in `State` beside
  the field cache, runtime-only, never serialised.

Wiring: `SceneFlow::World` and `PlaneFlow` gain a baked variant that implements
`Flow::eval` by sampling; `zero_velocity_is_stationary` is preserved. Particle
and streamline call sites choose the baked flow only when a bake exists;
otherwise behaviour is byte-identical to today. `advance_exact` keeps its
substep loop and exact time; a baked flow is only used when the bake has no
time dependence.

Accuracy: a `BakeAccuracy` note carries resolution, interpolation error
measured on a linear and a quadratic test field, and the divergence residual on
a divergence-free test field. The UI shows the note beside the particle
controls; export provenance records "baked N³" when active. Values stay in
normalized teaching units.

Tests: linear field reproduced to f32 tolerance at interior points; quadratic
field error bounded and reported; outside-domain returns `None`; time-dependent
field refuses to bake; non-finite corner handling; determinism (same bake, same
trajectories); byte-identical particle positions when the bake is disabled;
memory cap; rebuild on each key change.

## Slice 2 — optional GPU compute advection

- One compute pipeline in `src/object_renderer.rs`-style isolation
  (`src/particle_compute.rs`), reading a storage buffer of baked lattice values
  plus a particle position buffer, writing new positions for one substep.
- Positions read back after the dispatch and written into `Particles` so all
  existing rendering, colouring, trails and captures continue to consume CPU
  state. Trails are pushed on the CPU from the read-back positions.
- Capability gate: `DownlevelFlags::COMPUTE_SHADERS` plus storage-buffer limits;
  otherwise the CPU baked path runs and the setting is disabled with a reason.
- Opt-in, persisted app setting (not document state), default off until the
  native probe passes on the reference host.
- Probe: simple analytic linear field, compare GPU positions against the CPU
  reference after N substeps, report max error; second probe asserts a clean
  capability skip. No browser claim.

## Slice 3 — explicitly out of scope

GPU-native trail history, GPU particle rendering and compute-side colouring.
They would require a new painter/ordering and capture contract.

## Boundaries

- No new dependency, no change to the simulation clock or step semantics, no
  change to the particle document schema, no solver or source-model change.
- No floor or CI performance threshold; the bake slice records its own
  before/after on the P1 harness scenes, and the P1 grid-cache measurements
  (default-2D 562.9 → 4.25 µs, dipole-3D 9,543.0 → 15.2 µs) are the
  reference for what repeated evaluation costs.
- Visual verification remains owner-waived; wasm compute stays unverified and
  must be described that way.
