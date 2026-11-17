# ODE Workbench Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a bounded, deterministic ODE teaching workbench that integrates typed equations or the durable formula field with fixed-step RK4 and plays the same precomputed trajectories, phase portrait, and time series through the shared clock and existing paint seam.

**Architecture:** A new pure `src/ode.rs` owns scene validation, bounded RK4 integration, trajectory samples, bounds, and marker interpolation. `FieldMode::Ode` adds a vector-view concept adapter whose durable scene/solution live in `State`, whose app branch paints trajectory lines plus current-time markers through `SceneGPU`, and whose panel edits only drafts. Persistence and export reuse the existing request-time provenance seams.

**Tech Stack:** Rust 2024, eframe/egui, glam f32/f64, existing meval `FieldExpr`, existing `SimClock`, existing `SceneGPU` line/dot pipelines, serde persistence, `png`/SVG exporters.

**Spec:** `docs/superpowers/specs/2026-11-17-ode-workbench-design.md`

## Global Constraints

- Pure CPU model/solver; native + wasm; no new dependencies, GPU solver, private timer, or CLI extension.
- `ODE_STEP = 0.005`, `ODE_MAX_SAMPLES = 4096` stored samples per trajectory including `t=0`; duration `0.1..=25.0` (default `20.0`); at most eight seeds; typed dimensions `2..=3`.
- Fixed-step RK4 only. No adaptive/stiff chooser, symbolic solve/integrate, Poincare/Lyapunov/bifurcation, parameter sweep, or 3D phase cube.
- One shared `SimClock`; no second clock, loop period in Ode, or per-frame solve during drag.
- Existing particle RK4/advection is unchanged. Ode has no field sampling, particles, streamlines, click seeding, source glyphs, or scalar plot.
- E/B/source/induction behavior is untouched. CAS remains native-only and disabled outside Formula.
- Invalid/nonfinite work is explicit and bounded; never draw NaN/Inf or fabricate connecting lines.
- Tests belong in `tests/` and register in `tests/suite.rs`; no inline `#[cfg(test)]` under `src/`.
- Browser/manual visual gates are owner-waived, not passed. Only the known `nom 1.2.4` future-incompat warning is expected.

## Review Focus

1. **Broken formula draft in CurrentField mode:** the last valid trajectory must remain painted, while `ode_error` explains the parse failure and no stale field is re-solved.
2. **Step/duration cap:** duration `25.0` must stop with `StepLimit` at `ODE_MAX_SAMPLES`, not truncate silently or allocate unbounded storage.
3. **Mode/clock lifecycle:** entering, leaving, resetting, and restoring Ode must not leak a loop period, restart a private timer, or replay stale trajectories.
4. **Current-field domain vs presentation bounds:** integration seeds and evaluation must use the durable field domain, never the padded trajectory bounding box.
5. **Chart selection:** all time-series variables off must be recoverable and must not paint ambiguous/empty charts without explanation.
6. **Existing particle behavior:** explicit regression proof that particle RK4/advection and its clock path are unchanged.

---

### Task 1: Bounded ODE model, RK4 solver and evidence

**Files:**
- Create: `src/ode.rs`
- Modify: `src/field.rs`, `src/lib.rs`
- Create: `tests/ode_test.rs`, `tests/ode_bench_test.rs`
- Modify: `tests/suite.rs`, `docs/CODEBASE.md`
- Create: `scripts/bench-ode.sh`
- Evidence: `docs/perf/ode-performance.md` (Task 5 finalizes; Task 1 records measurements there)
- Report/evidence: `.superpowers/sdd/2026-11-17-ode-workbench/task-1-report.md`, `evidence/task-1/`

**Interfaces:**
- Consumes: `FieldExpr`, `Domain`, `AXIS_*_COLOR`.
- Produces: public `OdeSource`, `OdeSeed`, `OdeScene`, `OdeStop`, `OdeSample`, `OdeTrajectory`, `OdeBounds`, `OdeSolution`, `OdeError`, `OdePrepared`, `OdeChart`, `ODE_STEP`, `ODE_MAX_SAMPLES`, `ODE_MAX_SEEDS`, `ODE_MAX_DURATION`, `ODE_DEFAULT_DURATION`, `ODE_COLORS`; `prepare_ode(&State, &OdeScene)`, `integrate_ode`, `solve_ode`, hidden `integrate_ode_with_step`; `OdeSolution::{markers_at,view_domain,sample_at}`; `FieldExpr::eval3_at_f64`.

- [ ] **Step 1: Write failing solver/model tests**

Create `tests/ode_test.rs` with these imports and helper:

```rust
use vector_field_explorer::ode::{
    integrate_ode_with_step, prepare_ode, solve_ode, OdeChart, OdeError, OdeScene, OdeSeed,
    OdeSolution, OdeSource, OdeStop, ODE_COLORS, ODE_MAX_DURATION, ODE_MAX_SAMPLES,
};
use vector_field_explorer::viewmodel::State;
```

```rust
fn state_with_formula(formula: &str, dimension: usize) -> State {
    let mut state = State::new(true);
    state.set_formula(formula);
    assert_eq!(state.field.dimension, dimension);
    state
}

fn typed(equations: [&str; 3], dimension: usize) -> OdeScene {
    OdeScene {
        source: OdeSource::Equations,
        dimension,
        equations: [
            equations[0].to_owned(),
            equations[1].to_owned(),
            equations[2].to_owned(),
        ],
        seeds: vec![OdeSeed { position: [1.0, 0.0, 0.0] }],
        duration: 2.0,
    }
}
```

Tests:

```rust
#[test]
fn harmonic_oscillator_matches_the_analytic_circle() {
    let mut state = State::new(true);
    let scene = typed(["-y", "x", "0"], 2);
    let solution = solve_ode(&state, &scene).expect("valid harmonic scene");
    let end = solution.sample_at(0, 2.0).expect("sample at duration");
    assert!((end.time - 2.0).abs() < 1e-12);
    assert!((end.position[0] - 2.0f64.cos()).abs() <= 1e-5);
    assert!((end.position[1] - 2.0f64.sin()).abs() <= 1e-5);
    assert_eq!(solution.trajectories[0].stop, OdeStop::Complete);
    assert_eq!(solution.stopped, 0);
}

#[test]
fn damped_oscillator_decays_like_the_reference() {
    let state = State::new(true);
    let scene = typed(["-y", "x - 0.2 * y", "0"], 2);
    let solution = solve_ode(&state, &scene).expect("valid damped scene");
    let end = solution.sample_at(0, 2.0).expect("sample at duration");
    let reference = (-0.2 * 2.0).exp() * 2.0f64.cos();
    assert!((end.position[0] - reference).abs() <= 1e-5);
    assert!((end.position[1] + (-0.2 * 2.0).exp() * 2.0f64.sin()).abs() <= 1e-5);
}

#[test]
fn rk4_halving_the_step_reduces_error_by_at_least_eight() {
    let state = State::new(true);
    let scene = typed(["-y", "x", "0"], 2);
    let prepared = prepare_ode(&state, &scene).expect("prepared");
    let coarse = integrate_ode_with_step(&prepared, 0.01, ODE_MAX_SAMPLES);
    let fine = integrate_ode_with_step(&prepared, 0.005, ODE_MAX_SAMPLES);
    let analytic = |t: f64| [2.0f64.cos(), 2.0f64.sin()];
    let error = |solution: &OdeSolution| {
        let point = solution.sample_at(0, 2.0).expect("point").position;
        (point[0] - analytic(2.0)[0]).abs() + (point[1] - analytic(2.0)[1]).abs()
    };
    assert!(error(&fine) * 8.0 <= error(&coarse));
}

#[test]
fn step_limit_is_explicit_and_bounded() {
    let state = State::new(true);
    let mut scene = typed(["-y", "x", "0"], 2);
    scene.duration = ODE_MAX_DURATION;
    let solution = solve_ode(&state, &scene).expect("max duration is legal");
    assert_eq!(solution.trajectories[0].points.len(), ODE_MAX_SAMPLES);
    assert_eq!(solution.trajectories[0].stop, OdeStop::StepLimit);
    assert_eq!(solution.stopped, 1);
}

#[test]
fn nonfinite_evolution_stops_without_fake_points() {
    let state = State::new(true);
    let scene = typed(["1", "1 / (x - 1)", "0"], 2);
    let solution = solve_ode(&state, &scene).expect("valid equations");
    assert_eq!(solution.trajectories[0].stop, OdeStop::NonFinite);
    assert!(solution.trajectories[0].points.len() < ODE_MAX_SAMPLES);
    assert!(solution.trajectories[0].points.iter().all(|s| s.position[0].is_finite()));
}

#[test]
fn current_field_and_typed_rhs_agree() {
    let state = state_with_formula("y, -x, 0", 3);
    let typed_scene = typed(["-y", "x", "0"], 2);
    let mut current = typed_scene.clone();
    current.source = OdeSource::CurrentField;
    current.dimension = 3;
    let typed_solution = solve_ode(&state, &typed_scene).expect("typed");
    let current_solution = solve_ode(&state, &current).expect("current field");
    let typed_end = typed_solution.sample_at(0, 2.0).expect("typed end");
    let current_end = current_solution.sample_at(0, 2.0).expect("current end");
    assert!((typed_end.position[0] - current_end.position[0]).abs() <= 1e-12);
    assert!((typed_end.position[1] - current_end.position[1]).abs() <= 1e-12);
}

#[test]
fn invalid_scene_and_nonfinite_seed_are_rejected() {
    let state = State::new(true);
    let mut scene = typed(["x", "y", "0"], 2);
    scene.seeds.clear();
    assert!(matches!(solve_ode(&state, &scene), Err(OdeError::EmptySeeds)));
    scene = typed(["x", "y", "0"], 2);
    scene.seeds[0].position[0] = f64::NAN;
    assert!(matches!(solve_ode(&state, &scene), Err(OdeError::NonFiniteSeed(0))));
    let bad = typed(["x +", "y", "0"], 2);
    assert!(matches!(solve_ode(&state, &bad), Err(OdeError::Parse { .. })));
}

#[test]
fn markers_clamp_and_interpolate_between_samples() {
    let state = State::new(true);
    let scene = typed(["-y", "x", "0"], 2);
    let solution = solve_ode(&state, &scene).expect("solution");
    let start = solution.markers_at(0.0);
    let end = solution.markers_at(scene.duration);
    assert!((start[0][0] - 1.0).abs() <= 1e-12);
    assert!((end[0][0] - 2.0f64.cos()).abs() <= 1e-4);
    let clamped = solution.markers_at(-10.0);
    assert_eq!(clamped, solution.markers_at(0.0));
    assert!(solution.view_domain().domain_size().z >= 1e-3);
}

#[test]
fn time_series_defaults_and_chart_variants_are_total() {
    assert_eq!(OdeChart::default(), OdeChart::Phase);
    assert_eq!(OdeScene::default().seeds, vec![OdeSeed { position: [1.0, 0.0, 0.0] }]);
    assert_eq!(OdeScene::default().equations[0], "y");
    assert_eq!(ODE_COLORS.len(), 8);
}
```

- [ ] **Step 2: Run and confirm RED**

Run: `cargo test --test suite ode_test::`
Expected: compile failure because `src/ode.rs` and `FieldExpr::eval3_at_f64` do not exist.

- [ ] **Step 3: Implement the pure model and RK4**

Implement exactly the public items in the design. Additional deterministic rules:

- `OdeScene::validate` rejects empty seeds, >8 seeds, dimension outside 2..=3, non-finite seed/time/duration, non-positive duration, and more than `ODE_MAX_DURATION`. Typed equations require the first `dimension` entries to parse as `FieldExpr::parse` three-component source with unused components `"0"`; use `OdeError::Parse { component, message }`.
- `prepare_ode` snapshots the RHS:
  - `Equations`: take the first `dimension` entries, append `"0"` for unused components, then `FieldExpr::parse` the resulting three-component source; entries outside `dimension` are ignored. Require the parsed result to have dimension 3.
  - `CurrentField`: use `state.field.clone()` and `state.domain`; require `state.field.dimension == scene.dimension` (the UI keeps these in sync) and copy `state.parse_error` into `OdeError::UnsupportedField` when non-empty.
- `integrate_ode_with_step` samples `t = 0, h, 2h, ...` while `t <= duration + 1e-12`; stop at `max_steps`; each seed independently stops on nonfinite sample or RHS. Set `stopped` to the number of non-`Complete` trajectories.
- Bounds use only finite points, pad every axis by 5% of the largest overall extent (`max(max.x-min.x, max.y-min.y, max.z-min.z)`) with a `1e-3` minimum. Reject with `InvalidScene` if no finite points exist. `view_domain` gives a symmetric `z` half-extent of `max(1e-3, 0.05 * max(x,y) extent)` when z is flat.
- `OdeSolution::sample_at` uses linear interpolation between bracketing samples and clamps to `[0,duration]`; `markers_at` returns one interpolated point per trajectory.
- `FieldExpr::eval3_at_f64(DVec3, f64) -> Option<Vec3>` evaluates in f64 and casts the final components after finite checking; `eval3_at(Vec3,f32)` delegates.
- Export `ode_trajectory_lines(solution)` and `ode_axes(view_domain)` as pure helpers in this task (used by Task 2): one `LineInstanceGPU`-shaped tuple helper is acceptable, but keep `src/ode.rs` independent of `src/renderer.rs` by returning `(start:[f32;3], end:[f32;3], color:[f32;4], seed_index)`-style data; Task 2 converts to renderer instances.
  - Trajectory lines use `ODE_COLORS[seed % 8]`, omit repeating nonfinite points, and use at least two points.
  - Axes are `x`, `y`, and (for 3D) `z` from negative to positive padded extent through the origin; colors are `AXIS_X_COLOR`/`AXIS_Y_COLOR`/`AXIS_Z_COLOR`.

- [ ] **Step 4: Add the benchmark surface**

Create `scripts/bench-ode.sh` exporting `VFE_ODE_BENCH=1` and running the exact integration test. Add `tests/ode_bench_test.rs`:

```rust
#[test]
fn ode_benchmark_when_requested() {
    if std::env::var_os("VFE_ODE_BENCH").is_none() {
        return;
    }
    let state = State::new(true);
    let mut scene = OdeScene::default();
    scene.seeds = (0..8).map(|i| OdeSeed { position: [1.0 + i as f64 * 0.1, 0.0, 0.0] }).collect();
    let start = std::time::Instant::now();
    let solution = solve_ode(&state, &scene).expect("benchmark scene");
    let elapsed = start.elapsed();
    eprintln!("ode-bench seeds={} samples={} elapsed_ms={:.3}", solution.trajectories.len(), solution.trajectories.iter().map(|t| t.points.len()).sum::<usize>(), elapsed.as_secs_f64() * 1e3);
    assert!(solution.stopped == 0 || solution.trajectories.iter().any(|t| t.stop == OdeStop::StepLimit));
}
```

Record the default solve and cap solve. If a solve exceeds 2 s, lower `ODE_MAX_SAMPLES`/duration only with an update to both design and plan.

- [ ] **Step 5: Run focused tests and commit**

Run:

```bash
cargo test --test suite ode_test::
cargo test --test suite field_test::
VFE_ODE_BENCH=1 cargo test --test suite ode_bench_test:: -- --nocapture
```

Expected: all pass; benchmark prints host numbers. Then:

```bash
git add src/ode.rs src/field.rs src/lib.rs tests/ode_test.rs tests/ode_bench_test.rs tests/suite.rs docs/CODEBASE.md scripts/bench-ode.sh
git commit -m "feat: add bounded ODE trajectories" -m "Generated-by: OpenCode"
```

---

### Task 2: `FieldMode::Ode`, state, app and shared clock

**Files:**
- Modify: `src/source_scene.rs`, `src/viewmodel.rs`, `src/viewmodel/plot.rs`
- Modify: `src/sim_clock.rs`
- Modify: `src/app/mod.rs`, `src/app/simulation.rs`, `src/app/input.rs`, `src/app/plot.rs`, `src/app/panels.rs`, `src/app/viewport.rs`
- Create: `tests/ode_app_test.rs`
- Modify: `tests/suite.rs`, `docs/CODEBASE.md`

**Interfaces:**
- Consumes: Task 1 `OdeScene`, `OdeSolution`, `OdeChart`, `solve_ode`, `ode_trajectory_lines`, `ode_axes`, `ODE_COLORS`, `ODE_STEP`.
- Produces: `FieldMode::Ode`; `State::{ode,ode_solution,ode_error,ode_epoch,ode_chart,ode_variables,set_ode_scene,refresh_ode_if_dirty,reset_ode,set_ode_chart,set_ode_variables}`; `SimClock::{clamp_time,seek}`; app ODE branch and harness seams `ode_line_count`, `ode_marker_count`, `ode_step`, `ode_epoch`.

- [ ] **Step 1: Write failing state/app tests**

Create `tests/ode_app_test.rs`. Use `VfApp`, `State`, `OdeScene`, `OdeSource`, `FieldMode`, `OdeStop`, `ODE_STEP`, and existing `support::context` only where a frame is required.

```rust
use vector_field_explorer::VfApp;
use vector_field_explorer::ode::{OdeScene, OdeSeed, OdeSource, OdeStop, ODE_STEP};
use vector_field_explorer::plot::PlotKind;
use vector_field_explorer::source_scene::FieldMode;
```

```rust
#[test]
fn entering_ode_keeps_the_projection_and_uses_a_lazy_solution() {
    let mut app = VfApp::new();
    assert!(!app.state.mode_3d);
    let before = app.state.mode_3d;
    app.state.set_field_mode(FieldMode::Ode);
    assert_eq!(app.state.mode_3d, before);
    assert_eq!(app.state.ode_solution, None);
    app.test_harness().advance_simulation();
    assert!(app.state.ode_solution.is_some());
    assert!(app.state.is_vector_mode());
    assert_eq!(app.state.plot.kind, PlotKind::VectorField);
}

#[test]
fn typed_solution_uses_the_active_dimension_and_clears_loop_period() {
    let mut app = VfApp::new();
    app.clock.loop_period = Some(3.0);
    let scene = OdeScene { source: OdeSource::Equations, dimension: 3, equations: ["-y".into(), "x".into(), "0".into()], seeds: vec![OdeSeed { position: [1.0, 0.0, 0.0] }], duration: 2.0 };
    app.state.set_ode_scene(scene).expect("valid");
    app.state.set_field_mode(FieldMode::Ode);
    assert_eq!(app.state.active_dimension(), 3);
    app.test_harness().advance_simulation();
    assert_eq!(app.clock.loop_period, None);
    assert_eq!(app.state.ode_solution.as_ref().unwrap().dimension, 3);
}

#[test]
fn clock_playback_and_step_share_the_solution_and_stop_at_duration() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Ode);
    app.test_harness().advance_simulation();
    let duration = app.state.ode_solution.as_ref().unwrap().duration;
    app.test_harness().ode_step();
    assert!((app.clock.time - ODE_STEP).abs() < 1e-9);
    assert!(!app.clock.playing);
    app.clock.set_playing(true);
    app.test_harness().advance_clock(duration + 1.0);
    assert!((app.clock.time - duration).abs() < 1e-9);
    assert!(!app.clock.playing);
    assert!((app.state.time - duration).abs() < 1e-6);
}

#[test]
fn broken_current_field_draft_freezes_the_last_solution() {
    let mut app = VfApp::new();
    app.state.set_formula("y, -x, 0");
    app.state.set_field_mode(FieldMode::Ode);
    app.state.ode.source = OdeSource::CurrentField;
    app.state.set_ode_scene(app.state.ode.clone()).expect("first solution");
    app.test_harness().advance_simulation();
    let before = app.state.ode_solution.clone();
    app.state.set_formula("y, -x +");
    app.test_harness().advance_simulation();
    assert!(app.state.ode_error.is_some());
    assert_eq!(app.state.ode_solution.as_ref().unwrap().bounds, before.unwrap().bounds);
}

#[test]
fn ode_branch_has_lines_and_markers_but_no_particles_or_glyphs() {
    let mut app = VfApp::new();
    assert!(!app.state.mode_3d, "fresh app is 2D");
    app.state.set_field_mode(FieldMode::Ode);
    app.test_harness().advance_simulation();
    let scene = app.test_harness().test_assemble_viewport_scene();
    assert!(!scene.lines.is_empty());
    assert_eq!(app.test_harness().ode_line_count(), scene.lines.len());
    assert!(app.test_harness().ode_marker_count() > 0);
    assert_eq!(scene.dots.len(), app.test_harness().ode_marker_count());
    assert!(scene.arrows.is_empty());
    assert!(scene.combined.is_empty());
    app.state.set_mode(true);
    app.test_harness().advance_simulation();
    let scene = app.test_harness().test_assemble_viewport_scene();
    assert!(!scene.combined.is_empty());
    assert!(scene.combined.iter().any(|instance| instance.origin[3] == 0.0));
}

#[test]
fn leaving_and_resetting_ode_preserves_formula_and_clears_solution() {
    let mut app = VfApp::new();
    app.state.set_formula("y, -x, 0");
    app.state.set_ode_scene(OdeScene::default()).expect("ode");
    app.state.set_field_mode(FieldMode::Ode);
    app.state.reset_ode();
    app.state.set_field_mode(FieldMode::Formula);
    assert_eq!(app.state.formula, "y, -x, 0");
    assert!(app.state.ode_solution.is_none());
    app.clock.reset();
    assert_eq!(app.clock.time, 0.0);
}
```

- [ ] **Step 2: Run and confirm RED**

Run: `cargo test --test suite ode_app_test::`
Expected: compile failures for `FieldMode::Ode`, ODE State APIs, `SimClock` methods and harness seams.

- [ ] **Step 3: Implement state and mode integration**

- Add `Ode` to `FieldMode`; add an explicit `Ode => 3`/dimension arm to every exhaustive match in app/UI/persistence/export, keeping Sources/Induction semantics.
- Add State fields exactly as design, initialized `OdeScene::default()`, `ode_solution: None`, `ode_error: None`, `ode_epoch: 0`, `ode_chart: OdeChart::Phase`, `ode_variables: [true, true, false]`, `ode_dirty: false`.
- `active_dimension`: `Ode => ode.dimension`; `is_plane_slice`: false in Ode; `plane_frame`: None; `scene_domain`: solution `view_domain()` or durable `domain`; `particle_target`: None; `scene_flow`: leave unreachable/unused.
- `set_field_mode(Ode)` preserves `mode_3d`, formula/source/induction durable scenes and camera, sets vector plot kind, clears loop period, clears source/induction caches/notices, marks dirty, and does **not** solve synchronously. If restored from persistence, `ode_dirty` is set and the first Ode frame solves.
- `set_ode_scene(scene)` validates via `solve_ode`/`prepare_ode`, stores scene and `Arc` solution, clears `ode_error`, increments `ode_epoch` only on a new solved scene, marks dirty, and clears `ode_dirty`. On failure, retain the prior scene/solution and set `ode_error`.
- `refresh_ode_if_dirty` solves only when Ode and `ode_dirty`; for `CurrentField` require `state.parse_error.is_none()` and a parsed field; if invalid set `ode_error` and clear the flag without replacing the last solution. Typed scenes are not invalidated by field/color edits.
- `reset_ode` restores default scene and solution, clears error, bumps epoch, clears loop period, marks dirty. Global reset calls it, then selects Formula.
- Add `set_ode_chart`/`set_ode_variables` narrow setters used by the panel/persistence; variables keep at least one true (reject all-false or restore `[true,true,false]` and surface a panel message).
- `SimClock::clamp_time(max)` sets time `min(time,max)` and pauses once when clamped; `seek(time)` clamps nonnegative, sets time, clears step, pauses. `set_time` gains `FieldMode::Ode => {}`.

- [ ] **Step 4: Implement the app branch and markers**

- Add app fields `ode_lines: Arc<Vec<LineInstanceGPU>>`, `last_ode_epoch: Option<u64>`, and marker helpers.
- In `advance_simulation`, branch `FieldMode::Ode` before scalar/vector: call `refresh_ode_if_dirty`; if epoch changed, reset clock time to 0 preserving playing/speed and clear loop; rebuild `ode_lines` from the current solution plus axes; set `scene.lines` and empty arrows/trails/dots/combined; do not sample, retrace clicks, spawn/advect particles, or append source/induction glyphs.
- In `advance_clock`, after `advance_for`, clamp to solution duration with `clock.clamp_time(duration)` when Ode, then mirror via `state.set_time(clock.time)`.
- In `frame_simulation`/`app/panels`, call `ode_step` action: `clock.seek((clock.time + ODE_STEP).min(duration))`, pause, mirror state time; no solve.
- In `assemble_viewport_scene`, when Ode and a solution exists, append current-time marker dots to 2D `dots` and to 3D `combined` after the cached instances, then sort 3D back-to-front. Markers use `ARROW_KIND`? No: use `PARTICLE_KIND` (`0.0`) with radius `0.012 * max(extent,1e-6)` and `ODE_COLORS[seed % 8]`. Do not write markers back into the cached vector scene.
- First successful Ode solution after mode entry auto-fits `scene_domain()` once; subsequent epochs preserve the camera. `Fit` uses existing `ViewCommand`; `Reset ODE section` resets the scene and camera.
- Clear the cached ODE lines when leaving Ode or on reset; `sync_field_mode` already clears transient particle/click geometry.
- Add doc-hidden harness methods `ode_step`, `ode_line_count`, `ode_marker_count`, `ode_epoch`.

- [ ] **Step 5: Run focused regressions and commit**

Run:

```bash
cargo test --test suite ode_app_test::
cargo test --test suite particle_playback_test::
cargo test --test suite plot_app_test::
cargo test --test suite source_mode_test::
cargo test --test suite plane_particles_test::
```

Expected: ODE tests pass; particle/plot/source/plane regressions unchanged. Then:

```bash
git add src/source_scene.rs src/viewmodel.rs src/viewmodel/plot.rs src/sim_clock.rs src/app tests/ode_app_test.rs tests/suite.rs docs/CODEBASE.md
git commit -m "feat: play ODE trajectories on the shared clock" -m "Generated-by: OpenCode"
```

---

### Task 3: Responsive ODE panel, charts and help

**Files:**
- Create: `src/ui/ode_panel.rs`, `tests/ode_panel_test.rs`
- Modify: `src/ui/mod.rs`, `src/ui/formula_bar.rs`, `src/ui/layout.rs`, `src/ui/sidebar.rs`, `src/ui/cas_panel.rs`, `src/ui/help_overlay.rs`
- Modify: `src/app/panels.rs`, `tests/suite.rs`, `docs/CODEBASE.md`

**Interfaces:**
- Consumes: Task 2 `State::ode`, `ode_error`, `ode_solution`, `ode_chart`, `ode_variables`, setters; `OdeScene`, `OdeChart`; existing layout/rail/sheet patterns from Induction and Sources.
- Produces: `OdePanelAction::{Step,Fit,Reset}`, `OdePanelState { draft,error,advanced }`, `show(ui,&mut OdePanelState,&mut State,compact) -> Option<OdePanelAction>`, `ode_help_rows()`; layout `DEFAULT_ODE_WIDTH=300`, `LayoutState::ode_expanded/ode_width/toggle_ode`, `SheetState::Ode`; formula-bar fourth selector.

- [ ] **Step 1: Write failing real-widget tests**

Create `tests/ode_panel_test.rs`. Copy the accesskit `WidgetNode`/`nodes`/`find`/`has`/`pointer`/`context`/`run_frame` helpers from `tests/induction_panel_test.rs` and keep them private to the test module.

```rust
use vector_field_explorer::ode::OdeChart;
use vector_field_explorer::source_scene::FieldMode;
use vector_field_explorer::ui::ode_panel::{OdePanelState, ode_help_rows};
use vector_field_explorer::viewmodel::State;
```

```rust
#[test]
fn ode_panel_exposes_equations_seeds_and_actions() {
    let ctx = context();
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Ode);
    let mut panel = OdePanelState::default();
    let out = run_frame(&ctx, &mut state, &mut panel, 1200.0, 900.0);
    assert!(has(&out, "ODE"));
    assert!(has(&out, "Equations"));
    assert!(has(&out, "Initial conditions"));
    assert!(has(&out, "Step"));
    assert!(has(&out, "Fit"));
    assert!(has(&out, "Reset ODE section"));
}
```

Add concrete tests for:
- `ode_equation_edit_commits_valid_scene_only`: type a bad second equation, click Add or lose focus; `panel.error` is set, `state.ode` unchanged; fix it, commit, state changes and error clears.
- `ode_initial_conditions_are_bounded_and_distinct`: add/remove up to 8, attempt 9, and assert the 9th is absent/disabled.
- `time_series_variables_reject_all_false`: toggle all off, assert at least one remains true and a visible `Select at least one variable.` message appears.
- `ode_compact_and_wide_panels_reflow`: run at 1200 and 390 widths; required labels are within the relevant panel/rail and no row exceeds the available width.
- `ode_chart_switch_commits_state`: click `Time series`, assert `state.ode_chart == OdeChart::TimeSeries`; click `Phase portrait`, assert back.
- `ode_mode_disables_features_and_cas`: assert the Features controls are disabled and CAS Apply is disabled with `CAS edits the formula field; switch to Formula.`.
- `open_ode_reaches_the_wide_rail_or_compact_sheet`: click `Open ODE` at both widths; `LayoutState`/`SheetState` reflects it.
- `ode_help_rows_state_the_model_limits`: `ode_help_rows()` contains rows mentioning `fixed-step RK4`, `no private timer`, `bounded`, `phase portrait` and `current formula field`.

- [ ] **Step 2: Run and confirm RED**

Run: `cargo test --test suite ode_panel_test::`
Expected: compile failure for missing panel/layout/selector/help APIs.

- [ ] **Step 3: Implement the panel and layout**

- `OdePanelState::commit` validates a draft by calling `state.set_ode_scene`; only on success syncs the draft back; on failure retains draft and sets `panel.error` from `state.ode_error`.
- Draw source selectors, dimension, equation rows only for 2D/3D, seed rows with add/remove/reset, duration `DragValue` `0.1..=25.0`, chart selector, variable checkboxes, Step/Fit/Reset, and closed Advanced solver/status.
- Every edit commits only on text focus loss/Enter, DragValue release, add/remove/reset, or chart/checkbox click; never per-frame during drag. Seed edits are bounded to 8 and finite.
- CPU charts paint from `state.ode_solution`:
  - Phase portrait: default x/y for 2D and x/y projection for 3D; seed colors from `ODE_COLORS`; current-time marker from `markers_at(state.time)`.
  - Time series: `t` horizontal; selected variables only; no interpolation beyond the shared solution samples.
  - Empty/error solution paints a short explanation and no fake lines.
- Layout: `DEFAULT_ODE_WIDTH=300.0`; serde-default `ode_expanded=false`, `ode_width`, `toggle_ode`, sanitized to the same bounds as other rails; `SheetState::Ode`; ODE rail/panel only in Ode; `Open ODE` opens wide rail or compact sheet.
- Formula bar adds a fourth selector `ODE`; hide the editor in Ode and show `ODE scene summary` plus a detail line and `Open ODE`.
- Features controls disabled with a note; Coloring hidden in Ode; CAS disabled outside Formula with the existing text. Do not alter existing formula/source/induction behavior.

- [ ] **Step 4: Add help and run focused UI regressions**

Add `INDUCTION_HELP`-style constants/functions for `ode_help_rows` and render them in the Help window. Ensure the rows include the exact concepts above.

Run:

```bash
cargo test --test suite ode_panel_test::
cargo test --test suite button_sizing_test::
cargo test --test suite responsive_test::
cargo test --test suite layout_test::
cargo test --test suite induction_panel_test::
cargo test --test suite sources_panel_test::
cargo test --test suite cas_panel_test::
cargo test --test suite help_overlay_test::
```

Expected: all pass; wide/compact ODE labels are accessible and existing panels are unchanged.

- [ ] **Step 5: Commit**

```bash
git add src/ui/ode_panel.rs src/ui/mod.rs src/ui/formula_bar.rs src/ui/layout.rs src/ui/sidebar.rs src/ui/cas_panel.rs src/ui/help_overlay.rs src/app/panels.rs tests/ode_panel_test.rs tests/suite.rs docs/CODEBASE.md
git commit -m "feat: add the responsive ODE workbench panel" -m "Generated-by: OpenCode"
```

---

### Task 4: ODE persistence, reset and export provenance

**Files:**
- Modify: `src/persistence.rs`, `src/viewmodel.rs`, `src/app/input.rs`, `src/export_provenance.rs`, `src/export.rs`, `src/screenshot.rs`, `src/app/plot.rs`, `src/app/viewport.rs`
- Create: `tests/ode_persistence_test.rs`, `tests/ode_export_test.rs`
- Modify: `tests/suite.rs`, `tests/persistence_test.rs`, `tests/source_export_test.rs`, `docs/CODEBASE.md`

**Interfaces:**
- Consumes: Task 2 State ODE fields; Task 3 panel/setters; existing `ViewRecord`, `ExportProvenance`, request-time PNG/SVG routing, decoded image helpers, `SimClock`.
- Produces: `ViewRecord` ODE fields with serde defaults; `OdeChart` mapping; `ode_export_provenance(mode, solution, timeline)`; `ExportProvenance.ode_mode`; PNG/SVG warning and metadata; reset clears ODE state.

- [ ] **Step 1: Write failing persistence/export tests**

Create `tests/ode_persistence_test.rs`:

```rust
use vector_field_explorer::ode::{OdeChart, OdeScene, OdeSeed, OdeSource};
use vector_field_explorer::persistence::ViewRecord;
use vector_field_explorer::source_scene::FieldMode;
use vector_field_explorer::ui::sidebar::Controls;
use vector_field_explorer::viewmodel::State;
```

```rust
#[test]
fn ode_scene_round_trips_all_fields() {
    let mut state = State::new(true);
    state.ode = OdeScene { source: OdeSource::Equations, dimension: 3, equations: ["-0.5 * y".into(), "2.0 * x".into(), "-0.1 * z".into()], seeds: vec![OdeSeed { position: [1.5, -0.25, 0.75] }, OdeSeed { position: [0.0, 1.0, -1.0] }], duration: 7.5 };
    state.ode_chart = OdeChart::TimeSeries;
    state.ode_variables = [true, false, true];
    state.field_mode = FieldMode::Ode;
    let record = ViewRecord::capture(&state);
    let mut restored = State::new(false);
    let mut controls = Controls::new();
    record.restore(&mut restored, &mut controls);
    assert_eq!(restored.ode, state.ode);
    assert_eq!(restored.field_mode, FieldMode::Ode);
    assert_eq!(restored.ode_chart, OdeChart::TimeSeries);
    assert_eq!(restored.ode_variables, [true, false, true]);
    assert!(restored.ode_solution.is_none());
    restored.refresh_ode_if_dirty();
    assert!(restored.ode_solution.is_some());
}

#[test]
fn malformed_ode_payload_falls_back_lazily() {
    let mut record = ViewRecord::default();
    record.field_mode = FieldMode::Ode;
    record.ode_seeds = vec![[f64::NAN, 0.0, 0.0]];
    let mut state = State::new(false);
    let mut controls = Controls::new();
    record.restore(&mut state, &mut controls);
    assert_eq!(state.field_mode, FieldMode::Ode);
    assert_eq!(state.ode, OdeScene::default());
    assert!(state.ode_error.is_some());
    assert!(state.ode_solution.is_none());
}

#[test]
fn global_reset_clears_ode_fields() {
    let mut state = State::new(true);
    state.ode = OdeScene { duration: 3.0, ..OdeScene::default() };
    state.ode_chart = OdeChart::TimeSeries;
    state.reset_ode();
    assert_eq!(state.ode, OdeScene::default());
    assert_eq!(state.ode_chart, OdeChart::Phase);
    assert_eq!(state.ode_variables, [true, true, false]);
}
```

Create `tests/ode_export_test.rs` using existing `source_export_test` decoded PNG/SVG helpers. Assert:
- Decode PNG and inspect pixels in the bottom banner region for `ODE` warning/model text pixels (non-background) and tEXt `vfe.model = "ode-fixed-step-rk4"`, `vfe.ode_source`, `vfe.ode_dimension`, `vfe.ode_duration`.
- SVG bytes contain a visible `<text>` line mentioning `ODE`, fixed-step RK4, and `bounded`, plus `<metadata>` keys; no `ode` metadata when not in Ode.
- Warning remains when solution is unavailable; no arrow/trajectory fake geometry.
- Ode export never claims accuracy/particle targets.

- [ ] **Step 2: Run and confirm RED**

Run:

```bash
cargo test --test suite ode_persistence_test::
cargo test --test suite ode_export_test::
```

Expected: compile failures for missing record/provenance fields.

- [ ] **Step 3: Implement persistence and reset**

- Add `ViewRecord` fields exactly from design, all `#[serde(default)]`, and `field_mode` accepts `"ode"` only when explicitly stored. Capture scene, chart, variables, no solution.
- `restore`: set Ode scene through `set_ode_scene`? That would solve synchronously; do not. Set `state.ode` directly after validation, `state.ode_chart`/variables, `state.ode_dirty=true`, no solution. For malformed values, fall back to default scene, set `ode_error`, keep `ode_dirty=true`. Ensure the `field_mode` fallback does not promote Ode without a valid stored Ode record.
- `State::reset_ode` resets chart/variables? Global reset should restore defaults; yes `reset_ode` sets chart `Phase`, variables `[true,true,false]`.
- Global reset calls `reset_ode` as well as existing reset paths.

- [ ] **Step 4: Implement request-time export provenance**

- Add `ExportProvenance.ode_mode: bool` and `ode_export_provenance(mode: FieldMode, solution: Option<&OdeSolution>, timeline: &SimClock)`.
- Exact warning lines:
  - `ODE - fixed-step RK4 teaching model - h = 0.005`
  - `ODE solution unavailable` when Ode has no solution
  - `ODE scene invalid: {error}` when `ode_error` exists
  - Include `timeline: duration = {duration}, step = {step}` for available solutions.
- Metadata keys: `vfe.model = "ode-fixed-step-rk4"`, `vfe.ode_source = "equations"|"current_field"`, `vfe.ode_dimension`, `vfe.ode_step`, `vfe.ode_duration`, `vfe.validation_error` only when present. No CAS text in metadata.
- Add one app `field_export_provenance` routing helper covering Formula/Sources/Induction/Ode and use it for PNG and SVG request-time capture and fallback. Never sample ODE at export.
- If the solution is absent or invalid, still emit warning and no trajectories; nonfinite never exported.
- Reuse `encode_png_with_text`, `apply_export_banner`, and SVG provenance helpers; do not add dependencies.

- [ ] **Step 5: Run focused tests and commit**

Run:

```bash
cargo test --test suite ode_persistence_test::
cargo test --test suite ode_export_test::
cargo test --test suite persistence_test::
cargo test --test suite source_export_test::
cargo test --test suite svg_export_test::
```

Expected: pass with decoded warning/metadata and no regressions. Then:

```bash
git add src/persistence.rs src/viewmodel.rs src/app/input.rs src/export_provenance.rs src/export.rs src/screenshot.rs src/app/plot.rs src/app/viewport.rs tests/ode_persistence_test.rs tests/ode_export_test.rs tests/persistence_test.rs tests/source_export_test.rs tests/suite.rs docs/CODEBASE.md
git commit -m "feat: persist and export ODE workbench context" -m "Generated-by: OpenCode"
```

---

### Task 5: Docs, integrated benchmark and final gates

**Files:**
- Modify: `README.md`, `GLOSSARY.md`, `docs/CODEBASE.md`, `docs/superpowers/README.md`, `docs/perf/ode-performance.md`
- Test: no new production tests unless a docs claim is false; fix docs instead.
- Evidence/report: `.superpowers/sdd/2026-11-17-ode-workbench/evidence/task-5/`, `task-5-report.md`

**Interfaces:**
- Consumes all prior tasks.
- Produces user/reviewer documentation and final measured gate evidence.

- [ ] **Step 1: Document the feature**

Update docs to state:
- ODE workbench and the four concept modes; typed 2D/3D equations and current formula field.
- Fixed-step RK4, `h=0.005`, 4096 stored samples including t=0, duration 20 default/25 max, eight seeds, StepLimit/NonFinite behavior.
- Current-field integration uses the durable formula domain and world coordinates, not the ODE view bounds; broken drafts retain the last solution.
- Shared `SimClock` playback/pause/step/reset, auto-pause at duration, no private timer or loop period in Ode.
- Phase portrait is a 2D projection; time series uses the same precomputed samples; up to eight distinguishable seed colors.
- ODE is vector-only, arrows/particles/streamlines/clicks disabled or absent; existing particle RK4 unchanged.
- Persistence/migration, global reset, chart/variable state, malformed fallback lazy solve.
- PNG/SVG warning/provenance, nonfinite and unavailable behavior.
- Benchmark is solve/cache evidence, not frame-time; no 33 ms or browser claim.
- Native CAS remains only for Formula; ODE does not use CAS. Browser/wasm runtime/manual visual gates owner-waived, not passed.

Use `GLOSSARY.md` additions for **ODE workbench**, **ODE seed**, **phase portrait**, and **RK4 step**, avoiding conflicting definitions.

- [ ] **Step 2: Rerun integrated benchmark**

Run `scripts/bench-ode.sh` at default 8-seed scene and a single-seed max-duration scene. Record host, date, build profile, point counts, solve ms, and stop status. If default solve exceeds 2 s or max duration exceeds a practical gate, leave it as evidence and do not claim otherwise; do not silently change caps.

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

If the wasm target is unavailable, record unavailable; never claim pass. Native smoke must show the app stays up for 15 s.

- [ ] **Step 4: Final review package and commit**

Generate `review-<base>..HEAD.diff` after every task and hand it to a fresh reviewer; this task's final diff must include docs/perf only. Then:

```bash
git add README.md GLOSSARY.md docs/CODEBASE.md docs/superpowers/README.md docs/perf/ode-performance.md
git commit -m "docs: document the ODE workbench" -m "Generated-by: OpenCode"
```

---

## Self-Review

- **Spec coverage:** Five tasks cover model/solver, mode/state/app/clock, panel/charts/help, persistence/export, docs/benchmark/gates. Every design goal and acceptance class has an owning task and test.
- **Step scan:** Every step names the exact file, API, test, command, or commit. No placeholders or assertion-free comments remain.
- **Type consistency:** `OdeScene`, `OdeSolution`, `OdeChart`, State fields, panel actions, provenance fields and harness seams are named identically across tasks.
- **Review Focus:** Each listed risk has a test: broken formula, step cap, clock/mode lifecycle, domain vs bounds, all-false variables, particle regression (existing particle/plot tests plus Task 2 focused regressions).
- **Proportion:** The plan specifies decisions and tests without transcribing every implementation body. Algorithms that are fixed by the design are stated; ordinary bodies remain the implementer's work.
