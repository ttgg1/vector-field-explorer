//! Shared headless egui context for integration tests.
//!
//! Ticket 67 removed eframe's bundled fonts, so a bare `Context::default()`
//! has empty font families and lays out synthetic tofu. Tests that measure or
//! assert real text use this helper, which installs the shipped faces through
//! the same production seam the app uses.

use eframe::egui;

/// A headless context with the shipped faces installed.
pub fn context() -> egui::Context {
    let ctx = egui::Context::default();
    vector_field_explorer::fonts::install(&ctx);
    ctx
}

/// Run one empty 320×200 pass and discard the returned texture delta.
///
/// Tests call this once after [`context`] so font layout is materialised.
/// Shared module: only the font tests use this, so the other crates that
/// include `support` would otherwise warn on it.
#[allow(dead_code)]
pub fn run_empty_pass(ctx: &egui::Context) {
    let mut output = ctx.run_pass(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(320.0, 200.0),
            )),
            ..Default::default()
        },
        |_| {},
    );
    output.textures_delta.clear();
}
