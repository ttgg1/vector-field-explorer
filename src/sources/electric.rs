//! Stable electric geometry reductions.
//!
//! Each charged geometry reduces to a single bounded one-dimensional integral
//! (or an analytic near-axis/surface path) in a source-local frame. The
//! quadrature paths reuse Task 2's complete-coverage partition and absolute
//! uncertainty accounting; the ring keeps a near-axis binomial series with an
//! explicit positive tail, the disc uses the exact transverse chord integral,
//! and the sphere uses the centre-stable divergence-theorem surface integral.

use std::f64::consts::PI;

use glam::DVec3;

use super::model::{Source, SourceKind};
use super::numeric::{
    FieldContribution, FiniteLineGeometry, NumericRange, allowance_for, finite_line_axial_field,
    local_frame, point_field, stable_norm,
};
use super::quadrature::{self, Budget, Cell, Failure, KernelSample, ResolutionGuard};

/// One complete per-source electric contribution with counted work and an
/// optional refinement stop reason.
pub(crate) type ElectricResult = (FieldContribution, u64, usize, Option<Failure>);

/// Evaluate one electric source at `point`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn contribution(
    point: DVec3,
    source: &Source,
    softening: f64,
    target: f64,
    work_remaining: u64,
    cells_per_family: usize,
    max_depth: u8,
    scratch: &mut Vec<Cell>,
    cancelled: &dyn Fn() -> bool,
) -> Result<ElectricResult, Failure> {
    match source.kind {
        SourceKind::PointCharge { charge } => {
            let contribution =
                point_field(point - source.center, softening, charge).map_err(numeric_failure)?;
            Ok((contribution, 1, 0, None))
        }
        SourceKind::FiniteLineCharge { charge, length } => line(
            point,
            source,
            charge,
            length,
            softening,
            target,
            work_remaining,
            cells_per_family,
            max_depth,
            scratch,
            cancelled,
        ),
        SourceKind::ChargedRing { charge, radius } => ring(
            point,
            source,
            charge,
            radius,
            softening,
            target,
            work_remaining,
            cells_per_family,
            max_depth,
            scratch,
            cancelled,
        ),
        SourceKind::ChargedDisc { charge, radius } => disc(
            point,
            source,
            charge,
            radius,
            softening,
            target,
            work_remaining,
            cells_per_family,
            max_depth,
            scratch,
            cancelled,
        ),
        SourceKind::SolidSphere { charge, radius } => sphere(
            point,
            source,
            charge,
            radius,
            softening,
            target,
            work_remaining,
            cells_per_family,
            max_depth,
            scratch,
            cancelled,
        ),
        SourceKind::StraightWire { .. }
        | SourceKind::CurrentLoop { .. }
        | SourceKind::Solenoid { .. } => Err(Failure::UnresolvedFeature),
    }
}

fn numeric_failure(_: NumericRange) -> Failure {
    Failure::NumericRange
}

#[allow(clippy::too_many_arguments)]
fn line(
    point: DVec3,
    source: &Source,
    charge: f64,
    length: f64,
    softening: f64,
    target: f64,
    work_remaining: u64,
    cells_per_family: usize,
    max_depth: u8,
    scratch: &mut Vec<Cell>,
    cancelled: &dyn Fn() -> bool,
) -> Result<ElectricResult, Failure> {
    let geometry =
        FiniteLineGeometry::new(point, source.center, source.axis, softening, charge, length)
            .map_err(numeric_failure)?;

    if geometry.radial == 0.0 {
        let contribution =
            finite_line_axial_field(point, source.center, source.axis, softening, charge, length)
                .map_err(numeric_failure)?;
        return Ok((contribution, 1, 0, None));
    }

    let half_length = 0.5 * length;
    if !half_length.is_finite() {
        return Err(Failure::NumericRange);
    }
    let guard = LineGuard {
        radial: geometry.radial,
        axial: geometry.axial,
        softening,
    };
    let mut kernel = |parameter: f64| geometry.sample(parameter);
    let mut budget = Budget::new(work_remaining, cells_per_family, max_depth, target);
    let seeds = [geometry.axial];
    let partition = quadrature::integrate(
        (-half_length, half_length),
        &seeds,
        &guard,
        &mut kernel,
        &mut budget,
        scratch,
        cancelled,
    )?;

    from_partition(partition, geometry.radial_direction, geometry.unit_axis)
}

#[allow(clippy::too_many_arguments)]
fn ring(
    point: DVec3,
    source: &Source,
    charge: f64,
    radius: f64,
    softening: f64,
    target: f64,
    work_remaining: u64,
    cells_per_family: usize,
    max_depth: u8,
    scratch: &mut Vec<Cell>,
    cancelled: &dyn Fn() -> bool,
) -> Result<ElectricResult, Failure> {
    let frame = local_frame(point, source.center, source.axis).map_err(numeric_failure)?;
    let scale = frame
        .axial
        .abs()
        .max(frame.radial)
        .max(radius)
        .max(softening);
    if !scale.is_finite() || scale <= 0.0 {
        return Err(Failure::NumericRange);
    }
    let rn = frame.radial / scale;
    let zn = frame.axial / scale;
    let rn_radius = radius / scale;
    let an = softening / scale;
    let a_n = rn * rn + rn_radius * rn_radius + zn * zn + an * an;
    if !a_n.is_finite() || a_n <= 0.0 {
        return Err(Failure::NumericRange);
    }
    let delta = 2.0 * rn * rn_radius / a_n;
    if !delta.is_finite() || delta < 0.0 || delta > 1.0 {
        return Err(Failure::NumericRange);
    }
    let factor = charge / (scale * scale);
    if !factor.is_finite() {
        return Err(Failure::NumericRange);
    }
    let base = factor / (a_n * a_n.sqrt());
    if !base.is_finite() {
        return Err(Failure::NumericRange);
    }

    if delta <= 0.125 {
        let series = ring_series(
            delta,
            base,
            rn,
            zn,
            rn_radius,
            target,
            work_remaining,
            cancelled,
        )?;
        let radial = base * (rn * series.s0 - rn_radius * series.s1);
        let axial = base * (zn * series.s0);
        let tail = base.abs() * ((rn.abs() + zn.abs()) * series.tail0 + rn_radius * series.tail1);
        let underflow = charge != 0.0
            && (base == 0.0
                || (radial == 0.0 && rn * series.s0 - rn_radius * series.s1 != 0.0)
                || (axial == 0.0 && zn * series.s0 != 0.0));
        let mut contribution = reconstruct(
            frame.radial_direction,
            frame.unit_axis,
            radial,
            axial,
            underflow,
        )?;
        contribution.allowance += tail;
        if !contribution.allowance.is_finite() {
            return Err(Failure::NumericRange);
        }
        return Ok((contribution, series.work, 0, series.stop));
    }

    let kernel_factor = base / (2.0 * PI);
    let mut kernel = |phi: f64| -> Result<KernelSample, NumericRange> {
        if !phi.is_finite() {
            return Err(NumericRange::Unrepresentable);
        }
        let (_, cos) = phi.sin_cos();
        let denominator = 1.0 - delta * cos;
        if !denominator.is_finite() || denominator <= 0.0 {
            return Err(NumericRange::Unrepresentable);
        }
        let weight = 1.0 / (denominator * denominator.sqrt());
        let value = [
            kernel_factor * (rn - rn_radius * cos) * weight,
            kernel_factor * zn * weight,
        ];
        if !value[0].is_finite() || !value[1].is_finite() {
            return Err(NumericRange::Unrepresentable);
        }
        Ok(KernelSample {
            value,
            underflow: charge != 0.0 && kernel_factor == 0.0,
        })
    };
    let guard = RingGuard {
        radius,
        radial: frame.radial,
        axial: frame.axial,
        softening,
    };
    let mut budget = Budget::new(work_remaining, cells_per_family, max_depth, target);
    let partition = quadrature::integrate(
        (-PI, PI),
        &[0.0],
        &guard,
        &mut kernel,
        &mut budget,
        scratch,
        cancelled,
    )?;
    from_partition(partition, frame.radial_direction, frame.unit_axis)
}

struct RingSeries {
    s0: f64,
    s1: f64,
    tail0: f64,
    tail1: f64,
    work: u64,
    stop: Option<Failure>,
}

#[allow(clippy::too_many_arguments)]
fn ring_series(
    delta: f64,
    base: f64,
    rn: f64,
    zn: f64,
    rn_radius: f64,
    target: f64,
    work_remaining: u64,
    cancelled: &dyn Fn() -> bool,
) -> Result<RingSeries, Failure> {
    if work_remaining == 0 {
        return Err(Failure::WorkLimit);
    }
    let mut s0 = 1.0;
    let mut s1 = 0.75 * delta;
    let mut b = 1.0;
    let mut d = 0.75;
    let mut p0 = 1.0;
    let mut p1 = delta;
    let delta_squared = delta * delta;
    let inverse_ratio = 1.0 / (1.0 - 2.0 * delta);
    if !inverse_ratio.is_finite() || inverse_ratio <= 0.0 {
        return Err(Failure::NumericRange);
    }
    let component_scale = base.abs() * (rn.abs() + zn.abs() + rn_radius);
    if !component_scale.is_finite() {
        return Err(Failure::NumericRange);
    }
    let threshold = 0.25 * target;
    let mut tail0 = 0.0;
    let mut tail1 = 0.0;
    let mut work = 0u64;
    let mut converged = false;
    let mut stop = None;
    for k in 0..64usize {
        if k > 0 {
            b *= ratio_b(k);
            d *= ratio_d(k);
            p0 *= delta_squared;
            p1 *= delta_squared;
            s0 += b * p0;
            s1 += d * p1;
        }
        work += 1;
        if work.is_multiple_of(32) && cancelled() {
            return Err(Failure::Cancelled);
        }
        let next_b = b * ratio_b(k + 1);
        let next_d = d * ratio_d(k + 1);
        let next_p0 = p0 * delta_squared;
        let next_p1 = p1 * delta_squared;
        tail0 = next_b * next_p0 * inverse_ratio;
        tail1 = next_d * next_p1 * inverse_ratio;
        let error = component_scale * (tail0 + tail1);
        if error <= threshold {
            converged = true;
            break;
        }
        if work >= work_remaining {
            stop = Some(Failure::WorkLimit);
            break;
        }
    }
    if !converged && stop.is_none() {
        stop = Some(Failure::RoundoffLimited);
    }
    if !s0.is_finite() || !s1.is_finite() || !tail0.is_finite() || !tail1.is_finite() {
        return Err(Failure::NumericRange);
    }
    Ok(RingSeries {
        s0,
        s1,
        tail0,
        tail1,
        work,
        stop,
    })
}

/// `b_k / b_{k-1}` for the even angular moments of `(1 - x)^(-3/2)`.
fn ratio_b(k: usize) -> f64 {
    let k = k as f64;
    (4.0 * k + 1.0) / (4.0 * k) * (4.0 * k - 1.0) / (4.0 * k - 2.0) * (2.0 * (2.0 * k - 1.0) / k)
        / 4.0
}

/// `d_k / d_{k-1}` for the cosine-weighted angular moments.
fn ratio_d(k: usize) -> f64 {
    let k = k as f64;
    (4.0 * k + 3.0) / (4.0 * k + 2.0) * (4.0 * k + 1.0) / (4.0 * k)
        * (2.0 * (2.0 * k + 1.0) / (k + 1.0))
        / 4.0
}

#[allow(clippy::too_many_arguments)]
fn disc(
    point: DVec3,
    source: &Source,
    charge: f64,
    radius: f64,
    softening: f64,
    target: f64,
    work_remaining: u64,
    cells_per_family: usize,
    max_depth: u8,
    scratch: &mut Vec<Cell>,
    cancelled: &dyn Fn() -> bool,
) -> Result<ElectricResult, Failure> {
    let frame = local_frame(point, source.center, source.axis).map_err(numeric_failure)?;

    if frame.radial == 0.0 {
        let scale = frame.axial.abs().max(radius).max(softening);
        if !scale.is_finite() || scale <= 0.0 {
            return Err(Failure::NumericRange);
        }
        let zn = frame.axial / scale;
        let rn_radius = radius / scale;
        let an = softening / scale;
        let b = (zn * zn + an * an).sqrt();
        let d = (rn_radius * rn_radius + zn * zn + an * an).sqrt();
        if b <= 0.0 || d <= 0.0 {
            return Err(Failure::NumericRange);
        }
        let axial = if zn == 0.0 {
            0.0
        } else {
            2.0 * charge * zn / (scale * scale * b * d * (d + b))
        };
        if !axial.is_finite() {
            return Err(Failure::NumericRange);
        }
        let value = frame.unit_axis * axial;
        // z = 0 on the axis is an exact symmetry zero, not rounded underflow.
        let underflow = charge != 0.0 && zn != 0.0 && axial == 0.0;
        let allowance = allowance_for(value, underflow).map_err(numeric_failure)?;
        return Ok((
            FieldContribution {
                value,
                allowance,
                underflow,
            },
            1,
            0,
            None,
        ));
    }

    let scale = frame
        .radial
        .abs()
        .max(frame.axial.abs())
        .max(radius)
        .max(softening);
    if !scale.is_finite() || scale <= 0.0 {
        return Err(Failure::NumericRange);
    }
    let rn = frame.radial / scale;
    let zn = frame.axial / scale;
    let rn_radius = radius / scale;
    let an = softening / scale;
    let factor = 2.0 * charge / (PI * scale * scale);
    if !factor.is_finite() {
        return Err(Failure::NumericRange);
    }
    let mut kernel = |theta: f64| -> Result<KernelSample, NumericRange> {
        if !theta.is_finite() {
            return Err(NumericRange::Unrepresentable);
        }
        let (sin, cos) = theta.sin_cos();
        let x_n = rn_radius * sin;
        let h_n = rn_radius * cos;
        let c_n = (rn - x_n) * (rn - x_n) + zn * zn + an * an;
        let d_n = (c_n + h_n * h_n).sqrt();
        if !c_n.is_finite() || c_n <= 0.0 || !d_n.is_finite() || d_n <= 0.0 {
            return Err(NumericRange::Unrepresentable);
        }
        let weight = factor * cos * cos / (c_n * d_n);
        let value = [weight * (rn - x_n), weight * zn];
        if !value[0].is_finite() || !value[1].is_finite() {
            return Err(NumericRange::Unrepresentable);
        }
        Ok(KernelSample {
            value,
            underflow: charge != 0.0 && factor == 0.0,
        })
    };
    let clamped = frame.radial.clamp(-radius, radius);
    let seed = (clamped / radius).asin();
    let guard = DiscGuard {
        radius,
        radial: frame.radial,
        axial: frame.axial,
        softening,
    };
    let mut budget = Budget::new(work_remaining, cells_per_family, max_depth, target);
    let partition = quadrature::integrate(
        (-0.5 * PI, 0.5 * PI),
        &[seed],
        &guard,
        &mut kernel,
        &mut budget,
        scratch,
        cancelled,
    )?;
    from_partition(partition, frame.radial_direction, frame.unit_axis)
}

#[allow(clippy::too_many_arguments)]
fn sphere(
    point: DVec3,
    source: &Source,
    charge: f64,
    radius: f64,
    softening: f64,
    target: f64,
    work_remaining: u64,
    cells_per_family: usize,
    max_depth: u8,
    scratch: &mut Vec<Cell>,
    cancelled: &dyn Fn() -> bool,
) -> Result<ElectricResult, Failure> {
    let displacement = point - source.center;
    if !displacement.is_finite() {
        return Err(Failure::NumericRange);
    }
    let distance = stable_norm(displacement).map_err(numeric_failure)?;
    if distance == 0.0 {
        return Ok((
            FieldContribution {
                value: DVec3::ZERO,
                allowance: 0.0,
                underflow: false,
            },
            1,
            0,
            None,
        ));
    }
    let direction = displacement / distance;
    if !direction.is_finite() {
        return Err(Failure::NumericRange);
    }
    let scale = distance.max(radius).max(softening);
    if !scale.is_finite() || scale <= 0.0 {
        return Err(Failure::NumericRange);
    }
    let sn = distance / scale;
    let rn = radius / scale;
    let an = softening / scale;
    let a_n = sn * sn + rn * rn + an * an;
    let b_n = 2.0 * sn * rn;
    let factor = 3.0 * charge / (2.0 * radius * scale);
    if !a_n.is_finite() || a_n <= 0.0 || !factor.is_finite() {
        return Err(Failure::NumericRange);
    }
    let mut kernel = |u: f64| -> Result<KernelSample, NumericRange> {
        if !u.is_finite() || !(0.0..=1.0).contains(&u) {
            return Err(NumericRange::Unrepresentable);
        }
        // d_minus^2 = (s - R)^2 + a^2 + 2sR(1 - u). Forming this directly keeps
        // the small near-surface term away from the O(s^2) subtraction in
        // A - B u, which otherwise destroys it when a is tiny. d_plus has no
        // cancellation, so its A + B u form stays stable.
        let difference = sn - rn;
        let d_minus = (difference * difference + an * an + 2.0 * sn * rn * (1.0 - u)).sqrt();
        let d_plus = (a_n + b_n * u).sqrt();
        if !d_minus.is_finite() || !d_plus.is_finite() || d_minus <= 0.0 || d_plus <= 0.0 {
            return Err(NumericRange::Unrepresentable);
        }
        let denominator = d_minus * d_plus * (d_minus + d_plus);
        if !denominator.is_finite() || denominator <= 0.0 {
            return Err(NumericRange::Unrepresentable);
        }
        let value = factor * 2.0 * b_n * u * u / denominator;
        if !value.is_finite() {
            return Err(NumericRange::Unrepresentable);
        }
        Ok(KernelSample {
            value: [value, 0.0],
            underflow: charge != 0.0 && factor == 0.0,
        })
    };
    let guard = SphereGuard {
        distance,
        radius,
        softening,
    };
    let mut budget = Budget::new(work_remaining, cells_per_family, max_depth, target);
    let partition = quadrature::integrate(
        (0.0, 1.0),
        &[],
        &guard,
        &mut kernel,
        &mut budget,
        scratch,
        cancelled,
    )?;
    let (local, work, cells, stop) = from_partition(partition, direction, DVec3::ZERO)?;
    Ok((local, work, cells, stop))
}

fn reconstruct(
    radial_direction: DVec3,
    unit_axis: DVec3,
    radial: f64,
    axial: f64,
    underflow: bool,
) -> Result<FieldContribution, Failure> {
    let world = radial_direction * radial + unit_axis * axial;
    if !world.is_finite() {
        return Err(Failure::NumericRange);
    }
    let allowance = allowance_for(world, underflow).map_err(numeric_failure)?;
    Ok(FieldContribution {
        value: world,
        allowance,
        underflow,
    })
}

fn from_partition(
    partition: quadrature::Partition,
    radial_direction: DVec3,
    unit_axis: DVec3,
) -> Result<(FieldContribution, u64, usize, Option<Failure>), Failure> {
    let world = radial_direction * partition.value[0] + unit_axis * partition.value[1];
    if !world.is_finite() {
        return Err(Failure::NumericRange);
    }
    let norm = stable_norm(world).map_err(numeric_failure)?;
    let allowance = partition.uncertainty + 64.0 * f64::EPSILON * norm;
    if !allowance.is_finite() {
        return Err(Failure::NumericRange);
    }
    Ok((
        FieldContribution {
            value: world,
            allowance,
            underflow: partition.underflow,
        },
        partition.work_used,
        partition.cells_high_water,
        partition.stop,
    ))
}

struct LineGuard {
    radial: f64,
    axial: f64,
    softening: f64,
}

impl ResolutionGuard for LineGuard {
    fn resolved(&self, lo: f64, hi: f64) -> bool {
        let width = hi - lo;
        if width <= 0.0 || !width.is_finite() {
            return false;
        }
        let distance = if self.axial < lo {
            lo - self.axial
        } else if self.axial > hi {
            self.axial - hi
        } else {
            0.0
        };
        let minimum_distance = self.radial.hypot(distance).hypot(self.softening);
        width <= 0.5 * minimum_distance
    }
}

struct RingGuard {
    radius: f64,
    radial: f64,
    axial: f64,
    softening: f64,
}

impl ResolutionGuard for RingGuard {
    fn resolved(&self, lo: f64, hi: f64) -> bool {
        let width = self.radius * (hi - lo);
        if width <= 0.0 || !width.is_finite() {
            return false;
        }
        let radial_gap = (self.radial.abs() - self.radius).max(0.0);
        let minimum_distance = radial_gap.hypot(self.axial).hypot(self.softening);
        width <= 0.5 * minimum_distance
    }
}

struct DiscGuard {
    radius: f64,
    radial: f64,
    axial: f64,
    softening: f64,
}

impl ResolutionGuard for DiscGuard {
    fn resolved(&self, lo: f64, hi: f64) -> bool {
        let width = self.radius * (hi - lo);
        if width <= 0.0 || !width.is_finite() {
            return false;
        }
        let left = self.radius * lo.sin();
        let right = self.radius * hi.sin();
        let (low, high) = if left <= right {
            (left, right)
        } else {
            (right, left)
        };
        let distance = if self.radial < low {
            low - self.radial
        } else if self.radial > high {
            self.radial - high
        } else {
            0.0
        };
        let minimum_distance = self.softening.hypot(self.axial).hypot(distance);
        width <= 0.5 * minimum_distance
    }
}

struct SphereGuard {
    distance: f64,
    radius: f64,
    softening: f64,
}

impl ResolutionGuard for SphereGuard {
    fn resolved(&self, lo: f64, hi: f64) -> bool {
        if !(0.0..=1.0).contains(&lo) || !(0.0..=1.0).contains(&hi) || hi <= lo {
            return false;
        }
        // Surface arc width for the cos(theta) interval.
        let width = self.radius * (lo.acos() - hi.acos());
        if width <= 0.0 || !width.is_finite() {
            return false;
        }
        // d_minus^2 decreases with u, so the minimum softened distance over
        // [lo, hi] is at hi. Use the cancellation-safe normalized form
        // (s - R)^2 + a^2 + 2sR(1 - hi), matching the kernel.
        let scale = self.distance.max(self.radius).max(self.softening);
        if !scale.is_finite() || scale <= 0.0 {
            return false;
        }
        let sn = self.distance / scale;
        let rn = self.radius / scale;
        let an = self.softening / scale;
        let difference = sn - rn;
        let squared = difference * difference + an * an + 2.0 * sn * rn * (1.0 - hi);
        if !squared.is_finite() || squared <= 0.0 {
            return false;
        }
        width <= 0.5 * squared.sqrt() * scale
    }
}
