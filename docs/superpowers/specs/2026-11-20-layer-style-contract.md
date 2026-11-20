# Layer style contract (#84)

Status: accepted 2026-11-20 by owner decision (six questions answered in
session). Applies to issue #84, plan task 3 of the visualization plan.

## Purpose

Give each drawn layer and each physics family its own bounded appearance
override without weakening the colour semantics earned in #83. Global controls
stay the base; overrides only multiply opacity/width or replace colours that
are already fixed.

## Layers

| Layer | Draws |
| --- | --- |
| Arrows | Formula/Materials/Maxwell/Induction vector arrows and ODE trajectory arrows |
| Flow | Streamlines, left-click bursts, particle dots and trails |
| Sources | Source glyph lines (charges, wires, loops, solenoids) |
| Induction | Induction conductor glyphs and their arrows |
| Teaching scenes | Materials and Maxwell outlines, surfaces and arrows |
| Reference | Grid, domain box, coordinate axes |

Plot curves and surfaces keep their own panel; the plot viewport is out of
scope.

ODE trajectories are the arrow family for `FieldMode::Ode`, so they sit in the
Arrows layer rather than adding a seventh layer to the owner's six-layer
ruling.

## Families

`Electric` and `Magnetic` are a second axis inside Sources, Induction and
Teaching scenes. They do not split Arrows or Flow, whose colours are owned by
the #83 colour mapping.

## Attributes and precedence

Per layer and per family, an override may set:

- `opacity` — multiplier, `0.0..=1.0`;
- `width` — multiplier, `0.05..=4.0`;
- `color` — `[u8; 3]`, honoured **only** where the drawable already uses a
  fixed family colour (Both-mode source/induction glyphs and teaching-scene
  family glyphs). Mapped colours are never replaced.

Resolution is per attribute: global setting → family override → layer
override; the most specific non-default value wins. Defaults are neutral
(`1.0`, `1.0`, no colour), so an absent record renders exactly today's scene.

## Composition with existing controls

A Flow override multiplies the appearance the particle controls already
produce: opacity after `ParticleAppearance::opacity`, width after the dot
radius or trail width. Particle colour stays owned by the existing particle
colour controls; the Flow row has no colour override. The particle panel and
the Appearance panel both remain authoritative for their own fields, and
resetting either leaves the other unchanged.

Legends describe value-to-colour mapping, not styling, so they do not change
with layer overrides. Layer styling is a viewport and raster-export concern:
SVG keeps its line-blueprint semantics and does not apply overrides, and
provenance metadata is unaffected.

## Persistence and scenes

Overrides persist in the settings envelope (`layer_styles`, serde-defaulted,
following the `preset_ui` and `viewport_style` precedent). They are **not**
part of `SceneDocument`: a saved scene keeps view + parameters + camera, and
appearance bundles are #85 profiles. Old settings files load neutral.

Two mechanisms stay distinct. `commit_layer_styles` validates and rejects a
non-finite or out-of-range programmatic value, keeping the previous styles;
the UI sliders cannot produce one. Loading a stored record goes through
`LayerStyles::sanitized`, which clamps finite out-of-range values and resets
non-finite ones to neutral with a warning, so a corrupt envelope never fails
to load.

## UI

A new **Appearance** panel (right side, rail entry, collapsed by default) lists
one row per layer with opacity/width sliders, a family selector for the three
family-aware layers, a colour row only where a fixed colour exists, a per-row
reset and one global reset. No numeric text entry is required; sliders carry
the bounds.

## Safety

All values are finite and bounded; `validate` rejects and `sanitized`
repairs, as described under Persistence. Rendering must stay byte-identical
when every override is neutral.

## Delivery slices

1. **A1** — `src/layer_style.rs` model, validation, resolution and settings
   persistence; no UI or renderer effect.
2. **A2** — Appearance panel plus wiring for Arrows, Flow and Reference, with
   native smoke.
3. **B** — Sources, Induction and Teaching-scene wiring, fixed-colour
   overrides and their tests.

Each slice needs its own claim, TDD, gates and independent review.
