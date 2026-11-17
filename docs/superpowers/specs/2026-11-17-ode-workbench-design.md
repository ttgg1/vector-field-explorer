# ODE Workbench — Design

- **Status:** APPROVED by owner on 2026-11-17
- **Date:** 2026-11-17
- **Issue:** [#28](https://github.com/ttgg1/vector-field-explorer/issues/28)
- **Epic:** [#17](https://github.com/ttgg1/vector-field-explorer/issues/17)
- **Dependencies:** #12 scalar plotting and #16 scalar animation are merged;
  #24/#25 source mode, #26 source visualization, and #27 induction are merged.
  #18 performance evidence is not a dependency of this design.

## Intent

Add a dedicated ODE workbench that makes a small dynamical-system teaching
experience first-class: type `dx/dt`, `dy/dt`, `dz/dt` or integrate the current
formula field, choose bounded initial conditions, and watch trajectories, a
phase portrait, and per-variable time series stay coherent on the existing
simulation clock.

This is not a general solver or a second simulation engine. It is a precomputed
teaching trajectory viewer using a fixed-step RK4 method, the shared
`SimClock`, the existing `SceneGPU` line/dot paint boundary, and plain CPU data
for the panel charts.

## Goals

- A fourth *concept mode* beside Formula, Sources, and Induction: `Ode`.
- Typed 2D systems and the current formula field as trajectory sources.
- Up to eight initial conditions, each visually distinguishable.
- Full trajectory, deterministic replay, current-time markers, 2D phase
  portrait, and time-series charts.
- One shared `SimClock` for play/pause/step/reset; no private timer.
- Bounded, explicit failure behavior for invalid equations, non-finite
  integration, and step/storage limits.
- Persist the scene and seed configuration; reset it with global reset.
- Reuse camera, pan/zoom, PNG/SVG, and warning/provenance seams.
- Keep the existing particle RK4 implementation untouched.

## Non-goals

- No stiff/adaptive solver chooser, step-size controller, or implicit method.
- No Poincare sections, Lyapunov exponents, bifurcations, or parameter sweeps.
- No 3D phase-portrait cube; phase portraits are 2D projections.
- No symbolic solve/integrate and no CAS dependency for ODE integration.
- No GPU solver, second clock, private playback timer, or CLI extension.
- No formula preset conversion and no per-geometry issue split.
- No changes to particle spawning, advection, or its RK4 behavior.
- No 3D phase-portrait expansion, arbitrary coordinate transforms, or network
  export.

## Chosen approach

**Approach A — dedicated Ode mode with a precomputed CPU solution.**

A separate `FieldMode::Ode` keeps the workbench out of Formula/scalar state and
from polluting field sampling. A new pure `src/ode.rs` owns scene validation,
RHS preparation, fixed-step RK4, bounds, status, and deterministic solution
data. `State` holds the durable scene and an `Arc<OdeSolution>`; the app turns
the solution into line/dot instances for the existing vector viewport.

Rejected alternatives:

- **Formula-generated workbench:** would make typed systems compete with field
  formulas, force encoding initial conditions into strings, and lose a clean
  place for the phase/time charts.
- **Extend scalar plotting:** scalar plots are functions of one or two spatial
  variables, not trajectories with initial conditions and a shared time axis.
  Reusing their renderer would also mix two camera/generation models.

### Vector viewport invariant

Ode is a **vector-viewport concept**. Entering Ode ensures
`plot.kind == PlotKind::VectorField` while preserving the user's current
`mode_3d` and camera. Scalar curve/surface drafts remain untouched; leaving Ode
returns to the vector viewport, not to a previously displayed scalar plot.
`is_vector_mode()` therefore stays true in Ode, `SceneGPU` keeps its single
vector cache path, and `set_time` has an `Ode` arm that only mirrors time. It
never re-integrates or dirties the solution. Restoring a saved Ode session also
ensures the vector plot kind without solving synchronously.

### Why RK4

RK4 is already the conceptual integrator in the particle path, is deterministic
without an external dependency, and is adequate for the bounded teaching cases.
The workbench uses its own pure implementation so particle behavior remains
frozen and independently regression-tested. Error is established against
independent analytic references; it is not claimed as a general accuracy
guarantee.

## Model and API

New module `src/ode.rs`, exported as `pub mod ode` from `src/lib.rs`.

```rust
pub const ODE_STEP: f64 = 0.005;
/// Hard cap on stored samples per trajectory, including the `t = 0` sample.
pub const ODE_MAX_SAMPLES: usize = 4096;
pub const ODE_MAX_DURATION: f64 = 25.0;
pub const ODE_DEFAULT_DURATION: f64 = 20.0;
pub const ODE_MAX_SEEDS: usize = 8;
pub const ODE_MIN_DIMENSION: usize = 2;
pub const ODE_MAX_DIMENSION: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum OdeSource { Equations, CurrentField }

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OdeSeed {
    pub position: [f64; 3],
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OdeScene {
    pub source: OdeSource,
    pub dimension: usize,
    pub equations: [String; 3],
    pub seeds: Vec<OdeSeed>,
    pub duration: f64,
}

impl Default for OdeScene {
    fn default() -> Self {
        Self {
            source: OdeSource::Equations,
            dimension: 2,
            equations: ["y".into(), "-x".into(), "0".into()],
            seeds: vec![OdeSeed { position: [1.0, 0.0, 0.0] }],
            duration: ODE_DEFAULT_DURATION,
        }
    }
}
```

`OdeScene::validate()` rejects non-finite/empty seeds, more than eight seeds,
dimensions outside `2..=3`, non-finite or out-of-range duration, and (for
`Equations`) unparseable or unbound components. For `CurrentField`, equations
are ignored; the field dimension and durable formula domain determine the RHS.

### Solution types

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OdeStop { Complete, StepLimit, NonFinite }

#[derive(Debug, Clone, PartialEq)]
pub struct OdeSample { pub time: f64, pub position: [f64; 3] }

#[derive(Debug, Clone, PartialEq)]
pub struct OdeTrajectory {
    pub seed_index: usize,
    pub points: Vec<OdeSample>,
    pub stop: OdeStop,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OdeBounds { pub min: [f64; 3], pub max: [f64; 3] }

#[derive(Debug, Clone, PartialEq)]
pub struct OdeSolution {
    pub dimension: usize,
    pub step: f64,
    pub duration: f64,
    pub trajectories: Vec<OdeTrajectory>,
    pub bounds: OdeBounds,
    pub stopped: usize,
}
```

`OdeSolution::markers_at(time: f64) -> Vec<[f64; 3]>` returns one interpolated
position per trajectory at the clamped playback time. `OdeSolution::view_domain()
-> Domain` converts padded bounds to a camera domain (`z` gets a small symmetric
extent when flat so 3D cameras have a usable box). `OdeSolution::sample_at` is
exposed for tests and deterministic replay checks.

### Visual constants

ODE trajectories use a fixed eight-colour palette so seeds stay distinguishable
without the shared vector colormap:

```rust
pub const ODE_COLORS: [[f32; 4]; 8] = [
    [0.95, 0.35, 0.25, 1.0],
    [0.25, 0.75, 1.00, 1.0],
    [0.50, 0.90, 0.35, 1.0],
    [1.00, 0.75, 0.20, 1.0],
    [0.70, 0.45, 1.00, 1.0],
    [1.00, 0.45, 0.75, 1.0],
    [0.35, 0.90, 0.85, 1.0],
    [0.80, 0.80, 0.80, 1.0],
];
```

Trajectory line width is `0.006 * max(view_domain_extent, 1e-6)` world units;
current-time marker radius is `0.012 * max(view_domain_extent, 1e-6)`. Axes use
the existing `AXIS_X_COLOR`/`AXIS_Y_COLOR`/`AXIS_Z_COLOR` constants and the same
world-width convention as reference lines.

### Preparation and integration

```rust
pub enum OdeError {
    InvalidScene(String),
    Parse { component: usize, message: String },
    EmptySeeds,
    NonFiniteSeed(usize),
    UnsupportedField(String),
}

pub struct OdePrepared { /* dimension, duration, seed data, RHS */ }

pub fn prepare_ode(state: &State, scene: &OdeScene) -> Result<OdePrepared, OdeError>;
pub fn integrate_ode(prepared: &OdePrepared) -> OdeSolution;
pub fn solve_ode(state: &State, scene: &OdeScene) -> Result<OdeSolution, OdeError>;

#[doc(hidden)]
pub fn integrate_ode_with_step(
    prepared: &OdePrepared,
    step: f64,
    max_steps: usize,
) -> OdeSolution;
```

`OdePrepared` owns no borrows. For `Equations`, it stores a `FieldExpr` parsed
from the first `dimension` equation strings; the same meval parser and `x`, `y`,
`z`, `t` binding used by Formula fields are the only expression stack. For
`CurrentField`, it stores a clone of the durable `FieldExpr`, the durable
`state.domain`, and the field dimension. Add
`FieldExpr::eval3_at_f64(point: DVec3, time: f64) -> Option<Vec3>` as the
integration boundary; the existing `eval3_at(Vec3, f32)` delegates to it after
casting once, so Formula/sampling behavior is unchanged. The solver evaluates at
the integration time `τ` in f64, not at playback state, so solutions are
precomputed once and replay deterministically.

`integrate_ode_with_step` is the evidence seam: Task 1 uses it with `ODE_STEP`
and `ODE_STEP/2` to demonstrate convergence, while the production path always
uses `ODE_STEP`.

### Current-field semantics

`CurrentField` integrates the durable formula field in **world coordinates**
using `state.domain`; it does not use a `PlaneFrame` projection. This avoids
hijacking #13's selected-plane particle/streamline contract. The ODE viewport
is a projection of the trajectory in the current 2D/3D camera. Formula
expressions may use `t` as the ODE integration time. If the field is
two-component, `z` remains zero.

`CurrentField` requires a valid parsed field **and** an empty
`State::parse_error`. While the formula draft is broken, the last valid field
still exists internally, but Ode must not keep integrating it as if the edit
succeeded: the panel/status reports the field parse error, the retained ODE
solution stays visible, and `ode_dirty` clears without a new solve. A successful
parse clears `parse_error` and the next Ode frame re-solves. This never
fabricates trajectories or silently converts the draft.

### Integration semantics

- Each seed begins at `τ = 0`, with the first `dimension` coordinates used.
- RK4 uses the fixed `ODE_STEP`; time advances as `n * step`, not by the clock.
- Integration stops at `duration` (Complete), at the sample cap (StepLimit), or
  at the last finite sample when the RHS/state becomes non-finite (NonFinite).
- The cap is on **stored samples per trajectory**, including `t = 0`. A
  trajectory may store at most `ODE_MAX_SAMPLES`; the next would-be sample ends
  it with `StepLimit`. Default duration 20 at `h = 0.005` stores 4001 samples
  and completes; the allowed maximum duration 25 intentionally exercises
  StepLimit and is covered by an explicit test, not the default.
- A trajectory with a non-finite initial state is rejected at validation, not
  drawn from a fake origin.
- A non-finite later state stops that trajectory. Other seeds continue.
- Bounds include only finite samples. If no finite sample exists, `solve_ode`
  returns `InvalidScene("no finite trajectory samples")`.
- Bounds are padded by 5% of the largest extent, with at least `1e-3` on every
  axis. The camera never fits to a zero-size box.

### Error and status policy

`OdeError` derives `Debug, Clone, PartialEq` and implements `Display`; it is shown in the panel and status bar. `OdeSolution.stopped`
counts trajectories whose stop is not `Complete`. The panel labels a stopped
trajectory (`Step limit reached`, `Stopped: non-finite value`) and does not draw
a connecting segment past the last finite sample. No inf/NaN reaches
`SceneGPU`.

## State integration

`State` gains:

```rust
pub ode: OdeScene,
pub ode_solution: Option<std::sync::Arc<OdeSolution>>,
pub ode_error: Option<String>,
pub ode_epoch: u64,
ode_dirty: bool,
```

- `State::active_dimension()` returns the ODE solution dimension when in Ode
  (or the selected scene dimension when no solution yet), and never changes
  `mode_3d`.
- `State::is_plane_slice()` and `State::plane_frame()` return false/None in
  Ode. ODE has no field slice.
- `State::scene_domain()` returns the solution's padded view domain in Ode,
  falling back to the durable domain when no solution exists.
- `State::scene_flow()` is not used in Ode. The app never samples the field
  there.
- `State::particle_target()` returns `None` in Ode. The existing mode-switch
  transient rule clears the particle cloud when entering Ode; no ODE spawn,
  advection, or click is introduced. Particle RK4 itself is untouched.
- `State::has_time_dependent_consumer()` returns true in Ode while a solution
  or a pending solution exists. This shows the shared transport but does not
  introduce a timer.
- `set_field_mode(Ode)` preserves `mode_3d`, formula/source/induction state, and
  camera; ensures the vector plot kind; clears source/induction caches and
  notices; synchronizes the plane if the active dimension changed; ensures an
  ODE solution lazily; marks dirty. It never integrates synchronously during
  persistence restore.
- `set_field_mode(Ode)` also clears any shared `loop_period` (ODE uses duration,
  not looping) before the first clock frame. Returning to another mode leaves
  the loop control as the user set it.
- `State::set_ode_scene(scene) -> Result<(), OdeError>` validates/integrates
  before replacing the durable scene. On success it stores the solution, clears
  `ode_error`, increments `ode_epoch`, marks dirty, and clears its own
  `ode_dirty` flag so the app does not solve the same scene twice. On failure it retains the
  last valid scene/solution and sets `ode_error`.
- `State::refresh_ode_if_dirty()` recomputes only when `ode_dirty` is set. It
  retains the last valid solution on error and clears the flag.
- `State::reset_ode()` restores `OdeScene::default()`, solves it, clears error,
  increments epoch, and marks dirty. Global reset calls the existing reset
  paths and then `reset_ode()`.
- `mark_dirty()` sets `ode_dirty` when in Ode with `CurrentField`, because the
  durable formula/domain may have changed. Typed-equation solutions are not
  invalidated by unrelated field/color edits.

`set_time` behavior in Ode is limited to mirroring the shared clock time; it
does not re-integrate. `set_time` must not dirty the ODE solution.

## App integration

`VfApp` gains:

```rust
ode_lines: Arc<Vec<LineInstanceGPU>>,
last_ode_epoch: u64,
```

`sync_field_mode` clears formula transients and particle targets on mode change
as it already does; ODE follows that same transient-clear rule. Entering Ode also
calls `state.refresh_ode_if_dirty()` so the scene is ready before the first paint.
The first successful Ode solution after mode entry auto-fits the vector camera
to `scene_domain()` once; later epochs and edits preserve the camera. `Fit`
remains the explicit way to reframe after edits.

### Solve cadence and clock ownership

Panel edits update drafts only. `OdePanelState::commit` runs when a text field
loses focus/Enter, a DragValue is released, or add/remove/reset occurs; it never
runs per frame during a drag. One commit produces at most one solve and one
`ode_epoch` bump. `Duration` and seed edits therefore cannot freeze the UI at the
~2 s worst case.

Ode owns no timer and no duration accumulator. On entry it clears
`SimClock::loop_period`. The app advances the shared clock normally, then calls
a new `SimClock::clamp_time(max)` (clamps the stored time and returns whether the
end was reached). If reached while playing, the app pauses once and mirrors the
clamped time through `State::set_time` so clock and state cannot drift. `Step`
uses `SimClock::seek`.

`advance_simulation` branches for Ode before scalar/vector field handling:

1. `sync_field_mode()`
2. `state.refresh_ode_if_dirty()`
3. If `ode_epoch` changed, `clock.reset_time()` (preserve playing/speed) and
   record the new epoch.
4. `clock.clamp_time(solution.duration)`; if it reports the end while playing,
   pause once and mirror the clamped time through `state.set_time`.
5. `rebuild_scene()` if dirty (trajectory lines + axes).
6. Recompute current markers through `assemble_viewport_scene`.

`rebuild_scene` in Ode:

- clears flow/click/streamline state;
- builds trajectory lines from the solution with the fixed seed palette;
- appends 2D x/y axes or 3D x/y/z axes controlled by
  `show_coordinate_system`; no reference grid;
- leaves arrows/trails/dots empty in the cached scene;
- clears `source_glyph_line_count`;
- clears the dirty flag.

`assemble_viewport_scene` in Ode adds current-time markers as `ArrowInstanceGPU`
dots for every seed. In 3D they join `combined` and are sorted far-to-near with
the existing dot path; in 2D they are appended to the cached `dots` vector, which
is the 2D dot path that already paints after arrows. Markers are recomputed per
frame without rebuilding trajectory lines and are never written into the line
buffer.

`State::set_mode` or ODE scene edits that produce a new `ode_epoch` reset the
shared clock to time 0. Returning to Ode without a new epoch preserves the
current playback time.

### Step action

The Induction/Sources transport has no manual step button. The ODE panel adds a
`Step` action that:

- pauses the shared clock;
- seeks to `min(time + ODE_STEP, solution.duration)`;
- mirrors the time through `state.set_time`;
- requests one repaint.

Add `SimClock::seek(time)` (finite non-negative, clamps at zero, clears the
per-frame step) for this. No private timer or step accumulator is introduced.

## Resorting and camera

ODE reuses `ViewCommand::Fit` and `ViewContext` with `scene_domain()`. 2D uses
the existing front view and X/Y pan/zoom; 3D uses the existing orbit camera.
Reset restores the default ODE camera target and zoom; `Fit` frames the
solution's padded bounds.

`FieldMode::Ode` is excluded from plane controls and from field domain controls:
the panel note explains that the viewport fits trajectory bounds and that the
durable field domain still governs a `CurrentField` RHS. Features (particles,
streamlines, clicks) are disabled with values preserved; Coloring is hidden
because trajectories use the fixed palette; Camera and coordinate-system toggle
remain available. CAS is disabled outside Formula with the existing shared
message.

## UI and panel

New `src/ui/ode_panel.rs`. The mode selector in `src/ui/formula_bar.rs` gains a
fourth option `ODE`; in Ode it hides the formula editor and shows
`ODE scene summary` plus `Open ODE`.

Panel state:

```rust
pub struct OdePanelState {
    pub draft: OdeScene,
    pub error: Option<String>,
    pub advanced: bool,
    pub chart: OdeChart,
}

pub enum OdeChart { Phase, TimeSeries }
```

`OdePanelState::commit(&mut State) -> bool` validates the draft, calls
`State::set_ode_scene`, syncs the draft back on success, and exposes the error
on failure. Invalid drafts never replace the last valid scene.

Responsive placement:

- Wide: left rail `ODE` with width `DEFAULT_ODE_WIDTH = 300`, collapsible.
- Compact: `SheetState::Ode` tab; same controls reflow vertically.
- The panel exists only in Ode; Formula/Sources/Induction rails are unchanged.

Controls:

- Source: `Equations` / `Current field`.
- Dimension: `2D` / `3D` (typed equations only).
- Equation rows `dx/dt =`, `dy/dt =`, `dz/dt =` (z only for 3D), with
  variables `x`, `y`, `z` and `t`; inline parse/unknown-variable error.
- Initial condition rows for each seed, add/remove up to eight, plus a
  `Reset initial conditions` action.
- `Duration` DragValue in `0.1..=25.0`.
- Display: `Phase portrait` / `Time series`; time-series variable checkboxes
  for `x`, `y`, `z` (at least one selected).
- `Step`, `Fit`, `Reset ODE section`.
- `Advanced` (closed by default): solver text `Fixed-step RK4, h = 0.005`,
  sample/step counts, stop status, and the current field dimension when using
  `CurrentField`.

Charts are CPU-painted inside the panel:

- Phase portrait: selected 2D projection (x/y default; x/z or y/z in 3D) from
  the same `OdeSolution` points, fixed seed colors, seed markers, and the
  current-time marker.
- Time series: `t` horizontal, selected variables vertical, same points and
  colors. No interpolation for values, no separate solver.

When the ODE solution is invalid or absent, the panel shows the error and
disables chart painting; the viewport shows axes only, never fabricated lines.

## Persistence

`ViewRecord` gains serde-default fields:

```text
ode_source: "equations" | "current_field"   (default "equations")
ode_dimension: u8                           (default 2)
ode_equations: [String; 3]                  (default y, -x, 0)
ode_seeds: Vec<[f64; 3]>                    (default [[1,0,0]])
ode_duration: f64                           (default 20)
ode_chart: "phase" | "time_series"          (default "phase")
ode_variables: [bool; 3]                    (default [true,true,false])
```

Restore validates through `OdeScene::validate`; malformed ODE fields fall back
to `OdeScene::default()` and set `ode_error`, without blocking the rest of the
view. Restore never calls the solver synchronously: it restores the scene,
sets `ode_dirty`, and the first Ode frame/`set_field_mode(Ode)` solves lazily.
Malformed fallback follows the same lazy path. Legacy payloads leave Ode
unselected and produce no error. `field_mode` adds `ode` only when restored
explicitly. Global reset restores the default ODE scene, clears its
solution/error, and returns to Formula.

## Export

`src/export_provenance.rs` gains ODE provenance and `ExportProvenance` gains
`ode_mode: bool`. `ode_export_provenance(mode, Option<&OdeSolution>, timeline)
-> Option<ExportProvenance>`:

- `None` outside Ode.
- `"ODE workbench - fixed-step RK4 - h = {step}"` always in Ode.
- `"ODE solution unavailable"` when no solution.
- `"ODE invalid: {error}"` when the retained solution is unavailable due to an
  invalid scene.
- `"ODE stopped: {n} trajectory(s)"` when any trajectory is not Complete.
- Metadata keys: `vfe.model=ode-fixed-rk4`, `vfe.ode_step`, `vfe.ode_duration`,
  `vfe.ode_seeds`, `vfe.ode_stopped`, and `vfe.ode_status` (one of
  `complete|step_limit|non_finite`).

PNG uses the existing baked banner/tEXt path; SVG uses visible text and
`<metadata>`. ODE provenance is captured at export-request time, like Sources
and Induction. No warning is suppressible by any opt-in; no non-finite data is
exported.

## Tests and evidence

### Pure model (`tests/ode_test.rs`)

- `harmonic_oscillator_matches_analytic_trajectory`: `dx/dt=y`, `dy/dt=-x`,
  seed (1,0), duration 20; compare positions and velocities at selected times
  against `cos/sin`; max error <= `1e-5`.
- `damped_oscillator_matches_analytic_solution`: `x'' + 0.2 x' + x = 0`
  with an independent closed-form reference; max error <= `1e-5`.
- `rk4_converges_at_fourth_order`: use `integrate_ode_with_step` at `h` and
  `h/2`; observed error ratio is at least 8 and the fine error is strictly
  smaller.
- `deterministic_seeds_produce_identical_solutions`: solve twice; equality.
- `multiple_initial_conditions_remain_distinguishable`: eight seeds, finite
  bounds, no merged lines.
- `invalid_equations_and_seeds_are_rejected`: unknown variable, non-finite
  seed, empty seed set, too many seeds, bad duration.
- `nonfinite_evolution_stops_without_a_fake_segment`: a test RHS that becomes
  non-finite; last finite point retained, status NonFinite.
- `step_limit_is_explicit_and_bounded`: duration 25; at most 4096 stored
  samples, StepLimit, no panic.
- `current_field_and_typed_rhs_agree`: enter from a formula with
  `(y, -x)`; typed and current-field trajectories agree at sampled times, and
  both use the f64 evaluation boundary.
- `ode_does_not_change_particle_rk4`: existing particle regression suite
  remains unchanged; a smoke test confirms `Particles::advect` still follows
  its fixture.

### State/app (`tests/ode_app_test.rs`)

- entering/leaving Ode preserves formula, sources, induction, projection, and
  camera; the particle cloud follows the existing transient-clear rule and no
  ODE particle path exists;
- `ode_epoch` changes on valid scene edit/reset and the app resets clock time,
  preserving playing/speed;
- invalid edit retains the last valid solution and reports the error;
- play/pause/step/reset under one `SimClock`; step advances exactly one solver
  step and pauses; end-of-trajectory auto-pauses at duration; entering Ode
  clears a lingering loop period and clock/state time stays mirrored;
- `CurrentField` recalculation after a formula/domain edit; a broken formula
  draft (`parse_error`) keeps the last valid solution and does not re-solve the
  stale field;
- only ODE lines + axes + current markers reach `SceneGPU`; markers appear on
  both the 2D `dots` path and the 3D combined path; no field arrows, particles,
  streamlines, clicks, source glyphs, or scalar plot;
- scalar mode switch pauses ODE and returning reuses the retained solution;
- projection preservation (2D stays 2D, 3D stays 3D);
- first entry auto-fits padded trajectory bounds once; later epochs preserve
  the camera; Fit reframes and reset restores the default camera.

### UI (`tests/ode_panel_test.rs`)

Real egui/accesskit widget tests at 1200 pt and 390 pt:

- `ode_controls_wide_and_compact_reflow`;
- `typed_edits_and_seeds_update_the_real_state`;
- `invalid_equation_shows_inline_error_and_keeps_last_solution`;
- `phase_and_time_series_charts_use_the_same_solution`;
- `step_fit_and_reset_actions_use_the_shared_paths`;
- `features_and_cas_are_disabled_with_explanations`;
- `help_documents_solver_assumptions_and_limits`.

### Persistence/export (`tests/ode_persistence_test.rs`,
`tests/ode_export_test.rs`)

- all fields round-trip, including eight seeds and chart selection;
- legacy JSON gets sentinel defaults with no error;
- malformed ODE fields fall back to the default ODE scene and set an error;
- restoring Ode does not solve synchronously; the first Ode frame solves the
  restored or fallback scene and sets the vector plot kind;
- global reset clears the ODE scene/solution/error and returns Formula;
- decoded PNG banner/tEXt and SVG text/metadata include the RK4 model, step,
  duration, seeds, and stop status when stopped;
- non-finite samples never appear in decoded output; no opt-in can suppress the
  warning.

### Performance/evidence

Task 1 records solve time and memory for eight seeds at both 2D and 3D cap,
including a near-step-limit case. Task 5 reruns an integrated benchmark and
documents host, caps, and the fact that this is a precomputed trajectory solve,
not a 33 ms frame claim. If solve time exceeds 2 s at the default scene, reduce
`ODE_MAX_SAMPLES`/duration or revise the design before UI tasks.

## Review focus

- A time-dependent field used by `CurrentField` is evaluated at the ODE
  integration time, not the playback clock; phase/time charts must not disagree
  with the viewport.
- A non-finite point in one seed must stop only that seed; no fake connecting
  segment and no NaN in `LineInstanceGPU`.
- Clock reset on a new `ode_epoch` must preserve play/pause and speed, and must
  not reset when simply returning to Ode.
- `ODE_MAX_SAMPLES` exhaustion must be a visible StepLimit, not silent truncation
  presented as Complete.
- Ode must not enter any plane/source/induction sampler or particle target;
  `scene_domain` must never produce a plane-local domain for ODE.
- Zero-length extents and a single trajectory must still produce a valid camera
  domain.

## Delivery shape

Five independently reviewable tasks:

1. `src/ode.rs`, solution/integration, independent references, bounds/status,
   tests/benchmark.
2. `FieldMode::Ode`, State cache/epoch/clock integration, app scene/line/marker
   path, test seams.
3. responsive ODE panel, formula-bar selector, help, charts.
4. persistence/migration/reset and PNG/SVG provenance.
5. docs, integrated benchmark, gates, regression review.

No implementation is authorized until this design and a written plan are
approved by the owner.
