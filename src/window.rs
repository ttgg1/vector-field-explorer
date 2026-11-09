//! The app window's startup options (ticket 14): a desktop-sized, centred,
//! persistence-backed window instead of eframe's small default.
//!
//! Kept as a pure function so the sizes and flags can be unit-tested without
//! opening a window; `main` is the only caller.

#[cfg(not(target_arch = "wasm32"))]
use eframe::egui;

/// The window title. The menu bar and panels never change it.
pub const WINDOW_TITLE: &str = "Vector Field Explorer";

/// Inner size (logical points) the app opens at.
pub const DEFAULT_INNER_SIZE: [f32; 2] = [1280.0, 800.0];

/// Smallest inner size the window may be resized to.
pub const MIN_INNER_SIZE: [f32; 2] = [960.0, 600.0];

/// The startup options: large centred window, resizable, with eframe's window
/// persistence enabled so the last size/position survives a restart.
#[cfg(not(target_arch = "wasm32"))]
pub fn window_options() -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(WINDOW_TITLE)
            .with_inner_size(DEFAULT_INNER_SIZE)
            .with_min_inner_size(MIN_INNER_SIZE)
            .with_resizable(true),
        centered: true,
        persist_window: true,
        ..Default::default()
    }
}
