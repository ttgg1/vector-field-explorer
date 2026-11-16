//! Scene-frame transitions for plane slices (issue 13 task 6).
//!
//! The app keeps one [`SceneFrameKey`] describing the frame the cached scene
//! and click seeds belong to. Comparing it at the start of each vector frame
//! lets a plane entry/exit/step drop stale local seeds and particles, and lets a
//! real plane change refit the orthographic camera, without touching ordinary
//! 3D or two-component 2D state.

use glam::UVec3;

use crate::domain::Domain;
use crate::particles::clear;
use crate::plane::PlaneAxis;
use crate::view_command::ViewCommand;
use crate::viewmodel::State;

use super::VfApp;

/// Which frame the cached scene belongs to.
///
/// `World2D` is the ordinary two-component/3D-mode view; `Plane` carries the
/// exact slice selection so a density or domain edit inside the same plane is
/// distinguishable from an axis/index change.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SceneFrameKey {
    World3D,
    World2D,
    Plane {
        axis: PlaneAxis,
        index: u32,
        plane_grid: UVec3,
        domain: Domain,
    },
}

impl SceneFrameKey {
    /// Classify the state's current visible frame.
    pub fn of(state: &State) -> Self {
        if state.is_plane_slice() {
            Self::Plane {
                axis: state.plane.axis,
                index: state.plane.index,
                plane_grid: state.plane_grid(),
                domain: state.domain,
            }
        } else if state.mode_3d {
            Self::World3D
        } else {
            Self::World2D
        }
    }

    fn is_plane(self) -> bool {
        matches!(self, Self::Plane { .. })
    }
}

/// Entering a plane or changing its axis/index needs a local refit. A density
/// or domain edit inside the same plane keeps the user's current framing.
fn fits_on_change(previous: SceneFrameKey, next: SceneFrameKey) -> bool {
    let SceneFrameKey::Plane { axis, index, .. } = next else {
        return false;
    };
    match previous {
        SceneFrameKey::Plane {
            axis: previous_axis,
            index: previous_index,
            ..
        } => previous_axis != axis || previous_index != index,
        SceneFrameKey::World2D | SceneFrameKey::World3D => true,
    }
}

impl VfApp {
    /// Reconcile the cached scene with the state's frame before this frame's
    /// rebuild and particle step. A plane key change drops the local seeds,
    /// undo record and particle cloud; entering a plane or changing its
    /// axis/index also fits the local camera.
    pub(super) fn sync_scene_frame(&mut self) {
        let previous = self.scene_frame;
        let next = SceneFrameKey::of(&self.state);
        let plane_changed = (previous.is_plane() || next.is_plane()) && previous != next;
        let fit = fits_on_change(previous, next);
        let available = self.state.field.dimension == 3;
        let lost_availability = self.particle_available && !available;

        if plane_changed {
            // Local click seeds belong to one slice; a plane entry, exit, step
            // or grid/domain change cannot retrace them in the new frame.
            self.click_seeds.clear();
            self.last_click_seed = None;
            clear(&mut self.particles);
            self.particle_target = None;
            self.flow_lines.clear();
            self.click_lines.clear();
            self.streamline_count = 0;
            self.state.mark_dirty();
        }

        if lost_availability {
            // A two-component field has no third axis, so the cloud and its
            // burst-undo record are stale even though the 3D/2D key did not
            // move. Streamline click seeds are field-anchored and survive.
            clear(&mut self.particles);
            self.particle_target = None;
            if self.last_click_seed.is_some_and(|last| !last.stored) {
                self.last_click_seed = None;
            }
            self.state.mark_dirty();
        }

        self.particle_available = available;
        self.scene_frame = next;
        if fit {
            self.apply_view_command(ViewCommand::Fit);
        }
    }

    /// Test seam: whether a click burst undo record is still pending.
    #[doc(hidden)]
    pub fn pending_burst(&self) -> bool {
        self.last_click_seed.is_some()
    }
}
