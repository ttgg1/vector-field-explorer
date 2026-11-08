//! Ticket 25: text must render without relying on system fonts.
//!
//! `eframe` is built with `default-features = false`, so text only appears when
//! its `default_fonts` feature is on: that is what makes epaint bundle Hack,
//! Ubuntu-Light and the egui icon face. The hosted CI runner has no system
//! fonts, so a build that enables only `system_fonts` opens a textless window,
//! which the smoke test must never call a pass.

use eframe::egui::{FontDefinitions, FontFamily};

#[test]
fn bundles_faces_for_both_text_families() {
    let definitions = FontDefinitions::default();

    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        let faces = definitions
            .families
            .get(&family)
            .unwrap_or_else(|| panic!("{family:?} family is missing"));
        assert!(
            !faces.is_empty(),
            "{family:?} has no font faces; enable eframe's `default_fonts` feature",
        );
    }
}

#[test]
fn ships_at_least_one_builtin_face() {
    let definitions = FontDefinitions::default();

    assert!(
        !FontDefinitions::builtin_font_names().is_empty(),
        "epaint bundles no builtin fonts; enable eframe's `default_fonts` feature",
    );
    assert!(
        definitions.font_data.contains_key("Hack"),
        "the bundled Hack face is missing from font_data",
    );
}
