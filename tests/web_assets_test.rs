//! Ticket 68: the browser build's identity surface.
//!
//! `index.html`, `web/style.css` and `web/manifest.webmanifest` are shipped
//! verbatim by Trunk, so they are pinned here the same way `pointer_lock_test`
//! guards the pointer-lock bridge. No test needs `rsvg-convert`, `inkscape`, or
//! any other generator: the committed assets are the contract.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use vector_field_explorer::ui::menu::{ABOUT_NAME, ABOUT_TAGLINE};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn repo_file(relative: &str) -> String {
    let path = repo_root().join(relative);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

fn png_size(relative: &str) -> (u32, u32) {
    let path = repo_root().join(relative);
    let bytes = fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let image = image::load_from_memory(&bytes)
        .unwrap_or_else(|error| panic!("{} is not a decodable image: {error}", path.display()));
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    (width, height)
}

/// A local URL is written for `dist/`; its source lives either at the repo root
/// or under `web/` (Trunk copies those files to the dist root).
fn assert_referenced_asset(relative: &str) {
    let root = repo_root();
    let local = relative.trim_start_matches("./");
    let direct = root.join(local);
    let under_web = root.join("web").join(local);
    assert!(
        direct.is_file() || under_web.is_file(),
        "index.html references `{relative}`, but neither {} nor {} exists",
        direct.display(),
        under_web.display(),
    );
}

#[test]
fn index_html_carries_the_web_identity_head() {
    let html = repo_file("index.html");
    let head = html.split("</head>").next().expect("index.html has a head");

    for token in [
        r#"name="description""#,
        r#"rel="canonical""#,
        r#"name="theme-color""#,
        r##"content="#101a26""##,
        r#"rel="manifest""#,
        r#"rel="icon""#,
        r#"sizes="16x16""#,
        r#"sizes="32x32""#,
        r#"rel="apple-touch-icon""#,
        r#"sizes="180x180""#,
        r#"property="og:type""#,
        r#"property="og:title""#,
        r#"property="og:description""#,
        r#"property="og:image""#,
        r#"property="og:url""#,
        r#"name="twitter:card""#,
        r#"name="twitter:title""#,
        r#"name="twitter:description""#,
        r#"name="twitter:image""#,
    ] {
        assert!(
            head.contains(token),
            "index.html <head> must carry {token}:\n{head}"
        );
    }

    assert!(
        head.contains(r#"rel="canonical" href="https://ttgg1.github.io/vector-field-explorer/""#),
        "the canonical URL is the Pages site",
    );
    assert!(
        head.contains(r#"content="https://ttgg1.github.io/vector-field-explorer/og-image.png""#),
        "OG and Twitter share the deployed OG image URL",
    );
    assert!(
        head.contains(r#"property="og:image:width" content="1280""#)
            && head.contains(r#"property="og:image:height" content="640""#),
        "the OG card declares its 1280x640 size",
    );
}

#[test]
fn index_html_ships_the_brand_header_before_the_canvas() {
    let html = repo_file("index.html");
    let header = html
        .find(r#"<header class="brand">"#)
        .expect("the page has the branded header");
    let canvas = html
        .find(r#"id="vfe-canvas""#)
        .expect("the canvas id used by the wasm runner and pointer lock is unchanged");

    assert!(
        header < canvas,
        "the header sits above the canvas, not over it"
    );
    assert!(
        html.contains(r#"class="brand-logo""#) && html.contains(r#"src="icons/favicon-32.png""#),
        "the header shows the 32 px mark",
    );
    assert!(
        html.contains(r#"class="brand-name""#),
        "the header shows the wordmark"
    );
    assert!(
        html.contains(r#"class="brand-tagline""#),
        "the header shows the tagline",
    );
    assert!(
        html.contains(ABOUT_TAGLINE),
        "the web tagline is the one About and the menu constants carry",
    );
    assert!(
        html.contains(r#"class="brand-link""#)
            && html.contains("https://github.com/ttgg1/vector-field-explorer"),
        "the header links to the repository",
    );
    assert!(
        html.contains(ABOUT_NAME),
        "the wordmark is the product name",
    );
    assert!(
        html.contains(r#"id="loading""#)
            && html.contains(r#"id="webgpu-unsupported""#)
            && html.contains("<noscript>"),
        "the loading, WebGPU-gate and no-JavaScript overlays are kept",
    );
}

#[test]
fn index_html_trunk_copies_every_local_asset() {
    let html = repo_file("index.html");

    for href in [
        "icons/favicon-16.png",
        "icons/favicon-32.png",
        "icons/apple-touch-icon.png",
        "icons/icon-192.png",
        "icons/icon-512.png",
        "manifest.webmanifest",
        "og-image.png",
    ] {
        assert_referenced_asset(href);
        assert!(
            html.contains(href),
            "index.html must reference the shipped asset `{href}`",
        );
    }

    for source in [
        "web/icons/favicon-16.png",
        "web/icons/favicon-32.png",
        "web/icons/apple-touch-icon.png",
        "web/icons/icon-192.png",
        "web/icons/icon-512.png",
        "web/manifest.webmanifest",
        "web/og-image.png",
    ] {
        assert!(
            html.contains(source),
            "Trunk must copy `{source}` into dist/ (a copy-file entry is missing)",
        );
    }
}

#[test]
fn the_web_icon_set_has_the_reference_dimensions() {
    for (relative, expected) in [
        ("web/icons/favicon-16.png", 16),
        ("web/icons/favicon-32.png", 32),
        ("web/icons/apple-touch-icon.png", 180),
        ("web/icons/icon-192.png", 192),
        ("web/icons/icon-512.png", 512),
        ("web/og-image.png", 1280),
    ] {
        let size = png_size(relative);
        if relative.ends_with("og-image.png") {
            assert_eq!(size, (1280, 640), "the OG card is wide, not square");
        } else {
            assert_eq!(
                size,
                (expected, expected),
                "`{relative}` has the wrong size"
            );
        }
    }
}

#[test]
fn style_css_uses_the_brand_chrome_and_collapses_the_header() {
    let css = repo_file("web/style.css");

    for token in [
        "#101a26",
        "#22313f",
        "#vfe-canvas",
        "flex-direction: column",
        "flex: 1",
        "min-height: 0",
        ".brand-logo",
    ] {
        assert!(css.contains(token), "web/style.css must carry `{token}`");
    }

    let breakpoint = css
        .find("@media (max-width: 700px)")
        .expect("the compact breakpoint matches egui at 700 px");
    let narrow = &css[breakpoint..];
    let brand = narrow
        .find("header.brand {")
        .expect("the entire web-page branding header is targeted");
    let rule = &narrow[brand..];
    assert!(
        rule[..rule.find('}').unwrap_or(rule.len())].contains("display: none"),
        "the whole branding header is hidden in Compact",
    );
    assert!(!css.contains("@media (max-width: 600px)"));
}

#[test]
fn apple_touch_icon_is_opaque_on_the_brand_chrome() {
    // iOS rounds/masks the touch icon; a transparent corner would become an
    // unpredictable colour, so the generator renders it on #101a26.
    let path = repo_root().join("web/icons/apple-touch-icon.png");
    let bytes = fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let image = image::load_from_memory(&bytes).expect("apple-touch-icon is a decodable PNG");
    let rgba = image.to_rgba8();

    assert_eq!(rgba.get_pixel(0, 0).0, [0x10, 0x1a, 0x26, 255]);
    assert_eq!(rgba.get_pixel(179, 179).0, [0x10, 0x1a, 0x26, 255]);
}

#[test]
fn manifest_parses_and_lists_the_shipped_icons() {
    let path = repo_root().join("web/manifest.webmanifest");
    let raw = fs::read_to_string(&path).unwrap_or_else(|error| panic!("read manifest: {error}"));
    let manifest: serde_json::Value = serde_json::from_str(&raw)
        .unwrap_or_else(|error| panic!("manifest is valid JSON: {error}"));

    assert_eq!(manifest["name"], ABOUT_NAME);
    assert_eq!(manifest["theme_color"], "#101a26");
    assert_eq!(manifest["background_color"], "#101a26");
    assert_eq!(manifest["display"], "standalone");
    assert!(
        manifest["short_name"]
            .as_str()
            .is_some_and(|name| !name.is_empty()),
        "the manifest carries a non-empty short_name",
    );
    assert!(
        manifest["start_url"]
            .as_str()
            .is_some_and(|url| !url.is_empty()),
        "the manifest carries a start_url",
    );

    let icons = manifest["icons"]
        .as_array()
        .expect("the manifest lists icons");
    let mut sizes = HashSet::new();
    for icon in icons {
        let src = icon["src"].as_str().expect("every icon has a src");
        let size = icon["sizes"]
            .as_str()
            .expect("every icon declares its sizes");
        assert_eq!(icon["type"], "image/png", "`{src}` is a PNG");
        assert_referenced_asset(src);
        sizes.insert(size.to_string());
    }
    for expected in ["16x16", "32x32", "180x180", "192x192", "512x512"] {
        assert!(
            sizes.contains(expected),
            "the manifest is missing the {expected} icon; got {sizes:?}",
        );
    }
}

#[test]
fn no_web_asset_was_left_on_the_old_grey_chrome() {
    let css = repo_file("web/style.css");
    assert!(
        !css.contains("#1e2129"),
        "web/style.css still uses the superseded #1e2129 page chrome",
    );
    assert!(
        Path::new(&repo_root().join("web/icons")).is_dir(),
        "web/icons/ is the shipped favicon/manifest source directory",
    );
}
