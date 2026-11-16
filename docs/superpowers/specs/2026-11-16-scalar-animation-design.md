# Scalar Plot Animation with the Shared Clock

**Status:** APPROVED by owner on 2026-11-16; implementation plan and sub-issues follow

**Issue:** [#16](https://github.com/ttgg1/vector-field-explorer/issues/16)

**Dependency:** #12 static scalar plotting (merged), #15 native CLI export (merged)

## Intent

Scalar curve and surface plots should animate when their formula depends on time,
using the same `SimClock` that already drives particles and time-dependent vector
fields. A formula such as `sin(t) * x` or `cos(t) * exp(-(x^2+y^2))` must
resample at the clock's current time while playback runs, wrap with the existing
loop period, and stop rebuilding when paused or when the active plot is static.

This is the remaining animation prerequisite for the physics roadmap (#17). It
must reuse the existing clock, transport, and sampling seams; it must not add a
second timer, a second transport row, a shortcut, or persisted clock state.

## Goals

- Bind `t` in scalar curve and surface expressions.
- Resample the active time-dependent scalar plot at the shared clock time while
  playing.
- Preserve loop-period wrapping exactly as the clock already implements it.
- Keep paused plots at their last mesh and rebuild nothing.
- Keep static plots idle: no clock consumer, no repaint request, no per-frame
  rebuild.
- Retain the last valid mesh and show the parse error when an edit is invalid.
  The mesh freezes at its last sampled frame; the clock may continue running.
- Resample at the current time when switching 2D/3D or returning to a scalar kind.
- Show the existing transport strip, including its loop control, whenever an
  active scalar expression is time-dependent.
- Cover playing, paused, static, invalid-edit, mode-switch, and repaint truth
  tables under `tests/`.

## Non-goals

- No second animation timer, clock, transport row, keyboard shortcut, or persisted
  clock/animation state.
- No scalar-specific playback speed or separate time.
- No precomputation of animation frames or interpolation between samples.
- No time-dependent vector-field behavior changes; #12/local 39 already owns it.
- No new CLI flag; CLI export remains static at its deterministic sampled time.
- No physics implementation or new symbolic solver.
- No wasm/browser/manual visual verification claim. Existing test waivers remain
  waived, not passed.

## Approaches Considered

### A — Direct shared-clock resampling (chosen)

Extend `ScalarExpr` to bind and evaluate `t`, thread the clock time through the
existing sampling functions, and invalidate the scalar mesh cache when the active
time-dependent expression sees a new time. Make the existing clock consumer,
transport visibility, repaint loop, and loop control recognize an active
time-dependent scalar draft.

Trade-off: the CPU resamples the curve or surface every animated frame. At the
existing caps (513 curve samples; 96×96 surface maximum; default 64×64) this is
small, and it keeps one time model and one cache. It also composes directly with
domain, density, settings, and mode transitions.

### B — Precompute a timeline of meshes

Sample a fixed number of frames over a loop period and display the nearest frame.

Trade-off: faster playback, but much more memory, ambiguous behavior without a
loop period, costly invalidation on every domain/settings edit, and a second time
mapping that can drift from the shared clock. Rejected.

### C — Separate scalar animation timer

Run a scalar-only timer and derive plot time from it.

Trade-off: simple locally, but violates the one-clock requirement and creates
drift between plots, particles, and time-dependent vector fields. Rejected.

## Design

### Expression syntax and evaluation

`ScalarExpr` gains a time-dependence flag and an explicit time parameter:

```rust
pub fn parse(source: &str, context: ScalarContext) -> Result<Self, ScalarError>;
pub fn is_time_dependent(&self) -> bool;
pub fn eval(&self, x: f64, y: f64) -> Option<f64>;      // t = 0 compatibility
pub fn eval_at(&self, x: f64, y: f64, t: f64) -> Option<f64>;
```

Parsing binds the allowed variables by trying the static binding first
(`bind("x")` for curves, `bind2("x","y")` for surfaces). If that fails, it tries
the time-aware binding (`bind2("x","t")` for curves, `bind3("x","y","t")` for
surfaces). The successful branch sets `is_time_dependent`. If both fail, the
original static-binding error is reported, preserving today's user-visible
allowed-variable text (`x` or `x, y`) and its existing diagnostics. `t(...)`
remains a function call, not a free variable. `eval_at` accepts `f64`; the
sampling boundary casts the clock's `f32` time once with `f64::from(t)`. The
compatibility `eval` uses `t = 0.0`.

Sampling keeps the existing public entry points as `t = 0` compatibility wrappers
and adds time-aware variants:

- `sample_curve(expr, domain, samples)` delegates to
  `sample_curve_at(expr, domain, samples, t = 0.0)`.
- `curve_edges(expr, domain, slots)` delegates to
  `curve_edges_at(expr, domain, slots, t = 0.0)`.
- `sample_surface(expr, domain, resolution)` delegates to
  `sample_surface_at(expr, domain, resolution, t = 0.0)`.

The `_at` variants pass `t` through their internal position/probe helpers. The
plot state uses the `_at` variants with the shared clock time. Static expressions
ignore the value. No interpolation is introduced.

### Cache invalidation and state

`ScalarDraft` records the time of its cached samples:

```rust
pub sampled_at: Option<f32>,
```

`plot_scene` resamples when:

- the draft has no samples,
- the draft was invalidated by an edit/settings/domain change, or
- the accepted expression is time-dependent, there is no current parse error,
  and `sampled_at != Some(self.time)`.

On a successful sample it stores both the mesh and `sampled_at = Some(self.time)`.
`sampled_at` is always cleared wherever `samples` is cleared (edits, settings,
domain changes, kind switches, restores), so a later time-dependent edit cannot
inherit a stale timestamp. Static expressions never compare `sampled_at` and
never resample merely because time changed.

`State::set_time` currently marks the vector field dirty only when its field is
time-dependent. It additionally bumps `plot.generation` when the active scalar
draft has a time-dependent accepted expression, no parse error, and the time
actually changed. This is what makes `refresh_plot_scene` replace the cached GUI
scene. Because `plot_changed` lives in the child `viewmodel::plot` module, expose
a narrow `pub(super)` method (for example `State::note_scalar_time_changed`) or
make `plot_changed` `pub(super)`; the parent `viewmodel` module cannot call a
private child item. Paused frames do not change `time`, so they do not bump the
generation.

Invalid-edit freeze: `update_scalar_source` continues to retain the last accepted
expression and its samples while setting `draft.error`. While `error` is present,
`set_time` does not bump the scalar generation, so the displayed mesh stays at
its last sampled frame. The clock and transport may continue running. A later
valid edit clears the error, invalidates samples, and resumes sampling at the
current clock time.

### Clock consumer, transport, and repaint

The existing shared clock remains the only time source. Add a State predicate for
the active scalar draft's accepted time dependence, then use it consistently:

- `has_clock_consumer`: vector behavior unchanged; scalar kinds consume the clock
  only when the active accepted expression is time-dependent. The current module
  comment saying scalar kinds pause every consumer and the existing tests that
  pin `has_clock_consumer`/`transport_visible` false for scalar kinds must be
  updated; the static-scalar case remains false.
- `transport_visible`: follows `has_clock_consumer`, so the existing strip appears
  for animated scalar plots and stays hidden for static ones.
- `should_animate`: unchanged shape — playing and a consumer. Static scalar plots
  request no repaint; paused animated plots request no repaint.
- The transport loop control is shown for any time-dependent consumer, including
  the active scalar expression, and keeps using the existing `SimClock.loop_period`.
  The loop test pins the exact wrapped value, not just that time changed.
- Space play/pause uses the same visibility predicate; no new shortcut.
- During an invalid edit of a previously time-dependent expression, the accepted
  expression remains time-dependent, so the transport/clock stay visible and
  running while the mesh is frozen.

Switching between Curve2D and Surface3D already resets the destination draft's
accepted expression/samples and recurses through `set_plot_kind`, so the first
`plot_scene` after the switch samples at the current `State::time`. Returning to
vector mode keeps the existing pause semantics.

### Rendering and error behavior

`build_plot_scene` continues to consume only `SampledPlot`; it does not need the
expression or clock. `PlotScene.generation` remains the bridge to
`refresh_plot_scene`. The plot status continues to show `draft.error` while the
last valid mesh remains visible.

## Testing Strategy

Add focused tests under `tests/` and keep them registered in `tests/suite.rs`:

- scalar parse/eval accepts `t`, detects time dependence, rejects unknown free
  variables, and leaves static formulas unchanged; update the existing
  `tests/scalar_test.rs` list that currently treats `t` as invalid;
- sampling at two times produces different geometry only for time-dependent
  expressions, and curve edge discontinuity probes use the same time;
- `has_clock_consumer`, `transport_visible`, and `should_animate` truth tables for
  playing/paused/static/animated scalar and existing vector cases;
- `set_time` bumps the scalar generation only when time changed, the accepted
  expression is time-dependent, and no error is present; a static scalar plot
  beside a playing clock neither writes `sampled_at` nor bumps the generation;
- `transport_visible` stays true during an invalid edit of a previously
  time-dependent expression while the mesh is frozen;
- app-level playback resamples the active plot; pause retains the same scene;
  loop-period wrapping resamples at the exact wrapped time;
- invalid edit freezes the last mesh and shows the parse error; fixing it resumes
  at the current time;
- kind/mode switching resamples the destination plot at the current time;
- static plot does not rebuild or request repaint while the clock has no consumer.

Applicable gates: `cargo test --test suite`, `cargo build`,
`cargo clippy --all-targets -- -D warnings`, changed-file rustfmt,
`python3 scripts/check-docs.py`, `git diff --check`, and `scripts/smoke-run.sh 15`
because this changes frame-time rendering behavior. Browser/wasm/manual visual
gates remain waived, not passed.

## Risks

- Per-frame CPU sampling cost. Bounded by existing curve/surface caps and accepted
  for correctness.
- Cache invalidation bugs could show a stale mesh or rebuild static plots. Tests
  pin the generation and repaint truth tables.
- `f32` time precision at very large times. The clock already uses `f32` and
  wraps into `[0, period)`; this design adds no new precision contract.
- Freezing on invalid edits is intentional and user-visible; docs/help must say
  the last valid mesh is retained while the error is shown.

## Decisions

- Approach A and freeze-on-invalid-edit were approved by the owner on 2026-11-16.
- `t` is the only time variable; it is legal in both curve and surface formulas.
- No separate scalar timer, persisted animation state, CLI flag, new row, or new
  shortcut.
- Sub-issues for implementation tasks will be created after the plan fixes the
  task boundaries and file ownership.
