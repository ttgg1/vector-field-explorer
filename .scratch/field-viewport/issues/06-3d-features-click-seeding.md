# 06: Streamlines + particles in 3D, click-to-seed

**What to build:** From the user's perspective: with 3D mode on, streamlines and particles run on the three-dimensional samples (depth-correct against the far-to-near painter's sort); clicking in the viewport seeds a burst of streamlines/particles at the picked spot — the click becomes a camera ray through the seam's pure input→camera-action mapping and is placed on the ray at the focal plane.

**Blocked by:** 04 (2D streamlines), 05 (particles + dots pipeline).

**Status:** ready-for-agent

- [ ] 3D sampling feeds streamline/particle paths; sort keeps lines/dots ordered with arrows (existing CPU far-to-near contract)
- [ ] Pure click→ray→seed-placement function RED→GREEN watched (unproject helpers already exist behind the camera module)
- [ ] Click seeding works in 2D too (ray degenerates to the ortho plane) — reviewer requirement only if cheap per brief task 11
- [ ] Gates: build / clippy `-D warnings` / full `cargo test` green; commit `feat: 3D features with click seeding`

**Dispatch notes:** brief = plan task 11 via `scripts/task-brief … 11`.
