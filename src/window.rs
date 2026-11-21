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

/// The committed 256 px project mark, decoded into the window/taskbar icon.
///
/// A stale or corrupt asset must never keep the app from opening: a decode
/// failure logs and falls back to eframe's default icon.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn icon_data() -> Option<egui::IconData> {
    const LOGO_PNG: &[u8] = include_bytes!("../packaging/linux/vector-field-explorer-256.png");

    match image::load_from_memory(LOGO_PNG) {
        Ok(image) => {
            let rgba = image.to_rgba8();
            let (width, height) = rgba.dimensions();
            Some(egui::IconData {
                rgba: rgba.into_raw(),
                width,
                height,
            })
        }
        Err(error) => {
            log::warn!("window icon could not be decoded: {error}");
            None
        }
    }
}

/// The startup options: large centred window, resizable, with eframe's window
/// persistence enabled so the last size/position survives a restart.
#[cfg(not(target_arch = "wasm32"))]
pub fn window_options() -> eframe::NativeOptions {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(WINDOW_TITLE)
        .with_inner_size(DEFAULT_INNER_SIZE)
        .with_min_inner_size(MIN_INNER_SIZE)
        .with_resizable(true);
    if let Some(icon) = icon_data() {
        viewport = viewport.with_icon(icon);
    }

    eframe::NativeOptions {
        viewport,
        centered: true,
        persist_window: true,
        ..Default::default()
    }
}

/// Startup options for the bounded present-path measurement (#18 task b): the
/// documented 1280×800 logical inner size, not resizable, with window
/// persistence disabled so the measurement cannot rewrite the user's saved
/// window geometry.
#[cfg(not(target_arch = "wasm32"))]
pub fn window_options_present() -> eframe::NativeOptions {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(WINDOW_TITLE)
        // A distinct app id keeps compositor window rules (for example a
        // remembered size for the normal app) from overriding the forced
        // 1280x800 measurement geometry.
        .with_app_id("vector-field-explorer-present")
        .with_inner_size(DEFAULT_INNER_SIZE)
        .with_min_inner_size(DEFAULT_INNER_SIZE)
        .with_max_inner_size(DEFAULT_INNER_SIZE)
        .with_resizable(false);
    if let Some(icon) = icon_data() {
        viewport = viewport.with_icon(icon);
    }

    eframe::NativeOptions {
        viewport,
        centered: true,
        persist_window: false,
        // eframe always creates a file-backed store and restores window
        // geometry from it. On Unix `/dev/null` discards every write and
        // yields no saved geometry, so the measurement neither reads nor
        // overwrites user settings; the app itself is also built with `None`
        // storage in `main`. Other platforms get a throwaway temp file.
        persistence_path: Some(present_persistence_path()),
        ..Default::default()
    }
}

/// Where a measurement run lets eframe's own storage live.
///
/// Unix uses `/dev/null` (nothing is read back, nothing survives). Other
/// platforms use a per-process temp file: still never the user's app store.
#[cfg(not(target_arch = "wasm32"))]
fn present_persistence_path() -> std::path::PathBuf {
    #[cfg(unix)]
    {
        std::path::PathBuf::from("/dev/null")
    }
    #[cfg(not(unix))]
    {
        std::env::temp_dir().join(format!("vfe-present-{}.ron", std::process::id()))
    }
}
