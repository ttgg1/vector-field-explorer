//! Saved-scene store (issue #90 slice B2b).
//!
//! A strict, versioned JSON envelope holding zero or more [`SceneDocument`]s in
//! insertion order. Native storage is `scenes.json` in a caller-supplied
//! directory; browser storage is one `localStorage` key. This module picks no
//! path and touches no UI or app state.
//!
//! Imports are atomic: every document and every envelope field is checked
//! before a store is returned, and a failure never leaves a partial store.

use serde::{Deserialize, Serialize};

use crate::scene_document::SceneDocument;

/// The only scene-store envelope version this module writes or accepts.
pub const SCENE_STORE_VERSION: u32 = 1;

/// Native file name inside the caller-supplied directory.
pub const SCENES_FILE: &str = "scenes.json";

/// Most scenes one store may hold.
pub const MAX_SCENES: usize = 64;

/// localStorage key for the browser's saved-scene store. Deliberately separate
/// from `eframe::APP_KEY` and the camera-preset key.
#[cfg(target_arch = "wasm32")]
pub const SCENES_STORAGE_KEY: &str = "vector-field-explorer.saved-scenes";

/// The on-disk / on-the-wire envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SceneStoreFile {
    /// Envelope format version; must be [`SCENE_STORE_VERSION`].
    version: u32,
    /// Scenes in insertion order.
    scenes: Vec<SceneDocument>,
}

/// A validated, ordered collection of saved scenes.
///
/// The only ways to add a scene are [`SceneStore::from_documents`] and
/// [`SceneStore::upsert`], so every stored document has passed
/// [`SceneDocument::validate`] and names stay unique. Names are compared
/// exactly and case-sensitively: `"Case"` and `"case"` are distinct.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SceneStore {
    scenes: Vec<SceneDocument>,
}

impl SceneStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a store from `scenes`, preserving insertion order.
    ///
    /// Rejects more than [`MAX_SCENES`], any invalid document and any
    /// duplicate name. Names are compared exactly and case-sensitively, so
    /// `"Case"` and `"case"` are distinct. A failure returns no partial store.
    pub fn from_documents(scenes: Vec<SceneDocument>) -> Result<Self, String> {
        if scenes.len() > MAX_SCENES {
            return Err(format!(
                "a scene store holds at most {MAX_SCENES} scenes; found {}",
                scenes.len()
            ));
        }

        let mut store = Self {
            scenes: Vec::with_capacity(scenes.len()),
        };
        for scene in scenes {
            validate_scene(&scene)?;
            if store.get(&scene.name).is_some() {
                return Err(format!("duplicate scene name {:?}", scene.name));
            }
            store.scenes.push(scene);
        }
        Ok(store)
    }

    /// Scenes in insertion order.
    pub fn scenes(&self) -> &[SceneDocument] {
        &self.scenes
    }

    /// The scene named `name`, if present.
    ///
    /// The lookup compares names exactly and case-sensitively: `"Case"` and
    /// `"case"` are distinct.
    pub fn get(&self, name: &str) -> Option<&SceneDocument> {
        self.scenes.iter().find(|scene| scene.name == name)
    }

    /// Validate and insert `scene`, replacing a scene with the same name in
    /// place or appending it otherwise.
    ///
    /// Rejects an invalid document and a new scene once the store already holds
    /// [`MAX_SCENES`]. A rejected scene leaves the store unchanged.
    pub fn upsert(&mut self, scene: SceneDocument) -> Result<(), String> {
        validate_scene(&scene)?;
        if let Some(index) = self
            .scenes
            .iter()
            .position(|existing| existing.name == scene.name)
        {
            if let Some(slot) = self.scenes.get_mut(index) {
                *slot = scene;
            }
            return Ok(());
        }
        if self.scenes.len() >= MAX_SCENES {
            return Err(format!(
                "scene store is full: at most {MAX_SCENES} scenes are allowed"
            ));
        }
        self.scenes.push(scene);
        Ok(())
    }

    /// Remove the scene named `name`, reporting whether one was removed.
    pub fn delete(&mut self, name: &str) -> bool {
        let before = self.scenes.len();
        self.scenes.retain(|scene| scene.name != name);
        self.scenes.len() != before
    }

    /// Serialize the compact envelope `{"version":1,"scenes":[...]}`.
    ///
    /// Never returns an empty string: a document serde cannot represent (for
    /// example a non-finite view field) yields an `Err`.
    pub fn to_json(&self) -> Result<String, String> {
        let file = SceneStoreFile {
            version: SCENE_STORE_VERSION,
            scenes: self.scenes.clone(),
        };
        serde_json::to_string(&file)
            .map_err(|error| format!("scene store could not be serialised: {error}"))
    }

    /// Parse and validate a compact envelope.
    ///
    /// Unknown envelope fields, an unsupported version, an invalid document, a
    /// duplicate name or more than [`MAX_SCENES`] rejects the whole payload.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let file: SceneStoreFile = serde_json::from_str(text)
            .map_err(|error| format!("invalid scene store JSON: {error}"))?;
        if file.version != SCENE_STORE_VERSION {
            return Err(format!(
                "scene store version {} is unsupported; expected version {}",
                file.version, SCENE_STORE_VERSION
            ));
        }
        Self::from_documents(file.scenes)
    }
}

fn validate_scene(scene: &SceneDocument) -> Result<(), String> {
    scene
        .validate()
        .map_err(|error| format!("invalid scene {:?}: {error}", scene.name))
}

/// Validate and serialize a store through [`SceneStore::to_json`].
pub fn export_store(store: &SceneStore) -> Result<String, String> {
    store.to_json()
}

/// Parse and validate a store through [`SceneStore::from_json`].
pub fn import_store(text: &str) -> Result<SceneStore, String> {
    SceneStore::from_json(text)
}

/// Load `scenes.json` from `dir`.
///
/// A missing file is an empty store with no note. An unreadable, corrupt or
/// invalid file is an empty store plus a note naming the unusable file; the
/// failure is logged and never panics.
#[cfg(not(target_arch = "wasm32"))]
pub fn load_dir(dir: impl AsRef<std::path::Path>) -> (SceneStore, Option<String>) {
    use std::io::ErrorKind;

    let path = dir.as_ref().join(SCENES_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => return (SceneStore::new(), None),
        Err(error) => {
            log::warn!("scene store {} could not be read: {error}", path.display());
            return (
                SceneStore::new(),
                Some(unusable_note(&path, &error.to_string())),
            );
        }
    };
    match SceneStore::from_json(&text) {
        Ok(store) => (store, None),
        Err(error) => {
            log::warn!("scene store {} is unusable: {error}", path.display());
            (SceneStore::new(), Some(unusable_note(&path, &error)))
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn unusable_note(path: &std::path::Path, error: &str) -> String {
    format!(
        "Saved scenes file {} is unusable; starting with an empty list: {error}",
        path.display()
    )
}

/// Create `dir`, serialize `store` and write `scenes.json` into it.
#[cfg(not(target_arch = "wasm32"))]
pub fn save_dir(dir: impl AsRef<std::path::Path>, store: &SceneStore) -> Result<(), String> {
    let dir = dir.as_ref();
    std::fs::create_dir_all(dir).map_err(|error| {
        format!(
            "could not create scene directory {}: {error}",
            dir.display()
        )
    })?;
    let text = store.to_json()?;
    let path = dir.join(SCENES_FILE);
    std::fs::write(&path, text)
        .map_err(|error| format!("could not write scene file {}: {error}", path.display()))
}

#[cfg(target_arch = "wasm32")]
fn local_storage() -> Result<web_sys::Storage, String> {
    let window = web_sys::window().ok_or_else(|| "browser window unavailable".to_string())?;
    window
        .local_storage()
        .map_err(js_error)?
        .ok_or_else(|| "browser localStorage unavailable".to_string())
}

#[cfg(target_arch = "wasm32")]
fn js_error(error: impl Into<wasm_bindgen::JsValue>) -> String {
    let error = error.into();
    error.as_string().unwrap_or_else(|| format!("{error:?}"))
}

/// Load the browser's saved-scene store.
///
/// A missing key is an empty store with no note. Corrupt or unavailable
/// storage is an empty store plus [`storage_note`], logged without panicking.
#[cfg(target_arch = "wasm32")]
pub fn load_wasm() -> (SceneStore, Option<String>) {
    let storage = match local_storage() {
        Ok(storage) => storage,
        Err(error) => {
            log::warn!("saved-scene storage unavailable: {error}");
            return (SceneStore::new(), Some(storage_note()));
        }
    };
    match storage.get_item(SCENES_STORAGE_KEY) {
        Ok(None) => (SceneStore::new(), None),
        Ok(Some(text)) => match SceneStore::from_json(&text) {
            Ok(store) => (store, None),
            Err(error) => {
                log::warn!("saved-scene storage is corrupt: {error}");
                (SceneStore::new(), Some(storage_note()))
            }
        },
        Err(error) => {
            log::warn!("saved-scene storage read failed: {}", js_error(error));
            (SceneStore::new(), Some(storage_note()))
        }
    }
}

/// Write the browser's saved-scene store through to localStorage.
#[cfg(target_arch = "wasm32")]
pub fn save_wasm(store: &SceneStore) -> Result<(), String> {
    let storage = local_storage()?;
    let text = store.to_json()?;
    storage
        .set_item(SCENES_STORAGE_KEY, &text)
        .map_err(js_error)
}

/// Wording for the one non-blocking note when scene storage cannot be used.
pub fn storage_note() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        "Browser storage is unavailable; saved scenes will not survive a reload.".to_string()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        "Scene storage is unavailable; saved scenes will not persist.".to_string()
    }
}
