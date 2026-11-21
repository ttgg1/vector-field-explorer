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

/// Largest accepted angular frequency for a harmonic time law, in normalized
/// units. The bound keeps the declared clock finite and bounded without
/// offering an arbitrary high-frequency source.
pub const MAX_ANGULAR_FREQUENCY: f64 = 1.0e3;

/// Clock step cap for the harmonic continuity finite difference, in seconds.
///
/// The check compares the sampled `div J(t)` with
/// `-(rho(t + dt) - rho(t)) / dt` from the *independent* paired charge formula,
/// not with an analytically differentiated expression. `dt` is
/// `min(1e-3 / omega, 1e-3)`, so `omega * dt <= 1e-3` and the forward
/// difference's truncation error stays far below [`CONTINUITY_TOLERANCE`].
const HARMONIC_DIFFERENCE_STEP: f64 = 1.0e-3;

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

/// The declared clock of a density.
///
/// `Steady` is the historical behaviour and multiplies the spatial density by
/// one. `Harmonic` multiplies it by `cos(omega * t + phase)`; the spatial
/// formula itself stays independent of the law.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum TimeLaw {
    #[default]
    Steady,
    Harmonic {
        omega: f64,
        phase: f64,
    },
}

impl TimeLaw {
    /// Validate the law's parameters without sampling a formula.
    ///
    /// A harmonic frequency must be finite and satisfy
    /// `0 < omega <= MAX_ANGULAR_FREQUENCY`; the phase must be finite.
    pub fn validate(&self) -> Result<(), String> {
        match *self {
            TimeLaw::Steady => Ok(()),
            TimeLaw::Harmonic { omega, phase } => {
                if !omega.is_finite() || omega <= 0.0 || omega > MAX_ANGULAR_FREQUENCY {
                    return Err(format!(
                        "harmonic angular frequency omega must be finite and satisfy \
                         0 < omega <= MAX_ANGULAR_FREQUENCY ({MAX_ANGULAR_FREQUENCY}); found {omega}"
                    ));
                }
                if !phase.is_finite() {
                    return Err(format!("harmonic phase must be finite; found {phase}"));
                }
                Ok(())
            }
        }
    }

    /// Whether this law varies with the clock.
    #[must_use]
    pub fn is_harmonic(self) -> bool {
        matches!(self, TimeLaw::Harmonic { .. })
    }

    /// The multiplicative clock factor at `time`, or `None` when the declared
    /// clock is not finite. `Steady` is always exactly one.
    fn scale(self, time: f64) -> Option<f64> {
        match self {
            TimeLaw::Steady => Some(1.0),
            TimeLaw::Harmonic { omega, phase } => {
                let factor = omega.mul_add(time, phase).cos();
                factor.is_finite().then_some(factor)
            }
        }
    }
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
    /// Declared clock; `Steady` is the historical zero-frequency behaviour.
    pub time_law: TimeLaw,
    /// Independent charge density paired with a harmonic current.
    ///
    /// A scalar `rho(x,y,z,t)` in the same shared grammar as `formula`. It is
    /// required for a genuine harmonic-current continuity check and must be
    /// absent for the charge families, whose own `formula` is already `rho`.
    /// It is ignored by a steady law, which keeps the historical `div J = 0`
    /// comparison unchanged.
    pub charge_formula: Option<String>,
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

/// Why a continuity diagnostic is or is not a usable estimate.
///
/// Unlike [`DiagnosticsStatus`], a violation carries the measured number so the
/// caller can show exactly how far the sampled divergence missed the charge
/// derivative (zero for a steady law, the paired finite difference for a
/// harmonic current).
#[derive(Debug, Clone, PartialEq)]
pub enum ContinuityStatus {
    /// Both terms were finite; `value` is the largest sampled residual and
    /// `error` is the full/half-pass difference.
    Estimate { value: f64, error: f64 },
    /// The comparison could not be made; the reason names the missing term.
    Unresolved { reason: String },
    /// The residual exceeded the normalized tolerance; both numbers are kept.
    Violation { residual: f64, tolerance: f64 },
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

/// Current continuity diagnostics over the sampled interior.
#[derive(Debug, Clone, PartialEq)]
pub struct ContinuityReport {
    pub div_j_min: f64,
    pub div_j_max: f64,
    pub worst_point: Option<Vec3>,
    pub status: ContinuityStatus,
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
        self.time_law.validate()?;
        if matches!(
            self.family,
            DensityFamily::VolumeCharge | DensityFamily::SurfaceCharge
        ) && self.charge_formula.is_some()
        {
            return Err(
                "a charge density's formula is its charge density; charge_formula must be absent"
                    .to_string(),
            );
        }
        self.validate_charge_formula(parameters)?;
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
                    eval_current(&expression, point, 0.0, self.time_law).map(|_| ())
                })
            }
            DensityFamily::VolumeCharge | DensityFamily::SurfaceCharge => {
                validate_interior(&self.support, self.samples, salt, |point| {
                    eval_scalar(&expression, point, 0.0, self.time_law).map(|_| ())
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

    /// Validate the optional paired charge formula without sampling it.
    ///
    /// A present formula must be non-empty and parse under the shared grammar
    /// with the committed parameters. It is a scalar, so it is wrapped exactly
    /// like a charge family's `formula` before `FieldExpr` sees it. Whether the
    /// formula is physically consistent with the current is a diagnostic
    /// question, answered only by [`continuity_report`]; validation checks
    /// syntax, not the continuity identity.
    pub fn validate_charge_formula(&self, parameters: &ParameterSet) -> Result<(), String> {
        self.parse_charge_expression(parameters).map(|_| ())
    }

    fn parse_charge_expression(
        &self,
        parameters: &ParameterSet,
    ) -> Result<Option<FieldExpr>, String> {
        let Some(formula) = self.charge_formula.as_deref() else {
            return Ok(None);
        };
        let formula = formula.trim();
        if formula.is_empty() {
            return Err("paired charge formula must not be empty".to_string());
        }
        let source = format!("(0,({formula}),0)");
        FieldExpr::parse_with_parameters(&source, parameters)
            .map(Some)
            .map_err(|error| format!("paired charge formula: {error}"))
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
    let full = match charge_pass(spec, &expression, spec.samples, time, salt, &spec.time_law) {
        Ok(pass) => pass,
        Err(reason) => return Ok(unresolved_charge(reason)),
    };
    let half_samples = (spec.samples / 2).max(MIN_SAMPLES / 2);
    let half = match charge_pass(spec, &expression, half_samples, time, salt, &spec.time_law) {
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

/// Continuity diagnostic bounded by [`CONTINUITY_TOLERANCE`].
///
/// A [`DensityFamily::SteadyCurrent`] is checked against `div J = 0`
/// ([`TimeLaw::Steady`]) or against an independently declared paired charge
/// density ([`TimeLaw::Harmonic`]). A harmonic charge family has no modelled
/// induced current, so it reports `Unresolved` instead of inventing one. A
/// steady charge family remains an error because continuity is not defined for
/// it here.
pub fn continuity_report(
    spec: &DensitySpec,
    parameters: &ParameterSet,
    time: f64,
) -> Result<ContinuityReport, String> {
    match spec.family {
        DensityFamily::SteadyCurrent => {}
        DensityFamily::VolumeCharge | DensityFamily::SurfaceCharge => {
            if spec.time_law.is_harmonic() {
                spec.validate(parameters)?;
                return Ok(unresolved_continuity(
                    "a harmonic charge density needs its induced current, which this build \
                     does not model, so continuity cannot be checked"
                        .to_string(),
                ));
            }
            return Err("continuity applies to steady currents only".to_string());
        }
    }
    spec.validate(parameters)?;

    let expression = spec.parse_expression(parameters)?;
    let salt = support_salt(&spec.support);
    match spec.time_law {
        TimeLaw::Steady => steady_continuity(spec, &expression, time, salt),
        TimeLaw::Harmonic { .. } => harmonic_continuity(spec, &expression, parameters, time, salt),
    }
}

/// Today's zero-derivative comparison for a steady current.
fn steady_continuity(
    spec: &DensitySpec,
    expression: &FieldExpr,
    time: f64,
    salt: u64,
) -> Result<ContinuityReport, String> {
    let full = match divergence_pass(spec, expression, spec.samples, time, salt, TimeLaw::Steady) {
        Ok(pass) => pass,
        Err(reason) => return Ok(unresolved_continuity(reason)),
    };
    let scale = full.max_j.max(CONTINUITY_MIN_SCALE);
    let threshold = CONTINUITY_TOLERANCE * scale;
    let status = if full.max_abs > threshold {
        ContinuityStatus::Violation {
            residual: full.max_abs,
            tolerance: threshold,
        }
    } else {
        let half_samples = (spec.samples / 2).max(MIN_SAMPLES / 2);
        match divergence_pass(spec, expression, half_samples, time, salt, TimeLaw::Steady) {
            Ok(half) => {
                let error = (full.max_abs - half.max_abs).abs();
                if error.is_finite() {
                    ContinuityStatus::Estimate {
                        value: full.max_abs,
                        error,
                    }
                } else {
                    ContinuityStatus::Unresolved {
                        reason: "the full and half divergence estimates were not finite"
                            .to_string(),
                    }
                }
            }
            Err(reason) => ContinuityStatus::Unresolved { reason },
        }
    };
    Ok(ContinuityReport {
        div_j_min: full.min,
        div_j_max: full.max,
        worst_point: Some(full.worst.as_vec3()),
        status,
    })
}

struct HarmonicPass {
    min: f64,
    max: f64,
    worst: DVec3,
    residual: f64,
    scale: f64,
}

/// The bounded clock step for a harmonic law, or `None` for a non-harmonic or
/// invalid frequency. See [`HARMONIC_DIFFERENCE_STEP`].
fn harmonic_difference_step(law: TimeLaw) -> Option<f64> {
    match law {
        TimeLaw::Steady => None,
        TimeLaw::Harmonic { omega, .. } => {
            if !omega.is_finite() || omega <= 0.0 {
                return None;
            }
            let step = (HARMONIC_DIFFERENCE_STEP / omega).min(HARMONIC_DIFFERENCE_STEP);
            (step.is_finite() && step > 0.0).then_some(step)
        }
    }
}

/// Compare the harmonic current with an independent paired charge density.
///
/// The convention remains `J(t) = J0 cos(omega t + phase)`, but the charge is
/// now `charge_formula`, evaluated directly at `t` and `t + dt`. The check is
/// the finite-difference continuity statement
/// `div J(t) ~ -(rho(t + dt) - rho(t)) / dt`, not an analytic identity and not a
/// self-derived `rho0 = div J0 / omega` comparison. A non-finite clock, current
/// or charge sample makes the pass `Err`; the measured residual is always kept.
fn harmonic_pass(
    spec: &DensitySpec,
    expression: &FieldExpr,
    charge: &FieldExpr,
    samples: u32,
    time: f64,
    salt: u64,
    law: TimeLaw,
) -> Result<HarmonicPass, String> {
    let lattice = Lattice::new(&spec.support, samples)?;
    let Some(clock) = law.scale(time) else {
        return Err(format!(
            "the declared harmonic clock is not finite at t = {time}"
        ));
    };
    let Some(step) = harmonic_difference_step(law) else {
        return Err(format!(
            "the declared harmonic frequency has no finite difference step at t = {time}"
        ));
    };
    let next_time = time + step;
    if !next_time.is_finite() {
        return Err(format!(
            "the finite-difference time t + dt is not finite at t = {time}"
        ));
    }

    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    let mut worst = DVec3::ZERO;
    let mut residual = -1.0f64;
    let mut scale = 0.0f64;
    let mut accepted = 0u32;

    for index in 0..lattice.len() {
        let Some(position) = lattice.position(index, salt) else {
            continue;
        };
        if !spec.support.contains_dvec(position) {
            continue;
        }
        if eval_current_raw(expression, position, time).is_none() {
            return Err(format!(
                "current is non-finite at ({:.6}, {:.6}, {:.6}) inside the support",
                position.x, position.y, position.z
            ));
        }
        let mut divergence = 0.0;
        for axis in 0..3 {
            let cell = lattice.cell[axis];
            if !(cell.is_finite() && cell > 0.0) {
                return Err("interior divergence step is degenerate".to_string());
            }
            let mut forward = position;
            let mut backward = position;
            forward[axis] += cell;
            backward[axis] -= cell;
            let Some(plus) = eval_current_raw(expression, forward, time) else {
                return Err(format!(
                    "current is non-finite at ({:.6}, {:.6}, {:.6}) on a divergence stencil",
                    forward.x, forward.y, forward.z
                ));
            };
            let Some(minus) = eval_current_raw(expression, backward, time) else {
                return Err(format!(
                    "current is non-finite at ({:.6}, {:.6}, {:.6}) on a divergence stencil",
                    backward.x, backward.y, backward.z
                ));
            };
            divergence += (plus[axis] - minus[axis]) / (2.0 * cell);
        }
        if !divergence.is_finite() {
            return Err(format!(
                "divergence was not finite at ({:.6}, {:.6}, {:.6})",
                position.x, position.y, position.z
            ));
        }

        let Some(rho_now) = eval_scalar_raw(charge, position, time) else {
            return Err(format!(
                "the paired charge density is non-finite at ({:.6}, {:.6}, {:.6}) at t = {time}",
                position.x, position.y, position.z
            ));
        };
        let Some(rho_next) = eval_scalar_raw(charge, position, next_time) else {
            return Err(format!(
                "the paired charge density is non-finite at ({:.6}, {:.6}, {:.6}) at t = {next_time}",
                position.x, position.y, position.z
            ));
        };
        let charge_derivative = -(rho_next - rho_now) / step;
        if !charge_derivative.is_finite() {
            return Err(format!(
                "the paired charge finite difference was not finite at ({:.6}, {:.6}, {:.6})",
                position.x, position.y, position.z
            ));
        }

        let sampled = divergence * clock;
        let difference = (sampled - charge_derivative).abs();
        if !sampled.is_finite() || !difference.is_finite() {
            return Err(format!(
                "the harmonic continuity terms were not finite at ({:.6}, {:.6}, {:.6})",
                position.x, position.y, position.z
            ));
        }

        scale = scale.max(sampled.abs()).max(charge_derivative.abs());
        if !scale.is_finite() {
            return Err("the harmonic continuity scale was not finite".to_string());
        }
        min = min.min(sampled);
        max = max.max(sampled);
        if difference > residual {
            residual = difference;
            worst = position;
        }
        accepted += 1;
    }

    if accepted == 0 || !min.is_finite() || !max.is_finite() || !residual.is_finite() {
        return Err("the support interior could not be sampled finitely".to_string());
    }
    Ok(HarmonicPass {
        min,
        max,
        worst,
        residual,
        scale,
    })
}

fn harmonic_continuity(
    spec: &DensitySpec,
    expression: &FieldExpr,
    parameters: &ParameterSet,
    time: f64,
    salt: u64,
) -> Result<ContinuityReport, String> {
    let law = spec.time_law;
    let Some(charge) = spec.parse_charge_expression(parameters)? else {
        return Ok(unresolved_continuity(
            "a harmonic current needs its paired charge density (charge_formula) for a \
             continuity check"
                .to_string(),
        ));
    };
    let full = match harmonic_pass(spec, expression, &charge, spec.samples, time, salt, law) {
        Ok(pass) => pass,
        Err(reason) => return Ok(unresolved_continuity(reason)),
    };
    let threshold = CONTINUITY_TOLERANCE * full.scale.max(CONTINUITY_MIN_SCALE);
    let status = if full.residual > threshold {
        ContinuityStatus::Violation {
            residual: full.residual,
            tolerance: threshold,
        }
    } else {
        let half_samples = (spec.samples / 2).max(MIN_SAMPLES / 2);
        match harmonic_pass(spec, expression, &charge, half_samples, time, salt, law) {
            Ok(half) => {
                let error = (full.residual - half.residual).abs();
                if error.is_finite() {
                    ContinuityStatus::Estimate {
                        value: full.residual,
                        error,
                    }
                } else {
                    ContinuityStatus::Unresolved {
                        reason: "the full and half harmonic residuals were not finite".to_string(),
                    }
                }
            }
            Err(reason) => ContinuityStatus::Unresolved { reason },
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
        status: ContinuityStatus::Unresolved { reason },
    }
}

/// Reject a density list that pairs a steady current with a harmonic charge
/// density.
///
/// A steady current has a zero time derivative, so it cannot balance a charge
/// density that varies with the clock. The rule is deliberately cross-spec and
/// explicit: the current may declare [`TimeLaw::Harmonic`] itself instead of
/// the input being silently coerced.
pub fn validate_time_law_combination(densities: &[DensitySpec]) -> Result<(), String> {
    let steady_current = densities
        .iter()
        .any(|spec| spec.family == DensityFamily::SteadyCurrent && !spec.time_law.is_harmonic());
    let harmonic_charge = densities.iter().any(|spec| {
        matches!(
            spec.family,
            DensityFamily::VolumeCharge | DensityFamily::SurfaceCharge
        ) && spec.time_law.is_harmonic()
    });
    if steady_current && harmonic_charge {
        return Err(
            "a steady current cannot balance a harmonic charge density; declare the current \
             harmonic or keep the charge steady"
                .to_string(),
        );
    }
    Ok(())
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
    law: &TimeLaw,
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
        let Some(value) = eval_scalar(expression, position, time, *law) else {
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
    law: TimeLaw,
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
        let Some(current) = eval_current(expression, position, time, law) else {
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
            let Some(plus) = eval_current(expression, forward, time, law) else {
                return Err(format!(
                    "steady current is non-finite at ({:.6}, {:.6}, {:.6}) on a divergence stencil",
                    forward.x, forward.y, forward.z
                ));
            };
            let Some(minus) = eval_current(expression, backward, time, law) else {
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

fn eval_scalar(expression: &FieldExpr, point: DVec3, time: f64, law: TimeLaw) -> Option<f64> {
    let value = eval_scalar_raw(expression, point, time)?;
    let scaled = value * law.scale(time)?;
    scaled.is_finite().then_some(scaled)
}

fn eval_scalar_raw(expression: &FieldExpr, point: DVec3, time: f64) -> Option<f64> {
    let components = expression.eval3_components_at_f64(point, time)?;
    let value = components[1];
    value.is_finite().then_some(value)
}

fn eval_current(expression: &FieldExpr, point: DVec3, time: f64, law: TimeLaw) -> Option<[f64; 3]> {
    let current = eval_current_raw(expression, point, time)?;
    let factor = law.scale(time)?;
    let scaled = [
        current[0] * factor,
        current[1] * factor,
        current[2] * factor,
    ];
    scaled
        .iter()
        .all(|value| value.is_finite())
        .then_some(scaled)
}

fn eval_current_raw(expression: &FieldExpr, point: DVec3, time: f64) -> Option<[f64; 3]> {
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
