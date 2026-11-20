# Interaction profiles contract (issue #88)

Accepted 2026-11-20. Owner decisions: named profiles with a built-in Default
and portable JSON; one chord per action with replace/clear conflict handling;
Escape, undo and redo reserved; sensitivity/zoom multipliers; a dedicated
right-side Interaction panel; logical keys with `Mod`.

This contract bounds slices A1 (model), A2 (persistence and panel) and A3
(input wiring). It does not authorize changing camera constants, adding
gesture recognition or replaying input.

## Action model

Bindings are action-based, never raw events. `InteractionAction` covers the
remappable actions that exist today:

- Panels and surfaces: controls panel, presets panel, appearance panel,
  interaction panel, help.
- View: reset, fit, zoom in, zoom out, view left/right/up/down.
- Simulation: play/pause.
- Tools: toggle fly, seed streamline, add particles.
- Files: screenshot export, copy formula, quit.

Reserved actions are never remappable and are shown read-only:
`Escape` (dismiss/cancel/leave capture), `Mod+Z` (undo) and `Mod+Shift+Z`
(redo).

`KeyChord { key, modifiers }` is a logical egui key plus a modifier set
`{ Mod, Shift, Alt }`, where `Mod` is Ctrl on Linux/Windows and Command on
macOS. Physical key positions and raw egui events are out of scope.

## Bindings and conflicts

- `MAX_BINDINGS` equals the number of remappable actions; a binding table maps
  each action to exactly one chord, so a profile can never be partially bound.
- A chord may bind only one action. Assigning a chord already in use reports
  the owning action; the UI offers `Replace` (swap the two chords atomically,
  so both actions stay bound and no duplicate exists) or cancelling the edit.
  There is no silent takeover and no duplicate binding.
- Every remappable action always has a chord. `Reset action` restores that
  action's default chord; it is not a way to leave an action unbound.
- Chords that a browser owns (`Mod+W`, `Mod+T`, `Mod+N`, `Mod+Shift+W`) are
  refused with a stated reason on every platform, so a saved profile behaves
  the same natively and in wasm.
- Chords are matched only while no text field has focus; the existing
  typing-focus protection is preserved.
- Defaults reproduce today's bindings exactly. `Restore defaults` always
  exists, ignores profile contents and never requires a valid stored profile.

## Camera feel

`CameraFeel { orbit_sensitivity, zoom_speed, invert_yaw, invert_pitch,
invert_zoom }`:

- `orbit_sensitivity` and `zoom_speed` are multipliers in `0.25..=4.0`,
  default `1.0`; they scale existing constants rather than replace them.
- Invert flags default to `false` and flip the sign of the corresponding
  orbit/look or wheel input only.
- Validation rejects non-finite and out-of-range values; loading sanitizes to
  the default and reports a warning. Fly speed keeps its own existing bounds
  and is not part of this record.

## Profiles and storage

`InteractionProfile { name, bindings, camera_feel }` with
`MAX_PROFILE_NAME_BYTES = 48` and `MAX_USER_PROFILES = 16`.

- Built-in `Default` is the only built-in profile; it equals the documented
  defaults and cannot be deleted or overwritten.
- User profiles are validated in full before use; `sanitized` repairs a stored
  record (name trim/truncate, clamp multipliers, reset non-finite, restore any
  missing default binding) and is total.
- Portable documents are versioned (`INTERACTION_FORMAT_VERSION = 1`),
  `deny_unknown_fields`, reject a newer version before parsing profiles,
  reject duplicate names, and import atomically — a failure leaves the library
  untouched.
- Profiles persist in the settings envelope (`AppSettings`), not in scenes.
  Startup restores them without applying a profile or dirtying the scene.
- Applying a profile is explicit, atomic and appearance-neutral: it replaces
  bindings and camera feel only. It never changes the document, camera pose,
  simulation or layout.

## Input safety

- Escape and pointer-capture recovery work with any stored or imported
  profile, including one that has been hand-edited.
- Losing window focus clears held movement keys; a chord never stays latched.
- Remapped bindings respect the existing `text_edit_focused()` gate and the
  existing per-mode gating (2D/3D, fly, panel visibility).
- A profile that cannot be validated is never applied; the current bindings
  stay active and the rejection is reported.

## UI

A dedicated right-side Interaction panel (rail, compact sheet tab, menu entry
and `Ctrl+Shift+I`), containing:

1. Bindings: one row per remappable action with the current chord, a rebind
   capture mode, conflict text and a reset action; reserved rows read-only.
2. Camera feel: the two multipliers and three invert toggles with a reset.
3. Profiles: built-ins and user profiles with apply/delete, save current,
   restore defaults, and portable import/export/copy text.

Panel drafts and messages are session-only. Width, collapsed state and compact
tab follow the existing panel pattern and persisted layout rules.

## Slices

- **A1** — pure model: `InteractionAction`, `KeyChord`, `Bindings` with
  defaults, conflict/assign/clear, reserved handling, `CameraFeel`,
  `InteractionProfile`, portable `ProfileDocument` and `ProfileLibrary`
  (bounded, duplicate-rejecting, atomic import). No UI, input or persistence.
- **A2** — settings-envelope persistence, startup restore, and the Interaction
  panel with bindings, camera feel and profiles.
- **A3** — route the existing shortcut table, view keys, fly keys, transport
  and click modifiers through the active bindings; apply camera feel to orbit,
  wheel zoom, fly look and pointer drags; preserve Escape/undo/redo and the
  typing gate.

## Pinned tests

- Defaults equal today's chords and give one chord per action.
- Assigning an in-use chord reports the owner and changes nothing until
  replaced; replace updates both actions in one step.
- Reserved actions cannot be rebound or cleared; `Restore defaults` works with
  a corrupt stored profile.
- Browser-owned chords are refused with a reason.
- Multipliers clamp, non-finite values sanitize, invert flags round-trip.
- Profile validation, duplicate rejection, cap, atomic import, version
  rejection and compact JSON round trip.
- Applying a profile changes bindings and camera feel only.
- Typing focus suppresses single-key chords; focus loss clears held keys.
