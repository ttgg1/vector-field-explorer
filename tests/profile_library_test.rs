//! Issue #85 slice A2a: the persisted user appearance-profile library.
//!
//! The library holds validated user profiles beside the nine shipped looks.
//! Duplicate names, invalid records and the user-profile cap are rejected;
//! imports are atomic and exports round-trip through the strict document.

use vector_field_explorer::appearance_profile::{
    AppearanceProfile, DEFAULT_PROFILE_NAME, PROFILE_FORMAT_VERSION, ProfileDocument, builtins,
};
use vector_field_explorer::colormap::Colormap;
use vector_field_explorer::profile_library::{MAX_USER_PROFILES, ProfileLibrary};

fn builtin(name: &str) -> AppearanceProfile {
    builtins()
        .into_iter()
        .find(|profile| profile.name == name)
        .expect("the named builtin exists")
}

fn look(index: usize) -> AppearanceProfile {
    let mut profile = builtin(DEFAULT_PROFILE_NAME);
    profile.name = format!("Look {index}");
    profile
}

#[test]
fn library_rejects_duplicate_and_invalid_profiles() {
    let error = ProfileLibrary::new(vec![builtin("Nord"), builtin("Nord")])
        .expect_err("same-named profiles are rejected");
    assert!(
        error.contains("duplicate profile name"),
        "the error names the duplicate: {error}"
    );
    assert!(
        error.contains("Nord"),
        "the error names the profile: {error}"
    );

    let mut invalid = builtin("Nord");
    invalid.name = "  padded".to_owned();
    let error = ProfileLibrary::new(vec![invalid]).expect_err("an invalid name is rejected");
    assert!(
        error.starts_with("invalid profile"),
        "validation uses the invalid-profile prefix: {error}"
    );

    let library = ProfileLibrary::new((0..MAX_USER_PROFILES).map(look).collect())
        .expect("a duplicate-free library at the cap is accepted");
    assert_eq!(library.len(), MAX_USER_PROFILES);
}

#[test]
fn library_upsert_replaces_in_place_and_preserves_order() {
    let mut library =
        ProfileLibrary::new(vec![builtin("Nord"), builtin("Solarized Light")]).expect("valid");
    let mut replaced = builtin("Nord");
    replaced.color.colormap = Colormap::Turbo;

    library
        .upsert(replaced.clone())
        .expect("a valid replacement is accepted");
    assert_eq!(
        library.user(),
        [replaced.clone(), builtin("Solarized Light")],
        "a same-named profile replaces in place"
    );
    assert_eq!(library.len(), 2);

    let appended = builtin("Tokyo Night");
    library
        .upsert(appended.clone())
        .expect("a new name appends");
    assert_eq!(library.user().last(), Some(&appended));

    let mut invalid = builtin("Nord");
    invalid.name = " ".to_owned();
    let error = library
        .upsert(invalid)
        .expect_err("an invalid profile is rejected");
    assert!(error.starts_with("invalid profile"), "{error}");
    assert_eq!(library.len(), 3, "a rejected upsert changes nothing");

    assert!(
        library.remove("Nord"),
        "removing a present name reports true"
    );
    assert_eq!(
        library.user().first(),
        Some(&builtin("Solarized Light")),
        "removal preserves the remaining order"
    );
    assert!(
        !library.remove("Nord"),
        "removing a missing name reports false"
    );
    assert_eq!(library.len(), 2);
    assert!(!library.is_empty());
}

#[test]
fn library_enforces_the_user_profile_cap() {
    let cap_message = format!("at most {MAX_USER_PROFILES} user profiles");

    let over_cap: Vec<AppearanceProfile> = (0..=MAX_USER_PROFILES).map(look).collect();
    let error = ProfileLibrary::new(over_cap).expect_err("one profile over the cap is rejected");
    assert!(error.contains(&cap_message), "{error}");

    let mut library = ProfileLibrary::new((0..MAX_USER_PROFILES).map(look).collect())
        .expect("exactly the cap is accepted");
    let error = library
        .upsert(look(MAX_USER_PROFILES))
        .expect_err("appending beyond the cap is rejected");
    assert!(error.contains(&cap_message), "{error}");
    assert_eq!(library.len(), MAX_USER_PROFILES);

    let mut replacement = look(0);
    replacement.color.colormap = Colormap::Mono;
    library
        .upsert(replacement)
        .expect("replacing an existing name does not grow the library");
    assert_eq!(library.len(), MAX_USER_PROFILES);
}

#[test]
fn library_import_is_atomic_on_invalid_or_duplicate_documents() {
    let mut library = ProfileLibrary::new(vec![builtin("Nord")]).expect("valid");
    let original = library.user().to_vec();

    let error = library
        .import_text("not json")
        .expect_err("malformed JSON is rejected");
    assert!(
        error.starts_with("invalid appearance profile JSON:"),
        "serde failures use the JSON prefix: {error}"
    );
    assert_eq!(library.user(), original);

    let duplicate = ProfileDocument {
        version: PROFILE_FORMAT_VERSION,
        profiles: vec![look(1), look(1)],
    };
    let text = serde_json::to_string(&duplicate).expect("the raw document serializes");
    let error = library
        .import_text(&text)
        .expect_err("duplicate names are rejected");
    assert!(error.contains("duplicate profile name"), "{error}");
    assert_eq!(library.user(), original);

    let excess: Vec<AppearanceProfile> = (0..=MAX_USER_PROFILES).map(look).collect();
    let excess = ProfileDocument {
        version: PROFILE_FORMAT_VERSION,
        profiles: excess,
    };
    let text = serde_json::to_string(&excess).expect("the raw document serializes");
    let error = library
        .import_text(&text)
        .expect_err("an over-cap document is rejected");
    assert!(error.contains("at most"), "{error}");
    assert_eq!(
        library.user(),
        original,
        "a rejected import leaves nothing behind"
    );
}

#[test]
fn library_import_merges_new_names_and_reports_the_user_count() {
    let mut library = ProfileLibrary::new(vec![builtin("Nord")]).expect("valid");
    let mut updated = builtin("Nord");
    updated.color.colormap = Colormap::Turbo;
    let added = builtin("Tokyo Night");
    let document = ProfileDocument {
        version: PROFILE_FORMAT_VERSION,
        profiles: vec![updated.clone(), added.clone()],
    };
    let text = document.to_json().expect("the document serializes");

    let count = library.import_text(&text).expect("the document merges");
    assert_eq!(
        count, 2,
        "the returned count is the user-profile count after the merge"
    );
    assert_eq!(library.user(), [updated, added]);
}

#[test]
fn library_export_round_trips_through_import() {
    let profiles = vec![look(1), look(2)];
    let library = ProfileLibrary::new(profiles.clone()).expect("valid");

    let text = library.export_text();
    let document = ProfileDocument::from_json(&text).expect("the export is a valid document");
    assert_eq!(document.version, PROFILE_FORMAT_VERSION);
    assert_eq!(document.profiles, profiles);

    let mut imported = ProfileLibrary::default();
    assert_eq!(
        imported.import_text(&text).expect("the export imports"),
        profiles.len()
    );
    assert_eq!(imported.user(), profiles.as_slice());
}

#[test]
fn library_all_lists_builtins_before_user_profiles() {
    let shipped = builtins();
    let users = vec![look(1), look(2)];
    let library = ProfileLibrary::new(users.clone()).expect("valid");

    let all = library.all();
    assert_eq!(all.len(), shipped.len() + users.len());
    assert_eq!(&all[..shipped.len()], shipped.as_slice());
    assert_eq!(&all[shipped.len()..], users.as_slice());

    let empty = ProfileLibrary::default();
    assert!(empty.is_empty());
    assert_eq!(empty.all(), shipped);
}
