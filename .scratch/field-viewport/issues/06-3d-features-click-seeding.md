# 06: Streamlines + particles in 3D, click-to-seed

**What to build:** From the user's perspective: with 3D mode on, streamlines and particles run on the three-dimensional samples (depth-correct against the far-to-near painter's sort); clicking in the viewport seeds a burst of streamlines/particles at the picked spot — the click becomes a camera ray through the seam's pure input→camera-action mapping and is placed on the ray at the focal plane.

**Blocked by:** 04 (2D streamlines), 05 (particles + dots pipeline).

**Status:** COMPLETE (`ff5e708`, review clean; 3 minors parked for final review)

- [x] 3D sampling feeds streamline/particle paths — ruled: sort stays arrows-only, lines/dots keep insertion order (pre-existing contract)
- [x] Pure click→ray→seed-placement function RED→GREEN watched (`ray_seed` + `click_seed`, 4 tests)
- [x] Click seeding works in 2D too (ortho ray → domain z-mid plane)
- [x] Gates: build / clippy `-D warnings` / full `cargo test` green; smoke PASS; commit `feat: 3D features with click seeding`

**Rulings:** click bursts paint even with the Streamlines toggle off (a click is an explicit request; the toggle governs the ambient grid only). Depth sort stays arrows-only; extending far-to-near sorting to lines/dots is out of scope (parked).

**Dispatch notes:** brief = plan task 11 via `scripts/task-brief … 11`.
