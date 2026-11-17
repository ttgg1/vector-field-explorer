# Induction Demo Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a bounded, editable Faraday induction teaching demo with separate E/B fields, analytic/softened vector-potential E, native CAS/web waveform parity, one shared clock, persistence and export warnings.

**Architecture:** A dedicated `FieldMode::Induction` owns one current-loop or finite-solenoid scene. B comes from the existing softened source evaluator with unit current; E comes from a bounded vector-potential quadrature. Per-point unit-current values are cached by geometry key and rescaled on time-only changes. The app reuses the source glyph, vector scene, clock, export provenance and responsive panel seams.

**Tech Stack:** Rust 2024, glam DVec3, meval/exmex (native CAS), egui/eframe, existing source evaluator, PNG/SVG export seams.

**Spec:** `docs/superpowers/specs/2026-11-17-induction-demo-design.md` (APPROVED 2026-11-17)

## Global Constraints

- One global `SimClock`; no private timer, time mirror or persisted clock state.
- E and B are separate outputs. Never add, average or share a colormap range in `Both`; fixed E amber `[255,176,64]` and B cyan `[64,196,255]`.
- E is `-∂A/∂t` from the softened axisymmetric vector potential; never a general inverse-Faraday solve or finite difference in time.
- Native CAS differentiates the canonical waveform. Wasm uses the analytic derivative and must not depend on CAS.
- Softening is one shared `SourceScene.softening`; no per-source or induction-only softening.
- Accuracy-not-met is default-hidden; opt-in changes drawing only. Warnings and caps are never bypassed. Nonfinite values are never valid arrows or exports.
- Induction arrows only: no induction particles, streamlines, click seeding or CAS panel. Preserve Formula/Sources state and formula particle state.
- #13 plane selection/seeding, #15 CLI, and #26 export backends are reused, not duplicated. No new dependencies beyond the already-direct `png`.
- Tests live in `tests/`, register in `tests/suite.rs`; no inline `#[cfg(test)]` under `src/`. Browser/manual/wasm visual gates are owner-waived, not passed.
- Changed files must pass `cargo test`, build, clippy `-D warnings`, rustfmt with `skip_children=true`, `check-docs.py`, `git diff --check`; native smoke for UI changes.

## Review Focus

1. **Sign and axis convention:** reversed current/axis must reverse B and E, and Faraday contours must close with the right sign. Pinned by Task 1 contour tests.
2. **Stale cache after geometry/plane/mode changes:** time-only rescaling must never reuse arrows from old geometry or a stale slice. Pinned by Task 2 cache tests.
3. **Accuracy hiding:** a cached TargetMet point that becomes AccuracyNotMet after current rescaling must obey opt-in; Unavailable/nonfinite never draw. Pinned by Task 1 classification and Task 2 display tests.
4. **Clock edge cases:** zero amplitude/frequency is static and consumes no clock; pause/step/reset preserve the last valid field. Pinned by Task 2 clock tests.
5. **Export warning truthfulness:** PNG/SVG warnings must survive opt-in and missing sampling. Pinned by Task 4 provenance and decoded-output tests.

---

### Task 1: Induction model, vector-potential evaluator and fixtures

**Files:**
- Create: `src/induction.rs`
- Modify: `src/lib.rs`, `docs/CODEBASE.md`
- Create: `tests/induction_test.rs`, `tests/induction_bench_test.rs`
- Create: `tests/support/induction/{mod.rs,loop.csv,solenoid.csv,generate.py,README.md}`
- Modify: `tests/support/mod.rs`, `tests/suite.rs`
- Create: `scripts/bench-induction.sh`

**Interfaces:**
- Produces: `InductionCase`, `InductionScene` (derive `Debug, Clone, PartialEq`), `InductionKey`, `InductionPrepared`, `InductionSampling`, `InductionFamilyResult`, `InductionEstimate`, `InductionFamilyStatus`, `InductionPointSample`, `prepare_induction`, `sample_induction`, `evaluate_induction_point`, `induction_key`.
- `InductionScene` methods: `current(&self, t: f32) -> f64`, `current_derivative(&self, t: f32) -> f64`, `is_animated(&self) -> bool`, `validate(&self) -> Result<(), String>`, `unit_source(&self) -> Source`, `unit_vector_potential(&self, point: DVec3, softening: f64) -> Result<f64, String>`.
- Consumes: `Source`, `SourceKind`, `SourceSet`, `Evaluator`, `EvalOptions`, `FamilyResult`, `StopReason`, `AccuracyRequest`, `PlaneFrame`, `Domain`, `ArrowData`, `ViewOptions`, `magnitude_range`.

- [ ] **Step 1: Write failing model/waveform tests**

In `tests/induction_test.rs`:

```rust
use glam::{DVec3, UVec3, Vec3};
use vector_field_explorer::domain::Domain;
use vector_field_explorer::induction::{
    InductionCase, InductionScene, evaluate_induction_point, prepare_induction,
    sample_induction,
};
use vector_field_explorer::sampling::ViewOptions;
use vector_field_explorer::source_scene::SourceDisplay;

fn loop_scene() -> InductionScene {
    InductionScene {
        case: InductionCase::CurrentLoop,
        center: DVec3::ZERO,
        axis: DVec3::Z,
        radius: 1.0,
        length: 0.0,
        turns: 1,
        amplitude: 1.0,
        frequency: 0.25,
        display: SourceDisplay::Both,
        allow_inaccurate: false,
    }
}

#[test]
fn waveform_and_static_predicate_are_exact() {
    let mut scene = loop_scene();
    assert!((scene.current(0.0) - 1.0).abs() < 1e-15);
    assert!(scene.current_derivative(0.0).abs() < 1e-15);
    assert!((scene.current(0.5) - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-15);
    assert!((scene.current_derivative(0.5) + 1.110_720_734_680_402_5).abs() < 1e-12);
    assert!(scene.is_animated());
    scene.amplitude = 0.0;
    assert!(!scene.is_animated());
    scene.amplitude = 1.0;
    scene.frequency = 0.0;
    assert!(!scene.is_animated());
}
```

- [ ] **Step 2: Add fixture-driven physics tests**

Define `tests/support/induction::rows(case)` reading the committed CSV. Add:

```rust
#[test]
fn loop_vector_potential_matches_independent_fixtures() {
    let scene = loop_scene();
    for row in support::induction::rows("loop") {
        let got = scene.unit_vector_potential(DVec3::new(row.r, 0.0, row.z), row.softening).unwrap();
        let tolerance = 1e-9 * row.reference.abs().max(1.0);
        assert!((got - row.reference).abs() <= tolerance, "{row:?}: got {got:e}");
    }
}

#[test]
fn solenoid_vector_potential_matches_independent_fixtures_and_curl_matches_b() {
    let mut scene = loop_scene();
    scene.case = InductionCase::Solenoid;
    scene.radius = 0.5;
    scene.length = 2.0;
    scene.turns = 8;
    for row in support::induction::rows("solenoid") {
        let got = scene.unit_vector_potential(DVec3::new(row.r, 0.0, row.z), row.softening).unwrap();
        let tolerance = 1e-9 * row.reference.abs().max(1.0);
        assert!((got - row.reference).abs() <= tolerance, "{row:?}: got {got:e}");
    }
    let point = DVec3::new(0.7, 0.0, 0.3);
    let h = 1e-4;
    let a = |p| scene.unit_vector_potential(p, 0.1).unwrap();
    let b_r = -(a(DVec3::new(0.7, 0.0, 0.3 + h)) - a(DVec3::new(0.7, 0.0, 0.3 - h))) / (2.0 * h);
    let br = a(DVec3::new(0.7 + h, 0.0, 0.3)) * (0.7 + h);
    let bl = a(DVec3::new(0.7 - h, 0.0, 0.3)) * (0.7 - h);
    let b_z = (br - bl) / (2.0 * h * 0.7);
    let sample = evaluate_induction_point(&scene, point, 0.0, 0.1);
    let vector_field_explorer::sources::FamilyResult::TargetMet(b) = sample.magnetic else {
        panic!("unit-current B must be TargetMet")
    };
    assert!((b.value.x - b_r).abs() < 2e-5);
    assert!((b.value.z - b_z).abs() < 2e-5);
}
```

- [ ] **Step 3: Add Faraday, classification and CAS tests**

```rust
#[test]
fn faraday_contour_matches_flux_rate_for_both_cases() {
    for case in [InductionCase::CurrentLoop, InductionCase::Solenoid] {
        let mut scene = loop_scene();
        scene.case = case;
        if case == InductionCase::Solenoid {
            scene.radius = 0.5;
            scene.length = 2.0;
            scene.turns = 8;
        }
        let t = 0.5;
        let radius = 1.25;
        let n = 64;
        let mut contour = 0.0;
        for i in 0..n {
            let phi = std::f64::consts::TAU * (i as f64 + 0.5) / n as f64;
            let p = DVec3::new(radius * phi.cos(), radius * phi.sin(), 0.0);
            let sample = evaluate_induction_point(&scene, p, t as f32, 0.1);
            let vector_field_explorer::induction::InductionFamilyResult::TargetMet(e) = sample.electric else {
                panic!("contour E must be TargetMet")
            };
            contour += e.value.dot(DVec3::new(-phi.sin(), phi.cos(), 0.0)) * radius * std::f64::consts::TAU / n as f64;
        }
        let (flux_rate, flux_uncertainty) = flux_rate(&scene, radius, t, 0.1);
        let tolerance = (1e-3 * flux_rate.abs()).max(20.0 * flux_uncertainty);
        assert!((contour - flux_rate).abs() <= tolerance, "{case:?}: {contour:e} vs {flux_rate:e}");
    }
}

fn flux_rate(scene: &InductionScene, radius: f64, t: f64, softening: f64) -> (f64, f64) {
    // Integrate the unit-current B_z over a polar disk; the field at `t` is
    // scaled by I(t), so divide it out before applying the analytic dI/dt.
    let n = 64;
    let current = scene.current(t as f32);
    assert!(current.abs() > 1e-6);
    let mut flux_unit = 0.0;
    let mut uncertainty = 0.0;
    for ir in 0..n {
        let r = radius * (ir as f64 + 0.5) / n as f64;
        for ip in 0..n {
            let phi = std::f64::consts::TAU * (ip as f64 + 0.5) / n as f64;
            let p = DVec3::new(r * phi.cos(), r * phi.sin(), 0.0);
            let sample = evaluate_induction_point(scene, p, t as f32, softening);
            let vector_field_explorer::sources::FamilyResult::TargetMet(b) = sample.magnetic else {
                panic!("unit-current B must be TargetMet")
            };
            let area = r * (radius / n as f64) * (std::f64::consts::TAU / n as f64);
            flux_unit += b.value.z / current * area;
            uncertainty += b.uncertainty / current.abs() * area;
        }
    }
    (
        -scene.current_derivative(t as f32) * flux_unit,
        scene.current_derivative(t as f32).abs() * uncertainty,
    )
}

#[test]
fn static_current_has_exactly_zero_induced_field() {
    let mut scene = loop_scene();
    scene.frequency = 0.0;
    let prepared = prepare_induction(
        &scene,
        &Domain::new(Vec3::splat(-2.0), Vec3::splat(2.0)),
        UVec3::new(5, 5, 5),
        None,
        0.1,
        Default::default(),
    )
    .unwrap();
    let sampled = sample_induction(&prepared, &scene, 0.0, &ViewOptions::default());
    assert!(sampled.arrows.iter().all(|a| a.mag == 0.0));
    assert_eq!(sampled.electric.drawn, 0);
    assert_eq!(sampled.electric.accuracy_not_met_drawn, 0);
}

#[test]
fn cas_derivative_matches_the_analytic_waveform() {
    let scene = loop_scene();
    let source = format!("{}*cos({}*t)", scene.amplitude, std::f64::consts::TAU * scene.frequency);
    let derivative = vector_field_explorer::cas::differentiate(&source, vector_field_explorer::cas::Variable::T).unwrap();
    let expr: meval::Expr = derivative.parse().unwrap();
    let f = expr.bind("t").unwrap();
    for t in [0.0, 0.25, 0.5, 1.0] {
        assert!((f(t) - scene.current_derivative(t as f32)).abs() <= 1e-12 * scene.current_derivative(t as f32).abs().max(1.0));
    }
}
```

`flux_rate` is a test helper integrating `-B_z` over a 64×64 polar disk using `evaluate_induction_point(..., t).magnetic`; tolerance already includes a 1e-3 allowance. Add `nonfinite_scene_is_rejected` and `validation_rejects_bad_geometry` with exact messages/`Err`.

- [ ] **Step 4: Run RED**

Run: `cargo test --test suite induction_test::`
Expected: compile failure because `vector_field_explorer::induction` does not exist.

- [ ] **Step 5: Implement `src/induction.rs`**

Implement the design types exactly. `prepare_induction` must:
1. validate the scene and softening/accuracy;
2. build `unit_source()` and one `Evaluator` with `EvalOptions { softening, accuracy, ..Default::default() }`;
3. iterate the world grid or `PlaneFrame` local grid exactly as `sample_source_scene` does;
4. store local/world position, unit `E` from `unit_vector_potential` times the unit azimuthal basis (`φhat`; exact zero on axis), its uncertainty, and the magnetic `Estimate` value/uncertainty/stop reason;
5. reject nonfinite/over-cap results as `Unavailable`/`Validation` without drawing.
Use the fixed eight-panel, 32-point Gauss–Legendre azimuthal quadrature from the design, coarse four-panel error estimate, and `max(|fine-coarse|, 32*f64::EPSILON*|fine|)`.

`sample_induction` computes `I(t)`, `dI/dt`, scales E by `dI/dt`, B by `I`, reclassifies against the target, filters display/opt-in, colours, and returns arrows. `Both` concatenates E then B with fixed colours; single families use `magnitude_range`.

- [ ] **Step 6: Add fixture generator and committed rows**

`generate.py` uses `mpmath` at 40/70 digits, true `mp.pi`, the exact softened loop/solenoid integrals, rejects a convergence change over `1e-12 + 1e-10*|ref|`, and writes CSV. `README.md` records commands and method independence. Commit the CSV rows. Tests load them with a small parser; no `mpmath` runtime dependency.

- [ ] **Step 7: Add benchmark and freeze caps**

Create `tests/induction_bench_test.rs` that returns immediately unless `VFE_INDUCTION_BENCH` is set; otherwise prepares both cases at 20³ and 120² (provisional) plus the frozen 80² cap, and prints `INDUCTION BENCH case=... view=... points=... ms=...`. Create `scripts/bench-induction.sh` setting the env var and running the exact test with `--nocapture`. Record the measured times in `docs/perf/induction-performance.md`; the Task 1 run lowered `MAX_INDUCTION_DENSITY_2D` from 120 to 80 because 120² exceeded 2 s. Update the design/plan cap values if lowered.

- [ ] **Step 8: Verify and commit**

Run focused tests, full suite, build, clippy, rustfmt, docs, diff. Commit:

```bash
git add src/induction.rs src/lib.rs tests/induction_test.rs tests/induction_bench_test.rs \
  tests/support/induction tests/support/mod.rs tests/suite.rs docs/CODEBASE.md \
  docs/perf/induction-performance.md scripts/bench-induction.sh
git commit -m "feat: evaluate induced fields from a softened vector potential" -m "Generated-by: OpenCode"
```

---

### Task 2: Shared-clock, state and app integration

**Files:**
- Modify: `src/source_scene.rs`, `src/viewmodel.rs`, `src/app/simulation.rs`, `src/app/input.rs`, `src/app/panels.rs`, `src/app/plot.rs`, `src/sim_clock.rs`
- Create: `tests/induction_app_test.rs`
- Modify: `tests/suite.rs`, `docs/CODEBASE.md`

**Interfaces:**
- Consumes Task 1 API and `prepare_induction`, `sample_induction`, `InductionKey`.
- Produces: `FieldMode::Induction`, `State::induction`, `State::induction_prepared: Option<Arc<InductionPrepared>>`, `State::induction_sampling: Option<InductionSampling>`, `State::set_induction_scene(scene) -> bool` (retain-last-valid), `State::refresh_induction()`, harness `induction_sampling() -> Option<InductionSampling>`, `induction_prepared_arc() -> Option<Arc<InductionPrepared>>`.

- [ ] **Step 1: Write failing app/clock tests**

Use `VfApp::new()`, `app.state`, `app.test_harness()` and the real `SimClock` methods. Required tests:

```rust
#[test]
fn induction_mode_uses_caps_and_preserves_formula_and_sources() {
    let mut app = VfApp::new();
    let formula = app.state.formula.clone();
    let sources = app.state.sources.clone();
    app.state.set_density(200);
    app.state.set_field_mode(FieldMode::Induction);
    app.test_harness().advance_simulation();
    let grid = app.state.plane_grid();
    assert!(grid.x.max(grid.y).max(grid.z) <= 20, "3D induction cap: {grid:?}");
    let sampling = app.test_harness().induction_sampling().expect("sampling");
    assert!(sampling.arrows.len() <= 80 * 80, "at most the 2D cap");
    assert_eq!(app.state.formula, formula);
    assert_eq!(app.state.sources, sources);
}

#[test]
fn animated_induction_consumes_the_shared_clock_and_static_does_not() {
    use vector_field_explorer::sim_clock::{has_clock_consumer, should_animate};
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Induction);
    let scene = InductionScene {
        amplitude: 1.0,
        frequency: 0.25,
        ..InductionScene::default()
    };
    assert!(app.state.set_induction_scene(scene));
    assert!(has_clock_consumer(&app.state));
    assert!(should_animate(&app.state, &app.clock));
    let mut still = app.state.induction.clone();
    still.frequency = 0.0;
    assert!(app.state.set_induction_scene(still));
    assert!(!has_clock_consumer(&app.state));
    assert!(!should_animate(&app.state, &app.clock));
}

#[test]
fn pause_freezes_arrows_and_step_advances_one_coherent_step() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Induction);
    let scene = InductionScene {
        amplitude: 1.0,
        frequency: 0.25,
        ..InductionScene::default()
    };
    assert!(app.state.set_induction_scene(scene));
    app.test_harness().advance_clock(0.05);
    app.test_harness().advance_simulation();
    app.clock.set_playing(false);
    let stopped = app.clock.time;
    let stopped_sample = app.state.time;
    app.test_harness().advance_clock(0.05);
    app.test_harness().advance_simulation();
    assert_eq!(app.clock.time, stopped, "pause holds time");
    assert_eq!(
        app.state.time,
        stopped_sample,
        "pause holds the sampled field"
    );
    app.clock.set_playing(true);
    app.test_harness().advance_clock(0.05);
    app.test_harness().advance_simulation();
    assert!(app.clock.time > stopped, "one resumed frame advances");
    assert_ne!(
        app.state.time,
        stopped_sample
    );
}

#[test]
fn geometry_change_reprepares_and_time_only_change_rescales() {
    use std::sync::Arc;
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Induction);
    let scene = InductionScene {
        amplitude: 1.0,
        frequency: 0.25,
        ..InductionScene::default()
    };
    assert!(app.state.set_induction_scene(scene));
    app.test_harness().advance_clock(0.05);
    app.test_harness().advance_simulation();
    let first = app.test_harness().induction_prepared_arc().unwrap();
    app.test_harness().advance_clock(0.05);
    app.test_harness().advance_simulation();
    let second = app.test_harness().induction_prepared_arc().unwrap();
    assert!(Arc::ptr_eq(&first, &second), "time-only keeps the prepared cache");
    let mut moved = app.state.induction.clone();
    moved.axis = DVec3::X;
    assert!(app.state.set_induction_scene(moved));
    app.test_harness().advance_simulation();
    let third = app.test_harness().induction_prepared_arc().unwrap();
    assert!(!Arc::ptr_eq(&second, &third), "geometry change re-prepares");
}

#[test]
fn e_and_b_display_never_sum_and_use_fixed_colours() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Induction);
    let mut scene = InductionScene::default();
    scene.display = SourceDisplay::Both;
    assert!(app.state.set_induction_scene(scene));
    app.test_harness().advance_simulation();
    let sampling = app.test_harness().induction_sampling().unwrap();
    let colors: Vec<[u8; 3]> = sampling.arrows.iter().map(|arrow| arrow.color).collect();
    assert!(colors.contains(&[255, 176, 64]), "E amber present");
    assert!(colors.contains(&[64, 196, 255]), "B cyan present");
    assert!(sampling.electric.drawn > 0 && sampling.magnetic.drawn > 0);
    let mut electric = app.state.induction.clone();
    electric.display = SourceDisplay::Electric;
    assert!(app.state.set_induction_scene(electric));
    app.test_harness().advance_simulation();
    let sampling = app.test_harness().induction_sampling().unwrap();
    assert!(sampling.electric.drawn > 0);
    assert_eq!(sampling.magnetic.drawn, 0);
}

#[test]
fn induction_slice_uses_local_plane_positions_and_no_particles() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Induction);
    app.state.set_mode(false);
    app.state.set_plane_axis(vector_field_explorer::plane::PlaneAxis::X);
    app.test_harness().advance_simulation();
    let sampling = app.test_harness().induction_sampling().unwrap();
    assert!(!sampling.arrows.is_empty());
    assert!(sampling.arrows.iter().all(|arrow| arrow.position.z == 0.0));
    assert!(app.state.particle_target().is_none());
    let (positions, _, _, _) = app.test_harness().particle_snapshot();
    assert_eq!(positions.len(), 0, "no induction particle cloud is created");
}

#[test]
fn scalar_plot_switch_pauses_induction_and_preserves_time() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Induction);
    app.test_harness().advance_clock(0.05);
    app.test_harness().advance_simulation();
    let before = app.clock.time;
    app.state.set_plot_kind(vector_field_explorer::plot_state::PlotKind::Curve2D);
    app.test_harness().advance_clock(0.05);
    app.test_harness().advance_simulation();
    assert_eq!(app.clock.time, before, "scalar display pauses vector consumers");
    app.state.set_plot_kind(vector_field_explorer::plot_state::PlotKind::VectorField);
    app.test_harness().advance_clock(0.05);
    app.test_harness().advance_simulation();
    assert!(app.clock.time > before);
}
```

Assert `grid()`/`plane_grid()` are at most 20³/80², `source_sampling` is unchanged, formula particle cloud is preserved but `particle_target()` is `None`, and empty lines/trails/dots. For cache identity, compare `Arc::ptr_eq` before/after a time-only step and `sample.current` changes; after an axis/radius edit the prepared Arc must differ and positions update. `Both` arrows must contain exact amber/cyan; `Electric`/`Magnetic` arrows must not include the other family colour. Plane slice positions must have local `z == 0`; a plane axis/index change must resample.

- [ ] **Step 2: Run RED**

Run: `cargo test --test suite induction_app_test::`
Expected: missing `FieldMode::Induction`/state methods.

- [ ] **Step 3: Implement viewmodel and app**

Add `FieldMode::Induction` and state fields. Implement `set_induction_scene` with validation/retain-last-valid semantics, `refresh_induction` key comparison/reprepare/resample, `has_time_dependent_consumer` induction animation, `particle_target` none, `active_dimension` 3, induction caps, and dirty/time rules. In `snapshot` return induction arrows and empty geometry; in `rebuild_scene` append the selected conductor glyph in Induction. Extend `sync_field_mode` for cache clearing and transient rule; preserve Formula/Sources/Induction scenes. Add harness accessors for `induction_sampling`, prepared cache identity, arrows, and time.

- [ ] **Step 4: Verify and commit**

Run focused app/clock/plane tests, full suite, build, clippy, rustfmt, docs, diff, native smoke. Commit:

```bash
git commit -m "feat: animate induction fields on the shared clock" -m "Generated-by: OpenCode"
```

---

### Task 3: Induction panel, adapter selector and help

**Files:**
- Create: `src/ui/induction_panel.rs`
- Modify: `src/ui/mod.rs`, `src/ui/formula_bar.rs`, `src/ui/sidebar.rs`, `src/ui/layout.rs`, `src/ui/help_overlay.rs`, `src/app/panels.rs`
- Create: `tests/induction_panel_test.rs`
- Modify: `tests/suite.rs`, `docs/CODEBASE.md`

**Interfaces:**
- Consumes Task 2 state/actions.
- Produces: `InductionPanelState`, `InductionPanelAction::{Fit,Reset}`, `induction_panel::show(ui, &mut InductionPanelState, &mut State, compact) -> Option<InductionPanelAction>`, `FormulaBarState::open_induction`, `SheetState::Induction`, `induction_help_rows()`.
- `InductionPanelState` public fields: `scene: InductionScene`, `error: Option<String>`, `advanced: bool`; `commit(&mut self, state: &mut State) -> bool` validates and commits the draft through `State::set_induction_scene`, preserving the last valid scene and setting `error` on failure.

- [ ] **Step 1: Write failing real-widget tests**

Copy the `WidgetNode`/`nodes`/`find`/`has`/`pointer`/`context`/`run_frame` helpers from `tests/plot_ui_test.rs` verbatim (they are local integration-test helpers, not a public API). Then assert:

```rust
#[test]
fn induction_panel_wide_and_compact_expose_all_controls() {
    for (width, compact) in [(1200.0, false), (390.0, true)] {
        let ctx = context();
        let mut panel = InductionPanelState::default();
        let mut state = State::new(true);
        state.set_field_mode(FieldMode::Induction);
        let output = run_frame(&ctx, width, 900.0, Vec::new(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let _ = induction_panel::show(ui, &mut panel, &mut state, compact);
            });
        });
        let nodes = nodes(&output);
        for label in ["Current loop", "Finite solenoid", "Amplitude", "Frequency", "Both", "Fit"] {
            assert!(has(&nodes, label), "{width} pt is missing {label}");
        }
        assert!(has(&nodes, "Radius"));
        assert!(has(&nodes, "Advanced"));
    }
}

#[test]
fn formula_bar_selects_induction_and_hides_the_formula_editor() {
    let ctx = context();
    let mut state = State::new(true);
    let mut bar = FormulaBarState::default();
    let mut output = run_frame(&ctx, 1200.0, 200.0, Vec::new(), |ui| {
        formula_bar::show(ui, &mut state, &mut bar, false);
    });
    let induction = find(&nodes(&output), "Induction").expect("Induction selector").rect;
    run_frame(&ctx, 1200.0, 200.0, pointer(induction.center(), true), |ui| {
        formula_bar::show(ui, &mut state, &mut bar, false);
    });
    output = run_frame(&ctx, 1200.0, 200.0, pointer(induction.center(), false), |ui| {
        formula_bar::show(ui, &mut state, &mut bar, false);
    });
    assert_eq!(state.field_mode, FieldMode::Induction);
    let nodes = nodes(&output);
    assert!(!has(&nodes, "Field formula"), "the formula editor is hidden");
    assert!(has(&nodes, "Induction scene summary"));
}

#[test]
fn edits_validate_and_keep_the_last_valid_scene() {
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Induction);
    let valid = state.induction.clone();
    let mut panel = InductionPanelState::default();
    panel.scene.radius = -1.0;
    assert!(!panel.commit(&mut state));
    assert_eq!(state.induction, valid, "invalid geometry is not committed");
    assert!(panel.error.as_deref().unwrap_or("").contains("radius"));
    panel.scene = valid.clone();
    panel.scene.amplitude = 2.5;
    assert!(panel.commit(&mut state));
    assert_eq!(state.induction.amplitude, 2.5);
    assert!(panel.error.is_none());
}

#[test]
fn help_explains_assumptions_clock_and_arrow_scope() {
    let text = induction_help_rows()
        .iter()
        .map(|row| format!("{} {}", row.input, row.action))
        .collect::<Vec<_>>()
        .join(" ");
    for needle in ["quasistatic", "vector potential", "shared clock", "arrows only", "arbitrary B"] {
        assert!(text.contains(needle), "help is missing {needle}");
    }
}
```

- [ ] **Step 2: Run RED**

Run: `cargo test --test suite induction_panel_test::`
Expected: missing module and panel state.

- [ ] **Step 3: Implement UI and layout**

Add the third adapter selector, hide the formula editor in Induction, add the dedicated left rail/compact sheet, reuse `Controls` patterns where possible, add section reset and Fit, disable vector-only features with explanation, and render the CAS derivative diagnostic only on native. Add `induction_help_rows()` and render it in the Help window. Do not change the source panel or #13 plane controls.

- [ ] **Step 4: Verify and commit**

Run induction panel, button sizing, responsive, layout, menu, source panel, sidebar, full suite, build, clippy, rustfmt, docs, diff, native smoke. Commit:

```bash
git commit -m "feat: add the responsive induction panel" -m "Generated-by: OpenCode"
```

---

### Task 4: Persistence, reset and export provenance

**Files:**
- Modify: `src/persistence.rs`, `src/export_provenance.rs`, `src/app/plot.rs`, `src/app/viewport.rs`, `src/app/panels.rs`, `src/app/input.rs`
- Modify: `tests/persistence_test.rs`, `tests/source_export_app_test.rs`, `tests/source_export_test.rs`
- Create: `tests/induction_export_test.rs`
- Modify: `tests/suite.rs`, `docs/CODEBASE.md`

**Interfaces:**
- Consumes `InductionScene`, `InductionSampling`, `FieldMode::Induction`, existing provenance APIs.
- Produces: `induction_export_provenance`, induction metadata keys, persistent induction fields.

- [ ] **Step 1: Write failing persistence/export tests**

```rust
#[test]
fn induction_scene_round_trips_and_legacy_defaults_to_formula() {
    let scene = InductionScene {
        case: InductionCase::Solenoid,
        radius: 0.5,
        length: 2.0,
        turns: 8,
        amplitude: 2.0,
        frequency: 0.125,
        ..InductionScene::default()
    };
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Induction);
    assert!(state.set_induction_scene(scene.clone()));
    let record = ViewRecord::capture(&state);
    let json = serde_json::to_string(&record).unwrap();
    let back: ViewRecord = serde_json::from_str(&json).unwrap();
    let mut restored = State::new(true);
    let mut controls = Controls::default();
    back.restore(&mut restored, &mut controls);
    assert_eq!(restored.field_mode, FieldMode::Induction);
    assert_eq!(restored.induction, scene);

    let legacy: ViewRecord = serde_json::from_str("{}").unwrap();
    let mut legacy_state = State::new(false);
    legacy.restore(&mut legacy_state, &mut Controls::default());
    assert_eq!(legacy_state.field_mode, FieldMode::Formula);
    assert_eq!(legacy_state.induction, InductionScene::default());
    assert!(legacy_state.source_notice.is_none());
}

#[test]
fn malformed_induction_fields_fall_back_and_notice() {
    let text = r#"{
        "field_mode": "Induction",
        "induction_case": "bogus",
        "induction_radius": -1.0,
        "induction_frequency": -2.0
    }"#;
    let record: ViewRecord = serde_json::from_str(text).unwrap();
    let mut state = State::new(true);
    record.restore(&mut state, &mut Controls::default());
    assert_eq!(state.induction, InductionScene::default());
    assert!(state.source_notice.as_deref().unwrap_or("").contains("induction"));
}

#[test]
fn global_reset_clears_induction_cache_and_restores_defaults() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Induction);
    let mut scene = app.state.induction.clone();
    scene.radius = 1.75;
    assert!(app.state.set_induction_scene(scene));
    app.test_harness().advance_simulation();
    assert!(app.test_harness().induction_prepared_arc().is_some());
    app.test_harness().reset_all();
    assert_eq!(app.state.field_mode, FieldMode::Formula);
    assert_eq!(app.state.induction, InductionScene::default());
    assert!(app.state.induction_prepared.is_none());
    assert!(app.state.induction_sampling.is_none());
}

#[test]
fn induction_png_and_svg_carry_model_and_accuracy_warnings() {
    let scene = InductionScene::default();
    let sampling = InductionSampling {
        arrows: Vec::new(),
        electric: InductionFamilyStatus {
            drawn: 1,
            target_met: 0,
            accuracy_not_met_total: 1,
            accuracy_not_met_drawn: 1,
            unavailable: 0,
        },
        magnetic: InductionFamilyStatus {
            drawn: 0,
            target_met: 0,
            accuracy_not_met_total: 0,
            accuracy_not_met_drawn: 0,
            unavailable: 0,
        },
        current: 1.0,
        derivative: 0.25,
        derivative_source: None,
        softening: 0.1,
        validation_error: None,
    };
    let provenance = induction_export_provenance(FieldMode::Induction, &scene, Some(&sampling))
        .expect("induction exports always carry provenance");
    assert!(provenance.warning_lines[0].contains(
        "Induction - quasistatic axisymmetric softened model - a = 0.1"
    ));
    assert!(provenance
        .metadata
        .iter()
        .any(|(key, value)| key == "vfe.induction_case" && value == "current_loop"));
    assert!(provenance
        .warning_lines
        .iter()
        .any(|line| line.contains("accuracy not met")));

    let mut rgba = vec![20u8; 320 * 120 * 4];
    for pixel in rgba.chunks_exact_mut(4) {
        pixel[3] = 255;
    }
    vector_field_explorer::screenshot::apply_export_banner(&mut rgba, 320, 120, &provenance)
        .unwrap();
    let png = vector_field_explorer::export::encode_png_with_text(
        &rgba,
        320,
        120,
        &[(String::from("vfe.induction_case"), String::from("current_loop"))],
    )
    .unwrap();
    let decoded = image::load_from_memory(&png).unwrap().to_rgba8();
    let bottom = decoded.view(0, 100, 320, 20);
    assert!(bottom.pixels().any(|pixel| pixel.0[0] > 240), "baked banner pixels");
}

#[test]
fn induction_opt_in_cannot_remove_warning_or_bypass_caps() {
    let mut scene = InductionScene::default();
    scene.allow_inaccurate = true;
    let sampling = InductionSampling {
        arrows: Vec::new(),
        electric: InductionFamilyStatus {
            drawn: 3,
            target_met: 0,
            accuracy_not_met_total: 3,
            accuracy_not_met_drawn: 3,
            unavailable: 0,
        },
        magnetic: InductionFamilyStatus {
            drawn: 0,
            target_met: 0,
            accuracy_not_met_total: 0,
            accuracy_not_met_drawn: 0,
            unavailable: 2,
        },
        current: 1.0,
        derivative: 0.25,
        derivative_source: None,
        softening: 0.1,
        validation_error: None,
    };
    let provenance = induction_export_provenance(FieldMode::Induction, &scene, Some(&sampling))
        .unwrap();
    assert!(provenance
        .warning_lines
        .iter()
        .any(|line| line.contains("accuracy not met: 0 hidden, 3 drawn")));
    assert!(provenance
        .warning_lines
        .iter()
        .any(|line| line.contains("Unavailable: 2")));
    assert_eq!(sampling.electric.accuracy_not_met_drawn, 3);
    assert_eq!(sampling.magnetic.unavailable, 2);
}
```

`test_banner_image` is a local helper drawing a known background; use the existing `source_export` test pattern. SVG assertions use `scene_to_svg_with_provenance` and must find both the visible warning text and `<metadata>`.

Assert exact model line contains `Induction - quasistatic axisymmetric softened model - a = 0.1`; metadata includes `vfe.induction_case`, `vfe.waveform`, `vfe.waveform_derivative`, `vfe.amplitude`, `vfe.frequency`, `vfe.display`, and counts. PNG path: decode the image and assert the banner is present outside GUI chrome; PNG tEXt has `vfe.induction_case`. SVG: visible `<text>` and `<metadata>` contain the warning. Missing/invalid sampling still yields a warning. `allow_inaccurate` true draws eligible hidden points but leaves warning text. Nonfinite sample never appears in arrows/serialized output.

- [ ] **Step 2: Run RED**

Run: `cargo test --test suite induction_export_test::`
Expected: missing provenance/persistence fields.

- [ ] **Step 3: Implement persistence and provenance**

Add serde-defaulted induction fields to `ViewRecord`; capture/restore through `State` setters; default valid unsampled scene; malformed fallback notice; preserve `source_softening`; reset clears cache and scene. Extend `ExportProvenance` with induction mode, add `induction_export_provenance(mode, scene, sampling)` mirroring `source_export_provenance`, and route request-time provenance in app export dispatch. Reuse existing banner/tEXt/SVG functions; no new export backend.

- [ ] **Step 4: Verify and commit**

Run persistence, export, screenshot, SVG, full suite, build, clippy, rustfmt, docs, diff, native smoke. Commit:

```bash
git commit -m "feat: persist and export induction demo context" -m "Generated-by: OpenCode"
```

---

### Task 5: Documentation, performance evidence and final gates

**Files:**
- Modify: `README.md`, `GLOSSARY.md`, `docs/CODEBASE.md`, `docs/help.md` if present, `docs/perf/induction-performance.md`, `docs/superpowers/README.md`
- Test: existing suite only; no new production code.

**Interfaces:**
- Consumes all prior tasks.
- Produces: user-facing documentation and honest performance evidence.

- [ ] **Step 1: Update documentation**

Document the Induction mode, assumptions (quasistatic, axisymmetric, softened, no displacement current), why arbitrary B does not determine E, E/B selection, clock behavior, static zero, arrows-only scope, native CAS/wasm parity, persistence, export warning, and the measured caps. Mark browser/wasm/manual visual gates owner-waived, not passed.

- [ ] **Step 2: Re-run the integrated benchmark and record results**

Run `scripts/bench-induction.sh` through the integrated app path at both caps; update `docs/perf/induction-performance.md` with host, time, point counts and the explicit statement that the 33 ms particle target is not claimed.

- [ ] **Step 3: Full gates**

```bash
cargo test --test suite
cargo build
cargo clippy --all-targets -- -D warnings
rustfmt --edition 2024 --check --config skip_children=true <changed rs files>
python3 scripts/check-docs.py
git diff --check
scripts/smoke-run.sh 15
```

Run `cargo check --target wasm32-unknown-unknown` if the toolchain is available; record if unavailable, do not claim a pass. Commit:

```bash
git commit -m "docs: document the induction teaching demo" -m "Generated-by: OpenCode"
```

---

## Self-Review

**Spec coverage:** Tasks 1–2 cover the physical model, waveform, vector potential, B consistency, Faraday contour, classification, caps/cache, shared clock, E/B selection, plane behavior and arrows-only scope. Task 3 covers the dedicated panel, adapter selector, responsive layout, validation and help. Task 4 covers persistence/migration/reset and PNG/SVG warning provenance. Task 5 covers docs, benchmark evidence and gates. No design goal lacks an owning task.

**Step scan:** Every test step names executable tests and pinned values; implementation steps name exact APIs and ownership. The only open implementation details are the body of the Gauss–Legendre kernel and egui layout, which the signatures and design leave to the implementer.

**Type consistency:** `InductionScene`, `InductionCase`, `InductionKey`, `InductionPrepared`, `InductionSampling`, `InductionPointSample`, `prepare_induction`, `sample_induction`, `evaluate_induction_point` and `induction_key` are used consistently across tasks. `FieldMode::Induction`, `State::induction`, `State::induction_prepared`, `State::induction_sampling`, `set_induction_scene` and `refresh_induction` are named once and reused.

**Review Focus:** Sign/axis (Task 1), stale cache (Task 2), accuracy hiding (Tasks 1–2), clock edge cases (Task 2), export truthfulness (Task 4). Each has a pinned test in its owning task.

**Proportion:** The plan is longer than the design only because it carries executable test contracts and exact paths; algorithm bodies are left to the implementation.
