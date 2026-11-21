//! Bounded CPU heatmap preview for scalar CAS results.
//!
//! The CAS panel shows a small thumbnail of the derived scalar expression. The
//! GPU plot renderer needs a device and a native-only readback, so a thumbnail
//! uses the same `meval` scalar environment the field parser uses and the
//! active [`Colormap`] instead. Native and wasm therefore paint the preview
//! identically, and tests need no graphics device.
//!
//! Sampling is capped at [`PREVIEW_MAX_EDGE`] texels per axis. A non-finite
//! sample is transparent instead of a NaN colour, and [`PreviewCache`] uploads
//! one texture per [`preview_key`] so an unchanged result is never resampled or
//! re-uploaded each frame.

use std::hash::{Hash, Hasher};

use eframe::egui;

use crate::colormap::Colormap;

/// Maximum preview texels per axis.
pub const PREVIEW_MAX_EDGE: u32 = 160;

/// Half-extent of the fixed preview plane in world units.
///
/// The preview is a teaching thumbnail, not an SI-scaled plot, so it always
/// spans this symmetric window around the origin and clamps values outside the
/// caller-supplied range. The panel fixes that range at a normalized `[-1, 1]`,
/// while [`preview_key`] still covers the range the caller passes.
pub const PREVIEW_SPAN: f64 = 5.0;

/// The fixed plane a scalar result is sampled on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreviewPlane {
    /// `x` across, `y` up; `z = 0`.
    #[default]
    Xy,
    /// `x` across, `z` up; `y = 0`.
    Xz,
    /// `y` across, `z` up; `x = 0`.
    Yz,
}

impl PreviewPlane {
    /// Map the two preview axes to the `(x, y, z)` evaluation environment.
    fn coordinates(self, u: f64, v: f64) -> (f64, f64, f64) {
        match self {
            Self::Xy => (u, v, 0.0),
            Self::Xz => (u, 0.0, v),
            Self::Yz => (0.0, u, v),
        }
    }
}

/// One bounded RGBA heatmap ready for `egui` upload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Sample `expr` on `plane` and colour it through `palette`.
///
/// `range` is the scalar value window mapped to the palette; values outside it
/// clamp to the ramp ends. A sample that is NaN or infinite is left fully
/// transparent. Parsing reuses the repository's `meval` scalar environment and
/// binds the same `x`, `y`, `z`, `t` names as the vector formula evaluator.
pub fn scalar_heatmap(
    expr: &str,
    plane: PreviewPlane,
    range: [f64; 2],
    palette: Colormap,
) -> Result<PreviewImage, String> {
    let [low, high] = range;
    if !low.is_finite() || !high.is_finite() || high <= low {
        return Err("preview range must be finite and increasing".into());
    }
    let expression = expr
        .parse::<meval::Expr>()
        .map_err(|error| format!("preview parse error: {error}"))?;
    let function = expression
        .bind4("x", "y", "z", "t")
        .map_err(|error| format!("preview bind error: {error}"))?;

    let width = PREVIEW_MAX_EDGE;
    let height = PREVIEW_MAX_EDGE;
    let mut rgba = vec![0; (width * height * 4) as usize];
    let column_step = 2.0 * PREVIEW_SPAN / f64::from(PREVIEW_MAX_EDGE - 1);
    let row_step = 2.0 * PREVIEW_SPAN / f64::from(PREVIEW_MAX_EDGE - 1);

    let mut index = 0;
    for row in 0..height {
        let v = PREVIEW_SPAN - f64::from(row) * row_step;
        for column in 0..width {
            let u = -PREVIEW_SPAN + f64::from(column) * column_step;
            let (x, y, z) = plane.coordinates(u, v);
            let value = function(x, y, z, 0.0);
            if value.is_finite() {
                let t = ((value - low) / (high - low)).clamp(0.0, 1.0) as f32;
                let [r, g, b] = palette.from_unit(t);
                rgba[index..index + 4].copy_from_slice(&[r, g, b, 255]);
            }
            index += 4;
        }
    }

    Ok(PreviewImage {
        width,
        height,
        rgba,
    })
}

/// The cache identity of a preview: expression, value range and palette.
///
/// The preview plane is fixed by the panel, so it is not part of the key.
pub fn preview_key(expr: &str, range: [f64; 2], palette: Colormap) -> u64 {
    // `DefaultHasher` is intentionally unpersisted; it need not be stable across releases.
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    expr.hash(&mut hasher);
    range[0].to_bits().hash(&mut hasher);
    range[1].to_bits().hash(&mut hasher);
    palette.label().hash(&mut hasher);
    hasher.finish()
}

/// One texture handle per preview key.
///
/// [`Self::sync`] ignores an unchanged key, so the caller may hand in a cached
/// image every frame without triggering another upload.
#[derive(Clone, Default)]
pub struct PreviewCache {
    texture: Option<egui::TextureHandle>,
    key: Option<u64>,
}

impl std::fmt::Debug for PreviewCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreviewCache")
            .field("key", &self.key)
            .field("has_texture", &self.texture.is_some())
            .finish()
    }
}

impl PreviewCache {
    /// Upload `image` only when `key` differs from the cached key.
    ///
    /// A `None` image clears the texture only when the key changed.
    pub fn sync(&mut self, ctx: &egui::Context, image: Option<&PreviewImage>, key: u64) {
        if self.key == Some(key) && self.texture.is_some() {
            return;
        }
        let Some(image) = image else {
            self.texture = None;
            self.key = None;
            return;
        };
        let pixels = egui::ColorImage::from_rgba_unmultiplied(
            [image.width as usize, image.height as usize],
            &image.rgba,
        );
        self.texture =
            Some(ctx.load_texture("cas-result-preview", pixels, egui::TextureOptions::LINEAR));
        self.key = Some(key);
    }

    /// The cached texture, if one has been uploaded for the current key.
    pub fn texture(&self) -> Option<&egui::TextureHandle> {
        self.texture.as_ref()
    }

    /// Drop the cached texture so a non-scalar result cannot reuse it later.
    pub fn clear(&mut self) {
        self.texture = None;
        self.key = None;
    }
}
