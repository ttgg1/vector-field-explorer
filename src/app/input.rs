//! Frame input: menu actions, command dispatch, keyboard handling, pointer
//! capture, and click seeding.

use super::*;

impl VfApp {
    /// Frame phase: screenshot completions, menu shortcuts, the transport key,
    /// the clock, camera keys, and the help keys. Captures the domain before
    /// anything can edit it so the camera can follow a moved box this frame.
    pub(super) fn frame_input(&mut self, ctx: &egui::Context) -> Domain {
        // Screenshot completions arrive on a channel from the save thread; pick
        // them up here so the toast reflects the latest attempt this frame.
        self.screenshot.tick(Instant::now());

        // Menu shortcuts (ticket 15): one table in `ui::menu`, consumed before
        // any widget so a focused text field never receives a key the menu
        // owns. Ctrl+Q is the one global shortcut (it quits from anywhere);
        // view state is already persisted, so there is no save prompt.
        //
        // Ticket 47: every domain change this frame starts here. The Controls
        // panel's extent editor (and `Edit > Reset all controls`, which lands
        // during menu handling) can move the sampled box; remember it before
        // any of that runs so the camera can follow the new centre once the
        // sidebar's `apply` has committed the change below.
        let domain_before = self.state.domain;

        // Ticket 65: a window that loses focus leaves fly immediately so the
        // locked/hidden pointer is never stranded in the background. The
        // capture release follows later this frame from `sync_fly_capture`.
        ctx.input(|input| {
            exit_on_focus_loss(&mut self.fly, &mut self.fly_keys, &input.events);
        });

        for action in menu::shortcut_actions(ctx) {
            self.apply_menu_action(action, ctx);
        }

        // Space is the transport's play/pause (ticket 26). The helper is inert
        // while a text field owns the keyboard or no clock consumer exists,
        // and consumes the key so a focused widget never also activates.
        crate::ui::transport::toggle_on_space(ctx, &self.state, &mut self.clock);

        // One global clock advances once per frame from egui's stable frame
        // time, but only while something consumes it. `advance_for` clamps to
        // MAX_STEP, ignores non-finite frames, holds the held time while
        // particles are off/in 2D or paused, and resumes from that time when a
        // consumer returns. A long idle gap still cannot make the particle
        // cloud jump.
        self.advance_clock(ctx);

        // Camera keys (ticket 18): arrows, `+`/`-`, `R` and `F` feed the same
        // view-command seam as the double-click and the camera buttons. The
        // loop is inert while a text field has focus, so the formula bar keeps
        // every key it needs (the menu's global shortcuts already ran above).
        for command in view_command::view_key_commands(ctx, &self.orbit, self.view_context()) {
            self.apply_view_command(command);
        }

        // `?` is the second help key (F1 goes through the menu table); Esc
        // dismisses the transient help surfaces. Neither fires while typing.
        if menu::help_key_pressed(ctx) {
            self.layout.toggle_help();
        }
        if !ctx.text_edit_focused() && ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.appearance_open = false;
            self.help_open = false;
            self.layout.help_visible = false;
        }
        domain_before
    }

    /// Apply one menu or shortcut action through the pure seam, then run the
    /// side effect that needs the app (popover, capture, clipboard, quit).
    /// Everything the seam can mutate goes in through [`menu::UiState`], so
    /// there is no second copy of the layout, camera or controls records.
    pub(super) fn apply_menu_action(&mut self, action: MenuAction, ctx: &egui::Context) {
        let effect = menu::apply_action(
            action,
            &mut menu::UiState {
                layout: &mut self.layout,
                orbit: &mut self.orbit,
                fly: &mut self.fly,
                camera: &mut self.camera,
                state: &mut self.state,
                controls: &mut self.sidebar,
                clock: &mut self.clock,
            },
        );
        match effect {
            menu::SideEffect::None => {}
            menu::SideEffect::OpenAppearance => self.appearance_open = true,
            menu::SideEffect::ShowAbout => self.about_open = true,
            menu::SideEffect::ShowHelp => self.help_open = true,
            // The capture itself happens in the viewport, where this frame's
            // rect and camera are known.
            menu::SideEffect::Screenshot => self.screenshot.request(),
            menu::SideEffect::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            menu::SideEffect::CopyText(text) => ctx.copy_text(text),
            menu::SideEffect::ResetLayout => {
                // New panel ids forget egui's remembered drag width, so the
                // default widths really apply.
                self.panel_generation = self.panel_generation.wrapping_add(1);
            }
            menu::SideEffect::ResetCamera => self.presets.clear_active(),
        }
    }

    /// The current view context for the command seam: which projection is
    /// showing, which domain it frames, and the viewport's aspect ratio. The
    /// aspect comes from the last painted viewport (keyboard and panel buttons
    /// run before the central panel), so a resize takes effect one frame later.
    fn view_context(&self) -> ViewContext {
        let aspect = self
            .last_viewport_rect
            .map(|rect| rect.width().max(1.0) / rect.height().max(1.0))
            .unwrap_or(1.0);
        ViewContext::new(self.state.mode_3d, self.state.domain, aspect)
    }

    /// Apply one camera command from any input surface (keyboard, double-click,
    /// camera buttons, menu). Reset and fit invalidate the highlighted preset:
    /// the camera no longer matches the saved view.
    pub(super) fn apply_view_command(&mut self, command: ViewCommand) {
        let context = self.view_context();
        self.orbit = view_command::apply_view_command_with_fly(
            self.orbit,
            &mut self.fly,
            context,
            command,
        );
        if matches!(command, ViewCommand::Reset | ViewCommand::Fit) {
            self.presets.clear_active();
        }
    }

    /// Accumulate this frame's fly keys from the raw key events. `keys_down` is
    /// logical-key only, so the physical QWERTY position is taken first; the
    /// app also clears the set on focus loss and on Esc, where Esc leaves fly
    /// mode. Inert unless fly is active, so movement keys never leak into the
    /// orbit view.
    pub(super) fn update_fly_keys(&mut self, ctx: &egui::Context) {
        if self.fly.is_none() || ctx.text_edit_focused() {
            // Never steal keys from the formula bar: typing WASD must not fly,
            // and Esc belongs to the text field while it has focus.
            self.fly_keys.clear();
            return;
        }
        // Ctrl/Cmd/Alt chords belong to menus and the browser; holding one
        // clears the set so Ctrl+S or Cmd+W never move the camera.
        if is_command_chord(ctx.input(|input| input.modifiers)) {
            self.fly_keys.clear();
            return;
        }
        let mut exit = false;
        {
            let keys = &mut self.fly_keys;
            ctx.input(|input| {
                for event in &input.events {
                    match event {
                        egui::Event::WindowFocused(false) => keys.clear(),
                        egui::Event::Key { key, physical_key, pressed, .. } => {
                            if *pressed && *key == egui::Key::Escape {
                                exit = true;
                            }
                            keys.apply_key(*key, *physical_key, *pressed);
                        }
                        _ => {}
                    }
                }
            });
        }
        if exit {
            self.fly = None;
            self.fly_keys.clear();
        }
    }

    /// Edge-triggered pointer capture for fly mode (ticket 65). Capture is
    /// best-effort: fly never depends on it, and drag-look remains the
    /// fallback when the platform refuses the locked grab (winit's X11 backend
    /// never locks; some compositors/browsers refuse it) or the web bridge is
    /// missing. While captured, a text field taking focus exits fly so a
    /// hidden pointer cannot be stranded in the formula bar; on the web a lost
    /// pointer lock (browser Esc) does the same.
    pub(super) fn sync_fly_capture(&mut self, ctx: &egui::Context) {
        let text_focused = ctx.text_edit_focused();
        #[cfg(target_arch = "wasm32")]
        let lock_lost = crate::pointer_lock::lock_lost();
        #[cfg(not(target_arch = "wasm32"))]
        let lock_lost = false;
        if capture_exit_required(self.capture_active, text_focused, lock_lost) {
            self.fly = None;
            self.fly_keys.clear();
        }

        let desired = self.fly.is_some() && self.state.is_3d();
        if let Some((grabs, visible)) = capture_transition(self.capture_active, desired) {
            #[cfg(not(target_arch = "wasm32"))]
            {
                // Confined first, then Locked: X11 refuses Locked and keeps
                // the confinement; other platforms upgrade to a real lock.
                for grab in grabs {
                    ctx.send_viewport_cmd(egui::ViewportCommand::CursorGrab(*grab));
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::CursorVisible(visible));
            }
            #[cfg(target_arch = "wasm32")]
            {
                let _ = (grabs, visible);
                if desired {
                    crate::pointer_lock::request();
                } else {
                    crate::pointer_lock::release();
                    // Drop any motion accumulated before the exit.
                    let _ = crate::pointer_lock::take_delta();
                }
            }
            self.capture_active = desired;
            if desired {
                // The click that checked the sidebar box must not also seed
                // the field at the crosshair this frame.
                self.suppress_capture_click = true;
            }
        }

        #[cfg(target_arch = "wasm32")]
        if self.capture_active {
            // Locked-pointer motion does not arrive as egui events, so keep a
            // frame coming to drain the bridge even without movement keys.
            ctx.request_repaint();
        }
    }

    /// Turn one viewport click into a deterministic streamline burst (and a
    /// particle burst when particles are on). The pure entry point maps the
    /// pointer position onto the camera ray (3D, at the focal plane) or the 2D
    /// plane; the viewmodel retraces the bounded burst list.
    pub(super) fn seed_click(&mut self, pos: egui::Pos2, rect: egui::Rect, aspect: f32, time: f64) {
        let width = rect.width().max(1.0);
        let height = rect.height().max(1.0);
        let ndc = Vec2::new(
            (pos.x - rect.left()) / width * 2.0 - 1.0,
            1.0 - (pos.y - rect.top()) / height * 2.0,
        );
        let ortho = if self.state.mode_3d { None } else { Some(self.state.plane_size()) };
        let view_camera = match &self.fly {
            Some(fly) => ViewCamera::Fly(fly),
            None => ViewCamera::Orbit(&self.orbit),
        };
        let Some(seed) = click_seed(view_camera, &self.state.domain, aspect, ndc, ortho) else {
            // A click that lands off the domain plants nothing, so it cannot
            // be the first half of a seeding double-click either.
            self.last_click_seed = None;
            return;
        };
        // Streamlines off hides both seed sources and refuses new seeds, but
        // the click's particle burst is out of scope (ticket 60 owns the
        // left-click action): it still happens below. An off-state click is
        // recorded as unstored so a following double-click cannot delete a
        // retained seed that a deterministic click at the same point matches.
        let stored = self.state.accepts_click_seed();
        if stored {
            if self.click_seeds.len() >= MAX_CLICK_SEEDS {
                self.click_seeds.remove(0);
            }
            self.click_seeds.push(seed);
            self.click_lines =
                self.state.click_streamlines_snapshot(self.dots_range, &self.click_seeds);
            self.streamline_count = self.flow_lines.len() + self.click_lines.len();
            self.scene.lines = Arc::new(compose_lines(
                &self.flow_lines,
                &self.click_lines,
                &self.state.domain,
                self.state.is_3d(),
            ));
            log::trace!(
                "click seed {} of {} at {seed:?}: {} click streamline segments",
                self.click_seeds.len(),
                MAX_CLICK_SEEDS,
                self.click_lines.len()
            );
        }
        // Particles only exist in the 3D view; a click adds a capped burst
        // around the pick (the per-frame dot rebuild picks it up next frame).
        // If the cloud has not spawned yet, fill it first so the click does not
        // leave a cloud of just this burst. The count after that ambient spawn
        // is what a double-click truncates back to.
        let mut particles_before = particle_count(&self.particles);
        if let Some(target) = self.state.particle_target() {
            if particle_count(&self.particles) == 0 {
                spawn(&mut self.particles, &self.state.domain, target, 3);
            }
            particles_before = particle_count(&self.particles);
            spawn_at(
                &mut self.particles,
                &self.state.domain,
                seed,
                CLICK_BURST_PARTICLES,
                3,
            );
        }
        self.last_click_seed = Some(LastClickSeed { seed, particles_before, time, stored });
    }

    /// Take back the burst the previous single click planted when it turns out
    /// to be the first half of a double-click: the double-click resets the view
    /// and must not leave a seed behind. The decision and the seed-list edit
    /// live in the pure [`undo_click_seed`] seam; this wrapper applies the
    /// particle truncation and recomposes the line collection.
    pub(super) fn undo_last_click_seed(&mut self, now: f64) {
        let undo = undo_click_seed(
            &self.click_seeds,
            self.last_click_seed.take(),
            now,
            particle_count(&self.particles),
        );
        // The particle rollback is independent of the seed edit: an unstored
        // click still burst particles and must still truncate them, even
        // though it removed no list entry.
        if self.state.particles
            && self.state.mode_3d
            && let Some(before) = undo.particles
        {
            self.particles.pos.truncate(before);
        }
        if !undo.removed {
            return;
        }
        self.click_seeds = undo.seeds;
        self.click_lines = self.state.click_streamlines_snapshot(self.dots_range, &self.click_seeds);
        self.streamline_count = self.flow_lines.len() + self.click_lines.len();
        self.scene.lines = Arc::new(compose_lines(
            &self.flow_lines,
            &self.click_lines,
            &self.state.domain,
            self.state.is_3d(),
        ));
    }
}
