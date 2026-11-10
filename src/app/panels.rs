//! Panels and overlays: top bar, formula bar, Controls and Presets rails,
//! status bar, help overlay, and the domain-recentre step.

use super::*;

impl VfApp {
    /// Frame phase: the top menu bar, formula bar, fly-input sync, both side
    /// panels, the domain recentre and the capture transition, in frame order.
    pub(super) fn frame_chrome(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        domain_before: Domain,
    ) {
        self.show_top_bar(ui, ctx);
        self.show_formula_bar(ui);

        // Free flight is 3D-only: the formula bar's 2D/3D toggle (or a formula
        // that loses its third component) drops the fly camera before any
        // camera work this frame, then the held movement keys are accumulated.
        sync_fly(&mut self.fly, self.state.is_3d());
        self.update_fly_keys(ctx);

        // Ticket 65: mirror the real camera into the sidebar's Camera section
        // before it draws, so C/Esc, a 2D switch, Reset/Fit and presets are all
        // reflected on this frame. A user edit is reconciled after the panel
        // through the one `sync_fly_mirror` seam.
        self.sidebar.fly = self.fly.is_some();

        // Ticket 53: one plan decides the real assembly. Compact hides both
        // side panels/rails and moves Controls/Presets into the bottom sheet,
        // so only the camera mirror and domain recentre run here.
        let plan = layout::compact_plan(self.layout_mode);
        if plan.controls_panel {
            self.show_controls_panel(ui);

            // Ticket 65: a click on the Camera section's checkbox edits only the
            // mirror; apply it through the same fly seam C uses, then resync.
            sync_fly_mirror(
                &mut self.sidebar.fly,
                &mut self.fly,
                &self.orbit,
                self.state.is_3d(),
            );
        }

        self.recentre_domain(domain_before);
        if plan.presets_panel {
            self.show_presets_panel(ui);
        } else {
            // The sheet draws after the status bar; mirror the camera now so
            // "Save current view" captures the recentred view.
            self.sync_presets_camera();
        }

        // Ticket 65: capture is edge-triggered after every path that can
        // enter or leave fly this frame (C, the sidebar, Esc, 2D switch,
        // Reset/Fit, presets), so a preset that exits releases the pointer in
        // the same frame it applies.
        self.sync_fly_capture(ctx);
    }

    fn show_top_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // Top menu bar: File / Edit / View / Help, the Appearance gear, and the
        // screenshot action. The bar only reports the action; the app dispatches
        // it. Ticket 53: compact replaces the menu row with a hamburger that
        // exposes the wide-menu commands; Appearance lives in the Controls sheet.
        let mut style_changed = false;
        let plan = layout::compact_plan(self.layout_mode);
        egui::Panel::top("top_bar").show(ui, |ui| {
            let (action, changed) = if plan.compact_top_bar {
                top_bar::show_compact(ui, &self.screenshot, self.state.mode_3d)
            } else {
                top_bar::show(
                    ui,
                    &mut self.style,
                    &self.screenshot,
                    &mut self.appearance_open,
                    self.state.mode_3d,
                )
            };
            style_changed = changed;
            if let Some(action) = action {
                self.apply_menu_action(action, ctx);
            }
        });
        if style_changed {
            style_panel::apply(ui.ctx(), &self.style);
        }
    }

    fn show_formula_bar(&mut self, ui: &mut egui::Ui) {
        // Formula bar below the menu row: live formula edits and the 2D/3D
        // toggle. Field presets load from the grouped Presets panel. Every
        // edit goes through a viewmodel setter, which owns parsing, clamping
        // and the dirty flag.
        egui::Panel::top("formula_bar").show(ui, |ui| {
            crate::ui::formula_bar::show(
                ui,
                &mut self.state,
                &mut self.bar,
                self.layout_mode == LayoutMode::Compact,
            );
            // The transport is a compact strip, not a panel: it appears in the
            // same top panel directly under the formula row, and only while a
            // clock consumer exists (ticket 26).
            if crate::sim_clock::transport_visible(&self.state) {
                crate::ui::transport::show(ui, &mut self.clock, &self.state, false);
            }
        });
    }

    fn show_controls_panel(&mut self, ui: &mut egui::Ui) {
        // Left Controls panel, or its collapsed rail. The panel is shown
        // before the scene rebuild below so a control edit is reflected in this
        // frame's snapshot and status-bar count. Its width is remembered in the
        // layout record and restored on the next launch.
        if self.layout.controls_expanded {
            let mut collapse = false;
            let response = egui::Panel::left(("controls_panel", self.panel_generation))
                .resizable(true)
                .default_size(self.layout.controls_width)
                .min_size(layout::MIN_PANEL_WIDTH)
                .max_size(layout::MAX_PANEL_WIDTH)
                .show(ui, |ui| {
                    collapse = layout::collapse_button(
                        ui,
                        layout::RailSide::Left,
                        menu::CONTROLS_SHORTCUT,
                    );
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        crate::ui::sidebar::show(ui, &mut self.state, &mut self.sidebar);
                    });
                });
            self.layout.controls_width = layout::clamp_width(
                response.response.rect.width(),
                layout::DEFAULT_CONTROLS_WIDTH,
            );
            if collapse {
                self.layout.controls_expanded = false;
            }
        } else {
            if layout::rail_panel(
                ui,
                ("controls_rail", self.panel_generation),
                layout::RailSide::Left,
                "Controls",
                menu::CONTROLS_SHORTCUT,
            )
            .clicked
            {
                self.layout.controls_expanded = true;
            }
        }
    }

    fn recentre_domain(&mut self, domain_before: Domain) {
        // Ticket 47: a domain edit moves the view with the box. The orbit record
        // follows unconditionally — 3D to the full new centre, 2D in x/y with
        // its depth kept — so exiting fly mode can never restore a target that
        // frames the old box. While flying, the free camera also translates by
        // the centre delta, keeping its relative view. Orientation, distance,
        // FOV and the 2D zoom are untouched, and the Presets panel below still
        // applies an explicit preset/Reset/Fit command afterwards, so those win.
        if self.state.domain != domain_before {
            if self.state.mode_3d {
                self.orbit.recentre_on(&self.state.domain);
            } else {
                self.orbit.recentre_on_2d(&self.state.domain);
            }
            if let Some(fly) = self.fly.as_mut() {
                fly.translate(view_command::domain_centre_delta(
                    &domain_before,
                    &self.state.domain,
                ));
            }
        }
    }

    /// Mirror the live 2D view into the preset record so "Save current view"
    /// captures what is on screen. Called before either the wide panel or the
    /// compact sheet draws.
    fn sync_presets_camera(&mut self) {
        self.camera.position = [self.orbit.target.x, self.orbit.target.y];
        self.camera.scale = self.orbit.ortho_zoom;
    }

    /// Apply what the Presets body reported this frame: a camera button picked
    /// inside the panel/sheet, then any applied preset record.
    fn finish_presets(&mut self, camera_request: Option<ViewCommand>) {
        if let Some(command) = camera_request {
            self.apply_view_command(command);
        }
        if let Some(applied) = self.presets.take_applied() {
            // Presets are orbit views: apply one to the untouched orbit camera
            // and leave fly mode, with the hand-editable file's non-finite
            // components ignored (review M-3).
            apply_camera_record(&applied, &mut self.orbit, &mut self.fly);
        }
    }

    fn show_presets_panel(&mut self, ui: &mut egui::Ui) {
        // Right Presets panel, or its rail; same pin/collapse contract. The
        // preset record's camera mirrors the live 2D view (target + zoom), so
        // "Save current view" captures what is on screen; loading a preset
        // hands its camera back and moves the orbit target/zoom.
        self.sync_presets_camera();
        // A camera button (Reset view / Fit domain) picked in the Presets panel
        // this frame; applied after the panel so the view-command seam owns the
        // camera update.
        let mut camera_request = None;
        if self.layout.presets_expanded {
            let mut collapse = false;
            let response = egui::Panel::right(("presets_panel", self.panel_generation))
                .resizable(true)
                .default_size(self.layout.presets_width)
                .min_size(layout::MIN_PANEL_WIDTH)
                .max_size(layout::MAX_PANEL_WIDTH)
                .show(ui, |ui| {
                    collapse = layout::collapse_button(
                        ui,
                        layout::RailSide::Right,
                        menu::PRESETS_SHORTCUT,
                    );
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        camera_request = self.presets.show(ui, &mut self.camera, &mut self.state);
                    });
                });
            self.layout.presets_width = layout::clamp_width(
                response.response.rect.width(),
                layout::DEFAULT_PRESETS_WIDTH,
            );
            if collapse {
                self.layout.presets_expanded = false;
            }
        } else if layout::rail_panel(
            ui,
            ("presets_rail", self.panel_generation),
            layout::RailSide::Right,
            "Presets",
            menu::PRESETS_SHORTCUT,
        )
        .clicked
        {
            self.layout.presets_expanded = true;
        }
        self.finish_presets(camera_request);
    }

    /// Frame phase: the bottom status bar readouts and the panel itself.
    pub(super) fn frame_status_bar(&mut self, ui: &mut egui::Ui) {
        // One FPS sample per frame, from the same stable delta the simulation
        // clock consumes. Non-finite/zero frames leave the last reading alone.
        let dt = ui.ctx().input(|input| input.stable_dt);
        self.fps_meter.update(dt);
        self.status.fps = self.fps_meter.fps();
        self.status.frame_ms = self.fps_meter.frame_ms();

        // Counts come from the snapshot being painted, so they move exactly
        // when the scene does. Streamlines stay the app's field-geometry
        // segment count: `scene.lines` also holds the decorative backdrop.
        self.status.parse_message = self.state.parse_error.clone();
        self.status.arrow_count = self.scene.arrows.len();
        self.status.streamline_count = self.streamline_count;
        self.status.particle_count = self.scene.dots.len();
        self.status.trail_count = self.scene.trails.len();
        self.status.density = self.state.density;
        self.status.is_3d = self.state.is_3d();
        self.status.sim_time = if crate::sim_clock::transport_visible(&self.state) {
            Some(self.clock.time)
        } else {
            None
        };
        self.status.flying = self.fly.is_some();

        // Ticket 60 sets `status.hint`; a changed text restarts the 2.5 s
        // frame-time lifetime, which the pure helper then expires.
        if self.status.hint != self.hint_seen {
            self.hint_seen = self.status.hint.clone();
            self.hint_elapsed = 0.0;
        }
        if self.status.hint.is_some() {
            if dt.is_finite() && dt > 0.0 {
                self.hint_elapsed += dt;
            }
            advance_hint(&mut self.status.hint, self.hint_elapsed);
            if self.status.hint.is_none() {
                self.hint_seen = None;
            }
        }

        egui::Panel::bottom("status_bar").show(ui, |ui| {
            crate::ui::status_bar::show(ui, &self.status, self.layout_mode);
        });

        // Ticket 53: compact bottom chrome registers after the status bar and
        // before the central viewport, so the viewport reflows above it and the
        // sheet can never cover the status or tab bars.
        let plan = layout::compact_plan(self.layout_mode);
        if plan.tab_bar {
            self.show_compact_tab_bar(ui);
        }
        if plan.sheet {
            self.show_compact_sheet(ui);
        }
    }

    /// Compact bottom tab bar: one Controls/Presets tab each, with a 44 pt
    /// target and a close affordance while a sheet is open. Tapping the active
    /// tab closes its sheet; tapping the other switches.
    fn show_compact_tab_bar(&mut self, ui: &mut egui::Ui) {
        let sheet_before = self.sheet;
        egui::Panel::bottom("compact_tab_bar")
            .exact_size(layout::TAB_BAR_HEIGHT)
            .show(ui, |ui| {
                ui.spacing_mut().interact_size.y = layout::COMPACT_ROW_HEIGHT;
                ui.horizontal(|ui| {
                    let spacing = ui.spacing().item_spacing.x;
                    let close_width = if self.sheet.is_open() {
                        layout::TOUCH_TARGET + spacing
                    } else {
                        0.0
                    };
                    let tab_width = ((ui.available_width() - close_width) / 2.0 - spacing)
                        .max(layout::TOUCH_TARGET);
                    if ui
                        .add_sized(
                            [tab_width, layout::TOUCH_TARGET],
                            egui::Button::selectable(
                                self.sheet.shows(SheetState::Controls),
                                "Controls",
                            ),
                        )
                        .clicked()
                    {
                        self.sheet.toggle(SheetState::Controls);
                        if self.sheet.is_open() {
                            self.sheet_expanded = SheetSize::Default;
                        }
                    }
                    if ui
                        .add_sized(
                            [tab_width, layout::TOUCH_TARGET],
                            egui::Button::selectable(
                                self.sheet.shows(SheetState::Presets),
                                "Presets",
                            ),
                        )
                        .clicked()
                    {
                        self.sheet.toggle(SheetState::Presets);
                        if self.sheet.is_open() {
                            self.sheet_expanded = SheetSize::Default;
                        }
                    }
                    if self.sheet.is_open()
                        && ui
                            .add_sized(
                                [layout::TOUCH_TARGET, layout::TOUCH_TARGET],
                                egui::Button::new("✕"),
                            )
                            .on_hover_text("Close panel")
                            .clicked()
                    {
                        self.sheet.close();
                        self.sheet_expanded = SheetSize::Default;
                    }
                });
            });
        if self.sheet != sheet_before {
            // The sheet/viewport split changes size; ask for the frame that
            // applies the new panel height even if the click was the last input.
            ui.ctx().request_repaint();
        }
    }

    /// Compact bottom sheet: at most one of Controls/Presets, clamped to 55% of
    /// the window and to the room above the viewport floor. The body keeps the
    /// wide panel's [`egui::ScrollArea`]; the stored wide widths are never read
    /// or written here.
    fn show_compact_sheet(&mut self, ui: &mut egui::Ui) {
        let sheet = self.sheet;
        if !sheet.is_open() {
            return;
        }
        let window_height = ui.ctx().content_rect().height();
        let available_height = ui.available_height();
        let height = layout::sheet_height(window_height, available_height, self.sheet_expanded);
        if height < 1.0 {
            // No room above the status/tab bars: an invisible open sheet would
            // leave the tab UI claiming a panel with nothing on screen.
            self.sheet.close();
            ui.ctx().request_repaint();
            return;
        }

        let mut close = false;
        let mut camera_request = None;
        let controls_shown = sheet == SheetState::Controls;
        egui::Panel::bottom("compact_sheet")
            .exact_size(height)
            .show(ui, |ui| {
                ui.spacing_mut().interact_size.y = layout::COMPACT_ROW_HEIGHT;
                let title = match sheet {
                    SheetState::Controls => "Controls",
                    SheetState::Presets => "Presets",
                    SheetState::None => "",
                };
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(title).strong());
                });

                // A visible drag handle that is also a 44 pt-wide close target:
                // tapping it, or dragging it down past 24 pt, dismisses the
                // sheet. While dragging, `total_drag_delta` is the cumulative
                // distance; on the release frame `dragged()` is already false.
                let (rect, handle) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), 20.0),
                    egui::Sense::click_and_drag(),
                );
                let weak = ui.visuals().weak_text_color();
                let bar = egui::Rect::from_center_size(rect.center(), egui::vec2(36.0, 4.0));
                ui.painter().rect_filled(bar, 2.0, weak);
                let swiped_down = handle.dragged()
                    && handle
                        .total_drag_delta()
                        .is_some_and(|delta| delta.y > 24.0);
                let swiped_up = handle.dragged()
                    && handle
                        .total_drag_delta()
                        .is_some_and(|delta| delta.y < -24.0);
                if swiped_up {
                    (self.sheet_expanded, _) = layout::sheet_swipe_transition(
                        self.sheet_expanded,
                        handle.total_drag_delta().unwrap_or_default().y,
                    );
                    ui.ctx().request_repaint();
                }
                if swiped_down {
                    (self.sheet_expanded, close) = layout::sheet_swipe_transition(
                        self.sheet_expanded,
                        handle.total_drag_delta().unwrap_or_default().y,
                    );
                }
                if handle.on_hover_text("Tap or swipe down to close").clicked() {
                    close = true;
                }

                egui::ScrollArea::vertical().show(ui, |ui| match sheet {
                    SheetState::Controls => {
                        crate::ui::sidebar::show_compact(
                            ui,
                            &mut self.state,
                            &mut self.sidebar,
                            true,
                        );
                        ui.separator();
                        ui.label(egui::RichText::new("Appearance").strong());
                        let controls = crate::ui::style_panel::show_controls(ui, &mut self.style);
                        if controls.changed {
                            crate::ui::style_panel::apply(ui.ctx(), &self.style);
                        }
                    }
                    SheetState::Presets => {
                        camera_request =
                            self.presets
                                .show_compact(ui, &mut self.camera, &mut self.state, true);
                    }
                    SheetState::None => {}
                });
            });

        if close {
            self.sheet.close();
            self.sheet_expanded = SheetSize::Default;
            // The sheet is gone; request the frame that grows the viewport.
            ui.ctx().request_repaint();
        }
        self.finish_presets(camera_request);
        if controls_shown && layout::fly_allowed(self.layout_mode) {
            sync_fly_mirror(
                &mut self.sidebar.fly,
                &mut self.fly,
                &self.orbit,
                self.state.is_3d(),
            );
        }
        // A preset (or the sheet's camera toggle) can leave fly; release the
        // pointer in this same frame, as the wide chrome path does.
        self.sync_fly_capture(ui.ctx());
    }

    /// Frame phase: the help overlay and the two help windows, painted last
    /// so they sit over the viewport.
    pub(super) fn frame_overlays(&mut self, ui: &mut egui::Ui) {
        // Help overlay last, so it paints over the viewport (and the field
        // callback). Anchored at the viewport's bottom-left corner; the ×
        // persists the hidden state through the layout record.
        //
        // Ticket 53: the overlay is a mouse/keyboard reference sized for the
        // desktop. At phone width it would cover and clip the whole viewport, so
        // compact suppresses the persistent overlay below this width while the
        // full Help > Keyboard & mouse window stays reachable from the menu.
        const HELP_OVERLAY_MIN_WIDTH: f32 = 520.0;
        let overlay_fits = self.layout_mode == LayoutMode::Wide
            || self
                .last_viewport_rect
                .is_some_and(|rect| rect.width() >= HELP_OVERLAY_MIN_WIDTH);
        if self.layout.help_visible
            && overlay_fits
            && let Some(rect) = self.last_viewport_rect
        {
            let anchor = egui::pos2(rect.left() + 12.0, rect.bottom() - 12.0);
            if crate::ui::help_overlay::show(ui.ctx(), anchor, self.state.mode_3d) {
                self.layout.help_visible = false;
            }
        }

        // Help > About (ticket 15): a small window over everything, closed by
        // its × (its open flag is the window's visibility).
        top_bar::about_window(ui.ctx(), &mut self.about_open);

        // Help > Keyboard & mouse (ticket 18): the full help table, the same
        // strings the overlay above renders.
        crate::ui::help_overlay::help_window(ui.ctx(), &mut self.help_open);
    }
}
