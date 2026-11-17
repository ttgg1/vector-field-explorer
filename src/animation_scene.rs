//! Detached animation frame source.
//!
//! An [`AnimationSnapshot`] owns a clone of the app's view state, particle
//! cloud and camera. [`AnimationSnapshot::frame_at`] advances only that copy by
//! an explicit frame step, so an export never mutates live playback. Scene
//! composition reuses the same assembly the live viewport uses.

use glam::{Mat4, Vec3};

use crate::animation::AnimationError;
use crate::camera::{OrbitCamera, ViewCamera};
use crate::fly_camera::FlyCamera;
use crate::particles::{AdvectSettings, Particles, advance_exact};
use crate::renderer::SceneGPU;
use crate::source_scene::FieldMode;
use crate::viewmodel::State;

/// One rendered animation frame: geometry plus its camera matrices.
pub struct AnimationFrame {
    pub scene: SceneGPU,
    pub view_proj: Mat4,
    pub eye: Vec3,
}

/// A detached copy of everything an animation frame needs.
#[derive(Clone)]
pub struct AnimationSnapshot {
    pub state: State,
    pub particles: Particles,
    pub orbit: OrbitCamera,
    pub fly: Option<FlyCamera>,
    pub clear: [f64; 4],
}

impl AnimationSnapshot {
    /// Build a snapshot from explicit parts (the pure test seam).
    pub fn from_parts(
        state: State,
        particles: Particles,
        orbit: OrbitCamera,
        fly: Option<FlyCamera>,
        clear: [f64; 4],
    ) -> Self {
        Self {
            state,
            particles,
            orbit,
            fly,
            clear,
        }
    }

    /// Build one frame at `time`, advancing the detached particle cloud by
    /// `dt` unless this is the first captured frame.
    ///
    /// `first` is `true` for the captured frame: particles keep the positions
    /// they had when the snapshot was taken. Later frames integrate exactly
    /// `dt` seconds through [`advance_exact`], never the GUI `MAX_STEP` clamp.
    pub fn frame_at(
        &mut self,
        time: f32,
        dt: f32,
        first: bool,
    ) -> Result<AnimationFrame, AnimationError> {
        if !time.is_finite() || !dt.is_finite() || dt < 0.0 {
            return Err(AnimationError::InvalidSpec(
                "animation frame time and step must be finite".into(),
            ));
        }
        match self.state.field_mode {
            FieldMode::Formula | FieldMode::Induction => {}
            FieldMode::Sources | FieldMode::Materials | FieldMode::Ode => {
                return Err(AnimationError::InvalidSpec(
                    "animation supports Formula and Induction scenes".into(),
                ));
            }
        }

        let start_time = self.state.time;
        self.state.set_time(time);
        // Rebuild from scratch for this frame: the snapshot has no app-side
        // dirty cache and must not depend on how the live app last sampled.
        self.state.mark_dirty();
        let mut base = self
            .state
            .snapshot_if_dirty()
            .ok_or_else(|| AnimationError::InvalidSpec("animation scene did not rebuild".into()))?;
        let dots_range = crate::app::scene_dots_range(&self.state);

        if !first && self.state.particles && self.state.particle_target().is_some() && dt > 0.0 {
            let flow = self.state.scene_flow();
            let domain = self.state.scene_domain();
            let appearance = self.state.particle_appearance.sanitized();
            advance_exact(
                &mut self.particles,
                &flow,
                &domain,
                start_time,
                dt,
                AdvectSettings {
                    dt,
                    time: start_time,
                    trail_len: appearance.trail_len,
                    loop_on_stagnation: appearance.loop_on_stagnation,
                    stagnation_seconds: appearance.stagnation_seconds,
                },
            );
            let (dots, trails) =
                crate::app::particle_scene_instances(&self.state, &self.particles, dots_range);
            base.dots = dots;
            base.trails = trails;
        }

        let (lines, _) = crate::app::compose_vector_lines(&self.state, &base.lines, &[]);
        base.lines = std::sync::Arc::new(lines);

        let camera = match &self.fly {
            Some(fly) => ViewCamera::Fly(fly),
            None => ViewCamera::Orbit(&self.orbit),
        };
        // The snapshot carries no viewport dimensions; a square aspect keeps
        // the returned matrix usable and deterministic. Callers that paint a
        // non-square output can rescale the projection when they know the
        // output size.
        let aspect = 1.0;
        let view_proj = camera.vp_matrix(
            aspect,
            if self.state.mode_3d {
                None
            } else {
                Some(self.state.plane_size())
            },
        );
        let eye = camera.eye_position(!self.state.mode_3d);
        let view = camera.view();
        let scene = crate::app::assemble_vector_scene(&self.state, &mut base, &[], &view);
        Ok(AnimationFrame {
            scene,
            view_proj,
            eye,
        })
    }
}
