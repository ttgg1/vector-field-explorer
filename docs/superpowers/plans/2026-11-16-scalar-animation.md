# Scalar Plot Animation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Animate time-dependent scalar curves and surfaces from the existing shared simulation clock without rebuilding static plots or introducing a second time source.

**Architecture:** `ScalarExpr` gains an explicit time-dependent binding and `eval_at`. Sampling gains `_at` variants while all existing public wrappers keep `t = 0` behavior. `State::set_time` invalidates only an accepted, error-free time-dependent scalar draft; `plot_scene` resamples only when its cached draft time differs. `SimClock` and the transport consume the clock for an active time-dependent scalar just as they do for particles or a time-dependent vector field.

**Tech Stack:** Rust 2024, eframe/egui, `meval` 0.2, glam, existing `SimClock`, existing scalar plot sampling and `PlotScene` cache.

**Spec:** `docs/superpowers/specs/2026-11-16-scalar-animation-design.md`

**Sub-issues:** #48 (Task 1), #49 (Task 2, depends on #48), #50 (Task 3, depends on #49).

## Global Constraints

- `t` is the only new free variable. Curves bind `x` or `x, t`; surfaces bind `x, y` or `x, y, t`. `t(...)` remains a function call and is not the variable.
- If both the static and time-aware bind fail, report the original static-binding error and its existing allowed-variable text (`x` or `x, y`).
- One `SimClock` remains the only time source. No second timer, transport row, shortcut, persisted clock state, CSV animation flag, or CLI animation flag.
- Static scalar plots must not resample, bump plot generation, mark dirty, request repaint, or advance the clock merely because another consumer is playing.
- Invalid edits retain the last accepted expression and last sampled mesh. The parse error is visible; the clock and transport may continue, but the mesh must not resample until a valid source is accepted.
- Kind or mode switches sample at the current `State::time`, never at zero.
- `f32` clock time is converted to `f64` once at the sampling boundary. Existing t=0 public wrappers stay source-compatible for CLI, tests, and preview code.
- No scalar physics, solver, units, interpolation, or new dependency.

## Review Focus

- A time-dependent expression that becomes invalid only at some `t`: sampling must leave holes or absent geometry, never panic, invent zeroes, or keep stale non-finite vertices.
- Invalid edit while playing: the last mesh and `sampled_at` must stay fixed, the clock must keep advancing, and the error must remain visible without generation churn every frame.
- Static scalar while a vector or particle consumer plays: no scalar generation bump, no resample, and no extra repaint request attributable to the static plot.
- Loop wrap and transport visibility during an invalid edit of a previously time-dependent expression; pin the exact wrapped `t` and that transport stays visible.
- Kind switch at nonzero `t`: a curve or surface must sample at the current clock time rather than resetting to zero.

---

### Task 1: Time-aware scalar expressions and sampling

**Files:**
- Modify: `src/scalar.rs`
- Modify: `src/plot.rs`
- Modify: `tests/scalar_test.rs`
- Create: `tests/scalar_animation_test.rs`
- Modify: `tests/suite.rs`

**Interfaces:**
- Consumes: existing `ScalarExpr`, `ScalarContext`, `Domain`, `sample_curve`, `curve_edges`, `sample_surface`.
- Produces:
  - `ScalarExpr::is_time_dependent(&self) -> bool`
  - `ScalarExpr::eval(&self, x: f64, y: f64) -> Option<f64>` remains the `t = 0` compatibility method.
  - `ScalarExpr::eval_at(&self, x: f64, y: f64, t: f64) -> Option<f64>`
  - `plot::sample_curve_at(expr: &ScalarExpr, domain: &Domain, samples: usize, time: f32) -> Vec<Option<Vec3>>`
  - `plot::sample_curve_with_diagnostics_at(expr: &ScalarExpr, domain: &Domain, samples: usize, time: f32) -> (Vec<Option<Vec3>>, SampleDiagnostics)`
  - `plot::curve_edges_at(expr: &ScalarExpr, domain: &Domain, slots: &[Option<Vec3>], time: f32) -> CurveEdges`
  - `plot::sample_surface_at(expr: &ScalarExpr, domain: &Domain, resolution: UVec2, time: f32) -> SurfaceMesh`
  - Existing `sample_curve`, `sample_curve_with_diagnostics`, `curve_edges`, and `sample_surface` call the `_at` functions with `0.0`.

- [ ] **Step 1: Write the failing expression and sampling tests**

Create `tests/scalar_animation_test.rs`:

```rust
use glam::{UVec2, Vec3};
use vector_field_explorer::{
    domain::Domain,
    plot::{curve_edges_at, sample_curve, sample_curve_at, sample_surface, sample_surface_at},
    scalar::{ScalarContext, ScalarExpr},
};

fn domain() -> Domain {
    Domain::new(Vec3::new(-1.0, -1.0, 0.0), Vec3::new(1.0, 1.0, 0.0))
}

fn close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 1e-6,
        "expected {expected}, got {actual}"
    );
}

#[test]
fn scalar_expr_tracks_time_only_when_it_uses_t() {
    let curve = ScalarExpr::parse("sin(t)*x", ScalarContext::Curve).unwrap();
    assert!(curve.is_time_dependent());
    close(
        curve
            .eval_at(1.0, 0.0, std::f64::consts::FRAC_PI_2)
            .unwrap() as f32,
        1.0,
    );
    assert_eq!(curve.eval(1.0, 0.0), Some(0.0));

    let still = ScalarExpr::parse("x^2", ScalarContext::Curve).unwrap();
    assert!(!still.is_time_dependent());
    assert_eq!(still.eval_at(2.0, 0.0, 9.0), still.eval(2.0, 0.0));

    let surface = ScalarExpr::parse("x+y+t", ScalarContext::Surface).unwrap();
    assert!(surface.is_time_dependent());
    assert_eq!(surface.eval_at(1.0, 2.0, 3.0), Some(6.0));
}

#[test]
fn curve_samples_move_with_time_and_static_samples_do_not() {
    let domain = domain();
    let expr = ScalarExpr::parse("sin(t)*x", ScalarContext::Curve).unwrap();
    let at_zero = sample_curve_at(&expr, &domain, 2, 0.0);
    let at_peak = sample_curve_at(
        &expr,
        &domain,
        2,
        std::f32::consts::FRAC_PI_2,
    );
    close(at_zero[0].unwrap().y, 0.0);
    close(at_zero[1].unwrap().y, 0.0);
    close(at_peak[0].unwrap().y, -1.0);
    close(at_peak[1].unwrap().y, 1.0);

    let stationary = ScalarExpr::parse("x", ScalarContext::Curve).unwrap();
    let first = sample_curve_at(&stationary, &domain, 5, 0.0);
    let later = sample_curve_at(&stationary, &domain, 5, 7.0);
    assert_eq!(first, later);
    assert_eq!(sample_curve(&stationary, &domain, 5), first);
}

#[test]
fn surface_samples_move_with_time_and_static_samples_do_not() {
    let domain = domain();
    let expr = ScalarExpr::parse("t", ScalarContext::Surface).unwrap();
    let at_zero = sample_surface_at(&expr, &domain, UVec2::splat(2), 0.0);
    let at_one = sample_surface_at(&expr, &domain, UVec2::splat(2), 1.0);
    assert_eq!(at_zero.positions.len(), 4);
    for point in &at_zero.positions {
        close(point.z, 0.0);
    }
    for point in &at_one.positions {
        close(point.z, 1.0);
    }

    let stationary = ScalarExpr::parse("x+y", ScalarContext::Surface).unwrap();
    assert_eq!(
        sample_surface(&stationary, &domain, UVec2::splat(3)),
        sample_surface_at(&stationary, &domain, UVec2::splat(3), 4.0)
    );
}

#[test]
fn curve_edge_probes_use_the_same_time() {
    let domain = domain();
    let expr = ScalarExpr::parse("1/(x-t)", ScalarContext::Curve).unwrap();
    let slots = vec![
        Some(Vec3::new(-1.0, 0.0, 0.0)),
        Some(Vec3::new(0.0, 0.0, 0.0)),
        Some(Vec3::new(1.0, 0.0, 0.0)),
    ];
    let at_zero = curve_edges_at(&expr, &domain, &slots, 0.0);
    let at_half = curve_edges_at(&expr, &domain, &slots, 0.5);
    assert_eq!(at_zero.diagnostics.rejected_edges, 0);
    assert!(
        at_half.diagnostics.rejected_edges > 0,
        "the midpoint probe at x = 0.5 must see the pole when t = 0.5"
    );
}
```

Update the existing invalid-source list in `tests/scalar_test.rs`: remove `"t"` from `[ "z", "t", "mystery", "foo(x)", "y=x", "x+" ]` and from the symbol loop. Keep the remaining invalid cases and assertions unchanged.

Register the new module in `tests/suite.rs` next to the other scalar test modules:

```rust
mod scalar_animation_test;
```

- [ ] **Step 2: Run the new tests to verify they fail**

Run:

```bash
export PATH=/home/paul/.cargo/bin:/usr/bin:/bin
export RUSTC=/usr/bin/rustc
export CARGO_TARGET_DIR=/home/paul/.cache/vector-field-explorer-targets/issue-16
export TMPDIR=/home/paul/.cache/vector-field-explorer-targets/issue-16/tmp
cargo test --test suite scalar_animation_test:: -- --nocapture
```

Expected: compile failure naming `is_time_dependent`, `eval_at`, `sample_curve_at`, `curve_edges_at`, and `sample_surface_at` as missing. Record the log under `.superpowers/sdd/2026-11-16-scalar-animation/evidence/task-1/`.

- [ ] **Step 3: Implement time-aware `ScalarExpr`**

In `src/scalar.rs`:

```rust
#[derive(Clone, Debug)]
pub struct ScalarExpr {
    expr: meval::Expr,
    time_dependent: bool,
}

impl ScalarExpr {
    pub fn parse(source: &str, context: ScalarContext) -> Result<Self, ScalarError> {
        let error = |cause| ScalarError {
            source: source.to_owned(),
            context,
            cause,
        };
        let expr = source.parse::<meval::Expr>().map_err(error)?;
        let static_bind = match context {
            ScalarContext::Curve => expr.clone().bind("x").map(|_| ()),
            ScalarContext::Surface => expr.clone().bind2("x", "y").map(|_| ()),
        };
        if static_bind.is_ok() {
            return Ok(Self {
                expr,
                time_dependent: false,
            });
        }
        let static_error = static_bind.expect_err("checked immediately above");
        let time_bind = match context {
            ScalarContext::Curve => expr.clone().bind2("x", "t").map(|_| ()),
            ScalarContext::Surface => expr.clone().bind3("x", "y", "t").map(|_| ()),
        };
        time_bind.map_err(|_| error(static_error))?;
        Ok(Self {
            expr,
            time_dependent: true,
        })
    }

    pub fn is_time_dependent(&self) -> bool {
        self.time_dependent
    }

    /// Compatibility evaluation at `t = 0`.
    pub fn eval(&self, x: f64, y: f64) -> Option<f64> {
        self.eval_at(x, y, 0.0)
    }

    /// Return only finite f64 heights at the supplied simulated time.
    pub fn eval_at(&self, x: f64, y: f64, t: f64) -> Option<f64> {
        let value = self
            .expr
            .eval_with_context((
                [("x", x), ("y", y), ("t", t)],
                meval::Context::new(),
            ))
            .ok()?;
        value.is_finite().then_some(value)
    }
}
```

The static bind is attempted first on purpose: an expression that binds without `t` is static, while `sin(t)*x` fails that bind and then succeeds with `t`. If the time-aware bind also fails, return the original static error so `y` in a curve still reports the existing allowed variables.

- [ ] **Step 4: Add `_at` sampling variants and keep t=0 wrappers**

In `src/plot.rs`:

```rust
pub fn sample_surface(expr: &ScalarExpr, domain: &Domain, resolution: UVec2) -> SurfaceMesh {
    sample_surface_at(expr, domain, resolution, 0.0)
}

pub fn sample_surface_at(
    expr: &ScalarExpr,
    domain: &Domain,
    resolution: UVec2,
    time: f32,
) -> SurfaceMesh {
    let time = f64::from(time);
    // existing `sample_surface` body, with every `surface_position` call
    // receiving `time`
}

fn surface_position(
    expr: &ScalarExpr,
    x: f64,
    y: f64,
    time: f64,
    diagnostics: &mut SampleDiagnostics,
) -> Option<Vec3> {
    let Some(z) = expr.eval_at(x, y, time) else { /* unchanged */ };
    // unchanged f32 representability check
}

pub fn sample_curve(expr: &ScalarExpr, domain: &Domain, samples: usize) -> Vec<Option<Vec3>> {
    sample_curve_at(expr, domain, samples, 0.0)
}

pub fn sample_curve_at(
    expr: &ScalarExpr,
    domain: &Domain,
    samples: usize,
    time: f32,
) -> Vec<Option<Vec3>> {
    sample_curve_with_diagnostics_at(expr, domain, samples, time).0
}

pub fn sample_curve_with_diagnostics(
    expr: &ScalarExpr,
    domain: &Domain,
    samples: usize,
) -> (Vec<Option<Vec3>>, SampleDiagnostics) {
    sample_curve_with_diagnostics_at(expr, domain, samples, 0.0)
}

pub fn curve_edges(expr: &ScalarExpr, domain: &Domain, slots: &[Option<Vec3>]) -> CurveEdges {
    curve_edges_at(expr, domain, slots, 0.0)
}

pub fn curve_edges_at(
    expr: &ScalarExpr,
    domain: &Domain,
    slots: &[Option<Vec3>],
    time: f32,
) -> CurveEdges {
    let time = f64::from(time);
    // existing `curve_edges` body, with `probe_edge(..., time, ...)`
}

fn checked_height(
    expr: &ScalarExpr,
    x: f64,
    time: f64,
    diagnostics: &mut SampleDiagnostics,
) -> Option<f64> {
    let Some(height) = expr.eval_at(x, 0.0, time).filter(|_| x.is_finite()) else {
        // unchanged
    };
    // unchanged f32 representability check
}
```

Keep the existing public `sample_curve_with_diagnostics(expr, domain, samples)` signature as a `0.0` wrapper. Add the time-aware `sample_curve_with_diagnostics_at(expr, domain, samples, time)` implementation with the added `time: f32` parameter, convert once with `f64::from(time)`, and pass it through `checked_height` and `probe_edge`. Keep the public `sample_curve` wrapper and all existing diagnostics fields unchanged. `MAX_CURVE_SLOTS`, `MAX_SURFACE_AXIS`, and `MAX_PROBE_LEVEL` remain unchanged.

- [ ] **Step 5: Run the focused tests to verify they pass**

```bash
export PATH=/home/paul/.cargo/bin:/usr/bin:/bin
export RUSTC=/usr/bin/rustc
export CARGO_TARGET_DIR=/home/paul/.cache/vector-field-explorer-targets/issue-16
export TMPDIR=/home/paul/.cache/vector-field-explorer-targets/issue-16/tmp
cargo test --test suite scalar_animation_test:: -- --nocapture
cargo test --test suite scalar_test:: -- --nocapture
cargo test --test suite plot_curve_test:: plot_surface_test:: -- --nocapture
```

Expected: all pass. Record the logs under `evidence/task-1/`.

- [ ] **Step 6: Run the formatting and docs checks for owned files**

```bash
rustfmt --edition 2024 --check --config skip_children=true src/scalar.rs src/plot.rs tests/scalar_test.rs tests/scalar_animation_test.rs tests/suite.rs
python3 scripts/check-docs.py
git diff --check
```

Expected: exit 0 for every command. If rustfmt reports only pre-existing drift outside new hunks, record the exact file and compare against `1906113`; do not claim a pass.

- [ ] **Step 7: Commit**

```bash
git add src/scalar.rs src/plot.rs tests/scalar_test.rs tests/scalar_animation_test.rs tests/suite.rs
git commit -m "feat: sample scalar plots at simulated time" -m "Generated-by: OpenCode"
```

---

### Task 2: Scalar cache and shared-clock integration

**Files:**
- Modify: `src/plot_state.rs`
- Modify: `src/viewmodel/plot.rs`
- Modify: `src/viewmodel.rs`
- Modify: `src/sim_clock.rs`
- Modify: `src/ui/transport.rs`
- Create: `tests/scalar_clock_test.rs`
- Modify: `tests/suite.rs`

**Interfaces:**
- Consumes: `ScalarExpr::is_time_dependent`, `sample_curve_at`, `curve_edges_at`, `sample_surface_at`, existing `PlotState`/`ScalarDraft`.
- Produces:
  - `ScalarDraft.sampled_at: Option<f32>`
  - `State::active_scalar_is_time_dependent(&self) -> bool` (`pub(super)` from `viewmodel::plot`)
  - `State::scalar_time_changed(&mut self) -> bool` (`pub(super)` from `viewmodel::plot`)
  - `State::has_time_dependent_consumer(&self) -> bool`
  - `sim_clock::has_clock_consumer`, `transport_visible`, and `should_animate` retain their signatures and now include an active time-dependent scalar.
  - `ui::transport::show` shows Loop whenever `state.has_time_dependent_consumer()` is true.

- [ ] **Step 1: Write the failing clock, cache, and transport tests**

Create `tests/scalar_clock_test.rs`:

```rust
use std::sync::Arc;

use vector_field_explorer::{
    plot::{PlotKind, PlotScene},
    sim_clock::{
        SimClock, has_clock_consumer, should_animate, transport_visible,
    },
    viewmodel::State,
};

fn scalar(source: &str, kind: PlotKind) -> State {
    let mut state = State::new(false);
    state.set_plot_kind(kind);
    state.set_active_source(source.into());
    state
}

fn scene(state: &mut State) -> Arc<PlotScene> {
    state.plot_scene()
}

#[test]
fn static_scalar_has_no_clock_consumer() {
    let state = scalar("x^2", PlotKind::Curve2D);
    assert!(!state.has_time_dependent_consumer());
    assert!(!has_clock_consumer(&state));
    assert!(!transport_visible(&state));
    assert!(!should_animate(&state, &SimClock::default()));
}

#[test]
fn time_dependent_scalar_consumes_the_clock_only_while_playing() {
    let state = scalar("sin(t)*x", PlotKind::Curve2D);
    assert!(state.has_time_dependent_consumer());
    assert!(has_clock_consumer(&state));
    assert!(transport_visible(&state));

    let mut clock = SimClock::default();
    assert!(should_animate(&state, &clock));
    clock.set_playing(false);
    assert!(!should_animate(&state, &clock));
    assert!(transport_visible(&state), "pause does not hide the transport");

    let surface = scalar("x+y+t", PlotKind::Surface3D);
    assert!(has_clock_consumer(&surface));
}

#[test]
fn animated_scalar_wraps_time_at_the_loop_period() {
    let state = scalar("sin(t)*x", PlotKind::Curve2D);
    let mut clock = SimClock::default();
    clock.set_loop_period(2.0);
    clock.time = 1.9;
    clock.advance_for(&state, 0.2);
    assert!((clock.time - 0.1).abs() < 1e-6, "got {}", clock.time);
}

#[test]
fn time_changes_bump_generation_only_for_a_valid_time_dependent_draft() {
    let mut animated = scalar("sin(t)*x", PlotKind::Curve2D);
    let _ = scene(&mut animated);
    let before = animated.plot.generation;
    assert!(animated.set_time(0.5));
    assert!(animated.plot.generation > before);

    let mut stationary = scalar("x^2", PlotKind::Curve2D);
    let _ = scene(&mut stationary);
    let before = stationary.plot.generation;
    assert!(stationary.set_time(0.5));
    assert_eq!(stationary.plot.generation, before);

    let mut invalid = scalar("sin(t)*x", PlotKind::Curve2D);
    let _ = scene(&mut invalid);
    invalid.set_active_source("sin(t)*".into());
    let before = invalid.plot.generation;
    invalid.set_time(0.75);
    assert_eq!(invalid.plot.generation, before);
}

#[test]
fn sampled_at_tracks_the_frame_and_freezes_on_invalid_edits() {
    let mut animated = scalar("sin(t)*x", PlotKind::Curve2D);
    let _ = scene(&mut animated);
    assert_eq!(animated.plot.curve.sampled_at, Some(0.0));
    animated.set_time(0.5);
    let _ = scene(&mut animated);
    assert_eq!(animated.plot.curve.sampled_at, Some(0.5));

    let mut stationary = scalar("x^2", PlotKind::Curve2D);
    let _ = scene(&mut stationary);
    stationary.set_time(0.5);
    let _ = scene(&mut stationary);
    assert_eq!(stationary.plot.curve.sampled_at, Some(0.0));

    let mut invalid = scalar("sin(t)*x", PlotKind::Curve2D);
    let _ = scene(&mut invalid);
    invalid.set_active_source("sin(t)*".into());
    invalid.set_time(0.5);
    let frozen = scene(&mut invalid);
    assert_eq!(invalid.plot.curve.sampled_at, Some(0.0));
    assert!(invalid.plot.curve.error.is_some());
    assert!(frozen.status.contains("scalar parse error"));
}

#[test]
fn invalid_time_dependent_edit_keeps_the_transport_and_clock_alive() {
    let mut state = scalar("sin(t)*x", PlotKind::Curve2D);
    let _ = scene(&mut state);
    state.set_active_source("sin(t)*".into());
    assert!(state.plot.curve.error.is_some());
    assert!(state.plot.curve.accepted.is_some());
    assert!(state.plot.curve.samples.is_some());
    assert!(has_clock_consumer(&state));
    assert!(transport_visible(&state));
    assert!(should_animate(&state, &SimClock::default()));
}
```

Register `mod scalar_clock_test;` in `tests/suite.rs`.

- [ ] **Step 2: Run the tests to verify they fail**

```bash
export PATH=/home/paul/.cargo/bin:/usr/bin:/bin
export RUSTC=/usr/bin/rustc
export CARGO_TARGET_DIR=/home/paul/.cache/vector-field-explorer-targets/issue-16
export TMPDIR=/home/paul/.cache/vector-field-explorer-targets/issue-16/tmp
cargo test --test suite scalar_clock_test:: -- --nocapture
```

Expected: missing `sampled_at`, `has_time_dependent_consumer`, and scalar clock consumer behavior. Record the log under `evidence/task-2/`.

- [ ] **Step 3: Add `sampled_at` and update cache invalidation**

In `src/plot_state.rs` add:

```rust
pub struct ScalarDraft {
    pub source: String,
    pub error: Option<String>,
    pub accepted: Option<ScalarExpr>,
    pub samples: Option<Arc<SampledPlot>>,
    pub sampled_at: Option<f32>,
}
```

`sampled_at` defaults to `None`. In `src/viewmodel/plot.rs`:

- Clear `sampled_at` wherever `samples` is cleared: `set_plot_kind`, successful `update_scalar_source`, `set_plot_samples`, `set_plot_resolution`, and `restore_plot_drafts`.
- Keep `sampled_at` unchanged on a parse error, alongside the retained samples.
- In `plot_scene`, sample when:

```rust
let needs_sample = draft.samples.is_none()
    || (draft.error.is_none()
        && draft
            .accepted
            .as_ref()
            .is_some_and(ScalarExpr::is_time_dependent)
        && draft.sampled_at != Some(self.time));
```

When sampling, call `sample_curve_at(..., self.time)`, `curve_edges_at(..., self.time)`, and `sample_surface_at(..., self.time)`, then store `sampled_at = Some(self.time)` with the new samples. A static draft samples once at whatever time it was first displayed and is never resampled by time changes.

- [ ] **Step 4: Make time changes invalidate only the active scalar**

Add to `src/viewmodel/plot.rs`:

```rust
pub(super) fn active_scalar_is_time_dependent(&self) -> bool {
    match self.plot.kind {
        PlotKind::Curve2D => self
            .plot
            .curve
            .accepted
            .as_ref()
            .is_some_and(ScalarExpr::is_time_dependent),
        PlotKind::Surface3D => self
            .plot
            .surface
            .accepted
            .as_ref()
            .is_some_and(ScalarExpr::is_time_dependent),
        PlotKind::VectorField => false,
    }
}

pub(super) fn scalar_time_changed(&mut self) -> bool {
    let context = match self.plot.kind {
        PlotKind::Curve2D => Some(ScalarContext::Curve),
        PlotKind::Surface3D => Some(ScalarContext::Surface),
        PlotKind::VectorField => None,
    };
    let Some(context) = context else {
        return false;
    };
    let draft = self.scalar_draft(context);
    if draft.error.is_some()
        || !draft
            .accepted
            .as_ref()
            .is_some_and(ScalarExpr::is_time_dependent)
    {
        return false;
    }
    self.plot.generation += 1;
    self.mark_dirty()
}
```

`active_scalar_is_time_dependent` intentionally ignores `error`: an invalid edit of a previously time-dependent expression must keep the transport visible and the clock running while `scalar_time_changed` refuses to bump generation and freeze the mesh.

Add to `src/viewmodel.rs`:

```rust
pub fn has_time_dependent_consumer(self: &State) -> bool {
    if self.is_vector_mode() {
        self.particle_target().is_some() || self.field.is_time_dependent()
    } else {
        self.active_scalar_is_time_dependent()
    }
}

pub fn set_time(self: &mut State, t: f32) -> bool {
    if self.time == t {
        return false;
    }
    self.time = t;
    if self.is_vector_mode() {
        if self.field.is_time_dependent() {
            self.mark_dirty();
        }
    } else {
        self.scalar_time_changed();
    }
    true
}
```

Update the `set_time` doc comment: scalar mode invalidates only a valid active time-dependent draft; vector mode keeps its field-dependent behavior.

- [ ] **Step 5: Route the clock and transport through the new consumer predicate**

In `src/sim_clock.rs`:

```rust
pub fn has_clock_consumer(state: &State) -> bool {
    state.has_time_dependent_consumer()
}
```

Update the module and function docs: consumers are particles, time-dependent vector fields, and an active accepted time-dependent scalar; a static scalar has no consumer; an invalid scalar edit of a previously time-dependent draft keeps the transport and clock alive but freezes geometry.

In `src/ui/transport.rs`, replace the loop gate:

```rust
if state.has_time_dependent_consumer() {
    // existing Loop checkbox and Period slider body
}
```

Update the transport module doc: the strip appears for particles, time-dependent vector fields, and time-dependent scalar plots; it still owns no state.

- [ ] **Step 6: Run the focused tests to verify they pass**

```bash
export PATH=/home/paul/.cargo/bin:/usr/bin:/bin
export RUSTC=/usr/bin/rustc
export CARGO_TARGET_DIR=/home/paul/.cache/vector-field-explorer-targets/issue-16
export TMPDIR=/home/paul/.cache/vector-field-explorer-targets/issue-16/tmp
cargo test --test suite scalar_clock_test:: -- --nocapture
cargo test --test suite plot_state_test:: -- --nocapture
cargo test --test suite particle_playback_test:: -- --nocapture
cargo test --test suite plot_app_test:: -- --nocapture
```

Expected: all pass. Existing static scalar tests remain green; vector consumer tests keep their behavior.

- [ ] **Step 7: Run the owned-file checks**

```bash
rustfmt --edition 2024 --check --config skip_children=true src/plot_state.rs src/viewmodel/plot.rs src/viewmodel.rs src/sim_clock.rs src/ui/transport.rs tests/scalar_clock_test.rs tests/suite.rs
python3 scripts/check-docs.py
git diff --check
```

Expected: exit 0. Record any baseline-only rustfmt drift against `1906113`.

- [ ] **Step 8: Commit**

```bash
git add src/plot_state.rs src/viewmodel/plot.rs src/viewmodel.rs src/sim_clock.rs src/ui/transport.rs tests/scalar_clock_test.rs tests/suite.rs
git commit -m "feat: animate scalar plots from the shared clock" -m "Generated-by: OpenCode"
```

---

### Task 3: App playback, invalid-edit freeze, mode switch, docs, and gates

**Files:**
- Modify: `src/app/simulation.rs` (comments only unless a seam is required)
- Modify: `src/ui/help_overlay.rs`
- Modify: `docs/scalar-plotting.md`
- Modify: `README.md`
- Modify: `GLOSSARY.md`
- Create: `tests/scalar_animation_app_test.rs`
- Modify: `tests/plot_app_test.rs` only if its existing static assertions need wording; behavior must remain.
- Modify: `tests/suite.rs`

**Interfaces:**
- Consumes: Task 2's `State::has_time_dependent_consumer`, `State::set_time`, `sampled_at`, and existing `PlotAppHarness::{reconcile, advance_clock, advance_simulation, plot_scene}`.
- Produces: end-to-end playback, pause, static, invalid-edit, recovery, and kind-switch behavior; no new production API required.

- [ ] **Step 1: Write the failing app-level animation tests**

Create `tests/scalar_animation_app_test.rs`:

```rust
use std::sync::Arc;

use vector_field_explorer::{
    VfApp,
    plot::{PlotGeometry, PlotKind, PlotScene},
    sim_clock::{has_clock_consumer, should_animate},
};

fn step(app: &mut VfApp, dt: f32) {
    let mut harness = app.test_harness();
    harness.reconcile();
    harness.advance_clock(dt);
    harness.advance_simulation();
}

fn curve_geometry(scene: &PlotScene) -> Vec<([f32; 4], [f32; 4], [f32; 4])> {
    match &scene.geometry {
        PlotGeometry::Curve(lines) => lines
            .iter()
            .map(|line| (line.a, line.b, line.color))
            .collect(),
        PlotGeometry::Surface(_, _) => panic!("expected curve geometry"),
    }
}

fn scene(app: &mut VfApp) -> Arc<PlotScene> {
    app.test_harness()
        .plot_scene()
        .expect("the app holds a plot scene")
}

#[test]
fn animated_curve_resamples_while_playing_and_holds_while_paused() {
    let mut app = VfApp::new();
    app.state.set_plot_kind(PlotKind::Curve2D);
    app.state.set_active_source("sin(t)*x".into());
    step(&mut app, 0.1);
    let first = scene(&mut app);
    let first_geometry = curve_geometry(&first);
    let first_time = app.state.time;

    step(&mut app, 0.1);
    let second = scene(&mut app);
    assert!(second.generation > first.generation);
    assert_ne!(curve_geometry(&second), first_geometry);
    assert!(app.state.time > first_time);

    app.clock.set_playing(false);
    step(&mut app, 0.1);
    let paused = scene(&mut app);
    assert_eq!(paused.generation, second.generation);
    assert!(Arc::ptr_eq(&paused, &scene(&mut app)));
}

#[test]
fn static_scalar_is_idle_even_while_the_shared_clock_could_run() {
    let mut app = VfApp::new();
    app.state.set_plot_kind(PlotKind::Curve2D);
    app.state.set_active_source("x^2".into());
    step(&mut app, 0.1);
    let first = scene(&mut app);
    assert!(!has_clock_consumer(&app.state));
    assert!(!should_animate(&app.state, &app.clock));
    let time = app.clock.time;

    step(&mut app, 0.1);
    let second = scene(&mut app);
    assert_eq!(app.clock.time, time);
    assert_eq!(second.generation, first.generation);
    assert!(Arc::ptr_eq(&first, &second));
}

#[test]
fn invalid_edit_freezes_the_mesh_while_time_keeps_running() {
    let mut app = VfApp::new();
    app.state.set_plot_kind(PlotKind::Curve2D);
    app.state.set_active_source("sin(t)*x".into());
    step(&mut app, 0.1);
    let valid = scene(&mut app);
    let geometry = curve_geometry(&valid);
    let sampled_at = app.state.plot.curve.sampled_at;
    let time = app.clock.time;

    app.state.set_active_source("sin(t)*".into());
    step(&mut app, 0.1);
    let invalid = scene(&mut app);
    assert!(app.clock.time > time);
    assert_eq!(app.state.plot.curve.sampled_at, sampled_at);
    assert_eq!(curve_geometry(&invalid), geometry);
    assert!(invalid.status.contains("scalar parse error"));

    let frozen = scene(&mut app);
    step(&mut app, 0.1);
    let still_frozen = scene(&mut app);
    assert!(Arc::ptr_eq(&frozen, &still_frozen));
    assert!(app.clock.time > time);
}

#[test]
fn fixing_the_source_resumes_at_the_current_time() {
    let mut app = VfApp::new();
    app.state.set_plot_kind(PlotKind::Curve2D);
    app.state.set_active_source("sin(t)*x".into());
    step(&mut app, 0.1);
    let before = scene(&mut app).generation;
    app.state.set_active_source("sin(t)*".into());
    step(&mut app, 0.1);
    let frozen = scene(&mut app).generation;

    app.state.set_active_source("cos(t)*x".into());
    step(&mut app, 0.1);
    let recovered = scene(&mut app);
    assert!(recovered.generation > frozen);
    assert!(recovered.generation > before);
    assert!(app.state.plot.curve.error.is_none());
    assert_eq!(app.state.plot.curve.sampled_at, Some(app.state.time));
}

#[test]
fn kind_switch_samples_at_the_current_clock_time() {
    let mut app = VfApp::new();
    app.state.set_plot_kind(PlotKind::Curve2D);
    app.state.set_active_source("sin(t)*x".into());
    step(&mut app, 0.1);
    step(&mut app, 0.1);
    let switched_at = app.state.time;

    app.state.set_plot_kind(PlotKind::Surface3D);
    app.state.set_active_source("x+y+t".into());
    step(&mut app, 0.0);
    let surface = scene(&mut app);
    assert_eq!(surface.kind, PlotKind::Surface3D);
    assert_eq!(app.state.plot.surface.sampled_at, Some(switched_at));

    let curve_time = app.state.time;
    app.state.set_plot_kind(PlotKind::Curve2D);
    step(&mut app, 0.0);
    let curve = scene(&mut app);
    assert_eq!(curve.kind, PlotKind::Curve2D);
    assert_eq!(app.state.plot.curve.sampled_at, Some(curve_time));
}
```

Register `mod scalar_animation_app_test;` in `tests/suite.rs`.

- [ ] **Step 2: Run the app tests to verify they fail only where expected**

```bash
export PATH=/home/paul/.cargo/bin:/usr/bin:/bin
export RUSTC=/usr/bin/rustc
export CARGO_TARGET_DIR=/home/paul/.cache/vector-field-explorer-targets/issue-16
export TMPDIR=/home/paul/.cache/vector-field-explorer-targets/issue-16/tmp
cargo test --test suite scalar_animation_app_test:: -- --nocapture
```

Expected: after Task 2, these should pass except for any missing test registration or a stale app comment. If a test fails, fix the production seam it names rather than weakening the assertion. Record the log under `evidence/task-3/`.

- [ ] **Step 3: Correct app and transport comments to the new contract**

In `src/app/simulation.rs`, update the `advance_clock` and `advance_simulation` comments: scalar mode still skips vector particle advection, but `advance_clock` now advances the shared clock for a time-dependent scalar because the scalar is a clock consumer. Do not add a second timer or a scalar-specific clock call.

In `src/ui/transport.rs`, update the module and `show` comments so the transport is described as shared by particles, time-dependent vector fields, and time-dependent scalar plots. The Loop control is visible whenever `state.has_time_dependent_consumer()` is true.

- [ ] **Step 4: Update the scalar guide and help**

In `docs/scalar-plotting.md` replace the current “not an ... animation system” and “They do not reference time `t`” statements with a time section:

```markdown
## Time-dependent plots

Curves and surfaces may use the shared simulation variable `t`:

- Curves: `x` or `x` and `t`, for example `sin(t)*x`.
- Surfaces: `x` and `y`, or `x`, `y`, and `t`, for example
  `sin(3*x)*cos(3*y)*sin(t)`.
- `t` is the same elapsed simulation time used by particles and
  time-dependent vector fields. There is no separate scalar timer.
- **Play** resamples a time-dependent plot at the current `t`; **Pause**
  keeps the last mesh. The clock's loop period wraps `t` exactly as it does
  for other consumers.
- A static plot does not rebuild or request repaint merely because the clock
  is running for another consumer.
- An invalid edit keeps the last valid mesh and shows the parse error. The
  clock and transport may keep running while the mesh stays frozen until the
  text becomes valid again.
- Switching curve/surface kind or 2D/3D mode samples at the current `t`; it
  does not restart at zero.
```

Update `README.md` to say scalar plots can be animated by the shared clock and link the guide. Add a short `GLOSSARY.md` entry for **Scalar animation**: a time-dependent scalar curve or surface resampled from the one `SimClock`, with static plots remaining idle.

In `src/ui/help_overlay.rs`, extend the scalar help section with rows for:

```rust
HelpRow {
    input: "t",
    action: "shared simulated time in curve/surface formulas",
},
HelpRow {
    input: "Space",
    action: "play/pause the shared clock (animated scalar or other consumer)",
},
HelpRow {
    input: "Loop",
    action: "wrap shared time at the transport period",
},
```

Use the existing `HelpRow` shape and keep the lines short enough for the compact overlay. Mention that an invalid edit freezes the last mesh while playback can continue.

- [ ] **Step 5: Run the focused app and UI regressions**

```bash
export PATH=/home/paul/.cargo/bin:/usr/bin:/bin
export RUSTC=/usr/bin/rustc
export CARGO_TARGET_DIR=/home/paul/.cache/vector-field-explorer-targets/issue-16
export TMPDIR=/home/paul/.cache/vector-field-explorer-targets/issue-16/tmp
cargo test --test suite scalar_animation_app_test:: -- --nocapture
cargo test --test suite plot_app_test:: -- --nocapture
cargo test --test suite scalar_clock_test:: -- --nocapture
cargo test --test suite help_overlay_test:: -- --nocapture
cargo test --test suite responsive_test:: -- --nocapture
```

Expected: all pass. Record logs under `evidence/task-3/`.

- [ ] **Step 6: Run full repository gates**

```bash
export PATH=/home/paul/.cargo/bin:/usr/bin:/bin
export RUSTC=/usr/bin/rustc
export CARGO_TARGET_DIR=/home/paul/.cache/vector-field-explorer-targets/issue-16
export TMPDIR=/home/paul/.cache/vector-field-explorer-targets/issue-16/tmp
export KEEP_TEST_ARTIFACTS=1
cargo test --test suite
cargo build
cargo clippy --all-targets -- -D warnings
rustfmt --edition 2024 --check --config skip_children=true src/scalar.rs src/plot.rs src/plot_state.rs src/viewmodel/plot.rs src/viewmodel.rs src/sim_clock.rs src/ui/transport.rs src/ui/help_overlay.rs tests/scalar_test.rs tests/scalar_animation_test.rs tests/scalar_clock_test.rs tests/scalar_animation_app_test.rs tests/suite.rs
python3 scripts/check-docs.py
git diff --check
scripts/smoke-run.sh 15
```

Expected: suite, build, clippy, docs, diff, and smoke exit 0. Rustfmt may report only pre-existing baseline drift; compare against `1906113` and record the exact files and hunks. Do not call a baseline failure a pass.

- [ ] **Step 7: Commit**

```bash
git add src/app/simulation.rs src/ui/help_overlay.rs docs/scalar-plotting.md README.md GLOSSARY.md tests/scalar_animation_app_test.rs tests/plot_app_test.rs tests/suite.rs
git commit -m "feat: play animated scalar plots from the shared clock" -m "Generated-by: OpenCode"
```

---

## Self-Review

- **Spec coverage:** Task 1 covers `t` binding, expression evaluation, curve/surface sampling, edge probes, and t=0 compatibility. Task 2 covers `sampled_at`, generation invalidation, static no-rebuild behavior, clock/transport predicates, loop visibility, and invalid-edit transport. Task 3 covers playback, pause, static idle, invalid freeze/recovery, kind switching at current time, docs/help, full gates, and smoke. The no-second-timer, no-persisted-clock, no-CLI, and no-physics constraints are global.
- **Step scan:** Every test step has executable code and named assertions. Every implementation step names the exact API, file, and values. The only intentionally open details are bodies of existing loops and the exact visual spacing of help rows, neither of which changes behavior.
- **Type consistency:** `sampled_at` is `Option<f32>`; `_at` sampling takes `time: f32`; `eval_at` takes `t: f64`; `has_time_dependent_consumer` is the single producer for `sim_clock` and transport. Existing t=0 wrappers keep their signatures.
- **Review Focus:** Each of the five focus lines is exercised: time-dependent invalid values by sampling and app invalid tests; invalid-edit freeze/recovery by `scalar_clock_test` and `scalar_animation_app_test`; static scalar idle by `static_scalar_has_no_clock_consumer` and `static_scalar_is_idle_even_while_the_shared_clock_could_run`; loop/transport by `animated_scalar_wraps_time_at_the_loop_period` and `invalid_time_dependent_edit_keeps_the_transport_and_clock_alive`; mode/kind switch by `kind_switch_samples_at_the_current_clock_time`.
- **Proportion:** Three tasks, one per independently reviewable seam; test bodies carry the exact values, implementation steps name signatures and only describe the existing loop bodies that remain unchanged.
