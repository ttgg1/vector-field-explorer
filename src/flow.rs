//! Flow: the vector field as the tracer sees it.
//!
//! A [`Flow`] maps a point in the space being traced to the velocity there.
//! The world [`FieldExpr`] is one flow; a projected slice is another, whose
//! points live in the plane's local `(u, v, 0)` frame and whose velocities are
//! the field projected onto that frame. Keeping this seam generic lets the
//! RK4 tracer walk either space without knowing which one it is in.

use glam::Vec3;

use crate::field::FieldExpr;
use crate::field_bake::BakedField;
use crate::plane::PlaneFrame;

/// A velocity field the streamline tracer can integrate.
pub trait Flow {
    /// Velocity at `point` at time `t`; `None` where the field cannot be
    /// evaluated (the trace stops there).
    fn eval(&self, point: Vec3, t: f32) -> Option<Vec3>;

    /// Whether an exact zero velocity means "stationary in this space" rather
    /// than "cannot move, respawn". A plane slice is a lower-dimensional
    /// space: a purely normal world field has no in-plane motion, so its dots
    /// should stay put instead of jumping to a new random spot every frame.
    /// The world flow keeps the historical respawn behavior.
    fn zero_velocity_is_stationary(&self) -> bool {
        false
    }
}

impl Flow for FieldExpr {
    fn eval(&self, point: Vec3, t: f32) -> Option<Vec3> {
        self.eval3_at(point, t)
    }
}

/// A slice's flow: local plane coordinates in, projected local velocity out.
///
/// Positions are `(u, v, 0)` in [`PlaneFrame`]'s basis; evaluation lifts the
/// point to world space for the field, then projects the world velocity back
/// onto the plane so the tracer never leaves the slice.
#[derive(Debug, Clone, Copy)]
pub struct PlaneFlow<'a> {
    pub field: &'a FieldExpr,
    pub frame: PlaneFrame,
}

impl Flow for PlaneFlow<'_> {
    fn eval(&self, point: Vec3, t: f32) -> Option<Vec3> {
        let world = self.frame.to_world(point);
        self.field
            .eval3_at(world, t)
            .map(|velocity| self.frame.project(velocity))
    }

    fn zero_velocity_is_stationary(&self) -> bool {
        true
    }
}

/// A baked lattice viewed through the same seam as [`FieldExpr`] or
/// [`PlaneFlow`].
///
/// The lattice is stored in world coordinates. With `plane = Some(frame)`,
/// evaluation lifts the local `(u, v, 0)` point through
/// [`PlaneFrame::to_world`], samples the world bake, then projects the velocity
/// with [`PlaneFrame::project`], so the result stays in the slice exactly like
/// [`PlaneFlow`]. With `plane = None` it samples the world point directly.
///
/// The simulated time is ignored: a bake is only built for
/// [`FieldExpr::is_time_dependent`] `== false`, so there is no time input to
/// lose. Slice 1a adds this model only; no default particle, streamline or
/// `advance_exact` call site selects it yet.
#[derive(Debug, Clone, Copy)]
pub struct BakedFlow<'a> {
    pub baked: &'a BakedField,
    pub plane: Option<PlaneFrame>,
}

impl Flow for BakedFlow<'_> {
    fn eval(&self, point: Vec3, _t: f32) -> Option<Vec3> {
        match self.plane {
            Some(frame) => {
                let world = frame.to_world(point);
                self.baked
                    .sample(world)
                    .map(|velocity| frame.project(velocity))
            }
            None => self.baked.sample(point),
        }
    }

    fn zero_velocity_is_stationary(&self) -> bool {
        // Mirrors `PlaneFlow`: a lower-dimensional slice treats an exact zero
        // in-plane velocity as stationary; the world flow keeps respawning.
        self.plane.is_some()
    }
}

/// The field the current scene traces: the world field or a projected slice.
#[derive(Debug, Clone, Copy)]
pub enum SceneFlow<'a> {
    World(&'a FieldExpr),
    Plane(PlaneFlow<'a>),
}

impl Flow for SceneFlow<'_> {
    fn eval(&self, point: Vec3, t: f32) -> Option<Vec3> {
        match self {
            SceneFlow::World(field) => field.eval(point, t),
            SceneFlow::Plane(flow) => flow.eval(point, t),
        }
    }

    fn zero_velocity_is_stationary(&self) -> bool {
        match self {
            SceneFlow::World(_) => false,
            SceneFlow::Plane(flow) => flow.zero_velocity_is_stationary(),
        }
    }
}
