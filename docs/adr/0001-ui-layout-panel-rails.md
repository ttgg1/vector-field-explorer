# Collapsible edge rails for the Controls and Presets panels

On 2026-11-08 the app opened at eframe's small default size, both side panels
were permanently visible, and appearance controls sat above the field controls
in the left sidebar. The user wanted an uncluttered plot with instant access to
the panels. We kept the three-region layout (top menu +
formula rows, left Controls, right Presets) but each side panel now collapses
to a thin edge rail; clicking the rail re-expands and pins the panel, and
visibility persists. Appearance moved to a Settings popover in the top menu
bar, the screenshot action moved to that bar, and a dismissible help overlay
documents the gestures. We rejected hide-only toggles (no affordance to
restore a hidden panel), hover-to-peek rails (accidental openings while aiming
at the viewport), and moving presets into the formula bar (loses the save/apply
workflow and the right-hand space). Consequence: the top bar becomes the host
for future File/Edit menus, and layout plus view state join style in
persistence.

Status: accepted
