//! CPU-baked field lattice (P3 slice 1a).
//!
//! A time-independent formula field can be evaluated once onto a bounded
//! axis-aligned lattice and then sampled with trilinear interpolation. This
//! module is the **model only**: it builds, validates and samples a
//! [`BakedField`]. No particle, streamline, `advance_exact`, animation or CLI
//! call site switches to it in this slice, so every existing result stays
//! byte-identical. The opt-in wiring belongs to slice 1b.
//!
//! # 2D versus 3D
//!
//! [`BakeAxis::Volume3d`] samples the full world box on `(x, y, z)` and
//! interpolates all three axes. [`BakeAxis::Plane2d`] samples the domain on
//! `(x, y)` with one z layer and ignores the query's z when interpolating,
//! exactly like [`FieldExpr`]'s two-component evaluation (which already fixes
//! `z = 0`). That preserves genuine 2D scene semantics while the 2D domain may
//! still carry a z extent for camera framing.
//!
//! # Accuracy
//!
//! A bake is an approximation. [`BakedField::accuracy_report`] measures the
//! interpolation error against a supplied reference and can measure the
//! divergence residual of a supplied divergence-free field. A number that was
//! not measured stays [`None`]; nothing is fabricated.
//!
//! Built only for [`FieldExpr::is_time_dependent`] `== false`; a time-dependent
//! field is refused rather than silently frozen at `t = 0`.

use std::fmt;

use glam::Vec3;

use crate::domain::Domain;
use crate::field::FieldExpr;
use crate::parameters::ParameterSet;
use crate::plane::PlaneFrame;
use crate::source_scene::FieldMode;

/// The ADR 0005 bound on f32 lattice storage: `3 × 2²⁰` bytes (3 MiB), exactly
/// `64³ × 12` bytes. Build refuses rather than allocating beyond it.
pub const MAX_LATTICE_BYTES: usize = 3_145_728;

/// Smallest accepted resolution along a volume axis.
pub const MIN_VOLUME_RESOLUTION: u32 = 16;
/// Largest accepted resolution along a volume axis.
pub const MAX_VOLUME_RESOLUTION: u32 = 64;
/// Smallest accepted resolution along a plane axis.
pub const MIN_PLANE_RESOLUTION: u32 = 32;
/// Largest accepted resolution along a plane axis.
pub const MAX_PLANE_RESOLUTION: u32 = 256;

/// Which lattice a bake occupies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BakeAxis {
    /// A full `n × n × n` world volume.
    Volume3d,
    /// A genuine 2D field on `n × n` over the domain's `(x, y)`.
    Plane2d,
}

impl BakeAxis {
    fn resolution_bounds(self) -> (u32, u32) {
        match self {
            Self::Volume3d => (MIN_VOLUME_RESOLUTION, MAX_VOLUME_RESOLUTION),
            Self::Plane2d => (MIN_PLANE_RESOLUTION, MAX_PLANE_RESOLUTION),
        }
    }
}

/// A resolution already validated against its axis. Construction never clamps:
/// an out-of-range value returns [`BakeError::Resolution`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BakeResolution {
    axis: BakeAxis,
    resolution: u32,
}

impl BakeResolution {
    /// Validate `resolution` for `axis` and keep it verbatim.
    pub fn new(axis: BakeAxis, resolution: u32) -> Result<Self, BakeError> {
        let (min, max) = axis.resolution_bounds();
        if !(min..=max).contains(&resolution) {
            return Err(BakeError::Resolution {
                axis,
                min,
                max,
                got: resolution,
            });
        }
        Ok(Self { axis, resolution })
    }

    pub fn axis(self) -> BakeAxis {
        self.axis
    }

    pub fn resolution(self) -> u32 {
        self.resolution
    }

    /// Lattice points: `n³` for a volume, `n²` for a plane.
    pub fn points(self) -> usize {
        let n = self.resolution as usize;
        match self.axis {
            BakeAxis::Volume3d => n * n * n,
            BakeAxis::Plane2d => n * n,
        }
    }

    /// Lattice dimensions; a plane keeps one z layer.
    pub fn dims(self) -> [usize; 3] {
        let n = self.resolution as usize;
        match self.axis {
            BakeAxis::Volume3d => [n, n, n],
            BakeAxis::Plane2d => [n, n, 1],
        }
    }
}

/// Everything that selects and identifies one bake.
///
/// Exact [`PartialEq`] is the rebuild rule: any changed field-affecting input
/// is a different spec, never a tolerance match. Simulation time is absent
/// because time-dependent fields are refused.
#[derive(Debug, Clone, PartialEq)]
pub struct BakeSpec {
    /// Formula text as committed.
    pub formula: String,
    /// The full committed parameter set.
    pub parameters: ParameterSet,
    /// The world box the lattice covers.
    pub domain: Domain,
    /// Which lattice shape to allocate.
    pub axis: BakeAxis,
    /// Requested points per axis, validated by [`BakedField::build`].
    pub resolution: u32,
    /// The active field adapter; only Formula fields can be baked today.
    pub field_mode: FieldMode,
    /// The projected plane this bake belongs to, when the scene samples one.
    pub plane_frame: Option<PlaneFrame>,
}

impl BakeSpec {
    /// Checked lattice dimensions. `None` when the unvalidated resolution
    /// would overflow the point count, so an extreme request is refused before
    /// any bound check or allocation.
    pub fn dims(&self) -> Option<[usize; 3]> {
        let n = usize::try_from(self.resolution).ok()?;
        match self.axis {
            BakeAxis::Volume3d => n
                .checked_mul(n)
                .and_then(|squared| squared.checked_mul(n))
                .map(|_| [n, n, n]),
            BakeAxis::Plane2d => n.checked_mul(n).map(|_| [n, n, 1]),
        }
    }
}

/// Why a bake could not be built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BakeError {
    /// The field references `t`; freezing it at any time would be a lie.
    TimeDependent,
    /// The active field adapter is not the formula field this bake samples.
    /// Only [`FieldMode::Formula`] describes the [`FieldExpr`] passed to
    /// [`BakedField::build`].
    UnsupportedMode { mode: FieldMode },
    /// A resolution outside the axis's accepted range.
    Resolution {
        axis: BakeAxis,
        min: u32,
        max: u32,
        got: u32,
    },
    /// The requested lattice would exceed [`MAX_LATTICE_BYTES`].
    Memory { requested_bytes: u64, cap: usize },
    /// The domain has no positive finite extent on an active axis.
    EmptyDomain,
}

impl fmt::Display for BakeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TimeDependent => write!(f, "cannot bake a time-dependent field"),
            Self::UnsupportedMode { mode } => {
                write!(
                    f,
                    "cannot bake the {mode:?} field; only Formula can be baked"
                )
            }
            Self::Resolution {
                axis,
                min,
                max,
                got,
            } => write!(
                f,
                "bake resolution {got} is outside {min}..={max} for {axis:?}"
            ),
            Self::Memory {
                requested_bytes,
                cap,
            } => write!(
                f,
                "bake lattice needs {requested_bytes} bytes, above the {cap}-byte cap"
            ),
            Self::EmptyDomain => write!(f, "bake domain is empty on an active axis"),
        }
    }
}

impl std::error::Error for BakeError {}

/// One measured accuracy statement. Each optional number is present only when
/// the caller supplied the matching probe reference.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BakeAccuracy {
    /// Points per axis of the bake these numbers describe.
    pub resolution: u32,
    /// Maximum `|baked - reference|` at cell centres, when measured.
    pub interpolation_max_error: Option<f32>,
    /// Maximum measured divergence residual of the bake, when a divergence-free
    /// reference was supplied.
    pub divergence_residual: Option<f32>,
}

/// Optional references [`BakedField::accuracy_report`] measures against.
#[derive(Debug, Clone, Copy, Default)]
pub struct BakeAccuracyProbes<'a> {
    /// A reference implementation of the baked formula; cell centres are
    /// compared against it.
    pub interpolation: Option<&'a FieldExpr>,
    /// The same formula when it is divergence-free. This is a
    /// presence/assertion gate, **not** a measurement source: the reference is
    /// never evaluated. Supplying it asserts the property and enables
    /// measuring the baked field's own divergence residual. The caller owns
    /// that assertion.
    pub divergence_free: Option<&'a FieldExpr>,
}

/// A bounded f32 lattice plus the spec that produced it.
///
/// `lattice` is x-major, then y, then z (the same order as
/// [`Domain::grid_points`]). A node whose evaluation was absent or non-finite
/// stores `NaN`; sampling any cell that touches it returns [`None`] rather
/// than propagating NaN into motion.
#[derive(Debug, Clone, PartialEq)]
pub struct BakedField {
    spec: BakeSpec,
    lattice: Vec<Vec3>,
}

impl BakedField {
    /// Evaluate `field` onto the lattice described by `spec`.
    ///
    /// * A spec whose `field_mode` is not [`FieldMode::Formula`] returns
    ///   [`BakeError::UnsupportedMode`]: only the formula field is `field`.
    /// * A time-dependent field returns [`BakeError::TimeDependent`] — never a
    ///   silent freeze.
    /// * The byte budget is checked before the resolution bound, so an
    ///   over-cap request is refused without validating or allocating it.
    /// * A resolution outside `16..=64` (3D) or `32..=256` (2D) returns
    ///   [`BakeError::Resolution`]; values are never clamped.
    /// * A zero or non-finite active extent returns [`BakeError::EmptyDomain`].
    pub fn build(spec: BakeSpec, field: &FieldExpr) -> Result<Self, BakeError> {
        if spec.field_mode != FieldMode::Formula {
            return Err(BakeError::UnsupportedMode {
                mode: spec.field_mode,
            });
        }
        if field.is_time_dependent() {
            return Err(BakeError::TimeDependent);
        }

        let Some(dims) = spec.dims() else {
            return Err(BakeError::Memory {
                requested_bytes: u64::MAX,
                cap: MAX_LATTICE_BYTES,
            });
        };
        let points = dims
            .iter()
            .try_fold(1u64, |points, &side| points.checked_mul(side as u64));
        let requested_bytes = points.and_then(|points| points.checked_mul(12));
        let Some(requested_bytes) = requested_bytes else {
            return Err(BakeError::Memory {
                requested_bytes: u64::MAX,
                cap: MAX_LATTICE_BYTES,
            });
        };
        if requested_bytes > MAX_LATTICE_BYTES as u64 {
            return Err(BakeError::Memory {
                requested_bytes,
                cap: MAX_LATTICE_BYTES,
            });
        }

        let resolution = BakeResolution::new(spec.axis, spec.resolution)?;

        let size = spec.domain.domain_size();
        let active_extent_positive = match spec.axis {
            BakeAxis::Volume3d => size.x > 0.0 && size.y > 0.0 && size.z > 0.0,
            BakeAxis::Plane2d => size.x > 0.0 && size.y > 0.0,
        };
        if !size.is_finite() || !active_extent_positive {
            return Err(BakeError::EmptyDomain);
        }

        let mut lattice = Vec::with_capacity(resolution.points());
        for point in spec.domain.grid_points(glam::UVec3::new(
            dims[0] as u32,
            dims[1] as u32,
            dims[2] as u32,
        )) {
            let value = field
                .eval3(point)
                .filter(|value| value.is_finite())
                .unwrap_or(Vec3::splat(f32::NAN));
            lattice.push(value);
        }
        // Keep the validated value, not the unchecked request.
        let mut spec = spec;
        spec.resolution = resolution.resolution();
        Ok(Self { spec, lattice })
    }

    pub fn spec(&self) -> &BakeSpec {
        &self.spec
    }

    /// The raw lattice, x-major then y then z. Exposed for determinism checks
    /// and the slice 1b GPU upload; it is not a sampling API.
    pub fn lattice(&self) -> &[Vec3] {
        &self.lattice
    }

    pub fn axis(&self) -> BakeAxis {
        self.spec.axis
    }

    pub fn resolution(&self) -> u32 {
        self.spec.resolution
    }

    fn dims(&self) -> [usize; 3] {
        match self.spec.axis {
            BakeAxis::Volume3d => {
                let n = self.spec.resolution as usize;
                [n, n, n]
            }
            BakeAxis::Plane2d => {
                let n = self.spec.resolution as usize;
                [n, n, 1]
            }
        }
    }

    /// Trilinear (bilinear on [`BakeAxis::Plane2d`]) sample at `point`.
    ///
    /// `None` outside the sampled domain, for non-finite input, and for any
    /// cell with a non-finite corner or non-finite interpolated result. A
    /// plane bake ignores the query's z, matching `FieldExpr`'s 2D components.
    pub fn sample(&self, point: Vec3) -> Option<Vec3> {
        if !point.is_finite() {
            return None;
        }
        let [nx, ny, nz] = self.dims();
        if nx == 0 || ny == 0 || nz == 0 {
            return None;
        }
        let min = self.spec.domain.min;
        let max = self.spec.domain.max;
        let span = max - min;

        if point.x < min.x || point.x > max.x || point.y < min.y || point.y > max.y {
            return None;
        }
        if self.spec.axis == BakeAxis::Volume3d && (point.z < min.z || point.z > max.z) {
            return None;
        }

        let (x0, x1, fx) = axis_weights(point.x, min.x, span.x, nx);
        let (y0, y1, fy) = axis_weights(point.y, min.y, span.y, ny);
        let (z0, z1, fz) = match self.spec.axis {
            BakeAxis::Volume3d => axis_weights(point.z, min.z, span.z, nz),
            BakeAxis::Plane2d => (0, 0, 0.0),
        };

        let corner = |x: usize, y: usize, z: usize| self.lattice[x + nx * (y + ny * z)];
        let c000 = corner(x0, y0, z0);
        let c100 = corner(x1, y0, z0);
        let c010 = corner(x0, y1, z0);
        let c110 = corner(x1, y1, z0);
        let c001 = corner(x0, y0, z1);
        let c101 = corner(x1, y0, z1);
        let c011 = corner(x0, y1, z1);
        let c111 = corner(x1, y1, z1);
        for value in [c000, c100, c010, c110, c001, c101, c011, c111] {
            if !value.is_finite() {
                return None;
            }
        }

        let c00 = c000.lerp(c100, fx);
        let c10 = c010.lerp(c110, fx);
        let c01 = c001.lerp(c101, fx);
        let c11 = c011.lerp(c111, fx);
        let bottom = c00.lerp(c10, fy);
        let top = c01.lerp(c11, fy);
        let value = bottom.lerp(top, fz);
        value.is_finite().then_some(value)
    }

    /// Measure the bake against the supplied references. Anything not supplied
    /// stays [`None`]; the resolution is always reported.
    pub fn accuracy_report(&self, probes: BakeAccuracyProbes<'_>) -> BakeAccuracy {
        BakeAccuracy {
            resolution: self.spec.resolution,
            interpolation_max_error: probes
                .interpolation
                .and_then(|reference| self.interpolation_error(reference)),
            divergence_residual: probes
                .divergence_free
                .and_then(|_| self.divergence_residual()),
        }
    }

    /// Maximum error at cell centres; `None` when no cell could be compared.
    fn interpolation_error(&self, reference: &FieldExpr) -> Option<f32> {
        let [nx, ny, nz] = self.dims();
        let min = self.spec.domain.min;
        let span = self.spec.domain.max - min;
        let cells_x = nx.saturating_sub(1);
        let cells_y = ny.saturating_sub(1);
        let cells_z = match self.spec.axis {
            BakeAxis::Volume3d => nz.saturating_sub(1),
            BakeAxis::Plane2d => 1,
        };
        let mut worst: Option<f32> = None;
        for ix in 0..cells_x {
            for iy in 0..cells_y {
                for iz in 0..cells_z {
                    let x = min.x + span.x * (ix as f32 + 0.5) / cells_x.max(1) as f32;
                    let y = min.y + span.y * (iy as f32 + 0.5) / cells_y.max(1) as f32;
                    let z = match self.spec.axis {
                        BakeAxis::Volume3d => {
                            min.z + span.z * (iz as f32 + 0.5) / cells_z.max(1) as f32
                        }
                        BakeAxis::Plane2d => min.z + span.z * 0.5,
                    };
                    let point = Vec3::new(x, y, z);
                    let (Some(exact), Some(sampled)) = (reference.eval3(point), self.sample(point))
                    else {
                        continue;
                    };
                    let error = (sampled - exact).abs().max_element();
                    if error.is_finite() {
                        worst = Some(worst.map_or(error, |current| current.max(error)));
                    }
                }
            }
        }
        worst
    }

    /// Maximum measured divergence of the baked field at interior lattice
    /// nodes. The caller of [`Self::accuracy_report`] asserts the reference is
    /// divergence-free; an absent measurement stays [`None`].
    fn divergence_residual(&self) -> Option<f32> {
        let [nx, ny, nz] = self.dims();
        let min = self.spec.domain.min;
        let span = self.spec.domain.max - min;
        let h = Vec3::new(
            span.x / (nx.saturating_sub(1)).max(1) as f32,
            span.y / (ny.saturating_sub(1)).max(1) as f32,
            span.z / (nz.saturating_sub(1)).max(1) as f32,
        );
        if !h.is_finite() || h.x <= 0.0 || h.y <= 0.0 {
            return None;
        }
        let z_index = match self.spec.axis {
            BakeAxis::Volume3d => (1..nz.saturating_sub(1)).collect::<Vec<_>>(),
            BakeAxis::Plane2d => vec![0],
        };
        let mut worst: Option<f32> = None;
        for ix in 1..nx.saturating_sub(1) {
            for iy in 1..ny.saturating_sub(1) {
                for &iz in &z_index {
                    let point = Vec3::new(
                        min.x + span.x * ix as f32 / (nx - 1) as f32,
                        min.y + span.y * iy as f32 / (ny - 1) as f32,
                        min.z + span.z * iz as f32 / (nz.max(2) - 1) as f32,
                    );
                    let derivative = |axis: Vec3| {
                        let plus = self.sample(point + axis * h)?;
                        let minus = self.sample(point - axis * h)?;
                        Some((plus - minus) * 0.5)
                    };
                    let Some(du_dx) = derivative(Vec3::X).map(|d| d.x / h.x) else {
                        continue;
                    };
                    let Some(dv_dy) = derivative(Vec3::Y).map(|d| d.y / h.y) else {
                        continue;
                    };
                    let dw_dz = match self.spec.axis {
                        BakeAxis::Volume3d => {
                            let Some(value) = derivative(Vec3::Z).map(|d| d.z / h.z) else {
                                continue;
                            };
                            value
                        }
                        BakeAxis::Plane2d => 0.0,
                    };
                    let divergence = (du_dx + dv_dy + dw_dz).abs();
                    if divergence.is_finite() {
                        worst = Some(worst.map_or(divergence, |current| current.max(divergence)));
                    }
                }
            }
        }
        worst
    }
}

/// Grid index pair and fractional weight for one axis. Endpoints are
/// inclusive; an axis with one sample collapses to that sample.
fn axis_weights(value: f32, lo: f32, span: f32, count: usize) -> (usize, usize, f32) {
    if count <= 1 || !span.is_finite() || span <= 0.0 {
        return (0, 0, 0.0);
    }
    let last = (count - 1) as f32;
    let t = ((value - lo) / span).clamp(0.0, 1.0) * last;
    let i0 = (t.floor() as usize).min(count - 1);
    let i1 = (i0 + 1).min(count - 1);
    (i0, i1, t - i0 as f32)
}
