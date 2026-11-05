# Vector Field Explorer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rust desktop app where the user types a vector-field formula and sees arrows, streamlines, and particles for it live, in 2D and 3D, with a resizable modern egui UI and GPU rendering.

**Architecture:** eframe/egui 0.37 as UI shell; all scene drawing (2D and 3D) through one instanced wgpu renderer registered via `egui_wgpu::CallbackTrait` — 2D differs only by an ortho camera. Formula parsing (`meval`), sampling, colormaps, cameras, RK4 streamlines, and the particle system are plain Rust modules with unit tests; the wgpu pipeline and UI wiring are verified by building/running the app.

**Tech Stack:** eframe/egui 0.37 (`wgpu` + `persistence` features), meval 0.2, glam, colorous, image, rfd.

**Spec:** `docs/superpowers/specs/2026-11-05-vector-field-explorer-design.md`

## Global Constraints

- Rust edition 2024 (eframe 0.37 requires edition 2024 or `resolver = "2"`).
- GUI deps: `eframe = { version = "0.37", default-features = false, features = ["wgpu", "persistence", "wayland", "x11", "accesskit", "system_fonts"] }`.
- `wgpu` types always via `eframe::wgpu` re-export; never a separate `wgpu` dependency version.
- Other deps: `meval = "0.2"`, `glam = "0.30"`, `colorous = "1"`, `image = "0.25"`, `rfd = "0.15"`.
- Domain default extent `[-1, 1]` per axis; 2D grid cap 200×200; 3D grid cap 40³; particle cap 5000.
- NaN/∞ at a sample point: skip the arrow (clamp magnitude first); planes with all-degenerate points render an empty-but-valid scene.
- Project root: `/home/paul/projects/vector-field-explorer`; git repo initialized in Task 1, commit after every task.

## Review Focus

Thes inputs most likely to bite a user; each is pinned by a test in its owning task:

1. Formula with a pole inside the domain (e.g. `1/x`) — app must skip those arrows, not crash or blank: Task 2 test `eval_nan_becomes_none`, Task 5 test `sample_skips_nan`.
2. 3D formula viewed in 2D mode — `(x,-y,z)` must silently use `(u,v)`: Task 2 test `three_component_evaluated_in_two_d`.
3. Malformed input mid-typing (e.g. `(x, sin(`) — parse error surfaces in the status bar and the previous field stays rendered: Task 2 tests, Task 8 wiring.
4. Degenerate field with zero magnitude everywhere (e.g. `0`) — normalize must not divide by zero: Task 5 test `all_zero_field`.
5. RK4 through a stiff/vortex field must not blow up in a step across the singularity — step is capped and degenerate steps end lines: Task 9 test `vortex_stays_bounded`.

---

### Task 1: Project scaffold + eframe shell

**Files:**
- Create: `Cargo.toml`, `src/main.rs`, `src/app.rs`, `src/ui/mod.rs`, `src/ui/formula_bar.rs`, `src/ui/status_bar.rs`, `src/ui/sidebar.rs` (stub), `docs/superpowers/specs/…` (already present)

**Interfaces:**
- Produces: `app::VfApp` struct implementing `eframe::App`; `formula_bar::show(ui, app_state)` and `status_bar::show(ui, app_state)` take `&mut StatusBarState`; `sidebar::show(ui, &mut Controls)`.

- [ ] **Step 1:** `cargo init` in the project root; write `Cargo.toml` with the Global-Constraints deps, edition 2024. Add `src/main.rs` booting eframe with `NativeOptions { renderer: Renderer::Wgpu, ..Default::default() }` and a `VfApp` holding placeholder state: `formula: String`, 2D/3D mode toggle, `Controls` (density, arrow_scale, normalize, colormap, color_range, extent, toggles), `StatusBarState { parse_message, instance_count }`.
- [ ] **Step 2:** Build the UI layout exactly (data wiring comes later): top horizontal formula bar with a text edit + 2D/3D toggle buttons; `SidePanel::left().resizable(true).min_width(180.0).max_width(480.0)` containing three `egui::CollapsingHeader` stubs (Display, Coloring, Features) plus a Domain section; `CentralPanel` with a `egui_wgpu` paint-callback *placeholder* rect (for now, draw `ui.painter().rect_filled`); bottom status bar panel showing `parse_message` / `instance_count`.
- [ ] **Step 3: Verify build and launch.** Run: `cargo build`. Then run `cargo run` with a timeout — app window must open over Wayland without panicking, kill it afterwards. If it cannot open, stop and fix environment before proceeding.
- [ ] **Step 4:** `git init`, add `.gitignore` (`/target`), commit `feat: eframe shell with resizable sidebar and placeholder panels`.

### Task 2: Formula field module (parsing + evaluation)

**Files:**
- Create: `src/field.rs`
- Test: unit tests inside `src/field.rs`

**Interfaces:**
- Produces: `pub enum FieldError { Parse(String) }`; `pub struct FieldExpr { pub dimension: usize, components: Vec<meval::Expr> }`; `fn FieldExpr::parse(src: &str) -> Result<Self, FieldError>`; `fn FieldExpr::eval3(&self, p: glam::Vec3) -> Option<glam::Vec3>` (None for any NaN/∞; for `dimension == 2` uses `p.xy` and returns `Vec3::new(u, v, 0.0)`); `fn FieldExpr::eval2(&self, p: glam::Vec2) -> Option<glam::Vec2>`.

- [ ] **Step 1: Write the failing tests** in `src/field.rs` under `#[cfg(test)]`:
  - `parse_two_component`: `parse("(sin(y), x*cos(y))")` → dimension 2; `eval2` at (1,0) equals approximately (0, 1).
  - `parse_three_component`: `parse("(x, -y, z)")` → dimension 3.
  - `three_component_evaluated_in_two_d`: dimension-3 field's `eval2` at (1,0) is (1,0).
  - `top_level_comma_only_must_not_split`: `parse("(max(0,1), x)")` parses (first component uses an inner comma).
  - `err_on_invalid`: `parse("(x, sin(")` → `FieldError::Parse` whose message contains the offending component text `sin(`.
  - `eval_nan_becomes_none`: field `(1/x, 0)` at x=0 → `None`.
  - `unknown_variable_not_evaluated`: field using `w` parses but `eval*` returns `None` (meval reports free variables; treat unknown-variable evaluation as None).
- [ ] **Step 2: Run:** `cargo test field` → compile/test failure (module missing).
- [ ] **Step 3: Implement.** Splitting: scan top-level commas (track paren depth); zero components → parse the whole string as a single component treated as the u-component? No — plain single scalar `x` means a 2D field whose v-component is 0 only if the user meant it; instead: 1 component = parse error "expected 2 or 3 components". Each component compiled via `meval::parse_str(..)?.bind3("x","y","z")`. `eval3` computes each bound component (ignore bind errors at eval time → None).
- [ ] **Step 4: Run:** `cargo test field` → all pass. Then `cargo test` full suite still clean.
- [ ] **Step 5:** Commit `feat: field parsing and evaluation via meval`.

### Task 3: Domain + presets

**Files:**
- Create: `src/domain.rs`, `src/presets.rs`
- Test: unit tests in both files

**Interfaces:**
- Produces: `pub struct Domain { pub min: glam::Vec3, pub max: glam::Vec3 }` with `Domain::new(min, max)`, `Domain::default()` = [-1,1]³, `fn domain_size(&self) -> glam::Vec3`, `fn grid_points(&self, dims: glam::UVec3) -> impl Iterator<Item = glam::Vec3>` covering min..max inclusive on both ends in each axis (2D callers pass `UVec3::new(n, n, 1)`); presets: `pub fn presets() -> &'static [Preset]` where `pub struct Preset { pub name: &'static str, pub formula: &'static str, pub dimension: usize }` containing: Pump `(x, y)`, Vortex `(-y, x)`, Dipole `((x-0.5)/((x-0.5)²+(y-0.1)²+0.05) - (x+0.5)/((x+0.5)²+(y-0.1)²+0.05), …)` (builder composes a valid dipole pair), Shear flow `(1 - y*y, 0.05*x)`, Source and sink `(x+0.5)/… , (x-0.5)/…`, Curl example `(-y, x)`, No-curl example `(x, y)`, 3D B-field `(-y, x, 0)`, 3D radial `(x, y, z)`, 3D helix `(-y, x, 0.5)`.

- [ ] **Step 1: failing tests**: `Domain::default` extents; `grid_points` at dims (2,1,1) yields exactly {-1.0, 1.0} on x and 0 on the others; grid_points(3) count == 3 per axis and both endpoints included. `presets()` is non-empty and every preset `FieldExpr::parse(formula)` succeeds (drives formulas being valid), names unique.
- [ ] **Step 2:** `cargo test domain presets` fails.
- [ ] **Step 3:** Implement domain + the preset table (iterate wording until the dipole parses in meval syntax, e.g. `(x-0.5)/((x-0.5)^2+(y-0.1)^2+0.05)`).
- [ ] **Step 4:** tests pass; commit `feat: domain grid and preset list`.

### Task 4: Colormaps

**Files:**
- Create: `src/colormap.rs`
- Test: unit tests in `src/colormap.rs`

**Interfaces:**
- Produces: `pub enum Colormap { Viridis, Turbo, Mono }` with `fn colormap(&self, magnitude: f32, min: f32, max: f32) -> [u8; 3]` (clamps m to [min,max]; max == min → mid-color; Mono = grayscale ramp black→white).

- [ ] **Step 1: failing tests**: `Viridis` at min..max with m = min gives the viridis(0) color `(68, 1, 84)`; m = max gives `(253, 231, 37)`; `Mono` at max gives `(255,255,255)`; out-of-range m clamps; `max == min` → `(128,128,128)`.
- [ ] **Step 2:** `cargo test colormap` fails.
- [ ] **Step 3:** Implement via `colorous::VIRIDIS.eval_cont(t)` / `colorous::TURBO.eval_cont(t)` (t in 0..1; map m into t; colorous `Color` → `[u8;3]`).
- [ ] **Step 4:** tests pass; commit `feat: colormaps over colorous`.

### Task 5: Sampling — field + domain → arrow instances

**Files:**
- Create: `src/sampling.rs`
- Test: unit tests in `src/sampling.rs`

**Interfaces:**
- Consumes: `FieldExpr`, `Domain`, `Colormap`.
- Produces: `pub struct ArrowData { pub position: glam::Vec3, pub dir: glam::Vec3 /*unit*/, pub mag: f32, pub color: [u8; 3] }`; `fn sample_field(field: &FieldExpr, domain: &Domain, grid: glam::UVec3, view: &ViewOptions) -> Vec<ArrowData>` where `ViewOptions { colormap, color_range: Option<(f32, f32)>, } `· separate `fn magnitude_range(arrows: &[ArrowData]) -> (f32, f32)`; dot product callers later use `arrow.mag` for scaling. Arrow length contribution = `mag` clamped to 1.0 when `normalize` is on (caller handles geometry scaling).

- [ ] **Step 1: failing tests** on the constant field `(1, 0, 0)`, grid (2,2,1), default domain:
  - count == 4; every arrow `dir ≈ Vec3::X`; `mag ≈ 1`.
  - `magnitude_range` on it is `(1.0, 1.0)`.
  - `sample_skips_nan`: field `(1/x, 0, 0)` with grid (5,1,1) → the x=0 arrow dropped, count == 4.
  - `all_zero_field`: field `(0, 0, 0)` → arrows are skipped (zero-length direction is degenerate) so the vector is empty; `magnitude_range` on empty is `(0.0, 0.0)`.
  - color: with `Colormap::Mono` and `color_range` Some((0.0, 1.0)), the field `(x, 0, 0)` on grid (2,1,1) gives the left arrow darker than the right.
- [ ] **Step 2:** `cargo test sampling` fails.
- [ ] **Step 3:** Implement: iterate `domain.grid_points(grid)`, `field.eval3`, skip None and near-zero vectors (length < 1e-6); normalize dir; `mag` = field length (normalized field: mag still the true length, per design's normalize semantics); compute color per colormap with either provided range or auto-fit range of sampled mags.
- [ ] **Step 4:** tests pass; `cargo test` clean; commit `feat: field sampling to arrow instances`.

### Task 6: Camera (orbit + ortho + unproject)

**Files:**
- Create: `src/camera.rs`
- Test: unit tests in `src/camera.rs`

**Interfaces:**
- Produces: `pub struct OrbitCamera { pub yaw: f32, pub pitch: f32, pub distance: f32, pub target: glam::Vec3, pub fov_y: f32 }` with `Default` (pitch 0.9, yaw 0.8, distance 3.2, target origin); ``fn orbit(&mut self, dx: f32, dy: f32)``, ``fn zoom(&mut self, factor: f32)`` (clamps distance 0.5..20); `fn view(&self) -> glam::Mat4`, `fn proj_perspective(&self, aspect: f32) -> glam::Mat4` (`perspective_rh`), `fn proj_ortho(&self, aspect: f32, plane_size: f32) -> glam::Mat4` centered on target and perpendicular to the view direction (for the 2D view); `fn vp_matrix(&self, aspect: f32, ortho_plane_size: Option<f32>) -> glam::Mat4`, `fn ndc_ray(&self, aspect: f32, ndc_xy: glam::Vec2) -> (glam::Vec3 /*origin*/, glam::Vec3 /*dir*/)` — direction unnormalized.

- [ ] **Step 1: failing tests**: the world origin in `vp_matrix` map-forward (`(vp * Vec4::W).xyz()` stay same as projection — simpler: test that a point equal to the target maps to `ndc = (0,0,0)` up to w-division; use `proj_perspective * view * Vec4` with w-divide); `ndc_ray` at ndc (0,0) is the target point-ish line back through the camera; `zoom` clamping; `proj_ortho` maps the target to center; continuity: `orbit` changes `view`.
- [ ] **Step 2:** `cargo test camera` fails.
- [ ] **Step 3:** Implement (look-at from a spherical position around target; `inverse` Mat4s as needed).
- [ ] **Step 4:** tests pass; commit `feat: orbit camera with ortho mode and ray unproject`.

### Task 7: wgpu renderer (instanced arrows, both views)

**Files:**
- Create: `src/renderer.rs`, `src/shaders.wgsl`
- No unit tests (GPU code) — verified by build + launch render inspection.

**Interfaces:**
- Consumes: `glam` matrices.
- Produces: `pub struct Renderer` holding the wgpu pipelines, inserted into `render_state.renderer.write().callback_resources` under a registry struct; `pub struct SceneGPU { pub arrows: Arc<Vec<ArrowInstanceGPU>>, pub lines: Arc<Vec<LineInstanceGPU>>, pub dots: Arc<Vec<ArrowInstanceGPU>> }` where the `dots` field reuses the arrow instance layout with negligible head geometry; instances are `#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)] ArrowInstanceGPU { pub origin: [f32; 4] /* pos.xyz, 1.0 */, pub dir_len: [f32; 4] /* unit dir.xyz, length */, pub color: [f32; 4] /* rgba 0..1 */ }` and `LineInstanceGPU { pub a: [f32; 4] /* start.xyz, 1.0 */, pub b: [f32; 4] /* end.xyz, line width */, pub color: [f32; 4] }`; uniform `Globals { view_proj: [[f32; 4]; 4], light_dir: [f32; 4] }`. Applications upload `SceneGPU` contents each frame from the app state (accumulator owned by `app.rs`). The paint callback runs per frame via `egui_wgpu::CallbackTrait::prepare` (upload buffers + Globals with the pass's screen size) and `paint` (draw order: grid, axes+bounding box, lit depth-tested arrows, streamline segments, dots).

**Geometry choice:** arrows as tight instanced *tapered shaft + cone head* built in the vertex shader from an instance matrix (shader maps local geometry Y along `dir`); depth 24-bit; BGRA8 sRGB surface assumed (`surface_texture_format()` from `render_state.target_format()`).

- [ ] **Step 1:** Write `shaders.wgsl`: shared globals bind group; two draw modes — `arrow` (shaft + cone, half-lambert lit, unpacks p0/dir/len/color from the instance) and `line` (segment billboarded or plain-colored, unlit), each with its own vertex body driven by an extra "flavor" uniform/binding per instance type or two pipelines.
- [ ] **Step 2:** `Renderer::new` compiles pipelines (one per mode), binds group for `Globals` uniform.
- [ ] **Step 3:** Replace Task 1's placeholder rect with `egui_wgpu::Callback::new_paint_callback(rect, DrawCallback { … })` added to the CentralPanel painter each frame, carrying the current `SceneGPU` (arrows reused via `Arc` when unchanged).
- [ ] **Step 4: Verify build and visual** — `cargo run`; the 2D view shows a grid of lit arrows for the default field `(x, y)` (hardcode the scene in `app.rs` for now: parse the default formula, sample 24×24, upload instances). Camera interaction is not wired yet; use the ortho camera's default. Kill the window after inspecting. If the viewport is blank, debug shader/bind-group mismatches before proceeding.
- [ ] **Step 5:** Commit `feat: instanced wgpu renderer for arrows, grid, axes`.

### Task 8: Wire formula bar, sidebar controls, and mode toggle

**Files:**
- Modify: `src/app.rs`, `src/ui/formula_bar.rs`, `src/ui/sidebar.rs`, `src/ui/status_bar.rs`

**Interfaces:**
- Consumes: `FieldExpr`, `presets()`, `sample_field`, `OrbitCamera`, `Renderer`.
- Produces: a runtime `SceneGPU { arrows, lines, dots }` snapshot rebuilt (once per frame, only when dirty) whenever the formula, domain, density, coloring, or mode changes. Streamline/particle paths join `lines`/`dots` in Tasks 9–11.

- [ ] **Step 1:** In `formula_bar::show`: `ui.add(egui::TextEdit::singleline(&mut app_state.formula))` (losses `desired_width(f32::INFINITY)`), preset `egui::ComboBox` whose selection overwrites the formula (and optionally switches mode), and the 2D/3D `SelectableLabel` pair. On edit, mark field dirty; if `FieldExpr::parse` succeeds → replace current field + clear old error; if it fails → keep old field and write the error into `StatusBarState.parse_message` (red).
- [ ] **Step 2:** In `sidebar.rs`: Display = density `DragValue` (2D: n×n grid value 4..=200; 3D: 4..=40), arrow length scale `Slider 0.1..=4`, `Checkbox "Normalize lengths"`. Coloring = `Colormap` `SelectableLabel` row, manual range toggle + two DragValues when off auto-fit. Features = checkboxes for streamlines and particles + speed slider (wired to stubs until Task 9/10), and the Domain section = six extrema DragValues with min < max enforcement (swap if violated). Every control sets the dirty flag; the snapshot carries it into the status-bar instance count.
- [ ] **Step 3:** Mode toggle switches the camera projection (ortho for 2D, perspective + orbit for 3D); 3D arrow grid extent derives from the domain's x/y/z extents so "density" keeps one meaning. In 3D: left-drag orbits (`camera.orbit`), scroll zooms (`camera.zoom`); wheel click in 2D pans the ortho plane is out of scope — pan is view-only via target drag on right-drag if cheap. Verify with the pump and vortex presets from several orbit angles before moving on.
- [ ] **Step 4:** `cargo run`, manually verify: typing garbage shows the red error and keeps the old field; presets load and redraw; 3D arrow `(x,-y,z)` tumble; density changes re-instance; colors shift with the selected colormap; status bar counts match expectations (e.g. 24×24 → 576). Commit `feat: live formula editing, sidebar controls, mode toggle`.

### Task 9: Streamlines (RK4, both views)

**Files:**
- Create: `src/streamlines.rs`
- Modify: `src/app.rs`, `src/renderer.rs` (streamline → `stream_segs` upload)
- Test: unit tests in `src/streamlines.rs`

**Interfaces:**
- Consumes: `FieldExpr`, `Domain`.
- Produces: `pub fn trace(field: &FieldExpr, start: glam::Vec3, domain: &Domain, opts: &StreamlineOpts) -> Vec<glam::Vec3>` with `StreamlineOpts { max_len: usize (cap 400 points), step_scale: f32, both_directions: bool }`; `fn preset_seeds(domain: &Domain, count: usize) -> Vec<glam::Vec3>` — a ring around the domain center in the field's plain; plus app-level click seeds.

- [ ] **Step 1: failing tests** on the vortex field `(-y, x, 0)` domain [-1,1]²:
  - `vortex_stays_bounded`: every point of `trace` from (1, 0) satisfies |pos| ≤ 1.3 over the whole polyline and there are ≥ 50 points (`both_directions` true).
  - radius is roughly conserved (max radius − min radius < 0.15).
  - `trace` halts at the domain edge — with domain [-1,1]², the streamline from (2,2) (outside) is empty.
  - `preset_seeds` returns `count` distinct points inside the domain.
- [ ] **Step 2:** `cargo test streamlines` fails.
- [ ] **Step 3:** Implement RK4 (h scaled by `step_scale` and inverse of local |f| so fast regions take shorter steps); abandon on NaN, zero-length, or leaving the domain; optional backward integration by negating the field. Both-direction default true.
- [ ] **Step 4:** Render: convert traced points into `LineInstanceGPU` (segments, colored by local magnitude through `Colormap`), feed into `SceneGPU.stream_segs`, paint unlit. Click-to-seed: 2D → pick the plane-crossing point via `ndc_ray`; 3D → also via `ndc_ray` with seed = first box-intersection (or the click-plane at target depth). Store click seeds in app state; "Clear seeds" button in Features.
- [ ] **Step 5:** `cargo run`: vortex streamlines should form closed circles; a click anywhere adds a seeded line; toggling Features-off removes them. Commit `feat: streamlines with RK4 + click-to-seed in 2D and 3D`.

### Task 10: Particle system (both views)

**Files:**
- Create: `src/particles.rs`
- Modify: `src/app.rs` (advect in `update()` with `ctx.input(|i| i.stable_dt)`), `src/renderer.rs` (`dots`)
- Test: unit tests in `src/particles.rs`

**Interfaces:**
- Consumes: `FieldExpr`, `Domain`.
- Produces: `pub struct Particles { pub positions: Vec<glam::Vec3>, pub rng: rand::rngs::SmallRng }` Crate — wait, no direct `rand` dependency yet; use `std::time`-seeded simple LCG implemented inline (no new dependency). `fn spawn(&mut self, domain: &Domain, n: usize)`; `fn advect(&mut self, field: &FieldExpr, domain: &Domain, dt: f32, speed: f32)` — RK4 one step, respawn a particle when it NaNs, hits zero velocity, or leaves the domain.

- [ ] **Step 1: failing tests**: `spawn` fills exactly n positions strictly inside the domain; `advect` on the pump field `(1, 0, 0)` moves every particle's x by ≥ dt·0.5; a particle pushed outside respawns inside after `advect` (craft one starting point outside via direct field manipulation, then verify); count is constant over 100 advect steps in a swirling field.
- [ ] **Step 2:** `cargo test particles` fails.
- [ ] **Step 3:** Implement with a deterministic LCG (`u64` state, `state.wrapping_mul(6364136223846793005) + 1442695040888963407`) — keep every op inside one function so the tests are deterministic when the seed is fixed.
- [ ] **Step 4:** Render dots as small lit octahedra (tiny scaled instance of the arrow pipeline's shaft-only vertex shape or a dedicated 12-triangle octahedron vertex buf) colored by local magnitude. Animation toggle in Features + speed slider drives `dt = speed * stable_dt`.
- [ ] **Step 5:** `cargo run`: pump field streams left→right; vortex swarms; toggling off freezes/removes dots; particle count never exceeds the cap. Commit `feat: advected particle animation`.

### Task 11: 3D streamlines + particles in the 3D view (completion of Features)

**Files:**
- Modify: `src/streamlines.rs` (seed from camera ray), `src/particles.rs` (3D spawn volume)
- Test: unit tests added to both files for the 3D paths

**Interfaces:**
- Consumes: `OrbitCamera::ndc_ray`, `Domain`.

- [ ] **Step 1: failing test** `ray_seed_inside_domain`: unproject the ndc center (0,0) with a camera at default orbit pointing at origin, domain [-1,1]³ → origin lands inside domain and `trace` from it yields a polyline in the 3D field `(-y, x, 0.5)` of length ≥ 50 like Task 9.
- [ ] **Step 2:** implement the ray-seed path (first box hit from `ndc_ray`, fallback: shell entry at distance where sphere r = |target-dist|·0.5); `Particles::spawn` gains a 3D-wide mode when `dimension == 3`.
- [ ] **Step 3:** `cargo test` passes; `cargo run`: in 3D mode on the helix preset, streamlines spiral and particles fill the volume. Commit `feat: 3D streamlines and volume particles`.

### Task 12: Style panel + persistence

**Files:**
- Create: `src/ui/style_panel.rs`
- Modify: `src/app.rs` (apply styles + persistence)
- Test: unit test in `src/ui/style_panel.rs` for the state→visuals mapping

**Interfaces:**
- Produces: `pub struct AppStyle { theme_dark: bool, accent: egui::Color32, rounding: f32, spacing: f32, font_size: f32 }` (defaults per design: dark, accent #009688-ish teal); `fn apply(ctx: &egui::Context, style: &AppStyle)`; persistence via `App::save` returning `&AppSettings { formula, mode, controls, style }` (serde derive; whole `VfApp` state stored with `cc.storage`).

- [ ] **Step 1: failing test** (pure logic): `AppStyle::apply` on a fresh context sets `visuals.dark_mode == expected`, accent used for selection, and the font family's size increases with `font_size` (use `egui::Visuals::clone` on a throwaway `Context` and assert the fields).
- [ ] **Step 2:** implement `apply` (dark/light `Visuals`, `accent_color`, `Visuals::window_rounding` + widget rounding via `ctx.style_mut`, `spacing.item_spacing`, text-style font size mutated through `ctx.all_styles_mut()`). Wire a collapsible "Style" section at the top of the sidebar with the controls; the accent uses `egui::color_picker`.
- [ ] **Step 3:** persistence: derive `serde` on the settings struct; in `App::new(cc)`, load with `cc.storage.and_then(|s| eframe::get_value(s, eframe::APP_KEY))`; `App::update` uses it. Note: egui persistence serializes via `ron` internally — needs `#[derive(Serialize, Deserialize)]` and no references. Compile-time caveat: `field` parsing is re-done at load (store the formula *string* only).
- [ ] **Step 4:** `cargo run`: change accent → whole UI re-colors; font-size slider resizes live; restart the app and verify theme + formula persisted. Commit `feat: theme/accent/rounding/spacing/font controls, persisted`.

### Task 13: Screenshot export

**Files:**
- Create: `src/screenshot.rs`
- Modify: `src/app.rs`, `src/renderer.rs` (expose an offscreen pass)
- Test: unit test in `src/screenshot.rs` (filename/scene-arg logic only; GPU part verified manually)

**Interfaces:**
- Consumes: `Renderer` pipelines, scene snapshot, `OrbitCamera`.
- Produces: app-level button (sidebar bottom): renders the scene to an offscreen RGBA8 texture at the viewport pixel size (cap 4096²), maps it, encodes PNG via `image`, spawns a thread showing the `rfd::FileDialog` save dialog and writes the file (must not block the UI thread); toast notification on completion.

- [ ] **Step 1: failing test** `default_filename`: `screenshot::default_filename(now, "2D")` matches `vf-2026-11-05-2d.png` given a fixed timestamp input.
- [ ] **Step 2:** implement offscreen re-render (own `CommandEncoder`, copy to buffer, map_async + poll; TSAN-ignore the wgpu callback ordering: use `queue.submit` + `device.poll(wgpu::PollType::Wait)`).
- [ ] **Step 3:** `cargo run`: press the button, choose a path in the dialog, verify the PNG exists and depicts the current scene (arrows, grid; streamline/particle overlays acceptable either way — prefer included if cheap).
- [ ] **Step 4:** Commit `feat: PNG screenshot export via native save dialog`.

### Task 14: Final review + README

**Files:**
- Create: `README.md` (one screen: what it is, format examples, build/run, key bindings)
- Modify: whatever the review surfaces

- [ ] **Step 1:** Full `cargo test` + `cargo clippy -- -D warnings` + `cargo build --release`.
- [ ] **Step 2:** Run the app and walk the design's checklist page by page: every formula example in README runs; presets all load; 2D/3D toggle; streamlines in both; particles in both; screenshot saved; theme + persistence; resize handles on the sidebar.
- [ ] **Step 3:** Fix what fails; commit `docs: README` last so the manual pass precedes it.
