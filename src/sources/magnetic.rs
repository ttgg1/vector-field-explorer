//! Stable magnetic geometry reductions.
//!
//! The infinite wire keeps its exact azimuthal world reconstruction. The
//! current loop reuses the ring's near-axis binomial moments above the shared
//! direct angular kernel. The finite continuous solenoid integrates its loop
//! measure analytically in the axial direction and then evaluates one bounded
//! angular integral with rationalized endpoint differences.

use std::f64::consts::PI;

use glam::DVec3;

use super::model::{Source, SourceKind};
use super::numeric::{
    FieldContribution, NumericRange, allowance_for, local_frame, ring_series, stable_norm,
    wire_field,
};
use super::quadrature::{self, Budget, Cell, Failure, KernelSample, ResolutionGuard};

/// One complete per-source magnetic contribution with counted work and an
/// optional refinement stop reason.
pub(crate) type MagneticResult = (FieldContribution, u64, usize, Option<Failure>);

/// Evaluate one magnetic source at `point`.
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
) -> Result<MagneticResult, Failure> {
    match source.kind {
        SourceKind::StraightWire { current } => {
            let contribution = wire_field(point, source.center, source.axis, softening, current)
                .map_err(numeric_failure)?;
            Ok((contribution, 1, 0, None))
        }
        SourceKind::CurrentLoop { current, radius } => current_loop(
            point,
            source,
            current,
            radius,
            softening,
            target,
            work_remaining,
            cells_per_family,
            max_depth,
            scratch,
            cancelled,
        ),
        SourceKind::Solenoid {
            current,
            radius,
            length,
            turns,
        } => solenoid(
            point,
            source,
            current,
            radius,
            length,
            turns,
            softening,
            target,
            work_remaining,
            cells_per_family,
            max_depth,
            scratch,
            cancelled,
        ),
        _ => Err(Failure::UnresolvedFeature),
    }
}

fn numeric_failure(_: NumericRange) -> Failure {
    Failure::NumericRange
}

#[allow(clippy::too_many_arguments)]
fn current_loop(
    point: DVec3,
    source: &Source,
    current: f64,
    radius: f64,
    softening: f64,
    target: f64,
    work_remaining: u64,
    cells_per_family: usize,
    max_depth: u8,
    scratch: &mut Vec<Cell>,
    cancelled: &dyn Fn() -> bool,
) -> Result<MagneticResult, Failure> {
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
    let inverse_scale = 1.0 / scale;
    let physical = current * inverse_scale;
    let base = 2.0 * PI * physical * rn_radius / (a_n * a_n.sqrt());
    if !base.is_finite() {
        return Err(Failure::NumericRange);
    }

    if delta <= 0.125 {
        let series = ring_series(
            delta,
            base.abs() * (zn.abs() + rn_radius + rn.abs()),
            0.25 * target,
            work_remaining,
            cancelled,
        )?;
        let radial = base * zn * series.s1;
        let axial = base * (rn_radius * series.s0 - rn * series.s1);
        let tail = base.abs()
            * (zn.abs() * series.tail1 + rn_radius * series.tail0 + rn.abs() * series.tail1);
        let underflow = current != 0.0
            && (base == 0.0
                || (radial == 0.0 && zn * series.s1 != 0.0)
                || (axial == 0.0 && (rn_radius * series.s0 - rn * series.s1) != 0.0));
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

    // The series base already carries the 2 pi of the angular average; the
    // direct kernel integrates over the full symmetric angle.
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
            kernel_factor * zn * cos * weight,
            kernel_factor * (rn_radius - rn * cos) * weight,
        ];
        if !value[0].is_finite() || !value[1].is_finite() {
            return Err(NumericRange::Unrepresentable);
        }
        Ok(KernelSample {
            value,
            underflow: current != 0.0 && kernel_factor == 0.0,
        })
    };
    let guard = LoopGuard {
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

#[allow(clippy::too_many_arguments)]
fn solenoid(
    point: DVec3,
    source: &Source,
    current: f64,
    radius: f64,
    length: f64,
    turns: u32,
    softening: f64,
    target: f64,
    work_remaining: u64,
    cells_per_family: usize,
    max_depth: u8,
    scratch: &mut Vec<Cell>,
    cancelled: &dyn Fn() -> bool,
) -> Result<MagneticResult, Failure> {
    let frame = local_frame(point, source.center, source.axis).map_err(numeric_failure)?;
    let scale = frame
        .axial
        .abs()
        .max(frame.radial)
        .max(radius)
        .max(length)
        .max(softening);
    if !scale.is_finite() || scale <= 0.0 {
        return Err(Failure::NumericRange);
    }
    let rn = frame.radial / scale;
    let zn = frame.axial / scale;
    let rn_radius = radius / scale;
    let ln = length / scale;
    let an = softening / scale;
    let factor = (current / scale) * f64::from(turns);
    if !factor.is_finite() || !rn.is_finite() || !zn.is_finite() || !ln.is_finite() || ln <= 0.0 {
        return Err(Failure::NumericRange);
    }
    let un = zn + 0.5 * ln;
    let vn = zn - 0.5 * ln;
    if !un.is_finite() || !vn.is_finite() {
        return Err(Failure::NumericRange);
    }

    if frame.radial == 0.0 {
        let c_n = rn_radius * rn_radius + an * an;
        let dun = (c_n + un * un).sqrt();
        let dvn = (c_n + vn * vn).sqrt();
        if !c_n.is_finite()
            || c_n <= 0.0
            || !dun.is_finite()
            || !dvn.is_finite()
            || dun <= 0.0
            || dvn <= 0.0
        {
            return Err(Failure::NumericRange);
        }
        let endpoint = axial_endpoint_difference(un, vn, dun, dvn, ln, zn, c_n);
        let axial = 2.0 * PI * factor * (rn_radius / ln) * (rn_radius / c_n) * endpoint;
        if !axial.is_finite() {
            return Err(Failure::NumericRange);
        }
        let value = frame.unit_axis * axial;
        let underflow = current != 0.0 && (factor == 0.0 || (axial == 0.0 && endpoint != 0.0));
        return Ok((
            FieldContribution {
                value,
                allowance: allowance_for(value, underflow).map_err(numeric_failure)?,
                underflow,
            },
            1,
            0,
            None,
        ));
    }

    let same_sign = (un > 0.0 && vn > 0.0) || (un < 0.0 && vn < 0.0);
    let shape_factor = factor * (rn_radius / ln);
    let mut kernel = |phi: f64| -> Result<KernelSample, NumericRange> {
        if !phi.is_finite() {
            return Err(NumericRange::Unrepresentable);
        }
        let (sin_half, _) = (0.5 * phi).sin_cos();
        let cos = phi.cos();
        let sin_half_squared = sin_half * sin_half;
        let c_n =
            (rn - rn_radius) * (rn - rn_radius) + 4.0 * rn * rn_radius * sin_half_squared + an * an;
        if !c_n.is_finite() || c_n <= 0.0 {
            return Err(NumericRange::Unrepresentable);
        }
        let dun = (c_n + un * un).sqrt();
        let dvn = (c_n + vn * vn).sqrt();
        if !dun.is_finite() || !dvn.is_finite() || dun <= 0.0 || dvn <= 0.0 {
            return Err(NumericRange::Unrepresentable);
        }
        // 1/dvn - 1/dun. Opposite endpoint signs are a stable sum; same-sign
        // endpoints cancel and are rationalized.
        let radial_bracket = if same_sign {
            let denominator = dun * dvn * (dun + dvn);
            if !denominator.is_finite() || denominator <= 0.0 {
                return Err(NumericRange::Unrepresentable);
            }
            2.0 * zn * ln / denominator
        } else {
            1.0 / dvn - 1.0 / dun
        };
        // un/dun - vn/dvn. Same-sign endpoints cancel; rationalize them.
        let axial_bracket = if same_sign {
            let denominator = dun * dvn * (un * dvn + vn * dun);
            if !denominator.is_finite() || denominator == 0.0 {
                return Err(NumericRange::Unrepresentable);
            }
            2.0 * zn * ln * c_n / denominator
        } else {
            un / dun - vn / dvn
        };
        // (R - r cos phi) / C = ((R - r) + 2 r sin^2(phi/2)) / C.
        let shape = ((rn_radius - rn) + 2.0 * rn * sin_half_squared) / c_n;
        let value = [
            shape_factor * cos * radial_bracket,
            shape_factor * shape * axial_bracket,
        ];
        if !value[0].is_finite() || !value[1].is_finite() {
            return Err(NumericRange::Unrepresentable);
        }
        Ok(KernelSample {
            value,
            underflow: current != 0.0 && factor == 0.0,
        })
    };
    let guard = SolenoidGuard {
        radius,
        radial: frame.radial,
        axial: frame.axial,
        length,
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

/// Stable `u/sqrt(C+u^2) - v/sqrt(C+v^2)` for the on-axis solenoid field.
fn axial_endpoint_difference(
    un: f64,
    vn: f64,
    dun: f64,
    dvn: f64,
    ln: f64,
    zn: f64,
    c_n: f64,
) -> f64 {
    let same_sign = (un > 0.0 && vn > 0.0) || (un < 0.0 && vn < 0.0);
    if same_sign {
        let denominator = dun * dvn * (un * dvn + vn * dun);
        if denominator.is_finite() && denominator != 0.0 {
            return 2.0 * zn * ln * c_n / denominator;
        }
    }
    un / dun - vn / dvn
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
) -> Result<MagneticResult, Failure> {
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

struct LoopGuard {
    radius: f64,
    radial: f64,
    axial: f64,
    softening: f64,
}

impl ResolutionGuard for LoopGuard {
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

struct SolenoidGuard {
    radius: f64,
    radial: f64,
    axial: f64,
    length: f64,
    softening: f64,
}

impl ResolutionGuard for SolenoidGuard {
    fn resolved(&self, lo: f64, hi: f64) -> bool {
        let width = self.radius * (hi - lo);
        if width <= 0.0 || !width.is_finite() {
            return false;
        }
        // Conservative distance to the cylindrical strip over the whole axial
        // extent: radial gap to the cylinder plus axial gap to the segment.
        let radial_gap = (self.radial.abs() - self.radius).max(0.0);
        let half_length = 0.5 * self.length;
        let axial_gap = if self.axial < -half_length {
            -half_length - self.axial
        } else if self.axial > half_length {
            self.axial - half_length
        } else {
            0.0
        };
        let minimum_distance = radial_gap.hypot(axial_gap).hypot(self.softening);
        width <= 0.5 * minimum_distance
    }
}
