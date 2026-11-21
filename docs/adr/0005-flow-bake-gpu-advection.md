# 0005: Baked flow field and optional GPU particle advection

Particle advection is the remaining CPU cost that scales with particle count:
`particles::advect` evaluates the field at four RK4 stages per particle per
substep (`src/particles.rs:710`), and the P1 field cache does not help because
each particle sits at a different point. P1 removed repeated grid sampling and
P4 removed redundant painter-depth work, but neither touches per-particle
evaluation. P3 was deferred until an ADR and a browser/compute plan existed,
because it changes the simulation model, the accuracy story and the browser
compute strategy.

## Decision

Adopt a **staged** approach, implemented only after this record and its design
are approved:

1. **CPU baked field (deterministic).** For time-independent fields, bake the
   velocity field once per formula/parameter/domain change onto a bounded
   lattice and sample it with trilinear interpolation inside the existing
   `Flow` seam. CPU results stay deterministic and remain the reference path.
   Time-dependent fields keep direct evaluation.
2. **Optional GPU compute advection.** On adapters that advertise compute
   shaders, an opt-in setting may advance particles in a compute pass reading
   the same baked lattice, then read positions back into the existing CPU
   `Particles` state so dots, trails, colouring and captures are unchanged.
   Without the capability, or with the setting off, the CPU path runs exactly
   as today.
3. **No GPU-native trail or particle rendering** in this decision. A compute-
   built trail/vertex path would bypass the painter ordering, CPU scene model
   and capture parity that the renderer and tests depend on; it needs its own
   record.

## Why not alternatives

- **Compute the formula directly on the GPU.** Formulas are parsed by `meval`
  into an AST at runtime; WGSL cannot consume it without a compiler and a
  numeric-accuracy audit.
- **GPU-only advection with no CPU reference.** It would break deterministic
  animation export, CLI output and the existing tests that compare exact
  positions, and would leave browsers without compute unusable.
- **Bake without GPU.** It is still worth doing first because it is bounded,
  deterministic, testable offline and removes most per-particle evaluation cost
  even when compute is unavailable. It is the prerequisite lattice for step 2.

## Accuracy and honesty

- A baked field is an approximation. Resolution, interpolation error and the
  divergence introduced by interpolation must be stated where the result is
  read, not hidden behind the existing normalized-teaching-units caption.
- GPU advection changes f32 evaluation order, so positions will drift from the
  CPU reference. Any measured comparison must report a tolerance and the field
  used, never "identical".
- The CPU path remains the reference for animation export, CLI output and
  accuracy claims unless a later record proves GPU parity for those paths.
- Non-finite samples, particles leaving the domain and stagnation respawn keep
  today's rules; a bake must not silently clamp them into finite motion.

## Browser and compute plan

- Native and wasm share the wgpu device. GPU advection requires
  `DownlevelFlags::COMPUTE_SHADERS`; the path is skipped with a stated reason
  when absent. No browser harness is authorized (P5 was dropped), so wasm
  compute remains **unverified** until the owner tests it manually.
- Native verification uses an env-gated GPU probe on the RX 9070 XT reference
  host, comparing a simple analytic field against the CPU reference within a
  stated tolerance, plus a capability-skip probe.
- Budgets: baked lattice ≤ 64³ (2D ≤ 256²), ≤ ~1.5 MiB, ≤ 4,000 particles,
  one compute dispatch per substep, and a bounded readback. Exceeding a budget
  falls back to CPU with a reason.

## Boundaries

- No general solver, no arbitrary-source full-wave change, no change to the
  single simulation clock, and no second particle model.
- No new dependency in the bake slice; compute uses the existing wgpu device.
- No floor or CI threshold change. Visual verification stays owner-waived.

Status: proposed 2026-11-21. Implementation of step 1 still needs owner
approval of the design and its own claim.
