//! The app window's startup options (ticket 14): a desktop-sized, centred,
//! persistence-backed window instead of eframe's small default.
//!
//! Kept as a pure function so the sizes and flags can be unit-tested without
//! opening a window; `main` is the only caller.

use eframe::egui;

/// The window title. The menu bar and panels never change it.
pub const WINDOW_TITLE: &str = "Vector Field Explorer";

/// Inner size (logical points) the app opens at.
pub const DEFAULT_INNER_SIZE: [f32; 2] = [1280.0, 800.0];

/// Smallest inner size the window may be resized to.
pub const MIN_INNER_SIZE: [f32; 2] = [960.0, 600.0];

/// The startup options: large centred window, resizable, with eframe's window
/// persistence enabled so the last size/position survives a restart.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_at_the_desktop_default_inner_size() {
        let options = window_options();
        assert_eq!(
            options.viewport.inner_size,
            Some(egui::vec2(DEFAULT_INNER_SIZE[0], DEFAULT_INNER_SIZE[1])),
            "the window opens at 1280x800 logical points",
        );
        assert_eq!(DEFAULT_INNER_SIZE, [1280.0, 800.0]);
    }

    #[test]
    fn refuses_to_shrink_below_the_minimum() {
        let options = window_options();
        assert_eq!(
            options.viewport.min_inner_size,
            Some(egui::vec2(MIN_INNER_SIZE[0], MIN_INNER_SIZE[1])),
            "960x600 is the smallest usable layout",
        );
        assert_eq!(MIN_INNER_SIZE, [960.0, 600.0]);
    }

    #[test]
    fn starts_centred_and_resizable() {
        let options = window_options();
        assert!(options.centered, "the window starts centred on the desktop");
        assert_eq!(
            options.viewport.resizable,
            Some(true),
            "the window stays resizable",
        );
    }

    #[test]
    fn remembers_its_size_and_position() {
        let options = window_options();
        assert!(options.persist_window, "eframe restores size/position on restart");
    }

    #[test]
    fn keeps_the_product_title() {
        let options = window_options();
        assert_eq!(options.viewport.title.as_deref(), Some(WINDOW_TITLE));
        assert_eq!(WINDOW_TITLE, "Vector Field Explorer");
    }
}
