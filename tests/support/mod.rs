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
