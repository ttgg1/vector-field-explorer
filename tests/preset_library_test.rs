//! Preset library coverage (ticket 90, slice A).
//!
//! The catalogue is a pure index over the existing formula and source
//! presets: stable IDs, deterministic order, group/filter metadata, and
//! search. These tests pin the contract without touching the app.

use std::collections::HashSet;

use vector_field_explorer::preset_library::{
    PresetEntry, PresetFilters, PresetGroup, PresetId, PresetLibrary, PresetLibraryError,
    PresetValidity,
};
use vector_field_explorer::presets::{PresetCategory, presets};
use vector_field_explorer::source_presets::source_presets;

fn builtin() -> PresetLibrary {
    PresetLibrary::builtin().expect("the built-in catalogue must build")
}

fn names(entries: &[&PresetEntry]) -> Vec<&'static str> {
    entries.iter().map(|entry| entry.name).collect()
}

fn ids(entries: &[&PresetEntry]) -> Vec<String> {
    entries
        .iter()
        .map(|entry| entry.id.as_str().to_owned())
        .collect()
}

/// Ticket-91 formula entries advanced by the shared clock.
const ANIMATED_FORMULAS: [&str; 6] = [
    "Pulsating radial field",
    "Traveling wave field",
    "Standing wave field",
    "Moving Gaussian packet",
    "Rotating uniform field",
    "3D plane wave",
];

#[test]
fn ids_are_unique_and_stable() {
    let library = builtin();
    let id_for = |name: &str| {
        library
            .entries()
            .iter()
            .find(|entry| entry.name == name)
            .map(|entry| entry.id.as_str())
    };

    assert_eq!(id_for("Vortex"), Some("formula.vortex"));
    assert_eq!(id_for("3D swirl"), Some("formula.3d_swirl"));
    assert_eq!(id_for("ABC flow"), Some("formula.abc-flow"));
    assert_eq!(id_for("3D plane wave"), Some("formula.3d-plane-wave"));
    assert_eq!(id_for("Coulomb point"), Some("source.coulomb_point"));

    let first: Vec<&str> = library
        .entries()
        .iter()
        .map(|entry| entry.id.as_str())
        .collect();
    let unique: HashSet<&str> = first.iter().copied().collect();
    assert_eq!(unique.len(), first.len(), "preset ids must be unique");

    let second = builtin();
    let second_ids: Vec<&str> = second
        .entries()
        .iter()
        .map(|entry| entry.id.as_str())
        .collect();
    assert_eq!(first, second_ids, "two builds must agree");
}

#[test]
fn accepts_valid_ids_and_rejects_malformed() {
    assert_eq!(PresetId::new("a").unwrap().as_str(), "a");
    let boundary = "a".repeat(64);
    assert_eq!(PresetId::new(&boundary).unwrap().as_str(), boundary);

    let too_long = "a".repeat(65);
    let invalid = [
        "",
        "Vortex",
        "has space",
        "a/b",
        "a:b",
        ".leading",
        "vortexé",
        too_long.as_str(),
    ];
    for value in invalid {
        let error =
            PresetId::new(value).expect_err(&format!("`{value}` must be rejected as a preset id"));
        assert!(
            !error.to_string().is_empty(),
            "a rejection needs a descriptive message for `{value}`"
        );
    }
}

#[test]
fn formula_entries_keep_catalogue_order_and_groups() {
    let library = builtin();
    let formula: Vec<&PresetEntry> = library
        .entries()
        .iter()
        .filter(|entry| matches!(entry.group, PresetGroup::Classic | PresetGroup::Physics))
        .collect();

    let expected: Vec<&str> = presets().iter().map(|preset| preset.name).collect();
    assert_eq!(names(&formula), expected, "formula order follows presets()");

    for (entry, preset) in formula.iter().zip(presets()) {
        let expected_group = match preset.category {
            PresetCategory::Classic => PresetGroup::Classic,
            PresetCategory::Electrostatics
            | PresetCategory::Magnetostatics
            | PresetCategory::Electrodynamics => PresetGroup::Physics,
        };
        assert_eq!(entry.group, expected_group, "group for `{}`", preset.name);
        assert_eq!(
            entry.filters.dimension,
            Some(preset.dimension),
            "dimension for `{}`",
            preset.name
        );
        assert_eq!(
            entry.filters.animated,
            ANIMATED_FORMULAS.contains(&preset.name),
            "animated flag for `{}`",
            preset.name
        );
    }
}

#[test]
fn source_entries_use_the_source_group_and_gallery_last() {
    let library = builtin();
    let source_entries: Vec<&PresetEntry> = library
        .entries()
        .iter()
        .filter(|entry| matches!(entry.group, PresetGroup::Sources | PresetGroup::Gallery))
        .collect();

    let expected: Vec<&str> = source_presets().iter().map(|preset| preset.name).collect();
    assert_eq!(
        names(&source_entries),
        expected,
        "source order follows source_presets()"
    );

    let (last, rest) = source_entries.split_last().expect("source presets exist");
    assert_eq!(last.name, "Source gallery");
    assert_eq!(last.group, PresetGroup::Gallery);
    for entry in rest {
        assert_eq!(entry.group, PresetGroup::Sources, "{}", entry.name);
    }
    for entry in &source_entries {
        assert_eq!(
            entry.filters.dimension, None,
            "sources are dimension-agnostic"
        );
        assert!(!entry.filters.animated, "sources are static scenes");
    }
}

#[test]
fn search_is_trimmed_case_insensitive_and_ordered() {
    let library = builtin();

    let all = library.search("   ");
    assert_eq!(
        all.len(),
        library.entries().len(),
        "blank query returns all"
    );
    let catalogue: Vec<&str> = library.entries().iter().map(|entry| entry.name).collect();
    assert_eq!(names(&all), catalogue, "results stay in catalogue order");

    let by_name = library.search("  vOrTeX  ");
    assert_eq!(by_name.len(), 1);
    assert_eq!(by_name[0].name, "Vortex");
    assert_eq!(by_name[0].id.as_str(), "formula.vortex");

    let by_summary = library.search("UNIFORMLY CHARGED");
    assert_eq!(
        names(&by_summary),
        ["Uniform ring", "Thin disc", "Charged sphere"],
        "summary matches keep catalogue order"
    );

    let by_id = library.search("source.charged_line");
    assert_eq!(by_id.len(), 1);
    assert_eq!(by_id[0].name, "Charged line");

    assert!(library.search("no such preset").is_empty());
}

#[test]
fn dimension_filter_keeps_sources_and_matching_dimension() {
    let library = builtin();
    let source_count = source_presets().len();

    for dimension in [2, 3] {
        let filtered = library.filtered(PresetFilters {
            dimension: Some(dimension),
            animated: false,
        });
        let mut expected: Vec<&str> = presets()
            .iter()
            .filter(|preset| preset.dimension == dimension)
            .map(|preset| preset.name)
            .collect();
        expected.extend(source_presets().iter().map(|preset| preset.name));

        assert_eq!(names(&filtered), expected, "dimension {dimension}");
        assert_eq!(
            filtered.len(),
            expected.len(),
            "every source is kept and only matching formulas are"
        );
        assert!(
            filtered.len() >= source_count,
            "sources survive every dimension filter"
        );
    }

    let all = library.filtered(PresetFilters::default());
    assert_eq!(all.len(), library.entries().len());
}

#[test]
fn animated_filter_returns_exactly_the_clock_driven_entries() {
    let library = builtin();

    let static_only = library.filtered(PresetFilters {
        dimension: None,
        animated: false,
    });
    assert_eq!(
        static_only.len(),
        library.entries().len(),
        "animated = false applies no filter"
    );

    let animated_only = library.filtered(PresetFilters {
        dimension: None,
        animated: true,
    });
    assert_eq!(
        names(&animated_only),
        ANIMATED_FORMULAS,
        "animated entries are exactly the clock-driven teaching presets"
    );
}

#[test]
fn teaching_formula_entries_carry_stable_metadata() {
    let library = builtin();
    let expected = [
        (
            "formula.pulsating-radial",
            "Pulsating radial field",
            PresetGroup::Classic,
            true,
            PresetValidity::MathematicalIllustration,
        ),
        (
            "formula.rotating-field",
            "Rotating field",
            PresetGroup::Classic,
            false,
            PresetValidity::MathematicalIllustration,
        ),
        (
            "formula.traveling-wave-field",
            "Traveling wave field",
            PresetGroup::Physics,
            true,
            PresetValidity::MathematicalIllustration,
        ),
        (
            "formula.standing-wave-field",
            "Standing wave field",
            PresetGroup::Physics,
            true,
            PresetValidity::MathematicalIllustration,
        ),
        (
            "formula.quadrupole-unequal-charges",
            "Quadrupole (unequal charges)",
            PresetGroup::Physics,
            false,
            PresetValidity::MathematicalIllustration,
        ),
        (
            "formula.saddle-potential",
            "Saddle potential",
            PresetGroup::Classic,
            false,
            PresetValidity::MathematicalIllustration,
        ),
        (
            "formula.anisotropic-well",
            "Anisotropic well",
            PresetGroup::Classic,
            false,
            PresetValidity::MathematicalIllustration,
        ),
        (
            "formula.double-well",
            "Double-well potential",
            PresetGroup::Classic,
            false,
            PresetValidity::MathematicalIllustration,
        ),
        (
            "formula.softened-dipole",
            "Softened dipole",
            PresetGroup::Physics,
            false,
            PresetValidity::PhysicallyValidRegularized,
        ),
        (
            "formula.moving-gaussian",
            "Moving Gaussian packet",
            PresetGroup::Physics,
            true,
            PresetValidity::MathematicalIllustration,
        ),
    ];

    for (id, name, group, animated, validity) in expected {
        let entry = library
            .get(&PresetId::new(id).unwrap())
            .unwrap_or_else(|| panic!("entry `{id}` must be retrievable"));
        assert_eq!(entry.name, name, "name for `{id}`");
        assert_eq!(entry.group, group, "group for `{id}`");
        assert_eq!(entry.filters.dimension, Some(2), "dimension for `{id}`");
        assert_eq!(entry.filters.animated, animated, "animated flag for `{id}`");
        assert_eq!(entry.validity, Some(validity), "validity label for `{id}`");
        assert!(!entry.summary.is_empty(), "summary for `{id}`");
        assert!(!entry.assumptions.is_empty(), "assumptions for `{id}`");
        assert!(
            !entry.exploration_prompt.is_empty(),
            "exploration prompt for `{id}`"
        );
    }
}

#[test]
fn slice_b_3d_formula_entries_carry_stable_metadata() {
    let library = builtin();
    let expected = [
        (
            "formula.uniform-field",
            "Uniform field",
            PresetGroup::Classic,
            false,
            PresetValidity::PhysicallyValid,
        ),
        (
            "formula.quadrupole-field",
            "Quadrupole field",
            PresetGroup::Physics,
            false,
            PresetValidity::PhysicallyValid,
        ),
        (
            "formula.softened-point-charge",
            "Softened point charge",
            PresetGroup::Physics,
            false,
            PresetValidity::PhysicallyValidRegularized,
        ),
        (
            "formula.helical-solenoid-field",
            "Helical solenoid field",
            PresetGroup::Physics,
            false,
            PresetValidity::PhysicallyValid,
        ),
        (
            "formula.abc-flow",
            "ABC flow",
            PresetGroup::Classic,
            false,
            PresetValidity::MathematicalIllustration,
        ),
        (
            "formula.3d-shear-flow",
            "3D shear flow",
            PresetGroup::Classic,
            false,
            PresetValidity::MathematicalIllustration,
        ),
        (
            "formula.rotating-uniform-field",
            "Rotating uniform field",
            PresetGroup::Physics,
            true,
            PresetValidity::MathematicalIllustration,
        ),
        (
            "formula.3d-plane-wave",
            "3D plane wave",
            PresetGroup::Physics,
            true,
            PresetValidity::PhysicallyValid,
        ),
    ];

    for (id, name, group, animated, validity) in expected {
        let entry = library
            .get(&PresetId::new(id).unwrap())
            .unwrap_or_else(|| panic!("entry `{id}` must be retrievable"));
        assert_eq!(entry.name, name, "name for `{id}`");
        assert_eq!(entry.group, group, "group for `{id}`");
        assert_eq!(entry.filters.dimension, Some(3), "dimension for `{id}`");
        assert_eq!(entry.filters.animated, animated, "animated flag for `{id}`");
        assert_eq!(entry.validity, Some(validity), "validity label for `{id}`");
        assert!(!entry.summary.is_empty(), "summary for `{id}`");
        assert!(!entry.assumptions.is_empty(), "assumptions for `{id}`");
        assert!(
            !entry.exploration_prompt.is_empty(),
            "exploration prompt for `{id}`"
        );
    }
}

#[test]
fn validity_labels_are_explicit_and_stable() {
    assert_eq!(
        PresetValidity::MathematicalIllustration.label(),
        "Mathematical illustration"
    );
    assert_eq!(PresetValidity::PhysicallyValid.label(), "Physically valid");
    assert_eq!(
        PresetValidity::PhysicallyValidRegularized.label(),
        "Physically valid (regularized)"
    );
}

#[test]
fn teaching_source_entries_carry_stable_metadata() {
    let library = builtin();
    let expected = [
        "Unequal charges",
        "Off-axis ring",
        "Sphere inside and out",
        "Paired loops",
        "Helmholtz pair",
        "Opposing wires",
        "Solenoid ends",
    ];

    for name in expected {
        let entry = library
            .entries()
            .iter()
            .find(|entry| entry.name == name)
            .unwrap_or_else(|| panic!("source entry `{name}` must be indexed"));
        assert_eq!(entry.group, PresetGroup::Sources, "group for `{name}`");
        assert_eq!(
            entry.filters.dimension, None,
            "source dimension for `{name}`"
        );
        assert!(!entry.filters.animated, "source animation for `{name}`");
        assert_eq!(
            entry.validity,
            Some(PresetValidity::PhysicallyValid),
            "validity label for `{name}`"
        );
        assert!(!entry.summary.is_empty(), "summary for `{name}`");
        assert!(!entry.assumptions.is_empty(), "assumptions for `{name}`");
        assert!(
            !entry.exploration_prompt.is_empty(),
            "exploration prompt for `{name}`"
        );
    }
}

#[test]
fn combined_search_and_filter_matches_manual_intersection() {
    let library = builtin();
    let cases = [
        (
            "charged",
            PresetFilters {
                dimension: Some(3),
                animated: false,
            },
        ),
        (
            "source",
            PresetFilters {
                dimension: Some(2),
                animated: false,
            },
        ),
        ("", PresetFilters::default()),
        (
            "3d",
            PresetFilters {
                dimension: Some(2),
                animated: false,
            },
        ),
    ];

    for (query, filters) in cases {
        let combined = ids(&library.search_filtered(query, filters));
        let manual: Vec<String> = library
            .search(query)
            .into_iter()
            .filter(|entry| {
                let dimension_ok = match filters.dimension {
                    Some(dimension) => {
                        entry.filters.dimension.is_none()
                            || entry.filters.dimension == Some(dimension)
                    }
                    None => true,
                };
                let animated_ok = !filters.animated || entry.filters.animated;
                dimension_ok && animated_ok
            })
            .map(|entry| entry.id.as_str().to_owned())
            .collect();

        assert_eq!(combined, manual, "query {query:?}");
    }
}

#[test]
fn get_returns_the_entry_or_none() {
    let library = builtin();

    let vortex = PresetId::new("formula.vortex").unwrap();
    let entry = library.get(&vortex).expect("Vortex must be indexed");
    assert_eq!(entry.name, "Vortex");
    assert_eq!(entry.group, PresetGroup::Classic);

    let missing = PresetId::new("formula.missing").unwrap();
    assert!(library.get(&missing).is_none());
}

#[test]
fn duplicate_ids_are_rejected() {
    let library = builtin();
    let vortex = PresetId::new("formula.vortex").unwrap();
    let entry = library.get(&vortex).unwrap().clone();

    match PresetLibrary::from_entries(vec![entry.clone(), entry]) {
        Err(PresetLibraryError::DuplicateId(id)) => {
            assert_eq!(id.as_str(), "formula.vortex");
        }
        other => panic!("expected a duplicate-id error, got {other:?}"),
    }
}
