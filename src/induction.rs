//! Pure induction model for the teaching demo.
//!
//! The magnetic field is the existing softened source evaluator at unit
//! current; the induced electric field is reconstructed from the softened
//! axisymmetric vector potential, `E = -dA/dt`. The two families stay separate
//! at every seam.

use std::f64::consts::PI;

use glam::{DVec3, UVec3, Vec3};

use crate::domain::Domain;
use crate::plane::{PlaneAxis, PlaneFrame};
use crate::sampling::{ArrowData, ViewOptions, magnitude_range};
use crate::source_scene::SourceDisplay;
use crate::sources::{
    AccuracyRequest, Diagnostics, Estimate, EvalOptions, Evaluator, FamilyResult, KERNEL_ID,
    Source, SourceKind, SourceSet, StopReason, WorkLimits,
};

/// Shared-clock integration cap for the 3D induction grid.
pub const MAX_INDUCTION_DENSITY_3D: u32 = 20;
/// Shared-clock integration cap for the 2D induction slice.
///
/// Task 1 measured 120² at 2.7–4.1 s on the recorded host, so the frozen cap is
/// 80² (6,400 points), which measured under 2 s for both cases.
pub const MAX_INDUCTION_DENSITY_2D: u32 = 80;

/// The two supported axisymmetric conductors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum InductionCase {
    #[default]
    CurrentLoop,
    Solenoid,
}

/// One editable induction conductor and its sinusoidal current.
#[derive(Debug, Clone, PartialEq)]
pub struct InductionScene {
    pub case: InductionCase,
    pub center: DVec3,
    pub axis: DVec3,
    pub radius: f64,
    pub length: f64,
    pub turns: u32,
    pub amplitude: f64,
    pub frequency: f64,
    pub display: SourceDisplay,
    pub allow_inaccurate: bool,
}

impl Default for InductionScene {
    fn default() -> Self {
        Self {
            case: InductionCase::CurrentLoop,
            center: DVec3::ZERO,
            axis: DVec3::Z,
            radius: 1.0,
            length: 0.0,
            turns: 1,
            amplitude: 1.0,
            frequency: 0.25,
            display: SourceDisplay::Both,
            allow_inaccurate: false,
        }
    }
}

/// One induction family result with its estimate and target.
#[derive(Debug, Clone, PartialEq)]
pub enum InductionFamilyResult {
    TargetMet(InductionEstimate),
    AccuracyNotMet(InductionEstimate, StopReason),
    Unavailable(StopReason),
}

/// Finite induction estimate and the target it was checked against.
#[derive(Debug, Clone, PartialEq)]
pub struct InductionEstimate {
    pub value: DVec3,
    pub uncertainty: f64,
    pub target: f64,
}

/// Per-family counters for one induction sampling pass.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct InductionFamilyStatus {
    pub drawn: usize,
    pub target_met: usize,
    pub accuracy_not_met_total: usize,
    pub accuracy_not_met_drawn: usize,
    pub unavailable: usize,
}

/// One frame of induction arrows and provenance.
#[derive(Debug, Clone, PartialEq)]
pub struct InductionSampling {
    pub arrows: Vec<ArrowData>,
    pub electric: InductionFamilyStatus,
    pub magnetic: InductionFamilyStatus,
    pub current: f64,
    pub derivative: f64,
    pub derivative_source: Option<String>,
    pub softening: f64,
    pub validation_error: Option<String>,
}

/// Independent E and B results for one direct query point.
#[derive(Debug, Clone, PartialEq)]
pub struct InductionPointSample {
    pub electric: InductionFamilyResult,
    pub magnetic: FamilyResult,
}

/// Geometry key for the immutable unit-current cache.
#[derive(Debug, Clone, PartialEq)]
pub struct InductionKey {
    pub case: InductionCase,
    pub center: [f64; 3],
    pub axis: [f64; 3],
    pub radius: f64,
    pub length: f64,
    pub turns: u32,
    pub domain: (f32, f32, f32, f32, f32, f32),
    pub grid: (u32, u32, u32),
    pub plane: Option<(PlaneAxis, u32)>,
    pub softening: f64,
    pub accuracy: AccuracyRequest,
}

/// Immutable unit-current geometry and per-point kernel values.
#[derive(Debug)]
pub struct InductionPrepared {
    pub key: InductionKey,
    points: Vec<InductionUnitPoint>,
}

#[derive(Debug)]
struct InductionUnitPoint {
    position: Vec3,
    #[allow(dead_code)]
    world: DVec3,
    unit_e: DVec3,
    e_uncertainty: f64,
    b_value: DVec3,
    b_uncertainty: f64,
    b_stop: Option<StopReason>,
}

impl InductionScene {
    /// `I(t) = amplitude * cos(2 pi frequency t)`.
    pub fn current(&self, t: f32) -> f64 {
        let omega = std::f64::consts::TAU * self.frequency;
        self.amplitude * (omega * f64::from(t)).cos()
    }

    /// `dI/dt = -amplitude * 2 pi frequency * sin(2 pi frequency t)`.
    pub fn current_derivative(&self, t: f32) -> f64 {
        let omega = std::f64::consts::TAU * self.frequency;
        -self.amplitude * omega * (omega * f64::from(t)).sin()
    }

    /// The shared clock has work to do only for a non-zero amplitude and a
    /// strictly positive frequency.
    pub fn is_animated(&self) -> bool {
        self.amplitude != 0.0 && self.frequency > 0.0
    }

    /// Validate the editable scene before any physics runs.
    pub fn validate(&self) -> Result<(), String> {
        if !self.center.is_finite() {
            return Err("induction center must be finite".into());
        }
        if !self.axis.is_finite() {
            return Err("induction axis must be finite".into());
        }
        if (self.axis.length() - 1.0).abs() > 1e-9 {
            return Err("induction axis must be normalized".into());
        }
        if !self.radius.is_finite() || self.radius <= 0.0 {
            return Err("induction radius must be finite and positive".into());
        }
        match self.case {
            InductionCase::CurrentLoop => {}
            InductionCase::Solenoid => {
                if !self.length.is_finite() || self.length <= 0.0 {
                    return Err("induction solenoid length must be finite and positive".into());
                }
                if self.turns == 0 {
                    return Err("induction solenoid turns must be at least 1".into());
                }
            }
        }
        if !self.amplitude.is_finite() {
            return Err("induction amplitude must be finite".into());
        }
        if !self.frequency.is_finite() || self.frequency < 0.0 {
            return Err("induction frequency must be finite and non-negative".into());
        }
        Ok(())
    }

    /// The corresponding source at unit current, sharing the geometry exactly.
    pub fn unit_source(&self) -> Source {
        let kind = match self.case {
            InductionCase::CurrentLoop => SourceKind::CurrentLoop {
                current: 1.0,
                radius: self.radius,
            },
            InductionCase::Solenoid => SourceKind::Solenoid {
                current: 1.0,
                radius: self.radius,
                length: self.length,
                turns: self.turns,
            },
        };
        Source {
            center: self.center,
            axis: self.axis,
            kind,
        }
    }

    /// Unit-current azimuthal vector potential `A_phi(r, z)` at a world point.
    pub fn unit_vector_potential(&self, point: DVec3, softening: f64) -> Result<f64, String> {
        self.validate()?;
        validate_softening(softening)?;
        if !point.is_finite() {
            return Err("induction query point must be finite".into());
        }
        self.azimuthal_potential(point, softening)
            .map(|(fine, _)| fine)
    }

    fn azimuthal_potential(&self, point: DVec3, softening: f64) -> Result<(f64, f64), String> {
        let offset = point - self.center;
        let z = offset.dot(self.axis);
        let radial = offset - self.axis * z;
        let r = radial.length();
        if r == 0.0 {
            return Ok((0.0, 0.0));
        }
        if !r.is_finite() || !z.is_finite() {
            return Err("induction query point must be finite".into());
        }
        let fine = azimuthal_integral(self, r, z, softening, 8);
        let coarse = azimuthal_integral(self, r, z, softening, 4);
        if !fine.is_finite() || !coarse.is_finite() {
            return Err("induction vector potential is not finite".into());
        }
        let uncertainty = (fine - coarse).abs().max(32.0 * f64::EPSILON * fine.abs());
        if !uncertainty.is_finite() {
            return Err("induction vector-potential uncertainty is not finite".into());
        }
        Ok((fine, uncertainty))
    }

    fn azimuthal_basis(&self, point: DVec3) -> DVec3 {
        let offset = point - self.center;
        let z = offset.dot(self.axis);
        let radial = offset - self.axis * z;
        let r = radial.length();
        if r == 0.0 {
            DVec3::ZERO
        } else {
            self.axis.cross(radial / r)
        }
    }
}

fn validate_softening(softening: f64) -> Result<(), String> {
    if softening.is_finite() && softening > 0.0 {
        Ok(())
    } else {
        Err("induction softening must be finite and positive".into())
    }
}

/// Fixed 32-point Gauss-Legendre nodes and weights on `[-1, 1]`.
const GAUSS_LEGENDRE_32: [(f64, f64); 32] = [
    (-0.9972638618494816, 0.007018610009470506),
    (-0.9856115115452684, 0.016274394730905743),
    (-0.9647622555875064, 0.025392065309262024),
    (-0.9349060759377397, 0.034273862913021765),
    (-0.8963211557660521, 0.042835898022226836),
    (-0.84936761373257, 0.05099805926237609),
    (-0.7944837959679424, 0.058684093478535565),
    (-0.7321821187402897, 0.06582222277636168),
    (-0.6630442669302152, 0.07234579410884834),
    (-0.5877157572407623, 0.07819389578707023),
    (-0.5068999089322294, 0.08331192422694671),
    (-0.42135127613063533, 0.08765209300440378),
    (-0.33186860228212767, 0.09117387869576378),
    (-0.23928736225213706, 0.09384439908080451),
    (-0.1444719615827965, 0.09563872007927471),
    (-0.048307665687738324, 0.09654008851472766),
    (0.048307665687738324, 0.09654008851472766),
    (0.1444719615827965, 0.09563872007927471),
    (0.23928736225213706, 0.09384439908080451),
    (0.33186860228212767, 0.09117387869576378),
    (0.42135127613063533, 0.08765209300440378),
    (0.5068999089322294, 0.08331192422694671),
    (0.5877157572407623, 0.07819389578707023),
    (0.6630442669302152, 0.07234579410884834),
    (0.7321821187402897, 0.06582222277636168),
    (0.7944837959679424, 0.058684093478535565),
    (0.84936761373257, 0.05099805926237609),
    (0.8963211557660521, 0.042835898022226836),
    (0.9349060759377397, 0.034273862913021765),
    (0.9647622555875064, 0.025392065309262024),
    (0.9856115115452684, 0.016274394730905743),
    (0.9972638618494816, 0.007018610009470506),
];

/// Integrate the symmetric half-range `[0, pi]` and double it to `[0, 2pi]`.
fn azimuthal_integral(
    scene: &InductionScene,
    r: f64,
    z: f64,
    softening: f64,
    panels: usize,
) -> f64 {
    let radius = scene.radius;
    let a2 = softening * softening;
    let mut sum = 0.0;
    for panel in 0..panels {
        let lo = PI * panel as f64 / panels as f64;
        let hi = PI * (panel + 1) as f64 / panels as f64;
        let mid = 0.5 * (lo + hi);
        let half = 0.5 * (hi - lo);
        for &(node, weight) in &GAUSS_LEGENDRE_32 {
            let phi = mid + half * node;
            let cos_phi = phi.cos();
            let axial_term = match scene.case {
                InductionCase::CurrentLoop => z * z,
                InductionCase::Solenoid => 0.0,
            };
            let d = (r * r + radius * radius - 2.0 * r * radius * cos_phi + axial_term + a2).sqrt();
            if !d.is_finite() || d <= 0.0 {
                return f64::NAN;
            }
            let kernel = match scene.case {
                InductionCase::CurrentLoop => cos_phi / d,
                InductionCase::Solenoid => {
                    let half_length = 0.5 * scene.length;
                    cos_phi * (((z + half_length) / d).asinh() - ((z - half_length) / d).asinh())
                }
            };
            sum += half * weight * kernel;
        }
    }
    let doubled = 2.0 * sum;
    match scene.case {
        InductionCase::CurrentLoop => radius * doubled,
        InductionCase::Solenoid => (f64::from(scene.turns) / scene.length) * radius * doubled,
    }
}

/// Build the cache identity from every geometry/accuracy input.
pub fn induction_key(
    scene: &InductionScene,
    domain: &Domain,
    grid: UVec3,
    plane: Option<&PlaneFrame>,
    softening: f64,
    accuracy: AccuracyRequest,
) -> InductionKey {
    InductionKey {
        case: scene.case,
        center: scene.center.to_array(),
        axis: scene.axis.to_array(),
        radius: scene.radius,
        length: scene.length,
        turns: scene.turns,
        domain: (
            domain.min.x,
            domain.min.y,
            domain.min.z,
            domain.max.x,
            domain.max.y,
            domain.max.z,
        ),
        grid: (grid.x, grid.y, grid.z),
        plane: plane.map(|frame| (frame.axis, plane_index(frame, domain, grid))),
        softening,
        accuracy,
    }
}

fn plane_index(frame: &PlaneFrame, domain: &Domain, grid: UVec3) -> u32 {
    let n = frame.axis.count(grid);
    if n <= 1 {
        return 0;
    }
    let (lo, size) = match frame.axis {
        PlaneAxis::X => (domain.min.x, domain.max.x - domain.min.x),
        PlaneAxis::Y => (domain.min.y, domain.max.y - domain.min.y),
        PlaneAxis::Z => (domain.min.z, domain.max.z - domain.min.z),
    };
    let coordinate = match frame.axis {
        PlaneAxis::X => frame.origin.x,
        PlaneAxis::Y => frame.origin.y,
        PlaneAxis::Z => frame.origin.z,
    };
    if !size.is_finite() || size <= 0.0 {
        return 0;
    }
    let t = (coordinate - lo) / size;
    (t * (n - 1) as f32).round().clamp(0.0, (n - 1) as f32) as u32
}

/// Prepare the bounded unit-current cache for one geometry and grid.
pub fn prepare_induction(
    scene: &InductionScene,
    domain: &Domain,
    grid: UVec3,
    plane: Option<&PlaneFrame>,
    softening: f64,
    accuracy: AccuracyRequest,
) -> Result<InductionPrepared, String> {
    scene.validate()?;
    validate_softening(softening)?;
    let key = induction_key(scene, domain, grid, plane, softening, accuracy);
    let options = EvalOptions {
        softening,
        accuracy,
        ..EvalOptions::default()
    };
    let source = scene.unit_source();
    let set = SourceSet::new(&[source], &WorkLimits::default(), &|| false)
        .map_err(|error| error.to_string())?;
    let mut evaluator =
        Evaluator::new(&set, options, &|| false).map_err(|error| error.to_string())?;
    let mut points = Vec::new();

    let mut visit = |position: Vec3, world: Vec3, points: &mut Vec<InductionUnitPoint>| {
        let world = world.as_dvec3();
        let magnetic = evaluator.evaluate(world, &|| false).magnetic;
        let (b_value, b_uncertainty, b_stop) = match magnetic {
            FamilyResult::TargetMet(estimate) => (estimate.value, estimate.uncertainty, None),
            FamilyResult::AccuracyNotMet(estimate, reason) => {
                (estimate.value, estimate.uncertainty, Some(reason))
            }
            FamilyResult::Unavailable(reason) => (DVec3::NAN, f64::INFINITY, Some(reason)),
        };

        let (unit_e, e_uncertainty) = match scene.azimuthal_potential(world, softening) {
            Ok((potential, uncertainty)) => {
                let value = -potential * scene.azimuthal_basis(world);
                if value.is_finite() && uncertainty.is_finite() {
                    (value, uncertainty)
                } else {
                    (DVec3::NAN, f64::INFINITY)
                }
            }
            Err(_) => (DVec3::NAN, f64::INFINITY),
        };

        let (unit_e, b_value) = match plane {
            Some(frame) => (
                frame.project(unit_e.as_vec3()).as_dvec3(),
                frame.project(b_value.as_vec3()).as_dvec3(),
            ),
            None => (unit_e, b_value),
        };
        points.push(InductionUnitPoint {
            position,
            world,
            unit_e,
            e_uncertainty,
            b_value,
            b_uncertainty,
            b_stop,
        });
    };

    if let Some(frame) = plane {
        for local in frame
            .local_domain()
            .grid_points(UVec3::new(grid.x, grid.y, 1))
        {
            visit(local, frame.to_world(local), &mut points);
        }
    } else {
        for world in domain.grid_points(grid) {
            visit(world, world, &mut points);
        }
    }

    Ok(InductionPrepared { key, points })
}

/// Rescale the unit-current cache to the requested time and build one frame.
pub fn sample_induction(
    prepared: &InductionPrepared,
    scene: &InductionScene,
    t: f32,
    view: &ViewOptions,
) -> InductionSampling {
    let empty = |message: Option<String>| InductionSampling {
        arrows: Vec::new(),
        electric: InductionFamilyStatus::default(),
        magnetic: InductionFamilyStatus::default(),
        current: 0.0,
        derivative: 0.0,
        derivative_source: derivative_source(scene),
        softening: prepared.key.softening,
        validation_error: message,
    };
    if let Err(message) = scene.validate() {
        return empty(Some(message));
    }
    let current = scene.current(t);
    let derivative = scene.current_derivative(t);
    if !current.is_finite() || !derivative.is_finite() {
        return empty(Some("induction waveform is not finite at this time".into()));
    }

    let electric_displayed = matches!(scene.display, SourceDisplay::Electric | SourceDisplay::Both);
    let magnetic_displayed = matches!(scene.display, SourceDisplay::Magnetic | SourceDisplay::Both);
    let mut electric_status = InductionFamilyStatus::default();
    let mut magnetic_status = InductionFamilyStatus::default();
    let mut electric_points = Vec::new();
    let mut magnetic_points = Vec::new();
    let epsilon = prepared.key.accuracy.epsilon;

    for point in &prepared.points {
        let e_value = point.unit_e * derivative;
        let e_uncertainty = derivative.abs() * point.e_uncertainty;
        let e_target = epsilon * prepared.key.accuracy.e_scale + epsilon * e_value.length();
        let electric = classify_induction(e_value, e_uncertainty, e_target);
        observe_induction(
            &electric,
            scene.allow_inaccurate,
            electric_displayed,
            point.position,
            &mut electric_status,
            &mut electric_points,
        );

        let b_value = point.b_value * current;
        let b_uncertainty = current.abs() * point.b_uncertainty;
        let b_target = epsilon * prepared.key.accuracy.b_scale + epsilon * b_value.length();
        let magnetic = classify_magnetic(
            b_value,
            b_uncertainty,
            b_target,
            point.b_stop,
            prepared.key.softening,
            prepared.key.accuracy,
        );
        observe_magnetic(
            &magnetic,
            scene.allow_inaccurate,
            magnetic_displayed,
            point.position,
            &mut magnetic_status,
            &mut magnetic_points,
        );
    }

    let arrows = match scene.display {
        SourceDisplay::Both => {
            let mut arrows = Vec::with_capacity(electric_points.len() + magnetic_points.len());
            arrows.extend(
                electric_points
                    .into_iter()
                    .map(|(position, dir, mag)| ArrowData {
                        position,
                        dir,
                        mag,
                        color: [255, 176, 64],
                    }),
            );
            arrows.extend(
                magnetic_points
                    .into_iter()
                    .map(|(position, dir, mag)| ArrowData {
                        position,
                        dir,
                        mag,
                        color: [64, 196, 255],
                    }),
            );
            arrows
        }
        SourceDisplay::Electric => color_family(electric_points, view),
        SourceDisplay::Magnetic => color_family(magnetic_points, view),
    };

    InductionSampling {
        arrows,
        electric: electric_status,
        magnetic: magnetic_status,
        current,
        derivative,
        derivative_source: derivative_source(scene),
        softening: prepared.key.softening,
        validation_error: None,
    }
}

fn color_family(points: Vec<(Vec3, Vec3, f32)>, view: &ViewOptions) -> Vec<ArrowData> {
    let mut arrows: Vec<ArrowData> = points
        .into_iter()
        .map(|(position, dir, mag)| ArrowData {
            position,
            dir,
            mag,
            color: [128, 128, 128],
        })
        .collect();
    let (min, max) = view.color_range.unwrap_or_else(|| magnitude_range(&arrows));
    for arrow in &mut arrows {
        arrow.color = view.colormap.colormap(arrow.mag, min, max);
    }
    arrows
}

fn observe_induction(
    result: &InductionFamilyResult,
    allow_inaccurate: bool,
    displayed: bool,
    position: Vec3,
    status: &mut InductionFamilyStatus,
    points: &mut Vec<(Vec3, Vec3, f32)>,
) {
    match result {
        InductionFamilyResult::TargetMet(_) => status.target_met += 1,
        InductionFamilyResult::AccuracyNotMet(_, _) => status.accuracy_not_met_total += 1,
        InductionFamilyResult::Unavailable(_) => status.unavailable += 1,
    }
    if !displayed
        || (matches!(result, InductionFamilyResult::AccuracyNotMet(_, _)) && !allow_inaccurate)
    {
        return;
    }
    let estimate = match result {
        InductionFamilyResult::TargetMet(estimate)
        | InductionFamilyResult::AccuracyNotMet(estimate, _) => estimate,
        InductionFamilyResult::Unavailable(_) => return,
    };
    if push_arrow(position, estimate.value, points) {
        status.drawn += 1;
        if matches!(result, InductionFamilyResult::AccuracyNotMet(_, _)) {
            status.accuracy_not_met_drawn += 1;
        }
    }
}

fn observe_magnetic(
    result: &FamilyResult,
    allow_inaccurate: bool,
    displayed: bool,
    position: Vec3,
    status: &mut InductionFamilyStatus,
    points: &mut Vec<(Vec3, Vec3, f32)>,
) {
    match result {
        FamilyResult::TargetMet(_) => status.target_met += 1,
        FamilyResult::AccuracyNotMet(_, _) => status.accuracy_not_met_total += 1,
        FamilyResult::Unavailable(_) => status.unavailable += 1,
    }
    if !displayed || (matches!(result, FamilyResult::AccuracyNotMet(_, _)) && !allow_inaccurate) {
        return;
    }
    let estimate = match result {
        FamilyResult::TargetMet(estimate) | FamilyResult::AccuracyNotMet(estimate, _) => estimate,
        FamilyResult::Unavailable(_) => return,
    };
    if push_arrow(position, estimate.value, points) {
        status.drawn += 1;
        if matches!(result, FamilyResult::AccuracyNotMet(_, _)) {
            status.accuracy_not_met_drawn += 1;
        }
    }
}

fn push_arrow(position: Vec3, value: DVec3, points: &mut Vec<(Vec3, Vec3, f32)>) -> bool {
    if !value.is_finite() {
        return false;
    }
    let vector = value.as_vec3();
    if !vector.is_finite() {
        return false;
    }
    let magnitude = vector.length();
    if !magnitude.is_finite() || magnitude < 1e-6 {
        return false;
    }
    points.push((position, vector / magnitude, magnitude));
    true
}

fn classify_induction(value: DVec3, uncertainty: f64, target: f64) -> InductionFamilyResult {
    let magnitude = value.length();
    if !value.is_finite()
        || !magnitude.is_finite()
        || !uncertainty.is_finite()
        || uncertainty < 0.0
        || !target.is_finite()
        || target < 0.0
    {
        return InductionFamilyResult::Unavailable(StopReason::NumericRange);
    }
    let estimate = InductionEstimate {
        value,
        uncertainty,
        target,
    };
    if uncertainty <= target {
        InductionFamilyResult::TargetMet(estimate)
    } else {
        InductionFamilyResult::AccuracyNotMet(estimate, StopReason::RoundoffLimited)
    }
}

fn classify_magnetic(
    value: DVec3,
    uncertainty: f64,
    target: f64,
    stop: Option<StopReason>,
    softening: f64,
    accuracy: AccuracyRequest,
) -> FamilyResult {
    let magnitude = value.length();
    if !value.is_finite()
        || !magnitude.is_finite()
        || !uncertainty.is_finite()
        || uncertainty < 0.0
        || !target.is_finite()
        || target < 0.0
    {
        return FamilyResult::Unavailable(stop.unwrap_or(StopReason::NumericRange));
    }
    let roundoff_limited = stop == Some(StopReason::RoundoffLimited);
    let estimate = Estimate {
        value,
        uncertainty,
        target,
        absolute_contributions: 0.0,
        diagnostics: Diagnostics {
            kernel_id: KERNEL_ID,
            family: crate::sources::FieldFamily::Magnetic,
            softening,
            epsilon: accuracy.epsilon,
            family_scale: accuracy.b_scale,
            source_count: 1,
            work_units: 0,
            cells_high_water: 0,
            model_regularized: true,
            roundoff_limited,
            underflow: false,
        },
    };
    if uncertainty <= target {
        FamilyResult::TargetMet(estimate)
    } else {
        FamilyResult::AccuracyNotMet(estimate, stop.unwrap_or(StopReason::RoundoffLimited))
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn derivative_source(scene: &InductionScene) -> Option<String> {
    let source = format!(
        "{}*cos({}*t)",
        scene.amplitude,
        std::f64::consts::TAU * scene.frequency
    );
    crate::cas::differentiate(&source, crate::cas::Variable::T).ok()
}

#[cfg(target_arch = "wasm32")]
fn derivative_source(_scene: &InductionScene) -> Option<String> {
    None
}

/// Direct per-point evaluation for physics tests and diagnostics.
pub fn evaluate_induction_point(
    scene: &InductionScene,
    point: DVec3,
    t: f32,
    softening: f64,
) -> InductionPointSample {
    let unavailable = || InductionPointSample {
        electric: InductionFamilyResult::Unavailable(StopReason::InvalidQuery),
        magnetic: FamilyResult::Unavailable(StopReason::InvalidQuery),
    };
    if scene.validate().is_err() {
        return unavailable();
    }
    if !point.is_finite() {
        return unavailable();
    }
    if validate_softening(softening).is_err() {
        return InductionPointSample {
            electric: InductionFamilyResult::Unavailable(StopReason::NumericRange),
            magnetic: FamilyResult::Unavailable(StopReason::NumericRange),
        };
    }
    let accuracy = AccuracyRequest::default();
    let options = EvalOptions {
        softening,
        accuracy,
        ..EvalOptions::default()
    };
    let source = scene.unit_source();
    let set = match SourceSet::new(&[source], &WorkLimits::default(), &|| false) {
        Ok(set) => set,
        Err(_) => return unavailable(),
    };
    let mut evaluator = match Evaluator::new(&set, options, &|| false) {
        Ok(evaluator) => evaluator,
        Err(_) => return unavailable(),
    };
    let unit_magnetic = evaluator.evaluate(point, &|| false).magnetic;

    let current = scene.current(t);
    let derivative = scene.current_derivative(t);
    if !current.is_finite() || !derivative.is_finite() {
        return unavailable();
    }

    let magnetic = match unit_magnetic {
        FamilyResult::TargetMet(estimate) => {
            let value = estimate.value * current;
            let uncertainty = current.abs() * estimate.uncertainty;
            let target = accuracy.epsilon * accuracy.b_scale + accuracy.epsilon * value.length();
            classify_magnetic(value, uncertainty, target, None, softening, accuracy)
        }
        FamilyResult::AccuracyNotMet(estimate, reason) => {
            let value = estimate.value * current;
            let uncertainty = current.abs() * estimate.uncertainty;
            let target = accuracy.epsilon * accuracy.b_scale + accuracy.epsilon * value.length();
            classify_magnetic(
                value,
                uncertainty,
                target,
                Some(reason),
                softening,
                accuracy,
            )
        }
        FamilyResult::Unavailable(reason) => FamilyResult::Unavailable(reason),
    };

    let (potential, uncertainty) = match scene.azimuthal_potential(point, softening) {
        Ok(pair) => pair,
        Err(_) => {
            return InductionPointSample {
                electric: InductionFamilyResult::Unavailable(StopReason::NumericRange),
                magnetic,
            };
        }
    };
    let unit_e = -potential * scene.azimuthal_basis(point);
    let value = unit_e * derivative;
    let e_uncertainty = derivative.abs() * uncertainty;
    let target = accuracy.epsilon * accuracy.e_scale + accuracy.epsilon * value.length();
    let electric = classify_induction(value, e_uncertainty, target);
    InductionPointSample { electric, magnetic }
}
