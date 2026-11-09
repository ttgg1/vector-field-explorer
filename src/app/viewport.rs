//! Central viewport: camera input, click and double-click handling, scene
//! painting, screenshots, and the fly crosshair.

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

            if !self.renderer_ready {
                let mut renderer = render_state.renderer.write();
                renderer
                    .callback_resources
                    .insert(DrawState::new(&render_state.device, render_state.target_format));
                self.renderer_ready = true;
                log::info!("wgpu renderer initialized");
            }

            // Free flight drives the camera directly: held WASD/QE keys step
            // along the look basis, primary drag looks, the wheel scales speed.
            // Everything else (orbit/pan/zoom gestures) is bypassed so the
            // orbit camera keeps the pose the exit will restore.
            let resp = ui.allocate_response(rect.size(), egui::Sense::click_and_drag());
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
                let look = frame_look(captured, raw, drag);
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
                let pointer_delta = resp.drag_delta();
                let pointer_delta = Vec2::new(pointer_delta.x, pointer_delta.y);
                let buttons = DragButtons {
                    primary: resp.dragged_by(egui::PointerButton::Primary),
                    secondary: resp.dragged_by(egui::PointerButton::Secondary),
                    middle: resp.dragged_by(egui::PointerButton::Middle),
                    shift: ui.ctx().input(|input| input.modifiers.shift),
                };
                let channel = drag_channel(self.state.mode_3d, buttons);
                let (drag, pan) = match channel {
                    Some(DragChannel::Orbit) => (pointer_delta, Vec2::ZERO),
                    Some(DragChannel::Pan) => (Vec2::ZERO, pointer_delta),
                    None => (Vec2::ZERO, Vec2::ZERO),
                };
                let zoom = if resp.hovered() || resp.dragged() {
                    crate::ui::top_bar::wheel_zoom(ui.ctx())
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
                    let icon = match channel {
                        Some(DragChannel::Orbit) => egui::CursorIcon::Grabbing,
                        Some(DragChannel::Pan) => egui::CursorIcon::Move,
                        None if self.state.mode_3d => egui::CursorIcon::Grab,
                        None => egui::CursorIcon::Move,
                    };
                    ui.ctx().set_cursor_icon(icon);
                }
            }

            let aspect = rect.width().max(1.0) / rect.height().max(1.0);

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
                        if self.state.mode_3d { None } else { Some(self.state.plane_size()) },
                    ),
                    camera.eye_position(!self.state.mode_3d),
                )
            };
            // Ticket 62: the combined stream is the single sorted source for
            // 3D. The viewmodel's arrow order stays frame-invariant (no
            // write-back), so equal-depth ties are deterministic; this vector
            // is built before the screenshot block and consumed by both the
            // capture and the viewport. 2D keeps its arrows-then-dots path and
            // never reads the combined stream, but it still paints a stable
            // far-to-near copy of the arrows (the pre-ticket-62 order).
            let combined = if self.state.mode_3d {
                let mut instances = combined_instances(
                    self.scene.arrows.as_slice(),
                    self.scene.dots.as_slice(),
                );
                sort_scene_back_to_front(&mut instances, &view);
                Arc::new(instances)
            } else {
                Arc::new(Vec::new())
            };
            let arrows = if self.state.mode_3d {
                Arc::clone(&self.scene.arrows)
            } else {
                Arc::new(sorted_arrows_back_to_front(self.scene.arrows.as_slice(), &view))
            };
            // Trails are 3D-only and painted behind the combined stream, so
            // their own stable far-to-near order is enough (ticket 48).
            let trails = if self.state.mode_3d {
                Arc::new(sorted_lines_back_to_front(self.scene.trails.as_slice(), &view))
            } else {
                Arc::new(Vec::new())
            };
            self.scene.combined = Arc::clone(&combined);

            let scene = SceneGPU {
                arrows,
                lines: Arc::clone(&self.scene.lines),
                trails,
                dots: Arc::clone(&self.scene.dots),
                combined,
                arrow_style: self.scene.arrow_style,
                arrow_style_2d: self.scene.arrow_style_2d,
                arrow_params: self.scene.arrow_params,
                mode_3d: self.state.mode_3d,
            };

            // Screenshot export (ticket 08): re-render this frame's scene
            // offscreen at the viewport's pixel size (capped at 4096 on the
            // long edge, both axes scaled so the framing is unchanged) and
            // hand the pixels to a worker thread that shows the native save
            // dialog and writes the PNG. Nothing here touches the live
            // surface textures, and the readback is RGBA by contract.
            if self.screenshot.take_request() {
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
                        Ok(rgba) => screenshot::spawn_save(
                            self.screenshot.sender(),
                            ui.ctx().clone(),
                            rgba,
                            capture_width,
                            capture_height,
                            filename,
                        ),
                        Err(error) => self.screenshot.fail(error),
                    }
                }

                #[cfg(target_arch = "wasm32")]
                {
                    let tx = self.screenshot.sender();
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
                                        tx,
                                        ctx,
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
                let stroke = egui::Stroke::new(
                    1.0,
                    egui::Color32::from_white_alpha(CROSSHAIR_ALPHA),
                );
                for (from, to) in crosshair_segments(rect.center()) {
                    ui.painter().line_segment([from, to], stroke);
                }
            }
        });
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
