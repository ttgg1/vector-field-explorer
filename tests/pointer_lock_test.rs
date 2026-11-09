//! Ticket 65: the web Pointer Lock bridge and the native raw-motion seam.
//!
//! The browser surface cannot run here, so the script/include contract is
//! pinned by source guards against the files Trunk ships. The native
//! `motion_delta` path is driven headlessly through egui's event stream, which
//! is also the fallback when a platform refuses the locked grab.

use eframe::egui;

mod support;

use glam::Vec2;
use std::fs;
use std::path::PathBuf;
use vector_field_explorer::pointer_lock::{motion_delta, release, request, take_delta};

fn repo_file(relative: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn index_html_ships_the_pointer_lock_script_before_the_wasm_module() {
    let html = repo_file("index.html");
    assert!(
        html.contains(r#"<link data-trunk rel="copy-file" href="web/pointer_lock.js" />"#),
        "Trunk must copy the bridge into dist:\n{html}",
    );
    let script = html
        .find(r#"<script src="pointer_lock.js"></script>"#)
        .expect("index.html loads the bridge as a classic script");
    let wasm = html
        .find(r#"data-trunk rel="rust""#)
        .expect("index.html still loads the wasm module");
    assert!(
        script < wasm,
        "the bridge global must exist before the wasm module starts",
    );
}

#[test]
fn the_bridge_exposes_request_release_and_a_drained_delta() {
    let js = repo_file("web/pointer_lock.js");
    for token in [
        "window.vfePointerLock",
        "request",
        "release",
        "takeDelta",
        "requestPointerLock",
        "exitPointerLock",
        "pointerlockchange",
        "pointermove",
        "mousemove",
        "movementX",
        "movementY",
        "lockLost",
    ] {
        assert!(js.contains(token), "pointer_lock.js must define `{token}`:\n{js}");
    }
    assert!(
        js.contains("dx = 0") && js.contains("dy = 0"),
        "takeDelta must zero the accumulator so each delta is consumed once",
    );
}

#[test]
fn native_take_delta_is_empty_and_request_release_are_noops() {
    assert_eq!(take_delta(), Vec2::ZERO, "native has no JS bridge to drain");
    request();
    release();
    assert_eq!(take_delta(), Vec2::ZERO, "native stubs stay empty");
}

/// One egui pass carrying `events`, then this frame's native motion delta.
fn delta_for(events: Vec<egui::Event>, pixels_per_point: f32) -> Vec2 {
    let ctx = support::context();
    ctx.set_pixels_per_point(pixels_per_point);
    let input = egui::RawInput {
        events,
        ..Default::default()
    };
    let mut delta = None;
    let mut output = ctx.run_pass(input, |ctx| {
        if delta.is_none() {
            delta = Some(motion_delta(ctx));
        }
    });
    output.textures_delta.clear();
    delta.unwrap_or(Vec2::ZERO)
}

#[test]
fn native_motion_delta_sums_raw_mouse_moves_in_points() {
    let delta = delta_for(
        vec![
            egui::Event::MouseMoved(egui::vec2(10.0, -4.0)),
            egui::Event::MouseMoved(egui::vec2(2.0, 1.0)),
            // Pointer events are absolute/synthetic and must not double-count.
            egui::Event::PointerMoved(egui::pos2(500.0, 300.0)),
        ],
        1.0,
    );
    assert_eq!(delta, Vec2::new(12.0, -3.0), "raw deltas are summed once per frame");
}

#[test]
fn native_motion_delta_scales_hidpi_pixels_to_points() {
    let delta = delta_for(vec![egui::Event::MouseMoved(egui::vec2(10.0, -4.0))], 2.0);
    assert_eq!(delta, Vec2::new(5.0, -2.0), "physical raw motion is divided by pixels-per-point");
}

#[test]
fn native_motion_delta_ignores_non_finite_values() {
    let delta = delta_for(
        vec![egui::Event::MouseMoved(egui::vec2(f32::NAN, 3.0))],
        1.0,
    );
    assert!(delta.is_finite(), "a non-finite raw delta must not poison the look: {delta:?}");
}
