//! Panels and overlays: top bar, formula bar, Controls and Presets rails,
//! status bar, help overlay, and the domain-recentre step.

use super::*;

use crate::scene_document::{SceneCamera, SceneDocument};
use crate::ui::presets::SceneAction;

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
        self.apply_pending_panel_request();
        self.sync_materials_draft_on_entry();
        self.sync_maxwell_draft_on_entry();

        // Free flight is 3D-only: the formula bar's 2D/3D toggle (or a formula
        // that loses its third component) drops the fly camera before any
        // camera work this frame, then the held movement keys are accumulated.
        sync_fly(&mut self.fly, self.state.is_3d());
        self.update_fly_keys(ctx);

        // Ticket 65: mirror the real camera into the sidebar's Camera section
        // before it draws, so C/Esc, a 2D switch, Reset/Fit and presets are all
        // reflected on this frame. A user edit is reconciled after the panel
        // through the one `sync_fly_mirror` seam.
        self.controls.fly = self.fly.is_some();

        // Ticket 53: one plan decides the real assembly. Compact hides both
        // side panels/rails and moves Controls/Presets into the bottom sheet,
        // so only the camera mirror and domain recentre run here.
        let plan = layout::compact_plan(self.layout_mode);
        if plan.controls_panel {
            self.show_left_panels(ui);

            // Ticket 65: a click on the Camera section's checkbox edits only the
            // mirror; apply it through the same fly seam C uses, then resync.
            sync_fly_mirror(
                &mut self.controls.fly,
                &mut self.fly,
                &self.orbit,
                self.state.is_3d(),
                self.controls.sensitivity,
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
        let animation_reason = if self.animation_job.is_some() {
            Some("Animation export is already running.")
        } else {
            AnimationEligibility::from_state(&self.state).reason
        };
        egui::Panel::top("top_bar").show(ui, |ui| {
            let (action, changed) = if plan.compact_top_bar {
                top_bar::show_compact_with_plot_kind(
                    ui,
                    &self.screenshot,
                    self.state.mode_3d,
                    self.state.plot.kind,
                    animation_reason,
                )
            } else {
                top_bar::show_with_plot_kind(
                    ui,
                    &mut self.style,
                    &self.screenshot,
                    &mut self.appearance_open,
                    self.state.mode_3d,
                    self.state.plot.kind,
                    animation_reason,
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

    /// Apply the formula bar's one-shot "Open Sources"/"Open induction"
    /// requests to the active layout: expand the wide rail or open the compact
    /// sheet.
    fn apply_pending_panel_request(&mut self) {
        if self.bar.open_sources {
            self.bar.open_sources = false;
            if self.layout_mode == LayoutMode::Compact {
                self.sheet.open(SheetState::Sources);
                self.sheet_expanded = SheetSize::Default;
            } else {
                self.layout.sources_expanded = true;
            }
        }
        if self.bar.open_induction {
            self.bar.open_induction = false;
            if self.layout_mode == LayoutMode::Compact {
                self.sheet.open(SheetState::Induction);
                self.sheet_expanded = SheetSize::Default;
            } else {
                self.layout.induction_expanded = true;
            }
        }
        if self.bar.open_ode {
            self.bar.open_ode = false;
            if self.layout_mode == LayoutMode::Compact {
                self.sheet.open(SheetState::Ode);
                self.sheet_expanded = SheetSize::Default;
            } else {
                self.layout.ode_expanded = true;
            }
        }
        if self.bar.open_materials {
            self.bar.open_materials = false;
            if self.layout_mode == LayoutMode::Compact {
                self.sheet.open(SheetState::Materials);
                self.sheet_expanded = SheetSize::Default;
            } else {
                self.layout.materials_expanded = true;
            }
        }
        if self.bar.open_maxwell {
            self.bar.open_maxwell = false;
            if self.layout_mode == LayoutMode::Compact {
                self.sheet.open(SheetState::Maxwell);
                self.sheet_expanded = SheetSize::Default;
            } else {
                self.layout.maxwell_expanded = true;
            }
        }
    }

    /// Materials owns a draft separate from the committed scene. Entering the
    /// mode (or Reset, routed below) refreshes it once; a dirty draft survives
    /// every ordinary frame.
    fn sync_materials_draft_on_entry(&mut self) {
        if self.state.field_mode == FieldMode::Materials
            && self.last_field_mode != FieldMode::Materials
        {
            self.material.sync_from(&self.state);
        }
    }

    /// Maxwell owns a draft separate from the committed scene. Entering the
    /// mode (or Reset, routed below) refreshes it once; a dirty draft survives
    /// every ordinary frame.
    fn sync_maxwell_draft_on_entry(&mut self) {
        if self.state.field_mode == FieldMode::Maxwell && self.last_field_mode != FieldMode::Maxwell
        {
            self.maxwell.sync_from(&self.state);
        }
    }

    fn show_left_panels(&mut self, ui: &mut egui::Ui) {
        use crate::ui::panel_rails::{RailEntry, fit_panel_widths, show_rail_strip};
        self.sync_materials_draft_on_entry();
        self.sync_maxwell_draft_on_entry();
        let mut entries = vec![
            RailEntry {
                id: "controls",
                label: "Controls",
                shortcut: Some(menu::CONTROLS_SHORTCUT),
                expanded: &mut self.layout.controls_expanded,
            },
            #[cfg(not(target_arch = "wasm32"))]
            RailEntry {
                id: "cas",
                label: "CAS",
                shortcut: None,
                expanded: &mut self.layout.cas_expanded,
            },
            RailEntry {
                id: "sources",
                label: "Sources",
                shortcut: None,
                expanded: &mut self.layout.sources_expanded,
            },
        ];
        if self.state.field_mode == FieldMode::Induction {
            entries.push(RailEntry {
                id: "induction",
                label: "Induction",
                shortcut: None,
                expanded: &mut self.layout.induction_expanded,
            });
        }
        if self.state.field_mode == FieldMode::Ode {
            entries.push(RailEntry {
                id: "ode",
                label: "ODE",
                shortcut: None,
                expanded: &mut self.layout.ode_expanded,
            });
        }
        if self.state.field_mode == FieldMode::Materials {
            entries.push(RailEntry {
                id: "materials",
                label: "Materials",
                shortcut: None,
                expanded: &mut self.layout.materials_expanded,
            });
        }
        if self.state.field_mode == FieldMode::Maxwell {
            entries.push(RailEntry {
                id: "maxwell",
                label: "Maxwell",
                shortcut: None,
                expanded: &mut self.layout.maxwell_expanded,
            });
        }
        show_rail_strip(
            ui,
            ("left_panel_rails", self.panel_generation),
            layout::RailSide::Left,
            &mut entries,
        );
        // One budget after top chrome and strip toggles, shared by all bodies.
        // Include central frame margins so the painted viewport retains 100pt.
        let mut plot_action = None;
        let mut requests = Vec::new();
        if self.layout.controls_expanded {
            requests.push(self.layout.controls_width);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if self.layout.cas_expanded {
            requests.push(self.layout.cas_width);
        }
        if self.layout.sources_expanded {
            requests.push(self.layout.sources_width);
        }
        if self.state.field_mode == FieldMode::Induction && self.layout.induction_expanded {
            requests.push(self.layout.induction_width);
        }
        if self.state.field_mode == FieldMode::Ode && self.layout.ode_expanded {
            requests.push(self.layout.ode_width);
        }
        if self.state.field_mode == FieldMode::Materials && self.layout.materials_expanded {
            requests.push(self.layout.materials_width);
        }
        if self.state.field_mode == FieldMode::Maxwell && self.layout.maxwell_expanded {
            requests.push(self.layout.maxwell_width);
        }
        if self.layout.presets_expanded {
            requests.push(self.layout.presets_width);
        }
        let right_rail = if self.layout.presets_expanded {
            0.0
        } else {
            layout::RAIL_WIDTH
        };
        let widths = fit_panel_widths(
            &requests,
            ui.available_width() - right_rail - 100.0 - layout::VIEWPORT_CHROME_ALLOWANCE,
        );
        let mut widths = widths.into_iter();
        if self.layout.controls_expanded {
            let width = widths.next().unwrap();
            let mut collapse = false;
            let response = egui::Panel::left(("controls_panel", self.panel_generation))
                .exact_size(width)
                .show(ui, |ui| {
                    collapse = layout::collapse_button(
                        ui,
                        layout::RailSide::Left,
                        menu::CONTROLS_SHORTCUT,
                    );
                    egui::ScrollArea::vertical()
                        .id_salt("controls_body")
                        .show(ui, |ui| {
                            // The sidebar legend reads these mirrored facts, not
                            // `SceneColorFacts` directly, so copy stats and clips
                            // before rendering or the legend silently shows empty
                            // stats.
                            self.controls.color_stats = self.color_facts.stats;
                            self.controls.color_clips = self.color_facts.clips;
                            plot_action = crate::ui::sidebar::show_with_plot_action(
                                ui,
                                &mut self.state,
                                &mut self.controls,
                                false,
                            );
                        });
                });
            if let Some(width) = resize_panel(ui, &response.response, layout::RailSide::Left) {
                self.layout.controls_width = width;
            }
            if collapse {
                self.layout.controls_expanded = false;
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        if self.layout.cas_expanded {
            let width = widths.next().unwrap();
            let mut collapse = false;
            let response = egui::Panel::left(("cas_panel", self.panel_generation))
                .exact_size(width)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        collapse = ui
                            .add(egui::Button::new("‹").min_size(style_panel::button_min_size(ui)))
                            .on_hover_text("Collapse CAS")
                            .clicked();
                        ui.strong("CAS");
                    });
                    egui::ScrollArea::vertical()
                        .id_salt("cas_body")
                        .show(ui, |ui| self.show_cas_body(ui));
                });
            if let Some(width) = resize_panel(ui, &response.response, layout::RailSide::Left) {
                self.layout.cas_width = width;
            }
            if collapse {
                self.layout.cas_expanded = false;
            }
        }
        if self.layout.sources_expanded {
            let width = widths.next().unwrap();
            let mut collapse = false;
            let response = egui::Panel::left(("sources_panel", self.panel_generation))
                .exact_size(width)
                .show(ui, |ui| {
                    collapse = layout::collapse_button_named(
                        ui,
                        layout::RailSide::Left,
                        "Sources",
                        "Sources",
                    );
                    egui::ScrollArea::vertical()
                        .id_salt("sources_body")
                        .show(ui, |ui| {
                            crate::ui::sources_panel::show(
                                ui,
                                &mut self.state,
                                &mut self.sources,
                                false,
                            );
                        });
                });
            if let Some(width) = resize_panel(ui, &response.response, layout::RailSide::Left) {
                self.layout.sources_width = width;
            }
            if collapse {
                self.layout.sources_expanded = false;
            }
        }
        if self.state.field_mode == FieldMode::Induction && self.layout.induction_expanded {
            let width = widths.next().unwrap();
            let mut collapse = false;
            let mut induction_action = None;
            let response = egui::Panel::left(("induction_panel", self.panel_generation))
                .exact_size(width)
                .show(ui, |ui| {
                    collapse = layout::collapse_button_named(
                        ui,
                        layout::RailSide::Left,
                        "Induction",
                        "Induction",
                    );
                    egui::ScrollArea::vertical()
                        .id_salt("induction_body")
                        .show(ui, |ui| {
                            induction_action = crate::ui::induction_panel::show(
                                ui,
                                &mut self.induction,
                                &mut self.state,
                                false,
                            );
                        });
                });
            if let Some(width) = resize_panel(ui, &response.response, layout::RailSide::Left) {
                self.layout.induction_width = width;
            }
            if collapse {
                self.layout.induction_expanded = false;
            }
            if let Some(action) = induction_action {
                self.apply_induction_panel_action(action);
            }
        }
        if self.state.field_mode == FieldMode::Ode && self.layout.ode_expanded {
            if self.last_field_mode != FieldMode::Ode {
                self.ode.sync_from(&self.state);
            }
            let width = widths.next().unwrap();
            let mut collapse = false;
            let mut ode_action = None;
            let response = egui::Panel::left(("ode_panel", self.panel_generation))
                .exact_size(width)
                .show(ui, |ui| {
                    collapse =
                        layout::collapse_button_named(ui, layout::RailSide::Left, "ODE", "ODE");
                    egui::ScrollArea::vertical()
                        .id_salt("ode_body")
                        .show(ui, |ui| {
                            ode_action = crate::ui::ode_panel::show(
                                ui,
                                &mut self.ode,
                                &mut self.state,
                                false,
                            );
                        });
                });
            if let Some(width) = resize_panel(ui, &response.response, layout::RailSide::Left) {
                self.layout.ode_width = width;
            }
            if collapse {
                self.layout.ode_expanded = false;
            }
            if let Some(action) = ode_action {
                self.apply_ode_panel_action(action);
            }
        }
        if self.state.field_mode == FieldMode::Materials && self.layout.materials_expanded {
            let width = widths.next().unwrap();
            let mut collapse = false;
            let mut material_action = None;
            let response = egui::Panel::left(("materials_panel", self.panel_generation))
                .exact_size(width)
                .show(ui, |ui| {
                    collapse = layout::collapse_button_named(
                        ui,
                        layout::RailSide::Left,
                        "Materials",
                        "Materials",
                    );
                    egui::ScrollArea::vertical()
                        .id_salt("materials_body")
                        .show(ui, |ui| {
                            material_action = crate::ui::material_panel::show(
                                ui,
                                &mut self.material,
                                &mut self.state,
                                false,
                            );
                        });
                });
            if let Some(width) = resize_panel(ui, &response.response, layout::RailSide::Left) {
                self.layout.materials_width = width;
            }
            if collapse {
                self.layout.materials_expanded = false;
            }
            if let Some(action) = material_action {
                self.apply_material_panel_action(action);
            }
        }
        if self.state.field_mode == FieldMode::Maxwell && self.layout.maxwell_expanded {
            let width = widths.next().unwrap();
            let mut collapse = false;
            let mut maxwell_action = None;
            let response = egui::Panel::left(("maxwell_panel", self.panel_generation))
                .exact_size(width)
                .show(ui, |ui| {
                    collapse = layout::collapse_button_named(
                        ui,
                        layout::RailSide::Left,
                        "Maxwell",
                        "Maxwell",
                    );
                    egui::ScrollArea::vertical()
                        .id_salt("maxwell_body")
                        .show(ui, |ui| {
                            maxwell_action = crate::ui::maxwell_panel::show(
                                ui,
                                &mut self.maxwell,
                                &mut self.state,
                                false,
                            );
                        });
                });
            if let Some(width) = resize_panel(ui, &response.response, layout::RailSide::Left) {
                self.layout.maxwell_width = width;
            }
            if collapse {
                self.layout.maxwell_expanded = false;
            }
            if let Some(action) = maxwell_action {
                self.apply_maxwell_panel_action(action);
            }
        }
        if self.layout.presets_expanded {
            self.wide_presets_width = widths.next().unwrap();
        }
        if let Some(action) = plot_action {
            self.apply_plot_panel_action(action);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn show_cas_body(&mut self, ui: &mut egui::Ui) {
        crate::ui::cas_panel::show(ui, &mut self.state, &mut self.cas);
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
            let delta = view_command::domain_centre_delta(&domain_before, &self.state.domain);
            if !self.state.is_vector_mode() {
                // Scalar domain edits move the plot camera, never the saved
                // vector orbit that a return to vector mode restores.
                self.plot_camera.target += delta.as_dvec3();
            } else {
                if self.state.mode_3d {
                    self.orbit.recentre_on(&self.state.scene_domain());
                } else {
                    self.orbit.recentre_on_2d(&self.state.scene_domain());
                }
                if let Some(fly) = self.fly.as_mut() {
                    fly.translate(delta);
                }
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

    /// Apply the ODE panel's one-shot request through the existing seams.
    fn apply_ode_panel_action(&mut self, action: crate::ui::ode_panel::OdePanelAction) {
        match action {
            crate::ui::ode_panel::OdePanelAction::Step => self.ode_step(),
            crate::ui::ode_panel::OdePanelAction::Fit => {
                self.apply_view_command(ViewCommand::Fit);
            }
            crate::ui::ode_panel::OdePanelAction::Reset => {
                self.state.reset_ode();
                self.apply_view_command(ViewCommand::Reset);
                self.ode.sync_from(&self.state);
            }
        }
    }

    /// ODE panel Step: pause, seek one fixed step (bounded by the solution
    /// duration), and mirror the clock into state time. No solve.
    pub(super) fn ode_step(&mut self) {
        let Some(duration) = self
            .state
            .ode_solution
            .as_ref()
            .map(|solution| solution.duration as f32)
        else {
            return;
        };
        let target = (self.clock.time + crate::ode::ODE_STEP as f32).min(duration);
        self.clock.seek(target);
        self.state.set_time(self.clock.time);
    }

    /// Apply the Induction panel's one-shot request through the existing seams.
    fn apply_induction_panel_action(
        &mut self,
        action: crate::ui::induction_panel::InductionPanelAction,
    ) {
        match action {
            crate::ui::induction_panel::InductionPanelAction::Fit => {
                self.apply_view_command(ViewCommand::Fit);
            }
            crate::ui::induction_panel::InductionPanelAction::Reset => {
                self.state
                    .set_induction_scene(crate::induction::InductionScene::default());
            }
        }
    }

    /// Apply the Materials panel's one-shot request. Reset restores the default
    /// material scene *and* the default camera; Fit reuses the shared seam.
    fn apply_material_panel_action(
        &mut self,
        action: crate::ui::material_panel::MaterialPanelAction,
    ) {
        match action {
            crate::ui::material_panel::MaterialPanelAction::Fit => {
                self.apply_view_command(ViewCommand::Fit);
            }
            crate::ui::material_panel::MaterialPanelAction::Reset => {
                self.state.reset_material();
                self.material.sync_from(&self.state);
                self.apply_view_command(ViewCommand::Reset);
            }
        }
    }

    /// Apply the Maxwell panel's one-shot request. Reset restores the default
    /// scene and camera and syncs the draft; SwitchTo2d only changes the
    /// projection and re-samples the same case, preserving the camera and mode.
    fn apply_maxwell_panel_action(&mut self, action: crate::ui::maxwell_panel::MaxwellPanelAction) {
        match action {
            crate::ui::maxwell_panel::MaxwellPanelAction::Fit => {
                self.apply_view_command(ViewCommand::Fit);
            }
            crate::ui::maxwell_panel::MaxwellPanelAction::Reset => {
                self.state.reset_maxwell();
                self.maxwell.sync_from(&self.state);
                self.apply_view_command(ViewCommand::Reset);
            }
            crate::ui::maxwell_panel::MaxwellPanelAction::SwitchTo2d => {
                self.state.set_mode(false);
                // The explicit action keeps the user's framing: record the new
                // frame before simulation so the plane-entry refit never runs.
                self.scene_frame = SceneFrameKey::of(&self.state);
                self.state.refresh_maxwell_sampling();
            }
        }
    }

    /// Apply what the Presets body reported this frame: a camera button picked
    /// inside the panel/sheet, then any applied preset record, then a whole
    /// scene save or load.
    pub(super) fn finish_presets(&mut self, camera_request: Option<ViewCommand>) {
        // Expansion persists through live state, not this report: `save`
        // captures `expanded_state()` every save point. Draining the one-shot
        // report here only keeps it from leaking into the next frame.
        let _ = self.presets.take_layout_changed();
        if let Some(command) = camera_request {
            self.apply_view_command(command);
        }
        if let Some(applied) = self.presets.take_applied() {
            // Presets are orbit views: apply one to the untouched orbit camera
            // and leave fly mode, with the hand-editable file's non-finite
            // components ignored (review M-3).
            apply_camera_record(&applied, &mut self.orbit, &mut self.fly);
        }
        if let Some(action) = self.presets.take_scene_action() {
            match action {
                SceneAction::Save => self.save_scene(),
                SceneAction::Load(name) => self.load_scene(&name),
            }
        }
    }

    /// Capture the live view as a [`SceneCamera`]. The 2D record mirrors the
    /// orbit target and ortho zoom, exactly like the existing camera-preset
    /// handoff.
    fn scene_camera(&self) -> SceneCamera {
        SceneCamera {
            position: [self.camera.position[0], self.camera.position[1]],
            scale: self.camera.scale,
            yaw: self.orbit.yaw,
            pitch: self.orbit.pitch,
            distance: self.orbit.distance,
            target: self.orbit.target.to_array(),
            fov_y: self.orbit.fov_y,
            ortho_zoom: self.orbit.ortho_zoom,
        }
    }

    /// Save the live scene under the panel's typed name. The panel validates
    /// the document, rejects duplicates and persists it.
    fn save_scene(&mut self) {
        let document = SceneDocument::capture(
            self.presets.scene_name().to_string(),
            ViewRecord::capture(&self.state),
            self.state.parameters.clone(),
            self.scene_camera(),
        );
        if let Err(error) = self.presets.insert_scene(document) {
            self.presets.set_scene_message(Some(error));
        }
    }

    /// Load the named scene: view first, then parameters, then the camera.
    /// Every step stops on its own error, and no UI preference record is
    /// touched.
    fn load_scene(&mut self, name: &str) {
        let Some(document) = self.presets.scenes().get(name).cloned() else {
            self.presets
                .set_scene_message(Some(format!("Scene {name:?} is no longer available.")));
            return;
        };
        if let Err(error) = document.validate() {
            self.presets
                .set_scene_message(Some(format!("Scene {name:?} is invalid: {error}")));
            return;
        }
        document.view.restore(&mut self.state, &mut self.controls);
        if let Err(error) = self.state.commit_parameters(document.parameters.clone()) {
            self.presets.set_scene_message(Some(format!(
                "Scene {name:?} parameters are invalid: {error}"
            )));
            return;
        }
        self.apply_scene_camera(&document.camera);
        self.presets
            .set_scene_message(Some(format!("Loaded scene {name:?}.")));
    }

    /// Apply a saved pose to the live cameras, clamping to the orbit camera's
    /// live limits so a hand-edited document cannot poison the view. Fly mode
    /// is left: a scene is an orbit view.
    fn apply_scene_camera(&mut self, camera: &SceneCamera) {
        self.orbit.yaw = camera.yaw;
        // Deliberately mirrors the live `OrbitCamera::orbit` / `PlotCamera::orbit`
        // pitch clamp; no shared constant exists yet.
        self.orbit.pitch = camera.pitch.clamp(-1.55, 1.55);
        self.orbit.distance = camera
            .distance
            .clamp(OrbitCamera::MIN_DISTANCE, OrbitCamera::MAX_DISTANCE);
        self.orbit.target = Vec3::from_array(camera.target);
        self.orbit.fov_y = camera
            .fov_y
            .clamp(1.0_f32.to_radians(), 179.0_f32.to_radians());
        self.orbit.ortho_zoom = camera.ortho_zoom;
        self.fly = None;
        self.camera.position = [self.orbit.target.x, self.orbit.target.y];
        self.camera.scale = self.orbit.ortho_zoom;
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
                .exact_size(self.wide_presets_width)
                .show(ui, |ui| {
                    collapse = layout::collapse_button(
                        ui,
                        layout::RailSide::Right,
                        menu::PRESETS_SHORTCUT,
                    );
                    egui::ScrollArea::vertical()
                        .id_salt("presets_body")
                        .show(ui, |ui| {
                            camera_request =
                                self.presets.show(ui, &mut self.camera, &mut self.state);
                        });
                });
            if let Some(width) = resize_panel(ui, &response.response, layout::RailSide::Right) {
                self.layout.presets_width = width;
            }
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
        // Scalar mode reports its own status, never stale vector parse/counts.
        if let Some(scene) = self.scene.as_vector() {
            self.status.parse_message = match self.state.field_mode {
                FieldMode::Induction => self
                    .state
                    .induction_error
                    .clone()
                    .or_else(|| {
                        self.state
                            .induction_sampling
                            .as_ref()
                            .and_then(|sampling| sampling.validation_error.clone())
                    })
                    .unwrap_or_default(),
                FieldMode::Ode => self.state.ode_error.clone().unwrap_or_default(),
                FieldMode::Materials => self
                    .state
                    .material_error
                    .clone()
                    .or_else(|| {
                        self.state
                            .material_sampling
                            .as_ref()
                            .and_then(|sampling| sampling.warning.clone())
                    })
                    .unwrap_or_default(),
                FieldMode::Maxwell => self
                    .state
                    .maxwell_error
                    .clone()
                    .or_else(|| {
                        self.state
                            .maxwell_sampling
                            .as_ref()
                            .and_then(|sampling| sampling.warning.clone())
                    })
                    .unwrap_or_default(),
                _ => self.state.parse_error.clone(),
            };
            self.status.arrow_count = scene.arrows.len();
            self.status.streamline_count = self.streamline_count;
            self.status.particle_count = scene.dots.len();
            self.status.trail_count = scene.trails.len();
        } else if let Some(plot) = self.scene.as_plot() {
            self.status.parse_message = if self.plot_status.is_empty() {
                plot.status.clone()
            } else {
                self.plot_status.clone()
            };
            self.status.arrow_count = 0;
            self.status.streamline_count = 0;
            self.status.particle_count = 0;
            self.status.trail_count = 0;
        }
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

    /// Compact bottom tab bar: font-responsive Controls/Presets targets and a
    /// close affordance while a sheet is open. Tapping the active
    /// tab closes its sheet; tapping the other switches.
    fn show_compact_tab_bar(&mut self, ui: &mut egui::Ui) {
        let sheet_before = self.sheet;
        let row_height = style_panel::button_height(ui);
        let frame = egui::Frame::side_top_panel(ui.style());
        let bar_height = (row_height + frame.total_margin().sum().y).max(layout::TAB_BAR_HEIGHT);
        egui::Panel::bottom("compact_tab_bar")
            .exact_size(bar_height)
            .show(ui, |ui| {
                ui.spacing_mut().interact_size.y = layout::COMPACT_ROW_HEIGHT;
                ui.horizontal(|ui| {
                    let spacing = ui.spacing().item_spacing.x;
                    let close_width = if self.sheet.is_open() {
                        layout::TOUCH_TARGET + spacing
                    } else {
                        0.0
                    };
                    let mut tabs = vec![
                        (SheetState::Controls, "Controls"),
                        #[cfg(not(target_arch = "wasm32"))]
                        (SheetState::Cas, "CAS"),
                        (SheetState::Presets, "Presets"),
                        (SheetState::Sources, "Sources"),
                    ];
                    if self.state.field_mode == FieldMode::Induction {
                        tabs.push((SheetState::Induction, "Induction"));
                    }
                    if self.state.field_mode == FieldMode::Ode {
                        tabs.push((SheetState::Ode, "ODE"));
                    }
                    if self.state.field_mode == FieldMode::Materials {
                        tabs.push((SheetState::Materials, "Materials"));
                    }
                    if self.state.field_mode == FieldMode::Maxwell {
                        tabs.push((SheetState::Maxwell, "Maxwell"));
                    }
                    let available = (ui.available_width() - close_width).max(0.0);
                    egui::ScrollArea::horizontal()
                        .id_salt("compact_tabs")
                        .max_width(available)
                        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let equal = (available / tabs.len() as f32 - spacing)
                                    .max(layout::TOUCH_TARGET);
                                for (sheet, label) in tabs {
                                    // add_sized retains the measured text+padding minimum;
                                    // excess width scrolls rather than shrinking touch targets.
                                    if ui
                                        .add_sized(
                                            [equal, row_height],
                                            egui::Button::selectable(
                                                self.sheet.shows(sheet),
                                                label,
                                            ),
                                        )
                                        .clicked()
                                    {
                                        self.sheet.toggle(sheet);
                                        self.sheet_expanded = SheetSize::Default;
                                    }
                                }
                            });
                        });
                    if self.sheet.is_open()
                        && ui
                            .add_sized([layout::TOUCH_TARGET, row_height], egui::Button::new("✕"))
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
        self.sync_materials_draft_on_entry();
        // The Induction surface exists only while its adapter is active; an
        // open sheet must not outlive a mode switch (the wide rail is gated the
        // same way in `show_left_panels`).
        if sheet == SheetState::Induction && self.state.field_mode != FieldMode::Induction {
            self.sheet.close();
            self.sheet_expanded = SheetSize::Default;
            ui.ctx().request_repaint();
            return;
        }
        if sheet == SheetState::Ode && self.state.field_mode != FieldMode::Ode {
            self.sheet.close();
            self.sheet_expanded = SheetSize::Default;
            ui.ctx().request_repaint();
            return;
        }
        if sheet == SheetState::Materials && self.state.field_mode != FieldMode::Materials {
            self.sheet.close();
            self.sheet_expanded = SheetSize::Default;
            ui.ctx().request_repaint();
            return;
        }
        if sheet == SheetState::Maxwell && self.state.field_mode != FieldMode::Maxwell {
            self.sheet.close();
            self.sheet_expanded = SheetSize::Default;
            ui.ctx().request_repaint();
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
        let mut plot_action = None;
        let mut induction_action = None;
        let mut ode_action = None;
        let mut material_action = None;
        let mut maxwell_action = None;
        let controls_shown = sheet == SheetState::Controls;
        egui::Panel::bottom("compact_sheet")
            .exact_size(height)
            .show(ui, |ui| {
                ui.spacing_mut().interact_size.y = layout::COMPACT_ROW_HEIGHT;
                let title = match sheet {
                    SheetState::Controls => "Controls",
                    #[cfg(not(target_arch = "wasm32"))]
                    SheetState::Cas => "CAS",
                    SheetState::Presets => "Presets",
                    SheetState::Sources => "Sources",
                    SheetState::Induction => "Induction",
                    SheetState::Ode => "ODE",
                    SheetState::Materials => "Materials",
                    SheetState::Maxwell => "Maxwell",
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
                        // Copy the facts into `Controls` before rendering: the
                        // legend reads this mirror, so skipping it silently
                        // shows empty stats and clips.
                        self.controls.color_stats = self.color_facts.stats;
                        self.controls.color_clips = self.color_facts.clips;
                        plot_action = crate::ui::sidebar::show_compact_with_plot_action(
                            ui,
                            &mut self.state,
                            &mut self.controls,
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
                    SheetState::Sources => {
                        crate::ui::sources_panel::show(
                            ui,
                            &mut self.state,
                            &mut self.sources,
                            true,
                        );
                    }
                    SheetState::Induction => {
                        induction_action = crate::ui::induction_panel::show(
                            ui,
                            &mut self.induction,
                            &mut self.state,
                            true,
                        );
                    }
                    SheetState::Ode => {
                        if self.last_field_mode != FieldMode::Ode {
                            self.ode.sync_from(&self.state);
                        }
                        ode_action =
                            crate::ui::ode_panel::show(ui, &mut self.ode, &mut self.state, true);
                    }
                    SheetState::Materials => {
                        material_action = crate::ui::material_panel::show(
                            ui,
                            &mut self.material,
                            &mut self.state,
                            true,
                        );
                    }
                    SheetState::Maxwell => {
                        maxwell_action = crate::ui::maxwell_panel::show(
                            ui,
                            &mut self.maxwell,
                            &mut self.state,
                            true,
                        );
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    SheetState::Cas => self.show_cas_body(ui),
                    SheetState::None => {}
                });
            });

        if close {
            self.sheet.close();
            self.sheet_expanded = SheetSize::Default;
            // The sheet is gone; request the frame that grows the viewport.
            ui.ctx().request_repaint();
        }
        if let Some(action) = plot_action {
            self.apply_plot_panel_action(action);
        }
        if let Some(action) = induction_action {
            self.apply_induction_panel_action(action);
        }
        if let Some(action) = ode_action {
            self.apply_ode_panel_action(action);
        }
        if let Some(action) = material_action {
            self.apply_material_panel_action(action);
        }
        if let Some(action) = maxwell_action {
            self.apply_maxwell_panel_action(action);
        }
        self.finish_presets(camera_request);
        if controls_shown && layout::fly_allowed(self.layout_mode) {
            sync_fly_mirror(
                &mut self.controls.fly,
                &mut self.fly,
                &self.orbit,
                self.state.is_3d(),
                self.controls.sensitivity,
            );
        }
        // A preset (or the sheet's camera toggle) can leave fly; release the
        // pointer in this same frame, as the wide chrome path does.
        self.sync_fly_capture(ui.ctx());
    }

    /// Run a Plot-section action against the current viewport. Fit uses the
    /// last painted aspect because the panel runs before the central viewport.
    fn apply_plot_panel_action(&mut self, action: crate::ui::plot_panel::PlotPanelAction) {
        match action {
            crate::ui::plot_panel::PlotPanelAction::FitPlot => {
                let aspect = self
                    .last_viewport_rect
                    .map(|rect| rect.width().max(1.0) / rect.height().max(1.0))
                    .unwrap_or(1.0);
                let _ = self.fit_plot(aspect);
            }
        }
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

        // Materials: presentation chrome naming the painted case and view. The
        // Response-empty note comes from the current sampling only.
        if self.state.field_mode == FieldMode::Materials
            && let Some(rect) = self.last_viewport_rect
        {
            let label = format!(
                "Materials - {} - {}",
                crate::ui::material_panel::case_label(self.state.material.case),
                crate::ui::material_panel::view_label(self.state.material.view),
            );
            let pos = egui::pos2(rect.left() + 12.0, rect.top() + 12.0);
            ui.painter().text(
                pos,
                egui::Align2::LEFT_TOP,
                label,
                egui::FontId::proportional(13.0),
                egui::Color32::WHITE,
            );
            if self
                .state
                .material_sampling
                .as_ref()
                .is_some_and(|sampling| sampling.response_empty)
            {
                ui.painter().text(
                    egui::pos2(rect.left() + 12.0, rect.top() + 34.0),
                    egui::Align2::LEFT_TOP,
                    crate::material::MATERIAL_RESPONSE_EMPTY_WARNING,
                    egui::FontId::proportional(13.0),
                    egui::Color32::WHITE,
                );
            }
        }

        // Sources: one small, readable status line over the viewport, derived
        // from the same sampling counters as the panel. It never replaces the
        // field or invents a second accuracy accounting.
        if self.state.field_mode == FieldMode::Sources
            && let (Some(rect), Some(text)) = (
                self.last_viewport_rect,
                self.state
                    .source_sampling()
                    .and_then(crate::ui::sources_panel::status_text),
            )
        {
            let pos = egui::pos2(rect.left() + 12.0, rect.top() + 12.0);
            ui.painter().text(
                pos,
                egui::Align2::LEFT_TOP,
                text,
                egui::FontId::proportional(13.0),
                egui::Color32::WHITE,
            );
        }

        // The `Both` source display paints fixed family colours, so the
        // viewport names those colours under the status line. The status text
        // above is untouched; this is a second, short line.
        if self.state.field_mode == FieldMode::Sources
            && self.state.sources.display == crate::source_scene::SourceDisplay::Both
            && let Some(rect) = self.last_viewport_rect
        {
            crate::ui::sources_panel::paint_both_legend(
                ui.painter(),
                egui::pos2(rect.left() + 12.0, rect.top() + 34.0),
            );
        }

        // Help > About (ticket 15): a small window over everything, closed by
        // its × (its open flag is the window's visibility).
        top_bar::about_window(ui.ctx(), &mut self.about_open);

        // Help > Keyboard & mouse (ticket 18): the full help table, the same
        // strings the overlay above renders.
        crate::ui::help_overlay::help_window(ui.ctx(), &mut self.help_open);

        // Animation export: publish the running job's progress, then draw the
        // dialog over everything. The dialog returns one action; the app owns
        // screenshot mutual exclusion, snapshot capture and playback pauses.
        self.sync_animation_job_status();
        let eligibility = AnimationEligibility::from_state(&self.state);
        let screenshot_busy = self.screenshot.is_busy();
        let action = animation_dialog::show(
            ui.ctx(),
            &mut self.animation,
            eligibility,
            self.state.time,
            screenshot_busy,
        );
        self.apply_animation_dialog_action(action, ui.ctx());
    }
}

/// Intentional edge drags alone update requested widths. Panels use exact
/// budgeted widths so egui's stored automatic squeeze can never become a request.
fn resize_panel(ui: &mut egui::Ui, panel: &egui::Response, side: layout::RailSide) -> Option<f32> {
    let rect = panel.rect;
    let edge = match side {
        layout::RailSide::Left => rect.right(),
        layout::RailSide::Right => rect.left(),
    };
    let handle = egui::Rect::from_min_max(
        egui::pos2(edge - 3.0, rect.top()),
        egui::pos2(edge + 3.0, rect.bottom()),
    );
    let response = ui
        .interact(
            handle,
            panel.id.with("requested_width"),
            egui::Sense::drag(),
        )
        .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
    if (response.dragged() || response.drag_stopped())
        && let Some(pos) = response.interact_pointer_pos()
    {
        let width = match side {
            layout::RailSide::Left => pos.x - rect.left(),
            layout::RailSide::Right => rect.right() - pos.x,
        };
        ui.ctx().request_repaint();
        Some(layout::clamp_width(width, rect.width()))
    } else {
        None
    }
}

/// The validated, captured inputs one Start attempt hands to the sink/path
/// phase. Kept separate so a retry clears the previous error before any new
/// work and so tests can exercise validation without the native file dialog.
struct PreparedAnimation {
    spec: crate::animation::AnimationSpec,
    snapshot: crate::animation_scene::AnimationSnapshot,
    provenance: Option<crate::export_provenance::ExportProvenance>,
}

impl VfApp {
    /// Open the animation export dialog with its size defaulted to the current
    /// viewport, clamped to the animation pixel budget.
    pub(super) fn open_animation_dialog(&mut self, ctx: &egui::Context) {
        if self.animation_job.is_some() {
            self.screenshot
                .notify("Animation export is already running.".into(), true);
            return;
        }
        let viewport = self
            .last_viewport_rect
            .map(|rect| {
                let pixels_per_point = ctx.pixels_per_point();
                (
                    (rect.width() * pixels_per_point).round().max(1.0) as u32,
                    (rect.height() * pixels_per_point).round().max(1.0) as u32,
                )
            })
            .unwrap_or((1280, 720));
        self.animation.open_for_viewport(viewport);
        ctx.request_repaint();
    }

    /// Validate the dialog, capture the detached snapshot and provenance, pick
    /// the native sink, pause playback and start the job. A retry clears the
    /// previous runtime/start error first; invalid fields still block.
    pub(super) fn start_animation_export(&mut self, ctx: &egui::Context) {
        if self.animation_job.is_some() {
            return;
        }
        let Some(PreparedAnimation {
            spec,
            snapshot,
            provenance,
        }) = self.prepare_animation_start()
        else {
            return;
        };

        #[cfg(not(target_arch = "wasm32"))]
        {
            let Some(path) = self.choose_animation_path(&spec) else {
                self.animation.status = Some("Animation export cancelled.".into());
                return;
            };
            let empty = crate::export_provenance::ExportProvenance::default();
            let provenance_ref = provenance.as_ref().unwrap_or(&empty);
            let sink = match spec.format {
                crate::animation::AnimationFormat::Gif => crate::animation::gif_sink(&spec, &path),
                crate::animation::AnimationFormat::Mp4
                | crate::animation::AnimationFormat::Webm => {
                    crate::animation::video_sink(&spec, &path, provenance_ref)
                }
            };
            let sink = match sink {
                Ok(sink) => sink,
                Err(error) => {
                    self.animation.error = Some(error.to_string());
                    return;
                }
            };
            let job = match AnimationJob::new(spec, snapshot, sink, provenance) {
                Ok(job) => job,
                Err(error) => {
                    self.animation.error = Some(error.to_string());
                    return;
                }
            };
            self.begin_animation_job(job, ctx);
        }

        #[cfg(target_arch = "wasm32")]
        {
            let sink = match crate::animation_job::memory_gif_sink(&spec) {
                Ok(sink) => sink,
                Err(error) => {
                    self.animation.error = Some(error.to_string());
                    return;
                }
            };
            let job = match AnimationJob::new(spec, snapshot, sink, provenance) {
                Ok(job) => job,
                Err(error) => {
                    self.animation.error = Some(error.to_string());
                    return;
                }
            };
            self.begin_animation_job(job, ctx);
        }
    }

    /// Clear the previous Start failure, validate the current dialog and
    /// capture the detached inputs. Invalid fields still return `None` and keep
    /// Start blocked; a sink-open/snapshot failure no longer blocks a retry.
    fn prepare_animation_start(&mut self) -> Option<PreparedAnimation> {
        self.animation.error = None;
        let eligibility = AnimationEligibility::from_state(&self.state);
        if !eligibility.allowed {
            self.animation.error = eligibility.reason.map(str::to_owned);
            return None;
        }
        if self.screenshot.is_busy() {
            self.animation.error = Some(animation_dialog::SCREENSHOT_BUSY_REASON.into());
            return None;
        }
        let start_time = self.state.time;
        if let Some(error) = self.animation.validation_error(start_time) {
            self.animation.error = Some(error);
            return None;
        }
        let spec = self.animation.spec(start_time);
        let snapshot = match self.animation_snapshot() {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.animation.error = Some(error.to_string());
                return None;
            }
        };
        let provenance = self.field_export_provenance();
        Some(PreparedAnimation {
            spec,
            snapshot,
            provenance,
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn choose_animation_path(
        &self,
        spec: &crate::animation::AnimationSpec,
    ) -> Option<std::path::PathBuf> {
        use crate::animation::AnimationFormat;
        use crate::ui::animation_dialog::{default_animation_filename, ensure_animation_extension};

        let suggested = default_animation_filename(std::time::SystemTime::now(), spec.format);
        let dialog = rfd::FileDialog::new()
            .set_title("Export animation")
            .set_file_name(&suggested);
        let dialog = match spec.format {
            AnimationFormat::Gif => dialog.add_filter("Animated GIF", &["gif"]),
            AnimationFormat::Mp4 => dialog.add_filter("MP4 video", &["mp4"]),
            AnimationFormat::Webm => dialog.add_filter("WebM video", &["webm"]),
        };
        dialog
            .save_file()
            .map(|path| ensure_animation_extension(path, spec.format))
    }

    fn begin_animation_job(&mut self, job: AnimationJob, ctx: &egui::Context) {
        #[cfg(target_arch = "wasm32")]
        self.clear_animation_readback();
        self.animation_restore_playing = Some(self.clock.playing);
        self.clock.set_playing(false);
        self.animation.running = true;
        self.animation.progress = Some((0, job.frames_total()));
        self.animation.error = None;
        self.animation.status = Some("Exporting animation…".into());
        self.animation_job = Some(job);
        ctx.request_repaint();
    }

    /// Cancel a running job. The next pump drops the sink, which deletes its
    /// temporary file, and the completion path restores playback.
    pub(super) fn cancel_animation_export(&mut self, ctx: &egui::Context) {
        if let Some(job) = self.animation_job.as_mut() {
            job.cancel();
            ctx.request_repaint();
        } else {
            self.animation.running = false;
            self.animation.progress = None;
        }
    }

    /// Publish progress and finish a terminal job exactly once: restore the
    /// previous play state, close the progress line and report the outcome.
    pub(super) fn sync_animation_job_status(&mut self) {
        let Some(job) = self.animation_job.as_ref() else {
            self.status.animation_status = None;
            return;
        };
        let state = job.state();
        let done = job.frames_done();
        let total = job.frames_total();
        let error = job.error().map(str::to_owned);

        if !state.is_terminal() {
            self.status.animation_status = Some(match state {
                AnimationJobState::Encoding | AnimationJobState::Writing => {
                    "Animation encoding…".to_string()
                }
                _ => format!("Animation {done}/{total} frames"),
            });
            self.animation.progress = Some((done, total));
            self.animation.running = true;
            return;
        }

        self.animation_job = None;
        #[cfg(target_arch = "wasm32")]
        self.clear_animation_readback();
        self.animation.running = false;
        self.animation.progress = None;
        self.status.animation_status = None;
        if let Some(playing) = self.animation_restore_playing.take() {
            self.clock.set_playing(playing);
        }

        match state {
            AnimationJobState::Done => {
                self.animation.error = None;
                self.animation.status = Some("Animation export complete.".into());
                self.screenshot
                    .notify("animation export complete".into(), false);
            }
            AnimationJobState::Cancelled => {
                self.animation.status = Some("Animation export cancelled.".into());
                self.screenshot.notify("animation cancelled".into(), false);
            }
            AnimationJobState::Failed => {
                let message = error.unwrap_or_else(|| "animation export failed".into());
                self.animation.error = Some(message.clone());
                self.animation.status = None;
                self.screenshot
                    .notify(format!("animation export failed: {message}"), true);
            }
            _ => {}
        }
    }

    /// Dialogue action dispatch after the widget pass.
    pub(super) fn apply_animation_dialog_action(
        &mut self,
        action: AnimationDialogAction,
        ctx: &egui::Context,
    ) {
        match action {
            AnimationDialogAction::None => {}
            AnimationDialogAction::Start => self.start_animation_export(ctx),
            AnimationDialogAction::Cancel => self.cancel_animation_export(ctx),
        }
    }
}

#[doc(hidden)]
impl VfApp {
    /// Test seam: install an already-built animation job through the same
    /// pause/capture path the dialog uses.
    pub fn test_begin_animation_job(&mut self, job: AnimationJob) {
        self.begin_animation_job(job, &egui::Context::default());
    }

    /// Test seam: run the terminal-state reconciliation once.
    pub fn test_sync_animation_job(&mut self) {
        self.sync_animation_job_status();
    }

    /// Test seam: clear/validate/capture exactly as Start does, without the
    /// native file dialog.
    pub fn test_prepare_animation_start(&mut self) -> bool {
        self.prepare_animation_start().is_some()
    }
}
