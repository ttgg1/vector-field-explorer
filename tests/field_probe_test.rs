//! Field probe geometry and bounded integral coverage (#95 slice A).
//!
//! Every value here is a normalized teaching quantity, not an SI measurement.

use glam::Vec3;
use vector_field_explorer::field_probe::{
    CirculationLoop, GaussianSurface, IntegralStatus, LoopKind, MAX_PROBE_SAMPLES, MAX_SEGMENTS,
    MIN_SEGMENTS, ProbeField, SurfaceKind, circulation, flux,
};
use vector_field_explorer::sources::FieldFamily;

fn sphere(center: Vec3, radius: f32, segments: u32) -> GaussianSurface {
    GaussianSurface {
        kind: SurfaceKind::Sphere,
        center,
        axis: Vec3::Z,
        size: Vec3::splat(2.0 * radius),
        segments,
    }
}

fn circle(center: Vec3, radius: f32, segments: u32) -> CirculationLoop {
    CirculationLoop {
        kind: LoopKind::Circle,
        center,
        axis: Vec3::Z,
        size: Vec3::new(2.0 * radius, 2.0 * radius, 2.0 * radius),
        segments,
    }
}

struct PointCharge {
    charge: f64,
    at: Vec3,
}

impl ProbeField for PointCharge {
    fn eval(&self, point: Vec3, _time: f64) -> Option<Vec3> {
        let offset = point.as_dvec3() - self.at.as_dvec3();
        let distance = offset.length();
        if distance == 0.0 || !distance.is_finite() {
            return None;
        }
        Some((offset * (self.charge / (distance * distance * distance))).as_vec3())
    }
}

struct UniformField(Vec3);

impl ProbeField for UniformField {
    fn eval(&self, _point: Vec3, _time: f64) -> Option<Vec3> {
        Some(self.0)
    }
}

struct StraightWire {
    current: f64,
}

impl ProbeField for StraightWire {
    fn eval(&self, point: Vec3, _time: f64) -> Option<Vec3> {
        let x = point.x as f64;
        let y = point.y as f64;
        let rho_squared = x * x + y * y;
        if rho_squared == 0.0 || !rho_squared.is_finite() {
            return None;
        }
        let scale = self.current / (std::f64::consts::TAU * rho_squared);
        Some(Vec3::new((-y * scale) as f32, (x * scale) as f32, 0.0))
    }
}

struct MissingField;

impl ProbeField for MissingField {
    fn eval(&self, _point: Vec3, _time: f64) -> Option<Vec3> {
        None
    }
}

struct NonFiniteField;

impl ProbeField for NonFiniteField {
    fn eval(&self, _point: Vec3, _time: f64) -> Option<Vec3> {
        Some(Vec3::new(f32::NAN, 0.0, 0.0))
    }
}

fn estimate(status: IntegralStatus) -> (f64, f64) {
    match status {
        IntegralStatus::Estimate { value, error } => (value, error),
        other => panic!("expected a complete estimate, got {other:?}"),
    }
}

#[test]
fn sphere_flux_of_a_point_charge_follows_the_enclosed_charge_relation() {
    let charge = 1.75;
    let field = PointCharge {
        charge,
        at: Vec3::ZERO,
    };
    let surface = sphere(Vec3::ZERO, 1.0, 64);
    assert!((surface.area() - 4.0 * std::f64::consts::PI).abs() < 1e-9);
    let report = flux(&field, &surface, FieldFamily::Electric, 0.0);
    assert_eq!(report.failed_patches, 0);
    assert!(report.samples > 0);
    assert!(report.samples <= MAX_PROBE_SAMPLES);

    let (value, error) = estimate(report.status);
    let expected = 4.0 * std::f64::consts::PI * charge;
    assert!(
        (value - expected).abs() <= error + 1e-5,
        "flux {value} should be the normalized enclosed-charge value {expected} \
         within the reported error {error}"
    );
}

#[test]
fn uniform_field_has_zero_flux_through_a_closed_surface() {
    let field = UniformField(Vec3::new(0.75, -1.25, 0.5));
    let surface = GaussianSurface {
        kind: SurfaceKind::Box,
        center: Vec3::new(0.25, -0.5, 0.75),
        axis: Vec3::new(0.0, 0.0, 1.0),
        size: Vec3::new(2.0, 1.5, 1.0),
        segments: 16,
    };
    assert!((surface.area() - 13.0).abs() < 1e-9);
    let report = flux(&field, &surface, FieldFamily::Magnetic, 0.0);
    let (value, error) = estimate(report.status);
    assert!(
        value.abs() <= error + 1e-6,
        "a uniform field has zero closed flux, got {value} +/- {error}"
    );

    let cylinder = GaussianSurface {
        kind: SurfaceKind::Cylinder,
        center: Vec3::ZERO,
        axis: Vec3::Z,
        size: Vec3::splat(2.0),
        segments: 16,
    };
    assert!(
        (cylinder.area() - 6.0 * std::f64::consts::PI).abs() < 1e-9,
        "a radius-1, height-2 cylinder has area 6*pi"
    );
    let report = flux(&field, &cylinder, FieldFamily::Magnetic, 0.0);
    let (value, error) = estimate(report.status);
    assert!(
        value.abs() <= error + 1e-6,
        "a uniform field has zero closed flux through a cylinder, got {value} +/- {error}"
    );
}

#[test]
fn tilted_cylinder_in_a_uniform_field_has_zero_flux() {
    // This pins tilted-axis sampling and plumbing: a uniform field's closed-surface
    // flux is zero under either normal convention, so this passes whether the side
    // normal is the local radial direction `u*cos + v*sin` or a world-space
    // `(cos, sin, 0)` that assumes the axis is +Z. Discrimination for the normals is
    // provided by `tilted_cylinder_normals_follow_the_local_basis`.
    let field = UniformField(Vec3::new(0.75, -1.25, 0.5));
    let surface = GaussianSurface {
        kind: SurfaceKind::Cylinder,
        center: Vec3::new(0.25, -0.5, 0.75),
        axis: Vec3::new(0.6, 0.8, 0.0),
        size: Vec3::splat(2.0),
        segments: 16,
    };
    let report = flux(&field, &surface, FieldFamily::Magnetic, 0.0);
    assert_eq!(report.failed_patches, 0);
    let (value, error) = estimate(report.status);
    assert!(
        value.abs() <= error + 1e-6,
        "a uniform field has zero closed flux through a tilted cylinder, got {value} +/- {error}"
    );
}

#[test]
fn tilted_cylinder_normals_follow_the_local_basis() {
    // A centered point charge has closed flux `4*pi*q`; with a world-space side
    // normal the tilted side contributes the wrong amount, so this pins the
    // local-basis normal rather than any axis alignment.
    let charge = 1.75;
    let field = PointCharge {
        charge,
        at: Vec3::ZERO,
    };
    let surface = GaussianSurface {
        kind: SurfaceKind::Cylinder,
        center: Vec3::ZERO,
        axis: Vec3::new(0.6, 0.8, 0.0),
        size: Vec3::new(2.0, 2.0, 2.0),
        segments: 32,
    };
    let report = flux(&field, &surface, FieldFamily::Electric, 0.0);
    assert_eq!(report.failed_patches, 0);
    let (value, error) = estimate(report.status);
    let expected = 4.0 * std::f64::consts::PI * charge;
    assert!(
        (value - expected).abs() <= error + 1e-4,
        "tilted-cylinder flux {value} should be the enclosed-charge value {expected} \
         within the reported error {error}"
    );
}

#[test]
fn loop_around_a_straight_wire_recovers_the_ampere_relation() {
    let current = 2.5;
    let field = StraightWire { current };
    let loop_ = circle(Vec3::ZERO, 1.5, 64);
    let report = circulation(&field, &loop_, FieldFamily::Magnetic, 0.0);
    assert_eq!(report.failed_segments, 0);
    assert!(report.samples > 0);
    assert!(!report.one_sided, "a supported loop is closed");

    let (value, error) = estimate(report.status);
    assert!(
        (value - current).abs() <= error + 1e-5,
        "circulation {value} should recover the normalized current {current} \
         within the reported error {error}"
    );

    let rectangle = CirculationLoop {
        kind: LoopKind::Rectangle,
        center: Vec3::ZERO,
        axis: Vec3::Z,
        size: Vec3::splat(2.0),
        segments: 64,
    };
    let report = circulation(&field, &rectangle, FieldFamily::Magnetic, 0.0);
    let (value, error) = estimate(report.status);
    assert!(
        (value - current).abs() <= error + 1e-5,
        "rectangle circulation {value} should recover the normalized current \
         {current} within the reported error {error}"
    );
}

#[test]
fn missing_and_non_finite_samples_are_reported_not_zeroed() {
    let surface = sphere(Vec3::ZERO, 1.0, 8);

    let missing = flux(&MissingField, &surface, FieldFamily::Electric, 0.0);
    assert!(missing.failed_patches > 0, "failed patches are counted");
    match missing.status {
        IntegralStatus::Unresolved { reason } => {
            assert!(
                reason.to_lowercase().contains("missing"),
                "reason `{reason}` should name the missing sample"
            );
        }
        other => panic!("missing samples must be unresolved, got {other:?}"),
    }

    let non_finite = flux(&NonFiniteField, &surface, FieldFamily::Electric, 0.0);
    assert!(non_finite.failed_patches > 0, "failed patches are counted");
    match non_finite.status {
        IntegralStatus::Singular { reason } => {
            assert!(
                reason.to_lowercase().contains("non-finite"),
                "reason `{reason}` should name the non-finite sample"
            );
        }
        other => panic!("non-finite samples must be singular, got {other:?}"),
    }

    let loop_ = circle(Vec3::ZERO, 1.0, 8);
    let missing = circulation(&MissingField, &loop_, FieldFamily::Magnetic, 0.0);
    assert!(missing.failed_segments > 0);
    assert!(matches!(missing.status, IntegralStatus::Unresolved { .. }));

    let non_finite = circulation(&NonFiniteField, &loop_, FieldFamily::Magnetic, 0.0);
    assert!(non_finite.failed_segments > 0);
    assert!(matches!(non_finite.status, IntegralStatus::Singular { .. }));
}

#[test]
fn validation_rejects_non_finite_and_degenerate_geometry() {
    let mut surface = sphere(Vec3::ZERO, 1.0, 16);
    assert!(surface.validate().is_ok());

    surface.center = Vec3::new(f32::NAN, 0.0, 0.0);
    assert!(surface.validate().unwrap_err().contains("center"));
    surface = sphere(Vec3::ZERO, 1.0, 16);
    surface.axis = Vec3::ZERO;
    assert!(surface.validate().unwrap_err().contains("axis"));
    surface = sphere(Vec3::ZERO, 1.0, 16);
    surface.size = Vec3::new(2.0, 0.0, 2.0);
    assert!(surface.validate().unwrap_err().contains("size"));
    surface = sphere(Vec3::ZERO, 1.0, 16);
    surface.size = Vec3::new(2.0, -1.0, 2.0);
    assert!(surface.validate().unwrap_err().contains("size"));
    surface = sphere(Vec3::ZERO, 1.0, 16);
    surface.size = Vec3::new(f32::INFINITY, 1.0, 1.0);
    assert!(surface.validate().unwrap_err().contains("size"));
    surface = sphere(Vec3::ZERO, 1.0, MIN_SEGMENTS - 1);
    assert!(surface.validate().unwrap_err().contains("segments"));
    surface = sphere(Vec3::ZERO, 1.0, MAX_SEGMENTS + 1);
    assert!(surface.validate().unwrap_err().contains("segments"));

    let mut loop_ = circle(Vec3::ZERO, 1.0, 16);
    assert!(loop_.validate().is_ok());
    loop_.center = Vec3::new(0.0, f32::NAN, 0.0);
    assert!(loop_.validate().unwrap_err().contains("center"));
    loop_ = circle(Vec3::ZERO, 1.0, 16);
    loop_.axis = Vec3::ZERO;
    assert!(loop_.validate().unwrap_err().contains("axis"));
    loop_ = circle(Vec3::ZERO, 1.0, 16);
    loop_.size = Vec3::new(1.0, 1.0, 0.0);
    assert!(loop_.validate().unwrap_err().contains("size"));
    loop_ = circle(Vec3::ZERO, 1.0, 16);
    loop_.segments = MIN_SEGMENTS - 1;
    assert!(loop_.validate().unwrap_err().contains("segments"));
}

#[test]
fn validation_rejects_geometry_over_the_sample_budget() {
    let oversized = GaussianSurface {
        kind: SurfaceKind::Box,
        center: Vec3::ZERO,
        axis: Vec3::Z,
        size: Vec3::splat(2.0),
        segments: MAX_SEGMENTS,
    };
    let error = oversized.validate().unwrap_err();
    assert!(
        error.to_lowercase().contains("max_probe_samples"),
        "error `{error}` should name MAX_PROBE_SAMPLES"
    );
    assert!(
        error.contains(&MAX_PROBE_SAMPLES.to_string()),
        "error `{error}` should name the {MAX_PROBE_SAMPLES} limit"
    );

    let allowed = GaussianSurface {
        kind: SurfaceKind::Box,
        center: Vec3::ZERO,
        axis: Vec3::Z,
        size: Vec3::splat(2.0),
        segments: 26,
    };
    assert!(
        allowed.validate().is_ok(),
        "6 * 26^2 stays below the sample budget"
    );

    let exact_sphere = sphere(Vec3::ZERO, 1.0, MAX_SEGMENTS);
    assert!(
        exact_sphere.validate().is_ok(),
        "a sphere at MAX_SEGMENTS uses exactly MAX_PROBE_SAMPLES"
    );
}

#[test]
// Regression pin for the offset-charge case only: halving the patch spacing
// keeps this particular reported error from growing. It is not a general
// monotonicity theorem about the full/half-resolution spread.
fn refinement_does_not_increase_the_reported_error() {
    let field = PointCharge {
        charge: 1.0,
        at: Vec3::new(0.3, 0.0, 0.0),
    };
    let coarse = sphere(Vec3::ZERO, 1.0, 16);
    let fine = sphere(Vec3::ZERO, 1.0, 32);
    let (_, coarse_error) = estimate(flux(&field, &coarse, FieldFamily::Electric, 0.0).status);
    let (_, fine_error) = estimate(flux(&field, &fine, FieldFamily::Electric, 0.0).status);
    assert!(
        fine_error <= coarse_error + 1e-9,
        "refinement error {fine_error} should not exceed the coarse estimate {coarse_error}"
    );
}

#[test]
fn reports_are_deterministic_for_repeated_calls() {
    let field = PointCharge {
        charge: -1.25,
        at: Vec3::new(-0.2, 0.4, 0.1),
    };
    let surface = sphere(Vec3::ZERO, 1.0, 24);
    let first = flux(&field, &surface, FieldFamily::Electric, 0.75);
    let second = flux(&field, &surface, FieldFamily::Electric, 0.75);
    assert_eq!(first, second);

    let wire = StraightWire { current: 1.5 };
    let loop_ = circle(Vec3::ZERO, 1.0, 24);
    let first = circulation(&wire, &loop_, FieldFamily::Magnetic, 0.75);
    let second = circulation(&wire, &loop_, FieldFamily::Magnetic, 0.75);
    assert_eq!(first, second);
}
