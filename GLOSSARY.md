# Vector Field Explorer

A native and browser app for exploring vector fields typed as formulas. This
glossary names the app's view, controls, camera, motion, and physics concepts.

## View and chrome

**Viewport**:
The GPU-rendered scene area where arrows, streamlines, and particles appear.

**View state**:
The persisted inputs that determine what the viewport shows: formula or source
configuration, 2D/3D mode, domain, and the control values in the Controls panel.
Restored on launch.
_Avoid_: session state, app state (that also covers style and layout)

**Plane-by-plane mode**:
A 3D display mode that highlights one sampled slice of the field cube and fades
the rest, so the cube keeps its shape while a single plane reads clearly. The
slice is chosen by axis and grid index.
_Avoid_: slice mode, cross-section

**Plane slice**:
The 2D rendering of a three-component field: one selected plane in its own flat
local frame, with arrows, streamlines and particles tracing the field projected
onto that plane. The out-of-plane (normal) component is omitted. The 2D selector
and the 3D plane-by-plane mode share the same axis/index selection.
_Avoid_: 2D field, flattened cube

**Camera state**:
The live orientation, target, and zoom of the orbit camera. Resets to the
default on launch; changed by mouse gestures or by applying a camera preset.
_Avoid_: camera view

**Camera preset**:
A named, saved viewpoint that can be applied later.
_Avoid_: view preset, camera state

**Field preset**:
A named formula or source configuration listed in the grouped Presets panel;
clicking one loads its definition and 2D/3D dimension into the scene.
_Avoid_: preset (unqualified)

**Controls panel**:
The left side panel holding field controls, grouped into Display, Features,
Coloring, and Domain.

**Arrow length mode**:
How a sampled field magnitude becomes an arrow's world-space length: Proportional
(the longest arrow is the arrow length scale in grid cells), Uniform (every
arrow has the same length), or Magnitude (length is the field magnitude itself,
scaled by the magnitude scale). The retired `normalize` boolean migrated
`true` → Uniform and `false` → Proportional.
_Avoid_: normalize, normalize lengths

**Magnitude scale**:
The world-units-per-unit-of-magnitude factor used only in Magnitude arrow-length
mode, where it is the sole length multiplier. The arrow length scale applies
only to Proportional and Uniform.

**Presets panel**:
The right side panel holding field presets and saved camera views.

**Panel rail**:
The narrow edge strip left behind by a collapsed Controls or Presets panel;
clicking it expands the panel.

**Pinned panel**:
An expanded panel that stays open until collapsed; the app has no hover-preview
panels.

**Top menu bar**:
The row above the formula bar hosting File, Edit, View, Help, the Appearance
gear, and the screenshot action.

**Formula bar**:
The row holding the 2D/3D toggle and the editable formula.

**Appearance popover**:
A non-modal panel opened by `View > Appearance…` or the top-bar gear for
appearance settings (theme, accent, rounding, spacing, font size).

**Help overlay**:
Dismissible grey text in the viewport corner listing mouse gestures,
click-to-seed, and formula syntax; its visibility is a setting.

**Style**:
App-wide appearance (theme, accent, rounding, spacing, font size), persisted
apart from view state and layout.

**Accent preset**:
A fixed accent colour offered as a swatch beside the custom colour picker; it
sets the app accent but does not replace the picker.
_Avoid_: theme preset

## Field and motion

**Field**:
The vector function sampled over the domain, defined by a typed formula.
_Avoid_: vector field function, field expression

**Field source**:
A placed charge or current distribution (point, line, ring, disc, sphere, wire,
loop, or solenoid) that contributes to the field.
_Avoid_: emitter, object

**Superposition**:
The field at a point is the vector sum of the contributions of every visible
source. Electric and magnetic families are summed independently.
_Avoid_: blending, additive mode

**Softening**:
The scene-wide length scale that keeps analytic source fields finite at their
singularities; display-only, not a physical regularization or wire radius.
_Avoid_: epsilon, clamp

**Quasistatic**:
The induction teaching approximation that keeps Faraday's law but omits
displacement current, radiation, retardation, and free-charge electrostatics;
it is not a full Maxwell evolution.
_Avoid_: static, electrostatics

**Softened vector potential**:
The azimuthal potential `A_phi` of the selected loop or solenoid evaluated with
the shared softening, from which Induction reconstructs `E = -dA/dt`. It is an
analytic model result, not a time finite difference or a general inverse-Faraday
solution.
_Avoid_: vector field, EMF potential

**2D slice**:
The plane of a three-component field rendered in 2D, selected by axis and grid
index (x, y, or z). Existing two-component fields keep their ordinary flat X/Y
view and have no slice selection.
_Avoid_: plane-by-plane mode

**2D approximation**:
A field preset whose 2D form is a simplified formula rather than the z slice of
a 3D source model; labelled in the preset UI.
_Avoid_: 2D slice

**CAS**:
The optional symbolic tool that differentiates formulas and derives gradient,
divergence, and curl without solving equations. It edits the formula field only,
never a source, induction, or ODE scene; it is disabled outside Formula.
_Avoid_: solver, evaluator

**Source scene**:
The durable, persisted list of placed electric and magnetic sources plus its
display choice, scene-wide softening and accuracy opt-in. It is sampled only in
Sources field mode and is never merged with the formula.
_Avoid_: source preset, source configuration

**Field mode**:
The exclusive choice among Formula (a typed vector expression), Sources (the
editable source scene), Induction (the Faraday teaching demo), and Ode (the
precomputed trajectory workbench). Switching preserves every durable state and
never merges them; loading a classic preset returns to Formula.
_Avoid_: source toggle, plot kind

**Induction mode**:
The dedicated Faraday teaching adapter: one axisymmetric current loop or finite
continuous solenoid driven by a sinusoidal current, with separate E and B arrow
families sampled on the shared clock. It is arrows-only and never merges with
Formula or Sources.
_Avoid_: Maxwell mode, induction formula

**Induction scene**:
The durable, persisted one-conductor geometry and waveform: case, center, axis,
radius (plus solenoid length and turns), signed amplitude, frequency, display,
and accuracy opt-in. It is sampled only in Induction mode.
_Avoid_: source scene, induction preset

**Induction waveform**:
The sinusoidal current `I(t) = amplitude * cos(2*pi*frequency*t)` and its
analytic derivative. Zero amplitude or zero frequency makes the scene static and
gives exactly zero induced E.
_Avoid_: AC source, oscillator

**ODE workbench**:
The dedicated fourth concept mode for bounded dynamical-system teaching. It
accepts typed 2D/3D first-order equations or integrates the current formula
field, solves once with fixed-step RK4, and plays back precomputed trajectories
on the shared clock with an x/y phase portrait and time-series charts. It is a
trajectory viewer, not an adaptive or general solver.
_Avoid_: ODE solver, solver mode

**ODE scene**:
The durable, persisted workbench configuration: source (typed equations or the
current formula field), dimension, equation strings, seeds, duration, chart and
visible time-series variables. It is never merged with a formula or source
scene.
_Avoid_: ODE preset, phase state

**ODE seed**:
One finite initial condition of the ODE scene, stored as a world position. Each
seed produces its own trajectory and fixed palette colour; the scene allows up
to eight. This is not a streamline seed, which starts field-tangent tracing.
_Avoid_: initial point, streamline seed

**Trajectory**:
The precomputed solution curve for one ODE seed, stored as time-position
samples. It is not a streamline, which is everywhere tangent to the spatial
field.
_Avoid_: streamline, path

**RK4 step**:
One fixed advance of the classical fourth-order Runge-Kutta integrator used by
the ODE workbench, `h = 0.005`; production never adapts or subdivides it. The
existing particle integrator is separate.
_Avoid_: frame step, simulation tick

**Phase portrait**:
The ODE chart plotting trajectory state components against each other. The
workbench draws the x/y projection, including x/y in a 3D scene; it is not a 3D
phase cube.
_Avoid_: phase plot, state plot

**Time series** (ODE):
The ODE chart plotting selected `x`, `y`, and/or `z` components against the
integration time `t`, from the same precomputed samples as the phase portrait.
It is not the shared playback clock.
_Avoid_: waveform, trace

**E/B display**:
The Sources or Induction panel choice Electric, Magnetic or Both. Both draws E
amber and B cyan as separate families, without summation or one shared magnitude
scale.
_Avoid_: combined field, total field

**Accuracy opt-in**:
The default-off Sources or Induction setting that permits finite
AccuracyNotMet results to be drawn and labelled. It never makes non-finite or
unavailable values drawable, never suppresses a warning or cap, and restore
accepts only an exact persisted true.
_Avoid_: ignore accuracy, hidden results

**Domain**:
The axis-aligned box `[x_min,x_max] × [y_min,y_max] × [z_min,z_max]` over which
the field is sampled; 2D uses the x/y extents and the selected z slice (the z
midpoint by default).
_Avoid_: bounds, range (that is a control)

**Grid density**:
The number of samples per axis: n² in 2D and n³ in 3D.
_Avoid_: resolution

**Arrow**:
One sampled field vector drawn at its sample point, with a direction and a
length derived from the vector's magnitude or a uniform scale.
_Avoid_: vector glyph

**Arrow style**:
The shape family used to draw arrows in the 3D view: Cone (tapered shaft with
a round head), Cylinder (plain capped cylinder, no head), Flat (headed
camera-facing ribbon), or Line (thin headless camera-facing ribbon, like a
streamline segment), together with its geometry parameters: head length and
width, shaft radius, and sides. The Line family persists as `Streamline`.
_Avoid_: arrow type, arrow texture

**2D arrow style**:
The separate, persisted shape-family selection used to draw arrows in the 2D
view (`ArrowStyle2D`): Flat (headed ribbon) or Line (headless thin segment; it
persists as `Streamline`). 2D never reads the 3D Arrow style set.
_Avoid_: 2D arrow type, flat arrow

**Plane dimming**:
The greyscale and opacity applied to arrows that are not on the selected plane
in plane-by-plane mode, so the selected slice reads clearly.
_Avoid_: slicing opacity, cross-section fade

**Streamline**:
A curve everywhere tangent to the field, traced forward from a seed and drawn
as a polyline. Seeds come from the sampling grid or from a click.
_Avoid_: trace, flow line

**Particle**:
A point carried through the field over time, advancing with the simulation
clock and respawning when it leaves the domain or stagnates.
_Avoid_: dot

**Particle size**:
The dot radius as a multiple of the base grid-spacing fraction, so it scales
with the grid and the user can tune visibility independently of density.
_Avoid_: dot scale, point size

**Particle count**:
The explicit number of particles in the cloud. Zero is **Auto** (density ×
16, capped); any other value overrides the density-derived count.
_Avoid_: particle limit, dot budget

**Particle trail**:
The fading history of a particle's recent positions, drawn behind it.
_Avoid_: trace, tail

**Simulation clock**:
The single time source for animated formulas, plots, particles, induction
fields, and ODE trajectory playback. It owns the current simulation time,
whether time is playing, and the speed multiplier. ODE playback has no private
timer or loop period of its own.
_Avoid_: animation timer, per-feature clock

**Scalar animation**:
A time-dependent scalar curve or surface resampled from the one `SimClock` as
the shared time `t` advances; static plots remain idle and are not rebuilt just
because the clock runs for another consumer.
_Avoid_: scalar timer, separate plot clock

**Transport**:
The Play/Pause, reset-time, time readout, and speed controls for the simulation
clock; shown when at least one feature consumes the clock.
_Avoid_: playback bar, timeline

**Fly camera**:
A free six-degrees-of-freedom camera mode for the 3D view, moved with physical
WASD/QE positions and drag-to-look, alongside the orbit camera.
_Avoid_: freecam, FPS camera

**Seed mode**:
How streamlines get their seeds: Auto uses the sampling grid and also accepts
click seeds; Click uses only click seeds.
_Avoid_: seed source, seeding toggle

**Streamline count**:
The requested number of Auto-seeded streamlines. `0` means Auto, using the
density-derived lattice; `1–4096` seeds exactly that many lines from a
density-independent lattice. Click seeds are additional.
_Avoid_: streamline density, line count

**Left-click action**:
What a single left click in the viewport does: seed a streamline or add a
particle burst. The two are exclusive.
_Avoid_: click mode, click behavior

## Physics (future scope)

The following concepts are not implemented in v0.1.0; they are reserved for
the post-v1 physics work (ADR 0003). Implemented terms — field mode, field
source, superposition, 2D slice, 2D approximation, softening, CAS, and the ODE
workbench concepts — are defined under Field and motion.

**Source configuration**:
A named, persisted set of field sources that can be loaded into source mode.
_Avoid_: source preset, scene

**Source glyph**:
The marker drawn at a source's position and orientation in the viewport, with
its kind, family and signed charge/current. In 2D it is projected into the
selected plane; off-slice sources are dashed and translucent, not hidden.
_Avoid_: icon, marker

**Source preset**:
An illustrative built-in SourceScene in the separate Sources catalogue,
loaded through the ordinary validated setters. Never a formula preset.
_Avoid_: source configuration, formula preset

**Material region**:
A region with relative permittivity and permeability that modifies the field.
_Avoid_: medium, dielectric

**Bound field**:
The field contribution from polarised or magnetised matter in a material
region, shown separately from the applied field.
_Avoid_: induced field (that is the Faraday field)

**Induced field**:
The electric field produced by a time-varying magnetic field, shown separately
from B and from bound matter. In Induction it is reconstructed as `E = -dA/dt`
from the softened axisymmetric vector potential; arbitrary B alone cannot
determine it.
_Avoid_: EMF field

**Maxwell mode**:
A conductor-focused field mode offering canonical geometries and free 2D
conductor outlines, with validity notes.
_Avoid_: electrostatics mode

**Canonical geometry**:
A named, closed-form Maxwell configuration (parallel plates, coaxial cylinder,
hollow sphere, infinite wire, or solenoid) with editable parameters.
_Avoid_: preset geometry

**Conductor outline**:
A free-form 2D conductor boundary treated as an infinite cylinder; v1 solves it
approximately and labels the approximation.
_Avoid_: shape, polygon
