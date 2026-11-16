# Scalar plots: curves and surfaces

Vector Field Explorer can plot a scalar right-hand side as a 2D curve
`y = f(x)` or a 3D surface `z = f(x, y)`. Scalar plotting is a desktop GUI and
native PNG feature. It is not a command-line mode, physical solver, or
unit-aware simulation. Curves and surfaces may use the shared simulation time
`t`, so Play/Pause animates them from the same clock as particles and
time-dependent vector fields.

The exact built-in examples and the settled design are in the
[scalar plotting spec](superpowers/specs/2026-11-15-scalar-plotting-design.md);
the implementation plan is in the
[scalar plotting plan](superpowers/plans/2026-11-15-scalar-plotting.md).

## Syntax

Choose **Curve** or **Surface** in the Plot panel. The editor holds only the
right-hand side; the UI supplies a non-editable `y =` or `z =` label.

- Curves use `x`, or `x` and `t`: `0.5*x^2`, `exp(-x^2)*cos(8*x)`,
  `sin(t)*x`.
- Surfaces use `x` and `y`, or `x`, `y`, and `t`:
  `1/(sqrt(x^2+y^2)+0.1)`, `exp(-(x^2+y^2))`, `sin(3*x)*cos(3*y)*sin(t)`.
- Ordinary `meval` functions and constants are available. Unknown names,
  assignment syntax, and variables outside the active context are rejected with
  an error naming the problem.
- Expressions are pure and dimensionless. `t` is the shared elapsed simulation
  time; there is no separate scalar timer, and expressions never reference the
  vector field or physical units.

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

The built-in catalogue contains these exact examples:

| Kind | Name | Expression |
| --- | --- | --- |
| Curve | Harmonic oscillator potential | `0.5*x^2` |
| Curve | Gaussian wave packet | `exp(-x^2)*cos(8*x)` |
| Curve | Damped oscillator | `exp(-0.2*abs(x))*sin(3*x)` |
| Surface | Point-charge potential | `1/(sqrt(x^2+y^2)+0.1)` |
| Surface | Two like charges | `1/(sqrt((x-0.6)^2+y^2)+0.1) + 1/(sqrt((x+0.6)^2+y^2)+0.1)` |
| Surface | Quantum oscillator ground state | `exp(-(x^2+y^2))` |
| Surface | Standing wave | `sin(3*x)*cos(3*y)` |
| Surface | Radial outgoing wave | `sin(6*sqrt(x^2+y^2))/(1+sqrt(x^2+y^2))` |

These are illustrative dimensionless formulas. The charge examples use an
epsilon of **0.1** to keep values finite near the singularities; the epsilon is
not a physical permittivity and the examples are not a numerical solver.

## Drafts, last-valid plots, and restarts

Curve and surface text are independent drafts. They do not overwrite each
other, the vector formula, or a persisted vector preset.

- Typing a valid expression replaces that context's accepted expression and
  resamples it.
- An invalid edit in the active context shows the parse error while that
  context's last valid plot remains visible.
- Switching to a context whose draft is invalid shows that context's error and
  no plot. It never borrows another kind's cached geometry.
- Returning to **Vector Field** restores the preserved vector expression,
  settings, and projection; scalar drafts remain available for the next switch.
- After restart, saved drafts are restored, but only accepted expressions have
  geometry. An invalid restored draft shows its error until corrected; no
  hidden substitute expression is persisted.
- Choosing a vector preset returns explicitly to vector mode. CAS remains a
  vector-only tool; its Apply action is disabled in scalar mode.

## Controls

The Plot section exposes only the controls relevant to the selected kind:

- **Curve:** sample count and line width in **world units**.
- **Surface:** one square per-axis resolution, not separate X/Y sliders.
- shared height range/auto mode, colormap, grid and axes, and camera controls.
- **Fit plot** frames the finite rendered geometry.
- **F** remains **Fit domain**: it frames the domain box, not the curve or
  surface extrema.
- The plot-section reset restores plot settings and the default draft without
  erasing the vector formula. Global Reset returns to vector mode.

Height color is signed `y`/`z` from the displayed range. Manual color limits
clamp colors only; they never clamp or invent heights. Automatic range uses
finite rendered geometry, not missing samples.

## Gaps, caps, and open meshes

Sampling is bounded and deterministic:

- Curves request **2..4097** slots (default **513**). Only adjacent present
  samples form segments. A bounded midpoint probe may reject an edge whose
  spread looks discontinuous, using at most six subdivision levels and 127
  probes per edge pair. This is a display heuristic, not a continuity proof.
  Extremely steep continuous curves can therefore show false gaps, and some
  poles between samples can still be missed.
- Surfaces use an inclusive XY grid with a default **64×64** vertices and a
  maximum of **96×96** vertices per axis. A cell is emitted only when all four
  corners are finite and nondegenerate; a non-finite cell-center probe also
  omits it. Missing cells remain holes with conservative margins. Coarse
  surface meshes can therefore show visible holes or false gaps where a fine
  grid would not.
- Missing or non-representable values are absent geometry, never replacement
  zeroes or fake origin/fill vertices.

A surface is an open one-sided graph, not a closed solid. Its front face is
counter-clockwise and backface culling is enabled. Viewing the back of a
surface can legitimately make it disappear; there are no side walls, bottom,
or hole caps.

## Export and platform scope

Scalar plots support **PNG** capture through the isolated depth-equipped plot
pass. SVG export supports vector fields only, so both scalar kinds are disabled
with the message `Scalar plots require PNG; SVG export supports vector mode`.
The reason is that the vector SVG exporter treats line width as pixel strokes,
while live scalar curves use world-unit widths; pretending otherwise would
produce a different plot. There is no raster-in-SVG or wireframe substitute.

There are no scalar command-line flags; the CLI remains explicitly vector-only.
The layout, sampling, and PNG capture paths retain automated native tests, but
dedicated wasm/browser compatibility and manual visual/screenshot checks are
not part of this feature's accepted evidence. See the CLI guide for vector
export flags.
