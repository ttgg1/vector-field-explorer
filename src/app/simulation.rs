//! Simulation: the clock, particle spawning/advection, and scene rebuilds.

use super::*;

impl VfApp {
    pub(super) fn advance_clock(&mut self, ctx: &egui::Context) {
        // One global clock advances once per frame from egui's stable frame
        // time, but only while something consumes it. `advance_for` clamps to
        // MAX_STEP, ignores non-finite frames, holds the held time while
        // particles are off/in 2D or paused, and resumes from that time when a
        // consumer returns. A long idle gap still cannot make the particle
        // cloud jump.
        self.clock
            .advance_for(&self.state, ctx.input(|input| input.stable_dt));
    }

    /// Frame phase: scene rebuild on a dirty state, particle advection and dot
    /// rebuild, then the repaint request that keeps particles animating.
    pub(super) fn frame_simulation(&mut self, ctx: &egui::Context) {
        self.rebuild_scene();
        self.advance_particles();

        // Keep egui painting only while something consumes the clock (particles
        // on in 3D and playing). Paused or consumer-free frames request
        // nothing, so the app goes idle instead of busy-looping.
        crate::ui::transport::request_particle_repaint(ctx, &self.state, &self.clock);
    }

    fn rebuild_scene(&mut self) {
        // Rebuild the scene only when a setter marked the state dirty. The
        // app keeps owning the backdrop lines: fresh snapshots arrive on top
        // of a rebuilt grid + axes, and the streamline segments the viewmodel
        // seam emitted join them.
        if let Some(fresh) = self.state.snapshot_if_dirty() {
            self.scene.arrow_style = fresh.arrow_style;
            self.scene.arrow_style_2d = fresh.arrow_style_2d;
            self.scene.arrow_params = fresh.arrow_params;
            self.scene.mode_3d = fresh.mode_3d;
            self.scene.arrows = fresh.arrows;
            self.scene.dots = fresh.dots;
            self.scene.combined = fresh.combined;
            // A rebuild refreshes the magnitude range the per-frame dots ramp
            // with (one extra sample pass on rebuild frames only), so dots
            // keep matching the arrows' colours.
            self.dots_range = self.state.color_range(&self.state.sample());
            self.flow_lines = Arc::unwrap_or_clone(fresh.lines);
            // Click bursts outlive the rebuild: they are anchored on the field,
            // not on the sampling grid, so retrace them against the new
            // domain/formula rather than dropping the user's picks.
            self.click_lines = self.state.click_streamlines_snapshot(self.dots_range, &self.click_seeds);
            self.streamline_count = self.flow_lines.len() + self.click_lines.len();
            self.scene.lines = Arc::new(compose_lines(
                &self.flow_lines,
                &self.click_lines,
                &self.state.domain,
                self.state.is_3d(),
            ));
            log::trace!(
                "scene rebuilt: {} arrows, {} streamline segments, {} dots",
                self.scene.arrows.len(),
                self.streamline_count,
                self.scene.dots.len()
            );
        }
    }

    fn advance_particles(&mut self) {
        // Particle cloud: while the toggle is on (3D view only — the 2D
        // camera has no depth for the dots to sit in), keep the cloud alive,
        // advance it by the simulation clock's step and rebuild the dot
        // instances so they ride the same color_range as the arrows. A paused
        // clock reports a zero step, so the cloud keeps its last positions.
        // Placed after the dirty-rebuild block so this frame's dots already see
        // a fresh density, and before the status bar so its count includes them.
        //
        // The target follows `particle_count` (0 = Auto, density x 16). Only a
        // change retargets: a lower target trims, a higher one spawns just the
        // difference so existing particles never jump, and a click burst that
        // sits above the configured count survives until the count changes.
        // Off/2D yields `None` and spawns nothing (the clear branch drops the
        // cloud).
        let target = self.state.particle_target();
        self.particle_target = retarget(
            &mut self.particles,
            &self.state.domain,
            self.particle_target,
            target,
            3,
        );
        if target.is_some() {
            advect(
                &mut self.particles,
                &self.state.field,
                &self.state.domain,
                self.clock.step_dt(),
            );
            self.scene.dots = Arc::new(
                dot_instances(
                    &self.particles,
                    &self.state.field,
                    &self.state.domain,
                    self.state.colormap,
                    self.dots_range,
                    self.state.particle_radius(self.state.spacing()),
                ),
            );
            log::trace!(
                "particles: {} advected, {} dot instances",
                particle_count(&self.particles),
                self.scene.dots.len()
            );
        } else if particle_count(&self.particles) > 0 {
            // Toggle off (or the 2D view): drop the cloud so switching back
            // spawns fresh at the fixed seed; the dots were already emptied
            // because the toggle marks the state dirty.
            clear(&mut self.particles);
        }
    }
}
