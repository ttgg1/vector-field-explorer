# 07: Style panel + persistence

**What to build:** From the user's perspective: a style panel groups the look knobs (colormap, color range, density, arrow scale, normalize) and the app remembers the last-used style across restarts — persisted beside the existing camera-preset file in the app data dir, so quitting and relaunching restores the same look.

**Blocked by:** 03 (the knobs to persist must exist and be seam-addressable).

**Status:** ready-for-agent

- [ ] Pure serde round-trip for the style record, RED→GREEN watched; corrupt-file behavior ruled in the ledger (precedent: camera presets fail fast with io::Error — decide same or fall-back-to-defaults and ledger the ruling)
- [ ] Style panel reads from / writes to the same seam State the sidebar drives (no parallel truth)
- [ ] Survives restart: load on startup applies to initial State
- [ ] Gates: build / clippy `-D warnings` / full `cargo test` green; commit `feat: style panel with persistence`

**Dispatch notes:** brief = plan task 12 via `scripts/task-brief … 12`. Note the two `Preset` types in this repo (camera-preset vs field-preset table) — the style record is a third, distinct type; keep names distinct.
