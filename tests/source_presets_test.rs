use std::collections::HashSet;
use vector_field_explorer::presets::{preset_approximation_label, presets};
use vector_field_explorer::source_presets::{
    load_source_preset, source_preset, source_presets, source_strength,
};
use vector_field_explorer::source_scene::{FieldMode, SourceKindTag};
use vector_field_explorer::sources::SourceKind;
use vector_field_explorer::viewmodel::State;

/// The ticket-91 source entries built only from existing source kinds.
const NEW_SOURCE_PRESETS: [&str; 7] = [
    "Unequal charges",
    "Off-axis ring",
    "Sphere inside and out",
    "Paired loops",
    "Helmholtz pair",
    "Opposing wires",
    "Solenoid ends",
];

#[test]
fn source_presets_have_unique_names_and_cover_all_eight_kinds() {
    let presets = source_presets();
    let names: HashSet<&str> = presets.iter().map(|preset| preset.name).collect();
    assert_eq!(names.len(), presets.len());
    assert_eq!(presets.len(), 17);
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

fn built_scene(name: &str) -> vector_field_explorer::source_scene::SourceScene {
    let preset = source_preset(name).unwrap_or_else(|| panic!("source preset `{name}` missing"));
    (preset.build)()
}

#[test]
fn teaching_source_presets_use_the_expected_existing_kinds_and_signs() {
    let unequal = built_scene("Unequal charges");
    assert_eq!(unequal.entries.len(), 4);
    let mut positive = 0;
    let mut negative = 0;
    for entry in &unequal.entries {
        assert!(matches!(entry.source.kind, SourceKind::PointCharge { .. }));
        let strength = source_strength(&entry.source);
        if strength > 0.0 {
            positive += 1;
        } else {
            negative += 1;
        }
    }
    assert_eq!((positive, negative), (2, 2), "alternating quadrupole signs");

    let ring = built_scene("Off-axis ring");
    assert_eq!(ring.entries.len(), 1);
    assert!(matches!(
        ring.entries[0].source.kind,
        SourceKind::ChargedRing { .. }
    ));
    assert!(
        ring.entries[0].source.center.x.abs() > 1e-9,
        "the ring is deliberately off-axis"
    );

    let sphere = built_scene("Sphere inside and out");
    assert_eq!(sphere.entries.len(), 1);
    assert!(matches!(
        sphere.entries[0].source.kind,
        SourceKind::SolidSphere { .. }
    ));

    let paired = built_scene("Paired loops");
    assert_eq!(paired.entries.len(), 2);
    assert!(
        paired
            .entries
            .iter()
            .all(|entry| matches!(entry.source.kind, SourceKind::CurrentLoop { .. }))
    );
    assert!(
        paired
            .entries
            .iter()
            .all(|entry| source_strength(&entry.source) > 0.0),
        "paired loops drive current the same way"
    );

    let helmholtz = built_scene("Helmholtz pair");
    assert_eq!(helmholtz.entries.len(), 2);
    let mut axial = Vec::new();
    let mut radius = 0.0;
    for entry in &helmholtz.entries {
        match entry.source.kind {
            SourceKind::CurrentLoop { current, radius: r } => {
                assert!(current > 0.0);
                axial.push(entry.source.center.z);
                radius = r;
            }
            _ => panic!("Helmholtz pair must use CurrentLoop"),
        }
    }
    axial.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let separation = (axial[1] - axial[0]).abs();
    assert!(
        (separation - radius).abs() < 1e-9,
        "the classic separation equals the loop radius: {separation} vs {radius}"
    );

    let wires = built_scene("Opposing wires");
    assert_eq!(wires.entries.len(), 2);
    let mut currents: Vec<f64> = wires
        .entries
        .iter()
        .map(|entry| {
            assert!(matches!(entry.source.kind, SourceKind::StraightWire { .. }));
            source_strength(&entry.source)
        })
        .collect();
    currents.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(currents, vec![-1.0, 1.0], "opposing wire currents");

    let solenoid = built_scene("Solenoid ends");
    assert_eq!(solenoid.entries.len(), 1);
    assert!(matches!(
        solenoid.entries[0].source.kind,
        SourceKind::Solenoid { .. }
    ));
}

#[test]
fn teaching_source_presets_validate_and_load_inside_the_canonical_domain() {
    for name in NEW_SOURCE_PRESETS {
        let scene = built_scene(name);
        scene
            .validate()
            .unwrap_or_else(|error| panic!("source preset `{name}` must validate: {error:?}"));

        let preset = source_preset(name).unwrap();
        let mut state = State::new(true);
        assert!(load_source_preset(preset, &mut state), "{name} must load");
        assert_eq!(state.field_mode, FieldMode::Sources);
        assert!(state.mode_3d);
        let domain = state.domain;
        assert_eq!(
            domain.domain_size(),
            glam::Vec3::new(4.0, 4.0, 4.0),
            "{name} keeps the canonical domain"
        );
        for entry in &state.sources.entries {
            let center = entry.source.center.as_vec3();
            assert!(
                (0..3).all(
                    |axis| domain.min[axis] <= center[axis] && center[axis] <= domain.max[axis]
                ),
                "{name} places every source inside the sampled domain"
            );
        }
        assert!(
            state.sources.entries.len() <= vector_field_explorer::sources::MAX_SOURCES,
            "{name} stays within MAX_SOURCES"
        );
        assert_eq!(
            SourceKindTag::of(&state.sources.entries[0].source.kind),
            SourceKindTag::of(&scene.entries[0].source.kind),
            "{name} loads the built scene unchanged"
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
