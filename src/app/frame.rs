//! Frame orchestration: the eframe entry point runs the phase methods in the
//! order the frame has always used.

use super::*;

impl eframe::App for VfApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        // Restyle the whole frame from the persisted record before any widget
        // runs. `apply` is idempotent, so doing this every frame is cheap and
        // keeps the UI in sync even if a theme/visual was changed elsewhere.
        style_panel::apply(ui.ctx(), &self.style);

        // Ticket 53: one layout decision per frame from the root width, before
        // the top/formula bars or any side panel consume width. The mode is
        // transient; a wide frame closes any stale compact sheet. A mode change
        // requests one more frame so the bottom panels resize before the app
        // can go idle (the first narrow frame measures the two-row status).
        let mode = layout::layout_mode(ui.available_width());
        if mode != self.layout_mode {
            self.layout_mode = mode;
            if mode == LayoutMode::Compact {
                crate::fly_camera::exit_fly_for_layout(&mut self.fly, &mut self.fly_keys, mode);
            }
            ui.ctx().request_repaint();
        }
        if self.layout_mode == LayoutMode::Wide {
            self.sheet.close();
        }

        // Input, chrome, simulation, status, viewport, then the overlays on
        // top: the same statement order as the pre-split `ui` body.
        let ctx = ui.ctx().clone();
        let domain_before = self.frame_input(&ctx);
        self.prepare_tour_frame();
        self.frame_chrome(ui, &ctx, domain_before);
        // Chrome setters (formula bar 2D/3D, presets, CAS, future plot panel)
        // can change the plot kind. Reconcile and release capture again before
        // simulation so a same-frame transition cannot advance/render stale
        // vector geometry or leave a captured pointer behind.
        self.reconcile_plot_transition();
        self.sync_fly_capture(&ctx);
        self.frame_simulation(&ctx);
        self.frame_status_bar(ui);
        self.frame_viewport(ui, frame);
        self.frame_overlays(ui);
        // The guided tour is painted last so its card is never hidden by the
        // help overlay or an open dialog.
        self.frame_tour_overlay(&ctx);
        // Issue #86 slice A2: one driver pass after the UI has had its say.
        self.drive_history(&ctx);
    }

    /// Persist the settings envelope (style + layout + view state) under
    /// eframe's app key. Camera state is intentionally absent: presets are the
    /// way to restore a viewpoint.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let settings = AppSettings {
            style: self.style.clone(),
            layout: self.layout.clone(),
            view: {
                let view = ViewRecord::capture(&self.state);
                #[cfg(not(target_arch = "wasm32"))]
                let view = view.with_typeset_output(self.cas.typeset_output);
                view
            },
            parameters: self.state.parameters.clone(),
            viewport_style: self.state.viewport_style,
            layer_styles: self.state.layer_styles,
            object_style: self.state.object_style(),
            appearance_profiles: self.profiles.user().to_vec(),
            interaction: self.interaction.clone(),
            interaction_profiles: self.interaction_profiles.user().to_vec(),
            seed_layouts: self.seed_layouts.user().to_vec(),
            preset_ui: PresetUiRecord {
                expanded: self.presets.expanded_state(),
            },
        };
        persistence::save(storage, &settings);
    }
}
