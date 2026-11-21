//! Bounded custom density model and diagnostics (issue #96 slice A).
//!
//! A [`DensitySpec`] declares a named support, a formula and a sampling lattice.
//! Formulas use the shared field grammar: `x`, `y`, `z`, `t` and the committed
//! [`ParameterSet`] names. A volume or surface charge is one scalar
//! `rho(x,y,z,t)`; a steady current is three comma-separated components
//! `Jx,Jy,Jz`.
//!
//! Density outside a support is identically zero. Nothing in this module clamps
//! or invents a value: a formula that is non-finite inside the support is
//! rejected by [`DensitySpec::validate`], and a report that cannot evaluate its
//! interior returns [`DiagnosticsStatus::Unresolved`] with non-finite readouts
//! rather than a zero.
//!
//! The model is in normalized teaching units (`eps0 = mu0 = 1`); it makes no SI
//! claim and never combines an electric result with a magnetic one.
//!
//! Geometry: `size` holds full extents. A sphere uses `size.x` as its diameter,
//! a box uses all three extents, and a cylinder uses `size.x` as the length along
//! `axis` and `size.y` as its diameter. `axis` is normalized internally and is
//! required to be non-degenerate only for a cylinder. All supports are closed
//! (the boundary is included).
//!
//! Sampling is a deterministic jittered lattice of `samples^3` candidate cell
//! centres over the support's axis-aligned bounding box. Each point is displaced
//! by at most `0.45` cell, keyed on the support geometry and lattice index with
//! the same splitmix64 scheme the streamline seeds use. Points outside the
//! support are excluded, never zeroed. The lattice budget is
//! [`MAX_DENSITY_SAMPLES`].

use glam::{DVec3, Vec3};

use crate::field::FieldExpr;
use crate::parameters::ParameterSet;

/// Largest accepted lattice: exactly `48^3` candidate points.
pub const MAX_DENSITY_SAMPLES: u32 = 110_592;
/// Smallest lattice side.
pub const MIN_SAMPLES: u32 = 8;
/// Largest lattice side.
pub const MAX_SAMPLES: u32 = 48;
/// Longest accepted name, in UTF-8 bytes.
pub const MAX_NAME_BYTES: usize = 48;

/// Largest allowed `|div J|` relative to the largest sampled `|J|`.
///
/// The units are normalized (`eps0 = mu0 = 1`); a current whose divergence is
/// within this fraction of its own scale is reported as an estimate near zero,
/// and a larger one is a violation.
pub const CONTINUITY_TOLERANCE: f64 = 0.05;

/// Floor for the divergence normalization scale, so an identically zero
/// current is still compared against a finite, meaningful scale.
const CONTINUITY_MIN_SCALE: f64 = 1e-12;
/// Per-axis displacement as a fraction of one lattice cell. Below half a cell so
/// neighbours cannot swap.
const JITTER: f64 = 0.45;
/// A cylinder axis shorter than this is degenerate.
const MIN_AXIS_LENGTH_SQUARED: f64 = 1e-24;
/// Largest finite `f32` magnitude. `worst_point` is a `Vec3`, so a support whose
/// centre plus any half-extent exceeds this limit could round a finite sample
/// position to infinity.
const MAX_REPORT_COORDINATE: f64 = f32::MAX as f64;

/// The shape of a bounded density support.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportKind {
    Sphere,
    Box,
    Cylinder,
}

/// A closed support with a centre, an axis and full extents.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DensitySupport {
    pub kind: SupportKind,
    pub center: Vec3,
    /// Cylinder axis; ignored by a sphere or box.
    pub axis: Vec3,
    /// Full extents (see the module geometry contract).
    pub size: Vec3,
}

/// What a density formula describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DensityFamily {
    VolumeCharge,
    SurfaceCharge,
    SteadyCurrent,
}

/// One declared density over a bounded support.
#[derive(Debug, Clone, PartialEq)]
pub struct DensitySpec {
    /// Trimmed, at most [`MAX_NAME_BYTES`], unique in the caller's scene.
    pub name: String,
    pub family: DensityFamily,
    pub support: DensitySupport,
    /// Scalar `rho` for the charge families, `Jx,Jy,Jz` for a steady current.
    pub formula: String,
    /// Lattice side, `MIN_SAMPLES..=MAX_SAMPLES`.
    pub samples: u32,
    /// Normalized softening in `0..=1`.
    pub softening: f64,
}

/// Why a diagnostic is or is not a usable estimate.
#[derive(Debug, Clone, PartialEq)]
pub enum DiagnosticsStatus {
    /// The full and half passes completed; `error` is their difference.
    Estimate { error: f64 },
    /// The interior could not be sampled finitely, or no diagnostic applies.
    Unresolved { reason: String },
    /// A steady-current divergence exceeded [`CONTINUITY_TOLERANCE`].
    Violation { reason: String },
}

/// Volume-charge diagnostics over the sampled interior.
#[derive(Debug, Clone, PartialEq)]
pub struct ChargeReport {
    pub total: f64,
    pub min: f64,
    pub max: f64,
    pub sign_changes: u32,
    /// Accepted interior samples in the full pass.
    pub samples: u32,
    pub status: DiagnosticsStatus,
}

/// Steady-current continuity diagnostics over the sampled interior.
#[derive(Debug, Clone, PartialEq)]
pub struct ContinuityReport {
    pub div_j_min: f64,
    pub div_j_max: f64,
    pub worst_point: Option<Vec3>,
    pub status: DiagnosticsStatus,
}

impl DensitySupport {
    /// Validate the frozen geometry without sampling any formula.
    pub fn validate(&self) -> Result<(), String> {
        if !self.center.is_finite() {
            return Err("support centre must be finite".to_string());
        }
        if !self.axis.is_finite() {
            return Err("support axis must be finite".to_string());
        }
        if !self.size.is_finite() {
            return Err("support size components must be finite".to_string());
        }
        if self.size.x <= 0.0 || self.size.y <= 0.0 || self.size.z <= 0.0 {
            return Err(format!(
                "support size components must be positive; found ({}, {}, {})",
                self.size.x, self.size.y, self.size.z
            ));
        }
        if self.kind == SupportKind::Cylinder
            && self.axis.as_dvec3().length_squared() <= MIN_AXIS_LENGTH_SQUARED
        {
            return Err("cylinder support axis must be non-degenerate".to_string());
        }
        let half = self
            .half_extents()
            .ok_or_else(|| "support has no finite half-extents".to_string())?;
        let reach = self.center.as_dvec3().abs() + half;
        if !reach.is_finite()
            || reach.x > MAX_REPORT_COORDINATE
            || reach.y > MAX_REPORT_COORDINATE
            || reach.z > MAX_REPORT_COORDINATE
        {
            return Err(format!(
                "support centre plus half-extents must stay within the finite f32 range ({MAX_REPORT_COORDINATE:e}); found reach ({}, {}, {})",
                reach.x, reach.y, reach.z
            ));
        }
        Ok(())
    }

    /// Whether `point` is finite and inside the closed support.
    pub fn contains(&self, point: Vec3) -> bool {
        if !point.is_finite() {
            return false;
        }
        self.contains_dvec(point.as_dvec3())
    }

    /// The analytic volume of the support, or `0.0` for an invalid geometry.
    pub fn volume(&self) -> f64 {
        let Some(half) = self.half_extents() else {
            return 0.0;
        };
        let volume = match self.kind {
            SupportKind::Sphere => 4.0 / 3.0 * std::f64::consts::PI * half.x * half.x * half.x,
            SupportKind::Box => 8.0 * half.x * half.y * half.z,
            SupportKind::Cylinder => std::f64::consts::PI * half.y * half.y * 2.0 * half.x,
        };
        if volume.is_finite() { volume } else { 0.0 }
    }

    fn contains_dvec(&self, point: DVec3) -> bool {
        if !point.is_finite() {
            return false;
        }
        let Some(half) = self.half_extents() else {
            return false;
        };
        let offset = point - self.center.as_dvec3();
        match self.kind {
            SupportKind::Sphere => offset.length_squared() <= half.x * half.x,
            SupportKind::Box => {
                offset.x.abs() <= half.x && offset.y.abs() <= half.y && offset.z.abs() <= half.z
            }
            SupportKind::Cylinder => {
                let Some(axis) = self.normalized_axis() else {
                    return false;
                };
                let along = offset.dot(axis);
                if along.abs() > half.x {
                    return false;
                }
                let perpendicular = (offset.length_squared() - along * along).max(0.0);
                perpendicular <= half.y * half.y
            }
        }
    }

    /// Full extents split in half, or `None` for a degenerate geometry.
    fn half_extents(&self) -> Option<DVec3> {
        if !self.size.is_finite() || self.size.x <= 0.0 || self.size.y <= 0.0 || self.size.z <= 0.0
        {
            return None;
        }
        let half = self.size.as_dvec3() * 0.5;
        match self.kind {
            SupportKind::Sphere => Some(DVec3::splat(half.x)),
            SupportKind::Box => Some(half),
            SupportKind::Cylinder => {
                if half.y <= 0.0 {
                    return None;
                }
                let axis = self.normalized_axis()?;
                let x = axis.x.abs();
                let y = axis.y.abs();
                let z = axis.z.abs();
                Some(DVec3::new(
                    half.x * x + half.y * (1.0 - x * x).max(0.0).sqrt(),
                    half.x * y + half.y * (1.0 - y * y).max(0.0).sqrt(),
                    half.x * z + half.y * (1.0 - z * z).max(0.0).sqrt(),
                ))
            }
        }
    }

    fn normalized_axis(&self) -> Option<DVec3> {
        if !self.axis.is_finite() {
            return None;
        }
        let axis = self.axis.as_dvec3();
        let length_squared = axis.length_squared();
        if length_squared <= MIN_AXIS_LENGTH_SQUARED {
            None
        } else {
            Some(axis / length_squared.sqrt())
        }
    }

    fn bounds(&self) -> Option<(DVec3, DVec3)> {
        let half = self.half_extents()?;
        let center = self.center.as_dvec3();
        Some((center - half, center + half))
    }
}

impl DensitySpec {
    /// Validate the name, support geometry, softening, sample budget and formula.
    ///
    /// The formula must parse under the shared grammar and evaluate finitely on
    /// every accepted interior lattice point at `t = 0`. A non-finite point is
    /// rejected with its position; it is never clamped or replaced by zero.
    pub fn validate(&self, parameters: &ParameterSet) -> Result<(), String> {
        self.validate_name()?;
        self.support.validate()?;
        if !self.softening.is_finite() || !(0.0..=1.0).contains(&self.softening) {
            return Err(format!(
                "softening must be finite and in 0..=1; found {}",
                self.softening
            ));
        }
        if self.samples < MIN_SAMPLES {
            return Err(format!(
                "lattice side {} is below MIN_SAMPLES ({MIN_SAMPLES})",
                self.samples
            ));
        }
        if samples_cubed(self.samples).is_none_or(|count| count > u64::from(MAX_DENSITY_SAMPLES)) {
            return Err(format!(
                "lattice side {} would sample {}^3 points, exceeding MAX_DENSITY_SAMPLES ({MAX_DENSITY_SAMPLES})",
                self.samples, self.samples
            ));
        }
        if self.samples > MAX_SAMPLES {
            return Err(format!(
                "lattice side {} exceeds MAX_SAMPLES ({MAX_SAMPLES})",
                self.samples
            ));
        }

        let expression = self.parse_expression(parameters)?;
        let salt = support_salt(&self.support);
        match self.family {
            DensityFamily::SteadyCurrent => {
                validate_interior(&self.support, self.samples, salt, |point| {
                    eval_current(&expression, point, 0.0).map(|_| ())
                })
            }
            DensityFamily::VolumeCharge | DensityFamily::SurfaceCharge => {
                validate_interior(&self.support, self.samples, salt, |point| {
                    eval_scalar(&expression, point, 0.0).map(|_| ())
                })
            }
        }
    }

    fn validate_name(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("density name must not be empty".to_string());
        }
        if self.name.trim() != self.name {
            return Err(format!("density name \"{}\" must be trimmed", self.name));
        }
        if self.name.len() > MAX_NAME_BYTES {
            return Err(format!(
                "density name is {} bytes, exceeding MAX_NAME_BYTES ({MAX_NAME_BYTES})",
                self.name.len()
            ));
        }
        if self.name.chars().any(char::is_control) {
            return Err(format!(
                "density name \"{}\" must not contain control characters",
                self.name
            ));
        }
        Ok(())
    }

    fn parse_expression(&self, parameters: &ParameterSet) -> Result<FieldExpr, String> {
        let formula = self.formula.trim();
        if formula.is_empty() {
            return Err("density formula must not be empty".to_string());
        }
        match self.family {
            DensityFamily::SteadyCurrent => {
                let expression = FieldExpr::parse_with_parameters(formula, parameters)
                    .map_err(|error| format!("density formula: {error}"))?;
                if expression.dimension != 3 {
                    return Err(
                        "steady current formula must have three comma-separated components Jx,Jy,Jz"
                            .to_string(),
                    );
                }
                Ok(expression)
            }
            DensityFamily::VolumeCharge | DensityFamily::SurfaceCharge => {
                // The shared field grammar parses vector components; wrapping
                // the scalar keeps every substitution, token and finite check on
                // the same path.
                let source = format!("(0,({formula}),0)");
                FieldExpr::parse_with_parameters(&source, parameters)
                    .map_err(|error| format!("density formula: {error}"))
            }
        }
    }
}

/// Midpoint-weighted volume charge total and interior diagnostics.
///
/// Only [`DensityFamily::VolumeCharge`] has a volume total. Any other family is
/// `Unresolved`, with non-finite readouts rather than a fabricated zero.
pub fn charge_report(
    spec: &DensitySpec,
    parameters: &ParameterSet,
    time: f64,
) -> Result<ChargeReport, String> {
    spec.validate(parameters)?;
    if spec.family != DensityFamily::VolumeCharge {
        return Ok(unresolved_charge(format!(
            "a {:?} density has no volume-charge total",
            spec.family
        )));
    }

    let expression = spec.parse_expression(parameters)?;
    let salt = support_salt(&spec.support);
    let full = match charge_pass(spec, &expression, spec.samples, time, salt) {
        Ok(pass) => pass,
        Err(reason) => return Ok(unresolved_charge(reason)),
    };
    let half_samples = (spec.samples / 2).max(MIN_SAMPLES / 2);
    let half = match charge_pass(spec, &expression, half_samples, time, salt) {
        Ok(pass) => pass,
        Err(reason) => {
            return Ok(ChargeReport {
                total: full.total,
                min: full.min,
                max: full.max,
                sign_changes: full.sign_changes,
                samples: full.samples,
                status: DiagnosticsStatus::Unresolved { reason },
            });
        }
    };

    let error = (full.total - half.total).abs();
    let status = if error.is_finite() {
        DiagnosticsStatus::Estimate { error }
    } else {
        DiagnosticsStatus::Unresolved {
            reason: "the full and half charge totals were not finite".to_string(),
        }
    };
    Ok(ChargeReport {
        total: full.total,
        min: full.min,
        max: full.max,
        sign_changes: full.sign_changes,
        samples: full.samples,
        status,
    })
}

/// Central-difference divergence bounded by [`CONTINUITY_TOLERANCE`].
///
/// Only a [`DensityFamily::SteadyCurrent`] is accepted; any other family is an
/// error. A current whose interior cannot be sampled finitely is `Unresolved`.
pub fn continuity_report(
    spec: &DensitySpec,
    parameters: &ParameterSet,
    time: f64,
) -> Result<ContinuityReport, String> {
    if spec.family != DensityFamily::SteadyCurrent {
        return Err("continuity applies to steady currents only".to_string());
    }
    spec.validate(parameters)?;

    let expression = spec.parse_expression(parameters)?;
    let salt = support_salt(&spec.support);
    let full = match divergence_pass(spec, &expression, spec.samples, time, salt) {
        Ok(pass) => pass,
        Err(reason) => return Ok(unresolved_continuity(reason)),
    };
    let scale = full.max_j.max(CONTINUITY_MIN_SCALE);
    let threshold = CONTINUITY_TOLERANCE * scale;
    let status = if full.max_abs > threshold {
        DiagnosticsStatus::Violation {
            reason: format!(
                "max |div J| {:.6e} exceeds the normalized tolerance {:.6e}",
                full.max_abs, threshold
            ),
        }
    } else {
        let half_samples = (spec.samples / 2).max(MIN_SAMPLES / 2);
        match divergence_pass(spec, &expression, half_samples, time, salt) {
            Ok(half) => {
                let error = (full.max_abs - half.max_abs).abs();
                if error.is_finite() {
                    DiagnosticsStatus::Estimate { error }
                } else {
                    DiagnosticsStatus::Unresolved {
                        reason: "the full and half divergence estimates were not finite"
                            .to_string(),
                    }
                }
            }
            Err(reason) => DiagnosticsStatus::Unresolved { reason },
        }
    };
    Ok(ContinuityReport {
        div_j_min: full.min,
        div_j_max: full.max,
        worst_point: Some(full.worst.as_vec3()),
        status,
    })
}

fn unresolved_charge(reason: String) -> ChargeReport {
    ChargeReport {
        total: f64::NAN,
        min: f64::NAN,
        max: f64::NAN,
        sign_changes: 0,
        samples: 0,
        status: DiagnosticsStatus::Unresolved { reason },
    }
}

fn unresolved_continuity(reason: String) -> ContinuityReport {
    ContinuityReport {
        div_j_min: f64::NAN,
        div_j_max: f64::NAN,
        worst_point: None,
        status: DiagnosticsStatus::Unresolved { reason },
    }
}

fn samples_cubed(samples: u32) -> Option<u64> {
    let side = u64::from(samples);
    side.checked_mul(side)?.checked_mul(side)
}

fn validate_interior<F>(
    support: &DensitySupport,
    samples: u32,
    salt: u64,
    mut evaluate: F,
) -> Result<(), String>
where
    F: FnMut(DVec3) -> Option<()>,
{
    let lattice = Lattice::new(support, samples)?;
    let mut accepted = 0u32;
    for index in 0..lattice.len() {
        let Some(point) = lattice.position(index, salt) else {
            continue;
        };
        if !support.contains_dvec(point) {
            continue;
        }
        accepted += 1;
        if evaluate(point).is_none() {
            return Err(format!(
                "density formula is non-finite at ({:.6}, {:.6}, {:.6}) inside the support",
                point.x, point.y, point.z
            ));
        }
    }
    if accepted == 0 {
        return Err("support interior contains no sample points".to_string());
    }
    Ok(())
}

struct ChargePass {
    total: f64,
    min: f64,
    max: f64,
    sign_changes: u32,
    samples: u32,
}

fn charge_pass(
    spec: &DensitySpec,
    expression: &FieldExpr,
    samples: u32,
    time: f64,
    salt: u64,
) -> Result<ChargePass, String> {
    let lattice = Lattice::new(&spec.support, samples)?;
    let mut interior: Vec<(u32, u32, u32, f64)> = Vec::new();
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    let mut total = 0.0;

    for index in 0..lattice.len() {
        let Some(position) = lattice.position(index, salt) else {
            continue;
        };
        if !spec.support.contains_dvec(position) {
            continue;
        }
        let Some(value) = eval_scalar(expression, position, time) else {
            return Err(format!(
                "density formula is non-finite at ({:.6}, {:.6}, {:.6}) inside the support",
                position.x, position.y, position.z
            ));
        };
        let (ix, iy, iz) = lattice.coordinates(index);
        interior.push((ix, iy, iz, value));
        min = min.min(value);
        max = max.max(value);
        total += value;
    }

    if interior.is_empty() {
        return Err("support interior contains no sample points".to_string());
    }

    let sign_changes = count_sign_changes(lattice.side, &interior);
    let total = total * lattice.cell_volume;
    if !total.is_finite() || !min.is_finite() || !max.is_finite() {
        return Err("interior charge diagnostics were not finite".to_string());
    }
    Ok(ChargePass {
        total,
        min,
        max,
        sign_changes,
        samples: interior.len() as u32,
    })
}

struct DivergencePass {
    min: f64,
    max: f64,
    worst: DVec3,
    max_abs: f64,
    max_j: f64,
}

fn divergence_pass(
    spec: &DensitySpec,
    expression: &FieldExpr,
    samples: u32,
    time: f64,
    salt: u64,
) -> Result<DivergencePass, String> {
    let lattice = Lattice::new(&spec.support, samples)?;
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    let mut worst = DVec3::ZERO;
    let mut max_abs = -1.0f64;
    let mut max_j = 0.0f64;
    let mut accepted = 0u32;

    for index in 0..lattice.len() {
        let Some(position) = lattice.position(index, salt) else {
            continue;
        };
        if !spec.support.contains_dvec(position) {
            continue;
        }
        let Some(current) = eval_current(expression, position, time) else {
            return Err(format!(
                "steady current is non-finite at ({:.6}, {:.6}, {:.6}) inside the support",
                position.x, position.y, position.z
            ));
        };
        let mut divergence = 0.0;
        for axis in 0..3 {
            let step = lattice.cell[axis];
            if !(step.is_finite() && step > 0.0) {
                return Err("interior divergence step is degenerate".to_string());
            }
            let mut forward = position;
            let mut backward = position;
            forward[axis] += step;
            backward[axis] -= step;
            let Some(plus) = eval_current(expression, forward, time) else {
                return Err(format!(
                    "steady current is non-finite at ({:.6}, {:.6}, {:.6}) on a divergence stencil",
                    forward.x, forward.y, forward.z
                ));
            };
            let Some(minus) = eval_current(expression, backward, time) else {
                return Err(format!(
                    "steady current is non-finite at ({:.6}, {:.6}, {:.6}) on a divergence stencil",
                    backward.x, backward.y, backward.z
                ));
            };
            divergence += (plus[axis] - minus[axis]) / (2.0 * step);
        }
        if !divergence.is_finite() {
            return Err(format!(
                "divergence was not finite at ({:.6}, {:.6}, {:.6})",
                position.x, position.y, position.z
            ));
        }
        accepted += 1;
        let magnitude = current[0]
            .mul_add(
                current[0],
                current[1].mul_add(current[1], current[2] * current[2]),
            )
            .sqrt();
        max_j = max_j.max(magnitude);
        min = min.min(divergence);
        max = max.max(divergence);
        let absolute = divergence.abs();
        if absolute > max_abs {
            max_abs = absolute;
            worst = position;
        }
    }

    if accepted == 0 || !min.is_finite() || !max.is_finite() || !max_abs.is_finite() {
        return Err("the support interior could not be sampled finitely".to_string());
    }
    Ok(DivergencePass {
        min,
        max,
        worst,
        max_abs,
        max_j,
    })
}

fn sign_class(value: f64) -> i8 {
    if value > 0.0 {
        1
    } else if value < 0.0 {
        -1
    } else {
        0
    }
}

/// Count axis-adjacent sign changes over sparse interior samples.
///
/// `samples` pairs each sample's lattice coordinate with its value and `side` is
/// the full lattice side. A zero value is neither positive nor negative and
/// never contributes a change. This is a `#[doc(hidden)]` test seam over the
/// same rule [`charge_pass`] applies to its sampled interior.
#[doc(hidden)]
pub fn count_sign_changes(side: u32, samples: &[(u32, u32, u32, f64)]) -> u32 {
    if side == 0 {
        return 0;
    }
    let side_len = side as usize;
    let Some(capacity) = side_len
        .checked_mul(side_len)
        .and_then(|len| len.checked_mul(side_len))
    else {
        return 0;
    };
    let mut values: Vec<Option<f64>> = vec![None; capacity];
    for &(ix, iy, iz, value) in samples {
        if ix < side && iy < side && iz < side {
            values[flatten_coordinates(side, ix, iy, iz)] = Some(value);
        }
    }

    let mut sign_changes = 0u32;
    for &(ix, iy, iz, value) in samples {
        if ix >= side || iy >= side || iz >= side {
            continue;
        }
        for (dx, dy, dz) in [(1u32, 0u32, 0u32), (0, 1, 0), (0, 0, 1)] {
            let nx = ix + dx;
            let ny = iy + dy;
            let nz = iz + dz;
            if nx >= side || ny >= side || nz >= side {
                continue;
            }
            let Some(other) = values[flatten_coordinates(side, nx, ny, nz)] else {
                continue;
            };
            let left = sign_class(value);
            let right = sign_class(other);
            if left != 0 && right != 0 && left != right {
                sign_changes += 1;
            }
        }
    }
    sign_changes
}

fn eval_scalar(expression: &FieldExpr, point: DVec3, time: f64) -> Option<f64> {
    let components = expression.eval3_components_at_f64(point, time)?;
    let value = components[1];
    value.is_finite().then_some(value)
}

fn eval_current(expression: &FieldExpr, point: DVec3, time: f64) -> Option<[f64; 3]> {
    expression.eval3_components_at_f64(point, time)
}

fn flatten_coordinates(side: u32, ix: u32, iy: u32, iz: u32) -> usize {
    let side = u64::from(side);
    ((u64::from(iy) * side + u64::from(ix)) * side + u64::from(iz)) as usize
}

struct Lattice {
    side: u32,
    min: DVec3,
    max: DVec3,
    cell: DVec3,
    cell_volume: f64,
}

impl Lattice {
    fn new(support: &DensitySupport, samples: u32) -> Result<Self, String> {
        if samples == 0 {
            return Err("lattice side must be at least one".to_string());
        }
        let (min, max) = support
            .bounds()
            .ok_or_else(|| "support has no finite bounding box".to_string())?;
        let cell = (max - min) / f64::from(samples);
        if !(cell.x > 0.0 && cell.y > 0.0 && cell.z > 0.0) {
            return Err("support has a degenerate bounding box".to_string());
        }
        Ok(Self {
            side: samples,
            min,
            max,
            cell,
            cell_volume: cell.x * cell.y * cell.z,
        })
    }

    fn len(&self) -> usize {
        let side = self.side as usize;
        side * side * side
    }

    fn coordinates(&self, index: usize) -> (u32, u32, u32) {
        let side = u64::from(self.side);
        let index = index as u64;
        let iz = (index % side) as u32;
        let ix = ((index / side) % side) as u32;
        let iy = (index / (side * side)) as u32;
        (ix, iy, iz)
    }

    /// The jittered centre of one lattice cell, or `None` for an out-of-range
    /// index (which the bounded loops never produce).
    fn position(&self, index: usize, salt: u64) -> Option<DVec3> {
        if index >= self.len() {
            return None;
        }
        let (ix, iy, iz) = self.coordinates(index);
        let base = self.min
            + DVec3::new(
                (f64::from(ix) + 0.5) * self.cell.x,
                (f64::from(iy) + 0.5) * self.cell.y,
                (f64::from(iz) + 0.5) * self.cell.z,
            );
        let (ux, uy, uz) = unit_triple(rand_stream(salt, index as u64));
        let jitter = DVec3::new(ux, uy, uz) * (self.cell * JITTER);
        let point = base + jitter;
        if point.is_finite()
            && point.x >= self.min.x
            && point.x <= self.max.x
            && point.y >= self.min.y
            && point.y <= self.max.y
            && point.z >= self.min.z
            && point.z <= self.max.z
        {
            Some(point)
        } else {
            None
        }
    }
}

/// Stable salt over the frozen support geometry.
fn support_salt(support: &DensitySupport) -> u64 {
    let mut hash = match support.kind {
        SupportKind::Sphere => 0x5350_4845_5245_0001,
        SupportKind::Box => 0x424F_5800_0000_0002,
        SupportKind::Cylinder => 0x4359_4C00_0000_0003,
    };
    for value in [
        support.center.x,
        support.center.y,
        support.center.z,
        support.axis.x,
        support.axis.y,
        support.axis.z,
        support.size.x,
        support.size.y,
        support.size.z,
    ] {
        hash = mix64(hash ^ u64::from(value.to_bits()));
    }
    hash
}

/// Splitmix64 stream value for `(salt, index)`, matching the streamline seed
/// scheme. Every operation wraps explicitly.
fn rand_stream(salt: u64, index: u64) -> u64 {
    let z = salt.wrapping_add((index + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    mix64(z)
}

fn mix64(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Three independent 21-bit slices mapped onto `[-1, 1)`.
fn unit_triple(z: u64) -> (f64, f64, f64) {
    const SLICE: u64 = 0x1F_FFFF;
    const SCALE: f64 = 2.0 / 2_097_152.0;
    let a = ((z >> 42) & SLICE) as u32 as f64;
    let b = ((z >> 21) & SLICE) as u32 as f64;
    let c = (z & SLICE) as u32 as f64;
    (a * SCALE - 1.0, b * SCALE - 1.0, c * SCALE - 1.0)
}
