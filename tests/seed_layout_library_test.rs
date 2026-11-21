//! Issue #87 slice S3a: the named seed-layout library, its strict versioned
//! document and its validation/sanitising rules.

use glam::Vec3;

use vector_field_explorer::domain::Domain;
use vector_field_explorer::plane::PlaneAxis;
use vector_field_explorer::seed_layout::{SeedAnchor, SeedLayout, SeedPoint};
use vector_field_explorer::seed_layout_library::{
    DEFAULT_LAYOUT_NAME, MAX_LAYOUT_NAME_BYTES, MAX_USER_LAYOUTS, SEED_LAYOUT_FORMAT_VERSION,
    SeedLayoutDocument, SeedLayoutLibrary, SeedLayoutProfile,
};

fn seed(x: f32, y: f32, z: f32) -> SeedPoint {
    SeedPoint {
        position: Vec3::new(x, y, z),
        anchor: None,
    }
}

fn anchored_seed() -> SeedPoint {
    SeedPoint {
        position: Vec3::new(0.25, -0.5, 0.0),
        anchor: Some(SeedAnchor {
            axis: PlaneAxis::Z,
            index: 3,
            u: 0.25,
            v: -0.5,
        }),
    }
}

fn profile(name: &str, seeds: Vec<SeedPoint>) -> SeedLayoutProfile {
    SeedLayoutProfile {
        name: name.to_owned(),
        seeds: SeedLayout { seeds },
    }
}

fn layout_named(name: &str) -> SeedLayoutProfile {
    profile(name, vec![seed(0.0, 0.0, 0.0)])
}

#[test]
fn layouts_round_trip_through_a_versioned_document() {
    let document = SeedLayoutDocument {
        version: SEED_LAYOUT_FORMAT_VERSION,
        layouts: vec![
            profile("Alpha", vec![seed(0.0, 0.0, 0.0), anchored_seed()]),
            profile("Beta", vec![seed(-0.5, 0.5, 0.0)]),
        ],
    };
    let json = document.to_json().expect("the document is valid");
    assert_eq!(
        json,
        concat!(
            r#"{"version":1,"layouts":["#,
            r#"{"name":"Alpha","seeds":{"seeds":["#,
            r#"{"position":[0.0,0.0,0.0],"anchor":null},"#,
            r#"{"position":[0.25,-0.5,0.0],"anchor":{"axis":"z","index":3,"u":0.25,"v":-0.5}}"#,
            r#"]}},"#,
            r#"{"name":"Beta","seeds":{"seeds":[{"position":[-0.5,0.5,0.0],"anchor":null}]}}"#,
            r#"]}"#
        ),
        "compact JSON pins the documented field order"
    );
    assert_eq!(
        SeedLayoutDocument::from_json(&json).expect("the round trip parses"),
        document,
        "a self-produced document round-trips"
    );
}

#[test]
fn documents_reject_unknown_fields_future_versions_and_duplicate_names() {
    let future = SeedLayoutDocument::from_json(r#"{"version":2,"layouts":[]}"#)
        .expect_err("a future version is rejected");
    assert!(
        future.starts_with("invalid seed layouts: "),
        "semantic prefix: {future}"
    );
    assert!(
        future.contains("version 2 is not supported"),
        "the version is reported first: {future}"
    );

    let unknown_root = SeedLayoutDocument::from_json(r#"{"version":1,"layouts":[],"extra":true}"#)
        .expect_err("an unknown root field is rejected");
    assert!(
        unknown_root.starts_with("invalid seed layout JSON: "),
        "serde prefix: {unknown_root}"
    );

    let unknown_layout = SeedLayoutDocument::from_json(
        r#"{"version":1,"layouts":[{"name":"A","seeds":{"seeds":[]},"extra":1}]}"#,
    )
    .expect_err("an unknown layout field is rejected");
    assert!(
        unknown_layout.starts_with("invalid seed layout JSON: "),
        "serde prefix: {unknown_layout}"
    );

    let duplicate = SeedLayoutDocument {
        version: SEED_LAYOUT_FORMAT_VERSION,
        layouts: vec![layout_named("Alpha"), layout_named("Alpha")],
    };
    let error = duplicate
        .to_json()
        .expect_err("case-sensitive duplicates are rejected");
    assert!(
        error.starts_with("invalid seed layouts: ") && error.contains("duplicate"),
        "duplicate error: {error}"
    );

    let case_variants = SeedLayoutDocument {
        version: SEED_LAYOUT_FORMAT_VERSION,
        layouts: vec![layout_named("Alpha"), layout_named("alpha")],
    };
    case_variants
        .to_json()
        .expect("names differing only by case are distinct");
}

#[test]
fn library_rejects_duplicates_the_builtin_name_and_the_cap() {
    let error = SeedLayoutLibrary::new(
        vec![layout_named("Alpha"), layout_named("Alpha")],
        &Domain::default(),
    )
    .expect_err("duplicate names are rejected");
    assert!(error.contains("duplicate"), "duplicate error: {error}");

    let error = SeedLayoutLibrary::new(vec![layout_named(DEFAULT_LAYOUT_NAME)], &Domain::default())
        .expect_err("the built-in name cannot be stored");
    assert!(
        error.contains(DEFAULT_LAYOUT_NAME),
        "built-in error: {error}"
    );

    let over_cap: Vec<_> = (0..=MAX_USER_LAYOUTS)
        .map(|index| layout_named(&format!("Layout {index}")))
        .collect();
    let error =
        SeedLayoutLibrary::new(over_cap, &Domain::default()).expect_err("the user cap is enforced");
    assert!(error.contains("at most"), "cap error: {error}");

    let mut library =
        SeedLayoutLibrary::new(vec![layout_named("Alpha")], &Domain::default()).unwrap();
    let error = library
        .upsert(layout_named(DEFAULT_LAYOUT_NAME), &Domain::default())
        .expect_err("the built-in layout cannot be overwritten");
    assert!(
        error.contains(DEFAULT_LAYOUT_NAME),
        "built-in upsert error: {error}"
    );
    assert_eq!(library.user(), &[layout_named("Alpha")]);

    let full: Vec<_> = (0..MAX_USER_LAYOUTS)
        .map(|index| layout_named(&format!("Full {index}")))
        .collect();
    let mut full = SeedLayoutLibrary::new(full, &Domain::default()).unwrap();
    let error = full
        .upsert(layout_named("One more"), &Domain::default())
        .expect_err("a new name past the cap is rejected");
    assert!(error.contains("at most"), "cap upsert error: {error}");
    assert_eq!(
        full.len(),
        MAX_USER_LAYOUTS,
        "a rejected upsert changes nothing"
    );

    assert!(!library.remove(DEFAULT_LAYOUT_NAME));
    assert!(!library.remove("missing"));
    assert_eq!(library.len(), 1, "failed removals change nothing");
}

#[test]
fn library_import_merges_atomically_and_reports_the_user_count() {
    let mut library =
        SeedLayoutLibrary::new(vec![layout_named("Alpha")], &Domain::default()).unwrap();
    let document = SeedLayoutDocument {
        version: SEED_LAYOUT_FORMAT_VERSION,
        layouts: vec![layout_named("Beta"), layout_named("Gamma")],
    };
    let count = library
        .import_text(&document.to_json().unwrap(), &Domain::default())
        .expect("the import is valid");
    assert_eq!(count, 3, "the post-merge user count is reported");
    assert_eq!(
        library
            .user()
            .iter()
            .map(|layout| layout.name.as_str())
            .collect::<Vec<_>>(),
        ["Alpha", "Beta", "Gamma"],
        "imports append in document order"
    );

    let replacement = SeedLayoutDocument {
        version: SEED_LAYOUT_FORMAT_VERSION,
        layouts: vec![profile("Alpha", vec![seed(0.5, 0.5, 0.0)])],
    };
    let count = library
        .import_text(&replacement.to_json().unwrap(), &Domain::default())
        .expect("an import can replace an existing layout");
    assert_eq!(count, 3, "replacing does not grow the user count");
    assert_eq!(
        library.user()[0].seeds.seeds,
        vec![seed(0.5, 0.5, 0.0)],
        "the same-named layout is replaced in place"
    );

    let before = library.clone();
    let invalid = SeedLayoutDocument {
        version: SEED_LAYOUT_FORMAT_VERSION,
        layouts: vec![
            layout_named("Valid"),
            profile("Broken", vec![seed(9.0, 0.0, 0.0)]),
        ],
    };
    let error = library
        .import_text(&invalid.to_json().unwrap(), &Domain::default())
        .expect_err("an out-of-domain seed rejects the import");
    assert!(error.contains("outside the scene domain"), "error: {error}");
    assert_eq!(
        library, before,
        "a failed import leaves the library untouched"
    );

    let mut over_cap: Vec<_> = (0..MAX_USER_LAYOUTS)
        .map(|index| layout_named(&format!("Full {index}")))
        .collect();
    over_cap.push(layout_named("Extra"));
    let mut full =
        SeedLayoutLibrary::new(over_cap[..MAX_USER_LAYOUTS].to_vec(), &Domain::default()).unwrap();
    let before = full.clone();
    let document = SeedLayoutDocument {
        version: SEED_LAYOUT_FORMAT_VERSION,
        layouts: vec![layout_named("Extra")],
    };
    let error = full
        .import_text(&document.to_json().unwrap(), &Domain::default())
        .expect_err("an over-cap merge is rejected");
    assert!(error.contains("at most"), "over-cap error: {error}");
    assert_eq!(full, before, "the over-cap import changes nothing");
}

#[test]
fn library_export_is_user_only_compact_and_empty_when_unset() {
    let empty = SeedLayoutLibrary::default();
    assert_eq!(empty.export_text(), "", "an empty library exports nothing");

    let library = SeedLayoutLibrary::new(
        vec![
            layout_named("Alpha"),
            profile("Beta", vec![anchored_seed()]),
        ],
        &Domain::default(),
    )
    .unwrap();
    let exported = library.export_text();
    assert!(
        !exported.contains(DEFAULT_LAYOUT_NAME),
        "the built-in is never exported: {exported}"
    );
    assert!(
        !exported.contains(char::is_whitespace),
        "the export is compact JSON: {exported}"
    );
    let document = SeedLayoutDocument::from_json(&exported).expect("the export parses");
    assert_eq!(document.version, SEED_LAYOUT_FORMAT_VERSION);
    assert_eq!(document.layouts, library.user());
}

#[test]
fn built_in_default_is_always_first_and_never_stored() {
    let library = SeedLayoutLibrary::new(vec![layout_named("Alpha")], &Domain::default()).unwrap();
    let all = library.all(&Domain::default());
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].name, DEFAULT_LAYOUT_NAME);
    assert!(all[0].seeds.is_empty(), "the built-in is the empty layout");
    assert_eq!(all[0].seeds, SeedLayout::default());
    assert_eq!(
        library
            .user()
            .iter()
            .map(|layout| layout.name.as_str())
            .collect::<Vec<_>>(),
        ["Alpha"],
        "the built-in is not in the user slice"
    );
    assert_eq!(library.len(), 1);
    assert!(!library.is_empty());
    assert!(!library.export_text().contains(DEFAULT_LAYOUT_NAME));
}

#[test]
fn sanitized_repairs_names_and_drops_invalid_seeds_total() {
    let domain = Domain::default();
    let valid = seed(0.0, 0.0, 0.0);
    let repaired = profile(
        "  Spaced  ",
        vec![
            valid,
            seed(f32::NAN, 0.0, 0.0),
            seed(5.0, 0.0, 0.0),
            SeedPoint {
                position: Vec3::ZERO,
                anchor: Some(SeedAnchor {
                    axis: PlaneAxis::X,
                    index: 0,
                    u: f32::INFINITY,
                    v: 0.0,
                }),
            },
        ],
    )
    .sanitized(&domain);
    assert_eq!(repaired.name, "Spaced");
    assert_eq!(repaired.seeds.seeds, vec![valid], "invalid seeds drop");
    repaired
        .validate(&domain)
        .expect("the sanitised record validates");

    let too_long = "é".repeat(MAX_LAYOUT_NAME_BYTES);
    let repaired = profile(&too_long, vec![]).sanitized(&domain);
    assert!(repaired.name.len() <= MAX_LAYOUT_NAME_BYTES);
    assert!(
        repaired.name.len() <= MAX_LAYOUT_NAME_BYTES && repaired.name.chars().all(|c| c == 'é'),
        "truncation stops on a char boundary: {:?}",
        repaired.name
    );
    repaired
        .validate(&domain)
        .expect("the truncated name validates");

    for unusable in ["", "   ", DEFAULT_LAYOUT_NAME, "  Default  "] {
        let repaired = profile(unusable, vec![]).sanitized(&domain);
        assert_eq!(
            repaired.name, "Layout",
            "an unusable name resets to the stable placeholder"
        );
        repaired
            .validate(&domain)
            .expect("the placeholder validates");
    }

    let repaired = profile("Trailing  ", vec![valid]).sanitized(&domain);
    assert_eq!(repaired.name, "Trailing");
    repaired
        .validate(&domain)
        .expect("the trimmed name validates");
}

#[test]
fn saving_a_layout_replaces_in_place_and_preserves_order() {
    let mut library = SeedLayoutLibrary::new(
        vec![
            layout_named("Alpha"),
            layout_named("Beta"),
            layout_named("Gamma"),
        ],
        &Domain::default(),
    )
    .unwrap();
    let replacement = profile("Beta", vec![seed(0.5, 0.5, 0.0)]);
    library
        .upsert(replacement.clone(), &Domain::default())
        .expect("replacing an existing name is allowed");
    assert_eq!(
        library
            .user()
            .iter()
            .map(|layout| layout.name.as_str())
            .collect::<Vec<_>>(),
        ["Alpha", "Beta", "Gamma"],
        "replacing preserves the library order"
    );
    assert_eq!(library.user()[1], replacement);
    assert_eq!(library.len(), 3, "replacing never grows the library");

    let alpha = profile("Alpha", vec![seed(-0.5, 0.5, 0.0)]);
    library
        .upsert(alpha.clone(), &Domain::default())
        .expect("replacing the first entry keeps its position");
    assert_eq!(library.user()[0], alpha);
    assert_eq!(library.user()[1], replacement);
}
