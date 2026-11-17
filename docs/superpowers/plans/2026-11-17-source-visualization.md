# Source Visualization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Draw source-kind glyphs in the existing line pass, add source presets, and carry an honest softened-model/accuracy warning into PNG and SVG exports.

**Architecture:** A pure `source_glyph` module turns the committed `SourceScene` into kind/sign/orientation glyphs and line instances, using the active `PlaneFrame` for the selected 2D slice. The app composes those lines into the vector scene without touching #13 plane transitions. A separate provenance module renders the same warning lines into a baked PNG banner, PNG tEXt chunks, visible SVG text, and SVG metadata. Source presets are a separate catalogue; formula presets keep their current formula/migration behaviour.

**Tech Stack:** Rust 2024, eframe/egui, glam, existing `LineInstanceGPU`/`SceneGPU`, `png 0.18` (already in the lock graph), `image` decoding in tests.

**Spec:** `docs/superpowers/specs/2026-11-17-source-visualization-design.md`

## Global Constraints

- E and B are separate outputs. Never sum unlike quantities. Source glyphs use fixed E amber `[255,176,64]` and B cyan `[64,196,255]`.
- Never draw or export NaN/Inf as valid vectors. `allow_inaccurate` may reveal finite accuracy-not-met arrows but never removes a warning or bypasses caps.
- 3D source sampling cap is 40³; 2D source sampling cap is 200²; source count cap is 64. Do not change these.
- One global softening (`SourceScene::softening`, range `1e-6..=1.0`) and the existing shared clock. No per-source softening or private timer.
- #13 owns selected-plane transitions and particle/streamline seeding. Reuse `PlaneFrame`, `scene_domain`, and existing frame sync; do not add a competing plane or seeding path.
- Classic formula presets remain formulas. Do not convert saved formulas or camera presets into source scenes. Grade 3-6 in the project notes apply.
- No new geometry kinds, drag placement, arbitrary grouping, solver, render backend, or CLI flags.
- Preserve wasm compilation. Native CAS remains native-only. New dependencies must be GPL-3.0-compatible; only promote already-locked `png` to a direct dependency.
- Tests live in `tests/` and register in `tests/suite.rs`. No inline `#[cfg(test)]` modules under `src/`.
- Existing wording and seams for scalar-plot export remain unchanged.

## Review Focus

1. **A source sits off the selected 2D slice.** It must still be visible as a projected, clearly off-slice dashed/translucent glyph, not hidden or drawn as if on-slice. Pinned by `source_glyph::off_slice_glyphs_project_and_are_visually_distinct`.
2. **The user changes the selected plane while Sources is active.** Glyphs and sampled arrows must both update in the same rebuild, with no stale line instances from the previous slice. Pinned by `source_slice::plane_change_updates_glyphs_and_samples_together`.
3. **The scene is invalid or has no cached sampling when export starts.** The export must not silently omit the warning and must not draw stale arrows. Pinned by `source_export::missing_or_invalid_sampling_keeps_the_warning`.
4. **The user opts into accuracy-not-met data.** The result is visible but the PNG/SVG warning remains. Pinned by `source_export::opt_in_adds_visible_data_but_never_removes_the_warning`.
5. **A legacy 2D formula preset is selected.** It must stay a Formula-mode formula labelled “2D approximation”, not become a source scene. Pinned by `source_presets::legacy_formula_presets_keep_their_mode_and_label`.

---

### Task 1: Pure source glyph model and line geometry

**Files:**
- Create: `src/source_glyph.rs`
- Modify: `src/lib.rs`
- Test: `tests/source_glyph_test.rs`
- Modify: `tests/suite.rs`
- Modify: `docs/CODEBASE.md`

**Interfaces:**
- Consumes: `SourceScene`, `SourceEntry`, `Source`, `SourceKind`, `SourceKindTag`, `SourceDisplay`, `FieldFamily`, `PlaneFrame`, `Domain`, `LineInstanceGPU`.
- Produces:
  - `pub enum GlyphSign { Positive, Negative }`
  - `pub struct SourceGlyph { pub index: usize, pub kind: SourceKindTag, pub family: FieldFamily, pub sign: GlyphSign, pub center: Vec3, pub axis: Vec3, pub radius: f32, pub length: f32, pub turns: u32, pub domain_scale: f32, pub off_slice: bool }`
  - `pub fn source_glyphs(scene: &SourceScene, domain: &Domain, plane: Option<&PlaneFrame>) -> Vec<SourceGlyph>`
  - `pub fn source_glyph_lines(glyphs: &[SourceGlyph], domain: &Domain) -> Vec<LineInstanceGPU>`

- [ ] **Step 1: Write the failing glyph tests**

Create `tests/source_glyph_test.rs`:

```rust
use glam::{DVec3, UVec3, Vec3};
use vector_field_explorer::domain::Domain;
use vector_field_explorer::plane::{PlaneAxis, PlaneFrame, PlaneView};
use vector_field_explorer::source_glyph::{GlyphSign, source_glyph_lines, source_glyphs};
use vector_field_explorer::source_scene::{
    SourceDisplay, SourceEntry, SourceKindTag, SourceScene,
};
use vector_field_explorer::sources::{FieldFamily, Source, SourceKind};

fn all_kinds() -> SourceScene {
    let mut scene = SourceScene::default();
    for tag in SourceKindTag::ALL {
        scene.entries.push(SourceEntry {
            enabled: true,
            source: tag.default_source(),
        });
    }
    scene
}

fn point(charge: f64, center: DVec3) -> SourceScene {
    let mut scene = SourceScene::default();
    scene.entries.push(SourceEntry {
        enabled: true,
        source: Source {
            center,
            axis: DVec3::Z,
            kind: SourceKind::PointCharge { charge },
        },
    });
    scene
}

#[test]
fn glyphs_cover_every_kind_with_family_colour_and_sign() {
    let domain = Domain::default();
    let scene = all_kinds();
    let glyphs = source_glyphs(&scene, &domain, None);
    assert_eq!(glyphs.len(), 8);
    for tag in SourceKindTag::ALL {
        assert!(glyphs.iter().any(|glyph| glyph.kind == tag), "missing {tag:?}");
    }
    let electric = glyphs
        .iter()
        .find(|glyph| glyph.kind == SourceKindTag::PointCharge)
        .unwrap();
    assert_eq!(electric.family, FieldFamily::Electric);
    assert_eq!(electric.sign, GlyphSign::Positive);
    let magnetic = glyphs
        .iter()
        .find(|glyph| glyph.kind == SourceKindTag::CurrentLoop)
        .unwrap();
    assert_eq!(magnetic.family, FieldFamily::Magnetic);
    assert_eq!(magnetic.sign, GlyphSign::Positive);

    let lines = source_glyph_lines(&glyphs, &domain);
    assert!(lines.len() > glyphs.len() * 4, "each kind has visible geometry");
    let amber = [255.0 / 255.0, 176.0 / 255.0, 64.0 / 255.0];
    assert!(lines.iter().any(|line| {
        (line.color[0] - amber[0]).abs() < 1e-6
            && (line.color[1] - amber[1]).abs() < 1e-6
            && (line.color[2] - amber[2]).abs() < 1e-6
    }));
    assert!(lines.iter().all(|line| {
        line.a[..3].iter().chain(&line.b[..3]).all(|value| value.is_finite())
            && line.color.iter().all(|value| value.is_finite())
    }));
}

#[test]
fn negative_strengths_choose_the_minus_marker() {
    let domain = Domain::default();
    let scene = point(-1.0, DVec3::ZERO);
    let glyphs = source_glyphs(&scene, &domain, None);
    assert_eq!(glyphs.len(), 1);
    assert_eq!(glyphs[0].sign, GlyphSign::Negative);
}

#[test]
fn glyph_center_and_axis_follow_the_source() {
    let domain = Domain::new(Vec3::splat(-2.0), Vec3::splat(2.0));
    let mut scene = SourceScene::default();
    scene.entries.push(SourceEntry {
        enabled: true,
        source: Source {
            center: DVec3::new(1.0, 0.0, 0.0),
            axis: DVec3::Y,
            kind: SourceKind::FiniteLineCharge {
                charge: 1.0,
                length: 2.0,
            },
        },
    });
    let glyphs = source_glyphs(&scene, &domain, None);
    assert_eq!(glyphs[0].center, Vec3::new(1.0, 0.0, 0.0));
    assert!((glyphs[0].axis - Vec3::Y).length() < 1e-6);
    let lines = source_glyph_lines(&glyphs, &domain);
    let ys: Vec<f32> = lines.iter().flat_map(|line| [line.a[1], line.b[1]]).collect();
    assert!(ys.iter().any(|y| (*y - 1.0).abs() < 1e-5), "line end tick at +Y");
    assert!(ys.iter().any(|y| (*y + 1.0).abs() < 1e-5), "line end tick at -Y");
}

#[test]
fn family_selection_and_disabled_entries_filter_glyphs() {
    let domain = Domain::default();
    let mut scene = all_kinds();
    scene.entries[0].enabled = false;
    scene.set_display(SourceDisplay::Magnetic);
    let glyphs = source_glyphs(&scene, &domain, None);
    assert_eq!(glyphs.len(), 3);
    assert!(glyphs.iter().all(|glyph| glyph.family == FieldFamily::Magnetic));
    assert!(!glyphs.iter().any(|glyph| glyph.index == 0));
}

#[test]
fn off_slice_glyphs_project_and_are_visually_distinct() {
    let domain = Domain::new(Vec3::splat(-2.0), Vec3::splat(2.0));
    let frame = PlaneFrame::new(
        &domain,
        PlaneView {
            enabled: true,
            axis: PlaneAxis::Z,
            index: 2,
        },
        UVec3::new(5, 5, 5),
    );
    let mut scene = point(1.0, DVec3::new(0.0, 0.0, 2.0));
    scene.entries.push(SourceEntry {
        enabled: true,
        source: Source {
            center: DVec3::ZERO,
            axis: DVec3::Z,
            kind: SourceKind::PointCharge { charge: 1.0 },
        },
    });
    let glyphs = source_glyphs(&scene, &domain, Some(&frame));
    assert!(glyphs[0].off_slice);
    assert_eq!(glyphs[0].center.z, 0.0, "projected onto the selected plane");
    assert!(!glyphs[1].off_slice);
    let lines = source_glyph_lines(&glyphs, &domain);
    let off_alpha = lines.iter().map(|line| line.color[3]).fold(0.0, f32::max);
    assert!((off_alpha - 0.45).abs() < 1e-6);
    assert!(lines.iter().any(|line| (line.color[3] - 0.9).abs() < 1e-6));
}
```

- [ ] **Step 2: Run the glyph tests and confirm RED**

Run: `cargo test --test suite source_glyph_test::`
Expected: compile failure because `vector_field_explorer::source_glyph` does not exist.

- [ ] **Step 3: Implement `src/source_glyph.rs` and export it**

Shapes, all in a local `(right, up, normal)` basis and with the exact design sizes:
- point: three-axis cross;
- finite line: projected line plus end ticks;
- ring: 32-segment projected circle;
- disc: ring plus two diameters;
- sphere: three projected great circles;
- straight wire: line `2.2 * scene_extent` plus chevrons;
- current loop: ring plus two right-hand chevrons;
- solenoid: helix of at most 12 visible turns plus end rings and a chevron.

Use `domain_scale = 0.05 * max(domain.domain_size().max_element(), 1e-6)`. Line widths are `0.005 * scene_extent` on-slice/3D and `0.0035 * scene_extent` off-slice. For 2D, project center and axis with `PlaneFrame::to_local`/`project`, set glyph `center.z = 0`, and set `off_slice = |local.z| > 16.0 * f32::EPSILON * domain_extent.max(1.0)`. Sign comes from the signed charge/current inside `SourceKind`; use `GlyphSign::Negative` for a value `< 0.0`.

Add `pub mod source_glyph;` to `src/lib.rs` and a short module-map entry in `docs/CODEBASE.md`.

- [ ] **Step 4: Run the glyph tests and confirm GREEN**

Run: `cargo test --test suite source_glyph_test::`
Expected: 5 passed.

- [ ] **Step 5: Commit**

```bash
git add src/source_glyph.rs src/lib.rs tests/source_glyph_test.rs tests/suite.rs docs/CODEBASE.md
git commit -m "feat: add source glyph geometry" -m "Generated-by: OpenCode"
```

---

### Task 2: Selected-slice glyph integration and plane freshness

**Files:**
- Modify: `src/app/simulation.rs`
- Modify: `src/app/plot.rs` (test harness only)
- Test: `tests/source_slice_test.rs`
- Modify: `tests/suite.rs`
- Modify: `docs/CODEBASE.md`

**Interfaces:**
- Consumes: `source_glyphs`/`source_glyph_lines` from Task 1; `State::sources`, `State::scene_domain()`, `State::plane_frame()`, `sample_source_scene`.
- Produces: source glyph lines appended after reference/flow lines in `VfApp::rebuild_scene`; test seams `PlotAppHarness::source_glyph_line_count()` and `PlotAppHarness::viewport_line_count()`.

- [ ] **Step 1: Write the failing slice tests**

Create `tests/source_slice_test.rs`:

```rust
use glam::{DVec3, UVec3, Vec3};
use vector_field_explorer::domain::Domain;
use vector_field_explorer::plane::{PlaneAxis, PlaneFrame, PlaneView};
use vector_field_explorer::source_glyph::source_glyphs;
use vector_field_explorer::source_scene::{
    FieldMode, SourceEntry, SourceKindTag, SourceScene, sample_source_scene,
};
use vector_field_explorer::sampling::ViewOptions;
use vector_field_explorer::sources::Source;
use vector_field_explorer::viewmodel::State;

fn plane(domain: &Domain, index: u32) -> PlaneFrame {
    PlaneFrame::new(
        domain,
        PlaneView {
            enabled: true,
            axis: PlaneAxis::Z,
            index,
        },
        UVec3::new(5, 5, 5),
    )
}

fn one_point(z: f64) -> SourceScene {
    let mut scene = SourceScene::default();
    scene.entries.push(SourceEntry {
        enabled: true,
        source: Source {
            center: DVec3::new(0.0, 0.0, z),
            axis: DVec3::Z,
            kind: SourceKindTag::PointCharge.default_source().kind,
        },
    });
    scene
}

#[test]
fn slice_sampling_matches_the_3d_samples_projected_onto_the_plane() {
    let domain = Domain::new(Vec3::splat(-2.0), Vec3::splat(2.0));
    let frame = plane(&domain, 2);
    let scene = one_point(0.5);
    let view = ViewOptions::default();
    let world = sample_source_scene(&scene, &domain, UVec3::new(3, 3, 3), None, &view);
    let slice = sample_source_scene(
        &scene,
        &domain,
        UVec3::new(3, 3, 3),
        Some(&frame),
        &view,
    );
    assert_eq!(slice.arrows.len(), 9);
    for arrow in &slice.arrows {
        let world_point = frame.to_world(arrow.position);
        let expected = world
            .arrows
            .iter()
            .find(|candidate| (candidate.position - world_point).length() < 1e-5)
            .expect("same sample point exists in the 3D pass");
        let projected = frame.project(expected.dir);
        assert!((arrow.dir - projected).length() < 1e-4);
        assert!((arrow.mag - projected.length()).abs() < 1e-4);
    }
}

#[test]
fn plane_change_updates_glyphs_and_samples_together() {
    let mut app = vector_field_explorer::VfApp::new();
    app.state.set_field_mode(FieldMode::Sources);
    app.state.set_source_scene(one_point(2.0));
    app.state.set_mode(false);
    app.state.set_plane_index(0);
    app.test_harness().advance_simulation();
    let off_lines = app.test_harness().source_glyph_line_count();
    let off_arrows = app.state.source_sampling().unwrap().arrows.clone();
    assert!(off_lines > 0);

    app.state.set_plane_index(4);
    app.test_harness().advance_simulation();
    let on_lines = app.test_harness().source_glyph_line_count();
    let on_arrows = app.state.source_sampling().unwrap().arrows.clone();
    assert_eq!(off_lines, on_lines, "the same glyph shape changes slice state");
    assert!(!off_arrows.is_empty());
    assert_ne!(off_arrows[0].mag, on_arrows[0].mag, "samples are from the new plane");
    assert!(!source_glyphs(
        &app.state.sources,
        &app.state.scene_domain(),
        app.state.plane_frame().as_ref()
    )[0]
    .off_slice);
}

#[test]
fn formula_mode_keeps_the_existing_scene_and_emits_no_source_glyphs() {
    let mut app = vector_field_explorer::VfApp::new();
    app.state.set_source_scene(one_point(0.0));
    app.state.set_field_mode(FieldMode::Sources);
    app.test_harness().advance_simulation();
    assert!(app.test_harness().source_glyph_line_count() > 0);
    app.state.set_field_mode(FieldMode::Formula);
    app.test_harness().advance_simulation();
    assert_eq!(app.test_harness().source_glyph_line_count(), 0);
    assert!(!app.state.source_sampling().is_some());
}
```

- [ ] **Step 2: Run and confirm RED**

Run: `cargo test --test suite source_slice_test::`
Expected: compile failure on the two harness methods.

- [ ] **Step 3: Implement app composition and test seams**

In `src/app/simulation.rs::rebuild_scene`, after the existing `compose_lines_with_frame` call, generate and append glyph lines:

```rust
let glyphs = crate::source_glyph::source_glyphs(
    &self.state.sources,
    &self.state.scene_domain(),
    self.state.plane_frame().as_ref(),
);
let glyph_lines = crate::source_glyph::source_glyph_lines(&glyphs, &self.state.scene_domain());
lines.extend(glyph_lines.iter().copied());
self.source_glyph_line_count = glyph_lines.len();
```

Reset `source_glyph_line_count` to zero on the no-rebuild path only if the cached scene has no sources; otherwise keep it. Add the private field next to `streamline_count` and initialise it to zero. Add harness methods in `src/app/plot.rs`:

```rust
pub fn source_glyph_line_count(&self) -> usize { self.app.source_glyph_line_count }
pub fn viewport_line_count(&self) -> usize {
    match &self.app.scene {
        crate::plot_scene::ViewportScene::Vector(scene) => scene.lines.len(),
        crate::plot_scene::ViewportScene::Plot(_) => 0,
    }
}
```

Do not add glyph lines in the scalar plot branch. Do not modify `sync_scene_frame` or any #13 plane clearing. Update `docs/CODEBASE.md` with the source-glyph composition point.

- [ ] **Step 4: Run focused app and existing plane tests**

Run: `cargo test --test suite source_slice_test:: plane_app_test:: particle_playback_test::`
Expected: PASS, including the existing #13 transition tests.

- [ ] **Step 5: Commit**

```bash
git add src/app/simulation.rs src/app/plot.rs src/app/mod.rs tests/source_slice_test.rs tests/suite.rs docs/CODEBASE.md
git commit -m "feat: draw source glyphs on the selected slice" -m "Generated-by: OpenCode"
```

---

### Task 3: Source preset catalogue and legacy formula labels

**Files:**
- Create: `src/source_presets.rs`
- Modify: `src/lib.rs`
- Modify: `src/presets.rs`
- Modify: `src/ui/presets.rs`
- Test: `tests/source_presets_test.rs`
- Modify: `tests/suite.rs`
- Modify: `docs/CODEBASE.md`

**Interfaces:**
- Consumes: `SourceScene`, `SourceKindTag::default_source`, `State::set_field_mode`, `set_plot_kind`, `set_mode`, `set_domain`, `set_source_scene`.
- Produces:
  - `pub struct SourcePreset { pub name: &'static str, pub summary: &'static str, pub build: fn() -> SourceScene }`
  - `pub fn source_presets() -> &'static [SourcePreset]`
  - `pub fn source_preset(name: &str) -> Option<&'static SourcePreset>`
  - `pub fn source_strength(source: &Source) -> f64`
  - `pub fn load_source_preset(preset: &SourcePreset, state: &mut State) -> bool`
  - `PresetPanel::load_source_field(name: &str, state: &mut State) -> bool`
  - `pub fn preset_approximation_label(name: &str) -> Option<&'static str>` in `src/presets.rs`.

- [ ] **Step 1: Write the failing preset tests**

Create `tests/source_presets_test.rs`:

```rust
use std::collections::HashSet;
use vector_field_explorer::presets::{preset_approximation_label, presets};
use vector_field_explorer::source_presets::{
    load_source_preset, source_preset, source_presets,
};
use vector_field_explorer::source_scene::FieldMode;
use vector_field_explorer::viewmodel::State;

#[test]
fn source_presets_have_unique_names_and_cover_all_eight_kinds() {
    let presets = source_presets();
    let names: HashSet<&str> = presets.iter().map(|preset| preset.name).collect();
    assert_eq!(names.len(), presets.len());
    assert_eq!(presets.len(), 10);
    let mut kinds = HashSet::new();
    for preset in presets {
        let scene = (preset.build)();
        for entry in &scene.entries {
            kinds.insert(vector_field_explorer::source_scene::SourceKindTag::of(
                &entry.source.kind,
            ));
        }
    }
    assert_eq!(
        kinds.len(),
        8,
        "the catalogue covers every source kind"
    );
    let gallery = source_preset("Source gallery").expect("the all-eight preset");
    assert_eq!((gallery.build)().entries.len(), 8);
    assert!(source_preset("missing").is_none());
}

#[test]
fn loading_a_source_preset_uses_real_setters_and_a_visible_domain() {
    let mut state = State::new(true);
    let preset = source_preset("Coulomb point").unwrap();
    assert!(load_source_preset(preset, &mut state));
    assert_eq!(state.field_mode, FieldMode::Sources);
    assert!(state.mode_3d);
    assert_eq!(state.domain.domain_size(), glam::Vec3::new(4.0, 4.0, 4.0));
    assert_eq!(state.sources.entries.len(), 1);
    assert!(!state.sources.allow_inaccurate);
    let entry = &state.sources.entries[0];
    assert!((entry.source.center.as_vec3() - glam::Vec3::ZERO).length() < 1e-6);
    assert!(entry.source.axis.is_finite());
    assert_eq!(state.sources.softening, 0.1);
}

#[test]
fn every_preset_strength_is_the_illustrative_unit_and_finite() {
    for preset in source_presets() {
        let scene = (preset.build)();
        assert!(!scene.entries.is_empty());
        assert!(!scene.allow_inaccurate);
        assert!(scene.softening.is_finite());
        for entry in &scene.entries {
            let strength = vector_field_explorer::source_presets::source_strength(&entry.source);
            assert!(strength.is_finite());
            assert!((strength.abs() - 1.0).abs() < 1e-12, "{}", preset.name);
        }
    }
}

#[test]
fn legacy_formula_presets_keep_their_mode_and_label() {
    let mut state = State::new(false);
    let before = state.sources.clone();
    let mut panel = vector_field_explorer::ui::presets::PresetPanel::default();
    assert!(panel.load_field("Dipole", &mut state));
    assert_eq!(state.field_mode, FieldMode::Formula);
    assert_eq!(state.sources, before, "a formula preset never rewrites sources");
    assert_eq!(preset_approximation_label("Dipole"), Some("2D approximation"));
    assert_eq!(preset_approximation_label("Point charge"), Some("2D approximation"));
    assert_eq!(preset_approximation_label("3D Dipole"), None);
    assert_eq!(preset_approximation_label("Pump"), None);
    for preset in presets() {
        assert!(
            preset_approximation_label(preset.name).is_none()
                || preset.dimension == 2 && preset.formula.contains("x")
        );
    }
}
```

- [ ] **Step 2: Run and confirm RED**

Run: `cargo test --test suite source_presets_test::`
Expected: compile failure because `source_presets` does not exist.

- [ ] **Step 3: Implement the catalogue and UI**

`source_presets()` returns these ten names in this order, each built from `SourceKindTag::default_source()` with strength `1.0` (dipoles use `+1`/`-1`):
`Coulomb point`, `Electric dipole`, `Charged line`, `Uniform ring`, `Thin disc`, `Charged sphere`, `Infinite wire`, `Current loop`, `Finite solenoid`, `Source gallery`.

Use `build: fn() -> SourceScene` closures as const-compatible function pointers. `load_source_preset` sets `PlotKind::VectorField`, `FieldMode::Sources`, `mode_3d = true`, `domain = Domain::new(Vec3::splat(-2.0), Vec3::splat(2.0))`, then `set_source_scene`. Return false only if the preset is missing or validation fails.

In `src/presets.rs`, add `preset_approximation_label` returning `Some("2D approximation")` for exactly `Dipole`, `Source and sink`, `Point charge`, `Like charges`, `Quadrupole`, `Straight wire`; return `None` otherwise.

In `src/ui/presets.rs`, add a `Source presets` section; clicking a row calls `load_source_field`, sets `requested = Some(ViewCommand::Fit)`, and `active_source = Some(name)`. Append ` (2D approximation)` to formula preset labels when `preset_approximation_label` returns `Some`. Keep the existing field preset rows and camera rows unchanged.

- [ ] **Step 4: Run preset and UI tests**

Run: `cargo test --test suite source_presets_test:: ui_preset_test:: preset_panel_test::`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/source_presets.rs src/lib.rs src/presets.rs src/ui/presets.rs tests/source_presets_test.rs tests/suite.rs docs/CODEBASE.md
git commit -m "feat: add source scene presets" -m "Generated-by: OpenCode"
```

---

### Task 4: Export provenance, PNG banner/metadata, and SVG warning

**Files:**
- Create: `src/export_provenance.rs`
- Modify: `src/lib.rs`
- Modify: `src/export.rs`
- Modify: `src/screenshot.rs`
- Modify: `Cargo.toml`
- Test: `tests/source_export_test.rs`
- Modify: `tests/suite.rs`
- Modify: `docs/CODEBASE.md`

**Interfaces:**
- Consumes: `FieldMode`, `SourceScene`, `SourceSampling`, `SourceDisplay`, `SourceFamilyStatus`, `SceneGPU`, `LineInstanceGPU`.
- Produces:
  - `pub struct ExportProvenance { pub warning_lines: Vec<String>, pub metadata: Vec<(String, String)>, pub source_mode: bool }`
  - `pub fn source_export_provenance(mode: FieldMode, scene: &SourceScene, sampling: Option<&SourceSampling>) -> Option<ExportProvenance>`
  - `pub fn provenance_lines(provenance: &ExportProvenance) -> &[String]`
  - `pub fn provenance_metadata(provenance: &ExportProvenance) -> &[(String, String)]`
  - `pub fn screenshot::apply_export_banner(rgba: &mut [u8], width: u32, height: u32, provenance: &ExportProvenance) -> Result<(), String>`
  - `pub fn export::encode_png_with_text(frame: &[u8], width: u32, height: u32, metadata: &[(String, String)]) -> std::io::Result<Vec<u8>>`
  - `pub fn scene_to_svg_with_provenance(scene, view_proj, width, height, clear, provenance: Option<&ExportProvenance>) -> String`
  - Screenshot methods `request_with_provenance`, `request_svg_with_provenance`, `take_request_with_provenance`, `take_svg_request_with_provenance`.

- [ ] **Step 1: Write the failing provenance tests**

Create `tests/source_export_test.rs`:

```rust
use vector_field_explorer::export_provenance::{ExportProvenance, source_export_provenance};
use vector_field_explorer::source_scene::{
    FieldMode, SourceDisplay, SourceFamilyStatus, SourceSampling, SourceScene,
};

fn sampling(allow_inaccurate: bool) -> SourceSampling {
    SourceSampling {
        arrows: Vec::new(),
        electric: SourceFamilyStatus {
            sources: 2,
            drawn: 1,
            target_met: 1,
            accuracy_not_met_total: 1,
            accuracy_not_met_drawn: usize::from(allow_inaccurate),
            unavailable: 1,
            empty: false,
        },
        magnetic: SourceFamilyStatus {
            sources: 0,
            drawn: 0,
            target_met: 0,
            accuracy_not_met_total: 0,
            accuracy_not_met_drawn: 0,
            unavailable: 0,
            empty: true,
        },
        softening: 0.1,
        allow_inaccurate,
        validation_error: None,
    }
}

#[test]
fn formula_exports_have_no_source_provenance() {
    let scene = SourceScene::default();
    assert!(source_export_provenance(FieldMode::Formula, &scene, None).is_none());
}

#[test]
fn source_warning_lines_and_metadata_are_exact_and_separate() {
    let provenance =
        source_export_provenance(FieldMode::Sources, &SourceScene::default(), None).unwrap();
    assert_eq!(
        provenance.warning_lines.last().unwrap(),
        "Source sampling unavailable"
    );
    assert!(provenance.warning_lines[0].contains("softened teaching model"));
    assert_eq!(
        provenance.metadata.iter().find(|(key, _)| key == "vfe.source_model").unwrap().1,
        "softened teaching model"
    );

    let scene = SourceScene {
        display: SourceDisplay::Electric,
        ..SourceScene::default()
    };
    let provenance =
        source_export_provenance(FieldMode::Sources, &scene, Some(&sampling(true))).unwrap();
    assert!(provenance.warning_lines.iter().any(|line| line.contains("accuracy not met: 1 hidden, 1 drawn")));
    assert!(provenance.warning_lines.iter().any(|line| line == "Unavailable: 1"));
    assert!(provenance.metadata.iter().any(|(key, value)| key == "vfe.accuracy_not_met_total" && value == "1"));
    assert!(provenance.metadata.iter().any(|(key, value)| key == "vfe.accuracy_not_met_drawn" && value == "1"));
}

#[test]
fn opt_in_adds_visible_data_but_never_removes_the_warning() {
    let scene = SourceScene {
        display: SourceDisplay::Electric,
        ..SourceScene::default()
    };
    let without =
        source_export_provenance(FieldMode::Sources, &scene, Some(&sampling(false))).unwrap();
    let with =
        source_export_provenance(FieldMode::Sources, &scene, Some(&sampling(true))).unwrap();
    assert!(with.metadata.iter().any(|(key, value)| key == "vfe.accuracy_not_met_drawn" && value == "1"));
    assert_eq!(without.warning_lines.iter().filter(|line| line.contains("accuracy not met")).count(), 1);
    assert_eq!(with.warning_lines.iter().filter(|line| line.contains("accuracy not met")).count(), 1);
}

#[test]
fn missing_or_invalid_sampling_keeps_the_warning() {
    let mut scene = SourceScene::default();
    let missing = source_export_provenance(FieldMode::Sources, &scene, None).unwrap();
    assert!(missing.warning_lines.iter().any(|line| line == "Source sampling unavailable"));
    assert!(missing.warning_lines[0].contains("softened teaching model"));
    scene.softening = 0.1;
    let mut invalid = sampling(false);
    invalid.validation_error = Some("index 0: charge must be finite".to_string());
    let provenance = source_export_provenance(FieldMode::Sources, &scene, Some(&invalid)).unwrap();
    assert!(provenance.warning_lines.iter().any(|line| line.contains("invalid")));
    assert!(provenance.metadata.iter().any(|(key, _)| key == "vfe.validation_error"));
}

#[test]
fn png_banner_and_text_chunks_are_in_the_decoded_file() {
    let mut rgba = vec![255u8; 160 * 100 * 4];
    let provenance = ExportProvenance {
        warning_lines: vec![
            "Sources - softened teaching model - a = 0.1".to_string(),
            "Electric - accuracy not met: 1 hidden, 0 drawn".to_string(),
        ],
        metadata: vec![
            ("vfe.source_model".to_string(), "softened teaching model".to_string()),
            ("vfe.softening".to_string(), "0.1".to_string()),
        ],
        source_mode: true,
    };
    vector_field_explorer::screenshot::apply_export_banner(&mut rgba, 160, 100, &provenance)
        .unwrap();
    let png = vector_field_explorer::export::encode_png_with_text(
        &rgba,
        160,
        100,
        vector_field_explorer::export_provenance::provenance_metadata(&provenance),
    )
    .unwrap();
    let decoder = png::Decoder::new(std::io::Cursor::new(&png));
    let reader = decoder.read_info().unwrap();
    let info = reader.info();
    assert!(info.uncompressed_latin1_text.iter().any(|chunk| {
        chunk.keyword == "vfe.source_model" && chunk.text == "softened teaching model"
    }));
    let mut pixels = vec![0u8; 160 * 100 * 4];
    let mut decoder = png::Decoder::new(std::io::Cursor::new(&png));
    let mut reader = decoder.read_info().unwrap();
    reader.next_frame(&mut pixels).unwrap();
    let bottom = &pixels[90 * 160 * 4..100 * 160 * 4];
    assert!(bottom.chunks_exact(4).any(|pixel| pixel[..3] != [255, 255, 255]));
}
```

Also add to the same file:

```rust
#[test]
fn svg_warning_is_visible_and_metadata_is_present() {
    let mut scene = vector_field_explorer::renderer::SceneGPU::default();
    scene.mode_3d = false;
    let provenance = ExportProvenance {
        warning_lines: vec!["Sources - softened teaching model - a = 0.1".to_string()],
        metadata: vec![("vfe.softening".to_string(), "0.1".to_string())],
        source_mode: true,
    };
    let svg = vector_field_explorer::screenshot::scene_to_svg_with_provenance(
        &scene,
        glam::Mat4::IDENTITY,
        320,
        240,
        [0.0, 0.0, 0.0, 1.0],
        Some(&provenance),
    );
    assert!(svg.contains("<text"));
    assert!(svg.contains("softened teaching model"));
    assert!(svg.contains("<metadata"));
    assert!(svg.contains("vfe.softening"));
}
```

Use the existing `SceneGPU` constructor/seam; if none exists, construct the same literal fields used by `scene_to_svg` tests.

- [ ] **Step 2: Run and confirm RED**

Run: `cargo test --test suite source_export_test::`
Expected: compile failure because `export_provenance` and the new screenshot/export functions do not exist.

- [ ] **Step 3: Implement provenance, banner, and SVG**

Add `png = "0.18"` to `Cargo.toml`.

Implement the exact ASCII copy from the design. `source_export_provenance` returns `None` for Formula. For Sources, always include the model line. When `sampling` is `None`, return exactly the no-sampling line plus `vfe.source_model` metadata. When `validation_error` is present, include the invalid line. Add accuracy and unavailable lines only when their counts are nonzero. `allow_inaccurate` affects only the drawn count, never warning presence.

`screenshot::apply_export_banner` draws a semi-transparent dark strip at the bottom using a 5×7 built-in bitmap font with a 2 px scale and 4 px padding; lines are painted bottom-up above the padding. `export::encode_png_with_text` uses the `png` encoder directly, adds every metadata pair through `add_text_chunk`, and writes RGBA8. Sanitize metadata to printable ASCII and replace newlines with spaces.

Extend `src/screenshot.rs`:
- add pending provenance to both pending slots;
- keep `request()`/`request_svg()` as no-provenance wrappers;
- add `request_with_provenance`/`request_svg_with_provenance`;
- add `take_request_with_provenance`/`take_svg_request_with_provenance` returning `Option<Option<ExportProvenance>>` (outer `None` = no request);
- keep `take_request`/`take_svg_request` bool wrappers for existing callers;
- `scene_to_svg` delegates to `scene_to_svg_with_provenance(..., None)`;
- `scene_to_svg_with_provenance` inserts an escaped `<text>` block above the bottom-left corner and a `<metadata>` element with `vfe:` keys. Convert world line widths to pixels using the projected endpoint distance divided by the world endpoint distance, clamped `0.5..=3.0`; fall back to the old `max(0.5)` when the world distance is zero.

- [ ] **Step 4: Run export unit tests**

Run: `cargo test --test suite source_export_test:: screenshot_test:: svg_export_test::`
Expected: PASS; existing no-provenance SVG output semantics are unchanged.

- [ ] **Step 5: Commit**

```bash
git add src/export_provenance.rs src/lib.rs src/export.rs src/screenshot.rs Cargo.toml Cargo.lock tests/source_export_test.rs tests/suite.rs docs/CODEBASE.md
git commit -m "feat: carry source warnings into exports" -m "Generated-by: OpenCode"
```

---

### Task 5: App export wiring, docs/help, benchmark and full gates

**Files:**
- Modify: `src/app/viewport.rs`
- Modify: `src/app/input.rs`
- Modify: `src/app/plot.rs`
- Modify: `src/screenshot.rs`
- Modify: `src/ui/help_overlay.rs`
- Modify: `README.md`, `docs/sources-panel.md`, `GLOSSARY.md`, `docs/CODEBASE.md`, `docs/superpowers/README.md`
- Create: `docs/perf/source-preset-benchmark.md`, `scripts/bench-source-presets.sh` (executable)
- Test: `tests/source_export_app_test.rs`, `tests/source_preset_bench_test.rs`
- Modify: `tests/suite.rs`

**Interfaces:**
- Consumes: all prior task APIs; `SourceSampling` cache; `Screenshot` provenance pending variants.
- Produces: `VfApp::source_export_provenance()` (private), `PlotAppHarness::export_png_provenance()` and `export_svg_provenance()`; capture-time provenance in both export paths; benchmark evidence.

- [ ] **Step 1: Write the failing app wiring tests**

Create `tests/source_export_app_test.rs`:

```rust
use vector_field_explorer::source_scene::{FieldMode, SourceKindTag, SourceScene};
use vector_field_explorer::VfApp;

#[test]
fn png_request_captures_source_provenance_at_execution_time() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Sources);
    let mut scene = SourceScene::default();
    scene
        .add(SourceKindTag::PointCharge.default_source())
        .unwrap();
    app.state.set_source_scene(scene);
    app.test_harness().advance_simulation();
    app.test_harness().request_png_export();
    let provenance = app.test_harness().export_png_provenance().unwrap();
    assert!(provenance.warning_lines.iter().any(|line| line.contains("softened teaching model")));
    assert!(app.test_harness().export_svg_provenance().is_none());
}

#[test]
fn no_sampling_warns_instead_of_exporting_stale_vectors() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Sources);
    app.test_harness().request_png_export();
    let provenance = app.test_harness().export_png_provenance().unwrap();
    assert!(provenance.warning_lines.iter().any(|line| line == "Source sampling unavailable"));
    assert!(app.test_harness().source_glyph_line_count() == 0);
}

#[test]
fn source_svg_request_captures_the_same_provenance() {
    let mut app = VfApp::new();
    app.state.set_mode(false);
    app.state.set_field_mode(FieldMode::Sources);
    app.test_harness().request_svg_export();
    let provenance = app.test_harness().export_svg_provenance().unwrap();
    assert!(provenance.source_mode);
    assert!(app.test_harness().export_png_provenance().is_none());
}
```

Add the necessary test harness methods:
`request_png_export()` calls the same request path as the menu; `request_svg_export()` likewise; `export_png_provenance()` and `export_svg_provenance()` return clones of the pending provenance without taking the request.

- [ ] **Step 2: Run and confirm RED**

Run: `cargo test --test suite source_export_app_test::`
Expected: compile failure on the harness methods.

- [ ] **Step 3: Wire request-time provenance into both exports**

Add `VfApp::source_export_provenance()` returning `crate::export_provenance::source_export_provenance(self.state.field_mode, &self.state.sources, self.state.source_sampling())`. In the menu `ExportScreenshot` and `ExportScreenshotSvg` actions, call `request_with_provenance`/`request_svg_with_provenance` with that value before setting the request. The scalar-plot branch always requests with `None`.

In `src/app/viewport.rs`, switch to the `take_*_with_provenance` variants. Capture the returned provenance once before the scene build, then pass it to:
- `scene_to_svg_with_provenance` for SVG;
- `screenshot::spawn_save` / `finish_download` for PNG.

Update `spawn_save` and `finish_download` signatures to accept `Option<ExportProvenance>`, and encode through `export::encode_png_with_text` with `provenance_metadata`. Update scalar `dispatch_plot_png` call sites with `None`.

When the capture path receives `None` but `state.field_mode == Sources` (for a defensive request made without provenance), synthesize the fallback warning rather than exporting silently.

- [ ] **Step 4: Add docs, help, and benchmark evidence**

- `docs/sources-panel.md`: glyph legend/shapes, 2D projected/off-slice appearance, slice freshness, preset catalogue, preserved formula presets and 2D approximation labels, export warning/metadata, no CLI/source animation claim.
- `README.md` and `GLOSSARY.md`: short source-visualization entries.
- `help_overlay.rs`: a Sources row explaining glyphs, slice projection, “accuracy not met”, and export warning.
- `docs/superpowers/README.md`: add the approved design and plan rows.
- Create `tests/source_preset_bench_test.rs` as `#[ignore]`: build `Source gallery`, call `sample_source_scene` at 40³ and 200², assert finite arrows and nonempty diagnostics, print elapsed milliseconds and counts.
- Create `scripts/bench-source-presets.sh` that runs `cargo test --release --test suite source_preset_bench_test:: -- --ignored --nocapture` and sets only the standard issue-26 environment. Run the script, record the machine/platform and measured values in `docs/perf/source-preset-benchmark.md`, and state honestly whether the 33 ms densest-frame context is met. Do not claim browser/manual visual evidence.

- [ ] **Step 5: Run the full gates**

Environment:

```bash
export PATH=/home/paul/.cargo/bin:/usr/bin:/bin
export RUSTC=/usr/bin/rustc
export CARGO_TARGET_DIR=/home/paul/.cache/vector-field-explorer-targets/issue-26
export TMPDIR=/home/paul/.cache/vector-field-explorer-targets/issue-26/tmp
export KEEP_TEST_ARTIFACTS=1
```

Run in order:

```bash
cargo test --test suite source_export_app_test:: source_export_test:: source_glyph_test:: source_slice_test:: source_presets_test::
cargo test --test suite
cargo build
cargo clippy --all-targets -- -D warnings
rustfmt --edition 2024 --check --config skip_children=true src/source_glyph.rs src/source_presets.rs src/export_provenance.rs src/export.rs src/screenshot.rs src/app/viewport.rs src/app/input.rs src/app/plot.rs src/ui/presets.rs src/presets.rs tests/source_glyph_test.rs tests/source_slice_test.rs tests/source_presets_test.rs tests/source_export_test.rs tests/source_export_app_test.rs tests/source_preset_bench_test.rs
python3 scripts/check-docs.py
git diff --check
SMOKE_BIN=$CARGO_TARGET_DIR/debug/vector-field-explorer scripts/smoke-run.sh 15
```

Expected: every command exits 0. Only the known `nom 1.2.4` future-incompat warning may remain. Record the benchmark result separately; it has no pass/fail threshold beyond finite, diagnosed output.

- [ ] **Step 6: Commit**

```bash
git add src/app/viewport.rs src/app/input.rs src/app/plot.rs src/screenshot.rs src/ui/help_overlay.rs README.md GLOSSARY.md docs/CODEBASE.md docs/sources-panel.md docs/superpowers/README.md docs/perf/source-preset-benchmark.md scripts/bench-source-presets.sh tests/source_export_app_test.rs tests/source_preset_bench_test.rs tests/suite.rs
git commit -m "feat: finish source visualization exports and documentation" -m "Generated-by: OpenCode"
```

---

## Self-Review

**Spec coverage:** Glyph kinds/sign/orientation and selected-plane projection are Task 1; app slice integration, plane freshness, and non-interference with #13 are Task 2; preset catalogue and legacy formula labels/migration are Task 3; shared softened-model/accuracy provenance, PNG banner/metadata, SVG visible text/metadata, invalid/missing sampling, and opt-in non-suppression are Task 4; app request-time wiring, help/docs, benchmark and full gates are Task 5. Nonfinite exclusion is enforced by the existing source sampler and pinned in Task 4's invalid/missing warning and Task 1's finite-line assertions. E/B separation is untouched and pinned by Task 1 family filtering.

**Step scan:** Every test step names exact test functions and assertions; implementation steps name signatures and fixed values, leaving only shape math and atomic drawing details to the implementer. No TBDs or placeholder assertions remain.

**Type consistency:** `SourceGlyph`, `source_glyphs`, `source_glyph_lines`, `SourcePreset`, `load_source_preset`, `preset_approximation_label`, `ExportProvenance`, `source_export_provenance`, `provenance_lines`, `provenance_metadata`, `apply_export_banner`, `encode_png_with_text`, and `scene_to_svg_with_provenance` are introduced once and consumed by later tasks under the same names. Screenshot wrapper names preserve existing bool APIs.

**Review Focus:** Each of the five named inputs has an owning test: off-slice (Task 1), plane change (Task 2), missing/invalid sampling (Task 4), opt-in warning (Task 4), legacy formula preset (Task 3).

**Proportion:** Five tasks, one per reviewable seam; the plan fixes public APIs, exact warning strings and tests, not implementation bodies.
