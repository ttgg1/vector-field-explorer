//! Built-in field-preset coverage (ticket 31).
//!
//! Moved out of `src/presets.rs`'s inline module and extended for the
//! categorized physics presets: every formula must parse, match its declared
//! dimension, stay finite on the default domain, and appear under its
//! declared category in the declared order.

use std::collections::HashSet;

use glam::{UVec3, Vec3};
use vector_field_explorer::domain::Domain;
use vector_field_explorer::field::FieldExpr;
use vector_field_explorer::presets::{PresetCategory, presets, presets_in};

const REQUIRED: [&str; 18] = [
    // Classic (pre-existing)
    "Pump",
    "Vortex",
    "Shear flow",
    "Curl example",
    "No-curl example",
    "3D radial",
    "3D swirl",
    "3D helix",
    // Electrostatics (pre-existing + ticket 31)
    "Dipole",
    "Source and sink",
    "Point charge",
    "Like charges",
    "Quadrupole",
    // Magnetostatics (ticket 31)
    "Straight wire",
    "Magnetic dipole",
    "Solenoid",
    // Electrodynamics (ticket 31)
    "Plane wave",
    "Circular wave",
];

#[test]
fn every_preset_parses_with_declared_dimension() {
    assert!(!presets().is_empty(), "preset list must not be empty");
    for preset in presets() {
        let parsed = FieldExpr::parse(preset.formula).unwrap_or_else(|error| {
            panic!(
                "preset `{}` formula `{}` must parse: {error}",
                preset.name, preset.formula
            )
        });
        assert_eq!(
            parsed.dimension, preset.dimension,
            "preset `{}` declared dimension must match its formula",
            preset.name
        );
    }
}

#[test]
fn preset_names_are_unique() {
    let names: HashSet<&str> = presets().iter().map(|preset| preset.name).collect();
    assert_eq!(names.len(), presets().len(), "preset names must be unique");
}

#[test]
fn required_presets_present() {
    let names: HashSet<&str> = presets().iter().map(|preset| preset.name).collect();
    for required in REQUIRED {
        assert!(
            names.contains(required),
            "preset `{required}` must be listed"
        );
    }
}

#[test]
fn preset_count_stays_within_the_ticket_limit() {
    assert!(
        presets().len() <= 20,
        "ticket 31 caps the built-in list at 20 presets, found {}",
        presets().len()
    );
}

#[test]
fn categories_are_ordered_and_titled() {
    let titles: Vec<&str> = PresetCategory::ALL
        .iter()
        .map(|category| category.title())
        .collect();
    assert_eq!(
        titles,
        [
            "Classic",
            "Electrostatics",
            "Magnetostatics",
            "Electrodynamics"
        ],
        "the panel renders categories in this declared order"
    );
    for category in PresetCategory::ALL {
        assert!(
            !category.title().is_empty(),
            "every category needs a section heading"
        );
        assert!(
            !category.description().is_empty(),
            "every category heading needs an explaining line"
        );
    }
}

#[test]
fn every_category_is_non_empty() {
    for category in PresetCategory::ALL {
        assert!(
            presets_in(category).next().is_some(),
            "category `{}` must list at least one preset",
            category.title()
        );
    }
}

#[test]
fn grouping_preserves_declared_category_order() {
    let mut grouped: Vec<&str> = Vec::new();
    for category in PresetCategory::ALL {
        grouped.extend(presets_in(category).map(|preset| preset.name));
    }
    let declared: Vec<&str> = presets().iter().map(|preset| preset.name).collect();
    assert_eq!(
        grouped, declared,
        "presets() must be ordered by category, declaration order within each"
    );
}

#[test]
fn new_physics_presets_have_the_declared_category_and_dimension() {
    let expected = [
        ("Point charge", PresetCategory::Electrostatics, 2),
        ("Like charges", PresetCategory::Electrostatics, 2),
        ("Quadrupole", PresetCategory::Electrostatics, 2),
        ("Straight wire", PresetCategory::Magnetostatics, 2),
        ("Magnetic dipole", PresetCategory::Magnetostatics, 3),
        ("Solenoid", PresetCategory::Magnetostatics, 3),
        ("Plane wave", PresetCategory::Electrodynamics, 3),
        ("Circular wave", PresetCategory::Electrodynamics, 3),
    ];
    for (name, category, dimension) in expected {
        let preset = presets()
            .iter()
            .find(|preset| preset.name == name)
            .unwrap_or_else(|| panic!("preset `{name}` must be listed"));
        assert_eq!(preset.category, category, "preset `{name}` category");
        assert_eq!(preset.dimension, dimension, "preset `{name}` dimension");
    }
}

#[test]
fn every_preset_stays_finite_on_the_default_domain() {
    let domain = Domain::default();
    for preset in presets() {
        let field = FieldExpr::parse(preset.formula)
            .unwrap_or_else(|error| panic!("preset `{}` must parse: {error}", preset.name));
        for point in domain.grid_points(UVec3::new(11, 11, 11)) {
            let value = field.eval3(point).unwrap_or_else(|| {
                panic!("preset `{}` evaluated to NaN/Inf at {point:?}", preset.name)
            });
            assert!(
                value.is_finite()
                    && value.x.is_finite()
                    && value.y.is_finite()
                    && value.z.is_finite(),
                "preset `{}` must stay finite at {point:?}, got {value:?}",
                preset.name
            );
        }
    }
}

#[test]
fn softening_covers_the_origin() {
    // The singular-looking presets are sampled exactly at the origin, the
    // worst point for every softened denominator.
    for name in ["Point charge", "Straight wire", "Magnetic dipole"] {
        let preset = presets()
            .iter()
            .find(|preset| preset.name == name)
            .unwrap_or_else(|| panic!("preset `{name}` must be listed"));
        let field = FieldExpr::parse(preset.formula).expect("preset parses");
        let value = field
            .eval3(Vec3::ZERO)
            .unwrap_or_else(|| panic!("preset `{name}` must be finite at the origin"));
        assert!(value.is_finite(), "preset `{name}` at origin: {value:?}");
    }
}
