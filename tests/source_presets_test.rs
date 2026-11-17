use std::collections::HashSet;
use vector_field_explorer::presets::{preset_approximation_label, presets};
use vector_field_explorer::source_presets::{load_source_preset, source_preset, source_presets};
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
    assert_eq!(kinds.len(), 8, "the catalogue covers every source kind");
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
    assert_eq!(
        state.sources, before,
        "a formula preset never rewrites sources"
    );
    assert_eq!(
        preset_approximation_label("Dipole"),
        Some("2D approximation")
    );
    assert_eq!(
        preset_approximation_label("Point charge"),
        Some("2D approximation")
    );
    assert_eq!(preset_approximation_label("3D Dipole"), None);
    assert_eq!(preset_approximation_label("Pump"), None);
    for preset in presets() {
        assert!(
            preset_approximation_label(preset.name).is_none()
                || preset.dimension == 2 && preset.formula.contains("x")
        );
    }
}

#[test]
fn panel_source_preset_loads_the_scene_and_tracks_the_active_source() {
    let mut state = State::new(true);
    let mut panel = vector_field_explorer::ui::presets::PresetPanel::default();
    assert!(panel.load_source_field("Electric dipole", &mut state));
    assert_eq!(state.field_mode, FieldMode::Sources);
    assert_eq!(state.sources.entries.len(), 2);
    assert_eq!(panel.active_source_name(), Some("Electric dipole"));
    assert!(!panel.load_source_field("missing", &mut state));
    assert_eq!(
        panel.active_source_name(),
        Some("Electric dipole"),
        "a failed load keeps the highlight"
    );
}
