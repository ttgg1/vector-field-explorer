//! Ticket 67: the app ships its own DejaVu faces, so epaint must not compile
//! its UFL-licensed Ubuntu-Light/Hack bytes into either binary.
//!
//! `builtin_font_names()` is the compile-time proof that eframe's
//! `default_fonts` feature is off. The remaining tests pin the exact installed
//! key set, family order, real font bytes, and glyph coverage through the
//! production `fonts::install` seam.

use eframe::egui::{FontDefinitions, FontFamily, FontId};

use crate::support;

/// Every non-ASCII glyph that appears in a shipped `src/ui` string. The pause
/// icon is U+275A twice (ticket 67 replaced U+23F8, which DejaVu lacks).
///
/// Hand-maintained: add new UI literals here in the same change that introduces
/// them. A source scan for non-ASCII literals is deferred.
const UI_GLYPHS: &str = "×…·–²³›‹❚▶↺⚙—";

#[test]
fn epaint_bundles_no_builtin_faces() {
    assert!(
        FontDefinitions::builtin_font_names().is_empty(),
        "epaint still compiles bundled faces {:?}; remove eframe's `default_fonts` feature",
        FontDefinitions::builtin_font_names(),
    );
}

#[test]
fn the_shared_context_has_real_text_faces() {
    let ctx = support::context();
    support::run_empty_pass(&ctx);

    assert!(
        ctx.fonts_mut(|fonts| fonts.has_glyph(&FontId::proportional(14.0), 'A')),
        "support::context() must install the shipped faces, not lay out tofu",
    );
}

#[test]
fn installs_exactly_the_three_vendored_faces() {
    let definitions = vector_field_explorer::fonts::font_definitions();
    let mut names: Vec<&str> = definitions.font_data.keys().map(String::as_str).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        ["DejaVu Sans", "DejaVu Sans Mono", "egui-icons"],
        "the installed face set must be exactly the vendored three",
    );
    for forbidden in ["Ubuntu-Light", "Hack"] {
        assert!(
            !definitions.font_data.contains_key(forbidden),
            "`{forbidden}` must not be installed; it is UFL/nonfree",
        );
    }
}

#[test]
fn families_are_in_the_ruled_order() {
    let definitions = vector_field_explorer::fonts::font_definitions();

    assert_eq!(
        definitions.families.get(&FontFamily::Proportional),
        Some(&vec![
            "DejaVu Sans".to_owned(),
            "egui-icons".to_owned(),
        ]),
        "Proportional is DejaVu Sans with the MIT icon fallback",
    );
    assert_eq!(
        definitions.families.get(&FontFamily::Monospace),
        Some(&vec![
            "DejaVu Sans Mono".to_owned(),
            "DejaVu Sans".to_owned(),
            "egui-icons".to_owned(),
        ]),
        "Monospace keeps the Sans symbol fallback, then the icon face",
    );
}

#[test]
fn every_shipped_ui_glyph_has_a_face_in_both_families() {
    let ctx = support::context();
    support::run_empty_pass(&ctx);

    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        for ch in UI_GLYPHS.chars() {
            assert!(
                ctx.fonts_mut(|fonts| fonts.has_glyph(&FontId::new(14.0, family.clone()), ch)),
                "{family:?} has no face for {ch:?} (U+{:04X})",
                ch as u32,
            );
        }
    }
}

#[test]
fn vendored_bytes_are_real_truetype_fonts() {
    let definitions = vector_field_explorer::fonts::font_definitions();
    for name in ["DejaVu Sans", "DejaVu Sans Mono", "egui-icons"] {
        let bytes = definitions
            .font_data
            .get(name)
            .unwrap_or_else(|| panic!("`{name}` is missing from font_data"))
            .bytes();
        assert!(!bytes.is_empty(), "`{name}` has no bytes");
        let signature = bytes.get(..4).unwrap_or_else(|| {
            panic!(
                "`{name}` has only {} bytes, shorter than a font signature",
                bytes.len(),
            )
        });
        assert!(
            matches!(signature, [0x00, 0x01, 0x00, 0x00] | b"OTTO" | b"true" | b"ttcf"),
            "`{name}` does not start with a TrueType/OpenType signature: {signature:02x?}",
        );
    }
}
