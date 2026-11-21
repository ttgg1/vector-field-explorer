# Scalar comparisons and potential gradients — contract (#93)

Status: accepted 2026-11-21 (owner approved; slice A implementation needs its own claim).
Parent issue: [#93](https://github.com/ttgg1/vector-field-explorer/issues/93),
epic [#81](https://github.com/ttgg1/vector-field-explorer/issues/81).
Depends on: scalar views (#92, contract accepted), colour semantics (#83, merged).

## Intent

Give students side-by-side scalar comparison curves and a clearly-labelled
potential-to-field view (`E = -∇V`) while keeping numerical differentiation honest
about error, poles and units.

## Ownership and interpretation rules

- Comparisons are **display-only series** over the existing scalar evaluator
  (`ScalarExpr`); they never feed vector arrows, colour mapping or probes.
- A gradient is labelled `E = -∇V` **only** when its source is marked as an electric
  potential (`SeriesRole::ElectricPotential`). Values stay in normalized teaching units.
  A generic scalar gradient is labelled `∇f` and never called an electric field.
- Values stay in normalized teaching units; no SI calibration or dimensional analysis
  is implied, and electric and magnetic quantities are never mixed.

## Value model

```rust
/// One comparison curve. `expr` is parsed with the existing `ScalarContext::Curve`.
pub struct ComparisonSeries {
    pub id: SeriesId,          // stable, user-visible label key
    pub label: String,         // <= MAX_SERIES_LABEL_BYTES
    pub expr: String,
    pub style: SeriesStyle,    // colour + dash; independent of other series
}

pub enum SeriesRole { Scalar, ElectricPotential }

pub struct GradientSpec {
    pub role: SeriesRole,       // ElectricPotential => E = -grad(V)
    /// The differentiated source: a surface-context formula sampled on `frame`
    /// through the #92 seam (`ScalarExpr::eval_at`, committed parameters, shared
    /// clock). Comparison curves are Curve-context and display-only; they never
    /// feed gradients.
    pub expr: String,
    pub frame: PlaneFrame,
    pub step: f32,              // finite-difference step; validated, never clamped
    pub max_error: f32,         // budget for a usable sample
}

pub struct GradientSample {
    pub position: Vec3,
    pub gradient: Option<Vec3>, // None at poles/invalid points
    // `error_estimate` is 0.0 exactly when `gradient` is `None`; it carries no
    // meaning for a missing sample.
    /// Agreement indicator between `h` and `h / 2`, not a bound on the true
    /// derivative error.
    pub error_estimate: f32,
    /// False when `error_estimate > max_error`: the overlay omits the arrow and
    /// reports the count; the value is never silently presented as trusted.
    pub usable: bool,
    pub one_sided: bool,        // boundary fallback
}
```

Bounds: `MAX_COMPARISON_SERIES = 4`, `MAX_SERIES_LABEL_BYTES = 48`,
`MAX_GRADIENT_SAMPLES = 25_600` (reuses the #92 grid bound), `MIN_STEP = 1e-6`,
`MAX_STEP = 1e-1`.

## Differentiation method (proposal, not a locked algorithm)

- The gradient source is a `ScalarContext::Surface` formula in `x`/`y`, sampled on its
  `PlaneFrame`; a Curve-only formula or a `z`-referencing formula is unsupported and
  reports the #92 unsupported reason instead of sampling.
- Central differences on `ScalarExpr::eval_at` at `h = step`, falling back to one-sided
  differences on the domain boundary and reporting `one_sided`. The one-sided stencil
  always points inward; when the inward sample is invalid the gradient is `None`. A step
  outside `[MIN_STEP, MAX_STEP]` or non-finite is rejected before sampling, never clamped.
- `error_estimate` is the largest component difference between `h` and `h / 2`
  evaluations. It is a refinement-agreement indicator, not a bound on the true error, and
  is surfaced to the user; a sample above `max_error` is marked unusable rather than
  silently trusted. A finite sample adjacent to a pole can still produce a large finite
  gradient; only non-finite evaluations become holes.
- Any non-finite evaluation yields `gradient: None` and preserves the hole; no
  interpolation across poles.
- No symbolic CAS and no wasm-only path: one implementation runs native and in the
  browser.

Alternatives considered: analytic/symbolic differentiation (the locked `meval 0.2.0`
exposes no derivative API, so this needs a new CAS dependency), forward differences
(cheaper but systematically biased), automatic differentiation (new dependency and a
change to the evaluator; not a wasm blocker but unnecessary here). Central differences
with an explicit error estimate is the smallest honest option; the contract records the
choice as a proposal for the implementation subplan to confirm.

## Synchronisation and capture

- Comparison curves share one domain, one clock value and the committed parameter set;
  editing any of them invalidates all series together.
- Gradient sampling obeys the #92 visibility and 10 Hz refresh rules and does not run
  while its panel is hidden.
- Raster/screenshot and animation capture include comparison curves and gradient arrows.
  SVG keeps the line-blueprint contract: comparison curves and gradient arrows export as
  lines, never filled heatmap cells.
- Series/gradient settings are document state (`ViewRecord`) and follow undo/redo, saved
  scenes and the envelope; missing fields in older records restore as no series with no
  dirtying.

## Consumers and ownership

| Consumer | Owner | Slice |
| --- | --- | --- |
| Series/gradient model, bounds, error estimate | `src/scalar_analysis.rs` (new) | A |
| Curve evaluation and plot rendering | `src/plot.rs`, `src/ui/plot_panel.rs` (existing) | B |
| Gradient arrows on the slice | `src/ui/scalar_panel.rs`, sampling path | B |
| Persistence | `src/persistence.rs`, `src/scene_document.rs` | A |
| Colour/dash independence | `src/color_mapping.rs` (read-only; no override) | B |
| Export/capture | existing raster/SVG split | B |

## Slices

- **A (model):** `ComparisonSeries`, `SeriesRole`, `GradientSpec`, `GradientSample`,
  bounded validation, central/one-sided differentiation with error estimates and hole
  preservation, series persistence with non-dirtying restore, invalidating cache.
  No UI, renderer, shader, dependency.
- **B (UI/capture):** comparison list and styling in the plot/scalar panel, gradient
  arrow overlay with the honest label, raster/animation capture, SVG line export.

## Required tests (slice A)

1. `constant_potential_has_zero_gradient`
2. `linear_potential_gradient_is_exact`
3. `quadratic_potential_gradient_matches_the_analytic_derivative`
4. `pole_samples_report_none_and_no_error_fabrication`
5. `domain_boundary_uses_inward_one_sided_differences_and_flags_it`
6. `step_refinement_shrinks_the_reported_error_quadratically_for_sin`
7. `generic_scalar_gradient_is_never_labelled_as_an_electric_field`
8. `comparison_series_are_bounded_labelled_and_independently_styled`
9. `series_and_gradient_settings_restore_without_dirtying`
10. `invalid_or_non_finite_steps_are_rejected_before_sampling`
11. `comparison_series_do_not_reach_arrows_colours_or_probes`
12. `editing_parameters_clock_or_source_invalidates_the_gradient_cache`
13. `curve_only_or_z_formulas_report_the_scalar_unsupported_reason`
14. `samples_over_the_error_budget_are_marked_unusable`

## Gates

`cargo build`, `scripts/test.sh`, clippy `-D warnings`, `check-docs`,
`git diff --check`, changed-file formatting; native smoke once the UI slice lands.
Independent review of the actual diff; browser/manual visual verification remains
owner-waived, not passed.
