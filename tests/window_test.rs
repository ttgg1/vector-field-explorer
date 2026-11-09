// Moved out of `src/window.rs`'s inline module (ticket-14 review I-4). The
// constant-restating asserts were dropped (review M-5): checking the built
// `NativeOptions` already pins the behaviour.

use eframe::egui;
use vector_field_explorer::window::{DEFAULT_INNER_SIZE, MIN_INNER_SIZE, WINDOW_TITLE, window_options};

#[test]
fn opens_at_the_desktop_default_inner_size() {
    let options = window_options();
    assert_eq!(
        options.viewport.inner_size,
        Some(egui::vec2(DEFAULT_INNER_SIZE[0], DEFAULT_INNER_SIZE[1])),
        "the window opens at the desktop default size",
    );
}

#[test]
fn refuses_to_shrink_below_the_minimum() {
    let options = window_options();
    assert_eq!(
        options.viewport.min_inner_size,
        Some(egui::vec2(MIN_INNER_SIZE[0], MIN_INNER_SIZE[1])),
        "960x600 is the smallest usable layout",
    );
}

#[test]
fn starts_centred_and_resizable() {
    let options = window_options();
    assert!(options.centered, "the window starts centred on the desktop");
    assert_eq!(options.viewport.resizable, Some(true), "the window stays resizable");
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
}

#[test]
fn installs_the_mark_on_the_window_options() {
    let options = window_options();
    let icon = options
        .viewport
        .icon
        .as_ref()
        .expect("window_options must hand eframe the project mark");

    assert_eq!((icon.width, icon.height), (256, 256));
    assert_eq!(icon.rgba.len(), 256 * 256 * 4);
}
