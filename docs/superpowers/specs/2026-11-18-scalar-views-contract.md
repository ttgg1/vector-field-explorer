# Scalar views, contours and probes — contract (#92)

Status: accepted 2026-11-21 (owner approved the contract; each slice needs its own claim).
Parent issue: [#92](https://github.com/ttgg1/vector-field-explorer/issues/92),
epic [#81](https://github.com/ttgg1/vector-field-explorer/issues/81).

## Intent

Extend the existing scalar-curve/surface and formula backends with readable heatmaps,
bounded contours and point/line probes without inventing a second field evaluator or
fabricating values where the field is undefined.

## Non-goals

- No general-purpose CAS, marching-cubes/volume rendering, arbitrary-resolution
  export, SI dimensional analysis or new solver.
- No scalar/vector numeric addition, no silent interpolation across holes.
- No per-frame full re-evaluation of large grids; no unbounded cached sample counts.
- Contours stay 2D slice contours (marching squares is a proposal here, not a locked
algorithm choice); no 3D isosurfaces.

## Value model

```rust
/// One evaluated scalar sample. `value` is `None` when evaluation failed at this
/// point (pole, domain exit, non-finite result), never a substituted number.
pub struct ScalarSample {
    pub position: Vec3,
    pub value: Option<f32>,
}

/// A sampled rectangular region on the current plane frame.
pub struct ScalarGrid {
    pub frame: PlaneFrame,
    pub width: u32,      // <= MAX_GRID_SIDE
    pub height: u32,
    pub time: f64,
    pub samples: Vec<ScalarSample>, // row-major, len == width * height
}

pub struct ContourSegment { pub start: Vec2, pub end: Vec2, pub level: f32 }

/// Probes carry a world position plus the same optional anchor record the seed
/// layout contract uses (`SeedAnchor`), so a moved plane never moves a probe and
/// local editing stays possible. `anchor = None` means world-only.
pub struct PointProbe {
    pub position: Vec3,
    pub anchor: Option<SeedAnchor>,
    pub value: Option<f32>,
}
pub struct LineProbe {
    pub start: Vec3,
    pub end: Vec3,
    pub anchor: Option<SeedAnchor>,
    pub samples: Vec<ScalarSample>,
}
```

Rules:

- `MAX_GRID_SIDE = 160` (≤ 25,600 evaluations per rebuild), `MIN_GRID_SIDE = 8`.
- `MAX_LINE_SAMPLES = 512`, `MAX_PROBES = 8`.
- Grid and line sampling reuse the existing scalar evaluator `ScalarExpr::eval_at(x, y, t)`
  with the committed parameter set and the shared clock value; no separate evaluator and
  no parser change. A sample maps the plane's local `(u, v)` to `(x, y)`, so only
  formulas in those coordinates are supported in scalar views: a formula referencing `z`
  reports the explicit unsupported reason "scalar views support x/y formulas" instead of
  sampling, and a non-XY plane uses that plane's own local axes. The unsupported reason is
  produced by the scalar panel mapping the existing `ScalarError` ("allowed variables: x,
  y") to user-facing text; the parser error itself is unchanged. Point and line probes
  evaluate through the same `eval_at` seam after projecting world positions to those
  local coordinates. Scalar views parse with `ScalarContext::Surface` (both `x` and `y`
  in scope); a formula valid only in `ScalarContext::Curve` is unsupported on grids and
  surfaces. Grid time is held as `f64` and converted once at the existing
  `sample_surface_at` boundary, which is where the `f32` value conversion also happens.
- A cell is painted only when all four corner samples are `Some`. A cell with any
  missing corner is left transparent and counted, not filled with a neighbour value.
- Contour levels are explicit, strictly increasing, bounded to `MAX_CONTOUR_LEVELS = 16`;
  a segment is emitted only when both endpoints interpolate between two finite samples
  of the same cell. Cells with missing corners emit nothing. No smoothing, no implicit
  level guessing.

## Synchronisation

Every grid, probe and contour record carries the `time`, the plane frame and the
parameter-set identity it was produced with. A change to formula text, parameters, plane
frame, domain or density invalidates the cached records and forces exactly one bounded
resample.

The shared clock is reconciled with the no-per-frame-resample rule by sampling only while
the scalar panel is visible and coalescing animated refreshes to at most one resample per
`SCALAR_RESAMPLE_INTERVAL = 100 ms` (10 Hz), keyed on the quantised clock value. A hidden
panel performs no sampling at all; showing it samples once immediately.

Probes are document state (`ViewRecord`), so undo/redo, saved scenes and the settings
envelope cover them with the existing mechanisms; a missing `probes` field in an older
envelope or scene restores as no probes with no dirtying. Cached samples are runtime
state and never persisted.

## Consumers and ownership

| Consumer | Owner | Slice |
| --- | --- | --- |
| Sampling model + bounds | `src/scalar_view.rs` (new) | A |
| Plane/world mapping | `src/plane.rs` (existing helpers) | A |
| Cache invalidation | `src/viewmodel.rs` | A |
| Probe persistence | `src/persistence.rs`, `src/scene_document.rs` | A |
| Heatmap/contour painting | `src/ui/scalar_panel.rs` (new), renderer pass | B |
| Export/raster capture | existing capture path | B |
| Preset catalogue entries that need it | `src/presets.rs` metadata only | C (#91 tie-in) |

## Slices

- **A (pure model):** `ScalarSample`/`ScalarGrid`/`ContourSegment`/`PointProbe`/`LineProbe`,
  validated construction, bounded sampling through `ScalarExpr`, invalidity preservation,
  `ViewRecord` probe persistence with non-dirtying restore, cache-key invalidation.
  No UI, no renderer, no shader.
- **B (UI/render):** Scalar panel section (grid resolution, contour levels, probe list),
  painted heatmap with honest hole rendering, contour segments, probe markers and readouts,
  raster/screenshot inclusion. SVG export includes contour segments and probe markers as
  lines but never the heatmap fill or text labels, so the existing line-blueprint contract
  holds.
- **C (catalogue tie-in):** scalar teaching presets that only need A/B.

## Required tests (slice A)

1. `constant_field_grid_is_uniform_and_finite`
2. `linear_field_grid_matches_analytic_corners`
3. `saddle_field_grid_is_antisymmetric`
4. `pole_samples_are_none_and_cells_stay_unpainted`
5. `domain_edges_do_not_fabricate_samples`
6. `animated_samples_track_the_shared_clock`
7. `grid_and_line_bounds_are_enforced_before_allocation`
8. `contour_levels_must_be_finite_strictly_increasing_and_bounded`
9. `contours_emit_nothing_across_missing_corners`
10. `probes_are_document_state_and_restore_without_dirtying`
11. `parameter_edit_invalidates_cached_records`
12. `line_probe_samples_preserve_invalidity`
13. `linear_field_contour_segments_lie_at_the_level`
14. `hidden_panel_samples_nothing_and_animated_refresh_is_throttled`
15. `z_formula_reports_the_scalar_unsupported_reason`

## Gates

`cargo build`, `scripts/test.sh`, clippy `-D warnings`, `check-docs`,
`git diff --check`, changed-file formatting; native smoke once the UI slice lands.
Independent review of the actual diff; browser/manual visual verification stays
owner-waived, not passed.
