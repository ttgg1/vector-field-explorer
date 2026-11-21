//! Bounded analytic source-free plane-wave teaching model.
//!
//! Everything here is expressed in **normalized teaching units**, with
//! `c = 1` by construction ([`TEACHING_SPEED`]). The model is the analytic
//! full-wave plane wave
//!
//! ```text
//! E(r, t) = A p̂ cos(k·r − ωt + φ)
//! B(r, t) = (1/c) k̂ × E(r, t)
//! ```
//!
//! with `ω = 2πc/λ` and `|k| = 2π/λ`. It is an illustration of a synchronized
//! `E`/`B` full-wave solution, **not** an SI measurement, not a boundary-value
//! solution, and not a solver for arbitrary sources or current distributions.
//! There is no panel, persistence, renderer, capture or clock state here.
//!
//! The wave is source-free, so `div E = div B = 0` exactly and `rho = 0`
//! exactly; the model never invents a free-current distribution, so
//! [`equation_terms`] leaves `j` as `None`. Fed to
//! [`crate::maxwell_residual::residual`], that means both Gauss laws and
//! Faraday's law report [`crate::maxwell_residual::TermStatus::Analytic`],
//! while Ampere–Maxwell honestly reports
//! [`crate::maxwell_residual::TermStatus::Unsupported`]`("j")` because no
//! free-current term is modelled.
//!
//! Every input is validated rather than clamped: a non-finite or out-of-range
//! amplitude/wavelength, a degenerate propagation direction, or a polarization
//! (near-)parallel to propagation is rejected by [`WaveSpec::validate`]. The
//! sampling functions re-check the same bounds and also reject a non-finite
//! point or time, returning `None` instead of propagating NaN.

use std::f64::consts::TAU;

use glam::{DVec3, UVec3, Vec3};

use crate::domain::Domain;
use crate::maxwell_residual::FieldTerms;
use crate::plane::PlaneFrame;

/// Speed of light in these normalized teaching units.
///
/// The model defines `c = 1` by construction; the constant exists so the
/// `B = (1/c) k̂ × E` relation is written explicitly rather than hidden.
pub const TEACHING_SPEED: f64 = 1.0;

/// Exclusive lower bound for a finite wavelength.
pub const MIN_WAVELENGTH: f64 = 1e-6;

/// Inclusive absolute bound on the electric-field amplitude.
pub const MAX_AMPLITUDE: f64 = 1e6;

/// Smallest direction/polarization length treated as a usable axis.
pub const MIN_AXIS_LENGTH: f64 = 1e-9;

/// Editable parameters of one plane wave, in normalized teaching units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaveSpec {
    /// `|E|` peak amplitude.
    pub amplitude: f64,
    /// Wavelength `λ`, strictly greater than [`MIN_WAVELENGTH`].
    pub wavelength: f64,
    /// Propagation direction; renormalized by [`WaveSpec::validate`].
    pub direction: Vec3,
    /// Polarization seed; normalized and projected perpendicular to
    /// propagation by [`WaveSpec::validate`].
    pub polarization: Vec3,
    /// Phase offset `φ` in radians.
    ///
    /// [`WaveSpec::validate`] checks only finiteness; there is no range bound.
    /// A very large `φ` loses accuracy through argument reduction in
    /// `sin`/`cos`; if a panel later exposes `phase`, any bound belongs there,
    /// not in this model.
    pub phase: f64,
}

impl WaveSpec {
    /// Validate and repair the editable parameters.
    ///
    /// `direction` is renormalized and `polarization` is projected
    /// perpendicular to it and renormalized. Non-finite or out-of-range
    /// amplitude/wavelength/phase, a degenerate direction, and a (near-)
    /// parallel polarization are rejected; on error nothing is modified.
    ///
    /// The rewritten unit vectors are stored back as `f32` `Vec3`, so validating
    /// an already-validated spec re-projects from the rounded values and may
    /// shift the unit vectors by `f32` rounding; repeated calls are therefore
    /// not bit-idempotent, which is within teaching tolerance.
    pub fn validate(&mut self) -> Result<(), String> {
        let geometry = Geometry::new(self)?;
        self.direction = geometry.direction.as_vec3();
        self.polarization = geometry.polarization.as_vec3();
        Ok(())
    }

    /// Angular frequency `ω = 2πc/λ`.
    pub fn omega(&self) -> f64 {
        TAU * TEACHING_SPEED / self.wavelength
    }

    /// Wave vector `k = (2π/λ) k̂`.
    ///
    /// Returns [`Vec3::ZERO`] when the stored direction is not a usable axis;
    /// call [`WaveSpec::validate`] first for a meaningful vector. That zero is
    /// a deliberate "no direction" sentinel, not a substitute axis: rejection
    /// without substitution stays [`WaveSpec::validate`]'s job, so `k` never
    /// invents a fallback direction.
    pub fn k(&self) -> Vec3 {
        let direction = self.direction.as_dvec3();
        let length = direction.length();
        if !direction.is_finite() || length <= MIN_AXIS_LENGTH {
            return Vec3::ZERO;
        }
        ((direction / length) * (TAU / self.wavelength)).as_vec3()
    }
}

/// One synchronized electric/magnetic sample in normalized teaching units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaveSample {
    pub e: Vec3,
    pub b: Vec3,
    /// Evaluated phase `k·r − ωt + φ`.
    pub phase: f64,
}

/// Sample `E` and `B` at one finite point and time.
///
/// Returns `None` for an invalid spec or a non-finite point/time, and never
/// returns a sample containing NaN or infinity.
pub fn sample_wave(spec: &WaveSpec, point: Vec3, time: f64) -> Option<WaveSample> {
    let geometry = Geometry::new(spec).ok()?;
    let point = point.as_dvec3();
    if !point.is_finite() || !time.is_finite() {
        return None;
    }
    let phase = geometry.phase_at(point, time);
    let cosine = phase.cos();
    if !phase.is_finite() || !cosine.is_finite() {
        return None;
    }
    let amplitude = geometry.amplitude * cosine;
    let e = geometry.polarization * amplitude;
    let b = geometry.direction.cross(e) / TEACHING_SPEED;
    if !e.is_finite() || !b.is_finite() {
        return None;
    }
    Some(WaveSample {
        e: e.as_vec3(),
        b: b.as_vec3(),
        phase,
    })
}

/// Sample [`sample_wave`] over a viewport grid, returning finite samples only.
///
/// Without `plane`, the grid spans `domain` and each position is a world point.
/// With `plane`, the grid spans the frame's local domain, the position is the
/// plane-local point the caller draws at, and the world point is
/// `frame.to_world(local)`. A sample the model rejects (invalid spec, non-finite
/// point/time, non-finite field) is skipped rather than returned as NaN. The
/// caller owns any further density cap; this helper adds none.
pub fn sample_wave_grid(
    spec: &WaveSpec,
    domain: &Domain,
    grid: UVec3,
    plane: Option<&PlaneFrame>,
    time: f64,
) -> Vec<(Vec3, WaveSample)> {
    let (effective_domain, effective_grid) = match plane {
        Some(frame) => (frame.local_domain(), UVec3::new(grid.x, grid.y, 1)),
        None => (*domain, grid),
    };
    let mut samples = Vec::new();
    for local in effective_domain.grid_points(effective_grid) {
        if !local.is_finite() {
            continue;
        }
        let world = plane.map_or(local, |frame| frame.to_world(local));
        if !world.is_finite() {
            continue;
        }
        let Some(sample) = sample_wave(spec, world, time) else {
            continue;
        };
        samples.push((local, sample));
    }
    samples
}

/// Closed-form field terms at one finite point and time, for
/// [`crate::maxwell_residual::residual`].
///
/// For `θ = k·r − ωt + φ`, `q̂ = k̂ × p̂` and `c = 1`:
///
/// ```text
/// div E = 0,            div B = 0,            rho = 0,   j = None
/// curl E = −A sinθ |k| q̂
/// curl B =  A sinθ |k| p̂
/// dE/dt =  A ω sinθ p̂
/// dB/dt =  A ω sinθ q̂
/// ```
///
/// so `curl E + dB/dt = 0` analytically. `rho` is an exact analytic zero for
/// the source-free wave, not a fabricated placeholder. `j` stays `None`
/// because no free-current distribution is modelled, which keeps
/// Ampere–Maxwell `Unsupported` instead of claiming a current identity.
/// Returns `None` for an invalid spec or a non-finite point/time.
pub fn equation_terms(spec: &WaveSpec, point: Vec3, time: f64) -> Option<FieldTerms> {
    let geometry = Geometry::new(spec).ok()?;
    let point = point.as_dvec3();
    if !point.is_finite() || !time.is_finite() {
        return None;
    }
    let phase = geometry.phase_at(point, time);
    let sine = phase.sin();
    if !phase.is_finite() || !sine.is_finite() {
        return None;
    }

    let amplitude = geometry.amplitude;
    let wavenumber = geometry.wavenumber;
    let transverse = geometry.direction.cross(geometry.polarization);

    let curl_e = transverse * (-amplitude * sine * wavenumber);
    let curl_b = geometry.polarization * (amplitude * sine * wavenumber);
    let de_dt = geometry.polarization * (amplitude * geometry.omega * sine);
    let db_dt = transverse * (amplitude * geometry.omega * sine);

    let terms = FieldTerms {
        div_e: Some(0.0),
        div_b: Some(0.0),
        curl_e: Some(curl_e.to_array()),
        curl_b: Some(curl_b.to_array()),
        rho: Some(0.0),
        j: None,
        de_dt: Some(de_dt.to_array()),
        db_dt: Some(db_dt.to_array()),
    };
    if [terms.curl_e, terms.curl_b, terms.de_dt, terms.db_dt]
        .into_iter()
        .flatten()
        .all(|components| components.iter().all(|component| component.is_finite()))
    {
        Some(terms)
    } else {
        None
    }
}

/// Fully validated phase geometry, shared by validation and sampling.
struct Geometry {
    /// Unit propagation direction `k̂`.
    direction: DVec3,
    /// Unit polarization `p̂`, perpendicular to `direction`.
    polarization: DVec3,
    /// `ω = 2πc/λ`.
    omega: f64,
    /// `|k| = 2π/λ`.
    wavenumber: f64,
    amplitude: f64,
    phase: f64,
}

impl Geometry {
    fn new(spec: &WaveSpec) -> Result<Self, String> {
        if !spec.amplitude.is_finite() || spec.amplitude.abs() > MAX_AMPLITUDE {
            return Err(format!(
                "amplitude must be finite and within +/-{MAX_AMPLITUDE}"
            ));
        }
        if !spec.wavelength.is_finite() || spec.wavelength <= MIN_WAVELENGTH {
            return Err(format!(
                "wavelength must be finite and greater than {MIN_WAVELENGTH}"
            ));
        }
        if !spec.phase.is_finite() {
            return Err("phase must be finite".to_owned());
        }

        let direction = unit_axis(spec.direction, "direction")?;
        let seed = unit_axis(spec.polarization, "polarization")?;
        let projected = seed - direction * seed.dot(direction);
        let projected_length = projected.length();
        if !projected_length.is_finite() || projected_length <= MIN_AXIS_LENGTH {
            return Err(
                "polarization must not be parallel (or nearly parallel) to the propagation direction"
                    .to_owned(),
            );
        }

        let wavenumber = TAU / spec.wavelength;
        Ok(Self {
            direction,
            polarization: projected / projected_length,
            omega: wavenumber * TEACHING_SPEED,
            wavenumber,
            amplitude: spec.amplitude,
            phase: spec.phase,
        })
    }

    fn phase_at(&self, point: DVec3, time: f64) -> f64 {
        self.direction.dot(point) * self.wavenumber - self.omega * time + self.phase
    }
}

fn unit_axis(value: Vec3, label: &str) -> Result<DVec3, String> {
    let axis = value.as_dvec3();
    if !axis.is_finite() {
        return Err(format!("{label} must be finite"));
    }
    let length = axis.length();
    if length <= MIN_AXIS_LENGTH {
        return Err(format!(
            "{label} must be longer than {MIN_AXIS_LENGTH} (degenerate axis)"
        ));
    }
    Ok(axis / length)
}
