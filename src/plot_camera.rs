//! Plot-only framing and navigation, independent of the vector camera limits.
//!
//! All fit and camera arithmetic stays in f64. A collapsed axis is padded by
//! `max(largest_span * 1e-6, 1e-6)` on each side before the 10% framing margin
//! is applied; the retained bounds include the padding but not the margin.
//! The final f32 transform is checked against the f64 construction, including
//! world positions the GPU will actually use. Nothing here creates a GPU device,
//! so these matrices are geometry evidence rather than rendered-pixel proof.

use std::{error::Error, fmt};

use glam::{DMat4, DVec3, Mat4, Vec3};

use crate::{
    domain::Domain,
    plot::{PlotBounds, PlotKind},
    view_command::ViewCommand,
    viewmodel::{ORBIT_SENSITIVITY, PLANE_MARGIN, ViewGesture},
};

/// 10% total-span framing margin: fitted corners stay inside `1/1.1` NDC.
const FIT_MARGIN: f64 = 1.1;
/// Maximum f32-vs-f64 NDC drift accepted for a usable final matrix.
const PROJECTION_TOLERANCE: f64 = 2e-4;
/// f32 rounding allowance for the framing-margin check.
const FRAMING_EPSILON: f64 = 2e-5;

/// A finite, usable world-to-clip transform using WebGPU's `[0, 1]` depth range.
#[derive(Clone, Copy, Debug)]
pub struct PlotFrame {
    pub view_proj: Mat4,
    pub eye: Vec3,
    pub aspect: f32,
}

/// A rejected fit or navigation operation; the camera remains unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlotFitError {
    InvalidAspect,
    InvalidBounds,
    InvalidState,
    InvalidInput,
    UnusablePrecision,
}

impl fmt::Display for PlotFitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidAspect => "plot aspect must be finite and positive",
            Self::InvalidBounds => "plot bounds must be finite, ordered and bounded",
            Self::InvalidState => "plot camera state is not finite or usable",
            Self::InvalidInput => "plot navigation input is not finite or valid",
            Self::UnusablePrecision => "plot bounds cannot produce a usable f32 projection",
        })
    }
}

impl Error for PlotFitError {}

/// Orbit/front-view camera with plot-specific, unclamped framing parameters.
#[derive(Clone, Debug)]
pub struct PlotCamera {
    pub target: DVec3,
    pub distance: f64,
    /// Full visible vertical span in the orthographic front view.
    pub ortho_extent: f64,
    pub yaw: f64,
    /// Polar angle from +Y, matching the existing orbit camera.
    pub pitch: f64,
    /// Vertical perspective field of view, in radians.
    pub fov_y: f64,
    /// Retained plot bounds, including collapsed-axis padding.
    pub fitted_bounds: PlotBounds,
    // Gestures have no aspect parameter, so use the last committed frame aspect.
    aspect: f32,
}

impl Default for PlotCamera {
    fn default() -> Self {
        Self {
            target: DVec3::ZERO,
            distance: 3.2,
            // Matches the vector front view's default `2 * PLANE_MARGIN`.
            ortho_extent: 2.0 * f64::from(PLANE_MARGIN),
            yaw: 0.8,
            pitch: 0.9,
            fov_y: 55.0_f64.to_radians(),
            fitted_bounds: PlotBounds {
                min: DVec3::splat(-1.0),
                max: DVec3::ONE,
            },
            aspect: 1.0,
        }
    }
}

impl PlotCamera {
    /// Frame sampled plot bounds, preserving the current orbit/front orientation.
    /// The candidate is committed only after its final f32 transform passes
    /// finite, positive-w, framing-margin and WebGPU-depth checks.
    pub fn fit(
        &mut self,
        bounds: PlotBounds,
        kind: PlotKind,
        aspect: f32,
    ) -> Result<(), PlotFitError> {
        valid_aspect(aspect)?;
        self.frame(kind, self.aspect)?;
        let mut next = self.clone();
        next.fitted_bounds = padded_bounds(bounds)?;
        let span = next.fitted_bounds.max - next.fitted_bounds.min;
        next.target = next.fitted_bounds.min + span * 0.5;
        let half = span * 0.5;
        let radius = half.length();
        if !positive(radius) {
            return Err(PlotFitError::UnusablePrecision);
        }
        if kind == PlotKind::Curve2D {
            next.ortho_extent = FIT_MARGIN * span.y.max(span.x / f64::from(aspect));
            // Eye on +Z, outside the padded Z slab of a curve.
            next.distance = radius * 2.0;
        } else {
            let tan_y = (next.fov_y * 0.5).tan();
            let tan_x = tan_y * f64::from(aspect);
            let tight = tan_y.min(tan_x);
            let half_angle = (tight / FIT_MARGIN).atan();
            let sin_half = half_angle.sin();
            if !positive(sin_half) {
                return Err(PlotFitError::UnusablePrecision);
            }
            next.distance = radius / sin_half;
        }
        if !positive(next.ortho_extent) || !positive(next.distance) {
            return Err(PlotFitError::UnusablePrecision);
        }
        next.aspect = aspect;
        let frame = next.frame(kind, aspect)?;
        for corner in corners(next.fitted_bounds) {
            let clip = frame.view_proj * corner.as_vec3().extend(1.0);
            if !clip.is_finite() || clip.w <= 0.0 {
                return Err(PlotFitError::UnusablePrecision);
            }
            let ndc = (clip.truncate() / clip.w).as_dvec3();
            if !ndc.is_finite()
                || ndc.x.abs() > 1.0 / FIT_MARGIN + FRAMING_EPSILON
                || ndc.y.abs() > 1.0 / FIT_MARGIN + FRAMING_EPSILON
                || !(-FRAMING_EPSILON..=1.0 + FRAMING_EPSILON).contains(&ndc.z)
            {
                return Err(PlotFitError::UnusablePrecision);
            }
        }
        *self = next;
        Ok(())
    }

    /// Recompute the final transform and clipping at the current eye. Navigation
    /// may move geometry offscreen or behind the eye; it never edits the bounds.
    pub fn frame(&self, kind: PlotKind, aspect: f32) -> Result<PlotFrame, PlotFitError> {
        valid_aspect(aspect)?;
        self.validate()?;
        let (right, up, outward) = self.basis(kind)?;
        let eye = self.target + outward * self.distance;
        if !eye.is_finite() {
            return Err(PlotFitError::UnusablePrecision);
        }
        let eye32 = eye.as_vec3();
        if !eye32.is_finite() {
            return Err(PlotFitError::UnusablePrecision);
        }
        // Camera-facing ribbons need a usable f32 eye offset and direction.
        let gpu_offset = eye32.as_dvec3() - self.target.as_vec3().as_dvec3();
        if !gpu_offset.is_finite() || gpu_offset.length_squared() == 0.0 {
            return Err(PlotFitError::UnusablePrecision);
        }
        if gpu_offset.normalize().dot(outward) < 0.9 {
            return Err(PlotFitError::UnusablePrecision);
        }
        let (near, far) = self.clip_planes(eye, outward)?;
        // Build the view from the known basis so a large target offset does not
        // lose the eye-target difference to cancellation in `look_at`.
        let view = DMat4::from_cols(
            DVec3::new(right.x, up.x, outward.x).extend(0.0),
            DVec3::new(right.y, up.y, outward.y).extend(0.0),
            DVec3::new(right.z, up.z, outward.z).extend(0.0),
            DVec3::new(-right.dot(eye), -up.dot(eye), -outward.dot(eye)).extend(1.0),
        );
        let projection = if kind == PlotKind::Curve2D {
            let half_y = self.ortho_extent * 0.5;
            let half_x = half_y * f64::from(aspect);
            if !positive(half_x) || !positive(half_y) {
                return Err(PlotFitError::UnusablePrecision);
            }
            DMat4::orthographic_rh(-half_x, half_x, -half_y, half_y, near, far)
        } else {
            DMat4::perspective_rh(self.fov_y, f64::from(aspect), near, far)
        };
        let precise = projection * view;
        let view_proj = Mat4::from_cols_array(&precise.to_cols_array().map(|value| value as f32));
        if !precise.is_finite() || !view_proj.is_finite() {
            return Err(PlotFitError::UnusablePrecision);
        }
        let frame = PlotFrame {
            view_proj,
            eye: eye32,
            aspect,
        };
        self.validate_projection(precise, frame)?;
        Ok(frame)
    }

    /// Apply pointer gestures transactionally. `viewport_height` is the
    /// authoritative point height used to scale pans; the gesture's own height
    /// field is part of the shared input type but is not consulted here.
    pub fn apply_gesture(
        &mut self,
        kind: PlotKind,
        gesture: ViewGesture,
        viewport_height: f32,
    ) -> Result<(), PlotFitError> {
        self.frame(kind, self.aspect)?;
        if !gesture.drag.is_finite()
            || !gesture.pan.is_finite()
            || !positive(f64::from(gesture.zoom))
            || !positive(f64::from(viewport_height))
        {
            return Err(PlotFitError::InvalidInput);
        }
        let mut next = self.clone();
        let height = f64::from(viewport_height);
        if kind == PlotKind::Curve2D {
            // Match the existing front-view input: pan at the old scale, then
            // apply the multiplicative wheel zoom.
            let scale = next.ortho_extent / height;
            let pan = gesture.drag.as_dvec2() + gesture.pan.as_dvec2();
            next.target.x -= pan.x * scale;
            next.target.y += pan.y * scale;
            next.ortho_extent /= f64::from(gesture.zoom);
        } else {
            next.orbit(
                f64::from(gesture.drag.x) * f64::from(ORBIT_SENSITIVITY),
                f64::from(gesture.drag.y) * f64::from(ORBIT_SENSITIVITY),
            );
            next.distance /= f64::from(gesture.zoom);
            let scale = 2.0 * next.distance * (next.fov_y * 0.5).tan() / height;
            next.pan(
                kind,
                -f64::from(gesture.pan.x) * scale,
                f64::from(gesture.pan.y) * scale,
            )?;
        }
        next.frame(kind, next.aspect)?;
        *self = next;
        Ok(())
    }

    /// Apply existing commands. `Fit` frames the domain, not sampled plot
    /// extrema; `Reset` restores default orientation/distance around the domain.
    pub fn apply_command(
        &mut self,
        kind: PlotKind,
        command: ViewCommand,
        domain: &Domain,
        aspect: f32,
    ) -> Result<(), PlotFitError> {
        valid_aspect(aspect)?;
        self.frame(kind, self.aspect)?;
        let mut next = self.clone();
        next.aspect = aspect;
        match command {
            ViewCommand::Orbit { dx, dy } => {
                finite_pair(dx, dy)?;
                if kind != PlotKind::Curve2D {
                    next.orbit(f64::from(dx), f64::from(dy));
                }
            }
            ViewCommand::Pan { dx, dy } => {
                finite_pair(dx, dy)?;
                next.pan(kind, f64::from(dx), f64::from(dy))?;
            }
            ViewCommand::Zoom { factor } => {
                if !positive(f64::from(factor)) {
                    return Err(PlotFitError::InvalidInput);
                }
                if kind == PlotKind::Curve2D {
                    next.ortho_extent /= f64::from(factor);
                } else {
                    next.distance /= f64::from(factor);
                }
            }
            ViewCommand::Fit => {
                next.fit(domain_bounds(domain), kind, aspect)?;
            }
            ViewCommand::Reset => {
                let domain_bounds = domain_bounds(domain);
                let padded = padded_bounds(domain_bounds)?;
                let span = padded.max - padded.min;
                let defaults = Self::default();
                next.target = padded.min + span * 0.5;
                next.yaw = defaults.yaw;
                next.pitch = defaults.pitch;
                next.distance = defaults.distance;
                next.fov_y = defaults.fov_y;
                let domain_span = domain_bounds.max - domain_bounds.min;
                let extent = domain_span.x.max(domain_span.y) * f64::from(PLANE_MARGIN);
                next.ortho_extent = if positive(extent) {
                    extent
                } else {
                    defaults.ortho_extent
                };
                next.fitted_bounds = padded;
            }
        }
        next.frame(kind, aspect)?;
        *self = next;
        Ok(())
    }

    fn validate(&self) -> Result<(), PlotFitError> {
        if !self.target.is_finite()
            || !positive(self.distance)
            || !positive(self.ortho_extent)
            || !self.yaw.is_finite()
            || !self.pitch.is_finite()
            || !positive(self.fov_y)
            || self.fov_y >= std::f64::consts::PI
        {
            return Err(PlotFitError::InvalidState);
        }
        valid_aspect(self.aspect)?;
        validate_bounds(self.fitted_bounds)?;
        let span = self.fitted_bounds.max - self.fitted_bounds.min;
        if !span.cmpgt(DVec3::ZERO).all() {
            return Err(PlotFitError::InvalidState);
        }
        Ok(())
    }

    fn basis(&self, kind: PlotKind) -> Result<(DVec3, DVec3, DVec3), PlotFitError> {
        if kind == PlotKind::Curve2D {
            return Ok((DVec3::X, DVec3::Y, DVec3::Z));
        }
        let outward = DVec3::new(
            self.yaw.cos() * self.pitch.sin(),
            self.pitch.cos(),
            self.yaw.sin() * self.pitch.sin(),
        );
        let across = DVec3::Y.cross(outward);
        if !positive(across.length()) {
            return Err(PlotFitError::InvalidState);
        }
        let right = across.normalize();
        let up = outward.cross(right).normalize();
        if !right.is_finite() || !up.is_finite() || !positive(up.length()) {
            return Err(PlotFitError::InvalidState);
        }
        Ok((right, up, outward))
    }

    fn orbit(&mut self, dx: f64, dy: f64) {
        self.yaw -= dx;
        // Keep the same polar margin as the vector orbit camera.
        self.pitch = (self.pitch - dy).clamp(-1.55, 1.55);
    }

    fn pan(&mut self, kind: PlotKind, dx: f64, dy: f64) -> Result<(), PlotFitError> {
        let (right, up, _) = self.basis(kind)?;
        self.target += right * dx + up * dy;
        Ok(())
    }

    fn clip_planes(&self, eye: DVec3, outward: DVec3) -> Result<(f64, f64), PlotFitError> {
        let mut closest = f64::INFINITY;
        let mut farthest = f64::NEG_INFINITY;
        for corner in corners(self.fitted_bounds) {
            let depth = (eye - corner).dot(outward);
            if !depth.is_finite() {
                return Err(PlotFitError::UnusablePrecision);
            }
            closest = closest.min(depth);
            farthest = farthest.max(depth);
        }
        if !positive(farthest) {
            return Err(PlotFitError::UnusablePrecision);
        }
        let span = (self.fitted_bounds.max - self.fitted_bounds.min).max_element();
        let scale = self.distance.max(span).max(farthest.abs());
        let padding = ((farthest - closest) * 0.05).max(scale * 1e-6);
        let near = if positive(closest) {
            (closest - padding).max(closest * 0.5)
        } else {
            // The eye has entered or passed the bounds: retain the positive
            // portion with a small, finite near plane.
            farthest * 1e-6
        };
        let far = farthest + padding;
        if !positive(near) || !positive(far) || far <= near {
            return Err(PlotFitError::UnusablePrecision);
        }
        Ok((near, far))
    }

    fn validate_projection(&self, precise: DMat4, frame: PlotFrame) -> Result<(), PlotFitError> {
        let mut expected_min = DVec3::splat(f64::INFINITY);
        let mut expected_max = DVec3::splat(f64::NEG_INFINITY);
        let mut actual_min = expected_min;
        let mut actual_max = expected_max;
        for corner in corners(self.fitted_bounds) {
            let point = corner.as_vec3();
            let expected_clip = precise * corner.extend(1.0);
            let actual_clip = frame.view_proj * point.extend(1.0);
            if !point.is_finite()
                || !expected_clip.is_finite()
                || !actual_clip.is_finite()
                || expected_clip.w == 0.0
                || actual_clip.w == 0.0
                || expected_clip.w.signum() != f64::from(actual_clip.w).signum()
            {
                return Err(PlotFitError::UnusablePrecision);
            }
            let expected = expected_clip.truncate() / expected_clip.w;
            let actual = (actual_clip.truncate() / actual_clip.w).as_dvec3();
            if !expected.is_finite() || !actual.is_finite() {
                return Err(PlotFitError::UnusablePrecision);
            }
            let tolerance = expected.abs().max(DVec3::ONE) * PROJECTION_TOLERANCE;
            if (actual - expected).abs().cmpgt(tolerance).any() {
                return Err(PlotFitError::UnusablePrecision);
            }
            if expected_clip.w > 0.0
                && (0.0..=1.0).contains(&expected.z)
                && !(-FRAMING_EPSILON..=1.0 + FRAMING_EPSILON).contains(&actual.z)
            {
                return Err(PlotFitError::UnusablePrecision);
            }
            expected_min = expected_min.min(expected);
            expected_max = expected_max.max(expected);
            actual_min = actual_min.min(actual);
            actual_max = actual_max.max(actual);
        }
        // A finite f32 matrix can still erase the picture through cancellation.
        for axis in 0..2 {
            let expected_span = expected_max[axis] - expected_min[axis];
            let actual_span = actual_max[axis] - actual_min[axis];
            if !positive(expected_span)
                || !positive(actual_span)
                || actual_span < expected_span * 0.5
            {
                return Err(PlotFitError::UnusablePrecision);
            }
        }
        Ok(())
    }
}

fn positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}

fn finite_pair(x: f32, y: f32) -> Result<(), PlotFitError> {
    if x.is_finite() && y.is_finite() {
        Ok(())
    } else {
        Err(PlotFitError::InvalidInput)
    }
}

fn valid_aspect(aspect: f32) -> Result<(), PlotFitError> {
    if positive(f64::from(aspect)) {
        Ok(())
    } else {
        Err(PlotFitError::InvalidAspect)
    }
}

fn validate_bounds(bounds: PlotBounds) -> Result<(), PlotFitError> {
    if !bounds.min.is_finite()
        || !bounds.max.is_finite()
        || !bounds.min.cmple(bounds.max).all()
        || !(bounds.max - bounds.min).is_finite()
    {
        return Err(PlotFitError::InvalidBounds);
    }
    Ok(())
}

/// Pad collapsed axes in f64 before any framing arithmetic.
fn padded_bounds(mut bounds: PlotBounds) -> Result<PlotBounds, PlotFitError> {
    validate_bounds(bounds)?;
    let span = bounds.max - bounds.min;
    let padding = (span.max_element() * 1e-6).max(1e-6);
    if !positive(padding) {
        return Err(PlotFitError::UnusablePrecision);
    }
    for axis in 0..3 {
        if span[axis] == 0.0 {
            let min = bounds.min[axis] - padding;
            let max = bounds.max[axis] + padding;
            if !min.is_finite()
                || !max.is_finite()
                || min >= bounds.min[axis]
                || max <= bounds.max[axis]
            {
                return Err(PlotFitError::UnusablePrecision);
            }
            bounds.min[axis] = min;
            bounds.max[axis] = max;
        }
    }
    validate_bounds(bounds)?;
    if !(bounds.max - bounds.min).cmpgt(DVec3::ZERO).all() {
        return Err(PlotFitError::UnusablePrecision);
    }
    Ok(bounds)
}

fn domain_bounds(domain: &Domain) -> PlotBounds {
    PlotBounds {
        min: domain.min.as_dvec3(),
        max: domain.max.as_dvec3(),
    }
}

fn corners(bounds: PlotBounds) -> [DVec3; 8] {
    std::array::from_fn(|index| {
        DVec3::new(
            if index & 1 == 0 {
                bounds.min.x
            } else {
                bounds.max.x
            },
            if index & 2 == 0 {
                bounds.min.y
            } else {
                bounds.max.y
            },
            if index & 4 == 0 {
                bounds.min.z
            } else {
                bounds.max.z
            },
        )
    })
}
