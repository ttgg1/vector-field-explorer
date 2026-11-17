//! Bounded ODE workbench model: typed or current-field right-hand sides,
//! fixed-step RK4, deterministic samples, bounds and playback markers.
//!
//! This module is pure CPU data. It never samples a renderer type, a plane
//! frame, or the shared clock; [`crate::viewmodel::State`] only supplies the
//! durable formula field and domain for [`OdeSource::CurrentField`].

use glam::{DVec3, Vec3};

use crate::domain::Domain;
use crate::field::FieldExpr;
use crate::reference::{AXIS_X_COLOR, AXIS_Y_COLOR, AXIS_Z_COLOR};
use crate::viewmodel::State;

/// Fixed RK4 step used by the production solve path.
pub const ODE_STEP: f64 = 0.005;
/// Hard cap on stored samples per trajectory, including the `t = 0` sample.
pub const ODE_MAX_SAMPLES: usize = 4096;
/// Largest accepted scene duration.
pub const ODE_MAX_DURATION: f64 = 25.0;
/// Default scene duration.
pub const ODE_DEFAULT_DURATION: f64 = 20.0;
/// Largest accepted number of initial conditions.
pub const ODE_MAX_SEEDS: usize = 8;
/// Smallest accepted typed dimension.
pub const ODE_MIN_DIMENSION: usize = 2;
/// Largest accepted typed dimension.
pub const ODE_MAX_DIMENSION: usize = 3;

/// Fixed eight-colour trajectory palette; seed `i` uses `ODE_COLORS[i % 8]`.
pub const ODE_COLORS: [[f32; 4]; 8] = [
    [0.95, 0.35, 0.25, 1.0],
    [0.25, 0.75, 1.00, 1.0],
    [0.50, 0.90, 0.35, 1.0],
    [1.00, 0.75, 0.20, 1.0],
    [0.70, 0.45, 1.00, 1.0],
    [1.00, 0.45, 0.75, 1.0],
    [0.35, 0.90, 0.85, 1.0],
    [0.80, 0.80, 0.80, 1.0],
];

/// Trajectory line width, as a factor of the view-domain extent.
pub const ODE_LINE_WIDTH_FACTOR: f64 = 0.006;
/// Current-time marker radius, as a factor of the view-domain extent.
pub const ODE_MARKER_RADIUS_FACTOR: f64 = 0.012;

/// Where the integrated right-hand side comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum OdeSource {
    Equations,
    CurrentField,
}

/// One initial condition in world coordinates.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OdeSeed {
    pub position: [f64; 3],
}

/// Durable, editable ODE workbench scene.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OdeScene {
    pub source: OdeSource,
    pub dimension: usize,
    pub equations: [String; 3],
    pub seeds: Vec<OdeSeed>,
    pub duration: f64,
}

impl Default for OdeScene {
    fn default() -> Self {
        Self {
            source: OdeSource::Equations,
            dimension: 2,
            equations: ["y".into(), "-x".into(), "0".into()],
            seeds: vec![OdeSeed {
                position: [1.0, 0.0, 0.0],
            }],
            duration: ODE_DEFAULT_DURATION,
        }
    }
}

impl OdeScene {
    /// Rejects empty/too many/non-finite seeds, unsupported dimensions,
    /// invalid durations, and unparseable typed equations.
    ///
    /// `CurrentField` equations are ignored; the field dimension and parse
    /// error are checked by [`prepare_ode`].
    pub fn validate(&self) -> Result<(), OdeError> {
        if self.seeds.is_empty() {
            return Err(OdeError::EmptySeeds);
        }
        if self.seeds.len() > ODE_MAX_SEEDS {
            return Err(OdeError::InvalidScene(format!(
                "at most {ODE_MAX_SEEDS} initial conditions are supported, found {}",
                self.seeds.len()
            )));
        }
        for (index, seed) in self.seeds.iter().enumerate() {
            if !seed.position.iter().all(|value| value.is_finite()) {
                return Err(OdeError::NonFiniteSeed(index));
            }
        }
        if !(ODE_MIN_DIMENSION..=ODE_MAX_DIMENSION).contains(&self.dimension) {
            return Err(OdeError::InvalidScene(format!(
                "dimension must be {ODE_MIN_DIMENSION}..={ODE_MAX_DIMENSION}, found {}",
                self.dimension
            )));
        }
        if !self.duration.is_finite() || self.duration <= 0.0 {
            return Err(OdeError::InvalidScene(
                "duration must be finite and positive".into(),
            ));
        }
        if self.duration > ODE_MAX_DURATION {
            return Err(OdeError::InvalidScene(format!(
                "duration must be at most {ODE_MAX_DURATION}"
            )));
        }
        if self.source == OdeSource::Equations {
            parse_typed_rhs(self)?;
        }
        Ok(())
    }
}

/// Why a scene could not be prepared or solved.
#[derive(Debug, Clone, PartialEq)]
pub enum OdeError {
    InvalidScene(String),
    Parse { component: usize, message: String },
    EmptySeeds,
    NonFiniteSeed(usize),
    UnsupportedField(String),
}

impl std::fmt::Display for OdeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OdeError::InvalidScene(message) => write!(f, "invalid ODE scene: {message}"),
            OdeError::Parse { component, message } => {
                write!(f, "equation {component} failed to parse: {message}")
            }
            OdeError::EmptySeeds => write!(f, "at least one initial condition is required"),
            OdeError::NonFiniteSeed(index) => {
                write!(f, "initial condition {index} contains a non-finite value")
            }
            OdeError::UnsupportedField(message) => {
                write!(f, "current field is unavailable: {message}")
            }
        }
    }
}

impl std::error::Error for OdeError {}

/// A snapshot of everything integration needs; owns no borrows.
pub struct OdePrepared {
    dimension: usize,
    duration: f64,
    seeds: Vec<[f64; 3]>,
    field: FieldExpr,
    domain: Domain,
}

impl OdePrepared {
    /// The durable field domain the scene was prepared against.
    pub fn domain(&self) -> Domain {
        self.domain
    }
}

/// How one trajectory ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OdeStop {
    Complete,
    StepLimit,
    NonFinite,
}

/// One stored integration sample.
#[derive(Debug, Clone, PartialEq)]
pub struct OdeSample {
    pub time: f64,
    pub position: [f64; 3],
}

/// One seed's bounded sample list.
#[derive(Debug, Clone, PartialEq)]
pub struct OdeTrajectory {
    pub seed_index: usize,
    pub points: Vec<OdeSample>,
    pub stop: OdeStop,
}

/// Padded finite bounds around every stored sample.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OdeBounds {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

/// A precomputed, deterministic solution for every seed.
#[derive(Debug, Clone, PartialEq)]
pub struct OdeSolution {
    pub dimension: usize,
    pub step: f64,
    pub duration: f64,
    pub trajectories: Vec<OdeTrajectory>,
    pub bounds: OdeBounds,
    pub stopped: usize,
}

impl OdeSolution {
    /// Linear interpolation between bracketing samples at a playback time,
    /// clamped to `[0, duration]`. Stops past a `StepLimit` or `NonFinite`
    /// trajectory clamp to their last stored sample.
    pub fn sample_at(&self, trajectory: usize, time: f64) -> Option<OdeSample> {
        let trajectory = self.trajectories.get(trajectory)?;
        if !time.is_finite() {
            return None;
        }
        let first = trajectory.points.first()?;
        let t = time.clamp(0.0, self.duration);
        if t <= first.time {
            return Some(OdeSample {
                time: t,
                position: first.position,
            });
        }
        let last = trajectory.points.last()?;
        if t >= last.time {
            return Some(OdeSample {
                time: t,
                position: last.position,
            });
        }
        let right = trajectory.points.partition_point(|sample| sample.time <= t);
        let a = &trajectory.points[right - 1];
        let b = &trajectory.points[right];
        let span = b.time - a.time;
        let ratio = if span > 0.0 { (t - a.time) / span } else { 0.0 };
        let mut position = [0.0; 3];
        for ((out, start), end) in position
            .iter_mut()
            .zip(a.position.iter())
            .zip(b.position.iter())
        {
            *out = start + (end - start) * ratio;
        }
        Some(OdeSample { time: t, position })
    }

    /// One interpolated position per trajectory at the clamped playback time.
    pub fn markers_at(&self, time: f64) -> Vec<[f64; 3]> {
        (0..self.trajectories.len())
            .filter_map(|index| self.sample_at(index, time).map(|sample| sample.position))
            .collect()
    }

    /// Padded trajectory bounds as a camera domain. A flat/2D `z` gets a
    /// symmetric extent so 3D cameras always have a usable box.
    pub fn view_domain(&self) -> Domain {
        let mut min = self.bounds.min;
        let mut max = self.bounds.max;
        let z_extent = raw_z_extent(&self.trajectories);
        if self.dimension < 3 || z_extent <= f64::EPSILON {
            let half = (0.05 * (max[0] - min[0]).max(max[1] - min[1])).max(1e-3);
            let centre = (min[2] + max[2]) * 0.5;
            min[2] = centre - half;
            max[2] = centre + half;
        }
        Domain::new(
            Vec3::new(min[0] as f32, min[1] as f32, min[2] as f32),
            Vec3::new(max[0] as f32, max[1] as f32, max[2] as f32),
        )
    }
}

/// Chart selection for the ODE panel; default is the phase portrait.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OdeChart {
    #[default]
    Phase,
    TimeSeries,
}

/// Line instances as `(start, end, colour, seed_index)` tuples. Task 2
/// converts them to the renderer's line type.
pub type OdeLine = ([f32; 3], [f32; 3], [f32; 4], usize);

/// Axis instances as `(start, end, colour)` tuples.
pub type OdeAxisLine = ([f32; 3], [f32; 3], [f32; 4]);

/// Builds the trajectory polyline data for the renderer seam. Non-finite
/// points never reach a line, and a trajectory needs at least two points.
pub fn ode_trajectory_lines(solution: &OdeSolution) -> Vec<OdeLine> {
    let mut lines = Vec::new();
    for trajectory in &solution.trajectories {
        let color = ODE_COLORS[trajectory.seed_index % ODE_COLORS.len()];
        let finite: Vec<[f64; 3]> = trajectory
            .points
            .iter()
            .map(|sample| sample.position)
            .filter(|position| position.iter().all(|value| value.is_finite()))
            .collect();
        for pair in finite.windows(2) {
            let start: [f32; 3] = std::array::from_fn(|axis| pair[0][axis] as f32);
            let end: [f32; 3] = std::array::from_fn(|axis| pair[1][axis] as f32);
            if start.iter().all(|value| value.is_finite())
                && end.iter().all(|value| value.is_finite())
            {
                lines.push((start, end, color, trajectory.seed_index));
            }
        }
    }
    lines
}

/// Axis line data from a padded view domain. 2D scenes get one line per
/// visible axis; 3D scenes add `z`. Every line runs from `-extent` to
/// `+extent` through the origin, where the half-extent is the larger absolute
/// bound on that axis. That symmetric-max geometry is intentional: the axes
/// must pass through the origin (an asymmetric padded trajectory domain need
/// not be centred on it), so on an off-centre domain one end may extend past
/// the padded bound. Task 1 review item 1 confirmed this is the intended
/// drawing behaviour, not a fit bug.
pub fn ode_axes(view_domain: Domain, dimension: usize) -> Vec<OdeAxisLine> {
    let min = view_domain.min;
    let max = view_domain.max;
    let mut axes = Vec::with_capacity(if dimension >= 3 { 3 } else { 2 });
    let push = |axis: usize, color: [f32; 4], axes: &mut Vec<OdeAxisLine>| {
        let half = min[axis].abs().max(max[axis].abs()).max(1e-3);
        let mut start = [0.0f32; 3];
        let mut end = [0.0f32; 3];
        start[axis] = -half;
        end[axis] = half;
        axes.push((start, end, color));
    };
    push(0, AXIS_X_COLOR, &mut axes);
    push(1, AXIS_Y_COLOR, &mut axes);
    if dimension >= 3 {
        push(2, AXIS_Z_COLOR, &mut axes);
    }
    axes
}

/// Validates and snapshots the scene and its right-hand side.
pub fn prepare_ode(state: &State, scene: &OdeScene) -> Result<OdePrepared, OdeError> {
    scene.validate()?;
    let field = match scene.source {
        OdeSource::Equations => parse_typed_rhs(scene)?,
        OdeSource::CurrentField => {
            if !state.parse_error.is_empty() {
                return Err(OdeError::UnsupportedField(state.parse_error.clone()));
            }
            if state.field.dimension != scene.dimension {
                return Err(OdeError::InvalidScene(format!(
                    "current field has dimension {}, scene requests {}",
                    state.field.dimension, scene.dimension
                )));
            }
            state.field.clone()
        }
    };
    Ok(OdePrepared {
        dimension: scene.dimension,
        duration: scene.duration,
        seeds: scene
            .seeds
            .iter()
            .map(|seed| {
                let mut position = seed.position;
                for value in position.iter_mut().skip(scene.dimension) {
                    *value = 0.0;
                }
                position
            })
            .collect(),
        field,
        domain: state.domain,
    })
}

/// Integrates a prepared scene with the production step and sample caps.
pub fn integrate_ode(prepared: &OdePrepared) -> OdeSolution {
    integrate_ode_with_step(prepared, ODE_STEP, ODE_MAX_SAMPLES)
}

/// Validates, prepares and solves a scene.
pub fn solve_ode(state: &State, scene: &OdeScene) -> Result<OdeSolution, OdeError> {
    let prepared = prepare_ode(state, scene)?;
    let solution = integrate_ode(&prepared);
    if !solution.trajectories.iter().any(|trajectory| {
        trajectory
            .points
            .iter()
            .any(|sample| sample.position.iter().all(|value| value.is_finite()))
    }) {
        return Err(OdeError::InvalidScene(
            "no finite trajectory samples".into(),
        ));
    }
    Ok(solution)
}

/// Fixed-step RK4 evidence seam. Production always uses [`ODE_STEP`] and
/// [`ODE_MAX_SAMPLES`]; tests call this with `h` and `h / 2`.
#[doc(hidden)]
pub fn integrate_ode_with_step(prepared: &OdePrepared, step: f64, max_steps: usize) -> OdeSolution {
    let step = if step.is_finite() && step > 0.0 {
        step
    } else {
        ODE_STEP
    };
    let max_steps = max_steps.max(1);
    let mut trajectories = Vec::with_capacity(prepared.seeds.len());
    for (seed_index, seed) in prepared.seeds.iter().enumerate() {
        let mut points = Vec::new();
        let mut position = *seed;
        points.push(OdeSample {
            time: 0.0,
            position,
        });
        let mut stop = OdeStop::Complete;
        let mut n = 1usize;
        loop {
            let time = n as f64 * step;
            if time > prepared.duration + 1e-12 {
                break;
            }
            if points.len() >= max_steps {
                stop = OdeStop::StepLimit;
                break;
            }
            let previous_time = (n - 1) as f64 * step;
            match rk4_step(
                &prepared.field,
                prepared.dimension,
                &position,
                previous_time,
                step,
            ) {
                Some(next) => {
                    position = next;
                    points.push(OdeSample { time, position });
                    n += 1;
                }
                None => {
                    stop = OdeStop::NonFinite;
                    break;
                }
            }
        }
        trajectories.push(OdeTrajectory {
            seed_index,
            points,
            stop,
        });
    }
    let stopped = trajectories
        .iter()
        .filter(|trajectory| trajectory.stop != OdeStop::Complete)
        .count();
    let bounds = compute_bounds(&trajectories).unwrap_or(OdeBounds {
        min: [-1e-3; 3],
        max: [1e-3; 3],
    });
    OdeSolution {
        dimension: prepared.dimension,
        step,
        duration: prepared.duration,
        trajectories,
        bounds,
        stopped,
    }
}

/// One fixed RK4 step. `None` when any stage or the result is non-finite.
fn rk4_step(
    field: &FieldExpr,
    dimension: usize,
    position: &[f64; 3],
    time: f64,
    step: f64,
) -> Option<[f64; 3]> {
    let k1 = rhs(field, position, time)?;
    let k2 = rhs(field, &offset(position, &k1, step * 0.5), time + step * 0.5)?;
    let k3 = rhs(field, &offset(position, &k2, step * 0.5), time + step * 0.5)?;
    let k4 = rhs(field, &offset(position, &k3, step), time + step)?;
    let mut next = std::array::from_fn(|axis| {
        position[axis] + step / 6.0 * (k1[axis] + 2.0 * k2[axis] + 2.0 * k3[axis] + k4[axis])
    });
    for value in next.iter_mut().skip(dimension) {
        *value = 0.0;
    }
    if next.iter().all(|value| value.is_finite()) {
        Some(next)
    } else {
        None
    }
}

fn rhs(field: &FieldExpr, position: &[f64; 3], time: f64) -> Option<[f64; 3]> {
    field.eval3_components_at_f64(DVec3::from_array(*position), time)
}

fn offset(position: &[f64; 3], derivative: &[f64; 3], scale: f64) -> [f64; 3] {
    [
        position[0] + derivative[0] * scale,
        position[1] + derivative[1] * scale,
        position[2] + derivative[2] * scale,
    ]
}

/// Parses the first `dimension` typed equations with `"0"` in unused slots.
/// Entries outside `dimension` are ignored.
fn parse_typed_rhs(scene: &OdeScene) -> Result<FieldExpr, OdeError> {
    let mut parts: Vec<&str> = scene.equations[..scene.dimension]
        .iter()
        .map(String::as_str)
        .collect();
    while parts.len() < 3 {
        parts.push("0");
    }
    let source = parts.join(", ");
    FieldExpr::parse(&source).map_err(|error| OdeError::Parse {
        component: first_bad_component(scene).unwrap_or(0),
        message: error.to_string(),
    })
}

fn first_bad_component(scene: &OdeScene) -> Option<usize> {
    for index in 0..scene.dimension {
        let components = ["0", "0", "0"];
        let mut probe = components;
        probe[index] = &scene.equations[index];
        if FieldExpr::parse(&probe.join(", ")).is_err() {
            return Some(index);
        }
    }
    None
}

fn compute_bounds(trajectories: &[OdeTrajectory]) -> Option<OdeBounds> {
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    let mut any = false;
    for sample in trajectories
        .iter()
        .flat_map(|trajectory| &trajectory.points)
    {
        if !sample.position.iter().all(|value| value.is_finite()) {
            continue;
        }
        any = true;
        for ((lo, hi), value) in min
            .iter_mut()
            .zip(max.iter_mut())
            .zip(sample.position.iter())
        {
            *lo = lo.min(*value);
            *hi = hi.max(*value);
        }
    }
    if !any {
        return None;
    }
    let mut bounds = OdeBounds { min, max };
    let largest_extent = (0..3)
        .map(|axis| bounds.max[axis] - bounds.min[axis])
        .fold(0.0_f64, f64::max);
    let pad = (0.05 * largest_extent).max(1e-3);
    for (lo, hi) in bounds.min.iter_mut().zip(bounds.max.iter_mut()) {
        *lo -= pad;
        *hi += pad;
    }
    Some(bounds)
}

fn raw_z_extent(trajectories: &[OdeTrajectory]) -> f64 {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for sample in trajectories
        .iter()
        .flat_map(|trajectory| &trajectory.points)
    {
        if sample.position.iter().all(|value| value.is_finite()) {
            min = min.min(sample.position[2]);
            max = max.max(sample.position[2]);
        }
    }
    if min.is_finite() && max.is_finite() {
        max - min
    } else {
        0.0
    }
}
