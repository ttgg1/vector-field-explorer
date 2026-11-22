//! Frame input: menu actions, command dispatch, keyboard handling, pointer
//! capture, and click seeding.

use super::*;

use glam::Vec4;

use crate::seed_layout::{SeedAnchor, SeedPoint, clamp_seed_local};

/// How close a viewport click must land to a seed to pick it, in points.
const SEED_PICK_RADIUS: f32 = 12.0;

impl VfApp {
    /// Frame phase: screenshot completions, menu shortcuts, the transport key,
    /// the clock, camera keys, and the help keys. Captures the domain before
    /// anything can edit it so the camera can follow a moved box this frame.
    pub(super) fn frame_input(&mut self, ctx: &egui::Context) -> Domain {
        // Issue #208: input/picking below must use the same display basis the
        // viewport draws. The assignment is display-only and never dirties.
        self.orbit.up_axis = self.state.up_axis;
        // #99 slice B: the guided tour owns Enter/Right, Left and Escape before
        // any menu, viewport or camera handler sees them. A focused text field
        // keeps every key, so editing is never interrupted.
        self.handle_tour_keys(ctx);
        self.sync_tour_snapshot();

        // Screenshot completions arrive on a channel from the save thread; pick
        // them up here so the toast reflects the latest attempt this frame.
        self.screenshot.tick(Instant::now());

        // Task 6: reconcile an external/programmatic plot transition before any
        // input or simulation consumer runs. Capture release follows in the
        // same frame through the existing edge-triggered path.
        self.reconcile_plot_transition();
        self.sync_fly_capture(ctx);

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

        // Issue #88 A3: while the Interaction panel is capturing a new chord,
        // none of the normal key handlers may also act. The panel consumes the
        // key itself when it draws later this frame.
        let capture_armed = self.interaction_panel.capturing.is_some();

        // Ticket 70: while flying, global menu shortcuts are suppressed so a
        // movement key (physical A/D/...) can never fire a toggle (e.g.
        // ToggleFly on a non-QWERTY layout). Esc still exits through the
        // existing path below.
        if !capture_armed && self.fly.is_none() {
            for action in menu::shortcut_actions(ctx, &self.interaction.bindings) {
                self.apply_menu_action(action, ctx);
            }
        }

        // Ticket 70: layout-aware fly toggle. Match on the physical key when
        // the backend provides it (native), falling back to the logical key
        // (web), mirroring `FlyKeys::apply_key`. This is what makes the
        // toggle fire on the key physically in the C position instead of on
        // whatever key happens to report logical C on a non-QWERTY layout.
        if !capture_armed
            && !ctx.text_edit_focused()
            && self.state.is_vector_mode()
            && layout::fly_allowed(self.layout_mode)
        {
            // Ticket 70: match on the physical key when the backend provides it
            // (native), falling back to the logical key (web). Ignore key-repeat
            // events so a held key cannot toggle fly on/off/on. Consume the
            // logical key of the event that actually matched, not a hardcoded C.
            let toggle_key = ctx.input(|input| crate::fly_camera::fly_toggle_key(&input.events));
            if let Some(key) = toggle_key {
                ctx.input_mut(|input| {
                    input.consume_key(egui::Modifiers::NONE, key);
                });
                crate::fly_camera::toggle_fly(
                    &mut self.fly,
                    &self.orbit,
                    self.state.is_3d(),
                    self.controls.sensitivity,
                );
            }
        }

        // Play/pause is the transport's bound chord (Space by default). The
        // helper is inert while a text field owns the keyboard or no clock
        // consumer exists, and consumes the key so a focused widget never also
        // activates.
        if !capture_armed {
            crate::ui::transport::toggle_on_bindings(
                ctx,
                &self.state,
                &mut self.clock,
                &self.interaction.bindings,
            );
        }

        // Ticket 60: the bound seed/particle chords act at the pointer (viewport
        // centre as fallback). Runs after the menu and transport tables so
        // their chords win, and before the camera keys; Seed is inert while
        // flying because it is fly's backward key.
        if !capture_armed {
            self.apply_seed_shortcuts(ctx);
        }

        // One global clock advances once per frame from egui's stable frame
        // time, but only while something consumes it. An animation export owns
        // the only animation clock that may run: force live playback paused
        // before advancing so a transport click during export cannot resume it.
        if self.animation_job.is_some() {
            self.clock.playing = false;
        }
        self.advance_clock(ctx.input(|input| input.stable_dt));

        // Camera keys (ticket 18): the bound view chords feed the same
        // view-command seam as the double-click and the camera buttons. The
        // loop is inert while a text field has focus, so the formula bar keeps
        // every key it needs (the menu's global shortcuts already ran above).
        if !capture_armed {
            for command in view_command::view_key_commands_with_bindings(
                ctx,
                &self.orbit,
                self.view_context(),
                &self.interaction.bindings,
                self.interaction.camera_feel,
            ) {
                self.apply_view_command(command);
            }
        }

        // `?` is the second help key (F1 goes through the binding table); Esc
        // dismisses the transient help surfaces. Neither fires while typing.
        if !capture_armed && menu::help_key_pressed(ctx) {
            self.layout.toggle_help();
        }
        if !capture_armed
            && !ctx.text_edit_focused()
            && ctx.input(|input| input.key_pressed(egui::Key::Escape))
        {
            self.appearance_open = false;
            self.help_open = false;
            self.layout.help_visible = false;
            // Issue #228: Escape always returns the viewport click tool to
            // Inspect and cancels any armed one-shot placement.
            crate::ui::viewport_tool::inspect(&mut self.controls);
        }
        domain_before
    }

    /// Capture the export context for the active adapter now. Formula mode has
    /// none; Sources, Induction and Ode always produce a warning-bearing
    /// provenance, even before sampling or a lazy solve. Induction uses the
    /// shared scene softening; Ode reads the durable scene, the cached solution
    /// and the shared clock and never re-integrates.
    pub(super) fn field_export_provenance(
        &self,
    ) -> Option<crate::export_provenance::ExportProvenance> {
        match self.state.field_mode {
            FieldMode::Formula => None,
            FieldMode::Maxwell => crate::export_provenance::maxwell_export_provenance(
                self.state.field_mode,
                &self.state.maxwell,
                self.state.maxwell_sampling.as_ref(),
            ),
            FieldMode::Ode => crate::export_provenance::ode_export_provenance(
                self.state.field_mode,
                &self.state.ode,
                self.state.ode_solution.as_deref(),
                self.state.ode_error.as_deref(),
                &self.clock,
            ),
            FieldMode::Sources => crate::export_provenance::source_export_provenance(
                self.state.field_mode,
                &self.state.sources,
                self.state.source_sampling(),
            ),
            // Materials export provenance is request-time and pure: it reads
            // the durable scene and cached sampling, never re-samples.
            FieldMode::Materials => Some(crate::export_provenance::material_export_provenance(
                self.state.field_mode,
                &self.state.material,
                self.state.material_sampling.as_ref(),
            )),
            FieldMode::Induction => {
                let softening = self.state.sources.softening;
                // A cached pass built at an older softening cannot be
                // presented as the current model. Treat it as unavailable
                // rather than pairing stale counts with the new `a` value.
                let sampling = self
                    .state
                    .induction_sampling
                    .as_ref()
                    .filter(|sampling| sampling.softening == softening);
                crate::export_provenance::induction_export_provenance(
                    self.state.field_mode,
                    &self.state.induction,
                    softening,
                    sampling,
                )
            }
        }
    }

    /// Queue a PNG request with the provenance captured at request time.
    pub(super) fn request_png_export(&mut self) {
        if self.animation_job.is_some() {
            self.screenshot
                .notify("Wait for the animation export to finish.".into(), true);
            return;
        }
        let provenance = self.field_export_provenance();
        self.screenshot.request_with_provenance(provenance);
    }

    /// Queue an SVG request behind the shared execution guard, with the
    /// provenance captured at request time.
    pub(super) fn request_svg_export(&mut self) {
        if self.animation_job.is_some() {
            self.screenshot
                .notify("Wait for the animation export to finish.".into(), true);
            return;
        }
        if let Some(reason) = self.svg_block_reason() {
            self.screenshot.notify(reason, true);
            return;
        }
        let provenance = self.field_export_provenance();
        self.screenshot.request_svg_with_provenance(provenance);
    }

    /// Apply one menu or shortcut action through the pure seam, then run the
    /// side effect that needs the app (popover, capture, clipboard, quit).
    /// Everything the seam can mutate goes in through [`menu::UiState`], so
    /// there is no second copy of the layout, camera or controls records.
    pub(super) fn apply_menu_action(&mut self, action: MenuAction, ctx: &egui::Context) {
        if action == MenuAction::ToggleFly && !layout::fly_allowed(self.layout_mode) {
            return;
        }
        // Ticket 53: in compact the side panels are bottom sheets, so the two
        // panel toggles must not flip the persisted wide-layout flags. Route
        // them to the transient sheet; everything else dispatches as usual.
        if self.layout_mode == LayoutMode::Compact {
            match action {
                MenuAction::ToggleControls => {
                    self.sheet.toggle(SheetState::Controls);
                    self.sheet_expanded = SheetSize::Default;
                    ctx.request_repaint();
                    return;
                }
                MenuAction::TogglePresets => {
                    self.sheet.toggle(SheetState::Presets);
                    self.sheet_expanded = SheetSize::Default;
                    ctx.request_repaint();
                    return;
                }
                MenuAction::ToggleAppearancePanel => {
                    self.sheet.toggle(SheetState::Appearance);
                    self.sheet_expanded = SheetSize::Default;
                    ctx.request_repaint();
                    return;
                }
                MenuAction::ToggleInteractionPanel => {
                    self.sheet.toggle(SheetState::Interaction);
                    self.sheet_expanded = SheetSize::Default;
                    ctx.request_repaint();
                    return;
                }
                MenuAction::OpenInteraction => {
                    self.sheet.open(SheetState::Interaction);
                    self.sheet_expanded = SheetSize::Default;
                    ctx.request_repaint();
                    return;
                }
                #[cfg(not(target_arch = "wasm32"))]
                MenuAction::ToggleCas => {
                    self.sheet.toggle(SheetState::Cas);
                    self.sheet_expanded = SheetSize::Default;
                    ctx.request_repaint();
                    return;
                }
                _ => {}
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        if action == MenuAction::ResetAllControls {
            self.cas.reset();
        }
        let effect = menu::apply_action(
            action,
            &mut menu::UiState {
                layout: &mut self.layout,
                orbit: &mut self.orbit,
                fly: &mut self.fly,
                camera: &mut self.camera,
                state: &mut self.state,
                controls: &mut self.controls,
                clock: &mut self.clock,
            },
        );
        match effect {
            menu::SideEffect::None => {}
            menu::SideEffect::OpenAppearance => self.appearance_open = true,
            menu::SideEffect::ShowAbout => self.about_open = true,
            menu::SideEffect::ShowHelp => self.help_open = true,
            menu::SideEffect::StartTour => {
                self.start_or_restart_tour();
                self.sync_tour_snapshot();
            }
            // The capture itself happens in the viewport, where this frame's
            // rect and camera are known.
            menu::SideEffect::Screenshot => self.request_png_export(),
            menu::SideEffect::ScreenshotSvg => self.request_svg_export(),
            menu::SideEffect::Animation => self.open_animation_dialog(ctx),
            menu::SideEffect::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            menu::SideEffect::CopyText(text) => ctx.copy_text(text),
            // Help > Download desktop app (ticket 16a): hand the static page to
            // the platform opener; on wasm this is a new browser tab.
            menu::SideEffect::OpenUrl(url) => ctx.open_url(egui::OpenUrl::new_tab(url)),
            menu::SideEffect::ResetLayout => {
                // New panel ids forget egui's remembered drag width, so the
                // default widths really apply. The compact sheet closes through
                // the transient helper: reset never writes sheet/mode state.
                self.panel_generation = self.panel_generation.wrapping_add(1);
                layout::reset_sheet(&mut self.sheet);
                ctx.request_repaint();
            }
            menu::SideEffect::Undo => {
                self.undo_document(ctx);
            }
            menu::SideEffect::Redo => {
                self.redo_document(ctx);
            }
            // Every scoped reset (Color, Camera, Simulation, All) has one
            // implementation: the app seam, which also clears the preset
            // highlight the menu used to handle itself.
            menu::SideEffect::ResetScope(scope) => self.reset_scope(scope),
        }
        if action == MenuAction::ResetAllControls {
            // Task 6: the menu reset owns controls/clock; the app-owned half
            // cancels captures, returns to vector mode and empties plot state.
            self.reset_plot_state();
            // Sources are a separate durable scene: global reset empties it,
            // drops the cached sampling and returns to the formula adapter.
            // Plot-section reset deliberately leaves them alone. Induction
            // likewise restores its default scene and drops its cache; neither
            // touch the formula or the shared clock.
            self.state.reset_sources();
            self.state.reset_induction();
            // Global reset restores the default ODE scene and solution before
            // returning to Formula; a later ODE section reset is separate.
            self.state.reset_ode();
            // Materials is durable too: global reset clears its scene, cache
            // and edit error before returning to Formula.
            self.state.reset_material();
            // Maxwell is durable too; drop its scene, sampling, BEM cache and
            // error before returning to Formula.
            self.state.reset_maxwell();
            // Ode never loops: clear any shared period left over so reset
            // cannot leave a lingering Ode cadence behind.
            self.clock.loop_period = None;
            self.state.set_field_mode(FieldMode::Formula);
        }
    }

    /// The current view context for the command seam: which projection is
    /// showing, which domain it frames, and the viewport's aspect ratio. The
    /// aspect comes from the last painted viewport (keyboard and panel buttons
    /// run before the central panel), so a resize takes effect one frame later.
    pub(super) fn view_context(&self) -> ViewContext {
        let aspect = self
            .last_viewport_rect
            .map(|rect| rect.width().max(1.0) / rect.height().max(1.0))
            .unwrap_or(1.0);
        ViewContext::new(self.state.mode_3d, self.state.scene_domain(), aspect)
    }

    /// Apply one camera command from any input surface (keyboard, double-click,
    /// camera buttons, menu). Reset and fit invalidate the highlighted preset:
    /// the camera no longer matches the saved view.
    pub(super) fn apply_view_command(&mut self, command: ViewCommand) {
        if !self.state.is_vector_mode() {
            self.apply_plot_view_command(command);
            return;
        }
        let context = self.view_context();
        self.orbit =
            view_command::apply_view_command_with_fly(self.orbit, &mut self.fly, context, command);
        if command == ViewCommand::Fit {
            // The viewport tour stop waits on a real Fit (see `tour_control_value`).
            self.camera_fit_generation = self.camera_fit_generation.wrapping_add(1);
        }
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
        if self.fly.is_none()
            || ctx.text_edit_focused()
            || self.interaction_panel.capturing.is_some()
        {
            // Never steal keys from the formula bar or a chord capture: typing
            // WASD must not fly, and Esc belongs to the field/capture while it
            // has focus.
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
                        egui::Event::Key {
                            key,
                            physical_key,
                            pressed,
                            ..
                        } => {
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

        let desired = self.state.is_vector_mode() && self.fly.is_some() && self.state.is_3d();
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

    /// Run this frame's plain S/P shortcut, if any: S seeds a streamline and P
    /// adds the particle burst, each at the pointer or the viewport centre.
    /// Feature availability and the hinted no-op live in the same pure plan as
    /// the mouse click. A keyboard seed deliberately clears the mouse rollback
    /// record: it is not the first half of a double-click and must never let a
    /// later double-click take back an unrelated earlier mouse click.
    pub(super) fn apply_seed_shortcuts(&mut self, ctx: &egui::Context) {
        if !self.state.is_vector_mode() {
            // Scalar mode has no vector seeding or particle bursts.
            return;
        }
        let text_focused = ctx.text_edit_focused();
        let flying = self.fly.is_some();
        let bindings = &self.interaction.bindings;
        let shortcut = ctx.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Key {
                    key,
                    pressed: true,
                    repeat,
                    modifiers,
                    ..
                } => view_command::seed_shortcut_with_bindings(
                    bindings,
                    *key,
                    *modifiers,
                    *repeat,
                    text_focused,
                    flying,
                ),
                _ => None,
            })
        });
        let Some(action) = shortcut else {
            return;
        };
        let bound = match action {
            LeftClickAction::SeedStreamline => interaction::InteractionAction::SeedStreamline,
            LeftClickAction::AddParticles => interaction::InteractionAction::AddParticles,
        };
        if let Some(chord) = self.interaction.bindings.chord(bound) {
            let modifiers = view_command::egui_modifiers(chord);
            ctx.input_mut(|input| {
                input.consume_key(modifiers, chord.key);
            });
        }
        // The viewport rect is recorded by the central panel every frame, so a
        // first-frame shortcut has no target and does nothing.
        let Some(rect) = self.last_viewport_rect else {
            return;
        };
        let aspect = rect.width().max(1.0) / rect.height().max(1.0);
        // A captured/locked pointer is hidden and may not hover reliably; the
        // crosshair-centre fallback is the correct target there. Otherwise the
        // pointer seeds where the user is pointing.
        let pointer = if self.capture_active {
            None
        } else {
            ctx.input(|input| input.pointer.hover_pos())
        };
        let pos = view_command::shortcut_seed_point(pointer, rect);
        let time = ctx.input(|input| input.time);
        self.apply_click_action(action, pos, rect, aspect, time);
        self.last_click_seed = None;
    }

    /// Turn one viewport click into the selected left-click action (ticket 60):
    /// a single streamline seed or a particle burst, never both. The pure
    /// [`view_command::click_action_plan`] owns the feature gates and the
    /// one-line no-op hint; this wrapper owns the seed/burst side effects.
    pub(super) fn seed_click(&mut self, pos: egui::Pos2, rect: egui::Rect, aspect: f32, time: f64) {
        // Issue #92 slice B1: an armed "Place probe" plants one point probe on
        // the active plane and disarms; it never also seeds a streamline. If
        // the probe path disappeared (field mode or plot kind switched), disarm
        // it and report why instead of silently running the default click.
        if self.controls.scalar_place_probe {
            if self.scalar_probes_editable() {
                let placed = self.place_scalar_probe(pos, rect, aspect);
                if placed {
                    // Issue #228: the strip's placement mode owns what happens
                    // after a probe lands. A one-shot (or enable-and-place)
                    // returns to Inspect; "Keep placing" leaves the probe armed
                    // for the next click. The arming is always consumed.
                    let one_shot = crate::ui::viewport_tool::placement_is_one_shot(&self.controls);
                    self.controls.viewport_armed_one_shot = false;
                    if one_shot {
                        crate::ui::viewport_tool::inspect(&mut self.controls);
                    }
                }
            } else {
                self.controls.scalar_place_probe = false;
                self.controls.viewport_armed_one_shot = false;
                self.controls.scalar_message = "Probes are unavailable in this view".to_owned();
                restart_hint(
                    &mut self.status.hint,
                    &mut self.hint_elapsed,
                    "Probes are unavailable in this view",
                );
            }
            self.last_click_seed = None;
            return;
        }
        // Issue #87 S2: an armed "Place seed" plants one document seed and
        // disarms; it never also plants a runtime click burst.
        if self.controls.place_seed && self.seeds_editable() {
            self.place_explicit_seed(pos, rect, aspect);
            self.last_click_seed = None;
            return;
        }
        // A click on an existing explicit seed selects it instead of planting
        // a runtime click seed; clicks that miss keep today's behaviour.
        if self.seeds_editable() && !self.capture_active && self.select_seed_at(pos, rect, aspect) {
            self.last_click_seed = None;
            return;
        }
        // Issue #228: the click uses the derived effective tool, not a stale
        // selector. Inspect places nothing; a gated action is not effective and
        // the strip offers enable-and-place instead.
        let view = crate::ui::viewport_tool::derive_for(&self.state, &self.controls);
        let action = match view.selected {
            crate::ui::viewport_tool::ViewportTool::SeedStreamline => {
                LeftClickAction::SeedStreamline
            }
            crate::ui::viewport_tool::ViewportTool::AddParticles => LeftClickAction::AddParticles,
            crate::ui::viewport_tool::ViewportTool::Inspect
            | crate::ui::viewport_tool::ViewportTool::PlaceProbe => {
                self.last_click_seed = None;
                return;
            }
        };
        if self.apply_click_action(action, pos, rect, aspect, time) {
            let one_shot = crate::ui::viewport_tool::placement_is_one_shot(&self.controls);
            self.controls.viewport_armed_one_shot = false;
            if one_shot {
                self.controls.viewport_inspect = true;
            }
        }
    }

    /// Test seam: run the viewport click path without a wgpu frame. The
    /// headless harness returns before the viewport body, so tests that need to
    /// observe placement must call the same production entry directly.
    #[doc(hidden)]
    pub fn test_seed_click(&mut self, pos: egui::Pos2, rect: egui::Rect, aspect: f32, time: f64) {
        self.seed_click(pos, rect, aspect, time);
    }

    /// Whether seeds are editable in the current field/view: the same gate the
    /// Flow-section editor uses.
    pub(super) fn seeds_editable(&self) -> bool {
        self.state.is_vector_mode()
            && self.state.streamlines
            && self.state.field_mode == FieldMode::Formula
    }

    /// One viewport frame of explicit-seed pointer handling: begin a drag on a
    /// seed press, move it while the primary button is down, and report whether
    /// the camera must stay still for this gesture.
    pub(super) fn seed_pointer_interaction(
        &mut self,
        resp: &egui::Response,
        rect: egui::Rect,
        aspect: f32,
    ) -> bool {
        let pointer_pos = resp.ctx.input(|input| input.pointer.interact_pos());
        let primary_down = resp.ctx.input(|input| input.pointer.primary_down());
        if self.seeds_editable()
            && self.fly.is_none()
            && !self.capture_active
            && !self.controls.place_seed
            && resp.hovered()
            && resp
                .ctx
                .input(|input| input.pointer.button_pressed(egui::PointerButton::Primary))
            && let Some(pos) = pointer_pos
        {
            self.begin_seed_drag(pos, rect, aspect);
        }
        if self.controls.seed_drag.is_some() {
            let dragged = resp.dragged_by(egui::PointerButton::Primary);
            if dragged {
                self.controls.seed_drag_moved = true;
            }
            if (dragged || (!primary_down && self.controls.seed_drag_moved))
                && let Some(pos) = pointer_pos
            {
                self.update_seed_drag(pos, rect, aspect);
            }
        }
        let suppress_camera = self.controls.seed_drag.is_some();
        if suppress_camera && !primary_down {
            self.controls.seed_drag = None;
            self.controls.seed_drag_moved = false;
        }
        suppress_camera
    }

    /// Plant one explicit seed at a viewport pick on the active plane.
    fn place_explicit_seed(&mut self, pos: egui::Pos2, rect: egui::Rect, aspect: f32) {
        let Some(position) = self.click_world_seed(pos, rect, aspect) else {
            self.controls.seed_error = "Click inside the scene domain to place a seed".to_owned();
            return;
        };
        match self.state.place_seed(position) {
            Ok(()) => {
                self.controls.place_seed = false;
                self.controls.selected_seed = Some(self.state.seed_layout().len() - 1);
                self.controls.seed_error.clear();
            }
            Err(error) => {
                self.controls.seed_error = error.to_string();
                restart_hint(
                    &mut self.status.hint,
                    &mut self.hint_elapsed,
                    &format!("Seed not placed: {error}"),
                );
            }
        }
    }

    /// Whether point probes can be placed in the current view: the same
    /// applicability predicate the Scalar section and overlay use (issue #92
    /// slice B1).
    pub(super) fn scalar_probes_editable(&self) -> bool {
        self.state.scalar_view_applicable()
    }

    /// Plant one point probe at a viewport pick on the scalar plane. The pick
    /// maps through the active [`crate::plane::PlaneFrame`] and is rejected when
    /// it misses the scene domain or the shared probe cap is full. Returns
    /// whether a probe was actually placed.
    fn place_scalar_probe(&mut self, pos: egui::Pos2, rect: egui::Rect, aspect: f32) -> bool {
        let Some((position, anchor)) = self.probe_world_point(pos, rect, aspect) else {
            // A pick outside the scene domain is retryable, so the probe stays
            // armed for the next click; only the message reports the miss.
            self.controls.scalar_message =
                "Click inside the scene domain to place a probe".to_owned();
            return false;
        };
        match self.state.add_point_probe(position, anchor) {
            Ok(_) => {
                // The caller applies the placement mode: one-shot disarms via
                // `inspect`, sticky leaves the probe armed for another click.
                self.controls.scalar_message.clear();
                true
            }
            Err(error) => {
                // Retrying a cap rejection cannot succeed until a probe is
                // removed, so disarm and report; an out-of-domain miss above
                // stays armed.
                if matches!(
                    error,
                    crate::scalar_view::ScalarViewError::ProbeLimit { .. }
                ) {
                    self.controls.scalar_place_probe = false;
                    self.controls.viewport_armed_one_shot = false;
                }
                self.controls.scalar_message = error.to_string();
                false
            }
        }
    }

    /// Map a viewport point onto the scalar plane frame without clamping, then
    /// require it to lie inside the visible scene domain.
    fn probe_world_point(
        &self,
        pos: egui::Pos2,
        rect: egui::Rect,
        aspect: f32,
    ) -> Option<(Vec3, Option<SeedAnchor>)> {
        let width = rect.width().max(1.0);
        let height = rect.height().max(1.0);
        let ndc = Vec2::new(
            (pos.x - rect.left()) / width * 2.0 - 1.0,
            1.0 - (pos.y - rect.top()) / height * 2.0,
        );
        let frame = self.state.scalar_view_frame();
        let view_camera = match &self.fly {
            Some(fly) => ViewCamera::Fly(fly),
            None => ViewCamera::Orbit(&self.orbit),
        };
        let ortho = if self.state.plane_frame().is_some() || !self.state.mode_3d {
            Some(self.state.plane_size())
        } else {
            None
        };
        let inverse = view_camera.vp_matrix(aspect, ortho).inverse();
        if !inverse.is_finite() {
            return None;
        }
        let unproject = |z: f32| {
            let p = inverse * Vec4::new(ndc.x, ndc.y, z, 1.0);
            Vec3::new(p.x, p.y, p.z) / p.w
        };
        let near = unproject(-1.0);
        let far = unproject(1.0);
        let direction = far - near;
        let denominator = direction.dot(frame.normal);
        if !direction.is_finite() || !denominator.is_finite() || denominator.abs() < 1e-9 {
            return None;
        }
        let hit = near + direction * ((frame.origin - near).dot(frame.normal) / denominator);
        if !hit.is_finite() {
            return None;
        }
        // Snap the hit onto the frame to remove projection epsilon before the
        // scene-domain test (the 2D scene domain is the exact mid-z plane).
        let local = frame.to_local(hit);
        let hit = frame.to_world(Vec3::new(local.x, local.y, 0.0));
        let domain = self.state.scene_domain();
        if !hit.cmpge(domain.min).all() || !hit.cmple(domain.max).all() {
            return None;
        }
        let anchor = self.state.plane_frame().map(|frame| {
            let grid = self.state.plane_grid();
            let plane = self.state.plane.clamped(grid);
            SeedAnchor {
                axis: frame.axis,
                index: plane.index,
                u: local.x,
                v: local.y,
            }
        });
        Some((hit, anchor))
    }

    /// Project a stored seed to screen coordinates for picking.
    fn seed_screen_position(
        &self,
        position: Vec3,
        rect: egui::Rect,
        aspect: f32,
    ) -> Option<egui::Pos2> {
        let view_camera = match &self.fly {
            Some(fly) => ViewCamera::Fly(fly),
            None => ViewCamera::Orbit(&self.orbit),
        };
        let ortho = if self.state.mode_3d {
            None
        } else {
            Some(self.state.plane_size())
        };
        let vp = view_camera.vp_matrix(aspect, ortho);
        if !vp.is_finite() {
            return None;
        }
        let clip = vp * position.extend(1.0);
        if !clip.is_finite() || clip.w <= 0.0 {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        if !(-1.0..=1.0).contains(&ndc.x) || !(-1.0..=1.0).contains(&ndc.y) {
            return None;
        }
        Some(egui::pos2(
            rect.left() + (ndc.x * 0.5 + 0.5) * rect.width(),
            rect.top() + (0.5 - ndc.y * 0.5) * rect.height(),
        ))
    }

    /// The seed nearest `pos` within [`SEED_PICK_RADIUS`], if any.
    pub(super) fn pick_seed(
        &self,
        pos: egui::Pos2,
        rect: egui::Rect,
        aspect: f32,
    ) -> Option<usize> {
        self.state
            .seed_layout()
            .seeds
            .iter()
            .enumerate()
            .filter_map(|(index, seed)| {
                let screen = self.seed_screen_position(seed.position, rect, aspect)?;
                let distance = screen.distance(pos);
                (distance <= SEED_PICK_RADIUS).then_some((distance, index))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, index)| index)
    }

    /// Select the seed nearest `pos`, clearing the selection when the click
    /// misses. Returns whether a seed was selected.
    pub(super) fn select_seed_at(
        &mut self,
        pos: egui::Pos2,
        rect: egui::Rect,
        aspect: f32,
    ) -> bool {
        match self.pick_seed(pos, rect, aspect) {
            Some(index) => {
                self.controls.selected_seed = Some(index);
                true
            }
            None => {
                self.controls.selected_seed = None;
                false
            }
        }
    }

    /// Begin a drag when the press lands on a seed. Returns whether the seed
    /// now owns the gesture, so the camera must not also orbit.
    pub(super) fn begin_seed_drag(
        &mut self,
        pos: egui::Pos2,
        rect: egui::Rect,
        aspect: f32,
    ) -> bool {
        let Some(index) = self.pick_seed(pos, rect, aspect) else {
            return false;
        };
        self.controls.selected_seed = Some(index);
        self.controls.seed_drag = Some(index);
        self.controls.seed_drag_moved = false;
        true
    }

    /// Move the dragged seed on the active plane with live clamps. Rejections
    /// leave the committed seed where it was and report why.
    pub(super) fn update_seed_drag(&mut self, pos: egui::Pos2, rect: egui::Rect, aspect: f32) {
        let Some(index) = self.controls.seed_drag else {
            return;
        };
        if index >= self.state.seed_layout().len() {
            self.controls.seed_drag = None;
            return;
        }
        if self.fly.is_some() || self.capture_active {
            // Fly or pointer capture can start mid-drag. Re-check the same
            // invariant the press path uses and end the gesture instead of
            // continuing to write while the camera owns the pointer.
            self.controls.seed_drag = None;
            self.controls.seed_drag_moved = false;
            return;
        }
        let Some((position, anchor)) = self.seed_drag_point(pos, rect, aspect) else {
            restart_hint(
                &mut self.status.hint,
                &mut self.hint_elapsed,
                "Seed move rejected: outside the editable plane",
            );
            return;
        };
        let mut layout = self.state.seed_layout().clone();
        let next = SeedPoint { position, anchor };
        if layout.seeds[index] == next {
            return;
        }
        layout.seeds[index] = next;
        if let Err(error) = self.state.commit_seed_layout(layout) {
            self.controls.seed_error = format!("Seed move rejected: {error}");
            restart_hint(
                &mut self.status.hint,
                &mut self.hint_elapsed,
                &format!("Seed move rejected: {error}"),
            );
        } else {
            self.controls.seed_error.clear();
        }
    }

    /// Test seam: drive one move of the explicit-seed drag without a wgpu
    /// viewport response.
    #[doc(hidden)]
    pub fn test_update_seed_drag(&mut self, pos: egui::Pos2, rect: egui::Rect, aspect: f32) {
        self.update_seed_drag(pos, rect, aspect);
    }

    /// Map a pointer position onto the active plane: a clamped local plane
    /// hit when a plane slice is selected, otherwise the existing camera-plane
    /// pick clamped into the scene domain.
    fn seed_drag_point(
        &self,
        pos: egui::Pos2,
        rect: egui::Rect,
        aspect: f32,
    ) -> Option<(Vec3, Option<SeedAnchor>)> {
        let width = rect.width().max(1.0);
        let height = rect.height().max(1.0);
        let ndc = Vec2::new(
            (pos.x - rect.left()) / width * 2.0 - 1.0,
            1.0 - (pos.y - rect.top()) / height * 2.0,
        );
        let view_camera = match &self.fly {
            Some(fly) => ViewCamera::Fly(fly),
            None => ViewCamera::Orbit(&self.orbit),
        };
        if let Some(frame) = self.state.plane_frame() {
            let vp = view_camera.vp_matrix(aspect, Some(self.state.plane_size()));
            let inverse = vp.inverse();
            if !inverse.is_finite() {
                return None;
            }
            let unproject = |z: f32| {
                let p = inverse * Vec4::new(ndc.x, ndc.y, z, 1.0);
                Vec3::new(p.x, p.y, p.z) / p.w
            };
            let near = unproject(-1.0);
            let far = unproject(1.0);
            let direction = far - near;
            let denominator = direction.dot(frame.normal);
            if !direction.is_finite() || !denominator.is_finite() || denominator.abs() < 1e-9 {
                return None;
            }
            let hit = near + direction * ((frame.origin - near).dot(frame.normal) / denominator);
            if !hit.is_finite() {
                return None;
            }
            let local = clamp_seed_local(&frame, frame.to_local(hit));
            let world = frame.to_world(local);
            if !world.is_finite() {
                return None;
            }
            let plane = self.state.plane.clamped(self.state.plane_grid());
            return Some((
                world,
                Some(SeedAnchor {
                    axis: frame.axis,
                    index: plane.index,
                    u: local.x,
                    v: local.y,
                }),
            ));
        }
        let ortho = if self.state.mode_3d {
            None
        } else {
            Some(self.state.plane_size())
        };
        let domain = self.state.scene_domain();
        let position = click_seed(view_camera, &domain, aspect, ndc, ortho)?;
        if !position.is_finite() {
            return None;
        }
        if !position.cmpge(domain.min).all() || !position.cmple(domain.max).all() {
            return None;
        }
        Some((position, None))
    }

    /// Apply one explicit click action at `pos`. Shared by the mouse and the
    /// S/P shortcuts; a shortcut passes its own action, so it works whatever
    /// the selector says. Returns whether the action actually placed something.
    pub(super) fn apply_click_action(
        &mut self,
        action: LeftClickAction,
        pos: egui::Pos2,
        rect: egui::Rect,
        aspect: f32,
        time: f64,
    ) -> bool {
        if !self.state.is_vector_mode() {
            // Plot mode never seeds or bursts, including the captured-pointer
            // and S/P routes that share this wrapper.
            self.last_click_seed = None;
            return false;
        }
        if matches!(
            self.state.field_mode,
            FieldMode::Sources
                | FieldMode::Induction
                | FieldMode::Materials
                | FieldMode::Ode
                | FieldMode::Maxwell
        ) {
            // These modes are static and Ode owns precomputed trajectories:
            // no streamline seeds and no particle bursts.
            self.last_click_seed = None;
            return false;
        }
        match view_command::click_action_plan(
            action,
            self.state.streamlines,
            self.state.particle_target().is_some(),
            self.state.mode_3d,
        ) {
            view_command::ClickActionPlan::Ignore(hint) => {
                // No seed, no burst. Set the hint only on this event and restart
                // its lifetime even when the same text is already live, so a
                // repeated no-op does not inherit the old countdown.
                restart_hint(&mut self.status.hint, &mut self.hint_elapsed, hint);
                self.last_click_seed = None;
                false
            }
            view_command::ClickActionPlan::SeedStreamline => {
                let Some(seed) = self.click_world_seed(pos, rect, aspect) else {
                    // A click that lands off the domain plants nothing, so it
                    // cannot be the first half of a seeding double-click either.
                    self.last_click_seed = None;
                    return false;
                };
                if self.click_seeds.len() >= MAX_CLICK_SEEDS {
                    self.click_seeds.remove(0);
                }
                self.click_seeds.push(seed);
                self.click_lines = self
                    .state
                    .click_streamlines_snapshot(self.dots_range, &self.click_seeds);
                self.streamline_count = self.flow_lines.len() + self.click_lines.len();
                if let Some(scene) = self.scene.as_vector_mut() {
                    scene.lines = Arc::new(compose_lines_with_frame_style(
                        &self.flow_lines,
                        &self.click_lines,
                        &self.state.scene_domain(),
                        self.state.is_3d(),
                        self.state.plane_frame(),
                        self.state.show_grid,
                        self.state.show_coordinate_system,
                        &self.state.reference_viewport_style(),
                    ));
                }
                log::trace!(
                    "click seed {} of {} at {seed:?}: {} click streamline segments",
                    self.click_seeds.len(),
                    MAX_CLICK_SEEDS,
                    self.click_lines.len()
                );
                // No particle burst for a streamline click. The pre-click count
                // keeps the double-click rollback a safe no-op.
                self.last_click_seed = Some(LastClickSeed {
                    seed,
                    particles_before: particle_count(&self.particles),
                    time,
                    stored: true,
                });
                true
            }
            view_command::ClickActionPlan::AddParticles => {
                let Some(seed) = self.click_world_seed(pos, rect, aspect) else {
                    self.last_click_seed = None;
                    return false;
                };
                // Particles exist for any three-component field: the 3D view or
                // a 2D plane slice (the plan already gated this). If the cloud
                // has not spawned yet, fill it first so the click does not
                // leave a cloud of just this burst. The count after that
                // ambient spawn is what a double-click truncates to.
                let mut particles_before = particle_count(&self.particles);
                if let Some(target) = self.state.particle_target() {
                    let domain = self.state.scene_domain();
                    let dimension = if self.state.mode_3d { 3 } else { 2 };
                    if particle_count(&self.particles) == 0 {
                        spawn(&mut self.particles, &domain, target, dimension);
                    }
                    particles_before = particle_count(&self.particles);
                    spawn_at(
                        &mut self.particles,
                        &domain,
                        seed,
                        CLICK_BURST_PARTICLES,
                        dimension,
                    );
                }
                // No streamline seed. `stored = false` still models the burst so
                // a following double-click truncates the particles back, while
                // never deleting a coincident retained seed.
                self.last_click_seed = Some(LastClickSeed {
                    seed,
                    particles_before,
                    time,
                    stored: false,
                });
                true
            }
        }
    }

    /// Map one viewport point onto the active camera to get a seed position.
    /// The pure `click_seed` seam maps the pointer to the orbit ray (3D) or the
    /// 2D plane; `None` means the point is off the domain.
    fn click_world_seed(&self, pos: egui::Pos2, rect: egui::Rect, aspect: f32) -> Option<Vec3> {
        if self.state.field_mode == FieldMode::Ode {
            // Explicit refusal even though Ode is a vector view: its scene is
            // precomputed, so a click has no seed target.
            return None;
        }
        let width = rect.width().max(1.0);
        let height = rect.height().max(1.0);
        let ndc = Vec2::new(
            (pos.x - rect.left()) / width * 2.0 - 1.0,
            1.0 - (pos.y - rect.top()) / height * 2.0,
        );
        let ortho = if self.state.mode_3d {
            None
        } else {
            Some(self.state.plane_size())
        };
        let view_camera = match &self.fly {
            Some(fly) => ViewCamera::Fly(fly),
            None => ViewCamera::Orbit(&self.orbit),
        };
        click_seed(view_camera, &self.state.scene_domain(), aspect, ndc, ortho)
    }

    /// Take back the burst the previous single click planted when it turns out
    /// to be the first half of a double-click: the double-click resets the view
    /// and must not leave a seed behind. The decision and the seed-list edit
    /// live in the pure [`undo_click_seed`] seam; this wrapper applies the
    /// particle truncation and recomposes the line collection.
    pub(super) fn undo_last_click_seed(&mut self, now: f64) {
        if self.state.field_mode == FieldMode::Ode {
            // No Ode click ever stored a seed or a particle burst, so the
            // double-click reset has nothing to take back.
            self.last_click_seed = None;
            return;
        }
        let undo = undo_click_seed(
            &self.click_seeds,
            self.last_click_seed.take(),
            now,
            particle_count(&self.particles),
        );
        // The particle rollback is independent of the seed edit: an unstored
        // click still burst particles and must still truncate them, even
        // though it removed no list entry.
        if self.state.particle_target().is_some()
            && let Some(before) = undo.particles
        {
            self.particles.truncate(before);
        }
        if !undo.removed {
            return;
        }
        self.click_seeds = undo.seeds;
        self.click_lines = self
            .state
            .click_streamlines_snapshot(self.dots_range, &self.click_seeds);
        self.streamline_count = self.flow_lines.len() + self.click_lines.len();
        if let Some(scene) = self.scene.as_vector_mut() {
            scene.lines = Arc::new(compose_lines_with_frame_style(
                &self.flow_lines,
                &self.click_lines,
                &self.state.scene_domain(),
                self.state.is_3d(),
                self.state.plane_frame(),
                self.state.show_grid,
                self.state.show_coordinate_system,
                &self.state.reference_viewport_style(),
            ));
        }
    }
}
