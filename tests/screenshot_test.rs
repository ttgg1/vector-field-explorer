//! Screenshot-export coverage: file naming, capture sizing, readback padding,
//! RGBA byte order and the toast lifecycle.
//!
//! Moved out of `src/screenshot.rs`'s inline module (ticket 13) without changing
//! a test or an assertion. Every case used the module's public contract; the
//! GPU capture itself stays covered by the runtime smoke gate, as before.

use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use eframe::wgpu::TextureFormat;
use vector_field_explorer::screenshot::{
    MAX_CAPTURE_EDGE, SaveOutcome, Screenshot, TOAST_TTL, capture_size, date_stamp,
    default_filename, ensure_png_extension, padded_row_bytes, png_readback_check,
    strip_row_padding, to_rgba,
};

/// 2026-11-05T12:00:00Z, a fixed "now" so the stamp is testable.
fn nov_5_2026() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_793_880_000)
}

#[test]
fn default_filename_stamps_the_day_and_mode() {
    assert_eq!(default_filename(nov_5_2026(), "2D"), "vf-2026-11-05-2d.png");
    assert_eq!(default_filename(nov_5_2026(), "3D"), "vf-2026-11-05-3d.png");
    assert_eq!(
        default_filename(nov_5_2026(), "Orbit  View"),
        "vf-2026-11-05-orbit-view.png",
        "the mode label is lowercased and separator-safe",
    );
}

#[test]
fn date_stamp_is_utc_civil_date() {
    assert_eq!(date_stamp(UNIX_EPOCH), "1970-01-01");
    assert_eq!(date_stamp(nov_5_2026()), "2026-11-05");
    assert_eq!(
        date_stamp(UNIX_EPOCH + Duration::from_secs(1_790_000_000)),
        "2026-09-21",
    );
    assert_eq!(
        date_stamp(UNIX_EPOCH + Duration::from_secs(951_782_400)),
        "2000-02-29",
        "leap day survives the civil-date maths",
    );
}

#[test]
fn capture_size_keeps_the_aspect_and_the_4096_cap() {
    let cap = MAX_CAPTURE_EDGE;
    assert_eq!(
        capture_size(800.0, 600.0, 1.0, cap),
        (800, 600),
        "small viewports capture 1:1",
    );
    assert_eq!(
        capture_size(4096.0, 4096.0, 1.0, cap),
        (4096, 4096),
        "exactly at the cap",
    );
    assert_eq!(
        capture_size(8192.0, 4096.0, 1.0, cap),
        (4096, 2048),
        "oversized captures scale both axes so the framing is unchanged",
    );
    assert_eq!(capture_size(8192.0, 100.0, 1.0, cap), (4096, 50));
    assert_eq!(
        capture_size(0.0, 0.0, 1.0, cap),
        (1, 1),
        "a degenerate rect still captures one pixel",
    );
}

#[test]
fn capture_size_uses_physical_pixels_on_hidpi() {
    let cap = MAX_CAPTURE_EDGE;
    // egui rects are logical points; a 2x display renders twice the pixels.
    assert_eq!(
        capture_size(400.0, 300.0, 2.0, cap),
        (800, 600),
        "a 400x300-point viewport on a 2x display captures 800x600 pixels",
    );
    assert_eq!(
        capture_size(400.0, 300.0, 2.0, cap),
        (800, 600),
        "and so does a second capture: the conversion is stable",
    );
    assert_eq!(
        capture_size(123.4, 56.7, 2.0, cap),
        (247, 113),
        "points are scaled then rounded to whole pixels",
    );
    // The cap applies AFTER the point→pixel conversion.
    assert_eq!(
        capture_size(4096.0, 2048.0, 2.0, cap),
        (4096, 2048),
        "a 8192x4096-pixel viewport scales down to the 4096 cap",
    );
    assert_eq!(
        capture_size(5000.0, 2500.0, 1.5, cap),
        (4096, 2048),
        "1.5x keeps the aspect at the cap: 7500x3750 -> 4096x2048",
    );
    assert_eq!(
        capture_size(400.0, 300.0, 0.0, cap),
        (400, 300),
        "a nonsense scale factor falls back to 1x instead of capturing nothing",
    );
    assert_eq!(
        capture_size(400.0, 300.0, f32::NAN, cap),
        (400, 300),
        "a NaN scale factor falls back to 1x too",
    );
}

#[test]
fn rows_are_padded_to_the_256_byte_copy_alignment() {
    assert_eq!(padded_row_bytes(64), 256, "64 RGBA pixels are exactly 256 bytes");
    assert_eq!(padded_row_bytes(1), 256);
    assert_eq!(padded_row_bytes(500), 2048, "500*4 = 2000 rounds up to 2048");
    assert_eq!(padded_row_bytes(4096), 16384, "the cap itself is aligned");
}

#[test]
fn strip_row_padding_returns_exactly_the_image_bytes() {
    // Two 1-pixel-wide rows: 4 payload bytes then 252 padding bytes each.
    let mut readback = Vec::new();
    for value in [10u8, 20u8] {
        readback.extend_from_slice(&[value, value, value, 255]);
        readback.extend(std::iter::repeat_n(0u8, 252));
    }
    let stripped = strip_row_padding(&readback, 1, 2);
    assert_eq!(stripped, vec![10, 10, 10, 255, 20, 20, 20, 255]);
    assert_eq!(stripped.len(), 2 * 4, "only the payload survives");
}

#[test]
fn strip_row_padding_is_a_no_op_when_rows_are_already_aligned() {
    let payload: Vec<u8> = (0..64 * 4).map(|i| i as u8).collect();
    assert_eq!(strip_row_padding(&payload, 64, 1), payload);
}

#[test]
fn the_early_format_gate_matches_the_converter() {
    // Fix round 1, finding 2: the GPU path calls `png_readback_check`
    // BEFORE creating a texture/readback buffer, so it must reject exactly
    // what `to_rgba` rejects and with the same message — otherwise a
    // 16-float surface would trip wgpu validation instead of returning Err.
    for supported in [
        TextureFormat::Rgba8Unorm,
        TextureFormat::Rgba8UnormSrgb,
        TextureFormat::Bgra8Unorm,
        TextureFormat::Bgra8UnormSrgb,
    ] {
        assert!(png_readback_check(supported).is_ok(), "{supported:?} is supported");
        assert!(to_rgba(supported, vec![1, 2, 3, 4]).is_ok());
    }
    for unsupported in [
        TextureFormat::Rgba16Float,
        TextureFormat::Rgba32Float,
        TextureFormat::Rgba8Sint,
        TextureFormat::Depth32Float,
    ] {
        let early = png_readback_check(unsupported).expect_err("the gate must reject");
        let late = to_rgba(unsupported, vec![0; 8]).expect_err("the converter must reject");
        assert_eq!(early, late, "the gate reports the converter's own error");
    }
}

#[test]
fn screenshots_are_rgba_never_bgra() {
    // Ruling (ticket 08): the PNG byte order contract is RGBA — the same
    // pin as the preset PNGs. The offscreen texture borrows the surface
    // format, so a BGRA surface is swizzled on readback rather than
    // silently producing blue-for-red screenshots.
    let rgba = vec![1, 2, 3, 4];
    assert_eq!(to_rgba(TextureFormat::Rgba8UnormSrgb, rgba.clone()).unwrap(), rgba);
    assert_eq!(
        to_rgba(TextureFormat::Bgra8UnormSrgb, rgba.clone()).unwrap(),
        vec![3, 2, 1, 4],
        "a BGRA readback is swapped back to RGBA",
    );
    assert_eq!(
        to_rgba(TextureFormat::Bgra8Unorm, rgba).unwrap(),
        vec![3, 2, 1, 4],
    );
    assert!(
        to_rgba(TextureFormat::Rgba16Float, vec![0; 8]).is_err(),
        "a format the PNG path cannot describe is reported, not guessed",
    );
}

#[test]
fn png_extension_is_forced() {
    assert_eq!(ensure_png_extension(PathBuf::from("/tmp/a.png")), PathBuf::from("/tmp/a.png"));
    assert_eq!(ensure_png_extension(PathBuf::from("/tmp/a.PNG")), PathBuf::from("/tmp/a.PNG"));
    assert_eq!(ensure_png_extension(PathBuf::from("/tmp/a")), PathBuf::from("/tmp/a.png"));
    assert_eq!(ensure_png_extension(PathBuf::from("/tmp/a.jpg")), PathBuf::from("/tmp/a.png"));
}

#[test]
fn a_click_asks_for_exactly_one_capture() {
    let mut shot = Screenshot::new();
    assert!(!shot.take_request(), "an idle app captures nothing");
    shot.request();
    assert!(shot.take_request(), "the click asks for a capture");
    assert!(!shot.take_request(), "and only one frame consumes it");
}

#[test]
fn a_successful_capture_reopens_the_slot() {
    let (tx, rx) = channel();
    let mut shot = Screenshot::with_channel(tx, rx);
    let t0 = Instant::now();

    shot.request();
    assert!(shot.take_request(), "the first capture starts");

    shot.sender().send(SaveOutcome::DownloadStarted).unwrap();
    shot.tick(t0);
    assert_eq!(shot.toast_text(), Some("screenshot download started"));

    shot.request();
    assert!(
        shot.take_request(),
        "success clears the in-flight slot for the next capture"
    );
}

#[test]
#[cfg(not(target_arch = "wasm32"))]
fn native_allows_a_second_capture_while_one_is_in_flight() {
    let (tx, rx) = channel();
    let mut shot = Screenshot::with_channel(tx, rx);

    shot.request();
    assert!(shot.take_request(), "the first capture starts");
    shot.request();
    assert!(
        shot.take_request(),
        "native keeps the old behavior: a new click is not blocked by the save dialog"
    );
}

#[test]
#[cfg(target_arch = "wasm32")]
fn wasm_ignores_a_second_capture_while_one_is_in_flight() {
    let (tx, rx) = channel();
    let mut shot = Screenshot::with_channel(tx, rx);

    shot.request();
    assert!(shot.take_request(), "the first capture starts");
    shot.request();
    assert!(
        !shot.take_request(),
        "wasm blocks a second overlapping GPU readback"
    );
}

#[test]
fn a_failed_capture_clears_the_slot_and_reports() {
    let (tx, rx) = channel();
    let mut shot = Screenshot::with_channel(tx, rx);
    let t0 = Instant::now();

    shot.request();
    assert!(shot.take_request(), "the first capture starts");
    shot.sender().send(SaveOutcome::Failed("mapping failed".into())).unwrap();
    shot.tick(t0);
    assert_eq!(shot.toast_text(), Some("screenshot failed: mapping failed"));

    shot.request();
    assert!(
        shot.take_request(),
        "failure clears the in-flight slot for a retry"
    );
}

#[test]
fn a_failure_before_the_async_completion_clears_the_slot() {
    let (tx, rx) = channel();
    let mut shot = Screenshot::with_channel(tx, rx);

    shot.request();
    assert!(shot.take_request(), "the capture starts");
    shot.fail("unsupported surface format".into());
    assert_eq!(
        shot.toast_text(),
        Some("screenshot failed: unsupported surface format")
    );

    shot.request();
    assert!(shot.take_request(), "an immediate failure does not wedge the slot");
}

#[test]
fn completion_lands_as_a_toast_and_expires() {
    let (tx, rx) = channel();
    let mut shot = Screenshot::with_channel(tx, rx);
    let t0 = Instant::now();
    shot.sender().send(SaveOutcome::Cancelled).unwrap();
    shot.tick(t0);
    assert_eq!(shot.toast_text(), Some("screenshot cancelled"));
    shot.tick(t0 + TOAST_TTL / 2);
    assert_eq!(shot.toast_text(), Some("screenshot cancelled"), "still up");
    shot.tick(t0 + TOAST_TTL + Duration::from_millis(1));
    assert_eq!(shot.toast_text(), None, "the toast clears after its TTL");
}

#[test]
fn a_browser_download_reports_its_start_without_a_file_path() {
    let (tx, rx) = channel();
    let mut shot = Screenshot::with_channel(tx, rx);
    let t0 = Instant::now();

    shot.sender().send(SaveOutcome::DownloadStarted).unwrap();
    shot.tick(t0);

    assert_eq!(shot.toast_text(), Some("screenshot download started"));
}

#[test]
fn a_failure_is_reported_with_the_encoder_error() {
    let (tx, rx) = channel();
    let mut shot = Screenshot::with_channel(tx, rx);
    let t0 = Instant::now();
    let path = PathBuf::from("/tmp/vf-shot.png");
    shot.sender().send(SaveOutcome::Saved(path.clone())).unwrap();
    shot.tick(t0);
    assert_eq!(shot.toast_text(), Some("saved /tmp/vf-shot.png"));
    shot.sender().send(SaveOutcome::Failed("disk full".into())).unwrap();
    shot.tick(t0);
    assert_eq!(shot.toast_text(), Some("screenshot failed: disk full"));
}
