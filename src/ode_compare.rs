//! Session-only comparison between the frozen field line of `v(x, 0)` and the
//! time-dependent ODE trajectory of `dx/dt = v(x, t)`.
//!
//! The frozen line is display geometry traced with the shipped formula field;
//! the trajectory remains the existing fixed-step RK4 solution from
//! [`crate::ode`]. Nothing here is persisted.

use glam::DVec3;

use crate::domain::Domain;
use crate::field::FieldExpr;
use crate::ode::{
    ODE_MAX_DIMENSION, ODE_MAX_SAMPLES, ODE_MIN_DIMENSION, OdeError, OdeScene, OdeSeed, OdeSource,
};
use crate::viewmodel::State;

/// Colour of the frozen field line at the reference time. Deliberately outside
/// the eight-colour trajectory palette so both curves stay distinguishable.
pub const FROZEN_COLOR: [f32; 4] = [0.15, 0.95, 0.65, 1.0];
/// The frozen line is always the field line of `v(x, t)` at this time.
pub const FROZEN_REFERENCE_TIME: f64 = 0.0;
/// Shared seed used by both reference actions.
pub const REFERENCE_SEED: [f64; 3] = [0.0, 0.0, 0.0];
/// Readable duration for the reference scenes.
pub const REFERENCE_DURATION: f64 = 2.0;
/// Spatial step of the frozen polyline, as a fraction of the view extent.
pub const FROZEN_STEP_FACTOR: f64 = 0.002;
/// Bound on the frozen polyline work per direction; mirrors the ODE sample cap.
pub const FROZEN_MAX_STEPS: usize = ODE_MAX_SAMPLES;

/// Legend label for the frozen field line.
pub const FROZEN_LABEL: &str = "Frozen field line at t = 0";
/// Legend label for the integrated trajectory.
pub const TRAJECTORY_LABEL: &str = "Trajectory dx/dt = v(x, t)";
/// The exercise's velocity-field caveat.
pub const CAVEAT: &str =
    "This is a velocity field: a field line is not a charged particle trajectory.";

/// A display polyline plus its colour.
#[derive(Debug, Clone, PartialEq)]
pub struct FrozenLine {
    pub points: Vec<[f64; 3]>,
    pub color: [f32; 4],
}

/// Session-only comparison state. Never serialized.
#[derive(Debug, Clone, PartialEq)]
pub struct OdeCompareState {
    pub enabled: bool,
    pub arm_seed: bool,
    pub seed: [f64; 3],
    pub reference_time: f64,
    pub frozen: Option<FrozenLine>,
    pub previous_formula: Option<String>,
}

impl Default for OdeCompareState {
    fn default() -> Self {
        Self {
            enabled: false,
            arm_seed: false,
            seed: REFERENCE_SEED,
            reference_time: FROZEN_REFERENCE_TIME,
            frozen: None,
            previous_formula: None,
        }
    }
}

/// The time-dependent reference formula.
pub fn reference_formula() -> &'static str {
    "(1, t)"
}

/// The steady uniform reference formula.
pub fn steady_formula() -> &'static str {
    "(1, 0)"
}

/// Trace the field line of `v(x, 0)` from `seed` in both directions, bounded by
/// `domain` and `max_steps` per direction. The step is a fixed fraction of the
/// view extent along the unit field direction; this is display geometry, not
/// an adaptive integrator.
pub fn trace_frozen_line(
    field: &FieldExpr,
    seed: [f64; 3],
    dimension: usize,
    domain: Domain,
    max_steps: usize,
) -> Option<FrozenLine> {
    if !(ODE_MIN_DIMENSION..=ODE_MAX_DIMENSION).contains(&dimension)
        || !seed.iter().all(|value| value.is_finite())
    {
        return None;
    }
    let size = domain.domain_size();
    let extent = size.x.abs().max(size.y.abs()).max(size.z.abs()).max(1e-6) as f64;
    let step = (FROZEN_STEP_FACTOR * extent).max(1e-9);
    let max_steps = max_steps.max(1);

    let forward = trace_direction(field, seed, dimension, domain, step, max_steps, 1.0);
    let backward = trace_direction(field, seed, dimension, domain, step, max_steps, -1.0);

    let mut points = Vec::with_capacity(forward.len() + backward.len());
    points.extend(backward.into_iter().rev());
    points.extend(forward.into_iter().skip(1));
    if points.len() < 2 {
        return None;
    }
    Some(FrozenLine {
        points,
        color: FROZEN_COLOR,
    })
}

fn trace_direction(
    field: &FieldExpr,
    seed: [f64; 3],
    dimension: usize,
    domain: Domain,
    step: f64,
    max_steps: usize,
    sign: f64,
) -> Vec<[f64; 3]> {
    let mut points = Vec::with_capacity(max_steps + 1);
    let mut position = seed;
    points.push(position);
    for _ in 0..max_steps {
        let Some(direction) =
            field.eval3_components_at_f64(DVec3::from_array(position), FROZEN_REFERENCE_TIME)
        else {
            break;
        };
        let norm = direction
            .iter()
            .map(|value| value * value)
            .sum::<f64>()
            .sqrt();
        if !norm.is_finite() || norm <= f64::EPSILON {
            break;
        }
        let mut next = position;
        for axis in 0..dimension.min(3) {
            next[axis] += sign * step * direction[axis] / norm;
        }
        for value in next.iter_mut().skip(dimension) {
            *value = 0.0;
        }
        if !next.iter().all(|value| value.is_finite()) || !inside_domain(next, domain) {
            break;
        }
        points.push(next);
        position = next;
    }
    points
}

fn inside_domain(point: [f64; 3], domain: Domain) -> bool {
    let size = domain.domain_size();
    let tolerance = (size.x.abs().max(size.y.abs()).max(size.z.abs()) as f64).max(1.0) * 1e-9;
    (0..3).all(|axis| {
        let value = point[axis];
        let min = domain.min[axis] as f64 - tolerance;
        let max = domain.max[axis] as f64 + tolerance;
        value >= min && value <= max
    })
}

/// Set the time-dependent reference in one call and return the previous
/// formula so the caller can offer it back. On failure the durable formula is
/// restored and the previous scene/solution stay visible.
pub fn apply_time_dependent_reference(state: &mut State) -> Result<String, OdeError> {
    apply_reference(state, reference_formula(), REFERENCE_DURATION)
}

/// Set the steady uniform reference in one call and return the previous
/// formula so the caller can offer it back.
pub fn apply_steady_reference(state: &mut State) -> Result<String, OdeError> {
    apply_reference(state, steady_formula(), REFERENCE_DURATION)
}

fn apply_reference(state: &mut State, formula: &str, duration: f64) -> Result<String, OdeError> {
    let previous = state.formula.clone();
    state.set_formula(formula.to_string());
    if !state.parse_error.is_empty() {
        let message = state.parse_error.clone();
        state.set_formula(previous.clone());
        return Err(OdeError::Parse {
            component: 0,
            message,
        });
    }
    // The reference always integrates the durable formula field at the origin.
    let dimension = state
        .field
        .dimension
        .clamp(ODE_MIN_DIMENSION, ODE_MAX_DIMENSION);
    let scene = OdeScene {
        source: OdeSource::CurrentField,
        dimension,
        equations: state.ode.equations.clone(),
        seeds: vec![OdeSeed {
            position: REFERENCE_SEED,
        }],
        duration,
    };
    match state.set_ode_scene(scene) {
        Ok(()) => Ok(previous),
        Err(error) => {
            state.set_formula(previous);
            Err(error)
        }
    }
}
