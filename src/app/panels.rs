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

        self.show_controls_panel(ui);

        // Ticket 65: a click on the Camera section's checkbox edits only the
        // mirror; apply it through the same fly seam C uses, then resync.
        sync_fly_mirror(
            &mut self.sidebar.fly,
            &mut self.fly,
            &self.orbit,
            self.state.is_3d(),
        );

        self.recentre_domain(domain_before);
        self.show_presets_panel(ui);

        // Ticket 65: capture is edge-triggered after every path that can
        // enter or leave fly this frame (C, the sidebar, Esc, 2D switch,
        // Reset/Fit, presets), so a preset that exits releases the pointer in
        // the same frame it applies.
        self.sync_fly_capture(ctx);
    }

    fn show_top_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // Top menu bar: File / Edit / View / Help, the Appearance gear, and the
        // screenshot action. The bar only reports the action; the app dispatches
        // it.
        let mut style_changed = false;
        egui::Panel::top("top_bar").show(ui, |ui| {
            let (action, changed) = top_bar::show(
                ui,
                &mut self.style,
                &self.screenshot,
                &mut self.appearance_open,
            );
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
            crate::ui::formula_bar::show(ui, &mut self.state, &mut self.bar);
            // The transport is a compact strip, not a panel: it appears in the
            // same top panel directly under the formula row, and only while a
            // clock consumer exists (ticket 26).
            if crate::sim_clock::transport_visible(&self.state) {
                crate::ui::transport::show(ui, &mut self.clock, false);
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
                    collapse =
                        layout::collapse_button(ui, layout::RailSide::Left, menu::CONTROLS_SHORTCUT);
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

    fn show_presets_panel(&mut self, ui: &mut egui::Ui) {
        // Right Presets panel, or its rail; same pin/collapse contract. The
        // preset record's camera mirrors the live 2D view (target + zoom), so
        // "Save current view" captures what is on screen; loading a preset
        // hands its camera back and moves the orbit target/zoom.
        self.camera.position = [self.orbit.target.x, self.orbit.target.y];
        self.camera.scale = self.orbit.ortho_zoom;
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
                        camera_request =
                            self.presets.show(ui, &mut self.camera, &mut self.state);
                    });
                });
            self.layout.presets_width = layout::clamp_width(
                response.response.rect.width(),
                layout::DEFAULT_PRESETS_WIDTH,
            );
            if collapse {
                self.layout.presets_expanded = false;
            }
        } else {
            if layout::rail_panel(
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
        }
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

    /// Frame phase: the bottom status bar readouts and the panel itself.
    pub(super) fn frame_status_bar(&mut self, ui: &mut egui::Ui) {
        // Bottom status bar: the parse state (red when bad) plus how many
        // field-geometry instances are being painted — arrows, streamline
        // segments and dots; the app's backdrop lines are decoration and
        // stay out of the count, while the streamline portion gets its own
        // readout so toggling streamlines moves a visible number.
        self.status.parse_message = self.state.parse_error.clone();
        self.status.instance_count =
            self.scene.arrows.len() + self.streamline_count + self.scene.dots.len();
        self.status.streamline_count = self.streamline_count;
        egui::Panel::bottom("status_bar").show(ui, |ui| {
            crate::ui::status_bar::show(ui, &self.status);
        });
    }

    /// Frame phase: the help overlay and the two help windows, painted last
    /// so they sit over the viewport.
    pub(super) fn frame_overlays(&mut self, ui: &mut egui::Ui) {
        // Help overlay last, so it paints over the viewport (and the field
        // callback). Anchored at the viewport's bottom-left corner; the ×
        // persists the hidden state through the layout record.
        if self.layout.help_visible
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
