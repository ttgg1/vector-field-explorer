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
use std::sync::mpsc::{Receiver, Sender, channel};
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
use glam::Mat4;

use crate::export_provenance::{ExportProvenance, provenance_lines, provenance_metadata};

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
        if pixels.is_finite() {
            pixels.round().max(1.0)
        } else {
            1.0
        }
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
        other => Err(format!(
            "unsupported surface format {other:?} for screenshots"
        )),
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
        other => Err(format!(
            "unsupported surface format {other:?} for screenshots"
        )),
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

/// SVG variant of [`default_filename`]: same stamp, `.svg` extension.
pub fn default_svg_filename(now: SystemTime, mode_label: &str) -> String {
    let png = default_filename(now, mode_label);
    png.strip_suffix(".png").unwrap_or(&png).to_string() + ".svg"
}

/// The save dialog's opening name: `vector-field-explorer-YYYY-MM-DD-<mode>.png`.
///
/// `now` is a parameter (not a hidden `SystemTime::now()`) so the stamp is
/// testable, and the mode label is lowercased with any non-alphanumeric run
/// folded to a single `-`, so "2D" gives the brief's
/// `vector-field-explorer-2026-11-05-2d.png`.
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
    format!("vector-field-explorer-{}-{}.png", date_stamp(now), mode)
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

/// Encode a captured frame through the historical no-provenance path or the
/// provenance-aware `tEXt` path.
#[cfg(not(target_arch = "wasm32"))]
fn encode_capture(
    rgba: &[u8],
    width: u32,
    height: u32,
    provenance: Option<&ExportProvenance>,
) -> std::io::Result<Vec<u8>> {
    match provenance {
        Some(provenance) => crate::export::encode_png_with_text(
            rgba,
            width,
            height,
            provenance_metadata(provenance),
        ),
        None => crate::export::encode_png(rgba, width, height),
    }
}

/// Write a captured frame, reporting success/failure back to the UI. The path
/// goes to the encoder as a `Path`, exactly as the dialog returned it.
#[cfg(not(target_arch = "wasm32"))]
fn save_capture(
    rgba: &[u8],
    width: u32,
    height: u32,
    path: PathBuf,
    provenance: Option<&ExportProvenance>,
) -> SaveOutcome {
    let path = ensure_png_extension(path);
    match encode_capture(rgba, width, height, provenance).and_then(|png| std::fs::write(&path, png))
    {
        Ok(()) => SaveOutcome::Saved(path),
        Err(error) => SaveOutcome::Failed(error.to_string()),
    }
}

/// A capture completion tagged with the generation that started it. Reset can
/// invalidate a cancelled generation without the worker thread racing a new
/// request on the same untagged channel.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureReport {
    pub generation: u64,
    pub outcome: SaveOutcome,
}

/// The sender handed to a PNG capture worker. It stamps every completion with
/// the capture generation so [`Screenshot::tick`] can discard results from a
/// request that Reset cancelled.
#[doc(hidden)]
#[derive(Clone)]
pub struct CaptureSink {
    tx: Sender<CaptureReport>,
    generation: u64,
}

impl CaptureSink {
    pub fn send(&self, outcome: SaveOutcome) {
        let _ = self.tx.send(CaptureReport {
            generation: self.generation,
            outcome,
        });
    }
}

/// Show the native save dialog on a worker thread and write the PNG there.
///
/// The dialog and the file write both block, so neither may run on the egui
/// frame: the thread owns the capture bytes and reports its [`SaveOutcome`]
/// through the capture-generation [`CaptureSink`]. `ctx` is only used to wake
/// the UI so the toast appears at once.
#[cfg(not(target_arch = "wasm32"))]
pub fn spawn_save(
    sink: CaptureSink,
    ctx: egui::Context,
    mut rgba: Vec<u8>,
    width: u32,
    height: u32,
    suggested: String,
    provenance: Option<ExportProvenance>,
) {
    std::thread::spawn(move || {
        let outcome = match rfd::FileDialog::new()
            .set_title("Save screenshot")
            .set_file_name(&suggested)
            .add_filter("PNG image", &["png"])
            .save_file()
        {
            Some(path) => {
                let mut banner_error = None;
                if let Some(provenance) = &provenance
                    && let Err(error) = apply_export_banner(&mut rgba, width, height, provenance)
                {
                    banner_error = Some(error);
                }
                match banner_error {
                    Some(error) => SaveOutcome::Failed(error),
                    None => save_capture(&rgba, width, height, path, provenance.as_ref()),
                }
            }
            None => SaveOutcome::Cancelled,
        };
        sink.send(outcome);
        ctx.request_repaint();
    });
}

/// Show the native save dialog on a worker thread and write the SVG there.
#[cfg(not(target_arch = "wasm32"))]
pub fn spawn_save_svg(tx: Sender<SaveOutcome>, ctx: egui::Context, svg: String, suggested: String) {
    std::thread::spawn(move || {
        let outcome = match rfd::FileDialog::new()
            .set_title("Save screenshot (SVG)")
            .set_file_name(&suggested)
            .add_filter("SVG image", &["svg"])
            .save_file()
        {
            Some(path) => {
                let path = crate::export::ensure_svg_extension(path);
                match crate::export::save_svg(&svg, &path) {
                    Ok(()) => SaveOutcome::Saved(path),
                    Err(error) => SaveOutcome::Failed(error.to_string()),
                }
            }
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
            log::warn!(
                "could not revoke screenshot object URL: {}",
                js_error(error)
            );
        }
    });
    if let Err(error) = window
        .set_timeout_with_callback_and_timeout_and_arguments_0(callback.unchecked_ref(), 1_000)
    {
        anchor.remove();
        let _ = web_sys::Url::revoke_object_url(&url);
        return Err(js_error(error));
    }

    anchor.click();
    anchor.remove();
    Ok(())
}

#[cfg(target_arch = "wasm32")]
pub fn download_svg(svg: &str, filename: &str) -> Result<(), String> {
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
    let bytes = svg.as_bytes();
    let parts = js_sys::Array::of1(js_sys::Uint8Array::from(bytes).as_ref());
    let options = web_sys::BlobPropertyBag::new();
    options.set_type("image/svg+xml");
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(parts.as_ref(), &options)
        .map_err(js_error)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(js_error)?;
    anchor.set_href(&url);
    anchor.set_download(filename);
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
        let _ = web_sys::Url::revoke_object_url(&revoke_url);
    });
    let _ = window
        .set_timeout_with_callback_and_timeout_and_arguments_0(callback.unchecked_ref(), 1_000);
    anchor.click();
    anchor.remove();
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn save_capture(
    rgba: &[u8],
    width: u32,
    height: u32,
    filename: &str,
    provenance: Option<&ExportProvenance>,
) -> SaveOutcome {
    let encoded = match provenance {
        Some(provenance) => crate::export::encode_png_with_text(
            rgba,
            width,
            height,
            provenance_metadata(provenance),
        ),
        None => crate::export::encode_png(rgba, width, height),
    };
    match encoded {
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
    sink: CaptureSink,
    ctx: egui::Context,
    provenance: Option<ExportProvenance>,
) {
    let outcome = match result {
        Ok(mut rgba) => {
            let mut banner_error = None;
            if let Some(provenance) = &provenance
                && let Err(error) = apply_export_banner(&mut rgba, width, height, provenance)
            {
                banner_error = Some(error);
            }
            match banner_error {
                Some(error) => SaveOutcome::Failed(error),
                None => save_capture(&rgba, width, height, &filename, provenance.as_ref()),
            }
        }
        Err(error) => SaveOutcome::Failed(error),
    };
    sink.send(outcome);
    ctx.request_repaint();
}

/// Project a world point through `view_proj` into screen pixels for a
/// `width`×`height` image. Returns `None` when the point is behind the
/// camera or outside the view frustum.
fn world_to_screen(
    view_proj: Mat4,
    point: [f32; 3],
    width: u32,
    height: u32,
) -> Option<(f32, f32)> {
    let v = view_proj * glam::Vec4::new(point[0], point[1], point[2], 1.0);
    if !v.w.is_finite() || v.w <= 0.0 {
        return None;
    }
    let ndc_x = v.x / v.w;
    let ndc_y = v.y / v.w;
    if !ndc_x.is_finite() || !ndc_y.is_finite() {
        return None;
    }
    // Allow a small margin so partially off-screen strokes still render.
    if ndc_x < -1.5 || ndc_x > 1.5 || ndc_y < -1.5 || ndc_y > 1.5 {
        return None;
    }
    let sx = (ndc_x + 1.0) * 0.5 * width as f32;
    let sy = (1.0 - ndc_y) * 0.5 * height as f32;
    Some((sx, sy))
}

fn color_to_hex(c: [f32; 4]) -> String {
    let r = (c[0].clamp(0.0, 1.0) * 255.0).round() as u8;
    let g = (c[1].clamp(0.0, 1.0) * 255.0).round() as u8;
    let b = (c[2].clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", r, g, b)
}

/// Approximate printable ASCII as 5×7 rows, most significant bit leftmost.
///
/// This is deliberately a tiny built-in font: the export banner must not need
/// an egui context, a font file, or a new dependency. Unknown characters render
/// as `?`.
fn bitmap_glyph(c: char) -> [u8; 7] {
    match c.to_ascii_uppercase() {
        'A' => [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'B' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
        ],
        'C' => [
            0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110,
        ],
        'D' => [
            0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110,
        ],
        'E' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
        ],
        'F' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'G' => [
            0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111,
        ],
        'H' => [
            0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'I' => [
            0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        'J' => [
            0b00111, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100,
        ],
        'K' => [
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
        'L' => [
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
        ],
        'M' => [
            0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
        ],
        'N' => [
            0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001,
        ],
        'O' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'P' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'Q' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101,
        ],
        'R' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
        'S' => [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        'T' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'U' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'V' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
        ],
        'W' => [
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010,
        ],
        'X' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001,
        ],
        'Y' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'Z' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
        ],
        '0' => [
            0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
        ],
        '1' => [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        '2' => [
            0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
        ],
        '3' => [
            0b11111, 0b00010, 0b00100, 0b00010, 0b00001, 0b10001, 0b01110,
        ],
        '4' => [
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
        ],
        '5' => [
            0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110,
        ],
        '6' => [
            0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
        ],
        '7' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ],
        '8' => [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
        '9' => [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100,
        ],
        '-' => [
            0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000,
        ],
        '=' => [
            0b00000, 0b00000, 0b11111, 0b00000, 0b11111, 0b00000, 0b00000,
        ],
        ':' => [
            0b00000, 0b00100, 0b00100, 0b00000, 0b00100, 0b00100, 0b00000,
        ],
        ',' => [
            0b00000, 0b00000, 0b00000, 0b00000, 0b00100, 0b00100, 0b01000,
        ],
        '.' => [
            0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b01100, 0b01100,
        ],
        '(' => [
            0b00010, 0b00100, 0b01000, 0b01000, 0b01000, 0b00100, 0b00010,
        ],
        ')' => [
            0b01000, 0b00100, 0b00010, 0b00010, 0b00010, 0b00100, 0b01000,
        ],
        '/' => [
            0b00001, 0b00010, 0b00010, 0b00100, 0b01000, 0b01000, 0b10000,
        ],
        '_' => [
            0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b11111,
        ],
        '+' => [
            0b00000, 0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0b00000,
        ],
        ' ' => [0; 7],
        _ => [
            0b01110, 0b10001, 0b00001, 0b00110, 0b00100, 0b00000, 0b00100,
        ],
    }
}

/// Bake the provenance warning into an RGBA frame as a dark translucent strip
/// with near-white bitmap text, bottom-left, above 4 px of padding.
///
/// The banner is part of the decoded PNG pixels, so a saved source image keeps
/// its softened-model and accuracy context even outside the app. Lines paint
/// bottom-up in provenance order and are clipped to the image width/height.
pub fn apply_export_banner(
    rgba: &mut [u8],
    width: u32,
    height: u32,
    provenance: &ExportProvenance,
) -> Result<(), String> {
    let expected = width as usize * height as usize * 4;
    if rgba.len() != expected {
        return Err(format!(
            "invalid frame size: expected {expected} bytes, got {}",
            rgba.len()
        ));
    }
    if width == 0 || height == 0 {
        return Ok(());
    }
    let lines = provenance_lines(provenance);
    if lines.is_empty() {
        return Ok(());
    }

    const SCALE: u32 = 2;
    const PADDING: u32 = 4;
    const FONT_HEIGHT: u32 = 7;
    const GLYPH_WIDTH: u32 = 5;
    const GLYPH_ADVANCE: u32 = 6;
    let line_height = FONT_HEIGHT * SCALE + 2;
    let banner_height = (lines.len() as u32)
        .saturating_mul(line_height)
        .saturating_add(PADDING)
        .min(height);
    let banner_top = height - banner_height;

    // Semi-transparent dark strip composited over whatever pixels are below.
    for y in banner_top..height {
        for x in 0..width {
            let offset = ((y * width + x) * 4) as usize;
            for channel in 0..3 {
                rgba[offset + channel] = (f32::from(rgba[offset + channel]) * 0.35).round() as u8;
            }
        }
    }

    // Text: line 0 sits at the bottom, matching "above the padding" order.
    for (line_index, line) in lines.iter().enumerate() {
        let baseline = height
            .saturating_sub(PADDING + line_index as u32 * line_height)
            .saturating_sub(FONT_HEIGHT * SCALE);
        let mut pen_x = PADDING;
        for c in line.chars() {
            if pen_x + GLYPH_WIDTH * SCALE > width.saturating_sub(PADDING) {
                break;
            }
            let glyph = bitmap_glyph(c);
            for (row, bits) in glyph.iter().enumerate() {
                for col in 0..GLYPH_WIDTH {
                    if bits & (1 << (GLYPH_WIDTH - 1 - col)) == 0 {
                        continue;
                    }
                    for dy in 0..SCALE {
                        for dx in 0..SCALE {
                            let x = pen_x + col * SCALE + dx;
                            let y = baseline + row as u32 * SCALE + dy;
                            if x < width && y < height {
                                let offset = ((y * width + x) * 4) as usize;
                                rgba[offset..offset + 4].copy_from_slice(&[255, 255, 255, 255]);
                            }
                        }
                    }
                }
            }
            pen_x += GLYPH_ADVANCE * SCALE;
        }
    }

    Ok(())
}

/// Escape text for XML content/attributes.
fn xml_escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            other => escaped.push(other),
        }
    }
    escaped
}

/// Convert a world-unit line width into screen pixels from the projected
/// endpoint span. Falls back to the historical `max(0.5)` when the world span
/// is zero or either endpoint cannot be projected.
fn line_stroke_width(
    line: &crate::renderer::LineInstanceGPU,
    view_proj: Mat4,
    width: u32,
    height: u32,
) -> f32 {
    let fallback = line.b[3].max(0.5);
    let world = ((line.b[0] - line.a[0]).powi(2)
        + (line.b[1] - line.a[1]).powi(2)
        + (line.b[2] - line.a[2]).powi(2))
    .sqrt();
    if !world.is_finite() || world <= 0.0 {
        return fallback;
    }
    let Some((x1, y1)) =
        world_to_screen(view_proj, [line.a[0], line.a[1], line.a[2]], width, height)
    else {
        return fallback;
    };
    let Some((x2, y2)) =
        world_to_screen(view_proj, [line.b[0], line.b[1], line.b[2]], width, height)
    else {
        return fallback;
    };
    let pixels = ((x2 - x1).powi(2) + (y2 - y1).powi(2)).sqrt();
    let scale = pixels / world;
    if !scale.is_finite() || scale <= 0.0 {
        return fallback;
    }
    (line.b[3].abs() * scale).clamp(0.5, 3.0)
}

/// Serialize a 2D scene as an SVG document. `view_proj` is the active
/// camera's view-projection matrix; `width`/`height` are the output pixel
/// dimensions. Arrows, streamlines, trails and particles are emitted as
/// vector elements with their colormap colour baked in.
///
/// Size risk: a dense scene can produce tens of thousands of elements and a
/// multi-megabyte file. When the element count exceeds [`SVG_ELEMENT_CAP`] a
/// warning is logged (and the caller is expected to surface it to the user);
/// the export is never truncated or silently dropped.
pub fn scene_to_svg(
    scene: &crate::renderer::SceneGPU,
    view_proj: Mat4,
    width: u32,
    height: u32,
    clear: [f64; 4],
) -> String {
    scene_to_svg_with_provenance(scene, view_proj, width, height, clear, None)
}

/// Serialize a 2D scene as an SVG document with optional source provenance.
///
/// With `None` the output is exactly the historical [`scene_to_svg`] document.
/// With provenance the document also carries:
/// - a `<metadata>` block with the `vfe.*` key/value pairs, XML-escaped;
/// - a visible bottom-left `<text>` warning over a translucent `<rect>`;
/// - line/trail widths converted from world units to pixels using the projected
///   endpoint span, clamped to `0.5..=3.0` (zero world span falls back to the
///   old `max(0.5)` width).
pub fn scene_to_svg_with_provenance(
    scene: &crate::renderer::SceneGPU,
    view_proj: Mat4,
    width: u32,
    height: u32,
    clear: [f64; 4],
    provenance: Option<&ExportProvenance>,
) -> String {
    // Warn (never truncate) when the scene is larger than the safe cap.
    let element_count = svg_element_count(scene);
    if element_count > SVG_ELEMENT_CAP {
        log::warn!(
            "SVG export contains {} elements, exceeding the safe cap of {}; \
             file may be large",
            element_count,
            SVG_ELEMENT_CAP
        );
    }

    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">",
        width, height, width, height
    ));
    // Background
    let bg = format!(
        "rgb({},{},{})",
        (clear[0] * 255.0).round() as u8,
        (clear[1] * 255.0).round() as u8,
        (clear[2] * 255.0).round() as u8,
    );
    out.push_str(&format!(
        "<rect width=\"100%\" height=\"100%\" fill=\"{}\"/>",
        bg
    ));

    // Arrowhead marker
    out.push_str(
        "<defs><marker id=\"ah\" markerWidth=\"6\" markerHeight=\"6\" \
         refX=\"5\" refY=\"3\" orient=\"auto\"><path d=\"M0,0 L6,3 L0,6 Z\" \
         fill=\"context-stroke\"/></marker></defs>",
    );

    // Lines (streamlines, trails, backdrop)
    for line in scene.lines.iter().chain(scene.trails.iter()) {
        let Some((x1, y1)) =
            world_to_screen(view_proj, [line.a[0], line.a[1], line.a[2]], width, height)
        else {
            continue;
        };
        let Some((x2, y2)) =
            world_to_screen(view_proj, [line.b[0], line.b[1], line.b[2]], width, height)
        else {
            continue;
        };
        let color = color_to_hex(line.color);
        let w = match provenance {
            Some(_) => line_stroke_width(line, view_proj, width, height),
            None => line.b[3].max(0.5),
        };
        out.push_str(&format!(
            "<line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" stroke=\"{}\" stroke-width=\"{:.2}\" stroke-linecap=\"round\"/>",
            x1, y1, x2, y2, color, w
        ));
    }

    // Arrows (2D uses scene.arrows; 3D uses combined). For SVG, draw each
    // arrow as a shaft line with an arrowhead marker.
    let arrow_instances = if scene.mode_3d {
        scene.combined.as_slice()
    } else {
        scene.arrows.as_slice()
    };
    for inst in arrow_instances {
        // inst.origin.xyz = position; dir_len.xyz = unit dir, .w = length (arrows)
        // or radius (dots). Tag is origin.w: 1.0 arrow, 0.0 particle.
        let tag = inst.origin[3];
        let pos = [inst.origin[0], inst.origin[1], inst.origin[2]];
        let Some((sx, sy)) = world_to_screen(view_proj, pos, width, height) else {
            continue;
        };
        let color = color_to_hex(inst.color);
        if tag > 0.5 {
            let dir = [inst.dir_len[0], inst.dir_len[1], inst.dir_len[2]];
            let len = inst.dir_len[3];
            let end = [
                pos[0] + dir[0] * len,
                pos[1] + dir[1] * len,
                pos[2] + dir[2] * len,
            ];
            if let Some((ex, ey)) = world_to_screen(view_proj, end, width, height) {
                let sw = 1.5f32;
                out.push_str(&format!(
                    "<line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" stroke=\"{}\" stroke-width=\"{:.2}\" marker-end=\"url(#ah)\"/>",
                    sx, sy, ex, ey, color, sw
                ));
            }
        } else {
            // particle dot
            let r = inst.dir_len[3].max(1.0);
            out.push_str(&format!(
                "<circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"{:.2}\" fill=\"{}\"/>",
                sx, sy, r, color
            ));
        }
    }

    // 2D particles live in scene.dots (not in scene.arrows), so emit them as
    // circles here. In 3D they are part of scene.combined above.
    if !scene.mode_3d {
        for inst in scene.dots.iter() {
            let pos = [inst.origin[0], inst.origin[1], inst.origin[2]];
            let Some((sx, sy)) = world_to_screen(view_proj, pos, width, height) else {
                continue;
            };
            let color = color_to_hex(inst.color);
            let r = inst.dir_len[3].max(1.0);
            out.push_str(&format!(
                "<circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"{:.2}\" fill=\"{}\"/>",
                sx, sy, r, color
            ));
        }
    }

    if let Some(provenance) = provenance {
        if !provenance.metadata.is_empty() {
            out.push_str("<metadata>");
            for (key, value) in &provenance.metadata {
                let tag = xml_escape(key);
                out.push_str(&format!("<{tag}>{}</{tag}>", xml_escape(value)));
            }
            out.push_str("</metadata>");
        }

        let lines = provenance_lines(provenance);
        if !lines.is_empty() {
            const FONT_SIZE: f32 = 12.0;
            const LINE_HEIGHT: f32 = 14.0;
            const PADDING: f32 = 6.0;
            let banner_height =
                (lines.len() as f32 * LINE_HEIGHT + PADDING * 2.0).min(height as f32);
            let banner_top = height as f32 - banner_height;
            out.push_str(&format!(
                "<rect x=\"0\" y=\"{banner_top:.2}\" width=\"{width}\" height=\"{banner_height:.2}\" fill=\"#000000\" fill-opacity=\"0.65\"/>",
            ));
            for (index, line) in lines.iter().enumerate() {
                let baseline =
                    height as f32 - PADDING - (lines.len() - 1 - index) as f32 * LINE_HEIGHT;
                out.push_str(&format!(
                    "<text x=\"8\" y=\"{baseline:.2}\" font-family=\"monospace\" font-size=\"{FONT_SIZE:.0}\" fill=\"#ffffff\">{}</text>",
                    xml_escape(line)
                ));
            }
        }
    }

    out.push_str("</svg>");
    out
}

/// Count the vector elements a scene would serialize to (arrows, particles,
/// streamlines, trails). Used for the size warning; never affects output.
pub fn svg_element_count(scene: &crate::renderer::SceneGPU) -> usize {
    let arrow_count = if scene.mode_3d {
        scene.combined.len()
    } else {
        scene.arrows.len() + scene.dots.len()
    };
    arrow_count + scene.lines.len() + scene.trails.len()
}

pub fn exceeds_svg_element_cap(scene: &crate::renderer::SceneGPU) -> bool {
    svg_element_count(scene) > SVG_ELEMENT_CAP
}

/// Compose the user-facing toast message for an SVG export. Carries the
/// download confirmation and, when the scene exceeds [`SVG_ELEMENT_CAP`],
/// the size warning in a single message so the warning is not overwritten
/// by the download confirmation on the web path. Promoted as a single
/// `#[doc(hidden)]` seam so the message text is testable without an app
/// harness (and without a browser). Reason: the wasm export path replaces
/// the whole toast in one frame, so the two facts must be composed into one
/// string before notifying.
#[doc(hidden)]
pub fn compose_svg_export_message(element_count: usize, download_started: bool) -> String {
    let over_cap = element_count > SVG_ELEMENT_CAP;
    match (download_started, over_cap) {
        (true, true) => format!(
            "screenshot download started; scene has {} elements, file may be large",
            element_count
        ),
        (true, false) => "screenshot download started".to_string(),
        (false, true) => format!(
            "SVG export: scene has {} elements, file may be large",
            element_count
        ),
        _ => "SVG export complete".to_string(),
    }
}

/// The exact user-facing reason scalar SVG export is unavailable. The menu,
/// help/disabled state (Task 7) and every execution guard share this string.
pub const SCALAR_SVG_MESSAGE: &str = "Scalar plots require PNG; SVG export supports vector mode";

/// Whether SVG export is allowed for an explicit plot kind. Scalar plots have
/// no SVG contract, so both scalar kinds are rejected at execution, not only
/// by disabled UI state. Vector mode keeps its existing 2D-only rule in
/// [`svg_export_enabled`].
#[doc(hidden)]
pub fn svg_export_allowed(kind: crate::plot::PlotKind) -> Result<(), &'static str> {
    match kind {
        crate::plot::PlotKind::VectorField => Ok(()),
        crate::plot::PlotKind::Curve2D | crate::plot::PlotKind::Surface3D => {
            Err(SCALAR_SVG_MESSAGE)
        }
    }
}

/// Whether an SVG export is allowed for the current view mode. SVG export is
/// 2D only; in 3D the entry is disabled and PNG remains the only raster
/// export. Promoted as a single `#[doc(hidden)]` seam so the rejection rule
/// is testable without an app harness.
#[doc(hidden)]
pub fn svg_export_enabled(mode_3d: bool) -> bool {
    !mode_3d
}

/// Maximum number of vector elements an SVG export is expected to carry
/// before it becomes large enough to warn the user about file size / render
/// cost. The export is never truncated: exceeding this only logs a warning so
/// the caller can surface a toast.
pub const SVG_ELEMENT_CAP: usize = 50_000;

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
    /// `Some(provenance)` while a PNG request waits for dispatch; the inner
    /// value is the provenance captured at request time.
    pending: Option<Option<ExportProvenance>>,
    svg_pending: Option<Option<ExportProvenance>>,
    capturing: bool,
    tx: Sender<SaveOutcome>,
    rx: Receiver<SaveOutcome>,
    capture_tx: Sender<CaptureReport>,
    capture_rx: Receiver<CaptureReport>,
    /// Generations whose completions may still attach a toast. `take_request`
    /// adds one; Reset clears the set, so an old worker cannot overwrite a
    /// newer request's state.
    active_generations: Vec<u64>,
    next_generation: u64,
    toast: Option<Toast>,
}

impl Screenshot {
    pub fn new() -> Self {
        let (tx, rx) = channel();
        let (capture_tx, capture_rx) = channel();
        Self::with_channels(tx, rx, capture_tx, capture_rx)
    }

    /// Test seam: the caller owns both channel ends, so a test can play the
    /// finisher's part and watch the toast lifecycle without a dialog. This
    /// plain channel keeps the historical untagged outcome API.
    pub fn with_channel(tx: Sender<SaveOutcome>, rx: Receiver<SaveOutcome>) -> Self {
        let (capture_tx, capture_rx) = channel();
        Self::with_channels(tx, rx, capture_tx, capture_rx)
    }

    /// Test seam for the generation-tagged capture channel. The app's real
    /// constructor uses an internal tagged channel; this lets a test deliver a
    /// completion for a specific (possibly cancelled) generation.
    #[doc(hidden)]
    pub fn with_capture_channel(
        capture_tx: Sender<CaptureReport>,
        capture_rx: Receiver<CaptureReport>,
    ) -> Self {
        let (tx, rx) = channel();
        Self::with_channels(tx, rx, capture_tx, capture_rx)
    }

    fn with_channels(
        tx: Sender<SaveOutcome>,
        rx: Receiver<SaveOutcome>,
        capture_tx: Sender<CaptureReport>,
        capture_rx: Receiver<CaptureReport>,
    ) -> Self {
        Self {
            pending: None,
            svg_pending: None,
            capturing: false,
            tx,
            rx,
            capture_tx,
            capture_rx,
            active_generations: Vec::new(),
            next_generation: 0,
            toast: None,
        }
    }

    /// Record a click. The capture itself happens in the viewport, where this
    /// frame's exact rect and camera are known.
    pub fn request(&mut self) {
        self.request_with_provenance(None);
    }

    /// Record a click together with the source provenance captured now.
    pub fn request_with_provenance(&mut self, provenance: Option<ExportProvenance>) {
        self.pending = Some(provenance);
    }

    /// Request an SVG (vector) export of the current 2D view.
    pub fn request_svg(&mut self) {
        self.request_svg_with_provenance(None);
    }

    /// Request an SVG export with the source provenance captured now.
    pub fn request_svg_with_provenance(&mut self, provenance: Option<ExportProvenance>) {
        self.svg_pending = Some(provenance);
    }

    /// Whether a PNG or SVG capture is pending or in flight. Animation export
    /// uses this to keep one offscreen readback at a time; the screenshot guards
    /// use the animation job's existence symmetrically.
    pub fn is_busy(&self) -> bool {
        self.capturing || self.pending.is_some() || self.svg_pending.is_some()
    }

    /// Consume an SVG request; `Some(provenance)` means "export this frame".
    /// The outer `None` means no request was pending.
    pub fn take_svg_request_with_provenance(&mut self) -> Option<Option<ExportProvenance>> {
        self.svg_pending.take()
    }

    /// Consume an SVG request; true means "export this frame".
    pub fn take_svg_request(&mut self) -> bool {
        self.take_svg_request_with_provenance().is_some()
    }

    /// Whether an SVG request is still waiting for the viewport dispatch.
    #[doc(hidden)]
    pub fn svg_request_pending(&self) -> bool {
        self.svg_pending.is_some()
    }

    /// Whether a PNG request is still waiting for the viewport dispatch.
    #[doc(hidden)]
    pub fn png_request_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// The pending PNG provenance without consuming the request. The outer
    /// `None` means no request; `Some(None)` means a request captured without
    /// source provenance.
    #[doc(hidden)]
    pub fn pending_png_provenance(&self) -> Option<&Option<ExportProvenance>> {
        self.pending.as_ref()
    }

    /// The pending SVG provenance with the same slot distinction as
    /// [`Self::pending_png_provenance`].
    #[doc(hidden)]
    pub fn pending_svg_provenance(&self) -> Option<&Option<ExportProvenance>> {
        self.svg_pending.as_ref()
    }

    /// Consume the click; true means "capture this frame".
    ///
    /// Wasm only: while an async capture is in flight the slot stays closed, so
    /// a second click cannot start an overlapping GPU readback. Native keeps
    /// its old behavior and allows another request while the save dialog owns
    /// the first capture.
    pub fn take_request(&mut self) -> bool {
        self.take_request_with_provenance().is_some()
    }

    /// Consume the click and return the provenance captured with it.
    ///
    /// `None` means no request was pending (the outer option is the request
    /// slot); `Some(None)` means a request without source provenance.
    pub fn take_request_with_provenance(&mut self) -> Option<Option<ExportProvenance>> {
        let provenance = self.pending.take()?;
        if cfg!(target_arch = "wasm32") && self.capturing {
            return None;
        }
        self.capturing = true;
        let generation = self.next_generation;
        self.next_generation = self.next_generation.wrapping_add(1);
        self.active_generations.push(generation);
        Some(provenance)
    }

    /// The generation of the capture just claimed by [`Self::take_request`].
    #[doc(hidden)]
    pub fn capture_generation(&self) -> Option<u64> {
        self.active_generations.last().copied()
    }

    /// A sender for the saver thread spawned by this capture. The sink stamps
    /// the completion with the current capture generation.
    #[doc(hidden)]
    pub fn capture_sink(&self) -> Option<CaptureSink> {
        Some(CaptureSink {
            tx: self.capture_tx.clone(),
            generation: self.capture_generation()?,
        })
    }

    /// A sender for the historical untagged channel (SVG and tests).
    pub fn sender(&self) -> Sender<SaveOutcome> {
        self.tx.clone()
    }

    /// Cancel every pending/requested capture and invalidate in-flight
    /// completions. Used by Reset; the worker may still finish, but its tagged
    /// result is ignored.
    pub fn cancel_pending(&mut self) {
        self.pending = None;
        self.svg_pending = None;
        self.capturing = false;
        self.active_generations.clear();
        self.toast = None;
    }

    /// Drain the capture's report, clear the in-flight slot on success or
    /// failure, and expire the toast. A tagged report is accepted only while
    /// its generation is active; anything from a cancelled capture is dropped.
    pub fn tick(&mut self, now: Instant) {
        while let Ok(report) = self.capture_rx.try_recv() {
            if let Some(index) = self
                .active_generations
                .iter()
                .position(|generation| *generation == report.generation)
            {
                self.active_generations.swap_remove(index);
                self.capturing = !self.active_generations.is_empty();
                self.set_outcome(report.outcome, now);
            }
        }
        while let Ok(outcome) = self.rx.try_recv() {
            self.capturing = false;
            self.set_outcome(outcome, now);
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
        self.active_generations.pop();
        self.capturing = !self.active_generations.is_empty();
        self.set_outcome(SaveOutcome::Failed(message), Instant::now());
    }

    /// Fail a pending PNG request that cannot be dispatched (for example,
    /// because the live plot renderer could not be installed). Consumes the
    /// request through the normal take/generation lifecycle, reports through
    /// the existing failure toast, and reopens the capture slot. A no-op when
    /// no PNG request is pending; an in-flight capture is left untouched.
    pub fn fail_pending(&mut self, message: String) {
        if self.take_request() {
            self.fail(message);
        }
    }

    /// Show a toast without a save-dialog outcome (used by the SVG path and
    /// by the 3D-not-supported notice).
    pub fn notify(&mut self, text: String, failed: bool) {
        let now = Instant::now();
        self.toast = Some(Toast {
            text,
            failed,
            until: now + TOAST_TTL,
        });
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
