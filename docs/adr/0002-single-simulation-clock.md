# 0002: One simulation clock for all animation

Animated formulas, plots, and particles all advance from a single `SimClock`
instead of each feature owning a timer. Per-feature timers were considered and
rejected: they drift apart, pause inconsistently, and multiply the repaint
drivers, while a single clock keeps fields, trajectories, and particles in the
same simulated time and gives the transport one Play/Pause owner. The cost is
that every new animated feature must consume the clock and declare whether it
keeps the app repainting; a feature that needs its own time base requires a new
ADR. Status: accepted.
