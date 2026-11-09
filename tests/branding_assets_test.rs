//! Ticket 68: committed identity assets.
//!
//! The generators (`scripts/gen-icons.sh`, `scripts/make-icns.py`) run once by
//! a maintainer and their output is committed; CI and `cargo test` only read
//! these files. This pins the format set the packaging workflow installs and
//! the provenance document that records where the mark comes from.

use std::fs;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn repo_bytes(relative: &str) -> Vec<u8> {
    let path = repo_root().join(relative);
    fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

fn repo_text(relative: &str) -> String {
    let path = repo_root().join(relative);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

fn png_size(relative: &str) -> (u32, u32) {
    let bytes = repo_bytes(relative);
    let image = image::load_from_memory(&bytes)
        .unwrap_or_else(|error| panic!("`{relative}` is not a decodable image: {error}"));
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    (width, height)
}

#[test]
fn the_committed_png_set_covers_every_packaged_size() {
    for size in [16, 32, 48, 64, 128, 256, 512] {
        let relative = format!("packaging/icons/vector-field-explorer-{size}.png");
        assert_eq!(
            png_size(&relative),
            (size, size),
            "`{relative}` must be a {size}x{size} PNG",
        );
    }

    // The linuxdeploy path stays stable and is the same 256 px mark.
    assert_eq!(
        png_size("packaging/linux/vector-field-explorer-256.png"),
        (256, 256),
    );
}

#[test]
fn the_macos_icns_is_a_well_formed_container() {
    let bytes = repo_bytes("packaging/icons/vector-field-explorer.icns");
    assert_eq!(
        &bytes[..4],
        b"icns",
        "the file starts with the Apple Icon Image magic"
    );

    let declared = u32::from_be_bytes(bytes[4..8].try_into().expect("icns length field"));
    assert_eq!(
        declared as usize,
        bytes.len(),
        "the icns header length must match the file exactly",
    );

    // Every element is type + big-endian length + payload, and the modern
    // PNG-bearing element types cover the small, medium and large slots.
    let mut offset = 8;
    let mut found = Vec::new();
    while offset < bytes.len() {
        let kind = &bytes[offset..offset + 4];
        let length = u32::from_be_bytes(
            bytes[offset + 4..offset + 8]
                .try_into()
                .expect("icns element length field"),
        ) as usize;
        assert!(
            length >= 8,
            "an icns element cannot be shorter than its header"
        );
        assert!(
            offset + length <= bytes.len(),
            "an icns element overruns the file"
        );
        let payload = &bytes[offset + 8..offset + length];
        if matches!(
            kind,
            b"icp4" | b"icp5" | b"icp6" | b"ic07" | b"ic08" | b"ic09"
        ) {
            assert_eq!(
                &payload[..8],
                b"\x89PNG\r\n\x1a\n",
                "PNG-bearing slots hold PNG data"
            );
            found.push(String::from_utf8_lossy(kind).into_owned());
        }
        offset += length;
    }
    assert_eq!(offset, bytes.len(), "the elements tile the file exactly");
    for expected in ["icp4", "icp5", "icp6", "ic07", "ic08", "ic09"] {
        assert!(
            found.iter().any(|kind| kind == expected),
            "the icns needs the `{expected}` slot; got {found:?}",
        );
    }
}

#[test]
fn readme_shows_the_mark_above_the_title() {
    let readme = repo_text("README.md");
    let logo = readme
        .find("packaging/linux/vector-field-explorer.svg")
        .expect("README embeds the canonical SVG");
    let title = readme
        .find("# Vector Field Explorer")
        .expect("README keeps its H1");

    assert!(logo < title, "the mark sits above the H1");
    assert!(
        readme.contains(r#"width="128""#),
        "the README mark is 128 px wide"
    );
    assert!(
        readme.contains(r#"alt="Vector Field Explorer logo""#),
        "the README mark has accessible alt text",
    );
}

#[test]
fn artwork_provenance_records_the_source_license_and_commands() {
    let artwork = repo_text("packaging/ARTWORK.md");

    for token in [
        "packaging/linux/vector-field-explorer.svg",
        "GPL-3.0-only",
        "scripts/gen-icons.sh",
        "scripts/make-icns.py",
        "2026-11-09",
        "third-party",
    ] {
        assert!(
            artwork.contains(token),
            "packaging/ARTWORK.md must record `{token}`",
        );
    }
}
