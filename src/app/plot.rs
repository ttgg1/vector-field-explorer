//! Scalar plot app integration: transition reconciliation, plot navigation,
//! isolated viewport dispatch, honest PNG/SVG execution and the transient
//! state that must never leak into the saved vector view.

use super::*;
use crate::plot_camera::{PlotFitError, PlotFrame};
use crate::plot_renderer::capture::PlotCapture;
use crate::plot_renderer::{PlotCallback, PlotDrawState, PlotFormatCapabilities};
use crate::plot_state::PlotSettings;

impl ViewportScene {
    pub(super) fn as_vector(&self) -> Option<&SceneGPU> {
        match self {
            ViewportScene::Vector(scene) => Some(scene),
            ViewportScene::Plot(_) => None,
        }
    }

    pub(super) fn as_vector_mut(&mut self) -> Option<&mut SceneGPU> {
        match self {
            ViewportScene::Vector(scene) => Some(scene),
            ViewportScene::Plot(_) => None,
        }
    }

    pub(super) fn as_plot(&self) -> Option<&Arc<PlotScene>> {
        match self {
            ViewportScene::Plot(scene) => Some(scene),
            ViewportScene::Vector(_) => None,
        }
    }
}

/// The outcome of the final SVG execution route in the viewport. Menu state
/// uses the same [`VfApp::svg_block_reason`] guard, so a programmatic request
/// cannot bypass the rejection.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SvgExportRoute {
    NoRequest,
    Rejected(String),
    Allowed,
}

impl VfApp {
    /// An empty vector scene with the current style. Used when a scalar scene
    /// must replace cached vector geometry before the next vector rebuild.
    fn empty_vector_scene(&self) -> SceneGPU {
        SceneGPU {
            arrows: Arc::new(Vec::new()),
            lines: Arc::new(Vec::new()),
            trails: Arc::new(Vec::new()),
            dots: Arc::new(Vec::new()),
            combined: Arc::new(Vec::new()),
            arrow_style: self.state.arrow_style,
            arrow_style_2d: self.state.arrow_style_2d,
            arrow_params: self.state.arrow_params,
            mode_3d: self.state.mode_3d,
        }
    }

    /// Rebuild the scalar scene through the Task 3 state seam when its
    /// generation changed, and install it as the one cached GUI scene.
    pub(super) fn refresh_plot_scene(&mut self) -> Option<Arc<PlotScene>> {
        if self.state.is_vector_mode() {
            return None;
        }
        if let Some(scene) = self.scene.as_plot()
            && self.plot_scene_generation == scene.generation
            && self.plot_scene_generation == self.state.plot.generation
        {
            return Some(Arc::clone(scene));
        }
        let scene = self.state.plot_scene();
        self.plot_scene_generation = scene.generation;
        self.scene = ViewportScene::Plot(Arc::clone(&scene));
        Some(scene)
    }

    /// Detect one `State::plot.transition_epoch` change and reconcile every
    /// transient app resource before input/simulation/render can consume stale
    /// geometry. Vector mode pauses; it is never deleted or re-seeded.
    pub(super) fn reconcile_plot_transition(&mut self) {
        let epoch = self.state.plot.transition_epoch;
        if epoch == self.last_plot_epoch {
            return;
        }
        let was_vector = self.last_plot_was_vector;
        let now_vector = self.state.is_vector_mode();

        // A kind/example transition ends pointer-locked flight. Capture release
        // itself stays edge-triggered through `sync_fly_capture` this frame.
        self.fly = None;
        self.fly_keys.clear();

        if was_vector && !now_vector {
            // Entering scalar mode remembers the exact vector orbit/2D pose.
            self.saved_vector_orbit = Some(self.orbit);
            self.plot_camera = PlotCamera::default();
            self.plot_status.clear();
        } else if !was_vector && now_vector {
            // Returning restores the saved pose; scalar navigation never edited it.
            if let Some(orbit) = self.saved_vector_orbit.take() {
                self.orbit = orbit;
            }
            self.plot_scene_generation = 0;
            self.scene = ViewportScene::Vector(self.empty_vector_scene());
            self.plot_release_pending = true;
            self.plot_status.clear();
        }

        if now_vector {
            if self.scene.as_plot().is_some() {
                self.scene = ViewportScene::Vector(self.empty_vector_scene());
            }
            self.plot_release_pending = true;
        } else {
            self.refresh_plot_scene();
        }

        self.last_plot_epoch = epoch;
        self.last_plot_was_vector = now_vector;
        // The single formula mirror follows whatever editor is active. Task 7
        // owns the editable scalar widget; this keeps the transition honest now.
        self.bar.text = self.state.active_source().to_string();
    }

    /// UI action seam for "Fit plot": frame the scalar scene's rendered bounds
    /// with the plot-only camera. Failure retains the previous camera and the
    /// previous scene, exits fly, and reports through the status hint.
    pub fn fit_plot(&mut self, aspect: f32) -> Result<(), PlotFitError> {
        self.fly = None;
        self.fly_keys.clear();
        if self.state.is_vector_mode() {
            let error = PlotFitError::InvalidState;
            self.report_plot_error(format!("Fit plot failed: {error}"));
            return Err(error);
        }
        let Some(scene) = self.refresh_plot_scene() else {
            self.report_plot_error("Fit plot failed: no scalar scene".to_string());
            return Err(PlotFitError::InvalidState);
        };
        let Some(bounds) = scene.bounds else {
            self.report_plot_error("Fit plot failed: no drawable plot samples".to_string());
            return Err(PlotFitError::InvalidBounds);
        };
        let kind = scene.kind;
        let previous = self.plot_camera.clone();
        match self.plot_camera.fit(bounds, kind, aspect) {
            Ok(()) => {
                self.plot_status.clear();
                Ok(())
            }
            Err(error) => {
                self.plot_camera = previous;
                self.report_plot_error(format!("Fit plot failed: {error}"));
                Err(error)
            }
        }
    }

    fn report_plot_error(&mut self, message: String) {
        self.plot_status = message.clone();
        restart_hint(&mut self.status.hint, &mut self.hint_elapsed, &message);
    }

    /// Shared execution guard for menu and final viewport dispatch. The vector
    /// 3D rule is retained; both scalar kinds use the exact constant.
    pub(super) fn svg_block_reason(&self) -> Option<String> {
        if let Err(message) = screenshot::svg_export_allowed(self.state.plot.kind) {
            return Some(message.to_string());
        }
        if !screenshot::svg_export_enabled(self.state.mode_3d) {
            return Some("SVG export is 2D only; use PNG in 3D".to_string());
        }
        None
    }

    /// Consume an SVG request at the final execution dispatch. A rejected
    /// request becomes a failure toast and never reaches `scene_to_svg` or a
    /// saver thread.
    pub(super) fn take_svg_export_route(&mut self) -> SvgExportRoute {
        let Some(provenance) = self.screenshot.take_svg_request_with_provenance() else {
            return SvgExportRoute::NoRequest;
        };
        let provenance = provenance.or_else(|| self.source_export_provenance());
        self.svg_export_provenance = provenance;
        match self.svg_block_reason() {
            Some(message) => {
                self.screenshot.notify(message.clone(), true);
                SvgExportRoute::Rejected(message)
            }
            None => SvgExportRoute::Allowed,
        }
    }

    /// Claim the provenance that travelled with the SVG request just routed.
    /// Runs after [`Self::take_svg_export_route`]; a rejected request leaves it
    /// unused and the next route overwrites it.
    pub(super) fn take_svg_export_provenance(
        &mut self,
    ) -> Option<crate::export_provenance::ExportProvenance> {
        self.svg_export_provenance.take()
    }

    /// Consume one PNG request for the active scalar plot. The returned scene
    /// Arc is the same one the live callback paints; the frame aspect always
    /// comes from the source viewport even when the capture extent is capped or
    /// rounded. An invalid draft's last valid scene is captured as displayed.
    pub(super) fn begin_plot_png_plan(
        &mut self,
        rect: egui::Rect,
        pixels_per_point: f32,
    ) -> Option<(Arc<PlotScene>, PlotFrame, [u32; 2], u64)> {
        if self.state.is_vector_mode() {
            return None;
        }
        // Scalar exports always carry `None` provenance; consume through the
        // same request lifecycle as the vector PNG path.
        let _ = self.screenshot.take_request_with_provenance()?;
        let Some(scene) = self.refresh_plot_scene() else {
            self.screenshot.fail("plot scene unavailable".into());
            return None;
        };
        let aspect = rect.width().max(1.0) / rect.height().max(1.0);
        self.sync_plot_aspect(aspect);
        let frame = match self.plot_camera.frame(scene.kind, aspect) {
            Ok(frame) => frame,
            Err(error) => {
                self.screenshot.fail(error.to_string());
                return None;
            }
        };
        let (width, height) = screenshot::capture_size(
            rect.width(),
            rect.height(),
            pixels_per_point,
            screenshot::MAX_CAPTURE_EDGE,
        );
        let generation = self.screenshot.capture_generation()?;
        Some((scene, frame, [width, height], generation))
    }

    /// Keep the plot camera's stored frame aspect current before gestures.
    /// `apply_command` is the Task 4 seam that commits aspect; a neutral zoom is
    /// the one no-op command it exposes.
    fn sync_plot_aspect(&mut self, aspect: f32) {
        let kind = self.state.plot.kind;
        let _ = self.plot_camera.apply_command(
            kind,
            ViewCommand::Zoom { factor: 1.0 },
            &self.state.domain,
            aspect,
        );
    }

    /// Route one view command through the plot camera without touching the
    /// saved vector orbit. `Fit` still frames the domain, not plot extrema.
    pub(super) fn apply_plot_view_command(&mut self, command: ViewCommand) {
        let kind = self.state.plot.kind;
        let context = self.view_context();
        if let Err(error) =
            self.plot_camera
                .apply_command(kind, command, &self.state.domain, context.aspect)
        {
            self.report_plot_error(error.to_string());
        }
        if matches!(command, ViewCommand::Reset | ViewCommand::Fit) {
            self.presets.clear_active();
        }
    }

    /// Global reset's app-owned half: cancel captures, restore vector mode and
    /// scalar defaults, empty plot caches/errors and ask the renderer to drop
    /// plot targets. Called after the menu's pure control/clock reset.
    pub(super) fn reset_plot_state(&mut self) {
        self.screenshot.cancel_pending();
        let plot_defaults = crate::plot_state::PlotState::default();
        self.state.set_plot_kind(PlotKind::VectorField);
        self.state
            .restore_plot_drafts(plot_defaults.curve.source, plot_defaults.surface.source);
        let settings = PlotSettings::default();
        self.state.set_plot_samples(settings.curve_samples);
        self.state.set_plot_resolution(settings.surface_resolution);
        self.state.set_plot_width(settings.line_width);
        self.state.set_plot_auto_height(settings.auto_height);
        self.state
            .set_plot_height_range(settings.height_min, settings.height_max);
        self.fly = None;
        self.fly_keys.clear();
        self.plot_camera = PlotCamera::default();
        self.plot_scene_generation = 0;
        self.scene = ViewportScene::Vector(self.empty_vector_scene());
        self.plot_status.clear();
        if let Ok(mut slot) = self.plot_errors.lock() {
            *slot = None;
        }
        self.plot_release_pending = true;
        self.last_plot_epoch = self.state.plot.transition_epoch;
        self.last_plot_was_vector = true;
        self.bar.text = self.state.active_source().to_string();
    }

    /// Drain a callback prepare error into readable app status. The renderer
    /// trait cannot return an error, so this is the only consumer.
    fn take_plot_callback_error(&mut self) {
        if let Ok(mut slot) = self.plot_errors.lock()
            && let Some(error) = slot.take()
        {
            self.plot_status = error.clone();
            log::error!("scalar plot callback failed: {error}");
        }
    }

    /// Frame phase for the scalar viewport: plot gestures, isolated callback,
    /// capture dispatch and target release bookkeeping. It never builds or
    /// draws vector geometry.
    pub(super) fn frame_plot_viewport(
        &mut self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        render_state: &eframe::egui_wgpu::RenderState,
    ) {
        self.take_plot_callback_error();
        // Final SVG execution guard: a programmatic request in scalar mode is
        // rejected here exactly as the menu route rejects it.
        let _ = self.take_svg_export_route();
        let Some(scene) = self.refresh_plot_scene() else {
            draw_plot_status(ui, rect, "No scalar plot scene");
            return;
        };
        let aspect = rect.width().max(1.0) / rect.height().max(1.0);
        self.sync_plot_aspect(aspect);

        let resp = ui.allocate_response(rect.size(), egui::Sense::click_and_drag());
        let viewport_active = resp.hovered() || resp.dragged();
        let touch = if viewport_active {
            ui.ctx().input(|input| {
                input
                    .multi_touch()
                    .map(|info| (info.num_touches, info.translation_delta))
            })
        } else {
            None
        };
        let (num_touches, translation) = touch.unwrap_or((0, egui::Vec2::ZERO));
        let touch_pan = two_finger_pan(
            self.state.mode_3d,
            num_touches,
            Vec2::new(translation.x, translation.y),
        );
        let pointer_delta = if touch_pan.active {
            Vec2::ZERO
        } else {
            let delta = resp.drag_delta();
            Vec2::new(delta.x, delta.y)
        };
        let buttons = DragButtons {
            primary: resp.dragged_by(egui::PointerButton::Primary),
            secondary: resp.dragged_by(egui::PointerButton::Secondary),
            middle: resp.dragged_by(egui::PointerButton::Middle),
            shift: ui.ctx().input(|input| input.modifiers.shift),
        };
        let channel = if touch_pan.active {
            None
        } else {
            drag_channel(self.state.mode_3d, buttons)
        };
        let (drag, pan) = match channel {
            Some(DragChannel::Orbit) => (pointer_delta, Vec2::ZERO),
            Some(DragChannel::Pan) => (Vec2::ZERO, pointer_delta),
            None => (Vec2::ZERO, Vec2::ZERO),
        };
        let pan = pan + touch_pan.pan;
        let zoom = if viewport_active {
            crate::ui::top_bar::wheel_zoom(ui.ctx())
        } else {
            1.0
        };
        let kind = scene.kind;
        if let Err(error) = self.plot_camera.apply_gesture(
            kind,
            ViewGesture {
                drag,
                pan,
                zoom,
                viewport_height: rect.height(),
            },
            rect.height(),
        ) {
            self.report_plot_error(error.to_string());
        }
        if resp.hovered() || resp.dragged() {
            let icon = match channel {
                Some(DragChannel::Orbit) => egui::CursorIcon::Grabbing,
                _ => egui::CursorIcon::Move,
            };
            ui.ctx().set_cursor_icon(icon);
        }

        // Install the live plot renderer once the device/format is available.
        if !self.plot_renderer_ready {
            let format = render_state.target_format;
            let capabilities = plot_capabilities(&render_state.adapter, format);
            let mut renderer = render_state.renderer.write();
            if renderer.callback_resources.get::<PlotDrawState>().is_some() {
                self.plot_renderer_ready = true;
            } else {
                // eframe's default `multisampling` is 0, which its wgpu renderer
                // installs as a single-sample target; the render state exposes
                // no sample-count getter. Task 8's device proof records the
                // actual format/capabilities.
                match PlotDrawState::new(&render_state.device, format, 1, capabilities) {
                    Ok(state) => {
                        renderer.callback_resources.insert(state);
                        self.plot_renderer_ready = true;
                    }
                    Err(error) => {
                        self.plot_status = error.clone();
                        log::error!("plot renderer installation failed: {error}");
                        // Installation left the renderer unusable, so a queued
                        // PNG cannot be dispatched now or deferred to a later
                        // successful install: fail it and reopen the slot.
                        self.screenshot
                            .fail_pending(format!("plot renderer unavailable: {error}"));
                    }
                }
            }
        }
        if !self.plot_renderer_ready {
            draw_plot_status(ui, rect, &self.plot_status);
            return;
        }

        // Capture the exact displayed scene before the callback. The capture
        // owns independent resources; the scene Arc and frame aspect are shared.
        if let Some((capture_scene, frame, extent, _generation)) =
            self.begin_plot_png_plan(rect, ui.ctx().pixels_per_point())
        {
            let clear = linear_clear(ui);
            let capabilities = plot_capabilities(&render_state.adapter, render_state.target_format);
            self.dispatch_plot_png(
                render_state,
                capture_scene,
                frame,
                extent,
                clear,
                capabilities,
                ui.ctx(),
            );
        }

        let frame = match self.plot_camera.frame(kind, aspect) {
            Ok(frame) => frame,
            Err(error) => {
                self.report_plot_error(error.to_string());
                draw_plot_status(ui, rect, &self.plot_status);
                return;
            }
        };
        let Some(live) = self.refresh_plot_scene() else {
            draw_plot_status(ui, rect, "No scalar plot scene");
            return;
        };
        let callback = eframe::egui_wgpu::Callback::new_paint_callback(
            rect,
            PlotCallback {
                scene: live,
                frame,
                rect,
                clear: linear_clear(ui),
                errors: Arc::clone(&self.plot_errors),
            },
        );
        ui.painter().add(callback);
        if !self.plot_status.is_empty() {
            draw_plot_status(ui, rect, &self.plot_status);
        }
    }

    /// Execute one already-planned scalar capture. Setup/map/save failures all
    /// pass through the existing [`Screenshot::fail`] lifecycle.
    #[allow(clippy::too_many_arguments)]
    fn dispatch_plot_png(
        &mut self,
        render_state: &eframe::egui_wgpu::RenderState,
        scene: Arc<PlotScene>,
        frame: PlotFrame,
        extent: [u32; 2],
        clear: [f64; 4],
        capabilities: PlotFormatCapabilities,
        ctx: &egui::Context,
    ) {
        let filename = screenshot::default_filename(
            screenshot::SystemTime::now(),
            match scene.kind {
                PlotKind::Curve2D => "curve",
                PlotKind::Surface3D => "surface",
                PlotKind::VectorField => "plot",
            },
        );
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut capture = match PlotCapture::new(
                &render_state.device,
                render_state.target_format,
                capabilities,
            ) {
                Ok(capture) => capture,
                Err(error) => {
                    self.screenshot.fail(error);
                    return;
                }
            };
            match capture.render(
                &render_state.device,
                &render_state.queue,
                &scene,
                frame,
                extent,
                clear,
            ) {
                Ok(rgba) => match self.screenshot.capture_sink() {
                    Some(sink) => screenshot::spawn_save(
                        sink,
                        ctx.clone(),
                        rgba,
                        extent[0],
                        extent[1],
                        filename,
                        None,
                    ),
                    None => self
                        .screenshot
                        .fail("capture generation was cancelled".into()),
                },
                Err(error) => self.screenshot.fail(error.to_string()),
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let mut capture = match PlotCapture::new(
                &render_state.device,
                render_state.target_format,
                capabilities,
            ) {
                Ok(capture) => capture,
                Err(error) => {
                    self.screenshot.fail(error);
                    return;
                }
            };
            let Some(sink) = self.screenshot.capture_sink() else {
                self.screenshot
                    .fail("capture generation was cancelled".into());
                return;
            };
            let ctx = ctx.clone();
            let started = capture.render_async(
                &render_state.device,
                &render_state.queue,
                &scene,
                frame,
                extent,
                clear,
                move |result| {
                    screenshot::finish_download(
                        result, extent[0], extent[1], filename, sink, ctx, None,
                    );
                },
            );
            if let Err(error) = started {
                self.screenshot.fail(error);
            }
        }
    }
}

fn plot_capabilities(
    adapter: &eframe::wgpu::Adapter,
    format: eframe::wgpu::TextureFormat,
) -> PlotFormatCapabilities {
    PlotFormatCapabilities {
        color: adapter.get_texture_format_features(format),
        depth: adapter.get_texture_format_features(eframe::wgpu::TextureFormat::Depth32Float),
    }
}

fn linear_clear(ui: &egui::Ui) -> [f64; 4] {
    let clear = egui::Rgba::from(ui.visuals().panel_fill).to_array();
    [
        f64::from(clear[0]),
        f64::from(clear[1]),
        f64::from(clear[2]),
        f64::from(clear[3]),
    ]
}

fn draw_plot_status(ui: &egui::Ui, rect: egui::Rect, message: &str) {
    if message.is_empty() {
        return;
    }
    let painter = ui.painter();
    painter.text(
        rect.left_top() + egui::vec2(12.0, 12.0),
        egui::Align2::LEFT_TOP,
        message,
        egui::FontId::proportional(13.0),
        ui.visuals().error_fg_color,
    );
}

/// Minimal doc-hidden integration harness. External tests cannot name the
/// private app modules, but every method below calls the same production seam
/// the frame uses; none reimplements a rule. A single borrow-scoped harness is
/// used instead of making each phase public API.
#[doc(hidden)]
pub struct PlotAppHarness<'a> {
    app: &'a mut VfApp,
}

impl VfApp {
    #[doc(hidden)]
    pub fn test_harness(&mut self) -> PlotAppHarness<'_> {
        PlotAppHarness { app: self }
    }
}

impl PlotAppHarness<'_> {
    pub fn reconcile(&mut self) {
        self.app.reconcile_plot_transition();
    }

    pub fn advance_clock(&mut self, dt: f32) {
        self.app.advance_clock(dt);
    }

    pub fn advance_simulation(&mut self) {
        self.app.advance_simulation();
    }

    pub fn sync_fly_capture(&mut self, ctx: &egui::Context) {
        self.app.sync_fly_capture(ctx);
    }

    pub fn set_capture_active(&mut self, active: bool) {
        self.app.capture_active = active;
    }

    pub fn capture_active(&self) -> bool {
        self.app.capture_active
    }

    pub fn apply_click(
        &mut self,
        action: LeftClickAction,
        pos: egui::Pos2,
        rect: egui::Rect,
        aspect: f32,
        time: f64,
    ) {
        self.app.apply_click_action(action, pos, rect, aspect, time);
    }

    pub fn apply_view_command(&mut self, command: ViewCommand) {
        self.app.apply_view_command(command);
    }

    pub fn fit_plot(&mut self, aspect: f32) -> Result<(), PlotFitError> {
        self.app.fit_plot(aspect)
    }

    pub fn reset_all(&mut self) {
        self.app
            .apply_menu_action(MenuAction::ResetAllControls, &egui::Context::default());
    }

    pub fn menu_svg_export(&mut self, ctx: &egui::Context) {
        self.app
            .apply_menu_action(MenuAction::ExportScreenshotSvg, ctx);
    }

    /// Queue the same PNG request the menu queues, including request-time
    /// source provenance.
    pub fn request_png_export(&mut self) {
        self.app.request_png_export();
    }

    /// Queue the same SVG request the menu queues, behind its execution guard.
    pub fn request_svg_export(&mut self) {
        self.app.request_svg_export();
    }

    /// Clone the pending PNG provenance without consuming the request.
    pub fn export_png_provenance(&self) -> Option<crate::export_provenance::ExportProvenance> {
        self.app
            .screenshot
            .pending_png_provenance()
            .cloned()
            .flatten()
    }

    /// Clone the pending SVG provenance without consuming the request.
    pub fn export_svg_provenance(&self) -> Option<crate::export_provenance::ExportProvenance> {
        self.app
            .screenshot
            .pending_svg_provenance()
            .cloned()
            .flatten()
    }

    pub fn svg_dispatch(&mut self) -> SvgExportRoute {
        self.app.take_svg_export_route()
    }

    pub fn svg_request_pending(&self) -> bool {
        self.app.screenshot.svg_request_pending()
    }

    pub fn begin_plot_png_plan(
        &mut self,
        rect: egui::Rect,
        pixels_per_point: f32,
    ) -> Option<(Arc<PlotScene>, PlotFrame, [u32; 2], u64)> {
        self.app.begin_plot_png_plan(rect, pixels_per_point)
    }

    pub fn particle_snapshot(&self) -> (Vec<Vec3>, Vec<Vec<Vec3>>, f32, usize) {
        let (trails, _, _) = self.app.particles.history();
        (
            self.app.particles.pos.clone(),
            trails.to_vec(),
            self.app.clock.time,
            particle_count(&self.app.particles),
        )
    }

    pub fn click_seed_count(&self) -> usize {
        self.app.click_seeds.len()
    }

    /// Source-marker line segments in the latest vector rebuild.
    pub fn source_glyph_line_count(&self) -> usize {
        self.app.source_glyph_line_count
    }

    /// Every line in the current vector viewport scene, backdrop included.
    pub fn viewport_line_count(&self) -> usize {
        match &self.app.scene {
            ViewportScene::Vector(scene) => scene.lines.len(),
            ViewportScene::Plot(_) => 0,
        }
    }

    pub fn plot_scene(&self) -> Option<Arc<PlotScene>> {
        self.app.scene.as_plot().cloned()
    }

    pub fn scene_is_plot(&self) -> bool {
        self.app.scene.as_plot().is_some()
    }

    pub fn saved_vector_orbit(&self) -> Option<OrbitCamera> {
        self.app.saved_vector_orbit
    }

    pub fn plot_camera(&self) -> &PlotCamera {
        &self.app.plot_camera
    }

    pub fn plot_status(&self) -> &str {
        self.app.plot_status.as_str()
    }

    pub fn plot_release_pending(&self) -> bool {
        self.app.plot_release_pending
    }
}
