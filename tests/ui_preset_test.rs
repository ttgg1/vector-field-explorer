// Tests for the preset management panel's action layer (Task 2).
//
// egui's immediate-mode UI has no offscreen test backend wired up here, so the
// panel's user-facing actions are factored into plain methods and the buttons
// in `PresetPanel::show` route through them. These tests drive the actions the
// way a button press would.

use eframe::egui;
use glam::Vec3;

use crate::support;

use vector_field_explorer::VfApp;
use vector_field_explorer::camera::OrbitCamera;
use vector_field_explorer::export::{delete_preset, load_presets};
use vector_field_explorer::fly_camera::FlyCamera;
use vector_field_explorer::parameters::{Parameter, ParameterSet};
use vector_field_explorer::persistence::ViewRecord;
use vector_field_explorer::preset_library::{PresetFilters, PresetGroup, PresetLibrary};
use vector_field_explorer::presets::presets;
use vector_field_explorer::render::Camera;
use vector_field_explorer::scene_document::{SceneCamera, SceneDocument};
use vector_field_explorer::scene_store;
use vector_field_explorer::source_scene::{FieldMode, SourceKindTag};
use vector_field_explorer::ui::presets::PresetPanel;
use vector_field_explorer::viewmodel::State;

fn panel_in(dir: &std::path::Path) -> PresetPanel {
    PresetPanel::new_with_dir(dir.to_path_buf()).expect("panel with tempdir")
}

fn cam_home() -> Camera {
    Camera {
        position: [1.0, 2.0],
        scale: 3.0,
    }
}

#[test]
fn test_create_records_current_camera_state() {
    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());

    panel.save_current("home", &cam_home()).expect("save");

    let saved = panel
        .presets()
        .iter()
        .find(|p| p.name == "home")
        .expect("created preset listed immediately");
    assert_eq!(saved.position, [1.0, 2.0]);
    assert_eq!(saved.scale, 3.0);
}

#[test]
fn test_load_restores_exact_camera_state() {
    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());
    panel.save_current("home", &cam_home()).expect("save");

    // Drag the camera somewhere else, then load the preset back.
    let mut camera = Camera {
        position: [7.0, -7.0],
        scale: 0.5,
    };
    assert!(panel.load("home", &mut camera), "preset found");
    assert_eq!(camera.position, [1.0, 2.0]);
    assert_eq!(camera.scale, 3.0);

    assert!(
        !panel.load("missing", &mut camera),
        "unknown preset leaves camera unchanged"
    );
    assert_eq!(camera.position, [1.0, 2.0]);
}

#[test]
fn test_delete_removes_from_list_and_disk() {
    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());
    panel.save_current("home", &cam_home()).expect("save");

    panel.delete("home").expect("delete");

    assert!(
        !panel.presets().iter().any(|p| p.name == "home"),
        "removed from the panel list"
    );
    let on_disk = load_presets(dir.path()).expect("reload file");
    assert!(
        !on_disk.iter().any(|p| p.name == "home"),
        "removed from disk"
    );
}

#[test]
fn test_presets_survive_restart() {
    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());
    panel.save_current("home", &cam_home()).expect("save");
    drop(panel);

    // Simulate an application restart: a fresh panel pointing at the same directory.
    let mut fresh = panel_in(dir.path());
    let mut camera = Camera::default();
    assert!(
        fresh.load("home", &mut camera),
        "preset still present after restart"
    );
    assert_eq!(camera.position, [1.0, 2.0]);
    assert_eq!(camera.scale, 3.0);
}

#[test]
fn test_delete_preset_function_removes_file_entry() {
    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());
    panel.save_current("home", &cam_home()).expect("save");
    panel
        .save_current(
            "work",
            &Camera {
                position: [0.0, 0.0],
                scale: 2.0,
            },
        )
        .expect("save");

    delete_preset(dir.path(), "work").expect("delete");
    let on_disk = load_presets(dir.path()).expect("reload file");
    assert_eq!(on_disk.len(), 1);
    assert_eq!(on_disk[0].name, "home");
}

// Moved out of `src/ui/presets.rs`'s inline module (ticket-14 review I-4):
// active-row highlight, delete confirmation and the applied-camera handoff.

#[test]
fn saving_highlights_the_row_and_a_duplicate_replaces_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());
    panel.save_current("home", &cam_home()).expect("save");
    assert_eq!(
        panel.active_name(),
        Some("home"),
        "a saved view is the active row"
    );
    assert_eq!(panel.presets().len(), 1);

    let moved = Camera {
        position: [-4.0, 0.5],
        scale: 0.25,
    };
    panel.save_current("home", &moved).expect("replace");
    assert_eq!(
        panel.presets().len(),
        1,
        "the duplicate name still replaces"
    );
    let saved = &panel.presets()[0];
    assert_eq!(saved.position, moved.position);
    assert_eq!(saved.scale, moved.scale);
    assert_eq!(panel.active_name(), Some("home"));
}

#[test]
fn loading_highlights_the_applied_row_and_a_missing_name_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());
    panel.save_current("home", &cam_home()).expect("save");
    panel
        .save_current(
            "work",
            &Camera {
                position: [0.0, 0.0],
                scale: 2.0,
            },
        )
        .expect("save");
    let mut camera = Camera::default();
    assert!(panel.load("home", &mut camera));
    assert_eq!(panel.active_name(), Some("home"));
    assert!(panel.load("work", &mut camera));
    assert_eq!(panel.active_name(), Some("work"));
    assert!(
        !panel.load("missing", &mut camera),
        "unknown preset stays untouched"
    );
    assert_eq!(
        panel.active_name(),
        Some("work"),
        "a failed load keeps the highlight"
    );
}

#[test]
fn deleting_waits_for_confirmation_then_clears_the_highlight() {
    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());
    panel.save_current("home", &cam_home()).expect("save");
    panel.request_delete("home");
    assert_eq!(panel.pending_delete(), Some("home"));
    assert_eq!(
        panel.presets().len(),
        1,
        "requesting a delete removes nothing yet"
    );

    panel.cancel_delete();
    assert_eq!(panel.pending_delete(), None);
    assert_eq!(panel.presets().len(), 1, "cancel keeps the preset");

    panel.request_delete("home");
    panel.delete("home").expect("confirmed delete");
    assert!(
        panel.presets().is_empty(),
        "the confirmed delete removes the preset"
    );
    assert_eq!(panel.pending_delete(), None, "the confirmation clears");
    assert_eq!(
        panel.active_name(),
        None,
        "deleting the active row clears the highlight"
    );
}

#[test]
fn a_load_hands_the_camera_to_the_live_view_exactly_once() {
    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());
    panel.save_current("home", &cam_home()).expect("save");
    let mut camera = Camera::default();
    assert!(
        panel.take_applied().is_none(),
        "a save does not move the live view"
    );
    assert!(panel.load("home", &mut camera));
    assert_eq!(
        panel.take_applied().map(|c| (c.position, c.scale)),
        Some(([1.0, 2.0], 3.0)),
        "the loaded camera reaches the live view",
    );
    assert!(panel.take_applied().is_none(), "and only once per click");
}

#[test]
fn resetting_the_camera_can_clear_the_highlight() {
    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());
    panel.save_current("home", &cam_home()).expect("save");
    panel.clear_active();
    assert_eq!(panel.active_name(), None);
    assert_eq!(
        panel.presets().len(),
        1,
        "clearing the highlight keeps the preset"
    );
}

// Ticket 31: the Presets panel also lists the built-in field presets grouped
// by category. Clicking one loads its formula and dimension into the scene.

#[test]
fn loading_a_field_preset_sets_its_formula_and_dimension() {
    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());
    let mut state = State::new(false);

    let magnetic = presets()
        .iter()
        .find(|preset| preset.name == "Magnetic dipole")
        .expect("Magnetic dipole is listed");
    assert!(
        panel.load_field("Magnetic dipole", &mut state),
        "known preset loads"
    );
    assert!(state.mode_3d, "a 3D preset switches the view to 3D");
    assert_eq!(state.field.dimension, 3);
    assert_eq!(state.formula, magnetic.formula);

    let point = presets()
        .iter()
        .find(|preset| preset.name == "Point charge")
        .expect("Point charge is listed");
    assert!(
        panel.load_field("Point charge", &mut state),
        "known preset loads"
    );
    assert!(!state.mode_3d, "a 2D preset switches the view back to 2D");
    assert_eq!(state.field.dimension, 2);
    assert_eq!(state.formula, point.formula);

    assert!(
        !panel.load_field("No such preset", &mut state),
        "unknown name reports failure"
    );
}

#[test]
fn two_and_three_dimensional_dipoles_are_independently_selectable() {
    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());
    let mut state = State::new(false);
    for (name, dimension) in [("3D Dipole", 3), ("Dipole", 2)] {
        assert!(
            panel.load_field(name, &mut state),
            "{name} loads by its own name"
        );
        assert_eq!(state.field.dimension, dimension);
        assert_eq!(state.mode_3d, dimension == 3);
        assert_eq!(panel.active_field_name(), Some(name));
        let value = state.field.eval3(glam::Vec3::new(1.0, 0.4, 0.7)).unwrap();
        assert_eq!(
            value.z != 0.0,
            dimension == 3,
            "3D dipole retains depth flow"
        );
    }
}

#[test]
fn loading_a_scalar_preset_switches_to_its_kind_without_touching_the_vector_formula() {
    let library = PresetLibrary::builtin().expect("built-in catalogue");
    let scalars: Vec<_> = library
        .entries()
        .iter()
        .filter(|entry| entry.scalar.is_some())
        .collect();
    assert_eq!(scalars.len(), 5, "five scalar presets are indexed");

    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());
    let mut state = State::new(false);
    state.set_formula("(-y, x)".to_string());
    let vector_formula = state.formula.clone();

    for entry in scalars {
        let spec = entry.scalar.expect("scalar spec");
        assert!(
            panel.load_field(entry.name, &mut state),
            "scalar preset `{}` loads",
            entry.name
        );
        assert_eq!(
            state.plot.kind, spec.kind,
            "`{}` switches to its shipped scalar kind",
            entry.name
        );
        assert_eq!(
            state.plot.kind,
            vector_field_explorer::plot::PlotKind::Surface3D
        );
        assert!(
            state.plot.surface.error.is_none(),
            "`{}` accepts its surface expression",
            entry.name
        );
        assert_eq!(
            state.plot.surface.source, spec.expr,
            "`{}` installs the surface expression",
            entry.name
        );
        assert_eq!(
            state.show_scalar_heatmap(),
            spec.show_heatmap,
            "`{}` heatmap visibility",
            entry.name
        );
        assert_eq!(
            state.scalar_contour_levels(),
            spec.contour_levels,
            "`{}` contour levels",
            entry.name
        );
        assert_eq!(
            state.formula, vector_formula,
            "`{}` must not overwrite the vector formula",
            entry.name
        );
        assert_eq!(state.field_mode, FieldMode::Formula);
        assert_eq!(panel.active_field_name(), Some(entry.name));
    }

    // Switching back to a vector row restores the vector kind and formula.
    assert!(panel.load_field("Vortex", &mut state));
    assert_eq!(
        state.plot.kind,
        vector_field_explorer::plot::PlotKind::VectorField
    );
    assert_eq!(state.formula, "(-y, x)");
}

#[test]
fn every_built_in_field_preset_loads_through_the_panel() {
    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());
    for preset in presets() {
        let mut state = State::new(false);
        assert!(
            panel.load_field(preset.name, &mut state),
            "preset `{}` must load",
            preset.name
        );
        assert_eq!(
            state.formula, preset.formula,
            "preset `{}` formula",
            preset.name
        );
        assert_eq!(
            state.field.dimension, preset.dimension,
            "preset `{}` dimension",
            preset.name
        );
        assert_eq!(
            state.mode_3d,
            preset.dimension == 3,
            "preset `{}` mode follows its dimension",
            preset.name
        );
    }
}

#[test]
fn loading_a_duplicate_formula_preset_highlights_only_the_clicked_name() {
    // Pump and No-curl example (and Vortex/Curl example) share formulas but
    // stay separate presets; the clicked row must be the only highlight.
    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());
    let mut state = State::new(false);

    assert!(panel.load_field("Pump", &mut state));
    assert_eq!(panel.active_field_name(), Some("Pump"));

    assert!(panel.load_field("No-curl example", &mut state));
    assert_eq!(
        panel.active_field_name(),
        Some("No-curl example"),
        "the clicked duplicate owns the highlight, not the first formula match"
    );

    assert!(panel.load_field("Vortex", &mut state));
    assert_eq!(panel.active_field_name(), Some("Vortex"));
    assert!(panel.load_field("Curl example", &mut state));
    assert_eq!(panel.active_field_name(), Some("Curl example"));

    let names: Vec<&str> = presets().iter().map(|preset| preset.name).collect();
    assert!(
        names.contains(&"Pump") && names.contains(&"No-curl example"),
        "duplicate formulas are kept, not deduplicated"
    );
}

#[test]
fn save_failure_on_corrupt_preset_file_sets_storage_note_and_keeps_name() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("presets.json"), "not json").unwrap();
    let mut panel = panel_in(dir.path());
    panel.set_new_name("home");

    let result = panel.save_current("home", &cam_home());

    assert!(result.is_err(), "a corrupt preset file must fail the save");
    assert!(
        panel.storage_note().is_some(),
        "a failed save must surface a non-blocking panel note"
    );
    assert_eq!(
        panel.new_name(),
        "home",
        "the typed name must survive a failed save"
    );
    assert!(
        panel.presets().is_empty(),
        "a failed save must not mutate the in-memory list"
    );
    // No-overwrite: the corrupt file is left untouched rather than clobbered.
    assert_eq!(
        std::fs::read_to_string(dir.path().join("presets.json")).unwrap(),
        "not json"
    );
}

#[test]
fn delete_failure_on_corrupt_preset_file_sets_storage_note() {
    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());
    panel.save_current("home", &cam_home()).expect("save");

    // Corrupt the file after a successful save so the delete's reload fails.
    std::fs::write(dir.path().join("presets.json"), "not json").unwrap();
    panel.request_delete("home");

    let result = panel.delete("home");

    assert!(
        result.is_err(),
        "a corrupt preset file must fail the delete"
    );
    assert!(
        panel.storage_note().is_some(),
        "a failed delete must surface a non-blocking panel note"
    );
    assert!(
        panel.presets().iter().any(|p| p.name == "home"),
        "a failed delete must keep the preset in the list"
    );
    assert_eq!(
        panel.pending_delete(),
        Some("home"),
        "a failed delete keeps the confirmation open"
    );
}

#[test]
fn classic_presets_return_to_formula_and_keep_the_source_scene() {
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Sources);
    state
        .sources
        .add(SourceKindTag::PointCharge.default_source())
        .unwrap();
    let mut panel = PresetPanel::default();
    assert!(panel.load_field("Point charge", &mut state));
    assert_eq!(state.field_mode, FieldMode::Formula);
    assert_eq!(state.sources.entries.len(), 1);
    assert!(state.formula.len() > 3);
}

// Issue #90 slice B: the panel lists the merged preset library with search,
// filters and collapsible groups. The widget assertions use the same headless
// egui pass as the other panel tests; the accessors are exercised directly.

fn render_context() -> egui::Context {
    let ctx = support::context();
    ctx.enable_accesskit();
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    ctx
}

fn run_panel(
    ctx: &egui::Context,
    panel: &mut PresetPanel,
    state: &mut State,
    camera: &mut Camera,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 2200.0),
            )),
            events,
            ..Default::default()
        },
        |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let _ = panel.show(ui, camera, state);
                });
            });
        },
    );
    output.textures_delta.clear();
    output
}

fn painted_texts(output: &egui::FullOutput) -> Vec<String> {
    fn collect(shape: &egui::Shape, out: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(text) => out.push(text.galley.text().to_owned()),
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect(shape, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut out);
    }
    out
}

fn has_text(output: &egui::FullOutput, text: &str) -> bool {
    painted_texts(output).iter().any(|painted| painted == text)
}

struct WidgetNode {
    label: String,
    rect: egui::Rect,
}

fn nodes(output: &egui::FullOutput) -> Vec<WidgetNode> {
    let Some(update) = output.platform_output.accesskit_update.as_ref() else {
        return Vec::new();
    };
    update
        .nodes
        .iter()
        .filter_map(|(_, node)| {
            let bounds = node.bounds()?;
            let label = node
                .label()
                .or_else(|| node.value())
                .unwrap_or_default()
                .to_owned();
            Some(WidgetNode {
                label,
                rect: egui::Rect::from_min_max(
                    egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                    egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                ),
            })
        })
        .collect()
}

fn pointer(pos: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(pos),
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        },
    ]
}

fn click_label(
    ctx: &egui::Context,
    panel: &mut PresetPanel,
    state: &mut State,
    camera: &mut Camera,
    label: &str,
) {
    let out = run_panel(ctx, panel, state, camera, Vec::new());
    let rect = nodes(&out)
        .into_iter()
        .find(|node| node.label == label)
        .unwrap_or_else(|| panic!("missing clickable {label}"))
        .rect;
    run_panel(ctx, panel, state, camera, pointer(rect.center(), true));
    run_panel(ctx, panel, state, camera, pointer(rect.center(), false));
}

#[test]
fn search_narrows_the_library_case_insensitively_and_clearing_restores_it() {
    let ctx = render_context();
    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());
    let mut state = State::new(false);
    let mut camera = Camera::default();

    let all = run_panel(&ctx, &mut panel, &mut state, &mut camera, Vec::new());
    assert!(has_text(&all, "Vortex"), "the catalogue lists Vortex");
    assert!(
        has_text(&all, "Dipole (2D approximation)"),
        "the catalogue lists Dipole"
    );

    panel.set_query("VORTEX");
    let upper = run_panel(&ctx, &mut panel, &mut state, &mut camera, Vec::new());
    assert!(
        has_text(&upper, "Vortex"),
        "search is case-insensitive on names"
    );
    assert!(
        !has_text(&upper, "Dipole (2D approximation)"),
        "non-matching entries are hidden"
    );

    panel.set_query("formula.vortex");
    let by_id = run_panel(&ctx, &mut panel, &mut state, &mut camera, Vec::new());
    assert!(has_text(&by_id, "Vortex"), "search also matches the ID");

    panel.set_query("");
    let restored = run_panel(&ctx, &mut panel, &mut state, &mut camera, Vec::new());
    assert!(
        has_text(&restored, "Dipole (2D approximation)"),
        "clearing the query restores the catalogue"
    );
}

#[test]
fn dimension_and_animated_filters_narrow_entries_and_combine_with_search() {
    let ctx = render_context();
    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());
    let mut state = State::new(false);
    let mut camera = Camera::default();

    panel.set_filters(PresetFilters {
        dimension: Some(3),
        animated: false,
    });
    let three_d = run_panel(&ctx, &mut panel, &mut state, &mut camera, Vec::new());
    assert!(
        has_text(&three_d, "3D swirl (3D)"),
        "the 3D filter keeps 3D formulas"
    );
    assert!(
        !has_text(&three_d, "Vortex"),
        "the 3D filter drops 2D formulas"
    );

    panel.set_filters(PresetFilters {
        dimension: Some(2),
        animated: false,
    });
    let two_d = run_panel(&ctx, &mut panel, &mut state, &mut camera, Vec::new());
    assert!(
        has_text(&two_d, "Vortex"),
        "the 2D filter keeps 2D formulas"
    );
    assert!(
        !has_text(&two_d, "3D swirl (3D)"),
        "the 2D filter drops 3D formulas"
    );

    panel.set_query("dipole");
    let combined = run_panel(&ctx, &mut panel, &mut state, &mut camera, Vec::new());
    assert!(
        has_text(&combined, "Dipole (2D approximation)"),
        "search and filter combine: the 2D Dipole remains"
    );
    assert!(
        !has_text(&combined, "3D Dipole (3D)"),
        "search and filter combine: the 3D Dipole is filtered out"
    );

    panel.set_query("");
    panel.set_filters(PresetFilters {
        dimension: None,
        animated: true,
    });
    let animated = run_panel(&ctx, &mut panel, &mut state, &mut camera, Vec::new());
    assert!(
        has_text(&animated, "Traveling wave field"),
        "the animated filter keeps clock-driven teaching presets"
    );
    assert!(
        has_text(&animated, "Moving Gaussian packet"),
        "every animated preset survives the filter"
    );
    assert!(
        !has_text(&animated, "Vortex"),
        "the animated filter drops static presets"
    );
}

#[test]
fn a_query_with_no_matches_renders_the_explicit_empty_line() {
    let ctx = render_context();
    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());
    let mut state = State::new(false);
    let mut camera = Camera::default();

    panel.set_query("no-such-preset-xyz");
    let out = run_panel(&ctx, &mut panel, &mut state, &mut camera, Vec::new());
    assert!(
        has_text(&out, "No presets match"),
        "an empty result must say so instead of leaving the panel blank"
    );
    assert!(!has_text(&out, "Vortex"), "no entries are painted");
}

#[test]
fn search_matches_source_summaries_not_only_names() {
    let ctx = render_context();
    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());
    let mut state = State::new(false);
    let mut camera = Camera::default();

    // "at the origin" appears only in Coulomb point's summary, never in a
    // name or ID, so a hit proves the summary is searched.
    panel.set_query("at the origin");
    let out = run_panel(&ctx, &mut panel, &mut state, &mut camera, Vec::new());
    assert!(
        has_text(&out, "Coulomb point"),
        "a source summary word must find its row"
    );
    assert!(
        !has_text(&out, "Electric dipole"),
        "non-matching source rows are hidden"
    );
    assert!(
        !has_text(&out, "Vortex"),
        "non-matching formula rows are hidden"
    );
}

#[test]
fn a_group_with_no_matching_entries_renders_no_header() {
    let ctx = render_context();
    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());
    let mut state = State::new(false);
    let mut camera = Camera::default();

    // Only a Classic formula matches, so the Sources/Gallery groups are empty.
    panel.set_query("formula.vortex");
    let out = run_panel(&ctx, &mut panel, &mut state, &mut camera, Vec::new());
    assert!(has_text(&out, "Vortex"), "the matching entry is shown");
    let texts = painted_texts(&out);
    assert!(
        !texts.iter().any(|text| text.starts_with("Sources (")),
        "an empty Sources group must not paint its header"
    );
    assert!(
        !texts.iter().any(|text| text.starts_with("Gallery (")),
        "an empty Gallery group must not paint its header"
    );
    assert!(
        texts.iter().any(|text| text.starts_with("Classic (")),
        "a non-empty group still paints its header"
    );
}

#[test]
fn clicking_library_entries_loads_through_the_existing_seams() {
    let ctx = render_context();
    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());
    let mut state = State::new(false);
    let mut camera = Camera::default();

    click_label(&ctx, &mut panel, &mut state, &mut camera, "Vortex");
    let vortex = presets()
        .iter()
        .find(|preset| preset.name == "Vortex")
        .expect("Vortex is listed");
    assert_eq!(state.formula, vortex.formula);
    assert_eq!(state.mode_3d, vortex.dimension == 3);
    assert_eq!(panel.active_field_name(), Some("Vortex"));

    click_label(&ctx, &mut panel, &mut state, &mut camera, "Coulomb point");
    assert_eq!(
        state.field_mode,
        FieldMode::Sources,
        "a source entry loads the documented source scene"
    );
    assert_eq!(
        panel.active_source_name(),
        Some("Coulomb point"),
        "the clicked source entry owns the highlight"
    );
}

#[test]
fn clicking_a_scalar_library_row_loads_the_scalar_view() {
    let ctx = render_context();
    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());
    let mut state = State::new(false);
    let mut camera = Camera::default();
    state.set_formula("(-y, x)".to_string());

    click_label(&ctx, &mut panel, &mut state, &mut camera, "Scalar saddle");

    assert_eq!(
        state.plot.kind,
        vector_field_explorer::plot::PlotKind::Surface3D,
        "the clicked scalar row switches to the shipped surface view"
    );
    assert_eq!(state.plot.surface.source, "(x^2 - y^2)/2");
    assert_eq!(
        state.formula, "(-y, x)",
        "a scalar row never overwrites the vector formula"
    );
    assert_eq!(panel.active_field_name(), Some("Scalar saddle"));
}

#[test]
fn a_scalar_reload_that_only_changes_the_contour_count_dirties_once() {
    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());
    let mut state = State::new(false);
    assert!(panel.load_field("Scalar saddle", &mut state));
    let levels = state.scalar_contour_levels();

    // Make the contour count the only delta between the state and the row.
    state
        .set_scalar_contour_levels(levels - 1)
        .expect("one fewer level is in bounds");
    state.dirty = false;
    let generation = state.plot.generation;

    assert!(
        panel.load_field("Scalar saddle", &mut state),
        "the same scalar row reloads"
    );
    assert_eq!(state.scalar_contour_levels(), levels);
    assert!(state.dirty, "restoring the contour count dirties the scene");
    assert_eq!(
        state.plot.generation,
        generation + 1,
        "the contour-only reload advances the plot generation exactly once"
    );

    state.dirty = false;
    let generation = state.plot.generation;
    assert!(
        panel.load_field("Scalar saddle", &mut state),
        "the same scalar row reloads again"
    );
    assert!(!state.dirty, "a no-op reload leaves the scene clean");
    assert_eq!(
        state.plot.generation, generation,
        "a no-op reload leaves the plot generation alone"
    );
}

#[test]
fn expansion_changes_are_reported_once_for_persistence() {
    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());
    assert!(
        !panel.take_layout_changed(),
        "a fresh panel has no pending expansion change"
    );

    panel.set_expanded(PresetGroup::Physics, false);
    assert!(!panel.expanded(PresetGroup::Physics));
    assert!(
        panel.take_layout_changed(),
        "an expansion toggle is reported to the app"
    );
    assert!(
        !panel.take_layout_changed(),
        "the report is drained exactly once"
    );

    panel.set_expanded(PresetGroup::Physics, false);
    assert!(
        !panel.take_layout_changed(),
        "setting the same value does not dirty the layout"
    );
}

#[test]
fn camera_presets_active_highlight_and_storage_note_still_render() {
    let ctx = render_context();
    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());
    panel.save_current("home", &cam_home()).expect("save");
    let mut state = State::new(false);
    let mut camera = Camera::default();

    let out = run_panel(&ctx, &mut panel, &mut state, &mut camera, Vec::new());
    for text in [
        "Saved views",
        "home",
        "active",
        "Save current view",
        "Reset view",
        "Fit domain",
    ] {
        assert!(has_text(&out, text), "the panel still renders {text}");
    }
}

#[test]
fn collapsing_a_group_header_is_a_layout_change_not_a_preset_load() {
    let ctx = render_context();
    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());
    let mut state = State::new(false);
    let mut camera = Camera::default();
    let formula_before = state.formula.clone();

    let out = run_panel(&ctx, &mut panel, &mut state, &mut camera, Vec::new());
    let rect = nodes(&out)
        .into_iter()
        .find(|node| node.label.starts_with("Classic ("))
        .expect("Classic group header")
        .rect;
    run_panel(
        &ctx,
        &mut panel,
        &mut state,
        &mut camera,
        pointer(rect.center(), true),
    );
    run_panel(
        &ctx,
        &mut panel,
        &mut state,
        &mut camera,
        pointer(rect.center(), false),
    );

    assert!(!panel.expanded(PresetGroup::Classic));
    assert!(
        panel.take_layout_changed(),
        "the header toggle must reach the app"
    );
    assert_eq!(
        state.formula, formula_before,
        "collapsing a group must not apply a preset"
    );
}

// Issue #90 slice B2c: the My-presets section saves whole scenes, loads one
// atomically, deletes with confirmation and round-trips the collection as
// portable text. App-level tests drive the panel through the accesskit harness
// and then the real `finish_presets` seam.

fn document_named(name: &str) -> SceneDocument {
    SceneDocument::capture(
        name,
        ViewRecord::default(),
        ParameterSet::default(),
        SceneCamera::default(),
    )
}

fn test_parameters(value: f64) -> ParameterSet {
    let mut parameters = ParameterSet::default();
    parameters.entries.push(Parameter {
        name: "k".to_string(),
        value,
        min: Some(0.0),
        max: Some(10.0),
        step: 0.5,
    });
    parameters
}

fn app_in(dir: &std::path::Path) -> VfApp {
    let mut app = VfApp::new();
    app.presets = panel_in(dir);
    app
}

fn run_app_panel(
    ctx: &egui::Context,
    app: &mut VfApp,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 2200.0),
            )),
            events,
            ..Default::default()
        },
        |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let _ = app.presets.show(ui, &mut app.camera, &mut app.state);
                });
            });
        },
    );
    app.finish_presets_for_test();
    output.textures_delta.clear();
    output
}

fn click_app_label(ctx: &egui::Context, app: &mut VfApp, label: &str) {
    let out = run_app_panel(ctx, app, Vec::new());
    let rect = nodes(&out)
        .into_iter()
        .find(|node| node.label == label)
        .unwrap_or_else(|| panic!("missing clickable {label}"))
        .rect;
    run_app_panel(ctx, app, pointer(rect.center(), true));
    run_app_panel(ctx, app, pointer(rect.center(), false));
}

#[test]
fn saving_a_scene_captures_view_parameters_and_camera() {
    let ctx = render_context();
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.state.set_formula("x + y".to_string());
    app.state
        .set_domain(Vec3::new(-2.0, -1.0, 0.0), Vec3::new(2.0, 1.0, 1.0));
    app.state
        .commit_parameters(test_parameters(2.5))
        .expect("valid parameters");
    app.camera = Camera {
        position: [0.25, -0.5],
        scale: 2.5,
    };
    app.orbit = OrbitCamera {
        yaw: 0.7,
        pitch: 0.3,
        distance: 6.0,
        target: Vec3::new(1.0, 2.0, 3.0),
        fov_y: 1.2,
        ortho_zoom: 2.5,
        up_axis: Default::default(),
    };
    app.presets.set_scene_name("scene one");

    click_app_label(&ctx, &mut app, "Save current scene");

    let document = app
        .presets
        .scenes()
        .get("scene one")
        .expect("the scene was captured");
    assert_eq!(document.view, ViewRecord::capture(&app.state));
    assert_eq!(document.view.domain_min, [-2.0, -1.0, 0.0]);
    assert_eq!(document.view.domain_max, [2.0, 1.0, 1.0]);
    assert_eq!(document.parameters, app.state.parameters);
    assert_eq!(document.camera.position, [0.25, -0.5]);
    assert_eq!(document.camera.scale, 2.5);
    assert_eq!(document.camera.yaw, 0.7);
    assert_eq!(document.camera.pitch, 0.3);
    assert_eq!(document.camera.distance, 6.0);
    assert_eq!(document.camera.target, [1.0, 2.0, 3.0]);
    assert_eq!(document.camera.fov_y, 1.2);
    assert_eq!(document.camera.ortho_zoom, 2.5);
}

#[test]
fn duplicate_scene_names_are_rejected_with_a_message() {
    let ctx = render_context();
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.presets.set_scene_name("home");
    click_app_label(&ctx, &mut app, "Save current scene");
    assert_eq!(app.presets.scenes().scenes().len(), 1);

    app.state.set_formula("x - y".to_string());
    app.presets.set_scene_name("home");
    click_app_label(&ctx, &mut app, "Save current scene");

    assert_eq!(
        app.presets.scenes().scenes().len(),
        1,
        "a duplicate name must not append a second scene"
    );
    assert!(
        app.presets
            .scene_message()
            .is_some_and(|message| message.contains("already exists")),
        "the duplicate must be reported: {:?}",
        app.presets.scene_message()
    );
}

#[test]
fn loading_a_scene_applies_view_and_parameters_without_touching_ui_preferences() {
    let ctx = render_context();
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());

    let mut saved_state = State::new(false);
    saved_state.set_formula("x - y".to_string());
    saved_state.set_domain(Vec3::new(-3.0, -2.0, -1.0), Vec3::new(3.0, 2.0, 1.0));
    let saved_parameters = test_parameters(4.0);
    let saved_view = ViewRecord::capture(&saved_state);
    app.presets
        .insert_scene(SceneDocument::capture(
            "saved",
            saved_view.clone(),
            saved_parameters.clone(),
            SceneCamera::default(),
        ))
        .expect("scene stores");

    app.state.set_formula("x + y".to_string());
    app.state.set_domain(Vec3::splat(-1.0), Vec3::splat(1.0));
    app.state
        .commit_parameters(test_parameters(1.0))
        .expect("valid parameters");
    app.style.theme_dark = !app.style.theme_dark;
    app.layout.presets_expanded = false;
    app.presets.set_expanded(PresetGroup::Physics, false);
    let style_before = app.style.clone();
    let layout_before = app.layout.clone();
    let preset_ui_before = app.presets.expanded_state();
    let viewport_before = app.state.viewport_style;

    click_app_label(&ctx, &mut app, "Load");

    assert_eq!(app.state.formula, saved_view.formula);
    assert_eq!(
        app.state.domain.min,
        Vec3::from_array(saved_view.domain_min)
    );
    assert_eq!(
        app.state.domain.max,
        Vec3::from_array(saved_view.domain_max)
    );
    assert_eq!(app.state.parameters, saved_parameters);
    assert_eq!(app.style, style_before, "loading keeps the app style");
    assert_eq!(app.layout, layout_before, "loading keeps the layout record");
    assert_eq!(
        app.presets.expanded_state(),
        preset_ui_before,
        "loading keeps the preset expansion record"
    );
    assert_eq!(
        app.state.viewport_style, viewport_before,
        "loading keeps the viewport style"
    );
}

#[test]
fn loading_a_scene_clamps_the_camera_to_live_limits() {
    let ctx = render_context();
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    let camera = SceneCamera {
        position: [1.0, 2.0],
        scale: 1.5,
        yaw: 0.4,
        pitch: 2.0,
        distance: 100.0,
        target: [1.0, 2.0, 3.0],
        fov_y: 1.0,
        ortho_zoom: 1.5,
    };
    app.presets
        .insert_scene(SceneDocument::capture(
            "clamped",
            ViewRecord::default(),
            ParameterSet::default(),
            camera,
        ))
        .expect("scene stores");
    app.fly = Some(FlyCamera::default());

    click_app_label(&ctx, &mut app, "Load");

    assert_eq!(app.orbit.yaw, 0.4);
    assert_eq!(app.orbit.pitch, 1.55, "pitch clamps to the live limit");
    assert_eq!(
        app.orbit.distance,
        OrbitCamera::MAX_DISTANCE,
        "distance clamps to the live limit"
    );
    assert_eq!(app.orbit.target, Vec3::new(1.0, 2.0, 3.0));
    assert_eq!(app.orbit.fov_y, 1.0);
    assert_eq!(app.orbit.ortho_zoom, 1.5);
    assert_eq!(app.camera.position, [1.0, 2.0]);
    assert_eq!(app.camera.scale, 1.5);
    assert!(app.fly.is_none(), "loading a scene leaves fly mode");
}

#[test]
fn deleting_a_scene_needs_a_confirmation_click() {
    let ctx = render_context();
    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());
    panel
        .insert_scene(document_named("gone"))
        .expect("scene stores");
    let mut state = State::new(false);
    let mut camera = Camera::default();

    click_label(&ctx, &mut panel, &mut state, &mut camera, "Delete");
    assert!(
        panel.scenes().get("gone").is_some(),
        "the first click only asks for confirmation"
    );

    click_label(&ctx, &mut panel, &mut state, &mut camera, "Delete scene");
    assert!(
        panel.scenes().get("gone").is_none(),
        "the confirmation click deletes the scene"
    );
    let (persisted, _) = scene_store::load_dir(dir.path());
    assert!(
        persisted.get("gone").is_none(),
        "the deletion reaches the scene file"
    );
}

#[test]
fn portable_scene_text_round_trips_through_apply_text() {
    let ctx = render_context();
    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());
    panel
        .insert_scene(document_named("one"))
        .expect("scene one stores");
    panel
        .insert_scene(document_named("two"))
        .expect("scene two stores");
    let mut state = State::new(false);
    let mut camera = Camera::default();

    click_label(&ctx, &mut panel, &mut state, &mut camera, "Export text");
    assert!(
        !panel.scene_text().is_empty(),
        "export fills the portable text buffer"
    );
    assert!(panel.delete_scene("one"), "mutating the live store");

    click_label(&ctx, &mut panel, &mut state, &mut camera, "Apply text");

    assert!(panel.scenes().get("one").is_some());
    assert!(panel.scenes().get("two").is_some());
    let (persisted, _) = scene_store::load_dir(dir.path());
    assert!(persisted.get("one").is_some());
    assert!(persisted.get("two").is_some());
}

#[test]
fn invalid_portable_text_leaves_the_saved_scenes_untouched() {
    let ctx = render_context();
    let dir = tempfile::tempdir().unwrap();
    let mut panel = panel_in(dir.path());
    panel
        .insert_scene(document_named("keeper"))
        .expect("scene stores");
    let mut state = State::new(false);
    let mut camera = Camera::default();

    panel.set_scene_text("this is not scene JSON");
    click_label(&ctx, &mut panel, &mut state, &mut camera, "Apply text");
    assert!(
        panel.scenes().get("keeper").is_some(),
        "corrupt text must not change the store"
    );
    assert!(
        panel.scene_message().is_some(),
        "corrupt text must be reported"
    );

    panel.set_scene_text(r#"{"version":2,"scenes":[]}"#);
    click_label(&ctx, &mut panel, &mut state, &mut camera, "Apply text");
    assert!(
        panel.scenes().get("keeper").is_some(),
        "future-version text must not change the store"
    );
    assert!(
        panel
            .scene_message()
            .is_some_and(|message| message.contains("version")),
        "the unsupported version must be named: {:?}",
        panel.scene_message()
    );
}

#[test]
fn scene_name_rules_are_enforced_by_the_panel() {
    let ctx = render_context();
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());

    app.presets.set_scene_name("bad\u{0}name");
    click_app_label(&ctx, &mut app, "Save current scene");
    assert!(
        app.presets.scenes().scenes().is_empty(),
        "a control character must not be stored"
    );
    assert!(
        app.presets.scene_message().is_some(),
        "the rejected name must be reported"
    );

    app.presets.set_scene_name("a".repeat(65));
    click_app_label(&ctx, &mut app, "Save current scene");
    assert!(
        app.presets.scenes().scenes().is_empty(),
        "a 65-byte name must not be stored"
    );
    assert!(
        app.presets.scene_message().is_some(),
        "the over-long name must be reported"
    );
}

#[test]
fn saved_scenes_load_from_the_preset_directory() {
    let ctx = render_context();
    let dir = tempfile::tempdir().unwrap();
    let store = scene_store::SceneStore::from_documents(vec![document_named("from disk")])
        .expect("valid fixture store");
    scene_store::save_dir(dir.path(), &store).expect("fixture scene file");

    let mut panel = panel_in(dir.path());
    let mut state = State::new(false);
    let mut camera = Camera::default();
    let out = run_panel(&ctx, &mut panel, &mut state, &mut camera, Vec::new());

    assert!(
        has_text(&out, "from disk"),
        "a scene written to the preset directory appears as a row"
    );
    assert!(panel.scenes().get("from disk").is_some());
}

#[test]
fn save_failures_surface_without_panicking() {
    let ctx = render_context();
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("not-a-directory");
    std::fs::write(&file, "x").unwrap();
    let dir = file.join("child");
    let mut app = app_in(&dir);
    app.presets.set_scene_name("unwritable");

    click_app_label(&ctx, &mut app, "Save current scene");

    assert!(
        app.presets.scenes().scenes().is_empty(),
        "a failed save leaves the in-memory store consistent"
    );
    assert!(
        app.presets.scene_message().is_some(),
        "the write failure surfaces in the panel"
    );
    let result = app.presets.insert_scene(document_named("unwritable"));
    assert!(result.is_err(), "the write failure is reported");
}
