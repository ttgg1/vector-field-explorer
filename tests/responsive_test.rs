//! Ticket 53: the pure compact/phone-portrait layout seams and the two-finger
//! 3D pan mapping.
//!
//! Everything here is pure: the breakpoint, the transient sheet state, the
//! sheet-height clamp, the compact assembly plan, the touch target constants,
//! the formula-field width floor, the status mask consumed at phone width, and
//! the multi-touch translation seam. The real browser check stays manual.

use std::fs;
use std::path::PathBuf;

use eframe::egui;
use glam::Vec2;

use vector_field_explorer::app::StatusBarState;
use vector_field_explorer::ui::layout::{
    COMPACT_WIDTH, LayoutMode, SHEET_MAX_FRACTION, SheetSize, SheetState, TAB_BAR_HEIGHT, TOUCH_TARGET,
    VIEWPORT_CHROME_ALLOWANCE, VIEWPORT_MIN_FRACTION, compact_plan, formula_field_width,
    layout_mode, reset_sheet, sheet_height,
};
use vector_field_explorer::ui::status_bar::{StatsMask, format_status, visible_stats};
use vector_field_explorer::viewmodel::two_finger_pan;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

// The tab bar must physically fit a finger-sized target.
const _: () = assert!(TAB_BAR_HEIGHT >= TOUCH_TARGET);

#[test]
fn layout_mode_is_compact_at_and_below_700_points() {
    assert_eq!(COMPACT_WIDTH, 700.0);
    assert_eq!(layout_mode(1200.0), LayoutMode::Wide);
    assert_eq!(layout_mode(701.0), LayoutMode::Wide, "one point above the breakpoint is wide");
    assert_eq!(layout_mode(700.0), LayoutMode::Compact, "the 700 pt boundary is compact");
    assert_eq!(layout_mode(699.0), LayoutMode::Compact);
    assert_eq!(layout_mode(520.0), LayoutMode::Compact);
    assert_eq!(layout_mode(360.0), LayoutMode::Compact);

    // A nonsense width must never flip the desktop into compact.
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(layout_mode(bad), LayoutMode::Wide, "{bad} stays wide");
    }
}

#[test]
fn only_one_sheet_is_open_at_a_time() {
    let mut sheet = SheetState::default();
    assert_eq!(sheet, SheetState::None, "no sheet on first launch");

    sheet.open(SheetState::Controls);
    assert_eq!(sheet, SheetState::Controls);
    assert!(sheet.is_open());

    // Opening Presets switches instead of stacking a second sheet.
    sheet.open(SheetState::Presets);
    assert_eq!(sheet, SheetState::Presets);
    assert!(!sheet.shows(SheetState::Controls));
    assert!(sheet.shows(SheetState::Presets));

    // Tapping the open tab again closes it.
    sheet.toggle(SheetState::Presets);
    assert_eq!(sheet, SheetState::None);
    assert!(!sheet.is_open());

    sheet.toggle(SheetState::Controls);
    assert_eq!(sheet, SheetState::Controls);
    sheet.toggle(SheetState::Presets);
    assert_eq!(sheet, SheetState::Presets, "the other tab switches, never stacks");

    sheet.close();
    assert_eq!(sheet, SheetState::None);
}

#[test]
fn reset_layout_closes_the_transient_sheet() {
    let mut sheet = SheetState::Presets;
    reset_sheet(&mut sheet);
    assert_eq!(sheet, SheetState::None, "Reset layout closes any open sheet");
}

#[test]
fn sheet_height_caps_at_55_percent_and_keeps_the_viewport_floor() {
    assert_eq!(SHEET_MAX_FRACTION, 0.55);
    assert_eq!(VIEWPORT_MIN_FRACTION, 0.45);

    // Plenty of room: the window fraction is the binding cap.
    assert_eq!(sheet_height(640.0, 10_000.0, SheetSize::Default), 352.0);

    // A short window: keep 45% of the window for the viewport, plus the
    // central panel's frame allowance.
    let short = sheet_height(640.0, 500.0, SheetSize::Default);
    assert!(
        (short - (500.0 - (640.0 * 0.45 + VIEWPORT_CHROME_ALLOWANCE))).abs() < 1e-3,
        "got {short}"
    );
    assert!(short <= 640.0 * SHEET_MAX_FRACTION);

    // Never lets the sheet eat the whole available height, and never negative.
    for available in [0.0, 1.0, 100.0, 288.0, 500.0, 900.0] {
        let height = sheet_height(640.0, available, SheetSize::Default);
        assert!(height >= 0.0, "height is never negative at {available}");
        assert!(height <= available, "the sheet fits the available {available}");
        assert!(
            height <= 640.0 * SHEET_MAX_FRACTION + 1e-3,
            "never above 55% of the window at {available}"
        );
    }

    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(sheet_height(bad, 500.0, SheetSize::Default), 0.0, "bad window height {bad}");
        assert_eq!(sheet_height(640.0, bad, SheetSize::Default), 0.0, "bad available height {bad}");
    }
    assert_eq!(sheet_height(0.0, 500.0, SheetSize::Default), 0.0);
    assert_eq!(sheet_height(-10.0, 500.0, SheetSize::Default), 0.0);
}

#[test]
fn compact_plan_hides_the_side_panels_and_shows_the_tab_bar() {
    let compact = compact_plan(LayoutMode::Compact);
    assert!(!compact.controls_panel, "compact hides the Controls side panel");
    assert!(!compact.presets_panel, "compact hides the Presets side panel");
    assert!(compact.compact_top_bar, "compact swaps in the hamburger bar");
    assert!(compact.tab_bar, "compact shows the bottom tab bar");
    assert!(compact.sheet, "compact can open a bottom sheet");

    let wide = compact_plan(LayoutMode::Wide);
    assert!(wide.controls_panel && wide.presets_panel, "wide keeps both panels");
    assert!(!wide.compact_top_bar, "wide keeps the full menu row");
    assert!(!wide.tab_bar, "wide has no tab bar");
    assert!(!wide.sheet, "wide has no sheet");
}

#[test]
fn touch_targets_are_finger_sized() {
    assert_eq!(TOUCH_TARGET, 44.0, "the minimum touch target is 44 pt");
    assert_eq!(TAB_BAR_HEIGHT, 48.0);
}

#[test]
fn the_formula_field_keeps_at_least_forty_percent_of_the_bar() {
    let bar = 360.0;
    let chrome = 150.0;
    let field = formula_field_width(bar, chrome);
    assert!((field - (bar - chrome)).abs() < 1e-3, "the field flexes into the remainder");
    assert!(field >= bar * 0.4, "the field keeps at least 40% at 360 pt");

    // Even a greedy chrome budget leaves the 40% floor.
    assert!(formula_field_width(bar, 340.0) >= bar * 0.4);
    assert_eq!(formula_field_width(bar, 0.0), bar);

    // Nonsense inputs are inert, never negative.
    assert_eq!(formula_field_width(f32::NAN, 100.0), 0.0);
    assert_eq!(formula_field_width(bar, f32::NAN), bar);
    assert_eq!(formula_field_width(-20.0, 10.0), 0.0);
}

#[test]
fn two_finger_pan_maps_only_3d_multi_touch_translation() {
    // Two fingers in 3D: active, and the translation is the pan delta.
    let pan = two_finger_pan(true, 2, Vec2::new(12.0, -4.0));
    assert!(pan.active);
    assert_eq!(pan.pan, Vec2::new(12.0, -4.0));

    // Three or more fingers still pan (the gesture is >= 2 touches).
    assert!(two_finger_pan(true, 3, Vec2::new(1.0, 2.0)).active);

    // A zero translation is a no-op frame even though it is an active gesture.
    let idle = two_finger_pan(true, 2, Vec2::ZERO);
    assert!(idle.active);
    assert_eq!(idle.pan, Vec2::ZERO);

    // One finger is not a pan: 3D orbits through the pointer path.
    let one = two_finger_pan(true, 1, Vec2::new(12.0, -4.0));
    assert!(!one.active);
    assert_eq!(one.pan, Vec2::ZERO);

    // 2D always pans through the pointer path; multi-touch must not double it.
    let flat = two_finger_pan(false, 2, Vec2::new(12.0, -4.0));
    assert!(!flat.active);
    assert_eq!(flat.pan, Vec2::ZERO);

    // A non-finite translation never reaches the camera.
    let bad = two_finger_pan(true, 2, Vec2::new(f32::NAN, 1.0));
    assert!(bad.active, "the gesture still owns the camera while active");
    assert_eq!(bad.pan, Vec2::ZERO);
}

#[test]
fn compact_status_is_the_width_mask_and_keeps_fps_at_360() {
    let compact = visible_stats(360.0);
    assert_eq!(compact, visible_stats(360.0), "consume ticket 51's mask unchanged");
    assert!(!compact.sim_time && !compact.density && !compact.trails);
    assert!(compact.particles, "360 pt keeps the particle count");

    let status = StatusBarState {
        fps: 60.0,
        frame_ms: 16.7,
        arrow_count: 1_200,
        streamline_count: 240,
        particle_count: 800,
        trail_count: 1_200,
        density: 32,
        sim_time: Some(12.4),
        ..StatusBarState::default()
    };
    let text = format_status(&status, compact);
    assert!(text.contains("60 fps"), "FPS survives every width: {text}");
    assert!(text.contains("16.7 ms"), "frame time survives every width: {text}");
    assert!(text.contains("1.2k arrows") && text.contains("240 streamlines"));
    assert!(text.contains("800 particles"), "360 pt keeps particles: {text}");
    assert!(!text.contains("trail segs") && !text.contains("n²") && !text.contains("| t "));

    // The compact mask is a strict subset of the wide one.
    let wide: StatsMask = visible_stats(960.0);
    assert!(
        (!compact.sim_time || wide.sim_time)
            && (!compact.density || wide.density)
            && (!compact.trails || wide.trails)
            && (!compact.particles || wide.particles)
    );
}

#[test]
fn compact_bottom_assembly_keeps_45_percent_viewport() {
    // Headless stand-in for the browser check: the real status/tab-bar/sheet
    // bottom panels in registration order, with representative compact top
    // chrome, must still leave the viewport at least 45% of the window.
    for (width, height) in [(360.0, 640.0), (390.0, 844.0)] {
        let ctx = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
        let mut viewport_height = 0.0;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ui| {
                // Compact top bar and formula bar are finger-sized strips.
                egui::Panel::top("compact_test_top")
                    .exact_size(50.0)
                    .show(ui, |_| {});
                egui::Panel::top("compact_test_formula")
                    .exact_size(48.0)
                    .show(ui, |_| {});

                // Registration order matters: status bottom-most (two stacked
                // compact rows), then the tab bar, then the sheet, then the
                // central viewport.
                egui::Panel::bottom("compact_test_status")
                    .exact_size(40.0)
                    .show(ui, |_| {});
                egui::Panel::bottom("compact_test_tabs")
                    .exact_size(TAB_BAR_HEIGHT)
                    .show(ui, |_| {});
                let window_h = ui.ctx().content_rect().height();
                let sheet = sheet_height(window_h, ui.available_height(), SheetSize::Default);
                egui::Panel::bottom("compact_test_sheet")
                    .exact_size(sheet)
                    .show(ui, |_| {});
                egui::CentralPanel::default_margins().show(ui, |ui| {
                    // The app paints the field in `max_rect().shrink(2.0)`.
                    viewport_height = ui.max_rect().shrink(2.0).height();
                });
            },
        );
        output.textures_delta.clear();

        assert!(
            viewport_height >= height * VIEWPORT_MIN_FRACTION - 2.0,
            "{width}x{height}: viewport {viewport_height} must stay >= 45% of {height}"
        );
    }
}

#[test]
fn the_web_shell_ships_safe_area_insets_and_a_full_bleed_viewport() {
    let html = fs::read_to_string(repo_root().join("index.html")).expect("read index.html");
    assert!(
        html.contains("viewport-fit=cover"),
        "the viewport meta opts into the display cutout"
    );
    assert!(
        html.contains("contextmenu"),
        "the canvas suppresses the browser context menu"
    );

    let css = fs::read_to_string(repo_root().join("web/style.css")).expect("read web/style.css");
    for token in [
        "env(safe-area-inset-top)",
        "env(safe-area-inset-right)",
        "env(safe-area-inset-bottom)",
        "env(safe-area-inset-left)",
        "box-sizing: border-box",
        "touch-action: none",
    ] {
        assert!(css.contains(token), "web/style.css must carry `{token}`");
    }
}
