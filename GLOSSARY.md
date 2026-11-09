# Vector Field Explorer

A native and browser app for exploring vector fields typed as formulas or built
from placed sources. This glossary names the app's view, controls, camera,
motion, and physics concepts.

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
The right side panel holding camera presets.

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
The row holding the field-preset selector, the 2D/3D toggle, and the editable
formula.

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
The vector function sampled over the domain, defined either by a typed formula
or by placed sources.
_Avoid_: vector field function, field expression

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
The single time source for animated formulas, plots, and particles. It owns the
current simulation time, whether time is playing, and the speed multiplier.
_Avoid_: animation timer, per-feature clock

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

**Left-click action**:
What a single left click in the viewport does: seed a streamline or add a
particle burst. The two are exclusive.
_Avoid_: click mode, click behavior

## Physics

**Formula mode**:
The field is defined by a formula typed in the formula bar and evaluated per
sample. Exclusive with source mode.
_Avoid_: expression mode

**Source mode**:
The field is defined by placed sources and their superposition. Exclusive with
formula mode.
_Avoid_: object mode

**Field source**:
A placed charge or current distribution (point, line, ring, disc, sphere, wire,
loop, or solenoid) that contributes to the field.
_Avoid_: emitter, object

**Source configuration**:
A named, persisted set of field sources that can be loaded into source mode.
_Avoid_: source preset, scene

**Superposition**:
The field at a point is the vector sum of the contributions of every visible
source.
_Avoid_: blending, additive mode

**Source glyph**:
The marker drawn at a source's position and orientation in the viewport.
_Avoid_: icon, marker

**2D slice**:
The z coordinate sampled in 2D mode; v1 exposes the z slice only, and x/y slice
selection comes later.
_Avoid_: plane, cross-section

**2D approximation**:
A field preset whose 2D form is a simplified formula rather than the z slice of
a 3D source model; labelled in the preset UI.
_Avoid_: 2D slice

**Softening**:
The global length scale that keeps analytic source fields finite at their
singularities; display-only, not a physical regularization.
_Avoid_: epsilon, clamp

**CAS**:
The optional symbolic tool that differentiates formulas and derives gradient,
divergence, and curl without solving equations.
_Avoid_: solver, evaluator

**Material region**:
A region with relative permittivity and permeability that modifies the field.
_Avoid_: medium, dielectric

**Bound field**:
The field contribution from polarised or magnetised matter in a material
region, shown separately from the applied field.
_Avoid_: induced field (that is the Faraday field)

**Induced field**:
The electric field produced by a time-varying magnetic field, shown separately
from the applied field and from bound matter.
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

**ODE system**:
A user-defined first-order system `dx/dt = f(x,t)` integrated from seeds into
trajectories.
_Avoid_: flow, field

**Trajectory**:
A solution curve of an ODE system from one initial condition.
_Avoid_: streamline (that is a field-tangent curve)

**Phase portrait**:
A plot of trajectory state components against each other.
_Avoid_: phase plot
