//! Central viewport: camera input, click and double-click handling, scene
//! painting, screenshots, and the fly crosshair.

use glam::Mat4;

use crate::layer_style::LayerStyles;

use super::*;

impl VfApp {
    /// Frame phase: the wgpu field viewport, its camera input, click-to-seed,
    /// screenshot capture and the captured-pointer crosshair.
    pub(super) fn frame_viewport(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        // Central viewport: the wgpu field renderer, driven by the same state.
        egui::CentralPanel::default_margins().show(ui, |ui| {
            let rect = ui.max_rect().shrink(2.0);
            self.last_viewport_rect = Some(rect);
            let Some(render_state) = frame.wgpu_render_state() else {
                draw_webgpu_fallback(ui, rect);
                return;
            };

            // Animation export: render at most one detached frame per app
            // update, before the live paint. The job owns its snapshot, so this
            // never steps live state or the shared clock.
            #[cfg(not(target_arch = "wasm32"))]
            if self.animation_job.is_some() {
                let mut renderer = render_state.renderer.write();
                if renderer.callback_resources.get_mut::<DrawState>().is_none() {
                    renderer.callback_resources.insert(DrawState::new(
                        &render_state.device,
                        render_state.target_format,
                    ));
                    self.renderer_ready = true;
                }
                if let Some(draw) = renderer.callback_resources.get_mut::<DrawState>()
                    && let Some(job) = self.animation_job.as_mut()
                {
                    let mut bridge = NativeAnimationRenderer {
                        draw,
                        device: &render_state.device,
                        queue: &render_state.queue,
                    };
                    let _ = job.pump(&mut bridge);
                }
                drop(renderer);
                if self
                    .animation_job
                    .as_ref()
                    .is_some_and(|job| !job.state().is_terminal())
                {
                    ui.ctx().request_repaint();
                }
            }

            // Browser animation export: one async offscreen readback at a time.
            // The callback only posts RGBA; the next app update applies the
            // banner and pushes it to the job on the main thread.
            #[cfg(target_arch = "wasm32")]
            if self.animation_job.is_some() {
                let completed = self
                    .animation_wasm_rx
                    .as_ref()
                    .and_then(|rx| rx.try_recv().ok());
                if let Some(result) = completed {
                    self.animation_wasm_rx = None;
                    if let Some(job) = self.animation_job.as_mut() {
                        let _ = job.finish_rendered_frame(result);
                    }
                }
                if self.animation_wasm_rx.is_none()
                    && let Some(job) = self.animation_job.as_mut()
                {
                    match job.take_next_frame() {
                        Ok(Some(request)) => {
                            let (tx, rx) = std::sync::mpsc::channel();
                            self.animation_wasm_rx = Some(rx);
                            let mut renderer = render_state.renderer.write();
                            if renderer.callback_resources.get_mut::<DrawState>().is_none() {
                                renderer.callback_resources.insert(DrawState::new(
                                    &render_state.device,
                                    render_state.target_format,
                                ));
                                self.renderer_ready = true;
                            }
                            if let Some(draw) = renderer.callback_resources.get_mut::<DrawState>() {
                                let started = draw.render_offscreen_async(
                                    &render_state.device,
                                    &render_state.queue,
                                    &request.frame.scene,
                                    request.frame.view_proj,
                                    request.frame.eye,
                                    request.width,
                                    request.height,
                                    request.clear,
                                    move |result| {
                                        let _ = tx.send(result);
                                    },
                                );
                                if let Err(error) = started {
                                    drop(renderer);
                                    self.animation_wasm_rx = None;
                                    if let Some(job) = self.animation_job.as_mut() {
                                        let _ = job.finish_rendered_frame(Err(error));
                                    }
                                }
                            }
                        }
                        Ok(None) => {}
                        Err(_) => {}
                    }
                }
                ui.ctx().request_repaint();
            }

            // Task 6: scalar mode owns its isolated depth pass and never builds
            // or paints vector geometry. The plot path handles its own input,
            // capture dispatch and renderer installation.
            if !self.state.is_vector_mode() {
                self.frame_plot_viewport(ui, rect, render_state);
                return;
            }

            if self.plot_release_pending {
                // Leaving plot mode: drop the large target while the device is
                // available. Pipelines stay installed for a later re-entry.
                let mut renderer = render_state.renderer.write();
                if let Some(state) = renderer
                    .callback_resources
                    .get_mut::<crate::plot_renderer::PlotDrawState>()
                {
                    state.release_targets();
                }
                self.plot_release_pending = false;
            }

            if !self.renderer_ready {
                let mut renderer = render_state.renderer.write();
                renderer.callback_resources.insert(DrawState::new(
                    &render_state.device,
                    render_state.target_format,
                ));
                self.renderer_ready = true;
                log::info!("wgpu renderer initialized");
            }

            // Free flight drives the camera directly: held WASD/QE keys step
            // along the look basis, primary drag looks, the wheel scales speed.
            // Everything else (orbit/pan/zoom gestures) is bypassed so the
            // orbit camera keeps the pose the exit will restore.
            let resp = ui.allocate_response(rect.size(), egui::Sense::click_and_drag());
            let aspect = rect.width().max(1.0) / rect.height().max(1.0);
            // Issue #87 S2: explicit seeds own a press that lands on one. The
            // seed drag is one primary gesture; the camera must not also orbit.
            let suppress_camera = self.seed_pointer_interaction(&resp, rect, aspect);
            let feel = self.interaction.camera_feel;
            if let Some(fly) = self.fly.as_mut() {
                let captured = self.capture_active;
                let drag = if resp.dragged_by(egui::PointerButton::Primary) {
                    let delta = resp.drag_delta();
                    Vec2::new(delta.x, delta.y)
                } else {
                    Vec2::ZERO
                };
                // Raw pointer-lock motion (native `Event::MouseMoved`, or the
                // web bridge) is the captured look channel. Without it the
                // drag delta is the fallback. `frame_look` never sums both, so
                // one physical move cannot look twice.
                let raw = if captured {
                    crate::pointer_lock::motion_delta(ui.ctx())
                } else {
                    Vec2::ZERO
                };
                let look = crate::camera::feel_orbit(frame_look(captured, raw, drag), feel);
                let boost = ui.ctx().input(|input| input.modifiers.shift);
                let input = self.fly_keys.input(look, boost);
                let dt = ui.ctx().input(|input| input.stable_dt);
                fly.step(input, dt);
                if captured {
                    // A locked pointer may not hover the viewport response, so
                    // the wheel scales speed regardless of hover.
                    let scroll = ui.ctx().input(|input| input.smooth_scroll_delta.y);
                    fly.adjust_speed(scroll);
                } else if resp.hovered() || resp.dragged() {
                    let scroll = ui.ctx().input(|input| input.smooth_scroll_delta.y);
                    fly.adjust_speed(scroll);
                    let icon = if resp.dragged() {
                        egui::CursorIcon::Grabbing
                    } else {
                        egui::CursorIcon::Grab
                    };
                    ui.ctx().set_cursor_icon(icon);
                }
                // Movement continues while a key is held: keep painting even
                // when the pointer never moves. An empty set stops the loop so
                // an idle fly view costs nothing.
                if self.fly_keys.movement_held() {
                    ui.ctx().request_repaint();
                }
            } else {
                // Camera gestures feed the seam's pure camera action. Ticket 18
                // maps the buttons through `drag_channel`: in 3D left- and
                // right-drag orbit, middle-drag and Shift+right-drag pan; in 2D
                // every drag pans. The wheel/pinch factor zooms both views (the
                // camera clamps it).
                //
                // Ticket 53 adds two-finger 3D pan. `MultiTouchInfo`'s
                // translation feeds the same `ViewGesture.pan` channel, and
                // while that gesture is active the pointer-drag channel is
                // suppressed so the first finger cannot also orbit. Pinch zoom
                // stays on `top_bar::wheel_zoom` (egui folds the pinch into
                // `zoom_delta`).
                let viewport_active = !suppress_camera && (resp.hovered() || resp.dragged());
                let touch = if viewport_active {
                    ui.ctx().input(|input| {
                        input
                            .multi_touch()
                            .map(|info| (info.num_touches, info.translation_delta))
                    })
                } else {
                    // A two-finger gesture over the tab bar/sheet/top bar must
                    // not move the camera behind it.
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
                    primary: !suppress_camera && resp.dragged_by(egui::PointerButton::Primary),
                    secondary: !suppress_camera && resp.dragged_by(egui::PointerButton::Secondary),
                    middle: !suppress_camera && resp.dragged_by(egui::PointerButton::Middle),
                    shift: ui.ctx().input(|input| input.modifiers.shift),
                };
                let channel = if touch_pan.active {
                    None
                } else {
                    drag_channel(self.state.mode_3d, buttons)
                };
                let (drag, pan) = match channel {
                    Some(DragChannel::Orbit) => {
                        (crate::camera::feel_orbit(pointer_delta, feel), Vec2::ZERO)
                    }
                    Some(DragChannel::Pan) => (Vec2::ZERO, pointer_delta),
                    None => (Vec2::ZERO, Vec2::ZERO),
                };
                let pan = pan + touch_pan.pan;
                let zoom = if viewport_active {
                    crate::camera::feel_zoom(crate::ui::top_bar::wheel_zoom(ui.ctx()), feel)
                } else {
                    1.0
                };
                self.orbit = apply_camera_input(
                    self.orbit,
                    self.state.mode_3d,
                    self.state.plane_size(),
                    ViewGesture {
                        drag,
                        pan,
                        zoom,
                        viewport_height: rect.height(),
                    },
                );

                // Cursor feedback (ticket 18): orbiting shows grab/grabbing,
                // panning shows move; the wheel zoom leaves the cursor alone.
                if resp.hovered() || resp.dragged() {
                    let icon = if touch_pan.active {
                        egui::CursorIcon::Move
                    } else {
                        match channel {
                            Some(DragChannel::Orbit) => egui::CursorIcon::Grabbing,
                            Some(DragChannel::Pan) => egui::CursorIcon::Move,
                            None if self.state.mode_3d => egui::CursorIcon::Grab,
                            None => egui::CursorIcon::Move,
                        }
                    };
                    ui.ctx().set_cursor_icon(icon);
                }
            }

            // Click-to-seed and double-click reset (ticket 18): a click only
            // seeds when the gesture never became a drag, and a double-click
            // resets the view instead — taking back the seed its first click
            // planted so the gesture leaves no burst behind.
            let now = ui.ctx().input(|input| input.time);
            if self.capture_active {
                // Captured pointer is FPS-modal: every click seeds at the
                // crosshair (the viewport centre, NDC 0,0), and double-click
                // reset is orbit-only, so a second click seeds again. A locked
                // pointer may sit outside the response, so use the global
                // pointer click rather than `resp.clicked()`.
                let clicked = ui.ctx().input(|input| input.pointer.primary_clicked());
                if self.suppress_capture_click {
                    // Swallow the click that checked the sidebar toggle.
                    self.suppress_capture_click = false;
                } else {
                    match captured_click_outcome(clicked, resp.dragged()) {
                        CapturedClickOutcome::SeedCentre => {
                            self.seed_click(rect.center(), rect, aspect, now);
                        }
                        CapturedClickOutcome::Ignore => {}
                    }
                }
            } else {
                match click_outcome(resp.clicked(), resp.double_clicked(), resp.dragged()) {
                    ClickOutcome::ResetView => {
                        self.undo_last_click_seed(now);
                        self.apply_view_command(ViewCommand::Reset);
                    }
                    ClickOutcome::Seed => {
                        if let Some(pos) = resp.interact_pointer_pos() {
                            self.seed_click(pos, rect, aspect, now);
                        }
                    }
                    ClickOutcome::None => {}
                }
            }

            // Painter's algorithm: the wgpu pass has no depth attachment, so
            // order the combined arrow + particle stream far-to-near along this
            // frame's view. The active camera also provides the framing the
            // screenshot and the click seeding use.
            let (view, view_proj, eye_position) = {
                let camera = match &self.fly {
                    Some(fly) => ViewCamera::Fly(fly),
                    None => ViewCamera::Orbit(&self.orbit),
                };
                (
                    camera.view(),
                    camera.vp_matrix(
                        aspect,
                        if self.state.mode_3d {
                            None
                        } else {
                            Some(self.state.plane_size())
                        },
                    ),
                    camera.eye_position(!self.state.mode_3d),
                )
            };
            let scene = self.assemble_viewport_scene(&view);

            // SVG screenshot export (ticket 69): serialize the 2D scene as
            // vector geometry. No GPU readback is needed, so this runs
            // synchronously on the frame thread. Task 6 guards execution here
            // as well as in the menu: scalar kinds never fall through to the
            // vector exporter.
            if self.take_svg_export_route() == super::plot::SvgExportRoute::Allowed {
                // SVG keeps the unstyled line blueprint (issue #84 contract):
                // layer overrides belong to the live viewport and raster output.
                let mut scene =
                    self.assemble_viewport_scene_with_styles(&view, &LayerStyles::default());
                // Vector output is the line blueprint: the scalar heatmap fill
                // is raster-only (issue #92 contract).
                scene.heatmap = None;
                #[cfg(not(target_arch = "wasm32"))]
                let over_cap = screenshot::exceeds_svg_element_cap(&scene);
                let element_count = screenshot::svg_element_count(&scene);
                let pixels_per_point = ui.ctx().pixels_per_point();
                let (w, h) = screenshot::capture_size(
                    rect.width(),
                    rect.height(),
                    pixels_per_point,
                    screenshot::MAX_CAPTURE_EDGE,
                );
                let clear = egui::Rgba::from(ui.visuals().panel_fill).to_array();
                let clear_f = [
                    f64::from(clear[0]),
                    f64::from(clear[1]),
                    f64::from(clear[2]),
                    f64::from(clear[3]),
                ];
                let provenance = self.take_svg_export_provenance();
                let svg = screenshot::scene_to_svg_with_provenance(
                    &scene,
                    view_proj,
                    w,
                    h,
                    clear_f,
                    provenance.as_ref(),
                );
                let filename =
                    screenshot::default_svg_filename(screenshot::SystemTime::now(), "2D");
                #[cfg(not(target_arch = "wasm32"))]
                {
                    if over_cap {
                        self.screenshot.notify(
                            screenshot::compose_svg_export_message(element_count, false),
                            false,
                        );
                    }
                    screenshot::spawn_save_svg(
                        self.screenshot.sender(),
                        ui.ctx().clone(),
                        svg,
                        filename,
                    );
                }
                #[cfg(target_arch = "wasm32")]
                {
                    match screenshot::download_svg(&svg, &filename) {
                        Ok(()) => self.screenshot.notify(
                            screenshot::compose_svg_export_message(element_count, true),
                            false,
                        ),
                        Err(e) => self
                            .screenshot
                            .notify(format!("screenshot failed: {e}"), true),
                    }
                }
            }

            // Screenshot export (ticket 08): re-render this frame's scene
            // offscreen at the viewport's pixel size (capped at 4096 on the
            // long edge, both axes scaled so the framing is unchanged) and
            // hand the pixels to a worker thread that shows the native save
            // dialog and writes the PNG. Nothing here touches the live
            // surface textures, and the readback is RGBA by contract.
            if let Some(provenance) = self.screenshot.take_request_with_provenance() {
                // A request made without capture-time provenance still carries
                // the active adapter's warning: derive it from the current
                // state rather than exporting a Source/Induction frame
                // silently.
                let provenance = provenance.or_else(|| self.field_export_provenance());
                // `rect` is in logical points; a capture must have the viewport's
                // *physical* pixels, so scale by the display's pixels-per-point
                // before rounding (HiDPI fix round 1). The cap applies after that.
                let pixels_per_point = ui.ctx().pixels_per_point();
                let (capture_width, capture_height) = screenshot::capture_size(
                    rect.width(),
                    rect.height(),
                    pixels_per_point,
                    screenshot::MAX_CAPTURE_EDGE,
                );
                // egui paints this behind the field; clearing to the same
                // (linear-space) fill keeps the capture's background identical.
                let clear = egui::Rgba::from(ui.visuals().panel_fill).to_array();
                let filename = screenshot::default_filename(
                    screenshot::SystemTime::now(),
                    if self.state.mode_3d { "3D" } else { "2D" },
                );

                // Native readback is synchronous, then a worker thread owns
                // the save dialog. The wasm readback maps through the browser
                // event loop, so the frame returns immediately and the
                // completion closure encodes/downloads and wakes the UI.
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let capture = {
                        let mut renderer = render_state.renderer.write();
                        renderer
                            .callback_resources
                            .get_mut::<DrawState>()
                            .expect("DrawState must be installed before capturing")
                            .render_offscreen(
                                &render_state.device,
                                &render_state.queue,
                                &scene,
                                view_proj,
                                eye_position,
                                capture_width,
                                capture_height,
                                [
                                    f64::from(clear[0]),
                                    f64::from(clear[1]),
                                    f64::from(clear[2]),
                                    f64::from(clear[3]),
                                ],
                            )
                    };
                    match capture {
                        Ok(rgba) => match self.screenshot.capture_sink() {
                            Some(sink) => screenshot::spawn_save(
                                sink,
                                ui.ctx().clone(),
                                rgba,
                                capture_width,
                                capture_height,
                                filename,
                                provenance,
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
                    let Some(sink) = self.screenshot.capture_sink() else {
                        self.screenshot
                            .fail("capture generation was cancelled".into());
                        return;
                    };
                    let ctx = ui.ctx().clone();
                    let started = {
                        let mut renderer = render_state.renderer.write();
                        renderer
                            .callback_resources
                            .get_mut::<DrawState>()
                            .expect("DrawState must be installed before capturing")
                            .render_offscreen_async(
                                &render_state.device,
                                &render_state.queue,
                                &scene,
                                view_proj,
                                eye_position,
                                capture_width,
                                capture_height,
                                [
                                    f64::from(clear[0]),
                                    f64::from(clear[1]),
                                    f64::from(clear[2]),
                                    f64::from(clear[3]),
                                ],
                                move |result| {
                                    screenshot::finish_download(
                                        result,
                                        capture_width,
                                        capture_height,
                                        filename,
                                        sink,
                                        ctx,
                                        provenance,
                                    );
                                },
                            )
                    };
                    if let Err(error) = started {
                        self.screenshot.fail(error);
                    }
                }
            }

            let callback = eframe::egui_wgpu::Callback::new_paint_callback(
                rect,
                DrawCallback {
                    scene,
                    view_proj,
                    camera_pos: eye_position,
                },
            );
            ui.painter().add(callback);

            // Ticket 65: the cursor is hidden while captured, so mark where a
            // click will land. Two centred 12 px lines over the field pass;
            // painter-only, no interaction.
            if crosshair_visible(self.fly.is_some(), self.capture_active) {
                let stroke =
                    egui::Stroke::new(1.0, egui::Color32::from_white_alpha(CROSSHAIR_ALPHA));
                for (from, to) in crosshair_segments(rect.center()) {
                    ui.painter().line_segment([from, to], stroke);
                }
            }
        });
    }

    /// Build the renderer scene from the cached vector scene for one frame's
    /// view. Ticket 62: the combined stream is the single sorted source for
    /// 3D. The viewmodel's arrow order stays frame-invariant (no write-back),
    /// so equal-depth ties are deterministic. 2D keeps its arrows-then-dots
    /// path and never reads the combined stream, but it still paints a stable
    /// far-to-near copy of the arrows (the pre-ticket-62 order).
    ///
    /// Trails are painted behind the arrows/dots in both views: 3D plane
    /// particles and 2D plane-slice particles both produce them, while ordinary
    /// two-component 2D simply has an empty list.
    fn assemble_viewport_scene(&mut self, view: &Mat4) -> SceneGPU {
        let styles = *self.state.layer_styles();
        self.assemble_viewport_scene_with_styles(view, &styles)
    }

    /// [`Self::assemble_viewport_scene`] with an explicit layer-style record.
    /// The live path passes the committed styles; SVG export passes the neutral
    /// default so overrides never reach the vector output.
    fn assemble_viewport_scene_with_styles(
        &mut self,
        view: &Mat4,
        styles: &LayerStyles,
    ) -> SceneGPU {
        // Ode markers are recomputed per frame and appended to copies: 2D uses
        // the cached dots vector, 3D the sorted combined stream. The cached
        // scene itself is never written.
        let ode_markers = if self.state.field_mode == FieldMode::Ode {
            self.ode_marker_instances()
        } else {
            Vec::new()
        };
        if *styles == *self.state.layer_styles() {
            let cached = self.scene.as_vector_mut().expect("vector branch");
            return assemble_vector_scene(&self.state, cached, &ode_markers, view);
        }
        if self.state.field_mode == FieldMode::Ode {
            // Ode's trajectory lines are already unstyled (they use the stored
            // arrow params), so its cached geometry is the blueprint.
            let cached = self.scene.as_vector_mut().expect("vector branch");
            return assemble_vector_scene(&self.state, cached, &ode_markers, view);
        }

        // Rebuild the style-sensitive geometry from the state under the neutral
        // record instead of reusing the styled cache.
        let reference_style = self.state.reference_viewport_style_for(styles);
        let mut blueprint = self.state.snapshot_with_styles(styles);
        let click_lines = self.state.click_streamlines_snapshot_with_styles(
            self.dots_range,
            &self.click_seeds,
            styles,
        );
        let mut lines = compose_lines_with_frame_style(
            blueprint.lines.as_ref(),
            &click_lines,
            &self.state.scene_domain(),
            self.state.is_3d(),
            self.state.plane_frame(),
            self.state.show_grid,
            self.state.show_coordinate_system,
            &reference_style,
        );
        // Source/induction glyphs are fixed-colour geometry and take no style.
        // Reuse the app's own composition; it reports the glyph count, and the
        // glyphs are the tail, so the neutral reference prefix is dropped
        // without recomposing it.
        let (composed, glyph_count) = compose_vector_lines(&self.state, &[], &[]);
        let start = composed.len().saturating_sub(glyph_count);
        if let Some(glyphs) = composed.get(start..) {
            lines.extend_from_slice(glyphs);
        }
        blueprint.lines = Arc::new(lines);
        let (dots, trails) = self.blueprint_particle_instances(styles);
        blueprint.dots = dots;
        blueprint.trails = trails;
        blueprint.combined = Arc::new(Vec::new());
        assemble_vector_scene(&self.state, &mut blueprint, &ode_markers, view)
    }

    /// Dot and trail instances rebuilt under `styles`, mirroring
    /// [`super::particle_scene_instances`] without its committed-style lookup.
    fn blueprint_particle_instances(
        &self,
        styles: &LayerStyles,
    ) -> (Arc<Vec<ArrowInstanceGPU>>, Arc<Vec<LineInstanceGPU>>) {
        let state = &self.state;
        let appearance = state.particle_appearance.sanitized();
        let spacing = state.spacing();
        let flow = state.scene_flow();
        let domain = state.scene_domain();
        let palette = ParticlePalette {
            colormap: state.colormap,
            color_range: self.color_facts.range,
            speed_range: speed_range(&self.particles),
            mapping: self.color_facts.mapping.as_ref(),
            stats: &self.color_facts.stats,
        };
        let layer = state.flow_layer_style_for(styles);
        let dots = Arc::new(dot_instances_styled_at(
            &self.particles,
            &flow,
            &domain,
            &palette,
            &appearance,
            state.particle_radius(spacing),
            state.time,
            layer,
        ));
        let trails = Arc::new(trail_instances_styled_at(
            &self.particles,
            &flow,
            &palette,
            &appearance,
            spacing,
            state.time,
            layer,
        ));
        (dots, trails)
    }

    /// Integration-test seam: assemble the scene exactly as `frame_viewport`
    /// does, using the live orbit view. Doc-hidden because external tests
    /// cannot name the private viewport module.
    #[doc(hidden)]
    pub fn test_assemble_viewport_scene(&mut self) -> SceneGPU {
        let view = self.orbit.view();
        self.assemble_viewport_scene(&view)
    }

    /// Integration-test seam for the neutral blueprint assembly.
    #[doc(hidden)]
    pub fn test_assemble_viewport_scene_with_styles(&mut self, styles: &LayerStyles) -> SceneGPU {
        let view = self.orbit.view();
        self.assemble_viewport_scene_with_styles(&view, styles)
    }
}

/// Ticket 54 last resort: eframe only reaches this path when it has no wgpu
/// render state at paint time. Show readable text over the dark viewport
/// instead of a blank rectangle, with a Reload button on wasm. The failure is
/// logged once (not once per frame), and there is deliberately no WebGL2 or
/// CPU renderer behind it.
fn draw_webgpu_fallback(ui: &mut egui::Ui, rect: egui::Rect) {
    #[cfg(target_arch = "wasm32")]
    const HINT: &str = "Reload, or see the browser help for enabling it.";
    #[cfg(not(target_arch = "wasm32"))]
    const HINT: &str = "This build needs a working WebGPU adapter.";

    static LOGGED: std::sync::Once = std::sync::Once::new();
    LOGGED.call_once(|| {
        log::error!("WebGPU is unavailable at paint time: eframe has no wgpu render state");
    });

    {
        let painter = ui.painter();
        painter.rect_filled(rect, 8.0, egui::Color32::from_rgb(30, 33, 41));
        let center = rect.center();
        painter.text(
            center - egui::vec2(0.0, 24.0),
            egui::Align2::CENTER_CENTER,
            "WebGPU is unavailable.",
            egui::FontId::proportional(20.0),
            egui::Color32::from_rgb(232, 236, 243),
        );
        painter.text(
            center - egui::vec2(0.0, 2.0),
            egui::Align2::CENTER_CENTER,
            HINT,
            egui::FontId::proportional(14.0),
            egui::Color32::from_rgb(200, 208, 220),
        );
    }

    #[cfg(target_arch = "wasm32")]
    {
        let button_rect = egui::Rect::from_center_size(
            rect.center() + egui::vec2(0.0, 40.0),
            egui::vec2(104.0, 32.0),
        );
        if ui.put(button_rect, egui::Button::new("Reload")).clicked() {
            reload_page();
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn reload_page() {
    if let Some(window) = web_sys::window() {
        let _ = window.location().reload();
    }
}

/// Bridges the installed live `DrawState` to the animation job's synchronous
/// native renderer. It borrows the renderer for exactly one `pump`.
#[cfg(not(target_arch = "wasm32"))]
struct NativeAnimationRenderer<'a> {
    draw: &'a mut crate::renderer::DrawState,
    device: &'a eframe::wgpu::Device,
    queue: &'a eframe::wgpu::Queue,
}

#[cfg(not(target_arch = "wasm32"))]
impl crate::animation_job::AnimationRenderer for NativeAnimationRenderer<'_> {
    fn render(
        &mut self,
        frame: crate::animation_scene::AnimationFrame,
        width: u32,
        height: u32,
        clear: [f64; 4],
    ) -> Result<Vec<u8>, AnimationError> {
        self.draw
            .render_offscreen(
                self.device,
                self.queue,
                &frame.scene,
                frame.view_proj,
                frame.eye,
                width,
                height,
                clear,
            )
            .map_err(|error| AnimationError::Encoder(error.to_string()))
    }
}
