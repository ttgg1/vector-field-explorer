//! Preset library coverage (ticket 90, slice A).
//!
//! The catalogue is a pure index over the existing formula and source
//! presets: stable IDs, deterministic order, group/filter metadata, and
//! search. These tests pin the contract without touching the app.

use std::collections::HashSet;

use vector_field_explorer::preset_library::{
    PresetEntry, PresetFilters, PresetGroup, PresetId, PresetLibrary, PresetLibraryError,
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
        assert!(
            !entry.filters.animated,
            "no formula preset is animated: `{}`",
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
fn animated_filter_is_honest_for_an_all_static_catalogue() {
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
    assert!(
        animated_only.is_empty(),
        "no built-in preset is animated yet"
    );
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
