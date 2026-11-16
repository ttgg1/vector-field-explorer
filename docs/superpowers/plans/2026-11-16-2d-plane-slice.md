# 2D Plane Slice and Projected Flow Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make 2D mode a selected, projected slice of the same three-component field, keep particle and streamline seeds on that slice, and add a persisted streamline seed-count control.

**Architecture:** Add a plane-local coordinate frame to `plane.rs`, use it for slice sampling, references, camera/click coordinates, flow evaluation, streamlines and particles, and keep the existing world path untouched for 3D and two-component fields. A small `Flow` abstraction lets the tracer and particle code consume either the world field or a projected plane flow. A scene-frame key clears stale seeds when 2D plane geometry changes.

**Tech Stack:** Rust 2024, eframe/egui, glam, wgpu-backed renderer, serde persistence, existing `cargo test --test suite` harness.

**Spec:** `docs/superpowers/specs/2026-11-16-2d-plane-slice-design.md`

## Global Constraints

- GPL-3.0-only; add no dependency and no CLI flag.
- Preserve full 3D behavior and ordinary two-component 2D behavior except where this plan explicitly changes plane-slice behavior.
- `plane_grid` is `n³` for a three-component field in both modes, and `n×n×1` for a two-component field; the 2D render grid is always `n²`.
- Projected flow uses only the `right`/`up` components; normal velocity is omitted in 2D and retains its 3D meaning.
- 2D particles are available only for a three-component field. Two-component-field 2D particles remain disabled.
- Streamline count is `0 = Auto`, explicit values are clamped to `1..=4096`, and no CLI flag is added.
- Do not change density caps, scalar plotting, SVG/PNG behavior, or unrelated UI.
- Use `cargo fmt --edition 2024` only on changed files; do not reformat unrelated files.
- Use `CARGO_BUILD_JOBS=10`; serialize Cargo commands while another task owns the target directory.
- Every commit ends with `Generated-by: OpenCode`.

## Review Focus

- **World/local boundary:** camera target, fit, clicks, reference axes, streamlines and particle positions must all agree on whether a point is world or scene-local.
- **Degenerate projected flow:** a field whose velocity is entirely normal to the slice must produce no arrows/lines and must respawn or stagnate particles without leaving the plane.
- **Plane-grid stability:** switching 2D/3D must preserve the selected index where possible and must not fall back to a single z midpoint.
- **Stale seeds:** entering, leaving, or changing a plane must not leave hidden click streamlines, undo records, particles or trails.
- **Explicit count semantics:** explicit streamline counts must be exact, density-independent, bounded, persisted, and disabled in Click-only mode.
- **Ordinary 2D:** a two-component field must keep one XY layer, the fixed front camera, and disabled particles.

---

### Task 1: Plane frame, plane grid, and local scene geometry

**Files:**
- Modify: `src/plane.rs`
- Modify: `src/viewmodel.rs`
- Modify: `tests/plane_test.rs`
- Modify: `tests/viewmodel_test.rs`
- Modify: `tests/suite.rs` only if a new test module is introduced

**Interfaces:**
- Produces `PlaneFrame { axis: PlaneAxis, origin: Vec3, right: Vec3, up: Vec3, normal: Vec3, half_u: f32, half_v: f32 }`.
- Produces `PlaneFrame::new(domain: &Domain, plane: PlaneView, grid: UVec3) -> PlaneFrame`.
- Produces `PlaneFrame::local_domain(&self) -> Domain`, `to_world(&self, local: Vec3) -> Vec3`, `to_local(&self, world: Vec3) -> Vec3`, `project(&self, world_vector: Vec3) -> Vec3`.
- Produces `State::plane_grid(&self) -> UVec3`, `State::is_plane_slice(&self) -> bool`, `State::plane_frame(&self) -> Option<PlaneFrame>`, `State::scene_domain(&self) -> Domain`.
- Uses existing `PlaneAxis`, `PlaneView`, `plane_coordinate`, `Domain`.

- [ ] **Step 1: Write failing frame/grid tests**

In `tests/plane_test.rs`, add assertions with the exact basis table:

```rust
#[test]
fn plane_frame_basis_matches_the_design_table() {
    let domain = Domain::new(Vec3::new(-2.0, -3.0, -4.0), Vec3::new(2.0, 3.0, 4.0));
    let grid = UVec3::new(5, 5, 5);
    let z = PlaneFrame::new(&domain, PlaneView { axis: PlaneAxis::Z, index: 2, ..Default::default() }, grid);
    assert_eq!(z.right, Vec3::X);
    assert_eq!(z.up, Vec3::Y);
    assert_eq!(z.normal, Vec3::Z);
    assert_eq!(z.origin, Vec3::new(0.0, 0.0, 0.0));
    let x = PlaneFrame::new(&domain, PlaneView { axis: PlaneAxis::X, index: 2, ..Default::default() }, grid);
    assert_eq!(x.right, -Vec3::Z);
    assert_eq!(x.up, Vec3::Y);
    assert_eq!(x.normal, Vec3::X);
    let y = PlaneFrame::new(&domain, PlaneView { axis: PlaneAxis::Y, index: 2, ..Default::default() }, grid);
    assert_eq!(y.right, -Vec3::X);
    assert_eq!(y.up, Vec3::Z);
    assert_eq!(y.normal, Vec3::Y);
}
```

Add `local_domain`/round-trip assertions:

```rust
#[test]
fn plane_frame_round_trips_local_points_and_has_a_flat_domain() {
    let domain = Domain::new(Vec3::new(-4.0, -1.0, -6.0), Vec3::new(4.0, 1.0, 6.0));
    let frame = PlaneFrame::new(
        &domain,
        PlaneView { axis: PlaneAxis::X, index: 2, ..Default::default() },
        UVec3::splat(5),
    );
    let local = Vec3::new(1.25, -0.5, 0.0);
    assert!((frame.to_local(frame.to_world(local)) - local).length() < 1e-5);
    assert_eq!(frame.local_domain().domain_size().z, 0.0);
    assert!((frame.local_domain().domain_size().x - 12.0).abs() < 1e-5);
    assert!((frame.local_domain().domain_size().y - 2.0).abs() < 1e-5);
}
```

In `tests/viewmodel_test.rs`:

```rust
#[test]
fn plane_grid_stays_volumetric_for_3d_fields_in_2d() {
    let mut state = State::new(false);
    state.set_formula("x, y, z".into());
    assert_eq!(state.plane_grid(), UVec3::splat(state.density));
    assert_eq!(state.grid().z, 1);
    state.set_mode(true);
    assert_eq!(state.plane_grid(), UVec3::splat(state.density));
}

#[test]
fn plane_grid_is_flat_for_two_component_fields() {
    let state = State::new(false);
    assert_eq!(state.plane_grid(), UVec3::new(state.density, state.density, 1));
}

#[test]
fn sync_plane_uses_the_plane_grid() {
    let mut state = State::new(false);
    state.set_formula("x, y, z".into());
    state.set_plane_axis(PlaneAxis::X);
    state.set_plane_index(3);
    state.set_mode(true);
    assert_eq!(state.plane.index, 3);
    assert_eq!(state.plane.clamped(state.plane_grid()).index, 3);
}

#[test]
fn set_plane_preserves_dim_toggle_in_2d() {
    let mut state = State::new(false);
    state.set_formula("x, y, z".into());
    state.set_mode(true);
    state.set_plane_enabled(true);
    state.set_mode(false);
    state.set_plane_axis(PlaneAxis::X);
    assert!(state.plane.enabled);
}
```

- [ ] **Step 2: Run the tests and confirm RED**

Run:
`cargo test --test suite plane_frame_basis_matches_the_design_table -- --exact`
and the four viewmodel filters above.

Expected: compile errors for missing `PlaneFrame`, `plane_grid`, `is_plane_slice`, `plane_frame`, and `scene_domain`.

- [ ] **Step 3: Implement the frame and state seams**

In `src/plane.rs`, implement the basis exactly as specified. `to_local` is
`(p - origin).dot(right/up/normal)`; `to_world` is
`origin + right*u + up*v + normal*w`; `project` returns
`(v.dot(right), v.dot(up), 0.0)`. `local_domain` is
`Domain::new(Vec3::new(-half_u, -half_v, 0.0), Vec3::new(half_u, half_v, 0.0))`.

In `src/viewmodel.rs`:

```rust
pub fn plane_grid(self: &State) -> UVec3 {
    let n = self.density.max(MIN_DENSITY);
    if self.field.dimension == 3 {
        UVec3::new(n, n, n)
    } else {
        UVec3::new(n, n, 1)
    }
}
```

`is_plane_slice` is `field.dimension == 3 && !mode_3d`. `plane_frame` returns
`Some(PlaneFrame::new(&self.domain, self.plane, self.plane_grid()))` only for
that case. `scene_domain` returns the frame's local domain when present, the
world domain in 3D, and for a two-component 2D field returns the world x/y box
with z collapsed to the world z midpoint. `plane_size` uses
`self.scene_domain()`.

Use `plane_grid()` in `set_plane`, `sync_plane`, and the plane-selector
clamping. In `set_plane`, when `is_plane_slice()` is true, retain the previous
`enabled` value so 2D cannot mutate the 3D dim toggle. `plane_active` remains a
3D-only predicate.

- [ ] **Step 4: Run the tests and confirm GREEN**

Run the filters from Step 2 plus:
`cargo test --test suite plane_test::`
`cargo test --test suite viewmodel_test::`

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add src/plane.rs src/viewmodel.rs tests/plane_test.rs tests/viewmodel_test.rs
git commit -m "feat: add plane frame and local slice geometry" -m "Generated-by: OpenCode"
```

---

### Task 2: Projected slice sampling, local camera, click, and references

**Files:**
- Modify: `src/sampling.rs`
- Modify: `src/viewmodel.rs`
- Modify: `src/reference.rs`
- Modify: `src/app/mod.rs`
- Modify: `src/app/simulation.rs`
- Modify: `src/app/input.rs`
- Modify: `src/app/panels.rs`
- Create: `tests/plane_scene_test.rs`
- Modify: `tests/reference_lines_test.rs`
- Modify: `tests/view_command_test.rs`
- Modify: `tests/suite.rs`

**Interfaces:**
- Consumes `PlaneFrame`, `State::plane_frame`, `State::scene_domain`, `State::is_plane_slice`.
- Produces `sample_plane_at(field: &FieldExpr, frame: &PlaneFrame, n: u32, view: &ViewOptions, t: f32) -> Vec<ArrowData>`.
- Produces `reference_plane_lines(frame: &PlaneFrame, show_grid: bool, show_coords: bool) -> Vec<LineInstanceGPU>`.
- Produces `compose_lines_with_frame(flow, clicks, domain, sampling_3d, frame: Option<PlaneFrame>, show_grid, show_coords) -> Vec<LineInstanceGPU>`.
- `State::sample` returns local positions/directions/magnitudes in a plane slice.
- `VfApp::view_context`, `click_world_seed`, and domain recentring use `scene_domain()`.

- [ ] **Step 1: Write failing sampling/reference/camera tests**

In `tests/plane_scene_test.rs`:

```rust
#[test]
fn slice_sampling_projects_onto_plane_basis() {
    let field = FieldExpr::parse("1, 2, 3").unwrap();
    let domain = Domain::new(Vec3::splat(-1.0), Vec3::splat(1.0));
    let frame = PlaneFrame::new(&domain, PlaneView { axis: PlaneAxis::Z, index: 1, ..Default::default() }, UVec3::splat(3));
    let arrows = sample_plane_at(&field, &frame, 3, &ViewOptions::default(), 0.0);
    assert!(!arrows.is_empty());
    assert!(arrows.iter().all(|a| a.position.z == 0.0));
    assert!((arrows[0].mag - 5f32.sqrt()).abs() < 1e-5);
    assert!(arrows[0].dir.z == 0.0);
    assert!((arrows[0].dir.x - 1.0 / 5f32.sqrt()).abs() < 1e-5);
}

#[test]
fn slice_sampling_omits_a_purely_normal_field() {
    let field = FieldExpr::parse("0, 0, 1").unwrap();
    let domain = Domain::new(Vec3::splat(-1.0), Vec3::splat(1.0));
    let frame = PlaneFrame::new(
        &domain,
        PlaneView { axis: PlaneAxis::Z, index: 2, ..Default::default() },
        UVec3::splat(5),
    );
    assert!(sample_plane_at(&field, &frame, 9, &ViewOptions::default(), 0.0).is_empty());
}

#[test]
fn plane_reference_axes_use_world_axis_colors() {
    let domain = Domain::new(Vec3::new(-2.0, -1.0, -3.0), Vec3::new(2.0, 1.0, 3.0));
    let frame = PlaneFrame::new(
        &domain,
        PlaneView { axis: PlaneAxis::X, index: 2, ..Default::default() },
        UVec3::splat(5),
    );
    let lines = reference_plane_lines(&frame, true, true);
    assert!(lines.iter().any(|line| line.color == AXIS_Z_COLOR));
    assert!(lines.iter().any(|line| line.color == AXIS_Y_COLOR));
    assert!(lines.iter().all(|line| line.a[2].abs() < 1e-6 && line.b[2].abs() < 1e-6));
}

#[test]
fn plane_fit_uses_local_extents() {
    let domain = Domain::new(Vec3::new(-10.0, -2.0, -1.0), Vec3::new(10.0, 2.0, 1.0));
    let frame = PlaneFrame::new(
        &domain,
        PlaneView { axis: PlaneAxis::X, index: 2, ..Default::default() },
        UVec3::splat(5),
    );
    let view = ViewContext::new(false, frame.local_domain(), 1.0);
    let camera = apply_view_command(OrbitCamera::default(), view, ViewCommand::Fit);
    assert!(camera.ortho_zoom > 0.0);
    assert!(camera.target.x.abs() < 1e-6);
    assert!(camera.target.y.abs() < 1e-6);
    assert!(camera.target.z.abs() < 1e-6);
}

#[test]
fn plane_click_lands_on_the_local_slice() {
    let domain = Domain::new(Vec3::new(-5.0, -2.0, -2.0), Vec3::new(5.0, 2.0, 2.0));
    let frame = PlaneFrame::new(
        &domain,
        PlaneView { axis: PlaneAxis::X, index: 3, ..Default::default() },
        UVec3::splat(5),
    );
    let bounds = frame.local_domain();
    let camera = OrbitCamera::default();
    let seed = click_seed(
        ViewCamera::Orbit(&camera),
        &bounds,
        1.0,
        Vec2::ZERO,
        Some(bounds.domain_size().x.max(bounds.domain_size().y) * PLANE_MARGIN),
    )
    .expect("a centred click must hit the local plane");
    assert!(seed.z.abs() < 1e-5);
    assert!(bounds.min.x <= seed.x && seed.x <= bounds.max.x);
    assert!(bounds.min.y <= seed.y && seed.y <= bounds.max.y);
}
```

The last test also proves the local reference is flat: every emitted line
endpoint has `z == 0`, so no 3D cube edge can be present.

- [ ] **Step 2: Run the tests and confirm RED**

Run `cargo test --test suite plane_scene_test:: -- --nocapture`.

Expected: missing `sample_plane_at`, `reference_plane_lines`, or local-coordinate behavior failures.

- [ ] **Step 3: Implement the plane sampler and references**

`sample_plane_at` iterates `frame.local_domain().grid_points(UVec3::new(n, n, 1))`,
converts each local point to world, calls `field.eval3_at`, projects with
`frame.project`, drops non-finite or near-zero projected vectors, normalizes the
projected direction, and uses the projected magnitude for color auto-ranging.
`State::sample` calls it when `plane_frame()` is `Some`, otherwise keeps the
existing `sample_field_at` call.

`reference_plane_lines` emits a local grid and two axes. Colors follow the world
axis of `frame.right` and `frame.up`: Z slice → X/Y, X slice → Z/Y, Y slice →
X/Z. Do not emit a local cube.

- [ ] **Step 4: Route the app through local scene coordinates**

Add `compose_lines_with_frame` with the signature above; keep the existing
`compose_lines` wrapper for current callers. Plane frames use
`reference_plane_lines`; world frames use `reference_lines_visible`.

Change `VfApp::view_context()` to use `self.state.scene_domain()`. Change
`click_world_seed`/`apply_click_action` recomposition and `app/panels.rs`
domain recentring to use `scene_domain()`. `recentre_on_2d` now receives the
local domain for a plane slice. `VfApp::new_with_storage` recentres 2D on
`state.scene_domain()`.

- [ ] **Step 5: Run focused tests and the existing camera/reference tests**

Run:
`cargo test --test suite plane_scene_test::`
`cargo test --test suite reference_lines_test::`
`cargo test --test suite view_command_test::`
`cargo test --test suite camera_test::`

Expected: all pass; ordinary 2D and 3D camera tests stay green.

- [ ] **Step 6: Commit**

```bash
git add src/sampling.rs src/viewmodel.rs src/reference.rs src/app/mod.rs src/app/simulation.rs src/app/input.rs src/app/panels.rs tests/plane_scene_test.rs tests/reference_lines_test.rs tests/view_command_test.rs tests/suite.rs
git commit -m "feat: render projected plane slices" -m "Generated-by: OpenCode"
```

---

### Task 3: Shared flow abstraction and plane streamlines

**Files:**
- Create: `src/flow.rs`
- Modify: `src/lib.rs`
- Modify: `src/streamlines.rs`
- Modify: `src/viewmodel.rs`
- Modify: `tests/streamlines_test.rs`
- Create: `tests/scene_flow_test.rs`
- Modify: `tests/suite.rs`

**Interfaces:**
- Produces `pub trait Flow { fn eval(&self, point: Vec3, t: f32) -> Option<Vec3>; }`.
- `impl Flow for FieldExpr` delegates to `eval3_at`.
- Produces `PlaneFlow<'a> { field: &'a FieldExpr, frame: PlaneFrame }` and `SceneFlow<'a> { World(&'a FieldExpr), Plane(PlaneFlow<'a>) }`.
- Produces `State::scene_flow(&self) -> SceneFlow<'_>`.
- Genericizes `trace_at<F: Flow + ?Sized>(flow: &F, start: Vec3, domain: &Domain, opts: &StreamlineOpts, t: f32) -> Vec<Vec3>`.
- `State::streamlines_snapshot` and `click_streamlines_snapshot` use `scene_flow()` and `scene_domain()`, and color by the projected flow magnitude.

- [ ] **Step 1: Write failing projected-flow tests**

In `tests/scene_flow_test.rs`:

```rust
#[test]
fn plane_flow_projects_out_of_plane_velocity() {
    let field = FieldExpr::parse("1, 2, 3").unwrap();
    let domain = Domain::new(Vec3::splat(-1.0), Vec3::splat(1.0));
    let frame = PlaneFrame::new(
        &domain,
        PlaneView { axis: PlaneAxis::Z, index: 2, ..Default::default() },
        UVec3::splat(5),
    );
    let flow = PlaneFlow { field: &field, frame };
    let v = flow.eval(Vec3::ZERO, 0.0).unwrap();
    assert_eq!(v, Vec3::new(1.0, 2.0, 0.0));
}

#[test]
fn plane_streamlines_never_leave_the_slice() {
    let mut state = State::new(false);
    state.set_formula("y, -x, 1".into());
    state.set_streamlines(true);
    let lines = state.streamlines_snapshot(&state.sample());
    assert!(!lines.is_empty());
    assert!(lines.iter().all(|line| line.a[2].abs() < 1e-5 && line.b[2].abs() < 1e-5));
}

#[test]
fn click_streamlines_stay_on_the_slice() {
    let mut state = State::new(false);
    state.set_formula("y, -x, 1".into());
    state.set_streamlines(true);
    let range = state.color_range(&state.sample());
    let lines = state.click_streamlines_snapshot(range, &[Vec3::ZERO]);
    assert!(!lines.is_empty());
    assert!(lines.iter().all(|line| line.a[2].abs() < 1e-5 && line.b[2].abs() < 1e-5));
}

#[test]
fn world_streamlines_are_unchanged() {
    let mut state = State::new(true);
    state.set_formula("0, 0, 1".into());
    state.set_streamlines(true);
    let lines = state.streamlines_snapshot(&state.sample());
    assert!(!lines.is_empty());
    assert!(lines.iter().any(|line| line.a[2].abs() > 1e-4 || line.b[2].abs() > 1e-4));
}

#[test]
fn ordinary_2d_streamlines_seed_one_xy_layer() {
    let mut state = State::new(false);
    state.set_streamlines(true);
    let z = state.scene_domain().center().z;
    let lines = state.streamlines_snapshot(&state.sample());
    assert!(!lines.is_empty());
    assert!(lines.iter().all(|line| (line.a[2] - z).abs() < 1e-5 && (line.b[2] - z).abs() < 1e-5));
}
```

- [ ] **Step 2: Run the tests and confirm RED**

Run `cargo test --test suite scene_flow_test:: -- --nocapture`.

Expected: `flow` module/trait and generic `trace_at` are missing; plane lines leave z=0.

- [ ] **Step 3: Implement `src/flow.rs`**

Implement the trait, `FieldExpr` impl, `PlaneFlow` (local→world, evaluate,
`frame.project`), and `SceneFlow`. Add `pub mod flow;` to `src/lib.rs`.

- [ ] **Step 4: Make tracing and state snapshots use the scene flow**

Change `trace_at` to generic over `Flow + ?Sized`; keep existing calls valid
with `&FieldExpr`. Add `State::scene_flow()`: world in 3D, plane in a
three-component 2D slice. For an ordinary two-component 2D field, the scene
flow is world `FieldExpr` over the z-collapsed `scene_domain`.

Change auto streamline seeds to `self.scene_domain()`. Change click seeds to
burst on `self.scene_domain()` and trace through `self.scene_flow()`. Color
segments using `flow.eval(pa, t)` magnitude. Do not change `STREAMLINE_WIDTH`,
`STREAMLINE_MAX_LEN`, the 400-point cap, or 3D seed behavior.

- [ ] **Step 5: Run focused tests**

Run:
`cargo test --test suite scene_flow_test::`
`cargo test --test suite streamlines_test::`
`cargo test --test suite viewmodel_test::`

Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add src/flow.rs src/lib.rs src/streamlines.rs src/viewmodel.rs tests/scene_flow_test.rs tests/streamlines_test.rs tests/suite.rs
git commit -m "feat: trace streamlines through projected flow" -m "Generated-by: OpenCode"
```

---

### Task 4: Plane particles, projected advection, and clock consumption

**Files:**
- Modify: `src/particles.rs`
- Modify: `src/app/simulation.rs`
- Modify: `src/app/input.rs`
- Modify: `src/sim_clock.rs`
- Modify: `src/viewmodel.rs`
- Modify: `src/view_command.rs`
- Modify: `src/ui/sidebar.rs`
- Create: `tests/plane_particles_test.rs`
- Modify: `tests/particles_test.rs`
- Modify: `tests/left_click_action_test.rs`
- Modify: `tests/suite.rs`

**Interfaces:**
- Genericizes `advect<F: Flow + ?Sized>`, `dot_instances_at<F: Flow + ?Sized>`, `trail_instances_at<F: Flow + ?Sized>` and private `particle_rgb` over `Flow`.
- `State::particle_target` returns a target when `particles` is on and `field.dimension == 3`, independent of `mode_3d`.
- `click_action_plan(action, streamlines, particles_available: bool, mode_3d: bool)` (or the equivalent predicate) allows AddParticles in a plane slice.
- `SimClock::has_clock_consumer(state, field)` uses `state.particle_target().is_some() || field.is_time_dependent()`.
- `VfApp::advance_particles` uses `state.scene_flow()` and `state.scene_domain()`; plane spawn dimension is 2, 3D dimension is 3.

- [ ] **Step 1: Write failing particle tests**

In `tests/plane_particles_test.rs` drive the app or pure functions:

```rust
#[test]
fn plane_particles_spawn_and_stay_on_slice() {
    let mut app = VfApp::new();
    app.state.set_formula("x, y, z".into());
    app.state.set_mode(false);
    app.state.set_particles(true);
    app.advance_simulation();
    let (positions, trails, _, _) = app.test_harness().particle_snapshot();
    assert!(!positions.is_empty());
    assert!(positions.iter().all(|p| p.z.abs() < 1e-5));
    assert!(trails.iter().flatten().all(|p| p.z.abs() < 1e-5));
}

#[test]
fn plane_particles_advect_with_projected_flow() {
    let mut app = VfApp::new();
    app.state.set_formula("1, 0, 100".into());
    app.state.set_mode(false);
    app.state.set_particles(true);
    app.advance_simulation();
    let before = app.test_harness().particle_snapshot().0;
    app.advance_simulation();
    let after = app.test_harness().particle_snapshot().0;
    assert!(!before.is_empty() && before.len() == after.len());
    assert!(after.iter().all(|p| p.z.abs() < 1e-5));
    assert!(
        before.iter().zip(&after).any(|(a, b)| (a.x - b.x).abs() > 1e-6),
        "the in-plane component must move the particles"
    );
}

#[test]
fn ordinary_two_component_particles_remain_disabled() {
    let mut app = VfApp::new();
    app.state.set_particles(true);
    app.advance_simulation();
    assert!(app.test_harness().particle_snapshot().0.is_empty());
}

#[test]
fn plane_particles_consume_the_clock() {
    let mut state = State::new(false);
    state.set_formula("x, y, z".into());
    state.set_particles(true);
    assert!(state.particle_target().is_some());
    assert!(SimClock::has_clock_consumer(&state));
    state.set_particles(false);
    assert!(!SimClock::has_clock_consumer(&state));
}

#[test]
fn click_plan_allows_particles_for_a_3d_field_in_2d() {
    assert_eq!(click_action_plan(LeftClickAction::AddParticles, false, true, false), ClickActionPlan::AddParticles);
}
```

- [ ] **Step 2: Run the tests and confirm RED**

Run:
`cargo test --test suite plane_particles_test:: -- --nocapture`
`cargo test --test suite particles_test::`
`cargo test --test suite left_click_action_test::`

Expected: particles still clear in 2D, `particle_target` is `None`, and the click plan still says “Particles need the 3D view”.

- [ ] **Step 3: Genericize particle flow consumers**

Change the particle functions to take `flow: &F` rather than `field: &FieldExpr`.
`particle_rgb` magnitude uses `flow.eval(point, time)?.length()`. `spawn`,
`resize`, `retarget`, and `spawn_at` stay coordinate-agnostic; `dimension == 2`
pins z to the local scene domain midpoint (0 in a plane frame).

Change `State::particle_target` to `particles && field.dimension == 3`.
Update `SimClock::has_clock_consumer` to consume the clock while that target is
present.

- [ ] **Step 4: Route app particles and click actions through scene geometry**

In `app/simulation.rs`, use `state.scene_flow()`, `state.scene_domain()`, and
spawn dimension `if state.mode_3d { 3 } else { 2 }`. In `app/input.rs`, use the
scene domain for AddParticles and click bursts; gate undo with
`state.particle_target().is_some()`. Change `click_action_plan` to take the
availability predicate, update the hint to “Particles need a 3D field”, and
update the sidebar call site and tests.

- [ ] **Step 5: Run focused tests**

Run:
`cargo test --test suite plane_particles_test::`
`cargo test --test suite particles_test::`
`cargo test --test suite left_click_action_test::`
`cargo test --test suite sim_clock_test::`

Expected: all pass; 3D particle tests and ordinary 2D disabled behavior stay green.

- [ ] **Step 6: Commit**

```bash
git add src/particles.rs src/app/simulation.rs src/app/input.rs src/sim_clock.rs src/viewmodel.rs src/view_command.rs src/ui/sidebar.rs tests/plane_particles_test.rs tests/particles_test.rs tests/left_click_action_test.rs tests/suite.rs
git commit -m "feat: animate particles on plane slices" -m "Generated-by: OpenCode"
```

---

### Task 5: Explicit streamline seed count

**Files:**
- Modify: `src/streamlines.rs`
- Modify: `src/viewmodel.rs`
- Modify: `src/ui/sidebar.rs`
- Modify: `src/persistence.rs`
- Create: `tests/streamline_count_test.rs`
- Modify: `tests/persistence_test.rs`
- Modify: `tests/sidebar_controls_test.rs`
- Modify: `tests/suite.rs`

**Interfaces:**
- Produces `pub const MAX_STREAMLINES: u32 = 4096`.
- Produces `pub fn explicit_streamline_seeds(domain: &Domain, count: u32, three_dimensional: bool, salt: u64) -> Vec<Vec3>`: `n = ceil(count^(1/3))` for 3D, `ceil(sqrt(count))` for 2D; build a `seed_grid` lattice and take exactly `count` seeds.
- `State::streamline_count: u32` (default 0), `State::set_streamline_count(&mut self, count: u32) -> bool`.
- `Controls::streamline_count`, `streamline_count_label(u32) -> String`,
  `streamline_count_enabled(streamlines: bool, seed_mode: StreamlineSeedMode) -> bool`,
  apply/mirror/reset.
- `ViewRecord` gains `streamline_count: u32` with `#[serde(default)]`.

- [ ] **Step 1: Write failing count tests**

In `tests/streamline_count_test.rs`:

```rust
#[test]
fn explicit_streamline_seeds_returns_exactly_n() {
    let domain = Domain::new(Vec3::splat(-1.0), Vec3::splat(1.0));
    assert_eq!(explicit_streamline_seeds(&domain, 1, false, 7).len(), 1);
    assert_eq!(explicit_streamline_seeds(&domain, 37, false, 7).len(), 37);
    assert_eq!(explicit_streamline_seeds(&domain, 4096, false, 7).len(), 4096);
}

#[test]
fn explicit_seed_lattice_is_density_independent() {
    let domain = Domain::new(Vec3::splat(-1.0), Vec3::splat(1.0));
    assert_eq!(explicit_streamline_seeds(&domain, 26, false, 3), explicit_streamline_seeds(&domain, 26, false, 3));
}

#[test]
fn streamline_count_defaults_to_auto() {
    assert_eq!(State::new(false).streamline_count, 0);
}

#[test]
fn streamline_count_clamps_and_marks_dirty() {
    let mut state = State::new(false);
    assert!(state.set_streamline_count(99_999));
    assert_eq!(state.streamline_count, MAX_STREAMLINES);
    state.snapshot_if_dirty();
    assert!(!state.set_streamline_count(MAX_STREAMLINES));
}
```

In `tests/persistence_test.rs`:

```rust
#[test]
fn streamline_count_round_trips_and_missing_field_defaults_to_auto() {
    let mut state = State::new(false);
    state.set_streamline_count(37);
    let record = ViewRecord::capture(&state);
    assert_eq!(record.streamline_count, 37);

    let mut restored = State::new(false);
    let mut controls = Controls::new();
    record.restore(&mut restored, &mut controls);
    assert_eq!(restored.streamline_count, 37);
    assert_eq!(controls.streamline_count, 37);

    let missing: ViewRecord = serde_json::from_str("{}").expect("all fields default");
    assert_eq!(missing.streamline_count, 0);
}
```

In `tests/sidebar_controls_test.rs`:

```rust
#[test]
fn streamline_count_control_is_auto_only_and_mirrors_the_state() {
    assert_eq!(streamline_count_label(0), "Auto");
    assert_eq!(streamline_count_label(37), "37");
    assert!(!streamline_count_enabled(false, StreamlineSeedMode::Auto));
    assert!(streamline_count_enabled(true, StreamlineSeedMode::Auto));
    assert!(!streamline_count_enabled(true, StreamlineSeedMode::Click));

    let mut state = State::new(false);
    let mut controls = Controls::new();
    controls.streamline_count = 37;
    apply(&mut state, &mut controls);
    assert_eq!(state.streamline_count, 37);
    assert_eq!(controls.streamline_count, 37);
}
```

- [ ] **Step 2: Run the tests and confirm RED**

Run `cargo test --test suite streamline_count_test:: -- --nocapture`, then the
persistence and sidebar filters. Expected: missing API/field/control errors.

- [ ] **Step 3: Implement the seed planner and state setter**

Add `explicit_streamline_seeds` using `seed_grid` and `take(count)`. Use
`n = (count as f32).cbrt().ceil() as u32` for 3D and `sqrt` for 2D, with
`n.max(1)`. `set_streamline_count` clamps to 0..=`MAX_STREAMLINES` and marks
dirty only on change. `streamlines_snapshot` uses Auto's density-derived axis
when count is 0, otherwise the explicit planner; trace the same scene flow and
local domain.

- [ ] **Step 4: Add the UI and persistence fields**

Add `streamline_count` to `Controls`, `Controls::new`, reset, `apply`, and the
mirror. Add a DragValue immediately after the Seed mode row:

```rust
ui.horizontal(|ui| {
    ui.label("Streamline count");
    ui.add(
        egui::DragValue::new(&mut controls.streamline_count)
            .range(0..=MAX_STREAMLINES)
            .speed(1.0)
            .custom_formatter(|v, _| streamline_count_label(v.max(0.0).round() as u32)),
    )
    .on_hover_text("0 = Auto (density-derived); a value seeds exactly that many lines.");
});
```

Enable it only through
`streamline_count_enabled(controls.streamlines, controls.seed_mode)`; disabled
hover explains that Click-only seeding uses click bursts. Add the persistence
field with `serde(default)` and capture/restore it. Do not add a CLI option.

- [ ] **Step 5: Run focused tests**

Run:
`cargo test --test suite streamline_count_test::`
`cargo test --test suite persistence_test::`
`cargo test --test suite sidebar_controls_test::`
`cargo test --test suite streamlines_test::`

Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add src/streamlines.rs src/viewmodel.rs src/ui/sidebar.rs src/persistence.rs tests/streamline_count_test.rs tests/persistence_test.rs tests/sidebar_controls_test.rs tests/suite.rs
git commit -m "feat: add streamline seed count control" -m "Generated-by: OpenCode"
```

---

### Task 6: Plane transitions, selector UI, and documentation

**Files:**
- Create: `src/app/plane.rs`
- Modify: `src/app/mod.rs`
- Modify: `src/app/simulation.rs`
- Modify: `src/app/input.rs`
- Modify: `src/app/panels.rs`
- Modify: `src/ui/sidebar.rs`
- Modify: `src/ui/help_overlay.rs`
- Modify: `README.md`
- Modify: `GLOSSARY.md`
- Modify: `docs/CODEBASE.md`
- Modify: `tests/plane_particles_test.rs`
- Modify: `tests/sidebar_controls_test.rs`
- Modify: `tests/responsive_test.rs`
- Modify: `tests/suite.rs` only for new modules

**Interfaces:**
- Produces `SceneFrameKey` in `src/app/plane.rs`:
  `World3D`, `World2D`, `Plane { axis: PlaneAxis, index: u32, plane_grid: UVec3, domain: Domain }`.
- Produces `VfApp::sync_scene_frame(&mut self)`; it compares the current key with
  the stored `scene_frame`, clears plane-related seeds when either side is a
  plane or the key changes, and fits only when entering a plane or changing
  axis/index.
- `VfApp` gains `scene_frame: SceneFrameKey` initialized in `new_with_storage`.
- Produces `pub enum PlaneControlMode { Hidden, SliceOnly, Full }` and
  `pub fn plane_control_mode(field_dimension: usize, mode_3d: bool) -> PlaneControlMode`
  in `src/ui/sidebar.rs`. `plane_controls` uses this helper: `Hidden` draws
  nothing; `SliceOnly` draws axis/index with no enable or opacity controls;
  `Full` keeps the existing 3D UI.
- `plane_controls` gains a 2D mode that shows axis/index but not the dim toggle;
  `domain_axes` gates on `field.dimension == 3`.

- [ ] **Step 1: Write failing transition/UI tests**

Extend `tests/plane_particles_test.rs`:

```rust
#[test]
fn plane_transition_clears_seeds_and_refills_particles() {
    let mut app = VfApp::new();
    app.state.set_formula("y, -x, 1".into());
    app.state.set_mode(true);
    app.state.set_streamlines(true);
    app.state.set_particles(true);
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(200.0, 200.0));
    app.test_harness().advance_simulation();
    app.test_harness().apply_click(
        LeftClickAction::SeedStreamline,
        rect.center(),
        rect,
        1.0,
        0.0,
    );
    assert_eq!(app.test_harness().click_seed_count(), 1);

    app.state.set_mode(false);
    app.test_harness().advance_simulation();
    assert_eq!(app.test_harness().click_seed_count(), 0);
    let positions = app.test_harness().particle_snapshot().0;
    assert!(!positions.is_empty());
    assert!(positions.iter().all(|p| p.z.abs() < 1e-5));
}

#[test]
fn plane_axis_change_clears_stale_click_seeds() {
    let mut app = VfApp::new();
    app.state.set_formula("y, -x, 1".into());
    app.state.set_mode(false);
    app.state.set_streamlines(true);
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(200.0, 200.0));
    app.test_harness().advance_simulation();
    app.test_harness().apply_click(
        LeftClickAction::SeedStreamline,
        rect.center(),
        rect,
        1.0,
        0.0,
    );
    assert_eq!(app.test_harness().click_seed_count(), 1);

    app.state.set_plane_axis(PlaneAxis::X);
    app.test_harness().advance_simulation();
    assert_eq!(app.test_harness().click_seed_count(), 0);
}

#[test]
fn world_mode_keeps_existing_click_seeds() {
    let mut app = VfApp::new();
    app.state.set_formula("y, -x, 1".into());
    app.state.set_mode(true);
    app.state.set_streamlines(true);
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(200.0, 200.0));
    app.test_harness().advance_simulation();
    app.test_harness().apply_click(
        LeftClickAction::SeedStreamline,
        rect.center(),
        rect,
        1.0,
        0.0,
    );
    assert_eq!(app.test_harness().click_seed_count(), 1);

    app.state.set_formula("x, -y, 2".into());
    app.test_harness().advance_simulation();
    assert_eq!(app.test_harness().click_seed_count(), 1);
}
```

Add camera assertions:

```rust
#[test]
fn plane_enter_fits_local_camera() {
    let mut app = VfApp::new();
    app.state.set_formula("x, y, z".into());
    app.state.set_domain(Vec3::new(10.0, -2.0, -3.0), Vec3::new(20.0, 2.0, 3.0));
    app.state.set_mode(true);
    app.test_harness().advance_simulation();
    app.state.set_mode(false);
    app.test_harness().advance_simulation();
    assert!(app.orbit.target.length() < 1e-5);
    assert!(app.orbit.ortho_zoom.is_finite() && app.orbit.ortho_zoom > 0.0);
}
```

In `tests/sidebar_controls_test.rs`:

```rust
#[test]
fn plane_selector_is_available_in_2d_and_preserves_the_dim_toggle() {
    assert_eq!(plane_control_mode(3, false), PlaneControlMode::SliceOnly);
    assert_eq!(plane_control_mode(3, true), PlaneControlMode::Full);
    assert_eq!(plane_control_mode(2, false), PlaneControlMode::Hidden);

    let mut state = State::new(false);
    state.set_formula("x, y, z".into());
    state.set_mode(true);
    state.set_plane_enabled(true);
    state.set_mode(false);
    state.set_plane_axis(PlaneAxis::X);
    assert!(state.plane.enabled);
}

#[test]
fn domain_axes_include_z_for_a_3d_field_in_2d() {
    assert!(domain_axes(true).contains(&DomainAxis::Z));
    assert!(!domain_axes(false).contains(&DomainAxis::Z));
}
```

- [ ] **Step 2: Run the tests and confirm RED**

Run `cargo test --test suite plane_particles_test::` and the sidebar filters.
Expected: no transition clearing/fit, and the 2D plane selector is absent.

- [ ] **Step 3: Implement the frame transition**

Add `src/app/plane.rs` with `SceneFrameKey` and `sync_scene_frame`. Call it at
the start of `VfApp::advance_simulation`, before `rebuild_scene` and
`advance_particles`, so the new frame's scene and seeds are built in the same
frame. When the previous or next key is a plane and keys differ:

- clear `click_seeds`, `last_click_seed`, the particle cloud and its target;
- clear `flow_lines`/`click_lines`, reset the segment counter, and mark dirty;
- for a plane entry or axis/index change, apply `ViewCommand::Fit` to the orbit
  camera using the local `view_context()`, exiting fly mode.

Do not clear clicks for a 3D or ordinary-2D key change, and do not fit on
density/domain edits within the same plane.

- [ ] **Step 4: Update the sidebar and help**

Call `plane_controls` for `field.dimension == 3` in both modes using
`state.plane_grid()`; enforce its visibility with
`plane_control_mode(state.field.dimension, state.mode_3d)`. In 2D, hide “Plane
by plane” and “Off-plane opacity”, show “Slice axis” and “Position”, and let
`State::set_plane` preserve the saved `enabled` value. Gate domain axes on
`field.dimension == 3`. Update the particle hover to explain that 2D particles
need a three-component field and now show the selected slice.

Add help/README/GLOSSARY wording for selected-plane slices, projected flow,
normal-component omission, local Fit, plane changes clearing seeds, and the
streamline-count control. Keep ordinary two-component 2D documentation.

- [ ] **Step 5: Run focused tests and formatting**

Run:
`cargo test --test suite plane_particles_test::`
`cargo test --test suite sidebar_controls_test::`
`cargo test --test suite responsive_test::`
`cargo test --test suite plane_scene_test::`
`cargo test --test suite scene_flow_test::`
`cargo test --test suite streamline_count_test::`

Expected: all pass. Then run `cargo fmt --check` on every changed `.rs` file
only (use `rustfmt --edition 2024 --check --config skip_children=true <files>`)
and `python3 scripts/check-docs.py`.

- [ ] **Step 6: Run the full gates**

From `/home/paul/.local/share/opencode/worktree/9d1d25/issue-13-design`:

```bash
cargo test --test suite
cargo build
cargo clippy --all-targets -- -D warnings
python3 scripts/check-docs.py
git diff --check
```

Expected: full suite passes, build/clippy zero warnings, docs and diff checks
exit 0. Only the known nom 1.2.4 future-incompat warning may remain.

- [ ] **Step 7: Commit**

```bash
git add src/app/plane.rs src/app/mod.rs src/app/simulation.rs src/app/input.rs src/app/panels.rs src/ui/sidebar.rs src/ui/help_overlay.rs README.md GLOSSARY.md docs/CODEBASE.md tests/plane_particles_test.rs tests/sidebar_controls_test.rs tests/responsive_test.rs tests/suite.rs
git commit -m "feat: complete plane slicing integration" -m "Generated-by: OpenCode"
```

---

## Self-Review

- **Spec coverage:** Tasks 1–2 implement plane frames, plane grid, projected
  sampling and local camera/click/reference behavior; Task 3 implements
  projected streamlines; Task 4 re-enables plane particles and clock
  consumption; Task 5 implements the requested streamline-count DragValue and
  persistence; Task 6 covers transitions, selector/domain UI, docs and gates.
  `PlaneControlMode` and `streamline_count_enabled` are part of Task 5/6
  interfaces, so the tests and implementation steps name the same seams.
- **Placeholder scan:** Every test body is concrete executable Rust, including
  the transition, camera, reference, sampling, streamline, particle, count and
  UI-helper cases. The only intentionally open details are the body of
  `PlaneFrame` arithmetic, which follows the exact basis table in the design,
  and UI styling consistent with existing sidebar rows.
- **Type consistency:** `PlaneFrame`, `SceneFlow`, `scene_domain`,
  `scene_flow`, `streamline_count`, `SceneFrameKey`, `PlaneControlMode`,
  `streamline_count_enabled`, and `compose_lines_with_frame` are used
  consistently across tasks.
- **Review Focus coverage:** Each listed failure mode has a named test in its
  owning task; review should reject changes that bypass `scene_domain`/
  `scene_flow` in app camera, click, trace or particle paths, or that reintroduce
  a 3D-only particle gate.
- **Proportion:** The plan describes six independently testable slices; code
  bodies are left to the implementer except for exact basis and API shapes.
