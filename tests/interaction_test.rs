//! Issue #88 slice A1: the pure interaction-profile model.
//!
//! These tests pin the documented default chords, conflict handling, reserved
//! and browser-owned chords, camera-feel sanitization, and the strict portable
//! profile document and bounded library.

use eframe::egui::Key;
use vector_field_explorer::interaction::{
    BindError, Bindings, CameraFeel, ChordModifiers, INTERACTION_FORMAT_VERSION, InteractionAction,
    InteractionProfile, KeyChord, MAX_BINDINGS, MAX_PROFILE_NAME_BYTES, MAX_USER_PROFILES,
    ProfileDocument, ProfileLibrary,
};

fn chord(key: Key, command: bool, shift: bool, alt: bool) -> KeyChord {
    KeyChord {
        key,
        modifiers: ChordModifiers {
            command,
            shift,
            alt,
        },
    }
}

fn plain(key: Key) -> KeyChord {
    chord(key, false, false, false)
}

fn command(key: Key) -> KeyChord {
    chord(key, true, false, false)
}

fn profile_named(name: &str) -> InteractionProfile {
    InteractionProfile {
        name: name.to_owned(),
        bindings: Bindings::default_bindings(),
        camera_feel: CameraFeel::default(),
    }
}

fn documented_default(action: InteractionAction) -> KeyChord {
    match action {
        InteractionAction::ToggleControls => command(Key::B),
        InteractionAction::TogglePresets => command(Key::P),
        InteractionAction::ToggleAppearance => chord(Key::A, true, true, false),
        InteractionAction::ToggleInteraction => chord(Key::L, true, true, false),
        InteractionAction::ToggleHelp => plain(Key::F1),
        InteractionAction::ResetView => plain(Key::R),
        InteractionAction::FitView => plain(Key::F),
        InteractionAction::ZoomIn => plain(Key::Plus),
        InteractionAction::ZoomOut => plain(Key::Minus),
        InteractionAction::ViewLeft => plain(Key::ArrowLeft),
        InteractionAction::ViewRight => plain(Key::ArrowRight),
        InteractionAction::ViewUp => plain(Key::ArrowUp),
        InteractionAction::ViewDown => plain(Key::ArrowDown),
        InteractionAction::PlayPause => plain(Key::Space),
        InteractionAction::SeedStreamline => plain(Key::S),
        InteractionAction::AddParticles => plain(Key::P),
        InteractionAction::Screenshot => chord(Key::S, true, true, false),
        InteractionAction::CopyFormula => chord(Key::C, true, true, false),
        InteractionAction::Quit => command(Key::Q),
        InteractionAction::Dismiss | InteractionAction::Undo | InteractionAction::Redo => {
            panic!("reserved actions have no default chord")
        }
    }
}

#[test]
fn defaults_match_the_documented_bindings() {
    assert_eq!(InteractionAction::ALL.len(), 22);
    assert_eq!(InteractionAction::REMAPPABLE.len(), 19);
    assert_eq!(MAX_BINDINGS, 19);

    let bindings = Bindings::default_bindings();
    let mut seen = std::collections::HashSet::new();
    for action in InteractionAction::REMAPPABLE {
        assert!(!action.is_reserved(), "{action:?} must be remappable");
        let expected = documented_default(action);
        assert_eq!(
            bindings.chord(action),
            Some(expected),
            "chord for {action:?}"
        );
        assert!(seen.insert(expected), "duplicate default {expected:?}");
        assert_eq!(bindings.owner_of(expected), Some(action));
        assert_eq!(action.is_global(), action == InteractionAction::Quit);
    }
    assert!(InteractionAction::Quit.is_global());

    for action in InteractionAction::ALL {
        if action.is_reserved() {
            assert!(!InteractionAction::REMAPPABLE.contains(&action));
            assert_eq!(bindings.chord(action), None);
        }
    }
    assert!(InteractionAction::Dismiss.is_reserved());
    assert!(InteractionAction::Undo.is_reserved());
    assert!(InteractionAction::Redo.is_reserved());
    assert!(bindings.validate().is_ok());
}

#[test]
fn reset_action_restores_the_documented_default() {
    let mut bindings = Bindings::default_bindings();
    let rebound = plain(Key::J);
    bindings
        .assign(InteractionAction::ToggleControls, rebound)
        .expect("J is a free chord");
    assert_eq!(
        bindings.chord(InteractionAction::ToggleControls),
        Some(rebound)
    );

    bindings.reset_action(InteractionAction::ToggleControls);

    // The target returns to the documented default from the table, not to
    // whatever the assign path might remember.
    assert_eq!(
        bindings.chord(InteractionAction::ToggleControls),
        Some(documented_default(InteractionAction::ToggleControls))
    );
    // No other remappable action moved.
    for action in InteractionAction::REMAPPABLE {
        if action == InteractionAction::ToggleControls {
            continue;
        }
        assert_eq!(
            bindings.chord(action),
            Some(documented_default(action)),
            "reset_action({action:?}) changed the chord for {action:?}"
        );
    }
    // Reserved actions stay unbound.
    for action in [
        InteractionAction::Dismiss,
        InteractionAction::Undo,
        InteractionAction::Redo,
    ] {
        assert_eq!(bindings.chord(action), None);
    }
    assert!(bindings.validate().is_ok());
}

#[test]
fn reset_action_is_a_no_op_for_reserved_actions() {
    let mut bindings = Bindings::default_bindings();
    // A rebind elsewhere proves the whole table is compared, not just one row.
    bindings
        .assign(InteractionAction::FitView, plain(Key::J))
        .expect("J is a free chord");
    let before = bindings.clone();

    for action in [
        InteractionAction::Dismiss,
        InteractionAction::Undo,
        InteractionAction::Redo,
    ] {
        assert!(action.is_reserved());
        bindings.reset_action(action);
        assert_eq!(bindings.chord(action), None, "{action:?} must stay unbound");
        assert_eq!(
            bindings, before,
            "reset_action({action:?}) changed the table"
        );
    }
    // The unrelated rebind survives untouched.
    assert_eq!(
        bindings.chord(InteractionAction::FitView),
        Some(plain(Key::J))
    );
}

#[test]
fn reset_all_restores_default_bindings() {
    let mut bindings = Bindings::default_bindings();
    for (action, rebound) in [
        (InteractionAction::ToggleControls, plain(Key::J)),
        (InteractionAction::ViewLeft, plain(Key::K)),
        (InteractionAction::ToggleHelp, plain(Key::X)),
    ] {
        bindings
            .assign(action, rebound)
            .unwrap_or_else(|error| panic!("{action:?} -> {rebound:?}: {error:?}"));
    }
    assert!(bindings.validate().is_ok());

    bindings.reset_all();

    for action in InteractionAction::REMAPPABLE {
        assert_eq!(
            bindings.chord(action),
            Some(documented_default(action)),
            "reset_all did not restore {action:?}"
        );
        assert!(
            bindings.chord(action).is_some(),
            "{action:?} is left unbound"
        );
    }
    for action in [
        InteractionAction::Dismiss,
        InteractionAction::Undo,
        InteractionAction::Redo,
    ] {
        assert_eq!(bindings.chord(action), None);
    }
    assert!(bindings.validate().is_ok());
    assert_eq!(bindings, Bindings::default_bindings());
}

#[test]
fn assign_rejects_a_reserved_action_or_chord() {
    let mut bindings = Bindings::default_bindings();
    let before = bindings.clone();

    for action in [
        InteractionAction::Dismiss,
        InteractionAction::Undo,
        InteractionAction::Redo,
    ] {
        assert_eq!(
            bindings.assign(action, plain(Key::X)),
            Err(BindError::Reserved)
        );
    }
    assert_eq!(
        bindings.assign(InteractionAction::ToggleControls, plain(Key::Escape)),
        Err(BindError::Reserved)
    );
    assert_eq!(
        bindings.assign(InteractionAction::ToggleControls, command(Key::Z)),
        Err(BindError::Reserved)
    );
    assert_eq!(
        bindings.assign(
            InteractionAction::ToggleControls,
            chord(Key::Z, true, true, false)
        ),
        Err(BindError::Reserved)
    );

    assert_eq!(bindings, before);
}

#[test]
fn assign_reports_the_conflicting_owner_and_changes_nothing() {
    let mut bindings = Bindings::default_bindings();
    let before = bindings.clone();

    let taken = documented_default(InteractionAction::TogglePresets);
    assert_eq!(
        bindings.assign(InteractionAction::ToggleControls, taken),
        Err(BindError::Conflict(InteractionAction::TogglePresets))
    );
    assert_eq!(bindings, before);

    // Re-assigning an action its own chord is a no-op, not a conflict.
    assert_eq!(
        bindings.assign(
            InteractionAction::ToggleControls,
            documented_default(InteractionAction::ToggleControls)
        ),
        Ok(())
    );
    assert_eq!(bindings, before);
}

#[test]
fn replace_swaps_chords_so_no_duplicate_exists() {
    let mut bindings = Bindings::default_bindings();

    let controls = documented_default(InteractionAction::ToggleControls);
    let presets = documented_default(InteractionAction::TogglePresets);
    bindings
        .replace(InteractionAction::ToggleControls, presets)
        .expect("replace with a free or owned chord succeeds");

    assert_eq!(
        bindings.chord(InteractionAction::ToggleControls),
        Some(presets)
    );
    assert_eq!(
        bindings.chord(InteractionAction::TogglePresets),
        Some(controls)
    );
    assert_eq!(
        bindings.owner_of(presets),
        Some(InteractionAction::ToggleControls)
    );
    assert_eq!(
        bindings.owner_of(controls),
        Some(InteractionAction::TogglePresets)
    );
    assert!(bindings.validate().is_ok());

    // Replacing with a genuinely free chord just assigns it.
    let free = plain(Key::J);
    bindings
        .replace(InteractionAction::ViewLeft, free)
        .expect("a free chord is assigned");
    assert_eq!(bindings.chord(InteractionAction::ViewLeft), Some(free));
    assert!(bindings.validate().is_ok());
}

#[test]
fn replace_refuses_a_swap_that_would_move_a_grandfathered_chord() {
    let mut bindings = Bindings::default_bindings();
    let before = bindings.clone();
    let add_particles = documented_default(InteractionAction::AddParticles);

    // Swapping would move Screenshot's grandfathered Mod+Shift+S onto
    // AddParticles, leaving an invalid table that a restart would repair by
    // silently dropping the bind. Refuse the whole move instead.
    assert_eq!(
        bindings.replace(InteractionAction::Screenshot, add_particles),
        Err(BindError::BrowserReserved),
        "the displaced chord must also be valid for its new owner"
    );
    assert_eq!(bindings, before, "a refused swap changes nothing");
    assert!(bindings.validate().is_ok());
    assert_eq!(
        bindings.chord(InteractionAction::Screenshot),
        Some(chord(Key::S, true, true, false))
    );
    assert_eq!(
        bindings.chord(InteractionAction::AddParticles),
        Some(add_particles)
    );
}

#[test]
fn browser_reserved_chords_are_refused_with_a_reason() {
    let browser = [
        command(Key::W),
        command(Key::T),
        command(Key::N),
        chord(Key::W, true, true, false),
        chord(Key::I, true, true, false),
        chord(Key::J, true, true, false),
        chord(Key::K, true, true, false),
        chord(Key::C, true, true, false),
        command(Key::Q),
    ];
    for reserved in browser {
        assert!(reserved.is_browser_reserved(), "{reserved:?}");
    }
    assert!(!chord(Key::W, true, false, true).is_browser_reserved());
    assert!(!plain(Key::W).is_browser_reserved());
    // The shipped screenshot default is grandfathered, not browser-owned.
    assert!(!chord(Key::S, true, true, false).is_browser_reserved());

    let mut bindings = Bindings::default_bindings();
    let before = bindings.clone();
    for reserved in browser {
        assert_eq!(
            bindings.assign(InteractionAction::ToggleHelp, reserved),
            Err(BindError::BrowserReserved)
        );
        assert_eq!(
            bindings.replace(InteractionAction::ToggleHelp, reserved),
            Err(BindError::BrowserReserved)
        );
    }

    // The grandfathered defaults stay bindable to their original actions and
    // are refused for every other action.
    for (action, shipped) in [
        (InteractionAction::Quit, command(Key::Q)),
        (
            InteractionAction::CopyFormula,
            chord(Key::C, true, true, false),
        ),
        (
            InteractionAction::Screenshot,
            chord(Key::S, true, true, false),
        ),
    ] {
        assert_eq!(bindings.assign(action, shipped), Ok(()));
        assert_eq!(
            bindings.assign(InteractionAction::ToggleHelp, shipped),
            Err(BindError::BrowserReserved)
        );
    }
    assert_eq!(bindings, before);
    assert!(bindings.validate().is_ok());
}

#[test]
fn camera_feel_clamps_and_sanitizes() {
    let default = CameraFeel::default();
    assert!((default.orbit_sensitivity - 1.0).abs() < f32::EPSILON);
    assert!((default.zoom_speed - 1.0).abs() < f32::EPSILON);
    assert!(!default.invert_yaw && !default.invert_pitch && !default.invert_zoom);
    assert!(default.validate().is_ok());

    let extreme = CameraFeel {
        orbit_sensitivity: 0.01,
        zoom_speed: 99.0,
        invert_yaw: true,
        invert_pitch: false,
        invert_zoom: true,
    };
    assert!(extreme.validate().is_err());
    let repaired = extreme.sanitized();
    assert_eq!(repaired.orbit_sensitivity, 0.25);
    assert_eq!(repaired.zoom_speed, 4.0);
    assert!(repaired.validate().is_ok());
    assert!(repaired.invert_yaw && !repaired.invert_pitch && repaired.invert_zoom);

    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let feel = CameraFeel {
            orbit_sensitivity: bad,
            zoom_speed: bad,
            ..CameraFeel::default()
        };
        assert!(feel.validate().is_err());
        let repaired = feel.sanitized();
        assert_eq!(repaired.orbit_sensitivity, 1.0);
        assert_eq!(repaired.zoom_speed, 1.0);
        assert!(repaired.validate().is_ok());
    }
}

#[test]
fn sanitized_profiles_always_validate() {
    let long_name = format!("  bad\u{7}name {}", "x".repeat(MAX_PROFILE_NAME_BYTES * 2));
    let long_name_json = serde_json::to_string(&long_name).expect("name encodes as JSON");
    let json = format!(
        r#"{{
            "name": {name},
            "bindings": {{
                "toggle_controls": {{"key": "Escape", "modifiers": {{"command": false, "shift": false, "alt": false}}}},
                "toggle_presets": {{"key": "B", "modifiers": {{"command": true, "shift": false, "alt": false}}}},
                "toggle_appearance": {{"key": "B", "modifiers": {{"command": true, "shift": false, "alt": false}}}},
                "toggle_help": {{"key": "Q", "modifiers": {{"command": true, "shift": false, "alt": false}}}},
                "reset_view": {{"key": "W", "modifiers": {{"command": true, "shift": false, "alt": false}}}}
            }},
            "camera_feel": {{
                "orbit_sensitivity": 0.01,
                "zoom_speed": 99.0,
                "invert_yaw": true,
                "invert_pitch": false,
                "invert_zoom": false
            }}
        }}"#,
        name = long_name_json,
    );
    let broken: InteractionProfile =
        serde_json::from_str(&json).expect("invalid-but-parseable profiles must reach sanitized");
    assert!(broken.validate().is_err());

    let repaired = broken.sanitized();
    assert!(repaired.validate().is_ok());
    assert!(repaired.name.len() <= MAX_PROFILE_NAME_BYTES);
    assert_eq!(repaired.name, repaired.name.trim());
    assert!(!repaired.name.chars().any(char::is_control));
    assert_eq!(repaired.camera_feel.orbit_sensitivity, 0.25);
    assert_eq!(repaired.camera_feel.zoom_speed, 4.0);
    // Reserved/browser-reserved chords and missing entries return to defaults.
    assert_eq!(
        repaired.bindings.chord(InteractionAction::ToggleControls),
        Some(command(Key::B))
    );
    assert_eq!(
        repaired.bindings.chord(InteractionAction::TogglePresets),
        Some(command(Key::P))
    );
    assert_eq!(
        repaired.bindings.chord(InteractionAction::ResetView),
        Some(plain(Key::R))
    );
    assert_eq!(
        repaired.bindings.chord(InteractionAction::ToggleHelp),
        Some(plain(Key::F1))
    );
    assert!(repaired.bindings.validate().is_ok());

    let empty_name: InteractionProfile = serde_json::from_str(r#"{"name": "   "}"#)
        .expect("a defaulted bindings/camera-feel record parses");
    let repaired = empty_name.sanitized();
    assert!(repaired.validate().is_ok());
    assert!(!repaired.name.is_empty());
}

#[test]
fn profile_document_rejects_newer_versions_and_unknown_fields() {
    // The version is probed before the profile bodies are parsed.
    let future = r#"{"version": 2, "profiles": [{"not": "a profile"}]}"#;
    let error = ProfileDocument::from_json(future).expect_err("future version");
    assert!(error.contains("version 2"), "{error}");

    let older = r#"{"version": 0, "profiles": []}"#;
    assert!(ProfileDocument::from_json(older).is_err());

    let unknown_document =
        format!(r#"{{"version": {INTERACTION_FORMAT_VERSION}, "profiles": [], "extra": 1}}"#);
    assert!(ProfileDocument::from_json(&unknown_document).is_err());

    let unknown_profile = format!(
        r#"{{"version": {INTERACTION_FORMAT_VERSION}, "profiles": [{{"name": "x", "extra": 1}}]}}"#
    );
    assert!(ProfileDocument::from_json(&unknown_profile).is_err());

    let unknown_binding = format!(
        r#"{{"version": {INTERACTION_FORMAT_VERSION}, "profiles": [{{"name": "x", "bindings": {{"not_an_action": {{"key": "B", "modifiers": {{"command": true, "shift": false, "alt": false}}}}}}}}]}}"#
    );
    assert!(ProfileDocument::from_json(&unknown_binding).is_err());

    let duplicate = format!(
        r#"{{"version": {INTERACTION_FORMAT_VERSION}, "profiles": [{{"name": "x"}}, {{"name": "x"}}]}}"#
    );
    let error = ProfileDocument::from_json(&duplicate).expect_err("duplicate names");
    assert!(error.contains("duplicate"), "{error}");
}

#[test]
fn library_rejects_duplicates_and_the_cap() {
    let duplicate = vec![profile_named("Same"), profile_named("Same")];
    assert!(ProfileLibrary::new(duplicate).is_err());

    let over_cap: Vec<_> = (0..=MAX_USER_PROFILES)
        .map(|index| profile_named(&format!("Profile {index}")))
        .collect();
    assert_eq!(over_cap.len(), MAX_USER_PROFILES + 1);
    let error = ProfileLibrary::new(over_cap).expect_err("over the cap");
    assert!(error.contains(&MAX_USER_PROFILES.to_string()), "{error}");

    let at_cap: Vec<_> = (0..MAX_USER_PROFILES)
        .map(|index| profile_named(&format!("Profile {index}")))
        .collect();
    let mut library = ProfileLibrary::new(at_cap).expect("exactly at the cap");
    assert_eq!(library.len(), MAX_USER_PROFILES);
    assert!(library.upsert(profile_named("One more")).is_err());
    assert_eq!(library.len(), MAX_USER_PROFILES);

    // The built-in Default cannot be overwritten or removed.
    assert!(library.upsert(profile_named("Default")).is_err());
    assert!(!library.remove("Default"));
    assert!(ProfileLibrary::new(vec![profile_named("Default")]).is_err());
    assert_eq!(library.all()[0].name, "Default");
}

#[test]
fn library_import_is_atomic() {
    let mut library = ProfileLibrary::new(vec![profile_named("Keep")]).expect("valid library");
    let before = library.clone();

    let bad_document = ProfileDocument {
        version: INTERACTION_FORMAT_VERSION,
        profiles: vec![profile_named("New"), profile_named("Default")],
    };
    let text = bad_document
        .to_json()
        .expect("document is structurally valid");
    assert!(library.import_text(&text).is_err());
    assert_eq!(library, before);

    let duplicate_document = format!(
        r#"{{"version": {INTERACTION_FORMAT_VERSION}, "profiles": [{{"name": "New"}}, {{"name": "New"}}]}}"#
    );
    assert!(library.import_text(&duplicate_document).is_err());
    assert_eq!(library, before);

    assert!(library.import_text("not json").is_err());
    assert_eq!(library, before);

    let good = ProfileDocument {
        version: INTERACTION_FORMAT_VERSION,
        profiles: vec![profile_named("New"), profile_named("Keep")],
    };
    let text = good.to_json().expect("valid document");
    assert_eq!(library.import_text(&text).expect("atomic merge"), 2);
    let names: Vec<_> = library
        .user()
        .iter()
        .map(|profile| profile.name.as_str())
        .collect();
    assert_eq!(names, vec!["Keep", "New"]);
}

#[test]
fn library_export_excludes_builtins_and_handles_empty() {
    let empty = ProfileLibrary::new(Vec::new()).expect("empty library");
    assert_eq!(empty.export_text(), "");

    let library =
        ProfileLibrary::new(vec![profile_named("Alpha"), profile_named("Beta")]).expect("valid");
    let text = library.export_text();
    assert!(!text.is_empty());
    let document = ProfileDocument::from_json(&text).expect("export round trips");
    assert_eq!(document.version, INTERACTION_FORMAT_VERSION);
    let names: Vec<_> = document
        .profiles
        .iter()
        .map(|profile| profile.name.as_str())
        .collect();
    assert_eq!(names, vec!["Alpha", "Beta"]);
    assert!(!text.contains("\"Default\""));
}

#[test]
fn profile_and_document_json_round_trip_is_stable() {
    let mut bindings = Bindings::default_bindings();
    bindings
        .assign(InteractionAction::ViewLeft, plain(Key::J))
        .expect("J is free");
    let profile = InteractionProfile {
        name: "Custom".to_owned(),
        bindings,
        camera_feel: CameraFeel {
            orbit_sensitivity: 1.5,
            zoom_speed: 0.5,
            invert_yaw: true,
            invert_pitch: false,
            invert_zoom: true,
        },
    };
    let document = ProfileDocument {
        version: INTERACTION_FORMAT_VERSION,
        profiles: vec![profile.clone()],
    };

    let text = document.to_json().expect("valid document");
    assert!(!text.contains('\n'));
    assert!(
        text.starts_with(r#"{"version":1,"profiles":[{"name":"Custom","bindings":"#),
        "{text}"
    );
    assert!(text.contains(r#""camera_feel":{"#), "{text}");
    assert!(text.contains(r#""key":"J""#), "{text}");

    let parsed = ProfileDocument::from_json(&text).expect("round trip");
    assert_eq!(parsed, document);
    assert_eq!(parsed.to_json().expect("stable output"), text);
    assert_eq!(parsed.profiles[0], profile);

    // Special keys survive the string encoding.
    let special = InteractionProfile {
        name: "Keys".to_owned(),
        bindings: Bindings::default_bindings(),
        camera_feel: CameraFeel::default(),
    };
    let special_text = ProfileDocument {
        version: INTERACTION_FORMAT_VERSION,
        profiles: vec![special.clone()],
    }
    .to_json()
    .expect("valid document");
    let back = ProfileDocument::from_json(&special_text).expect("round trip");
    assert_eq!(back.profiles[0], special);
}
