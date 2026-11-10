use std::io;

use image::ImageEncoder as _;

/// Encode a packed RGBA8 frame as PNG bytes without touching a filesystem.
///
/// The encoder consumes the frame slice directly, so a 2048²/4096² capture
/// does not pay for an extra `Vec` copy on the way to the PNG writer.
pub fn encode_png(frame: &[u8], width: u32, height: u32) -> io::Result<Vec<u8>> {
    let expected = u64::from(width) * u64::from(height) * 4;
    if u64::try_from(frame.len()) != Ok(expected) {
        return Err(io::Error::other("invalid frame size"));
    }
    let mut encoded = Vec::new();
    image::codecs::png::PngEncoder::new(&mut encoded)
        .write_image(frame, width, height, image::ExtendedColorType::Rgba8)
        .map_err(io::Error::other)?;
    Ok(encoded)
}

/// Save a rendered frame buffer as PNG.
///
/// The frame is packed RGBA8 bytes (row-major, as produced by eframe/wgpu
/// readbacks); channels are written through unchanged. The destination is any
/// `AsRef<Path>`, so a native dialog's `PathBuf` is written exactly as chosen
/// — no lossy UTF-8 round trip for a non-UTF-8 path (screenshot fix round 1).
#[cfg(not(target_arch = "wasm32"))]
pub fn save_png(
    frame: &[u8],
    width: u32,
    height: u32,
    path: impl AsRef<std::path::Path>,
) -> io::Result<()> {
    std::fs::write(path, encode_png(frame, width, height)?)
}

/// Placeholder for SVG export – not yet implemented.
pub fn save_svg(_svg_data: &str, _path: &str) -> io::Result<()> {
    // Future implementation will generate an SVG string from the vector data.
    Err(io::Error::new(io::ErrorKind::Unsupported, "SVG export not implemented"))
}

#[cfg(not(target_arch = "wasm32"))]
use std::fs;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CameraPreset {
    pub name: String,
    pub position: [f32; 2],
    pub scale: f32,
}

/// Encode the camera-preset list in the same JSON shape the native file uses.
pub fn presets_to_json(presets: &[CameraPreset]) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(presets)
}

/// Decode a camera-preset JSON payload. Corrupt or foreign shapes are `None`;
/// callers fall back to an empty list rather than panicking.
pub fn presets_from_json(data: &str) -> Option<Vec<CameraPreset>> {
    serde_json::from_str(data).ok()
}

/// localStorage key for the browser's camera-preset list. Deliberately separate
/// from `eframe::APP_KEY`: the settings envelope and the preset list are two
/// records with different corruption rules.
#[cfg(target_arch = "wasm32")]
pub const PRESETS_STORAGE_KEY: &str = "vector-field-explorer.camera-presets";

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

/// Load the browser's camera-preset list. A missing key is an empty list; a
/// corrupt payload falls back to empty (logged) without panicking. The second
/// value is a panel note when browser storage itself is unavailable.
#[cfg(target_arch = "wasm32")]
pub fn load_wasm_presets() -> (Vec<CameraPreset>, Option<String>) {
    let storage = match local_storage() {
        Ok(storage) => storage,
        Err(error) => {
            log::warn!("camera preset storage unavailable: {error}");
            return (Vec::new(), Some(storage_note()));
        }
    };
    match storage.get_item(PRESETS_STORAGE_KEY) {
        Ok(None) => (Vec::new(), None),
        Ok(Some(data)) => match presets_from_json(&data) {
            Some(presets) => (presets, None),
            None => {
                log::warn!("camera preset storage is corrupt; starting with an empty list");
                (Vec::new(), None)
            }
        },
        Err(error) => {
            log::warn!("camera preset storage read failed: {}", js_error(error));
            (Vec::new(), Some(storage_note()))
        }
    }
}

/// Write the browser's camera-preset list through to localStorage. The panel
/// keeps the in-memory list when this fails so saving still works in-session.
#[cfg(target_arch = "wasm32")]
pub fn save_wasm_presets(presets: &[CameraPreset]) -> Result<(), String> {
    let storage = local_storage()?;
    let data = presets_to_json(presets).map_err(|error| error.to_string())?;
    storage.set_item(PRESETS_STORAGE_KEY, &data).map_err(js_error)
}

/// Wording for the one non-blocking Presets-panel note.
#[cfg(target_arch = "wasm32")]
pub fn storage_note() -> String {
    "Browser storage is unavailable; camera presets will not survive a reload.".to_string()
}

/// Save a camera preset to the given directory with the specified name.
#[cfg(not(target_arch = "wasm32"))]
pub fn save_preset<P: AsRef<std::path::Path>>(dir: P, name: &str, cam: &Camera) -> io::Result<()> {
    let mut presets = load_presets(&dir)?;
    presets.retain(|p| p.name != name);
    presets.push(CameraPreset { name: name.to_string(), position: cam.position, scale: cam.scale });
    let data = serde_json::to_string_pretty(&presets).map_err(io::Error::other)?;
    fs::create_dir_all(dir.as_ref())?;
    fs::write(dir.as_ref().join("presets.json"), data)?;
    Ok(())
}

/// Delete the named preset from the given directory's presets.json.
/// The update is written back even when the list becomes empty.
#[cfg(not(target_arch = "wasm32"))]
pub fn delete_preset<P: AsRef<std::path::Path>>(dir: P, name: &str) -> io::Result<()> {
    let mut presets = load_presets(&dir)?;
    presets.retain(|p| p.name != name);
    let data = serde_json::to_string_pretty(&presets).map_err(io::Error::other)?;
    fs::create_dir_all(dir.as_ref())?;
    fs::write(dir.as_ref().join("presets.json"), data)?;
    Ok(())
}

/// Load all presets from the given directory.
#[cfg(not(target_arch = "wasm32"))]
pub fn load_presets<P: AsRef<std::path::Path>>(dir: P) -> io::Result<Vec<CameraPreset>> {
    let path = dir.as_ref().join("presets.json");
    if !path.exists() { return Ok(vec![]); }
    let data = fs::read_to_string(path)?;
    let presets: Vec<CameraPreset> = serde_json::from_str(&data).map_err(io::Error::other)?;
    Ok(presets)
}

#[cfg(not(target_arch = "wasm32"))]
use crate::render::Camera;
