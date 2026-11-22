//! Issue #90 slice B2b: the saved-scene store.
//!
//! Pins insertion order, duplicate/cap/validation rejection, the versioned
//! compact JSON envelope, atomic import/export and the native/wasm storage
//! seams. No UI or app wiring is exercised here.

use vector_field_explorer::parameters::ParameterSet;
use vector_field_explorer::persistence::ViewRecord;
use vector_field_explorer::scene_document::{SceneCamera, SceneDocument};
use vector_field_explorer::scene_store::{
    MAX_SCENES, SCENE_STORE_VERSION, SceneStore, export_store, import_store,
};

#[cfg(not(target_arch = "wasm32"))]
use std::fs;
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};
#[cfg(not(target_arch = "wasm32"))]
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(not(target_arch = "wasm32"))]
use vector_field_explorer::scene_store::{SCENES_FILE, load_dir, save_dir};

fn scene_named(name: &str) -> SceneDocument {
    SceneDocument::capture(
        name,
        ViewRecord::default(),
        ParameterSet::default(),
        SceneCamera::default(),
    )
}

fn scene_with_scale(name: &str, scale: f32) -> SceneDocument {
    let camera = SceneCamera {
        scale,
        ..SceneCamera::default()
    };
    SceneDocument::capture(name, ViewRecord::default(), ParameterSet::default(), camera)
}

fn store_of(names: &[&str]) -> SceneStore {
    SceneStore::from_documents(names.iter().map(|name| scene_named(name)).collect())
        .expect("valid, unique scenes fit in an empty store")
}

fn names(store: &SceneStore) -> Vec<&str> {
    store
        .scenes()
        .iter()
        .map(|scene| scene.name.as_str())
        .collect()
}

fn envelope(scenes: &[SceneDocument]) -> String {
    let documents = scenes
        .iter()
        .map(|scene| scene.to_json().expect("test fixture scene serializes"))
        .collect::<Vec<_>>()
        .join(",");
    format!("{{\"version\":{SCENE_STORE_VERSION},\"scenes\":[{documents}]}}")
}

#[cfg(not(target_arch = "wasm32"))]
struct TempDir {
    path: PathBuf,
}

#[cfg(not(target_arch = "wasm32"))]
impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("test clock is after the Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "vfe-scene-store-{}-{tag}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("temporary scene directory is created");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn envelope_round_trips_through_json() {
    let mut first = scene_named("alpha");
    first.camera.scale = 2.5;
    let store = SceneStore::from_documents(vec![first.clone(), scene_named("beta")])
        .expect("unique valid scenes are accepted");

    let json = store.to_json().expect("a valid store serializes");
    let restored = SceneStore::from_json(&json).expect("a valid store parses");

    assert_eq!(restored, store);
    assert_eq!(names(&restored), vec!["alpha", "beta"]);
    assert_eq!(restored.get("alpha"), Some(&first));
    assert!(!json.is_empty());
}

#[test]
fn duplicate_scene_names_are_rejected() {
    let error = SceneStore::from_documents(vec![scene_named("same"), scene_named("same")])
        .expect_err("duplicate names are rejected");
    assert!(error.contains("same"), "error names the scene: {error}");

    let payload = envelope(&[scene_named("same"), scene_named("same")]);
    let error = SceneStore::from_json(&payload).expect_err("duplicates in JSON are rejected");
    assert!(error.contains("same"), "error names the scene: {error}");

    let payload = envelope(&[scene_named("Case"), scene_named("case")]);
    SceneStore::from_json(&payload).expect("names are compared exactly, case-sensitively");
}

#[test]
fn the_store_caps_at_sixty_four_scenes() {
    let documents = (0..MAX_SCENES)
        .map(|index| scene_named(&format!("scene-{index}")))
        .collect::<Vec<_>>();
    let mut store = SceneStore::from_documents(documents).expect("64 scenes are accepted");
    assert_eq!(store.scenes().len(), MAX_SCENES);

    let error = store
        .upsert(scene_named("one-more"))
        .expect_err("a 65th scene is rejected");
    assert!(
        error.contains(&MAX_SCENES.to_string()),
        "error names the cap: {error}"
    );
    assert_eq!(store.scenes().len(), MAX_SCENES);

    store
        .upsert(scene_with_scale("scene-0", 2.0))
        .expect("replacing a scene at the cap is allowed");
    assert_eq!(names(&store).first().copied(), Some("scene-0"));

    let mut documents = (0..MAX_SCENES)
        .map(|index| scene_named(&format!("scene-{index}")))
        .collect::<Vec<_>>();
    documents.push(scene_named("one-more"));
    let error = SceneStore::from_documents(documents).expect_err("65 scenes are rejected");
    assert!(
        error.contains(&MAX_SCENES.to_string()),
        "error names the cap: {error}"
    );
}

#[test]
fn future_store_version_is_rejected_and_named() {
    let payload = format!("{{\"version\":{},\"scenes\":[]}}", SCENE_STORE_VERSION + 1);
    let error = SceneStore::from_json(&payload).expect_err("a future store version is rejected");
    assert!(
        error.contains(&(SCENE_STORE_VERSION + 1).to_string()),
        "error names the found version: {error}"
    );
    assert!(
        error.contains(&SCENE_STORE_VERSION.to_string()),
        "error names the expected version: {error}"
    );
}

#[test]
fn future_scene_version_inside_the_store_rejects_everything() {
    let mut future = scene_named("future");
    future.version = 2;
    let payload = envelope(&[future]);

    let error = SceneStore::from_json(&payload).expect_err("the whole payload is rejected");
    assert!(error.contains("future"), "error names the scene: {error}");
    assert!(
        error.contains("version 2"),
        "error names the found scene version: {error}"
    );
}

#[test]
fn one_invalid_scene_rejects_the_whole_payload() {
    let valid = scene_named("valid");
    let invalid = scene_with_scale("invalid", 100.0);
    let payload = envelope(&[valid.clone(), invalid.clone()]);

    let target = store_of(&["kept"]);
    let before = target.clone();
    let error = import_store(&payload).expect_err("one invalid scene rejects the payload");
    assert!(error.contains("invalid"), "error names the scene: {error}");
    assert!(
        error.contains("camera scale"),
        "error names the failed rule: {error}"
    );
    assert_eq!(target, before, "the target store is untouched");
    assert_eq!(names(&target), vec!["kept"]);

    let error = SceneStore::from_documents(vec![valid, invalid])
        .expect_err("the constructor is atomic too");
    assert!(error.contains("invalid"), "error names the scene: {error}");
}

#[test]
fn upsert_replaces_same_name_in_place_and_appends_new() {
    let mut store = store_of(&["first", "second"]);
    store
        .upsert(scene_with_scale("second", 3.0))
        .expect("a replacement is accepted");

    assert_eq!(names(&store), vec!["first", "second"]);
    assert_eq!(
        store.get("second").map(|scene| scene.camera.scale),
        Some(3.0)
    );

    store
        .upsert(scene_named("third"))
        .expect("a new scene is appended");
    assert_eq!(names(&store), vec!["first", "second", "third"]);
}

#[test]
fn delete_reports_whether_a_scene_was_removed() {
    let mut store = store_of(&["first", "second"]);

    assert!(store.delete("first"));
    assert_eq!(names(&store), vec!["second"]);
    assert!(!store.delete("first"));
    assert!(!store.delete("missing"));
    assert_eq!(names(&store), vec!["second"]);
}

#[test]
fn duplicate_clones_a_scene_under_a_unique_independent_name() {
    let mut store =
        SceneStore::from_documents(vec![scene_with_scale("first", 2.5), scene_named("second")])
            .expect("valid, unique scenes fit in an empty store");

    let copy = store.duplicate("first").expect("the named scene is copied");
    assert_eq!(copy, "first copy");
    assert_eq!(
        names(&store),
        vec!["first", "second", "first copy"],
        "the copy appends under its new name"
    );
    assert_eq!(
        store.get("first copy").map(|scene| scene.camera.scale),
        Some(2.5),
        "the copy starts equal to its source"
    );

    // The copy is independent of later source edits.
    store
        .upsert(scene_with_scale("first", 8.0))
        .expect("the source can be replaced");
    assert_eq!(
        store.get("first copy").map(|scene| scene.camera.scale),
        Some(2.5),
        "editing the source does not change the copy"
    );

    assert_eq!(
        store.duplicate("first").expect("a second copy stores"),
        "first copy 2",
        "every copy name is unique"
    );

    let before = store.clone();
    let error = store
        .duplicate("missing")
        .expect_err("a missing source is rejected");
    assert!(
        error.contains("missing"),
        "the error names the missing scene: {error}"
    );
    assert_eq!(
        store, before,
        "a failed duplicate leaves the store unchanged"
    );

    // A full store rejects the copy and keeps every scene.
    let mut full = SceneStore::from_documents(
        (0..MAX_SCENES)
            .map(|index| scene_named(&format!("scene {index}")))
            .collect(),
    )
    .expect("fill the store");
    let error = full
        .duplicate("scene 0")
        .expect_err("a full store rejects the copy");
    assert!(
        error.contains(&MAX_SCENES.to_string()),
        "the error names the cap: {error}"
    );
    assert_eq!(full.scenes().len(), MAX_SCENES);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn native_missing_file_is_an_empty_store() {
    let dir = TempDir::new("missing");

    let (store, note) = load_dir(dir.path());

    assert!(store.scenes().is_empty());
    assert_eq!(note, None);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn native_round_trip_writes_and_reads_scenes() {
    let dir = TempDir::new("round-trip");
    let store = store_of(&["first", "second"]);

    save_dir(dir.path(), &store).expect("a valid store saves");
    assert!(dir.path().join(SCENES_FILE).is_file());

    let (loaded, note) = load_dir(dir.path());

    assert_eq!(note, None);
    assert_eq!(loaded, store);
    assert_eq!(names(&loaded), vec!["first", "second"]);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn native_corrupt_file_yields_empty_store_and_a_note() {
    let dir = TempDir::new("corrupt");
    fs::write(dir.path().join(SCENES_FILE), "this is not JSON").expect("corrupt file is written");

    let (store, note) = load_dir(dir.path());

    assert!(store.scenes().is_empty());
    let note = note.expect("a corrupt file produces a note");
    assert!(
        note.contains("unusable"),
        "the note says the file is unusable: {note}"
    );
    assert!(
        note.contains(SCENES_FILE),
        "the note names the file: {note}"
    );
}

#[test]
fn export_and_import_compose_the_helpers() {
    let store = store_of(&["first", "second"]);
    let json = export_store(&store).expect("a valid store exports");
    let imported = import_store(&json).expect("an exported store imports");
    assert_eq!(imported, store);

    let error = import_store(&format!(
        "{{\"version\":{},\"scenes\":[]}}",
        SCENE_STORE_VERSION + 1
    ))
    .expect_err("a future envelope version fails the import");
    assert!(
        !error.is_empty(),
        "a failed import is never an empty string"
    );
}

#[test]
fn json_field_order_and_compact_form_are_pinned() {
    let json = store_of(&["alpha"])
        .to_json()
        .expect("a valid store serializes");

    assert!(
        json.starts_with("{\"version\":1,\"scenes\":["),
        "the envelope opens with version then scenes: {json}"
    );
    assert!(json.contains("\"name\":\"alpha\""), "{json}");
    assert!(
        json.ends_with("]}"),
        "the envelope closes compactly: {json}"
    );
    assert!(!json.contains('\n'), "the export is one line: {json}");
    assert!(
        !json.contains("\": "),
        "the export has no pretty field spacing: {json}"
    );
    assert!(
        !json.contains("\", \""),
        "the export has no pretty array spacing: {json}"
    );
    assert!(
        json.find("\"version\"").expect("version is present")
            < json.find("\"scenes\"").expect("scenes is present"),
        "version precedes scenes: {json}"
    );
}

#[test]
fn upsert_rejects_an_invalid_scene_without_mutating_the_store() {
    let mut store = store_of(&["kept"]);
    let before = store.clone();

    let error = store
        .upsert(scene_with_scale("invalid", 100.0))
        .expect_err("an out-of-range camera is rejected");

    assert!(
        error.contains("camera scale"),
        "error names the failed rule: {error}"
    );
    assert_eq!(store, before);
    assert_eq!(names(&store), vec!["kept"]);
    assert!(store.get("invalid").is_none());
}

/// A staging failure must leave the previously saved store exactly as it was.
///
/// A read-only directory blocks creating the sibling temporary file, but it
/// does not block truncating an already-existing writable file. The direct
/// write this replaces therefore used to replace the only JSON despite the
/// reported failure; the staged write returns the error and preserves it.
#[cfg(unix)]
#[test]
fn a_refused_scene_write_preserves_the_previous_store() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let original = scene_with_scale("A", 1.0);
    let mut store = SceneStore::new();
    store.upsert(original.clone()).unwrap();
    save_dir(dir.path(), &store).expect("the first save succeeds");
    let path = dir.path().join(SCENES_FILE);
    let before = fs::read_to_string(&path).expect("the first save wrote a file");

    let mut changed = store.clone();
    changed
        .upsert(scene_with_scale("A", 4.0))
        .expect("the update stores in memory");
    let mut permissions = fs::metadata(dir.path()).unwrap().permissions();
    permissions.set_mode(0o555);
    fs::set_permissions(dir.path(), permissions).unwrap();
    let result = save_dir(dir.path(), &changed);
    let mut permissions = fs::metadata(dir.path()).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(dir.path(), permissions).unwrap();

    assert!(
        result.is_err(),
        "a write that cannot stage a sibling reports the failure"
    );
    assert_eq!(
        fs::read_to_string(&path).expect("the old file survives"),
        before,
        "the previous store is not truncated or replaced"
    );
    let (reloaded, _) = load_dir(dir.path());
    assert_eq!(reloaded.get("A").map(|scene| scene.camera.scale), Some(1.0));
}
