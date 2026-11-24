//! Bounded, streaming animation specification and encoders.
//!
//! This module is deliberately free of the app, GPU and clock: it defines the
//! frame schedule, validates the export budget, and streams tightly packed
//! RGBA frames into an animated GIF or into an external `ffmpeg` process. GIF
//! output is built in and works on every target; MP4/WebM use a user-installed
//! `ffmpeg` on native targets and never fall back to GIF.
//!
//! Every encoder writes to a sibling temporary file and only replaces the
//! destination after the byte budget is checked. Any error or dropped sink
//! deletes the temporary file.

use std::fmt;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::export_provenance::ExportProvenance;

/// Lowest accepted animation frame rate.
pub const ANIMATION_MIN_FPS: u32 = 1;
/// Highest accepted animation frame rate.
pub const ANIMATION_MAX_FPS: u32 = 30;
/// Hard cap on frames in one animation.
pub const ANIMATION_MAX_FRAMES: usize = 1800;
/// Longest accepted animation span.
pub const ANIMATION_MAX_DURATION: f32 = 60.0;
/// Shortest accepted animation span.
pub const ANIMATION_MIN_DURATION: f32 = 0.1;
/// Per-frame pixel budget (4K-class, 8,388,608 pixels).
pub const ANIMATION_MAX_PIXELS: u64 = 8_388_608;
/// Final and in-progress output byte budget (512 MiB).
pub const ANIMATION_MAX_BYTES: u64 = 512 * 1024 * 1024;
/// Maximum width/height for the wasm GIF path; validation rejects larger.
pub const ANIMATION_WASM_MAX_EDGE: u32 = 2048;
/// Maximum particle integration substep for deterministic animation export.
pub const ANIMATION_PARTICLE_SUBSTEP: f32 = 1.0 / 120.0;

/// Output container for an animation export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationFormat {
    /// Animated GIF, native and wasm.
    Gif,
    /// H.264 MP4 through external ffmpeg; native only.
    Mp4,
    /// VP9 WebM through external ffmpeg; native only.
    Webm,
}

/// Validated animation request.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimationSpec {
    /// Time of the first frame, in seconds.
    pub start_time: f32,
    /// Span from the first frame exclusive of a duplicate endpoint, in seconds.
    pub duration: f32,
    /// Integer frame rate, 1..=30.
    pub fps: u32,
    /// Output width in pixels.
    pub width: u32,
    /// Output height in pixels.
    pub height: u32,
    /// Whether a GIF repeats forever; video ignores this field.
    pub loop_gif: bool,
    /// Output container.
    pub format: AnimationFormat,
}

impl Default for AnimationSpec {
    fn default() -> Self {
        Self {
            start_time: 0.0,
            duration: 4.0,
            fps: 12,
            width: 1280,
            height: 720,
            loop_gif: true,
            format: AnimationFormat::Gif,
        }
    }
}

impl AnimationSpec {
    /// Reject requests outside the animation budget before a frame is made.
    ///
    /// The wasm edge limit is a GUI default, not a silent clamp here: callers
    /// that need it must check [`ANIMATION_WASM_MAX_EDGE`] themselves.
    pub fn validate(&self) -> Result<(), AnimationError> {
        if !self.start_time.is_finite() {
            return invalid("start time must be finite");
        }
        if !self.duration.is_finite() {
            return invalid("duration must be finite");
        }
        if !(ANIMATION_MIN_DURATION..=ANIMATION_MAX_DURATION).contains(&self.duration) {
            return invalid(format!(
                "duration {} s is outside {}..={} s",
                self.duration, ANIMATION_MIN_DURATION, ANIMATION_MAX_DURATION
            ));
        }
        if !(ANIMATION_MIN_FPS..=ANIMATION_MAX_FPS).contains(&self.fps) {
            return invalid(format!(
                "fps {} is outside {}..={}",
                self.fps, ANIMATION_MIN_FPS, ANIMATION_MAX_FPS
            ));
        }
        if self.width == 0 || self.height == 0 {
            return invalid("width and height must be nonzero");
        }
        let pixels = u64::from(self.width) * u64::from(self.height);
        if pixels > ANIMATION_MAX_PIXELS {
            return invalid(format!(
                "{}x{} is {pixels} pixels; the budget is {ANIMATION_MAX_PIXELS}",
                self.width, self.height
            ));
        }
        let frames = (self.duration * self.fps as f32).ceil();
        if frames > ANIMATION_MAX_FRAMES as f32 {
            return invalid(format!(
                "{} frames exceed the {ANIMATION_MAX_FRAMES}-frame budget",
                frames as usize
            ));
        }
        if self.format != AnimationFormat::Gif
            && (!self.width.is_multiple_of(2) || !self.height.is_multiple_of(2))
        {
            return invalid("MP4/WebM require even width and height");
        }
        Ok(())
    }
}

fn invalid(message: impl Into<String>) -> Result<(), AnimationError> {
    Err(AnimationError::InvalidSpec(message.into()))
}

/// Animation validation, encoder, I/O and external-encoder failures.
#[derive(Debug, Clone, PartialEq)]
pub enum AnimationError {
    /// The request is outside the documented animation budget.
    InvalidSpec(String),
    /// The GIF encoder rejected a frame or the encoder failed.
    Encoder(String),
    /// A local file operation failed.
    Io(String),
    /// An external tool (ffmpeg) is missing or failed.
    External(String),
    /// The export was cancelled.
    Cancelled,
}

impl fmt::Display for AnimationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSpec(message) => write!(formatter, "invalid animation spec: {message}"),
            Self::Encoder(message) => write!(formatter, "animation encoder: {message}"),
            Self::Io(message) => write!(formatter, "animation I/O: {message}"),
            Self::External(message) => write!(formatter, "{message}"),
            Self::Cancelled => write!(formatter, "animation cancelled"),
        }
    }
}

impl std::error::Error for AnimationError {}

/// Streaming destination for rendered RGBA animation frames.
pub trait FrameSink {
    /// Push one tightly packed non-premultiplied RGBA8 frame.
    fn push_frame(&mut self, rgba: &[u8], width: u32, height: u32) -> Result<(), AnimationError>;
    /// Try to submit an owned frame. A full queue returns the unchanged frame
    /// for a later update. Synchronous sinks accept it immediately.
    #[doc(hidden)]
    fn try_push_frame(
        &mut self,
        rgba: Vec<u8>,
        width: u32,
        height: u32,
    ) -> Result<Option<Vec<u8>>, AnimationError> {
        self.push_frame(&rgba, width, height)?;
        Ok(None)
    }
    /// Poll a sink with asynchronous finalization. Synchronous sinks use
    /// `finish` instead.
    #[doc(hidden)]
    fn poll_finish(&mut self) -> Option<std::task::Poll<Result<(), AnimationError>>> {
        None
    }
    /// Number of output bytes written so far, including container headers.
    fn bytes_written(&self) -> u64;
    /// Flush, enforce the output budget and atomically replace the destination.
    fn finish(self: Box<Self>) -> Result<(), AnimationError>;
}

/// Number of frames for a valid spec: `ceil(duration * fps)`.
///
/// The last frame is at `start + (frames - 1) / fps`; the endpoint is not
/// duplicated. The result is clamped to `1..=ANIMATION_MAX_FRAMES`.
pub fn frame_count(spec: &AnimationSpec) -> Result<usize, AnimationError> {
    spec.validate()?;
    let frames = (spec.duration * spec.fps as f32).ceil();
    Ok((frames as usize).clamp(1, ANIMATION_MAX_FRAMES))
}

/// Frame presentation times `start_time + i / fps`.
///
/// Allocates at most [`ANIMATION_MAX_FRAMES`] floats.
pub fn frame_times(spec: &AnimationSpec) -> Result<Vec<f32>, AnimationError> {
    let count = frame_count(spec)?;
    Ok((0..count)
        .map(|index| spec.start_time + index as f32 / spec.fps as f32)
        .collect())
}

/// GIF frame delay in centiseconds: `round(100 / fps).max(1)`.
pub fn gif_delay_centiseconds(fps: u32) -> u16 {
    let delay = (100.0 / fps.max(1) as f32).round() as u16;
    delay.max(1)
}

fn temporary_path(path: &Path) -> Result<PathBuf, AnimationError> {
    let name = path.file_name().ok_or_else(|| {
        AnimationError::InvalidSpec(format!("{} has no file name", path.display()))
    })?;
    Ok(path.with_file_name(format!(".{}.tmp", name.to_string_lossy())))
}

/// Publish a staged animation using the platform's replace-existing rename.
pub fn atomic_replace(temporary: &Path, destination: &Path) -> Result<(), AnimationError> {
    fs::rename(temporary, destination).map_err(|error| {
        AnimationError::Io(format!(
            "could not replace {} with {}: {error}",
            destination.display(),
            temporary.display()
        ))
    })
}

fn validate_frame(rgba: &[u8], width: u32, height: u32) -> Result<(), AnimationError> {
    if width == 0 || height == 0 {
        return Err(AnimationError::InvalidSpec(
            "animation frames must be nonzero".to_string(),
        ));
    }
    let expected = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| AnimationError::InvalidSpec("frame dimensions overflow".to_string()))?;
    if rgba.len() as u64 != expected {
        return Err(AnimationError::InvalidSpec(format!(
            "frame is {} bytes; {width}x{height} tightly packed RGBA needs {expected}",
            rgba.len()
        )));
    }
    Ok(())
}

/// Counting writer that aborts as soon as the byte cap is exceeded.
struct CountingWriter<W> {
    inner: W,
    count: u64,
    limit: u64,
}

impl<W> CountingWriter<W> {
    fn new(inner: W, limit: u64) -> Self {
        Self {
            inner,
            count: 0,
            limit,
        }
    }

    fn count(&self) -> u64 {
        self.count
    }

    fn into_inner(self) -> W {
        self.inner
    }
}

impl<W: Write> Write for CountingWriter<W> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let requested = buffer.len() as u64;
        if self.count.saturating_add(requested) > self.limit {
            return Err(io::Error::other(format!(
                "animation output exceeds the {}-byte limit",
                self.limit
            )));
        }
        let written = self.inner.write(buffer)?;
        self.count = self.count.saturating_add(written as u64);
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

struct GifFrameSink {
    encoder: Option<gif::Encoder<CountingWriter<File>>>,
    spec: AnimationSpec,
    temporary: PathBuf,
    destination: PathBuf,
    delay: u16,
    frames: usize,
    written: u64,
    finished: bool,
}

impl GifFrameSink {
    fn abort(&mut self) {
        self.encoder = None;
        let _ = fs::remove_file(&self.temporary);
        self.finished = true;
    }
}

impl FrameSink for GifFrameSink {
    fn push_frame(&mut self, rgba: &[u8], width: u32, height: u32) -> Result<(), AnimationError> {
        if self.finished {
            return Err(AnimationError::Encoder(
                "GIF encoder is already finished".to_string(),
            ));
        }
        if width != self.spec.width || height != self.spec.height {
            return Err(AnimationError::InvalidSpec(format!(
                "frame is {width}x{height}; the animation is {}x{}",
                self.spec.width, self.spec.height
            )));
        }
        validate_frame(rgba, width, height)?;
        let expected_frames = frame_count(&self.spec)?;
        if self.frames >= expected_frames {
            return Err(AnimationError::Encoder(format!(
                "more than the scheduled {expected_frames} GIF frames were pushed"
            )));
        }
        let width = u16::try_from(width)
            .map_err(|_| AnimationError::InvalidSpec("GIF width exceeds u16".to_string()))?;
        let height = u16::try_from(height)
            .map_err(|_| AnimationError::InvalidSpec("GIF height exceeds u16".to_string()))?;

        // `from_rgba_speed` builds a per-frame local palette (NeuQuant above
        // 256 colours) and never dithers. The copy keeps the caller's frame
        // intact because the helper rewrites alpha in place.
        let mut pixels = rgba.to_vec();
        let mut frame = gif::Frame::from_rgba_speed(width, height, &mut pixels, 10);
        frame.delay = self.delay;
        frame.dispose = gif::DisposalMethod::Keep;

        let result = self
            .encoder
            .as_mut()
            .ok_or_else(|| AnimationError::Encoder("GIF writer is missing".to_string()))?
            .write_frame(&frame);
        if let Err(error) = result {
            self.abort();
            return Err(AnimationError::Encoder(format!(
                "GIF frame {} failed: {error}",
                self.frames + 1
            )));
        }
        self.frames += 1;
        if let Some(encoder) = &self.encoder {
            self.written = encoder.get_ref().count();
        }
        Ok(())
    }

    fn bytes_written(&self) -> u64 {
        self.written
    }

    fn finish(mut self: Box<Self>) -> Result<(), AnimationError> {
        if self.finished {
            return Err(AnimationError::Encoder(
                "GIF encoder is already finished".to_string(),
            ));
        }
        if self.frames == 0 {
            self.abort();
            return Err(AnimationError::Encoder(
                "GIF animation contains no frames".to_string(),
            ));
        }
        let encoder = self
            .encoder
            .take()
            .ok_or_else(|| AnimationError::Encoder("GIF writer is missing".to_string()))?;
        let writer = match encoder.into_inner() {
            Ok(writer) => writer,
            Err(error) => {
                self.abort();
                return Err(AnimationError::Encoder(format!(
                    "GIF trailer failed: {error}"
                )));
            }
        };
        let written = writer.count();
        let file = writer.into_inner();
        if written > ANIMATION_MAX_BYTES {
            drop(file);
            self.abort();
            return Err(AnimationError::Encoder(format!(
                "GIF is {written} bytes; the limit is {ANIMATION_MAX_BYTES}"
            )));
        }
        if let Err(error) = file.sync_all() {
            drop(file);
            self.abort();
            return Err(AnimationError::Io(format!(
                "could not flush {}: {error}",
                self.temporary.display()
            )));
        }
        drop(file);
        if let Err(error) = atomic_replace(&self.temporary, &self.destination) {
            let _ = fs::remove_file(&self.temporary);
            self.finished = true;
            return Err(error);
        }
        self.written = written;
        self.finished = true;
        Ok(())
    }
}

impl Drop for GifFrameSink {
    fn drop(&mut self) {
        if !self.finished {
            self.abort();
        }
    }
}

/// Open a streaming GIF sink writing to a sibling temporary path.
///
/// Frames must match the spec dimensions, carry exactly `width*height*4` RGBA
/// bytes, and arrive in `frame_count(spec)` order. Per-frame palettes are
/// local; dithering is off. `Repeat::Infinite` is written only when
/// `loop_gif` is true, so single-play GIFs omit the repeat extension.
pub fn gif_sink(spec: &AnimationSpec, path: &Path) -> Result<Box<dyn FrameSink>, AnimationError> {
    spec.validate()?;
    if spec.format != AnimationFormat::Gif {
        return Err(AnimationError::InvalidSpec(
            "gif_sink requires AnimationFormat::Gif".to_string(),
        ));
    }
    let width = u16::try_from(spec.width)
        .map_err(|_| AnimationError::InvalidSpec("GIF width exceeds u16".to_string()))?;
    let height = u16::try_from(spec.height)
        .map_err(|_| AnimationError::InvalidSpec("GIF height exceeds u16".to_string()))?;
    let temporary = temporary_path(path)?;
    let file = File::create(&temporary).map_err(|error| {
        AnimationError::Io(format!("could not create {}: {error}", temporary.display()))
    })?;
    let writer = CountingWriter::new(file, ANIMATION_MAX_BYTES);
    let mut encoder = match gif::Encoder::new(writer, width, height, &[]) {
        Ok(encoder) => encoder,
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            return Err(AnimationError::Encoder(format!(
                "could not start GIF: {error}"
            )));
        }
    };
    if spec.loop_gif
        && let Err(error) = encoder.set_repeat(gif::Repeat::Infinite)
    {
        let _ = fs::remove_file(&temporary);
        return Err(AnimationError::Encoder(format!(
            "could not set GIF loop: {error}"
        )));
    }
    let written = encoder.get_ref().count();
    Ok(Box::new(GifFrameSink {
        encoder: Some(encoder),
        spec: spec.clone(),
        temporary,
        destination: path.to_path_buf(),
        delay: gif_delay_centiseconds(spec.fps),
        frames: 0,
        written,
        finished: false,
    }))
}

/// Encode one iterator of tightly packed RGBA frames into a GIF.
///
/// Convenience wrapper around [`gif_sink`]; it still streams and enforces the
/// same frame count and byte budgets.
pub fn encode_gif_to_path(
    frames: impl Iterator<Item = Vec<u8>>,
    spec: &AnimationSpec,
    path: &Path,
) -> Result<(), AnimationError> {
    let mut sink = gif_sink(spec, path)?;
    for frame in frames {
        sink.push_frame(&frame, spec.width, spec.height)?;
    }
    sink.finish()
}

/// Exact ffmpeg argv for a validated video spec.
///
/// Raw RGBA frames arrive on stdin at the spec frame rate. MP4 selects H.264
/// with `yuv420p` and fast-start; WebM selects VP9. Each video format also pins
/// its output muxer (`-f mp4` / `-f webm`) so ffmpeg does not need to infer it
/// from the atomic temporary filename, which ends in `.tmp`. The provenance
/// warning lines become one sanitized `comment`; `title` is always present. No
/// shell is involved.
pub fn ffmpeg_args(
    spec: &AnimationSpec,
    output: &Path,
    provenance: &ExportProvenance,
) -> Vec<String> {
    let mut arguments = vec![
        "-y".to_string(),
        "-loglevel".to_string(),
        "error".to_string(),
        "-f".to_string(),
        "rawvideo".to_string(),
        "-pixel_format".to_string(),
        "rgba".to_string(),
        "-video_size".to_string(),
        format!("{}x{}", spec.width, spec.height),
        "-framerate".to_string(),
        spec.fps.to_string(),
        "-i".to_string(),
        "pipe:0".to_string(),
        "-vf".to_string(),
        "vflip".to_string(),
    ];
    match spec.format {
        AnimationFormat::Mp4 => arguments.extend(
            [
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-movflags",
                "+faststart",
            ]
            .into_iter()
            .map(str::to_string),
        ),
        AnimationFormat::Webm => arguments.extend(
            ["-c:v", "libvpx-vp9", "-pix_fmt", "yuv420p"]
                .into_iter()
                .map(str::to_string),
        ),
        AnimationFormat::Gif => {}
    }
    arguments.push("-metadata".to_string());
    arguments.push("title=Vector Field Explorer animation".to_string());
    if let Some(comment) = provenance_comment(provenance) {
        arguments.push("-metadata".to_string());
        arguments.push(format!("comment={comment}"));
    }
    match spec.format {
        AnimationFormat::Mp4 => arguments.extend(["-f", "mp4"].into_iter().map(str::to_string)),
        AnimationFormat::Webm => arguments.extend(["-f", "webm"].into_iter().map(str::to_string)),
        AnimationFormat::Gif => {}
    }
    arguments.push(output.to_string_lossy().into_owned());
    arguments
}

fn provenance_comment(provenance: &ExportProvenance) -> Option<String> {
    let lines: Vec<String> = provenance
        .warning_lines
        .iter()
        .map(|line| sanitize_metadata(line))
        .filter(|line| !line.is_empty())
        .collect();
    if lines.is_empty() {
        return None;
    }
    let joined = lines.join("; ");
    let mut capped: String = joined.chars().take(1024).collect();
    capped.truncate(capped.trim_end().len());
    Some(capped)
}

fn sanitize_metadata(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '\r' => {
                if characters.peek() == Some(&'\n') {
                    characters.next();
                }
                output.push_str("; ");
            }
            '\n' => output.push_str("; "),
            character if character.is_control() => {}
            character => output.push(character),
        }
    }
    output.trim().to_string()
}

/// Whether a usable `ffmpeg` executable is available.
///
/// `VFE_FFMPEG` is an explicit executable override; when it is set and does not
/// exist this returns false instead of silently falling back to `PATH`.
#[cfg(not(target_arch = "wasm32"))]
pub fn ffmpeg_available() -> bool {
    use std::process::{Command, Stdio};

    if let Some(override_path) = std::env::var_os("VFE_FFMPEG")
        && !override_path.is_empty()
    {
        return Path::new(&override_path).is_file();
    }
    Command::new("ffmpeg")
        .arg("-version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// Native ffmpeg video sink using `VFE_FFMPEG` or `ffmpeg` from `PATH`.
#[cfg(not(target_arch = "wasm32"))]
pub fn video_sink(
    spec: &AnimationSpec,
    path: &Path,
    provenance: &ExportProvenance,
) -> Result<Box<dyn FrameSink>, AnimationError> {
    let program = std::env::var_os("VFE_FFMPEG")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("ffmpeg"));
    video_sink_with_program(spec, path, provenance, &program)
}

/// Testable video sink that spawns `program` without a shell.
///
/// Hidden from docs because callers should use [`video_sink`]; tests inject a
/// fake encoder to check argv, byte counts, stderr and cleanup.
#[cfg(not(target_arch = "wasm32"))]
#[doc(hidden)]
pub fn video_sink_with_program(
    spec: &AnimationSpec,
    path: &Path,
    provenance: &ExportProvenance,
    program: &Path,
) -> Result<Box<dyn FrameSink>, AnimationError> {
    use std::process::{Command, Stdio};

    spec.validate()?;
    if spec.format == AnimationFormat::Gif {
        return Err(AnimationError::InvalidSpec(
            "video_sink requires AnimationFormat::Mp4 or Webm".to_string(),
        ));
    }
    let stage = VideoStage::create(path)?;
    let arguments = ffmpeg_args(spec, &stage.path, provenance);
    let mut child = Command::new(program)
        .args(&arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            AnimationError::External(format!(
                "ffmpeg video export could not start `{}`: {error}. Install ffmpeg or set VFE_FFMPEG to its executable.",
                program.display()
            ))
        })?;
    let stdin = child.stdin.take();
    let stderr = child.stderr.take().map(|stderr| {
        let (send, receive) = mpsc::channel();
        thread::spawn(move || {
            let _ = send.send(read_bounded_stderr(stderr));
        });
        receive
    });

    // The watchdog owns the process even while the writer blocks in write_all.
    // The app can cancel with a flag and never needs to lock or join a thread.
    let cancelled = Arc::new(AtomicBool::new(false));
    let (send_status, status) = mpsc::channel();
    let child_cancelled = Arc::clone(&cancelled);
    thread::spawn(move || {
        let _ = send_status.send(wait_for_video_child(child, &child_cancelled));
    });

    let (sender, receiver) = mpsc::sync_channel(VIDEO_FRAME_QUEUE);
    let (send_result, result) = mpsc::channel();
    let writer_cancelled = Arc::clone(&cancelled);
    thread::spawn(move || {
        let outcome = run_video_worker(&receiver, stdin, status, stderr, stage, &writer_cancelled);
        let _ = send_result.send(outcome);
    });
    Ok(Box::new(VideoFrameSink {
        spec: spec.clone(),
        destination: path.to_path_buf(),
        frames: 0,
        written: 0,
        worker: Some(VideoWorker {
            sender,
            result,
            cancelled,
        }),
        push_deadline: None,
        finish_phase: VideoFinishPhase::Idle,
    }))
}

#[cfg(not(target_arch = "wasm32"))]
use std::{
    process::{Child, ChildStdin, ExitStatus},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    task::Poll,
    thread,
    time::{Duration, Instant},
};

/// Number of frames that can wait for the writer.
#[cfg(not(target_arch = "wasm32"))]
const VIDEO_FRAME_QUEUE: usize = 4;
/// Retained encoder diagnostics. The reader continues to drain beyond the cap.
#[cfg(not(target_arch = "wasm32"))]
const VIDEO_STDERR_LIMIT: usize = 8 * 1024;
#[cfg(not(target_arch = "wasm32"))]
const VIDEO_PUSH_TIMEOUT: Duration = Duration::from_millis(250);
#[cfg(not(target_arch = "wasm32"))]
const VIDEO_FLUSH_TIMEOUT: Duration = Duration::from_secs(5);
#[cfg(not(target_arch = "wasm32"))]
const VIDEO_FINISH_TIMEOUT: Duration = Duration::from_secs(30);

/// A unique sibling of the destination. Only the accepting sink can publish
/// this file, and abandoned results remove only their own staging path.
#[cfg(not(target_arch = "wasm32"))]
struct VideoStage {
    path: PathBuf,
}

#[cfg(not(target_arch = "wasm32"))]
impl VideoStage {
    fn create(destination: &Path) -> Result<Self, AnimationError> {
        static NEXT: AtomicU64 = AtomicU64::new(0);

        if destination.file_name().is_none() {
            return Err(AnimationError::InvalidSpec(format!(
                "{} has no file name",
                destination.display(),
            )));
        }
        for _ in 0..64 {
            let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = destination.with_file_name(format!(
                ".vfe-animation-{}-{sequence}.tmp",
                std::process::id(),
            ));
            match File::create_new(&path) {
                Ok(_) => return Ok(Self { path }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(AnimationError::Io(format!(
                        "could not stage video beside {}: {error}",
                        destination.display(),
                    )));
                }
            }
        }
        Err(AnimationError::Io(format!(
            "could not reserve a video stage beside {}",
            destination.display(),
        )))
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for VideoStage {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy)]
enum VideoFinishPhase {
    Idle,
    Sending(Instant),
    Waiting(Instant),
}

#[cfg(not(target_arch = "wasm32"))]
struct VideoFrameSink {
    spec: AnimationSpec,
    destination: PathBuf,
    frames: usize,
    written: u64,
    worker: Option<VideoWorker>,
    push_deadline: Option<Instant>,
    finish_phase: VideoFinishPhase,
}

#[cfg(not(target_arch = "wasm32"))]
struct VideoWorker {
    sender: mpsc::SyncSender<VideoWorkerMessage>,
    result: mpsc::Receiver<Result<VideoStage, AnimationError>>,
    cancelled: Arc<AtomicBool>,
}

#[cfg(not(target_arch = "wasm32"))]
enum VideoWorkerMessage {
    Frame(Vec<u8>),
    Finish,
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for VideoWorker {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn append_stderr(reason: String, stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let text = text.trim();
    if text.is_empty() {
        reason
    } else {
        format!("{reason}: {text}")
    }
}

/// Watch the encoder without making the UI wait for a pipe or process lock.
#[cfg(not(target_arch = "wasm32"))]
fn wait_for_video_child(mut child: Child, cancelled: &AtomicBool) -> io::Result<ExitStatus> {
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if cancelled.load(Ordering::Acquire) {
            let _ = child.kill();
            return child.wait();
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn read_bounded_stderr(mut stderr: impl std::io::Read) -> Vec<u8> {
    let mut kept = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        match stderr.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(read) => {
                let room = VIDEO_STDERR_LIMIT.saturating_sub(kept.len());
                if room > 0 {
                    kept.extend_from_slice(&buffer[..read.min(room)]);
                }
                // Continue draining so the child never blocks on a full pipe.
            }
        }
    }
    kept
}

#[cfg(not(target_arch = "wasm32"))]
fn run_video_worker(
    receiver: &mpsc::Receiver<VideoWorkerMessage>,
    stdin: Option<ChildStdin>,
    status: mpsc::Receiver<io::Result<ExitStatus>>,
    stderr: Option<mpsc::Receiver<Vec<u8>>>,
    stage: VideoStage,
    cancelled: &AtomicBool,
) -> Result<VideoStage, AnimationError> {
    let write_result = write_video_frames(receiver, stdin, cancelled);
    if write_result.is_err() {
        cancelled.store(true, Ordering::Release);
    }
    let status = status.recv();
    let outcome = write_result.and_then(|()| {
        if cancelled.load(Ordering::Acquire) {
            return Err(AnimationError::Cancelled);
        }
        let status = status
            .map_err(|_| AnimationError::External("ffmpeg process watcher stopped".into()))?
            .map_err(|error| {
                AnimationError::External(format!("could not wait for ffmpeg: {error}"))
            })?;
        if !status.success() {
            return Err(AnimationError::External(format!(
                "ffmpeg exited with {status}"
            )));
        }
        validate_video_output(&stage.path)?;
        Ok(stage)
    });

    outcome.map_err(|error| match error {
        AnimationError::External(reason) => {
            // An inherited stderr pipe must not hold finalization or stage
            // cleanup hostage after the encoder itself has exited.
            let stderr = stderr
                .and_then(|receive| receive.recv_timeout(Duration::from_millis(100)).ok())
                .unwrap_or_default();
            AnimationError::External(append_stderr(reason, &stderr))
        }
        error => error,
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn write_video_frames(
    receiver: &mpsc::Receiver<VideoWorkerMessage>,
    stdin: Option<ChildStdin>,
    cancelled: &AtomicBool,
) -> Result<(), AnimationError> {
    let mut pipe =
        stdin.ok_or_else(|| AnimationError::External("ffmpeg stdin is missing".into()))?;
    while let Ok(message) = receiver.recv() {
        if cancelled.load(Ordering::Acquire) {
            return Err(AnimationError::Cancelled);
        }
        match message {
            VideoWorkerMessage::Frame(bytes) => pipe.write_all(&bytes).map_err(|error| {
                AnimationError::External(format!("ffmpeg rejected a frame: {error}"))
            })?,
            VideoWorkerMessage::Finish => return Ok(()),
        }
    }
    // Dropping the sender or cancelling is never an implicit Finish.
    Err(AnimationError::Cancelled)
}

#[cfg(not(target_arch = "wasm32"))]
fn validate_video_output(temporary: &Path) -> Result<(), AnimationError> {
    let size = fs::metadata(temporary)
        .map_err(|error| {
            AnimationError::Io(format!(
                "could not inspect {}: {error}",
                temporary.display()
            ))
        })?
        .len();
    if size == 0 {
        return Err(AnimationError::Encoder(
            "ffmpeg produced an empty output".into(),
        ));
    }
    if size > ANIMATION_MAX_BYTES {
        return Err(AnimationError::Encoder(format!(
            "video is {size} bytes; the limit is {ANIMATION_MAX_BYTES}"
        )));
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
impl VideoFrameSink {
    fn abort(&mut self, reason: String) -> AnimationError {
        if let Some(worker) = self.worker.take()
            && let Ok(Err(error)) = worker.result.try_recv()
        {
            return error;
        }
        AnimationError::External(reason)
    }

    fn poll_video_finish(&mut self, now: Instant) -> Poll<Result<(), AnimationError>> {
        if self.frames == 0 {
            return Poll::Ready(Err(self.abort("video animation contains no frames".into())));
        }
        let Some(worker) = self.worker.as_ref() else {
            return Poll::Ready(Err(AnimationError::External(
                "ffmpeg writer is missing".into(),
            )));
        };
        if matches!(self.finish_phase, VideoFinishPhase::Idle) {
            self.finish_phase = VideoFinishPhase::Sending(now + VIDEO_FLUSH_TIMEOUT);
        }
        if let VideoFinishPhase::Sending(deadline) = self.finish_phase {
            if now >= deadline {
                return Poll::Ready(Err(self.abort(
                    "ffmpeg did not accept the final frame flush; the export was cancelled".into(),
                )));
            }
            match worker.sender.try_send(VideoWorkerMessage::Finish) {
                Ok(()) => self.finish_phase = VideoFinishPhase::Waiting(now + VIDEO_FINISH_TIMEOUT),
                Err(mpsc::TrySendError::Full(_)) => return Poll::Pending,
                Err(mpsc::TrySendError::Disconnected(_)) => {
                    return Poll::Ready(Err(
                        self.abort("ffmpeg writer stopped unexpectedly".into())
                    ));
                }
            }
        }

        let VideoFinishPhase::Waiting(deadline) = self.finish_phase else {
            unreachable!("finalization is waiting after it accepts Finish");
        };
        if now >= deadline {
            return Poll::Ready(Err(self.abort(format!(
                "ffmpeg did not finish within {} s",
                VIDEO_FINISH_TIMEOUT.as_secs(),
            ))));
        }
        match worker.result.try_recv() {
            Ok(result) => {
                self.worker = None;
                Poll::Ready(result.and_then(|stage| {
                    if Instant::now() >= deadline {
                        return Err(AnimationError::External(format!(
                            "ffmpeg did not finish within {} s",
                            VIDEO_FINISH_TIMEOUT.as_secs(),
                        )));
                    }
                    atomic_replace(&stage.path, &self.destination)
                }))
            }
            Err(mpsc::TryRecvError::Empty) => Poll::Pending,
            Err(mpsc::TryRecvError::Disconnected) => Poll::Ready(Err(
                self.abort("ffmpeg writer stopped without a result".into())
            )),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl FrameSink for VideoFrameSink {
    fn push_frame(&mut self, rgba: &[u8], width: u32, height: u32) -> Result<(), AnimationError> {
        let mut frame = rgba.to_vec();
        while let Some(pending) = self.try_push_frame(frame, width, height)? {
            frame = pending;
            thread::sleep(Duration::from_millis(5));
        }
        Ok(())
    }

    fn try_push_frame(
        &mut self,
        rgba: Vec<u8>,
        width: u32,
        height: u32,
    ) -> Result<Option<Vec<u8>>, AnimationError> {
        if !matches!(self.finish_phase, VideoFinishPhase::Idle) {
            return Err(AnimationError::External(
                "ffmpeg video is already finalizing".into(),
            ));
        }
        if width != self.spec.width || height != self.spec.height {
            return Err(AnimationError::InvalidSpec(format!(
                "frame is {width}x{height}; the animation is {}x{}",
                self.spec.width, self.spec.height,
            )));
        }
        validate_frame(&rgba, width, height)?;
        let expected_frames = frame_count(&self.spec)?;
        if self.frames >= expected_frames {
            return Err(AnimationError::External(format!(
                "more than the scheduled {expected_frames} video frames were pushed",
            )));
        }
        let Some(worker) = self.worker.as_ref() else {
            return Err(AnimationError::External("ffmpeg writer is missing".into()));
        };
        let now = Instant::now();
        let reason = format!("ffmpeg is not consuming frame {}", self.frames + 1);
        if self.push_deadline.is_some_and(|deadline| now >= deadline) {
            return Err(self.abort(format!(
                "{reason}; the encoder stalled and the export was cancelled",
            )));
        }
        let byte_count = rgba.len() as u64;
        match worker.sender.try_send(VideoWorkerMessage::Frame(rgba)) {
            Ok(()) => {
                self.push_deadline = None;
                self.frames += 1;
                self.written = self.written.saturating_add(byte_count);
                Ok(None)
            }
            Err(mpsc::TrySendError::Full(VideoWorkerMessage::Frame(frame))) => {
                self.push_deadline.get_or_insert(now + VIDEO_PUSH_TIMEOUT);
                Ok(Some(frame))
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                Err(self.abort(format!("{reason}; the encoder stopped unexpectedly")))
            }
            Err(mpsc::TrySendError::Full(VideoWorkerMessage::Finish)) => {
                unreachable!("only a frame was submitted")
            }
        }
    }

    fn bytes_written(&self) -> u64 {
        self.written
    }

    fn poll_finish(&mut self) -> Option<Poll<Result<(), AnimationError>>> {
        Some(self.poll_video_finish(Instant::now()))
    }

    fn finish(mut self: Box<Self>) -> Result<(), AnimationError> {
        loop {
            match self.poll_video_finish(Instant::now()) {
                Poll::Ready(result) => return result,
                Poll::Pending => thread::sleep(Duration::from_millis(5)),
            }
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod video_publication_tests {
    use super::*;

    #[test]
    fn expired_finish_discards_a_ready_video_and_preserves_the_destination() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("output.mp4");
        fs::write(&destination, b"old video").unwrap();
        let stage = VideoStage::create(&destination).unwrap();
        let stage_path = stage.path.clone();
        fs::write(&stage.path, b"validated staged video").unwrap();

        let (sender, _commands) = mpsc::sync_channel(1);
        let (send_result, result) = mpsc::channel();
        assert!(send_result.send(Ok(stage)).is_ok());
        let cancelled = Arc::new(AtomicBool::new(false));
        let deadline = Instant::now();
        let mut sink = VideoFrameSink {
            spec: AnimationSpec {
                format: AnimationFormat::Mp4,
                ..AnimationSpec::default()
            },
            destination: destination.clone(),
            frames: 1,
            written: 1,
            worker: Some(VideoWorker {
                sender,
                result,
                cancelled: Arc::clone(&cancelled),
            }),
            push_deadline: None,
            finish_phase: VideoFinishPhase::Waiting(deadline),
        };

        let Poll::Ready(Err(error)) = sink.poll_video_finish(deadline) else {
            panic!("a ready result must fail after its deadline");
        };
        assert!(error.to_string().contains("did not finish"));
        assert!(cancelled.load(Ordering::Acquire));
        assert_eq!(fs::read(&destination).unwrap(), b"old video");
        assert!(!stage_path.exists());

        // A worker delivering another result after the receiver was abandoned
        // cannot publish or leave its own sibling stage behind.
        let late_stage = VideoStage::create(&destination).unwrap();
        let late_path = late_stage.path.clone();
        let rejected = send_result
            .send(Ok(late_stage))
            .expect_err("receiver dropped");
        drop(rejected);
        assert!(!late_path.exists());
        assert_eq!(fs::read(destination).unwrap(), b"old video");
    }
}
