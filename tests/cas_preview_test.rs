use vector_field_explorer::colormap::Colormap;
use vector_field_explorer::ui::cas_preview::{
    PREVIEW_MAX_EDGE, PreviewCache, PreviewImage, PreviewPlane, preview_key, scalar_heatmap,
};

fn pixel(image: &PreviewImage, x: u32, y: u32) -> [u8; 4] {
    let index = ((y * image.width + x) * 4) as usize;
    image.rgba[index..index + 4].try_into().unwrap()
}

#[test]
fn test_preview_heatmap_is_bounded_and_finite() {
    let image = scalar_heatmap("x*y", PreviewPlane::Xy, [0.0, 1.0], Colormap::Turbo)
        .expect("a plain expression previews");
    assert!(image.width > 0 && image.height > 0);
    assert!(
        image.width <= PREVIEW_MAX_EDGE && image.height <= PREVIEW_MAX_EDGE,
        "preview must stay bounded: {}x{}",
        image.width,
        image.height
    );
    assert_eq!(
        image.rgba.len(),
        (image.width * image.height * 4) as usize,
        "RGBA byte count matches the bounded grid"
    );
    assert!(
        image
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .all(|sample| sample[3] == 0 || sample[3] == 255),
        "every texel is either transparent or fully opaque, never a partial NaN colour"
    );
    assert!(
        image
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .any(|sample| sample[3] == 255),
        "a finite expression produces visible texels"
    );
}

#[test]
fn test_preview_matches_palette_endpoints() {
    let palette = Colormap::Turbo;
    let image = scalar_heatmap("x", PreviewPlane::Xy, [0.0, 1.0], palette).expect("heatmap builds");
    let low = pixel(&image, 0, image.height / 2);
    let high = pixel(&image, image.width - 1, image.height / 2);
    assert_eq!(&low[..3], palette.from_unit(0.0));
    assert_eq!(&high[..3], palette.from_unit(1.0));
    assert_eq!(low[3], 255);
    assert_eq!(high[3], 255);
}

#[test]
fn test_preview_key_changes_with_expression_range_and_palette() {
    let base = preview_key("x", [0.0, 1.0], Colormap::Viridis);
    assert_ne!(base, preview_key("y", [0.0, 1.0], Colormap::Viridis));
    assert_ne!(base, preview_key("x", [-1.0, 1.0], Colormap::Viridis));
    assert_ne!(base, preview_key("x", [0.0, 1.0], Colormap::Turbo));
    assert_eq!(base, preview_key("x", [0.0, 1.0], Colormap::Viridis));
}

#[test]
fn test_non_finite_samples_are_transparent() {
    let image = scalar_heatmap("sqrt(x)", PreviewPlane::Xy, [0.0, 1.0], Colormap::Viridis)
        .expect("the expression parses");
    let left = pixel(&image, 0, image.height / 2);
    assert_eq!(left, [0, 0, 0, 0], "non-finite samples are transparent");
    let right = pixel(&image, image.width - 1, image.height / 2);
    assert_eq!(right[3], 255, "finite samples stay opaque");
}

#[test]
fn test_cache_does_not_rebuild_for_an_unchanged_key() {
    let ctx = eframe::egui::Context::default();
    let image =
        scalar_heatmap("x", PreviewPlane::Xy, [0.0, 1.0], Colormap::Mono).expect("heatmap builds");
    let key = preview_key("x", [0.0, 1.0], Colormap::Mono);
    let mut cache = PreviewCache::default();
    assert!(cache.texture().is_none());
    cache.sync(&ctx, Some(&image), key);
    let first = cache.texture().expect("first upload").id();
    cache.sync(&ctx, None, key);
    assert_eq!(
        cache
            .texture()
            .expect("unchanged key keeps the texture")
            .id(),
        first,
        "an unchanged key must not upload another texture"
    );
    cache.sync(&ctx, Some(&image), key + 1);
    assert_ne!(
        cache.texture().expect("new key uploads").id(),
        first,
        "a changed key replaces the texture"
    );
}
