//! CPU-baked field lattice (P3 slice 1b).
//!
//! A time-independent formula field can be evaluated once onto a bounded
//! axis-aligned lattice and then sampled with trilinear interpolation. This
//! module builds, validates, samples and measures a [`BakedField`], and owns
//! the durable [`BakeSettings`] record. The opt-in wiring lives in
//! [`crate::viewmodel::State::scene_flow`]; with the opt-in off (the default)
//! every existing result stays byte-identical.
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
//! A bake is an approximation. [`BakedField::build`] measures the
//! interpolation and divergence error against the exact field on a bounded
//! deterministic probe set and stores a [`BakeAccuracy`]. An unmeasured metric
//! stays [`None`]; nothing is fabricated, and an error above
//! [`BAKE_TOLERANCE`] is reported by [`BakeAccuracy::exceeds_tolerance`].
//!
//! Built only for [`FieldExpr::is_time_dependent`] `== false`; a time-dependent
//! field is refused rather than silently frozen at `t = 0`.

use std::fmt;

use glam::Vec3;
use serde::{Deserialize, Serialize};

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

/// Persisted default volume resolution. Coarse enough for a bounded startup
/// build while staying well inside the ADR 0005 lattice budget.
pub const DEFAULT_BAKE_RESOLUTION_3D: u32 = 48;
/// Persisted default plane resolution.
pub const DEFAULT_BAKE_RESOLUTION_2D: u32 = 128;

/// The stated CPU-vs-bake acceptance bound in normalized field units. A
/// measured error above this is reported by [`BakeAccuracy::exceeds_tolerance`]
/// rather than hidden behind the teaching-units caption.
pub const BAKE_TOLERANCE: f32 = 1e-3;

/// The durable opt-in and its per-axis lattice sizes.
///
/// This record is stored in [`crate::persistence::AppSettings`], never in a
/// `ViewRecord`, scene document or edit-history snapshot. The live bake itself
/// is runtime-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BakeSettings {
    /// Whether the scene should trace the baked lattice.
    pub enabled: bool,
    /// Requested resolution for a three-component volume lattice.
    pub resolution_3d: u32,
    /// Requested resolution for a genuine two-component plane lattice.
    pub resolution_2d: u32,
}

impl Default for BakeSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            resolution_3d: DEFAULT_BAKE_RESOLUTION_3D,
            resolution_2d: DEFAULT_BAKE_RESOLUTION_2D,
        }
    }
}

impl BakeSettings {
    /// Clamp both resolutions into their axis bounds, keeping the opt-in. The
    /// caller logs one warning when the record changed.
    pub fn sanitized(self) -> Self {
        Self {
            enabled: self.enabled,
            resolution_3d: self
                .resolution_3d
                .clamp(MIN_VOLUME_RESOLUTION, MAX_VOLUME_RESOLUTION),
            resolution_2d: self
                .resolution_2d
                .clamp(MIN_PLANE_RESOLUTION, MAX_PLANE_RESOLUTION),
        }
    }

    /// The stored resolution for `axis`.
    pub fn resolution_for(self, axis: BakeAxis) -> u32 {
        match axis {
            BakeAxis::Volume3d => self.resolution_3d,
            BakeAxis::Plane2d => self.resolution_2d,
        }
    }
}

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

/// One measured accuracy statement, computed during [`BakedField::build`]
/// against the exact field.
///
/// Every metric is finite-or-`None`: an unmeasured value is absent rather than
/// fabricated as zero. `tolerance` is the stated acceptance bound; a value above
/// it is reported by [`BakeAccuracy::exceeds_tolerance`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BakeAccuracy {
    /// Interpolation probes actually compared (bounded to 512).
    pub probes: u32,
    /// Maximum `|baked - exact|`, when any probe could be compared.
    pub max_abs_error: Option<f32>,
    /// Maximum relative error against the exact velocity magnitude.
    pub max_rel_error: Option<f32>,
    /// Maximum `|div(baked) - div(exact)|` over at most `8³` interior nodes.
    pub divergence_residual: Option<f32>,
    /// The CPU-vs-bake acceptance bound in normalized field units.
    pub tolerance: f32,
}

impl BakeAccuracy {
    /// No measurement was possible (an empty or non-finite probe set).
    fn unmeasured() -> Self {
        Self {
            probes: 0,
            max_abs_error: None,
            max_rel_error: None,
            divergence_residual: None,
            tolerance: BAKE_TOLERANCE,
        }
    }

    /// Whether a measured metric exceeds the stated tolerance.
    pub fn exceeds_tolerance(self) -> bool {
        self.max_abs_error
            .is_some_and(|error| error > self.tolerance)
            || self
                .divergence_residual
                .is_some_and(|residual| residual > self.tolerance)
    }
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
    accuracy: BakeAccuracy,
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
        let mut baked = Self {
            spec,
            lattice,
            accuracy: BakeAccuracy::unmeasured(),
        };
        baked.accuracy = baked.measure_accuracy(field);
        Ok(baked)
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

    /// The accuracy measured when this bake was built. Every metric is
    /// finite-or-`None`; nothing is fabricated.
    pub fn accuracy(&self) -> BakeAccuracy {
        self.accuracy
    }

    /// Measure interpolation and divergence against `reference` on bounded,
    /// deterministic probe sets: at most `8³ = 512` cell-centre points and at
    /// most `8³` interior divergence nodes.
    fn measure_accuracy(&self, reference: &FieldExpr) -> BakeAccuracy {
        let [nx, ny, nz] = self.dims();
        let min = self.spec.domain.min;
        let span = self.spec.domain.max - min;
        let probes_x = nx.saturating_sub(1).clamp(1, 8);
        let probes_y = ny.saturating_sub(1).clamp(1, 8);
        let probes_z = match self.spec.axis {
            BakeAxis::Volume3d => nz.saturating_sub(1).clamp(1, 8),
            BakeAxis::Plane2d => 1,
        };
        let mut probes = 0u32;
        let mut max_abs_error: Option<f32> = None;
        let mut max_rel_error: Option<f32> = None;
        for ix in 0..probes_x {
            for iy in 0..probes_y {
                for iz in 0..probes_z {
                    let x = min.x + span.x * (ix as f32 + 0.5) / probes_x as f32;
                    let y = min.y + span.y * (iy as f32 + 0.5) / probes_y as f32;
                    let z = match self.spec.axis {
                        BakeAxis::Volume3d => min.z + span.z * (iz as f32 + 0.5) / probes_z as f32,
                        BakeAxis::Plane2d => min.z + span.z * 0.5,
                    };
                    let point = Vec3::new(x, y, z);
                    let (Some(exact), Some(sampled)) = (reference.eval3(point), self.sample(point))
                    else {
                        continue;
                    };
                    let error = (sampled - exact).abs().max_element();
                    if !error.is_finite() {
                        continue;
                    }
                    probes += 1;
                    max_abs_error = Some(max_abs_error.map_or(error, |current| current.max(error)));
                    let scale = exact.abs().max_element().max(1e-6);
                    let relative = error / scale;
                    if relative.is_finite() {
                        max_rel_error =
                            Some(max_rel_error.map_or(relative, |current| current.max(relative)));
                    }
                }
            }
        }

        BakeAccuracy {
            probes,
            max_abs_error,
            max_rel_error,
            divergence_residual: self.divergence_residual(reference),
            tolerance: BAKE_TOLERANCE,
        }
    }

    /// Maximum `|div(baked) - div(exact)|` at up to `8³` interior nodes.
    fn divergence_residual(&self, reference: &FieldExpr) -> Option<f32> {
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
        let xs = interior_indices(nx);
        let ys = interior_indices(ny);
        let zs = match self.spec.axis {
            BakeAxis::Volume3d => interior_indices(nz),
            BakeAxis::Plane2d => vec![0],
        };
        let mut worst: Option<f32> = None;
        for &ix in &xs {
            for &iy in &ys {
                for &iz in &zs {
                    let point = Vec3::new(
                        min.x + span.x * ix as f32 / (nx.max(2) - 1) as f32,
                        min.y + span.y * iy as f32 / (ny.max(2) - 1) as f32,
                        match self.spec.axis {
                            BakeAxis::Volume3d => {
                                min.z + span.z * iz as f32 / (nz.max(2) - 1) as f32
                            }
                            BakeAxis::Plane2d => min.z + span.z * 0.5,
                        },
                    );
                    let baked_derivative = |axis: Vec3| {
                        let plus = self.sample(point + axis * h)?;
                        let minus = self.sample(point - axis * h)?;
                        Some((plus - minus) * 0.5)
                    };
                    let exact_derivative = |axis: Vec3| {
                        let plus = reference.eval3(point + axis * h)?;
                        let minus = reference.eval3(point - axis * h)?;
                        Some((plus - minus) * 0.5)
                    };
                    let Some(baked_dx) = baked_derivative(Vec3::X).map(|d| d.x / h.x) else {
                        continue;
                    };
                    let Some(baked_dy) = baked_derivative(Vec3::Y).map(|d| d.y / h.y) else {
                        continue;
                    };
                    let Some(exact_dx) = exact_derivative(Vec3::X).map(|d| d.x / h.x) else {
                        continue;
                    };
                    let Some(exact_dy) = exact_derivative(Vec3::Y).map(|d| d.y / h.y) else {
                        continue;
                    };
                    let (baked_dz, exact_dz) = match self.spec.axis {
                        BakeAxis::Volume3d => {
                            let Some(baked) = baked_derivative(Vec3::Z).map(|d| d.z / h.z) else {
                                continue;
                            };
                            let Some(exact) = exact_derivative(Vec3::Z).map(|d| d.z / h.z) else {
                                continue;
                            };
                            (baked, exact)
                        }
                        BakeAxis::Plane2d => (0.0, 0.0),
                    };
                    let baked_divergence = baked_dx + baked_dy + baked_dz;
                    let exact_divergence = exact_dx + exact_dy + exact_dz;
                    let residual = (baked_divergence - exact_divergence).abs();
                    if residual.is_finite() {
                        worst = Some(worst.map_or(residual, |current| current.max(residual)));
                    }
                }
            }
        }
        worst
    }
}

/// At most eight interior indices in `1..n-1`, evenly spaced and deterministic.
/// A lattice too small for an interior node falls back to its single node.
fn interior_indices(n: usize) -> Vec<usize> {
    if n <= 2 {
        return vec![0];
    }
    let count = (n - 2).min(8);
    let step = (n - 2) as f32 / count as f32;
    (0..count).map(|i| 1 + (i as f32 * step) as usize).collect()
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
