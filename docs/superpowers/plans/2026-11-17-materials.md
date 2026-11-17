# Fields in Matter Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a dedicated Materials mode with four bounded analytic material cases, explicit total/applied/response views, exact applicability warnings, and persistence/export context without implying a general material solver.

**Architecture:** A pure `src/material.rs` owns case validation, exact fields, application/response decomposition, outlines, and sampling. `FieldMode::Materials` reuses the existing `PlaneFrame`, vector camera, `SceneGPU` arrow/line passes, persistence, and request-time export provenance. The panel edits drafts only; valid commits rebuild material sampling without touching Formula/Sources/Induction/ODE.

**Tech Stack:** Rust 2024, eframe/egui, glam, existing `PlaneFrame`, `SceneGPU`, serde persistence, PNG/SVG exporters; no new dependencies.

**Spec:** `docs/superpowers/specs/2026-11-17-materials-design.md`

## Global Constraints

- Four cases only: dielectric sphere, dielectric plane boundary, permeable sphere, permeable plane boundary. No FEM/BEM/general solver/arbitrary overlapping regions, anisotropic/nonlinear/frequency-dependent media, added geometry, or source-stack dependency.
- Views are `Total`, `Applied`, `Response`; response is exactly `total - applied`. E and B are never mixed or summed.
- Homogeneous, linear, isotropic, nondispersive, nonconducting, quasistatic model; one inclusion in an infinite background; visualization domain clips only.
- Relative constants finite in `1.0..=100.0`; sphere radius finite positive; applied field finite and nonzero; interface offset finite for boundary cases.
- Exact interface hits are omitted with `1e-9 * max(domain extent, 1.0)` guard; no interpolation/averaging. No Plummer softening.
- 2D is a slice of the same 3D case; reuse #13 `PlaneFrame`; no separate solver or competing plane logic.
- Materials is vector-only: arrows + outline; no particles, streamlines, clicks, field glyphs, scalar plot, or clock consumer.
- No new dependencies/CLI/private timer. CAS stays Formula-only. wasm preserved.
- Invalid data never draws NaN/Inf; warnings and applicability context cannot be suppressed.
- Tests belong in `tests/` and register in `tests/suite.rs`; no inline `#[cfg(test)]` under `src/`.
- Browser/manual visual gates are owner-waived, not passed. Benchmark is sampling evidence, not fps.

## Review Focus

1. **Boundary/interface signs and conditions:** both sphere and plane formulas must satisfy the stated E/D and B/H continuity conditions and the no-contrast limit.
2. **Exact-on-interface behavior:** exactly-on-interface samples are omitted; near-interface samples on both sides remain.
3. **Applied/response reconstruction:** every case must satisfy `total = applied + response` without relabelling or mixing families.
4. **2D slice fidelity:** sampled values and slice outlines match the same 3D case; no projected great circles off the slice and no separate 2D physics.
5. **Mode isolation:** Materials edits, reset, plot-kind selection, and persistence must not disturb Formula/Sources/Induction/ODE or the shared clock.
6. **Export truthfulness:** decoded PNG and SVG retain warnings, units, view, and applicability; non-Materials output carries no material context.

---

### Task 1: Pure analytic material model, outline and evidence

**Files:**
- Create: `src/material.rs`
- Modify: `src/lib.rs`
- Create: `tests/material_test.rs`, `tests/material_bench_test.rs`
- Modify: `tests/suite.rs`, `docs/CODEBASE.md`
- Create: `scripts/bench-materials.sh`, `docs/perf/materials-performance.md` (Task 5 finalizes)
- Report/evidence: `.superpowers/sdd/2026-11-17-materials/task-1-report.md`, `evidence/task-1/`

**Interfaces:**
- Consumes: `PlaneFrame`, `Domain`, `PlaneAxis`, `ArrowData`, `AXIS_*_COLOR`.
- Produces: exact design constants/types/functions from `src/material.rs`, including `MaterialScene::default`, `MaterialCase`, `MaterialView`, `MaterialFamily`, `MaterialError`, `MaterialSampling`, `MaterialLine`, `material_family`, `material_assumptions`, `material_warning`, `validate_material`, `material_field`, `material_applied`, `material_response`, `material_outline`, `sample_materials`.

- [ ] **Step 1: Write failing model tests**

Create `tests/material_test.rs` with imports:

```rust
use glam::{UVec3, Vec3};
use vector_field_explorer::domain::Domain;
use vector_field_explorer::material::{
    material_applied, material_family, material_field, material_outline, material_response,
    sample_materials, validate_material, MaterialCase, MaterialError, MaterialFamily,
    MaterialScene, MaterialView,
};
use vector_field_explorer::plane::{PlaneAxis, PlaneFrame, PlaneView};
```

Helpers:

```rust
fn sphere(case: MaterialCase, eps: f64, mu: f64, applied: [f64; 3]) -> MaterialScene {
    MaterialScene {
        case,
        background_epsilon_r: 1.0,
        background_mu_r: 1.0,
        material_epsilon_r: eps,
        material_mu_r: mu,
        radius: 1.0,
        interface_axis: PlaneAxis::X,
        interface_offset: 0.0,
        applied,
        view: MaterialView::Total,
    }
}

fn boundary(case: MaterialCase, eps: f64, mu: f64, applied: [f64; 3]) -> MaterialScene {
    MaterialScene {
        case,
        background_epsilon_r: 1.0,
        background_mu_r: 1.0,
        material_epsilon_r: eps,
        material_mu_r: mu,
        radius: 1.0,
        interface_axis: PlaneAxis::X,
        interface_offset: 0.0,
        applied,
        view: MaterialView::Total,
    }
}

fn domain() -> Domain {
    Domain::new(Vec3::splat(-2.0), Vec3::splat(2.0))
}
```

Tests:

```rust
#[test]
fn dielectric_sphere_matches_inside_far_field_and_no_contrast() {
    let scene = sphere(MaterialCase::DielectricSphere, 4.0, 1.0, [1.0, 0.0, 0.0]);
    let inside = material_field(&scene, Vec3::new(0.5, 0.0, 0.0)).unwrap();
    assert!((inside.x - 0.5).abs() <= 1e-12);
    assert!(inside.y.abs() <= 1e-12 && inside.z.abs() <= 1e-12);
    let outside = material_field(&scene, Vec3::new(2.0, 0.0, 0.0)).unwrap();
    assert!((outside.x - 1.125).abs() <= 1e-12);
    let no = sphere(MaterialCase::DielectricSphere, 4.0, 1.0, [1.0, 0.0, 0.0]);
    let mut no = no;
    no.material_epsilon_r = 1.0;
    for point in [Vec3::new(0.5, 0.0, 0.0), Vec3::new(2.0, 0.0, 0.0)] {
        assert!((material_field(&no, point).unwrap() - Vec3::X).length() <= 1e-12);
    }
    assert!((material_field(&no, Vec3::new(2.0, 0.0, 0.0)).unwrap() - Vec3::X).length() <= 1e-12);
}

#[test]
fn dielectric_boundary_conditions_and_response_are_piecewise() {
    let scene = boundary(MaterialCase::DielectricBoundary, 4.0, 1.0, [1.0, 2.0, 3.0]);
    let background = material_field(&scene, Vec3::new(-0.5, 0.0, 0.0)).unwrap();
    let material = material_field(&scene, Vec3::new(0.5, 0.0, 0.0)).unwrap();
    assert!((background.x - 1.0).abs() <= 1e-12);
    assert!((material.x - 0.25).abs() <= 1e-12);
    assert!((material.y - 2.0).abs() <= 1e-12 && (material.z - 3.0).abs() <= 1e-12);
    assert!((material_response(&scene, Vec3::new(0.5, 0.0, 0.0)).unwrap() - Vec3::new(-0.75, 0.0, 0.0)).length() <= 1e-12);
    assert!(material_response(&scene, Vec3::new(-0.5, 0.0, 0.0)).unwrap().length() <= 1e-12);
}

#[test]
fn permeable_sphere_and_boundary_match_their_interface_conditions() {
    let sphere_scene = sphere(MaterialCase::PermeableSphere, 1.0, 4.0, [0.0, 0.0, 1.0]);
    let inside = material_field(&sphere_scene, Vec3::new(0.5, 0.0, 0.0)).unwrap();
    assert!((inside.z - 2.0).abs() <= 1e-12);
    let outside = material_field(&sphere_scene, Vec3::new(0.0, 0.0, 2.0)).unwrap();
    assert!((outside.z - 1.125).abs() <= 1e-12);

    let plane = boundary(MaterialCase::PermeableBoundary, 1.0, 4.0, [1.0, 2.0, 3.0]);
    let material = material_field(&plane, Vec3::new(0.5, 0.0, 0.0)).unwrap();
    assert!((material.x - 1.0).abs() <= 1e-12, "normal B is continuous");
    assert!((material.y - 8.0).abs() <= 1e-12, "in-plane tangential B scales by mu_in/mu_bg");
    assert!((material.z - 12.0).abs() <= 1e-12);
}

#[test]
fn applied_plus_response_reconstructs_total_for_all_cases() {
    let cases = [
        (MaterialCase::DielectricSphere, 4.0, 1.0),
        (MaterialCase::DielectricBoundary, 4.0, 1.0),
        (MaterialCase::PermeableSphere, 1.0, 4.0),
        (MaterialCase::PermeableBoundary, 1.0, 4.0),
    ];
    for (case, eps, mu) in cases {
        let scene = sphere(case, eps, mu, [0.3, -0.5, 0.8]);
        for point in [Vec3::new(0.5, 0.2, 0.1), Vec3::new(1.5, -0.4, 0.2)] {
            let total = material_field(&scene, point).unwrap();
            let response = material_response(&scene, point).unwrap();
            let applied = material_applied(&scene);
            assert!((total - (applied + response)).length() <= 1e-12);
        }
    }
}
```

Add executable tests:
- `exact_interface_hits_are_unsupported`: sphere at `(1,0,0)`, boundary at `(0,0,0)` return `MaterialError::Unsupported`; near-interface points at `1 ± 1e-5` succeed.
- `validation_rejects_bad_constants_geometry_and_applied`: eps/mu below 1 or above 100, zero/NaN radius, NaN offset, zero/NaN applied.
- `sampling_omits_exact_interface_but_keeps_near_interface`: domain `[-2,2]`, grid `41³`, sphere; assert `omitted_interface > 0` and warning contains exact interface sentence; assert finite arrows on both sides.
- `no_contrast_response_is_empty_with_exact_note`: `MaterialView::Response` with no contrast; `arrows` empty, `response_empty`, warning contains exact zero-response sentence.
- `empty_domain_returns_warning_not_arrows`: impossible grid/zero extent returns no arrows and a nonempty warning.
- `outline_uses_slice_intersection_only`: 3D sphere outline nonempty; 2D Z slice at offset 2 with radius 1 produces no lines; 2D Z slice at offset 0 produces a cut circle; boundary 2D parallel noncoincident produces no lines; coincident produces square lines and sampling warning contains `Slice lies on the interface; the field is discontinuous here.`
- `every_design_case_has_a_family_and_assumptions`: E cases electric amber, B cases magnetic cyan; assumptions nonempty.

- [ ] **Step 2: Run and confirm RED**

Run: `cargo test --test suite material_test::`
Expected: compile failure because `src/material.rs` is absent.

- [ ] **Step 3: Implement the pure model**

Implement exactly the design formulas and APIs. Deterministic rules beyond the formulas:

- `material_field` dispatches by case; sphere `r=0` uses inside. Boundary background is `n·p < offset`, material `n·p > offset`.
- Exact-interface detection in `material_field` uses `1e-9 * max(radius,1.0)` for spheres and `1e-9 * max(abs(offset),1.0)` for boundaries; return `Unsupported` with the exact interface sentence.
- `sample_materials` applies the domain-scaled exact-hit guard before evaluation, omits nonfinite/degenerate (`<1e-6`), and appends deterministic warnings in this order: interface omission, response-empty, empty/impossible domain, coincident-slice.
- `response_empty` is true only when `view == Response` and every finite response is `< 1e-6`; it is a warning, not an error.
- Sampling uses fixed family colors and no colormap. 2D returns local positions and `plane.project` of the active family vector.
- `material_outline`: 3D sphere three great circles alpha .35 plus a radial line of length `0.5*radius`; 2D sphere only the cut circle when `|d| < radius`; boundary 3D square spanning the padded domain plus a normal line of `0.25*extent`; boundary 2D line clipped to local bounds, nothing when parallel noncoincident, square when coincident. All finite.
- `material_assumptions` is case-specific static text including homogeneous/linear/isotropic/nondispersive/quasistatic/infinite background and domain-as-window; `material_warning` returns `Bounded analytic case; no general material solver.`

- [ ] **Step 4: Add benchmark evidence**

Create `tests/material_bench_test.rs` (only runs with `VFE_MATERIAL_BENCH`) sampling the densest case at 40³ and 200²; print host, case, view, samples, ms, omitted count. `scripts/bench-materials.sh` sets the env var and runs exact. If either exceeds 2 s, stop and report; do not silently lower caps.

- [ ] **Step 5: Run tests, record evidence, commit**

```bash
cargo test --test suite material_test::
VFE_MATERIAL_BENCH=1 cargo test --test suite material_bench_test:: -- --nocapture
git add src/material.rs src/lib.rs tests/material_test.rs tests/material_bench_test.rs tests/suite.rs docs/CODEBASE.md scripts/bench-materials.sh docs/perf/materials-performance.md
git commit -m "feat: add bounded analytic material cases" -m "Generated-by: OpenCode"
```

---

### Task 2: Materials mode, state, sampling and app

**Files:**
- Modify: `src/source_scene.rs`, `src/viewmodel.rs`, `src/viewmodel/plot.rs`
- Modify: `src/app/mod.rs`, `src/app/simulation.rs`, `src/app/input.rs`, `src/app/plot.rs`, `src/app/panels.rs`, `src/app/viewport.rs`
- Create: `tests/material_app_test.rs`
- Modify: `tests/suite.rs`, `docs/CODEBASE.md`

**Interfaces:**
- Consumes: Task 1 types/functions; existing `PlaneFrame`, `scene_domain`, `ViewportScene`, camera/ViewCommand.
- Produces: `FieldMode::Materials`; `State::{material,material_sampling,material_error,set_material_scene,refresh_material_sampling,reset_material}`; `MATERIAL_MAX_DENSITY_3D=40`, `MATERIAL_MAX_DENSITY_2D=200`; app branch and harness `material_line_count`, `material_arrow_count`, `material_sampling_error`, `material_scene`.

- [ ] **Step 1: Write failing state/app tests**

Create `tests/material_app_test.rs` with imports:

```rust
use vector_field_explorer::VfApp;
use vector_field_explorer::material::{material_field, MaterialCase, MaterialScene, MaterialView};
use vector_field_explorer::plane::{PlaneAxis, PlaneView};
use vector_field_explorer::source_scene::FieldMode;
```

Tests:

```rust
#[test]
fn materials_mode_is_static_arrows_and_outline_only() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Materials);
    app.test_harness().advance_simulation();
    let scene = app.test_harness().test_assemble_viewport_scene();
    assert!(app.test_harness().material_arrow_count() > 0);
    assert!(app.test_harness().material_line_count() > 0);
    assert!(scene.trails.is_empty() && scene.dots.is_empty() && scene.combined.is_empty());
    assert!(!app.state.has_time_dependent_consumer());
}

#[test]
fn materials_2d_sampling_is_a_projection_of_the_3d_case() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Materials);
    app.state.set_mode(true);
    app.test_harness().advance_simulation();
    let world = app.state.material_sampling.clone().unwrap();
    app.state.set_mode(false);
    app.test_harness().advance_simulation();
    let slice = app.state.material_sampling.clone().unwrap();
    assert!(!slice.arrows.is_empty());
    let frame = app.state.plane_frame().expect("2D Materials has a plane frame");
    for arrow in &slice.arrows {
        assert!(arrow.position.z.abs() <= 1e-6);
        let world_point = frame.to_world(arrow.position);
        let projected = frame.project(material_field(&app.state.material, world_point).unwrap());
        assert!((arrow.dir - projected.normalize()).length() <= 1e-4);
        assert!((arrow.mag - projected.length()).abs() <= 1e-4);
    }
    assert_eq!(world.family, slice.family);
    assert_eq!(world.view, slice.view);
}

#[test]
fn material_view_switches_without_mixing_families() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Materials);
    for view in [MaterialView::Total, MaterialView::Applied, MaterialView::Response] {
        app.state.set_material_scene(MaterialScene { view, ..MaterialScene::default() }).unwrap();
        app.test_harness().advance_simulation();
        let sampling = app.state.material_sampling.as_ref().unwrap();
        assert_eq!(sampling.family, vector_field_explorer::material::MaterialFamily::Electric);
        assert_eq!(sampling.view, view);
        assert!(sampling.arrows.iter().all(|a| a.color == [255, 176, 64]));
    }
}

#[test]
fn invalid_edit_retains_last_sampling_and_sets_error() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Materials);
    app.test_harness().advance_simulation();
    let before = app.state.material_sampling.clone().unwrap();
    let mut bad = MaterialScene::default();
    bad.material_epsilon_r = f64::NAN;
    assert!(app.state.set_material_scene(bad).is_err());
    assert!(app.state.material_error.is_some());
    app.test_harness().advance_simulation();
    assert_eq!(app.state.material_sampling.as_ref().unwrap().arrows.len(), before.arrows.len());
}

#[test]
fn material_edit_rebuilds_only_sampling_and_preserves_other_scenes() {
    let mut app = VfApp::new();
    app.state.set_formula("y, -x, 0");
    app.state.set_field_mode(FieldMode::Materials);
    app.test_harness().advance_simulation();
    let before = app.state.material_sampling.clone().unwrap();
    app.test_harness().advance_simulation();
    assert_eq!(app.state.material_sampling.as_ref().unwrap(), &before);
    let mut changed = MaterialScene::default();
    changed.radius = 0.5;
    app.state.set_material_scene(changed).unwrap();
    app.test_harness().advance_simulation();
    assert_ne!(app.state.material_sampling.as_ref().unwrap(), &before);
    app.state.set_field_mode(FieldMode::Formula);
    assert_eq!(app.state.formula, "y, -x, 0");
}

#[test]
fn plane_change_and_reset_use_the_existing_frame_contract() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Materials);
    app.state.set_mode(false);
    app.state.set_plane(PlaneView { enabled: true, axis: PlaneAxis::Z, index: 0 });
    app.test_harness().advance_simulation();
    let first = app.state.material_sampling.clone().unwrap();
    app.state.set_plane_index(4);
    app.test_harness().advance_simulation();
    let second = app.state.material_sampling.clone().unwrap();
    assert_ne!(first.arrows, second.arrows);
    app.state.reset_material();
    assert_eq!(app.state.material, MaterialScene::default());
    assert!(app.state.material_sampling.is_none());
}
```

Add tests for empty domain warning and outline reaching scene, and that entering Materials does not clear a retained particle cloud? Design says mode transient clear; so no need preserve particles. Test formula/source/induction/ODE preserved.

- [ ] **Step 2: Run and confirm RED**

Run: `cargo test --test suite material_app_test::`
Expected: compile failures for `FieldMode::Materials`, State APIs and harness methods.

- [ ] **Step 3: Implement State and mode integration**

- Add `FieldMode::Materials`; update every exhaustive match in UI/app/persistence/export. Keep Sources/Induction/Ode branches unchanged.
- State fields exactly design; default `MaterialScene::default()`; `material_sampling: None`; `material_error: None`.
- `active_dimension` returns 3 for Materials; `is_plane_slice`/`plane_frame`/`scene_domain` reuse the existing 3D slice behavior; `particle_target` None; `has_time_dependent_consumer` false; `scene_flow` unused.
- `set_material_scene(scene) -> Result<(), MaterialError>` validates, stores scene, clears sampling/error, marks dirty. On error retain prior scene/sampling and set readable `material_error`.
- `refresh_material_sampling` only in Materials; choose grid `40³` in 3D or `min(200,density)²` in 2D, call `sample_materials` with durable domain/plane; store sampling, and if `warning` or validation error store readable error/status.
- `reset_material` restores default, clears sampling/error, marks dirty.
- `set_field_mode(Materials)` preserves projection/camera/formula/source/induction/ode scenes, sets vector plot kind, clears transient field consumers, and does not sample synchronously.
- Mode switches preserve durable material scene when leaving.

- [ ] **Step 4: Implement app branch**

- In `advance_simulation`, branch Materials before Formula: refresh sampling if dirty; set `SceneGPU.arrows` from sampling arrows, `lines` from `material_outline`; empty trails/dots/combined; no field sampling, clicks, particles, streamlines, glyphs, or scalar plot.
- In `rebuild_scene`/`advance_particles`, treat Materials as arrows-only and do not advect particles.
- Status bar shows `material_error` or sampling warning in Materials.
- Add harness seams `material_line_count`, `material_arrow_count`, `material_sampling_error`, `material_scene`; outline lines count excludes reference lines.
- Fit/Reset use existing `ViewContext` with durable `scene_domain`; no new camera path.

- [ ] **Step 5: Run focused regressions and commit**

```bash
cargo test --test suite material_app_test::
cargo test --test suite particle_playback_test::
cargo test --test suite plot_app_test::
cargo test --test suite source_mode_test::
cargo test --test suite induction_app_test::
cargo test --test suite ode_app_test::
git add src/source_scene.rs src/viewmodel.rs src/viewmodel/plot.rs src/app tests/material_app_test.rs tests/suite.rs docs/CODEBASE.md
git commit -m "feat: add the Materials vector-field mode" -m "Generated-by: OpenCode"
```

---

### Task 3: Responsive Materials panel, selector and help

**Files:**
- Create: `src/ui/material_panel.rs`, `tests/material_panel_test.rs`
- Modify: `src/ui/mod.rs`, `src/ui/formula_bar.rs`, `src/ui/layout.rs`, `src/ui/sidebar.rs`, `src/ui/cas_panel.rs`, `src/ui/help_overlay.rs`
- Modify: `src/app/panels.rs`, `tests/suite.rs`, `docs/CODEBASE.md`

**Interfaces:**
- Consumes: Task 2 State fields/setters; existing layout/rail/sheet/accesskit patterns.
- Produces: `MaterialPanelState { draft, error, advanced }`, `MaterialPanelAction::{Fit,Reset}`, `MaterialPanelState::{commit,sync_from}`, `show(ui,&mut MaterialPanelState,&mut State,compact)->Option<MaterialPanelAction>`, `material_help_rows()`; `DEFAULT_MATERIALS_WIDTH=300`; `LayoutState::materials_expanded/materials_width/toggle_materials`; `SheetState::Materials`; FIFTH selector `Materials`.

- [ ] **Step 1: Write failing real-widget tests**

Create `tests/material_panel_test.rs`; copy the accesskit helpers from `tests/induction_panel_test.rs`.

```rust
use vector_field_explorer::material::{MaterialCase, MaterialScene, MaterialView};
use vector_field_explorer::source_scene::FieldMode;
use vector_field_explorer::ui::material_panel::{material_help_rows, MaterialPanelState};
use vector_field_explorer::viewmodel::State;

#[test]
fn material_panel_lists_all_cases_and_view_choices() {
    let ctx = context();
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Materials);
    let mut panel = MaterialPanelState::default();
    let out = run_frame(&ctx, &mut state, &mut panel, 1200.0, 900.0);
    for label in ["Materials", "Dielectric sphere", "Dielectric boundary", "Permeable sphere", "Permeable boundary", "Total", "Applied", "Response", "Fit", "Reset Materials section"] {
        assert!(has(&out, label), "missing {label}");
    }
}
```

Add tests:
- `material_geometry_and_constants_commit_on_release`: edit radius/eps/mu and applied; state changes only after `commit`; invalid draft retains last scene.
- `invalid_material_draft_is_actionable_and_keeps_last_valid`: NaN/0/out-of-range message visible.
- `materials_view_selection_and_reset_use_state`: click views, assert `state.material.view`; Reset restores default and calls Fit? Reset action only.
- `materials_compact_and_wide_panels_reflow`: 1200/390; all required widgets inside panel.
- `materials_features_and_cas_are_disabled`: exact note `Arrows only: streamlines, particles and click seeding stay off in Materials.`; CAS exact existing message.
- `material_help_rows_state_the_approximations`: rows contain `homogeneous`, `linear`, `quasistatic`, `no general solver`, `interface`.
- `open_materials_reaches_the_wide_rail_or_compact_sheet`.

- [ ] **Step 2: Run and confirm RED**

Run: `cargo test --test suite material_panel_test::`
Expected: missing panel/layout/selector/help APIs.

- [ ] **Step 3: Implement panel and layout**

- `MaterialPanelState::commit` calls `state.set_material_scene`; only on success syncs draft; on failure retains draft/error.
- `MaterialPanelState::sync_from(&State)` copies `state.material` into the draft and clears the panel error. Call it on mode entry, reset, and persistence restore; never every frame, so a dirty draft survives.
- Fields: case selector; sphere radius or boundary axis/offset as appropriate; background and material εr/μr; applied E/B with exact units `V/m`/`T`; View; Fit; Reset; closed Advanced with assumptions, tolerance, omitted count, warning, and `No general material solver.`
- Commit on focus loss/Enter, drag release, selector click, reset; never per frame.
- Layout: `DEFAULT_MATERIALS_WIDTH=300`; rail/sheet only in Materials; `Open materials` opens wide rail or compact sheet; close sheet on mode exit. App `panels` owns the panel state, calls `sync_from` on mode entry/reset/restore, and routes `Fit`/`Reset` actions.
- Formula bar fifth `Materials` selector; hide editor and show `Materials scene summary`, selected case, and `Open materials`.
- Sidebar: Features disabled with exact note; Coloring hidden; Domain remains. CAS disabled with existing exact text.
- Viewport label `Materials - {case} - {Total|Applied|Response}`; Response-empty note from sampling.

- [ ] **Step 4: Add help and run UI regressions**

Add `material_help_rows()` in `ui::material_panel` with case/family assumptions and bounded/no-solver/interface wording. Render in Help window.

```bash
cargo test --test suite material_panel_test::
cargo test --test suite button_sizing_test::
cargo test --test suite responsive_test::
cargo test --test suite layout_test::
cargo test --test suite induction_panel_test::
cargo test --test suite ode_panel_test::
cargo test --test suite sources_panel_test::
cargo test --test suite cas_panel_test::
cargo test --test suite help_overlay_test::
git add src/ui/material_panel.rs src/ui/mod.rs src/ui/formula_bar.rs src/ui/layout.rs src/ui/sidebar.rs src/ui/cas_panel.rs src/ui/help_overlay.rs src/app/panels.rs tests/material_panel_test.rs tests/suite.rs docs/CODEBASE.md
git commit -m "feat: add the responsive Materials panel" -m "Generated-by: OpenCode"
```

---

### Task 4: Materials persistence, reset and export provenance

**Files:**
- Modify: `src/persistence.rs`, `src/viewmodel.rs`, `src/app/input.rs`, `src/app/plot.rs`, `src/app/viewport.rs`, `src/export_provenance.rs`
- Create: `tests/material_persistence_test.rs`, `tests/material_export_test.rs`
- Modify: `tests/suite.rs`, `tests/persistence_test.rs`, `docs/CODEBASE.md`

**Interfaces:**
- Consumes: Task 2/3 State/panel; existing `ViewRecord`, PNG/SVG provenance helpers.
- Produces: `ViewRecord` material fields; `restore_material_fields`; `material_export_provenance(mode, scene, Option<&MaterialSampling>)`; `ExportProvenance.material_mode`.

- [ ] **Step 1: Write failing persistence/export tests**

Create `tests/material_persistence_test.rs` with imports and tests:
- full round-trip of all material fields (`case`, both ε/μ pairs, radius, axis, offset, applied, view), no sampling after restore.
- old record defaults to dielectric sphere; malformed (NaN/out-of-range, bad axis) falls back to default + `material_error`, does not promote Materials when stored mode absent.
- global reset clears material scene/sampling/error and returns Formula; Materials section reset only restores default.
- `restore_material_fields` validates via `validate_material` and sets pending/no synchronous sample.

Create `tests/material_export_test.rs` using existing decoded PNG/SVG helpers:
- PNG decoded banner contains `Materials`, bounded/no-solver, `Applied/response`, units; tEXt has `vfe.model=materials-bounded-analytic`, case/family/view/units/constants and warning.
- SVG visible text + `<metadata>` has same context; 2D-only SVG rule preserved.
- Non-Materials export carries no material metadata.
- Warning remains for `Response` empty and interface omission; opt-in/other modes cannot suppress it.
- No nonfinite arrows in exported geometry.

- [ ] **Step 2: Run and confirm RED**

Run: `cargo test --test suite material_persistence_test:: material_export_test::`
Expected: missing fields/functions.

- [ ] **Step 3: Implement persistence/reset**

- Add serde-default `material_case`, `material_background_epsilon_r`, `material_background_mu_r`, `material_material_epsilon_r`, `material_material_mu_r`, `material_radius`, `material_interface_axis`, `material_interface_offset`, `material_applied`, `material_view`.
- `restore_material_fields` validates and stores scene; no sampling. Malformed sets default + error, keeps `ode`/source/induction untouched, and never promotes Materials without stored mode.
- `reset_material` clears scene/sampling/error; global reset calls it then Formula. Materials section reset restores default scene only.

- [ ] **Step 4: Implement request-time provenance and routing**

- Add `material_export_provenance(mode, scene, sampling)`, pure; exact lines from design plus assumptions. Use `material_family` for E/B units.
- Add `ExportProvenance.material_mode`; request-time router covers all five modes; no sampling at export.
- Route PNG banner/tEXt and SVG text/metadata; preserve 2D-only SVG.
- No nonfinite data; warning never suppressible.

- [ ] **Step 5: Run focused tests and commit**

```bash
cargo test --test suite material_persistence_test::
cargo test --test suite material_export_test::
cargo test --test suite persistence_test::
cargo test --test suite source_export_test::
cargo test --test suite svg_export_test::
git add src/persistence.rs src/viewmodel.rs src/app/input.rs src/app/plot.rs src/app/viewport.rs src/export_provenance.rs tests/material_persistence_test.rs tests/material_export_test.rs tests/persistence_test.rs tests/suite.rs docs/CODEBASE.md
git commit -m "feat: persist and export Materials context" -m "Generated-by: OpenCode"
```

---

### Task 5: Docs, benchmark, full gates

**Files:**
- Modify: `README.md`, `GLOSSARY.md`, `docs/CODEBASE.md`, `docs/superpowers/README.md`, `docs/perf/materials-performance.md`
- Evidence/report: `.superpowers/sdd/2026-11-17-materials/evidence/task-5/`, `task-5-report.md`

- [ ] **Step 1: Document the bounded model**

Document all four cases, exact formulas/interface conditions, total/applied/response, no-contrast limit, applicability/validation, exact interface omission and near-interface retention, no softening, 2D slice, panel/view label, persistence/migration/reset, export warnings/metadata, no general solver, no particles/clock/CAS, and sampling-only benchmark. Add glossary entries **Material case**, **Applied field**, **Response field**.
- State browser/wasm-runtime/manual visual evidence is owner-waived, not passed.
- Correct any CODEBASE omissions from earlier tasks.

- [ ] **Step 2: Integrated benchmark**

Run `scripts/bench-materials.sh` at 40³ and 200² for the densest view. Record host/date/case/sample count/ms/omitted/response status. Do not claim fps; if >2 s, report honestly and do not change caps silently.

- [ ] **Step 3: Run all gates**

```bash
cargo test --test suite
cargo build
cargo clippy --all-targets -- -D warnings
rustfmt --edition 2024 --check --config skip_children=true <changed .rs files>
python3 scripts/check-docs.py
git diff --check
scripts/smoke-run.sh 15
cargo check --target wasm32-unknown-unknown
```

Record unavailable wasm/GPU targets honestly. Only nom 1.2.4 warning expected.

- [ ] **Step 4: Commit docs and evidence**

```bash
git add README.md GLOSSARY.md docs/CODEBASE.md docs/superpowers/README.md docs/perf/materials-performance.md
git commit -m "docs: document bounded material cases" -m "Generated-by: OpenCode"
```

---

## Self-Review

- **Spec coverage:** Five tasks cover pure formulas/outline, mode/state/app, UI, persistence/export, docs/benchmark/gates; every design acceptance class has an owning test.
- **Step scan:** Every step names exact files, APIs, test names/assertions, commands, or commit. No placeholders.
- **Type consistency:** `MaterialScene`, `MaterialSampling`, State fields/setters, panel actions, provenance and harness names are identical across tasks.
- **Review Focus:** Interface signs, exact-hit omission, reconstruction, 2D slice, mode isolation and decoded export are each pinned.
- **Proportion:** The plan specifies decisions and tests without transcribing every body; formulas the design fixes are stated.
