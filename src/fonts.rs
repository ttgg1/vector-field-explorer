//! The shipped text faces (ticket 67).
//!
//! eframe is built without `default_fonts` so epaint's Ubuntu-Light and Hack
//! bytes are never compiled into the native or WebAssembly binary. The app
//! installs its own unmodified DejaVu faces plus the MIT `egui-icons` subset
//! through [`install`], from both the native and wasm app creators.
//!
//! Construction uses [`FontDefinitions::empty`] rather than `default`: with the
//! feature off they are equivalent today, but `empty` guarantees that
//! re-enabling `default_fonts` later cannot silently reintroduce the UFL face.
//! Families are written explicitly:
//!
//! - `Proportional`: `DejaVu Sans`, then `egui-icons` for `special_emojis`.
//! - `Monospace`: `DejaVu Sans Mono`, `DejaVu Sans` for symbols the mono face
//!   lacks, then `egui-icons`.

use std::sync::Arc;

use eframe::egui::{self, FontData, FontDefinitions, FontFamily, FontTweak};

/// Key for the primary proportional face.
pub const DEJAVU_SANS: &str = "DejaVu Sans";
/// Key for the primary monospace face.
pub const DEJAVU_SANS_MONO: &str = "DejaVu Sans Mono";
/// Key for the MIT icon subset used by `egui::special_emojis`.
pub const EGUI_ICONS: &str = "egui-icons";

static SANS: &[u8] = include_bytes!("../assets/fonts/DejaVuSans.ttf");
static MONO: &[u8] = include_bytes!("../assets/fonts/DejaVuSansMono.ttf");
static ICONS: &[u8] = include_bytes!("../assets/fonts/egui-icons.ttf");

/// The complete shipped face set and family order.
pub fn font_definitions() -> FontDefinitions {
    let mut fonts = FontDefinitions::empty();

    fonts.font_data.insert(
        DEJAVU_SANS.to_owned(),
        Arc::new(FontData::from_static(SANS)),
    );
    fonts.font_data.insert(
        DEJAVU_SANS_MONO.to_owned(),
        Arc::new(FontData::from_static(MONO)),
    );
    fonts.font_data.insert(
        EGUI_ICONS.to_owned(),
        Arc::new(FontData::from_static(ICONS).tweak(FontTweak {
            // Match the scale epaint shipped for this face.
            scale: 0.90,
            ..Default::default()
        })),
    );

    fonts.families.insert(
        FontFamily::Proportional,
        vec![DEJAVU_SANS.to_owned(), EGUI_ICONS.to_owned()],
    );
    fonts.families.insert(
        FontFamily::Monospace,
        vec![
            DEJAVU_SANS_MONO.to_owned(),
            DEJAVU_SANS.to_owned(),
            EGUI_ICONS.to_owned(),
        ],
    );

    fonts
}

/// Install the shipped faces on `ctx`.
///
/// Called from both app creators; tests call it through
/// `tests/support/mod.rs` so headless text is laid out with the real faces.
pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(font_definitions());
}
