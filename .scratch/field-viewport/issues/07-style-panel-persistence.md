# 07: Style panel + persistence

**What to build:** From the user's perspective: a collapsible "Style" section at
the top of the sidebar offers theme (dark/light), accent color, corner rounding,
spacing, and font size. Every change applies to the whole UI immediately; the
app remembers the last-used style across restarts through eframe's persistent
app-state storage (`cc.storage`, `eframe::APP_KEY`).

**Blocked by:** nothing (app-level UI config; it does not touch the field
`State` seam).

**Status:** COMPLETE (2026-11-08, commit `3db30e1`, review clean: 0 critical/important; 3 minors parked for ticket 09)

**Ruling 2026-11-08 (controller):** `.scratch/field-viewport/spec.md` lines
32–33 and 98–101 and plan task 12 are binding. An earlier summary in this ticket
listed colormap / color range / density / arrow scale / normalize as the style
record — those are sidebar field controls delivered by ticket 03, not the style
panel. Corrected here. Persistence uses eframe storage, not `presets.json`
(spec: "persisted through the persistent app-state storage the framework already
provides, keyed consistently with the existing app key"). Persist the style
record only; the plan's broader `AppSettings { formula, mode, controls, style }`
envelope is out of scope for this ticket (formula/mode persistence is not in the
spec's user stories) — leave room to extend the envelope later.

- [x] `AppStyle { theme_dark: bool, accent: egui::Color32, rounding: f32,
  spacing: f32, font_size: f32 }` with serde derives and defaults (dark theme,
  teal accent ≈ `#009688`); serde round-trip test RED→GREEN; absent or corrupt
  stored value falls back to the defaults — **ruling: cosmetic UI state must
  never brick startup** (camera presets stay fail-fast for data integrity)
- [x] `apply(ctx: &egui::Context, style: &AppStyle)` pure mapping test on a
  fresh `egui::Context` (no GPU): dark/light `visuals.dark_mode`, accent in
  `visuals.selection.bg_fill`, `spacing.item_spacing`, body font size; apply
  must be idempotent because it runs every frame
- [x] Style section at top of the sidebar: theme toggle, accent color picker,
  rounding / spacing / font-size sliders; changes restyle the same frame
- [x] Load on startup from `cc.storage`; `eframe::App::save` writes the style;
  restart restores it; camera presets and their panel are untouched
- [x] Gates: `cargo build --all-targets`, `cargo test --all-targets`, clippy
  `--all-targets -- -D warnings`, `scripts/smoke-run.sh 15`; commit
  `feat: style panel with persistence`
- [x] Ledger the corrupt-storage ruling and the style-record-only scope

**Dispatch notes:** brief = plan task 12 via `scripts/task-brief … 12`, but the
ruling above narrows the plan's envelope. egui 0.37 API notes: `App` trait has
`fn ui`, not `update`; `CornerRadius` (not `Rounding`); use
`ctx.all_styles_mut` to set both themes deterministically; `eframe::Storage`
trait has `get_string` / `set_string`. Two other `Preset` types exist in the
repo; the style record is a fourth distinct name — keep names distinct.
