# Native CAS rails design

Owner-approved scope: [GitHub #14](https://github.com/ttgg1/vector-field-explorer/issues/14),
including the owner's request to support more rails in future. This document
records the controller's bounded infrastructure proposal, not a docking system.
Implementation follows the [plan](../plans/2026-11-14-cas-rails.md) and
[Workflow](../../../references/workflow.md); the controller retains review/merge
ownership and stops after this issue is merged.

## Intent and approach

CAS must be discoverable and usable without opening Controls. Use an ordered
slice of rail entries with stable string IDs and independent expansion state;
app code supplies panel bodies. This costs a small reusable UI module, instead
of duplicating a special CAS rail or introducing a plugin/docking framework.
Only Controls and native CAS use the new left strip. The existing right Presets
panel/rail stays unchanged. A third entry exists only in integration tests.

Wide layout, left to right:

```text
32pt strip       independently expanded bodies             viewport   Presets
Controls toggle | Controls (when open) | CAS (when open) |             | unchanged
CAS toggle      |                     |                 |
```

Entries stay in the strip even when expanded; clicking an active entry closes
only its panel. CAS is immediately below Controls, including when Controls is
open. Expanded headers retain collapse chevrons. Both may be open, both closed,
and each width survives collapse/reopen. Expansion order is Controls then CAS;
IDs derive from stable entry ID plus existing `panel_generation`, never index,
label, or current expansion state. Existing native theme, frames, rotated rail
label style, chevrons and rendering remain.

## Geometry and extensibility

The reusable strip accepts arbitrary entries, without matching Controls/CAS IDs.
Its entries use the existing 14pt rotated label and 32pt outer rail width. Each
entry's vertical extent is at least 40pt and sufficient for measured rotated
label ink, chevron zone and padding; entries are not equal fractions of height.
Use vertical scrolling when their combined heights exceed the available space.
Clip painting and hit testing to the strip, retain full-label tooltips, and
scroll the third/long-label entry into view in tests. The narrow Wide strip keeps
its existing 32pt horizontal convention; Compact controls use the 40pt floor
in both dimensions. No new user-facing scaling option or dependencies.

Stored widths remain requested widths, not a cache of automatic squeeze widths.
At narrow Wide sizes, reserve 100pt for the viewport and account for visible
strips; expanded panels share the remaining width, preserving the existing
180–460pt range when feasible. Allocate each 180pt first, distribute remaining
space in proportion to requested excess over 180pt, and retain requested widths
when space suffices. If even the minimums cannot fit, divide the budget equally.
Only intentional resizing updates persisted widths, never automatic squeeze or
Compact reflow. Apply this budget to Controls, CAS and expanded Presets so a
second left panel cannot push the viewport offscreen at width 701pt.

## State, compatibility and routing

- Keep `LayoutState`'s five existing public fields, serde names and methods.
  Add native-only `cas_expanded: bool` (default false), `cas_width: f32`
  (default 260pt), and `toggle_cas`. Continue struct-level serde defaults.
  Sanitize CAS width through existing `clamp_width`; reset restores CAS closed,
  existing panels open, all default widths/help, and fresh panel-generation IDs.
- Old five-field RON layouts migrate by deserializing unchanged fields and
  defaulting CAS; no whole-settings fallback or schema-version rewrite. Bare
  legacy `AppStyle` migration remains. New settings round-trip both CAS fields;
  scratch/preview and compact selection are never persisted. Web has no CAS
  fields, state, menu entry, tab, sheet variant or module exposure.
- Move native `CasPanelState` from `Controls::cas` to `VfApp::cas`; initialize
  once. Removing it from Controls also prevents sidebar rebuilding from losing
  scratch state. Preserve the previous global **Reset all controls** behavior by
  resetting app-owned CAS explicitly in app dispatch; the CAS-local Reset still
  resets only scratch state. Closing panels and Reset layout preserve scratch.
- Add native `MenuAction::ToggleCas`, View → **Toggle CAS**, without a new
  shortcut. Route wide actions to persisted visibility and Compact actions to
  transient `SheetState::Cas`; leave Controls/Presets shortcuts unchanged.
- Compact retains one bottom sheet, its height/swipe/close behavior and viewport
  floor. Native tab order is Controls, CAS, Presets; wasm remains Controls,
  Presets. Use a horizontally scrollable tab row if measured text, padding and
  40pt targets cannot fit, reserving the close target outside the scroll area.
  Selecting an active tab closes it; another replaces it. Compact operations do
  not change wide visibility or widths. Mode transitions keep existing behavior.

## CAS boundary

`cas_panel::show(ui, state, panel)` becomes the directly visible panel body,
without a second collapsed header. Preserve Differentiate (x/y/z/t), Gradient,
Divergence, Curl, scalar/vector input and preview, Copy, errors, Reset and
explicit **Apply vector**. Keep `refresh`, `can_apply`, `apply`, request identity
and dimension semantics intact. Rendering/preview/copy/open/close never apply a
field; only a valid current vector Apply invokes `State::set_formula`.
Formula editing and the formula bar's next-frame synchronization remain intact.
No engine/evaluator, numeric, grammar, Cargo or renderer changes.

## Acceptance and verification

Two serialized tasks: generic strip/layout compatibility, then native CAS
assembly/routing. Tests live only in `tests/` and new modules register in
`tests/suite.rs`. Exercise production widgets with egui pointer/text events,
not only state setters. Prove arbitrary third-entry order/identity/independence,
scroll access and label containment; legacy restore/round trip/reset and resize
retention; actual rail/menu/compact toggles; CAS isolation and explicit Apply;
formula bar synchronization and scalar/error/stale-preview safety. Cover fonts
8/13/24, widths 320/360/701/1280 and a short 240pt strip.

Run full native build, all-target suite, clippy, changed-file formatting, docs,
diff and mandatory 15-second runtime smoke under Workflow's toolchain/resource
rules. Dedicated wasm and browser/manual visual/painted-ink checks are owner-
waived, **not passed**; existing automated layout/platform tests remain required.
Initial independent review is required for each task and final integration.
