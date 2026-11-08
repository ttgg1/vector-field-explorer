//! Screenshot export (plan task 13, ticket 08): render the viewport exactly as
//! it looks now and write it to a PNG through a native save dialog.
//!
//! Layering, so the headless gates can test everything that is not a GPU:
//! - this module owns the pure contract — the default file name, the capture
//!   size clamp, the 256-byte row padding, and the RGBA byte order (ruling:
//!   screenshots are RGBA, never BGRA — the same pin the preset PNGs carry);
//! - the GPU pass itself is one thin method,
//!   [`crate::renderer::DrawState::render_offscreen`], verified by the runtime
//!   smoke gate because `cargo test` never creates a device;
//! - the save dialog and the file write run on a spawned thread so the egui
//!   frame never blocks on native UI; completion arrives over a channel and
//!   shows as a toast in the sidebar's Export block.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use eframe::egui;
use eframe::wgpu::TextureFormat;

/// Largest edge of a capture, pixels. A bigger viewport is scaled down on both
/// axes so the framing (and the camera aspect) stays what the user sees.
pub const MAX_CAPTURE_EDGE: u32 = 4096;

/// How long a completion toast stays up.
pub const TOAST_TTL: Duration = Duration::from_secs(6);

/// The bytes `copy_texture_to_buffer` requires per row (256-byte alignment).
const COPY_ROW_ALIGNMENT: u32 = 256;

/// What the saver thread reports back to the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveOutcome {
    /// The PNG was written to this path.
    Saved(PathBuf),
    /// The user closed the dialog without choosing a path.
    Cancelled,
    /// The capture could not be written (encoder or filesystem error).
    Failed(String),
}

/// The capture size for a viewport of `width_points`×`height_points` logical
/// points at `pixels_per_point`: unchanged while it fits, otherwise both axes
/// scaled by the same factor so neither exceeds `cap`. The logical size is
/// converted to **physical pixels** first — egui rects are in points, the
/// viewport is painted in pixels, so a 2× display must capture twice the
/// pixels (HiDPI fix round 1). Scaling both axes keeps the aspect ratio — and
/// therefore the camera framing — identical to what is on screen.
pub fn capture_size(
    width_points: f32,
    height_points: f32,
    pixels_per_point: f32,
    cap: u32,
) -> (u32, u32) {
    // A nonsense scale factor (0, negative, NaN, inf) must not collapse the
    // capture to nothing: fall back to one pixel per point.
    let scale_factor = if pixels_per_point.is_finite() && pixels_per_point > 0.0 {
        f64::from(pixels_per_point)
    } else {
        1.0
    };
    let cap = f64::from(cap.max(1));
    let to_pixels = |points: f32| -> f64 {
        let pixels = f64::from(points) * scale_factor;
        if pixels.is_finite() { pixels.round().max(1.0) } else { 1.0 }
    };
    let width = to_pixels(width_points);
    let height = to_pixels(height_points);
    let longest = width.max(height);
    if longest <= cap {
        return (width as u32, height as u32);
    }
    let down_scale = cap / longest;
    (
        ((width * down_scale).round() as u32).max(1),
        ((height * down_scale).round() as u32).max(1),
    )
}

/// Bytes per row the readback buffer must be laid out with for RGBA8 pixels:
/// wgpu's `copy_texture_to_buffer` requires multiples of 256 bytes.
pub fn padded_row_bytes(width: u32) -> u32 {
    (width * 4).div_ceil(COPY_ROW_ALIGNMENT) * COPY_ROW_ALIGNMENT
}

/// Drop the copy's row padding, leaving exactly `width * height * 4` bytes.
pub fn strip_row_padding(readback: &[u8], width: u32, height: u32) -> Vec<u8> {
    let row = (width * 4) as usize;
    let padded = padded_row_bytes(width) as usize;
    let mut out = Vec::with_capacity(row * height as usize);
    if padded == row {
        let end = (row * height as usize).min(readback.len());
        out.extend_from_slice(&readback[..end]);
        return out;
    }
    for y in 0..height as usize {
        let start = y * padded;
        if start >= readback.len() {
            break;
        }
        let end = (start + row).min(readback.len());
        out.extend_from_slice(&readback[start..end]);
    }
    out
}

/// Whether a readback in `format` can be normalised to the PNG contract, i.e.
/// one of the 8-bit RGBA/BGRA flavours [`to_rgba`] understands.
///
/// Kept as the shared predicate for the whole screenshot path: the GPU capture
/// calls it *before* creating a texture or a readback buffer, because a format
/// with a different bytes-per-texel would make the stride/buffer sizing invalid
/// and trip wgpu validation instead of returning the clean error.
pub fn png_readback_check(format: TextureFormat) -> Result<(), String> {
    match format {
        TextureFormat::Rgba8Unorm
        | TextureFormat::Rgba8UnormSrgb
        | TextureFormat::Bgra8Unorm
        | TextureFormat::Bgra8UnormSrgb => Ok(()),
        other => Err(format!("unsupported surface format {other:?} for screenshots")),
    }
}

/// Normalise a readback to the PNG byte order contract: RGBA, never BGRA.
///
/// Ruling (ticket 08): screenshots carry the same RGBA contract as the preset
/// PNGs. The offscreen pass borrows the surface's own format (the pipelines
/// were built for it), so on a BGRA surface the readback is swizzled here
/// instead of silently writing red as blue. A format the PNG path cannot
/// describe is an error, not a guess.
pub fn to_rgba(format: TextureFormat, mut pixels: Vec<u8>) -> Result<Vec<u8>, String> {
    png_readback_check(format)?;
    match format {
        TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb => Ok(pixels),
        TextureFormat::Bgra8Unorm | TextureFormat::Bgra8UnormSrgb => Ok(pixels
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .flat_map(|pixel| [pixel[2], pixel[1], pixel[0], pixel[3]])
            .collect()),
        // Unreachable: `png_readback_check` rejected everything else.
        other => Err(format!("unsupported surface format {other:?} for screenshots")),
    }
}

/// Days since 1970-01-01 → civil (year, month, day) in the proleptic Gregorian
/// calendar (Howard Hinnant's `civil_from_days`). One date stamp does not
/// justify a calendar dependency.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if month <= 2 { y + 1 } else { y }, month, day)
}

/// The UTC calendar date of `now` as `YYYY-MM-DD`.
pub fn date_stamp(now: SystemTime) -> String {
    let secs = now
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (year, month, day) = civil_from_days(secs.div_euclid(86_400));
    format!("{year:04}-{month:02}-{day:02}")
}

/// The save dialog's opening name: `vf-YYYY-MM-DD-<mode>.png`.
///
/// `now` is a parameter (not a hidden `SystemTime::now()`) so the stamp is
/// testable, and the mode label is lowercased with any non-alphanumeric run
/// folded to a single `-`, so "2D" gives the brief's `vf-2026-11-05-2d.png`.
pub fn default_filename(now: SystemTime, mode_label: &str) -> String {
    let mut mode = String::new();
    for ch in mode_label.chars() {
        if ch.is_ascii_alphanumeric() {
            mode.extend(ch.to_lowercase());
        } else if !mode.is_empty() && !mode.ends_with('-') {
            mode.push('-');
        }
    }
    let mode = mode.trim_end_matches('-');
    let mode = if mode.is_empty() { "view" } else { mode };
    format!("vf-{}-{}.png", date_stamp(now), mode)
}

/// Make sure the dialog's answer is written as what it is: a PNG. A missing or
/// foreign extension is replaced, so a typed "shot.jpg" never receives PNG
/// bytes under a JPEG name.
pub fn ensure_png_extension(path: PathBuf) -> PathBuf {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some(ext) if ext.eq_ignore_ascii_case("png") => path,
        _ => path.with_extension("png"),
    }
}

/// Write a captured frame, reporting success/failure back to the UI. The path
/// goes to the encoder as a `Path`, exactly as the dialog returned it.
fn save_capture(rgba: &[u8], width: u32, height: u32, path: PathBuf) -> SaveOutcome {
    let path = ensure_png_extension(path);
    match crate::export::save_png(rgba, width, height, &path) {
        Ok(()) => SaveOutcome::Saved(path),
        Err(error) => SaveOutcome::Failed(error.to_string()),
    }
}

/// Show the native save dialog on a worker thread and write the PNG there.
///
/// The dialog and the file write both block, so neither may run on the egui
/// frame: the thread owns the capture bytes and reports its [`SaveOutcome`]
/// over `tx`. `ctx` is only used to wake the UI so the toast appears at once.
pub fn spawn_save(
    tx: Sender<SaveOutcome>,
    ctx: egui::Context,
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    suggested: String,
) {
    std::thread::spawn(move || {
        let outcome = match rfd::FileDialog::new()
            .set_title("Save screenshot")
            .set_file_name(&suggested)
            .add_filter("PNG image", &["png"])
            .save_file()
        {
            Some(path) => save_capture(&rgba, width, height, path),
            None => SaveOutcome::Cancelled,
        };
        let _ = tx.send(outcome);
        ctx.request_repaint();
    });
}

/// A short-lived completion message.
struct Toast {
    text: String,
    failed: bool,
    until: Instant,
}

/// The screenshot action's UI state: a pending request, the channel the saver
/// thread reports on, and the current toast.
pub struct Screenshot {
    pending: bool,
    tx: Sender<SaveOutcome>,
    rx: Receiver<SaveOutcome>,
    toast: Option<Toast>,
}

impl Screenshot {
    pub fn new() -> Self {
        let (tx, rx) = channel();
        Self::with_channel(tx, rx)
    }

    /// Test seam: the caller owns both channel ends, so a test can play the
    /// finisher's part and watch the toast lifecycle without a dialog.
    pub fn with_channel(tx: Sender<SaveOutcome>, rx: Receiver<SaveOutcome>) -> Self {
        Self {
            pending: false,
            tx,
            rx,
            toast: None,
        }
    }

    /// Record a click. The capture itself happens in the viewport, where this
    /// frame's exact rect and camera are known.
    pub fn request(&mut self) {
        self.pending = true;
    }

    /// Consume the click; true means "capture this frame".
    pub fn take_request(&mut self) -> bool {
        std::mem::take(&mut self.pending)
    }

    /// A sender for the saver thread spawned by this capture.
    pub fn sender(&self) -> Sender<SaveOutcome> {
        self.tx.clone()
    }

    /// Drain the saver thread's report and expire the toast.
    pub fn tick(&mut self, now: Instant) {
        loop {
            match self.rx.try_recv() {
                Ok(outcome) => self.set_outcome(outcome, now),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }
        if let Some(toast) = &self.toast
            && now >= toast.until
        {
            self.toast = None;
        }
    }

    /// Report a capture that failed before the thread ever started (for
    /// example a surface format the PNG path cannot describe).
    pub fn fail(&mut self, message: String) {
        self.set_outcome(SaveOutcome::Failed(message), Instant::now());
    }

    /// Turn a report into the toast text the sidebar shows.
    fn set_outcome(&mut self, outcome: SaveOutcome, now: Instant) {
        let (text, failed) = match outcome {
            SaveOutcome::Saved(path) => (format!("saved {}", path.display()), false),
            SaveOutcome::Cancelled => ("screenshot cancelled".to_string(), false),
            SaveOutcome::Failed(error) => (format!("screenshot failed: {error}"), true),
        };
        self.toast = Some(Toast {
            text,
            failed,
            until: now + TOAST_TTL,
        });
    }

    /// The current toast text, if one is up.
    pub fn toast_text(&self) -> Option<&str> {
        self.toast.as_ref().map(|toast| toast.text.as_str())
    }

    /// The sidebar's Export block: the capture button plus the completion
    /// toast underneath it.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("Export").default_open(true).show(ui, |ui| {
            let clicked = ui
                .button("Screenshot (PNG)")
                .on_hover_text("Save the viewport exactly as it looks now")
                .clicked();
            if clicked {
                self.request();
            }
            if let Some(toast) = &self.toast {
                if toast.failed {
                    ui.colored_label(egui::Color32::from_rgb(230, 70, 70), toast.text.clone());
                } else {
                    ui.weak(toast.text.clone());
                }
                // Keep repainting while a toast is up so it disappears on time
                // even when nothing else asks for frames.
                ui.ctx()
                    .request_repaint_after(Duration::from_millis(250));
            }
        });
    }
}

impl Default for Screenshot {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

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
    fn screenshots_are_rgba_never_bgra() {        // Ruling (ticket 08): the PNG byte order contract is RGBA — the same
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
}
