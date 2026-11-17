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
        if !self.state.is_vector_mode() {
            // Pause, never delete: cloud/trail/click/time state is untouched.
            self.refresh_plot_scene();
            return;
        }
        self.sync_scene_frame();
        self.rebuild_scene();
        self.advance_particles();
    }

    /// Clear formula-owned transient geometry when the field mode changes.
    /// Durable formula and source state stays in the viewmodel untouched.
    fn sync_field_mode(&mut self) {
        let mode = self.state.field_mode;
        if mode == self.last_field_mode {
            return;
        }
        self.last_field_mode = mode;
        clear(&mut self.particles);
        self.particle_target = None;
        self.click_seeds.clear();
        self.last_click_seed = None;
        self.flow_lines.clear();
        self.click_lines.clear();
        self.streamline_count = 0;
        self.state.mark_dirty();
    }

    fn rebuild_scene(&mut self) {
        // Rebuild the scene only when a setter marked the state dirty. The
        // app keeps owning the backdrop lines: fresh snapshots arrive on top
        // of a rebuilt grid + axes, and the streamline segments the viewmodel
        // seam emitted join them.
        if let Some(fresh) = self.state.snapshot_if_dirty() {
            let arrow_style = fresh.arrow_style;
            let arrow_style_2d = fresh.arrow_style_2d;
            let arrow_params = fresh.arrow_params;
            let mode_3d = fresh.mode_3d;
            self.dots_range = self.state.color_range(&self.state.sample());
            self.flow_lines = Arc::unwrap_or_clone(fresh.lines);
            // Click bursts outlive the rebuild: they are anchored on the field,
            // not on the sampling grid, so retrace them against the new
            // domain/formula rather than dropping the user's picks.
            self.click_lines = self
                .state
                .click_streamlines_snapshot(self.dots_range, &self.click_seeds);
            self.streamline_count = self.flow_lines.len() + self.click_lines.len();
            let mut lines = compose_lines_with_frame(
                &self.flow_lines,
                &self.click_lines,
                &self.state.scene_domain(),
                self.state.is_3d(),
                self.state.plane_frame(),
                self.state.show_grid,
                self.state.show_coordinate_system,
            );
            // Source markers are contextual overlays on the current sampling
            // scene. Only the source adapter emits them; Formula preserves the
            // durable scene but draws none.
            let glyph_lines = if self.state.field_mode == crate::source_scene::FieldMode::Sources {
                let glyphs = crate::source_glyph::source_glyphs(
                    &self.state.sources,
                    &self.state.scene_domain(),
                    self.state.plane_frame().as_ref(),
                );
                crate::source_glyph::source_glyph_lines(&glyphs, &self.state.scene_domain())
            } else {
                Vec::new()
            };
            self.source_glyph_line_count = glyph_lines.len();
            lines.extend(glyph_lines);
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
            let spacing = self.state.spacing();
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
            // The speed range is observed once per frame and shared by the dots
            // and trails, so both colour modes agree on the same cloud.
            let speeds = speed_range(&self.particles);
            let dots = Arc::new(dot_instances_at(
                &self.particles,
                &flow,
                &domain,
                self.state.colormap,
                self.dots_range,
                &appearance,
                speeds,
                self.state.particle_radius(spacing),
                self.state.time,
            ));
            let trails = Arc::new(trail_instances_at(
                &self.particles,
                &flow,
                self.state.colormap,
                self.dots_range,
                &appearance,
                speeds,
                spacing,
                self.state.time,
            ));
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
