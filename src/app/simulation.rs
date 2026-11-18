//! Simulation: the clock, particle spawning/advection, and scene rebuilds.

use super::*;

impl VfApp {
    /// Advance the one global clock from the frame's stable dt and mirror the
    /// resulting time into the state. Scalar mode still skips vector particle
    /// advection, but a time-dependent scalar is itself a clock consumer: the
    /// shared clock advances and the plot resamples at the new `t`. A static
    /// scalar has no consumer, so the clock holds.
    pub(super) fn advance_clock(&mut self, dt: f32) {
        self.clock.advance_for(&self.state, dt);
        if self.state.field_mode == FieldMode::Ode
            && let Some(solution) = self.state.ode_solution.as_ref()
        {
            // The precomputed solution ends at its duration; stop there rather
            // than replaying past the last sample. `clamp_time` pauses once.
            self.clock.clamp_time(solution.duration as f32);
        }
        // Mirror the clock's t into the state so a time-dependent field's
        // rebuild follows the clock automatically.
        self.state.set_time(self.clock.time);
    }

    /// Frame phase: scene rebuild on a dirty state, particle advection and dot
    /// rebuild, then the repaint request that keeps particles animating.
    pub(super) fn frame_simulation(&mut self, ctx: &egui::Context) {
        self.advance_simulation();

        // Keep egui painting only while something consumes the clock (particles
        // on in 3D and playing). Paused or consumer-free frames request
        // nothing, so the app goes idle instead of busy-looping.
        crate::ui::transport::request_particle_repaint(ctx, &self.state, &self.clock);
    }

    /// The device-independent half of simulation, shared with the Task 6 test
    /// harness: scalar mode refreshes only its plot scene (resampling a
    /// time-dependent source at the mirrored `state.time`) and pauses every
    /// vector consumer; vector mode rebuilds and advects exactly as before.
    pub(super) fn advance_simulation(&mut self) {
        self.sync_field_mode();
        if self.state.field_mode == FieldMode::Ode {
            self.advance_ode();
            return;
        }
        if matches!(
            self.state.field_mode,
            FieldMode::Materials | FieldMode::Maxwell
        ) {
            // Static analytic arrows plus the case outline: no formula
            // sampling, particles, streamlines, clicks, glyphs or scalar plot.
            self.sync_scene_frame();
            self.rebuild_scene();
            return;
        }
        if !self.state.is_vector_mode() {
            // Pause, never delete: cloud/trail/click/time state is untouched.
            self.refresh_plot_scene();
            return;
        }
        self.sync_scene_frame();
        self.rebuild_scene();
        self.advance_particles();
    }

    /// Ode's frame path: solve only when its scene is pending, rebuild the
    /// trajectory lines once per solved scene/edit, and never sample the field,
    /// retrace clicks, spawn particles or append source/induction glyphs.
    fn advance_ode(&mut self) {
        self.state.refresh_ode_if_dirty();
        let epoch = self.state.ode_epoch;
        let epoch_changed = self.last_ode_epoch != Some(epoch);
        if epoch_changed {
            // A new solution is a new playback: restart the clock but keep the
            // user's play/pause and speed choice.
            self.clock.reset_time();
            self.clock.loop_period = None;
            self.last_ode_epoch = Some(epoch);
        }
        if epoch_changed || self.state.dirty {
            self.rebuild_ode_lines();
        }
        // A failed lazy solve has no solution to frame yet, so keep the one-shot
        // pending until the first valid solution arrives. A preserved solution
        // with no new epoch clears it without moving the camera; later epochs
        // never fit again.
        if self.ode_autofit_pending && self.state.ode_solution.is_some() {
            if epoch_changed {
                self.apply_view_command(ViewCommand::Fit);
            }
            self.ode_autofit_pending = false;
        }
        self.state.dirty = false;
    }

    /// Clear formula-owned transient geometry when the field mode changes.
    /// Durable formula and source state stays in the viewmodel untouched.
    fn sync_field_mode(&mut self) {
        let mode = self.state.field_mode;
        if mode == self.last_field_mode {
            return;
        }
        let leaving_ode = self.last_field_mode == FieldMode::Ode;
        self.last_field_mode = mode;
        clear(&mut self.particles);
        self.particle_target = None;
        self.click_seeds.clear();
        self.last_click_seed = None;
        self.flow_lines.clear();
        self.click_lines.clear();
        self.streamline_count = 0;
        if leaving_ode {
            // The next mode rebuilds its own lines; never repaint stale ODE
            // trajectories in Formula/Sources/Induction.
            self.ode_lines = Arc::new(Vec::new());
        }
        if mode == FieldMode::Ode {
            // ODE bounds its playback by the solution duration, so a shared
            // loop period must not wrap it. Arm the one auto-fit allowed for
            // this entry; a preserved solution with no new epoch leaves the
            // camera untouched.
            self.clock.loop_period = None;
            self.last_ode_epoch = Some(self.state.ode_epoch);
            self.ode_autofit_pending = true;
            self.ode.sync_from(&self.state);
        }
        if mode == FieldMode::Maxwell {
            // Maxwell 2D is a plane slice. Entering the mode is not a plane
            // edit, so record the new frame now: the user's camera/projection
            // must survive the round trip instead of a fresh plane fit.
            self.scene_frame = SceneFrameKey::of(&self.state);
        }
        self.state.mark_dirty();
    }

    fn rebuild_scene(&mut self) {
        // Maxwell is a static arrows + outline branch before the formula path.
        // It samples only when dirty, keeps click/streamline buffers empty and
        // never traces or advects the formula field.
        if self.state.field_mode == FieldMode::Maxwell {
            if !self.state.dirty {
                return;
            }
            self.state.refresh_maxwell_sampling();
            let fresh = self.state.snapshot();
            self.dots_range = scene_dots_range(&self.state);
            self.flow_lines.clear();
            self.click_lines.clear();
            self.streamline_count = 0;
            self.source_glyph_line_count = 0;
            let (mut lines, _) = compose_vector_lines(&self.state, &[], &[]);
            lines.extend(fresh.lines.iter().cloned());
            if let Some(scene) = self.scene.as_vector_mut() {
                scene.arrow_style = fresh.arrow_style;
                scene.arrow_style_2d = fresh.arrow_style_2d;
                scene.arrow_params = fresh.arrow_params;
                scene.mode_3d = fresh.mode_3d;
                scene.arrows = fresh.arrows;
                scene.lines = Arc::new(lines);
                scene.trails = Arc::new(Vec::new());
                scene.dots = Arc::new(Vec::new());
                scene.combined = Arc::new(Vec::new());
            }
            self.state.dirty = false;
            return;
        }
        // Rebuild the scene only when a setter marked the state dirty. The
        // app keeps owning the backdrop lines: fresh snapshots arrive on top
        // of a rebuilt grid + axes, and the streamline segments the viewmodel
        // seam emitted join them.
        if let Some(fresh) = self.state.snapshot_if_dirty() {
            let arrow_style = fresh.arrow_style;
            let arrow_style_2d = fresh.arrow_style_2d;
            let arrow_params = fresh.arrow_params;
            let mode_3d = fresh.mode_3d;
            // Induction and Materials are arrows-only: their arrows carry
            // their own colours and there is no particle cloud to share a
            // range with. Avoid sampling the formula field, which is not on
            // screen.
            self.dots_range = scene_dots_range(&self.state);
            self.flow_lines = Arc::unwrap_or_clone(fresh.lines);
            // Click bursts outlive the rebuild: they are anchored on the field,
            // not on the sampling grid, so retrace them against the new
            // domain/formula rather than dropping the user's picks.
            self.click_lines = self
                .state
                .click_streamlines_snapshot(self.dots_range, &self.click_seeds);
            self.streamline_count = self.flow_lines.len() + self.click_lines.len();
            let (lines, glyph_count) =
                compose_vector_lines(&self.state, &self.flow_lines, &self.click_lines);
            self.source_glyph_line_count = glyph_count;
            let lines = Arc::new(lines);
            if let Some(scene) = self.scene.as_vector_mut() {
                scene.arrow_style = arrow_style;
                scene.arrow_style_2d = arrow_style_2d;
                scene.arrow_params = arrow_params;
                scene.mode_3d = mode_3d;
                scene.arrows = fresh.arrows;
                scene.trails = fresh.trails;
                scene.dots = fresh.dots;
                scene.combined = fresh.combined;
                scene.lines = lines;
            }
            log::trace!(
                "scene rebuilt: {} arrows, {} streamline segments, {} dots",
                self.scene
                    .as_vector()
                    .map(|scene| scene.arrows.len())
                    .unwrap_or(0),
                self.streamline_count,
                self.scene
                    .as_vector()
                    .map(|scene| scene.dots.len())
                    .unwrap_or(0)
            );
        }
    }

    /// Rebuild the cached Ode line buffer from the current solution plus
    /// coordinate axes. One trajectory line instance per finite segment; width
    /// is the fixed fraction of the padded view-domain extent. Called only on
    /// a new solution or an Ode-relevant edit, never per frame.
    fn rebuild_ode_lines(&mut self) {
        let Some(solution) = self.state.ode_solution.as_ref() else {
            self.ode_lines = Arc::new(Vec::new());
            self.set_ode_scene_geometry();
            return;
        };
        let domain = solution.view_domain();
        let size = domain.domain_size();
        let extent = size.x.max(size.y).max(size.z).max(1e-6);
        let width = (crate::ode::ODE_LINE_WIDTH_FACTOR as f32) * extent;
        let mut lines = Vec::new();
        for (start, end, color, _seed) in crate::ode::ode_trajectory_lines(solution) {
            lines.push(LineInstanceGPU {
                a: [start[0], start[1], start[2], 1.0],
                b: [end[0], end[1], end[2], width],
                color,
            });
        }
        if self.state.show_coordinate_system {
            // `active_dimension` reads the solved scene; never hardcode 3.
            for (start, end, color) in crate::ode::ode_axes(domain, self.state.active_dimension()) {
                lines.push(LineInstanceGPU {
                    a: [start[0], start[1], start[2], 1.0],
                    b: [end[0], end[1], end[2], width],
                    color,
                });
            }
        }
        self.ode_lines = Arc::new(lines);
        self.set_ode_scene_geometry();
    }

    /// Replace the cached vector scene with Ode's lines and empty feature
    /// collections. The per-frame markers live only in `assemble_viewport_scene`.
    fn set_ode_scene_geometry(&mut self) {
        self.flow_lines.clear();
        self.click_lines.clear();
        self.streamline_count = 0;
        self.source_glyph_line_count = 0;
        self.dots_range = (0.0, 1.0);
        if let Some(scene) = self.scene.as_vector_mut() {
            scene.arrows = Arc::new(Vec::new());
            scene.lines = Arc::clone(&self.ode_lines);
            scene.trails = Arc::new(Vec::new());
            scene.dots = Arc::new(Vec::new());
            scene.combined = Arc::new(Vec::new());
            scene.arrow_style = self.state.arrow_style;
            scene.arrow_style_2d = self.state.arrow_style_2d;
            scene.arrow_params = self.state.arrow_params;
            scene.mode_3d = self.state.mode_3d;
        }
    }

    /// Current-time markers for every trajectory, as dot instances. 2D appends
    /// them to the cached dots copy; 3D joins the sorted combined stream. The
    /// cached scene is never mutated.
    pub(super) fn ode_marker_instances(&self) -> Vec<ArrowInstanceGPU> {
        let Some(solution) = self.state.ode_solution.as_ref() else {
            return Vec::new();
        };
        let domain = solution.view_domain();
        let size = domain.domain_size();
        let extent = size.x.max(size.y).max(size.z).max(1e-6);
        let radius = (crate::ode::ODE_MARKER_RADIUS_FACTOR as f32) * extent;
        solution
            .markers_at(self.state.time as f64)
            .into_iter()
            .enumerate()
            .filter_map(|(seed, position)| {
                let position: [f32; 3] = std::array::from_fn(|axis| position[axis] as f32);
                if !position.iter().all(|value| value.is_finite()) {
                    return None;
                }
                Some(ArrowInstanceGPU {
                    origin: [
                        position[0],
                        position[1],
                        position[2],
                        crate::renderer::PARTICLE_KIND,
                    ],
                    dir_len: [0.0, 0.0, 1.0, radius],
                    color: crate::ode::ODE_COLORS[seed % crate::ode::ODE_COLORS.len()],
                })
            })
            .collect()
    }

    fn advance_particles(&mut self) {
        // Particle cloud: while the toggle is on for a three-component field,
        // keep the cloud alive, advance it by the simulation clock's step and
        // rebuild the dot instances so they ride the same color_range as the
        // arrows. A 2D plane slice uses its local flat domain and projected
        // flow, so dots stay on the slice; the 3D view uses the world flow and
        // full depth. A paused clock reports a zero step, so the cloud keeps
        // its last positions.
        // Placed after the dirty-rebuild block so this frame's dots already see
        // a fresh density, and before the status bar so its count includes them.
        //
        // The target follows `particle_count` (0 = Auto, density x 16). Only a
        // change retargets: a lower target trims, a higher one spawns just the
        // difference so existing particles never jump, and a click burst that
        // sits above the configured count survives until the count changes.
        // Off/2D-without-depth yields `None` and spawns nothing (the clear
        // branch drops the cloud).
        let target = self.state.particle_target();
        let domain = self.state.scene_domain();
        let dimension = if self.state.mode_3d { 3 } else { 2 };
        self.particle_target = retarget(
            &mut self.particles,
            &domain,
            self.particle_target,
            target,
            dimension,
        );
        if target.is_some() {
            let appearance = self.state.particle_appearance.sanitized();
            let flow = self.state.scene_flow();
            advect(
                &mut self.particles,
                &flow,
                &domain,
                AdvectSettings {
                    dt: self.clock.step_dt(),
                    time: self.state.time,
                    trail_len: appearance.trail_len,
                    loop_on_stagnation: appearance.loop_on_stagnation,
                    stagnation_seconds: appearance.stagnation_seconds,
                },
            );
            let (dots, trails) =
                particle_scene_instances(&self.state, &self.particles, self.dots_range);
            let count = particle_count(&self.particles);
            if let Some(scene) = self.scene.as_vector_mut() {
                scene.dots = dots;
                scene.trails = trails;
            }
            log::trace!(
                "particles: {} advected, {} dot instances, {} trail segments",
                count,
                self.scene
                    .as_vector()
                    .map(|scene| scene.dots.len())
                    .unwrap_or(0),
                self.scene
                    .as_vector()
                    .map(|scene| scene.trails.len())
                    .unwrap_or(0)
            );
        } else if particle_count(&self.particles) > 0 {
            // Toggle off (or a two-component field with no depth/slice flow):
            // drop the cloud so switching back spawns fresh at the fixed seed;
            // the dots were already emptied because the toggle marks the state
            // dirty, and the trails reset here because the snapshot never
            // carries per-frame history. A 2D plane slice is a valid target
            // and keeps its cloud and trails.
            clear(&mut self.particles);
            if let Some(scene) = self.scene.as_vector_mut() {
                scene.trails = Arc::new(Vec::new());
            }
        }
    }
}
