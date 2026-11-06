use std::io;

/// Save a rendered frame buffer as PNG.
///
/// The frame is packed RGBA8 bytes (row-major, as produced by eframe/wgpu
/// readbacks); channels are written through unchanged.
pub fn save_png(frame: &[u8], width: u32, height: u32, path: &str) -> io::Result<()> {
    let buffer = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(width, height, frame.to_vec())
        .ok_or_else(|| io::Error::other("invalid frame size"))?;
    buffer.save_with_format(path, image::ImageFormat::Png)
        .map_err(io::Error::other)?;
    Ok(())
}

/// Placeholder for SVG export – not yet implemented.
pub fn save_svg(_svg_data: &str, _path: &str) -> io::Result<()> {
    // Future implementation will generate an SVG string from the vector data.
    Err(io::Error::new(io::ErrorKind::Unsupported, "SVG export not implemented"))
}

use std::fs;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Preset {
    pub name: String,
    pub position: [f32; 2],
    pub scale: f32,
}

/// Save a camera preset to the given directory with the specified name.
pub fn save_preset<P: AsRef<std::path::Path>>(dir: P, name: &str, cam: &Camera) -> io::Result<()> {
    let mut presets = load_presets(&dir)?;
    presets.retain(|p| p.name != name);
    presets.push(Preset { name: name.to_string(), position: cam.position, scale: cam.scale });
    let data = serde_json::to_string_pretty(&presets).map_err(io::Error::other)?;
    fs::create_dir_all(dir.as_ref())?;
    fs::write(dir.as_ref().join("presets.json"), data)?;
    Ok(())
}

/// Delete the named preset from the given directory's presets.json.
/// The update is written back even when the list becomes empty.
pub fn delete_preset<P: AsRef<std::path::Path>>(dir: P, name: &str) -> io::Result<()> {
    let mut presets = load_presets(&dir)?;
    presets.retain(|p| p.name != name);
    let data = serde_json::to_string_pretty(&presets).map_err(io::Error::other)?;
    fs::create_dir_all(dir.as_ref())?;
    fs::write(dir.as_ref().join("presets.json"), data)?;
    Ok(())
}

/// Load all presets from the given directory.
pub fn load_presets<P: AsRef<std::path::Path>>(dir: P) -> io::Result<Vec<Preset>> {
    let path = dir.as_ref().join("presets.json");
    if !path.exists() { return Ok(vec![]); }
    let data = fs::read_to_string(path)?;
    let presets: Vec<Preset> = serde_json::from_str(&data).map_err(io::Error::other)?;
    Ok(presets)
}

use crate::render::Camera;
