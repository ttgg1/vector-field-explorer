# Vector Field Explorer

A desktop app for exploring vector fields typed as formulas. This glossary names
the app's view, controls, and camera concepts.

## View and chrome

**Viewport**:
The GPU-rendered scene area where arrows, streamlines, and particles appear.

**View state**:
The persisted inputs that determine what the viewport shows: formula, 2D/3D
mode, domain, and the control values in the Controls panel. Restored on launch.
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
A named formula example listed in the grouped Presets panel; clicking one
loads its formula and 2D/3D dimension into the scene.
_Avoid_: preset (unqualified)

**Controls panel**:
The left side panel holding field controls, grouped into Display, Features,
Coloring, and Domain.

**Presets panel**:
The right side panel holding camera presets.

**Panel rail**:
The narrow edge strip left behind by a collapsed Controls or Presets panel;
clicking it expands the panel.

**Pinned panel**:
An expanded panel that stays open until collapsed; the app has no hover-preview
panels.

**Top menu bar**:
The row above the formula bar hosting View, Settings, Help, and the screenshot
action.

**Formula bar**:
The row holding the field-preset selector, the 2D/3D toggle, and the editable
formula.

**Settings popover**:
A non-modal panel opened from the top menu bar for appearance, help-overlay
visibility, and layout reset.

**Help overlay**:
Dismissible grey text in the viewport corner listing mouse gestures,
click-to-seed, and formula syntax; its visibility is a setting.

**Style**:
App-wide appearance (theme, accent, rounding, spacing, font size), persisted
apart from view state and layout.
