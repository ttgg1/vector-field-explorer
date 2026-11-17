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

**Display** chooses Electric, Magnetic or Both. In Both, electric arrows are
amber and magnetic arrows are cyan. The two families are never added together
and do not share one magnitude scale; an empty family simply contributes no
arrows.

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

## Formula-only features

Streamlines, particles and click seeding remain formula features in this
delivery. Sources mode draws static source arrows only; the Features section is
hidden and the panel says so. There is no separate source clock.

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
