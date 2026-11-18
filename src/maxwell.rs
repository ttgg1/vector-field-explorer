//! Bounded Maxwell teaching cases for the explorer.
//!
//! A named catalogue of ideal electrostatic and magnetostatic cases plus one
//! deliberately limited free-conductor cross-section. Everything here is pure
//! CPU data: closed-form normalized "display" amplitudes, renderer-independent
//! outlines and deterministic sampling diagnostics. There is no SI constant
//! engine, no general solver, and no renderer, clock, source or persistence
//! dependency. The 2D boundary-element conductor solve is added in Task 2;
//! until then [`MaxwellCase::Conductor2D`] samples as unavailable.
//!
//! 2D outlines are slice intersections only, so a slice that is tilted with
//! respect to an ideal infinite cylinder can produce no 2D outline by design.

use glam::{DVec3, UVec3, Vec3};

use crate::domain::Domain;
use crate::plane::PlaneFrame;
use crate::sampling::ArrowData;

/// Largest accepted 3D sample count per axis in the app.
pub const MAXWELL_MAX_DENSITY_3D: u32 = 40;
/// Largest accepted 2D sample count per axis in the app.
pub const MAXWELL_MAX_DENSITY_2D: u32 = 200;
/// Largest accepted boundary-element count (fine run).
pub const MAXWELL_MAX_BEM_ELEMENTS: u32 = 256;
/// Smallest accepted boundary-element count (fine run).
pub const MAXWELL_BEM_MIN_ELEMENTS: u32 = 32;
/// Global relative convergence target for the 2D BEM conductor.
pub const MAXWELL_BEM_TARGET: f64 = 1e-3;
/// Exact-interface guard factor: `factor * max(extent, 1)`.
pub const MAXWELL_INTERFACE_TOLERANCE_FACTOR: f64 = 1e-9;
/// Largest finite arrow relative to `max(|E0|, |B0|, 1e-9)` before omission.
pub const MAXWELL_MAX_RELATIVE: f64 = 1e3;
/// Largest accepted absolute display amplitude.
pub const MAXWELL_MAX_AMPLITUDE: f64 = 1e3;

/// Exact interface warning text; never suppressible.
pub const MAXWELL_INTERFACE_WARNING: &str = "Samples exactly on the interface are omitted because the analytic field is discontinuous there.";
/// Exact extreme-magnitude warning text.
pub const MAXWELL_EXTREME_WARNING: &str = "Extreme near-interface magnitudes are omitted.";
/// Exact 2D projection warning text.
pub const MAXWELL_PROJECTION_WARNING: &str =
    "2D slice shows only the in-plane component; the out-of-plane component is not visible.";
/// Exact coincident ground-slice warning text.
pub const MAXWELL_GROUND_SLICE_WARNING: &str =
    "Ground-plane slice is empty; move the plane off z=0.";
/// Exact empty/impossible-domain warning text.
pub const MAXWELL_EMPTY_DOMAIN_WARNING: &str = "The sampling domain or grid produces no samples.";
/// Exact 3D Conductor2D applicability text.
pub const MAXWELL_SWITCH_2D_WARNING: &str =
    "Conductor2D is a 2D infinite-cylinder cross-section; switch to 2D.";
/// Exact 2D Conductor2D pending-solver text.
pub const MAXWELL_BEM_PENDING_WARNING: &str =
    "Conductor2D 2D boundary solve is pending; no field is drawn yet.";

/// Fixed electric-family arrow colour (amber).
const ELECTRIC_COLOR: [u8; 3] = [255, 176, 64];
/// Fixed magnetic-family arrow colour (cyan).
const MAGNETIC_COLOR: [u8; 3] = [64, 196, 255];
/// Vectors below this magnitude are degenerate and are not drawn.
const DEGENERATE_MAGNITUDE: f64 = 1e-6;
/// Great-circle/circle segment count for outlines.
const OUTLINE_SEGMENTS: usize = 64;
/// Outline alpha for reference geometry.
const OUTLINE_ALPHA: f32 = 0.35;

/// One of the nine named teaching cases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum MaxwellCase {
    ParallelPlates,
    CoaxialElectric,
    CoaxialMagnetic,
    HollowSphere,
    InfiniteWire,
    Solenoid,
    ImageSphere,
    ImagePlane,
    Conductor2D,
}

impl MaxwellCase {
    /// All cases in catalogue order.
    pub const ALL: [MaxwellCase; 9] = [
        Self::ParallelPlates,
        Self::CoaxialElectric,
        Self::CoaxialMagnetic,
        Self::HollowSphere,
        Self::InfiniteWire,
        Self::Solenoid,
        Self::ImageSphere,
        Self::ImagePlane,
        Self::Conductor2D,
    ];
}

/// Which physical family a case draws. Families are never summed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum MaxwellFamily {
    Electric,
    Magnetic,
}

/// The selected display view. Only the field is delivered in this slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum MaxwellView {
    Field,
}

/// The two supported 2D conductor cross-sections.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ConductorOutline {
    Circle { radius: f64 },
    Square { half_extent: f64 },
}

/// How confidently a sampling run is supported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MaxwellStop {
    TargetMet,
    AccuracyNotMet,
    Unavailable,
}

/// Durable, editable Maxwell scene. Amplitudes are normalized display values,
/// not SI quantities.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MaxwellScene {
    pub case: MaxwellCase,
    pub e0: f64,
    pub b0: f64,
    pub separation: f64,
    pub radius: f64,
    pub outer_radius: f64,
    pub charge: f64,
    pub charge_height: f64,
    pub outline: ConductorOutline,
    pub applied: [f64; 2],
    pub elements: u32,
    pub allow_inaccurate: bool,
    pub view: MaxwellView,
}

impl Default for MaxwellScene {
    fn default() -> Self {
        Self {
            case: MaxwellCase::ParallelPlates,
            e0: 1.0,
            b0: 1.0,
            separation: 1.0,
            radius: 1.0,
            outer_radius: 2.0,
            charge: 1.0,
            charge_height: 1.0,
            outline: ConductorOutline::Circle { radius: 1.0 },
            applied: [1.0, 0.0],
            elements: 128,
            allow_inaccurate: false,
            view: MaxwellView::Field,
        }
    }
}

/// Why a scene or point cannot be evaluated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaxwellError {
    InvalidScene(String),
    Unsupported(String),
    Unavailable(String),
}

/// Renderer-independent outline geometry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MaxwellLine {
    pub a: [f32; 3],
    pub b: [f32; 3],
    pub color: [f32; 4],
}

/// Coarse/fine convergence evidence for the 2D BEM conductor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BemConvergence {
    pub coarse_elements: u32,
    pub fine_elements: u32,
    pub relative_difference: f64,
    pub target: f64,
}

/// One sampled Maxwell view plus provenance and deterministic diagnostics.
#[derive(Debug, Clone, PartialEq)]
pub struct MaxwellSampling {
    pub arrows: Vec<ArrowData>,
    pub family: MaxwellFamily,
    pub case: MaxwellCase,
    pub e0: f64,
    pub b0: f64,
    pub assumptions: &'static str,
    pub warning: Option<String>,
    pub omitted: usize,
    pub omitted_large: usize,
    pub out_of_plane_suppressed: bool,
    pub accuracy: MaxwellStop,
    pub convergence: Option<BemConvergence>,
}

/// Electric cases draw `E`; magnetic cases draw `B`.
pub fn maxwell_family(case: MaxwellCase) -> MaxwellFamily {
    match case {
        MaxwellCase::ParallelPlates
        | MaxwellCase::CoaxialElectric
        | MaxwellCase::HollowSphere
        | MaxwellCase::ImageSphere
        | MaxwellCase::ImagePlane
        | MaxwellCase::Conductor2D => MaxwellFamily::Electric,
        MaxwellCase::CoaxialMagnetic | MaxwellCase::InfiniteWire | MaxwellCase::Solenoid => {
            MaxwellFamily::Magnetic
        }
    }
}

/// Static applicability text for the selected case.
pub fn maxwell_assumptions(scene: &MaxwellScene) -> &'static str {
    match scene.case {
        MaxwellCase::ParallelPlates => {
            "Ideal infinite parallel plates with no fringing. E is uniform between the plates and zero outside; normalized display amplitude, not SI values."
        }
        MaxwellCase::CoaxialElectric => {
            "Ideal infinite coaxial cylinders. E = E0 (a/r) r-hat in the gap and zero elsewhere; normalized display amplitude, not SI values."
        }
        MaxwellCase::CoaxialMagnetic => {
            "Ideal infinite coaxial cylinders carrying steady current. B = B0 (a/r) phi-hat in the gap and zero elsewhere; normalized display amplitude."
        }
        MaxwellCase::HollowSphere => {
            "Uniformly charged thin spherical shell. E = E0 (R/r)^2 r-hat outside, zero inside; normalized display amplitude."
        }
        MaxwellCase::InfiniteWire => {
            "Ideal infinite straight thin surface-current shell, not a general current-carrying wire: B = B0 (a/r) phi-hat outside, zero inside and on the axis."
        }
        MaxwellCase::Solenoid => {
            "Ideal infinite solenoid. B = B0 z-hat inside, zero outside; normalized display amplitude, not SI values."
        }
        MaxwellCase::ImageSphere => {
            "Isolated conducting sphere of radius R in a uniform applied E0 z-hat. The charge distribution is the induced surface charge; inside is zero."
        }
        MaxwellCase::ImagePlane => {
            "Infinite grounded plane at z=0 with one point charge q at height h and its image -q. Only the z>=0 half-space is supported; normalized display charge."
        }
        MaxwellCase::Conductor2D => {
            "Grounded isolated-neutral infinite-cylinder cross-section in a uniform in-plane applied E. Infinite-cylinder approximation only; not a 3D finite conductor."
        }
    }
}

/// Shared bounded-catalogue warning; case-specific limits are appended.
pub fn maxwell_warning(scene: &MaxwellScene) -> Option<String> {
    let mut text = String::from("Bounded ideal teaching cases; no general Maxwell solver.");
    match scene.case {
        MaxwellCase::Conductor2D => {
            text.push_str(
                " Conductor2D is an infinite-cylinder cross-section, not a 3D finite object.",
            );
        }
        MaxwellCase::InfiniteWire => {
            text.push_str(
                " InfiniteWire is an ideal thin surface-current shell, not a general wire.",
            );
        }
        _ => {}
    }
    Some(text)
}

/// Validate every editable value before any physics runs.
pub fn validate_maxwell(scene: &MaxwellScene) -> Result<(), MaxwellError> {
    for (label, value) in [("E0", scene.e0), ("B0", scene.b0), ("charge", scene.charge)] {
        if !value.is_finite() || value.abs() > MAXWELL_MAX_AMPLITUDE {
            return Err(MaxwellError::InvalidScene(format!(
                "{label} must be finite and within +/-{MAXWELL_MAX_AMPLITUDE}"
            )));
        }
    }

    for (label, value) in [
        ("separation", scene.separation),
        ("radius", scene.radius),
        ("outer_radius", scene.outer_radius),
        ("charge height", scene.charge_height),
    ] {
        if !value.is_finite() || value <= 0.0 {
            return Err(MaxwellError::InvalidScene(format!(
                "{label} must be finite and greater than zero"
            )));
        }
    }
    if scene.outer_radius <= scene.radius {
        return Err(MaxwellError::InvalidScene(
            "outer radius must be greater than the inner radius".into(),
        ));
    }

    if scene.elements < MAXWELL_BEM_MIN_ELEMENTS
        || scene.elements > MAXWELL_MAX_BEM_ELEMENTS
        || !scene.elements.is_multiple_of(2)
    {
        return Err(MaxwellError::InvalidScene(format!(
            "boundary element count must be even and within {MAXWELL_BEM_MIN_ELEMENTS}..={MAXWELL_MAX_BEM_ELEMENTS}"
        )));
    }

    if scene.case == MaxwellCase::Conductor2D {
        match scene.outline {
            ConductorOutline::Circle { radius } => {
                if !radius.is_finite() || radius <= 0.0 {
                    return Err(MaxwellError::InvalidScene(
                        "conductor circle radius must be finite and greater than zero".into(),
                    ));
                }
            }
            ConductorOutline::Square { half_extent } => {
                if !half_extent.is_finite() || half_extent <= 0.0 {
                    return Err(MaxwellError::InvalidScene(
                        "conductor square half-extent must be finite and greater than zero".into(),
                    ));
                }
            }
        }
        let [ax, ay] = scene.applied;
        let magnitude = (ax * ax + ay * ay).sqrt();
        if !ax.is_finite() || !ay.is_finite() || !magnitude.is_finite() || magnitude <= 0.0 {
            return Err(MaxwellError::InvalidScene(
                "applied field must be finite and nonzero".into(),
            ));
        }
    }

    Ok(())
}

/// The analytic field at `point`, in normalized display units.
///
/// Returns [`MaxwellError::Unsupported`] exactly on a discontinuous interface
/// or a point-charge singularity. The unsupported half-space of the image plane
/// (`z <= 0`) returns zero, not an error; only the real charge point above the
/// plane is a singularity.
pub fn maxwell_field(scene: &MaxwellScene, point: Vec3) -> Result<Vec3, MaxwellError> {
    validate_maxwell(scene)?;
    if !point.is_finite() {
        return Err(MaxwellError::Unsupported(
            "sample point must be finite".into(),
        ));
    }
    let value = match scene.case {
        MaxwellCase::ParallelPlates => plates_field(scene, point)?,
        MaxwellCase::CoaxialElectric => coaxial_field(scene, point, MaxwellFamily::Electric)?,
        MaxwellCase::CoaxialMagnetic => coaxial_field(scene, point, MaxwellFamily::Magnetic)?,
        MaxwellCase::HollowSphere => hollow_sphere_field(scene, point)?,
        MaxwellCase::InfiniteWire => wire_field(scene, point)?,
        MaxwellCase::Solenoid => solenoid_field(scene, point)?,
        MaxwellCase::ImageSphere => image_sphere_field(scene, point)?,
        MaxwellCase::ImagePlane => image_plane_field(scene, point)?,
        MaxwellCase::Conductor2D => {
            return Err(MaxwellError::Unavailable(
                MAXWELL_BEM_PENDING_WARNING.into(),
            ));
        }
    };
    finite(value)
}

fn finite(value: DVec3) -> Result<Vec3, MaxwellError> {
    if !value.is_finite() {
        return Err(MaxwellError::Unsupported(
            "field evaluation produced a non-finite value".into(),
        ));
    }
    Ok(Vec3::new(value.x as f32, value.y as f32, value.z as f32))
}

fn case_tolerance(scale: f64) -> f64 {
    MAXWELL_INTERFACE_TOLERANCE_FACTOR * scale.max(1.0)
}

fn plates_field(scene: &MaxwellScene, point: Vec3) -> Result<DVec3, MaxwellError> {
    let p = point.as_dvec3();
    let half = 0.5 * scene.separation;
    let tolerance = case_tolerance(scene.separation);
    if (p.z.abs() - half).abs() <= tolerance {
        return Err(MaxwellError::Unsupported(MAXWELL_INTERFACE_WARNING.into()));
    }
    if p.z.abs() < half {
        Ok(DVec3::Z * scene.e0)
    } else {
        Ok(DVec3::ZERO)
    }
}

fn coaxial_field(
    scene: &MaxwellScene,
    point: Vec3,
    family: MaxwellFamily,
) -> Result<DVec3, MaxwellError> {
    let p = point.as_dvec3();
    let radius = (p.x * p.x + p.y * p.y).sqrt();
    let a = scene.radius;
    let b = scene.outer_radius;
    let tolerance = case_tolerance(b);
    if (radius - a).abs() <= tolerance || (radius - b).abs() <= tolerance {
        return Err(MaxwellError::Unsupported(MAXWELL_INTERFACE_WARNING.into()));
    }
    if radius <= a || radius >= b {
        return Ok(DVec3::ZERO);
    }
    let amplitude = match family {
        MaxwellFamily::Electric => scene.e0,
        MaxwellFamily::Magnetic => scene.b0,
    } * (a / radius);
    match family {
        MaxwellFamily::Electric => Ok(DVec3::new(
            amplitude * p.x / radius,
            amplitude * p.y / radius,
            0.0,
        )),
        MaxwellFamily::Magnetic => Ok(DVec3::new(
            -amplitude * p.y / radius,
            amplitude * p.x / radius,
            0.0,
        )),
    }
}

fn hollow_sphere_field(scene: &MaxwellScene, point: Vec3) -> Result<DVec3, MaxwellError> {
    let p = point.as_dvec3();
    let radius = p.length();
    let tolerance = case_tolerance(scene.radius);
    if (radius - scene.radius).abs() <= tolerance {
        return Err(MaxwellError::Unsupported(MAXWELL_INTERFACE_WARNING.into()));
    }
    if radius <= scene.radius {
        return Ok(DVec3::ZERO);
    }
    let scale = scene.e0 * scene.radius * scene.radius / (radius * radius * radius);
    Ok(p * scale)
}

fn wire_field(scene: &MaxwellScene, point: Vec3) -> Result<DVec3, MaxwellError> {
    let p = point.as_dvec3();
    let radius = (p.x * p.x + p.y * p.y).sqrt();
    let a = scene.radius;
    let tolerance = case_tolerance(a);
    if (radius - a).abs() <= tolerance {
        return Err(MaxwellError::Unsupported(MAXWELL_INTERFACE_WARNING.into()));
    }
    if radius <= a {
        return Ok(DVec3::ZERO);
    }
    let amplitude = scene.b0 * a / radius;
    Ok(DVec3::new(
        -amplitude * p.y / radius,
        amplitude * p.x / radius,
        0.0,
    ))
}

fn solenoid_field(scene: &MaxwellScene, point: Vec3) -> Result<DVec3, MaxwellError> {
    let p = point.as_dvec3();
    let radius = (p.x * p.x + p.y * p.y).sqrt();
    let tolerance = case_tolerance(scene.radius);
    if (radius - scene.radius).abs() <= tolerance {
        return Err(MaxwellError::Unsupported(MAXWELL_INTERFACE_WARNING.into()));
    }
    if radius < scene.radius {
        Ok(DVec3::Z * scene.b0)
    } else {
        Ok(DVec3::ZERO)
    }
}

fn image_sphere_field(scene: &MaxwellScene, point: Vec3) -> Result<DVec3, MaxwellError> {
    let p = point.as_dvec3();
    let radius = p.length();
    let sphere = scene.radius;
    let tolerance = case_tolerance(sphere);
    if (radius - sphere).abs() <= tolerance {
        return Err(MaxwellError::Unsupported(MAXWELL_INTERFACE_WARNING.into()));
    }
    if radius < sphere {
        return Ok(DVec3::ZERO);
    }
    let rho = (p.x * p.x + p.y * p.y).sqrt();
    let cos_theta = p.z / radius;
    let sin_theta = rho / radius;
    let ratio = sphere * sphere * sphere / (radius * radius * radius);
    let radial = scene.e0 * (1.0 + 2.0 * ratio) * cos_theta;
    let polar = scene.e0 * (ratio - 1.0) * sin_theta;
    let rhat = p / radius;
    let theta_hat = if rho > 0.0 {
        DVec3::new(cos_theta * p.x / rho, cos_theta * p.y / rho, -sin_theta)
    } else {
        DVec3::ZERO
    };
    Ok(rhat * radial + theta_hat * polar)
}

fn image_plane_field(scene: &MaxwellScene, point: Vec3) -> Result<DVec3, MaxwellError> {
    let p = point.as_dvec3();
    // The grounded plane and the unsupported half-space carry no displayed
    // field; the image half-space is never evaluated.
    if p.z <= 0.0 {
        return Ok(DVec3::ZERO);
    }
    let height = scene.charge_height;
    let source = DVec3::new(0.0, 0.0, height);
    let offset = p - source;
    let distance = offset.length();
    if distance <= case_tolerance(height) {
        return Err(MaxwellError::Unsupported("point-charge singularity".into()));
    }
    let image_source = DVec3::new(0.0, 0.0, -height);
    let image_offset = p - image_source;
    let image_distance = image_offset.length();
    if image_distance <= case_tolerance(height) {
        return Err(MaxwellError::Unsupported("image-charge singularity".into()));
    }
    let real = offset / (distance * distance * distance);
    let image = image_offset / (image_distance * image_distance * image_distance);
    Ok((real - image) * scene.charge)
}

/// Renderer-independent outline for the selected case.
///
/// `None` is the 3D world outline. `Some(frame)` is one 2D slice: only the
/// slice intersection is returned, in the frame's local `(u, v, 0)`
/// coordinates.
pub fn maxwell_outline(
    scene: &MaxwellScene,
    domain: &Domain,
    plane: Option<&PlaneFrame>,
) -> Vec<MaxwellLine> {
    let color = family_line_color(maxwell_family(scene.case));
    match plane {
        None => world_outline(scene, domain, color),
        Some(frame) => slice_outline(scene, domain, frame, color),
    }
}

/// Sample the selected analytic case over the 3D grid or one 2D slice.
///
/// Sampling never densifies the input grid and never clamps a finite value:
/// exact interfaces, singularities, degenerate vectors and extreme finite
/// magnitudes are omitted and counted.
pub fn sample_maxwell(
    scene: &MaxwellScene,
    domain: &Domain,
    grid: UVec3,
    plane: Option<&PlaneFrame>,
) -> MaxwellSampling {
    let mut sampling = MaxwellSampling {
        arrows: Vec::new(),
        family: maxwell_family(scene.case),
        case: scene.case,
        e0: scene.e0,
        b0: scene.b0,
        assumptions: maxwell_assumptions(scene),
        warning: None,
        omitted: 0,
        omitted_large: 0,
        out_of_plane_suppressed: false,
        accuracy: MaxwellStop::TargetMet,
        convergence: None,
    };

    if let Err(error) = validate_maxwell(scene) {
        sampling.accuracy = MaxwellStop::Unavailable;
        sampling.warning = Some(error_text(&error));
        return sampling;
    }

    if scene.case == MaxwellCase::Conductor2D {
        sampling.accuracy = MaxwellStop::Unavailable;
        sampling.warning = Some(if plane.is_none() {
            MAXWELL_SWITCH_2D_WARNING.to_string()
        } else {
            MAXWELL_BEM_PENDING_WARNING.to_string()
        });
        return sampling;
    }

    let effective_domain = match plane {
        Some(frame) => frame.local_domain(),
        None => *domain,
    };
    let effective_grid = match plane {
        Some(_) => UVec3::new(grid.x, grid.y, 1),
        None => grid,
    };
    if !sampling_domain_is_valid(&effective_domain, effective_grid, plane.is_some()) {
        sampling.accuracy = MaxwellStop::Unavailable;
        sampling.warning = Some(MAXWELL_EMPTY_DOMAIN_WARNING.to_string());
        return sampling;
    }

    let domain_extent = domain_extent(&effective_domain);
    let tolerance =
        MAXWELL_INTERFACE_TOLERANCE_FACTOR * domain_extent.max(case_extent(scene)).max(1.0);

    if scene.case == MaxwellCase::ImagePlane
        && plane.is_some_and(|frame| {
            coincident_ground_slice(frame, domain_extent.max(case_extent(scene)).max(1.0))
        })
    {
        sampling.warning = Some(MAXWELL_GROUND_SLICE_WARNING.to_string());
        return sampling;
    }

    let magnitude_limit = MAXWELL_MAX_RELATIVE * scene.e0.abs().max(scene.b0.abs()).max(1e-9);
    let color = match sampling.family {
        MaxwellFamily::Electric => ELECTRIC_COLOR,
        MaxwellFamily::Magnetic => MAGNETIC_COLOR,
    };

    let mut omitted_interface = 0usize;
    for raw_local in effective_domain.grid_points(effective_grid) {
        let (position, world) = match plane {
            Some(frame) => (raw_local, frame.to_world(raw_local)),
            None => (raw_local, raw_local),
        };
        if !position.is_finite() || !world.is_finite() {
            sampling.omitted += 1;
            continue;
        }
        if on_interface(scene, world, tolerance) || near_point_singularity(scene, world, tolerance)
        {
            omitted_interface += 1;
            sampling.omitted += 1;
            continue;
        }
        let full = match maxwell_field(scene, world) {
            Ok(value) if value.is_finite() => value,
            _ => {
                sampling.omitted += 1;
                continue;
            }
        };
        let full_length = f64::from(full.length());
        if !full_length.is_finite() || full_length < DEGENERATE_MAGNITUDE {
            sampling.omitted += 1;
            continue;
        }
        if full_length > magnitude_limit {
            sampling.omitted_large += 1;
            sampling.omitted += 1;
            continue;
        }
        let selected = match plane {
            Some(frame) => frame.project(full),
            None => full,
        };
        let projected_length = f64::from(selected.length());
        if plane.is_some()
            && full_length >= DEGENERATE_MAGNITUDE
            && projected_length < 0.1 * full_length
        {
            sampling.out_of_plane_suppressed = true;
        }
        if !selected.is_finite() || projected_length < DEGENERATE_MAGNITUDE {
            sampling.omitted += 1;
            continue;
        }
        let dir = selected / projected_length as f32;
        if !dir.is_finite() {
            sampling.omitted += 1;
            continue;
        }
        sampling.arrows.push(ArrowData {
            position,
            dir,
            mag: projected_length as f32,
            color,
        });
    }

    let mut warnings = Vec::new();
    if omitted_interface > 0 {
        warnings.push(MAXWELL_INTERFACE_WARNING);
    }
    if sampling.omitted_large > 0 {
        warnings.push(MAXWELL_EXTREME_WARNING);
    }
    if sampling.out_of_plane_suppressed {
        warnings.push(MAXWELL_PROJECTION_WARNING);
    }
    if !warnings.is_empty() {
        sampling.warning = Some(warnings.join("; "));
    }
    sampling
}

fn error_text(error: &MaxwellError) -> String {
    match error {
        MaxwellError::InvalidScene(message)
        | MaxwellError::Unsupported(message)
        | MaxwellError::Unavailable(message) => message.clone(),
    }
}

fn sampling_domain_is_valid(domain: &Domain, grid: UVec3, plane: bool) -> bool {
    let size = domain.domain_size();
    grid.x > 0
        && grid.y > 0
        && grid.z > 0
        && size.x > 0.0
        && size.y > 0.0
        && (plane || size.z > 0.0)
        && domain.min.is_finite()
        && domain.max.is_finite()
        && size.is_finite()
}

fn domain_extent(domain: &Domain) -> f64 {
    let size = domain.domain_size();
    f64::from(size.x.abs().max(size.y.abs()).max(size.z.abs()))
}

fn case_extent(scene: &MaxwellScene) -> f64 {
    match scene.case {
        MaxwellCase::ParallelPlates => scene.separation,
        MaxwellCase::CoaxialElectric | MaxwellCase::CoaxialMagnetic => scene.outer_radius,
        MaxwellCase::HollowSphere
        | MaxwellCase::InfiniteWire
        | MaxwellCase::Solenoid
        | MaxwellCase::ImageSphere => scene.radius,
        MaxwellCase::ImagePlane => scene.charge_height.abs(),
        MaxwellCase::Conductor2D => match scene.outline {
            ConductorOutline::Circle { radius } => radius,
            ConductorOutline::Square { half_extent } => half_extent,
        },
    }
    .max(1.0)
}

fn on_interface(scene: &MaxwellScene, point: Vec3, tolerance: f64) -> bool {
    let p = point.as_dvec3();
    match scene.case {
        MaxwellCase::ParallelPlates => (p.z.abs() - 0.5 * scene.separation).abs() <= tolerance,
        MaxwellCase::CoaxialElectric | MaxwellCase::CoaxialMagnetic => {
            let radius = (p.x * p.x + p.y * p.y).sqrt();
            (radius - scene.radius).abs() <= tolerance
                || (radius - scene.outer_radius).abs() <= tolerance
        }
        MaxwellCase::HollowSphere | MaxwellCase::ImageSphere => {
            (p.length() - scene.radius).abs() <= tolerance
        }
        MaxwellCase::InfiniteWire | MaxwellCase::Solenoid => {
            let radius = (p.x * p.x + p.y * p.y).sqrt();
            (radius - scene.radius).abs() <= tolerance
        }
        MaxwellCase::ImagePlane => p.z.abs() <= tolerance,
        MaxwellCase::Conductor2D => false,
    }
}

fn near_point_singularity(scene: &MaxwellScene, point: Vec3, tolerance: f64) -> bool {
    if scene.case != MaxwellCase::ImagePlane {
        return false;
    }
    let p = point.as_dvec3();
    let source = DVec3::new(0.0, 0.0, scene.charge_height);
    (p - source).length() <= tolerance.max(case_tolerance(scene.charge_height))
}

fn coincident_ground_slice(frame: &PlaneFrame, scale: f64) -> bool {
    if (frame.normal.dot(Vec3::Z).abs() - 1.0).abs() > 1e-6 {
        return false;
    }
    let gap = f64::from(frame.origin.z.abs());
    gap <= 1e-6 * scale
}

fn family_line_color(family: MaxwellFamily) -> [f32; 4] {
    let [r, g, b] = match family {
        MaxwellFamily::Electric => ELECTRIC_COLOR,
        MaxwellFamily::Magnetic => MAGNETIC_COLOR,
    };
    [
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
        OUTLINE_ALPHA,
    ]
}

fn line(a: Vec3, b: Vec3, color: [f32; 4]) -> Option<MaxwellLine> {
    if !a.is_finite() || !b.is_finite() {
        return None;
    }
    Some(MaxwellLine {
        a: [a.x, a.y, a.z],
        b: [b.x, b.y, b.z],
        color,
    })
}

fn circle(center: Vec3, u: Vec3, v: Vec3, radius: f32, color: [f32; 4]) -> Vec<MaxwellLine> {
    let mut lines = Vec::with_capacity(OUTLINE_SEGMENTS);
    if !center.is_finite() || !u.is_finite() || !v.is_finite() || !radius.is_finite() {
        return lines;
    }
    let mut previous = center + u * radius;
    for index in 1..=OUTLINE_SEGMENTS {
        let angle = std::f32::consts::TAU * index as f32 / OUTLINE_SEGMENTS as f32;
        let point = center + u * (radius * angle.cos()) + v * (radius * angle.sin());
        if let Some(segment) = line(previous, point, color) {
            lines.push(segment);
        }
        previous = point;
    }
    lines
}

fn square(center: Vec3, half_u: f32, half_v: f32, color: [f32; 4]) -> Vec<MaxwellLine> {
    if !half_u.is_finite() || !half_v.is_finite() || half_u <= 0.0 || half_v <= 0.0 {
        return Vec::new();
    }
    let corners = [
        center + Vec3::new(-half_u, -half_v, 0.0),
        center + Vec3::new(half_u, -half_v, 0.0),
        center + Vec3::new(half_u, half_v, 0.0),
        center + Vec3::new(-half_u, half_v, 0.0),
    ];
    let mut lines = Vec::with_capacity(4);
    for index in 0..4 {
        if let Some(segment) = line(corners[index], corners[(index + 1) % 4], color) {
            lines.push(segment);
        }
    }
    lines
}

fn world_outline(scene: &MaxwellScene, domain: &Domain, color: [f32; 4]) -> Vec<MaxwellLine> {
    match scene.case {
        MaxwellCase::ParallelPlates => {
            let size = domain.domain_size();
            if !size.is_finite() {
                return Vec::new();
            }
            let center = domain.center();
            let mut lines = Vec::with_capacity(2);
            for z in [
                -0.5 * scene.separation as f32,
                0.5 * scene.separation as f32,
            ] {
                if let Some(segment) = line(
                    Vec3::new(domain.min.x, center.y, z),
                    Vec3::new(domain.max.x, center.y, z),
                    color,
                ) {
                    lines.push(segment);
                }
            }
            lines
        }
        MaxwellCase::CoaxialElectric | MaxwellCase::CoaxialMagnetic => {
            let center = domain.center();
            let mut lines = circle(
                Vec3::new(0.0, 0.0, center.z),
                Vec3::X,
                Vec3::Y,
                scene.radius as f32,
                color,
            );
            lines.extend(circle(
                Vec3::new(0.0, 0.0, center.z),
                Vec3::X,
                Vec3::Y,
                scene.outer_radius as f32,
                color,
            ));
            lines
        }
        MaxwellCase::HollowSphere | MaxwellCase::ImageSphere => {
            let radius = scene.radius as f32;
            let mut lines = sphere_circles(Vec3::ZERO, radius, color);
            if scene.case == MaxwellCase::ImageSphere {
                let direction = DVec3::Z * (1.5 * scene.radius);
                if let Some(segment) = line(
                    Vec3::ZERO,
                    Vec3::new(direction.x as f32, direction.y as f32, direction.z as f32),
                    color,
                ) {
                    lines.push(segment);
                }
            }
            lines
        }
        MaxwellCase::InfiniteWire => {
            let mut lines = Vec::with_capacity(3);
            if let Some(axis) = line(
                Vec3::new(0.0, 0.0, domain.min.z),
                Vec3::new(0.0, 0.0, domain.max.z),
                color,
            ) {
                lines.push(axis);
            }
            if let Some(tick) = line(
                Vec3::new(-scene.radius as f32, 0.0, 0.0),
                Vec3::new(scene.radius as f32, 0.0, 0.0),
                color,
            ) {
                lines.push(tick);
            }
            if let Some(tick) = line(
                Vec3::new(0.0, -scene.radius as f32, 0.0),
                Vec3::new(0.0, scene.radius as f32, 0.0),
                color,
            ) {
                lines.push(tick);
            }
            lines
        }
        MaxwellCase::Solenoid => {
            let center = domain.center();
            let radius = scene.radius as f32;
            let half = 0.5 * domain.domain_size().z.abs().max(1.0);
            let mut lines = circle(
                Vec3::new(0.0, 0.0, center.z - half),
                Vec3::X,
                Vec3::Y,
                radius,
                color,
            );
            lines.extend(circle(
                Vec3::new(0.0, 0.0, center.z + half),
                Vec3::X,
                Vec3::Y,
                radius,
                color,
            ));
            for index in 0..4 {
                let angle = std::f32::consts::TAU * index as f32 / 4.0;
                let offset = Vec3::new(radius * angle.cos(), radius * angle.sin(), 0.0);
                if let Some(segment) = line(
                    Vec3::new(0.0, 0.0, center.z - half) + offset,
                    Vec3::new(0.0, 0.0, center.z + half) + offset,
                    color,
                ) {
                    lines.push(segment);
                }
            }
            lines
        }
        MaxwellCase::ImagePlane => {
            let size = domain.domain_size();
            let center = domain.center();
            let half_x = 0.5 * size.x.abs();
            let half_y = 0.5 * size.y.abs();
            let mut lines = square(Vec3::new(center.x, center.y, 0.0), half_x, half_y, color);
            let marker = 0.08 * size.x.abs().max(size.y.abs()).max(1.0);
            let height = scene.charge_height as f32;
            if let Some(segment) = line(
                Vec3::new(-marker, 0.0, height),
                Vec3::new(marker, 0.0, height),
                color,
            ) {
                lines.push(segment);
            }
            if let Some(segment) = line(
                Vec3::new(0.0, -marker, height),
                Vec3::new(0.0, marker, height),
                color,
            ) {
                lines.push(segment);
            }
            lines
        }
        MaxwellCase::Conductor2D => Vec::new(),
    }
}

fn slice_outline(
    scene: &MaxwellScene,
    domain: &Domain,
    frame: &PlaneFrame,
    color: [f32; 4],
) -> Vec<MaxwellLine> {
    let extent = domain_extent(domain).max(1.0);
    let mut lines = Vec::new();
    match scene.case {
        MaxwellCase::ParallelPlates => {
            for z in [
                -0.5 * scene.separation as f32,
                0.5 * scene.separation as f32,
            ] {
                if let Some(segment) =
                    slice_plane_intersection(frame, Vec3::Z, z, 1e-6 * extent, color)
                {
                    lines.push(segment);
                }
            }
        }
        MaxwellCase::CoaxialElectric | MaxwellCase::CoaxialMagnetic => {
            lines.extend(cylinder_slice(
                frame,
                scene.radius as f32,
                extent as f32,
                color,
            ));
            lines.extend(cylinder_slice(
                frame,
                scene.outer_radius as f32,
                extent as f32,
                color,
            ));
        }
        MaxwellCase::HollowSphere | MaxwellCase::ImageSphere => {
            if let Some((center, cut_radius)) = sphere_slice_circle(frame, scene.radius as f32) {
                lines.extend(circle(center, Vec3::X, Vec3::Y, cut_radius, color));
            }
            if scene.case == MaxwellCase::ImageSphere {
                let start = frame.to_local(Vec3::ZERO);
                let end = frame.to_local(Vec3::new(0.0, 0.0, 1.5 * scene.radius as f32));
                if let Some(segment) = line(
                    Vec3::new(start.x, start.y, 0.0),
                    Vec3::new(end.x, end.y, 0.0),
                    color,
                ) {
                    lines.push(segment);
                }
            }
        }
        MaxwellCase::InfiniteWire | MaxwellCase::Solenoid => {
            lines.extend(cylinder_slice(
                frame,
                scene.radius as f32,
                extent as f32,
                color,
            ));
            if scene.case == MaxwellCase::InfiniteWire
                && let Some(segment) =
                    slice_plane_intersection(frame, Vec3::Z, 0.0, 1e-6 * extent, color)
            {
                lines.push(segment);
            }
        }
        MaxwellCase::ImagePlane => {
            if let Some(segment) =
                slice_plane_intersection(frame, Vec3::Z, 0.0, 1e-6 * extent, color)
            {
                lines.push(segment);
            }
            let charge = frame.to_local(Vec3::new(0.0, 0.0, scene.charge_height as f32));
            if charge.z.abs() <= 1e-6 * extent as f32 {
                let marker = 0.08 * extent as f32;
                for (dx, dy) in [(-marker, 0.0), (0.0, -marker)] {
                    if let Some(segment) = line(
                        Vec3::new(charge.x + dx, charge.y + dy, 0.0),
                        Vec3::new(charge.x - dx, charge.y - dy, 0.0),
                        color,
                    ) {
                        lines.push(segment);
                    }
                }
            }
        }
        MaxwellCase::Conductor2D => match scene.outline {
            ConductorOutline::Circle { radius } => {
                if frame.normal.dot(Vec3::Z).abs() > 1.0 - 1e-6 {
                    lines.extend(circle(Vec3::ZERO, Vec3::X, Vec3::Y, radius as f32, color));
                } else {
                    lines.extend(cylinder_slice(frame, radius as f32, extent as f32, color));
                }
            }
            ConductorOutline::Square { half_extent } => {
                if frame.normal.dot(Vec3::Z).abs() > 1.0 - 1e-6 {
                    lines.extend(square(
                        Vec3::ZERO,
                        half_extent as f32,
                        half_extent as f32,
                        color,
                    ));
                } else {
                    for sign in [-1.0f32, 1.0] {
                        if let Some(segment) = line(
                            Vec3::new(-frame.half_u, sign * half_extent as f32, 0.0),
                            Vec3::new(frame.half_u, sign * half_extent as f32, 0.0),
                            color,
                        ) {
                            lines.push(segment);
                        }
                    }
                }
            }
        },
    }
    lines
}

fn sphere_circles(center: Vec3, radius: f32, color: [f32; 4]) -> Vec<MaxwellLine> {
    if !radius.is_finite() || radius <= 0.0 {
        return Vec::new();
    }
    let mut lines = circle(center, Vec3::X, Vec3::Y, radius, color);
    lines.extend(circle(center, Vec3::X, Vec3::Z, radius, color));
    lines.extend(circle(center, Vec3::Y, Vec3::Z, radius, color));
    lines
}

fn sphere_slice_circle(frame: &PlaneFrame, radius: f32) -> Option<(Vec3, f32)> {
    if !radius.is_finite() || radius <= 0.0 {
        return None;
    }
    let distance = frame.normal.dot(frame.origin);
    if !distance.is_finite() || distance.abs() >= radius {
        return None;
    }
    let center = frame.to_local(Vec3::ZERO);
    if !center.is_finite() {
        return None;
    }
    let cut_radius = (radius * radius - distance * distance).max(0.0).sqrt();
    if !cut_radius.is_finite() || cut_radius <= 0.0 {
        return None;
    }
    Some((Vec3::new(center.x, center.y, 0.0), cut_radius))
}

fn cylinder_slice(
    frame: &PlaneFrame,
    radius: f32,
    extent: f32,
    color: [f32; 4],
) -> Vec<MaxwellLine> {
    if !radius.is_finite() || radius <= 0.0 {
        return Vec::new();
    }
    let axis_dot = frame.normal.dot(Vec3::Z);
    if (axis_dot.abs() - 1.0).abs() <= 1e-6 {
        let world_center = Vec3::new(0.0, 0.0, frame.origin.z);
        let center = frame.to_local(world_center);
        return circle(
            Vec3::new(center.x, center.y, 0.0),
            Vec3::X,
            Vec3::Y,
            radius,
            color,
        );
    }
    if axis_dot.abs() > 1e-6 {
        return Vec::new();
    }
    let normal = Vec3::new(frame.normal.x, frame.normal.y, 0.0);
    if normal.length() <= 1e-6 {
        return Vec::new();
    }
    let normal = normal.normalize();
    let offset = normal.dot(Vec3::new(frame.origin.x, frame.origin.y, 0.0));
    let discriminant = radius * radius - offset * offset;
    if !discriminant.is_finite() || discriminant <= 0.0 {
        return Vec::new();
    }
    let across = Vec3::Z.cross(normal);
    let root = discriminant.sqrt();
    let direction = frame.project(Vec3::Z);
    let mut lines = Vec::with_capacity(2);
    for sign in [-1.0f32, 1.0] {
        let base = normal * offset + across * (sign * root);
        for z in [-extent, extent] {
            let start = frame.to_local(base + Vec3::Z * z);
            if let Some((a, b)) = clip_local(start, direction, frame.half_u, frame.half_v)
                && let Some(segment) = line(a, b, color)
            {
                lines.push(segment);
                break;
            }
        }
    }
    lines
}

fn slice_plane_intersection(
    frame: &PlaneFrame,
    normal: Vec3,
    offset: f32,
    coincident_tolerance: f64,
    color: [f32; 4],
) -> Option<MaxwellLine> {
    if !normal.is_finite() || !offset.is_finite() {
        return None;
    }
    let normal = normal.normalize();
    let alignment = normal.dot(frame.normal);
    if (alignment.abs() - 1.0).abs() <= 1e-6 {
        let gap = (f64::from(normal.dot(frame.origin)) - f64::from(offset)).abs();
        if gap <= coincident_tolerance {
            let a = Vec3::new(-frame.half_u, 0.0, 0.0);
            let b = Vec3::new(frame.half_u, 0.0, 0.0);
            return line(a, b, color);
        }
        return None;
    }
    if alignment.abs() <= 1e-6 {
        return None;
    }
    let direction = normal.cross(frame.normal);
    let denominator = direction.length_squared();
    if !denominator.is_finite() || denominator <= 1e-12 {
        return None;
    }
    let frame_offset = frame.normal.dot(frame.origin);
    let point = axis_plane_point(normal, offset, frame.normal, frame_offset);
    let start = frame.to_local(point);
    let direction_local = frame.project(direction);
    clip_local(start, direction_local, frame.half_u, frame.half_v)
        .and_then(|(a, b)| line(a, b, color))
}

fn axis_plane_point(normal: Vec3, offset: f32, plane_normal: Vec3, plane_offset: f32) -> Vec3 {
    // Solve n1.p = d1, n2.p = d2 with the minimum-norm representative.
    let n1 = normal.as_dvec3();
    let n2 = plane_normal.as_dvec3();
    let d1 = f64::from(offset);
    let d2 = f64::from(plane_offset);
    let cross = n1.cross(n2);
    let denominator = cross.length_squared();
    if !denominator.is_finite() || denominator <= 1e-18 {
        return Vec3::ZERO;
    }
    let value = (n2.cross(cross) * d1 + cross.cross(n1) * d2) / denominator;
    Vec3::new(value.x as f32, value.y as f32, value.z as f32)
}

fn clip_local(point: Vec3, direction: Vec3, half_u: f32, half_v: f32) -> Option<(Vec3, Vec3)> {
    if !point.is_finite() || !direction.is_finite() {
        return None;
    }
    if !half_u.is_finite() || !half_v.is_finite() || half_u <= 0.0 || half_v <= 0.0 {
        return None;
    }
    let mut t_min = f32::NEG_INFINITY;
    let mut t_max = f32::INFINITY;
    for (p, q) in [
        (-direction.x, point.x + half_u),
        (direction.x, half_u - point.x),
        (-direction.y, point.y + half_v),
        (direction.y, half_v - point.y),
    ] {
        if p.abs() <= f32::EPSILON {
            if q < 0.0 {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t_min = t_min.max(t);
            } else {
                t_max = t_max.min(t);
            }
        }
    }
    if t_min > t_max || !t_min.is_finite() || !t_max.is_finite() {
        return None;
    }
    let start = point + direction * t_min;
    let end = point + direction * t_max;
    if !start.is_finite() || !end.is_finite() {
        return None;
    }
    Some((
        Vec3::new(
            start.x.clamp(-half_u, half_u),
            start.y.clamp(-half_v, half_v),
            0.0,
        ),
        Vec3::new(
            end.x.clamp(-half_u, half_u),
            end.y.clamp(-half_v, half_v),
            0.0,
        ),
    ))
}
