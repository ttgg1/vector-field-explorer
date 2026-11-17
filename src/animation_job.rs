//! GUI animation export job.
//!
//! The job owns the detached [`AnimationSnapshot`], the validated
//! [`AnimationSpec`], the [`FrameSink`], the next frame index, the cancellation
//! flag and its state. It renders at most one frame per [`AnimationJob::pump`]
//! call, always steps the snapshot explicitly and never touches live app state.
//!
//! Rendering and sink output go through traits so the state machine, banner
//! application, failure paths and cancellation are testable with fakes.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::animation::{AnimationError, AnimationSpec, FrameSink, frame_count};
use crate::animation_scene::{AnimationFrame, AnimationSnapshot};
use crate::camera::OrbitCamera;
use crate::export_provenance::ExportProvenance;
use crate::particles::Particles;
use crate::screenshot::apply_export_banner;
use crate::source_scene::FieldMode;
use crate::viewmodel::State;

/// Formula mode has neither a `t` nor a particle cloud to move.
pub const FORMULA_ANIMATION_REASON: &str =
    "Animation needs a time-dependent formula or moving particles.";
/// Induction needs both a nonzero amplitude and a positive frequency.
pub const INDUCTION_ANIMATION_REASON: &str =
    "Set a nonzero amplitude and frequency to animate this induction case.";
/// Sources, Materials, Ode and scalar plots are outside this issue's scope.
pub const UNSUPPORTED_ANIMATION_REASON: &str =
    "Animation export supports time-dependent Formula fields, Induction and particles.";

/// Whether the current scene can be exported as an animation, with the exact
/// user-facing reason when it cannot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnimationEligibility {
    pub allowed: bool,
    pub reason: Option<&'static str>,
    /// The current scene has something driven by simulated time.
    pub animated: bool,
}

impl AnimationEligibility {
    /// Derive eligibility from the live view state. The scaled scene writes no
    /// live state and the job's snapshot is captured only after this check.
    pub fn from_state(state: &State) -> Self {
        if !state.is_vector_mode() {
            return Self::disabled(UNSUPPORTED_ANIMATION_REASON);
        }
        match state.field_mode {
            FieldMode::Formula => {
                if state.field.is_time_dependent() || state.particle_target().is_some() {
                    Self {
                        allowed: true,
                        reason: None,
                        animated: true,
                    }
                } else {
                    Self::disabled(FORMULA_ANIMATION_REASON)
                }
            }
            FieldMode::Induction => {
                if state.induction.is_animated() {
                    Self {
                        allowed: true,
                        reason: None,
                        animated: true,
                    }
                } else {
                    Self::disabled(INDUCTION_ANIMATION_REASON)
                }
            }
            FieldMode::Sources | FieldMode::Materials | FieldMode::Ode => {
                Self::disabled(UNSUPPORTED_ANIMATION_REASON)
            }
        }
    }

    const fn disabled(reason: &'static str) -> Self {
        Self {
            allowed: false,
            reason: Some(reason),
            animated: false,
        }
    }
}

/// The job's observable state. `RenderingFrame(n)` names the frame index being
/// rendered; `Encoding` waits for the final sink flush.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationJobState {
    Preparing,
    RenderingFrame(usize),
    Encoding,
    Writing,
    Done,
    Cancelled,
    Failed,
}

impl AnimationJobState {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Done | Self::Cancelled | Self::Failed)
    }
}

/// One frame producer: the app supplies the installed offscreen renderer; tests
/// substitute a fake.
pub trait AnimationRenderer {
    fn render(
        &mut self,
        frame: AnimationFrame,
        width: u32,
        height: u32,
        clear: [f64; 4],
    ) -> Result<Vec<u8>, AnimationError>;
}

/// One detached frame handed to an asynchronous renderer, with the export
/// dimensions and clear colour needed by the offscreen pass.
#[doc(hidden)]
pub struct AnimationFrameRequest {
    pub frame: AnimationFrame,
    pub width: u32,
    pub height: u32,
    pub clear: [f64; 4],
}

/// One GUI animation export.
pub struct AnimationJob {
    spec: AnimationSpec,
    snapshot: AnimationSnapshot,
    sink: Option<Box<dyn FrameSink>>,
    provenance: Option<ExportProvenance>,
    frames_total: usize,
    frames_done: usize,
    cancel_requested: bool,
    state: AnimationJobState,
    error: Option<String>,
    fake: Option<FakeControl>,
    /// The frame index handed to an asynchronous renderer. At most one
    /// readback is in flight at a time.
    pending_index: Option<usize>,
}

impl AnimationJob {
    /// Build a production job from an already captured snapshot and sink.
    pub fn new(
        spec: AnimationSpec,
        snapshot: AnimationSnapshot,
        sink: Box<dyn FrameSink>,
        provenance: Option<ExportProvenance>,
    ) -> Result<Self, AnimationError> {
        let frames_total = frame_count(&spec)?;
        Ok(Self {
            spec,
            snapshot,
            sink: Some(sink),
            provenance,
            frames_total,
            frames_done: 0,
            cancel_requested: false,
            state: AnimationJobState::Preparing,
            error: None,
            fake: None,
            pending_index: None,
        })
    }

    /// Fake job with a synthetic renderer and sink. `frames` overrides the
    /// spec-derived frame count so a two-frame state machine is cheap.
    pub fn fake(spec: AnimationSpec, frames: usize) -> Self {
        Self::fake_base(spec, frames, None, None, None)
    }

    /// Fake job whose renderer fails at `fail_at` (zero-based).
    pub fn fake_with_renderer_error(spec: AnimationSpec, frames: usize, fail_at: usize) -> Self {
        Self::fake_base(spec, frames, None, Some(fail_at), None)
    }

    /// Fake job whose sink rejects the frame at `fail_at` (zero-based).
    pub fn fake_with_sink_error(spec: AnimationSpec, frames: usize, fail_at: usize) -> Self {
        Self::fake_base(spec, frames, None, None, Some(fail_at))
    }

    /// Fake job that applies `provenance` to every frame before pushing it.
    pub fn fake_with_provenance(
        spec: AnimationSpec,
        frames: usize,
        provenance: ExportProvenance,
    ) -> Self {
        Self::fake_base(spec, frames, Some(provenance), None, None)
    }

    fn fake_base(
        spec: AnimationSpec,
        frames: usize,
        provenance: Option<ExportProvenance>,
        renderer_error_at: Option<usize>,
        sink_error_at: Option<usize>,
    ) -> Self {
        let probe = Arc::new(FakeJobProbe::default());
        let snapshot = AnimationSnapshot::from_parts(
            State::new(false),
            Particles::default(),
            OrbitCamera::default(),
            None,
            [0.0, 0.0, 0.0, 1.0],
        );
        Self {
            spec,
            snapshot,
            sink: Some(Box::new(FakeSink {
                probe: Arc::clone(&probe),
                fail_at: sink_error_at,
            })),
            provenance,
            frames_total: frames.max(1),
            frames_done: 0,
            cancel_requested: false,
            state: AnimationJobState::Preparing,
            error: None,
            fake: Some(FakeControl {
                probe,
                renderer_error_at,
            }),
            pending_index: None,
        }
    }

    /// The fake job's inspection handle. `None` for a production job.
    #[doc(hidden)]
    pub fn fake_probe(&self) -> Arc<FakeJobProbe> {
        self.fake
            .as_ref()
            .map(|fake| Arc::clone(&fake.probe))
            .unwrap_or_default()
    }

    pub fn state(&self) -> AnimationJobState {
        self.state
    }

    pub fn frames_done(&self) -> usize {
        self.frames_done
    }

    pub fn frames_total(&self) -> usize {
        self.frames_total
    }

    /// The last failure as user-facing text.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn spec(&self) -> &AnimationSpec {
        &self.spec
    }

    /// Stop future frames. The sink is dropped on the next [`Self::pump`], so
    /// its temporary file is deleted without finishing the output.
    pub fn cancel(&mut self) {
        if !self.state.is_terminal() {
            self.cancel_requested = true;
        }
    }

    /// Render, push and finish at most one step: one frame, or the final sink
    /// flush. Cancellation between frames drains without finishing the sink.
    pub fn pump(&mut self, renderer: &mut dyn AnimationRenderer) -> Result<(), AnimationError> {
        if self.state.is_terminal() {
            return Ok(());
        }
        if self.cancel_requested {
            self.sink = None;
            self.pending_index = None;
            self.state = AnimationJobState::Cancelled;
            self.error = None;
            return Ok(());
        }

        if self.state == AnimationJobState::Encoding {
            return self.finish_sink();
        }

        let index = self.frames_done;
        if index >= self.frames_total {
            self.state = AnimationJobState::Encoding;
            return Ok(());
        }
        self.state = AnimationJobState::RenderingFrame(index);

        let time = self.spec.start_time + index as f32 / self.spec.fps as f32;
        let dt = 1.0 / self.spec.fps.max(1) as f32;
        let aspect = self.spec.width as f32 / self.spec.height.max(1) as f32;
        let frame = match self.snapshot.frame_at(time, dt, aspect, index == 0) {
            Ok(frame) => frame,
            Err(error) => {
                self.fail_after(error.clone());
                return Err(error);
            }
        };
        let clear = self.snapshot.clear;
        let rgba = match renderer.render(frame, self.spec.width, self.spec.height, clear) {
            Ok(rgba) => rgba,
            Err(error) => {
                self.fail_after(error.clone());
                return Err(error);
            }
        };
        self.apply_and_push_frame(index, rgba)
    }

    /// Build the next detached frame for an asynchronous renderer. At most one
    /// frame is outstanding; `finish_rendered_frame` pushes its readback.
    #[doc(hidden)]
    pub fn take_next_frame(&mut self) -> Result<Option<AnimationFrameRequest>, AnimationError> {
        if self.state.is_terminal() {
            return Ok(None);
        }
        if self.cancel_requested {
            self.sink = None;
            self.pending_index = None;
            self.state = AnimationJobState::Cancelled;
            self.error = None;
            return Ok(None);
        }
        if self.pending_index.is_some() {
            return Ok(None);
        }
        if self.state == AnimationJobState::Encoding {
            self.finish_sink()?;
            return Ok(None);
        }
        let index = self.frames_done;
        if index >= self.frames_total {
            self.state = AnimationJobState::Encoding;
            return Ok(None);
        }
        let time = self.spec.start_time + index as f32 / self.spec.fps as f32;
        let dt = 1.0 / self.spec.fps.max(1) as f32;
        let aspect = self.spec.width as f32 / self.spec.height.max(1) as f32;
        let frame = match self.snapshot.frame_at(time, dt, aspect, index == 0) {
            Ok(frame) => frame,
            Err(error) => {
                self.fail_after(error.clone());
                return Err(error);
            }
        };
        self.state = AnimationJobState::RenderingFrame(index);
        self.pending_index = Some(index);
        Ok(Some(AnimationFrameRequest {
            frame,
            width: self.spec.width,
            height: self.spec.height,
            clear: self.snapshot.clear,
        }))
    }

    /// Complete the frame requested by [`Self::take_next_frame`].
    #[doc(hidden)]
    pub fn finish_rendered_frame(
        &mut self,
        result: Result<Vec<u8>, String>,
    ) -> Result<(), AnimationError> {
        let Some(index) = self.pending_index.take() else {
            return Err(AnimationError::InvalidSpec(
                "no animation frame was requested".into(),
            ));
        };
        let rgba = match result {
            Ok(rgba) => rgba,
            Err(message) => {
                let error = AnimationError::Encoder(message);
                self.fail_after(error.clone());
                return Err(error);
            }
        };
        self.apply_and_push_frame(index, rgba)
    }

    fn apply_and_push_frame(
        &mut self,
        index: usize,
        mut rgba: Vec<u8>,
    ) -> Result<(), AnimationError> {
        if let Some(provenance) = &self.provenance
            && let Err(message) =
                apply_export_banner(&mut rgba, self.spec.width, self.spec.height, provenance)
        {
            let error = AnimationError::Encoder(message);
            self.fail_after(error.clone());
            return Err(error);
        }

        let Some(sink) = self.sink.as_mut() else {
            let error = AnimationError::InvalidSpec("animation sink was lost".into());
            self.fail_after(error.clone());
            return Err(error);
        };
        match sink.push_frame(&rgba, self.spec.width, self.spec.height) {
            Ok(()) => {
                self.frames_done = index + 1;
                self.state = if self.frames_done < self.frames_total {
                    AnimationJobState::RenderingFrame(self.frames_done)
                } else {
                    AnimationJobState::Encoding
                };
                Ok(())
            }
            Err(error) => {
                self.fail_after(error.clone());
                Err(error)
            }
        }
    }

    fn finish_sink(&mut self) -> Result<(), AnimationError> {
        self.state = AnimationJobState::Writing;
        let Some(sink) = self.sink.take() else {
            let error = AnimationError::InvalidSpec("animation sink was lost".into());
            self.fail_after(error.clone());
            return Err(error);
        };
        match sink.finish() {
            Ok(()) => {
                self.state = AnimationJobState::Done;
                Ok(())
            }
            Err(error) => {
                self.fail_after(error.clone());
                Err(error)
            }
        }
    }

    /// Test driver: run at most `max_steps` ordinary pumps. Fake jobs supply
    /// their fake renderer; production jobs have no renderer to invent.
    #[doc(hidden)]
    pub fn pump_for_test(&mut self, max_steps: usize) -> Result<(), AnimationError> {
        let Some(fake) = self.fake.as_ref() else {
            return Err(AnimationError::InvalidSpec(
                "pump_for_test requires a fake job".into(),
            ));
        };
        let probe = Arc::clone(&fake.probe);
        let renderer_error_at = fake.renderer_error_at;
        let mut steps = 0;
        while !self.state.is_terminal() && steps < max_steps {
            let mut renderer = FakeRenderer {
                probe: Arc::clone(&probe),
                fail_at: renderer_error_at,
            };
            steps += 1;
            if let Err(error) = self.pump(&mut renderer) {
                if self.state == AnimationJobState::Cancelled {
                    return Ok(());
                }
                return Err(error);
            }
        }
        Ok(())
    }

    fn fail_after(&mut self, error: AnimationError) {
        self.error = Some(error_message(&error));
        self.state = AnimationJobState::Failed;
        // Dropping the sink abandons its temporary output.
        self.sink = None;
    }
}

/// Shared streaming-output budget check. The native `CountingWriter` enforces
/// the same wording; the browser's in-memory GIF sink calls this after each
/// frame so its `Vec` cannot grow unbounded.
#[doc(hidden)]
pub fn check_animation_byte_cap(bytes: u64, limit: u64) -> Result<(), AnimationError> {
    if bytes > limit {
        return Err(AnimationError::Encoder(format!(
            "animation output exceeds the {limit}-byte limit"
        )));
    }
    Ok(())
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

struct FakeControl {
    probe: Arc<FakeJobProbe>,
    renderer_error_at: Option<usize>,
}

struct FakeRenderer {
    probe: Arc<FakeJobProbe>,
    fail_at: Option<usize>,
}

impl AnimationRenderer for FakeRenderer {
    fn render(
        &mut self,
        _frame: AnimationFrame,
        width: u32,
        height: u32,
        _clear: [f64; 4],
    ) -> Result<Vec<u8>, AnimationError> {
        let index = self.probe.renders.fetch_add(1, Ordering::SeqCst);
        if Some(index) == self.fail_at {
            return Err(AnimationError::Encoder("fake renderer failed".into()));
        }
        Ok(vec![40; width as usize * height as usize * 4])
    }
}

struct FakeSink {
    probe: Arc<FakeJobProbe>,
    fail_at: Option<usize>,
}

impl FrameSink for FakeSink {
    fn push_frame(&mut self, rgba: &[u8], width: u32, height: u32) -> Result<(), AnimationError> {
        let index = self.probe.pushes.fetch_add(1, Ordering::SeqCst);
        if Some(index) == self.fail_at {
            return Err(AnimationError::Io("fake sink failed".into()));
        }
        let expected = width as usize * height as usize * 4;
        if rgba.len() != expected {
            return Err(AnimationError::InvalidSpec("fake frame size".into()));
        }
        self.probe.frames.lock().unwrap().push(rgba.to_vec());
        self.probe
            .written
            .fetch_add(rgba.len() as u64, Ordering::SeqCst);
        Ok(())
    }

    fn bytes_written(&self) -> u64 {
        self.probe.written.load(Ordering::SeqCst)
    }

    fn finish(self: Box<Self>) -> Result<(), AnimationError> {
        self.probe.finished.store(true, Ordering::SeqCst);
        Ok(())
    }
}

impl Drop for FakeSink {
    fn drop(&mut self) {
        if !self.probe.finished.load(Ordering::SeqCst) {
            self.probe.dropped.store(true, Ordering::SeqCst);
        }
    }
}

/// Shared inspection state for fake jobs.
#[doc(hidden)]
#[derive(Default)]
pub struct FakeJobProbe {
    frames: Mutex<Vec<Vec<u8>>>,
    pushes: AtomicUsize,
    renders: AtomicUsize,
    written: std::sync::atomic::AtomicU64,
    finished: AtomicBool,
    dropped: AtomicBool,
}

impl FakeJobProbe {
    pub fn frames(&self) -> Vec<Vec<u8>> {
        self.frames.lock().unwrap().clone()
    }

    pub fn pushes(&self) -> usize {
        self.pushes.load(Ordering::SeqCst)
    }

    pub fn renders(&self) -> usize {
        self.renders.load(Ordering::SeqCst)
    }

    pub fn bytes_written(&self) -> u64 {
        self.written.load(Ordering::SeqCst)
    }

    pub fn finished(&self) -> bool {
        self.finished.load(Ordering::SeqCst)
    }

    pub fn dropped(&self) -> bool {
        self.dropped.load(Ordering::SeqCst)
    }
}

/// In-memory GIF sink for the browser build: the encoder bytes become a Blob
/// download when the job finishes. Native uses the streaming path sink instead.
#[cfg(target_arch = "wasm32")]
pub fn memory_gif_sink(spec: &AnimationSpec) -> Result<Box<dyn FrameSink>, AnimationError> {
    spec.validate()?;
    if spec.format != crate::animation::AnimationFormat::Gif {
        return Err(AnimationError::InvalidSpec(
            "browser animation export is GIF only".into(),
        ));
    }
    let width = u16::try_from(spec.width)
        .map_err(|_| AnimationError::InvalidSpec("GIF width exceeds u16".into()))?;
    let height = u16::try_from(spec.height)
        .map_err(|_| AnimationError::InvalidSpec("GIF height exceeds u16".into()))?;
    let mut encoder = gif::Encoder::new(Vec::new(), width, height, &[])
        .map_err(|error| AnimationError::Encoder(format!("could not start GIF: {error}")))?;
    if spec.loop_gif {
        encoder
            .set_repeat(gif::Repeat::Infinite)
            .map_err(|error| AnimationError::Encoder(format!("could not set GIF loop: {error}")))?;
    }
    Ok(Box::new(MemoryGifSink {
        encoder: Some(encoder),
        spec: spec.clone(),
        delay: crate::animation::gif_delay_centiseconds(spec.fps),
        frames: 0,
    }))
}

#[cfg(target_arch = "wasm32")]
struct MemoryGifSink {
    encoder: Option<gif::Encoder<Vec<u8>>>,
    spec: AnimationSpec,
    delay: u16,
    frames: usize,
}

#[cfg(target_arch = "wasm32")]
impl FrameSink for MemoryGifSink {
    fn push_frame(&mut self, rgba: &[u8], width: u32, height: u32) -> Result<(), AnimationError> {
        if width != self.spec.width || height != self.spec.height {
            return Err(AnimationError::InvalidSpec(
                "animation frame does not match the GIF size".into(),
            ));
        }
        let mut rgba = rgba.to_vec();
        let mut frame = gif::Frame::from_rgba_speed(width as u16, height as u16, &mut rgba, 10);
        frame.delay = self.delay;
        let Some(encoder) = self.encoder.as_mut() else {
            return Err(AnimationError::Encoder(
                "GIF encoder was already finished".into(),
            ));
        };
        encoder
            .write_frame(&frame)
            .map_err(|error| AnimationError::Encoder(format!("GIF frame failed: {error}")))?;
        self.frames += 1;
        check_animation_byte_cap(
            encoder.get_ref().len() as u64,
            crate::animation::ANIMATION_MAX_BYTES,
        )
    }

    fn bytes_written(&self) -> u64 {
        self.encoder
            .as_ref()
            .map(|encoder| encoder.get_ref().len() as u64)
            .unwrap_or(0)
    }

    fn finish(mut self: Box<Self>) -> Result<(), AnimationError> {
        let expected = frame_count(&self.spec)?;
        if self.frames != expected {
            return Err(AnimationError::Encoder(format!(
                "GIF has {} of {expected} frames",
                self.frames
            )));
        }
        let encoder = self
            .encoder
            .take()
            .ok_or_else(|| AnimationError::Encoder("GIF encoder was already finished".into()))?;
        let bytes = encoder
            .into_inner()
            .map_err(|error| AnimationError::Encoder(format!("GIF finish failed: {error}")))?;
        download_gif(&bytes).map_err(AnimationError::External)
    }
}

#[cfg(target_arch = "wasm32")]
fn download_gif(bytes: &[u8]) -> Result<(), String> {
    use wasm_bindgen::JsCast as _;

    let window = web_sys::window().ok_or_else(|| "browser window unavailable".to_string())?;
    let document = window
        .document()
        .ok_or_else(|| "browser document unavailable".to_string())?;
    let anchor = document
        .create_element("a")
        .map_err(|error| format!("{error:?}"))?
        .dyn_into::<web_sys::HtmlAnchorElement>()
        .map_err(|error| format!("{error:?}"))?;

    let bytes = js_sys::Uint8Array::from(bytes);
    let parts = js_sys::Array::of1(bytes.as_ref());
    let options = web_sys::BlobPropertyBag::new();
    options.set_type("image/gif");
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(parts.as_ref(), &options)
        .map_err(|error| format!("{error:?}"))?;
    let url =
        web_sys::Url::create_object_url_with_blob(&blob).map_err(|error| format!("{error:?}"))?;

    anchor.set_href(&url);
    anchor.set_download("vector-field-explorer-animation.gif");
    let Some(body) = document.body() else {
        let _ = web_sys::Url::revoke_object_url(&url);
        return Err("browser document body unavailable".to_string());
    };
    if let Err(error) = body.append_child(&anchor) {
        let _ = web_sys::Url::revoke_object_url(&url);
        return Err(format!("{error:?}"));
    }
    let revoke_url = url.clone();
    let callback = wasm_bindgen::closure::Closure::once_into_js(move || {
        let _ = web_sys::Url::revoke_object_url(&revoke_url);
    });
    let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
        wasm_bindgen::JsCast::unchecked_ref(&callback),
        1_000,
    );
    anchor.click();
    anchor.remove();
    Ok(())
}
