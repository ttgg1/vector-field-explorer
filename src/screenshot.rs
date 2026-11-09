//! Screenshot export: render the viewport exactly as it looks now, encode a
//! PNG, then save through a native dialog or start a browser download.
//!
//! Layering, so the headless gates can test everything that is not a GPU:
//! - this module owns the pure contract — the default file name, the capture
//!   size clamp, the 256-byte row padding, and the RGBA byte order (ruling:
//!   screenshots are RGBA, never BGRA — the same pin the preset PNGs carry);
//! - the GPU pass itself is one thin method,
//!   [`crate::renderer::DrawState::render_offscreen`], verified by the runtime
//!   smoke gate because `cargo test` never creates a device;
//! - native dialog/file work stays on a worker thread; the browser path uses
//!   DOM downloads. Completion arrives over a channel and shows as a toast.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::time::Duration;

#[cfg(not(target_arch = "wasm32"))]
use std::time::UNIX_EPOCH;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use std::time::{Instant, SystemTime};
#[cfg(target_arch = "wasm32")]
use web_time::UNIX_EPOCH;
#[cfg(target_arch = "wasm32")]
pub(crate) use web_time::{Instant, SystemTime};

use eframe::egui;
use eframe::wgpu::TextureFormat;

/// Largest edge of a capture, pixels. A bigger viewport is scaled down on both
/// axes so the framing (and the camera aspect) stays what the user sees.
///
/// Native keeps 4096. The browser path encodes in the async readback completion
/// on the main thread, so it caps at 2048²: a 4096² PNG encode is a material
/// main-thread stall and no browser was available to measure a safe threshold.
#[cfg(not(target_arch = "wasm32"))]
pub const MAX_CAPTURE_EDGE: u32 = 4096;
#[cfg(target_arch = "wasm32")]
pub const MAX_CAPTURE_EDGE: u32 = 2048;

/// How long a completion toast stays up.
pub const TOAST_TTL: Duration = Duration::from_secs(6);

/// The bytes `copy_texture_to_buffer` requires per row (256-byte alignment).
const COPY_ROW_ALIGNMENT: u32 = 256;

/// What the saver thread reports back to the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveOutcome {
    /// The PNG was written to this path.
    Saved(PathBuf),
    /// The browser accepted the generated PNG download.
    DownloadStarted,
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
#[cfg(not(target_arch = "wasm32"))]
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
#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(target_arch = "wasm32")]
fn js_error(error: impl Into<wasm_bindgen::JsValue>) -> String {
    let error = error.into();
    error.as_string().unwrap_or_else(|| format!("{error:?}"))
}

/// Start a browser download for an already encoded PNG. The temporary anchor
/// is removed immediately; the object URL stays alive for one second so the
/// browser has time to consume it.
#[cfg(target_arch = "wasm32")]
fn download_png(png: &[u8], filename: &str) -> Result<(), String> {
    use wasm_bindgen::JsCast as _;

    let window = web_sys::window().ok_or_else(|| "browser window unavailable".to_string())?;
    let document = window
        .document()
        .ok_or_else(|| "browser document unavailable".to_string())?;
    let anchor = document
        .create_element("a")
        .map_err(js_error)?
        .dyn_into::<web_sys::HtmlAnchorElement>()
        .map_err(js_error)?;

    let bytes = js_sys::Uint8Array::from(png);
    let parts = js_sys::Array::of1(bytes.as_ref());
    let options = web_sys::BlobPropertyBag::new();
    options.set_type("image/png");
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(parts.as_ref(), &options)
        .map_err(js_error)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(js_error)?;

    anchor.set_href(&url);
    anchor.set_download(filename);
    // Firefox only activates a download for a connected anchor, so append it
    // to the body; it is removed again right after the click.
    let Some(body) = document.body() else {
        let _ = web_sys::Url::revoke_object_url(&url);
        return Err("browser document body unavailable".to_string());
    };
    if let Err(error) = body.append_child(&anchor) {
        let _ = web_sys::Url::revoke_object_url(&url);
        return Err(js_error(error));
    }

    let revoke_url = url.clone();
    let callback = wasm_bindgen::closure::Closure::once_into_js(move || {
        if let Err(error) = web_sys::Url::revoke_object_url(&revoke_url) {
            log::warn!("could not revoke screenshot object URL: {}", js_error(error));
        }
    });
    if let Err(error) = window.set_timeout_with_callback_and_timeout_and_arguments_0(
        callback.unchecked_ref(),
        1_000,
    ) {
        anchor.remove();
        let _ = web_sys::Url::revoke_object_url(&url);
        return Err(js_error(error));
    }

    anchor.click();
    anchor.remove();
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn save_capture(rgba: &[u8], width: u32, height: u32, filename: &str) -> SaveOutcome {
    match crate::export::encode_png(rgba, width, height) {
        Ok(png) => match download_png(&png, filename) {
            Ok(()) => SaveOutcome::DownloadStarted,
            Err(error) => SaveOutcome::Failed(error),
        },
        Err(error) => SaveOutcome::Failed(error.to_string()),
    }
}

/// Complete a browser capture after the GPU readback has mapped. This runs on
/// the browser event loop from the wgpu map callback, never on the egui frame:
/// encode the PNG, start the download, report through the screenshot channel,
/// and wake the UI for the toast.
#[cfg(target_arch = "wasm32")]
pub(crate) fn finish_download(
    result: Result<Vec<u8>, String>,
    width: u32,
    height: u32,
    filename: String,
    tx: Sender<SaveOutcome>,
    ctx: egui::Context,
) {
    let outcome = match result {
        Ok(rgba) => save_capture(&rgba, width, height, &filename),
        Err(error) => SaveOutcome::Failed(error),
    };
    let _ = tx.send(outcome);
    ctx.request_repaint();
}

/// A short-lived completion message.
struct Toast {
    text: String,
    failed: bool,
    until: Instant,
}

/// The screenshot action's UI state: a pending request, the one-capture-in-
/// flight flag, the channel the capture reports on, and the current toast.
///
/// The in-flight flag is the pollable result state machine: `take_request`
/// moves idle → pending, and `tick`/`fail` move pending → success/failure and
/// reopen the slot. On wasm a second click while a capture is in flight is
/// ignored; native keeps its previous allow-a-second-request behavior.
pub struct Screenshot {
    pending: bool,
    capturing: bool,
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
            capturing: false,
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
    ///
    /// Wasm only: while an async capture is in flight the slot stays closed, so
    /// a second click cannot start an overlapping GPU readback. Native keeps
    /// its old behavior and allows another request while the save dialog owns
    /// the first capture.
    pub fn take_request(&mut self) -> bool {
        let requested = std::mem::take(&mut self.pending);
        if !requested || (cfg!(target_arch = "wasm32") && self.capturing) {
            return false;
        }
        self.capturing = true;
        true
    }

    /// A sender for the saver thread spawned by this capture.
    pub fn sender(&self) -> Sender<SaveOutcome> {
        self.tx.clone()
    }

    /// Drain the capture's report, clear the in-flight slot on success or
    /// failure, and expire the toast.
    pub fn tick(&mut self, now: Instant) {
        loop {
            match self.rx.try_recv() {
                Ok(outcome) => {
                    self.capturing = false;
                    self.set_outcome(outcome, now);
                }
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

    /// Report a capture that failed before the async completion ran (for
    /// example a surface format the PNG path cannot describe). Clears the
    /// in-flight slot so the next click can try again.
    pub fn fail(&mut self, message: String) {
        self.capturing = false;
        self.set_outcome(SaveOutcome::Failed(message), Instant::now());
    }

    /// Turn a report into the toast text the sidebar shows.
    fn set_outcome(&mut self, outcome: SaveOutcome, now: Instant) {
        let (text, failed) = match outcome {
            SaveOutcome::Saved(path) => {
                log::info!("screenshot saved to {}", path.display());
                (format!("saved {}", path.display()), false)
            }
            SaveOutcome::DownloadStarted => {
                log::info!("screenshot download started");
                ("screenshot download started".to_string(), false)
            }
            SaveOutcome::Cancelled => {
                log::debug!("screenshot cancelled");
                ("screenshot cancelled".to_string(), false)
            }
            SaveOutcome::Failed(error) => {
                log::warn!("screenshot failed: {error}");
                (format!("screenshot failed: {error}"), true)
            }
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

    /// Draw just the completion toast (the top menu bar owns the button that
    /// requests a capture). Keeps repainting while a toast is up so it
    /// disappears on time even when nothing else asks for frames.
    pub fn toast(&self, ui: &mut egui::Ui) {
        if let Some(toast) = &self.toast {
            if toast.failed {
                ui.colored_label(egui::Color32::from_rgb(230, 70, 70), toast.text.clone());
            } else {
                ui.weak(toast.text.clone());
            }
            ui.ctx().request_repaint_after(Duration::from_millis(250));
        }
    }
}

impl Default for Screenshot {
    fn default() -> Self {
        Self::new()
    }
}

