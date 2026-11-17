# Fields in Matter — Bounded Analytic Material Cases Design

Status: APPROVED by owner on 2026-11-17
Date: 2026-11-17
Issue: GitHub #29 (planning approval under #17; detailed design/plan approval required before implementation)
Dependencies: #24 source evaluator, #25 source panel, #26 source visualization, #27 induction demo, #28 ODE workbench. #18 performance evidence is administratively deferred by owner decision and is not a passed floor.

## Intent

Add a teaching-grade way to inspect how simple materials reshape applied fields without pretending to solve arbitrary electrostatics or magnetostatics. Four canonical analytic cases are supported:

1. a dielectric sphere in a uniform applied electric field;
2. a planar dielectric interface in a uniform applied electric field;
3. a permeable sphere in a uniform applied magnetic field;
4. a planar permeable interface in a uniform applied magnetic field.

Each case shows **Total**, **Applied**, and **Response** separately. The response is the bound/induced part, defined as `total - applied`. E and B are never summed or presented as one vector family.

## Goals

- Pure CPU analytic model, native and wasm, no new dependencies.
- Exact canonical formulas, not FEM/BEM or numerical inversion.
- Explicit geometry, background constants, applied excitation, and approximation assumptions.
- Explicit field view: Total, Applied, Response.
- A dedicated Materials panel/mode beside Formula, Sources, Induction, and ODE.
- 2D view as a slice of the same 3D analytic case through the existing `PlaneFrame`.
- Clear applicability/status text when parameters or geometry are outside the catalog.
- Persistence, migration, reset, and export warning/metadata.
- Independent analytic/reference tests for interface conditions, limiting cases, and decomposition.

## Non-goals

- No arbitrary source/material superposition, overlapping materials, multiple inclusions, anisotropic/nonlinear/frequency-dependent media, conductors, free surface charge, magnetic hysteresis, or eddy currents.
- No FEM, general BEM, general inverse problem, or arbitrary geometry.
- No separate 2D material solver; 2D is a slice of the 3D case.
- No new CLI flags, private clock/timer, per-material softening, or wasm CAS.
- No claim that global εr/μr turn Formula/Sources into a general material solver.
- No particle/streamline/click behavior in Materials.
- No new glyphs unrelated to the material geometry; only simple outline geometry for the selected canonical case.

## Approaches Considered

**A — dedicated `FieldMode::Materials` + bounded analytic catalog (chosen).** A `MaterialScene` owns one case, background constants, applied field, and view selector. A sibling sampler evaluates exact formulas at world points or the selected plane and returns arrows plus applicability status. Reuses vector/plane/persistence/export seams and keeps Formula/Sources/Induction/ODE isolated.

**B — extend the Sources stack with material entries.** Reuses source sampling and glyphs, but forces material response into source semantics and invites uncontrolled superposition of material objects. It cannot honestly enforce the bounded catalog or the applied/response split without a separate solver layer.

**C — derive material response numerically from the formula field.** Requires a general boundary-value solver or finite-difference approximation, cannot meet the independent analytic acceptance tests, and is explicitly outside scope.

Chosen: **A**.

## Physical Model and Assumptions

The model is homogeneous, linear, isotropic, nondispersive, nonconducting, and quasistatic. Each case has one inclusion in an infinite homogeneous background. No free charge/current lies on an interface; only bound charge/current appears. The applied field is uniform at infinity. The visualization domain clips an infinite case; it does not change the analytic solution.

`ε_bg = ε0 εr_bg`, `ε_in = ε0 εr_in`, `μ_bg = μ0 μr_bg`, `μ_in = μ0 μr_in`. Relative constants are finite and `>= 1.0` (teaching range `1.0..=100.0`). The applied field is finite, has nonzero magnitude, and is stored per case in SI-like display units.

No Plummer softening is applied: these are exact canonical fields with no point singularity. The existing source softening setting is irrelevant and must not be shown as affecting Materials. The only numerical limitation is interface discontinuity handling.

### Case 1 — dielectric sphere

Sphere radius `a > 0`, centered at the origin. Uniform applied `E0`. Let `r = |p|`, `rhat = p/r`.

```
beta_e = (eps_in - eps_bg) / (eps_in + 2 eps_bg)
outside: E = E0 + beta_e * a^3 * (3 (E0 . rhat) rhat - E0) / r^3
inside:  E = 3 eps_bg / (eps_in + 2 eps_bg) * E0
```

At `r=0` the inside value is used. At `r=a` the field is discontinuous; points exactly on the interface (within the tolerance) are omitted, while near-interface points on either side remain valid.

Boundary checks: tangential E continuous, normal D continuous, outside dipole decays as `1/r^3`, no contrast (`eps_in=eps_bg`) gives `E=E0` everywhere.

### Case 2 — dielectric plane interface

Plane through `origin + offset * normal`, normal `n` (axis X/Y/Z). Background side is `n·p < offset`; material side is `n·p > offset`. Uniform applied `E0 = n E0n + E0t`.

```
tangential: E_t = E0t on both sides
normal:     E_bg_n = E0n
            E_in_n = (eps_bg / eps_in) E0n
```

The total field is `n E_n + E_t` on each side. Thus normal D is continuous, tangential E is continuous, and free surface charge is zero. The response is nonzero only in the normal component when `eps_in != eps_bg`.

### Case 3 — permeable sphere

Sphere radius `a > 0`, centered at the origin. Uniform applied `B0`. Let `r = |p|`, `rhat = p/r`.

```
beta_m = (mu_in - mu_bg) / (mu_in + 2 mu_bg)
outside: B = B0 + beta_m * a^3 * (3 (B0 . rhat) rhat - B0) / r^3
inside:  B = 3 mu_in / (mu_in + 2 mu_bg) * B0
```

Boundary checks: normal B continuous, tangential H continuous, outside dipole decays as `1/r^3`, no contrast (`mu_in=mu_bg`) gives `B=B0` everywhere.

### Case 4 — permeable plane interface

Plane geometry is identical to Case 2. Uniform applied `B0 = n B0n + B0t`.

```
normal:     B_n = B0n on both sides
tangential: B_bg_t = B0t
            B_in_t = (mu_in / mu_bg) B0t
```

Normal B and tangential H are continuous. The response is nonzero only in the tangential component when `mu_in != mu_bg`.

### Applied / response decomposition

For every sample point, the sampler computes:

```
total:    the case field above
applied:  E0 or B0
response: total - applied
```

The view selector draws exactly one of these vectors. The total is never silently replaced by the applied field; the response is labelled as the bound/induced contribution. For plane cases, `response` is piecewise and vanishes in the no-contrast limit.

### Interface handling

The field is discontinuous at `r=a` for spheres and at the plane surface for boundaries. A sample whose point lies exactly on the interface (within `INTERFACE_TOLERANCE = 1e-9 * max(domain extent, 1.0)`) is omitted because no single total value exists there. Ordinary samples on either side are kept; the discontinuity is not averaged, extrapolated, or interpolated, and the grid is not a smooth reconstruction of the interface.

It records `omitted_interface` and includes the exact warning text when any point is omitted:

`Samples exactly on the interface are omitted because the analytic field is discontinuous there.`

The panel states that this is an exact-hit guard, not a band of omitted near-interface samples. A view whose selected response is below the degenerate threshold everywhere is still meaningful: the sampler records `response_empty = true` and the UI/export shows `Response is identically zero for this view.`

### Applicability rules

`MaterialScene::validate` rejects and the panel explains:

- non-finite/zero/negative applied vector;
- relative constants outside `1.0..=100.0`;
- radius outside `> 0` or non-finite for sphere cases;
- interface offset non-finite for boundary cases;
- invalid axis;
- unsupported case/view (exhaustive enum, no user path);
- whether an empty domain or impossible grid can produce any sample; this is reported by sampling as a warning, not by `validate_material`, which has no domain argument.

The panel always shows the shared assumptions, case name, family, and a note that the domain is only a visualization window. It does not invent an uncertainty number for an exact formula.

## Architecture / APIs

### New pure module `src/material.rs`

```rust
pub const MATERIAL_DEFAULT_RADIUS: f64 = 1.0;
pub const MATERIAL_DEFAULT_INTERFACE_OFFSET: f64 = 0.0;
pub const MATERIAL_INTERFACE_TOLERANCE_FACTOR: f64 = 1e-6;
pub const MATERIAL_MAX_RELATIVE: f64 = 100.0;
pub const MATERIAL_MIN_RELATIVE: f64 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialCase {
    DielectricSphere,
    DielectricBoundary,
    PermeableSphere,
    PermeableBoundary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialView { Total, Applied, Response }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialFamily { Electric, Magnetic }

#[derive(Debug, Clone, PartialEq)]
pub struct MaterialScene {
    pub case: MaterialCase,
    pub background_epsilon_r: f64,
    pub background_mu_r: f64,
    pub material_epsilon_r: f64,
    pub material_mu_r: f64,
    pub radius: f64,
    pub interface_axis: PlaneAxis,
    pub interface_offset: f64,
    pub applied: [f64; 3],
    pub view: MaterialView,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaterialError {
    InvalidScene(String),
    Unsupported(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct MaterialSampling {
    pub arrows: Vec<ArrowData>,
    pub family: MaterialFamily,
    pub case: MaterialCase,
    pub view: MaterialView,
    pub background_epsilon_r: f64,
    pub background_mu_r: f64,
    pub material_epsilon_r: f64,
    pub material_mu_r: f64,
    pub applied: [f64; 3],
    pub omitted_interface: usize,
    pub response_empty: bool,
    pub assumptions: &'static str,
    pub warning: Option<String>,
}
```

Public functions:

```rust
pub fn material_family(case: MaterialCase) -> MaterialFamily;
pub fn material_assumptions(case: MaterialCase) -> &'static str;
pub fn material_warning(case: MaterialCase) -> &'static str;
pub fn validate_material(scene: &MaterialScene) -> Result<(), MaterialError>;
pub fn material_field(scene: &MaterialScene, point: Vec3) -> Result<Vec3, MaterialError>;
pub fn material_applied(scene: &MaterialScene) -> Vec3;
pub fn material_response(scene: &MaterialScene, point: Vec3) -> Result<Vec3, MaterialError>;
pub fn material_outline(scene: &MaterialScene, domain: &Domain, plane: Option<&PlaneFrame>) -> Vec<MaterialLine>;
pub fn sample_materials(
    scene: &MaterialScene,
    domain: &Domain,
    grid: UVec3,
    plane: Option<&PlaneFrame>,
) -> MaterialSampling;
```

`MaterialLine { a: [f32;3], b: [f32;3], color: [f32;4] }` is renderer-independent; Task 2 converts it to `LineInstanceGPU`. `material_outline` takes the active `PlaneFrame` when 2D is a slice; `None` means the 3D world outline. This keeps the pure outline geometry independent of renderer state while still letting 2D draw only the slice intersection.

`material_field` returns total; `material_response` subtracts applied. `material_applied` returns the uniform applied vector. `material_field` is defined for points off the interface; an exactly-on-interface point returns `MaterialError::Unsupported` with the discontinuity message. Sampling applies its domain-scaled exact-hit guard before calling the evaluator.

`sample_materials`:

- In 3D, visit `domain.grid_points(grid)` in world coordinates.
- In 2D, visit `plane.local_domain().grid_points(UVec3::new(grid.x, grid.y, 1))`, evaluate at `plane.to_world(local)`, return local positions, and project the selected family vector with `plane.project`.
- Omit nonfinite/degenerate vectors (`< 1e-6`), samples exactly on the interface, and vectors whose finite check fails.
- Set `response_empty = true` when `view == Response` and every finite sampled response is below `1e-6`.
- Use fixed family colors: Electric amber `[255,176,64]`, Magnetic cyan `[64,196,255]`; no colormap for this mode, so applied/response distinction is not encoded by color alone.
- `warning` is `Some` with the exact interface message when `omitted_interface > 0`, the zero-response message when `response_empty`, or the empty-domain/impossible-grid message when applicable. Validation failures return no arrows and a warning. Multiple warnings join with `; ` in deterministic order.

`material_outline`:

- Sphere (3D): three great circles at radius `a` in XY, XZ, and YZ, using the family color at alpha `0.35`, plus a short radial applied-direction line at the origin.
- Sphere (2D): draw only the circle where the slice cuts the sphere (`r = sqrt(a^2 - d^2)` when `|d| < a`, with `d` the signed slice offset); draw no projected great circles off the slice. If the slice misses the sphere, draw nothing.
- Boundary (3D): a square outline on the interface plane spanning the padded domain, plus a short normal arrow at the origin, alpha `0.35`.
- Boundary (2D): when the slice is not parallel to the interface, draw the intersection line; when the slice is parallel but not coincident, draw nothing; when it is coincident within the interface tolerance, draw the square outline and show `Slice lies on the interface; the field is discontinuous here.`
- All geometry is finite; clip via the existing renderer/camera, not a new clip path.

### State integration

Add `FieldMode::Materials`. `State` gains:

```rust
pub material: MaterialScene,
pub material_sampling: Option<MaterialSampling>,
pub material_error: Option<String>,
```

and setters:

```rust
pub fn set_material_scene(&mut self, scene: MaterialScene) -> Result<(), MaterialError>;
pub fn refresh_material_sampling(&mut self);
pub fn reset_material(&mut self);
```

- `active_dimension()` returns 3 for Materials; `is_plane_slice` uses the existing 3D-slice rule; `plane_frame`/`scene_domain` reuse `PlaneFrame` and the durable domain.
- `particle_target()` returns None; `has_time_dependent_consumer()` false; `set_time(Materials)` no-op; `snapshot_if_dirty` samples only Materials when active.
- `set_field_mode(Materials)` preserves formula/source/induction/ODE durable state, projection, and camera; clears transient particle/click geometry through the existing mode sync; invalidates `material_sampling`; marks dirty; does not run the ODE solver.
- `set_material_scene` validates first; on success stores the scene, clears `material_error`, clears the sampling cache, marks dirty, and increments a material epoch only when the effective scene changed. On failure, retains the prior scene/sampling and sets the error.
- No material state is shared with Sources/Induction; no source glyphs are emitted.
- `reset_material` restores defaults, clears error/sampling, marks dirty; global reset calls it and selects Formula.
- `scene_flow` is not used in Materials; the sampler is the only field producer.
- `snapshot` produces arrows only; ODE trajectory lines and plot scenes are untouched.

### App integration

- `advance_simulation` handles Materials before the general vector formula path: sync mode, refresh material sampling if dirty, build `SceneGPU` arrows and outline lines, clear particles/streams/clicks/trails/glyphs.
- `assemble_viewport_scene` passes material arrows and outline lines; no markers, no depth pass.
- Materials is vector-only; if the user is in plot view, entering Materials forces the vector viewport kind (same vector invariant as Ode) while preserving scalar drafts.
- `Fit` uses the durable domain / selected plane frame; `Reset` restores the default material scene and default camera. No new camera mode.
- The app exposes test seams `material_line_count`, `material_arrow_count`, `material_sampling_error`, and `material_scene` through the existing harness.

## UI

Add a fifth mode selector `Materials` in the formula bar (vector mode). When active, hide the formula editor and show `Materials scene summary`, the selected case, and `Open materials`. Add a dedicated rail/panel and compact sheet tab, with `DEFAULT_MATERIALS_WIDTH = 300.0`.

Panel contents:

- **Case:** Dielectric sphere / Dielectric boundary / Permeable sphere / Permeable boundary.
- **Geometry:** sphere radius, or interface axis (X/Y/Z) and offset.
- **Background:** εr and μr for this Materials case (not shared with Formula/Sources), with `ε0`/`μ0` context text. Both are shown and validated.
- **Material:** εr and μr for the inclusion. Both are shown; the active family is highlighted, and the other value is preserved but does not affect the field. Both are validated so no hidden out-of-range value can block an edit.
- **Applied field:** E in V/m or B in T, in three components, with the exact unit in the label and export. No unit conversion or dimensional engine.
- **View:** Total / Applied / Response.
- **Fit** and **Reset materials section**.
- **Advanced (closed by default):** assumptions, interface tolerance, omitted-sample status, exact warning, and "No general material solver" note.

Edits are draft-based and commit only on focus loss/Enter, drag release, selector click, or reset. Invalid values show an inline actionable error and leave the last valid scene/sampling visible. The status bar shows the material error in Materials mode.

A small viewport label `Materials - {case} - {Total|Applied|Response}` identifies the painted view; it is presentation chrome, not part of the numeric scene. The Response view shows `Response is identically zero for this view.` when `response_empty`.

Features are disabled with `Arrows only: streamlines, particles and click seeding stay off in Materials.` Coloring is hidden because family color is fixed; Domain remains shared. CAS is disabled outside Formula with the existing exact explanation. No new shortcuts.

## Persistence and Migration

`ViewRecord` gains serde-default fields:

`material_case`, `material_background_epsilon_r`, `material_background_mu_r`, `material_material_epsilon_r`, `material_material_mu_r`, `material_radius`, `material_interface_axis`, `material_interface_offset`, `material_applied` (three f64), `material_view`.

- Old records default to the dielectric sphere with `eps_bg=1`, `mu_bg=1`, `eps_in=4`, `mu_in=1`, `radius=1`, `axis=Z`, `offset=0`, `applied=[1,0,0]`, `view=Total`.
- `restore` validates and stores the scene, no sampling; malformed values fall back to the default scene and set `material_error` with a readable message. A malformed material record must not promote `FieldMode::Materials` if the stored mode is absent.
- Global reset clears scene/error/sampling and returns to Formula; Materials section reset only restores the default scene, leaving Formula/Sources/Induction/ODE and the shared clock untouched.
- No persisted camera or cache.

## Export Context

Add `material_export_provenance(mode, scene, sampling)` and `ExportProvenance.material_mode: bool`. The exact warning lines are:

```
Materials - bounded analytic cases - no general solver
Background eps_r = {eps_bg}, mu_r = {mu_bg}
Material eps_r = {eps_in}, mu_r = {mu_in}
Applied/response: {Total|Applied|Response}
Applied field: {E|B} = ({x}, {y}, {z}) {V/m|T}
```

and the applicability line from `material_assumptions(case)`.

Metadata keys:

`vfe.model=materials-bounded-analytic`, `vfe.material_case`, `vfe.material_family`, `vfe.background_eps_r`, `vfe.background_mu_r`, `vfe.material_eps_r`, `vfe.material_mu_r`, `vfe.material_view`, `vfe.material_units` (`V/m` or `T`), `vfe.material_interface_axis`, `vfe.material_interface_offset`, `vfe.material_applied`, `vfe.material_warning` only when nonempty.

PNG uses the existing baked banner + tEXt path; SVG uses visible text + metadata and keeps the existing 2D-only SVG rule. Non-Materials exports carry no material metadata. The warning is never suppressible. No nonfinite data is exported as arrows.

## Tests

### Pure model (`tests/material_test.rs`)

- `dielectric_sphere_boundary_conditions_and_far_field`: at `r=a±tol`, tangential E and normal D agree; far field approaches E0; response decays; no-contrast is applied everywhere.
- `dielectric_sphere_inside_is_uniform`.
- `dielectric_boundary_conditions`: normal D/tangential E; no contrast total=applied; response only normal; exact interface hit omitted count; near-interface samples kept.
- `permeable_sphere_boundary_conditions_and_far_field`: normal B/tangential H; no contrast; inside uniform.
- `permeable_boundary_conditions`: normal B/tangential H; no contrast; response only tangential.
- `applied_plus_response_reconstructs_total` for all four cases.
- `sphere_and_boundary_cases_are_finite_except_at_the_interface`.
- `invalid_constants_geometry_and_applied_field_are_rejected`: `eps<1`, `mu<1`, zero radius, NaN offset, zero applied; both background constants and both material constants are visible/validated.
- `material_outline_is_finite_for_sphere_and_boundary`.
- `no_contrast_response_is_exactly_zero`, and `response_empty` is set for the zero-response view.
- `interface_hit_is_omitted_but_near_interface_samples_are_kept`: a sample exactly on `r=a` increments `omitted_interface`; samples `a ± 1e-6 * extent` remain present.
- `outline_uses_slice_intersection_only`: 2D sphere misses draw no circles; 2D parallel boundary draws no line; coincident slice draws the square and warning note.

### Sampling/state/app (`tests/material_app_test.rs`)

- `materials_2d_sampling_is_a_projection_of_the_3d_case`: compare projected world samples at matching points.
- `materials_view_switches_without_mixing_families`: Total/Applied/Response, E only for dielectric, B only for permeable; no E+B sum.
- `materials_mode_is_arrows_only_and_does_not_disturb_the_clock`: no particles/streams/clicks, static transport.
- `material_edit_rebuilds_only_material_sampling`: same Arc when unchanged, new sampling when scene changes; formula/source/induction/ODE preserved.
- `invalid_material_edit_retains_last_valid_sampling_and_sets_error`.
- `materials_plane_change_reuses_the_13_frame_contract`: no stale plane samples; no competing seed logic.
- `material_outline_reaches_the_vector_scene` includes the viewport view label.
- `empty_domain_returns_a_visible_warning` rather than arrows.
- `materials_reset_and_mode_switch_preserve_other_modes`.

### UI (`tests/material_panel_test.rs`)

Real egui/accesskit at 1200/390:
- `material_panel_lists_all_cases_and_view_choices`;
- `material_geometry_and_constants_commit_on_release`;
- `invalid_material_draft_is_actionable_and_keeps_last_valid`;
- `materials_view_selection_and_reset_use_state`;
- `materials_compact_and_wide_panels_reflow`;
- `materials_features_and_cas_are_disabled`;
- `material_help_rows_state_the_approximations`.

### Persistence/export (`tests/material_persistence_test.rs`, `tests/material_export_test.rs`)

- Full field round-trip, malformed fallback, legacy default, reset.
- Decoded PNG banner/tEXt and SVG visible text/metadata; non-Materials no material context; warning never suppressed; no nonfinite arrows.

### Evidence/benchmark

- `tests/material_bench_test.rs` and `scripts/bench-materials.sh`: densest supported case at 40³ and 200² (or the current mode caps), record host, case, sample count, time, omitted interface count, and no frame-time/fps claim; sampling time only.
- If the densest case exceeds 2 s, reduce the material densest preset/caps only with design/plan update and independent review.

## Delivery Shape (five tasks)

1. Pure analytic model, formulas, outline, fixtures/tests, cap evidence.
2. `FieldMode::Materials`, State sampling, app branch, plane/2D, arrows-only, harness.
3. Responsive Materials panel, mode selector, help, real-widget tests.
4. Persistence/migration/reset and PNG/SVG provenance/metadata.
5. Docs, integrated benchmark, full gates, final review.

## Risks

- **Formula sign/orientation errors:** neutralized by interface-condition tests and no-contrast limits.
- **Mixing applied and total:** neutralized by a single `view` selector and reconstruction tests.
- **Implying a general solver:** exact catalog, explicit assumptions, and no arbitrary superposition.
- **Discontinuity artifacts:** exact-hit omission, visible warning where triggered, no fake interpolation; near-interface samples are not silently removed.
- **2D/3D drift:** one evaluator with `PlaneFrame` projection and equivalence tests.
- **Performance:** analytic per-sample cost; caps and benchmark required before UI completion.
- **Scope creep:** no material glyph beyond simple outlines, no extra cases without owner approval.

## Review Focus

1. Interface conditions and no-contrast limits for all four cases.
2. Applied/response reconstruction and no E+B mixing.
3. Interface-tolerance omission is visible and never averaged across a discontinuity.
4. Plane sampling is the same 3D case, not a separate 2D solver.
5. Persistence fallback never promotes Materials without a valid stored record or silently drops an invalid edit.
6. Export warning/metadata survives decoded PNG/SVG and cannot be suppressed.
7. Performance claims state that the benchmark is analytic sampling, not full-frame fps or browser evidence.
