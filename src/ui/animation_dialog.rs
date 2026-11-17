//! Animation export dialog: format, duration, frame rate, GIF loop, output
//! size, exact frame count and the explicit Start/Cancel actions.
//!
//! The dialog is a pure view over [`AnimationDialogState`]. It never captures
//! the scene or starts a job itself: the app applies the returned action after
//! the widget pass, so screenshot/animation mutual exclusion and playback state
//! stay in one place.

use std::path::PathBuf;

use eframe::egui;

use crate::animation::{
    ANIMATION_MAX_DURATION, ANIMATION_MAX_FPS, ANIMATION_MAX_PIXELS, ANIMATION_MIN_DURATION,
    ANIMATION_MIN_FPS, AnimationError, AnimationFormat, AnimationSpec, frame_count,
};
use crate::animation_job::AnimationEligibility;

/// The dialog window title and heading.
pub const ANIMATION_DIALOG_TITLE: &str = "Export animation";
/// Exact note shown while a screenshot readback owns the capture slot.
pub const SCREENSHOT_BUSY_REASON: &str = "Wait for the current screenshot capture to finish.";

/// What the app must do after the dialog's widget pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationDialogAction {
    None,
    Start,
    Cancel,
}

/// Dialog-local settings and progress. Nothing here is persisted.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimationDialogState {
    pub open: bool,
    pub format: AnimationFormat,
    pub duration: f32,
    pub fps: u32,
    pub loop_gif: bool,
    pub width: u32,
    pub height: u32,
    /// A validation/start failure shown in red.
    pub error: Option<String>,
    /// A running job is active and the dialog shows Cancel instead of Start.
    pub running: bool,
    /// `(frames_done, frames_total)` while running.
    pub progress: Option<(usize, usize)>,
    /// A non-error completion/cancel note.
    pub status: Option<String>,
}

impl Default for AnimationDialogState {
    fn default() -> Self {
        Self {
            open: false,
            format: AnimationFormat::Gif,
            duration: 4.0,
            fps: 12,
            loop_gif: true,
            width: 1280,
            height: 720,
            error: None,
            running: false,
            progress: None,
            status: None,
        }
    }
}

impl AnimationDialogState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Open with the output size defaulted to `viewport` pixels, reduced to the
    /// animation pixel budget when the viewport exceeds it.
    pub fn open_for_viewport(&mut self, viewport: (u32, u32)) {
        let (width, height) = clamp_animation_size(viewport.0, viewport.1);
        self.width = width;
        self.height = height;
        self.open = true;
        self.error = None;
        self.status = None;
        self.progress = None;
    }

    /// The spec this dialog describes. Validation is separate so an invalid
    /// typed value can be shown instead of silently clamped.
    pub fn spec(&self, start_time: f32) -> AnimationSpec {
        AnimationSpec {
            start_time,
            duration: self.duration,
            fps: self.fps,
            width: self.width,
            height: self.height,
            loop_gif: self.loop_gif,
            format: self.format,
        }
    }

    /// Validate the dialog's current values, including the wasm edge rule.
    pub fn validation_error(&self, start_time: f32) -> Option<String> {
        let spec = self.spec(start_time);
        if let Err(error) = spec.validate() {
            return Some(error_message(&error));
        }
        #[cfg(target_arch = "wasm32")]
        {
            if spec.format != AnimationFormat::Gif {
                return Some("Browser animation export is GIF only.".into());
            }
            if spec.width > crate::animation::ANIMATION_WASM_MAX_EDGE
                || spec.height > crate::animation::ANIMATION_WASM_MAX_EDGE
            {
                return Some(format!(
                    "Browser animation export is limited to {} pixels per edge.",
                    crate::animation::ANIMATION_WASM_MAX_EDGE
                ));
            }
        }
        None
    }

    /// The exact rendered frame count for the current values.
    pub fn frame_count(&self, start_time: f32) -> Result<usize, String> {
        frame_count(&self.spec(start_time)).map_err(|error| error_message(&error))
    }
}

fn error_message(error: &AnimationError) -> String {
    match error {
        AnimationError::InvalidSpec(message)
        | AnimationError::Encoder(message)
        | AnimationError::Io(message)
        | AnimationError::External(message) => message.clone(),
        AnimationError::Cancelled => "animation cancelled".into(),
    }
}

/// Scale a requested output size down into the animation pixel budget while
/// preserving its aspect ratio. A zero/undefined viewport falls back to 1×1
/// rather than producing an invalid spec.
pub fn clamp_animation_size(width: u32, height: u32) -> (u32, u32) {
    let width = width.max(1);
    let height = height.max(1);
    #[cfg(target_arch = "wasm32")]
    let (width, height) = {
        use crate::animation::ANIMATION_WASM_MAX_EDGE;
        let (mut width, mut height) = (width, height);
        let edge = width.max(height).min(ANIMATION_WASM_MAX_EDGE).max(1);
        if width > edge {
            height = ((u64::from(height) * u64::from(edge)) / u64::from(width)).max(1) as u32;
            width = edge;
        }
        if height > edge {
            width = ((u64::from(width) * u64::from(edge)) / u64::from(height)).max(1) as u32;
            height = edge;
        }
        (width, height)
    };
    let pixels = u64::from(width) * u64::from(height);
    if pixels <= ANIMATION_MAX_PIXELS {
        return (width, height);
    }
    let scale = (ANIMATION_MAX_PIXELS as f64 / pixels as f64).sqrt();
    let scaled_width = ((f64::from(width) * scale).floor() as u32).max(1);
    let scaled_height = ((f64::from(height) * scale).floor() as u32).max(1);
    (scaled_width, scaled_height)
}

pub fn format_label(format: AnimationFormat) -> &'static str {
    match format {
        AnimationFormat::Gif => "GIF",
        AnimationFormat::Mp4 => "MP4",
        AnimationFormat::Webm => "WebM",
    }
}

pub fn format_extension(format: AnimationFormat) -> &'static str {
    match format {
        AnimationFormat::Gif => "gif",
        AnimationFormat::Mp4 => "mp4",
        AnimationFormat::Webm => "webm",
    }
}

/// Replace a missing or foreign extension with the selected format's.
pub fn ensure_animation_extension(path: PathBuf, format: AnimationFormat) -> PathBuf {
    let extension = format_extension(format);
    match path.extension().and_then(|value| value.to_str()) {
        Some(value) if value.eq_ignore_ascii_case(extension) => path,
        _ => path.with_extension(extension),
    }
}

/// The native save dialog's opening name.
pub fn default_animation_filename(
    now: crate::screenshot::SystemTime,
    format: AnimationFormat,
) -> String {
    let png = crate::screenshot::default_filename(now, "animation");
    let stem = png.strip_suffix(".png").unwrap_or(&png);
    format!("{stem}.{}", format_extension(format))
}

/// Draw the dialog and report the one action the app must apply.
pub fn show(
    ctx: &egui::Context,
    dialog: &mut AnimationDialogState,
    eligibility: AnimationEligibility,
    start_time: f32,
    screenshot_busy: bool,
) -> AnimationDialogAction {
    if !dialog.open {
        return AnimationDialogAction::None;
    }

    let validation = dialog.validation_error(start_time);
    let frames = dialog.frame_count(start_time).ok();
    let settings_before = (
        dialog.format,
        dialog.duration,
        dialog.fps,
        dialog.loop_gif,
        dialog.width,
        dialog.height,
    );
    let mut action = AnimationDialogAction::None;
    let mut open = true;

    egui::Window::new(ANIMATION_DIALOG_TITLE)
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(340.0)
        .show(ctx, |ui| {
            ui.heading(ANIMATION_DIALOG_TITLE);
            ui.add_enabled_ui(!dialog.running, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Format");
                    egui::ComboBox::from_id_salt("animation_format")
                        .selected_text(format_label(dialog.format))
                        .show_ui(ui, |ui| {
                            for format in [
                                AnimationFormat::Gif,
                                AnimationFormat::Mp4,
                                AnimationFormat::Webm,
                            ] {
                                #[cfg(target_arch = "wasm32")]
                                if format != AnimationFormat::Gif {
                                    continue;
                                }
                                ui.selectable_value(
                                    &mut dialog.format,
                                    format,
                                    format_label(format),
                                );
                            }
                        });
                });
                ui.horizontal(|ui| {
                    ui.label("Duration");
                    ui.add(
                        egui::DragValue::new(&mut dialog.duration)
                            .speed(0.1)
                            .suffix(" s")
                            .range(ANIMATION_MIN_DURATION..=ANIMATION_MAX_DURATION),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("FPS");
                    ui.add(
                        egui::DragValue::new(&mut dialog.fps)
                            .speed(1)
                            .range(ANIMATION_MIN_FPS..=ANIMATION_MAX_FPS),
                    );
                });
                if dialog.format == AnimationFormat::Gif {
                    ui.checkbox(&mut dialog.loop_gif, "Loop GIF");
                } else {
                    ui.label("Video ignores the GIF loop setting.");
                }
                ui.horizontal(|ui| {
                    ui.label("Width");
                    ui.add(egui::DragValue::new(&mut dialog.width).speed(1));
                    ui.label("Height");
                    ui.add(egui::DragValue::new(&mut dialog.height).speed(1));
                });
            });

            ui.separator();
            if let Some((done, total)) = dialog.progress {
                ui.label(format!("Frames: {done}/{total}"));
            } else if let Some(frames) = frames {
                ui.label(format!("Frames: {frames}"));
            }
            if let Some(error) = &dialog.error {
                ui.colored_label(egui::Color32::from_rgb(230, 70, 70), error);
            } else if !eligibility.allowed {
                ui.colored_label(
                    egui::Color32::from_rgb(230, 70, 70),
                    eligibility.reason.unwrap_or_default(),
                );
            } else if screenshot_busy {
                ui.colored_label(egui::Color32::from_rgb(230, 70, 70), SCREENSHOT_BUSY_REASON);
            } else if let Some(error) = &validation {
                ui.colored_label(egui::Color32::from_rgb(230, 70, 70), error);
            }
            if let Some(status) = &dialog.status {
                ui.weak(status);
            }

            ui.separator();
            if dialog.running {
                if ui.button("Cancel").clicked() {
                    action = AnimationDialogAction::Cancel;
                }
            } else {
                let can_start = eligibility.allowed
                    && validation.is_none()
                    && !screenshot_busy
                    && dialog.error.is_none();
                if ui
                    .add_enabled(can_start, egui::Button::new("Start"))
                    .clicked()
                {
                    action = AnimationDialogAction::Start;
                }
            }
        });

    if !open {
        dialog.open = false;
        if dialog.running {
            action = AnimationDialogAction::Cancel;
        }
    }
    let settings_after = (
        dialog.format,
        dialog.duration,
        dialog.fps,
        dialog.loop_gif,
        dialog.width,
        dialog.height,
    );
    if settings_before != settings_after {
        // A typed/edited value means the previous failure may no longer apply;
        // Start becomes available again after the new values validate.
        dialog.error = None;
        dialog.status = None;
    }
    action
}
