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
use glam::Mat4;

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

/// The save dialog's opening name: `vector-field-explorer-YYYY-MM-DD-<mode>.png`.
///
/// `now` is a parameter (not a hidden `SystemTime::now()`) so the stamp is
/// testable, and the mode label is lowercased with any non-alphanumeric run
/// folded to a single `-`, so "2D" gives the brief's
/// `vector-field-explorer-2026-11-05-2d.png`.
/// Maximum number of vector elements an SVG export is expected to carry
/// before it becomes large enough to warn the user about file size / render
/// cost. The export is never truncated: exceeding this only logs a warning so
/// the caller can surface a toast.
pub const SVG_ELEMENT_CAP: usize = 50_000;
pub const MAX_SVG_ELEMENTS: usize = SVG_ELEMENT_CAP;
pub const MAX_SVG_ELEMENT_COUNT: usize = SVG_ELEMENT_CAP;
pub const SVG_SAFE_ELEMENT_COUNT: usize = SVG_ELEMENT_CAP;

/// SVG variant of [`default_filename`].
pub fn default_svg_filename(now: SystemTime, mode_label: &str) -> String {
    let png = default_filename(now, mode_label);
    png.strip_suffix(".png").unwrap_or(&png).to_string() + ".svg"
}

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

/// Show the native save dialog on a worker thread and write the SVG there.
#[cfg(not(target_arch = "wasm32"))]
pub fn spawn_save_svg(
    tx: Sender<SaveOutcome>,
    ctx: egui::Context,
    svg: String,
    suggested: String,
) {
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
pub fn download_svg(svg: &str, filename: &str) -> Result<(), String> {
    use wasm_bindgen::JsCast as _;
    let window = web_sys::window().ok_or_else(|| "browser window unavailable".to_string())?;
    let document = window.document().ok_or_else(|| "browser document unavailable".to_string())?;
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
    let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(callback.unchecked_ref(), 1_000);
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

/// Project a world point through `view_proj` into screen pixels for a
/// `width`×`height` image. Returns `None` when the point is behind the
/// camera or outside the view frustum.
fn world_to_screen(view_proj: Mat4, point: [f32; 3], width: u32, height: u32) -> Option<(f32, f32)> {
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
    // Warn (never truncate) when the scene is larger than the safe cap.
    let element_count = svg_element_count(scene);
    if element_count > SVG_ELEMENT_CAP {
        log::warn!(
            "SVG export contains {} elements, exceeding the safe cap of {}; \
             file may be large",
            element_count, SVG_ELEMENT_CAP
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
        let Some((x1, y1)) = world_to_screen(view_proj, [line.a[0], line.a[1], line.a[2]], width, height) else {
            continue;
        };
        let Some((x2, y2)) = world_to_screen(view_proj, [line.b[0], line.b[1], line.b[2]], width, height) else {
            continue;
        };
        let color = color_to_hex(line.color);
        let w = line.b[3].max(0.5);
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
            let end = [pos[0] + dir[0] * len, pos[1] + dir[1] * len, pos[2] + dir[2] * len];
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
    svg_pending: bool,
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
            svg_pending: false,
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

    /// Request an SVG (vector) export of the current 2D view.
    pub fn request_svg(&mut self) {
        self.svg_pending = true;
    }

    /// Consume an SVG request; true means "export this frame".
    pub fn take_svg_request(&mut self) -> bool {
        std::mem::take(&mut self.svg_pending)
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

