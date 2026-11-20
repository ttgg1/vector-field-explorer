# Edit history and scoped reset contract (#86)

Status: accepted 2026-11-20 (owner authorized slice A1/A2 under this contract).

## Intent

Make every accepted user edit reversible with one undo, without cloning arbitrary
renderer or simulation state, and let a user reset one part of the scene without
losing the rest.

## Snapshot scope

A history entry is an immutable value snapshot of the *editable document*:

- `view: ViewRecord` — formula, mode, plot, domain, density, arrows, colour
  settings, display toggles, streamline and particle settings, as already captured
  for saved scenes;
- `parameters: ParameterSet` — named parameter values;
- `camera: SceneCamera` — orbit/2D camera pose;
- `appearance: AppearanceProfile` — viewport style, layer styles and colour
  mapping (palette colormap, optional custom ramp, scale mode, quantity).

Everything else is runtime state and is never recorded: simulation time, particle
positions, sampled arrows, GPU buffers and caches, layout/panel widths and
expansion, preset-panel query, messages, and open sheets. Undo restores the
document; it does not rewind time.

## Storage and bounds

- In-memory only: history does not survive an application restart and is not
  written to the settings envelope or to files.
- `MAX_HISTORY = 64` entries; recording past the cap evicts the oldest entry.
- One logical cursor over two bounded stacks (undo and redo); undo moves the
  cursor back, redo forward.
- Recording after an undo drops the redo tail (branching edits discard the
  abandoned future).
- Recording a snapshot equal to the current entry is a no-op, so a gesture that
  changed nothing does not consume history.

## Transactions

```text
begin(label)   // capture the pre-edit snapshot once
commit()       // if the document changed, record one entry; otherwise discard
cancel()       // discard the transaction, history unchanged
undo() / redo()// move the cursor and return the snapshot to apply
```

- A transaction is the unit of undo for continuous gestures: pointer drags,
  slider drags and repeated value edits between press and release are one event.
- `commit` on an unchanged document, or after a rejected edit, records nothing.
- Rejected edits (setters returning `Err`, invalid imports, clamped-to-same
  values) never touch the cursor.
- Frames, animation ticks and simulation steps never call `begin`/`commit`; they
  are not edits.
- File effects never replay: saving settings, exporting PNG/SVG/animation,
  copying text, writing camera presets and reading files are outside history.

## Mode, preset and load transitions

- A mode or preset transition is one recorded entry: undo returns the previous
  mode/preset document (including the camera it set).
- Loading a saved scene, applying an appearance profile, applying an imported
  palette/profile document and resetting a scope each record one entry.
- Loading never records intermediate states of its own internal steps.

## Scoped reset

Resets are explicit actions with four scopes, matching #86's acceptance criteria:

- `Color` — colour mapping only: colormap, custom ramp override, scale mode,
  quantity, auto-fit and manual range. Document state (formula, parameters,
  camera) and layer/viewport styling are untouched. Recorded as one history entry.
- `Camera` — the orbit/2D camera pose only, back to its documented default.
  Recorded as one history entry.
- `Simulation` — runtime state only: simulation time, particle trails/dots and
  click-burst streamlines. Not recorded in history, because runtime state is not
  part of the document snapshot; it is its own undo-free action with no effect on
  the document.
- `All` — the whole document (scene fields plus appearance), recorded as one
  history entry.

Resetting one scope leaves every other scope byte-identical, and each recorded
reset is a single history entry (undo restores exactly the pre-reset document).

## Application rules

- Undo/redo apply a snapshot without file IO, without writing presets, and
  without applying a profile through the panel; they mark the document dirty once
  when it actually changed.
- Keyboard defaults: `Ctrl+Z` undo, `Ctrl+Shift+Z` redo (platform modifier
  conventions follow the existing shortcut handling).
- Menu entries expose Undo, Redo and the three resets; unavailable actions are
  disabled rather than silently ignored.
- SVG export keeps the existing neutral blueprint; history has no effect on
  export content.

## Slices

- **A1** — pure bounded history model: the `EditSnapshot` value type over the
  existing document types, transaction API, cursor/redo/eviction semantics,
  `EditScope`, and focused unit tests. No UI or input wiring.
- **A2** — application wiring: shortcuts and menu, drag coalescing, scope resets,
  and the interaction tests `drag_is_one_event`, `frames_do_not_record`,
  `file_effects_do_not_replay`, `rejected_edit_keeps_history`,
  `reset_scope_preserves_other_state`.

Each slice needs its own claim, TDD evidence, independent review of the actual
diff and the standard gates.
