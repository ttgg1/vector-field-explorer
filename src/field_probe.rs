//! Bounded Gaussian-surface and circulation-loop probes.
//!
//! This module owns the pure geometry and midpoint quadrature for the field
//! probes. It never evaluates a Maxwell solver itself: the caller supplies the
//! already selected field family through [`ProbeField`], and the report records
//! that family explicitly so electric and magnetic contributions are never
//! added together.
//!
//! Every value is a normalized teaching quantity. It is not an SI measurement
//! and carries no unit conversion. [`GaussianSurface::size`] and
//! [`CirculationLoop::size`] are full extents along the local axes: a sphere's
//! diameter is `size.x`, a box's edge lengths are `size`, a cylinder's diameter
//! and height are `size.x` and `size.y`, a circle's diameter is `size.x`, and a
//! rectangle's width and height are `size.x` and `size.y`. The local `z` axis is
//! [`GaussianSurface::axis`] or [`CirculationLoop::axis`], renormalized on use.

use glam::Vec3;

use crate::sources::FieldFamily;

/// Maximum number of Gaussian surfaces a document may own.
pub const MAX_SURFACES: usize = 8;
/// Maximum number of circulation loops a document may own.
pub const MAX_LOOPS: usize = 8;
/// Maximum number of midpoint patches or segments in one full-resolution pass.
pub const MAX_PROBE_SAMPLES: u32 = 4_096;
/// Minimum subdivision count along each loop or surface axis.
pub const MIN_SEGMENTS: u32 = 8;
/// Maximum subdivision count along each loop or surface axis.
pub const MAX_SEGMENTS: u32 = 64;

/// Supported closed Gaussian-surface shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceKind {
    Sphere,
    Box,
    Cylinder,
}

/// Supported closed circulation-loop shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopKind {
    Circle,
    Rectangle,
}

/// A bounded closed surface whose flux is integrated with midpoint patches.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GaussianSurface {
    pub kind: SurfaceKind,
    pub center: Vec3,
    pub axis: Vec3,
    pub size: Vec3,
    pub segments: u32,
}

/// A bounded oriented loop whose line integral is integrated with midpoint
/// segments.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CirculationLoop {
    pub kind: LoopKind,
    pub center: Vec3,
    pub axis: Vec3,
    pub size: Vec3,
    pub segments: u32,
}

/// The caller-supplied field for one family.
///
/// `None` means the active model cannot supply a sample (unsupported or
/// missing); `Some` with any non-finite component means the sample is singular.
/// Returned values are normalized teaching units, not SI quantities.
pub trait ProbeField {
    fn eval(&self, point: Vec3, time: f64) -> Option<Vec3>;
}

/// Honest outcome of one bounded integral.
#[derive(Debug, Clone, PartialEq)]
pub enum IntegralStatus {
    /// Both the full and half-resolution passes completed. `error` is the
    /// absolute difference between them, not a certified bound.
    Estimate { value: f64, error: f64 },
    /// At least one sample was missing or unsupported, so no value is claimed.
    Unresolved { reason: String },
    /// At least one sample was non-finite, so no value is claimed.
    Singular { reason: String },
}

/// Flux report for one surface and one explicit field family.
#[derive(Debug, Clone, PartialEq)]
pub struct FluxReport {
    pub surface: GaussianSurface,
    pub family: FieldFamily,
    pub status: IntegralStatus,
    /// Patches in the full-resolution pass, bounded by `MAX_PROBE_SAMPLES`.
    pub samples: u32,
    /// Patches without a finite sample in the failing pass. Never silently
    /// converted to a zero contribution.
    pub failed_patches: u32,
}

/// Circulation report for one loop and one explicit field family.
#[derive(Debug, Clone, PartialEq)]
pub struct CirculationReport {
    pub loop_: CirculationLoop,
    pub family: FieldFamily,
    pub status: IntegralStatus,
    /// Segments in the full-resolution pass, bounded by `MAX_PROBE_SAMPLES`.
    pub samples: u32,
    /// Segments without a finite sample in the failing pass. Never silently
    /// converted to a zero contribution.
    pub failed_segments: u32,
    /// Every supported loop is closed, so this is currently always `false`.
    /// The flag leaves room for future open paths without changing the report
    /// shape.
    pub one_sided: bool,
}

impl GaussianSurface {
    /// Validate the finite, non-degenerate geometry and the bounded sample
    /// budget before any integration work.
    pub fn validate(&self) -> Result<(), String> {
        validate_common(self.center, self.axis, self.size, self.segments)?;
        let count = surface_sample_count(self.kind, self.segments);
        if count > MAX_PROBE_SAMPLES {
            return Err(format!(
                "geometry needs {count} samples, exceeding MAX_PROBE_SAMPLES \
                 ({MAX_PROBE_SAMPLES})"
            ));
        }
        Ok(())
    }

    /// Analytic area in normalized teaching units squared. Invalid geometry
    /// returns a finite `0.0` rather than a non-finite result.
    pub fn area(&self) -> f64 {
        if !self.center.is_finite()
            || !self.axis.is_finite()
            || !self.size.is_finite()
            || self.size.x <= 0.0
            || self.size.y <= 0.0
            || self.size.z <= 0.0
        {
            return 0.0;
        }
        let x = self.size.x as f64;
        let y = self.size.y as f64;
        let z = self.size.z as f64;
        let area = match self.kind {
            SurfaceKind::Sphere => std::f64::consts::PI * x * x,
            SurfaceKind::Box => 2.0 * (x * y + y * z + z * x),
            SurfaceKind::Cylinder => {
                let radius = x * 0.5;
                std::f64::consts::PI * (2.0 * radius * radius + x * y)
            }
        };
        if area.is_finite() && area >= 0.0 {
            area
        } else {
            0.0
        }
    }
}

impl CirculationLoop {
    /// Validate the finite, non-degenerate geometry and the bounded sample
    /// budget before any integration work.
    pub fn validate(&self) -> Result<(), String> {
        validate_common(self.center, self.axis, self.size, self.segments)?;
        let count = loop_sample_count(self.kind, self.segments);
        if count > MAX_PROBE_SAMPLES {
            return Err(format!(
                "geometry needs {count} samples, exceeding MAX_PROBE_SAMPLES \
                 ({MAX_PROBE_SAMPLES})"
            ));
        }
        Ok(())
    }
}

/// Integrate `field · n` over one closed surface for the requested family.
///
/// The full and a half-resolution pass (halved segments, floor four) must both
/// complete before an [`IntegralStatus::Estimate`] is reported. Missing samples
/// make the result unresolved; non-finite samples make it singular.
pub fn flux(
    field: &dyn ProbeField,
    surface: &GaussianSurface,
    family: FieldFamily,
    time: f64,
) -> FluxReport {
    if let Err(error) = surface.validate() {
        return FluxReport {
            surface: *surface,
            family,
            status: IntegralStatus::Unresolved {
                reason: format!("invalid geometry: {error}"),
            },
            samples: 0,
            failed_patches: 0,
        };
    }

    let full = match integrate_surface(field, surface, surface.segments, time, "full") {
        Ok(pass) => pass,
        Err(error) => {
            return FluxReport {
                surface: *surface,
                family,
                status: IntegralStatus::Unresolved { reason: error },
                samples: 0,
                failed_patches: 0,
            };
        }
    };
    if full.failed() > 0 {
        let status = failure_status(&full, &PassTotals::default()).unwrap_or_else(|| {
            IntegralStatus::Unresolved {
                reason: "the full-resolution pass failed".to_string(),
            }
        });
        return FluxReport {
            surface: *surface,
            family,
            status,
            samples: full.samples,
            failed_patches: full.failed(),
        };
    }

    let half_resolution = half_segments(surface.segments);
    let half = match integrate_surface(field, surface, half_resolution, time, "half-resolution") {
        Ok(pass) => pass,
        Err(error) => {
            return FluxReport {
                surface: *surface,
                family,
                status: IntegralStatus::Unresolved { reason: error },
                samples: full.samples,
                failed_patches: 0,
            };
        }
    };

    let status = failure_status(&full, &half).unwrap_or_else(|| {
        let value = full.sum;
        let error = (full.sum - half.sum).abs();
        if value.is_finite() && error.is_finite() {
            IntegralStatus::Estimate { value, error }
        } else {
            IntegralStatus::Unresolved {
                reason: "the integral was not finite".to_string(),
            }
        }
    });
    FluxReport {
        surface: *surface,
        family,
        status,
        samples: full.samples,
        failed_patches: half.failed(),
    }
}

/// Integrate `field · dl` around one oriented loop for the requested family.
///
/// The full and a half-resolution pass (halved segments, floor four) must both
/// complete before an [`IntegralStatus::Estimate`] is reported. Missing samples
/// make the result unresolved; non-finite samples make it singular.
pub fn circulation(
    field: &dyn ProbeField,
    loop_: &CirculationLoop,
    family: FieldFamily,
    time: f64,
) -> CirculationReport {
    if let Err(error) = loop_.validate() {
        return CirculationReport {
            loop_: *loop_,
            family,
            status: IntegralStatus::Unresolved {
                reason: format!("invalid geometry: {error}"),
            },
            samples: 0,
            failed_segments: 0,
            one_sided: false,
        };
    }

    let full = match integrate_loop(field, loop_, loop_.segments, time, "full") {
        Ok(pass) => pass,
        Err(error) => {
            return CirculationReport {
                loop_: *loop_,
                family,
                status: IntegralStatus::Unresolved { reason: error },
                samples: 0,
                failed_segments: 0,
                one_sided: false,
            };
        }
    };
    if full.failed() > 0 {
        let status = failure_status(&full, &PassTotals::default()).unwrap_or_else(|| {
            IntegralStatus::Unresolved {
                reason: "the full-resolution pass failed".to_string(),
            }
        });
        return CirculationReport {
            loop_: *loop_,
            family,
            status,
            samples: full.samples,
            failed_segments: full.failed(),
            one_sided: false,
        };
    }

    let half_resolution = half_segments(loop_.segments);
    let half = match integrate_loop(field, loop_, half_resolution, time, "half-resolution") {
        Ok(pass) => pass,
        Err(error) => {
            return CirculationReport {
                loop_: *loop_,
                family,
                status: IntegralStatus::Unresolved { reason: error },
                samples: full.samples,
                failed_segments: 0,
                one_sided: false,
            };
        }
    };

    let status = failure_status(&full, &half).unwrap_or_else(|| {
        let value = full.sum;
        let error = (full.sum - half.sum).abs();
        if value.is_finite() && error.is_finite() {
            IntegralStatus::Estimate { value, error }
        } else {
            IntegralStatus::Unresolved {
                reason: "the integral was not finite".to_string(),
            }
        }
    });
    CirculationReport {
        loop_: *loop_,
        family,
        status,
        samples: full.samples,
        failed_segments: half.failed(),
        one_sided: false,
    }
}

#[derive(Debug, Default)]
struct PassTotals {
    sum: f64,
    samples: u32,
    missing: u32,
    non_finite: u32,
    first_missing: Option<String>,
    first_non_finite: Option<String>,
}

impl PassTotals {
    fn failed(&self) -> u32 {
        self.missing + self.non_finite
    }
}

fn accumulate(
    field: &dyn ProbeField,
    point: Vec3,
    direction: Vec3,
    measure: f64,
    time: f64,
    pass_label: &str,
    pass: &mut PassTotals,
) {
    pass.samples += 1;
    match field.eval(point, time) {
        None => {
            pass.missing += 1;
            if pass.first_missing.is_none() {
                pass.first_missing =
                    Some(format!("{} on the {pass_label} pass", point_label(point)));
            }
        }
        Some(value) if !value.is_finite() => {
            pass.non_finite += 1;
            if pass.first_non_finite.is_none() {
                pass.first_non_finite =
                    Some(format!("{} on the {pass_label} pass", point_label(point)));
            }
        }
        Some(value) => {
            pass.sum += value.as_dvec3().dot(direction.as_dvec3()) * measure;
        }
    }
}

fn failure_status(full: &PassTotals, half: &PassTotals) -> Option<IntegralStatus> {
    let non_finite = full.non_finite + half.non_finite;
    if non_finite > 0 {
        let first = full
            .first_non_finite
            .as_deref()
            .or(half.first_non_finite.as_deref())
            .unwrap_or("an unspecified point");
        return Some(IntegralStatus::Singular {
            reason: format!("{non_finite} non-finite field sample(s); first {first}"),
        });
    }
    let missing = full.missing + half.missing;
    if missing > 0 {
        let first = full
            .first_missing
            .as_deref()
            .or(half.first_missing.as_deref())
            .unwrap_or("an unspecified point");
        return Some(IntegralStatus::Unresolved {
            reason: format!("{missing} missing or unsupported field sample(s); first {first}"),
        });
    }
    None
}

fn integrate_surface(
    field: &dyn ProbeField,
    surface: &GaussianSurface,
    segments: u32,
    time: f64,
    pass_label: &str,
) -> Result<PassTotals, String> {
    let z = unit_axis(surface.axis).ok_or_else(|| "axis became degenerate".to_string())?;
    let mut pass = PassTotals::default();
    match surface.kind {
        SurfaceKind::Sphere => {
            integrate_sphere(field, surface, segments, time, pass_label, &mut pass);
        }
        SurfaceKind::Box => {
            let (u, v, _) = local_basis(z);
            integrate_box(
                field, surface, segments, time, pass_label, u, v, z, &mut pass,
            );
        }
        SurfaceKind::Cylinder => {
            let (u, v, _) = local_basis(z);
            integrate_cylinder(
                field, surface, segments, time, pass_label, u, v, z, &mut pass,
            );
        }
    }
    Ok(pass)
}

fn integrate_sphere(
    field: &dyn ProbeField,
    surface: &GaussianSurface,
    segments: u32,
    time: f64,
    pass_label: &str,
    pass: &mut PassTotals,
) {
    let count = segments as usize;
    let radius = (surface.size.x as f64) * 0.5;
    let dtheta = std::f64::consts::PI / segments as f64;
    let dphi = std::f64::consts::TAU / segments as f64;
    for lat in 0..count {
        let theta0 = lat as f64 * dtheta;
        let theta1 = (lat + 1) as f64 * dtheta;
        let theta = (theta0 + theta1) * 0.5;
        let sin_theta = theta.sin();
        let cos_theta = theta.cos();
        let area = radius * radius * dphi * (theta0.cos() - theta1.cos());
        for lon in 0..count {
            let phi = (lon as f64 + 0.5) * dphi;
            let normal = Vec3::new(
                (sin_theta * phi.cos()) as f32,
                (sin_theta * phi.sin()) as f32,
                cos_theta as f32,
            );
            let point = surface.center + normal * radius as f32;
            accumulate(field, point, normal, area, time, pass_label, pass);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn integrate_box(
    field: &dyn ProbeField,
    surface: &GaussianSurface,
    segments: u32,
    time: f64,
    pass_label: &str,
    u: Vec3,
    v: Vec3,
    w: Vec3,
    pass: &mut PassTotals,
) {
    let count = segments as usize;
    let half = surface.size.as_dvec3() * 0.5;
    let step_y = 2.0 * half.y / segments as f64;
    let step_z = 2.0 * half.z / segments as f64;
    let step_x = 2.0 * half.x / segments as f64;

    for i in 0..count {
        let y = -half.y + (i as f64 + 0.5) * step_y;
        for j in 0..count {
            let z = -half.z + (j as f64 + 0.5) * step_z;
            let area = step_y * step_z;
            let point = world(surface.center, u, v, w, [half.x, y, z]);
            accumulate(field, point, u, area, time, pass_label, pass);
            let point = world(surface.center, u, v, w, [-half.x, y, z]);
            accumulate(field, point, -u, area, time, pass_label, pass);
        }
    }

    for i in 0..count {
        let x = -half.x + (i as f64 + 0.5) * step_x;
        for j in 0..count {
            let z = -half.z + (j as f64 + 0.5) * step_z;
            let area = step_x * step_z;
            let point = world(surface.center, u, v, w, [x, half.y, z]);
            accumulate(field, point, v, area, time, pass_label, pass);
            let point = world(surface.center, u, v, w, [x, -half.y, z]);
            accumulate(field, point, -v, area, time, pass_label, pass);
        }
    }

    for i in 0..count {
        let x = -half.x + (i as f64 + 0.5) * step_x;
        for j in 0..count {
            let y = -half.y + (j as f64 + 0.5) * step_y;
            let area = step_x * step_y;
            let point = world(surface.center, u, v, w, [x, y, half.z]);
            accumulate(field, point, w, area, time, pass_label, pass);
            let point = world(surface.center, u, v, w, [x, y, -half.z]);
            accumulate(field, point, -w, area, time, pass_label, pass);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn integrate_cylinder(
    field: &dyn ProbeField,
    surface: &GaussianSurface,
    segments: u32,
    time: f64,
    pass_label: &str,
    u: Vec3,
    v: Vec3,
    w: Vec3,
    pass: &mut PassTotals,
) {
    let count = segments as usize;
    let radius = (surface.size.x as f64) * 0.5;
    let half_height = (surface.size.y as f64) * 0.5;
    let dphi = std::f64::consts::TAU / segments as f64;
    let dz = 2.0 * half_height / segments as f64;
    let side_area = radius * dphi * dz;

    for i in 0..count {
        let phi = (i as f64 + 0.5) * dphi;
        let radial = u * (phi.cos() as f32) + v * (phi.sin() as f32);
        for j in 0..count {
            let z = -half_height + (j as f64 + 0.5) * dz;
            let point = world(
                surface.center,
                u,
                v,
                w,
                [radius * phi.cos(), radius * phi.sin(), z],
            );
            accumulate(field, point, radial, side_area, time, pass_label, pass);
        }
    }

    for cap in 0..2 {
        let sign = if cap == 0 { 1.0 } else { -1.0 };
        let normal = if cap == 0 { w } else { -w };
        let z = sign * half_height;
        for ring in 0..count {
            let inner = radius * ring as f64 / segments as f64;
            let outer = radius * (ring + 1) as f64 / segments as f64;
            let mid = (inner + outer) * 0.5;
            let area = std::f64::consts::PI * (outer * outer - inner * inner) / segments as f64;
            for sector in 0..count {
                let phi = (sector as f64 + 0.5) * dphi;
                let point = world(
                    surface.center,
                    u,
                    v,
                    w,
                    [mid * phi.cos(), mid * phi.sin(), z],
                );
                accumulate(field, point, normal, area, time, pass_label, pass);
            }
        }
    }
}

fn integrate_loop(
    field: &dyn ProbeField,
    loop_: &CirculationLoop,
    segments: u32,
    time: f64,
    pass_label: &str,
) -> Result<PassTotals, String> {
    let w = unit_axis(loop_.axis).ok_or_else(|| "axis became degenerate".to_string())?;
    let (u, v, _) = local_basis(w);
    let mut pass = PassTotals::default();
    match loop_.kind {
        LoopKind::Circle => {
            integrate_circle(field, loop_, segments, time, pass_label, u, v, w, &mut pass);
        }
        LoopKind::Rectangle => {
            integrate_rectangle(field, loop_, segments, time, pass_label, u, v, w, &mut pass);
        }
    }
    Ok(pass)
}

#[allow(clippy::too_many_arguments)]
fn integrate_circle(
    field: &dyn ProbeField,
    loop_: &CirculationLoop,
    segments: u32,
    time: f64,
    pass_label: &str,
    u: Vec3,
    v: Vec3,
    w: Vec3,
    pass: &mut PassTotals,
) {
    let radius = (loop_.size.x as f64) * 0.5;
    let dphi = std::f64::consts::TAU / segments as f64;
    let length = radius * dphi;
    for i in 0..segments as usize {
        let phi = (i as f64 + 0.5) * dphi;
        let point = world(
            loop_.center,
            u,
            v,
            w,
            [radius * phi.cos(), radius * phi.sin(), 0.0],
        );
        let tangent = Vec3::new(-phi.sin() as f32, phi.cos() as f32, 0.0);
        let tangent = u * tangent.x + v * tangent.y;
        accumulate(field, point, tangent, length, time, pass_label, pass);
    }
}

#[allow(clippy::too_many_arguments)]
fn integrate_rectangle(
    field: &dyn ProbeField,
    loop_: &CirculationLoop,
    segments: u32,
    time: f64,
    pass_label: &str,
    u: Vec3,
    v: Vec3,
    w: Vec3,
    pass: &mut PassTotals,
) {
    let count = segments as usize;
    let half_x = (loop_.size.x as f64) * 0.5;
    let half_y = (loop_.size.y as f64) * 0.5;
    let step_x = 2.0 * half_x / segments as f64;
    let step_y = 2.0 * half_y / segments as f64;
    let length_x = step_x;
    let length_y = step_y;

    for i in 0..count {
        let t = i as f64 + 0.5;
        let x = -half_x + t * step_x;
        let point = world(loop_.center, u, v, w, [x, -half_y, 0.0]);
        accumulate(field, point, u, length_x, time, pass_label, pass);

        let y = -half_y + t * step_y;
        let point = world(loop_.center, u, v, w, [half_x, y, 0.0]);
        accumulate(field, point, v, length_y, time, pass_label, pass);

        let x = half_x - t * step_x;
        let point = world(loop_.center, u, v, w, [x, half_y, 0.0]);
        accumulate(field, point, -u, length_x, time, pass_label, pass);

        let y = half_y - t * step_y;
        let point = world(loop_.center, u, v, w, [-half_x, y, 0.0]);
        accumulate(field, point, -v, length_y, time, pass_label, pass);
    }
}

fn validate_common(center: Vec3, axis: Vec3, size: Vec3, segments: u32) -> Result<(), String> {
    if !center.is_finite() {
        return Err("center must have finite components".to_string());
    }
    if !axis.is_finite() {
        return Err("axis must have finite components".to_string());
    }
    if unit_axis(axis).is_none() {
        return Err("axis must be non-degenerate (nonzero finite length)".to_string());
    }
    if !size.is_finite() {
        return Err("size extents must be finite and positive".to_string());
    }
    if size.x <= 0.0 || size.y <= 0.0 || size.z <= 0.0 {
        return Err("size extents must be positive".to_string());
    }
    if !(MIN_SEGMENTS..=MAX_SEGMENTS).contains(&segments) {
        return Err(format!(
            "segments {segments} outside {MIN_SEGMENTS}..={MAX_SEGMENTS}"
        ));
    }
    Ok(())
}

fn surface_sample_count(kind: SurfaceKind, segments: u32) -> u32 {
    match kind {
        SurfaceKind::Sphere => segments * segments,
        SurfaceKind::Box => 6 * segments * segments,
        SurfaceKind::Cylinder => 3 * segments * segments,
    }
}

fn loop_sample_count(kind: LoopKind, segments: u32) -> u32 {
    match kind {
        LoopKind::Circle => segments,
        LoopKind::Rectangle => 4 * segments,
    }
}

fn half_segments(segments: u32) -> u32 {
    (segments / 2).max(4)
}

fn unit_axis(axis: Vec3) -> Option<Vec3> {
    let axis = axis.as_dvec3();
    let length = axis.length();
    if !length.is_finite() || length <= 0.0 {
        return None;
    }
    let unit = (axis / length).as_vec3();
    if unit.is_finite() { Some(unit) } else { None }
}

fn local_basis(z: Vec3) -> (Vec3, Vec3, Vec3) {
    let helper = if z.z.abs() < 0.9 { Vec3::Z } else { Vec3::Y };
    let x = helper.cross(z).normalize();
    let y = z.cross(x);
    (x, y, z)
}

fn world(center: Vec3, u: Vec3, v: Vec3, w: Vec3, local: [f64; 3]) -> Vec3 {
    let center = center.as_dvec3();
    let point =
        center + u.as_dvec3() * local[0] + v.as_dvec3() * local[1] + w.as_dvec3() * local[2];
    point.as_vec3()
}

fn point_label(point: Vec3) -> String {
    format!("at ({:.6}, {:.6}, {:.6})", point.x, point.y, point.z)
}
