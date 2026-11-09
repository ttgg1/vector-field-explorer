//! The one project mark, decoded once and cached per egui context.
//!
//! Both the About window and any future in-app identity surface draw the same
//! committed 256 px PNG; decoding it on every open would be wasteful, and a
//! second copy of the bytes would let the mark drift. The cache lives in egui
//! context data as a typed temporary, so it is dropped with the context instead
//! of leaking across test contexts.

use eframe::egui;

/// The canonical 256 px mark, compiled into the binary. `include_bytes!` paths
/// are relative to this source file.
const LOGO_PNG: &[u8] = include_bytes!("../../packaging/linux/vector-field-explorer-256.png");

/// Context-data key for the cached texture handle.
const CACHE_KEY: &str = "vector-field-explorer-logo-texture";

/// The decoded project mark as an egui texture.
///
/// Public only so the integration tests can verify the committed asset and its
/// caching; production code goes through [`logo_image`].
#[doc(hidden)]
pub fn logo_texture(ctx: &egui::Context) -> egui::TextureHandle {
    let cache_id = egui::Id::unique(CACHE_KEY);
    if let Some(texture) = ctx.data(|data| data.get_temp::<egui::TextureHandle>(cache_id)) {
        return texture;
    }

    let image = decode_logo().unwrap_or_else(|| {
        // A UI surface must stay usable even if the committed asset is broken.
        log::warn!("project logo could not be decoded; using a transparent placeholder");
        egui::ColorImage::new([1, 1], vec![egui::Color32::TRANSPARENT])
    });
    let texture = ctx.load_texture(CACHE_KEY, image, egui::TextureOptions::LINEAR);
    ctx.data_mut(|data| data.insert_temp(cache_id, texture.clone()));
    texture
}

/// The brand image at `size` points. The cached handle owns the GPU texture, so
/// the returned [`egui::Image`] only needs its id and display size.
pub(crate) fn logo_image(ctx: &egui::Context, size: f32) -> egui::Image<'static> {
    let texture = logo_texture(ctx);
    egui::Image::new((texture.id(), egui::vec2(size, size)))
}

fn decode_logo() -> Option<egui::ColorImage> {
    let image = image::load_from_memory(LOGO_PNG).ok()?;
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [width as usize, height as usize],
        rgba.as_raw(),
    ))
}
