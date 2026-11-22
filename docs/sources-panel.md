# Sources panel

The Sources panel builds a mixed electric/current scene and draws the field it
produces. It is an alternative to the formula field, not a second renderer:
arrows, camera, references, plane slicing and PNG capture are the same.
Source mathematics and its accuracy contract stay in the
[source accuracy notes](sources-accuracy.md).

## Field modes

The formula bar has a **Formula | Sources** switch while the vector plot is
active.

- **Formula** keeps today's typed expression, presets and CAS editor.
- **Sources** hides the formula editor and samples the source scene instead.
- Each mode keeps its own durable state. Switching never merges a formula
  vector and a source vector, and the 2D/3D projection is preserved.
- A source field is always three-dimensional. In 2D it appears as the selected
  plane slice, using the same axis/index selection as a three-component
  formula.

## Sources and display

All eight validated kinds are editable in the panel: point charge, finite line
charge, charged ring, charged disc, solid sphere, straight wire, current loop
and solenoid. Each kind shows only its relevant parameters.

**Display** chooses Electric, Magnetic or Both. In Both, each family uses its
Appearance colour: the shipped defaults are amber for electric and cyan for
magnetic, and either can be replaced in the Appearance panel. The two families
are never added together and do not share one magnitude scale; an empty family
simply contributes no arrows.

## Softening and accuracy

One **Softening** value applies to the whole scene and accepts `1e-6` through
`1.0` (default `0.1`). It is part of the teaching model: smaller values sharpen
near-source peaks and larger values smooth them. It is not a wire radius, disc
thickness, material model or physical regularization, and it is not per-source.

The evaluator reports independent electric and magnetic accuracy. A finite
result below the target is labelled **accuracy not met** and is hidden by
default. The **Show finite results labelled "accuracy not met"** opt-in makes
those finite results drawable; it cannot make non-finite or unavailable
results drawable, and an older saved payload can never enable it. The panel
status counts hidden, shown-with-accuracy-not-met and unavailable results per
family.

## Source glyphs and the selected slice

Every enabled source draws a line glyph in the viewport, coloured by family
(the family's Appearance colour, defaulting to amber for electric and cyan for
magnetic). A glyph marks the source's position,
axis and signed charge/current; it is a contextual marker, not a depth-tested
surface, so it may show through field geometry.

| Kind | Glyph |
| --- | --- |
| Point charge | Three-axis cross |
| Finite line charge | Axis segment with end ticks |
| Charged ring | Circle in the plane normal to the axis |
| Charged disc | Ring plus two diameters |
| Solid sphere | Three great circles |
| Straight wire | Axis line with chevrons showing current direction |
| Current loop | Circle plus right-hand-rule chevrons |
| Solenoid | Helix, capped visible turns, end circles and a direction chevron |

Positive sources get two perpendicular sign strokes; negative sources get one.
The glyphs share the scene's redraw path, so a domain, density, display or
plane change replaces them in the same rebuild as the arrows. There is no stale
glyph cache and no second scene model.

In 2D the glyph is projected into the selected plane at its in-plane location.
Sources off the selected slice are **dashed and translucent** rather than
hidden, so the slice does not pretend every source lies on it. On-slice and 3D
glyphs are solid. A source marker never moves the source or changes the sampled
field.

## Presets

The Presets panel has a separate **Sources (illustrative)** catalogue:

- Coulomb point
- Electric dipole
- Charged line
- Uniform ring
- Thin disc
- Charged sphere
- Infinite wire
- Current loop
- Finite solenoid
- Source gallery

Each preset is an illustrative scene at unit strength with softening `0.1`,
the accuracy opt-in off, and the canonical `[-2, 2]³` domain. Loading one
selects Sources, a 3D view and the scene through the ordinary validated
setters. These are not physical calibration claims. Classic formula presets
stay formulas; the existing 2D physics rows are labelled **2D approximation**,
and no preset conversion rewrites a saved formula or camera preset.

## Exports

Source PNG and SVG exports include the same warning context. PNG bakes a
bottom-left banner into the pixels and adds `vfe.*` text metadata; SVG adds a
visible warning and an escaped `<metadata>` block. The softened-model line is
always present in Sources mode. Missing or invalid sampling, unavailable
results, and finite results below the target each add their own line; the
accuracy opt-in never removes a warning. Formula exports keep their previous
bytes and semantics.

Browser and desktop export paths use the same contract. Source-mode CLI export
and any separate source animation clock are not part of this delivery.

## Formula-only features

Streamlines, particles and click seeding remain formula features in this
delivery. Sources mode draws static source arrows and glyphs only; the Features
section is hidden and the panel says so. There is no separate source clock.

## Presets, CAS and persistence

- Loading a classic field preset switches back to Formula before applying its
  formula. It does not clear or rewrite the saved source scene, so returning to
  Sources shows the untouched scene.
- Native CAS edits the formula field only. Apply is disabled in Sources mode
  with the explanation **CAS edits the formula field; switch to Formula.**
  Copy still copies the formula result, never a source scene.
- The source scene, display, softening, accuracy opt-in and field mode are saved
  in the existing view envelope. Missing keys open in Formula mode with no
  sources and the opt-in off; malformed source records are skipped with a
  one-line restore notice.
