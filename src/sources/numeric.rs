//! Private scaled arithmetic for finite elementary fields.
//!
//! Intermediate values are carried as a mantissa/exponent pair so that
//! representable answers survive overflowing squared/cubed magnitudes and
//! genuine underflow is distinguished from an unrepresentable transformation.

use glam::DVec3;

use super::quadrature::KernelSample;

/// A numerical transformation that cannot be represented in binary64.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NumericRange {
    Unrepresentable,
}

/// One elementary contribution with a finite absolute arithmetic allowance.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FieldContribution {
    pub value: DVec3,
    pub allowance: f64,
    pub underflow: bool,
}

impl FieldContribution {
    fn zero() -> Self {
        Self {
            value: DVec3::ZERO,
            allowance: 0.0,
            underflow: false,
        }
    }
}

/// Stable Euclidean norm using max-component scaling.
pub(crate) fn stable_norm(value: DVec3) -> Result<f64, NumericRange> {
    if !value.is_finite() {
        return Err(NumericRange::Unrepresentable);
    }
    let scale = value.abs().max_element();
    if scale == 0.0 {
        return Ok(0.0);
    }
    let scaled = value / scale;
    let norm = scale * scaled.dot(scaled).sqrt();
    if norm.is_finite() {
        Ok(norm)
    } else {
        Err(NumericRange::Unrepresentable)
    }
}

/// Normalize an axis by max-component scaling, never by a squared-norm cutoff.
pub(crate) fn normalize_axis(axis: DVec3) -> Result<DVec3, NumericRange> {
    if !axis.is_finite() {
        return Err(NumericRange::Unrepresentable);
    }
    let scale = axis.abs().max_element();
    if scale == 0.0 {
        return Err(NumericRange::Unrepresentable);
    }
    let scaled = axis / scale;
    let norm = stable_norm(scaled)?;
    if norm == 0.0 {
        return Err(NumericRange::Unrepresentable);
    }
    Ok(scaled / norm)
}

/// Source-local cylindrical frame: `radial` is the perpendicular distance to
/// `axis`, `axial` is the projection onto the unit axis, and
/// `radial_direction` is the unit world direction (zero exactly on axis).
#[derive(Debug, Clone, Copy)]
pub(crate) struct LocalFrame {
    pub radial: f64,
    pub axial: f64,
    pub radial_direction: DVec3,
    pub unit_axis: DVec3,
}

/// Project a world displacement into a source-local cylindrical frame.
pub(crate) fn local_frame(
    point: DVec3,
    center: DVec3,
    axis: DVec3,
) -> Result<LocalFrame, NumericRange> {
    let displacement = point - center;
    if !displacement.is_finite() {
        return Err(NumericRange::Unrepresentable);
    }
    let unit_axis = normalize_axis(axis)?;
    let scale = displacement.abs().max_element();
    let axial = if scale == 0.0 {
        0.0
    } else {
        (displacement / scale).dot(unit_axis) * scale
    };
    if !axial.is_finite() {
        return Err(NumericRange::Unrepresentable);
    }
    let radial_vector = displacement - unit_axis * axial;
    if !radial_vector.is_finite() {
        return Err(NumericRange::Unrepresentable);
    }
    let radial = stable_norm(radial_vector)?;
    let radial_direction = if radial == 0.0 {
        DVec3::ZERO
    } else {
        radial_vector / radial
    };
    Ok(LocalFrame {
        radial,
        axial,
        radial_direction,
        unit_axis,
    })
}

/// Softened point kernel `Q * s / (|s|^2 + a^2)^(3/2)`.
pub(crate) fn point_field(
    displacement: DVec3,
    softening: f64,
    strength: f64,
) -> Result<FieldContribution, NumericRange> {
    if !displacement.is_finite() {
        return Err(NumericRange::Unrepresentable);
    }
    if strength == 0.0 || displacement == DVec3::ZERO {
        return Ok(FieldContribution::zero());
    }
    let norm = stable_norm(displacement)?;
    if norm == 0.0 {
        return Ok(FieldContribution::zero());
    }
    let norm_scaled = Scaled::from_f64(norm).ok_or(NumericRange::Unrepresentable)?;
    let softening_scaled = Scaled::from_f64(softening).ok_or(NumericRange::Unrepresentable)?;
    let distance_squared = norm_scaled
        .mul(norm_scaled)
        .add(softening_scaled.mul(softening_scaled));
    let distance = distance_squared.sqrt();
    let distance_cubed = distance.mul(distance).mul(distance);
    let strength_scaled = Scaled::from_f64(strength).ok_or(NumericRange::Unrepresentable)?;
    let coefficient = strength_scaled.div(distance_cubed);

    let mut value = [0.0; 3];
    let mut underflow = false;
    for (index, component) in displacement.to_array().into_iter().enumerate() {
        if component == 0.0 {
            continue;
        }
        let component_scaled = Scaled::from_f64(component).ok_or(NumericRange::Unrepresentable)?;
        let (result, component_underflow) = component_scaled
            .mul(coefficient)
            .to_f64()
            .ok_or(NumericRange::Unrepresentable)?;
        value[index] = result;
        underflow |= component_underflow;
    }
    let value = DVec3::from_array(value);
    Ok(FieldContribution {
        value,
        allowance: allowance_for(value, underflow)?,
        underflow,
    })
}

/// Exact infinite-wire field `2I (axis x s_perp) / (|s_perp|^2 + a^2)`.
pub(crate) fn wire_field(
    point: DVec3,
    center: DVec3,
    axis: DVec3,
    softening: f64,
    current: f64,
) -> Result<FieldContribution, NumericRange> {
    let displacement = point - center;
    if !displacement.is_finite() {
        return Err(NumericRange::Unrepresentable);
    }
    if current == 0.0 || displacement == DVec3::ZERO {
        return Ok(FieldContribution::zero());
    }
    let unit_axis = normalize_axis(axis)?;

    // Project the displacement with max scaling so |dot| cannot overflow while
    // the actual parallel component is finite.
    let scale = displacement.abs().max_element();
    if scale == 0.0 {
        return Ok(FieldContribution::zero());
    }
    let parallel = (displacement / scale).dot(unit_axis) * scale;
    if !parallel.is_finite() {
        return Err(NumericRange::Unrepresentable);
    }
    let perpendicular = displacement - unit_axis * parallel;
    if !perpendicular.is_finite() {
        return Err(NumericRange::Unrepresentable);
    }
    if perpendicular == DVec3::ZERO {
        // The azimuthal basis is undefined exactly on the wire axis.
        return Ok(FieldContribution::zero());
    }

    let radius = stable_norm(perpendicular)?;
    // Subtracting two near-equal huge vectors loses the perpendicular to
    // rounding at the scale of the displacement itself. Refuse rather than
    // fabricate a direction or magnitude from that cancellation.
    if radius <= 64.0 * f64::EPSILON * scale {
        return Err(NumericRange::Unrepresentable);
    }
    if radius == 0.0 {
        return Ok(FieldContribution::zero());
    }
    let radius_scaled = Scaled::from_f64(radius).ok_or(NumericRange::Unrepresentable)?;
    let softening_scaled = Scaled::from_f64(softening).ok_or(NumericRange::Unrepresentable)?;
    let denominator = radius_scaled
        .mul(radius_scaled)
        .add(softening_scaled.mul(softening_scaled));
    let two = Scaled::from_f64(2.0).ok_or(NumericRange::Unrepresentable)?;
    let current_scaled = Scaled::from_f64(current).ok_or(NumericRange::Unrepresentable)?;
    let factor = two.mul(current_scaled).div(denominator);

    let cross_scale = perpendicular.abs().max_element();
    if cross_scale == 0.0 {
        return Ok(FieldContribution::zero());
    }
    let cross_direction = unit_axis.cross(perpendicular / cross_scale);
    if !cross_direction.is_finite() {
        return Err(NumericRange::Unrepresentable);
    }
    let cross_scale_scaled = Scaled::from_f64(cross_scale).ok_or(NumericRange::Unrepresentable)?;
    let cross_factor = cross_scale_scaled.mul(factor);
    let mut value = [0.0; 3];
    let mut underflow = false;
    for (index, component) in cross_direction.to_array().into_iter().enumerate() {
        if component == 0.0 {
            continue;
        }
        let component_scaled = Scaled::from_f64(component).ok_or(NumericRange::Unrepresentable)?;
        let (result, component_underflow) = component_scaled
            .mul(cross_factor)
            .to_f64()
            .ok_or(NumericRange::Unrepresentable)?;
        value[index] = result;
        underflow |= component_underflow;
    }
    let value = DVec3::from_array(value);
    Ok(FieldContribution {
        value,
        allowance: allowance_for(value, underflow)?,
        underflow,
    })
}

pub(crate) fn allowance_for(value: DVec3, underflow: bool) -> Result<f64, NumericRange> {
    let norm = stable_norm(value)?;
    let mut allowance = 64.0 * f64::EPSILON * norm;
    if underflow {
        allowance += 64.0 * f64::from_bits(1);
    }
    if allowance.is_finite() {
        Ok(allowance)
    } else {
        Err(NumericRange::Unrepresentable)
    }
}

/// Binary64 value represented as `mantissa * 2^exponent`.
#[derive(Debug, Clone, Copy)]
struct Scaled {
    mantissa: f64,
    exponent: i32,
}

impl Scaled {
    fn from_f64(value: f64) -> Option<Self> {
        if value == 0.0 || !value.is_finite() {
            return None;
        }
        let sign = if value.is_sign_negative() { -1.0 } else { 1.0 };
        let mut magnitude = value.abs();
        let mut adjustment = 0i32;
        if magnitude < f64::MIN_POSITIVE {
            magnitude *= 2f64.powi(54);
            adjustment = -54;
        }
        let bits = magnitude.to_bits();
        let exponent_bits = ((bits >> 52) & 0x7ff) as i32;
        if exponent_bits == 0 {
            return None;
        }
        let fraction = bits & ((1u64 << 52) - 1);
        let mantissa = f64::from_bits((1023u64 << 52) | fraction);
        Some(Self {
            mantissa: sign * mantissa,
            exponent: exponent_bits - 1023 + adjustment,
        })
    }

    fn normalize(mut self) -> Self {
        while self.mantissa.abs() >= 2.0 {
            self.mantissa *= 0.5;
            self.exponent += 1;
        }
        while self.mantissa != 0.0 && self.mantissa.abs() < 1.0 {
            self.mantissa *= 2.0;
            self.exponent -= 1;
        }
        self
    }

    fn mul(self, other: Self) -> Self {
        Self {
            mantissa: self.mantissa * other.mantissa,
            exponent: self.exponent + other.exponent,
        }
        .normalize()
    }

    fn div(self, other: Self) -> Self {
        Self {
            mantissa: self.mantissa / other.mantissa,
            exponent: self.exponent - other.exponent,
        }
        .normalize()
    }

    fn add(self, other: Self) -> Self {
        if self.mantissa == 0.0 {
            return other;
        }
        if other.mantissa == 0.0 {
            return self;
        }
        let (larger, smaller) = if self.exponent >= other.exponent {
            (self, other)
        } else {
            (other, self)
        };
        let difference = larger.exponent - smaller.exponent;
        if difference > 60 {
            return larger;
        }
        let Some(factor) = pow2(-difference) else {
            return larger;
        };
        Self {
            mantissa: larger.mantissa + smaller.mantissa * factor,
            exponent: larger.exponent,
        }
        .normalize()
    }

    fn sqrt(self) -> Self {
        let (mantissa, exponent) = if self.exponent % 2 == 0 {
            (self.mantissa.sqrt(), self.exponent / 2)
        } else {
            ((2.0 * self.mantissa).sqrt(), (self.exponent - 1) / 2)
        };
        Self { mantissa, exponent }.normalize()
    }

    fn to_f64(self) -> Option<(f64, bool)> {
        let scaled = self.normalize();
        if scaled.mantissa == 0.0 {
            return Some((0.0, false));
        }
        if scaled.exponent > 1023 {
            return None;
        }
        if scaled.exponent >= -1074 {
            let factor = pow2(scaled.exponent)?;
            let value = scaled.mantissa * factor;
            if !value.is_finite() {
                return None;
            }
            let underflow = value == 0.0 || (value.abs() < f64::MIN_POSITIVE && value != 0.0);
            return Some((value, underflow));
        }
        // The true result lies below the smallest subnormal. Keep the correctly
        // rounded zero or minimum subnormal and report genuine underflow.
        let shift = -1074 - scaled.exponent;
        if shift > 1022 {
            return Some((0.0, true));
        }
        let factor = pow2(-shift)?;
        let value = (scaled.mantissa * factor) * f64::from_bits(1);
        if !value.is_finite() {
            return None;
        }
        Some((value, true))
    }
}

fn pow2(exponent: i32) -> Option<f64> {
    if (-1022..=1023).contains(&exponent) {
        Some(f64::from_bits(((exponent + 1023) as u64) << 52))
    } else if (-1074..-1022).contains(&exponent) {
        Some(f64::from_bits(1u64 << (exponent + 1074)))
    } else if exponent > 1023 {
        None
    } else {
        Some(0.0)
    }
}

/// Source-local finite-line geometry and its direct segment kernel.
///
/// The line is centred on `center`. The kernel integrates
/// `(Q/L) * (r, z - t) / (r^2 + (z - t)^2 + a^2)^(3/2)` over the segment in
/// source-local cylindrical coordinates.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FiniteLineGeometry {
    pub radial: f64,
    pub axial: f64,
    pub radial_direction: DVec3,
    pub unit_axis: DVec3,
    softening: f64,
    factor: f64,
    factor_underflow: bool,
}

impl FiniteLineGeometry {
    pub(crate) fn new(
        point: DVec3,
        center: DVec3,
        axis: DVec3,
        softening: f64,
        charge: f64,
        length: f64,
    ) -> Result<Self, NumericRange> {
        let displacement = point - center;
        if !displacement.is_finite() {
            return Err(NumericRange::Unrepresentable);
        }
        let unit_axis = normalize_axis(axis)?;
        let scale = displacement.abs().max_element();
        let axial = if scale == 0.0 {
            0.0
        } else {
            (displacement / scale).dot(unit_axis) * scale
        };
        if !axial.is_finite() {
            return Err(NumericRange::Unrepresentable);
        }
        let radial_vector = displacement - unit_axis * axial;
        if !radial_vector.is_finite() {
            return Err(NumericRange::Unrepresentable);
        }
        let radial = stable_norm(radial_vector)?;
        let radial_direction = if radial == 0.0 {
            DVec3::ZERO
        } else {
            radial_vector / radial
        };
        let factor = charge / length;
        if !factor.is_finite() {
            return Err(NumericRange::Unrepresentable);
        }
        Ok(Self {
            radial,
            axial,
            radial_direction,
            unit_axis,
            softening,
            factor,
            factor_underflow: factor == 0.0 && charge != 0.0,
        })
    }

    /// One source-local segment sample, already scaled by `Q/L`.
    pub(crate) fn sample(&self, parameter: f64) -> Result<KernelSample, NumericRange> {
        if !parameter.is_finite() {
            return Err(NumericRange::Unrepresentable);
        }
        let axial_delta = self.axial - parameter;
        if !axial_delta.is_finite() {
            return Err(NumericRange::Unrepresentable);
        }
        let scale = self.radial.abs().max(axial_delta.abs()).max(self.softening);
        if scale <= 0.0 || !scale.is_finite() {
            return Err(NumericRange::Unrepresentable);
        }
        let radial_scaled = self.radial / scale;
        let axial_scaled = axial_delta / scale;
        let softening_scaled = self.softening / scale;
        let distance_squared = radial_scaled * radial_scaled
            + axial_scaled * axial_scaled
            + softening_scaled * softening_scaled;
        let distance = distance_squared.sqrt();
        let inverse_scale = 1.0 / scale;
        let coefficient =
            self.factor * inverse_scale * inverse_scale / (distance_squared * distance);
        if !coefficient.is_finite() {
            return Err(NumericRange::Unrepresentable);
        }
        let value = [radial_scaled * coefficient, axial_scaled * coefficient];
        if !value[0].is_finite() || !value[1].is_finite() {
            return Err(NumericRange::Unrepresentable);
        }
        let underflow = self.factor_underflow
            || (value[0] == 0.0 && radial_scaled != 0.0)
            || (value[1] == 0.0 && axial_scaled != 0.0);
        Ok(KernelSample { value, underflow })
    }
}

/// Analytic finite-line field for an exactly on-axis query.
///
/// Uses the design's stable endpoint primitive. For a same-sign far query the
/// endpoint difference is rationalized before evaluation.
pub(crate) fn finite_line_axial_field(
    point: DVec3,
    center: DVec3,
    axis: DVec3,
    softening: f64,
    charge: f64,
    length: f64,
) -> Result<FieldContribution, NumericRange> {
    let displacement = point - center;
    if !displacement.is_finite() {
        return Err(NumericRange::Unrepresentable);
    }
    if charge == 0.0 {
        return Ok(FieldContribution::zero());
    }
    let unit_axis = normalize_axis(axis)?;
    let scale = displacement.abs().max_element();
    let axial = if scale == 0.0 {
        0.0
    } else {
        (displacement / scale).dot(unit_axis) * scale
    };
    if !axial.is_finite() {
        return Err(NumericRange::Unrepresentable);
    }
    let u = axial + 0.5 * length;
    let v = axial - 0.5 * length;
    if !u.is_finite() || !v.is_finite() {
        return Err(NumericRange::Unrepresentable);
    }
    let a = v.hypot(softening);
    let b = u.hypot(softening);
    let factor = charge / length;
    if !factor.is_finite() {
        return Err(NumericRange::Unrepresentable);
    }
    let same_sign = (u > 0.0 && v > 0.0) || (u < 0.0 && v < 0.0);
    let difference = if same_sign {
        2.0 * axial * length / (a * b * (a + b))
    } else {
        1.0 / a - 1.0 / b
    };
    let axial_field = factor * difference;
    if !axial_field.is_finite() {
        return Err(NumericRange::Unrepresentable);
    }
    let value = unit_axis * axial_field;
    if !value.is_finite() {
        return Err(NumericRange::Unrepresentable);
    }
    let underflow = axial_field == 0.0 && factor != 0.0;
    Ok(FieldContribution {
        value,
        allowance: allowance_for(value, underflow)?,
        underflow,
    })
}
