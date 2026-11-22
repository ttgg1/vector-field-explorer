//! Panels and overlays: top bar, formula bar, Controls and Presets rails,
//! status bar, help overlay, and the domain-recentre step.

use super::*;

use crate::cas_actions::{CameraFraming, CasAction, CasActionError};
use crate::color_mapping::ColorQuantity;
use crate::color_ramp::ScaleMode;
use crate::colormap::Colormap;
use crate::field::FieldExpr;
use crate::field_probe::{CirculationLoop, GaussianSurface, LoopKind, SurfaceKind};
use crate::plane::{PlaneAxis, PlaneView};
use crate::scalar::{ScalarContext, ScalarExpr};
use crate::scalar_view::MAX_PROBES;
use crate::scene_document::{SceneCamera, SceneDocument};
use crate::sources::FieldFamily;
use crate::tour::{ControlId, TourTarget};
use crate::ui::griffiths::{GriffithsAction, GriffithsExplorationAction, GriffithsRoute};
use crate::ui::maxwell_learning_panel::{MeasurementKind, MeasurementRequest};
use crate::ui::pins::{self, PinAction, PinSnapshot};
use crate::ui::plane_lesson::{
    PlaneBoundaryReading, PlaneFieldReading, PlaneImageLocationPrediction,
    PlaneImageSignPrediction, PlaneLessonAction,
};
use crate::ui::presets::SceneAction;
use crate::ui::ring_lesson::{RingLessonAction, RingPrediction, RingReading};
use crate::ui::shell_lesson::{ShellFieldPrediction, ShellFluxPrediction, ShellLessonAction};
use crate::ui::wire_lesson::{
    WireDirectionPrediction, WireInsidePrediction, WireLessonAction, WireOrientation,
};
use crate::viewmodel::{MAX_DENSITY_2D, MAX_DENSITY_3D, MIN_DENSITY};

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
        self.sync_panel_drafts_after_restore();
        self.sync_materials_draft_on_entry();
        self.sync_maxwell_draft_on_entry();
        self.reveal_tour_probe_section();
        // A control rect is meaningful only for the frame that painted it; the
        // Field probes body sets this again below when it is actually drawn.
        self.controls.probe_controls.add_surface_rect = None;

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

        // Issue #229: a wide-layout shell drag applies in the same frame the
        // panel was drawn; the compact sheet's next frame is covered by the
        // matching call at the top of `frame_input`.
        self.apply_gauss_shell_pending();

        self.recentre_domain(domain_before);
        if plan.presets_panel {
            self.show_right_panels(ui);
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
        let top_response = egui::Panel::top("top_bar").show(ui, |ui| {
            let (action, changed) = if plan.compact_top_bar {
                top_bar::show_compact_with_plot_kind_history_bindings(
                    ui,
                    &self.screenshot,
                    self.state.mode_3d,
                    self.state.plot.kind,
                    animation_reason,
                    self.history.undo_label(),
                    self.history.redo_label(),
                    &self.interaction.bindings,
                )
            } else {
                top_bar::show_with_plot_kind_history_bindings(
                    ui,
                    &mut self.style,
                    &self.screenshot,
                    &mut self.appearance_open,
                    self.state.mode_3d,
                    self.state.plot.kind,
                    animation_reason,
                    self.history.undo_label(),
                    self.history.redo_label(),
                    &self.interaction.bindings,
                )
            };
            style_changed = changed;
            if let Some(action) = action {
                self.apply_menu_action(action, ctx);
            }
        });
        self.record_tour_panel(PanelId::TopBar, top_response.response.rect);
        if style_changed {
            style_panel::apply(ui.ctx(), &self.style);
        }
    }

    fn show_formula_bar(&mut self, ui: &mut egui::Ui) {
        // Formula bar below the menu row: live formula edits and the 2D/3D
        // toggle. Field presets load from the grouped Presets panel. Every
        // edit goes through a viewmodel setter, which owns parsing, clamping
        // and the dirty flag.
        let formula_response = egui::Panel::top("formula_bar").show(ui, |ui| {
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
        self.record_tour_panel(PanelId::FormulaBar, formula_response.response.rect);
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

    /// Rebind the active panel drafts once after a whole-view restore (load,
    /// undo/redo, startup). A draft survives ordinary frames, but a restore
    /// replaced the model underneath it.
    fn sync_panel_drafts_after_restore(&mut self) {
        let epoch = self.state.view_restore_epoch();
        if epoch == self.last_restore_epoch {
            return;
        }
        self.last_restore_epoch = epoch;
        self.sync_panel_drafts_from_state();
    }

    pub(crate) fn sync_panel_drafts_from_state(&mut self) {
        match self.state.field_mode {
            FieldMode::Materials => self.material.sync_from(&self.state),
            FieldMode::Maxwell => self.maxwell.sync_from(&self.state),
            FieldMode::Ode => self.ode.sync_from(&self.state),
            FieldMode::Formula | FieldMode::Sources | FieldMode::Induction => {}
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

    /// Build the pin tray's live values and honest unavailable reasons from the
    /// ordinary reports. Never serializes or mutates the model.
    fn pin_snapshot(&mut self) -> PinSnapshot {
        let mut snapshot = PinSnapshot::default();
        let gauss_ready = self.state.field_mode == FieldMode::Maxwell
            && self.state.maxwell.case == crate::maxwell::MaxwellCase::HollowSphere
            && self.maxwell.gauss.active;
        if gauss_ready {
            let family = match crate::maxwell::maxwell_family(self.state.maxwell.case) {
                crate::maxwell::MaxwellFamily::Electric => FieldFamily::Electric,
                crate::maxwell::MaxwellFamily::Magnetic => FieldFamily::Magnetic,
            };
            let report = self.maxwell.gauss.report_for(
                &self.state.maxwell,
                family,
                f64::from(self.state.time),
            );
            snapshot.gauss_radius = Some(report.radius);
            snapshot.gauss_shell_radius = Some(self.state.maxwell.radius);
            snapshot.gauss_flux = Some(pins::flux_text(&report.at_radius));
        } else {
            snapshot.gauss_unavailable = Some(gauss_unavailable_reason(&self.state));
        }
        // The scale lock is ordinary view state and never depends on the model.
        snapshot.scale_auto_fit = self.state.auto_fit;
        if self.state.field_mode == FieldMode::Induction {
            snapshot.induction_frequency = Some(self.state.induction.frequency);
            snapshot.induction_time = Some(f64::from(self.clock.time));
            snapshot.induction_orientation = Some(pins::induction_axis_text(
                self.state.induction.axis.to_array(),
            ));
        } else {
            snapshot.induction_unavailable = Some(
                "The induction frequency, time and axis exist only while the induction model \
                 is active."
                    .to_owned(),
            );
        }
        snapshot
    }

    /// Apply a pinned control's edit through the same setter as the original.
    pub fn apply_pin_action(&mut self, action: PinAction) {
        match action {
            PinAction::SetGaussRadius(radius) => {
                self.maxwell.gauss.set_probe_radius(radius);
            }
        }
    }

    fn show_left_panels(&mut self, ui: &mut egui::Ui) {
        use crate::ui::panel_rails::{RailEntry, fit_panel_widths, show_rail_strip};
        self.sync_panel_drafts_after_restore();
        self.sync_materials_draft_on_entry();
        self.sync_maxwell_draft_on_entry();
        let pin_snapshot = self.pin_snapshot();
        let controls_shortcut =
            menu::shortcut_label(MenuAction::ToggleControls, &self.interaction.bindings)
                .unwrap_or_default();
        let mut entries = vec![
            RailEntry {
                id: "controls",
                label: "Controls",
                shortcut: Some(controls_shortcut.as_str()),
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
        if self.layout.appearance_expanded {
            requests.push(self.layout.appearance_width);
        }
        if self.layout.interaction_expanded {
            requests.push(self.layout.interaction_width);
        }
        let right_rail = layout::RAIL_WIDTH;
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
                        controls_shortcut.as_str(),
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
                                &mut self.seed_layouts,
                                false,
                            );
                            ui.separator();
                            if let Some(action) =
                                pins::show(ui, &mut self.pins, &pin_snapshot, false)
                            {
                                self.apply_pin_action(action);
                            }
                        });
                });
            self.record_tour_probe_control();
            self.record_tour_panel(PanelId::Controls, response.response.rect);
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
            self.record_tour_panel(PanelId::Sources, response.response.rect);
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
            self.record_tour_panel(PanelId::Induction, response.response.rect);
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
            self.record_tour_panel(PanelId::Ode, response.response.rect);
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
            self.record_tour_panel(PanelId::Materials, response.response.rect);
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
            self.record_tour_panel(PanelId::Maxwell, response.response.rect);
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
        if self.layout.appearance_expanded {
            self.wide_appearance_width = widths.next().unwrap();
        }
        if self.layout.interaction_expanded {
            self.wide_interaction_width = widths.next().unwrap();
        }
        if let Some(action) = plot_action {
            self.apply_plot_panel_action(action);
        }
    }

    /// Show the right side as ONE rail column stacked exactly like the left
    /// strip, then the expanded panel bodies inside it. The strip is drawn
    /// first so it stays the outermost right column whether or not a body is
    /// expanded.
    fn show_right_panels(&mut self, ui: &mut egui::Ui) {
        use crate::ui::panel_rails::{RailEntry, show_rail_strip};
        let presets_shortcut =
            menu::shortcut_label(MenuAction::TogglePresets, &self.interaction.bindings)
                .unwrap_or_default();
        let appearance_shortcut = menu::shortcut_label(
            MenuAction::ToggleAppearancePanel,
            &self.interaction.bindings,
        )
        .unwrap_or_default();
        let interaction_shortcut = menu::shortcut_label(
            MenuAction::ToggleInteractionPanel,
            &self.interaction.bindings,
        )
        .unwrap_or_default();
        let mut entries = vec![
            RailEntry {
                id: "presets",
                label: "Presets",
                shortcut: Some(presets_shortcut.as_str()),
                expanded: &mut self.layout.presets_expanded,
            },
            RailEntry {
                id: "appearance",
                label: "Appearance",
                shortcut: Some(appearance_shortcut.as_str()),
                expanded: &mut self.layout.appearance_expanded,
            },
            RailEntry {
                id: "interaction",
                label: "Interaction",
                shortcut: Some(interaction_shortcut.as_str()),
                expanded: &mut self.layout.interaction_expanded,
            },
        ];
        show_rail_strip(
            ui,
            ("right_panel_rails", self.panel_generation),
            layout::RailSide::Right,
            &mut entries,
        );
        self.show_presets_panel(ui);
        self.show_appearance_panel(ui);
        self.show_interaction_panel(ui);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn show_cas_body(&mut self, ui: &mut egui::Ui) {
        if let Some(action) = crate::ui::cas_panel::show(ui, &mut self.state, &mut self.cas) {
            let result = self.apply_cas_action(&action);
            self.cas.action_message = None;
            self.cas.action_rejection = None;
            match result {
                Ok(message) => self.cas.action_message = Some(message),
                Err(error) => self.cas.action_rejection = Some(error.to_string()),
            }
        }
        // A result plotted into the active scene is one named document edit,
        // even when the two layers are applied in the same frame.
        if let Some(label) = self.cas.take_scene_edit() {
            self.record_explicit_edit(label);
        }
        self.show_prompt_model_body(ui);
    }

    /// Issue #213: render the session-only prompt-model section. The state lives
    /// in egui's temporary store, so previewing never enters the document, the
    /// dirty flag or the saved scene.
    #[cfg(not(target_arch = "wasm32"))]
    fn show_prompt_model_body(&mut self, ui: &mut egui::Ui) {
        use crate::ui::prompt_model::{self, PromptModelState};

        let id = prompt_model::state_id();
        let mut state = ui
            .data_mut(|data| data.get_temp::<PromptModelState>(id))
            .unwrap_or_default();
        if let Some(action) = prompt_model::show(ui, &mut state) {
            match self.apply_prompt_model_action(&mut state, action) {
                Ok(_) => state.error = None,
                Err(error) => state.error = Some(error.to_string()),
            }
        }
        ui.data_mut(|data| data.insert_temp(id, state));
    }

    /// Apply one typed CAS action through the existing setters.
    ///
    /// Every piece is validated before the first mutation, so a rejected
    /// action leaves the document, camera, dirty flag and edit history exactly
    /// as they were. An accepted action is one frame-boundary dirty
    /// transition, which the idle edit-history driver records as one undo
    /// entry. No action creates a source, runs a solver, writes a file or
    /// touches preset storage.
    pub fn apply_cas_action(&mut self, action: &CasAction) -> Result<String, CasActionError> {
        match action {
            CasAction::SetFormula { formula } => {
                require_formula_field_mode(&self.state)?;
                FieldExpr::parse_with_parameters(formula, &self.state.parameters)
                    .map_err(|error| bad_action("formula", error.to_string()))?;
                self.state.set_formula(formula.clone());
            }
            CasAction::SetScalarSource { expr } => {
                // The surface source is a Formula-mode concept: the scalar
                // overlay only applies when `scalar_view_applicable` holds
                // (`FieldMode::Formula`) and the Surface plot is a Formula-mode
                // view. Sources mode is refused for the same reason as the
                // formula field, with the same explanation.
                require_formula_field_mode(&self.state)?;
                ScalarExpr::parse(expr, ScalarContext::Surface)
                    .map_err(|error| bad_action("scalar", error.to_string()))?;
                self.state
                    .set_scalar_source(ScalarContext::Surface, expr.clone());
            }
            CasAction::SetParameters { values } => {
                let mut next = self.state.parameters.clone();
                for (name, value) in values {
                    if !value.is_finite() {
                        return Err(bad_action(
                            "param",
                            format!("parameter `{name}` must be finite"),
                        ));
                    }
                    let Some(entry) = next.entries.iter_mut().find(|entry| entry.name == *name)
                    else {
                        return Err(bad_action(
                            "param",
                            format!("unknown parameter `{name}`; define it first"),
                        ));
                    };
                    entry.value = f64::from(*value);
                }
                next.validate()
                    .map_err(|reason| bad_action("param", reason))?;
                self.state
                    .commit_parameters(next)
                    .map_err(|reason| bad_action("param", reason))?;
            }
            CasAction::SetDomain { half_extents } => {
                for value in half_extents {
                    if !value.is_finite()
                        || *value < crate::cas_actions::MIN_HALF_EXTENT
                        || *value > crate::cas_actions::MAX_HALF_EXTENT
                    {
                        return Err(bad_action(
                            "domain",
                            format!(
                                "half-extent {value} is outside {}..={}",
                                crate::cas_actions::MIN_HALF_EXTENT,
                                crate::cas_actions::MAX_HALF_EXTENT
                            ),
                        ));
                    }
                }
                let extents = Vec3::from_array(*half_extents);
                self.state.set_domain(-extents, extents);
                self.controls.domain_min = self.state.domain.min;
                self.controls.domain_max = self.state.domain.max;
            }
            CasAction::SetDensity { samples_per_axis } => {
                let max = if self.state.is_3d() {
                    MAX_DENSITY_3D
                } else {
                    MAX_DENSITY_2D
                };
                if !(MIN_DENSITY..=max).contains(samples_per_axis) {
                    return Err(bad_action(
                        "density",
                        format!(
                            "{samples_per_axis} samples per axis is outside \
                             {MIN_DENSITY}..={max} for the current view"
                        ),
                    ));
                }
                self.state.set_density(*samples_per_axis);
                self.controls.density = self.state.density;
            }
            CasAction::SetDimension { three_d } => {
                self.state.set_mode(*three_d);
            }
            CasAction::SetColors {
                colormap,
                quantity,
                mode,
            } => {
                quantity
                    .available(self.state.is_3d())
                    .map_err(|reason| bad_action("colors", reason))?;
                mode.validate()
                    .map_err(|reason| bad_action("colors", reason))?;
                self.state.set_colormap(*colormap);
                self.state.set_color_quantity(*quantity);
                self.state.set_color_mode(*mode);
                self.sync_color_controls(*colormap, *quantity, *mode);
            }
            CasAction::ApplyAppearanceProfile { name } => {
                let profile = self
                    .profiles
                    .all()
                    .into_iter()
                    .find(|profile| profile.name == *name)
                    .ok_or_else(|| {
                        CasActionError::UnsupportedHere(format!(
                            "appearance profile `{name}` was not found"
                        ))
                    })?;
                profile
                    .apply(&mut self.state)
                    .map_err(|reason| bad_action("appearance", reason))?;
                self.sync_color_controls(
                    self.state.colormap,
                    self.state.color_quantity,
                    self.state.color_mode,
                );
            }
            CasAction::AddPointProbe { position } => {
                let position = Vec3::from_array(*position);
                if !position.is_finite() {
                    return Err(bad_action("probe", "position must be finite"));
                }
                if !self.state.scalar_view_applicable() {
                    return Err(CasActionError::UnsupportedHere(
                        "probes need a formula vector view with a valid surface expression"
                            .to_owned(),
                    ));
                }
                let domain = self.state.scene_domain();
                if !domain_contains(&domain, position) {
                    return Err(bad_action("probe", "position is outside the scene domain"));
                }
                if self.state.probes.len() >= MAX_PROBES {
                    return Err(bad_action(
                        "probe",
                        format!("at most {MAX_PROBES} probes are allowed"),
                    ));
                }
                self.state
                    .add_point_probe(position, None)
                    .map_err(|error| bad_action("probe", error.to_string()))?;
            }
            CasAction::ApplySeedLayout { name } => {
                let domain = self.state.scene_domain();
                let layout = self
                    .seed_layouts
                    .all(&domain)
                    .into_iter()
                    .find(|layout| layout.name == *name)
                    .ok_or_else(|| {
                        CasActionError::UnsupportedHere(format!(
                            "seed layout `{name}` was not found"
                        ))
                    })?;
                layout
                    .seeds
                    .validate(&domain)
                    .map_err(|reason| bad_action("seeds", reason))?;
                self.state
                    .commit_seed_layout(layout.seeds)
                    .map_err(|reason| bad_action("seeds", reason))?;
            }
            CasAction::SetCamera { framing } => match framing {
                CameraFraming::Fit | CameraFraming::Reset => {
                    let command = if *framing == CameraFraming::Fit {
                        ViewCommand::Fit
                    } else {
                        ViewCommand::Reset
                    };
                    if self.state.is_vector_mode() {
                        self.apply_view_command(command);
                    } else {
                        self.apply_plot_view_command(command);
                    }
                }
                CameraFraming::Isometric => {
                    if self.state.is_vector_mode() {
                        let context = self.view_context();
                        self.orbit = view_command::apply_view_command(
                            OrbitCamera::default(),
                            context,
                            ViewCommand::Fit,
                        );
                        self.fly = None;
                        self.presets.clear_active();
                    } else {
                        self.apply_plot_view_command(ViewCommand::Reset);
                    }
                }
            },
            CasAction::SetTransport { playing } => {
                self.clock.set_playing(*playing);
            }
        }
        // Explicit camera framing is a named command; ordinary orbit and zoom
        // stay outside the history as navigation.
        if let CasAction::SetCamera { .. } = action {
            self.record_explicit_edit("change camera");
        }
        Ok(crate::cas_actions::preview(action))
    }

    /// Issue #213: apply one prompt-model panel action. Parsing, previewing and
    /// editing the separation stay session-local; only `Apply` touches the
    /// document, through the ordinary source-scene setters.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn apply_prompt_model_action(
        &mut self,
        state: &mut crate::ui::prompt_model::PromptModelState,
        action: crate::ui::prompt_model::PromptModelAction,
    ) -> Result<String, crate::prompt_model::PromptModelError> {
        use crate::prompt_model::PromptModelError;
        use crate::ui::prompt_model::PromptModelAction;

        match action {
            PromptModelAction::Parse => {
                state.parse();
                match &state.error {
                    Some(error) => Err(PromptModelError::BadValue(error.clone())),
                    None => Ok("Interpreted the prompt.".to_owned()),
                }
            }
            PromptModelAction::Resolve { question, answer } => {
                if state.resolve(&question, answer) {
                    Ok("Updated the dipole orientation.".to_owned())
                } else {
                    Err(PromptModelError::BadValue(
                        "the orientation question no longer applies".to_owned(),
                    ))
                }
            }
            PromptModelAction::ResolveAxis { axis } => {
                if state.resolve_axis(axis) {
                    Ok("Updated the dipole axis.".to_owned())
                } else {
                    Err(PromptModelError::BadValue(
                        "the axis question no longer applies".to_owned(),
                    ))
                }
            }
            PromptModelAction::SetSeparation(value) => {
                if state.set_separation(value) {
                    Ok(format!("Separation set to {value}."))
                } else {
                    Err(PromptModelError::BadValue(format!(
                        "separation `{value}` is not a finite positive number"
                    )))
                }
            }
            PromptModelAction::Apply => {
                let model = state.model.clone().ok_or(PromptModelError::NoModel)?;
                crate::prompt_model::apply(&model, &mut self.state)?;
                let message = "Applied the interpreted model.".to_owned();
                state.message = Some(message.clone());
                Ok(message)
            }
            PromptModelAction::Cancel => {
                state.cancel();
                Ok("Cancelled the interpretation.".to_owned())
            }
        }
    }

    /// Keep the Controls colour mirrors in step with an action-driven colour
    /// change. The sidebar re-applies its mirrors every frame, so a stale
    /// mirror would otherwise repaint the old mapping.
    fn sync_color_controls(
        &mut self,
        colormap: Colormap,
        quantity: ColorQuantity,
        mode: ScaleMode,
    ) {
        self.controls.colormap = colormap;
        self.controls.color_quantity = quantity;
        self.controls.color_mode = mode;
        match mode {
            ScaleMode::Linear => self.controls.signed_log = false,
            ScaleMode::SignedLog { half_width } => {
                self.controls.signed_log = true;
                self.controls.signed_log_half_width = half_width;
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
            crate::ui::ode_panel::OdePanelAction::ReferenceTimeDependent => {
                if self.ode.use_time_dependent_reference(&mut self.state) {
                    // A fresh reference is a fresh playback; let the ODE frame
                    // path reset the shared clock and rebuild the frozen line.
                    self.state.dirty = true;
                }
            }
            crate::ui::ode_panel::OdePanelAction::ReferenceSteady => {
                if self.ode.use_steady_reference(&mut self.state) {
                    self.state.dirty = true;
                }
            }
            crate::ui::ode_panel::OdePanelAction::RestoreFormula => {
                if self.ode.restore_previous_formula(&mut self.state) {
                    self.state.dirty = true;
                }
            }
            crate::ui::ode_panel::OdePanelAction::CompareVisibilityChanged { enabled } => {
                if !enabled {
                    // The panel already cleared its cache; make sure the drawn
                    // segments go away with it in this frame.
                    self.ode.compare.frozen = None;
                }
                // Checkbox visibility is display state, not a scene edit.
                // Mark dirty so `advance_ode`, which runs after chrome in the
                // same frame, rebuilds the cached lines for both transitions.
                self.state.dirty = true;
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
            crate::ui::maxwell_panel::MaxwellPanelAction::Measurement(request) => {
                self.apply_maxwell_measurement(request);
            }
            crate::ui::maxwell_panel::MaxwellPanelAction::InvestigateGaussShell => {
                self.open_gauss_shell(true);
            }
            crate::ui::maxwell_panel::MaxwellPanelAction::FreeExploreGaussShell => {
                self.open_gauss_shell(false);
            }
        }
    }

    /// Issue #229: consume the guided Gauss-shell activity's pending shell
    /// radius and apply it through the ordinary Maxwell scene setter. The
    /// activity never writes the document or a durable probe; a rejected value
    /// stays visible in the panel and leaves the committed scene untouched.
    pub(super) fn apply_gauss_shell_pending(&mut self) {
        let Some(radius) = self.maxwell.gauss.take_pending_shell_radius() else {
            return;
        };
        if !radius.is_finite() || radius <= 0.0 {
            return;
        }
        if radius.to_bits() == self.state.maxwell.radius.to_bits() {
            return;
        }
        let mut scene = self.state.maxwell.clone();
        scene.radius = radius;
        if self.state.set_maxwell_scene(scene) {
            // Keep the panel's own Geometry draft in step without discarding an
            // unrelated dirty field; the investigation does not re-sync the
            // whole Maxwell draft on every drag frame.
            self.maxwell.draft.radius = radius;
            self.maxwell.gauss.invalidate_report();
        } else {
            let error = self
                .state
                .maxwell_error
                .clone()
                .unwrap_or_else(|| "The shell radius was rejected.".to_owned());
            self.maxwell.gauss.note_shell_edit_error(error);
        }
    }

    /// Load the shipped Hollow sphere case for the guided Gauss-law
    /// investigation (or its `Free explore` entry). The investigation is
    /// session-only: no durable probe surface is fabricated, and the current
    /// view/camera or field mode is not switched.
    fn open_gauss_shell(&mut self, investigate: bool) {
        let mut scene = self.state.maxwell.clone();
        if scene.case != crate::maxwell::MaxwellCase::HollowSphere {
            scene = crate::maxwell::MaxwellScene {
                case: crate::maxwell::MaxwellCase::HollowSphere,
                ..crate::maxwell::MaxwellScene::default()
            };
        } else if !scene.radius.is_finite() || scene.radius <= 0.0 {
            scene.radius = 1.0;
        }
        self.state.set_maxwell_scene(scene);
        self.maxwell.sync_from(&self.state);
        if investigate {
            self.maxwell.gauss.activate(&self.state.maxwell);
        } else {
            self.maxwell.gauss.reset();
        }
        self.state.refresh_maxwell_sampling();

        // Reveal the existing Field probes measurement area without adding a
        // probe. In compact mode keep the guided Maxwell sheet; the section
        // opens the next time Controls is shown.
        self.controls.probe_controls.open_section = true;
        self.layout.maxwell_expanded = true;
        if self.layout_mode == LayoutMode::Compact {
            self.sheet.open(SheetState::Maxwell);
            self.sheet_expanded = SheetSize::Default;
        } else if !self.layout.controls_expanded {
            self.layout.toggle_controls();
        }
    }

    /// Open the optional Griffiths branch from the welcome card or the Help
    /// window. It never touches the scene until a route is chosen.
    pub fn open_griffiths_branch(&mut self) {
        self.help_open = false;
        self.griffiths.open();
    }

    /// Start one Griffiths route. The ring route captures the starting scene
    /// once and loads the shipped `Uniform ring` preset through its ordinary
    /// loader; the shell route loads the shipped Hollow sphere case and opens
    /// the #225 activity over #219's measurement state; the plane route loads
    /// the shipped Image plane case in a vertical slice; the wire route loads
    /// the shipped Infinite wire case in a transverse 2D slice.
    pub fn start_griffiths_route(&mut self, route: GriffithsRoute) {
        if !route.available() {
            self.griffiths.placeholder = Some(route);
            self.griffiths.picker_open = true;
            return;
        }
        self.griffiths.picker_open = false;
        self.griffiths.placeholder = None;
        self.clear_griffiths_exploration();
        if self.griffiths_start.is_none() {
            // A lesson is a temporary scene. Persist the bound experiment and
            // suspend its binding before any lesson state replaces the live
            // document, so an autosave cannot write lesson state under the
            // experiment's name. Exit resumes the explicit binding from the
            // snapshot; Keep intentionally leaves the adopted scene untitled.
            if let Err(error) = self.presets.flush_scenes() {
                self.presets.set_scene_message(Some(error));
            }
            self.griffiths_start = Some(self.griffiths_start_snapshot());
            self.presets.clear_active_scene();
        }
        match route {
            GriffithsRoute::Ring => {
                self.griffiths.shell.deactivate();
                self.griffiths.plane.deactivate();
                self.griffiths.wire.deactivate();
                let preset = crate::source_presets::source_preset("Uniform ring")
                    .expect("the shipped Uniform ring preset exists");
                if !crate::source_presets::load_source_preset(preset, &mut self.state) {
                    // The shipped preset is validated by tests; if it ever fails, do
                    // not leave a half-open lesson pretending a ring was loaded.
                    self.exit_griffiths_lesson();
                    return;
                }
                // Side view: look along +X so the ring is edge-on and the z axis runs
                // vertically in the viewport.
                self.orbit.yaw = 0.0;
                self.orbit.pitch = std::f32::consts::FRAC_PI_2;
                self.orbit.distance = 4.0;
                self.orbit.target = Vec3::ZERO;
                self.camera.position = [0.0, 0.0];
                self.camera.scale = 1.0;
                self.griffiths.lesson.activate();
            }
            GriffithsRoute::Shell => {
                self.griffiths.lesson.deactivate();
                self.griffiths.plane.deactivate();
                self.griffiths.wire.deactivate();
                // The shell lesson owns its own #219 measurement session; reset
                // any guided panel session so the two surfaces cannot disagree.
                self.maxwell.gauss.reset();
                let scene = crate::maxwell::MaxwellScene {
                    case: crate::maxwell::MaxwellCase::HollowSphere,
                    ..crate::maxwell::MaxwellScene::default()
                };
                self.state.set_field_mode(FieldMode::Maxwell);
                self.state.set_maxwell_scene(scene);
                self.state.refresh_maxwell_sampling();
                self.maxwell.sync_from(&self.state);
                self.griffiths.shell_completed = false;
                self.griffiths.shell.activate(&self.state.maxwell);
            }
            GriffithsRoute::Wire => {
                self.griffiths.lesson.deactivate();
                self.griffiths.shell.deactivate();
                self.griffiths.plane.deactivate();
                self.maxwell.gauss.reset();
                let scene = crate::maxwell::MaxwellScene {
                    case: crate::maxwell::MaxwellCase::InfiniteWire,
                    ..crate::maxwell::MaxwellScene::default()
                };
                self.state.set_field_mode(FieldMode::Maxwell);
                self.state.set_maxwell_scene(scene);
                // Look down the wire axis in a transverse slice: the 2D front
                // view is the exact z-normal cross-section, with the shell and
                // field arrows in the xy plane.
                self.state.set_mode(false);
                self.state.plane = PlaneView::middle(self.state.plane_grid());
                self.state.refresh_maxwell_sampling();
                self.maxwell.sync_from(&self.state);
                self.orbit.yaw = 0.0;
                self.orbit.pitch = 0.0;
                self.orbit.distance = 4.0;
                self.orbit.target = Vec3::ZERO;
                self.camera.position = [0.0, 0.0];
                self.camera.scale = 1.0;
                self.griffiths.wire.activate(&self.state.maxwell);
            }
            GriffithsRoute::Plane => {
                self.griffiths.lesson.deactivate();
                self.griffiths.shell.deactivate();
                self.griffiths.wire.deactivate();
                // The plane lesson owns no Gauss measurement session; reset a
                // guided panel session so its surface cannot disagree.
                self.maxwell.gauss.reset();
                let scene = crate::maxwell::MaxwellScene {
                    case: crate::maxwell::MaxwellCase::ImagePlane,
                    ..crate::maxwell::MaxwellScene::default()
                };
                self.state.set_field_mode(FieldMode::Maxwell);
                self.state.set_maxwell_scene(scene);
                // A y-normal slice leaves the x-z plane visible: the grounded
                // plane runs horizontally with the source above it.
                self.state.set_mode(false);
                let grid = self.state.plane_grid();
                self.state.set_plane(PlaneView {
                    enabled: false,
                    axis: PlaneAxis::Y,
                    index: PlaneAxis::Y.middle_index(grid),
                });
                self.state.refresh_maxwell_sampling();
                self.maxwell.sync_from(&self.state);
                self.orbit.yaw = 0.0;
                self.orbit.pitch = 0.0;
                self.orbit.distance = 4.0;
                self.orbit.target = Vec3::ZERO;
                self.camera.position = [0.0, 0.0];
                self.camera.scale = 1.0;
                self.griffiths.plane.activate(&self.state.maxwell);
            }
        }
        self.help_open = false;
    }

    /// Back/Exit from a Griffiths lesson: restore the exact scene and camera
    /// from before the branch opened. The shell activity records completion
    /// when the learner has pinned both an inside and an outside reading.
    pub fn exit_griffiths_lesson(&mut self) {
        if self.griffiths.shell.active() && self.griffiths.shell.finish(&self.state.maxwell) {
            self.griffiths.shell_completed = true;
        }
        self.clear_griffiths_exploration();
        let mut restore_scene = None;
        if let Some(start) = self.griffiths_start.take() {
            self.state = start.state;
            self.orbit = start.orbit;
            self.camera.position = start.camera_position;
            self.camera.scale = start.camera_scale;
            self.history = start.history;
            self.last_committed = start.last_committed;
            restore_scene = start.scene;
        }
        self.griffiths.lesson.deactivate();
        self.griffiths.shell.deactivate();
        self.griffiths.plane.deactivate();
        self.griffiths.wire.deactivate();
        self.griffiths.close();
        // Exit restores the exact pre-lesson document; an explicit binding to
        // that same saved scene resumes, so ordinary edits keep updating it.
        if let Some(name) = restore_scene
            && self.presets.scenes().get(&name).is_some()
        {
            self.presets.bind_active_scene(
                SceneDocument::capture(
                    name,
                    ViewRecord::capture(&self.state),
                    self.state.parameters.clone(),
                    self.scene_camera(),
                )
                .with_overlays(self.state.overlays().clone()),
            );
        }
    }

    /// Adopt the activity scene: drop the starting snapshot and keep the
    /// loaded ring, shell, plane or wire scene.
    pub fn keep_griffiths_scene(&mut self) {
        if self.griffiths.shell.active() && self.griffiths.shell.finish(&self.state.maxwell) {
            self.griffiths.shell_completed = true;
        }
        self.griffiths_start = None;
        self.clear_griffiths_exploration();
        self.griffiths.lesson.deactivate();
        self.griffiths.shell.deactivate();
        self.griffiths.plane.deactivate();
        self.griffiths.wire.deactivate();
        self.griffiths.close();
    }

    fn griffiths_start_snapshot(&self) -> GriffithsStart {
        GriffithsStart {
            state: self.state.clone(),
            orbit: self.orbit,
            camera_position: self.camera.position,
            camera_scale: self.camera.scale,
            history: self.history.clone(),
            last_committed: self.last_committed.clone(),
            scene: self.presets.active_scene().map(str::to_owned),
        }
    }

    fn clear_griffiths_exploration(&mut self) {
        self.griffiths_exploration = None;
        self.griffiths_return_prompt = false;
    }

    /// Temporarily leave the activity at its current checkpoint. The lesson
    /// stays active but its window is hidden so ordinary exploration can
    /// proceed; the starting-workspace snapshot is untouched.
    pub fn explore_from_griffiths(&mut self) {
        let Some((route, checkpoint)) = crate::ui::griffiths::active_checkpoint(&self.griffiths)
        else {
            return;
        };
        self.griffiths_exploration = Some(GriffithsExploration {
            route,
            checkpoint,
            scene: self.griffiths_start_snapshot(),
            lesson_state: self.griffiths.clone(),
        });
        self.griffiths_return_prompt = false;
    }

    /// The learner asked to return; offer resume or keep variation.
    pub fn return_to_griffiths_activity(&mut self) {
        if self.griffiths_exploration.is_some() {
            self.griffiths_return_prompt = true;
        }
    }

    /// Restore the scene, camera and lesson state saved at the checkpoint.
    pub fn resume_griffiths_checkpoint(&mut self) {
        if let Some(exploration) = self.griffiths_exploration.take() {
            let scene = exploration.scene;
            self.state = scene.state;
            self.orbit = scene.orbit;
            self.camera.position = scene.camera_position;
            self.camera.scale = scene.camera_scale;
            self.history = scene.history;
            self.last_committed = scene.last_committed;
            self.griffiths = exploration.lesson_state;
        }
        self.griffiths_return_prompt = false;
    }

    /// Keep the learner's current scene and lesson edits; only the saved
    /// checkpoint is discarded.
    pub fn keep_griffiths_variation(&mut self) {
        self.clear_griffiths_exploration();
    }

    /// Test seam: whether the four-route picker is up.
    #[doc(hidden)]
    pub fn griffiths_picker_open_for_test(&self) -> bool {
        self.griffiths.picker_open
    }

    /// Test seam: the active route's current checkpoint.
    #[doc(hidden)]
    pub fn griffiths_checkpoint_for_test(
        &self,
    ) -> Option<crate::ui::griffiths::GriffithsCheckpoint> {
        crate::ui::griffiths::active_checkpoint(&self.griffiths).map(|(_, checkpoint)| checkpoint)
    }

    /// Test seam: whether a temporary exploration snapshot is active.
    #[doc(hidden)]
    pub fn griffiths_exploration_active_for_test(&self) -> bool {
        self.griffiths_exploration.is_some()
    }

    /// Test seam: whether the resume/keep-variation prompt is up.
    #[doc(hidden)]
    pub fn griffiths_return_prompt_for_test(&self) -> bool {
        self.griffiths_return_prompt
    }

    /// Test seam: the unavailable route whose placeholder is showing.
    #[doc(hidden)]
    pub fn griffiths_placeholder(&self) -> Option<GriffithsRoute> {
        self.griffiths.placeholder
    }

    /// Test seam: clear an unavailable-route placeholder.
    #[doc(hidden)]
    pub fn close_griffiths_placeholder(&mut self) -> bool {
        self.griffiths.close_placeholder()
    }

    /// Test seam: whether the ring activity is active.
    #[doc(hidden)]
    pub fn ring_lesson_active_for_test(&self) -> bool {
        self.griffiths.lesson.active()
    }

    /// Test seam: whether the ring result has been revealed.
    #[doc(hidden)]
    pub fn ring_lesson_revealed_for_test(&self) -> bool {
        self.griffiths.lesson.revealed()
    }

    /// Test seam: whether the pre-lesson snapshot has been dropped.
    #[doc(hidden)]
    pub fn griffiths_start_snapshot_is_none_for_test(&self) -> bool {
        self.griffiths_start.is_none()
    }

    /// Test seam: move the lesson probe.
    #[doc(hidden)]
    pub fn set_ring_probe_for_test(&mut self, z: f64) {
        self.griffiths.lesson.set_probe_z(z);
    }

    /// Test seam: commit a prediction.
    #[doc(hidden)]
    pub fn predict_ring_for_test(&mut self, prediction: RingPrediction) {
        self.griffiths.lesson.select_prediction(prediction);
    }

    /// Test seam: reveal the lesson result.
    #[doc(hidden)]
    pub fn reveal_ring_for_test(&mut self) {
        self.griffiths.lesson.reveal();
    }

    /// Test seam: the current signed/ideal reading.
    #[doc(hidden)]
    pub fn ring_lesson_reading_for_test(&mut self) -> Option<RingReading> {
        self.griffiths
            .lesson
            .active()
            .then(|| self.griffiths.lesson.reading().clone())
    }

    /// Test seam: apply the "Show axis formula in CAS" action.
    #[doc(hidden)]
    pub fn send_ring_formula_to_cas_for_test(&mut self) {
        self.send_ring_formula_to_cas();
    }

    /// Test seam: whether the shell activity is active.
    #[doc(hidden)]
    pub fn shell_lesson_active_for_test(&self) -> bool {
        self.griffiths.shell.active()
    }

    /// Test seam: whether the shell activity has been completed for this branch
    /// session.
    #[doc(hidden)]
    pub fn shell_lesson_completed_for_test(&self) -> bool {
        self.griffiths.shell_completed || self.griffiths.shell.completed()
    }

    /// Test seam: whether the shell result has been revealed.
    #[doc(hidden)]
    pub fn shell_lesson_revealed_for_test(&self) -> bool {
        self.griffiths.shell.revealed()
    }

    /// Test seam: commit both shell predictions.
    #[doc(hidden)]
    pub fn predict_shell_for_test(
        &mut self,
        field: ShellFieldPrediction,
        flux: ShellFluxPrediction,
    ) {
        self.griffiths.shell.select_field_prediction(field);
        self.griffiths.shell.select_flux_prediction(flux);
    }

    /// Test seam: reveal the shell result.
    #[doc(hidden)]
    pub fn reveal_shell_for_test(&mut self) -> bool {
        self.griffiths.shell.reveal()
    }

    /// Test seam: move the shell lesson's measuring radius.
    #[doc(hidden)]
    pub fn set_shell_probe_for_test(&mut self, radius: f64) -> bool {
        self.griffiths.shell.set_probe_radius(radius)
    }

    /// Test seam: pin the live at-r shell reading.
    #[doc(hidden)]
    pub fn pin_shell_radius_for_test(&mut self) -> bool {
        if !self.griffiths.shell.active() {
            return false;
        }
        let reading = self
            .griffiths
            .shell
            .report_for(
                &self.state.maxwell,
                FieldFamily::Electric,
                f64::from(self.state.time),
            )
            .at_radius
            .clone();
        let finite = reading.field().is_some();
        self.griffiths.shell.pin(reading);
        finite
    }

    /// Test seam: pin the live at-2r shell reading.
    #[doc(hidden)]
    pub fn pin_shell_double_for_test(&mut self) -> bool {
        if !self.griffiths.shell.active() {
            return false;
        }
        let reading = self
            .griffiths
            .shell
            .report_for(
                &self.state.maxwell,
                FieldFamily::Electric,
                f64::from(self.state.time),
            )
            .at_double
            .clone();
        let finite = reading.field().is_some();
        self.griffiths.shell.pin(reading);
        finite
    }

    /// Test seam: the shell lesson's live r/2r report.
    #[doc(hidden)]
    pub fn shell_report_for_test(&mut self) -> Option<crate::ui::gauss_shell::GaussShellReport> {
        self.griffiths.shell.active().then(|| {
            self.griffiths
                .shell
                .report_for(
                    &self.state.maxwell,
                    FieldFamily::Electric,
                    f64::from(self.state.time),
                )
                .clone()
        })
    }

    /// Test seam: whether the pinned readings include inside and outside.
    #[doc(hidden)]
    pub fn shell_comparison_ready_for_test(&self) -> bool {
        self.griffiths.shell.comparison_ready(&self.state.maxwell)
    }

    /// Test seam: whether the wire activity is active.
    #[doc(hidden)]
    pub fn wire_lesson_active_for_test(&self) -> bool {
        self.griffiths.wire.active()
    }

    /// Test seam: whether the wire result has been revealed.
    #[doc(hidden)]
    pub fn wire_lesson_revealed_for_test(&self) -> bool {
        self.griffiths.wire.revealed()
    }

    /// Test seam: commit both wire predictions.
    #[doc(hidden)]
    pub fn predict_wire_for_test(
        &mut self,
        direction: WireDirectionPrediction,
        inside: WireInsidePrediction,
    ) {
        self.griffiths.wire.select_direction_prediction(direction);
        self.griffiths.wire.select_inside_prediction(inside);
    }

    /// Test seam: reveal the wire result.
    #[doc(hidden)]
    pub fn reveal_wire_for_test(&mut self) -> bool {
        self.griffiths.wire.reveal()
    }

    /// Test seam: move the wire lesson's loop radius.
    #[doc(hidden)]
    pub fn set_wire_loop_for_test(&mut self, radius: f64) -> bool {
        self.griffiths.wire.set_loop_radius(radius)
    }

    /// Test seam: select the wire loop orientation without changing the field.
    #[doc(hidden)]
    pub fn set_wire_orientation_for_test(&mut self, orientation: WireOrientation) {
        self.griffiths.wire.set_orientation(orientation);
    }

    /// Test seam: whether the grounded-plane activity is active.
    #[doc(hidden)]
    pub fn plane_lesson_active_for_test(&self) -> bool {
        self.griffiths.plane.active()
    }

    /// Test seam: whether the plane result has been revealed.
    #[doc(hidden)]
    pub fn plane_lesson_revealed_for_test(&self) -> bool {
        self.griffiths.plane.revealed()
    }

    /// Test seam: commit both plane trial predictions.
    #[doc(hidden)]
    pub fn predict_plane_for_test(
        &mut self,
        sign: PlaneImageSignPrediction,
        location: PlaneImageLocationPrediction,
    ) {
        self.griffiths.plane.select_sign_prediction(sign);
        self.griffiths.plane.select_location_prediction(location);
    }

    /// Test seam: reveal the plane result.
    #[doc(hidden)]
    pub fn reveal_plane_for_test(&mut self) -> bool {
        self.griffiths.plane.reveal()
    }

    /// Test seam: move the plane lesson's lateral measurement position.
    #[doc(hidden)]
    pub fn set_plane_lateral_for_test(&mut self, value: f64) -> bool {
        self.griffiths.plane.set_lateral(value)
    }

    /// Test seam: move the plane lesson's field probe height.
    #[doc(hidden)]
    pub fn set_plane_probe_height_for_test(&mut self, value: f64) -> bool {
        self.griffiths.plane.set_probe_height(value)
    }

    /// Test seam: the candidate boundary readings at the fixed laterals.
    #[doc(hidden)]
    pub fn plane_boundary_readings_for_test(&self) -> Vec<PlaneBoundaryReading> {
        self.griffiths.plane.boundary_readings(&self.state.maxwell)
    }

    /// Test seam: the shipped field just above the plane at the current probe.
    #[doc(hidden)]
    pub fn plane_field_reading_for_test(&self) -> Option<PlaneFieldReading> {
        self.griffiths
            .plane
            .active()
            .then(|| self.griffiths.plane.field_reading(&self.state.maxwell))
    }

    /// Test seam: apply the "Show potential formula in CAS" action.
    #[doc(hidden)]
    pub fn send_plane_formula_to_cas_for_test(&mut self) {
        self.send_plane_formula_to_cas();
    }

    /// Draw the optional Griffiths branch and apply its one-shot actions.
    fn show_griffiths_overlays(&mut self, ctx: &egui::Context) {
        if let Some(exploration) = self.griffiths_exploration.as_ref() {
            let route = exploration.route;
            let checkpoint = exploration.checkpoint;
            let prompt = self.griffiths_return_prompt;
            match crate::ui::griffiths::show_exploration(ctx, route, checkpoint, prompt) {
                GriffithsExplorationAction::Return => self.return_to_griffiths_activity(),
                GriffithsExplorationAction::Resume => self.resume_griffiths_checkpoint(),
                GriffithsExplorationAction::KeepVariation => self.keep_griffiths_variation(),
                GriffithsExplorationAction::None => {}
            }
            return;
        }
        match crate::ui::griffiths::show_picker(ctx, &mut self.griffiths) {
            GriffithsAction::OpenRing => self.start_griffiths_route(GriffithsRoute::Ring),
            GriffithsAction::OpenShell => self.start_griffiths_route(GriffithsRoute::Shell),
            GriffithsAction::OpenPlane => self.start_griffiths_route(GriffithsRoute::Plane),
            GriffithsAction::OpenWire => self.start_griffiths_route(GriffithsRoute::Wire),
            GriffithsAction::Placeholder(route) => self.griffiths.placeholder = Some(route),
            GriffithsAction::Dismiss => self.griffiths.close(),
            GriffithsAction::None => {}
        }
        match crate::ui::ring_lesson::show(ctx, &mut self.griffiths.lesson) {
            RingLessonAction::Back => self.exit_griffiths_lesson(),
            RingLessonAction::KeepExploring => self.keep_griffiths_scene(),
            RingLessonAction::ExploreFromHere => self.explore_from_griffiths(),
            RingLessonAction::SendToCas => self.send_ring_formula_to_cas(),
            RingLessonAction::None => {}
        }
        match crate::ui::shell_lesson::show(
            ctx,
            &mut self.griffiths.shell,
            &self.state.maxwell,
            FieldFamily::Electric,
            f64::from(self.state.time),
        ) {
            ShellLessonAction::Back => self.exit_griffiths_lesson(),
            ShellLessonAction::KeepExploring => self.keep_griffiths_scene(),
            ShellLessonAction::ExploreFromHere => self.explore_from_griffiths(),
            ShellLessonAction::None => {}
        }
        match crate::ui::plane_lesson::show(ctx, &mut self.griffiths.plane, &self.state.maxwell) {
            PlaneLessonAction::Back => self.exit_griffiths_lesson(),
            PlaneLessonAction::KeepExploring => self.keep_griffiths_scene(),
            PlaneLessonAction::ExploreFromHere => self.explore_from_griffiths(),
            PlaneLessonAction::SendToCas => self.send_plane_formula_to_cas(),
            PlaneLessonAction::None => {}
        }
        match crate::ui::wire_lesson::show(
            ctx,
            &mut self.griffiths.wire,
            &self.state.maxwell,
            FieldFamily::Magnetic,
            f64::from(self.state.time),
        ) {
            WireLessonAction::Back => self.exit_griffiths_lesson(),
            WireLessonAction::KeepExploring => self.keep_griffiths_scene(),
            WireLessonAction::ExploreFromHere => self.explore_from_griffiths(),
            WireLessonAction::None => {}
        }
    }

    /// Put the ideal axis formula in the native CAS input with `Q` and `R`
    /// symbolic. The core activity reading never depends on this backend.
    #[cfg(not(target_arch = "wasm32"))]
    fn send_ring_formula_to_cas(&mut self) {
        self.cas.input = crate::ui::ring_lesson::RING_CAS_EXPRESSION.to_owned();
        self.layout.cas_expanded = true;
        if self.layout_mode == LayoutMode::Compact {
            self.sheet.open(SheetState::Cas);
            self.sheet_expanded = SheetSize::Default;
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn send_ring_formula_to_cas(&mut self) {}

    /// Put the normalized vertical-slice potential in the native CAS input with
    /// `k`, `q` and `h` symbolic. The lesson reading never depends on the
    /// backend.
    #[cfg(not(target_arch = "wasm32"))]
    fn send_plane_formula_to_cas(&mut self) {
        self.cas.input = crate::ui::plane_lesson::PLANE_CAS_EXPRESSION.to_owned();
        self.layout.cas_expanded = true;
        if self.layout_mode == LayoutMode::Compact {
            self.sheet.open(SheetState::Cas);
            self.sheet_expanded = SheetSize::Default;
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn send_plane_formula_to_cas(&mut self) {}

    /// Open the probe that actually checks the selected Maxwell statement form,
    /// or run the existing pointwise check for a differential form. An integral
    /// action never overwrites an existing user probe; it reuses the first one
    /// and only adds the matching kind when none exists.
    fn apply_maxwell_measurement(&mut self, request: MeasurementRequest) {
        if request.kind == MeasurementKind::Point {
            self.maxwell.learning.point_highlight = true;
            let scene = self.state.maxwell.clone();
            let bem = self.state.maxwell_bem_solution();
            self.maxwell.learning.check(&scene, bem);
            return;
        }

        if let Some(reason) = self.state.probe_mode_unsupported_reason() {
            self.maxwell.learning.message = Some(reason);
            return;
        }

        let active = match crate::maxwell::maxwell_family(self.state.maxwell.case) {
            crate::maxwell::MaxwellFamily::Electric => FieldFamily::Electric,
            crate::maxwell::MaxwellFamily::Magnetic => FieldFamily::Magnetic,
        };
        if self.state.maxwell_wave().is_none() && active != request.family {
            // The learning panel already paints the current family refusal.
            // Do not store it: enabling a wave makes both families available.
            self.maxwell.learning.message = None;
            return;
        }

        self.state.set_probe_family(request.family);
        let domain = self.state.scene_domain();
        let result = match request.kind {
            MeasurementKind::Surface if self.state.probe_surfaces().is_empty() => self
                .state
                .add_probe_surface(GaussianSurface {
                    kind: SurfaceKind::Sphere,
                    center: domain.center(),
                    axis: Vec3::Z,
                    size: Vec3::splat(1.0),
                    segments: 16,
                })
                .map(|_| ()),
            MeasurementKind::Loop if self.state.probe_loops().is_empty() => self
                .state
                .add_probe_loop(CirculationLoop {
                    kind: LoopKind::Circle,
                    center: domain.center(),
                    axis: Vec3::Z,
                    size: Vec3::splat(1.0),
                    segments: 16,
                })
                .map(|_| ()),
            MeasurementKind::Surface | MeasurementKind::Loop => Ok(()),
            MeasurementKind::Point => Ok(()),
        };
        if let Err(error) = result {
            self.maxwell.learning.message = Some(error);
            return;
        }

        self.maxwell.learning.message = None;
        self.controls.probe_controls.open_section = true;
        if self.layout_mode == LayoutMode::Compact {
            self.sheet.open(SheetState::Controls);
            self.sheet_expanded = SheetSize::Default;
        } else if !self.layout.controls_expanded {
            self.layout.toggle_controls();
        }
    }

    /// Reveal the Field probes body once when the tour reaches the integral
    /// surface control, and forget it on every other stop. The reveal lives in
    /// `ProbeControls`, so a user close is not undone on later frames.
    ///
    /// This probe stop has one deliberate tour side effect: entering it once
    /// switches a Formula/Ode adapter to Sources, because only an adapter that
    /// supplies a field can paint the real `Add sphere` control the stop
    /// anchors to. The switch goes through the shipped `set_field_mode` seam
    /// and preserves every durable scene. When even that cannot make probes
    /// available (the scalar Plot view has no vector field), the card carries
    /// an actionable note instead of a dead section.
    fn reveal_tour_probe_section(&mut self) {
        let index = self.tour.as_ref().and_then(|progress| {
            let stop = progress.current()?;
            match stop.target {
                TourTarget::Control(ControlId::FieldProbeSurface) => Some(progress.index()),
                _ => None,
            }
        });
        match index {
            Some(index) => {
                if self.controls.probe_controls.reveal_for_tour(index) {
                    self.prepare_tour_probe_mode();
                }
                self.report_tour_probe_mode();
            }
            None => self.controls.probe_controls.forget_tour_reveal(),
        }
    }

    /// Reach a probe-capable adapter on tour entry when the current one cannot
    /// supply a field. Sources is the shipped adapter with a ready field model,
    /// so no probe-only numerical path is introduced here.
    fn prepare_tour_probe_mode(&mut self) {
        if self.state.probe_mode_unsupported_reason().is_some() && self.state.is_vector_mode() {
            // The adapter switch is a real, undoable document edit through the
            // shipped mode seam, so it reaches the normal history baseline. The
            // stop body names the switch rather than hiding it.
            self.state.set_field_mode(FieldMode::Sources);
        }
    }

    /// Tell the tour card why probes are still unavailable, if they are. This
    /// runs every frame so a manual adapter change during the stop is reflected.
    fn report_tour_probe_mode(&mut self) {
        let note = self.state.probe_mode_unsupported_reason().map(|reason| {
            if self.state.is_vector_mode() {
                format!("{reason} Switch the adapter to Sources to use Field probes.")
            } else {
                format!("{reason} Switch back to the vector view to use Field probes.")
            }
        });
        self.tour_rects.set_note(note);
    }

    /// Record the real `Add sphere` control rect for this frame's tour anchor.
    fn record_tour_probe_control(&mut self) {
        if let Some(rect) = self.controls.probe_controls.add_surface_rect {
            self.tour_rects
                .record_control(ControlId::FieldProbeSurface, rect);
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
        // Keep the active saved scene in memory current with the live document,
        // so switching between an experiment and its duplicate preserves each
        // one's independent edits. The store is flushed before any scene action
        // below; an idle frame only compares a snapshot.
        self.sync_active_scene_from_state();

        if let Some(action) = self.presets.take_scene_action() {
            // Persist pending edits before switching away; a failed write must
            // not silently discard them.
            if let Err(error) = self.presets.flush_scenes() {
                self.presets.set_scene_message(Some(error));
                return;
            }
            match action {
                SceneAction::Save => self.save_scene(),
                SceneAction::Load(name) => self.load_scene(&name),
                SceneAction::DuplicateCurrent => self.duplicate_current_scene(),
                SceneAction::Duplicate(name) => self.duplicate_scene(&name),
            }
        }
    }

    /// Capture the live document into its bound saved scene.
    ///
    /// This is called every frame after the chrome and viewport, independent of
    /// whether the wide Presets panel or the compact sheet drew, and again
    /// before the eframe save hook flushes. A no-op when nothing is bound or
    /// the capture already matches the stored baseline.
    pub(super) fn sync_active_scene_from_state(&mut self) {
        let Some(name) = self.presets.active_scene().map(str::to_owned) else {
            return;
        };
        let document = SceneDocument::capture(
            name,
            ViewRecord::capture(&self.state),
            self.state.parameters.clone(),
            self.scene_camera(),
        )
        .with_overlays(self.state.overlays().clone());
        self.presets.sync_active_scene(document);
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
        )
        .with_overlays(self.state.overlays().clone());
        if let Err(error) = self.presets.insert_scene(document) {
            self.presets.set_scene_message(Some(error));
        }
    }

    /// Save an independent copy of the live experiment. The panel picks the
    /// unique name (from the typed scene name, or `Experiment`) and persists
    /// both the original saved scene and the copy.
    fn duplicate_current_scene(&mut self) {
        let document = SceneDocument::capture(
            self.presets.scene_name().to_string(),
            ViewRecord::capture(&self.state),
            self.state.parameters.clone(),
            self.scene_camera(),
        )
        .with_overlays(self.state.overlays().clone());
        let _ = self.presets.duplicate_current_experiment(document);
    }

    /// Save an independent copy of the named saved scene.
    fn duplicate_scene(&mut self, name: &str) {
        let _ = self.presets.duplicate_scene(name);
    }

    /// Load the named scene: parameters, then the view, then the camera.
    /// The parameters are installed into a candidate state first so the view
    /// restore validates parameter-dependent records against them; the
    /// candidate swaps in only if every step succeeds, and any error leaves
    /// the live experiment unchanged. No UI preference record is touched.
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
        // Install the saved parameters first in a candidate state. The view
        // restore validates parameter-dependent entries (custom densities) and
        // prepares the Current Field ODE against the committed set; using the
        // previous experiment's parameters would silently drop them.
        let mut next_state = self.state.clone();
        let mut next_controls = self.controls.clone();
        if let Err(error) = next_state.commit_parameters(document.parameters.clone()) {
            self.presets.set_scene_message(Some(format!(
                "Scene {name:?} parameters are invalid: {error}"
            )));
            return;
        }
        document.view.restore(&mut next_state, &mut next_controls);
        if let Err(error) = next_state.set_overlays(document.overlays.clone()) {
            self.presets.set_scene_message(Some(format!(
                "Scene {name:?} overlays are invalid: {error}"
            )));
            return;
        }
        self.state = next_state;
        self.controls = next_controls;
        self.apply_scene_camera(&document.camera);
        // The load replaced the durable model; rebind any same-mode panel draft
        // before the next interaction can submit the stale one.
        self.sync_panel_drafts_from_state();
        // The loaded pose is explicit. Drop any scalar-plot return pose so the
        // pending plot transition cannot replace the saved camera.
        self.saved_vector_orbit = None;
        // Bind the live document to the loaded scene, using the post-clamp live
        // capture as the unchanged baseline so merely visiting a scene writes
        // nothing back.
        self.presets.bind_active_scene(
            SceneDocument::capture(
                name,
                ViewRecord::capture(&self.state),
                self.state.parameters.clone(),
                self.scene_camera(),
            )
            .with_overlays(self.state.overlays().clone()),
        );
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
        let shortcut = menu::shortcut_label(MenuAction::TogglePresets, &self.interaction.bindings)
            .unwrap_or_default();
        if self.layout.presets_expanded {
            let mut collapse = false;
            let response = egui::Panel::right(("presets_panel", self.panel_generation))
                .exact_size(self.wide_presets_width)
                .show(ui, |ui| {
                    collapse =
                        layout::collapse_button(ui, layout::RailSide::Right, shortcut.as_str());
                    egui::ScrollArea::vertical()
                        .id_salt("presets_body")
                        .show(ui, |ui| {
                            camera_request =
                                self.presets.show(ui, &mut self.camera, &mut self.state);
                        });
                });
            self.record_tour_panel(PanelId::Presets, response.response.rect);
            if let Some(width) = resize_panel(ui, &response.response, layout::RailSide::Right) {
                self.layout.presets_width = width;
            }
            if collapse {
                self.layout.presets_expanded = false;
            }
        }
        self.finish_presets(camera_request);
    }

    /// Right Appearance panel, or its rail; same pin/collapse contract as the
    /// Presets panel. Edits commit through the viewmodel's validating seam.
    fn show_appearance_panel(&mut self, ui: &mut egui::Ui) {
        let shortcut = menu::shortcut_label(
            MenuAction::ToggleAppearancePanel,
            &self.interaction.bindings,
        )
        .unwrap_or_default();
        if self.layout.appearance_expanded {
            let mut collapse = false;
            let response = egui::Panel::right(("appearance_panel", self.panel_generation))
                .exact_size(self.wide_appearance_width)
                .show(ui, |ui| {
                    collapse = layout::collapse_button_named(
                        ui,
                        layout::RailSide::Right,
                        "Appearance",
                        shortcut.as_str(),
                    );
                    egui::ScrollArea::vertical()
                        .id_salt("appearance_body")
                        .show(ui, |ui| {
                            self.appearance
                                .show(ui, &mut self.state, &mut self.profiles, false);
                        });
                });
            self.record_tour_panel(PanelId::Appearance, response.response.rect);
            if let Some(width) = resize_panel(ui, &response.response, layout::RailSide::Right) {
                self.layout.appearance_width = width;
            }
            if collapse {
                self.layout.appearance_expanded = false;
            }
        }
    }

    /// Right Interaction panel, or its rail; same pin/collapse contract as the
    /// Appearance panel. Binding and feel edits commit straight to
    /// `VfApp::interaction`; the profile library is edited in place.
    fn show_interaction_panel(&mut self, ui: &mut egui::Ui) {
        let shortcut = menu::shortcut_label(
            MenuAction::ToggleInteractionPanel,
            &self.interaction.bindings,
        )
        .unwrap_or_default();
        if self.layout.interaction_expanded {
            let mut collapse = false;
            let response = egui::Panel::right(("interaction_panel", self.panel_generation))
                .exact_size(self.wide_interaction_width)
                .show(ui, |ui| {
                    collapse = layout::collapse_button_named(
                        ui,
                        layout::RailSide::Right,
                        "Interaction",
                        shortcut.as_str(),
                    );
                    egui::ScrollArea::vertical()
                        .id_salt("interaction_body")
                        .show(ui, |ui| {
                            self.interaction_panel.show(
                                ui,
                                &mut self.interaction,
                                &mut self.interaction_profiles,
                                false,
                            );
                        });
                });
            self.record_tour_panel(PanelId::Interaction, response.response.rect);
            if let Some(width) = resize_panel(ui, &response.response, layout::RailSide::Right) {
                self.layout.interaction_width = width;
            }
            if collapse {
                self.layout.interaction_expanded = false;
            }
        } else {
            // The body is not drawn (collapsed to the rail strip): an armed
            // capture must not stay latched with nothing on screen.
            self.interaction_panel.cancel_capture();
        }
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

        let status_response = egui::Panel::bottom("status_bar").show(ui, |ui| {
            crate::ui::status_bar::show(ui, &self.status, self.layout_mode);
        });
        self.record_tour_panel(PanelId::StatusBar, status_response.response.rect);

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
                        (SheetState::Appearance, "Appearance"),
                        (SheetState::Interaction, "Interaction"),
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
        if sheet != SheetState::Interaction {
            // The Interaction panel is not drawn this frame (the sheet is
            // closed or another tab is active): drop any armed capture so a
            // hidden panel never binds the next key silently.
            self.interaction_panel.cancel_capture();
        }
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
            if sheet == SheetState::Interaction {
                self.interaction_panel.cancel_capture();
            }
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
        let mut pin_action = None;
        let pin_snapshot = self.pin_snapshot();
        let controls_shown = sheet == SheetState::Controls;
        let sheet_response = egui::Panel::bottom("compact_sheet")
            .exact_size(height)
            .show(ui, |ui| {
                ui.spacing_mut().interact_size.y = layout::COMPACT_ROW_HEIGHT;
                let title = match sheet {
                    SheetState::Controls => "Controls",
                    #[cfg(not(target_arch = "wasm32"))]
                    SheetState::Cas => "CAS",
                    SheetState::Presets => "Presets",
                    SheetState::Appearance => "Appearance",
                    SheetState::Interaction => "Interaction",
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

                egui::ScrollArea::vertical().show(ui, |ui| {
                    if let Some(action) = pins::show(ui, &mut self.pins, &pin_snapshot, true) {
                        pin_action = Some(action);
                    }
                    ui.separator();
                    match sheet {
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
                                &mut self.seed_layouts,
                                true,
                            );
                            ui.separator();
                            ui.label(egui::RichText::new("Appearance").strong());
                            let controls =
                                crate::ui::style_panel::show_controls(ui, &mut self.style);
                            if controls.changed {
                                crate::ui::style_panel::apply(ui.ctx(), &self.style);
                            }
                        }
                        SheetState::Presets => {
                            camera_request = self.presets.show_compact(
                                ui,
                                &mut self.camera,
                                &mut self.state,
                                true,
                            );
                        }
                        SheetState::Appearance => {
                            self.appearance
                                .show(ui, &mut self.state, &mut self.profiles, true);
                        }
                        SheetState::Interaction => {
                            self.interaction_panel.show(
                                ui,
                                &mut self.interaction,
                                &mut self.interaction_profiles,
                                true,
                            );
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
                            ode_action = crate::ui::ode_panel::show(
                                ui,
                                &mut self.ode,
                                &mut self.state,
                                true,
                            );
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
                    }
                });
            });
        self.record_tour_probe_control();
        if let Some(action) = pin_action {
            self.apply_pin_action(action);
        }

        let sheet_panel = match sheet {
            SheetState::Controls => Some(PanelId::Controls),
            SheetState::Presets => Some(PanelId::Presets),
            SheetState::Appearance => Some(PanelId::Appearance),
            SheetState::Interaction => Some(PanelId::Interaction),
            SheetState::Sources => Some(PanelId::Sources),
            SheetState::Induction => Some(PanelId::Induction),
            SheetState::Ode => Some(PanelId::Ode),
            SheetState::Materials => Some(PanelId::Materials),
            SheetState::Maxwell => Some(PanelId::Maxwell),
            #[cfg(not(target_arch = "wasm32"))]
            SheetState::Cas | SheetState::None => None,
            #[cfg(target_arch = "wasm32")]
            SheetState::None => None,
        };
        if let Some(panel) = sheet_panel {
            self.record_tour_panel(panel, sheet_response.response.rect);
        }

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
            if crate::ui::help_overlay::show_with_bindings(
                ui.ctx(),
                anchor,
                self.state.mode_3d,
                &self.interaction.bindings,
            ) {
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

        // The `Both` source display paints each family in its effective
        // Appearance colour, so the viewport names those colours under the
        // status line. The status text above is untouched; this is a second,
        // short line.
        if self.state.field_mode == FieldMode::Sources
            && self.state.sources.display == crate::source_scene::SourceDisplay::Both
            && let Some(rect) = self.last_viewport_rect
        {
            crate::ui::sources_panel::paint_both_legend(
                ui.painter(),
                egui::pos2(rect.left() + 12.0, rect.top() + 34.0),
                self.state.source_family_colors(),
            );
        }

        // Issue #230: a compact colour legend at the viewport's top-right. It
        // is a second view of the sidebar's existing mapping and writes through
        // the same State setters, so both stay in step. Modes whose Coloring
        // section is hidden keep their existing overlays and do not grow a new
        // colour editor here.
        let legend_visible = self.state.is_vector_mode()
            && !matches!(
                self.state.field_mode,
                FieldMode::Ode | FieldMode::Materials | FieldMode::Maxwell
            );
        if legend_visible && let Some(rect) = self.last_viewport_rect {
            let stats = self.color_facts.stats;
            crate::ui::viewport_legend::show(
                ui.ctx(),
                rect,
                &mut self.state,
                &mut self.controls,
                &stats,
                self.layout_mode == LayoutMode::Compact,
            );
        }

        // Help > About (ticket 15): a small window over everything, closed by
        // its × (its open flag is the window's visibility).
        top_bar::about_window(ui.ctx(), &mut self.about_open);

        let mut branch_requested = false;
        crate::ui::help_overlay::help_window_with_branch(
            ui.ctx(),
            &mut self.help_open,
            &self.interaction.bindings,
            &mut branch_requested,
        );
        if branch_requested {
            self.open_griffiths_branch();
        }

        // Issue #223 phase 1 / #224 / #225: the optional Griffiths picker plus
        // the ring and shell activities. All are session-only windows over the
        // current scene; they paint after Help so a branch opened from Help is
        // visible immediately.
        self.show_griffiths_overlays(ui.ctx());

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

    /// Test seam: load a named scene through the production load path.
    #[doc(hidden)]
    pub fn test_load_scene(&mut self, name: &str) {
        self.load_scene(name);
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
    /// Test seam: run the #229 shell-edit handler once, exactly as the frame
    /// does after the Maxwell panel draws.
    pub fn test_apply_gauss_shell_pending(&mut self) {
        self.apply_gauss_shell_pending();
    }

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

/// A rejected CAS action value, in the error shape the panel displays.
fn bad_action(field: &'static str, reason: impl Into<String>) -> CasActionError {
    CasActionError::BadValue {
        field,
        reason: reason.into(),
    }
}

/// The one refusal shared by the actions that edit the formula field. It
/// matches the #168 mathematical path exactly so a Sources-mode command can
/// never mutate a field the active adapter does not show.
fn require_formula_field_mode(state: &State) -> Result<(), CasActionError> {
    if state.field_mode == FieldMode::Formula {
        Ok(())
    } else {
        Err(CasActionError::UnsupportedHere(
            "CAS edits the formula field; switch to Formula.".to_owned(),
        ))
    }
}

/// Inclusive scene-domain containment, matching the probe placement gate.
fn domain_contains(domain: &Domain, position: Vec3) -> bool {
    position.x >= domain.min.x
        && position.x <= domain.max.x
        && position.y >= domain.min.y
        && position.y <= domain.max.y
        && position.z >= domain.min.z
        && position.z <= domain.max.z
}

/// Why a pinned Gauss item cannot show a live value in the active model.
fn gauss_unavailable_reason(state: &State) -> String {
    match state.field_mode {
        FieldMode::Maxwell if state.maxwell.case == crate::maxwell::MaxwellCase::HollowSphere => {
            "Activate the guided Gauss-shell investigation in the Maxwell panel.".to_owned()
        }
        FieldMode::Maxwell => {
            "The active Maxwell case is not the hollow sphere; the Gauss-shell radius and \
             flux belong to that case."
                .to_owned()
        }
        _ => "The Gauss-shell radius and flux are available only in the Maxwell hollow-sphere \
              model."
            .to_owned(),
    }
}
