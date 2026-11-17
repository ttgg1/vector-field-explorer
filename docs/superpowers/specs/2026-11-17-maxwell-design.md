# Maxwell Teaching Cases Design

**Status:** PROPOSED
**Date:** 2026-11-17
**Issue:** [#30](https://github.com/ttgg1/vector-field-explorer/issues/30)
**Dependencies:** #13 (plane slices), #24 (source evaluator conventions), #29 (bounded analytic case pattern), #18 (performance evidence)

## Intent

Add a dedicated Maxwell teaching mode for ideal electrostatic/magnetostatic cases and a deliberately limited free-conductor boundary solver. The mode is a catalog of named, bounded analytic or explicitly approximated cases, not a new source kind and not a general field solver.

The app must make the idealization visible: infinite plates, infinite cylinder/wire/solenoid and image boundaries are teaching models. A field that cannot be justified by the selected case is not drawn as if it were general.

## Goals

- Canonical ideal cases: parallel plates, coaxial cylinder (electric and magnetic), hollow charged sphere, infinite straight wire, ideal solenoid.
- Image-compatible 3D conductor cases: conducting sphere in uniform E and grounded plane with a point charge.
- One bounded 2D boundary-element case for a free conductor cross-section, explicitly labeled **infinite-cylinder approximation**.
- E and B remain separate outputs; no like/unlike summation.
- 2D is a slice of the same 3D case when the case is 3D. The 2D BEM conductor case is 2D-only and says so.
- Explicit applicability, convergence, accuracy and resource diagnostics; finite-inaccurate display remains default-off and visibly labelled.
- Native + wasm, pure CPU model, existing `SceneGPU` paint boundary, no new dependencies, no CLI expansion, no private clock.

## Non-goals

- No 3D BEM/FEM, arbitrary 3D CSG, arbitrary conductor outlines, automatic material/matter coupling, full-wave/FDTD, displacement current, radiation, retardation, nonlinear media, or iterative nonlinear solvers.
- No arbitrary-source/material superposition; ideal infinite cases do not replace finite sources (#24).
- No claim that a 2D infinite-cylinder result is a 3D finite object.
- No new source kind, no ninth family, no CLI flags.
- No browser/manual visual claim; wasm compile and pure tests only.

## Chosen approach

A dedicated `FieldMode::Maxwell` with a pure `src/maxwell.rs` catalog. Analytic cases are closed-form; the free-conductor case is a bounded 2D collocation BEM. The app samples either the 3D world grid or one `PlaneFrame` slice, evaluates the pure case, and renders arrows plus case outlines through the existing line/dot path.

Rejected alternatives:

- Adding these as source kinds: the source evaluator is a finite-source evaluator; ideal infinite cases and boundary approximations have different validity and diagnostics.
- General BEM/FEM: out of scope and not evidence-backed.
- Reusing Materials: Maxwell cases are not material regions and must not imply arbitrary material/field coupling.

## Catalog

```rust
pub enum MaxwellCase {
    ParallelPlates,
    CoaxialElectric,
    CoaxialMagnetic,
    HollowSphere,
    InfiniteWire,
    Solenoid,
    ImageSphere,
    ImagePlane,
    Conductor2D,
}
```

| Case | Family | Geometry | Analytic model | Validity |
|---|---|---|---|---|
| ParallelPlates | E | separation `d`, plate field `E0` | uniform `E0 ẑ` between `-d/2..d/2`, zero outside | infinite plates; no fringing |
| CoaxialElectric | E | inner radius `a`, outer radius `b` | `E = E0 (a/r) rhat` for `a<r<b`, zero outside | infinite coaxial cylinders |
| CoaxialMagnetic | B | same | `B = B0 (a/r) phi` in the gap, zero outside | infinite cylinder, steady current |
| HollowSphere | E | radius `R`, field `E0` at `R` | `E = E0 (R/r)² rhat` outside, zero inside | uniformly charged thin shell |
| InfiniteWire | B | radius `a`, field `B0` at `a` | `B = B0 (a/r) phi` outside, zero inside | infinite straight **surface-current shell** |
| Solenoid | B | radius `R`, field `B0` | `B = B0 ẑ` inside, zero outside | ideal infinite solenoid |
| ImageSphere | E | radius `R`, applied `E0 ẑ` | `φ=-E0(r-R³/r²)cosθ`; inside zero | isolated conducting sphere in uniform E |
| ImagePlane | E | charge `q` at height `h` above grounded plane `z=0` | charge + image `-q` at `-h`; region `z≥0` only | infinite grounded plane; point charge |
| Conductor2D | E | circle or square cross-section, applied `E0` in the plane | 2D BEM, grounded conductor, infinite-cylinder approximation | 2D only; bounded panels; no 3D claim |

All fields are normalized teaching fields: `E0`/`B0`/`q` are **display amplitudes**, not SI values or a unit/constant engine. The UI labels `E0 (display)`, `B0 (display)`, `q (display)` and states “idealized normalized teaching fields; no SI constant engine”. Export metadata says the same.

### Parameter defaults

```rust
pub struct MaxwellScene {
    pub case: MaxwellCase,
    pub e0: f64,          // parallel plates / coaxial / sphere / image
    pub b0: f64,          // wire / solenoid / coaxial magnetic
    pub separation: f64,  // plates / coaxial gap
    pub radius: f64,      // sphere/wire/solenoid/cylinder
    pub outer_radius: f64,
    pub charge: f64,      // image plane
    pub charge_height: f64,
    pub outline: ConductorOutline,
    pub applied: [f64; 2], // 2D BEM applied E in local x/y
    pub elements: u32,    // fine BEM panels, even; coarse = elements / 2
    pub allow_inaccurate: bool,
    pub view: MaxwellView,
}
```

Defaults: `ParallelPlates`, `e0=1`, `b0=1`, `separation=1`, `radius=1`, `outer_radius=2`, `charge=1`, `charge_height=1`, `outline=Circle{radius:1}`, `applied=[1,0]`, `elements=128` (fine; coarse 64), `allow_inaccurate=false`, `view=Field`.

`MaxwellView::{Field, Potential}`? Potential is useful for conductors but makes the catalog more complex. **Decision:** first delivery has `MaxwellView::Field` only. Potential is a future view. This keeps sampling/renderer semantics uniform.

## Pure model API

```rust
pub enum MaxwellFamily { Electric, Magnetic }
pub enum MaxwellView { Field }
pub enum ConductorOutline { Circle { radius: f64 }, Square { half_extent: f64 } }
pub enum MaxwellStop { TargetMet, AccuracyNotMet, Unavailable }

pub struct MaxwellSampling {
    pub arrows: Vec<ArrowData>,
    pub family: MaxwellFamily,
    pub case: MaxwellCase,
    pub e0: f64,
    pub b0: f64,
    pub assumptions: &'static str,
    pub warning: Option<String>,
    pub omitted: usize,
    pub omitted_large: usize,
    pub out_of_plane_suppressed: bool,
    pub accuracy: MaxwellStop,
    pub convergence: Option<BemConvergence>,
}
pub struct BemConvergence { pub coarse_elements: u32, pub fine_elements: u32, pub relative_difference: f64, pub target: f64 }

pub struct MaxwellLine { pub a: [f32;3], pub b: [f32;3], pub color: [f32;4] }

pub fn maxwell_family(case: MaxwellCase) -> MaxwellFamily;
pub fn maxwell_assumptions(scene: &MaxwellScene) -> &'static str;
pub fn maxwell_warning(scene: &MaxwellScene) -> Option<String>;
pub fn validate_maxwell(scene: &MaxwellScene) -> Result<(), MaxwellError>;
pub fn maxwell_field(scene: &MaxwellScene, point: Vec3) -> Result<Vec3, MaxwellError>;
pub fn maxwell_outline(scene: &MaxwellScene, domain: &Domain, plane: Option<&PlaneFrame>) -> Vec<MaxwellLine>;
pub fn sample_maxwell(scene: &MaxwellScene, domain: &Domain, grid: UVec3, plane: Option<&PlaneFrame>) -> MaxwellSampling;
```

Constants: `MAXWELL_MAX_DENSITY_3D=40`, `MAXWELL_MAX_DENSITY_2D=200`, `MAXWELL_MAX_BEM_ELEMENTS=256`, `MAXWELL_BEM_MIN_ELEMENTS=32`, `MAXWELL_BEM_TARGET=1e-3`, `MAXWELL_INTERFACE_TOLERANCE_FACTOR=1e-9`, `MAXWELL_MAX_RELATIVE=1e3`.

Validation: finite, strictly positive geometry; `outer_radius > radius` for coaxial; `separation > 0`; `charge_height > 0`; `elements` even, `32..=256`; applied finite/nonzero for BEM; `e0/b0` finite and within `±1e3`; no NaN/Inf.

### Field rules

- Parallel plates: return `E0 ẑ` only for `|z| < d/2`, zero outside; points exactly on plates are omitted by the interface guard.
- Coaxial electric: `a<r<b`; `E0*(a/r) rhat`; outside 0. Magnetic: `B0*(a/r) phi`; outside 0.
- Hollow sphere: `r>R`; `E0*(R/r)^2 rhat`; `r<R` zero; `r≈R` exact interface omitted.
- Infinite wire: **ideal thin surface-current shell**. `r>a`: `B0*(a/r) phi`; `r<a`: zero (a uniform volume current would instead give `B ∝ r/a`, which this case deliberately does not model). Axis `r=0` is zero by symmetry; the azimuthal basis is undefined there.
- Surface-current-shell assumption is stated in the UI/help/export; do not relabel it as a general current-carrying wire.
- Solenoid: `r<R`, `B0 ẑ`; outside zero; exact `r=R` omitted.
- Image sphere: outside `φ=-E0(r-R³/r²)cosθ`; E via analytic gradient; inside zero. Use f64 formulas.
- Image plane: region `z>0` only; field from `q` at `(0,0,h)` and `-q` at `(0,0,-h)`; `z<=0` zero. `length_scale = max(domain extent, |h|, 1)`. Omit real-charge samples within `1e-9*length_scale` and never evaluate a point at or below `z=0`; the image charge is outside the supported half-space. Boundary tests use a point such as `z = 1e-3*h`, not `z=0`.
- Conductor2D: 2D only. Reject `plane.is_none()`? The mode can be 3D but the case requires 2D. Design decision: **selecting Conductor2D forces an applicability message in 3D and draws nothing**; the panel offers `Switch to 2D`. The sampler called with `plane=None` returns `Unavailable` with `Conductor2D is a 2D infinite-cylinder cross-section; switch to 2D.` This avoids pretending a 3D finite cylinder.

### 2D sampling and outlines

- 3D: `domain.grid_points(grid)` world positions; exact-interface/near-singularity omissions; selected family vector.
- 2D: `plane.local_domain().grid_points((n,n,1))`; evaluate world; project selected vector to plane; local positions.
- 2D projection truthfulness: a near-zero projected vector is omitted; if a finite sample has `projected.length() < 0.1 * full.length()` (and full is above the degeneracy threshold), set `out_of_plane_suppressed` and add the warning `2D slice shows only the in-plane component; the out-of-plane component is not visible.` This is expected for azimuthal fields in a plane containing the axis; do not claim full-vector equality.
- Fixed family colors E amber `[255,176,64]`, B cyan `[64,196,255]`; no colormap.
- Arrow magnitude guard: omit a finite arrow when `|F| > MAXWELL_MAX_RELATIVE * max(|E0|, |B0|, 1e-9)`, count it in `omitted_large`, and add `Extreme near-interface magnitudes are omitted.` Never clamp a finite value into a plausible-looking arrow.
- Outlines: plates two lines at `±d/2`; coaxial two circles; hollow sphere 3 great circles (2D cut circle); wire line along z plus radial ticks; solenoid helix/circle; image sphere great circles + applied arrow; image plane square on z=0 + charge marker; BEM conductor circle/square cross-section.
- 2D outlines draw slice intersections only. A slice coincident with a material/ground interface within `1e-6*max(domain extent, case extent, 1)` warns and draws no field. For `ImagePlane`, the canonical `z=0` slice is therefore empty: show the exact `Ground-plane slice is empty; move the plane off z=0.` message and do not auto-move the plane.

## 2D BEM conductor case

Scope: **grounded, isolated-neutral** infinite circular or square cylinder in a uniform applied E in the cross-section plane. This is a teaching approximation, not a 3D finite conductor. The conductor boundary potential is `-E0·x`; grounding fixes the additive constant and excludes the constant-charge/net-charge mode. After solving, compute `Q = Σ σ_j l_j`; require `|Q| <= 1e-6 * max(|E0|*extent,1)`; otherwise return `Unavailable(NetChargeMode)` rather than a confident field.

Formulation:
- `elements` is the **fine** panel count (even); the coarse run is `elements/2`. Boundary panels constant charge density per panel; collocation at panel midpoints.
- 2D Laplace Green's function `G = -1/(2π) ln r`; matrix `A_ij = G(x_i, y_j) * length_j`; boundary potential `-E0·x`; solve `A σ = -φ_applied`.
- Field `E = E0 + Σ σ_j ∫ ∇G` evaluated at query points.
- Gaussian elimination with partial pivoting; pivot threshold `1e-12 * max_row`; non-finite or singular → `Unavailable(IllConditioned)`.
- Work cap `N³ ≤ 256³`; coarse/fine runs are `N/2` and `N`. The solve and convergence run are cached in `State` keyed by the BEM scene/domain; sampling refresh, plane changes and camera changes must not solve again. `set_maxwell_scene` invalidates the cache.
- Convergence: solve at `N/2` (coarse) and `N` (fine), never identical. Compare field at a fixed 8-point probe ring at `R_probe = 4 * max(outline_extent, 1e-6)` from the conductor center; relative difference is `max_probe|E_fine-E_coarse| / max_probe(max(|E_fine|, |E0|, 1e-9))`, a single global normalization. If `N == coarse` (unreachable with `elements >= 32`) report `AccuracyNotMet` with no false TargetMet. If > `MAXWELL_BEM_TARGET`, `AccuracyNotMet`; draw only if `allow_inaccurate`, with exact `accuracy not met` label. If no finite result, `Unavailable`.
- Diagnostics include fine N, coarse N/2, relative difference, target, work, net charge and probe-ring radius. Square corners use constant-density collocation and are expected to converge slowly: accuracy-not-met is a legitimate outcome; refinement must not be claimed monotone per point. Normalize error globally over the probe set. Tests require monotone decrease in the global error and accept `AccuracyNotMet` at the target.
- Pathological: collinear/degenerate panels, self-intersection, zero length, non-finite outline, singular matrix → finite error/unavailable, no NaN.

**Why BEM is bounded:** only two outline families, max 256 panels, one fixed applied direction, one solve per edit, no arbitrary 3D, no iterative solver, no source superposition.

## State and app integration

- `FieldMode::Maxwell`; `State::maxwell: MaxwellScene`, `maxwell_sampling: Option<MaxwellSampling>`, `maxwell_error: Option<String>`, and a private cached BEM solve/convergence keyed by the BEM scene/domain. `refresh_maxwell_sampling` only resamples; `set_maxwell_scene` clears the cache; plane/camera changes do not solve.
- `active_dimension`: 3 for Maxwell; `is_plane_slice`/`plane_frame` reuse existing plane rules; `scene_domain` = durable domain; no field flow.
- `set_maxwell_scene` validates, clears cache, marks dirty; `refresh_maxwell_sampling` samples current grid/plane; `reset_maxwell` default.
- Mode entry preserves camera/projection and durable scenes, clears transient particles/clicks/streams, no clock consumer. Maxwell is static.
- App branch before formula: arrows + outline lines only, no particles/streamlines/clicks/source glyphs; harness `maxwell_arrow_count`, `maxwell_line_count`, `maxwell_sampling`, `maxwell_error`.
- 2D/3D toggle does not force projection; Conductor2D in 3D shows the applicability message and no arrows.
- Fit/Reset use existing vector camera.
- Export provenance: `Maxwell - ideal canonical teaching cases - no general solver`, family/case/assumptions, BEM convergence/accuracy; PNG/SVG request-time, no sampling at export.

## UI

- Fifth selector? Existing modes are Formula/Sources/Induction/ODE/Materials; Maxwell is the sixth. Add `Maxwell` selector and `DEFAULT_MAXWELL_WIDTH=300`.
- `src/ui/maxwell_panel.rs`, `MaxwellPanelState { draft,error,advanced }`, `commit`, `sync_from`.
- Panel controls:
  - Case combo; family label (read-only).
  - Geometry fields relevant to case; `E0`, `B0`, `q`; units.
  - Applied direction for BEM (x/y).
  - Outline (Circle/Square) and elements slider for BEM.
  - View `Field` (only).
  - `Fit`, `Reset Maxwell section`.
  - Advanced: assumptions, accuracy/convergence/status, `allow_inaccurate` default-off, element count/work.
  - Exact note: `Arrows only: streamlines, particles and click seeding stay off in Maxwell.`
  - BEM note: `Conductor2D is an infinite-cylinder cross-section, not a 3D finite object.`
- Features disabled; Coloring hidden; CAS disabled outside Formula; 3D Conductor2D shows `Switch to 2D`.
- Real-widget tests at 1200/390; accessibility labels; help rows covering idealizations, BEM approximation, accuracy opt-in.

## Persistence

`ViewRecord` fields: `maxwell_case`, `maxwell_e0`, `maxwell_b0`, `maxwell_separation`, `maxwell_radius`, `maxwell_outer_radius`, `maxwell_charge`, `maxwell_charge_height`, `maxwell_outline`, `maxwell_applied`, `maxwell_elements`, `maxwell_allow_inaccurate`. No sampling stored.
- Serde defaults, legacy default scene, malformed fallback with visible error, no promotion to Maxwell without stored mode.
- Global reset clears Maxwell scene/sampling/error and returns to Formula; Maxwell section reset preserves other modes.

## Export

`maxwell_export_provenance(mode, scene, sampling)`, `ExportProvenance.maxwell_mode`.
- Warning lines: `Maxwell - ideal canonical teaching cases - no general solver`; `Conductor2D - infinite-cylinder BEM approximation`; `accuracy not met` when applicable; unavailable/validation lines.
- Metadata: `vfe.model=maxwell-bounded-teaching`, `vfe.maxwell_case`, `vfe.maxwell_family`, `vfe.maxwell_assumptions`, `vfe.maxwell_elements`, `vfe.maxwell_convergence` when present.
- Every frame/frame image includes the warning; SVG visible text + metadata, 2D-only rule; no sampling at export.
- Non-Maxwell exports carry no Maxwell metadata.

## Tests

### Analytic
- `parallel_plates_field_is_uniform_inside_and_zero_outside`
- `coaxial_electric_and_magnetic_match_radial_and_azimuthal_laws`
- `hollow_sphere_is_inverse_square_outside_and_zero_inside`
- `infinite_wire_is_azimuthal_and_zero_on_the_axis`
- `solenoid_is_axial_inside_and_zero_outside`
- `image_sphere_matches_the_conducting_sphere_boundary_conditions`
- `image_plane_zero_tangential_field_off_the_interface` (boundary at `z=1e-3*h`)
- `image_plane_omits_real_and_image_charge_singularities`
- `exact_interface_points_are_omitted_but_neighbours_survive`
- `extreme_near_interface_arrows_are_omitted_not_clamped`
- `no_contrast_or_zero_amplitude_cases_do_not_produce_nan`

### BEM
- `bem_circle_matches_the_analytic_conducting_cylinder`
- `bem_square_reports_accuracy_not_met_with_global_refinement`
- `bem_never_compares_identical_coarse_and_fine`
- `bem_net_charge_residual_is_explicit`
- `bem_bad_outline_and_singular_system_are_explicitly_unavailable`
- `bem_conductor_is_2d_only_and_says_so_in_3d`
- `bem_work_and_elements_are_bounded`
- `bem_solve_is_cached_across_sampling_and_plane_changes`
- `bem_accuracy_not_met_is_default_off_and_labeled`

### App/UI/persistence/export
- `maxwell_mode_isolated_and_static`
- `maxwell_2d_projection_matches_3d_where_visible`
- `azimuthal_2d_slice_warns_about_suppressed_component`
- `coincident_ground_slice_warns_and_shows_no_field`
- `maxwell_case_switch_refreshes_only_material`
- `maxwell_bem_solve_is_not_repeated_by_sampling`
- `maxwell_panel_wide_and_compact_reflow` (real widgets)
- `maxwell_bem_controls_and_switch_to_2d_are_accessible`
- `maxwell_persistence_round_trip_and_malformed_fallback`
- `maxwell_export_png_banner_and_svg_metadata_decoded`
- `non_maxwell_export_has_no_maxwell_metadata`

### Evidence
- Native full suite/build/clippy/rustfmt/docs/diff; smoke 15s; wasm check.
- Benchmark densest BEM (fine 256, coarse 128) and 40³/200² sampling; host recorded; no fps claim.
- Decoded export metadata asserts display-only units and no SI constant claim.
- Browser/manual waived, not passed.

## Acceptance mapping

- Canonical analytic cases with boundary conditions/limits: analytic tests.
- Independent BEM convergence/reference: circle analytic + refinement; square convergence.
- Conditioning/pathological/bounded: BEM tests and caps.
- 3D image-compatible sphere/plane: analytic tests; unsupported arbitrary geometry rejected.
- UI/persistence/export assumptions/accuracy: real-widget and decoded-output tests.
- Default-off inaccurate display cannot bypass caps/NaN/warnings: explicit tests.
- #18 reconciled: benchmark numbers recorded; no hide of unmet floors.

## Risks

- **BEM scope creep:** only circle/square and 2D; any other outline is visibly unsupported.
- **f32 sampling of singular fields:** exact-hit guard, finite-only, no interpolation; near-interface values may be large.
- **Idealization misleading:** assumptions/warnings always visible; no “general Maxwell” claim.
- **BEM conditioning:** pivot check, explicit Unavailable, no false TargetMet.
- **Performance:** 256³ matrix solve once per edit, not per frame; benchmark before freezing caps.
- **Plane semantics:** Conductor2D is 2D-only; 3D shows an explanation, not a fabricated finite cylinder.
